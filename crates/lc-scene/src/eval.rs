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
use lc_anim::{skin_positions_indexed, AnimProperty};
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

impl Document {
    pub fn evaluate(&self, time: f32) -> EvaluatedScene {
        let time = if time.is_finite() { time.max(0.0) } else { 0.0 };
        let order = self.scene.depth_first();
        let mut locals: BTreeMap<NodeId, Transform> = BTreeMap::new();

        // 1. Animated local transforms.
        for id in &order {
            let Some(node) = self.scene.get(*id) else {
                continue;
            };
            locals.insert(*id, node.animation.apply_transform(node.transform, time));
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
                for (tag_index, tag) in node.tags.iter().enumerate() {
                    match tag {
                        Tag::LookAt { target, up } => {
                            if let Some(target_world) = worlds.get(target) {
                                let position = (parent_world * local.matrix()).w_axis.truncate();
                                let desired =
                                    look_at_quat(position, target_world.w_axis.truncate(), *up);
                                let parent_rot = parent_world.to_scale_rotation_translation().1;
                                local.rotation = (parent_rot.inverse() * desired).normalize();
                            }
                        }
                        Tag::ClampRotation { min, max } => {
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
                                let weight = node
                                    .tag_animations
                                    .get(&(tag_index as u32))
                                    .and_then(|clip| {
                                        clip.sample(AnimProperty::CopyLocationWeight, time)
                                    })
                                    .or_else(|| {
                                        node.animation
                                            .sample(AnimProperty::CopyLocationWeight, time)
                                    })
                                    .unwrap_or(*weight);
                                local.translation =
                                    local.translation.lerp(local_target, weight.clamp(0.0, 1.0));
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

        // 4. Geometry, modifiers and skinning.
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
            let (scale, rotation, translation) = world.to_scale_rotation_translation();
            let world_transform = Transform {
                translation,
                rotation,
                scale,
            };
            let rendered = rendered_map.get(id).copied().unwrap_or(false);
            let visible = visible_map.get(id).copied().unwrap_or(false);
            let casts_shadow = shadow_map.get(id).copied().unwrap_or(true);

            let mut geo = EvaluatedGeometry::default();
            if matches!(node.kind, NodeKind::Mesh { .. } | NodeKind::Spline { .. }) {
                geo = self.evaluate_geometry_recursive(
                    *id,
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
                    cam.fov_y_deg = node
                        .animation
                        .sample(AnimProperty::Fov, time)
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
                    l.intensity = node
                        .animation
                        .sample(AnimProperty::LightIntensity, time)
                        .unwrap_or(l.intensity)
                        .max(0.0);
                    l.color = Vec3::new(
                        node.animation
                            .sample(AnimProperty::LightColorR, time)
                            .unwrap_or(l.color.x),
                        node.animation
                            .sample(AnimProperty::LightColorG, time)
                            .unwrap_or(l.color.y),
                        node.animation
                            .sample(AnimProperty::LightColorB, time)
                            .unwrap_or(l.color.z),
                    )
                    .max(Vec3::ZERO);
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

        for modifier in &node.modifiers {
            let operand_mesh = if let Modifier::Boolean { operand, .. } = modifier {
                if *operand != id {
                    let other = self.evaluate_geometry_recursive(*operand, worlds, cache, visiting);
                    other.mesh.map(|mut m| {
                        let target_world = worlds.get(&id).copied().unwrap_or(Mat4::IDENTITY);
                        let source_world = worlds.get(operand).copied().unwrap_or(Mat4::IDENTITY);
                        let relative = target_world.inverse() * source_world;
                        m.transform(relative);
                        m
                    })
                } else {
                    None
                }
            } else {
                None
            };
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
        matrices.push(node_inv * joint_world * bind);
    }
    let skinned = skin_positions_indexed(
        positions,
        &skin.joint_indices[..positions.len()],
        &skin.weights[..positions.len()],
        &matrices,
    );
    positions.copy_from_slice(&skinned);
}

/// Convenience: build a camera look-at quaternion for a node transform.
pub fn aim_at(eye: Vec3, target: Vec3, up: Vec3) -> Quat {
    look_at_quat(eye, target, up)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GeometrySource, Primitive, Skin};
    use lc_anim::{AnimationClip, Interpolation};

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
            node.animation.set_key(
                AnimProperty::CopyLocationWeight,
                0.0,
                0.0,
                Interpolation::Linear,
            );
            node.animation.set_key(
                AnimProperty::CopyLocationWeight,
                1.0,
                1.0,
                Interpolation::Linear,
            );
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
            node.modifiers.push(Modifier::Subdivision { levels: 2 });
        }
        let scene = doc.evaluate(0.0);
        let node = scene.node(id).unwrap();
        let tris = node.mesh().map(|m| m.triangle_count()).unwrap_or(0);
        assert!(tris > 100, "got {tris} triangles");
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
            AnimProperty::TranslationY,
            0.0,
            2.0,
            Interpolation::Linear,
        );
        let scene = doc.evaluate(0.0);
        let mesh = scene.node(mesh_id).unwrap().mesh().unwrap();
        assert!(mesh.positions.iter().all(|p| (p.y - 2.0).abs() < 1e-4));
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
