//! Fixed-pane panels: inspector, object browser, layers, takes and console.

use super::*;
use crate::icons::{self, Icon};
use crate::theme;

impl LeopardApp {
    // -----------------------------------------------------------------------
    // Object browser
    // -----------------------------------------------------------------------

    pub(super) fn ui_properties(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new("Select an object in the viewport or object browser")
                        .weak(),
                );
            });
            return;
        };
        let Some(original) = self.doc.scene.get(id).cloned() else {
            self.selected = None;
            return;
        };
        let mut node = original.clone();
        let node_list: Vec<(NodeId, String)> = self
            .doc
            .scene
            .depth_first()
            .into_iter()
            .filter_map(|nid| {
                let n = self.doc.scene.get(nid)?;
                Some((nid, n.name.clone()))
            })
            .collect();
        let materials: Vec<(MaterialId, String)> = self
            .doc
            .materials
            .iter()
            .map(|m| (m.id, m.name.clone()))
            .collect();
        let layers: Vec<(u8, String)> = self
            .doc
            .settings
            .layers
            .iter()
            .enumerate()
            .map(|(i, l)| (i as u8, l.name.clone()))
            .collect();
        let mut parent = node.parent;
        let mut parent_changed = false;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                    icons::paint(
                        ui.painter(),
                        icon_rect.shrink(1.0),
                        node_icon(&node.kind),
                        theme::TEXT,
                    );
                    ui.label(
                        egui::RichText::new("Name")
                            .size(11.0)
                            .color(theme::TEXT_WEAK),
                    );
                    let mut name = node.name.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY))
                        .changed()
                    {
                        node.name = name;
                    }
                });
                ui.label(
                    egui::RichText::new(format!("{} · id {}", node.kind.label(), node.id))
                        .small()
                        .weak(),
                );
                ui.separator();

                theme::section(ui, "visibility", "Visibility", |ui| {
                    egui::Grid::new("object_basics")
                        .num_columns(2)
                        .spacing([8.0, 5.0])
                        .show(ui, |ui| {
                            ui.label("Visible");
                            ui.checkbox(&mut node.visible, "");
                            ui.end_row();
                            ui.label("Renderable");
                            ui.checkbox(&mut node.renderable, "");
                            ui.end_row();
                            ui.label("Locked");
                            ui.checkbox(&mut node.locked, "");
                            ui.end_row();
                            ui.label("Layer");
                            egui::ComboBox::from_id_salt("layer_combo")
                                .selected_text(
                                    layers
                                        .iter()
                                        .find(|(l, _)| *l == node.layer)
                                        .map(|(_, n)| n.clone())
                                        .unwrap_or_else(|| "Layer".into()),
                                )
                                .show_ui(ui, |ui| {
                                    for (index, name) in &layers {
                                        ui.selectable_value(&mut node.layer, *index, name);
                                    }
                                });
                            ui.end_row();
                            ui.label("Parent");
                            egui::ComboBox::from_id_salt("parent_combo")
                                .selected_text(
                                    parent
                                        .and_then(|p| node_list.iter().find(|(id, _)| *id == p))
                                        .map(|(_, n)| n.clone())
                                        .unwrap_or_else(|| "— Root —".into()),
                                )
                                .show_ui(ui, |ui| {
                                    if ui.selectable_value(&mut parent, None, "— Root —").clicked()
                                    {
                                        parent_changed = true;
                                    }
                                    for (nid, name) in &node_list {
                                        if *nid == id || self.doc.scene.is_ancestor(id, *nid) {
                                            continue;
                                        }
                                        if ui
                                            .selectable_value(&mut parent, Some(*nid), name)
                                            .clicked()
                                        {
                                            parent_changed = true;
                                        }
                                    }
                                });
                            ui.end_row();
                            ui.label("Material");
                            egui::ComboBox::from_id_salt("material_combo")
                                .selected_text(
                                    node.material
                                        .and_then(|m| materials.iter().find(|(id, _)| *id == m))
                                        .map(|(_, n)| n.clone())
                                        .unwrap_or_else(|| "None".into()),
                                )
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut node.material, None, "None");
                                    for (mid, name) in &materials {
                                        ui.selectable_value(&mut node.material, Some(*mid), name);
                                    }
                                });
                            ui.end_row();
                        });
                });

                theme::section(ui, "transform", "Transform", |ui| {
                    let mut euler = node.transform.euler_degrees();
                    let mut rotation_changed = false;
                    egui::Grid::new("transform_grid")
                        .num_columns(2)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Location");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::DragValue::new(&mut node.transform.translation.x)
                                        .speed(0.01)
                                        .prefix("X "),
                                );
                                ui.add(
                                    egui::DragValue::new(&mut node.transform.translation.y)
                                        .speed(0.01)
                                        .prefix("Y "),
                                );
                                ui.add(
                                    egui::DragValue::new(&mut node.transform.translation.z)
                                        .speed(0.01)
                                        .prefix("Z "),
                                );
                            });
                            ui.end_row();
                            ui.label("Rotation°");
                            ui.horizontal(|ui| {
                                rotation_changed |= ui
                                    .add(egui::DragValue::new(&mut euler.x).speed(0.5).prefix("X "))
                                    .changed();
                                rotation_changed |= ui
                                    .add(egui::DragValue::new(&mut euler.y).speed(0.5).prefix("Y "))
                                    .changed();
                                rotation_changed |= ui
                                    .add(egui::DragValue::new(&mut euler.z).speed(0.5).prefix("Z "))
                                    .changed();
                            });
                            ui.end_row();
                            ui.label("Scale");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::DragValue::new(&mut node.transform.scale.x)
                                        .speed(0.01)
                                        .prefix("X "),
                                );
                                ui.add(
                                    egui::DragValue::new(&mut node.transform.scale.y)
                                        .speed(0.01)
                                        .prefix("Y "),
                                );
                                ui.add(
                                    egui::DragValue::new(&mut node.transform.scale.z)
                                        .speed(0.01)
                                        .prefix("Z "),
                                );
                            });
                            ui.end_row();
                        });
                    if rotation_changed {
                        node.transform.rotation = Transform::from_euler_degrees(euler).rotation;
                    }
                    ui.horizontal(|ui| {
                        ui.label("Gizmo");
                        for mode in [
                            GizmoMode::Translate,
                            GizmoMode::Rotate,
                            GizmoMode::Scale,
                            GizmoMode::Off,
                        ] {
                            let label = match mode {
                                GizmoMode::Translate => "Move",
                                GizmoMode::Rotate => "Rotate",
                                GizmoMode::Scale => "Scale",
                                GizmoMode::Off => "Off",
                            };
                            ui.selectable_value(&mut self.gizmo, mode, label);
                        }
                        ui.checkbox(&mut self.gizmo_world, "World axes");
                    });
                });

                theme::section(
                    ui,
                    "parameters",
                    "Object Parameters",
                    |ui| match &mut node.kind {
                        NodeKind::Mesh { geometry } => match geometry {
                            GeometrySource::Primitive { primitive } => match primitive {
                                Primitive::Box {
                                    width,
                                    height,
                                    depth,
                                } => {
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::DragValue::new(width).speed(0.01).prefix("W "),
                                        );
                                        ui.add(
                                            egui::DragValue::new(height).speed(0.01).prefix("H "),
                                        );
                                        ui.add(
                                            egui::DragValue::new(depth).speed(0.01).prefix("D "),
                                        );
                                    });
                                }
                                Primitive::Sphere {
                                    radius,
                                    segments,
                                    rings,
                                } => {
                                    ui.add(
                                        egui::DragValue::new(radius).speed(0.01).prefix("Radius "),
                                    );
                                    ui.add(
                                        egui::DragValue::new(segments)
                                            .speed(0.2)
                                            .range(3..=128)
                                            .prefix("Segments "),
                                    );
                                    ui.add(
                                        egui::DragValue::new(rings)
                                            .speed(0.2)
                                            .range(2..=128)
                                            .prefix("Rings "),
                                    );
                                }
                                Primitive::Cylinder {
                                    radius,
                                    height,
                                    segments,
                                } => {
                                    ui.add(
                                        egui::DragValue::new(radius).speed(0.01).prefix("Radius "),
                                    );
                                    ui.add(
                                        egui::DragValue::new(height).speed(0.01).prefix("Height "),
                                    );
                                    ui.add(
                                        egui::DragValue::new(segments)
                                            .speed(0.2)
                                            .range(3..=128)
                                            .prefix("Segments "),
                                    );
                                }
                                Primitive::Cone {
                                    radius,
                                    height,
                                    segments,
                                } => {
                                    ui.add(
                                        egui::DragValue::new(radius).speed(0.01).prefix("Radius "),
                                    );
                                    ui.add(
                                        egui::DragValue::new(height).speed(0.01).prefix("Height "),
                                    );
                                    ui.add(
                                        egui::DragValue::new(segments)
                                            .speed(0.2)
                                            .range(3..=128)
                                            .prefix("Segments "),
                                    );
                                }
                                Primitive::Torus {
                                    major_radius,
                                    minor_radius,
                                    segments,
                                    rings,
                                } => {
                                    ui.add(
                                        egui::DragValue::new(major_radius)
                                            .speed(0.01)
                                            .prefix("Major "),
                                    );
                                    ui.add(
                                        egui::DragValue::new(minor_radius)
                                            .speed(0.01)
                                            .prefix("Minor "),
                                    );
                                    ui.add(
                                        egui::DragValue::new(segments)
                                            .speed(0.2)
                                            .range(3..=128)
                                            .prefix("Segments "),
                                    );
                                    ui.add(
                                        egui::DragValue::new(rings)
                                            .speed(0.2)
                                            .range(3..=128)
                                            .prefix("Rings "),
                                    );
                                }
                                Primitive::Plane {
                                    width,
                                    depth,
                                    segments_x,
                                    segments_z,
                                } => {
                                    ui.add(
                                        egui::DragValue::new(width).speed(0.01).prefix("Width "),
                                    );
                                    ui.add(
                                        egui::DragValue::new(depth).speed(0.01).prefix("Depth "),
                                    );
                                    ui.add(
                                        egui::DragValue::new(segments_x)
                                            .speed(0.2)
                                            .range(1..=128)
                                            .prefix("X "),
                                    );
                                    ui.add(
                                        egui::DragValue::new(segments_z)
                                            .speed(0.2)
                                            .range(1..=128)
                                            .prefix("Z "),
                                    );
                                }
                            },
                            GeometrySource::Editable { mesh, origin } => {
                                ui.label(format!(
                                    "{} points · {} polygons · {} triangles",
                                    mesh.positions.len(),
                                    mesh.polygons.len(),
                                    mesh.triangle_count()
                                ));
                                ui.label(format!("{} UV coordinates", mesh.uvs.len()));
                                if let Some(origin) = origin {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Converted from {}",
                                            origin.label()
                                        ))
                                        .small()
                                        .color(theme::TEXT_WEAK),
                                    );
                                }
                            }
                        },
                        NodeKind::Camera { camera } => {
                            egui::ComboBox::from_id_salt("projection_combo")
                                .selected_text(camera.projection.label())
                                .show_ui(ui, |ui| {
                                    for mode in [
                                        ProjectionMode::Perspective,
                                        ProjectionMode::Orthographic,
                                        ProjectionMode::Panoramic,
                                    ] {
                                        ui.selectable_value(
                                            &mut camera.projection,
                                            mode,
                                            mode.label(),
                                        );
                                    }
                                });
                            ui.add(
                                egui::DragValue::new(&mut camera.fov_y_deg)
                                    .speed(0.2)
                                    .range(1.0..=179.0)
                                    .prefix("FOV "),
                            );
                            ui.add(
                                egui::DragValue::new(&mut camera.near)
                                    .speed(0.001)
                                    .prefix("Near "),
                            );
                            ui.add(
                                egui::DragValue::new(&mut camera.far)
                                    .speed(0.1)
                                    .prefix("Far "),
                            );
                            ui.add(
                                egui::DragValue::new(&mut camera.ortho_scale)
                                    .speed(0.01)
                                    .prefix("Ortho "),
                            );
                            ui.add(
                                egui::DragValue::new(&mut camera.aperture)
                                    .speed(0.001)
                                    .prefix("Aperture "),
                            );
                            ui.add(
                                egui::DragValue::new(&mut camera.focus_distance)
                                    .speed(0.01)
                                    .prefix("Focus "),
                            );
                        }
                        NodeKind::Light { light } => {
                            ui.horizontal(|ui| {
                                let mut rgb = light.color.to_array();
                                if ui.color_edit_button_rgb(&mut rgb).changed() {
                                    light.color = Vec3::from_array(rgb).max(Vec3::ZERO);
                                }
                                ui.add(
                                    egui::DragValue::new(&mut light.intensity)
                                        .speed(0.1)
                                        .prefix("Intensity "),
                                );
                            });
                            ui.checkbox(&mut light.cast_shadows, "Cast shadows");
                            match &mut light.kind {
                                LightKind::Directional => {
                                    ui.label("Direction follows the object's -Z axis");
                                }
                                LightKind::Point { radius } => {
                                    ui.add(
                                        egui::DragValue::new(radius).speed(0.01).prefix("Radius "),
                                    );
                                }
                                LightKind::Area { width, height } => {
                                    ui.add(
                                        egui::DragValue::new(width).speed(0.01).prefix("Width "),
                                    );
                                    ui.add(
                                        egui::DragValue::new(height).speed(0.01).prefix("Height "),
                                    );
                                }
                            }
                        }
                        NodeKind::Joint { length, width } => {
                            ui.add(egui::DragValue::new(length).speed(0.01).prefix("Length "));
                            ui.add(egui::DragValue::new(width).speed(0.005).prefix("Width "));
                        }
                        NodeKind::Spline { spline } => {
                            let mut changed = false;
                            egui::ComboBox::from_id_salt("spline_kind")
                                .selected_text(match spline.kind {
                                    lc_geom::SplineKind::Linear => "Linear",
                                    lc_geom::SplineKind::Bezier => "Bezier",
                                    lc_geom::SplineKind::Smooth => "Smooth",
                                })
                                .show_ui(ui, |ui| {
                                    changed |= ui
                                        .selectable_value(
                                            &mut spline.kind,
                                            lc_geom::SplineKind::Linear,
                                            "Linear",
                                        )
                                        .changed();
                                    changed |= ui
                                        .selectable_value(
                                            &mut spline.kind,
                                            lc_geom::SplineKind::Bezier,
                                            "Bezier",
                                        )
                                        .changed();
                                    changed |= ui
                                        .selectable_value(
                                            &mut spline.kind,
                                            lc_geom::SplineKind::Smooth,
                                            "Smooth",
                                        )
                                        .changed();
                                });
                            changed |= ui.checkbox(&mut spline.closed, "Closed").changed();
                            ui.add(
                                egui::DragValue::new(&mut spline.resolution)
                                    .range(1..=64)
                                    .prefix("Resolution "),
                            );
                            theme::section(ui, "control_points", "Control Points", |ui| {
                                let mut remove = None;
                                for (pi, point) in spline.points.iter_mut().enumerate() {
                                    ui.push_id(pi, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(format!("#{pi}"));
                                            changed |= ui
                                                .add(
                                                    egui::DragValue::new(&mut point.position.x)
                                                        .speed(0.01)
                                                        .prefix("X "),
                                                )
                                                .changed();
                                            changed |= ui
                                                .add(
                                                    egui::DragValue::new(&mut point.position.y)
                                                        .speed(0.01)
                                                        .prefix("Y "),
                                                )
                                                .changed();
                                            changed |= ui
                                                .add(
                                                    egui::DragValue::new(&mut point.position.z)
                                                        .speed(0.01)
                                                        .prefix("Z "),
                                                )
                                                .changed();
                                            if theme::close_icon(ui, 16.0)
                                                .on_hover_text("Remove control point")
                                                .clicked()
                                            {
                                                remove = Some(pi);
                                            }
                                        });
                                    });
                                }
                                if let Some(index) = remove {
                                    spline.points.remove(index);
                                    changed = true;
                                }
                                if ui.button("Add Control Point").clicked() {
                                    let position = spline
                                        .points
                                        .last()
                                        .map(|p| p.position + Vec3::X)
                                        .unwrap_or(Vec3::ZERO);
                                    spline.points.push(lc_geom::SplinePoint::new(position));
                                    changed = true;
                                }
                            });
                            if changed {
                                spline.set_auto_handles();
                            }
                        }
                        NodeKind::Null => {
                            ui.label("Empty transform group");
                        }
                    },
                );

                if node.is_mesh() {
                    theme::section(ui, "skinning", "Skinning", |ui| {
                        let evaluated = self.evaluate();
                        let vertex_count = evaluated
                            .node(id)
                            .and_then(|n| n.mesh())
                            .map(|m| m.positions.len())
                            .unwrap_or(0);
                        let joints: Vec<(NodeId, String, Mat4)> = node_list
                            .iter()
                            .filter_map(|(joint_id, name)| {
                                if !matches!(
                                    self.doc.scene.get(*joint_id).map(|n| &n.kind),
                                    Some(NodeKind::Joint { .. })
                                ) {
                                    return None;
                                }
                                let world = evaluated.node(*joint_id)?.world;
                                Some((*joint_id, name.clone(), world))
                            })
                            .collect();
                        ui.label(
                            egui::RichText::new(format!(
                                "{} mesh vertices · {} joint objects",
                                vertex_count,
                                joints.len()
                            ))
                            .small()
                            .weak(),
                        );
                        let bind = |node: &mut Node, joints: &[(NodeId, String, Mat4)]| {
                            if joints.is_empty() || vertex_count == 0 {
                                return;
                            }
                            let mut skin = Skin::new(
                                joints.iter().map(|(joint_id, _, _)| *joint_id).collect(),
                                joints.iter().map(|(_, _, world)| world.inverse()).collect(),
                            );
                            skin.bind_all_to_first(vertex_count);
                            node.skin = Some(skin);
                        };
                        let mut unbind = false;
                        let skin_info = node
                            .skin
                            .as_ref()
                            .map(|skin| (skin.joints.len(), skin.vertex_count()));
                        if let Some((joint_count, weighted_vertices)) = skin_info {
                            ui.label(format!(
                                "{} joints · {} weighted vertices",
                                joint_count, weighted_vertices
                            ));
                            if ui
                                .add_enabled(
                                    !joints.is_empty() && vertex_count > 0,
                                    egui::Button::new("Rebind to all joints"),
                                )
                                .clicked()
                            {
                                bind(&mut node, &joints);
                            }
                            if ui.button("Unbind skin").clicked() {
                                unbind = true;
                            }
                        } else {
                            if ui
                                .add_enabled(
                                    !joints.is_empty() && vertex_count > 0,
                                    egui::Button::new("Bind to all joints"),
                                )
                                .clicked()
                            {
                                bind(&mut node, &joints);
                            }
                            ui.label(
                                egui::RichText::new(
                                    "Add Joint objects, then bind to create a skin. \
                                     Every vertex starts fully weighted to the first joint.",
                                )
                                .small()
                                .weak(),
                            );
                        }
                        if unbind {
                            node.skin = None;
                        }
                    });
                }

                theme::section(ui, "modifiers", "Modifier Stack", |ui| {
                    let boolean_candidates: Vec<(NodeId, String)> = node_list
                        .iter()
                        .filter(|(nid, _)| {
                            *nid != id
                                && self
                                    .doc
                                    .scene
                                    .get(*nid)
                                    .map(|n| n.is_mesh())
                                    .unwrap_or(false)
                        })
                        .cloned()
                        .collect();
                    let mut remove = None;
                    for (i, modifier) in node.modifiers.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(modifier.label());
                            match modifier {
                                Modifier::Subdivision { levels } => {
                                    ui.add(egui::DragValue::new(levels).range(0..=4));
                                }
                                Modifier::Bend { angle_deg } => {
                                    ui.add(egui::DragValue::new(angle_deg).speed(1.0).suffix("°"));
                                }
                                Modifier::Taper { amount, .. } => {
                                    ui.add(egui::DragValue::new(amount).speed(0.01));
                                }
                                Modifier::Array { count, .. } => {
                                    ui.add(egui::DragValue::new(count).range(1..=256));
                                }
                                Modifier::BevelFaces { amount, depth, .. } => {
                                    ui.add(egui::DragValue::new(amount).speed(0.005));
                                    ui.add(egui::DragValue::new(depth).speed(0.005));
                                }
                                Modifier::ExtrudeFaces { distance, .. } => {
                                    ui.add(egui::DragValue::new(distance).speed(0.005));
                                }
                                Modifier::Boolean { operand, operation } => {
                                    egui::ComboBox::from_id_salt(("bool_operand", i))
                                        .selected_text(
                                            boolean_candidates
                                                .iter()
                                                .find(|(nid, _)| nid == operand)
                                                .map(|(_, name)| name.clone())
                                                .unwrap_or_else(|| "Operand".into()),
                                        )
                                        .show_ui(ui, |ui| {
                                            for (operand_id, name) in &boolean_candidates {
                                                ui.selectable_value(operand, *operand_id, name);
                                            }
                                        });
                                    egui::ComboBox::from_id_salt(("bool_op", i))
                                        .selected_text(operation.label())
                                        .show_ui(ui, |ui| {
                                            for op in [
                                                lc_scene::CsgOperation::Union,
                                                lc_scene::CsgOperation::Subtract,
                                                lc_scene::CsgOperation::Intersect,
                                            ] {
                                                ui.selectable_value(operation, op, op.label());
                                            }
                                        });
                                }
                                _ => {}
                            }
                            if theme::close_icon(ui, 16.0)
                                .on_hover_text("Remove modifier")
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        node.modifiers.remove(i);
                    }
                    ui.menu_button("Add Modifier", |ui| {
                        for modifier in Modifier::defaults() {
                            let label = modifier.label();
                            if modifier_is_boolean(&modifier) {
                                continue;
                            }
                            if ui.button(label).clicked() {
                                node.modifiers.push(modifier);
                            }
                        }
                    });
                    if !boolean_candidates.is_empty() {
                        ui.menu_button("Add Boolean", |ui| {
                            for (operand_id, name) in &boolean_candidates {
                                for operation in [
                                    lc_scene::CsgOperation::Subtract,
                                    lc_scene::CsgOperation::Union,
                                    lc_scene::CsgOperation::Intersect,
                                ] {
                                    if ui
                                        .button(format!("{} {}", operation.label(), name))
                                        .clicked()
                                    {
                                        node.modifiers.push(Modifier::Boolean {
                                            operand: *operand_id,
                                            operation,
                                        });
                                    }
                                }
                            }
                        });
                    }
                });

                theme::section(ui, "tags", "Tags", |ui| {
                    let mut remove = None;
                    for (i, tag) in node.tags.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(tag.label());
                            if theme::close_icon(ui, 16.0)
                                .on_hover_text("Remove tag")
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        node.tags.remove(i);
                    }
                    ui.menu_button("Add Tag", |ui| {
                        let target = node_list
                            .iter()
                            .find(|(nid, _)| *nid != id)
                            .map(|(nid, _)| *nid)
                            .unwrap_or(id);
                        if ui.button("Look At").clicked() {
                            node.tags.push(Tag::LookAt {
                                target,
                                up: Vec3::Y,
                            });
                        }
                        if ui.button("Render Override").clicked() {
                            node.tags.push(Tag::RenderOverride {
                                visible: node.visible,
                                casts_shadow: true,
                            });
                        }
                        if ui.button("Clamp Rotation").clicked() {
                            node.tags.push(Tag::ClampRotation {
                                min: Vec3::splat(-180.0),
                                max: Vec3::splat(180.0),
                            });
                        }
                        if ui.button("Copy Location").clicked() {
                            node.tags.push(Tag::CopyLocation {
                                source: target,
                                offset: Vec3::ZERO,
                                weight: 1.0,
                            });
                        }
                        if ui.button("Custom Data").clicked() {
                            node.tags.push(Tag::Custom {
                                name: "Meta".into(),
                                values: BTreeMap::new(),
                            });
                        }
                    });
                });

                if let Some(mesh) = self
                    .doc
                    .evaluate(self.playback.time)
                    .node(id)
                    .and_then(|n| n.mesh())
                {
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!(
                            "Evaluated: {} points · {} tris",
                            mesh.positions.len(),
                            mesh.triangle_count()
                        ))
                        .small()
                        .weak(),
                    );
                }

                theme::rule(ui);
                self.ui_mesh_edit(ui);
            });

        if parent_changed {
            if let Some(cmd) = ReparentCmd::new(&self.doc, id, parent) {
                self.history.execute(&mut self.doc, Box::new(cmd));
                self.status = "Reparented object".into();
            }
        }
        // Mesh-edit actions (entering edit mode, topology tools and component
        // transforms) replace the node kind directly. The property editor's
        // local clone predates that replacement, so applying it would undo the
        // mesh edit. Skip the stale commit for this frame and refresh next frame.
        let original_shape = Shape::of(&original);
        let shape_changed_below = self
            .doc
            .scene
            .get(id)
            .map(Shape::of)
            .map(|shape| {
                shape.kind != original_shape.kind
                    || shape.modifiers != original_shape.modifiers
                    || shape.skin != original_shape.skin
            })
            .unwrap_or(true);
        if !shape_changed_below {
            self.apply_node_edits(id, &original, &node);
        }
    }
}

impl LeopardApp {
    pub(super) fn ui_mesh_edit(&mut self, ui: &mut egui::Ui) {
        let mesh = self.selected_editable_mesh().map(|(_, mesh)| mesh);
        let mut action = None;
        theme::group_title(ui, "Tool");
        ui.horizontal_wrapped(|ui| {
            if self.mesh_edit {
                if ui.button("Exit Mesh Edit").clicked() {
                    action = Some(MeshAction::Exit);
                }
            } else if ui.button("Enter Mesh Edit").clicked() {
                action = Some(MeshAction::Enter);
            }
            if mesh.is_none() {
                ui.label(
                    egui::RichText::new("converts the primitive to an editable n-gon mesh")
                        .small()
                        .weak(),
                );
            }
        });
        if !self.mesh_edit {
            if let Some(action) = action {
                self.perform_mesh_action(action);
            }
            return;
        }
        let Some(mesh) = mesh else {
            ui.label("Select an editable mesh object to edit its components.");
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.label("Select");
            for mode in [
                MeshSelectMode::Points,
                MeshSelectMode::Edges,
                MeshSelectMode::Polygons,
            ] {
                if ui
                    .selectable_label(self.mesh_select_mode == mode, mode.label())
                    .clicked()
                    && self.mesh_select_mode != mode
                {
                    self.mesh_select_mode = mode;
                    self.mesh_selected.clear();
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("All").clicked() {
                action = Some(MeshAction::SelectAll);
            }
            if ui.button("None").clicked() {
                action = Some(MeshAction::SelectNone);
            }
            ui.label(
                egui::RichText::new(format!(
                    "{} selected · {} points · {} edges · {} polygons",
                    self.mesh_selected.len(),
                    mesh.positions.len(),
                    mesh.edges().len(),
                    mesh.polygons.len()
                ))
                .small()
                .weak(),
            );
        });
        ui.label(
            egui::RichText::new(
                "Click components in the viewport. Shift-click adds to the selection.",
            )
            .small()
            .weak(),
        );

        ui.separator();
        ui.label("Transform selection");
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("mesh_transform_mode")
                .selected_text(match self.mesh_transform {
                    MeshTransformMode::Move => "Move",
                    MeshTransformMode::Rotate => "Rotate",
                    MeshTransformMode::Scale => "Scale",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.mesh_transform, MeshTransformMode::Move, "Move");
                    ui.selectable_value(
                        &mut self.mesh_transform,
                        MeshTransformMode::Rotate,
                        "Rotate",
                    );
                    ui.selectable_value(
                        &mut self.mesh_transform,
                        MeshTransformMode::Scale,
                        "Scale",
                    );
                });
            egui::ComboBox::from_id_salt("mesh_transform_space")
                .selected_text(match self.mesh_space {
                    MeshTransformSpace::World => "World",
                    MeshTransformSpace::Local => "Local",
                    MeshTransformSpace::Normal => "Normal",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.mesh_space, MeshTransformSpace::World, "World");
                    ui.selectable_value(&mut self.mesh_space, MeshTransformSpace::Local, "Local");
                    ui.selectable_value(&mut self.mesh_space, MeshTransformSpace::Normal, "Normal");
                });
            egui::ComboBox::from_id_salt("mesh_transform_axis")
                .selected_text(["X", "Y", "Z"][self.mesh_axis.min(2)])
                .show_ui(ui, |ui| {
                    for (i, label) in ["X", "Y", "Z"].iter().enumerate() {
                        ui.selectable_value(&mut self.mesh_axis, i, *label);
                    }
                });
        });
        ui.horizontal(|ui| {
            let label = match self.mesh_transform {
                MeshTransformMode::Move => "Distance",
                MeshTransformMode::Rotate => "Degrees",
                MeshTransformMode::Scale => "Scale delta",
            };
            ui.add(
                egui::DragValue::new(&mut self.mesh_value)
                    .speed(0.01)
                    .prefix(format!("{label} ")),
            );
            if ui.button("Apply Transform").clicked() {
                action = Some(MeshAction::Transform);
            }
        });

        ui.separator();
        ui.label("Topology tools");
        let face_tools =
            self.mesh_select_mode == MeshSelectMode::Polygons && !self.mesh_selected.is_empty();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(face_tools, egui::Button::new("Extrude"))
                .clicked()
            {
                action = Some(MeshAction::Extrude);
            }
            if ui
                .add_enabled(face_tools, egui::Button::new("Inset"))
                .clicked()
            {
                action = Some(MeshAction::Inset);
            }
            if ui
                .add_enabled(face_tools, egui::Button::new("Bevel"))
                .clicked()
            {
                action = Some(MeshAction::Bevel);
            }
            ui.checkbox(&mut self.mesh_extrude_individual, "Individual");
        });
        ui.horizontal(|ui| {
            ui.label("Bevel depth");
            ui.add(egui::DragValue::new(&mut self.mesh_depth).speed(0.005));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Ring cut");
            egui::ComboBox::from_id_salt("mesh_ring_axis")
                .selected_text(["X", "Y", "Z"][self.mesh_axis.min(2)])
                .show_ui(ui, |ui| {
                    for (i, label) in ["X", "Y", "Z"].iter().enumerate() {
                        ui.selectable_value(&mut self.mesh_axis, i, *label);
                    }
                });
            ui.add(
                egui::DragValue::new(&mut self.mesh_offset)
                    .speed(0.01)
                    .prefix("Offset "),
            );
            if ui.button("Cut").clicked() {
                action = Some(MeshAction::RingCut);
            }
        });
        ui.horizontal_wrapped(|ui| {
            let can_weld =
                self.mesh_select_mode == MeshSelectMode::Points && self.mesh_selected.len() >= 2;
            if ui
                .add_enabled(can_weld, egui::Button::new("Weld Selected Points"))
                .clicked()
            {
                action = Some(MeshAction::Weld);
            }
            let can_bridge =
                self.mesh_select_mode == MeshSelectMode::Edges && !self.mesh_selected.is_empty();
            if ui
                .add_enabled(can_bridge, egui::Button::new("Bridge Selected Loops"))
                .clicked()
            {
                action = Some(MeshAction::Bridge);
            }
        });
        if self.mesh_select_mode == MeshSelectMode::Polygons && !self.mesh_selected.is_empty() {
            ui.separator();
            let material_name = self
                .selected_material
                .and_then(|id| self.doc.materials.get(id))
                .map(|material| material.name.clone())
                .unwrap_or_else(|| "selected material".into());
            ui.horizontal_wrapped(|ui| {
                ui.label("Polygon material");
                if ui
                    .button(format!("Assign {material_name} to selection"))
                    .clicked()
                {
                    action = Some(MeshAction::AssignMaterial);
                }
            });
            ui.label(
                egui::RichText::new("Choose the material in the Materials tab first.")
                    .small()
                    .weak(),
            );
        }
        if let Some(action) = action {
            self.perform_mesh_action(action);
        }
    }
}

struct ObjectRow {
    id: NodeId,
    depth: usize,
    has_children: bool,
    name: String,
    icon: Icon,
    visible: bool,
    renderable: bool,
    locked: bool,
    is_modifier: bool,
}

fn primitive_icon(primitive: &Primitive) -> Icon {
    match primitive {
        Primitive::Box { .. } => Icon::Box,
        Primitive::Sphere { .. } => Icon::Ball,
        Primitive::Cylinder { .. } => Icon::Cylinder,
        Primitive::Cone { .. } => Icon::Cone,
        Primitive::Torus { .. } => Icon::Torus,
        Primitive::Plane { .. } => Icon::Plane,
    }
}

fn node_icon(kind: &NodeKind) -> Icon {
    match kind {
        NodeKind::Mesh { geometry } => match geometry {
            GeometrySource::Primitive { primitive } => primitive_icon(primitive),
            GeometrySource::Editable { origin, .. } => origin
                .as_ref()
                .map(primitive_icon)
                .unwrap_or(Icon::EditableMesh),
        },
        NodeKind::Spline { .. } => Icon::Spline,
        NodeKind::Camera { .. } => Icon::Camera,
        NodeKind::Light { light } => match light.kind {
            LightKind::Directional => Icon::DirectionalLight,
            LightKind::Point { .. } => Icon::PointLight,
            LightKind::Area { .. } => Icon::AreaLight,
        },
        NodeKind::Joint { .. } => Icon::Joint,
        NodeKind::Null => Icon::Folder,
    }
}

impl LeopardApp {
    // -----------------------------------------------------------------------
    // Right-hand column: inspector tabs and browser tabs
    // -----------------------------------------------------------------------

    pub(super) fn ui_inspector(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Properties")
                    .size(11.5)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::icon_button_toggle(
                    ui,
                    Icon::Render,
                    "Render settings",
                    self.inspector_tab == InspectorTab::Render,
                    true,
                )
                .clicked()
                {
                    self.inspector_tab = InspectorTab::Render;
                }
                if icons::icon_button_toggle(
                    ui,
                    Icon::Material,
                    "Material properties",
                    self.inspector_tab == InspectorTab::Material,
                    true,
                )
                .clicked()
                {
                    self.inspector_tab = InspectorTab::Material;
                }
                if icons::icon_button_toggle(
                    ui,
                    Icon::ObjectMode,
                    "Object properties",
                    self.inspector_tab == InspectorTab::Object,
                    true,
                )
                .clicked()
                {
                    self.inspector_tab = InspectorTab::Object;
                }
            });
        });
        theme::rule(ui);
        ui.add_space(3.0);
        match self.inspector_tab {
            InspectorTab::Object => self.ui_properties(ui),
            InspectorTab::Material => self.ui_material_inspector(ui),
            InspectorTab::Render => self.ui_render_settings(ui),
        }
    }

    fn ui_material_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected_material else {
            theme::empty(ui, "Select a material in the shelf below");
            return;
        };
        let Some(material) = self.doc.materials.get(id) else {
            theme::empty(ui, "Select a material in the shelf below");
            return;
        };
        let mut open_nodes = false;
        ui.horizontal(|ui| {
            icons::paint(
                ui.painter(),
                Rect::from_min_size(ui.cursor().min + Vec2::new(0.0, 2.0), Vec2::splat(15.0)),
                Icon::Material,
                theme::TEXT,
            );
            ui.add_space(19.0);
            ui.label(egui::RichText::new(&material.name).size(11.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Node Editor").clicked() {
                    open_nodes = true;
                }
            });
        });
        if open_nodes {
            self.editor_view = EditorView::Nodes;
        }
        theme::rule(ui);
        ui.add_space(2.0);
        self.material_editor(ui, id);
    }

    pub(super) fn ui_browser(&mut self, ui: &mut egui::Ui) {
        theme::tabs(
            ui,
            &mut self.browser_tab,
            &[
                ("Objects", BrowserTab::Objects),
                ("Takes", BrowserTab::Takes),
                ("Layers", BrowserTab::Layers),
                ("Console", BrowserTab::Console),
            ],
        );
        ui.add_space(1.0);
        theme::rule(ui);
        ui.add_space(2.0);
        match self.browser_tab {
            BrowserTab::Objects => self.ui_objects(ui),
            BrowserTab::Takes => self.ui_takes(ui),
            BrowserTab::Layers => self.ui_layers(ui),
            BrowserTab::Console => self.ui_console(ui),
        }
    }

    fn object_rows(&self) -> Vec<ObjectRow> {
        let filter = self.hierarchy_filter.to_ascii_lowercase();
        let mut rows = Vec::new();
        for id in self.doc.scene.depth_first() {
            let Some(node) = self.doc.scene.get(id) else {
                continue;
            };
            if !object_matches_filter(&node.name, &filter) {
                continue;
            }
            let chain = self.doc.scene.parent_chain(id);
            if chain.iter().any(|parent| self.collapsed.contains(parent)) {
                continue;
            }
            rows.push(ObjectRow {
                id,
                depth: chain.len(),
                has_children: !node.children.is_empty(),
                name: node.name.clone(),
                icon: node_icon(&node.kind),
                visible: node.visible,
                renderable: node.renderable,
                locked: node.locked,
                is_modifier: false,
            });
            for modifier in &node.modifiers {
                rows.push(ObjectRow {
                    id,
                    depth: chain.len() + 1,
                    has_children: false,
                    name: modifier.label().to_string(),
                    icon: if modifier_is_boolean(modifier) {
                        Icon::Nodes
                    } else {
                        Icon::Subdivide
                    },
                    visible: node.visible,
                    renderable: node.renderable,
                    locked: node.locked,
                    is_modifier: true,
                });
            }
        }
        rows
    }

    fn ui_objects(&mut self, ui: &mut egui::Ui) {
        let rows = self.object_rows();
        if rows.is_empty() {
            theme::empty(ui, "No matching objects");
            return;
        }
        let mut select = None;
        let mut frame = false;
        let mut toggle_visible = None;
        let mut toggle_renderable = None;
        let mut toggle_locked = None;
        let mut toggle_collapsed = None;
        let mut duplicate = None;
        let mut delete = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (index, row) in rows.iter().enumerate() {
                    let selected = self.selected == Some(row.id) && !row.is_modifier;
                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width().max(40.0), 19.0),
                        Sense::click(),
                    );
                    let painter = ui.painter_at(rect);
                    if selected {
                        painter.rect_filled(rect, 0.0, theme::SELECTION);
                    } else if row.is_modifier {
                        painter.rect_filled(rect, 0.0, Color32::from_rgb(233, 233, 233));
                    } else if response.hovered() {
                        painter.rect_filled(rect, 0.0, Color32::from_rgb(246, 247, 249));
                    }
                    let left = rect.left() + 4.0 + row.depth as f32 * 12.0;
                    let text_color = if selected {
                        Color32::WHITE
                    } else if row.is_modifier {
                        theme::TEXT_WEAK
                    } else {
                        theme::TEXT
                    };
                    if row.has_children && !row.is_modifier {
                        let collapsed = self.collapsed.contains(&row.id);
                        let c = Pos2::new(left + 4.0, rect.center().y);
                        let tri = if collapsed {
                            vec![
                                c + Vec2::new(-2.0, -4.0),
                                c + Vec2::new(3.0, 0.0),
                                c + Vec2::new(-2.0, 4.0),
                            ]
                        } else {
                            vec![
                                c + Vec2::new(-4.0, -2.0),
                                c + Vec2::new(4.0, -2.0),
                                c + Vec2::new(0.0, 3.0),
                            ]
                        };
                        painter.add(egui::Shape::convex_polygon(
                            tri,
                            if selected {
                                Color32::WHITE
                            } else {
                                theme::TEXT_WEAK
                            },
                            egui::Stroke::NONE,
                        ));
                    }
                    let dot = Pos2::new(left + 16.0, rect.center().y);
                    if row.is_modifier {
                        painter.line_segment(
                            [
                                Pos2::new(left + 2.0, rect.top()),
                                Pos2::new(left + 2.0, rect.center().y),
                            ],
                            egui::Stroke::new(1.0, Color32::from_rgb(180, 180, 180)),
                        );
                        painter.line_segment(
                            [
                                Pos2::new(left + 2.0, rect.center().y),
                                Pos2::new(left + 9.0, rect.center().y),
                            ],
                            egui::Stroke::new(1.0, Color32::from_rgb(180, 180, 180)),
                        );
                    } else {
                        painter.circle_filled(
                            dot,
                            3.1,
                            if row.visible {
                                Color32::from_rgb(104, 163, 84)
                            } else {
                                Color32::from_rgb(178, 178, 178)
                            },
                        );
                        painter.circle_stroke(
                            dot,
                            3.1,
                            egui::Stroke::new(1.0, Color32::from_rgb(80, 110, 70)),
                        );
                    }
                    let icon_rect = Rect::from_center_size(
                        Pos2::new(left + 32.0, rect.center().y),
                        Vec2::splat(13.0),
                    );
                    icons::paint(
                        &painter,
                        icon_rect,
                        row.icon,
                        if selected {
                            Color32::WHITE
                        } else {
                            theme::TEXT
                        },
                    );
                    painter.text(
                        Pos2::new(left + 44.0, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        &row.name,
                        egui::FontId::proportional(if row.is_modifier { 10.0 } else { 11.0 }),
                        text_color,
                    );
                    if !row.is_modifier {
                        let render_rect = Rect::from_center_size(
                            Pos2::new(rect.right() - 30.0, rect.center().y),
                            Vec2::splat(13.0),
                        );
                        icons::paint(
                            &painter,
                            render_rect,
                            if row.renderable {
                                Icon::Camera
                            } else {
                                Icon::RenderOff
                            },
                            if selected {
                                Color32::WHITE
                            } else {
                                theme::TEXT_WEAK
                            },
                        );
                        let lock_rect = Rect::from_center_size(
                            Pos2::new(rect.right() - 12.0, rect.center().y),
                            Vec2::splat(12.0),
                        );
                        icons::paint(
                            &painter,
                            lock_rect,
                            if row.locked { Icon::Lock } else { Icon::Unlock },
                            if selected {
                                Color32::WHITE
                            } else {
                                theme::TEXT_WEAK
                            },
                        );
                    }

                    let mut handled = false;
                    if row.has_children && !row.is_modifier {
                        let hit = Rect::from_center_size(
                            Pos2::new(left + 4.0, rect.center().y),
                            Vec2::splat(14.0),
                        );
                        if ui
                            .interact(hit, ui.id().with(("expand", index, row.id)), Sense::click())
                            .clicked()
                        {
                            toggle_collapsed = Some(row.id);
                            handled = true;
                        }
                    }
                    if !row.is_modifier {
                        let dot_hit = Rect::from_center_size(dot, Vec2::splat(14.0));
                        if ui
                            .interact(
                                dot_hit,
                                ui.id().with(("eye", index, row.id)),
                                Sense::click(),
                            )
                            .clicked()
                        {
                            toggle_visible = Some(row.id);
                            handled = true;
                        }
                        let render_hit = Rect::from_center_size(
                            Pos2::new(rect.right() - 30.0, rect.center().y),
                            Vec2::splat(14.0),
                        );
                        if ui
                            .interact(
                                render_hit,
                                ui.id().with(("renderable", index, row.id)),
                                Sense::click(),
                            )
                            .clicked()
                        {
                            toggle_renderable = Some(row.id);
                            handled = true;
                        }
                        let lock_hit = Rect::from_center_size(
                            Pos2::new(rect.right() - 12.0, rect.center().y),
                            Vec2::splat(14.0),
                        );
                        if ui
                            .interact(
                                lock_hit,
                                ui.id().with(("locked", index, row.id)),
                                Sense::click(),
                            )
                            .clicked()
                        {
                            toggle_locked = Some(row.id);
                            handled = true;
                        }
                    }
                    if response.double_clicked() {
                        select = Some(row.id);
                        frame = true;
                    } else if response.clicked() && !handled {
                        select = Some(row.id);
                    }
                    if selected && self.reveal_selected_in_browser {
                        response.scroll_to_me(Some(egui::Align::Center));
                        self.reveal_selected_in_browser = false;
                    }
                    response.context_menu(|ui| {
                        if ui.button("Select").clicked() {
                            select = Some(row.id);
                        }
                        if ui.button("Duplicate").clicked() {
                            duplicate = Some(row.id);
                        }
                        if ui.button("Delete").clicked() {
                            delete = Some(row.id);
                        }
                        if ui.button("Frame Selected").clicked() {
                            select = Some(row.id);
                            frame = true;
                        }
                    });
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!(
                        "{} objects · {} mesh objects",
                        self.doc.scene.len(),
                        self.doc.mesh_node_count()
                    ))
                    .size(9.5)
                    .color(theme::TEXT_WEAK),
                );
            });

        if let Some(id) = toggle_collapsed {
            if !self.collapsed.remove(&id) {
                self.collapsed.insert(id);
            }
        }
        if let Some(id) = toggle_visible {
            if let Some(node) = self.doc.scene.get(id).cloned() {
                let before = NodeProps::of(&node);
                let mut after = before.clone();
                after.visible = !before.visible;
                self.history.execute(
                    &mut self.doc,
                    Box::new(SetNodePropsCmd { id, before, after }),
                );
            }
        }
        if let Some(id) = toggle_renderable {
            if let Some(node) = self.doc.scene.get(id).cloned() {
                let before = NodeProps::of(&node);
                let mut after = before.clone();
                after.renderable = !before.renderable;
                self.history.execute(
                    &mut self.doc,
                    Box::new(SetNodePropsCmd { id, before, after }),
                );
            }
        }
        if let Some(id) = toggle_locked {
            if let Some(node) = self.doc.scene.get(id).cloned() {
                let before = NodeProps::of(&node);
                let mut after = before.clone();
                after.locked = !before.locked;
                self.history.execute(
                    &mut self.doc,
                    Box::new(SetNodePropsCmd { id, before, after }),
                );
            }
        }
        if let Some(id) = select {
            self.selected = Some(id);
            self.mesh_edit = false;
            self.mesh_selected.clear();
            self.sync_selection_material();
            self.status = format!(
                "Selected {}",
                self.doc
                    .scene
                    .get(id)
                    .map(|node| node.name.clone())
                    .unwrap_or_default()
            );
            if frame {
                self.frame_selected();
            }
        }
        if let Some(id) = duplicate {
            self.selected = Some(id);
            self.duplicate_selected();
        }
        if let Some(id) = delete {
            self.selected = Some(id);
            self.delete_selected();
        }
    }

    fn ui_layers(&mut self, ui: &mut egui::Ui) {
        let before = self.doc.settings.clone();
        let mut changed = false;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (index, layer) in self.doc.settings.layers.iter_mut().enumerate() {
                    let count = self
                        .doc
                        .scene
                        .nodes
                        .values()
                        .filter(|node| node.layer as usize == index)
                        .count();
                    ui.horizontal(|ui| {
                        changed |= ui.checkbox(&mut layer.visible, "").changed();
                        changed |= ui.color_edit_button_srgb(&mut layer.color).changed();
                        ui.label(egui::RichText::new(&layer.name).size(11.0).color(
                            if layer.visible {
                                theme::TEXT
                            } else {
                                theme::TEXT_WEAK
                            },
                        ));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!("{count}"))
                                    .size(9.5)
                                    .color(theme::TEXT_WEAK),
                            );
                        });
                    });
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Assign objects to layers in the Properties panel.")
                        .size(9.5)
                        .color(theme::TEXT_WEAK),
                );
            });
        if changed {
            let after = self.doc.settings.clone();
            self.history.execute(
                &mut self.doc,
                Box::new(lc_scene::SetSettingsCmd { before, after }),
            );
            self.status = "Updated layer visibility".into();
        }
    }

    fn ui_takes(&mut self, ui: &mut egui::Ui) {
        let takes: Vec<(NodeId, String, usize, f32, f32)> = self
            .doc
            .scene
            .depth_first()
            .into_iter()
            .filter_map(|id| {
                let node = self.doc.scene.get(id)?;
                if !node.has_animation() {
                    return None;
                }
                let keys: usize = node
                    .animation
                    .tracks
                    .iter()
                    .map(|t| t.curve.keys.len())
                    .sum();
                let times = node.animation.key_times();
                Some((
                    id,
                    node.name.clone(),
                    keys,
                    times.first().copied().unwrap_or(0.0) * self.playback.fps,
                    times.last().copied().unwrap_or(0.0) * self.playback.fps,
                ))
            })
            .collect();
        if takes.is_empty() {
            theme::empty(
                ui,
                "No animated objects yet\nKey transforms to create a take",
            );
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (id, name, keys, first, last) in &takes {
                    let selected = self.selected == Some(*id);
                    let (rect, response) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
                    let painter = ui.painter_at(rect);
                    if selected {
                        painter.rect_filled(rect, 0.0, theme::SELECTION);
                    } else if response.hovered() {
                        painter.rect_filled(rect, 0.0, Color32::from_rgb(246, 247, 249));
                    }
                    let color = if selected {
                        Color32::WHITE
                    } else {
                        theme::TEXT
                    };
                    painter.text(
                        rect.left_top() + Vec2::new(6.0, 3.0),
                        egui::Align2::LEFT_TOP,
                        name,
                        egui::FontId::proportional(11.0),
                        color,
                    );
                    painter.text(
                        rect.left_bottom() + Vec2::new(6.0, -3.0),
                        egui::Align2::LEFT_BOTTOM,
                        format!("{keys} keys · frames {first:.0}–{last:.0}"),
                        egui::FontId::proportional(9.5),
                        if selected {
                            Color32::from_rgb(220, 232, 250)
                        } else {
                            theme::TEXT_WEAK
                        },
                    );
                    if response.clicked() {
                        self.selected = Some(*id);
                        self.inspector_tab = InspectorTab::Object;
                        self.mesh_edit = false;
                        self.sync_selection_material();
                        self.status = format!("Selected {name}");
                    }
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Takes are stored per object in this build.")
                        .size(9.5)
                        .color(theme::TEXT_WEAK),
                );
            });
    }

    fn ui_console(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{} messages", self.console.len()))
                    .size(9.5)
                    .color(theme::TEXT_WEAK),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Clear").clicked() {
                    self.console.clear();
                }
            });
        });
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &self.console {
                    ui.label(
                        egui::RichText::new(line)
                            .size(9.5)
                            .monospace()
                            .color(theme::TEXT),
                    );
                }
            });
    }
}

fn modifier_is_boolean(modifier: &Modifier) -> bool {
    matches!(modifier, Modifier::Boolean { .. })
}

fn object_matches_filter(name: &str, filter: &str) -> bool {
    filter.is_empty()
        || name
            .to_ascii_lowercase()
            .contains(&filter.to_ascii_lowercase())
}

#[cfg(test)]
mod browser_filter_tests {
    use super::object_matches_filter;

    #[test]
    fn search_filter_matches_names_case_insensitively() {
        assert!(object_matches_filter("Camera", "cam"));
        assert!(object_matches_filter("Camera", "CAM"));
        assert!(object_matches_filter("Cube", ""));
        assert!(!object_matches_filter("Cube", "cam"));
    }
}

#[cfg(test)]
mod node_icon_tests {
    use super::*;

    fn primitive_kind(primitive: Primitive) -> NodeKind {
        NodeKind::Mesh {
            geometry: GeometrySource::from_primitive(primitive),
        }
    }

    #[test]
    fn every_primitive_gets_its_own_icon() {
        let cases = [
            (
                Primitive::Box {
                    width: 1.0,
                    height: 1.0,
                    depth: 1.0,
                },
                Icon::Box,
            ),
            (
                Primitive::Sphere {
                    radius: 1.0,
                    segments: 12,
                    rings: 6,
                },
                Icon::Ball,
            ),
            (
                Primitive::Cylinder {
                    radius: 0.7,
                    height: 2.0,
                    segments: 12,
                },
                Icon::Cylinder,
            ),
            (
                Primitive::Cone {
                    radius: 0.8,
                    height: 2.0,
                    segments: 12,
                },
                Icon::Cone,
            ),
            (
                Primitive::Torus {
                    major_radius: 1.0,
                    minor_radius: 0.2,
                    segments: 12,
                    rings: 6,
                },
                Icon::Torus,
            ),
            (
                Primitive::Plane {
                    width: 2.0,
                    depth: 2.0,
                    segments_x: 1,
                    segments_z: 1,
                },
                Icon::Plane,
            ),
        ];
        for (primitive, expected) in cases {
            assert_eq!(node_icon(&primitive_kind(primitive)), expected);
        }
    }

    #[test]
    fn editable_meshes_keep_primitive_icons_and_have_a_fallback() {
        let converted = NodeKind::Mesh {
            geometry: GeometrySource::Editable {
                mesh: lc_geom::Mesh::new("Converted"),
                origin: Some(Primitive::Sphere {
                    radius: 1.0,
                    segments: 12,
                    rings: 6,
                }),
            },
        };
        assert_eq!(node_icon(&converted), Icon::Ball);

        let imported = NodeKind::Mesh {
            geometry: GeometrySource::Editable {
                mesh: lc_geom::Mesh::new("Imported"),
                origin: None,
            },
        };
        assert_eq!(node_icon(&imported), Icon::EditableMesh);
    }

    #[test]
    fn light_kinds_are_visually_distinct() {
        let light = |kind| NodeKind::Light {
            light: Light {
                kind,
                ..Default::default()
            },
        };
        assert_eq!(
            node_icon(&light(LightKind::Directional)),
            Icon::DirectionalLight
        );
        assert_eq!(
            node_icon(&light(LightKind::Point { radius: 1.0 })),
            Icon::PointLight
        );
        assert_eq!(
            node_icon(&light(LightKind::Area {
                width: 1.0,
                height: 1.0,
            })),
            Icon::AreaLight
        );
    }
}
