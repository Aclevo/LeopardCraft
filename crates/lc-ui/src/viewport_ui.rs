//! Viewport frame, navigation, picking, gizmo and overlay workflows.

use super::*;

impl LeopardApp {
    /// Keyboard commands shared by the real app frame and input-driven tests.
    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
            self.focus_search = true;
            self.browser_tab = BrowserTab::Objects;
        }
        // Check the more specific Shift+Cmd/Ctrl+Z first: egui ignores extra
        // Shift when matching, so otherwise a redo would be consumed as undo.
        let redo = ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers {
                    shift: true,
                    ..egui::Modifiers::COMMAND
                },
                egui::Key::Z,
            )
        });
        let undo = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        if self.scalpel_tool
            && self.scalpel_start.is_some()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::default(), egui::Key::Escape))
        {
            self.scalpel_cancel();
        }
        let typing = ctx.text_edit_focused();
        let native_copy = ctx.input(|i| i.events.contains(&egui::Event::Copy));
        let native_paste = ctx.input(|i| {
            i.events
                .iter()
                .any(|event| matches!(event, egui::Event::Paste(_)))
        });
        // egui-winit suppresses Ctrl/Cmd+V when the system clipboard has no
        // text, but still forwards the V key release. Fall back to the editor's
        // internal curve clipboard on that release.
        let paste_release = ctx.input(|i| {
            // Some backends report the modifier after releasing Ctrl, so a V
            // release in the same frame as a Ctrl release is still the chord.
            let ctrl_released = i.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::ControlLeft | egui::Key::ControlRight,
                        pressed: false,
                        ..
                    }
                )
            });
            i.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::V,
                        pressed: false,
                        modifiers,
                        ..
                    } if modifiers.command || ctrl_released
                )
            })
        });
        if !typing && self.show_curves {
            // Native backends translate Ctrl/Cmd+C and Ctrl/Cmd+V into clipboard
            // events instead of key presses, so handle both forms.
            if native_copy
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::C))
            {
                self.copy_selected_curve_keys();
            }
            if native_paste {
                self.paste_curve_keys();
                self.native_paste_pending = !paste_release;
            } else if paste_release {
                if self.native_paste_pending {
                    self.native_paste_pending = false;
                } else {
                    self.paste_curve_keys();
                }
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::V)) {
                self.paste_curve_keys();
                // Platforms that still deliver a raw V press will also deliver
                // the release; swallow it so the chord pastes only once.
                self.native_paste_pending = true;
            }
        } else if !self.show_curves {
            self.native_paste_pending = false;
        }
        if !typing && ctx.input(|i| i.key_pressed(egui::Key::Delete)) && !self.selection.is_empty()
        {
            self.delete_selected();
        }
        if !typing && ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.frame_selected();
        }
        if !typing
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::D))
            && !self.selection.is_empty()
        {
            self.duplicate_selected();
        }
    }

    pub(super) fn ui_viewport(&mut self, ui: &mut egui::Ui) {
        let available = ui.available_size();
        if available.x < 32.0 || available.y < 32.0 {
            return;
        }
        let (rect, response) = ui.allocate_exact_size(available, Sense::click_and_drag());
        self.last_viewport_rect = rect;
        self.last_viewport_size = available;
        let ppp = ui.ctx().pixels_per_point();
        let pixel_w = (available.x * ppp).round().max(16.0) as u32;
        let pixel_h = (available.y * ppp).round().max(16.0) as u32;
        let evaluated = self.evaluate();
        let frame = self.build_frame(&evaluated);
        let texture = self.viewport.render(&frame, pixel_w, pixel_h);
        let painter = ui.painter_at(rect);
        if let Some(texture) = texture {
            painter.image(
                texture,
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            painter.rect_filled(rect, 0.0, Color32::from_rgb(120, 131, 141));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "wgpu viewport unavailable on this backend",
                egui::FontId::proportional(16.0),
                Color32::LIGHT_GRAY,
            );
        }

        let mut mode_hovered = false;
        let mesh_ready = self.selected_is_mesh();
        let weight_ready = self.weight_paint_ready();
        if self.weight_paint && !weight_ready {
            self.weight_paint = false;
        }
        let strip = Rect::from_min_size(
            rect.left_top() + Vec2::new(7.0, 8.0),
            Vec2::new(30.0, 5.0 * 27.0 + 7.0),
        );
        painter.rect_filled(
            strip,
            4.0,
            Color32::from_rgba_unmultiplied(240, 240, 240, 218),
        );
        painter.rect_stroke(
            strip,
            4.0,
            egui::Stroke::new(1.0, Color32::from_rgb(170, 170, 170)),
            egui::StrokeKind::Inside,
        );
        for (index, (icon, label)) in [
            (Icon::ObjectMode, "Object mode"),
            (Icon::PointMode, "Point mode"),
            (Icon::EdgeMode, "Edge mode"),
            (Icon::PolygonMode, "Polygon mode"),
            (Icon::WeightPaint, "Weight paint"),
        ]
        .into_iter()
        .enumerate()
        {
            let active = match index {
                0 => !self.mesh_edit && !self.weight_paint,
                4 => self.weight_paint,
                _ => {
                    self.mesh_edit
                        && matches!(
                            (index, self.mesh_select_mode),
                            (1, MeshSelectMode::Points)
                                | (2, MeshSelectMode::Edges)
                                | (3, MeshSelectMode::Polygons)
                        )
                }
            };
            let hit = Rect::from_min_size(
                strip.left_top() + Vec2::new(3.0, 4.0 + index as f32 * 27.0),
                Vec2::splat(24.0),
            );
            let interactive = match index {
                0 => true,
                4 => weight_ready,
                _ => mesh_ready,
            };
            let item = ui.interact(
                hit,
                ui.id().with(("viewport_mode", index)),
                if interactive {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );
            if active {
                painter.rect_filled(hit, 3.0, theme::PRESS);
                painter.rect_stroke(
                    hit,
                    3.0,
                    egui::Stroke::new(1.0, theme::SELECTION),
                    egui::StrokeKind::Inside,
                );
            } else if item.hovered() {
                painter.rect_filled(hit, 3.0, theme::HOVER);
            }
            icons::paint(
                &painter,
                hit.shrink(3.5),
                icon,
                if !interactive {
                    theme::DISABLED
                } else if active {
                    theme::SELECTION
                } else {
                    theme::TEXT
                },
            );
            if item.clicked() {
                if index == 0 {
                    self.set_weight_paint(false);
                    self.leave_mesh_edit();
                } else if index == 4 {
                    let enable = !self.weight_paint;
                    self.set_weight_paint(enable);
                } else {
                    self.set_weight_paint(false);
                    let mode = match index {
                        1 => MeshSelectMode::Points,
                        2 => MeshSelectMode::Edges,
                        _ => MeshSelectMode::Polygons,
                    };
                    self.set_mesh_mode(mode);
                }
            }
            mode_hovered |= item.hovered();
            if interactive {
                item.on_hover_text(label);
            } else {
                item.on_hover_text(if index == 4 {
                    "Bind a modifier-free mesh to joints to paint weights"
                } else {
                    "Select a mesh object to edit components"
                });
            }
        }
        if !mode_hovered {
            self.handle_viewport_input(ui, &response, &evaluated, rect, pixel_w, pixel_h);
        }
        self.draw_viewport_overlay(ui, rect, &evaluated, pixel_w, pixel_h);
    }

    pub(super) fn build_frame(&self, evaluated: &EvaluatedScene) -> FrameData {
        #[cfg(test)]
        {
            let visible: Vec<NodeId> = evaluated.mesh_nodes().map(|node| node.id).collect();
            let selected = visible
                .iter()
                .copied()
                .filter(|id| self.selection.contains(*id))
                .collect();
            ui_test_support::record_viewport(visible, selected);
        }
        let aspect = (self.last_viewport_size.x / self.last_viewport_size.y.max(1.0)).max(0.01);
        let world = self.orbit.world();
        let camera = self.orbit.camera();
        let view_proj = camera.view_projection(world, aspect);
        let mut triangles = Vec::new();
        let mut lines = Vec::new();
        let light_dir = Vec3::new(0.42, 0.82, 0.38).normalize();
        let mut material_colors: HashMap<MaterialId, Vec3> = HashMap::new();

        if matches!(
            self.shading,
            ShadingMode::Shaded | ShadingMode::ShadedWireframe
        ) {
            for node in evaluated.mesh_nodes() {
                let Some(mesh) = node.mesh() else {
                    continue;
                };
                let normal_matrix = glam::Mat3::from_mat4(node.world).inverse().transpose();
                for tri in mesh.render_triangles(lc_geom::CREASE_ANGLE_DEG) {
                    let mat_id = node.material_for_slot(tri.material_slot).unwrap_or(0);
                    let base = *material_colors.entry(mat_id).or_insert_with(|| {
                        self.doc
                            .materials
                            .get(mat_id)
                            .map(|m| m.preview_color(&self.texture_cache))
                            .unwrap_or(Vec3::splat(0.65))
                    });
                    let color = base;
                    for v in &tri.v {
                        let p = node.world.transform_point3(v.position);
                        let n = (normal_matrix * v.normal).normalize_or(Vec3::Y);
                        triangles.push(Vertex::new(p, n, color));
                    }
                }
            }
        }

        if self.show_grid {
            let extent = 20i32;
            for i in -extent..=extent {
                let f = i as f32;
                let major = i % 5 == 0;
                let color = if i == 0 {
                    Vec3::new(0.25, 0.27, 0.30)
                } else if major {
                    Vec3::new(0.16, 0.18, 0.21)
                } else {
                    Vec3::new(0.09, 0.10, 0.12)
                };
                lines.push(Vertex::new(
                    Vec3::new(-extent as f32, 0.0, f),
                    Vec3::Y,
                    color,
                ));
                lines.push(Vertex::new(
                    Vec3::new(extent as f32, 0.0, f),
                    Vec3::Y,
                    color,
                ));
                lines.push(Vertex::new(
                    Vec3::new(f, 0.0, -extent as f32),
                    Vec3::Y,
                    color,
                ));
                lines.push(Vertex::new(
                    Vec3::new(f, 0.0, extent as f32),
                    Vec3::Y,
                    color,
                ));
            }
            // Coloured origin axes.
            lines.push(Vertex::new(
                Vec3::ZERO,
                Vec3::Y,
                Vec3::new(0.75, 0.22, 0.22),
            ));
            lines.push(Vertex::new(
                Vec3::X * 1.5,
                Vec3::Y,
                Vec3::new(0.75, 0.22, 0.22),
            ));
            lines.push(Vertex::new(
                Vec3::ZERO,
                Vec3::Y,
                Vec3::new(0.22, 0.65, 0.32),
            ));
            lines.push(Vertex::new(
                Vec3::Y * 1.5,
                Vec3::Y,
                Vec3::new(0.22, 0.65, 0.32),
            ));
            lines.push(Vertex::new(
                Vec3::ZERO,
                Vec3::Y,
                Vec3::new(0.27, 0.42, 0.85),
            ));
            lines.push(Vertex::new(
                Vec3::Z * 1.5,
                Vec3::Y,
                Vec3::new(0.27, 0.42, 0.85),
            ));
        }

        for node in &evaluated.nodes {
            if !node.rendered {
                continue;
            }
            let Some(spline) = node.spline() else {
                continue;
            };
            let selected = self.selection.contains(node.id);
            let color = if selected {
                Vec3::splat(0.97)
            } else {
                Vec3::new(0.80, 0.62, 0.20)
            };
            let sampled = spline.sample();
            for pair in sampled.windows(2) {
                lines.push(Vertex::new(
                    node.world.transform_point3(pair[0]),
                    Vec3::Y,
                    color,
                ));
                lines.push(Vertex::new(
                    node.world.transform_point3(pair[1]),
                    Vec3::Y,
                    color,
                ));
            }
            if spline.closed && sampled.len() > 2 {
                lines.push(Vertex::new(
                    node.world.transform_point3(*sampled.last().unwrap()),
                    Vec3::Y,
                    color,
                ));
                lines.push(Vertex::new(
                    node.world.transform_point3(sampled[0]),
                    Vec3::Y,
                    color,
                ));
            }
            for point in &spline.points {
                let p = node.world.transform_point3(point.position);
                let size = 0.07;
                for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                    lines.push(Vertex::new(p - axis * size, Vec3::Y, color));
                    lines.push(Vertex::new(p + axis * size, Vec3::Y, color));
                }
            }
        }

        let wireframe = matches!(
            self.shading,
            ShadingMode::Wireframe | ShadingMode::ShadedWireframe
        );
        let selected_overlay = !wireframe && !self.selection.is_empty();
        if wireframe || selected_overlay {
            for node in evaluated.mesh_nodes() {
                let Some(mesh) = node.mesh() else {
                    continue;
                };

                let selected = self.selection.contains(node.id);
                if selected_overlay && !selected {
                    continue;
                }
                let color = if selected {
                    Vec3::splat(0.98)
                } else {
                    Vec3::new(0.07, 0.08, 0.09)
                };
                for edge in mesh.edges() {
                    let a = mesh.positions.get(edge.a as usize).copied();
                    let b = mesh.positions.get(edge.b as usize).copied();
                    if let (Some(a), Some(b)) = (a, b) {
                        lines.push(Vertex::new(node.world.transform_point3(a), Vec3::Y, color));
                        lines.push(Vertex::new(node.world.transform_point3(b), Vec3::Y, color));
                    }
                }
            }
        }

        // Creased edges stay visible outside Edge mode, so the subdivisions a
        // user will get are readable while modelling in point or polygon mode.
        if !(self.mesh_edit && self.mesh_select_mode == MeshSelectMode::Edges) {
            for node in evaluated.mesh_nodes() {
                let Some(mesh) = node.mesh() else {
                    continue;
                };
                let smooth = Vec3::new(0.42, 0.52, 0.64);
                let sharp = Vec3::new(0.30, 0.68, 1.0);
                for [a, b] in &mesh.creases {
                    let (Some(a), Some(b)) = (
                        mesh.positions.get(*a as usize),
                        mesh.positions.get(*b as usize),
                    ) else {
                        continue;
                    };
                    lines.push(Vertex::new(node.world.transform_point3(*a), Vec3::Y, sharp));
                    lines.push(Vertex::new(node.world.transform_point3(*b), Vec3::Y, sharp));
                }
                for ([a, b], weight) in mesh.crease_weights.iter() {
                    let (Some(a), Some(b)) = (
                        mesh.positions.get(*a as usize),
                        mesh.positions.get(*b as usize),
                    ) else {
                        continue;
                    };
                    let color = smooth.lerp(sharp, weight.clamp(0.0, 1.0));
                    lines.push(Vertex::new(node.world.transform_point3(*a), Vec3::Y, color));
                    lines.push(Vertex::new(node.world.transform_point3(*b), Vec3::Y, color));
                }
            }
        }

        if self.show_helpers {
            for light in &evaluated.lights {
                let pos = light.world.w_axis.truncate();
                let color = light.light.color.normalize_or(Vec3::ONE);
                let size = 0.22;
                for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                    lines.push(Vertex::new(pos - axis * size, Vec3::Y, color));
                    lines.push(Vertex::new(pos + axis * size, Vec3::Y, color));
                }
            }
            for node in &evaluated.nodes {
                if let NodeKind::Camera { .. } = &self
                    .doc
                    .scene
                    .get(node.id)
                    .map(|n| n.kind.clone())
                    .unwrap_or(NodeKind::Null)
                {
                    let eye = node.world.w_axis.truncate();
                    let target = eye + lc_scene::Camera::forward(node.world) * 1.1;
                    let color = Vec3::new(0.95, 0.8, 0.28);
                    lines.push(Vertex::new(eye, Vec3::Y, color));
                    lines.push(Vertex::new(target, Vec3::Y, color));
                    let (right, up) =
                        lc_core::orthonormal_basis(lc_scene::Camera::forward(node.world));
                    let a = target + right * 0.35 + up * 0.22;
                    let b = target - right * 0.35 + up * 0.22;
                    let c = target - right * 0.35 - up * 0.22;
                    let d = target + right * 0.35 - up * 0.22;
                    for (p, q) in [
                        (a, b),
                        (b, c),
                        (c, d),
                        (d, a),
                        (eye, a),
                        (eye, b),
                        (eye, c),
                        (eye, d),
                    ] {
                        lines.push(Vertex::new(p, Vec3::Y, color));
                        lines.push(Vertex::new(q, Vec3::Y, color));
                    }
                }
            }
        }

        FrameData {
            triangles,
            lines,
            view_proj,
            eye: self.orbit.eye(),
            light_dir,
        }
    }

    /// Screen-space radius of a joint picking handle, in points.
    const JOINT_HANDLE_RADIUS: f32 = 9.0;

    /// Nearest joint of the primary mesh's skin whose viewport handle is under
    /// `pointer`. The drawn handle list and the pick list are the same, so an
    /// unbound joint can never become the active joint from a click.
    pub(super) fn pick_joint_handle(
        &self,
        evaluated: &EvaluatedScene,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
        pointer: Pos2,
    ) -> Option<NodeId> {
        let skin = self
            .doc
            .scene
            .get(self.selection.primary()?)?
            .skin
            .as_ref()?;
        let mut best: Option<(f32, NodeId)> = None;
        for joint in &skin.joints {
            let Some(node) = evaluated.node(*joint) else {
                continue;
            };
            let Some(screen) = self.project(node.world.w_axis.truncate(), rect, pixel_w, pixel_h)
            else {
                continue;
            };
            let distance = (screen - pointer).length();
            if distance <= Self::JOINT_HANDLE_RADIUS
                && best.is_none_or(|(nearest, _)| distance < nearest)
            {
                best = Some((distance, *joint));
            }
        }
        best.map(|(_, joint)| joint)
    }

    pub(super) fn handle_viewport_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        evaluated: &EvaluatedScene,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
    ) {
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.orbit.zoom(scroll);
                self.view_label = "Viewport".into();
            }
        }
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let modifiers = ui.input(|i| i.modifiers);
        if modifiers.alt {
            let dragged = [
                egui::PointerButton::Primary,
                egui::PointerButton::Middle,
                egui::PointerButton::Secondary,
            ]
            .into_iter()
            .find(|button| response.dragged_by(*button));
            if let Some(action) = dragged.and_then(|button| alt_navigation(modifiers, button)) {
                let delta = ui.input(|i| i.pointer.delta());
                match action {
                    ViewportNav::Orbit => self.orbit.orbit(delta),
                    ViewportNav::Pan => self.orbit.pan(delta),
                }
                self.view_label = "Viewport".into();
            }
        } else if response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
        {
            let delta = ui.input(|i| i.pointer.delta());
            if modifiers.shift || response.dragged_by(egui::PointerButton::Secondary) {
                self.orbit.pan(delta);
            } else {
                self.orbit.orbit(delta);
            }
            self.view_label = "Viewport".into();
        }

        if !modifiers.alt && self.weight_paint {
            let painting = response.dragged_by(egui::PointerButton::Primary)
                || response.clicked_by(egui::PointerButton::Primary);
            if response.drag_started_by(egui::PointerButton::Primary)
                || response.clicked_by(egui::PointerButton::Primary)
            {
                self.begin_weight_stroke();
            }
            if painting {
                if let Some(pointer) = pointer {
                    self.paint_weight_brush(evaluated, rect, pixel_w, pixel_h, pointer);
                }
            }
            if response.drag_stopped_by(egui::PointerButton::Primary)
                || response.clicked_by(egui::PointerButton::Primary)
            {
                self.commit_pending();
            }
            return;
        }

        let drag_started = !modifiers.alt && response.drag_started_by(egui::PointerButton::Primary);
        if drag_started {
            if let Some(pointer) = pointer {
                self.gizmo_drag = self.gizmo_hit(evaluated, rect, pixel_w, pixel_h, pointer);
                if self.gizmo_drag.is_none() && !self.mesh_edit {
                    let start = ui.input(|i| i.pointer.press_origin()).unwrap_or(pointer);
                    self.box_select = Some(BoxSelect {
                        start,
                        current: pointer,
                        base: self.selection.ids().to_vec(),
                        additive: modifiers.shift || modifiers.command,
                    });
                }
            }
        }
        if let Some(mut drag) = self.gizmo_drag.take() {
            if response.dragged_by(egui::PointerButton::Primary) {
                if let Some(pointer) = pointer {
                    let delta = pointer - drag.start_pointer;
                    if drag.screen_len > 1.0 {
                        let amount = delta.dot(drag.screen_dir) / drag.screen_len;
                        self.apply_gizmo_drag(&mut drag, amount);
                    }
                }
            }
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                self.commit_pending();
            } else {
                self.gizmo_drag = Some(drag);
            }
            return;
        }

        if let Some(mut marquee) = self.box_select.take() {
            if let Some(pointer) = pointer {
                marquee.current = pointer;
            }
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                self.finish_box_select(&marquee, evaluated, rect, pixel_w, pixel_h);
            } else {
                self.box_select = Some(marquee);
            }
            return;
        }

        if response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pointer) = pointer {
                if self.mesh_edit {
                    if self.scalpel_tool {
                        if let Some((id, mesh)) = self.selected_editable_mesh() {
                            if let Some(node) = evaluated.node(id) {
                                if let Some(pick) = self.pick_scalpel_pick(
                                    &mesh, node.world, rect, pixel_w, pixel_h, pointer,
                                ) {
                                    let face = self.pick_mesh_face(
                                        &mesh, node.world, rect, pixel_w, pixel_h, pointer,
                                    );
                                    self.scalpel_click_at(pick, face);
                                } else {
                                    self.status = "Click a point or edge of a polygon".into();
                                }
                            }
                        } else {
                            self.status = "Select an editable mesh first".into();
                        }
                        return;
                    }
                    if self.fill_hole_tool {
                        if let Some((id, mesh)) = self.selected_editable_mesh() {
                            if let Some(node) = evaluated.node(id) {
                                if let Some(point) = self.pick_boundary_point(
                                    &mesh, node.world, rect, pixel_w, pixel_h, pointer,
                                ) {
                                    self.fill_hole_at(point);
                                } else {
                                    self.status = "Click a point on an open hole border".into();
                                }
                            }
                        } else {
                            self.status = "Select an editable mesh first".into();
                        }
                        return;
                    }
                    if let Some((id, mesh)) = self.selected_editable_mesh() {
                        if let Some(node) = evaluated.node(id) {
                            if let Some(hit) = self.pick_mesh_component(
                                &mesh, node.world, rect, pixel_w, pixel_h, pointer,
                            ) {
                                self.toggle_mesh_selection(hit, modifiers.shift);
                                self.status = format!(
                                    "{} of {} selected",
                                    self.mesh_selected.len(),
                                    self.mesh_select_mode.label().to_lowercase()
                                );
                            } else if !modifiers.shift {
                                self.clear_mesh_component_selection();
                            }
                        }
                    }
                    return;
                }
                if let Some(joint) =
                    self.pick_joint_handle(evaluated, rect, pixel_w, pixel_h, pointer)
                {
                    self.set_active_weight_joint_node(joint);
                    return;
                }
                if let Some(id) = self.pick(evaluated, rect, pixel_w, pixel_h, pointer) {
                    if modifiers.shift || modifiers.command {
                        self.selection.toggle(id);
                    } else {
                        self.selection.select_one(id);
                    }
                    self.sync_selection_material();
                    self.status = self.selection_status();
                } else if !(modifiers.shift || modifiers.command) {
                    self.selection.clear();
                }
            }
        }
    }

    pub(super) fn select_all_objects(&mut self) {
        self.selection.set(self.doc.scene.depth_first());
        self.clear_mesh_component_selection();
        self.sync_selection_material();
        self.status = self.selection_status();
    }

    pub(super) fn deselect_all_objects(&mut self) {
        self.selection.clear();
        self.clear_mesh_component_selection();
        self.status = "Selection cleared".into();
    }

    pub(super) fn selection_status(&self) -> String {
        match self.selection.len() {
            0 => "Selection cleared".into(),
            1 => self
                .selection
                .primary()
                .and_then(|id| self.doc.scene.get(id))
                .map(|node| format!("Selected {}", node.name))
                .unwrap_or_else(|| "Selected object".into()),
            count => format!("Selected {count} objects"),
        }
    }

    pub(super) fn finish_box_select(
        &mut self,
        marquee: &BoxSelect,
        evaluated: &EvaluatedScene,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
    ) {
        let area = Rect::from_two_pos(marquee.start, marquee.current);
        if area.width() < 4.0 && area.height() < 4.0 {
            return;
        }
        let mut hits = Vec::new();
        for node in &evaluated.nodes {
            if !node.visible {
                continue;
            }
            if let Some(bounds) = self.node_screen_rect(node, rect, pixel_w, pixel_h) {
                if area.intersects(bounds) {
                    hits.push(node.id);
                }
            }
        }
        if marquee.additive {
            self.selection.set(marquee.base.iter().copied().chain(hits));
        } else {
            self.selection.set(hits);
        }
        self.sync_selection_material();
        self.status = self.selection_status();
    }

    pub(super) fn node_screen_rect(
        &self,
        node: &lc_scene::EvaluatedNode,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
    ) -> Option<Rect> {
        let points: Vec<Vec3> = if let Some(mesh) = node.mesh() {
            let bounds = mesh.bounds();
            if !bounds.is_valid() {
                return None;
            }
            let corners = [
                Vec3::new(bounds.min.x, bounds.min.y, bounds.min.z),
                Vec3::new(bounds.max.x, bounds.min.y, bounds.min.z),
                Vec3::new(bounds.min.x, bounds.max.y, bounds.min.z),
                Vec3::new(bounds.max.x, bounds.max.y, bounds.min.z),
                Vec3::new(bounds.min.x, bounds.min.y, bounds.max.z),
                Vec3::new(bounds.max.x, bounds.min.y, bounds.max.z),
                Vec3::new(bounds.min.x, bounds.max.y, bounds.max.z),
                Vec3::new(bounds.max.x, bounds.max.y, bounds.max.z),
            ];
            corners
                .into_iter()
                .map(|point| node.world.transform_point3(point))
                .collect()
        } else {
            vec![node.world.w_axis.truncate()]
        };
        let mut min = Pos2::new(f32::INFINITY, f32::INFINITY);
        let mut max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut any = false;
        for point in points {
            let Some(screen) = self.project(point, rect, pixel_w, pixel_h) else {
                continue;
            };
            any = true;
            min = Pos2::new(min.x.min(screen.x), min.y.min(screen.y));
            max = Pos2::new(max.x.max(screen.x), max.y.max(screen.y));
        }
        any.then(|| Rect::from_min_max(min, max).expand(3.0))
    }

    pub(super) fn gizmo_hit(
        &self,
        evaluated: &EvaluatedScene,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
        pointer: Pos2,
    ) -> Option<GizmoDrag> {
        if self.mesh_edit || self.weight_paint {
            return None;
        }
        if self.gizmo == GizmoMode::Off {
            return None;
        }
        let roots = self.selection_transform_roots();
        if roots.is_empty() {
            return None;
        }
        let origin = self.selection_pivot(evaluated)?;
        let axis_node = self
            .selection
            .primary()
            .and_then(|id| evaluated.node(id))
            .or_else(|| roots.first().and_then(|id| evaluated.node(*id)))?;
        let scale = (self.orbit.distance * 0.16).clamp(0.3, 6.0);
        let axes = self.gizmo_axes(axis_node.world);
        let origin_screen = self.project(origin, rect, pixel_w, pixel_h)?;
        let mut best: Option<(usize, f32, Pos2, Vec2)> = None;
        for (i, axis) in axes.iter().enumerate() {
            let end = origin + *axis * scale;
            let Some(end_screen) = self.project(end, rect, pixel_w, pixel_h) else {
                continue;
            };
            let dir = end_screen - origin_screen;
            let len = dir.length();
            if len < 4.0 {
                continue;
            }
            let dir_n = dir / len;
            let rel = pointer - origin_screen;
            let t = (rel.dot(dir_n) / len).clamp(0.0, 1.0);
            let closest = origin_screen + dir * t;
            let dist = (pointer - closest).length();
            if dist < 14.0 && t > 0.15 && best.is_none_or(|(_, d, _, _)| dist < d) {
                best = Some((i, dist, end_screen, dir_n));
            }
        }
        best.map(|(axis, _, _, screen_dir)| GizmoDrag {
            nodes: roots
                .into_iter()
                .filter_map(|id| {
                    let node = self.doc.scene.get(id)?;
                    Some((id, node.transform))
                })
                .collect(),
            pivot: origin,
            mode: self.gizmo,
            axis,
            start_pointer: pointer,
            screen_dir,
            screen_len: {
                let end = origin + axes[axis] * scale;
                self.project(end, rect, pixel_w, pixel_h)
                    .map(|p| (p - origin_screen).length())
                    .unwrap_or(0.0)
            },
            world_dir: axes[axis],
        })
    }

    pub(super) fn gizmo_axes(&self, world: Mat4) -> [Vec3; 3] {
        if self.gizmo_world {
            [Vec3::X, Vec3::Y, Vec3::Z]
        } else {
            [
                world.x_axis.truncate().normalize_or(Vec3::X),
                world.y_axis.truncate().normalize_or(Vec3::Y),
                world.z_axis.truncate().normalize_or(Vec3::Z),
            ]
        }
    }

    pub(super) fn apply_gizmo_drag(&mut self, drag: &mut GizmoDrag, amount: f32) {
        if drag.mode == GizmoMode::Off {
            return;
        }
        let scale = (self.orbit.distance * 0.16).clamp(0.3, 6.0);
        let axis = drag.world_dir.normalize_or(Vec3::Y);
        let world_delta = match drag.mode {
            GizmoMode::Translate => Mat4::from_translation(axis * (amount * scale)),
            GizmoMode::Rotate => {
                let rotation = Mat4::from_quat(Quat::from_axis_angle(axis, amount * 1.8));
                Mat4::from_translation(drag.pivot) * rotation * Mat4::from_translation(-drag.pivot)
            }
            GizmoMode::Scale => {
                let factor = (1.0 + amount * 1.2).clamp(0.02, 100.0);
                let spread = factor - 1.0;
                let cols = [
                    glam::Vec4::new(
                        1.0 + spread * axis.x * axis.x,
                        spread * axis.x * axis.y,
                        spread * axis.x * axis.z,
                        0.0,
                    ),
                    glam::Vec4::new(
                        spread * axis.y * axis.x,
                        1.0 + spread * axis.y * axis.y,
                        spread * axis.y * axis.z,
                        0.0,
                    ),
                    glam::Vec4::new(
                        spread * axis.z * axis.x,
                        spread * axis.z * axis.y,
                        1.0 + spread * axis.z * axis.z,
                        0.0,
                    ),
                    glam::Vec4::new(0.0, 0.0, 0.0, 1.0),
                ];
                Mat4::from_translation(drag.pivot)
                    * Mat4::from_cols(cols[0], cols[1], cols[2], cols[3])
                    * Mat4::from_translation(-drag.pivot)
            }
            GizmoMode::Off => return,
        };
        let mut updates = Vec::new();
        for (id, start) in drag.nodes.clone() {
            let Some(node) = self.doc.scene.get(id) else {
                continue;
            };
            if node.locked {
                continue;
            }
            let parent_world = node
                .parent
                .map(|p| self.doc.scene.world_matrix_plain(p))
                .unwrap_or(Mat4::IDENTITY);
            let transform = match drag.mode {
                GizmoMode::Translate => {
                    let local_delta = parent_world
                        .inverse()
                        .transform_vector3(axis * (amount * scale));
                    Transform {
                        translation: start.translation + local_delta,
                        ..start
                    }
                }
                GizmoMode::Rotate | GizmoMode::Scale => {
                    let start_world = parent_world * start.matrix();
                    let new_local = parent_world.inverse() * world_delta * start_world;
                    let (scale, rotation, translation) = new_local.to_scale_rotation_translation();
                    Transform::new(translation, rotation, scale)
                }
                GizmoMode::Off => start,
            };
            updates.push((id, start, transform));
        }
        if updates.is_empty() {
            return;
        }
        let pending = self.pending_transform.get_or_insert_with(Vec::new);
        for (id, start, transform) in updates {
            if !pending.iter().any(|(pending_id, _)| *pending_id == id) {
                pending.push((id, start));
            }
            if let Some(node) = self.doc.scene.get_mut(id) {
                node.transform = transform;
            }
        }
    }

    pub(super) fn pick(
        &self,
        evaluated: &EvaluatedScene,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
        pointer: Pos2,
    ) -> Option<NodeId> {
        let pixel = V2::new(
            (pointer.x - rect.min.x) / rect.width().max(1.0) * pixel_w as f32,
            (pointer.y - rect.min.y) / rect.height().max(1.0) * pixel_h as f32,
        );
        let world = self.orbit.world();
        let camera = self.orbit.camera();
        let mut rng = Pcg32::new(1);
        let ray = lc_render::RenderCamera::new(world, camera).ray(
            pixel,
            V2::new(pixel_w as f32, pixel_h as f32),
            &mut rng,
        );
        let mut best: Option<(f32, NodeId)> = None;
        for node in evaluated.mesh_nodes() {
            let Some(mesh) = node.mesh() else {
                continue;
            };
            let inv = node.world.inverse();
            let origin = inv.transform_point3(ray.origin);
            let dir = inv.transform_vector3(ray.dir);
            let local_ray = lc_core::Ray::new(origin, dir);
            for tri in mesh.render_triangles(lc_geom::CREASE_ANGLE_DEG) {
                if let Some((t, _, _)) = ray_triangle(
                    &local_ray,
                    tri.v[0].position,
                    tri.v[1].position,
                    tri.v[2].position,
                ) {
                    if t > 1e-5 && best.map(|(b, _)| t < b).unwrap_or(true) {
                        best = Some((t, node.id));
                    }
                }
            }
        }
        best.map(|(_, id)| id)
    }

    pub(super) fn project(
        &self,
        point: Vec3,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
    ) -> Option<Pos2> {
        let camera = lc_render::RenderCamera::new(self.orbit.world(), self.orbit.camera());
        let p = camera.project(point, V2::new(pixel_w as f32, pixel_h as f32))?;
        Some(Pos2::new(
            rect.min.x + p.x / pixel_w as f32 * rect.width(),
            rect.min.y + p.y / pixel_h as f32 * rect.height(),
        ))
    }

    pub(super) fn draw_weight_paint_overlay(
        &self,
        painter: &egui::Painter,
        ui: &egui::Ui,
        rect: Rect,
        evaluated: &EvaluatedScene,
        pixel_w: u32,
        pixel_h: u32,
    ) {
        let Some(id) = self.selection.primary() else {
            return;
        };
        let Some(node) = evaluated.node(id) else {
            return;
        };
        let Some(mesh) = node.mesh() else {
            return;
        };
        let Some(skin) = self.doc.scene.get(id).and_then(|node| node.skin.as_ref()) else {
            return;
        };
        let Some(joint) = self.active_weight_joint() else {
            return;
        };
        for (vertex, position) in mesh.positions.iter().enumerate() {
            let Some(screen) = self.project(
                node.world.transform_point3(*position),
                rect,
                pixel_w,
                pixel_h,
            ) else {
                continue;
            };
            let weight = skin.influence(vertex, joint).clamp(0.0, 1.0);
            let color = if weight > 0.001 {
                Color32::from_rgba_unmultiplied(245, 135, 35, (55.0 + 190.0 * weight) as u8)
            } else {
                Color32::from_rgba_unmultiplied(190, 195, 200, 80)
            };
            painter.circle_filled(screen, 2.4, color);
        }
        if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
            if rect.contains(pointer) {
                painter.circle_stroke(
                    pointer,
                    self.weight_brush.radius,
                    egui::Stroke::new(1.4, Color32::from_rgb(245, 160, 50)),
                );
            }
        }
    }

    pub(super) fn draw_viewport_overlay(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        evaluated: &EvaluatedScene,
        pixel_w: u32,
        pixel_h: u32,
    ) {
        let painter = ui.painter_at(rect);
        if self.weight_paint {
            self.draw_weight_paint_overlay(&painter, ui, rect, evaluated, pixel_w, pixel_h);
            return;
        }
        if self.mesh_edit {
            self.draw_mesh_edit_overlay(ui, rect, evaluated, pixel_w, pixel_h);
            return;
        }
        for id in self.selection.ids() {
            let Some(node) = evaluated.node(*id) else {
                continue;
            };
            if let Some(origin) = self.project(node.world.w_axis.truncate(), rect, pixel_w, pixel_h)
            {
                painter.circle_filled(origin, 2.6, Color32::from_rgb(245, 248, 255));
                painter.circle_stroke(origin, 4.2, egui::Stroke::new(1.2, theme::SELECTION));
            }
        }
        if let Some(primary) = self.selection.primary() {
            if let Some(skin) = self
                .doc
                .scene
                .get(primary)
                .and_then(|node| node.skin.as_ref())
            {
                let active = self.active_weight_joint();
                for (index, joint) in skin.joints.iter().enumerate() {
                    let Some(node) = evaluated.node(*joint) else {
                        continue;
                    };
                    let Some(screen) =
                        self.project(node.world.w_axis.truncate(), rect, pixel_w, pixel_h)
                    else {
                        continue;
                    };
                    if active == Some(index) {
                        painter.circle_filled(screen, 4.6, Color32::from_rgb(245, 150, 60));
                        painter.circle_stroke(
                            screen,
                            6.4,
                            egui::Stroke::new(1.2, Color32::from_rgb(255, 220, 170)),
                        );
                    } else {
                        painter.circle_stroke(
                            screen,
                            4.2,
                            egui::Stroke::new(1.4, Color32::from_rgb(210, 190, 150)),
                        );
                    }
                }
            }
        }
        if let Some(origin) = self.selection_pivot(evaluated) {
            let axis_node = self
                .selection
                .primary()
                .and_then(|id| evaluated.node(id))
                .or_else(|| {
                    self.selection
                        .ids()
                        .iter()
                        .find_map(|id| evaluated.node(*id))
                });
            if let (Some(node), Some(origin_screen)) =
                (axis_node, self.project(origin, rect, pixel_w, pixel_h))
            {
                let scale = (self.orbit.distance * 0.16).clamp(0.3, 6.0);
                let axes = self.gizmo_axes(node.world);
                let colors = [
                    Color32::from_rgb(230, 80, 80),
                    Color32::from_rgb(90, 210, 110),
                    Color32::from_rgb(80, 140, 245),
                ];
                if self.gizmo != GizmoMode::Off {
                    for (i, axis) in axes.iter().enumerate() {
                        if let Some(end) =
                            self.project(origin + *axis * scale, rect, pixel_w, pixel_h)
                        {
                            painter.line_segment(
                                [origin_screen, end],
                                egui::Stroke::new(
                                    if self
                                        .gizmo_drag
                                        .as_ref()
                                        .map(|d| d.axis == i)
                                        .unwrap_or(false)
                                    {
                                        4.0
                                    } else {
                                        2.2
                                    },
                                    colors[i],
                                ),
                            );
                            if self.gizmo == GizmoMode::Translate {
                                painter.circle_filled(end, 4.0, colors[i]);
                            }
                            painter.text(
                                end + Vec2::new(4.0, -4.0),
                                egui::Align2::LEFT_BOTTOM,
                                ["X", "Y", "Z"][i],
                                egui::FontId::proportional(11.0),
                                colors[i],
                            );
                        }
                    }
                    if self.gizmo == GizmoMode::Rotate {
                        painter.circle_stroke(
                            origin_screen,
                            38.0,
                            egui::Stroke::new(1.0, Color32::from_rgb(220, 180, 120)),
                        );
                    }
                }
            }
        }
        if let Some(marquee) = &self.box_select {
            let area = Rect::from_two_pos(marquee.start, marquee.current);
            painter.rect_filled(area, 0.0, Color32::from_rgba_unmultiplied(70, 120, 210, 28));
            painter.rect_stroke(
                area,
                0.0,
                egui::Stroke::new(1.0, theme::SELECTION),
                egui::StrokeKind::Inside,
            );
        }
    }
}
