//! Crash-recovery snapshots for unsaved documents.
//!
//! A snapshot is an ordinary `.lcscene` file, so the native format is
//! unchanged. Snapshots live in a per-user state directory and are removed
//! after a successful save, an explicit discard or a clean exit.

use lc_scene::Document;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const SNAPSHOT_FILE: &str = "recovery.lcscene";
const ORIGIN_FILE: &str = "recovery.origin";

#[derive(Clone, Debug)]
pub(super) struct RecoveredDocument {
    pub document: Document,
    pub origin: Option<PathBuf>,
    pub saved_at: Option<SystemTime>,
}

#[derive(Clone, Debug)]
pub(super) struct RecoveryStore {
    dir: PathBuf,
}

impl RecoveryStore {
    /// Platform state directory, overridable for tests and portable runs.
    pub(super) fn system() -> Self {
        if let Some(dir) = std::env::var_os("LEOPARDCRAFT_RECOVERY_DIR") {
            if !dir.is_empty() {
                return Self {
                    dir: PathBuf::from(dir),
                };
            }
        }
        if let Some(dir) = std::env::var_os("XDG_STATE_HOME") {
            if !dir.is_empty() {
                return Self {
                    dir: PathBuf::from(dir).join("leopardcraft"),
                };
            }
        }
        if let Some(home) = std::env::var_os("HOME") {
            if !home.is_empty() {
                return Self {
                    dir: PathBuf::from(home)
                        .join(".local")
                        .join("state")
                        .join("leopardcraft"),
                };
            }
        }
        if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
            if !dir.is_empty() {
                return Self {
                    dir: PathBuf::from(dir).join("LeopardCraft"),
                };
            }
        }
        Self {
            dir: std::env::temp_dir().join("leopardcraft-recovery"),
        }
    }

    pub(super) fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn scene_path(&self) -> PathBuf {
        self.dir.join(SNAPSHOT_FILE)
    }

    fn origin_path(&self) -> PathBuf {
        self.dir.join(ORIGIN_FILE)
    }

    pub(super) fn has_snapshot(&self) -> bool {
        self.scene_path().is_file()
    }

    pub(super) fn snapshot(&self) -> Result<Option<RecoveredDocument>, String> {
        let scene_path = self.scene_path();
        if !scene_path.is_file() {
            return Ok(None);
        }
        let document = lc_io::load_native(&scene_path)?;
        let origin = std::fs::read_to_string(self.origin_path())
            .ok()
            .map(|text| text.trim_end().to_string())
            .filter(|text| !text.is_empty())
            .map(PathBuf::from);
        let saved_at = std::fs::metadata(&scene_path)
            .and_then(|metadata| metadata.modified())
            .ok();
        Ok(Some(RecoveredDocument {
            document,
            origin,
            saved_at,
        }))
    }

    pub(super) fn write(&self, document: &Document, origin: Option<&Path>) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|error| {
            format!(
                "failed to create recovery directory {}: {error}",
                self.dir.display()
            )
        })?;
        let origin_text = origin
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        std::fs::write(self.origin_path(), origin_text)
            .map_err(|error| format!("failed to write recovery origin: {error}"))?;
        lc_io::save_native(self.scene_path(), document)
    }

    pub(super) fn discard(&self) {
        let _ = std::fs::remove_file(self.scene_path());
        let _ = std::fs::remove_file(self.origin_path());
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
                "leopardcraft-recovery-{}-{name}",
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

    #[test]
    fn missing_snapshot_is_none() {
        let dir = TempDir::new("missing");
        let store = RecoveryStore::at(dir.path());
        assert!(!store.has_snapshot());
        assert!(store.snapshot().unwrap().is_none());
    }

    #[test]
    fn snapshot_round_trips_document_and_origin() {
        let dir = TempDir::new("roundtrip");
        let store = RecoveryStore::at(dir.path());
        let doc = Document::demo();
        let origin = dir.path().join("studio scene.lcscene");

        store.write(&doc, Some(&origin)).unwrap();
        assert!(store.has_snapshot());

        let recovered = store.snapshot().unwrap().expect("snapshot exists");
        assert_eq!(recovered.document.signature(), doc.signature());
        assert_eq!(recovered.origin.as_deref(), Some(origin.as_path()));
        assert!(recovered.saved_at.is_some());

        store.discard();
        assert!(!store.has_snapshot());
        assert!(store.snapshot().unwrap().is_none());
    }

    #[test]
    fn a_newer_snapshot_replaces_the_previous_one() {
        let dir = TempDir::new("replace");
        let store = RecoveryStore::at(dir.path());
        let mut first = Document::demo();
        store
            .write(&first, Some(Path::new("first.lcscene")))
            .unwrap();

        let id = *first.scene.roots.first().unwrap();
        first.scene.get_mut(id).unwrap().name = "Replacement Scene".into();
        store.write(&first, None).unwrap();

        let recovered = store.snapshot().unwrap().unwrap();
        assert_eq!(recovered.document.signature(), first.signature());
        assert!(recovered.origin.is_none());
    }

    #[test]
    fn corrupt_snapshot_reports_an_error_and_can_be_discarded() {
        let dir = TempDir::new("corrupt");
        let store = RecoveryStore::at(dir.path());
        std::fs::write(store.scene_path(), b"not a scene").unwrap();
        assert!(store.has_snapshot());
        assert!(store.snapshot().is_err());
        store.discard();
        assert!(!store.has_snapshot());
    }
}
