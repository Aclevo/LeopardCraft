//! Material graph, UV, timeline and render panels.

use super::*;
use crate::icons::{self, Icon};
use crate::theme;

/// One draggable Bezier tangent handle in the curve canvas. Only selected
/// Spline scalar and morph-weight keys expose handles, matching the manual's
/// statement that handles appear with spline interpolation; rotation keys are
/// quaternion samples and have no scalar slope.
#[derive(Clone, Copy)]
struct CurveTangentHandle {
    target: AnimTarget,
    track: CurveTrackId,
    time: f32,
    incoming: bool,
    position: Pos2,
    key_value: f32,
    time_offset: f32,
    slope: f32,
}

fn set_principled(material: &mut Material, index: usize, value: ShaderValue) {
    if let Some(node) = material
        .graph
        .nodes
        .iter_mut()
        .find(|n| matches!(n.kind, ShaderNodeKind::Principled))
    {
        if let Some(input) = node.inputs.get_mut(index) {
            *input = value;
        }
    }
}

fn principled_float(material: &Material, index: usize) -> f32 {
    material
        .graph
        .nodes
        .iter()
        .find(|n| matches!(n.kind, ShaderNodeKind::Principled))
        .and_then(|n| n.inputs.get(index))
        .map(|v| v.as_float())
        .unwrap_or(0.0)
}

fn principled_color(material: &Material, index: usize) -> Vec3 {
    material
        .graph
        .nodes
        .iter()
        .find(|n| matches!(n.kind, ShaderNodeKind::Principled))
        .and_then(|n| n.inputs.get(index))
        .map(|v| v.as_color())
        .unwrap_or(Vec3::splat(0.8))
}

impl LeopardApp {
    pub(super) fn material_editor(&mut self, ui: &mut egui::Ui, id: MaterialId) {
        let Some(original) = self.doc.materials.get(id).cloned() else {
            return;
        };
        let mut edited = original.clone();
        let mut open_nodes = false;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                theme::section(ui, "material_name", "Material", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.add(
                            egui::TextEdit::singleline(&mut edited.name)
                                .desired_width(f32::INFINITY),
                        );
                    });
                    ui.checkbox(&mut edited.double_sided, "Double sided");
                    ui.checkbox(&mut edited.casts_shadow, "Casts shadows");
                });
                theme::section(ui, "material_surface", "Surface", |ui| {
                    theme::row(ui, "Base Color", |ui| {
                        let mut color = principled_color(&edited, 0).to_array();
                        if ui.color_edit_button_rgb(&mut color).changed() {
                            set_principled(
                                &mut edited,
                                0,
                                ShaderValue::Color(Vec3::from_array(color).max(Vec3::ZERO)),
                            );
                        }
                    });
                    let mut metallic = principled_float(&edited, 1);
                    theme::row(ui, "Metallic", |ui| {
                        if ui
                            .add(egui::Slider::new(&mut metallic, 0.0..=1.0).show_value(true))
                            .changed()
                        {
                            set_principled(&mut edited, 1, ShaderValue::Float(metallic));
                        }
                    });
                    let mut roughness = principled_float(&edited, 2);
                    theme::row(ui, "Roughness", |ui| {
                        if ui
                            .add(egui::Slider::new(&mut roughness, 0.02..=1.0).show_value(true))
                            .changed()
                        {
                            set_principled(&mut edited, 2, ShaderValue::Float(roughness));
                        }
                    });
                    let mut ior = principled_float(&edited, 3);
                    theme::row(ui, "IOR", |ui| {
                        if ui
                            .add(egui::Slider::new(&mut ior, 1.0..=2.5).show_value(true))
                            .changed()
                        {
                            set_principled(&mut edited, 3, ShaderValue::Float(ior));
                        }
                    });
                    let mut transmission = principled_float(&edited, 4);
                    theme::row(ui, "Transmission", |ui| {
                        if ui
                            .add(egui::Slider::new(&mut transmission, 0.0..=1.0).show_value(true))
                            .changed()
                        {
                            set_principled(&mut edited, 4, ShaderValue::Float(transmission));
                        }
                    });
                    theme::row(ui, "Emission", |ui| {
                        let mut emission = principled_color(&edited, 5).to_array();
                        if ui.color_edit_button_rgb(&mut emission).changed() {
                            set_principled(
                                &mut edited,
                                5,
                                ShaderValue::Color(Vec3::from_array(emission).max(Vec3::ZERO)),
                            );
                        }
                    });
                    let mut strength = principled_float(&edited, 6);
                    theme::row(ui, "Emission Str", |ui| {
                        if ui
                            .add(egui::Slider::new(&mut strength, 0.0..=20.0).show_value(true))
                            .changed()
                        {
                            set_principled(&mut edited, 6, ShaderValue::Float(strength));
                        }
                    });
                    let mut alpha = principled_float(&edited, 7);
                    theme::row(ui, "Alpha", |ui| {
                        if ui
                            .add(egui::Slider::new(&mut alpha, 0.0..=1.0).show_value(true))
                            .changed()
                        {
                            set_principled(&mut edited, 7, ShaderValue::Float(alpha));
                        }
                    });
                });
                theme::section(ui, "material_nodes", "Node Graph", |ui| {
                    let nodes: Vec<String> = edited
                        .graph
                        .nodes
                        .iter()
                        .map(|node| node.kind.label().to_string())
                        .collect();
                    ui.label(
                        egui::RichText::new(format!(
                            "{} nodes · {} links",
                            nodes.len(),
                            edited.graph.links.len()
                        ))
                        .size(10.0)
                        .color(theme::TEXT_WEAK),
                    );
                    if ui.button("Edit Node Graph").clicked() {
                        open_nodes = true;
                    }
                });
            });
        if open_nodes {
            self.selected_material = Some(id);
            self.editor_view = EditorView::Nodes;
        }

        if edited != original {
            if self.pending_material.is_none() {
                self.pending_material = Some((id, original));
            }
            self.doc.materials.insert(edited);
            self.status = "Editing material".into();
        }
    }

    pub(super) fn ui_uv(&mut self, ui: &mut egui::Ui) {
        if self.selection.primary().is_none() {
            ui.label("Select an object to edit UVs.");
            return;
        }
        let set_count = self
            .selected_editable_mesh()
            .map(|(_, mesh)| mesh.uv_set_count())
            .unwrap_or(0);
        let active_set = self
            .selected_editable_mesh()
            .map(|(_, mesh)| self.uv_active_set(&mesh))
            .unwrap_or(0);
        ui.horizontal_wrapped(|ui| {
            if set_count > 0 {
                egui::ComboBox::from_id_salt("uv-set-selector")
                    .selected_text(format!("UV{active_set}"))
                    .show_ui(ui, |ui| {
                        for set in 0..set_count {
                            if ui
                                .selectable_label(set == active_set, format!("UV{set}"))
                                .clicked()
                            {
                                self.uv_editor.set = set;
                            }
                        }
                    });
            }
            let can_add = set_count > 0;
            if ui
                .add_enabled(can_add, egui::Button::new("Add UV Set"))
                .clicked()
            {
                self.add_uv_set();
            }
            if ui
                .add_enabled(active_set > 0, egui::Button::new("Remove UV Set"))
                .clicked()
            {
                self.remove_uv_set();
            }
            ui.checkbox(&mut self.uv_editor.checker, "Checker");
            ui.checkbox(&mut self.uv_editor.distortion, "Distortion");
        });

        ui.horizontal_wrapped(|ui| {
            let uv_ready = self.selection_uv_ready();
            for (label, op) in [
                ("Planar", UvOp::Planar),
                ("Box", UvOp::Box),
                ("Cylindrical", UvOp::Cylindrical),
                ("Spherical", UvOp::Spherical),
                ("Auto Unwrap", UvOp::Unwrap),
                ("Relax", UvOp::Relax),
                ("Normalize", UvOp::Normalize),
            ] {
                if ui.add_enabled(uv_ready, egui::Button::new(label)).clicked() {
                    self.apply_uv_op(op);
                }
            }
            let pin_ready = self.uv_pin_ready();
            if ui
                .add_enabled(pin_ready, egui::Button::new("Toggle Pinned UV"))
                .clicked()
            {
                self.perform_mesh_action(MeshAction::TogglePinnedUv);
            }
        });

        ui.horizontal_wrapped(|ui| {
            let seam_ready = self.uv_pin_ready();
            if ui
                .add_enabled(seam_ready, egui::Button::new("Toggle Seam"))
                .clicked()
            {
                self.toggle_selected_uv_seam();
            }
            if ui
                .add_enabled(self.uv_edit_ready(), egui::Button::new("All Seams"))
                .clicked()
            {
                self.select_all_uv_seams();
            }
            if ui
                .add_enabled(self.uv_edit_ready(), egui::Button::new("Seam Loop"))
                .clicked()
            {
                self.select_uv_seam_loop();
            }
            if ui
                .add_enabled(self.uv_edit_ready(), egui::Button::new("Clear Seams"))
                .clicked()
            {
                self.clear_uv_seams();
            }
        });

        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(self.uv_edit_ready(), egui::Button::new("Select All"))
                .clicked()
            {
                self.select_all_uv_points();
            }
            if ui
                .add_enabled(self.uv_edit_ready(), egui::Button::new("Select None"))
                .clicked()
            {
                self.clear_uv_selection();
            }
            if ui
                .add_enabled(self.uv_pin_ready(), egui::Button::new("Island"))
                .clicked()
            {
                self.select_uv_island();
            }
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Chart");
            ui.add(
                egui::DragValue::new(&mut self.uv_editor.chart_move.x)
                    .speed(0.005)
                    .prefix("U "),
            );
            ui.add(
                egui::DragValue::new(&mut self.uv_editor.chart_move.y)
                    .speed(0.005)
                    .prefix("V "),
            );
            ui.add(
                egui::DragValue::new(&mut self.uv_editor.chart_rotate)
                    .speed(1.0)
                    .suffix("°"),
            );
            ui.add(
                egui::DragValue::new(&mut self.uv_editor.chart_scale)
                    .speed(0.01)
                    .range(0.05..=20.0),
            );
            if ui
                .add_enabled(self.uv_pin_ready(), egui::Button::new("Transform"))
                .clicked()
            {
                self.transform_selected_uv_charts();
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Pack");
            ui.add(
                egui::DragValue::new(&mut self.uv_editor.pack_padding)
                    .speed(0.002)
                    .range(0.0..=0.2)
                    .prefix("pad "),
            );
            ui.checkbox(&mut self.uv_editor.pack_rotate, "Rotate");
            egui::ComboBox::from_id_salt("uv-pack-align")
                .selected_text(match self.uv_editor.pack_align {
                    lc_geom::UvPackAlign::BottomLeft => "Bottom left",
                    lc_geom::UvPackAlign::TopLeft => "Top left",
                    lc_geom::UvPackAlign::Center => "Center",
                })
                .show_ui(ui, |ui| {
                    for (label, align) in [
                        ("Bottom left", lc_geom::UvPackAlign::BottomLeft),
                        ("Top left", lc_geom::UvPackAlign::TopLeft),
                        ("Center", lc_geom::UvPackAlign::Center),
                    ] {
                        ui.selectable_value(&mut self.uv_editor.pack_align, align, label);
                    }
                });
            if ui
                .add_enabled(self.uv_edit_ready(), egui::Button::new("Pack Charts"))
                .clicked()
            {
                self.pack_selected_uv_charts();
            }
        });

        ui.horizontal_wrapped(|ui| {
            for (label, op) in [
                ("Weld", UvSlotOp::Weld),
                ("Split", UvSlotOp::Split),
                ("Align U", UvSlotOp::AlignU),
                ("Align V", UvSlotOp::AlignV),
                ("Distribute U", UvSlotOp::DistributeU),
                ("Distribute V", UvSlotOp::DistributeV),
                ("Flip U", UvSlotOp::FlipU),
                ("Flip V", UvSlotOp::FlipV),
                ("Rotate 90°", UvSlotOp::Rotate90),
            ] {
                if ui
                    .add_enabled(self.uv_pin_ready(), egui::Button::new(label))
                    .clicked()
                {
                    self.run_uv_slot_op(op);
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Set UV");
            ui.add(egui::DragValue::new(&mut self.uv_editor.numeric.x).speed(0.005));
            ui.add(egui::DragValue::new(&mut self.uv_editor.numeric.y).speed(0.005));
            if ui
                .add_enabled(self.uv_pin_ready(), egui::Button::new("Apply UV"))
                .clicked()
            {
                self.run_uv_slot_op(UvSlotOp::Numeric);
            }
        });

        let Some((_, original_mesh)) = self.selected_editable_mesh() else {
            ui.label(
                egui::RichText::new(
                    "UV editing requires an editable mesh. Convert a primitive first.",
                )
                .small()
                .weak(),
            );
            if ui
                .add_enabled(
                    self.selection_convert_ready(),
                    egui::Button::new("Convert to Editable Mesh"),
                )
                .clicked()
            {
                self.make_selected_editable();
            }
            return;
        };
        if original_mesh.uv_set_count() == 0 {
            ui.label(
                egui::RichText::new(
                    "No UV points yet. Apply a projection above to create a UV set.",
                )
                .small()
                .weak(),
            );
            return;
        }
        self.prune_uv_selection(&original_mesh);
        let mesh = original_mesh.clone();
        let active_set = self.uv_active_set(&mesh);
        let values: Vec<V2> = mesh.uv_set(active_set).unwrap_or(&[]).to_vec();
        let side = ui
            .available_height()
            .max(160.0)
            .min(ui.available_width())
            .min(760.0);
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(248, 248, 248));
        let cells = if self.uv_editor.checker { 16 } else { 8 };
        for y in 0..cells {
            for x in 0..cells {
                let color = if (x + y) % 2 == 0 {
                    Color32::from_rgb(247, 247, 247)
                } else if self.uv_editor.checker {
                    Color32::from_rgb(206, 214, 226)
                } else {
                    Color32::from_rgb(231, 231, 231)
                };
                let cell = rect.width() / cells as f32;
                painter.rect_filled(
                    Rect::from_min_size(
                        rect.min + Vec2::new(x as f32 * cell, y as f32 * cell),
                        Vec2::splat(cell + 0.5),
                    ),
                    0.0,
                    color,
                );
            }
        }
        let to_screen = |uv: V2| {
            Pos2::new(
                rect.min.x + uv.x * rect.width(),
                rect.max.y - uv.y * rect.height(),
            )
        };
        let distortion = if self.uv_editor.distortion {
            lc_geom::uv_face_distortion(&mesh, active_set)
        } else {
            Vec::new()
        };
        for (pi, poly) in mesh.polygons.iter().enumerate() {
            let points: Vec<Pos2> = poly
                .corners
                .iter()
                .map(|c| to_screen(mesh.uv_of_set(c, active_set)))
                .collect();
            if let Some(stats) = distortion.get(pi) {
                let t = ((stats.stretch - 1.0) / 3.0).clamp(0.0, 1.0);
                let color =
                    Color32::from_rgb((70.0 + 170.0 * t) as u8, (170.0 - 110.0 * t) as u8, 70);
                painter.add(egui::Shape::convex_polygon(
                    points.clone(),
                    color.gamma_multiply(0.55),
                    egui::Stroke::NONE,
                ));
            }
            for i in 0..points.len() {
                painter.line_segment(
                    [points[i], points[(i + 1) % points.len()]],
                    egui::Stroke::new(1.0, Color32::from_rgb(47, 95, 153)),
                );
            }
        }
        // Seam edges are chart borders, so draw them green over the UV wire.
        let point_uv = |point: u32| -> Option<V2> {
            let mut sum = V2::ZERO;
            let mut count = 0u32;
            for poly in &mesh.polygons {
                for corner in &poly.corners {
                    if corner.point == point {
                        sum += mesh.uv_of_set(corner, active_set);
                        count += 1;
                    }
                }
            }
            (count > 0).then(|| sum / count as f32)
        };
        for [a, b] in &mesh.seams {
            if let (Some(a), Some(b)) = (point_uv(*a), point_uv(*b)) {
                painter.line_segment(
                    [to_screen(a), to_screen(b)],
                    egui::Stroke::new(2.0, Color32::from_rgb(70, 170, 95)),
                );
            }
        }
        painter.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.0, Color32::from_rgb(160, 160, 160)),
            egui::StrokeKind::Inside,
        );
        for (i, uv) in values.iter().enumerate() {
            let p = to_screen(*uv);
            let chosen = self.uv_editor.selection.contains(&(i as u32));
            let pinned = mesh.is_uv_pinned(i as u32);
            painter.circle_filled(
                p,
                if chosen {
                    4.5
                } else if pinned {
                    3.4
                } else {
                    2.2
                },
                if pinned {
                    // Yellow is the documented Cheetah3D pinned-UV colour.
                    Color32::from_rgb(242, 201, 44)
                } else if chosen {
                    theme::ACCENT
                } else {
                    theme::SELECTION
                },
            );
            if chosen && pinned {
                painter.circle_stroke(p, 6.0, egui::Stroke::new(1.5, theme::ACCENT));
            }
        }
        if response.clicked() {
            if let Some(pointer) = response.interact_pointer_pos() {
                let mut best: Option<(f32, usize)> = None;
                for (i, uv) in values.iter().enumerate() {
                    let p = to_screen(*uv);
                    let d = (p - pointer).length();
                    if d < 12.0 && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                        best = Some((d, i));
                    }
                }
                let additive = ui.input(|input| input.modifiers.shift || input.modifiers.command);
                match best {
                    Some((_, index)) => {
                        if !additive {
                            self.uv_editor.selection.clear();
                        }
                        if additive && !self.uv_editor.selection.insert(index as u32) {
                            self.uv_editor.selection.remove(&(index as u32));
                        } else {
                            self.uv_editor.selection.insert(index as u32);
                        }
                        self.selected_uv = Some(index);
                    }
                    None if !additive => {
                        self.uv_editor.selection.clear();
                        self.selected_uv = None;
                    }
                    None => {}
                }
            }
        }
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = ui.input(|input| input.pointer.delta());
            if delta != Vec2::ZERO {
                self.nudge_selected_uv(V2::new(
                    delta.x / rect.width().max(1.0),
                    -delta.y / rect.height().max(1.0),
                ));
            }
        }
        let stats = lc_geom::uv_distortion_stats(&mesh, active_set);
        let mut footer = format!(
            "{} UV{} points · {} polygons · {} seams · {} pinned · {} UV sets",
            values.len(),
            active_set,
            mesh.polygons.len(),
            mesh.seams.len(),
            mesh.pinned_uvs.len(),
            mesh.uv_set_count()
        );
        if let Some(stats) = stats {
            footer.push_str(&format!(
                " · distortion mean {:.2} max {:.2} ({} over 2×)",
                stats.mean, stats.max, stats.distorted
            ));
        }
        ui.label(egui::RichText::new(footer).small().weak());
    }
}

fn node_header_color(kind: &ShaderNodeKind) -> Color32 {
    match kind {
        ShaderNodeKind::MaterialOutput => Color32::from_rgb(198, 138, 53),
        ShaderNodeKind::Principled => Color32::from_rgb(78, 126, 174),
        ShaderNodeKind::Noise { .. } => Color32::from_rgb(141, 127, 166),
        ShaderNodeKind::Voronoi { .. } => Color32::from_rgb(124, 141, 170),
        ShaderNodeKind::Checker { .. } => Color32::from_rgb(95, 143, 132),
        ShaderNodeKind::ColorRamp { .. } => Color32::from_rgb(138, 111, 168),
        ShaderNodeKind::Texture { .. } => Color32::from_rgb(106, 154, 94),
        ShaderNodeKind::Mix { .. } => Color32::from_rgb(176, 104, 104),
        ShaderNodeKind::Rgb { .. } => Color32::from_rgb(160, 122, 74),
        ShaderNodeKind::Value { .. } => Color32::from_rgb(138, 138, 138),
        ShaderNodeKind::Bump { .. } => Color32::from_rgb(138, 106, 74),
        ShaderNodeKind::Math { .. } => Color32::from_rgb(106, 122, 154),
        ShaderNodeKind::Mapping { .. } => Color32::from_rgb(122, 138, 106),
        _ => Color32::from_rgb(110, 122, 138),
    }
}

fn shader_graph_ui(ui: &mut egui::Ui, material: &mut Material, selected: &mut Option<u32>) {
    let canvas_size = Vec2::new(
        ui.available_width().max(320.0),
        (ui.available_height() - 54.0).max(220.0),
    );
    let (rect, _) = ui.allocate_exact_size(canvas_size, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(237, 237, 237));
    let grid = 22.0;
    let mut gx = rect.left();
    while gx < rect.right() {
        painter.line_segment(
            [Pos2::new(gx, rect.top()), Pos2::new(gx, rect.bottom())],
            egui::Stroke::new(0.6, Color32::from_rgb(225, 225, 225)),
        );
        gx += grid;
    }
    let mut gy = rect.top();
    while gy < rect.bottom() {
        painter.line_segment(
            [Pos2::new(rect.left(), gy), Pos2::new(rect.right(), gy)],
            egui::Stroke::new(0.6, Color32::from_rgb(225, 225, 225)),
        );
        gy += grid;
    }
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, Color32::from_rgb(190, 190, 190)),
        egui::StrokeKind::Inside,
    );
    let scale = 0.9;
    let origin = rect.min + Vec2::new(16.0, 16.0);
    let node_size = Vec2::new(154.0, 66.0);
    let node_rect = |position: V2| {
        Rect::from_min_size(
            origin + Vec2::new(position.x * scale, position.y * scale),
            node_size,
        )
    };
    let mut node_centers: HashMap<u32, Rect> = HashMap::new();
    for node in &material.graph.nodes {
        node_centers.insert(node.id, node_rect(node.position));
    }
    for link in &material.graph.links {
        if let (Some(from_rect), Some(to_rect)) =
            (node_centers.get(&link.from), node_centers.get(&link.to))
        {
            let from = Pos2::new(from_rect.right(), from_rect.center().y);
            let to = Pos2::new(
                to_rect.left(),
                to_rect.top() + 24.0 + link.to_input as f32 * 11.0,
            );
            let bend = (to.x - from.x).abs().max(40.0) * 0.55;
            let mut points = Vec::new();
            for step in 0..=14 {
                let t = step as f32 / 14.0;
                let p0 = from;
                let p1 = from + Vec2::new(bend, 0.0);
                let p2 = to - Vec2::new(bend, 0.0);
                let p3 = to;
                let u = 1.0 - t;
                points.push(Pos2::new(
                    u * u * u * p0.x
                        + 3.0 * u * u * t * p1.x
                        + 3.0 * u * t * t * p2.x
                        + t * t * t * p3.x,
                    u * u * u * p0.y
                        + 3.0 * u * u * t * p1.y
                        + 3.0 * u * t * t * p2.y
                        + t * t * t * p3.y,
                ));
            }
            painter.add(egui::Shape::line(
                points,
                egui::Stroke::new(1.6, Color32::from_rgb(183, 158, 55)),
            ));
        }
    }
    for node in &material.graph.nodes {
        let rect = node_rect(node.position);
        let selected_node = *selected == Some(node.id);
        painter.rect_filled(rect, 3.0, Color32::from_rgb(249, 249, 249));
        let header = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 19.0));
        painter.rect_filled(header, 3.0, node_header_color(&node.kind));
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(header.left(), header.bottom() - 4.0),
                Vec2::new(header.width(), 4.0),
            ),
            0.0,
            node_header_color(&node.kind),
        );
        painter.text(
            header.left_center() + Vec2::new(7.0, 0.0),
            egui::Align2::LEFT_CENTER,
            node.kind.label(),
            egui::FontId::proportional(10.0),
            Color32::WHITE,
        );
        if matches!(
            node.kind,
            ShaderNodeKind::Noise { .. }
                | ShaderNodeKind::Voronoi { .. }
                | ShaderNodeKind::Checker { .. }
                | ShaderNodeKind::Texture { .. }
                | ShaderNodeKind::ColorRamp { .. }
        ) {
            let thumb = Rect::from_min_size(
                Pos2::new(header.right() - 22.0, header.top() + 2.0),
                Vec2::splat(15.0),
            );
            painter.rect_filled(thumb, 2.0, Color32::from_rgb(235, 235, 235));
            for (dx, dy) in [
                (0.25, 0.3),
                (0.55, 0.2),
                (0.7, 0.6),
                (0.35, 0.75),
                (0.8, 0.85),
            ] {
                painter.circle_filled(
                    thumb.min + Vec2::new(dx * thumb.width(), dy * thumb.height()),
                    1.2,
                    node_header_color(&node.kind),
                );
            }
        }
        let inputs = node.kind.input_names();
        for (i, name) in inputs.iter().enumerate() {
            if i >= 4 {
                break;
            }
            let y = rect.top() + 24.0 + i as f32 * 11.0;
            if y > rect.bottom() - 4.0 {
                break;
            }
            painter.circle_filled(
                Pos2::new(rect.left(), y),
                2.4,
                Color32::from_rgb(150, 150, 150),
            );
            painter.circle_filled(
                Pos2::new(rect.left(), y),
                1.2,
                node_header_color(&node.kind),
            );
            painter.text(
                Pos2::new(rect.left() + 6.0, y),
                egui::Align2::LEFT_CENTER,
                *name,
                egui::FontId::proportional(8.5),
                Color32::from_rgb(82, 82, 82),
            );
        }
        painter.circle_filled(
            Pos2::new(rect.right(), rect.center().y + 8.0),
            3.0,
            Color32::from_rgb(120, 120, 120),
        );
        painter.rect_stroke(
            rect,
            3.0,
            egui::Stroke::new(
                if selected_node { 2.0 } else { 1.0 },
                if selected_node {
                    theme::SELECTION
                } else {
                    Color32::from_rgb(168, 168, 168)
                },
            ),
            egui::StrokeKind::Inside,
        );
    }
    let node_data: Vec<(u32, V2)> = material
        .graph
        .nodes
        .iter()
        .map(|n| (n.id, n.position))
        .collect();
    for (id, position) in node_data {
        let rect = node_rect(position);
        let response = ui.interact(
            rect,
            ui.id().with(("shader-node", id)),
            Sense::click_and_drag(),
        );
        if response.clicked() {
            *selected = Some(id);
        }
        if response.dragged() {
            if let Some(node) = material.graph.node_mut(id) {
                let delta = response.drag_delta();
                node.position += V2::new(delta.x, delta.y) / scale;
                node.position = node.position.clamp(V2::ZERO, V2::new(520.0, 380.0));
            }
        }
    }
    ui.horizontal_wrapped(|ui| {
        ui.menu_button("Add Node", |ui| {
            let add = |_ui: &mut egui::Ui,
                       material: &mut Material,
                       kind: ShaderNodeKind,
                       selected: &mut Option<u32>| {
                let id = material.graph.add_node(
                    kind,
                    V2::new(40.0 + (material.graph.nodes.len() % 4) as f32 * 52.0, 180.0),
                );
                *selected = Some(id);
            };
            if ui.button("Noise").clicked() {
                add(
                    ui,
                    material,
                    ShaderNodeKind::Noise {
                        kind: lc_shading::NoiseKind::Fractal,
                        scale: 8.0,
                        detail: 4,
                        roughness: 0.5,
                        distortion: 0.0,
                    },
                    selected,
                );
            }
            if ui.button("Voronoi").clicked() {
                add(
                    ui,
                    material,
                    ShaderNodeKind::Voronoi {
                        scale: 8.0,
                        jitter: 1.0,
                    },
                    selected,
                );
            }
            if ui.button("Checker").clicked() {
                add(
                    ui,
                    material,
                    ShaderNodeKind::Checker {
                        scale: 6.0,
                        color_a: Vec3::splat(0.9),
                        color_b: Vec3::splat(0.1),
                    },
                    selected,
                );
            }
            if ui.button("Color Ramp").clicked() {
                add(
                    ui,
                    material,
                    ShaderNodeKind::ColorRamp {
                        stops: vec![
                            lc_shading::RampStop {
                                position: 0.0,
                                color: Vec3::splat(0.1),
                            },
                            lc_shading::RampStop {
                                position: 1.0,
                                color: Vec3::splat(0.9),
                            },
                        ],
                    },
                    selected,
                );
            }
            if ui.button("Image Texture").clicked() {
                add(
                    ui,
                    material,
                    ShaderNodeKind::Texture {
                        path: String::new(),
                        uv_set: 0,
                    },
                    selected,
                );
            }
            if ui.button("RGB").clicked() {
                add(
                    ui,
                    material,
                    ShaderNodeKind::Rgb {
                        color: Vec3::splat(0.8),
                    },
                    selected,
                );
            }
            if ui.button("Value").clicked() {
                add(ui, material, ShaderNodeKind::Value { value: 0.5 }, selected);
            }
            if ui.button("Bump").clicked() {
                add(
                    ui,
                    material,
                    ShaderNodeKind::Bump {
                        strength: 0.3,
                        distance: 0.02,
                    },
                    selected,
                );
            }
        });
        if let Some(id) = *selected {
            if ui.button("Remove Node").clicked()
                && !material
                    .graph
                    .node(id)
                    .is_some_and(|n| matches!(&n.kind, ShaderNodeKind::MaterialOutput))
            {
                material.graph.remove_node(id);
                *selected = None;
            }
            if let Some(node) = material.graph.node_mut(id) {
                if let ShaderNodeKind::Texture { path, uv_set } = &mut node.kind {
                    ui.label("Texture path");
                    ui.text_edit_singleline(path);
                    ui.horizontal(|ui| {
                        ui.label("UV set");
                        ui.add(
                            egui::DragValue::new(uv_set)
                                .range(0..=7)
                                .speed(0.1)
                                .prefix("UV"),
                        );
                    });
                    ui.label(format!("Samples generated UV{uv_set} (0 = primary set)"));
                }
            }
            let target = material
                .graph
                .nodes
                .iter()
                .find(|n| matches!(n.kind, ShaderNodeKind::Principled))
                .map(|n| n.id);
            if let Some(target) = target {
                if target != id {
                    ui.label("Connect selected →");
                    if ui.button("Base Color").clicked() {
                        material.graph.link(id, target, 0);
                    }
                    if ui.button("Metallic").clicked() {
                        material.graph.link(id, target, 1);
                    }
                    if ui.button("Roughness").clicked() {
                        material.graph.link(id, target, 2);
                    }
                    if ui.button("Emission").clicked() {
                        material.graph.link(id, target, 5);
                    }
                    if ui.button("Normal").clicked() {
                        material.graph.link(id, target, 8);
                    }
                }
            }
            let targets: Vec<_> = material
                .graph
                .nodes
                .iter()
                .filter(|target| target.id != id)
                .filter_map(|target| {
                    let inputs: Vec<(usize, String)> = target
                        .kind
                        .input_names()
                        .iter()
                        .enumerate()
                        .map(|(index, name)| (index, (*name).to_string()))
                        .collect();
                    (!inputs.is_empty())
                        .then(|| (target.id, target.kind.label().to_string(), inputs))
                })
                .collect();
            if !targets.is_empty() {
                ui.menu_button("Connect to any node input…", |ui| {
                    for (target_id, target_name, inputs) in &targets {
                        ui.menu_button(format!("{target_name} #{target_id}"), |ui| {
                            for (input_index, input_name) in inputs {
                                if ui.button(input_name).clicked() {
                                    material.graph.link(id, *target_id, *input_index);
                                }
                            }
                        });
                    }
                });
            }
            if ui.button("Disconnect selected node").clicked() {
                material.graph.links.retain(|link| link.from != id);
            }
        }
    });
}

impl LeopardApp {
    pub(super) fn ui_material_shelf(&mut self, ui: &mut egui::Ui) {
        let materials: Vec<(MaterialId, String, Vec3, f32)> = self
            .doc
            .materials
            .iter()
            .map(|material| {
                (
                    material.id,
                    material.name.clone(),
                    material.preview_color(&self.texture_cache),
                    material.preview_roughness(&self.texture_cache),
                )
            })
            .collect();
        let mut add: Option<u8> = None;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Materials")
                    .size(11.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.menu_button("Add material", |ui| {
                if ui.button("Principled Material").clicked() {
                    add = Some(0);
                }
                if ui.button("Metal").clicked() {
                    add = Some(1);
                }
                if ui.button("Glass").clicked() {
                    add = Some(2);
                }
                if ui.button("Matte").clicked() {
                    add = Some(3);
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{} materials", materials.len()))
                        .size(9.5)
                        .color(theme::TEXT_WEAK),
                );
            });
        });
        theme::rule(ui);
        ui.add_space(2.0);
        let mut select: Option<MaterialId> = None;
        let mut delete: Option<MaterialId> = None;
        let mut assign: Option<MaterialId> = None;
        let mut edit_nodes: Option<MaterialId> = None;
        egui::ScrollArea::horizontal()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (id, name, color, roughness) in &materials {
                        let selected = self.selected_material == Some(*id);
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(76.0, 84.0), Sense::click());
                        let painter = ui.painter_at(rect);
                        if selected {
                            painter.rect_filled(rect, 3.0, theme::SELECTION_SOFT);
                            painter.rect_stroke(
                                rect,
                                3.0,
                                egui::Stroke::new(1.0, theme::SELECTION),
                                egui::StrokeKind::Inside,
                            );
                        } else if response.hovered() {
                            painter.rect_filled(rect, 3.0, Color32::from_rgb(242, 242, 242));
                        }
                        let ball = Rect::from_min_size(
                            rect.min + Vec2::new(9.0, 3.0),
                            Vec2::new(58.0, 58.0),
                        );
                        theme::material_ball(ui, ball, *color, *roughness, selected);
                        let label = if name.chars().count() > 11 {
                            format!("{}…", name.chars().take(10).collect::<String>())
                        } else {
                            name.clone()
                        };
                        painter.text(
                            Pos2::new(rect.center().x, rect.bottom() - 3.0),
                            egui::Align2::CENTER_BOTTOM,
                            label,
                            egui::FontId::proportional(9.5),
                            theme::TEXT,
                        );
                        if response.clicked() {
                            select = Some(*id);
                        }
                        if response.double_clicked() {
                            select = Some(*id);
                            edit_nodes = Some(*id);
                        }
                        response.context_menu(|ui| {
                            if ui.button("Edit Node Graph").clicked() {
                                edit_nodes = Some(*id);
                            }
                            if ui.button("Assign to Selected Object").clicked() {
                                assign = Some(*id);
                            }
                            ui.separator();
                            if ui
                                .add_enabled(materials.len() > 1, egui::Button::new("Delete"))
                                .clicked()
                            {
                                delete = Some(*id);
                            }
                        });
                    }
                    ui.allocate_exact_size(Vec2::new(4.0, 1.0), Sense::hover());
                });
            });
        if let Some(preset) = add {
            self.add_material_preset(preset);
        }
        if let Some(id) = select {
            self.selected_material = Some(id);
            self.inspector_tab = InspectorTab::Material;
            self.status = self
                .doc
                .materials
                .get(id)
                .map(|material| format!("Selected material {}", material.name))
                .unwrap_or_else(|| "Selected material".into());
        }
        if let Some(id) = edit_nodes {
            self.selected_material = Some(id);
            self.editor_view = EditorView::Nodes;
        }
        if let Some(id) = assign {
            if self.selection.is_empty() {
                self.status = "Select an object before assigning a material".into();
            } else {
                let mut edits = Vec::new();
                for object in self.selection.ids().to_vec() {
                    if let Some(node) = self.doc.scene.get(object).cloned() {
                        let before = NodeProps::of(&node);
                        let mut after = before.clone();
                        after.material = Some(id);
                        if after != before {
                            edits.push((object, before, after));
                        }
                    }
                }
                if !edits.is_empty() {
                    let count = edits.len();
                    self.history
                        .execute(&mut self.doc, Box::new(SetNodesPropsCmd { edits }));
                    self.selected_material = Some(id);
                    self.status = if count == 1 {
                        "Assigned material to object".into()
                    } else {
                        format!("Assigned material to {count} objects")
                    };
                }
            }
        }
        if let Some(id) = delete {
            if self.doc.materials.iter().count() > 1 {
                if let Some(material) = self.doc.materials.remove(id) {
                    self.history.execute(
                        &mut self.doc,
                        Box::new(lc_scene::RemoveMaterialCmd { material }),
                    );
                    self.selected_material = self.doc.materials.iter().next().map(|m| m.id);
                    self.status = "Deleted material".into();
                }
            }
        }
    }

    fn add_material_preset(&mut self, preset: u8) {
        let mut material = match preset {
            1 => Material::pbr(0, "Metal", Vec3::new(0.72, 0.74, 0.78), 0.22, 1.0),
            2 => Material::pbr(0, "Glass", Vec3::ONE, 0.03, 0.0).with_transmission(1.0, 1.45),
            3 => Material::pbr(0, "Matte", Vec3::new(0.76, 0.71, 0.62), 0.88, 0.0),
            _ => {
                let mut material = Material::new(0, "Material");
                material.name = self.doc.unique_name(&material.name);
                material
            }
        };
        material.name = self.doc.unique_name(&material.name);
        let id = self.doc.materials.add(material);
        self.selected_material = Some(id);
        self.inspector_tab = InspectorTab::Material;
        self.status = format!(
            "Added {}",
            self.doc
                .materials
                .get(id)
                .map(|m| m.name.clone())
                .unwrap_or_default()
        );
    }

    pub(super) fn ui_node_editor(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected_material else {
            theme::empty(ui, "Select a material in the shelf to edit its nodes");
            return;
        };
        let Some(original) = self.doc.materials.get(id).cloned() else {
            theme::empty(ui, "Select a material in the shelf to edit its nodes");
            return;
        };
        let mut edited = original.clone();
        let mut selected = self.selected_shader_node;
        shader_graph_ui(ui, &mut edited, &mut selected);
        self.selected_shader_node = selected;
        if edited != original {
            if self.pending_material.is_none() {
                self.pending_material = Some((id, original));
            }
            self.doc.materials.insert(edited);
            self.status = "Editing node graph".into();
        }
    }
}

impl LeopardApp {
    pub(super) fn ui_timeline(&mut self, ui: &mut egui::Ui) {
        if self.show_curves {
            self.ui_curve_editor(ui);
        }
        let mut seek: Option<f32> = None;
        let mut toggle_play = false;
        let mut key = false;
        let mut range_start = self.playback.start * self.playback.fps.max(1.0);
        let mut range_end = self.playback.end * self.playback.fps.max(1.0);
        let mut range_looping = self.playback.looping;
        let mut range_start_changed = false;
        let mut range_end_changed = false;
        let mut range_loop_changed = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            if icons::icon_button_sized(ui, Icon::Start, "Go to start", false, true, 22.0).clicked()
            {
                seek = Some(self.playback.start);
            }
            if icons::icon_button_sized(
                ui,
                Icon::StepBack,
                "Step back one frame",
                false,
                true,
                22.0,
            )
            .clicked()
            {
                let frame = (self.playback.time * self.playback.fps).floor() - 1.0;
                seek = Some((frame / self.playback.fps).max(self.playback.start));
            }
            let transport = if self.playback.playing {
                Icon::Pause
            } else {
                Icon::Play
            };
            if icons::icon_button_sized(
                ui,
                transport,
                if self.playback.playing {
                    "Pause"
                } else {
                    "Play"
                },
                false,
                true,
                22.0,
            )
            .clicked()
            {
                toggle_play = true;
            }
            if icons::icon_button_sized(
                ui,
                Icon::StepForward,
                "Step forward one frame",
                false,
                true,
                22.0,
            )
            .clicked()
            {
                let frame = (self.playback.time * self.playback.fps).floor() + 1.0;
                seek = Some((frame / self.playback.fps).min(self.playback.end));
            }
            if icons::icon_button_sized(ui, Icon::End, "Go to end", false, true, 22.0).clicked() {
                seek = Some(self.playback.end);
            }
            ui.add_space(3.0);
            if icons::icon_button_sized(
                ui,
                Icon::Record,
                "Key transforms at the current frame",
                false,
                self.key_transforms_ready(),
                22.0,
            )
            .clicked()
            {
                key = true;
            }
            ui.add_space(8.0);
            let frame = self.playback.frame();
            ui.label(
                egui::RichText::new(format!("Frame {frame:>4}"))
                    .size(10.5)
                    .monospace()
                    .color(theme::TEXT),
            );
            ui.menu_button("Range", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Start frame");
                    range_start_changed |= ui
                        .add(
                            egui::DragValue::new(&mut range_start)
                                .speed(1.0)
                                .range(0.0..=MAX_PLAYBACK_FRAME),
                        )
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label("End frame");
                    range_end_changed |= ui
                        .add(
                            egui::DragValue::new(&mut range_end)
                                .speed(1.0)
                                .range(0.0..=MAX_PLAYBACK_FRAME),
                        )
                        .changed();
                });
                range_loop_changed |= ui.checkbox(&mut range_looping, "Loop playback").changed();
            })
            .response
            .on_hover_text("Playback start, end and loop mode");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::icon_button_sized(
                    ui,
                    Icon::Curves,
                    "Show F-curve editor",
                    self.show_curves,
                    true,
                    22.0,
                )
                .clicked()
                {
                    self.show_curves = !self.show_curves;
                }
                ui.label(
                    egui::RichText::new(format!("{} fps", self.playback.fps.round()))
                        .size(9.5)
                        .color(theme::TEXT_WEAK),
                );
            });
        });
        if range_start_changed {
            self.set_playback_start(range_start / self.playback.fps.max(1.0));
        }
        if range_end_changed {
            self.set_playback_end(range_end / self.playback.fps.max(1.0));
        }
        if range_loop_changed {
            self.set_playback_looping(range_looping);
        }
        if toggle_play {
            self.playback.playing = !self.playback.playing;
        }
        if let Some(time) = seek {
            self.playback.time = time.clamp(self.playback.start, self.playback.end);
            self.playback.playing = false;
        }
        if key {
            self.key_transform_channels();
        }
        self.ui_timeline_ruler(ui);
    }

    fn ui_timeline_ruler(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 16.0),
            Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(201, 201, 201));
        painter.line_segment(
            [rect.left_top(), rect.right_top()],
            egui::Stroke::new(1.0, Color32::from_rgb(170, 170, 170)),
        );
        let fps = self.playback.fps.max(1.0);
        let start = self.playback.start * fps;
        let end = (self.playback.end * fps).max(start + 1.0);
        let total = (end - start).max(1.0);
        let x_of = |frame: f32| rect.left() + (frame - start) / total * rect.width();
        let target_step = (total / (rect.width() / 72.0)).max(1.0);
        let mut step = 1.0;
        for candidate in [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0] {
            step = candidate;
            if candidate >= target_step {
                break;
            }
        }
        let mut frame = (start / step).floor() * step;
        while frame <= end + 0.1 {
            let x = x_of(frame);
            painter.line_segment(
                [Pos2::new(x, rect.top() + 7.0), Pos2::new(x, rect.bottom())],
                egui::Stroke::new(1.0, Color32::from_rgb(150, 150, 150)),
            );
            let label = if step < 1.0 {
                format!("{frame:.1}")
            } else {
                format!("{:.0}", frame)
            };
            painter.text(
                Pos2::new(x + 2.0, rect.top() + 2.0),
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::monospace(8.5),
                Color32::from_rgb(92, 92, 92),
            );
            frame += step;
        }
        if let Some(id) = self.selection.primary() {
            if let Some(node) = self.doc.scene.get(id) {
                for time in node.animation.key_times() {
                    let x = x_of(time * fps);
                    if x < rect.left() - 2.0 || x > rect.right() + 2.0 {
                        continue;
                    }
                    let c = Pos2::new(x, rect.top() + 10.0);
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            c + Vec2::new(0.0, -3.5),
                            c + Vec2::new(3.2, 0.0),
                            c + Vec2::new(0.0, 3.5),
                            c + Vec2::new(-3.2, 0.0),
                        ],
                        theme::SELECTION,
                        egui::Stroke::new(0.7, Color32::WHITE),
                    ));
                }
            }
        }
        let playhead = x_of(self.playback.time * fps).clamp(rect.left(), rect.right());
        painter.line_segment(
            [
                Pos2::new(playhead, rect.top() - 1.0),
                Pos2::new(playhead, rect.bottom()),
            ],
            egui::Stroke::new(1.5, theme::ACCENT),
        );
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(playhead - 4.0, rect.top()),
                Pos2::new(playhead + 4.0, rect.top()),
                Pos2::new(playhead, rect.top() + 5.0),
            ],
            theme::ACCENT,
            egui::Stroke::NONE,
        ));
        if response.clicked() || response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let t = ((pos.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
                let frame = start + t * total;
                self.playback.time = (frame / fps).clamp(self.playback.start, self.playback.end);
                self.playback.playing = false;
            }
        }
    }

    fn ui_curve_editor(&mut self, ui: &mut egui::Ui) {
        let mut start_frame = self.playback.start * self.playback.fps;
        let mut end_frame = self.playback.end * self.playback.fps;
        let mut fps = self.playback.fps;
        let mut loop_playback = self.playback.looping;
        let mut start_changed = false;
        let mut end_changed = false;
        let mut fps_changed = false;
        let mut loop_changed = false;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("F-Curves")
                    .size(10.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Start")
                    .size(9.0)
                    .color(theme::TEXT_WEAK),
            );
            start_changed |= ui
                .add(
                    egui::DragValue::new(&mut start_frame)
                        .speed(1.0)
                        .range(0.0..=MAX_PLAYBACK_FRAME),
                )
                .changed();
            ui.label(egui::RichText::new("End").size(9.0).color(theme::TEXT_WEAK));
            end_changed |= ui
                .add(
                    egui::DragValue::new(&mut end_frame)
                        .speed(1.0)
                        .range(0.0..=MAX_PLAYBACK_FRAME),
                )
                .changed();
            ui.label(egui::RichText::new("fps").size(9.0).color(theme::TEXT_WEAK));
            fps_changed |= ui
                .add(egui::DragValue::new(&mut fps).speed(0.2).range(1.0..=240.0))
                .changed();
            loop_changed |= ui.checkbox(&mut loop_playback, "Loop").changed();
            let has_selection = !self.selected_curve_keys().is_empty();
            if ui
                .add_enabled(
                    has_selection,
                    egui::Button::new(egui::RichText::new("Delete Keys").size(9.0)),
                )
                .on_hover_text("Delete every selected F-curve key")
                .clicked()
                && has_selection
            {
                self.delete_selected_curve_key();
            }
            if ui
                .add_enabled(
                    has_selection,
                    egui::Button::new(egui::RichText::new("Copy").size(9.0)),
                )
                .on_hover_text("Copy selected keys (Ctrl/Cmd+C)")
                .clicked()
            {
                self.copy_selected_curve_keys();
            }
            let paste_ready =
                !self.curve_clipboard.keys.is_empty() && self.current_anim_target().is_some();
            if ui
                .add_enabled(
                    paste_ready,
                    egui::Button::new(egui::RichText::new("Paste").size(9.0)),
                )
                .on_hover_text("Paste at the playhead (Ctrl/Cmd+V)")
                .clicked()
            {
                self.paste_curve_keys();
            }
            ui.separator();
            ui.label(
                egui::RichText::new("Retime ×")
                    .size(9.0)
                    .color(theme::TEXT_WEAK),
            );
            let mut retime_factor = self.retime_factor;
            ui.add(
                egui::DragValue::new(&mut retime_factor)
                    .speed(0.01)
                    .range(0.01..=100.0)
                    .max_decimals(3),
            );
            let retime_ready = has_selection && (retime_factor - 1.0).abs() > 1e-6;
            if ui
                .add_enabled(
                    retime_ready,
                    egui::Button::new(egui::RichText::new("Scale Keys").size(9.0)),
                )
                .on_hover_text(
                    "Scale selected keys around the earliest selected time; fractional times are preserved",
                )
                .clicked()
            {
                self.retime_factor = retime_factor;
                self.retime_selected_curve_keys(retime_factor);
            } else {
                self.retime_factor = retime_factor;
            }
            ui.separator();
            let selected_interpolation = self.selected_curve_interpolation();
            let mut choose_interpolation = None;
            ui.add_enabled_ui(has_selection, |ui| {
                egui::ComboBox::from_id_salt("curve_selected_interpolation")
                    .width(66.0)
                    .selected_text(match (has_selection, selected_interpolation) {
                        (false, _) => "—".to_string(),
                        (true, Some(interpolation)) => interpolation.label().to_string(),
                        (true, None) => "Mixed".to_string(),
                    })
                    .show_ui(ui, |ui| {
                        ui.label(
                            egui::RichText::new("Key interpolation")
                                .size(9.0)
                                .color(theme::TEXT_WEAK),
                        );
                        for interpolation in Interpolation::ALL {
                            if ui
                                .selectable_label(
                                    selected_interpolation == Some(interpolation),
                                    interpolation.label(),
                                )
                                .clicked()
                            {
                                choose_interpolation = Some(interpolation);
                            }
                        }
                    });
            })
            .response
            .on_hover_text("Set interpolation for every selected F-curve key");
            if let Some(interpolation) = choose_interpolation {
                self.set_selected_curve_key_interpolation(interpolation);
            }
            let selected_tangent = self.selected_curve_tangent_type();
            let mut choose_tangent = None;
            ui.add_enabled_ui(has_selection, |ui| {
                egui::ComboBox::from_id_salt("curve_selected_tangent")
                    .width(66.0)
                    .selected_text(match (has_selection, selected_tangent) {
                        (false, _) => "—".to_string(),
                        (true, Some(tangent)) => tangent.label().to_string(),
                        (true, None) => "Mixed".to_string(),
                    })
                    .show_ui(ui, |ui| {
                        ui.label(
                            egui::RichText::new("Tangent type (both sides)")
                                .size(9.0)
                                .color(theme::TEXT_WEAK),
                        );
                        for tangent in TangentType::ALL {
                            if ui
                                .selectable_label(
                                    selected_tangent == Some(tangent),
                                    tangent.label(),
                                )
                                .clicked()
                            {
                                choose_tangent = Some(tangent);
                            }
                        }
                    });
            })
            .response
            .on_hover_text("Set the tangent derivation for both sides of every selected key");
            if let Some(tangent) = choose_tangent {
                self.set_selected_curve_tangent_type(tangent, true, true);
            }
            ui.separator();
            let mut target_options: Vec<(AnimTarget, String)> = Vec::new();
            if let Some(id) = self.selection.primary() {
                if let Some(node) = self.doc.scene.get(id) {
                    target_options.push((AnimTarget::Object(id), format!("Object: {}", node.name)));
                    for (index, tag) in node.tags.iter().enumerate() {
                        if Self::tag_animation_channel(tag).is_some() {
                            target_options.push((
                                AnimTarget::Tag(id, index),
                                format!("Tag: {} #{}", tag.label(), index + 1),
                            ));
                        }
                    }
                }
            }
            if let Some(material_id) = self.selected_material {
                if let Some(material) = self.doc.materials.get(material_id) {
                    target_options.push((
                        AnimTarget::Material(material_id),
                        format!("Material: {}", material.name),
                    ));
                }
            }
            let current_target = self.current_anim_target();
            let mut chosen_target = current_target;
            egui::ComboBox::from_id_salt("curve_anim_target")
                .selected_text(
                    current_target
                        .map(|target| self.target_label(target))
                        .unwrap_or_else(|| "No target".into()),
                )
                .show_ui(ui, |ui| {
                    for (target, label) in &target_options {
                        ui.selectable_value(&mut chosen_target, Some(*target), label);
                    }
                });
            if chosen_target != current_target {
                if let Some(target) = chosen_target {
                    self.set_anim_target(target);
                }
            }
            ui.label(
                egui::RichText::new("Filter")
                    .size(9.0)
                    .color(theme::TEXT_WEAK),
            );
            egui::ComboBox::from_id_salt("curve_key_filter")
                .width(98.0)
                .selected_text(self.curve_filter.label())
                .show_ui(ui, |ui| {
                    for filter in CurveFilter::ALL {
                        ui.selectable_value(&mut self.curve_filter, filter, filter.label());
                    }
                });
            let key_ready = self.selected_curve_channel.is_some_and(|(target, row)| {
                self.current_anim_target() == Some(target)
                    && self.channel_applies_to_target(row.channel, target)
            });
            if ui
                .add_enabled(
                    key_ready,
                    egui::Button::new(egui::RichText::new("Key Channel").size(9.0)),
                )
                .on_hover_text("Record the selected registry channel at the playhead")
                .clicked()
            {
                if let Some((target, row)) = self.selected_curve_channel {
                    self.record_curve_row_key(target, row);
                }
            }
        });
        if fps_changed {
            self.set_playback_fps(fps);
        }
        if start_changed {
            self.set_playback_start(start_frame / self.playback.fps.max(1.0));
        }
        if end_changed {
            self.set_playback_end(end_frame / self.playback.fps.max(1.0));
        }
        if loop_changed {
            self.set_playback_looping(loop_playback);
        }

        let start = self.playback.start;
        let end = self.playback.end.max(start + 1e-4);
        let anim_target = self.current_anim_target();
        let mut clip = anim_target.and_then(|target| self.target_clip(target));
        let original = clip.clone();
        if !self.selected_keys.is_empty() && self.selected_curve_keys().is_empty() {
            self.selected_keys.clear();
        }
        if let Some(drag) = self.curve_key_drag.as_ref() {
            let valid = Some(drag.key.target) == anim_target
                && clip.as_ref().is_some_and(|clip| match drag.key.track {
                    CurveTrackId::Scalar(address) => {
                        Self::curve_key_index(clip, address, drag.original_time).is_some()
                    }
                    CurveTrackId::Rotation => clip.rotation_key_index(drag.original_time).is_some(),
                    CurveTrackId::MorphWeight(target) => {
                        Self::morph_curve_key_index(clip, target, drag.original_time).is_some()
                    }
                });
            if !valid {
                self.curve_key_drag = None;
            }
        }
        let mut remove_track: Option<ChannelAddress> = None;
        let mut remove_morph: Option<u32> = None;
        let mut remove_rotation = false;
        let mut set_row_interpolation: Option<(AnimTarget, CurveRow, Interpolation)> = None;
        let height = (ui.available_height() - 46.0).clamp(60.0, 140.0);
        let (area, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
        let left_rect = Rect::from_min_size(area.min, Vec2::new(170.0, height));
        let canvas_rect =
            Rect::from_min_max(Pos2::new(left_rect.right() + 1.0, area.top()), area.max);
        let list_painter = ui.painter_at(left_rect);
        list_painter.rect_filled(left_rect, 0.0, Color32::from_rgb(243, 243, 243));
        if let (Some(_), Some(clip)) = (anim_target, clip.as_mut()) {
            enum ListEntry {
                Header(String),
                Row(AnimTarget, CurveRow),
            }
            // The list groups channels under their target object, tag or
            // material. The current target shows every filtered row; other
            // animated targets show only the channels that own keys.
            let entries: Vec<ListEntry> = self
                .curve_groups()
                .iter()
                .flat_map(|group| {
                    std::iter::once(ListEntry::Header(group.label.clone())).chain(
                        group
                            .rows
                            .iter()
                            .map(|row| ListEntry::Row(group.target, *row)),
                    )
                })
                .collect();
            let row_count = entries.len();
            // Shrink rows slightly before hiding channels; a grouped list with
            // all transform rows plus several morph targets still fits.
            let row_h = if row_count > 9 {
                (height / row_count as f32).max(13.0)
            } else {
                16.0
            };
            let mut max_rows = (height / row_h).floor() as usize;
            if row_count > max_rows {
                max_rows = max_rows.saturating_sub(1).max(1);
            }
            for (row_index, entry) in entries.iter().take(max_rows).enumerate() {
                let y = left_rect.top() + row_index as f32 * row_h;
                let row_rect = Rect::from_min_size(
                    Pos2::new(left_rect.left(), y),
                    Vec2::new(left_rect.width(), row_h),
                );
                let (target, row) = match entry {
                    ListEntry::Header(label) => {
                        list_painter.text(
                            Pos2::new(row_rect.left() + 6.0, row_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            label,
                            egui::FontId::proportional(8.4),
                            theme::TEXT_WEAK,
                        );
                        continue;
                    }
                    ListEntry::Row(target, row) => (*target, *row),
                };
                let current_target_row = Some(target) == anim_target;
                let selected_row = self.selected_curve_channel == Some((target, row));
                if selected_row {
                    list_painter.rect_filled(row_rect, 2.0, Color32::from_rgb(231, 238, 247));
                }
                if ui
                    .interact(
                        row_rect,
                        ui.id().with(("curve_row", row_index)),
                        Sense::click(),
                    )
                    .clicked()
                {
                    // Clicking another target's row focuses that target before
                    // selecting the row; view state never edits history.
                    if !current_target_row {
                        self.set_anim_target(target);
                    }
                    self.selected_curve_channel = Some((target, row));
                }
                list_painter.circle_filled(
                    Pos2::new(row_rect.left() + 6.0, row_rect.center().y),
                    3.0,
                    channel_color(row.channel),
                );
                let label = self.curve_row_label(target, row);
                list_painter.text(
                    Pos2::new(row_rect.left() + 13.0, row_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    format!("{label} ·{}", row.channel.component_count()),
                    egui::FontId::proportional(8.8),
                    theme::TEXT,
                );
                let row_track = self.curve_row_track_for(target, row);
                let (has_track, interpolation) = match row_track {
                    CurveTrackId::MorphWeight(index) => {
                        let curve = clip.morph_weights.get(&index);
                        (
                            curve.is_some(),
                            curve
                                .and_then(|curve| curve.keys.first())
                                .map(|key| key.interpolation)
                                .unwrap_or(Interpolation::Smooth),
                        )
                    }
                    CurveTrackId::Scalar(address) => {
                        let track = clip.track(address);
                        (
                            track.is_some(),
                            track
                                .and_then(|track| track.curve.keys.first())
                                .map(|key| key.interpolation)
                                .unwrap_or(Interpolation::Smooth),
                        )
                    }
                    CurveTrackId::Rotation => {
                        let track = clip.rotation.as_ref();
                        (
                            track.is_some(),
                            track
                                .and_then(|track| track.keys.first())
                                .map(|key| key.interpolation)
                                .unwrap_or(Interpolation::Smooth),
                        )
                    }
                };
                let interp_rect = Rect::from_min_size(
                    Pos2::new(row_rect.left() + 74.0, y + 1.0),
                    Vec2::new(58.0, 14.0),
                );
                list_painter.rect_filled(interp_rect, 2.0, Color32::WHITE);
                list_painter.rect_stroke(
                    interp_rect,
                    2.0,
                    egui::Stroke::new(1.0, theme::BORDER_SOFT),
                    egui::StrokeKind::Inside,
                );
                list_painter.text(
                    interp_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    if has_track {
                        interpolation.label()
                    } else {
                        "—"
                    },
                    egui::FontId::proportional(8.3),
                    if has_track {
                        theme::TEXT
                    } else {
                        theme::TEXT_WEAK
                    },
                );
                if has_track
                    && current_target_row
                    && ui
                        .interact(
                            interp_rect,
                            ui.id().with(("curve_interp", row_index)),
                            Sense::click(),
                        )
                        .clicked()
                {
                    let next = match interpolation {
                        Interpolation::Smooth => Interpolation::Linear,
                        Interpolation::Linear => Interpolation::Constant,
                        Interpolation::Constant => Interpolation::Spline,
                        Interpolation::Spline => Interpolation::Smooth,
                    };
                    set_row_interpolation = Some((target, row, next));
                }
                if has_track && current_target_row {
                    let delete_rect = Rect::from_min_size(
                        Pos2::new(row_rect.right() - 18.0, y + 1.0),
                        Vec2::splat(14.0),
                    );
                    let x = delete_rect.shrink(3.0);
                    list_painter.line_segment(
                        [x.left_top(), x.right_bottom()],
                        egui::Stroke::new(1.2, theme::TEXT_WEAK),
                    );
                    list_painter.line_segment(
                        [x.right_top(), x.left_bottom()],
                        egui::Stroke::new(1.2, theme::TEXT_WEAK),
                    );
                    if ui
                        .interact(
                            delete_rect,
                            ui.id().with(("curve_delete", row_index)),
                            Sense::click(),
                        )
                        .clicked()
                    {
                        match row_track {
                            CurveTrackId::MorphWeight(index) => remove_morph = Some(index),
                            CurveTrackId::Scalar(address) => remove_track = Some(address),
                            CurveTrackId::Rotation => remove_rotation = true,
                        }
                    }
                }
            }
            if row_count == 0 {
                list_painter.text(
                    Pos2::new(left_rect.left() + 6.0, left_rect.top() + 14.0),
                    egui::Align2::LEFT_CENTER,
                    "No channels match the filter",
                    egui::FontId::proportional(9.0),
                    theme::TEXT_WEAK,
                );
            } else if row_count > max_rows {
                list_painter.text(
                    Pos2::new(left_rect.left() + 6.0, left_rect.bottom() - 5.0),
                    egui::Align2::LEFT_BOTTOM,
                    format!("+{} more rows", row_count - max_rows),
                    egui::FontId::proportional(8.0),
                    theme::TEXT_WEAK,
                );
            }
        } else {
            list_painter.text(
                Pos2::new(left_rect.left() + 6.0, left_rect.top() + 14.0),
                egui::Align2::LEFT_CENTER,
                "Select an object, tag or material",
                egui::FontId::proportional(9.0),
                theme::TEXT_WEAK,
            );
        }
        if let Some((target, row, interpolation)) = set_row_interpolation {
            self.set_curve_row_interpolation(target, row, interpolation);
        }
        let rect = canvas_rect;
        let painter = ui.painter_at(rect);
        painter.line_segment(
            [left_rect.right_top(), left_rect.right_bottom()],
            egui::Stroke::new(1.0, theme::BORDER),
        );
        painter.rect_filled(rect, 0.0, Color32::from_rgb(250, 250, 250));
        painter.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.0, theme::BORDER),
            egui::StrokeKind::Inside,
        );
        for i in 0..=4 {
            let x = rect.left() + rect.width() * i as f32 / 4.0;
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                egui::Stroke::new(0.6, Color32::from_rgb(228, 228, 228)),
            );
            let y = rect.top() + rect.height() * i as f32 / 4.0;
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                egui::Stroke::new(0.6, Color32::from_rgb(228, 228, 228)),
            );
        }
        let visible_rows = anim_target
            .map(|target| self.visible_curve_rows(target))
            .unwrap_or_default();
        let visible_tracks: Vec<CurveTrackId> = match anim_target {
            Some(target) => visible_rows
                .iter()
                .map(|row| self.curve_row_track_for(target, *row))
                .collect(),
            None => Vec::new(),
        };
        let track_visible = |track: CurveTrackId| visible_tracks.contains(&track);
        let series: Vec<(Color32, Vec<f32>)> = clip
            .as_ref()
            .map(|clip| {
                let mut series: Vec<(Color32, Vec<f32>)> = clip
                    .tracks
                    .iter()
                    .filter(|track| track_visible(CurveTrackId::Scalar(track.address)))
                    .map(|track| {
                        let values = (0..=64)
                            .map(|i| {
                                let t = i as f32 / 64.0;
                                track.curve.sample(start + (end - start) * t)
                            })
                            .collect();
                        (track_color(track.address), values)
                    })
                    .collect();
                if track_visible(CurveTrackId::Rotation) {
                    if let Some(track) = &clip.rotation {
                        let values = (0..=64)
                            .map(|i| {
                                let t = i as f32 / 64.0;
                                track
                                    .sample(start + (end - start) * t)
                                    .map(|q| q.w)
                                    .unwrap_or(1.0)
                            })
                            .collect();
                        series.push((channel_color(AnimChannel::Rotation), values));
                    }
                }
                for (&index, curve) in &clip.morph_weights {
                    if !track_visible(CurveTrackId::MorphWeight(index)) {
                        continue;
                    }
                    let values = (0..=64)
                        .map(|i| {
                            let t = i as f32 / 64.0;
                            curve.sample(start + (end - start) * t)
                        })
                        .collect();
                    series.push((channel_color(AnimChannel::MorphWeight), values));
                }
                series
            })
            .unwrap_or_default();
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for (_, values) in &series {
            for value in values {
                min = min.min(*value);
                max = max.max(*value);
            }
        }
        if !min.is_finite() || !max.is_finite() {
            min = -1.0;
            max = 1.0;
        }
        if (max - min).abs() < 1e-4 {
            min -= 1.0;
            max += 1.0;
        }
        let value_y =
            |value: f32| rect.bottom() - 6.0 - (value - min) / (max - min) * (rect.height() - 12.0);
        let canvas_response =
            ui.interact(rect, ui.id().with("curve_canvas"), Sense::click_and_drag());
        let fps = self.playback.fps.max(1.0);
        let time_at = |x: f32| {
            start + ((x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0) * (end - start)
        };
        let value_at = |y: f32| {
            min + ((rect.bottom() - 6.0 - y) / (rect.height() - 12.0).max(1.0)).clamp(0.0, 1.0)
                * (max - min)
        };
        let key_at = |pointer: Pos2| -> Option<(CurveTrackId, f32)> {
            let clip = clip.as_ref()?;
            let mut best: Option<(f32, CurveTrackId, f32)> = None;
            for track in &clip.tracks {
                if !track_visible(CurveTrackId::Scalar(track.address)) {
                    continue;
                }
                for key in &track.curve.keys {
                    if key.time < start || key.time > end {
                        continue;
                    }
                    let x = rect.left() + (key.time - start) / (end - start) * rect.width();
                    let y = value_y(key.value);
                    let distance = (Pos2::new(x, y) - pointer).length();
                    if distance <= 7.0 && best.map(|(best, _, _)| distance < best).unwrap_or(true) {
                        best = Some((distance, CurveTrackId::Scalar(track.address), key.time));
                    }
                }
            }
            if track_visible(CurveTrackId::Rotation) {
                if let Some(rotation) = &clip.rotation {
                    for key in &rotation.keys {
                        if key.time < start || key.time > end {
                            continue;
                        }
                        let x = rect.left() + (key.time - start) / (end - start) * rect.width();
                        let y = value_y(key.value.w);
                        let distance = (Pos2::new(x, y) - pointer).length();
                        if distance <= 7.0
                            && best.map(|(best, _, _)| distance < best).unwrap_or(true)
                        {
                            best = Some((distance, CurveTrackId::Rotation, key.time));
                        }
                    }
                }
            }
            for (target, curve) in &clip.morph_weights {
                if !track_visible(CurveTrackId::MorphWeight(*target)) {
                    continue;
                }
                for key in &curve.keys {
                    if key.time < start || key.time > end {
                        continue;
                    }
                    let x = rect.left() + (key.time - start) / (end - start) * rect.width();
                    let y = value_y(key.value);
                    let distance = (Pos2::new(x, y) - pointer).length();
                    if distance <= 7.0 && best.map(|(best, _, _)| distance < best).unwrap_or(true) {
                        best = Some((distance, CurveTrackId::MorphWeight(*target), key.time));
                    }
                }
            }
            best.map(|(_, track, time)| (track, time))
        };
        let tangent_dt = ((end - start) * 0.08).max(1.0 / fps);
        let tangent_handles =
            if let (Some(target), Some(clip)) = (anim_target, clip.as_ref()) {
                let mut handles = Vec::new();
                for selected in self.selected_curve_keys() {
                    if selected.target != target {
                        continue;
                    }
                    let (key, tangent_in, tangent_out) =
                        match selected.track {
                            CurveTrackId::Scalar(address) => {
                                let Some(track) = clip.track(address) else {
                                    continue;
                                };
                                let Some(key) = track.curve.keys.iter().find(|key| {
                                    (key.time - selected.time).abs() <= KEY_TIME_EPSILON
                                }) else {
                                    continue;
                                };
                                (*key, key.tangent_in, key.tangent_out)
                            }
                            CurveTrackId::MorphWeight(index) => {
                                let Some(curve) = clip.morph_weights.get(&index) else {
                                    continue;
                                };
                                let Some(key) = curve.keys.iter().find(|key| {
                                    (key.time - selected.time).abs() <= KEY_TIME_EPSILON
                                }) else {
                                    continue;
                                };
                                (*key, key.tangent_in, key.tangent_out)
                            }
                            CurveTrackId::Rotation => continue,
                        };
                    if key.interpolation != Interpolation::Spline {
                        continue;
                    }
                    for (incoming, slope) in [(true, tangent_in), (false, tangent_out)] {
                        let direction = if incoming { -1.0 } else { 1.0 };
                        let endpoint_time = (key.time + direction * tangent_dt).clamp(start, end);
                        let time_offset = endpoint_time - key.time;
                        if time_offset.abs() <= 1e-6 {
                            continue;
                        }
                        let value = key.value + slope * time_offset;
                        handles.push(CurveTangentHandle {
                            target,
                            track: selected.track,
                            time: key.time,
                            incoming,
                            position: Pos2::new(
                                rect.left()
                                    + (endpoint_time - start) / (end - start) * rect.width(),
                                value_y(value),
                            ),
                            key_value: key.value,
                            time_offset,
                            slope,
                        });
                    }
                }
                handles
            } else {
                Vec::new()
            };
        let tangent_at = |pointer: Pos2| -> Option<CurveTangentHandle> {
            tangent_handles
                .iter()
                .copied()
                .filter(|handle| (handle.position - pointer).length() <= 7.0)
                .min_by(|a, b| {
                    (a.position - pointer)
                        .length()
                        .total_cmp(&(b.position - pointer).length())
                })
        };
        let additive = ui.input(|input| input.modifiers.shift || input.modifiers.command);
        if canvas_response.clicked() {
            if let (Some(pointer), Some(target)) =
                (canvas_response.interact_pointer_pos(), anim_target)
            {
                if tangent_at(pointer).is_none() {
                    match key_at(pointer) {
                        Some((track, time)) if additive => {
                            self.toggle_target_track_key(target, track, time);
                        }
                        Some((track, time)) => {
                            self.select_target_track_key_additive(target, track, time, false);
                        }
                        None if !additive => self.clear_curve_key_selection(),
                        None => {}
                    }
                }
            }
        }
        if canvas_response.drag_started_by(egui::PointerButton::Primary) {
            if let (Some(pointer), Some(target)) =
                (canvas_response.interact_pointer_pos(), anim_target)
            {
                if let Some(handle) = tangent_at(pointer) {
                    self.curve_key_drag = None;
                    self.curve_box_select = None;
                    self.curve_tangent_drag = Some(CurveTangentDrag {
                        target: handle.target,
                        track: handle.track,
                        time: handle.time,
                        incoming: handle.incoming,
                        key_value: handle.key_value,
                        time_offset: handle.time_offset,
                        original_slope: handle.slope,
                        current_slope: handle.slope,
                    });
                } else if let Some((track, time)) = key_at(pointer) {
                    let key = SelectedKey {
                        target,
                        track,
                        time,
                    };
                    if !self.is_curve_key_selected(&key) {
                        self.select_target_track_key_additive(target, track, time, additive);
                    }
                    self.curve_box_select = None;
                    self.curve_tangent_drag = None;
                    self.curve_key_drag = Some(CurveKeyDrag {
                        key,
                        original_time: time,
                        current_time: time,
                        original_selection: self.selected_curve_keys(),
                    });
                } else {
                    self.curve_key_drag = None;
                    self.curve_tangent_drag = None;
                    self.curve_box_select = Some(CurveBoxSelect {
                        start: pointer,
                        current: pointer,
                        additive,
                    });
                }
            }
        }
        if canvas_response.dragged_by(egui::PointerButton::Primary) {
            if let (Some(pointer), Some(mut drag)) = (
                canvas_response.interact_pointer_pos(),
                self.curve_key_drag.clone(),
            ) {
                drag.current_time = ((time_at(pointer.x) * fps).round() / fps).max(0.0);
                self.curve_key_drag = Some(drag);
            }
            if let (Some(pointer), Some(mut drag)) = (
                canvas_response.interact_pointer_pos(),
                self.curve_tangent_drag,
            ) {
                drag.current_slope = (value_at(pointer.y) - drag.key_value) / drag.time_offset;
                self.curve_tangent_drag = Some(drag);
            }
            if let (Some(pointer), Some(mut marquee)) = (
                canvas_response.interact_pointer_pos(),
                self.curve_box_select,
            ) {
                marquee.current = pointer;
                self.curve_box_select = Some(marquee);
            }
        }
        if canvas_response.drag_stopped_by(egui::PointerButton::Primary) {
            if let Some(drag) = self.curve_tangent_drag.take() {
                if (drag.current_slope - drag.original_slope).abs() > 1e-4 {
                    self.set_target_curve_tangent(
                        drag.target,
                        drag.track,
                        drag.time,
                        drag.incoming,
                        drag.current_slope,
                    );
                }
            }
            if let Some(drag) = self.curve_key_drag.take() {
                let delta = drag.current_time - drag.original_time;
                if delta.abs() > 1e-4 {
                    self.selected_keys = drag.original_selection;
                    self.move_selected_curve_keys_by(delta);
                }
            }
            if let (Some(marquee), Some(clip), Some(target)) =
                (self.curve_box_select.take(), clip.as_ref(), anim_target)
            {
                let min_x = marquee.start.x.min(marquee.current.x);
                let max_x = marquee.start.x.max(marquee.current.x);
                let min_y = marquee.start.y.min(marquee.current.y);
                let max_y = marquee.start.y.max(marquee.current.y);
                self.select_keys_in_box(
                    target,
                    clip,
                    (time_at(min_x), time_at(max_x)),
                    (value_at(max_y), value_at(min_y)),
                    marquee.additive,
                );
            }
        }
        if canvas_response.secondary_clicked() {
            if let (Some(pointer), Some(target)) =
                (canvas_response.interact_pointer_pos(), anim_target)
            {
                if let Some((track, time)) = key_at(pointer) {
                    match track {
                        CurveTrackId::Scalar(address) => {
                            self.delete_target_curve_key(target, address, time);
                        }
                        CurveTrackId::Rotation => {
                            self.delete_target_rotation_curve_key(target, time);
                        }
                        CurveTrackId::MorphWeight(index) => {
                            self.delete_target_morph_weight_curve_key(target, index, time);
                        }
                    }
                }
            }
        }
        for (color, values) in &series {
            let points: Vec<Pos2> = values
                .iter()
                .enumerate()
                .map(|(i, value)| {
                    Pos2::new(
                        rect.left() + rect.width() * i as f32 / 64.0,
                        value_y(*value),
                    )
                })
                .collect();
            painter.add(egui::Shape::line(points, egui::Stroke::new(1.3, *color)));
        }
        if let (Some(target), Some(clip)) = (anim_target, clip.as_ref()) {
            for track in &clip.tracks {
                for key in &track.curve.keys {
                    if key.time < start || key.time > end {
                        continue;
                    }
                    let key_id = SelectedKey {
                        target,
                        track: CurveTrackId::Scalar(track.address),
                        time: key.time,
                    };
                    let selected = self.is_curve_key_selected(&key_id);
                    let dragged = self.curve_key_drag.as_ref().and_then(|drag| {
                        drag.original_selection
                            .iter()
                            .find(|selected| {
                                selected.target == target
                                    && selected.track == CurveTrackId::Scalar(track.address)
                                    && (selected.time - key.time).abs() <= 1e-3
                            })
                            .map(|_| key.time + (drag.current_time - drag.original_time))
                    });
                    let time = dragged.unwrap_or(key.time);
                    let x = rect.left() + (time - start) / (end - start) * rect.width();
                    let y = value_y(key.value);
                    let radius = if selected { 4.2 } else { 3.0 };
                    let fill = if selected {
                        theme::ACCENT
                    } else {
                        track_color(track.address)
                    };
                    let stroke = if selected {
                        egui::Stroke::new(1.5, Color32::WHITE)
                    } else {
                        egui::Stroke::new(0.7, Color32::WHITE)
                    };
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            Pos2::new(x, y - radius),
                            Pos2::new(x + radius, y),
                            Pos2::new(x, y + radius),
                            Pos2::new(x - radius, y),
                        ],
                        fill,
                        stroke,
                    ));
                }
            }
        }
        if let (Some(target), Some(clip)) = (anim_target, clip.as_ref()) {
            for (target_index, curve) in &clip.morph_weights {
                for key in &curve.keys {
                    if key.time < start || key.time > end {
                        continue;
                    }
                    let key_id = SelectedKey {
                        target,
                        track: CurveTrackId::MorphWeight(*target_index),
                        time: key.time,
                    };
                    let selected = self.is_curve_key_selected(&key_id);
                    let dragged = self.curve_key_drag.as_ref().and_then(|drag| {
                        drag.original_selection
                            .iter()
                            .find(|selected| {
                                selected.target == target
                                    && selected.track == CurveTrackId::MorphWeight(*target_index)
                                    && (selected.time - key.time).abs() <= 1e-3
                            })
                            .map(|_| key.time + (drag.current_time - drag.original_time))
                    });
                    let time = dragged.unwrap_or(key.time);
                    let x = rect.left() + (time - start) / (end - start) * rect.width();
                    let y = value_y(key.value);
                    let radius = if selected { 4.2 } else { 3.0 };
                    let fill = if selected {
                        theme::ACCENT
                    } else {
                        channel_color(AnimChannel::MorphWeight)
                    };
                    let stroke = if selected {
                        egui::Stroke::new(1.5, Color32::WHITE)
                    } else {
                        egui::Stroke::new(0.7, Color32::WHITE)
                    };
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            Pos2::new(x, y - radius),
                            Pos2::new(x + radius, y),
                            Pos2::new(x, y + radius),
                            Pos2::new(x - radius, y),
                        ],
                        fill,
                        stroke,
                    ));
                }
            }
        }
        if let (Some(target), Some(rotation)) = (
            anim_target,
            clip.as_ref().and_then(|clip| clip.rotation.as_ref()),
        ) {
            for key in &rotation.keys {
                if key.time < start || key.time > end {
                    continue;
                }
                let key_id = SelectedKey {
                    target,
                    track: CurveTrackId::Rotation,
                    time: key.time,
                };
                let selected = self.is_curve_key_selected(&key_id);
                let dragged = self.curve_key_drag.as_ref().and_then(|drag| {
                    drag.original_selection
                        .iter()
                        .find(|selected| {
                            selected.target == target
                                && selected.track == CurveTrackId::Rotation
                                && (selected.time - key.time).abs() <= 1e-3
                        })
                        .map(|_| key.time + (drag.current_time - drag.original_time))
                });
                let time = dragged.unwrap_or(key.time);
                let x = rect.left() + (time - start) / (end - start) * rect.width();
                let y = value_y(key.value.w);
                let radius = if selected { 4.2 } else { 3.0 };
                let fill = if selected {
                    theme::ACCENT
                } else {
                    channel_color(AnimChannel::Rotation)
                };
                let stroke = if selected {
                    egui::Stroke::new(1.5, Color32::WHITE)
                } else {
                    egui::Stroke::new(0.7, Color32::WHITE)
                };
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        Pos2::new(x, y - radius),
                        Pos2::new(x + radius, y),
                        Pos2::new(x, y + radius),
                        Pos2::new(x - radius, y),
                    ],
                    fill,
                    stroke,
                ));
            }
        }
        for handle in &tangent_handles {
            let slope = self
                .curve_tangent_drag
                .filter(|drag| {
                    drag.target == handle.target
                        && drag.track == handle.track
                        && (drag.time - handle.time).abs() <= KEY_TIME_EPSILON
                        && drag.incoming == handle.incoming
                })
                .map_or(handle.slope, |drag| drag.current_slope);
            let key_pos = Pos2::new(
                rect.left() + (handle.time - start) / (end - start) * rect.width(),
                value_y(handle.key_value),
            );
            let endpoint_time = handle.time + handle.time_offset;
            let endpoint = Pos2::new(
                (rect.left() + (endpoint_time - start) / (end - start) * rect.width())
                    .clamp(rect.left() + 1.0, rect.right() - 1.0),
                (value_y(handle.key_value + slope * handle.time_offset))
                    .clamp(rect.top() + 1.0, rect.bottom() - 1.0),
            );
            painter.line_segment(
                [key_pos, endpoint],
                egui::Stroke::new(1.0, Color32::from_rgb(92, 102, 124)),
            );
            painter.circle_filled(endpoint, 3.2, theme::ACCENT);
            painter.circle_stroke(endpoint, 3.2, egui::Stroke::new(1.0, Color32::WHITE));
        }
        if let Some(marquee) = self.curve_box_select {
            let box_rect = Rect::from_two_pos(marquee.start, marquee.current).intersect(rect);
            painter.rect_filled(
                box_rect,
                0.0,
                Color32::from_rgba_unmultiplied(64, 116, 190, 32),
            );
            painter.rect_stroke(
                box_rect,
                0.0,
                egui::Stroke::new(1.0, theme::ACCENT),
                egui::StrokeKind::Inside,
            );
        }
        if series.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Select a channel row and key it to see curves",
                egui::FontId::proportional(10.0),
                theme::TEXT_WEAK,
            );
        }
        let x = rect.left()
            + ((self.playback.time - start) / (end - start)).clamp(0.0, 1.0) * rect.width();
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            egui::Stroke::new(1.2, theme::ACCENT),
        );
        if let (Some(target), Some(mut clip)) = (anim_target, clip) {
            if let Some(address) = remove_track {
                clip.remove_track(address);
            }
            if remove_rotation {
                clip.remove_rotation_track();
            }
            if let Some(index) = remove_morph {
                clip.remove_morph_weight_track(index);
            }
            if original.as_ref() != Some(&clip) && self.commit_target_clip(target, clip) {
                self.status = "Edited animation curves".into();
            }
        }
    }

    pub(super) fn ui_render_settings(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut start = false;
        let mut stop = false;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                theme::section(ui, "render_output", "Output", |ui| {
                    theme::row(ui, "Resolution", |ui| {
                        let settings = &mut self.doc.settings.render;
                        ui.add(
                            egui::DragValue::new(&mut settings.width)
                                .speed(1.0)
                                .range(32..=4096),
                        );
                        ui.label("×");
                        ui.add(
                            egui::DragValue::new(&mut settings.height)
                                .speed(1.0)
                                .range(32..=4096),
                        );
                    });
                    theme::row(ui, "Image", |ui| {
                        ui.checkbox(
                            &mut self.doc.settings.render.transparent_background,
                            "Transparent",
                        );
                    });
                    theme::row(ui, "Output File", |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.render_path)
                                .desired_width(f32::INFINITY),
                        );
                    });
                });
                theme::section(ui, "render_quality", "Quality", |ui| {
                    theme::row(ui, "Samples", |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.doc.settings.render.samples)
                                .speed(1.0)
                                .range(1..=4096),
                        );
                    });
                    theme::row(ui, "Bounces", |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.doc.settings.render.max_bounces)
                                .speed(0.2)
                                .range(1..=64),
                        );
                    });
                    theme::row(ui, "Exposure", |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.doc.settings.render.exposure)
                                .speed(0.05)
                                .range(-10.0..=10.0),
                        );
                    });
                    theme::row(ui, "Seed", |ui| {
                        ui.add(egui::DragValue::new(&mut self.doc.settings.render.seed).speed(1.0));
                    });
                });
                theme::section(ui, "render_camera", "Camera", |ui| {
                    let cameras: Vec<(NodeId, String)> = self
                        .doc
                        .scene
                        .depth_first()
                        .into_iter()
                        .filter_map(|id| {
                            let node = self.doc.scene.get(id)?;
                            matches!(node.kind, NodeKind::Camera { .. })
                                .then(|| (id, node.name.clone()))
                        })
                        .collect();
                    let current = self.doc.active_camera_id();
                    let current_name = current
                        .and_then(|id| self.doc.scene.get(id))
                        .map(|node| node.name.clone())
                        .unwrap_or_else(|| "Viewport Camera".into());
                    theme::row(ui, "Active", |ui| {
                        egui::ComboBox::from_id_salt("render_camera")
                            .selected_text(current_name)
                            .width(140.0)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(current.is_none(), "Viewport Camera")
                                    .clicked()
                                {
                                    self.doc.settings.active_camera = None;
                                }
                                for (id, name) in &cameras {
                                    if ui.selectable_label(current == Some(*id), name).clicked() {
                                        self.doc.settings.active_camera = Some(*id);
                                    }
                                }
                            });
                    });
                    ui.label(
                        egui::RichText::new("Environment")
                            .size(10.0)
                            .color(theme::TEXT_WEAK),
                    );
                    environment_editor(ui, &mut self.doc.settings.environment);
                });
                theme::section(ui, "render_go", "Render", |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Render Frame").clicked() {
                            start = true;
                        }
                        if ui.button("Render Manager").clicked() {
                            self.show_render_manager = true;
                        }
                    });
                    if let Some(active) = &self.active_render {
                        let progress = active.done as f32 / active.total.max(1) as f32;
                        ui.add(
                            egui::ProgressBar::new(progress)
                                .text(format!("{}/{} spp", active.done, active.total)),
                        );
                        if ui.button("Stop Render").clicked() {
                            stop = true;
                        }
                    } else {
                        ui.label(
                            egui::RichText::new("Ready to render on the CPU path tracer.")
                                .size(9.5)
                                .color(theme::TEXT_WEAK),
                        );
                    }
                });
            });
        if start {
            self.start_render(&ctx);
        }
        if stop {
            if let Some(active) = &self.active_render {
                if !active.cancel.load(Ordering::Relaxed) {
                    active.cancel.store(true, Ordering::Relaxed);
                    self.status = "Stopping render…".into();
                }
            }
        }
    }

    pub(super) fn ui_render_manager_window(&mut self, ctx: &egui::Context) {
        if !self.show_render_manager {
            return;
        }
        let mut open = self.show_render_manager;
        let mut save_png = false;
        let mut save_exr = false;
        let mut delete_job: Option<u32> = None;
        let mut stop = false;
        let stop_enabled = self
            .active_render
            .as_ref()
            .is_some_and(|active| !active.cancel.load(Ordering::Relaxed));
        egui::Window::new("Render Manager")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(egui::vec2(980.0, 620.0))
            .min_width(720.0)
            .min_height(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if icons::icon_label_button(
                        ui,
                        Icon::Save,
                        "Save PNG",
                        self.selected_finished_render().is_some(),
                    )
                    .clicked()
                    {
                        save_png = true;
                    }
                    if icons::icon_label_button(
                        ui,
                        Icon::Save,
                        "Save EXR",
                        self.selected_finished_render().is_some(),
                    )
                    .clicked()
                    {
                        save_exr = true;
                    }
                    ui.add_space(6.0);
                    if icons::icon_label_button(ui, Icon::Info, "Info", true).clicked() {
                        self.render_info_open = !self.render_info_open;
                    }
                    if icons::icon_label_button(
                        ui,
                        Icon::Trash,
                        "Delete",
                        self.selected_render_job.is_some(),
                    )
                    .clicked()
                    {
                        delete_job = self.selected_render_job;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::icon_label_button(ui, Icon::Stop, "Stop", stop_enabled).clicked()
                        {
                            stop = true;
                        }
                    });
                });
                theme::rule(ui);
                ui.add_space(4.0);
                let available = ui.available_size();
                let right_width = 284.0_f32.min(available.x * 0.36).max(230.0);
                ui.horizontal_top(|ui| {
                    let preview_width = (available.x - right_width - 16.0).max(240.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(preview_width, available.y),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            let (rect, _) = ui.allocate_exact_size(
                                Vec2::new(preview_width, (available.y - 8.0).max(180.0)),
                                Sense::hover(),
                            );
                            let painter = ui.painter_at(rect);
                            painter.rect_filled(rect, 2.0, Color32::from_rgb(220, 220, 220));
                            painter.rect_stroke(
                                rect,
                                2.0,
                                egui::Stroke::new(1.0, theme::BORDER),
                                egui::StrokeKind::Inside,
                            );
                            let job = self
                                .selected_render_job
                                .and_then(|id| self.render_jobs.iter().find(|job| job.id == id));
                            if let Some(job) = job {
                                if let Some(texture) = job.texture.as_ref() {
                                    let pad = 10.0;
                                    let max_w = rect.width() - pad * 2.0;
                                    let max_h = rect.height() - pad * 2.0;
                                    let aspect = job.width.max(1) as f32 / job.height.max(1) as f32;
                                    let (w, h) = if max_w / max_h > aspect {
                                        (max_h * aspect, max_h)
                                    } else {
                                        (max_w, max_w / aspect)
                                    };
                                    let image_rect =
                                        Rect::from_center_size(rect.center(), Vec2::new(w, h));
                                    painter.image(
                                        texture.id(),
                                        image_rect,
                                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                        Color32::WHITE,
                                    );
                                } else {
                                    painter.text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "Render cancelled",
                                        egui::FontId::proportional(11.0),
                                        theme::TEXT_WEAK,
                                    );
                                }
                            } else if let Some(active) = &self.active_render {
                                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(rect.height() * 0.32);
                                        ui.spinner();
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "Rendering {}×{} · {} spp",
                                                active.width, active.height, active.total
                                            ))
                                            .size(11.0)
                                            .color(theme::TEXT_WEAK),
                                        );
                                    });
                                });
                            } else {
                                painter.text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "No render selected",
                                    egui::FontId::proportional(11.0),
                                    theme::TEXT_WEAK,
                                );
                            }
                        },
                    );
                    ui.vertical(|ui| {
                        ui.set_width(right_width);
                        let list_height = if self.render_info_open {
                            (available.y - 188.0).max(130.0)
                        } else {
                            available.y - 8.0
                        };
                        egui::ScrollArea::vertical()
                            .id_salt("render_jobs")
                            .max_height(list_height)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if let Some(active) = &self.active_render {
                                    let (rect, _) = ui.allocate_exact_size(
                                        Vec2::new(ui.available_width(), 54.0),
                                        Sense::hover(),
                                    );
                                    let painter = ui.painter_at(rect);
                                    painter.rect_filled(
                                        rect,
                                        2.0,
                                        Color32::from_rgb(227, 234, 244),
                                    );
                                    painter.text(
                                        rect.left_top() + Vec2::new(6.0, 5.0),
                                        egui::Align2::LEFT_TOP,
                                        format!(
                                            "Job {} — {}",
                                            active.id,
                                            if active.cancel.load(Ordering::Relaxed) {
                                                "stopping"
                                            } else {
                                                "rendering"
                                            }
                                        ),
                                        egui::FontId::proportional(10.5),
                                        theme::TEXT,
                                    );
                                    let progress = active.done as f32 / active.total.max(1) as f32;
                                    painter.rect_filled(
                                        Rect::from_min_size(
                                            rect.left_bottom() + Vec2::new(6.0, -14.0),
                                            Vec2::new((rect.width() - 12.0) * progress, 6.0),
                                        ),
                                        2.0,
                                        theme::SELECTION,
                                    );
                                    painter.text(
                                        rect.right_bottom() + Vec2::new(-6.0, -3.0),
                                        egui::Align2::RIGHT_BOTTOM,
                                        format!(
                                            "{}×{} · {}/{} spp",
                                            active.width, active.height, active.done, active.total
                                        ),
                                        egui::FontId::proportional(9.0),
                                        theme::TEXT_WEAK,
                                    );
                                }
                                for job in self.render_jobs.iter().rev() {
                                    let selected = self.selected_render_job == Some(job.id);
                                    let cancelled = !job.is_finished();
                                    let (rect, response) = ui.allocate_exact_size(
                                        Vec2::new(ui.available_width(), 64.0),
                                        Sense::click(),
                                    );
                                    let painter = ui.painter_at(rect);
                                    if selected {
                                        painter.rect_filled(rect, 2.0, theme::SELECTION);
                                    } else if response.hovered() {
                                        painter.rect_filled(
                                            rect,
                                            2.0,
                                            Color32::from_rgb(244, 246, 249),
                                        );
                                    }
                                    let thumb = Rect::from_min_size(
                                        rect.min + Vec2::new(5.0, 6.0),
                                        Vec2::new(66.0, 48.0),
                                    );
                                    painter.rect_filled(
                                        thumb,
                                        1.0,
                                        Color32::from_rgb(210, 210, 210),
                                    );
                                    let aspect = job.width.max(1) as f32 / job.height.max(1) as f32;
                                    let (tw, th) = if thumb.width() / thumb.height() > aspect {
                                        (thumb.height() * aspect, thumb.height())
                                    } else {
                                        (thumb.width(), thumb.width() / aspect)
                                    };
                                    if let Some(texture) = job.texture.as_ref() {
                                        painter.image(
                                            texture.id(),
                                            Rect::from_center_size(
                                                thumb.center(),
                                                Vec2::new(tw, th),
                                            ),
                                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                            Color32::WHITE,
                                        );
                                    } else {
                                        painter.text(
                                            thumb.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "CANCELLED",
                                            egui::FontId::proportional(7.5),
                                            Color32::from_rgb(172, 72, 60),
                                        );
                                    }
                                    let color = if selected {
                                        Color32::WHITE
                                    } else if cancelled {
                                        Color32::from_rgb(172, 72, 60)
                                    } else {
                                        theme::TEXT
                                    };
                                    painter.text(
                                        rect.left_top() + Vec2::new(78.0, 4.0),
                                        egui::Align2::LEFT_TOP,
                                        format!(
                                            "Job: {}{}",
                                            job.id,
                                            if cancelled { " — Cancelled" } else { "" }
                                        ),
                                        egui::FontId::proportional(10.5),
                                        color,
                                    );
                                    painter.text(
                                        rect.left_top() + Vec2::new(78.0, 18.0),
                                        egui::Align2::LEFT_TOP,
                                        format!("Time: {:.2} sec", job.seconds),
                                        egui::FontId::proportional(9.0),
                                        color,
                                    );
                                    painter.text(
                                        rect.left_top() + Vec2::new(78.0, 31.0),
                                        egui::Align2::LEFT_TOP,
                                        format!("Resolution: {}×{}", job.width, job.height),
                                        egui::FontId::proportional(9.0),
                                        color,
                                    );
                                    painter.text(
                                        rect.left_top() + Vec2::new(78.0, 44.0),
                                        egui::Align2::LEFT_TOP,
                                        format!("File: {}", job.name),
                                        egui::FontId::proportional(9.0),
                                        color,
                                    );
                                    if response.clicked() {
                                        self.selected_render_job = Some(job.id);
                                    }
                                }
                                if self.render_jobs.is_empty() && self.active_render.is_none() {
                                    ui.add_space(14.0);
                                    ui.label(
                                        egui::RichText::new("No render jobs yet")
                                            .size(10.0)
                                            .color(theme::TEXT_WEAK),
                                    );
                                }
                            });
                        if self.render_info_open {
                            theme::rule(ui);
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new("Info")
                                    .size(10.0)
                                    .strong()
                                    .color(theme::TEXT),
                            );
                            let job = self
                                .selected_render_job
                                .and_then(|id| self.render_jobs.iter().find(|job| job.id == id));
                            egui::Grid::new("render_info_grid")
                                .num_columns(2)
                                .spacing(Vec2::new(7.0, 2.0))
                                .show(ui, |ui| {
                                    let rows = [
                                        (
                                            "Render engine".to_string(),
                                            "Leopard Path Tracer".to_string(),
                                        ),
                                        ("Type".into(), "Image".into()),
                                        (
                                            "Status".into(),
                                            job.map(|job| {
                                                if job.is_finished() {
                                                    "Finished".to_string()
                                                } else {
                                                    "Cancelled".to_string()
                                                }
                                            })
                                            .unwrap_or_else(|| "—".into()),
                                        ),
                                        (
                                            "Time".into(),
                                            job.map(|job| format!("{:.2} sec", job.seconds))
                                                .unwrap_or_else(|| "—".into()),
                                        ),
                                        (
                                            "Quality".into(),
                                            job.map(|job| {
                                                if job.is_finished() {
                                                    format!("{} spp", job.samples)
                                                } else {
                                                    format!(
                                                        "{}/{} spp (cancelled)",
                                                        job.samples_done, job.samples
                                                    )
                                                }
                                            })
                                            .unwrap_or_else(|| "—".into()),
                                        ),
                                        (
                                            "Resolution".into(),
                                            job.map(|job| format!("{}×{}", job.width, job.height))
                                                .unwrap_or_else(|| "—".into()),
                                        ),
                                        ("Frames".into(), "1".into()),
                                        (
                                            "Date".into(),
                                            job.map(|job| job.date.clone())
                                                .unwrap_or_else(|| "—".into()),
                                        ),
                                        (
                                            "File".into(),
                                            job.map(|job| job.file.clone())
                                                .unwrap_or_else(|| "—".into()),
                                        ),
                                    ];
                                    for (label, value) in rows {
                                        ui.label(
                                            egui::RichText::new(label)
                                                .size(9.0)
                                                .color(theme::TEXT_WEAK),
                                        );
                                        ui.label(
                                            egui::RichText::new(value).size(9.0).color(theme::TEXT),
                                        );
                                        ui.end_row();
                                    }
                                });
                        }
                    });
                });
            });
        self.show_render_manager = open;
        if save_png {
            self.save_render(false);
        }
        if save_exr {
            self.save_render(true);
        }
        if stop {
            if let Some(active) = &self.active_render {
                active.cancel.store(true, Ordering::Relaxed);
                self.status = "Stopping render…".into();
            }
        }
        if let Some(id) = delete_job {
            self.render_jobs.retain(|job| job.id != id);
            self.selected_render_job = self.render_jobs.last().map(|job| job.id);
            self.status = "Deleted render job".into();
        }
    }
}

fn channel_color(channel: AnimChannel) -> Color32 {
    match channel {
        AnimChannel::Translation => Color32::from_rgb(206, 74, 66),
        AnimChannel::Rotation => Color32::from_rgb(196, 122, 72),
        AnimChannel::Scale => Color32::from_rgb(188, 104, 148),
        AnimChannel::Fov => Color32::from_rgb(96, 132, 186),
        AnimChannel::LightIntensity => Color32::from_rgb(212, 168, 70),
        AnimChannel::LightColor => Color32::from_rgb(210, 150, 92),
        AnimChannel::CopyLocationWeight => Color32::from_rgb(110, 156, 140),
        AnimChannel::MorphWeight => Color32::from_rgb(120, 100, 170),
        AnimChannel::MaterialRoughness => Color32::from_rgb(128, 128, 128),
        AnimChannel::MaterialMetallic => Color32::from_rgb(150, 150, 170),
    }
}

fn track_color(address: ChannelAddress) -> Color32 {
    // Components of one vector channel get distinct tints so its three curves
    // stay readable; every other channel uses its registry colour.
    match (address.channel, address.component) {
        (AnimChannel::Translation, 0) => Color32::from_rgb(206, 74, 66),
        (AnimChannel::Translation, 1) => Color32::from_rgb(86, 158, 82),
        (AnimChannel::Translation, 2) => Color32::from_rgb(70, 108, 190),
        (AnimChannel::Scale, 0) => Color32::from_rgb(188, 104, 148),
        (AnimChannel::Scale, 1) => Color32::from_rgb(92, 152, 152),
        (AnimChannel::Scale, 2) => Color32::from_rgb(102, 122, 178),
        (AnimChannel::LightColor, 0) => Color32::from_rgb(196, 122, 72),
        (AnimChannel::LightColor, 1) => Color32::from_rgb(132, 150, 62),
        (AnimChannel::LightColor, 2) => Color32::from_rgb(122, 92, 168),
        _ => channel_color(address.channel),
    }
}

fn environment_editor(ui: &mut egui::Ui, environment: &mut Environment) {
    let mut kind = match environment {
        Environment::Sky { .. } => 0,
        Environment::Hdri { .. } => 1,
        Environment::Solid { .. } => 2,
    };
    egui::ComboBox::from_id_salt("environment_kind")
        .selected_text(match kind {
            0 => "Procedural Sky",
            1 => "HDRI",
            _ => "Solid Color",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut kind, 0, "Procedural Sky");
            ui.selectable_value(&mut kind, 1, "HDRI");
            ui.selectable_value(&mut kind, 2, "Solid Color");
        });
    if (kind == 0 && !matches!(environment, Environment::Sky { .. }))
        || (kind == 1 && !matches!(environment, Environment::Hdri { .. }))
        || (kind == 2 && !matches!(environment, Environment::Solid { .. }))
    {
        *environment = match kind {
            1 => Environment::Hdri {
                path: String::new(),
                intensity: 1.0,
                rotation_deg: 0.0,
            },
            2 => Environment::Solid {
                color: Vec3::splat(0.16),
            },
            _ => Environment::default(),
        };
    }
    match environment {
        Environment::Sky {
            top,
            horizon,
            ground,
            sun_direction,
            sun_color,
            sun_intensity,
        } => {
            for (label, color) in [("Zenith", top), ("Horizon", horizon), ("Ground", ground)] {
                let mut rgb = color.to_array();
                if ui.color_edit_button_rgb(&mut rgb).changed() {
                    *color = Vec3::from_array(rgb).max(Vec3::ZERO);
                }
                ui.label(label);
            }
            let mut rgb = sun_color.to_array();
            if ui.color_edit_button_rgb(&mut rgb).changed() {
                *sun_color = Vec3::from_array(rgb).max(Vec3::ZERO);
            }
            ui.add(egui::Slider::new(sun_intensity, 0.0..=20.0).text("Sun"));
            let mut dir = sun_direction.to_array();
            ui.label("Sun direction");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut dir[0]).speed(0.01));
                ui.add(egui::DragValue::new(&mut dir[1]).speed(0.01));
                ui.add(egui::DragValue::new(&mut dir[2]).speed(0.01));
            });
            *sun_direction = Vec3::from_array(dir).normalize_or(Vec3::new(-0.4, -0.8, -0.3));
        }
        Environment::Hdri {
            path,
            intensity,
            rotation_deg,
        } => {
            ui.horizontal(|ui| {
                ui.label("Path");
                ui.text_edit_singleline(path);
            });
            ui.add(egui::Slider::new(intensity, 0.0..=10.0).text("Intensity"));
            ui.add(egui::Slider::new(rotation_deg, -180.0..=180.0).text("Rotation°"));
        }
        Environment::Solid { color } => {
            let mut rgb = color.to_array();
            if ui.color_edit_button_rgb(&mut rgb).changed() {
                *color = Vec3::from_array(rgb).max(Vec3::ZERO);
            }
        }
    }
}
