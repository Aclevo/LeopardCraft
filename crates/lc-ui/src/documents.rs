//! New/open/import/save workflows, file dialogs, pending actions and autosave.

use super::*;

impl LeopardApp {
    pub(super) fn new_scene(&mut self) {
        self.load_scene(Document::demo(), None);
        self.lifecycle.discard_recovery();
        self.status = "New scene from studio template".into();
    }

    pub(super) fn open_file(&mut self, path: PathBuf) -> bool {
        reconcile_native_scene(&path);
        let opened = match lc_io::import_with_report(&path) {
            Ok((doc, report)) => match report.summary() {
                Some(summary) => {
                    self.pending_import = Some(PendingImport {
                        path,
                        doc,
                        summary,
                        replace_scene: true,
                    });
                    self.status =
                        "Review the import report before the open scene is replaced".into();
                    false
                }
                None => {
                    self.finish_open(path, doc, None);
                    true
                }
            },
            Err(e) => {
                self.status = format!("Open failed: {e}");
                false
            }
        };
        self.drain_scene_warnings();
        opened
    }

    pub(super) fn import_file(&mut self, path: PathBuf) {
        reconcile_native_scene(&path);
        match lc_io::import_with_report(&path) {
            Ok((mut doc, report)) => {
                doc.sanitize();
                if let Some(summary) = report.summary() {
                    self.pending_import = Some(PendingImport {
                        path,
                        doc,
                        summary,
                        replace_scene: false,
                    });
                    self.status = "Review the import report before importing".into();
                    return;
                }
                self.finish_import(path, doc, None);
            }
            Err(e) => self.status = format!("Import failed: {e}"),
        }
        self.drain_scene_warnings();
    }

    fn finish_open(&mut self, path: PathBuf, doc: Document, summary: Option<String>) {
        let name = path.display().to_string();
        self.load_scene(doc, Some(path));
        self.lifecycle.discard_recovery();
        self.status = match summary {
            Some(summary) => format!("Opened {name} — {summary}"),
            None => format!("Opened {name}"),
        };
    }

    fn finish_import(&mut self, path: PathBuf, doc: Document, summary: Option<String>) {
        let before = self.doc.clone();
        let after = doc;
        self.history.execute(
            &mut self.doc,
            Box::new(lc_scene::ReplaceDocumentCmd {
                label: "Import".into(),
                before,
                after,
            }),
        );
        self.selection.set(self.doc.scene.roots.first().copied());
        self.sync_playback_to_document();
        self.status = match summary {
            Some(summary) => format!("Imported {} — {summary}", path.display()),
            None => format!("Imported {}", path.display()),
        };
    }

    /// Accept the staged import the report dialog is describing.
    pub(super) fn apply_pending_import(&mut self) {
        let Some(pending) = self.pending_import.take() else {
            return;
        };
        if pending.replace_scene {
            self.finish_open(pending.path, pending.doc, Some(pending.summary));
        } else {
            self.finish_import(pending.path, pending.doc, Some(pending.summary));
        }
        self.drain_scene_warnings();
    }

    /// Drop the staged import and leave the current scene exactly as it was.
    pub(super) fn cancel_pending_import(&mut self) {
        if self.pending_import.take().is_some() {
            self.status = "Import cancelled; the scene was left unchanged".into();
        }
    }

    pub(super) fn save_file(&mut self, path: PathBuf) -> Result<Option<PendingAction>, ()> {
        let result = match self.lifecycle.save_and_take_action(&self.doc, path.clone()) {
            Ok((SaveKind::Native, action)) => {
                self.status = format!("Saved {}", path.display());
                Ok(action)
            }
            Ok((SaveKind::Export, action)) => {
                self.status = format!("Exported {}", path.display());
                Ok(action)
            }
            Err(error) => {
                self.status = format!("Save failed: {error}");
                Err(())
            }
        };
        self.drain_scene_warnings();
        result
    }

    pub(super) fn apply_file_dialog(&mut self, ctx: &egui::Context, mode: FileMode, text: String) {
        match mode {
            FileMode::SaveAs => match self.lifecycle.save_as_path(&text) {
                Ok(path) => {
                    if let Ok(Some(action)) = self.save_file(path) {
                        self.execute_action(ctx, action);
                    }
                }
                Err(message) => self.status = message,
            },
            FileMode::Open => {
                let Some(path) = self.dialog_path(&text) else {
                    return;
                };
                self.request_action(ctx, PendingAction::OpenPath(path));
            }
            FileMode::Import => {
                let Some(path) = self.dialog_path(&text) else {
                    return;
                };
                self.import_file(path);
            }
            FileMode::Export => {
                let Some(path) = self.dialog_path(&text) else {
                    return;
                };
                let _ = self.save_file(path);
            }
        }
    }

    pub(super) fn dialog_path(&mut self, text: &str) -> Option<PathBuf> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            self.status = "No file path entered".into();
            None
        } else {
            Some(PathBuf::from(trimmed))
        }
    }

    pub(super) fn save_or_prompt(&mut self, ctx: &egui::Context) {
        if self.lifecycle.needs_save_as() {
            self.dialog = Some(FileDialog {
                mode: FileMode::SaveAs,
                path: self.save_as_default(),
            });
        } else {
            let path = self
                .lifecycle
                .path()
                .expect("a native document has a path")
                .to_path_buf();
            if let Ok(Some(action)) = self.save_file(path) {
                self.execute_action(ctx, action);
            }
        }
    }

    pub(super) fn save_as_default(&self) -> String {
        self.lifecycle
            .path()
            .map(|path| path.with_extension("lcscene").display().to_string())
            .unwrap_or_else(|| "scene.lcscene".into())
    }

    pub(super) fn drain_scene_warnings(&mut self) {
        self.report_scene_warnings(lc_scene::take_warnings());
    }

    /// Show non-fatal scene recovery or directory-flush warnings in the editor.
    pub fn report_scene_warnings(&mut self, warnings: Vec<String>) {
        if warnings.is_empty() {
            return;
        }
        let stamp = self.started_at.elapsed().as_secs_f32();
        for warning in &warnings {
            self.console
                .push_back(format!("[{stamp:6.1}s] warning: {warning}"));
        }
        while self.console.len() > 250 {
            self.console.pop_front();
        }
        let summary = if warnings.len() == 1 {
            format!("Scene warning: {}", warnings[0])
        } else {
            format!("{} scene warnings — see Console", warnings.len())
        };
        self.status = if self.status.is_empty() {
            summary
        } else {
            format!("{} — {summary}", self.status)
        };
    }

    pub(super) fn document_name(&self) -> String {
        self.lifecycle
            .path()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| "Untitled".into())
    }

    pub(super) fn request_action(&mut self, ctx: &egui::Context, action: PendingAction) {
        match self.lifecycle.request(&self.doc, action) {
            GuardResult::Run(action) => self.execute_action(ctx, action),
            GuardResult::Confirm => {}
            GuardResult::Save | GuardResult::Cancelled => {}
        }
    }

    pub(super) fn execute_action(&mut self, ctx: &egui::Context, action: PendingAction) {
        match action {
            PendingAction::NewScene => self.new_scene(),
            PendingAction::OpenScene => {
                self.dialog = Some(FileDialog {
                    mode: FileMode::Open,
                    path: self
                        .lifecycle
                        .path()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "scene.lcscene".into()),
                });
            }
            PendingAction::OpenPath(path) => {
                self.open_file(path);
            }
            PendingAction::Quit => {
                self.lifecycle.finish_quit();
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    pub(super) fn autosave_tick(&mut self, ctx: &egui::Context) {
        match self.lifecycle.autosave_tick(&self.doc) {
            AutosaveOutcome::Idle | AutosaveOutcome::Clean | AutosaveOutcome::Saved => {}
            AutosaveOutcome::Failed(error) => {
                self.status = format!("Autosave failed: {error}");
            }
        }
        self.drain_scene_warnings();
        // Autosave needs a wake-up even when the user is idle and no input
        // events would otherwise trigger another frame.
        ctx.request_repaint_after(Duration::from_secs(1));
    }
}
