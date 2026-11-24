//! Menu-driven file, guard, recovery and about dialogs.

use super::*;

impl LeopardApp {
    pub(super) fn show_file_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        let mode = dialog.mode;
        let title = match mode {
            FileMode::Open => "Open Scene",
            FileMode::SaveAs => "Save Scene As",
            FileMode::Import => "Import Geometry",
            FileMode::Export => "Export Geometry",
        };
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Path");
                    ui.text_edit_singleline(&mut dialog.path);
                });
                ui.label(
                    egui::RichText::new("Supported: .lcscene, .obj, .stl, .gltf, .glb")
                        .small()
                        .weak(),
                );
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    if ui.button("OK").clicked() {
                        apply = true;
                    }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    apply = true;
                }
            });
        if cancel {
            open = false;
        }
        if apply {
            let path = dialog.path.clone();
            self.dialog = None;
            self.apply_file_dialog(ctx, mode, path);
        } else if !open {
            self.dialog = None;
            if matches!(mode, FileMode::SaveAs) {
                self.lifecycle.after_save_cancelled();
            }
        }
    }

    pub(super) fn show_guard_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.lifecycle.pending_action() else {
            return;
        };
        self.lifecycle.refresh_dirty(&self.doc, true);
        if !self.lifecycle.is_dirty() {
            self.lifecycle.clear_guard();
            self.execute_action(ctx, action);
            return;
        }
        let document_name = self.document_name();
        let prompt = action.prompt();
        let mut choice = None;
        egui::Window::new("Unsaved Changes")
            .id(egui::Id::new("unsaved_changes_dialog"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_min_width(340.0);
                ui.label(format!("“{document_name}” has unsaved changes."));
                ui.label(
                    egui::RichText::new(format!("Save before {prompt}?"))
                        .small()
                        .weak(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        choice = Some(GuardChoice::Save);
                    }
                    if ui.button("Don't Save").clicked() {
                        choice = Some(GuardChoice::Discard);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(GuardChoice::Cancel);
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            choice = Some(GuardChoice::Cancel);
        }
        match choice {
            Some(GuardChoice::Save) => {
                self.lifecycle.choose(GuardChoice::Save);
                self.save_or_prompt(ctx);
            }
            Some(GuardChoice::Discard) => {
                if let GuardResult::Run(action) = self.lifecycle.choose(GuardChoice::Discard) {
                    self.lifecycle.discard_snapshot();
                    self.execute_action(ctx, action);
                }
            }
            Some(GuardChoice::Cancel) => {
                self.lifecycle.choose(GuardChoice::Cancel);
                self.status = "Cancelled".into();
            }
            None => {}
        }
    }

    pub(super) fn show_recovery_dialog(&mut self, ctx: &egui::Context) {
        let Some(recovered) = self.lifecycle.recovery_prompt() else {
            return;
        };
        let origin = recovered
            .origin
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "an untitled scene".into());
        let age = recovered
            .saved_at
            .map(format_age)
            .unwrap_or_else(|| "an unknown time".into());
        let mut choice = None;
        egui::Window::new("Recover Unsaved Work")
            .id(egui::Id::new("recovery_dialog"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_min_width(400.0);
                ui.label(format!("LeopardCraft found unsaved work for {origin}."));
                ui.label(
                    egui::RichText::new(format!("Snapshot written {age}."))
                        .small()
                        .weak(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Recover").clicked() {
                        choice = Some(true);
                    }
                    if ui.button("Discard").clicked() {
                        choice = Some(false);
                    }
                });
            });
        match choice {
            Some(true) => {
                let Some(recovered) = self.lifecycle.take_recovery() else {
                    return;
                };
                let origin = recovered.origin.clone();
                self.load_scene(recovered.document, origin.clone());
                self.lifecycle.mark_recovered(origin);
                self.status = "Recovered unsaved scene from the previous session".into();
            }
            Some(false) => {
                self.lifecycle.discard_recovery();
                self.status = "Discarded recovered scene".into();
            }
            None => {}
        }
    }

    pub(super) fn show_about(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let mut open = true;
        egui::Window::new("About LeopardCraft")
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.heading("LeopardCraft");
                ui.label("Clean-room 3D modelling, rendering and animation in pure Rust.");
                ui.label("egui + wgpu · deterministic CPU path tracer · offline-first");
                ui.separator();
                ui.label(
                    egui::RichText::new(
                        "No proprietary code, binaries or file internals were used. \
                         Geometry, shading, animation and file formats are implemented from \
                         public specifications and original architecture.",
                    )
                    .small(),
                );
                ui.label(egui::RichText::new("MIT OR Apache-2.0").small().weak());
            });
        self.show_about = open;
    }
}
