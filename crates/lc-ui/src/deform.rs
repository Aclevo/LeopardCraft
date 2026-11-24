//! Modifier stack, skin binding, weight painting and morph target workflows.

use super::*;

impl LeopardApp {
    /// Whether the inspector's modifier stack can meaningfully add this
    /// modifier to the primary object. The handler that pushes it calls the
    /// same predicate.
    pub(super) fn modifier_ready(&self, modifier: &Modifier) -> bool {
        let Some(node) = self
            .selection
            .primary()
            .and_then(|id| self.doc.scene.get(id))
        else {
            return false;
        };
        if !modifier_applies_to(node, modifier) {
            return false;
        }
        if let Modifier::Formula { x, y, z } = modifier {
            return [x, y, z]
                .iter()
                .all(|expression| lc_geom::formula_expression_is_valid(expression));
        }
        let operand = match modifier {
            Modifier::Boolean { operand, .. } => Some(*operand),
            Modifier::Wrap { target, .. } => Some(*target),
            _ => None,
        };
        if let Some(operand) = operand {
            return operand != node.id
                && self
                    .doc
                    .scene
                    .get(operand)
                    .map(|operand| operand.is_mesh())
                    .unwrap_or(false);
        }
        true
    }

    pub(super) fn add_modifier_to_primary(&mut self, modifier: Modifier) -> bool {
        let Some(id) = self.selection.primary() else {
            self.status = "Select an object before adding a modifier".into();
            return false;
        };
        if !self.modifier_ready(&modifier) {
            self.status = format!("{} needs a compatible object", modifier.label());
            return false;
        }
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        let before = Shape::of(node);
        let mut after = before.clone();
        let label = modifier.label();
        after.modifiers.push(modifier);
        self.history
            .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
        self.status = format!("Added {label}");
        true
    }

    pub(super) fn modifier_remove_ready(&self, index: usize) -> bool {
        self.selection
            .primary()
            .and_then(|id| self.doc.scene.get(id))
            .map(|node| index < node.modifiers.len())
            .unwrap_or(false)
    }

    pub(super) fn remove_modifier_from_primary(&mut self, index: usize) -> bool {
        let Some(id) = self.selection.primary() else {
            return false;
        };
        if !self.modifier_remove_ready(index) {
            self.status = "That modifier no longer exists".into();
            return false;
        }
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        let before = Shape::of(node);
        let mut after = before.clone();
        let label = after.modifiers.remove(index).label();
        self.history
            .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
        self.status = format!("Removed {label}");
        true
    }

    pub(super) fn all_joint_ids(&self) -> Vec<NodeId> {
        self.doc
            .scene
            .depth_first()
            .into_iter()
            .filter(|id| {
                matches!(
                    self.doc.scene.get(*id).map(|node| &node.kind),
                    Some(NodeKind::Joint { .. })
                )
            })
            .collect()
    }

    /// The joints selected for binding. `None` means every joint in the scene.
    pub(super) fn bind_joint_ids(&self) -> Vec<NodeId> {
        self.all_joint_ids()
            .into_iter()
            .filter(|id| {
                self.bind_joints
                    .as_ref()
                    .is_none_or(|joints| joints.contains(id))
            })
            .collect()
    }

    pub(super) fn set_all_bind_joints(&mut self, all: bool) {
        self.bind_joints = if all { None } else { Some(BTreeSet::new()) };
    }

    pub(super) fn set_bind_joint(&mut self, id: NodeId, enabled: bool) {
        let all = self.all_joint_ids();
        let mut joints = self
            .bind_joints
            .clone()
            .unwrap_or_else(|| all.iter().copied().collect());
        if enabled {
            joints.insert(id);
        } else {
            joints.remove(&id);
        }
        self.bind_joints = if joints.len() == all.len() {
            None
        } else {
            Some(joints)
        };
    }

    /// The evaluated mesh and chosen joints a bind operation would use.
    pub(super) fn skin_target(&self) -> Option<SkinTarget> {
        let id = self.selection.primary()?;
        let node = self.doc.scene.get(id)?;
        if !node.is_mesh() {
            return None;
        }
        let evaluated = self.evaluate();
        let vertex_count = evaluated
            .node(id)
            .and_then(|node| node.mesh())
            .map(|mesh| mesh.positions.len())
            .unwrap_or(0);
        let positions = evaluated
            .node(id)
            .and_then(|node| node.mesh())
            .map(|mesh| mesh.positions.clone())
            .unwrap_or_default();
        let mesh_inverse = evaluated
            .node(id)
            .map(|node| node.world.inverse())
            .unwrap_or(Mat4::IDENTITY);
        let joints: Vec<(NodeId, Mat4, f32)> = self
            .bind_joint_ids()
            .into_iter()
            .filter_map(|joint_id| {
                let node = self.doc.scene.get(joint_id)?;
                let length = match &node.kind {
                    NodeKind::Joint { length, .. } => *length,
                    _ => return None,
                };
                let world = evaluated.node(joint_id)?.world;
                Some((joint_id, world, length.max(0.0)))
            })
            .collect();
        Some(SkinTarget {
            vertex_count,
            joints: joints.iter().map(|(joint, _, _)| *joint).collect(),
            inverse_bind: joints.iter().map(|(_, world, _)| world.inverse()).collect(),
            segments: joints
                .iter()
                .map(|(_, world, length)| {
                    (
                        mesh_inverse.transform_point3(world.transform_point3(Vec3::ZERO)),
                        mesh_inverse.transform_point3(world.transform_point3(Vec3::Y * *length)),
                    )
                })
                .collect(),
            positions,
            existing: node.skin.clone(),
        })
    }

    pub(super) fn build_bound_skin(&self, target: &SkinTarget) -> Option<Skin> {
        if target.vertex_count == 0 || target.joints.is_empty() {
            return None;
        }
        let mut skin = Skin::new(target.joints.clone(), target.inverse_bind.clone());
        skin.clear_weights(target.vertex_count);
        let auto: Option<Skin> = if self.bind_auto_weights {
            let mut auto = Skin::new(target.joints.clone(), target.inverse_bind.clone());
            if auto.auto_weight_proximity(
                &target.positions,
                &target.segments,
                ProximityWeightOptions {
                    dropoff: self.bind_dropoff,
                    strength: self.bind_strength,
                    max_influences: self.bind_max_influences,
                },
            ) {
                Some(auto)
            } else {
                None
            }
        } else {
            None
        };
        for vertex in 0..target.vertex_count {
            let mut carried: Vec<(usize, f32)> = Vec::new();
            if let Some(existing) = &target.existing {
                let old_indices = existing.joint_indices.get(vertex);
                let old_weights = existing.weights.get(vertex);
                if let (Some(old_indices), Some(old_weights)) = (old_indices, old_weights) {
                    for (old_index, weight) in old_indices.iter().zip(old_weights) {
                        if *weight <= 0.0 || !weight.is_finite() {
                            continue;
                        }
                        let Some(old_joint) = existing.joints.get(*old_index as usize) else {
                            continue;
                        };
                        let Some(new_index) =
                            target.joints.iter().position(|joint| joint == old_joint)
                        else {
                            continue;
                        };
                        if let Some((_, total)) =
                            carried.iter_mut().find(|(index, _)| *index == new_index)
                        {
                            *total += *weight;
                        } else {
                            carried.push((new_index, *weight));
                        }
                    }
                }
            }
            if !carried.is_empty() {
                for (slot, (index, weight)) in carried.into_iter().take(4).enumerate() {
                    skin.joint_indices[vertex][slot] = index as u32;
                    skin.weights[vertex][slot] = weight;
                }
                skin.compact_vertex(vertex);
                skin.normalize_vertex(vertex);
            } else if let Some(auto) = &auto {
                skin.joint_indices[vertex] = auto.joint_indices[vertex];
                skin.weights[vertex] = auto.weights[vertex];
            }
        }
        Some(skin)
    }

    pub(super) fn bind_skin_to_joints(&mut self) -> bool {
        let Some(id) = self.selection.primary() else {
            return false;
        };
        let Some(target) = self.skin_target() else {
            self.status = "Select a mesh to bind".into();
            return false;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        let Some(skin) = self.build_bound_skin(&target) else {
            self.status = "Choose at least one joint and a non-empty mesh first".into();
            return false;
        };
        let active_joint = self
            .weight_joint
            .and_then(|index| target.existing.as_ref()?.joints.get(index).copied());
        let next_weight_joint = active_joint.map(|joint| {
            skin.joints
                .iter()
                .position(|candidate| *candidate == joint)
                .unwrap_or(0)
        });
        if node.skin.as_ref() == Some(&skin) {
            self.status = "Skin already matches the selected joints and weights".into();
            return false;
        }
        let before = Shape::of(node);
        let mut after = before.clone();
        after.skin = Some(skin);
        self.history
            .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
        self.status = if target.existing.is_some() {
            "Rebound skin, preserving covered weights".into()
        } else {
            "Bound skin to the selected joints".into()
        };
        if active_joint.is_some() {
            self.weight_joint = next_weight_joint;
        }
        true
    }

    pub(super) fn unbind_skin_from_primary(&mut self) -> bool {
        let Some(id) = self.selection.primary() else {
            return false;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        if !skin_unbind_ready(node) {
            self.status = "This object has no skin to unbind".into();
            return false;
        }
        let before = Shape::of(node);
        let mut after = before.clone();
        after.skin = None;
        self.history
            .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
        self.status = "Unbound skin".into();
        true
    }

    pub(super) fn active_weight_joint(&self) -> Option<usize> {
        let id = self.selection.primary()?;
        let skin = self.doc.scene.get(id)?.skin.as_ref()?;
        if skin.joints.is_empty() {
            return None;
        }
        Some(self.weight_joint.unwrap_or(0).min(skin.joints.len() - 1))
    }

    /// Make `joint` the active skinning joint when it belongs to the primary
    /// mesh's bound skin. A joint that is not bound is refused without touching
    /// the document, history or the current active joint, so viewport picking
    /// cannot silently point the heat map and brush at the wrong bone.
    pub(super) fn set_active_weight_joint_node(&mut self, joint: lc_scene::NodeId) -> bool {
        let Some(id) = self.selection.primary() else {
            self.status = "Select a skinned mesh before picking a joint".into();
            return false;
        };
        let Some(skin) = self.doc.scene.get(id).and_then(|node| node.skin.as_ref()) else {
            self.status = "The selected object has no bound skin".into();
            return false;
        };
        let Some(index) = skin.joints.iter().position(|candidate| *candidate == joint) else {
            self.status = "That joint is not bound to the selected mesh".into();
            return false;
        };
        self.weight_joint = Some(index);
        let name = self
            .doc
            .scene
            .get(joint)
            .map(|node| node.name.clone())
            .unwrap_or_else(|| format!("Joint {joint}"));
        self.status = format!("Active joint: {name}");
        true
    }

    pub(super) fn weight_paint_ready(&self) -> bool {
        let Some((id, mesh)) = self.selected_editable_mesh() else {
            return false;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        let Some(skin) = node.skin.as_ref() else {
            return false;
        };
        node.modifiers.is_empty()
            && skin.vertex_count() == mesh.positions.len()
            && !skin.joints.is_empty()
    }

    pub(super) fn set_weight_paint(&mut self, enabled: bool) {
        if enabled && !self.weight_paint_ready() {
            self.status = "Weight painting needs a bound, modifier-free editable mesh".into();
            self.weight_paint = false;
            return;
        }
        self.weight_paint = enabled;
        if enabled {
            self.mesh_edit = false;
            self.clear_mesh_component_selection();
            self.gizmo_drag = None;
            self.status = "Weight paint mode".into();
        }
    }

    pub(super) fn begin_weight_stroke(&mut self) -> bool {
        if !self.weight_paint_ready() {
            return false;
        }
        let Some(id) = self.selection.primary() else {
            return false;
        };
        if self.pending_shape.is_none() {
            if let Some(node) = self.doc.scene.get(id) {
                self.pending_shape = Some((id, Shape::of(node)));
            }
        }
        true
    }

    pub(super) fn paint_weight_brush(
        &mut self,
        evaluated: &EvaluatedScene,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
        pointer: Pos2,
    ) -> bool {
        if !self.weight_paint_ready() {
            return false;
        }
        let Some(id) = self.selection.primary() else {
            return false;
        };
        let Some(joint) = self.active_weight_joint() else {
            return false;
        };
        let Some(node) = evaluated.node(id) else {
            return false;
        };
        let Some(mesh) = node.mesh() else {
            return false;
        };
        let Some(mut skin) = self.doc.scene.get(id).and_then(|node| node.skin.clone()) else {
            return false;
        };
        if skin.vertex_count() < mesh.positions.len() {
            return false;
        }
        let radius = self.weight_brush.radius.max(1.0);
        let strength = self.weight_brush.strength.clamp(0.0, 1.0);
        let mode = self.weight_brush.mode;
        let falloff = self.weight_brush.falloff;
        let before_weights: Vec<f32> = (0..mesh.positions.len())
            .map(|vertex| skin.influence(vertex, joint))
            .collect();
        let adjacency: Vec<Vec<u32>> = if mode == WeightBrushMode::Smooth {
            let mut adjacency = vec![Vec::new(); mesh.positions.len()];
            for edge in mesh.edges() {
                let a = edge.a as usize;
                let b = edge.b as usize;
                if a < adjacency.len() && b < adjacency.len() {
                    adjacency[a].push(edge.b);
                    adjacency[b].push(edge.a);
                }
            }
            adjacency
        } else {
            Vec::new()
        };
        let mut painted = 0usize;
        for (vertex, position) in mesh.positions.iter().enumerate() {
            let Some(screen) = self.project(
                node.world.transform_point3(*position),
                rect,
                pixel_w,
                pixel_h,
            ) else {
                continue;
            };
            let distance = (screen - pointer).length();
            if distance > radius {
                continue;
            }
            let factor = strength * falloff.factor(distance, radius);
            if factor <= 0.0 {
                continue;
            }
            let current = before_weights[vertex];
            let changed = match mode {
                WeightBrushMode::Add => skin.set_influence(vertex, joint, current + factor),
                WeightBrushMode::Subtract => skin.set_influence(vertex, joint, current - factor),
                WeightBrushMode::Replace => skin.set_exclusive_influence(vertex, joint, factor),
                WeightBrushMode::Smooth => {
                    let neighbours = &adjacency[vertex];
                    let average = if neighbours.is_empty() {
                        current
                    } else {
                        neighbours
                            .iter()
                            .map(|neighbour| before_weights[*neighbour as usize])
                            .sum::<f32>()
                            / neighbours.len() as f32
                    };
                    skin.set_influence(vertex, joint, current + (average - current) * factor)
                }
            };
            painted += usize::from(changed);
        }
        if painted == 0 {
            return false;
        }
        if let Some(node) = self.doc.scene.get_mut(id) {
            node.skin = Some(skin);
        }
        self.status = format!("Painted {painted} weight vertices");
        true
    }

    pub(super) fn perform_morph_action(&mut self, id: NodeId, action: MorphAction) {
        if self.selection.primary() != Some(id) {
            return;
        }
        match action {
            MorphAction::RecordBase => {
                self.record_morph_base();
            }
            MorphAction::Capture => {
                self.capture_morph_target();
            }
            MorphAction::UpdateBase => {
                self.update_morph_base();
            }
            MorphAction::Restore => {
                self.restore_morph_base();
            }
            MorphAction::Remove(index) => {
                self.remove_morph_target(index);
            }
        }
    }

    pub(super) fn record_morph_base(&mut self) -> bool {
        let Some((id, mesh)) = self.selected_editable_mesh() else {
            self.status = "Morph targets need an editable mesh".into();
            return false;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        if node.morph.has_base() || !node.morph.targets.is_empty() {
            self.status = "This mesh already has a morph base".into();
            return false;
        }
        if mesh.positions.is_empty() {
            self.status = "The mesh has no vertices to record".into();
            return false;
        }
        let before = node.morph.clone();
        let vertices = mesh.positions.len();
        self.history.execute(
            &mut self.doc,
            Box::new(SetMorphCmd {
                id,
                before,
                after: MorphSet {
                    base: mesh.positions,
                    targets: Vec::new(),
                },
            }),
        );
        self.status = format!("Recorded morph base ({vertices} vertices)");
        true
    }

    pub(super) fn capture_morph_target(&mut self) -> bool {
        let Some((id, mesh)) = self.selected_editable_mesh() else {
            self.status = "Morph targets need an editable mesh".into();
            return false;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        if !node.morph.has_base() {
            self.status = "Record a morph base before capturing a target".into();
            return false;
        }
        if node.morph.base.len() != mesh.positions.len() {
            self.status = "The morph base does not match the current mesh topology".into();
            return false;
        }
        let deltas: Vec<Vec3> = mesh
            .positions
            .iter()
            .zip(&node.morph.base)
            .map(|(position, base)| *position - *base)
            .collect();
        if deltas.iter().all(|delta| delta.length_squared() < 1e-10) {
            self.status = "Move mesh vertices before capturing a morph target".into();
            return false;
        }
        let name = node.morph.unique_target_name();
        let before = node.morph.clone();
        let mut after = before.clone();
        after.targets.push(MorphTarget::new(name.clone(), deltas));
        self.history
            .execute(&mut self.doc, Box::new(SetMorphCmd { id, before, after }));
        self.status = format!("Captured morph target {name}");
        true
    }

    /// Re-record the rest base from the live mesh while keeping every target's
    /// absolute posed shape. Deltas are stored relative to the base, so each
    /// target absorbs the difference between the old and new base. Cheetah3D
    /// documents the Base/rest workflow but not how targets follow a re-recorded
    /// base; preserving target poses is LeopardCraft's clean-room choice.
    pub(super) fn update_morph_base(&mut self) -> bool {
        let Some((id, mesh)) = self.selected_editable_mesh() else {
            self.status = "Morph targets need an editable mesh".into();
            return false;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        if !node.morph.has_base() {
            self.status = "Record a morph base before updating it".into();
            return false;
        }
        if node.morph.base.len() != mesh.positions.len() {
            self.status = "The morph base does not match the current mesh topology".into();
            return false;
        }
        if node
            .morph
            .targets
            .iter()
            .any(|target| target.deltas.len() != mesh.positions.len())
        {
            self.status = "A morph target does not match the current mesh topology".into();
            return false;
        }
        if node.morph.base == mesh.positions {
            self.status = "The morph base already matches the current mesh".into();
            return false;
        }
        let before = node.morph.clone();
        let old_base = before.base.clone();
        let mut after = before.clone();
        for target in &mut after.targets {
            for (index, delta) in target.deltas.iter_mut().enumerate() {
                *delta = old_base[index] + *delta - mesh.positions[index];
            }
        }
        let rebased = after.targets.len();
        after.base = mesh.positions.clone();
        let vertices = mesh.positions.len();
        self.history
            .execute(&mut self.doc, Box::new(SetMorphCmd { id, before, after }));
        self.status = format!(
            "Updated morph base ({vertices} vertices, {rebased} target{} rebased)",
            if rebased == 1 { "" } else { "s" }
        );
        true
    }

    pub(super) fn restore_morph_base(&mut self) -> bool {
        let Some((id, mesh)) = self.selected_editable_mesh() else {
            self.status = "Morph targets need an editable mesh".into();
            return false;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        if !node.morph.has_base() {
            self.status = "Record a morph base first".into();
            return false;
        }
        let mut positions = mesh.positions.clone();
        if !node.morph.restore_base(&mut positions) {
            self.status = "The morph base does not match the current mesh topology".into();
            return false;
        }
        if positions == mesh.positions {
            self.status = "The mesh is already at the morph base".into();
            return false;
        }
        let before = Shape::of(node);
        let mut after = before.clone();
        let NodeKind::Mesh {
            geometry: GeometrySource::Editable { mesh, .. },
        } = &mut after.kind
        else {
            return false;
        };
        mesh.positions = positions;
        self.history
            .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
        self.status = "Restored morph base".into();
        true
    }

    pub(super) fn remove_morph_target(&mut self, index: usize) -> bool {
        let Some((id, _)) = self.selected_editable_mesh() else {
            return false;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        let Some(removed) = node.morph.targets.get(index) else {
            self.status = "That morph target no longer exists".into();
            return false;
        };
        let name = removed.name.clone();
        let before = node.morph.clone();
        let mut after = before.clone();
        after.targets.remove(index);
        self.history
            .execute(&mut self.doc, Box::new(SetMorphCmd { id, before, after }));
        self.status = format!("Removed morph target {name}");
        true
    }

    pub(super) fn apply_node_edits(&mut self, id: NodeId, original: &Node, edited: &Node) {
        let Some(current) = self.doc.scene.get(id).cloned() else {
            return;
        };
        if edited.name != original.name {
            if let Some(cmd) = RenameNodeCmd::new(&self.doc, id, edited.name.clone()) {
                self.history.execute(&mut self.doc, Box::new(cmd));
            }
        }
        let before_props = NodeProps::of(&current);
        let after_props = NodeProps::of(edited);
        if before_props != after_props {
            self.history.execute(
                &mut self.doc,
                Box::new(SetNodePropsCmd {
                    id,
                    before: before_props,
                    after: after_props,
                }),
            );
        }
        let before_shape = Shape::of(&current);
        let after_shape = Shape::of(edited);
        if before_shape.kind != after_shape.kind
            || before_shape.modifiers != after_shape.modifiers
            || before_shape.skin != after_shape.skin
            || before_shape.morph != after_shape.morph
        {
            if self.pending_shape.is_none() {
                self.pending_shape = Some((id, before_shape.clone()));
            }
            if let Some(node) = self.doc.scene.get_mut(id) {
                node.kind = edited.kind.clone();
                node.modifiers = edited.modifiers.clone();
                node.skin = edited.skin.clone();
                node.morph = edited.morph.clone();
            }
        }
        if edited.transform != original.transform {
            let pending = self.pending_transform.get_or_insert_with(Vec::new);
            if !pending.iter().any(|(pending_id, _)| *pending_id == id) {
                pending.push((id, current.transform));
            }
            if let Some(node) = self.doc.scene.get_mut(id) {
                node.transform = edited.transform;
            }
        }
        self.sync_selection_material();
    }
}
