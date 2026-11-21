//! Non-interactive verification of the document lifecycle.
//!
//! The harness drives the same `DocumentLifecycle` controller that the egui
//! editor calls from its window-close handler, File menu, confirmation dialog,
//! autosave timer and startup recovery prompt. It is only reachable through
//! `--headless lifecycle` and the integration test, never during normal editor
//! startup. It creates its own temporary directory and leaves no state behind.

use super::lifecycle::{
    AutosaveOutcome, DocumentLifecycle, GuardChoice, GuardResult, PendingAction, SaveKind,
    StartupOutcome,
};
use super::recovery::RecoveryStore;
use lc_scene::Document;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Short interval so the harness can prove autosave by real elapsed time
/// rather than by calling the scheduler internals.
const AUTOSAVE_INTERVAL: Duration = Duration::from_millis(120);
const AUTOSAVE_WAIT: Duration = Duration::from_millis(240);

pub(super) fn run() -> Result<(), String> {
    let root = TempRoot::new()?;
    scenario_quit_decisions(&root)?;
    scenario_pathless_save_as(&root)?;
    scenario_write_failure(&root)?;
    scenario_autosave_timing(&root)?;
    scenario_startup_recovery(&root)?;
    println!(
        "lifecycle selftest: OK — close/quit guard, pathless Save As, write-failure recovery, \
         timed autosave, startup recovery and clean-exit cleanup exercised"
    );
    Ok(())
}

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new() -> Result<Self, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "leopardcraft-lifecycle-harness-{}-{stamp}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path)
            .map_err(|error| format!("create harness directory {}: {error}", path.display()))?;
        Ok(Self { path })
    }

    fn dir(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn ensure(condition: bool, message: impl Into<String>) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn pass(message: &str) {
    println!("PASS  {message}");
}

fn prepare_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| format!("create {}: {error}", path.display()))
}

fn new_lifecycle(recovery_dir: &Path) -> DocumentLifecycle {
    DocumentLifecycle::new(RecoveryStore::at(recovery_dir), AUTOSAVE_INTERVAL)
}

fn rename_root(doc: &mut Document, name: &str) {
    let id = *doc.scene.roots.first().expect("demo scene has roots");
    doc.scene.get_mut(id).expect("root node exists").name = name.into();
}

fn seed_scene(path: &Path, doc: &Document) -> Result<(), String> {
    lc_io::save_native(path, doc).map_err(|error| format!("seed {}: {error}", path.display()))
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))
}

fn load_scene(path: &Path) -> Result<Document, String> {
    lc_io::load_native(path).map_err(|error| format!("load {}: {error}", path.display()))
}

fn request_quit(
    lifecycle: &mut DocumentLifecycle,
    doc: &Document,
    label: &str,
) -> Result<(), String> {
    let result = lifecycle.request(doc, PendingAction::Quit);
    ensure(
        result == GuardResult::Confirm,
        format!("{label}: expected a pending confirmation, got {result:?}"),
    )?;
    ensure(
        lifecycle.pending_action() == Some(PendingAction::Quit),
        format!("{label}: quit was not the pending action"),
    )
}

fn wait_for_autosave(
    lifecycle: &mut DocumentLifecycle,
    doc: &Document,
    label: &str,
) -> Result<(), String> {
    thread::sleep(AUTOSAVE_WAIT);
    match lifecycle.autosave_tick(doc) {
        AutosaveOutcome::Saved => Ok(()),
        other => Err(format!("{label}: expected a snapshot, got {other:?}")),
    }
}

fn scenario_quit_decisions(root: &TempRoot) -> Result<(), String> {
    let dir = root.dir("quit");
    prepare_dir(&dir)?;
    let scene_path = dir.join("studio.lcscene");
    let base = Document::demo();
    seed_scene(&scene_path, &base)?;
    let original = read_bytes(&scene_path)?;

    let mut lifecycle = new_lifecycle(&dir.join("recovery"));
    lifecycle.startup(&base);
    lifecycle.loaded(&base, Some(scene_path.clone()));

    let mut doc = base.clone();
    rename_root(&mut doc, "Edited Cube");

    // The window-close handler and File -> Quit both converge on this request.
    request_quit(&mut lifecycle, &doc, "window close")?;
    request_quit(&mut lifecycle, &doc, "File -> Quit")?;
    pass("dirty window-close and File -> Quit produce one pending confirmation");

    let cancelled = lifecycle.choose(GuardChoice::Cancel);
    ensure(
        cancelled == GuardResult::Cancelled,
        format!("Cancel returned {cancelled:?}"),
    )?;
    ensure(lifecycle.is_dirty(), "Cancel must keep the document dirty")?;
    ensure(
        lifecycle.pending_action().is_none(),
        "Cancel left a pending action behind",
    )?;
    ensure(
        read_bytes(&scene_path)? == original,
        "Cancel wrote to the scene file",
    )?;
    pass("Cancel keeps the editor alive and dirty without writing");

    request_quit(&mut lifecycle, &doc, "Don't Save")?;
    let discarded = lifecycle.choose(GuardChoice::Discard);
    ensure(
        discarded == GuardResult::Run(PendingAction::Quit),
        format!("Don't Save returned {discarded:?}"),
    )?;
    lifecycle.finish_quit();
    ensure(
        read_bytes(&scene_path)? == original,
        "Don't Save changed the scene file",
    )?;
    pass("Don't Save exits and leaves the .lcscene byte-identical");

    lifecycle.loaded(&base, Some(scene_path.clone()));
    request_quit(&mut lifecycle, &doc, "Save + quit")?;
    ensure(
        lifecycle.choose(GuardChoice::Save) == GuardResult::Save,
        "Save did not take the confirmation",
    )?;
    let (kind, action) = lifecycle
        .save_and_take_action(&doc, scene_path.clone())
        .map_err(|error| format!("Save failed: {error}"))?;
    ensure(
        kind == SaveKind::Native,
        "Save did not write a native scene",
    )?;
    ensure(
        action == Some(PendingAction::Quit),
        "the queued quit did not run after a successful save",
    )?;
    ensure(!lifecycle.is_dirty(), "document is still dirty after Save")?;
    lifecycle.finish_quit();
    let on_disk = load_scene(&scene_path)?;
    ensure(
        on_disk.signature() == doc.signature(),
        "the saved file does not contain the dirty document",
    )?;
    pass("Save writes the scene, cleans the document and exits");
    Ok(())
}

fn scenario_pathless_save_as(root: &TempRoot) -> Result<(), String> {
    let dir = root.dir("save-as");
    prepare_dir(&dir)?;
    let mut lifecycle = new_lifecycle(&dir.join("recovery"));
    let base = Document::demo();
    lifecycle.loaded(&base, None);

    let mut doc = base.clone();
    rename_root(&mut doc, "Pathless Cube");
    request_quit(&mut lifecycle, &doc, "pathless quit")?;
    ensure(
        lifecycle.choose(GuardChoice::Save) == GuardResult::Save,
        "pathless Save did not enter the Save As flow",
    )?;
    ensure(
        lifecycle.needs_save_as(),
        "a pathless document should require Save As",
    )?;

    let empty = lifecycle.save_as_path("   ");
    ensure(empty.is_err(), "an empty Save As target should be rejected")?;
    ensure(
        lifecycle.pending_action() == Some(PendingAction::Quit),
        "empty Save As did not restore the confirmation",
    )?;
    ensure(
        lifecycle.is_dirty(),
        "empty Save As cleared the dirty state",
    )?;
    ensure(
        lifecycle.saved().is_none(),
        "empty Save As released the queued action",
    )?;
    pass("empty Save As target is a failed save and returns to the confirmation");

    ensure(
        lifecycle.choose(GuardChoice::Save) == GuardResult::Save,
        "cancelled Save As did not re-enter the save flow",
    )?;
    lifecycle.after_save_cancelled();
    ensure(
        lifecycle.pending_action() == Some(PendingAction::Quit),
        "cancelled Save As did not restore the confirmation",
    )?;
    ensure(
        lifecycle.is_dirty(),
        "cancelled Save As cleared the dirty state",
    )?;
    ensure(
        lifecycle.saved().is_none(),
        "cancelled Save As released the queued action",
    )?;
    pass("Save on a pathless document routes to Save As; cancelling returns to the confirmation");

    let missing = dir.join("missing").join("scene.lcscene");
    ensure(
        lifecycle.choose(GuardChoice::Save) == GuardResult::Save,
        "failed Save As did not re-enter the save flow",
    )?;
    let failed = lifecycle.save_and_take_action(&doc, missing);
    ensure(
        failed.is_err(),
        "saving into a missing directory should fail",
    )?;
    ensure(
        lifecycle.pending_action() == Some(PendingAction::Quit),
        "failed Save As did not restore the confirmation",
    )?;
    ensure(
        lifecycle.is_dirty(),
        "failed Save As lost the dirty document",
    )?;
    ensure(
        lifecycle.saved().is_none(),
        "failed Save As released the queued action",
    )?;
    pass("failed Save As returns to the confirmation with the document intact");

    let target = dir.join("pathless.lcscene");
    ensure(
        lifecycle.choose(GuardChoice::Save) == GuardResult::Save,
        "Save As success did not start the save",
    )?;
    let (kind, action) = lifecycle
        .save_and_take_action(&doc, target.clone())
        .map_err(|error| format!("Save As failed: {error}"))?;
    ensure(kind == SaveKind::Native, "Save As wrote a non-native file")?;
    ensure(
        action == Some(PendingAction::Quit),
        "the queued action did not run after a successful Save As",
    )?;
    ensure(
        !lifecycle.is_dirty(),
        "document is still dirty after Save As",
    )?;
    let on_disk = load_scene(&target)?;
    ensure(
        on_disk.signature() == doc.signature(),
        "Save As did not persist the dirty scene",
    )?;
    pass("Save As persists a pathless scene and only then runs the queued action");
    Ok(())
}

fn scenario_write_failure(root: &TempRoot) -> Result<(), String> {
    let dir = root.dir("write-failure");
    prepare_dir(&dir)?;
    let mut lifecycle = new_lifecycle(&dir.join("recovery"));
    let base = Document::demo();
    lifecycle.loaded(&base, None);
    let mut doc = base.clone();
    rename_root(&mut doc, "Unwritable Cube");

    request_quit(&mut lifecycle, &doc, "write-failure quit")?;
    ensure(
        lifecycle.choose(GuardChoice::Save) == GuardResult::Save,
        "write-failure Save did not take the confirmation",
    )?;

    let target = dir.join("missing").join("scene.lcscene");
    let error = lifecycle
        .save_and_take_action(&doc, target.clone())
        .expect_err("a save into a missing directory should fail");
    ensure(
        error.contains("failed to write"),
        format!("write failure was not surfaced: {error}"),
    )?;
    ensure(
        lifecycle.pending_action() == Some(PendingAction::Quit),
        "write failure did not restore the pending quit",
    )?;
    ensure(
        lifecycle.is_dirty(),
        "write failure cleared the document's dirty state",
    )?;
    ensure(
        lifecycle.saved().is_none(),
        "write failure released the queued action",
    )?;
    ensure(!target.exists(), "write failure left a target file behind")?;

    // A directory at the target path fails during the final rename after the
    // temporary scene has been written. The target must stay untouched and
    // the temporary file must be removed.
    ensure(
        lifecycle.choose(GuardChoice::Save) == GuardResult::Save,
        "second write-failure Save did not re-enter the save flow",
    )?;
    let blocked = dir.join("blocked.lcscene");
    fs::create_dir(&blocked)
        .map_err(|error| format!("create blocked target {}: {error}", blocked.display()))?;
    let error = lifecycle
        .save_and_take_action(&doc, blocked.clone())
        .expect_err("saving over a directory should fail");
    ensure(
        lifecycle.pending_action() == Some(PendingAction::Quit),
        "rename failure did not restore the pending quit",
    )?;
    ensure(
        lifecycle.is_dirty(),
        "rename failure cleared the document's dirty state",
    )?;
    ensure(
        blocked.is_dir(),
        "rename failure replaced the directory target",
    )?;
    let mut leftovers = Vec::new();
    for entry in fs::read_dir(&dir)
        .map_err(|error| format!("read harness directory {}: {error}", dir.display()))?
    {
        let entry = entry.map_err(|error| format!("read harness entry: {error}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(".blocked.lcscene.tmp-") {
            leftovers.push(name);
        }
    }
    ensure(
        leftovers.is_empty(),
        format!("failed write left temporary files: {leftovers:?}"),
    )?;

    pass(&format!(
        "write failure restores the confirmation, keeps the document dirty and leaves no partial file ({error})"
    ));
    Ok(())
}

fn scenario_autosave_timing(root: &TempRoot) -> Result<(), String> {
    let dir = root.dir("autosave");
    prepare_dir(&dir)?;
    let recovery_dir = dir.join("recovery");
    let scene_path = dir.join("timed.lcscene");
    let base = Document::demo();
    seed_scene(&scene_path, &base)?;

    let mut lifecycle = new_lifecycle(&recovery_dir);
    lifecycle.startup(&base);
    lifecycle.loaded(&base, Some(scene_path.clone()));
    let mut doc = base.clone();
    rename_root(&mut doc, "Timed A");

    wait_for_autosave(&mut lifecycle, &doc, "first snapshot")?;
    pass("dirty document writes a recovery snapshot after the autosave interval");

    lifecycle
        .save_document(&doc, scene_path.clone())
        .map_err(|error| format!("autosave cleanup save: {error}"))?;
    ensure(
        !lifecycle.has_snapshot(),
        "a successful native save left a snapshot",
    )?;
    pass("successful native save removes the snapshot");

    rename_root(&mut doc, "Timed B");
    wait_for_autosave(&mut lifecycle, &doc, "New/Open snapshot")?;
    lifecycle.loaded(&base, None);
    lifecycle.discard_recovery();
    ensure(!lifecycle.has_snapshot(), "New/Open left a snapshot behind")?;
    pass("New/Open clears the snapshot");

    rename_root(&mut doc, "Timed C");
    wait_for_autosave(&mut lifecycle, &doc, "discard snapshot")?;
    lifecycle.discard_snapshot();
    ensure(
        !lifecycle.has_snapshot(),
        "explicit discard left a snapshot behind",
    )?;
    pass("explicit discard removes the snapshot");

    lifecycle.loaded(&base, Some(scene_path.clone()));
    lifecycle
        .save_document(&base, scene_path.clone())
        .map_err(|error| format!("clean-exit save: {error}"))?;
    lifecycle.finish_quit();
    ensure(
        !lifecycle.has_snapshot(),
        "a clean exit left a snapshot behind",
    )?;
    pass("clean exit with no unsaved work leaves no snapshot");

    let mut unanswered = base.clone();
    rename_root(&mut unanswered, "Timed D");
    lifecycle.loaded(&base, Some(scene_path));
    wait_for_autosave(&mut lifecycle, &unanswered, "unanswered snapshot")?;
    ensure(lifecycle.has_snapshot(), "missing snapshot before restart")?;

    let mut booted = new_lifecycle(&recovery_dir);
    ensure(
        booted.startup(&base) == StartupOutcome::Recoverable,
        "startup did not offer the snapshot",
    )?;
    booted.autosave_tick(&base);
    booted.finish_quit();
    ensure(
        booted.has_snapshot(),
        "quitting with the recovery prompt unanswered deleted the snapshot",
    )?;
    pass("quitting with the recovery prompt unanswered keeps the snapshot");
    Ok(())
}

fn scenario_startup_recovery(root: &TempRoot) -> Result<(), String> {
    let dir = root.dir("recovery");
    prepare_dir(&dir)?;
    let recovery_dir = dir.join("store");
    let origin = dir.join("recovered.lcscene");
    let base = Document::demo();
    let mut dirty = base.clone();
    rename_root(&mut dirty, "Recovered Cube");

    let mut writer = new_lifecycle(&recovery_dir);
    writer.loaded(&base, Some(origin.clone()));
    wait_for_autosave(&mut writer, &dirty, "recovery snapshot")?;

    let mut booted = new_lifecycle(&recovery_dir);
    ensure(
        booted.startup(&base) == StartupOutcome::Recoverable,
        "startup did not offer the snapshot",
    )?;
    let prompt = booted
        .recovery_prompt()
        .ok_or_else(|| "Recoverable startup left no prompt".to_string())?;
    ensure(
        prompt.origin.as_deref() == Some(origin.as_path()),
        "the recovery prompt lost the snapshot origin",
    )?;
    ensure(
        booted.has_snapshot(),
        "startup removed the snapshot before the user answered",
    )?;
    pass("startup offers Recover/Discard with the snapshot origin");

    booted.discard_recovery();
    ensure(!booted.has_snapshot(), "Discard left the snapshot on disk")?;
    ensure(
        booted.recovery_prompt().is_none(),
        "Discard left the prompt open",
    )?;
    pass("Discard removes the snapshot and the offer");

    let mut writer = new_lifecycle(&recovery_dir);
    writer.loaded(&base, Some(origin.clone()));
    wait_for_autosave(&mut writer, &dirty, "second recovery snapshot")?;

    let mut booted = new_lifecycle(&recovery_dir);
    ensure(
        booted.startup(&base) == StartupOutcome::Recoverable,
        "second startup did not offer the snapshot",
    )?;
    let recovered = booted
        .take_recovery()
        .ok_or_else(|| "no recovered document".to_string())?;
    ensure(
        recovered.document.signature() == dirty.signature(),
        "the recovered document does not contain the snapshot content",
    )?;
    ensure(
        booted.has_snapshot(),
        "the snapshot disappeared before it was consumed",
    )?;
    booted.mark_recovered(recovered.origin.clone());
    ensure(booted.is_dirty(), "the recovered document was marked clean")?;
    ensure(
        booted.path() == Some(origin.as_path()),
        "the recovered document lost its origin path",
    )?;
    ensure(
        !booted.has_snapshot(),
        "the consumed snapshot is still on disk",
    )?;
    pass("Recover yields a dirty document with the recovered content and consumes the snapshot");
    Ok(())
}
