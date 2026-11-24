//! Headless, deterministic scene evaluation.
//!
//! Evaluation resolves animation, constraints, visibility, modifier stacks,
//! boolean dependencies and skinning into a flat, render-ready snapshot.
//! Nothing here touches the GPU or the clock.

use crate::{
    Camera, Document, Environment, Light, Modifier, ModifierInput, NodeId, NodeKind,
    RenderSettings, Tag,
};
use glam::{Mat4, Quat, Vec3};
use lc_anim::{skin_positions_indexed, AnimChannel, ChannelSamples, ChannelTarget};
use lc_core::{look_at_quat, Aabb, StableHasher, Transform};
use lc_geom::{Mesh, Spline};
use lc_shading::MaterialId;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Debug, Default)]
pub struct EvaluatedGeometry {
    pub mesh: Option<Mesh>,
    pub spline: Option<Spline>,
}

#[derive(Clone, Debug)]
pub enum EvaluatedKind {
    Null,
    Mesh {
        mesh: Option<Mesh>,
        spline: Option<Spline>,
    },
    Camera(Camera),
    Light(Light),
    Joint {
        length: f32,
        width: f32,
    },
}

#[derive(Clone, Debug)]
pub struct EvaluatedNode {
    pub id: NodeId,
    pub name: String,
    pub parent: Option<NodeId>,
    pub local: Transform,
    pub world: Mat4,
    pub world_transform: Transform,
    pub visible: bool,
    pub rendered: bool,
    pub casts_shadow: bool,
    pub layer: u8,
    pub kind: EvaluatedKind,
    pub material: Option<MaterialId>,
    pub material_slots: BTreeMap<u16, MaterialId>,
}

impl EvaluatedNode {
    pub fn mesh(&self) -> Option<&Mesh> {
        match &self.kind {
            EvaluatedKind::Mesh { mesh, .. } => mesh.as_ref(),
            _ => None,
        }
    }

    pub fn spline(&self) -> Option<&Spline> {
        match &self.kind {
            EvaluatedKind::Mesh { spline, .. } => spline.as_ref(),
            _ => None,
        }
    }

    pub fn material_for_slot(&self, slot: u16) -> Option<MaterialId> {
        self.material_slots.get(&slot).copied().or(self.material)
    }
}

#[derive(Clone, Debug)]
pub struct EvaluatedCamera {
    pub node: NodeId,
    pub world: Mat4,
    pub camera: Camera,
}

#[derive(Clone, Debug)]
pub struct EvaluatedLight {
    pub node: NodeId,
    pub world: Mat4,
    pub light: Light,
    pub casts_shadow: bool,
}

#[derive(Clone, Debug)]
pub struct EvaluatedScene {
    pub time: f32,
    pub nodes: Vec<EvaluatedNode>,
    pub by_id: BTreeMap<NodeId, usize>,
    pub active_camera: Option<EvaluatedCamera>,
    pub lights: Vec<EvaluatedLight>,
    pub environment: Environment,
    pub settings: RenderSettings,
    pub background: Vec3,
    pub bounds: Aabb,
}

impl EvaluatedScene {
    pub fn node(&self, id: NodeId) -> Option<&EvaluatedNode> {
        self.by_id.get(&id).and_then(|&i| self.nodes.get(i))
    }

    pub fn mesh_nodes(&self) -> impl Iterator<Item = &EvaluatedNode> {
        self.nodes
            .iter()
            .filter(|n| n.rendered && n.mesh().is_some())
    }

    pub fn triangle_count(&self) -> usize {
        self.mesh_nodes()
            .filter_map(|n| n.mesh())
            .map(|m| m.triangle_count())
            .sum()
    }

    pub fn signature(&self) -> u64 {
        let mut h = StableHasher::new();
        for n in &self.nodes {
            h.write_u64(n.id);
            h.write_u8(n.rendered as u8);
            for v in n.world.to_cols_array() {
                h.write_f32(v);
            }
            if let Some(mesh) = n.mesh() {
                h.write_u64(mesh.positions.len() as u64);
                for p in &mesh.positions {
                    h.write_vec3(*p);
                }
            }
            if let Some(cam) = &self.active_camera {
                if cam.node == n.id {
                    h.write_f32(cam.camera.fov_y_deg);
                }
            }
        }
        h.finish()
    }
}

/// Normalise `ClampRotation` bounds so evaluation cannot panic on a
/// hand-edited or imported scene: a reversed pair is swapped per axis and a
/// non-finite bound falls back to the full +-180 degree range on that axis.
/// `f32::clamp` panics when its minimum exceeds its maximum, so the evaluator
/// must never pass raw authored values through.
pub(crate) fn normalized_clamp_bounds(min: Vec3, max: Vec3) -> (Vec3, Vec3) {
    let mut low = min;
    let mut high = max;
    for axis in 0..3 {
        let (a, b) = (low[axis], high[axis]);
        if !a.is_finite() || !b.is_finite() {
            low[axis] = -180.0;
            high[axis] = 180.0;
        } else if a > b {
            low[axis] = b;
            high[axis] = a;
        }
    }
    (low, high)
}

/// Unit direction that stays finite for very large components, unlike a bare
/// `normalize` on hand-edited data.
fn unit_axis(v: Vec3) -> Option<Vec3> {
    if !v.is_finite() {
        return None;
    }
    let scale = v.abs().max_element();
    if scale <= 1e-6 {
        return None;
    }
    Some((v / scale).normalize())
}

impl Document {
    pub fn evaluate(&self, time: f32) -> EvaluatedScene {
        let time = if time.is_finite() { time.max(0.0) } else { 0.0 };
        let order = self.scene.depth_first();
        let mut locals: BTreeMap<NodeId, Transform> = BTreeMap::new();

        // 0. Sample every animated channel once per frame into a scratch buffer.
        // The constraint passes and the geometry stage then read these values
        // instead of re-querying curves component by component.
        let mut node_samples: BTreeMap<NodeId, ChannelSamples> = BTreeMap::new();
        let mut tag_samples: BTreeMap<(NodeId, u32), ChannelSamples> = BTreeMap::new();
        for id in &order {
            let Some(node) = self.scene.get(*id) else {
                continue;
            };
            if !node.animation.is_empty() {
                node_samples.insert(*id, node.animation.sample_channels(time));
            }
            for (index, _) in node.tags.iter().enumerate() {
                if let Some(clip) = node.tag_animations.get(&(index as u32)) {
                    tag_samples.insert((*id, index as u32), clip.sample_channels(time));
                }
            }
        }

        // 1. Animated local transforms, with authored joint limits applied to
        // both direct animation and the rest pose before any solver runs.
        for id in &order {
            let Some(node) = self.scene.get(*id) else {
                continue;
            };
            let mut local = match node_samples.get(id) {
                Some(samples) => {
                    samples.apply_transform(node.transform, ChannelTarget::Object { id: *id })
                }
                None => node.transform,
            };
            let _ = crate::ik::clamp_joint_local(&self.scene, *id, &mut local);
            locals.insert(*id, local);
        }

        // 2. World matrices with tag constraints. Two passes let constrained
        // targets settle without any frame-to-frame state.
        let mut worlds: BTreeMap<NodeId, Mat4> = BTreeMap::new();
        for id in &order {
            let Some(node) = self.scene.get(*id) else {
                continue;
            };
            let parent_world = node
                .parent
                .and_then(|p| worlds.get(&p).copied())
                .unwrap_or(Mat4::IDENTITY);
            let local = locals.get(id).copied().unwrap_or(node.transform);
            worlds.insert(*id, parent_world * local.matrix());
        }
        // Solve IK before the tag-constraint passes. The constraint loop below
        // rebuilds every world matrix from the updated local rotations, so the
        // geometry and skinning stages see the solved pose.
        let _ = crate::ik::solve_ik_chains(&self.scene, &mut locals, &worlds);
        // Blending constraints measure their weight against the pose the solver
        // produced, so the second ordering pass reproduces the first result
        // instead of compounding a partial weight toward the target.
        let solver_pose = locals.clone();
        for _pass in 0..2 {
            for id in &order {
                let Some(node) = self.scene.get(*id) else {
                    continue;
                };
                let parent_world = node
                    .parent
                    .and_then(|p| worlds.get(&p).copied())
                    .unwrap_or(Mat4::IDENTITY);
                let mut local = locals.get(id).copied().unwrap_or(node.transform);
                let base_local = solver_pose.get(id).copied().unwrap_or(node.transform);
                for (tag_index, tag) in node.tags.iter().enumerate() {
                    match tag {
                        Tag::LookAt { target, up } => {
                            if let Some(target_world) = worlds.get(target) {
                                let position = (parent_world * local.matrix()).w_axis.truncate();
                                let desired =
                                    look_at_quat(position, target_world.w_axis.truncate(), *up);
                                let parent_rot = world_rotation(parent_world);
                                local.rotation = (parent_rot.inverse() * desired).normalize();
                            }
                        }
                        Tag::ClampRotation { min, max } => {
                            let (min, max) = normalized_clamp_bounds(*min, *max);
                            let e = local.euler_degrees();
                            let clamped = Vec3::new(
                                e.x.clamp(min.x, max.x),
                                e.y.clamp(min.y, max.y),
                                e.z.clamp(min.z, max.z),
                            );
                            local.rotation = Transform::from_euler_degrees(clamped).rotation;
                        }
                        Tag::CopyLocation {
                            source,
                            offset,
                            weight,
                        } => {
                            if let Some(source_world) = worlds.get(source) {
                                let target_world =
                                    source_world.transform_point3(Vec3::ZERO) + *offset;
                                let local_target =
                                    parent_world.inverse().transform_point3(target_world);
                                let weight = tag_samples
                                    .get(&(*id, tag_index as u32))
                                    .and_then(|samples| {
                                        samples.scalar_for(
                                            ChannelTarget::Tag {
                                                node: *id,
                                                index: tag_index as u32,
                                            },
                                            AnimChannel::CopyLocationWeight,
                                            0,
                                        )
                                    })
                                    .or_else(|| {
                                        node_samples.get(id).and_then(|samples| {
                                            samples.scalar_for(
                                                ChannelTarget::Object { id: *id },
                                                AnimChannel::CopyLocationWeight,
                                                0,
                                            )
                                        })
                                    })
                                    .unwrap_or(*weight);
                                local.translation =
                                    local.translation.lerp(local_target, weight.clamp(0.0, 1.0));
                            }
                        }
                        Tag::IkPoint { target, weight } => {
                            if let Some(target_world) = worlds.get(target) {
                                let weight = weight.clamp(0.0, 1.0);
                                let current =
                                    (parent_world * base_local.matrix()).w_axis.truncate();
                                let desired = current.lerp(target_world.w_axis.truncate(), weight);
                                let local_target = parent_world.inverse().transform_point3(desired);
                                if local_target.is_finite() {
                                    local.translation = local_target;
                                }
                            }
                        }
                        Tag::IkAngle {
                            target,
                            max_degrees,
                        } => {
                            if let Some(target_world) = worlds.get(target) {
                                let owner_world = parent_world * base_local.matrix();
                                let position = owner_world.w_axis.truncate();
                                let to_target = target_world.w_axis.truncate() - position;
                                let aim = world_rotation(owner_world) * Vec3::X;
                                let (Some(aim), Some(to_target)) =
                                    (unit_axis(aim), unit_axis(to_target))
                                else {
                                    continue;
                                };
                                let limit = max_degrees.clamp(0.0, 180.0).to_radians();
                                let angle = aim.angle_between(to_target);
                                if angle.is_finite() && angle > limit + 1e-6 {
                                    // Rotate the aim back toward the target by the
                                    // excess, about the axis the aim has to travel.
                                    let excess = angle - limit;
                                    if let Some(axis) = unit_axis(aim.cross(to_target)) {
                                        let correction = Quat::from_axis_angle(axis, excess);
                                        let desired = correction * world_rotation(owner_world);
                                        let parent_rot = world_rotation(parent_world);
                                        local.rotation =
                                            (parent_rot.inverse() * desired).normalize();
                                    }
                                }
                            }
                        }
                        Tag::Orient { target, weight } => {
                            if let Some(target_world) = worlds.get(target) {
                                let owner_world = parent_world * base_local.matrix();
                                let weight = weight.clamp(0.0, 1.0);
                                let desired = world_rotation(owner_world)
                                    .slerp(world_rotation(*target_world), weight);
                                let parent_rot = world_rotation(parent_world);
                                local.rotation = (parent_rot.inverse() * desired).normalize();
                            }
                        }
                        Tag::Parent { target, weight } => {
                            if let Some(target_world) = worlds.get(target) {
                                let weight = weight.clamp(0.0, 1.0);
                                let owner_world = parent_world * base_local.matrix();
                                let current_position = owner_world.w_axis.truncate();
                                let desired_position =
                                    current_position.lerp(target_world.w_axis.truncate(), weight);
                                let local_position =
                                    parent_world.inverse().transform_point3(desired_position);
                                if local_position.is_finite() {
                                    local.translation = local_position;
                                }
                                let desired_rotation = world_rotation(owner_world)
                                    .slerp(world_rotation(*target_world), weight);
                                let parent_rot = world_rotation(parent_world);
                                local.rotation =
                                    (parent_rot.inverse() * desired_rotation).normalize();
                            }
                        }
                        _ => {}
                    }
                }
                worlds.insert(*id, parent_world * local.matrix());
                locals.insert(*id, local);
            }
        }

        // 3. Visibility chains.
        let mut visible_map: BTreeMap<NodeId, bool> = BTreeMap::new();
        let mut rendered_map: BTreeMap<NodeId, bool> = BTreeMap::new();
        let mut shadow_map: BTreeMap<NodeId, bool> = BTreeMap::new();
        for id in &order {
            let Some(node) = self.scene.get(*id) else {
                continue;
            };
            let parent_visible = node
                .parent
                .map(|p| visible_map.get(&p).copied().unwrap_or(true))
                .unwrap_or(true);
            let parent_rendered = node
                .parent
                .map(|p| rendered_map.get(&p).copied().unwrap_or(true))
                .unwrap_or(true);
            let mut visible = parent_visible && node.visible;
            let mut rendered = parent_rendered
                && node.visible
                && node.renderable
                && self.settings.layer_visible(node.layer);
            let mut casts_shadow = true;
            for tag in &node.tags {
                if let Tag::RenderOverride {
                    visible: tag_visible,
                    casts_shadow: tag_shadows,
                } = tag
                {
                    visible &= *tag_visible;
                    rendered &= *tag_visible;
                    casts_shadow = *tag_shadows;
                }
            }
            visible_map.insert(*id, visible);
            rendered_map.insert(*id, rendered);
            shadow_map.insert(*id, casts_shadow);
        }

        // 4. Geometry, modifiers, morph targets and skinning.
        let mut geometry_cache: HashMap<NodeId, EvaluatedGeometry> = HashMap::new();
        let mut visiting: HashSet<NodeId> = HashSet::new();
        let mut nodes = Vec::with_capacity(order.len());
        let mut by_id = BTreeMap::new();
        let mut active_camera = None;
        let mut lights = Vec::new();
        let mut bounds = Aabb::EMPTY;
        let camera_id = self.active_camera_id();
        let mut first_camera: Option<EvaluatedCamera> = None;

        for id in &order {
            let Some(node) = self.scene.get(*id) else {
                continue;
            };
            let world = worlds.get(id).copied().unwrap_or(Mat4::IDENTITY);
            let local = locals.get(id).copied().unwrap_or(node.transform);
            let (scale, _, translation) = world.to_scale_rotation_translation();
            let world_transform = Transform {
                translation,
                rotation: world_rotation(world),
                scale,
            };
            let rendered = rendered_map.get(id).copied().unwrap_or(false);
            let visible = visible_map.get(id).copied().unwrap_or(false);
            let casts_shadow = shadow_map.get(id).copied().unwrap_or(true);

            let mut geo = EvaluatedGeometry::default();
            if matches!(node.kind, NodeKind::Mesh { .. } | NodeKind::Spline { .. }) {
                geo = self.evaluate_geometry_recursive(
                    *id,
                    time,
                    &worlds,
                    &mut geometry_cache,
                    &mut visiting,
                );
            }

            let kind = match &node.kind {
                NodeKind::Null => EvaluatedKind::Null,
                NodeKind::Mesh { .. } | NodeKind::Spline { .. } => EvaluatedKind::Mesh {
                    mesh: geo.mesh,
                    spline: geo.spline,
                },
                NodeKind::Camera { camera } => {
                    let mut cam = camera.clone();
                    cam.fov_y_deg = node_samples
                        .get(id)
                        .and_then(|samples| {
                            samples.scalar_for(
                                ChannelTarget::Object { id: *id },
                                AnimChannel::Fov,
                                0,
                            )
                        })
                        .unwrap_or(cam.fov_y_deg)
                        .clamp(1.0, 179.0);
                    let evaluated = EvaluatedCamera {
                        node: *id,
                        world,
                        camera: cam.clone(),
                    };
                    if camera_id == Some(*id) {
                        active_camera = Some(evaluated.clone());
                    }
                    if first_camera.is_none() {
                        first_camera = Some(evaluated);
                    }
                    EvaluatedKind::Camera(cam)
                }
                NodeKind::Light { light } => {
                    let mut l = light.clone();
                    let target = ChannelTarget::Object { id: *id };
                    if let Some(samples) = node_samples.get(id) {
                        l.intensity = samples
                            .scalar_for(target, AnimChannel::LightIntensity, 0)
                            .unwrap_or(l.intensity)
                            .max(0.0);
                        let mut color = l.color;
                        for (axis, component) in [0u8, 1, 2].into_iter().enumerate() {
                            if let Some(value) =
                                samples.scalar_for(target, AnimChannel::LightColor, component)
                            {
                                color[axis] = value;
                            }
                        }
                        l.color = color.max(Vec3::ZERO);
                    }
                    if visible && self.settings.layer_visible(node.layer) {
                        lights.push(EvaluatedLight {
                            node: *id,
                            world,
                            light: l.clone(),
                            casts_shadow: l.cast_shadows && casts_shadow,
                        });
                    }
                    EvaluatedKind::Light(l)
                }
                NodeKind::Joint { length, width } => EvaluatedKind::Joint {
                    length: *length,
                    width: *width,
                },
            };

            if rendered {
                if let EvaluatedKind::Mesh {
                    mesh: Some(mesh), ..
                } = &kind
                {
                    if !mesh.is_empty() {
                        let b = mesh.bounds();
                        if b.is_valid() {
                            bounds = bounds.union(&b.transformed(world));
                        }
                    }
                }
            }

            let evaluated = EvaluatedNode {
                id: *id,
                name: node.name.clone(),
                parent: node.parent,
                local,
                world,
                world_transform,
                visible,
                rendered,
                casts_shadow,
                layer: node.layer,
                kind,
                material: node.effective_material(),
                material_slots: node.material_slots.clone(),
            };
            by_id.insert(*id, nodes.len());
            nodes.push(evaluated);
        }

        let active_camera = active_camera.or(first_camera);

        EvaluatedScene {
            time,
            nodes,
            by_id,
            active_camera,
            lights,
            environment: self.settings.environment.clone(),
            settings: self.settings.render.clone(),
            background: self.settings.background,
            bounds,
        }
    }

    fn evaluate_geometry_recursive(
        &self,
        id: NodeId,
        time: f32,
        worlds: &BTreeMap<NodeId, Mat4>,
        cache: &mut HashMap<NodeId, EvaluatedGeometry>,
        visiting: &mut HashSet<NodeId>,
    ) -> EvaluatedGeometry {
        if let Some(hit) = cache.get(&id) {
            return hit.clone();
        }
        if !visiting.insert(id) {
            return EvaluatedGeometry::default();
        }
        let Some(node) = self.scene.get(id) else {
            visiting.remove(&id);
            return EvaluatedGeometry::default();
        };
        let mut spline = None;
        let mut mesh = match &node.kind {
            NodeKind::Mesh { geometry } => Some(geometry.build()),
            NodeKind::Spline { spline: s } => {
                spline = Some(s.clone());
                None
            }
            _ => None,
        };

        // The recorded morph base, not whatever pose the editable mesh is
        // currently displayed in, is the source surface. That keeps a weight
        // of zero at the base even after capturing a target from a moved mesh.
        if let Some(mesh) = mesh.as_mut() {
            node.morph.restore_base(&mut mesh.positions);
            let weights: Vec<f32> = node
                .morph
                .targets
                .iter()
                .enumerate()
                .map(|(index, target)| {
                    node.animation
                        .sample_morph_weight(index as u32, time)
                        .unwrap_or(target.weight)
                })
                .collect();
            node.morph.apply_weighted(&mut mesh.positions, &weights);
        }

        for modifier in &node.modifiers {
            // The inspector and undo command write modifier fields directly,
            // so repair a corrupt parameter here as well as on load.
            let repaired;
            let modifier = if modifier.parameters_are_finite() {
                modifier
            } else {
                repaired = {
                    let mut repaired = modifier.clone();
                    repaired.sanitize();
                    repaired
                };
                &repaired
            };
            // Boolean and Wrap take a second mesh; it is evaluated and moved into
            // the target's local space so both modifiers can treat it as a plain
            // mesh input. A modifier that names itself has no operand.
            let operand_id = match modifier {
                Modifier::Boolean { operand, .. } => Some(*operand),
                Modifier::Wrap { target, .. } => Some(*target),
                _ => None,
            };
            let operand_mesh = operand_id
                .filter(|operand| *operand != id)
                .and_then(|operand| {
                    let other =
                        self.evaluate_geometry_recursive(operand, time, worlds, cache, visiting);
                    other.mesh.map(|mut m| {
                        let target_world = worlds.get(&id).copied().unwrap_or(Mat4::IDENTITY);
                        let source_world = worlds.get(&operand).copied().unwrap_or(Mat4::IDENTITY);
                        let relative = target_world.inverse() * source_world;
                        m.transform(relative);
                        m
                    })
                });
            let input = ModifierInput {
                mesh: mesh.as_ref(),
                spline: spline.as_ref(),
                operand: operand_mesh.as_ref(),
            };
            if let Some(out) = modifier.apply(input) {
                mesh = Some(out);
            }
        }

        if let Some(skin) = &node.skin {
            if let Some(m) = mesh.as_mut() {
                apply_skin(&mut m.positions, skin, id, worlds);
            }
        }

        let result = EvaluatedGeometry { mesh, spline };
        visiting.remove(&id);
        cache.insert(id, result.clone());
        result
    }
}

fn apply_skin(
    positions: &mut [Vec3],
    skin: &crate::Skin,
    node_id: NodeId,
    worlds: &BTreeMap<NodeId, Mat4>,
) {
    if skin.joint_indices.len() < positions.len() || skin.weights.len() < positions.len() {
        return;
    }
    let node_world = worlds.get(&node_id).copied().unwrap_or(Mat4::IDENTITY);
    let node_inv = node_world.inverse();
    let mut matrices = Vec::with_capacity(skin.joints.len());
    for (i, joint) in skin.joints.iter().enumerate() {
        let joint_world = worlds.get(joint).copied().unwrap_or(Mat4::IDENTITY);
        let bind = skin.inverse_bind.get(i).copied().unwrap_or(Mat4::IDENTITY);
        // `inverse_bind` stores the joint's inverse world at bind time. The
        // trailing node world converts that world-space delta back into the
        // mesh node's local space, so a transformed mesh still rests correctly.
        matrices.push(node_inv * joint_world * bind * node_world);
    }
    let skinned = skin_positions_indexed(
        positions,
        &skin.joint_indices[..positions.len()],
        &skin.weights[..positions.len()],
        &matrices,
    );
    for (position, skinned) in positions.iter_mut().zip(&skinned) {
        // A singular or overflowing joint chain has no meaningful deformation;
        // keep that vertex at its rest pose instead of writing NaN or infinity.
        if skinned.is_finite() {
            *position = *skinned;
        }
    }
}

/// Convenience: build a camera look-at quaternion for a node transform.
pub fn aim_at(eye: Vec3, target: Vec3, up: Vec3) -> Quat {
    look_at_quat(eye, target, up)
}

/// Rotation carried by a world matrix, tolerating degenerate scale. A zero,
/// non-finite or unmeasurably large scale makes `to_scale_rotation_translation`
/// divide by it and return a NaN or non-unit quaternion. Such a matrix has no
/// recoverable orientation, so callers get identity instead of NaN; a later
/// non-degenerate frame restores the real rotation.
pub(crate) fn world_rotation(world: Mat4) -> Quat {
    let (scale, rotation, _) = world.to_scale_rotation_translation();
    if scale.is_finite()
        && scale.min_element() > 0.0
        && rotation.is_finite()
        && rotation.length_squared() > 1e-12
    {
        rotation.normalize()
    } else {
        Quat::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GeometrySource, MorphTarget, Primitive, Skin, Tag};
    use lc_anim::{AnimationClip, Interpolation};

    #[test]
    fn wrap_modifier_uses_its_target_and_ignores_a_self_target() {
        let mut doc = Document::empty();
        let source = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::from_primitive(Primitive::Box {
                    width: 4.0,
                    height: 0.6,
                    depth: 0.6,
                }),
            },
            "Wrap Source",
            None,
        );
        let target = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::from_primitive(Primitive::Box {
                    width: 1.0,
                    height: 1.0,
                    depth: 1.0,
                }),
            },
            "Wrap Target",
            None,
        );
        doc.scene.get_mut(target).unwrap().transform.translation = Vec3::new(3.0, 0.0, 0.0);
        let rest = match &doc.scene.get(source).unwrap().kind {
            NodeKind::Mesh { geometry } => geometry.build(),
            _ => unreachable!("the wrap source is a mesh"),
        };
        doc.scene
            .get_mut(source)
            .unwrap()
            .modifiers
            .push(Modifier::Wrap {
                target,
                angle_deg: 120.0,
                axis: 0,
            });
        let wrapped = doc.evaluate(0.0);
        let mesh = wrapped.node(source).unwrap().mesh().unwrap();
        assert!(mesh.validate().is_ok());
        assert!(mesh.positions.iter().all(|point| point.is_finite()));
        assert!(mesh
            .positions
            .iter()
            .zip(&rest.positions)
            .any(|(wrapped, rest)| (*wrapped - *rest).length() > 1e-3));

        // A Wrap that names its own object has no operand, so it leaves the mesh
        // alone instead of recursing into its own evaluation.
        doc.scene.get_mut(source).unwrap().modifiers = vec![Modifier::Wrap {
            target: source,
            angle_deg: 120.0,
            axis: 0,
        }];
        let self_target = doc.evaluate(0.0);
        assert_eq!(
            self_target.node(source).unwrap().mesh().unwrap().positions,
            rest.positions
        );
    }

    #[test]
    fn demo_evaluates_to_renderable_scene() {
        let doc = Document::demo();
        let scene = doc.evaluate(0.0);
        assert!(scene.active_camera.is_some());
        assert!(!scene.lights.is_empty());
        assert!(scene.mesh_nodes().count() >= 5);
        assert!(scene.bounds.is_valid());
        assert!(!scene.nodes.is_empty());
        // Ground + camera-less geometry must still be deterministic.
        assert_eq!(scene.signature(), doc.evaluate(0.0).signature());
    }

    #[test]
    fn animation_changes_world_transform() {
        let doc = Document::demo();
        let a = doc.evaluate(0.0);
        let b = doc.evaluate(24.0);
        let id = doc.scene.find_by_name("Sphere").unwrap();
        let ya = a.node(id).unwrap().world.w_axis.y;
        let yb = b.node(id).unwrap().world.w_axis.y;
        assert!((ya - 1.05).abs() < 1e-4);
        assert!(yb > ya + 0.5);
    }

    #[test]
    fn look_at_under_a_zero_scale_parent_keeps_a_finite_rotation() {
        let mut doc = Document::empty();
        let parent = doc.append_node(NodeKind::Null, "Parent", None);
        doc.scene.get_mut(parent).unwrap().transform.scale = Vec3::ZERO;
        let child = doc.append_node(NodeKind::Null, "Child", Some(parent));
        doc.scene.get_mut(child).unwrap().transform.translation = Vec3::new(0.0, 0.0, 10.0);
        let target = doc.append_node(NodeKind::Null, "Target", None);
        doc.scene.get_mut(child).unwrap().tags.push(Tag::LookAt {
            target,
            up: Vec3::Y,
        });

        // A zero-scale parent has no recoverable rotation; the tag must stay
        // finite instead of writing a NaN local rotation into the scene.
        let solved = doc.evaluate(0.0);
        let node = solved.node(child).unwrap();
        assert!(
            node.local.rotation.is_finite(),
            "look-at under a zero-scale parent wrote {:?}",
            node.local.rotation
        );
        assert!(
            (node.local.rotation.length() - 1.0).abs() < 1e-4,
            "the look-at rotation must stay unit length"
        );
        assert!(node.world.is_finite());
    }

    #[test]
    fn animated_copy_location_weight_is_sampled_during_evaluation() {
        let mut doc = Document::demo();
        let cube = doc.scene.find_by_name("Cube").unwrap();
        let target = doc.scene.find_by_name("Sphere").unwrap();
        {
            let node = doc.scene.get_mut(cube).unwrap();
            node.tags
                .retain(|tag| !matches!(tag, Tag::CopyLocation { .. }));
            node.tags.push(Tag::CopyLocation {
                source: target,
                offset: Vec3::ZERO,
                weight: 0.0,
            });
            node.animation = AnimationClip::default();
            let weight_address =
                lc_anim::ChannelAddress::owner(AnimChannel::CopyLocationWeight, 0).unwrap();
            node.animation
                .set_key(weight_address, 0.0, 0.0, Interpolation::Linear);
            node.animation
                .set_key(weight_address, 1.0, 1.0, Interpolation::Linear);
        }
        let scene0 = doc.evaluate(0.0);
        let scene1 = doc.evaluate(1.0);
        let target_pos = scene1.node(target).unwrap().world.w_axis.truncate();
        let pos0 = scene0.node(cube).unwrap().world.w_axis.truncate();
        let pos1 = scene1.node(cube).unwrap().world.w_axis.truncate();
        assert!((pos0 - pos1).length() > 0.3);
        assert!(
            (pos1 - target_pos).length() < 0.05,
            "pos1 {pos1:?} target {target_pos:?}"
        );
    }

    #[test]
    fn look_at_constraint_aims_light_at_target() {
        let doc = Document::demo();
        let scene = doc.evaluate(0.0);
        let key = doc.scene.find_by_name("Key Light").unwrap();
        let target = doc.scene.find_by_name("Light Target").unwrap();
        let key_node = scene.node(key).unwrap();
        let target_pos = scene.node(target).unwrap().world.w_axis.truncate();
        let pos = key_node.world.w_axis.truncate();
        let forward = Camera::forward(key_node.world);
        let want = (target_pos - pos).normalize();
        assert!(
            forward.dot(want) > 0.99,
            "forward {forward:?} want {want:?}"
        );
    }

    #[test]
    fn modifiers_apply_during_evaluation() {
        let mut doc = Document::demo();
        let id = doc.scene.find_by_name("Cube").unwrap();
        if let Some(node) = doc.scene.get_mut(id) {
            node.modifiers.push(Modifier::Subdivision {
                levels: 2,
                scheme: Default::default(),
            });
        }
        let scene = doc.evaluate(0.0);
        let node = scene.node(id).unwrap();
        let tris = node.mesh().map(|m| m.triangle_count()).unwrap_or(0);
        assert!(tris > 100, "got {tris} triangles");
    }

    #[test]
    fn non_finite_primitive_dimensions_never_evaluate_to_non_finite_geometry() {
        // The inspector edits primitive dimensions in place, so evaluation must
        // repair them even when no load-time sanitize pass has run.
        let primitives = [
            Primitive::Box {
                width: f32::INFINITY,
                height: 2.0,
                depth: 1.0,
            },
            Primitive::Sphere {
                radius: f32::NAN,
                segments: 8,
                rings: 6,
            },
            Primitive::Cylinder {
                radius: f32::INFINITY,
                height: 2.0,
                segments: 8,
            },
            Primitive::Capsule {
                radius: 1.0,
                height: f32::NEG_INFINITY,
                segments: 8,
                rings: 4,
            },
            Primitive::Disc {
                radius: f32::NAN,
                segments: 8,
            },
            Primitive::Cone {
                radius: f32::INFINITY,
                height: 2.0,
                segments: 8,
            },
            Primitive::Torus {
                major_radius: f32::INFINITY,
                minor_radius: 0.25,
                segments: 8,
                rings: 6,
            },
            Primitive::Plane {
                width: 2.0,
                depth: f32::NAN,
                segments_x: 2,
                segments_z: 2,
            },
        ];
        for primitive in primitives {
            let mut doc = Document::empty();
            let id = doc.append_node(
                NodeKind::Mesh {
                    geometry: GeometrySource::from_primitive(primitive),
                },
                "Poisoned Primitive",
                None,
            );
            let scene = doc.evaluate(0.0);
            let mesh = scene
                .node(id)
                .and_then(|node| node.mesh())
                .cloned()
                .expect("the primitive evaluates");
            assert!(
                mesh.positions.iter().all(|position| position.is_finite()),
                "{} evaluated to non-finite positions from an in-memory dimension: {:?}",
                primitive.label(),
                mesh.positions
            );
            assert!(mesh.validate().is_ok(), "{}", primitive.label());
            assert!(
                scene.bounds.min.is_finite() && scene.bounds.max.is_finite(),
                "{} produced non-finite scene bounds: {:?}",
                primitive.label(),
                scene.bounds
            );
        }
    }

    #[test]
    fn non_finite_modifier_parameters_are_repaired_before_evaluation() {
        // The inspector writes modifier fields directly into the document, so
        // evaluation must not rely on the load-time sanitize pass.
        for modifier in [
            Modifier::ExtrudeFaces {
                distance: f32::INFINITY,
                individual: false,
                faces: Vec::new(),
            },
            Modifier::BevelFaces {
                amount: f32::NAN,
                depth: 0.1,
                faces: Vec::new(),
            },
        ] {
            let mut doc = Document::empty();
            let id = doc.append_node(
                NodeKind::Mesh {
                    geometry: GeometrySource::from_primitive(Primitive::Box {
                        width: 1.0,
                        height: 1.0,
                        depth: 1.0,
                    }),
                },
                "Box",
                None,
            );
            doc.scene
                .get_mut(id)
                .unwrap()
                .modifiers
                .push(modifier.clone());
            let mesh = doc
                .evaluate(0.0)
                .node(id)
                .and_then(|node| node.mesh())
                .cloned()
                .expect("the box evaluates");
            assert!(
                mesh.positions.iter().all(|position| position.is_finite()),
                "{} emitted non-finite positions from a non-finite parameter: {:?}",
                modifier.label(),
                mesh.positions
            );
            assert!(mesh.validate().is_ok(), "{}", modifier.label());
        }
    }

    #[test]
    fn morph_targets_blend_additively_before_skinning() {
        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::from_primitive(Primitive::Box {
                    width: 1.0,
                    height: 1.0,
                    depth: 1.0,
                }),
            },
            "Morph Box",
            None,
        );
        let rest = doc
            .evaluate(0.0)
            .node(id)
            .and_then(|node| node.mesh())
            .map(|mesh| mesh.positions.clone())
            .unwrap();
        let mut down = MorphTarget::new("Down", vec![-Vec3::Y; rest.len()]);
        down.weight = 0.25;
        {
            let node = doc.scene.get_mut(id).unwrap();
            node.morph.base = rest.clone();
            node.morph
                .targets
                .push(MorphTarget::new("Up", vec![Vec3::Y; rest.len()]));
            node.morph.targets.push(down);
            node.morph.targets[0].weight = 0.5;
        }
        let posed = doc.evaluate(0.0);
        let positions = &posed.node(id).unwrap().mesh().unwrap().positions;
        for (rest, posed) in rest.iter().zip(positions) {
            assert!(
                (*posed - (*rest + Vec3::Y * 0.25)).length() < 1e-4,
                "rest {rest:?} posed {posed:?}"
            );
        }
    }

    #[test]
    fn morph_weights_clamp_and_stale_topology_is_ignored() {
        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::from_primitive(Primitive::Box {
                    width: 1.0,
                    height: 1.0,
                    depth: 1.0,
                }),
            },
            "Morph Box",
            None,
        );
        let rest = doc
            .evaluate(0.0)
            .node(id)
            .and_then(|node| node.mesh())
            .map(|mesh| mesh.positions.clone())
            .unwrap();
        {
            let node = doc.scene.get_mut(id).unwrap();
            node.morph.base = rest.clone();
            let mut up = MorphTarget::new("Overdriven", vec![Vec3::Y; rest.len()]);
            up.weight = 2.0;
            node.morph.targets.push(up);
            let mut stale = MorphTarget::new("Stale", vec![Vec3::X; 1]);
            stale.weight = 1.0;
            node.morph.targets.push(stale);
        }
        let posed = doc.evaluate(0.0);
        let positions = &posed.node(id).unwrap().mesh().unwrap().positions;
        for (rest, posed) in rest.iter().zip(positions) {
            assert!((*posed - (*rest + Vec3::Y)).length() < 1e-4);
        }
    }

    #[test]
    fn morph_weight_curve_clamps_extremes_and_roundtrips() {
        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::from_primitive(Primitive::Box {
                    width: 1.0,
                    height: 1.0,
                    depth: 1.0,
                }),
            },
            "Curve Morph",
            None,
        );
        let rest = doc
            .evaluate(0.0)
            .node(id)
            .and_then(|node| node.mesh())
            .map(|mesh| mesh.positions.clone())
            .unwrap();
        {
            let node = doc.scene.get_mut(id).unwrap();
            node.morph = crate::MorphSet {
                base: rest.clone(),
                targets: vec![MorphTarget::new("Lift", vec![Vec3::Y; rest.len()])],
            };
            for (time, value) in [(0.0, -0.5), (1.0, 0.5), (2.0, 1.5)] {
                node.animation
                    .set_morph_weight_key(0, time, value, Interpolation::Linear);
            }
        }

        let check = |doc: &Document, time: f32, expected: f32| {
            let position = doc
                .evaluate(time)
                .node(id)
                .unwrap()
                .mesh()
                .unwrap()
                .positions[0];
            assert!(
                (position - (rest[0] + Vec3::Y * expected)).length() < 1e-4,
                "at {time}s expected weight {expected}, got {}",
                (position - rest[0]).dot(Vec3::Y)
            );
        };
        // The curve interpolates 0.5 at the midpoint and clamps the negative
        // and over-one authored values at the extremes.
        check(&doc, 0.0, 0.0);
        check(&doc, 0.5, 0.0);
        check(&doc, 1.5, 1.0);
        check(&doc, 2.0, 1.0);

        let json = crate::to_json(&doc).unwrap();
        let loaded = crate::from_json(&json).unwrap();
        assert_eq!(loaded.scene.get(id).unwrap().morph.base, rest);
        assert_eq!(loaded.scene.get(id).unwrap().morph.targets.len(), 1);
        for (time, expected) in [(0.0, 0.0), (0.5, 0.0), (1.5, 1.0), (2.0, 1.0)] {
            check(&loaded, time, expected);
        }
        assert!(
            (loaded
                .scene
                .get(id)
                .unwrap()
                .animation
                .sample_morph_weight(0, 1.5)
                .unwrap()
                - 1.0)
                .abs()
                < 1e-5
        );
    }

    #[test]
    fn animated_morph_weight_is_sampled_from_the_take() {
        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::from_primitive(Primitive::Box {
                    width: 1.0,
                    height: 1.0,
                    depth: 1.0,
                }),
            },
            "Animated Morph",
            None,
        );
        let rest = doc
            .evaluate(0.0)
            .node(id)
            .and_then(|node| node.mesh())
            .map(|mesh| mesh.positions.clone())
            .unwrap();
        {
            let node = doc.scene.get_mut(id).unwrap();
            node.morph = crate::MorphSet {
                base: rest.clone(),
                targets: vec![MorphTarget::new("Lift", vec![Vec3::Y; rest.len()])],
            };
            node.animation
                .set_morph_weight_key(0, 0.0, 0.0, Interpolation::Linear);
            node.animation
                .set_morph_weight_key(0, 1.0, 1.0, Interpolation::Linear);
        }
        let at_zero = doc.evaluate(0.0);
        let at_one = doc.evaluate(1.0);
        let zero = at_zero.node(id).unwrap().mesh().unwrap().positions[0];
        let one = at_one.node(id).unwrap().mesh().unwrap().positions[0];
        assert!((zero - rest[0]).length() < 1e-5);
        assert!((one - (rest[0] + Vec3::Y)).length() < 1e-4);
    }

    #[test]
    fn boolean_modifier_uses_operand() {
        let mut doc = Document::empty();
        let a = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::from_primitive(Primitive::Box {
                    width: 2.0,
                    height: 2.0,
                    depth: 2.0,
                }),
            },
            "A",
            None,
        );
        let b = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::from_primitive(Primitive::Box {
                    width: 2.0,
                    height: 2.0,
                    depth: 2.0,
                }),
            },
            "B",
            None,
        );
        doc.scene
            .get_mut(a)
            .unwrap()
            .modifiers
            .push(Modifier::Boolean {
                operand: b,
                operation: crate::CsgOperation::Subtract,
            });
        doc.scene.get_mut(b).unwrap().transform.translation.x = 1.0;
        let scene = doc.evaluate(0.0);
        let mesh = scene.node(a).unwrap().mesh().unwrap();
        assert!(mesh.validate().is_ok());
        assert!(!mesh.polygons.is_empty());
    }

    #[test]
    fn skinning_deforms_mesh_with_joint() {
        let mut doc = Document::empty();
        let joint = doc.append_node(
            NodeKind::Joint {
                length: 1.0,
                width: 0.1,
            },
            "Joint",
            None,
        );
        let mesh_id = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::editable(lc_geom::plane_mesh(1.0, 1.0, 1, 1)),
            },
            "Skinned",
            None,
        );
        let mut skin = Skin::new(vec![joint], vec![Mat4::IDENTITY]);
        skin.bind_all_to_first(4);
        doc.scene.get_mut(mesh_id).unwrap().skin = Some(skin);
        doc.scene.get_mut(joint).unwrap().transform.translation.y = 2.0;
        doc.scene.get_mut(joint).unwrap().animation.set_key(
            lc_anim::ChannelAddress::owner(AnimChannel::Translation, 1).unwrap(),
            0.0,
            2.0,
            Interpolation::Linear,
        );
        let scene = doc.evaluate(0.0);
        let mesh = scene.node(mesh_id).unwrap().mesh().unwrap();
        assert!(mesh.positions.iter().all(|p| (p.y - 2.0).abs() < 1e-4));
    }

    #[test]
    fn skinning_under_a_singular_mesh_world_keeps_rest_positions() {
        let mut doc = Document::empty();
        let joint = doc.append_node(
            NodeKind::Joint {
                length: 1.0,
                width: 0.1,
            },
            "Joint",
            None,
        );
        let mesh_id = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::editable(lc_geom::plane_mesh(1.0, 1.0, 1, 1)),
            },
            "Skinned",
            None,
        );
        let mut skin = Skin::new(vec![joint], vec![Mat4::IDENTITY]);
        skin.bind_all_to_first(4);
        {
            let node = doc.scene.get_mut(mesh_id).unwrap();
            node.skin = Some(skin);
            // Collapsing the mesh node makes its world matrix singular, so the
            // skinning pass derives a non-finite inverse and joint matrices.
            node.transform.scale = Vec3::ZERO;
        }
        doc.scene.get_mut(joint).unwrap().transform.translation.y = 2.0;

        let scene = doc.evaluate(0.0);
        let node = scene.node(mesh_id).unwrap();
        let mesh = node.mesh().unwrap();
        assert!(
            mesh.positions.iter().all(|p| p.is_finite()),
            "a singular mesh world must not skin vertices into NaN"
        );
        assert!(node.world_transform.rotation.is_finite());
        assert!(node.world_transform.scale.is_finite());
        assert!(node.world_transform.translation.is_finite());
    }

    #[test]
    fn layer_visibility_hides_nodes() {
        let mut doc = Document::demo();
        doc.settings.layers[0].visible = false;
        let scene = doc.evaluate(0.0);
        assert!(scene.mesh_nodes().count() == 0);
        doc.settings.layers[0].visible = true;
        assert!(doc.evaluate(0.0).mesh_nodes().count() > 0);
    }

    #[test]
    fn reversed_clamp_rotation_bounds_are_ordered_instead_of_panicking() {
        let mut doc = Document::empty();
        let id = doc.append_node(NodeKind::Null, "Clamped", None);
        {
            let node = doc.scene.get_mut(id).unwrap();
            node.transform = Transform::from_euler_degrees(Vec3::new(0.0, 45.0, 30.0));
            node.tags.push(Tag::ClampRotation {
                min: Vec3::new(10.0, -180.0, 0.0),
                max: Vec3::new(-10.0, 180.0, 0.0),
            });
        }

        // Reversed X bounds are swapped and the zero-width Z range locks the
        // axis. `f32::clamp` would panic on the raw pair.
        let local = doc.evaluate(0.0).node(id).unwrap().local;
        let euler = local.euler_degrees();
        assert!(local.rotation.is_finite());
        assert!(euler.x.abs() <= 10.0 + 1e-3, "x {euler:?}");
        assert!((euler.y - 45.0).abs() < 1e-3, "y {euler:?}");
        assert!(euler.z.abs() < 1e-3, "z {euler:?}");

        // Loading normalises the stored tag so later saves stay sane.
        let back = crate::from_json(&crate::to_json(&doc).unwrap()).unwrap();
        match &back.scene.get(id).unwrap().tags[0] {
            Tag::ClampRotation { min, max } => {
                assert_eq!((min.x, min.z), (-10.0, 0.0));
                assert_eq!((max.x, max.z), (10.0, 0.0));
                assert_eq!((min.y, max.y), (-180.0, 180.0));
            }
            other => panic!("unexpected tag {other:?}"),
        }
    }

    #[test]
    fn non_finite_clamp_rotation_bounds_use_the_full_range() {
        let mut doc = Document::empty();
        let id = doc.append_node(NodeKind::Null, "Non-finite", None);
        {
            let node = doc.scene.get_mut(id).unwrap();
            node.transform = Transform::from_euler_degrees(Vec3::new(0.0, 45.0, 0.0));
            node.tags.push(Tag::ClampRotation {
                min: Vec3::new(f32::NAN, -10.0, f32::NEG_INFINITY),
                max: Vec3::new(20.0, 10.0, f32::INFINITY),
            });
        }
        // The NaN X and infinite Z bounds become the full range, so only the
        // finite Y range clamps the 45-degree rotation.
        let local = doc.evaluate(0.0).node(id).unwrap().local;
        assert!(local.rotation.is_finite());
        let euler = local.euler_degrees();
        assert!((euler.y - 10.0).abs() < 1e-3, "y {euler:?}");
    }

    #[test]
    fn broken_hierarchy_does_not_hang() {
        let mut doc = Document::demo();
        let sphere = doc.scene.find_by_name("Sphere").unwrap();
        // Deliberately corrupt parent pointers into a cycle.
        if let Some(n) = doc.scene.get_mut(sphere) {
            n.parent = Some(sphere);
        }
        let scene = doc.evaluate(0.0);
        assert!(scene.signature() != 0);
    }

    #[test]
    fn geometry_cache_is_deterministic() {
        let doc = Document::demo();
        let a = doc.evaluate(12.0);
        let b = doc.evaluate(12.0);
        assert_eq!(a.signature(), b.signature());
        assert_eq!(a.triangle_count(), b.triangle_count());
    }
}
