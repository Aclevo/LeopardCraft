//! The Cheetah-style application shell: light chrome, grouped icon toolbar,
//! view switcher, right-hand inspector/browser column, material shelf,
//! full-width timeline and status bar.

use super::*;
use crate::icons::{self, Icon};
use crate::theme;

impl LeopardApp {
    pub(super) fn shell(&mut self, root: &mut egui::Ui) {
        let menu_frame = egui::Frame::new()
            .fill(theme::MENU)
            .inner_margin(egui::Margin::symmetric(4, 2));
        egui::Panel::top("menubar")
            .exact_size(22.0)
            .frame(menu_frame)
            .show(root, |ui| self.cheetah_menu_bar(ui));

        let toolbar_frame = egui::Frame::new()
            .fill(theme::TOOLBAR)
            .inner_margin(egui::Margin::symmetric(6, 3))
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(201, 201, 201)));
        egui::Panel::top("toolbar")
            .exact_size(48.0)
            .frame(toolbar_frame)
            .show(root, |ui| self.toolbar(ui));

        let status_frame = egui::Frame::new()
            .fill(theme::CHROME)
            .inner_margin(egui::Margin::symmetric(6, 1))
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(205, 205, 205)));
        egui::Panel::bottom("statusbar")
            .exact_size(20.0)
            .frame(status_frame)
            .show(root, |ui| self.cheetah_status_bar(ui));

        let (timeline_id, timeline_height) = if self.show_curves {
            ("timeline_curves", 206.0)
        } else {
            ("timeline_closed", 46.0)
        };
        let timeline_frame = egui::Frame::new()
            .fill(Color32::from_rgb(214, 214, 214))
            .inner_margin(egui::Margin::symmetric(4, 0))
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(184, 184, 184)));
        egui::Panel::bottom(timeline_id)
            .exact_size(timeline_height)
            .frame(timeline_frame)
            .show(root, |ui| self.ui_timeline(ui));

        let right_frame = egui::Frame::new()
            .fill(theme::PANEL)
            .inner_margin(egui::Margin::ZERO)
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(185, 185, 185)));
        egui::Panel::right("inspector_column")
            .resizable(true)
            .default_size(292.0)
            .min_size(252.0)
            .max_size(440.0)
            .frame(right_frame)
            .show(root, |ui| {
                let properties_frame = egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::symmetric(6, 4));
                egui::Panel::top("properties_panel")
                    .resizable(true)
                    .default_size((ui.available_height() * 0.61).max(238.0))
                    .min_size(160.0)
                    .frame(properties_frame)
                    .show(ui, |ui| self.ui_inspector(ui));
                let browser_frame = egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::symmetric(4, 2));
                egui::CentralPanel::default()
                    .frame(browser_frame)
                    .show(ui, |ui| self.ui_browser(ui));
            });

        let central_frame = egui::Frame::new()
            .fill(theme::PANEL)
            .inner_margin(egui::Margin::ZERO);
        egui::CentralPanel::default()
            .frame(central_frame)
            .show(root, |ui| {
                let shelf_frame = egui::Frame::new()
                    .fill(Color32::from_rgb(233, 233, 233))
                    .inner_margin(egui::Margin::symmetric(6, 2))
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(190, 190, 190)));
                egui::Panel::bottom("materials_shelf")
                    .exact_size(116.0)
                    .frame(shelf_frame)
                    .show(ui, |ui| self.ui_material_shelf(ui));
                self.ui_editor(ui);
            });

        let ctx = root.ctx().clone();
        self.ui_render_manager_window(&ctx);
    }

    fn ui_editor(&mut self, ui: &mut egui::Ui) {
        let tabs_frame = egui::Frame::new()
            .fill(Color32::from_rgb(229, 229, 229))
            .inner_margin(egui::Margin::symmetric(5, 2))
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(198, 198, 198)));
        egui::Panel::top("editor_tabs")
            .exact_size(26.0)
            .frame(tabs_frame)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    theme::tabs(
                        ui,
                        &mut self.editor_view,
                        &[
                            ("3D view", EditorView::Scene3d),
                            ("UVs", EditorView::Uv),
                            ("Nodes", EditorView::Nodes),
                        ],
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(match self.editor_view {
                                EditorView::Scene3d => "Perspective",
                                EditorView::Uv => "UV editor",
                                EditorView::Nodes => "Node editor",
                            })
                            .size(10.0)
                            .color(theme::TEXT_WEAK),
                        );
                    });
                });
            });

        let bar_frame = egui::Frame::new()
            .fill(theme::PANEL)
            .inner_margin(egui::Margin::symmetric(6, 1));
        egui::Panel::top("view_bar")
            .exact_size(24.0)
            .frame(bar_frame)
            .show(ui, |ui| self.view_bar(ui));

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::PANEL))
            .show(ui, |ui| match self.editor_view {
                EditorView::Scene3d => self.ui_viewport(ui),
                EditorView::Uv => self.ui_uv(ui),
                EditorView::Nodes => self.ui_node_editor(ui),
            });
    }

    fn view_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| match self.editor_view {
            EditorView::Scene3d => {
                ui.label(
                    egui::RichText::new("Camera")
                        .size(10.5)
                        .color(theme::TEXT_WEAK),
                );
                egui::ComboBox::from_id_salt("viewport_camera")
                    .width(106.0)
                    .selected_text(self.view_label.clone())
                    .show_ui(ui, |ui| {
                        for preset in ["Viewport", "Front", "Top", "Right", "Left"] {
                            if ui
                                .selectable_label(self.view_label == preset, preset)
                                .clicked()
                            {
                                self.apply_view_preset(preset);
                            }
                        }
                    });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.menu_button(self.shading.label(), |ui| {
                        for mode in [
                            ShadingMode::Shaded,
                            ShadingMode::Wireframe,
                            ShadingMode::ShadedWireframe,
                        ] {
                            if ui
                                .selectable_label(self.shading == mode, mode.label())
                                .clicked()
                            {
                                self.shading = mode;
                            }
                        }
                        ui.separator();
                        ui.checkbox(&mut self.show_helpers, "Light / camera helpers");
                    });
                    if icons::icon_button_toggle(ui, Icon::Grid, "Show grid", self.show_grid, true)
                        .clicked()
                    {
                        self.show_grid = !self.show_grid;
                    }
                    if icons::icon_button(ui, Icon::ZoomIn, "Zoom in", true).clicked() {
                        self.orbit.distance = (self.orbit.distance * 0.86).max(0.2);
                        self.view_label = "Viewport".into();
                    }
                    if icons::icon_button(ui, Icon::Fit, "Frame selection", true).clicked() {
                        self.frame_selected();
                    }
                    if icons::icon_button(ui, Icon::ZoomOut, "Zoom out", true).clicked() {
                        self.orbit.distance = (self.orbit.distance * 1.16).min(500.0);
                        self.view_label = "Viewport".into();
                    }
                });
            }
            EditorView::Uv => {
                ui.label(
                    egui::RichText::new("UV set UV1")
                        .size(10.5)
                        .color(theme::TEXT_WEAK),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(
                            "Projection tools in the Properties panel · drag points to edit",
                        )
                        .size(10.0)
                        .color(theme::TEXT_WEAK),
                    );
                });
            }
            EditorView::Nodes => {
                let name = self
                    .selected_material
                    .and_then(|id| self.doc.materials.get(id))
                    .map(|m| m.name.clone())
                    .unwrap_or_else(|| "No material".into());
                ui.label(
                    egui::RichText::new(format!("Material: {name}"))
                        .size(10.5)
                        .color(theme::TEXT_WEAK),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("Drag nodes · connect outputs · delete removes")
                            .size(10.0)
                            .color(theme::TEXT_WEAK),
                    );
                });
            }
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let mesh_ready = self.selected_is_mesh();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            if icons::icon_button(ui, Icon::Select, "Select / object mode", true).clicked() {
                self.leave_mesh_edit();
            }
            if icons::icon_button_toggle(
                ui,
                Icon::Move,
                "Move gizmo (W)",
                self.gizmo == GizmoMode::Translate,
                true,
            )
            .clicked()
            {
                self.gizmo = GizmoMode::Translate;
            }
            if icons::icon_button_toggle(
                ui,
                Icon::Rotate,
                "Rotate gizmo (E)",
                self.gizmo == GizmoMode::Rotate,
                true,
            )
            .clicked()
            {
                self.gizmo = GizmoMode::Rotate;
            }
            if icons::icon_button_toggle(
                ui,
                Icon::Scale,
                "Scale gizmo (R)",
                self.gizmo == GizmoMode::Scale,
                true,
            )
            .clicked()
            {
                self.gizmo = GizmoMode::Scale;
            }
            theme::toolbar_sep(ui);
            if icons::icon_button_toggle(ui, Icon::ObjectMode, "Object mode", !self.mesh_edit, true)
                .clicked()
            {
                self.leave_mesh_edit();
            }
            for (icon, mode, tip) in [
                (Icon::PointMode, MeshSelectMode::Points, "Point mode"),
                (Icon::EdgeMode, MeshSelectMode::Edges, "Edge mode"),
                (Icon::PolygonMode, MeshSelectMode::Polygons, "Polygon mode"),
            ] {
                if icons::icon_button_toggle(
                    ui,
                    icon,
                    tip,
                    self.mesh_edit && self.mesh_select_mode == mode,
                    mesh_ready,
                )
                .clicked()
                {
                    self.set_mesh_mode(mode);
                }
            }
            theme::toolbar_sep(ui);
            if icons::icon_button(ui, Icon::Box, "Add box", true).clicked() {
                self.add_primitive(Primitive::Box {
                    width: 1.0,
                    height: 1.0,
                    depth: 1.0,
                });
            }
            if icons::icon_button(ui, Icon::Ball, "Add ball", true).clicked() {
                self.add_primitive(Primitive::Sphere {
                    radius: 1.0,
                    segments: 32,
                    rings: 16,
                });
            }
            if icons::icon_button(ui, Icon::Cylinder, "Add cylinder", true).clicked() {
                self.add_primitive(Primitive::Cylinder {
                    radius: 0.7,
                    height: 2.0,
                    segments: 32,
                });
            }
            if icons::icon_button(ui, Icon::Cone, "Add cone", true).clicked() {
                self.add_primitive(Primitive::Cone {
                    radius: 0.8,
                    height: 2.0,
                    segments: 32,
                });
            }
            if icons::icon_button(ui, Icon::Torus, "Add torus", true).clicked() {
                self.add_primitive(Primitive::Torus {
                    major_radius: 1.0,
                    minor_radius: 0.28,
                    segments: 40,
                    rings: 16,
                });
            }
            if icons::icon_button(ui, Icon::Plane, "Add plane", true).clicked() {
                self.add_primitive(Primitive::Plane {
                    width: 4.0,
                    depth: 4.0,
                    segments_x: 1,
                    segments_z: 1,
                });
            }
            if icons::icon_button(ui, Icon::Spline, "Add spline", true).clicked() {
                self.add_spline();
            }
            theme::toolbar_sep(ui);
            if icons::icon_button(ui, Icon::Folder, "Add null / folder", true).clicked() {
                self.add_null();
            }
            if icons::icon_button(ui, Icon::Joint, "Add joint", true).clicked() {
                self.add_joint();
            }
            if icons::icon_button(ui, Icon::Camera, "Add camera", true).clicked() {
                self.add_camera();
            }
            if icons::icon_button(ui, Icon::PointLight, "Add point light", true).clicked() {
                self.add_light(LightKind::Point { radius: 1.0 });
            }
            if icons::icon_button(ui, Icon::AreaLight, "Add area light", true).clicked() {
                self.add_light(LightKind::Area {
                    width: 2.0,
                    height: 1.5,
                });
            }
            theme::toolbar_sep(ui);
            if icons::icon_button(ui, Icon::Subdivide, "Add subdivision modifier", mesh_ready)
                .clicked()
            {
                self.add_subdivision();
            }
            theme::toolbar_sep(ui);
            if icons::icon_button(ui, Icon::Render, "Render frame", true).clicked() {
                self.start_render(ui.ctx());
            }
            if icons::icon_button(ui, Icon::Manager, "Render Manager", true).clicked() {
                self.show_render_manager = true;
            }
            if icons::icon_button(ui, Icon::Save, "Save scene", true).clicked() {
                self.save_or_prompt(ui.ctx());
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.hierarchy_filter)
                        .hint_text("Search")
                        .desired_width(150.0),
                );
                if std::mem::take(&mut self.focus_search) {
                    response.request_focus();
                }
                if response.changed() {
                    self.browser_tab = BrowserTab::Objects;
                }
                let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                icons::paint(
                    ui.painter(),
                    icon_rect.shrink(1.0),
                    Icon::Search,
                    theme::TEXT_WEAK,
                );
            });
        });
    }

    fn cheetah_menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                ui.set_min_width(190.0);
                if ui.button("New Studio Scene").clicked() {
                    self.request_action(ui.ctx(), PendingAction::NewScene);
                }
                if ui.button("Open…").clicked() {
                    self.request_action(ui.ctx(), PendingAction::OpenScene);
                }
                if ui.button("Save").clicked() {
                    self.save_or_prompt(ui.ctx());
                }
                if ui.button("Save As…").clicked() {
                    self.dialog = Some(FileDialog {
                        mode: FileMode::SaveAs,
                        path: "scene.lcscene".into(),
                    });
                }
                ui.separator();
                if ui.button("Import…").clicked() {
                    self.dialog = Some(FileDialog {
                        mode: FileMode::Import,
                        path: "model.obj".into(),
                    });
                }
                if ui.button("Export…").clicked() {
                    self.dialog = Some(FileDialog {
                        mode: FileMode::Export,
                        path: "export.glb".into(),
                    });
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    self.request_action(ui.ctx(), PendingAction::Quit);
                }
            });
            ui.menu_button("Edit", |ui| {
                ui.set_min_width(190.0);
                let undo_label = self
                    .history
                    .undo_label()
                    .map(|label| format!("Undo {label}"))
                    .unwrap_or_else(|| "Undo".into());
                if ui
                    .add_enabled(self.history.can_undo(), egui::Button::new(undo_label))
                    .clicked()
                {
                    self.undo();
                }
                let redo_label = self
                    .history
                    .redo_label()
                    .map(|label| format!("Redo {label}"))
                    .unwrap_or_else(|| "Redo".into());
                if ui
                    .add_enabled(self.history.can_redo(), egui::Button::new(redo_label))
                    .clicked()
                {
                    self.redo();
                }
                ui.separator();
                if ui
                    .add_enabled(self.selected.is_some(), egui::Button::new("Duplicate"))
                    .clicked()
                {
                    self.duplicate_selected();
                }
                if ui
                    .add_enabled(self.selected.is_some(), egui::Button::new("Delete"))
                    .clicked()
                {
                    self.delete_selected();
                }
            });
            ui.menu_button("Selection", |ui| {
                ui.set_min_width(190.0);
                if ui.button("Object Mode").clicked() {
                    self.leave_mesh_edit();
                }
                if ui.button("Point Mode").clicked() {
                    self.set_mesh_mode(MeshSelectMode::Points);
                }
                if ui.button("Edge Mode").clicked() {
                    self.set_mesh_mode(MeshSelectMode::Edges);
                }
                if ui.button("Polygon Mode").clicked() {
                    self.set_mesh_mode(MeshSelectMode::Polygons);
                }
                ui.separator();
                if ui.button("Select All Components").clicked() {
                    self.perform_mesh_action(MeshAction::SelectAll);
                }
                if ui.button("Deselect All Components").clicked() {
                    self.perform_mesh_action(MeshAction::SelectNone);
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.selected.is_some(),
                        egui::Button::new("Convert to Editable Mesh"),
                    )
                    .clicked()
                {
                    self.make_selected_editable();
                }
            });
            ui.menu_button("Objects", |ui| {
                ui.set_min_width(190.0);
                if ui.button("Box").clicked() {
                    self.add_primitive(Primitive::Box {
                        width: 1.0,
                        height: 1.0,
                        depth: 1.0,
                    });
                }
                if ui.button("Ball").clicked() {
                    self.add_primitive(Primitive::Sphere {
                        radius: 1.0,
                        segments: 32,
                        rings: 16,
                    });
                }
                if ui.button("Cylinder").clicked() {
                    self.add_primitive(Primitive::Cylinder {
                        radius: 0.7,
                        height: 2.0,
                        segments: 32,
                    });
                }
                if ui.button("Cone").clicked() {
                    self.add_primitive(Primitive::Cone {
                        radius: 0.8,
                        height: 2.0,
                        segments: 32,
                    });
                }
                if ui.button("Torus").clicked() {
                    self.add_primitive(Primitive::Torus {
                        major_radius: 1.0,
                        minor_radius: 0.28,
                        segments: 40,
                        rings: 16,
                    });
                }
                if ui.button("Plane").clicked() {
                    self.add_primitive(Primitive::Plane {
                        width: 4.0,
                        depth: 4.0,
                        segments_x: 1,
                        segments_z: 1,
                    });
                }
                if ui.button("Spline").clicked() {
                    self.add_spline();
                }
                ui.separator();
                if ui.button("Folder / Null").clicked() {
                    self.add_null();
                }
                if ui.button("Joint").clicked() {
                    self.add_joint();
                }
                if ui.button("Camera").clicked() {
                    self.add_camera();
                }
                if ui.button("Point Light").clicked() {
                    self.add_light(LightKind::Point { radius: 1.0 });
                }
                if ui.button("Area Light").clicked() {
                    self.add_light(LightKind::Area {
                        width: 2.0,
                        height: 1.5,
                    });
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.selected.is_some(),
                        egui::Button::new("Convert to Editable Mesh"),
                    )
                    .clicked()
                {
                    self.make_selected_editable();
                }
                if ui
                    .add_enabled(
                        self.selected.is_some(),
                        egui::Button::new("Add Subdivision"),
                    )
                    .clicked()
                {
                    self.add_subdivision();
                }
                if ui
                    .add_enabled(
                        self.selected.is_some(),
                        egui::Button::new("Add Boolean Subtract"),
                    )
                    .clicked()
                {
                    self.add_boolean();
                }
            });
            ui.menu_button("Tools", |ui| {
                ui.set_min_width(190.0);
                if ui
                    .selectable_label(self.gizmo == GizmoMode::Translate, "Move")
                    .clicked()
                {
                    self.gizmo = GizmoMode::Translate;
                }
                if ui
                    .selectable_label(self.gizmo == GizmoMode::Rotate, "Rotate")
                    .clicked()
                {
                    self.gizmo = GizmoMode::Rotate;
                }
                if ui
                    .selectable_label(self.gizmo == GizmoMode::Scale, "Scale")
                    .clicked()
                {
                    self.gizmo = GizmoMode::Scale;
                }
                if ui
                    .selectable_label(self.gizmo == GizmoMode::Off, "No Gizmo")
                    .clicked()
                {
                    self.gizmo = GizmoMode::Off;
                }
                ui.separator();
                if ui.button("Frame Selected (F)").clicked() {
                    self.frame_selected();
                }
                ui.separator();
                let mesh_ready = self.mesh_edit && self.selected.is_some();
                if ui
                    .add_enabled(mesh_ready, egui::Button::new("Extrude"))
                    .clicked()
                {
                    self.perform_mesh_action(MeshAction::Extrude);
                }
                if ui
                    .add_enabled(mesh_ready, egui::Button::new("Inset"))
                    .clicked()
                {
                    self.perform_mesh_action(MeshAction::Inset);
                }
                if ui
                    .add_enabled(mesh_ready, egui::Button::new("Bevel"))
                    .clicked()
                {
                    self.perform_mesh_action(MeshAction::Bevel);
                }
                if ui
                    .add_enabled(mesh_ready, egui::Button::new("Ring Cut"))
                    .clicked()
                {
                    self.perform_mesh_action(MeshAction::RingCut);
                }
                if ui
                    .add_enabled(mesh_ready, egui::Button::new("Weld Points"))
                    .clicked()
                {
                    self.perform_mesh_action(MeshAction::Weld);
                }
                if ui
                    .add_enabled(mesh_ready, egui::Button::new("Bridge Loops"))
                    .clicked()
                {
                    self.perform_mesh_action(MeshAction::Bridge);
                }
                ui.separator();
                ui.menu_button("UV Projection", |ui| {
                    ui.set_min_width(190.0);
                    if ui.button("Planar").clicked() {
                        self.apply_uv_op(UvOp::Planar);
                    }
                    if ui.button("Box").clicked() {
                        self.apply_uv_op(UvOp::Box);
                    }
                    if ui.button("Cylindrical").clicked() {
                        self.apply_uv_op(UvOp::Cylindrical);
                    }
                    if ui.button("Spherical").clicked() {
                        self.apply_uv_op(UvOp::Spherical);
                    }
                    if ui.button("Automatic Unwrap").clicked() {
                        self.apply_uv_op(UvOp::Unwrap);
                    }
                    if ui.button("Relax").clicked() {
                        self.apply_uv_op(UvOp::Relax);
                    }
                    if ui.button("Normalize").clicked() {
                        self.apply_uv_op(UvOp::Normalize);
                    }
                });
            });
            ui.menu_button("Render", |ui| {
                ui.set_min_width(190.0);
                if ui.button("Render Frame").clicked() {
                    self.start_render(ui.ctx());
                }
                if ui.button("Render Manager…").clicked() {
                    self.show_render_manager = true;
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.selected_finished_render().is_some(),
                        egui::Button::new("Save Last Render as PNG"),
                    )
                    .clicked()
                {
                    self.save_render(false);
                }
                if ui
                    .add_enabled(
                        self.selected_finished_render().is_some(),
                        egui::Button::new("Save Last Render as EXR"),
                    )
                    .clicked()
                {
                    self.save_render(true);
                }
                ui.separator();
                if ui.button("Render Settings…").clicked() {
                    self.inspector_tab = InspectorTab::Render;
                }
            });
            ui.menu_button("Animation", |ui| {
                ui.set_min_width(190.0);
                if ui
                    .button(if self.playback.playing {
                        "Pause"
                    } else {
                        "Play"
                    })
                    .clicked()
                {
                    self.playback.playing = !self.playback.playing;
                }
                if ui.button("Go to Start").clicked() {
                    self.playback.time = self.playback.start;
                    self.playback.playing = false;
                }
                if ui.button("Go to End").clicked() {
                    self.playback.time = self.playback.end;
                    self.playback.playing = false;
                }
                ui.separator();
                if ui.button("Key Transforms").clicked() {
                    self.key_transform_channels();
                }
                if ui
                    .selectable_label(self.show_curves, "F-Curve Editor")
                    .clicked()
                {
                    self.show_curves = !self.show_curves;
                }
                if ui
                    .checkbox(&mut self.playback.looping, "Loop Playback")
                    .clicked()
                {}
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Frames/sec");
                    ui.add(
                        egui::DragValue::new(&mut self.playback.fps)
                            .range(1.0..=240.0)
                            .speed(0.2),
                    );
                });
            });
            ui.menu_button("Window", |ui| {
                ui.set_min_width(190.0);
                if ui.button("3D View").clicked() {
                    self.editor_view = EditorView::Scene3d;
                }
                if ui.button("UV Editor").clicked() {
                    self.editor_view = EditorView::Uv;
                }
                if ui.button("Node Editor").clicked() {
                    self.editor_view = EditorView::Nodes;
                }
                ui.separator();
                if ui.button("Properties").clicked() {
                    self.inspector_tab = InspectorTab::Object;
                }
                if ui.button("Render Manager").clicked() {
                    self.show_render_manager = true;
                }
                ui.separator();
                if ui.checkbox(&mut self.show_grid, "Show Grid").clicked() {}
                if ui
                    .checkbox(&mut self.show_helpers, "Light / Camera Helpers")
                    .clicked()
                {}
            });
            ui.menu_button("Help", |ui| {
                ui.set_min_width(190.0);
                if ui.button("About LeopardCraft").clicked() {
                    self.show_about = true;
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let name = self
                    .lifecycle
                    .path()
                    .and_then(|path| path.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or("Untitled");
                ui.label(egui::RichText::new(name).size(10.0).color(theme::TEXT_WEAK));
            });
        });
    }

    fn cheetah_status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Status:")
                    .size(10.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.label(
                egui::RichText::new(&self.status)
                    .size(10.0)
                    .color(theme::TEXT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let evaluated = self.evaluate();
                ui.label(
                    egui::RichText::new(format!(
                        "Frame {}  ·  {} objects  ·  {} tris",
                        self.playback.frame(),
                        self.doc.scene.len(),
                        evaluated.triangle_count()
                    ))
                    .size(9.5)
                    .color(theme::TEXT_WEAK),
                );
            });
        });
    }

    pub(super) fn log_status(&mut self) {
        if self.status == self.last_status {
            return;
        }
        let stamp = self.started_at.elapsed().as_secs_f32();
        self.console
            .push_back(format!("[{stamp:6.1}s] {}", self.status));
        while self.console.len() > 250 {
            self.console.pop_front();
        }
        self.last_status = self.status.clone();
    }

    pub(super) fn update_window_title(&mut self, ctx: &egui::Context) {
        let name = self.document_name();
        let marker = if self.lifecycle.is_dirty() { "*" } else { "" };
        let title = format!("{name}{marker} — LeopardCraft");
        if self.last_title != title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }

    pub(super) fn leave_mesh_edit(&mut self) {
        self.mesh_edit = false;
        self.mesh_selected.clear();
        self.status = "Object mode".into();
    }

    pub(super) fn set_mesh_mode(&mut self, mode: MeshSelectMode) {
        self.mesh_select_mode = mode;
        self.mesh_selected.clear();
        if self.mesh_edit {
            return;
        }
        if self.selected_editable_mesh().is_some() {
            self.mesh_edit = true;
            self.status = format!("{} mode", mode.label());
        } else {
            self.perform_mesh_action(MeshAction::Enter);
        }
    }

    fn apply_view_preset(&mut self, preset: &str) {
        self.view_label = preset.to_string();
        match preset {
            "Front" => {
                self.orbit.yaw = 0.0;
                self.orbit.pitch = 0.0;
            }
            "Top" => {
                self.orbit.yaw = 0.0;
                self.orbit.pitch = 1.48;
            }
            "Right" => {
                self.orbit.yaw = std::f32::consts::FRAC_PI_2;
                self.orbit.pitch = 0.0;
            }
            "Left" => {
                self.orbit.yaw = -std::f32::consts::FRAC_PI_2;
                self.orbit.pitch = 0.0;
            }
            _ => {
                self.orbit = OrbitCamera::default();
                self.view_label = "Viewport".into();
            }
        }
        self.status = format!("View: {preset}");
    }

    fn add_subdivision(&mut self) {
        let Some(id) = self.selected else {
            self.status = "Select a mesh object first".into();
            return;
        };
        if !self
            .doc
            .scene
            .get(id)
            .map(|node| node.is_mesh())
            .unwrap_or(false)
        {
            self.status = "Select a mesh object first".into();
            return;
        }
        let Some(before) = self.doc.scene.get(id).map(Shape::of) else {
            return;
        };
        let mut after = before.clone();
        after.modifiers.push(Modifier::Subdivision { levels: 1 });
        self.history
            .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
        self.status = "Added subdivision modifier".into();
    }

    fn add_boolean(&mut self) {
        let Some(id) = self.selected else {
            self.status = "Select a mesh object first".into();
            return;
        };
        let operand = self.doc.scene.depth_first().into_iter().find(|other| {
            *other != id
                && self
                    .doc
                    .scene
                    .get(*other)
                    .map(|node| node.is_mesh())
                    .unwrap_or(false)
        });
        let Some(operand) = operand else {
            self.status = "Add a second mesh object to use as the boolean operand".into();
            return;
        };
        let Some(before) = self.doc.scene.get(id).map(Shape::of) else {
            return;
        };
        let mut after = before.clone();
        after.modifiers.push(Modifier::Boolean {
            operand,
            operation: lc_scene::CsgOperation::Subtract,
        });
        self.history
            .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
        self.status = "Added boolean subtract modifier".into();
    }
}
