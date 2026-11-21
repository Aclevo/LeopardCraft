//! Document lifecycle policy for the editor shell.
//!
//! Dirty tracking compares the document's stable semantic signature with the
//! signature recorded at the last successful native save (or at load). The
//! signature ignores allocator counters, so undoing back to the saved state
//! reports clean again.

use super::recovery::{RecoveredDocument, RecoveryStore};
use lc_scene::Document;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// An action that discards the current document unless it is saved first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PendingAction {
    NewScene,
    OpenScene,
    OpenPath(PathBuf),
    Quit,
}

impl PendingAction {
    pub(super) fn prompt(&self) -> &'static str {
        match self {
            Self::NewScene => "starting a new scene",
            Self::OpenScene | Self::OpenPath(_) => "opening another scene",
            Self::Quit => "quitting LeopardCraft",
        }
    }
}

pub(super) fn is_dirty(doc: &Document, saved_signature: Option<u64>) -> bool {
    saved_signature != Some(doc.signature())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GuardChoice {
    Save,
    Discard,
    Cancel,
}

/// Outcome of a guarded document action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum GuardResult {
    /// The action can run immediately.
    Run(PendingAction),
    /// The confirmation window should be shown; nothing runs yet.
    Confirm,
    /// The user chose Save; a queued action, if any, runs after a successful save.
    Save,
    /// The action was cancelled.
    Cancelled,
}

/// Close/open/new guard for a document with unsaved changes.
///
/// This is deliberately independent of egui so the confirmation flow can be
/// tested without constructing the editor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct CloseGuard {
    pending: Option<PendingAction>,
    after_save: Option<PendingAction>,
}

impl CloseGuard {
    pub(super) fn pending_action(&self) -> Option<PendingAction> {
        self.pending.clone()
    }

    pub(super) fn request(&mut self, dirty: bool, action: PendingAction) -> GuardResult {
        if dirty {
            if self.pending.is_none() {
                self.pending = Some(action);
            }
            GuardResult::Confirm
        } else {
            // A document that became clean no longer needs a stale
            // confirmation or a queued post-save action.
            self.clear();
            GuardResult::Run(action)
        }
    }

    pub(super) fn choose(&mut self, choice: GuardChoice) -> GuardResult {
        match choice {
            GuardChoice::Save => {
                self.after_save = self.pending.take();
                GuardResult::Save
            }
            GuardChoice::Discard => match self.pending.take() {
                Some(action) => GuardResult::Run(action),
                None => GuardResult::Cancelled,
            },
            GuardChoice::Cancel => {
                self.pending = None;
                GuardResult::Cancelled
            }
        }
    }

    /// Take the action queued by a successful save.
    pub(super) fn saved(&mut self) -> Option<PendingAction> {
        self.after_save.take()
    }

    /// A failed save returns the queued action to the confirmation window.
    pub(super) fn save_failed(&mut self) {
        if let Some(action) = self.after_save.take() {
            self.pending = Some(action);
        }
    }

    /// Cancelling a Save As dialog returns the action to the confirmation
    /// window; nothing is discarded and no file is written.
    pub(super) fn after_save_cancelled(&mut self) {
        if let Some(action) = self.after_save.take() {
            self.pending = Some(action);
        }
    }

    pub(super) fn clear(&mut self) {
        self.pending = None;
        self.after_save = None;
    }
}

pub(super) fn is_native_scene(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("lcscene"))
        .unwrap_or(false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AutosaveOutcome {
    /// No snapshot is due yet.
    Idle,
    /// The document matches its saved state; a stale snapshot may be removed.
    Clean,
    /// A fresh snapshot was written.
    Saved,
    Failed(String),
}

/// Timed recovery snapshots for the in-memory document.
pub(super) struct Autosave {
    store: RecoveryStore,
    interval: Duration,
    next_due: Instant,
    last_signature: Option<u64>,
}

impl Autosave {
    pub(super) fn new(store: RecoveryStore, interval: Duration) -> Self {
        Self {
            store,
            interval,
            next_due: Instant::now() + interval,
            last_signature: None,
        }
    }

    pub(super) fn due(
        &mut self,
        now: Instant,
        doc: &Document,
        saved_signature: Option<u64>,
        origin: Option<&Path>,
    ) -> AutosaveOutcome {
        if now < self.next_due {
            return AutosaveOutcome::Idle;
        }
        self.next_due = now + self.interval;
        let signature = doc.signature();
        if saved_signature == Some(signature) {
            self.last_signature = Some(signature);
            return AutosaveOutcome::Clean;
        }
        if self.last_signature == Some(signature) {
            return AutosaveOutcome::Idle;
        }
        match self.store.write(doc, origin) {
            Ok(()) => {
                self.last_signature = Some(signature);
                AutosaveOutcome::Saved
            }
            Err(error) => AutosaveOutcome::Failed(error),
        }
    }

    /// Record a manual save, which resets the snapshot clock.
    pub(super) fn note_saved(&mut self, signature: Option<u64>) {
        self.last_signature = signature;
        self.next_due = Instant::now() + self.interval;
    }

    /// Record a recovered document: it is unsaved until the user saves it.
    pub(super) fn note_recovered(&mut self) {
        self.last_signature = None;
        self.next_due = Instant::now() + self.interval;
    }

    /// Forget the last snapshot after an explicit discard so a still-dirty
    /// document can be snapshotted again on the next tick.
    pub(super) fn forget_snapshot(&mut self) {
        self.last_signature = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SaveKind {
    Native,
    Export,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum StartupOutcome {
    Clean,
    Recoverable,
    Unreadable(String),
}

/// The complete document lifecycle used by the editor: dirty signature,
/// close/open/new guard, timed recovery snapshots and startup recovery.
///
/// The egui shell owns one of these and delegates every menu, close-request,
/// dialog and autosave decision to it. Keeping it free of egui lets a
/// non-interactive harness drive exactly the same production decisions.
pub(super) struct DocumentLifecycle {
    recovery: RecoveryStore,
    autosave: Autosave,
    saved_signature: Option<u64>,
    file_path: Option<PathBuf>,
    dirty: bool,
    dirty_checked_at: Instant,
    guard: CloseGuard,
    recovery_prompt: Option<RecoveredDocument>,
}

impl DocumentLifecycle {
    pub(super) fn new(recovery: RecoveryStore, interval: Duration) -> Self {
        Self {
            autosave: Autosave::new(recovery.clone(), interval),
            recovery,
            saved_signature: None,
            file_path: None,
            dirty: false,
            dirty_checked_at: Instant::now(),
            guard: CloseGuard::default(),
            recovery_prompt: None,
        }
    }

    /// Establish the clean baseline and look for a snapshot from a previous run.
    pub(super) fn startup(&mut self, doc: &Document) -> StartupOutcome {
        self.loaded(doc, None);
        match self.recovery.snapshot() {
            Ok(Some(recovered)) => {
                self.recovery_prompt = Some(recovered);
                StartupOutcome::Recoverable
            }
            Ok(None) => StartupOutcome::Clean,
            Err(error) => {
                self.recovery.discard();
                StartupOutcome::Unreadable(error)
            }
        }
    }

    pub(super) fn loaded(&mut self, doc: &Document, path: Option<PathBuf>) {
        self.file_path = path;
        self.saved_signature = Some(doc.signature());
        self.dirty = false;
        self.dirty_checked_at = Instant::now();
        self.autosave.note_saved(self.saved_signature);
        self.guard.clear();
    }

    pub(super) fn path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    pub(super) fn needs_save_as(&self) -> bool {
        self.file_path
            .as_deref()
            .map(|path| !is_native_scene(path))
            .unwrap_or(true)
    }

    pub(super) fn refresh_dirty(&mut self, doc: &Document, force: bool) {
        if !force && self.dirty_checked_at.elapsed() < Duration::from_millis(200) {
            return;
        }
        self.dirty_checked_at = Instant::now();
        self.dirty = is_dirty(doc, self.saved_signature);
    }

    pub(super) fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub(super) fn request(&mut self, doc: &Document, action: PendingAction) -> GuardResult {
        self.refresh_dirty(doc, true);
        self.guard.request(self.dirty, action)
    }

    pub(super) fn choose(&mut self, choice: GuardChoice) -> GuardResult {
        self.guard.choose(choice)
    }

    pub(super) fn pending_action(&self) -> Option<PendingAction> {
        self.guard.pending_action()
    }

    pub(super) fn clear_guard(&mut self) {
        self.guard.clear();
    }

    pub(super) fn saved(&mut self) -> Option<PendingAction> {
        self.guard.saved()
    }

    pub(super) fn save_failed(&mut self) {
        self.guard.save_failed();
    }

    pub(super) fn after_save_cancelled(&mut self) {
        self.guard.after_save_cancelled();
    }

    /// Parse the target entered in the Save As dialog. An empty target is a
    /// failed save: a queued close/quit action returns to the confirmation
    /// and the document stays dirty.
    pub(super) fn save_as_path(&mut self, input: &str) -> Result<PathBuf, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            self.save_failed();
            Err("No file path entered".into())
        } else {
            Ok(PathBuf::from(trimmed))
        }
    }

    /// Write the document. Only a native `.lcscene` write marks it saved.
    pub(super) fn save_document(
        &mut self,
        doc: &Document,
        path: PathBuf,
    ) -> Result<SaveKind, String> {
        lc_io::export(&path, doc)?;
        if !is_native_scene(&path) {
            return Ok(SaveKind::Export);
        }
        self.file_path = Some(path);
        self.saved_signature = Some(doc.signature());
        self.dirty = false;
        self.dirty_checked_at = Instant::now();
        self.autosave.note_saved(self.saved_signature);
        self.recovery.discard();
        Ok(SaveKind::Native)
    }

    /// Save and release a guarded action only for a successful native save.
    /// A write error restores the pending confirmation before returning; an
    /// export does not make the document clean, so it restores the
    /// confirmation instead of running the queued action.
    pub(super) fn save_and_take_action(
        &mut self,
        doc: &Document,
        path: PathBuf,
    ) -> Result<(SaveKind, Option<PendingAction>), String> {
        let kind = match self.save_document(doc, path) {
            Ok(kind) => kind,
            Err(error) => {
                self.guard.save_failed();
                return Err(error);
            }
        };
        if kind == SaveKind::Native {
            Ok((kind, self.guard.saved()))
        } else {
            self.guard.save_failed();
            Ok((kind, None))
        }
    }

    pub(super) fn autosave_tick(&mut self, doc: &Document) -> AutosaveOutcome {
        if self.recovery_prompt.is_some() {
            return AutosaveOutcome::Idle;
        }
        let outcome = self.autosave.due(
            Instant::now(),
            doc,
            self.saved_signature,
            self.file_path.as_deref(),
        );
        if matches!(outcome, AutosaveOutcome::Clean) {
            self.recovery.discard();
        }
        outcome
    }

    pub(super) fn has_snapshot(&self) -> bool {
        self.recovery.has_snapshot()
    }

    pub(super) fn discard_snapshot(&mut self) {
        self.recovery.discard();
        self.autosave.forget_snapshot();
    }

    pub(super) fn discard_recovery(&mut self) {
        self.recovery_prompt = None;
        self.discard_snapshot();
    }

    /// Remove a stale snapshot on a clean exit, but never delete a snapshot
    /// the startup prompt is still offering.
    pub(super) fn finish_quit(&mut self) {
        if self.recovery_prompt.is_none() {
            self.discard_snapshot();
        }
    }

    pub(super) fn recovery_prompt(&self) -> Option<&RecoveredDocument> {
        self.recovery_prompt.as_ref()
    }

    pub(super) fn take_recovery(&mut self) -> Option<RecoveredDocument> {
        self.recovery_prompt.take()
    }

    /// Adopt recovered content as an unsaved document with the snapshot's
    /// origin path. The snapshot itself is consumed.
    pub(super) fn mark_recovered(&mut self, origin: Option<PathBuf>) {
        self.file_path = origin;
        self.saved_signature = None;
        self.dirty = true;
        self.dirty_checked_at = Instant::now();
        self.autosave.note_recovered();
        self.recovery.discard();
        self.recovery_prompt = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lc_scene::Document;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "leopardcraft-lifecycle-{}-{name}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn rename_first_node(doc: &mut Document) {
        let id = *doc.scene.roots.first().expect("demo scene has roots");
        let node = doc.scene.get_mut(id).expect("root exists");
        node.name.push_str(" edited");
    }

    #[test]
    fn clean_documents_run_immediately() {
        let mut guard = CloseGuard::default();
        assert_eq!(
            guard.request(false, PendingAction::NewScene),
            GuardResult::Run(PendingAction::NewScene)
        );
        assert!(guard.pending_action().is_none());
    }

    #[test]
    fn dirty_documents_wait_for_confirmation_without_duplicating_it() {
        let mut guard = CloseGuard::default();
        assert_eq!(
            guard.request(true, PendingAction::Quit),
            GuardResult::Confirm
        );
        assert_eq!(guard.pending_action(), Some(PendingAction::Quit));
        // A second request while the dialog is open does not replace the action.
        assert_eq!(
            guard.request(true, PendingAction::NewScene),
            GuardResult::Confirm
        );
        assert_eq!(guard.pending_action(), Some(PendingAction::Quit));
    }

    #[test]
    fn a_clean_request_supersedes_a_pending_confirmation() {
        let mut guard = CloseGuard::default();
        guard.request(true, PendingAction::Quit);
        assert_eq!(
            guard.request(false, PendingAction::NewScene),
            GuardResult::Run(PendingAction::NewScene)
        );
        assert!(guard.pending_action().is_none());

        guard.request(true, PendingAction::Quit);
        guard.choose(GuardChoice::Save);
        assert_eq!(
            guard.request(false, PendingAction::OpenScene),
            GuardResult::Run(PendingAction::OpenScene)
        );
        assert_eq!(guard.saved(), None);
    }

    #[test]
    fn save_runs_the_guarded_action_only_after_success() {
        let mut guard = CloseGuard::default();
        guard.request(true, PendingAction::Quit);
        assert_eq!(guard.choose(GuardChoice::Save), GuardResult::Save);
        assert!(guard.pending_action().is_none());
        assert_eq!(guard.saved(), Some(PendingAction::Quit));
        assert!(guard.saved().is_none());
    }

    #[test]
    fn failed_save_restores_the_confirmation() {
        let mut guard = CloseGuard::default();
        guard.request(true, PendingAction::OpenScene);
        guard.choose(GuardChoice::Save);
        guard.save_failed();
        assert_eq!(guard.pending_action(), Some(PendingAction::OpenScene));
        assert_eq!(guard.saved(), None);
    }

    #[test]
    fn discard_runs_the_action_and_cancel_does_not() {
        let mut guard = CloseGuard::default();
        guard.request(true, PendingAction::NewScene);
        assert_eq!(guard.choose(GuardChoice::Cancel), GuardResult::Cancelled);
        assert!(guard.pending_action().is_none());

        guard.request(true, PendingAction::NewScene);
        assert_eq!(
            guard.choose(GuardChoice::Discard),
            GuardResult::Run(PendingAction::NewScene)
        );
        assert!(guard.pending_action().is_none());
    }

    #[test]
    fn cancelling_save_as_restores_the_confirmation() {
        let mut guard = CloseGuard::default();
        guard.request(true, PendingAction::Quit);
        guard.choose(GuardChoice::Save);
        guard.after_save_cancelled();
        assert_eq!(guard.pending_action(), Some(PendingAction::Quit));
        assert_eq!(guard.saved(), None);
    }

    #[test]
    fn empty_save_as_target_is_a_failed_save() {
        let dir = TempDir::new("empty-save-as");
        let mut lifecycle = DocumentLifecycle::new(
            RecoveryStore::at(dir.path().join("recovery")),
            Duration::from_secs(15),
        );
        let base = Document::demo();
        lifecycle.loaded(&base, None);
        let mut edited = base.clone();
        rename_first_node(&mut edited);

        assert_eq!(
            lifecycle.request(&edited, PendingAction::Quit),
            GuardResult::Confirm
        );
        assert_eq!(lifecycle.choose(GuardChoice::Save), GuardResult::Save);
        let error = lifecycle
            .save_as_path("   ")
            .expect_err("an empty target must not produce a path");
        assert_eq!(error, "No file path entered");
        assert_eq!(
            lifecycle.pending_action(),
            Some(PendingAction::Quit),
            "the pending quit confirmation must return"
        );
        assert!(
            lifecycle.is_dirty(),
            "an empty save must not clean the document"
        );
        assert!(
            lifecycle.saved().is_none(),
            "the queued action must stay queued"
        );
    }

    #[test]
    fn write_failure_restores_the_confirmation_and_keeps_the_document_dirty() {
        let dir = TempDir::new("write-failure");
        let mut lifecycle = DocumentLifecycle::new(
            RecoveryStore::at(dir.path().join("recovery")),
            Duration::from_secs(15),
        );
        let base = Document::demo();
        lifecycle.loaded(&base, None);
        let mut edited = base.clone();
        rename_first_node(&mut edited);

        assert_eq!(
            lifecycle.request(&edited, PendingAction::Quit),
            GuardResult::Confirm
        );
        assert_eq!(lifecycle.choose(GuardChoice::Save), GuardResult::Save);

        let target = dir.path().join("missing").join("scene.lcscene");
        let error = lifecycle
            .save_and_take_action(&edited, target.clone())
            .expect_err("a save into a missing directory must fail");
        assert!(
            error.contains("failed to write"),
            "write failure was not surfaced: {error}"
        );
        assert_eq!(
            lifecycle.pending_action(),
            Some(PendingAction::Quit),
            "a write failure must restore the confirmation"
        );
        assert!(
            lifecycle.is_dirty(),
            "a write failure must keep the document dirty"
        );
        assert!(
            lifecycle.saved().is_none(),
            "a write failure must not release the queued action"
        );
        assert!(!target.exists(), "a failed write left a target file behind");
        assert!(
            !target.parent().expect("target has a parent").exists(),
            "a failed write created the missing parent directory"
        );
    }

    #[test]
    fn dirty_follows_the_semantic_signature() {
        let doc = Document::demo();
        let saved = Some(doc.signature());
        assert!(!is_dirty(&doc, saved));
        assert!(is_dirty(&doc, None));

        let mut changed = doc.clone();
        rename_first_node(&mut changed);
        assert!(is_dirty(&changed, saved));

        let resaved = Some(changed.signature());
        assert!(!is_dirty(&changed, resaved));
        // Returning to the original content is clean again even though the
        // scene was edited in between.
        assert!(!is_dirty(&doc, Some(doc.signature())));
    }

    #[test]
    fn native_scene_paths_are_recognised() {
        assert!(is_native_scene(Path::new("scene.lcscene")));
        assert!(is_native_scene(Path::new("SCENE.LCScene")));
        assert!(!is_native_scene(Path::new("scene.obj")));
        assert!(!is_native_scene(Path::new("scene")));
    }

    #[test]
    fn autosave_writes_only_when_the_document_changed() {
        let dir = TempDir::new("autosave");
        let store = RecoveryStore::at(dir.path());
        let mut autosave = Autosave::new(store.clone(), Duration::from_secs(10));
        let mut doc = Document::demo();
        let saved = Some(doc.signature());
        let t0 = Instant::now();

        assert_eq!(autosave.due(t0, &doc, saved, None), AutosaveOutcome::Idle);
        let t1 = t0 + Duration::from_secs(11);
        assert_eq!(autosave.due(t1, &doc, saved, None), AutosaveOutcome::Clean);
        assert!(!store.has_snapshot());

        rename_first_node(&mut doc);
        let t2 = t1 + Duration::from_secs(11);
        assert_eq!(autosave.due(t2, &doc, saved, None), AutosaveOutcome::Saved);
        assert!(store.has_snapshot());

        // The same dirty signature is not written again.
        let t3 = t2 + Duration::from_secs(11);
        assert_eq!(autosave.due(t3, &doc, saved, None), AutosaveOutcome::Idle);
    }

    #[test]
    fn autosave_respects_the_interval() {
        let dir = TempDir::new("interval");
        let store = RecoveryStore::at(dir.path());
        let mut autosave = Autosave::new(store.clone(), Duration::from_secs(30));
        let mut doc = Document::demo();
        rename_first_node(&mut doc);
        let t0 = Instant::now();
        assert_eq!(autosave.due(t0, &doc, None, None), AutosaveOutcome::Idle);
        assert!(!store.has_snapshot());
        let t1 = t0 + Duration::from_secs(31);
        assert_eq!(autosave.due(t1, &doc, None, None), AutosaveOutcome::Saved);
        assert!(store.has_snapshot());
    }

    #[test]
    fn manual_save_resets_the_snapshot_clock() {
        let dir = TempDir::new("reset");
        let store = RecoveryStore::at(dir.path());
        let mut autosave = Autosave::new(store, Duration::from_secs(10));
        let mut doc = Document::demo();
        rename_first_node(&mut doc);
        let saved = Some(doc.signature());
        autosave.note_saved(saved);
        assert_eq!(
            autosave.due(Instant::now(), &doc, saved, None),
            AutosaveOutcome::Idle
        );
    }
}
