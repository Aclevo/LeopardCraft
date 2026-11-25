//! The Cheetah-style application shell: light chrome, grouped icon toolbar,
//! view switcher, right-hand inspector/browser column, material shelf,
//! full-width timeline and status bar.

use super::*;
use crate::icons::{self, Icon};
use crate::theme;

impl LeopardApp {
    pub(super) fn shell(&mut self, root: &mut egui::Ui) {
        #[cfg(test)]
        ui_test_support::clear_menu_hits();
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
                    let frame_button = icons::icon_button(ui, Icon::Fit, "Frame selection", true);
                    record_menu_item_state("toolbar:Frame Selected", &frame_button, true);
                    if frame_button.clicked() {
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
        let mesh_ready = self.mesh_mode_ready();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            if icons::icon_button(ui, Icon::Select, "Select / object mode", true).clicked() {
                self.leave_mesh_edit();
            }
            let move_button = icons::icon_button_toggle(
                ui,
                Icon::Move,
                "Move gizmo (W)",
                self.gizmo == GizmoMode::Translate,
                true,
            );
            record_menu_item_state("toolbar:Move", &move_button, true);
            if move_button.clicked() {
                self.gizmo = GizmoMode::Translate;
            }
            let rotate_button = icons::icon_button_toggle(
                ui,
                Icon::Rotate,
                "Rotate gizmo (E)",
                self.gizmo == GizmoMode::Rotate,
                true,
            );
            record_menu_item_state("toolbar:Rotate", &rotate_button, true);
            if rotate_button.clicked() {
                self.gizmo = GizmoMode::Rotate;
            }
            let scale_button = icons::icon_button_toggle(
                ui,
                Icon::Scale,
                "Scale gizmo (R)",
                self.gizmo == GizmoMode::Scale,
                true,
            );
            record_menu_item_state("toolbar:Scale", &scale_button, true);
            if scale_button.clicked() {
                self.gizmo = GizmoMode::Scale;
            }
            theme::toolbar_sep(ui);
            let object_mode_button = icons::icon_button_toggle(
                ui,
                Icon::ObjectMode,
                "Object mode",
                !self.mesh_edit,
                true,
            );
            record_menu_item_state("toolbar:Object Mode", &object_mode_button, true);
            if object_mode_button.clicked() {
                self.leave_mesh_edit();
            }
            for (icon, mode, tip, key) in [
                (
                    Icon::PointMode,
                    MeshSelectMode::Points,
                    "Point mode",
                    "toolbar:Point Mode",
                ),
                (
                    Icon::EdgeMode,
                    MeshSelectMode::Edges,
                    "Edge mode",
                    "toolbar:Edge Mode",
                ),
                (
                    Icon::PolygonMode,
                    MeshSelectMode::Polygons,
                    "Polygon mode",
                    "toolbar:Polygon Mode",
                ),
            ] {
                let button = icons::icon_button_toggle(
                    ui,
                    icon,
                    tip,
                    self.mesh_edit && self.mesh_select_mode == mode,
                    mesh_ready,
                );
                record_menu_item_state(key, &button, mesh_ready);
                if button.clicked() {
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
            if icons::icon_button(ui, Icon::Cylinder, "Add capsule", true).clicked() {
                self.add_primitive(Primitive::Capsule {
                    radius: 0.5,
                    height: 2.0,
                    segments: 32,
                    rings: 8,
                });
            }
            if icons::icon_button(ui, Icon::Plane, "Add disc", true).clicked() {
                self.add_primitive(Primitive::Disc {
                    radius: 1.0,
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
            let subdivide_button = icons::icon_button(
                ui,
                Icon::Subdivide,
                "Add subdivision modifier",
                self.selection_has_mesh(),
            );
            record_menu_item_state(
                "toolbar:Add Subdivision",
                &subdivide_button,
                self.selection_has_mesh(),
            );
            if subdivide_button.clicked() {
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
            recorded_submenu(ui, "menu:Edit", "Edit", |ui| {
                ui.set_min_width(190.0);
                let undo_label = self
                    .history
                    .undo_label()
                    .map(|label| format!("Undo {label}"))
                    .unwrap_or_else(|| "Undo".into());
                if recorded_button_enabled(ui, "Edit:Undo", self.history.can_undo(), undo_label)
                    .clicked()
                {
                    self.undo();
                }
                let redo_label = self
                    .history
                    .redo_label()
                    .map(|label| format!("Redo {label}"))
                    .unwrap_or_else(|| "Redo".into());
                if recorded_button_enabled(ui, "Edit:Redo", self.history.can_redo(), redo_label)
                    .clicked()
                {
                    self.redo();
                }
                ui.separator();
                if recorded_button_enabled(
                    ui,
                    "Edit:Duplicate",
                    !self.selection.is_empty(),
                    "Duplicate",
                )
                .clicked()
                {
                    self.duplicate_selected();
                }
                if recorded_button_enabled(ui, "Edit:Delete", !self.selection.is_empty(), "Delete")
                    .clicked()
                {
                    self.delete_selected();
                }
            });
            recorded_submenu(ui, "menu:Selection", "Selection", |ui| {
                ui.set_min_width(190.0);
                if recorded_button(ui, "Selection:Object Mode", "Object Mode").clicked() {
                    self.leave_mesh_edit();
                }
                if recorded_button_enabled(
                    ui,
                    "Selection:Point Mode",
                    self.mesh_mode_ready(),
                    "Point Mode",
                )
                .clicked()
                {
                    self.set_mesh_mode(MeshSelectMode::Points);
                }
                if recorded_button_enabled(
                    ui,
                    "Selection:Edge Mode",
                    self.mesh_mode_ready(),
                    "Edge Mode",
                )
                .clicked()
                {
                    self.set_mesh_mode(MeshSelectMode::Edges);
                }
                if recorded_button_enabled(
                    ui,
                    "Selection:Polygon Mode",
                    self.mesh_mode_ready(),
                    "Polygon Mode",
                )
                .clicked()
                {
                    self.set_mesh_mode(MeshSelectMode::Polygons);
                }
                ui.separator();
                if recorded_button(ui, "Selection:Select All Objects", "Select All Objects")
                    .clicked()
                {
                    self.select_all_objects();
                }
                if recorded_button(ui, "Selection:Deselect All Objects", "Deselect All Objects")
                    .clicked()
                {
                    self.deselect_all_objects();
                }
                ui.separator();
                if recorded_button_enabled(
                    ui,
                    "Selection:Select All Components",
                    self.mesh_component_selection_ready(),
                    "Select All Components",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::SelectAll);
                }
                if recorded_button_enabled(
                    ui,
                    "Selection:Deselect All Components",
                    self.mesh_component_selection_ready(),
                    "Deselect All Components",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::SelectNone);
                }
                if recorded_button_enabled(
                    ui,
                    "Selection:Toggle Seam",
                    self.mesh_topology_ready(MeshSelectMode::Edges, 1),
                    "Toggle Seam",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::ToggleSeam);
                }
                if recorded_button_enabled(
                    ui,
                    "Selection:Toggle Pinned UV",
                    self.uv_pin_ready(),
                    "Toggle Pinned UV",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::TogglePinnedUv);
                }
                ui.separator();
                if recorded_button_enabled(
                    ui,
                    "Selection:Convert to Editable Mesh",
                    self.selection_convert_ready(),
                    "Convert to Editable Mesh",
                )
                .clicked()
                {
                    self.make_selected_editable();
                }
            });
            recorded_submenu(ui, "menu:Objects", "Objects", |ui| {
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
                if ui.button("Capsule").clicked() {
                    self.add_primitive(Primitive::Capsule {
                        radius: 0.5,
                        height: 2.0,
                        segments: 32,
                        rings: 8,
                    });
                }
                if ui.button("Disc").clicked() {
                    self.add_primitive(Primitive::Disc {
                        radius: 1.0,
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
                if recorded_button_enabled(
                    ui,
                    "Objects:Group Selection",
                    self.selection.len() > 1,
                    "Group Selection",
                )
                .clicked()
                {
                    self.group_selection();
                }
                recorded_submenu(ui, "Objects:Selected Flags", "Selected Flags", |ui| {
                    if recorded_button_enabled(
                        ui,
                        "Objects:Flags:Show Selected",
                        self.selection_flag_ready(NodeFlag::Visible, true),
                        "Show Selected",
                    )
                    .clicked()
                    {
                        self.set_selection_flag(NodeFlag::Visible, true);
                    }
                    if recorded_button_enabled(
                        ui,
                        "Objects:Flags:Hide Selected",
                        self.selection_flag_ready(NodeFlag::Visible, false),
                        "Hide Selected",
                    )
                    .clicked()
                    {
                        self.set_selection_flag(NodeFlag::Visible, false);
                    }
                    if recorded_button_enabled(
                        ui,
                        "Objects:Flags:Enable Render on Selected",
                        self.selection_flag_ready(NodeFlag::Renderable, true),
                        "Enable Render on Selected",
                    )
                    .clicked()
                    {
                        self.set_selection_flag(NodeFlag::Renderable, true);
                    }
                    if recorded_button_enabled(
                        ui,
                        "Objects:Flags:Disable Render on Selected",
                        self.selection_flag_ready(NodeFlag::Renderable, false),
                        "Disable Render on Selected",
                    )
                    .clicked()
                    {
                        self.set_selection_flag(NodeFlag::Renderable, false);
                    }
                    if recorded_button_enabled(
                        ui,
                        "Objects:Flags:Lock Selected",
                        self.selection_flag_ready(NodeFlag::Locked, true),
                        "Lock Selected",
                    )
                    .clicked()
                    {
                        self.set_selection_flag(NodeFlag::Locked, true);
                    }
                    if recorded_button_enabled(
                        ui,
                        "Objects:Flags:Unlock Selected",
                        self.selection_flag_ready(NodeFlag::Locked, false),
                        "Unlock Selected",
                    )
                    .clicked()
                    {
                        self.set_selection_flag(NodeFlag::Locked, false);
                    }
                });
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
                if recorded_button_enabled(
                    ui,
                    "Objects:Convert to Editable Mesh",
                    self.selection_convert_ready(),
                    "Convert to Editable Mesh",
                )
                .clicked()
                {
                    self.make_selected_editable();
                }
                if recorded_button_enabled(
                    ui,
                    "Objects:Add Subdivision",
                    self.selection_has_mesh(),
                    "Add Subdivision",
                )
                .clicked()
                {
                    self.add_subdivision();
                }
                if recorded_button_enabled(
                    ui,
                    "Objects:Add Boolean Subtract",
                    self.boolean_ready(),
                    "Add Boolean Subtract",
                )
                .clicked()
                {
                    self.add_boolean();
                }
            });
            recorded_submenu(ui, "menu:Tools", "Tools", |ui| {
                ui.set_min_width(190.0);
                if recorded_selectable(ui, "Tools:Move", self.gizmo == GizmoMode::Translate, "Move")
                    .clicked()
                {
                    self.gizmo = GizmoMode::Translate;
                }
                if recorded_selectable(
                    ui,
                    "Tools:Rotate",
                    self.gizmo == GizmoMode::Rotate,
                    "Rotate",
                )
                .clicked()
                {
                    self.gizmo = GizmoMode::Rotate;
                }
                if recorded_selectable(ui, "Tools:Scale", self.gizmo == GizmoMode::Scale, "Scale")
                    .clicked()
                {
                    self.gizmo = GizmoMode::Scale;
                }
                if recorded_selectable(
                    ui,
                    "Tools:No Gizmo",
                    self.gizmo == GizmoMode::Off,
                    "No Gizmo",
                )
                .clicked()
                {
                    self.gizmo = GizmoMode::Off;
                }
                ui.separator();
                if recorded_button(ui, "Tools:Frame Selected", "Frame Selected (F)").clicked() {
                    self.frame_selected();
                }
                ui.separator();
                if recorded_button_enabled(
                    ui,
                    "Tools:Extrude",
                    self.mesh_topology_ready(MeshSelectMode::Polygons, 1),
                    "Extrude",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::Extrude);
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:Inset",
                    self.mesh_topology_ready(MeshSelectMode::Polygons, 1),
                    "Inset",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::Inset);
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:Bevel",
                    self.mesh_topology_ready(MeshSelectMode::Polygons, 1),
                    "Bevel",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::Bevel);
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:Ring Cut",
                    self.mesh_ring_cut_ready(),
                    "Ring Cut",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::RingCut);
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:Weld Points",
                    self.mesh_weld_ready(),
                    "Weld Points",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::Weld);
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:Collapse",
                    self.mesh_collapse_ready(),
                    "Collapse",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::Collapse);
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:Create Polygon",
                    self.mesh_create_polygon_ready(),
                    "Create Polygon",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::CreatePolygon);
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:Optimize",
                    self.mesh_optimize_ready(),
                    "Optimize",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::Optimize);
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:Point Slide",
                    self.mesh_point_slide_ready(),
                    "Point Slide",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::PointSlide);
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:Bridge Loops",
                    self.mesh_bridge_ready(),
                    "Bridge Loops",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::Bridge);
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:Fill Hole",
                    self.mesh_fill_hole_ready(),
                    "Fill Hole",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::FillHole);
                }
                let fill_tool_label = if self.fill_hole_tool {
                    "Fill Hole Tool ✓"
                } else {
                    "Fill Hole Tool"
                };
                if recorded_button_enabled(
                    ui,
                    "Tools:Fill Hole Tool",
                    self.mesh_edit,
                    fill_tool_label,
                )
                .clicked()
                {
                    self.fill_hole_tool = !self.fill_hole_tool;
                    self.status = if self.fill_hole_tool {
                        "Fill Hole tool: click a point on an open hole border".into()
                    } else {
                        "Fill Hole tool off".into()
                    };
                }
                let bridge_label = if self.live_bridge.is_some() {
                    "Live Agent Bridge ✓"
                } else {
                    "Live Agent Bridge"
                };
                if recorded_button_enabled(ui, "Tools:Live Agent Bridge", true, bridge_label)
                    .clicked()
                {
                    self.toggle_live_bridge();
                }
                let scalpel_tool_label = if self.scalpel_tool {
                    "Scalpel Tool ✓"
                } else {
                    "Scalpel Tool"
                };
                if recorded_button_enabled(
                    ui,
                    "Tools:Scalpel Tool",
                    self.mesh_edit,
                    scalpel_tool_label,
                )
                .clicked()
                {
                    self.scalpel_tool = !self.scalpel_tool;
                    self.scalpel_start = None;
                    if self.scalpel_tool {
                        self.pick_tool = None;
                        self.pick_start = None;
                    }
                    self.status = if self.scalpel_tool {
                        "Scalpel tool: click a point or edge to start a cut".into()
                    } else {
                        "Scalpel tool off".into()
                    };
                }
                for tool in [
                    MeshPickTool::RingCut,
                    MeshPickTool::Weld,
                    MeshPickTool::Bridge,
                    MeshPickTool::InsertPoint,
                ] {
                    let active = self.pick_tool == Some(tool);
                    let label = if active {
                        format!("{} Tool ✓", tool.label())
                    } else {
                        format!("{} Tool", tool.label())
                    };
                    let key = format!("Tools:{} Tool", tool.label());
                    if recorded_button_enabled(ui, &key, self.mesh_edit, &label).clicked() {
                        self.toggle_pick_tool(tool);
                    }
                }
                if recorded_button_enabled(ui, "Tools:Split", self.mesh_split_ready(), "Split")
                    .clicked()
                {
                    self.perform_mesh_action(MeshAction::Split);
                }
                if recorded_button_enabled(ui, "Tools:Shell", self.mesh_shell_ready(), "Shell")
                    .clicked()
                {
                    self.perform_mesh_action(MeshAction::Shell);
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:Triangulate",
                    self.mesh_triangulate_ready(),
                    "Triangulate",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::Triangulate);
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:Scalpel",
                    self.mesh_scalpel_ready(),
                    "Scalpel",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::Scalpel);
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:Subdivide",
                    self.mesh_subdivide_ready(),
                    "Subdivide",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::Subdivide);
                }
                if recorded_button_enabled(ui, "Tools:Clone", self.mesh_clone_ready(), "Clone")
                    .clicked()
                {
                    self.perform_mesh_action(MeshAction::Clone);
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:Flip Normal",
                    self.mesh_flip_normal_ready(),
                    "Flip Normal",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::FlipNormal);
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:Disconnect",
                    self.mesh_disconnect_ready(),
                    "Disconnect",
                )
                .clicked()
                {
                    self.perform_mesh_action(MeshAction::Disconnect);
                }
                if recorded_button_enabled(ui, "Tools:Cover", self.mesh_cover_ready(), "Cover")
                    .clicked()
                {
                    self.perform_mesh_action(MeshAction::Cover);
                }
                ui.separator();
                recorded_submenu(ui, "Tools:UV Projection", "UV Projection", |ui| {
                    ui.set_min_width(190.0);
                    if recorded_button_enabled(
                        ui,
                        "Tools:UV Projection:Planar",
                        self.selection_uv_ready(),
                        "Planar",
                    )
                    .clicked()
                    {
                        self.apply_uv_op(UvOp::Planar);
                    }
                    if recorded_button_enabled(
                        ui,
                        "Tools:UV Projection:Box",
                        self.selection_uv_ready(),
                        "Box",
                    )
                    .clicked()
                    {
                        self.apply_uv_op(UvOp::Box);
                    }
                    if recorded_button_enabled(
                        ui,
                        "Tools:UV Projection:Cylindrical",
                        self.selection_uv_ready(),
                        "Cylindrical",
                    )
                    .clicked()
                    {
                        self.apply_uv_op(UvOp::Cylindrical);
                    }
                    if recorded_button_enabled(
                        ui,
                        "Tools:UV Projection:Spherical",
                        self.selection_uv_ready(),
                        "Spherical",
                    )
                    .clicked()
                    {
                        self.apply_uv_op(UvOp::Spherical);
                    }
                    if recorded_button_enabled(
                        ui,
                        "Tools:UV Projection:Automatic Unwrap",
                        self.selection_uv_ready(),
                        "Automatic Unwrap",
                    )
                    .clicked()
                    {
                        self.apply_uv_op(UvOp::Unwrap);
                    }
                    if recorded_button_enabled(
                        ui,
                        "Tools:UV Projection:Relax",
                        self.selection_uv_ready(),
                        "Relax",
                    )
                    .clicked()
                    {
                        self.apply_uv_op(UvOp::Relax);
                    }
                    if recorded_button_enabled(
                        ui,
                        "Tools:UV Projection:Normalize",
                        self.selection_uv_ready(),
                        "Normalize",
                    )
                    .clicked()
                    {
                        self.apply_uv_op(UvOp::Normalize);
                    }
                });
            });
            recorded_submenu(ui, "Tools:UV Editing", "UV Editing", |ui| {
                ui.set_min_width(210.0);
                let ready = self.uv_edit_ready();
                if recorded_button_enabled(ui, "Tools:UV Editing:Toggle Seam", ready, "Toggle Seam")
                    .clicked()
                {
                    self.toggle_selected_uv_seam();
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:UV Editing:Select All Seams",
                    ready,
                    "Select All Seams",
                )
                .clicked()
                {
                    self.select_all_uv_seams();
                }
                if recorded_button_enabled(ui, "Tools:UV Editing:Seam Loop", ready, "Seam Loop")
                    .clicked()
                {
                    self.select_uv_seam_loop();
                }
                if recorded_button_enabled(ui, "Tools:UV Editing:Clear Seams", ready, "Clear Seams")
                    .clicked()
                {
                    self.clear_uv_seams();
                }
                ui.separator();
                if recorded_button_enabled(
                    ui,
                    "Tools:UV Editing:Select Island",
                    ready,
                    "Select Island",
                )
                .clicked()
                {
                    self.select_uv_island();
                }
                for (key, label, op) in [
                    ("Weld UVs", "Weld UVs", UvSlotOp::Weld),
                    ("Split UVs", "Split UVs", UvSlotOp::Split),
                    ("Align U", "Align U", UvSlotOp::AlignU),
                    ("Align V", "Align V", UvSlotOp::AlignV),
                    ("Distribute U", "Distribute U", UvSlotOp::DistributeU),
                    ("Distribute V", "Distribute V", UvSlotOp::DistributeV),
                    ("Flip U", "Flip U", UvSlotOp::FlipU),
                    ("Flip V", "Flip V", UvSlotOp::FlipV),
                    ("Rotate 90", "Rotate 90°", UvSlotOp::Rotate90),
                ] {
                    if recorded_button_enabled(ui, &format!("Tools:UV Editing:{key}"), ready, label)
                        .clicked()
                    {
                        self.run_uv_slot_op(op);
                    }
                }
                ui.separator();
                if recorded_button_enabled(ui, "Tools:UV Editing:Pack Charts", ready, "Pack Charts")
                    .clicked()
                {
                    self.pack_selected_uv_charts();
                }
                if recorded_button_enabled(ui, "Tools:UV Editing:Add UV Set", ready, "Add UV Set")
                    .clicked()
                {
                    self.add_uv_set();
                }
                if recorded_button_enabled(
                    ui,
                    "Tools:UV Editing:Remove UV Set",
                    ready,
                    "Remove UV Set",
                )
                .clicked()
                {
                    self.remove_uv_set();
                }
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
            recorded_submenu(ui, "menu:Animation", "Animation", |ui| {
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
                if recorded_button_enabled(
                    ui,
                    "Animation:Key Transforms",
                    self.key_transforms_ready(),
                    "Key Transforms",
                )
                .clicked()
                {
                    self.key_transform_channels();
                }
                if ui
                    .selectable_label(self.show_curves, "F-Curve Editor")
                    .clicked()
                {
                    self.show_curves = !self.show_curves;
                }
                let has_curve_selection = !self.selected_curve_keys().is_empty();
                ui.add_enabled_ui(has_curve_selection, |ui| {
                    ui.menu_button("Tangent", |ui| {
                        ui.menu_button("Type", |ui| {
                            for tangent_type in TangentType::ALL {
                                if ui.button(tangent_type.label()).clicked() {
                                    self.set_selected_curve_tangent_type(tangent_type, true, true);
                                }
                            }
                        });
                    });
                    ui.menu_button("Left Tangent", |ui| {
                        for tangent_type in TangentType::ALL {
                            if ui.button(tangent_type.label()).clicked() {
                                self.set_selected_curve_tangent_type(tangent_type, true, false);
                            }
                        }
                    });
                    ui.menu_button("Right Tangent", |ui| {
                        for tangent_type in TangentType::ALL {
                            if ui.button(tangent_type.label()).clicked() {
                                self.set_selected_curve_tangent_type(tangent_type, false, true);
                            }
                        }
                    });
                });
                let mut looping = self.playback.looping;
                if ui.checkbox(&mut looping, "Loop Playback").changed() {
                    self.set_playback_looping(looping);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Frames/sec");
                    let mut fps = self.playback.fps;
                    if ui
                        .add(egui::DragValue::new(&mut fps).range(1.0..=240.0).speed(0.2))
                        .changed()
                    {
                        self.set_playback_fps(fps);
                    }
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
                ui.checkbox(&mut self.show_grid, "Show Grid");
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
        self.weight_paint = false;
        self.clear_mesh_component_selection();
        self.status = "Object mode".into();
    }

    pub(super) fn set_mesh_mode(&mut self, mode: MeshSelectMode) {
        if !self.mesh_mode_ready() {
            self.status = if self.selection.is_empty() {
                "Select a mesh object to edit components".into()
            } else {
                "Component editing needs a mesh-only selection".into()
            };
            return;
        }
        self.mesh_select_mode = mode;
        self.weight_paint = false;
        self.clear_mesh_component_selection();
        if self.mesh_edit {
            return;
        }
        if self.selected_meshes_editable() {
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

    pub(super) fn add_subdivision(&mut self) {
        if !self.selection_has_mesh() {
            self.status = "Select a mesh object first".into();
            return;
        }
        let mut edits = Vec::new();
        for id in self.selection.ids() {
            let Some(node) = self.doc.scene.get(*id) else {
                continue;
            };
            if !node.is_mesh() {
                continue;
            }
            let before = Shape::of(node);
            let mut after = before.clone();
            after.modifiers.push(Modifier::Subdivision {
                levels: 2,
                scheme: lc_scene::SubdivisionScheme::CatmullClark,
            });
            edits.push((*id, before, after));
        }
        let count = edits.len();
        self.history
            .execute(&mut self.doc, Box::new(SetShapesCmd { edits }));
        self.status = if count == 1 {
            "Added subdivision modifier".into()
        } else {
            format!("Added subdivision modifier to {count} objects")
        };
    }

    pub(super) fn add_boolean(&mut self) {
        if !self.boolean_ready() {
            self.status = if self.selection_has_mesh() {
                "Add an unselected mesh object to use as the boolean operand".into()
            } else {
                "Select a mesh object first".into()
            };
            return;
        }
        let Some(operand) = self.boolean_operand() else {
            return;
        };
        let mut edits = Vec::new();
        for id in self.selection.ids() {
            let Some(node) = self.doc.scene.get(*id) else {
                continue;
            };
            if !node.is_mesh() {
                continue;
            }
            let before = Shape::of(node);
            let mut after = before.clone();
            after.modifiers.push(Modifier::Boolean {
                operand,
                operation: lc_scene::CsgOperation::Subtract,
            });
            edits.push((*id, before, after));
        }
        let count = edits.len();
        self.history
            .execute(&mut self.doc, Box::new(SetShapesCmd { edits }));
        self.status = if count == 1 {
            "Added boolean subtract modifier".into()
        } else {
            format!("Added boolean subtract modifier to {count} objects")
        };
    }
}
