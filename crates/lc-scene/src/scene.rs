//! Scene graph: hierarchical nodes, transforms, tags and skins.

use crate::ik::{IkChain, JointLimits};
use crate::{Camera, GeometrySource, Light, Modifier};
use glam::{Mat4, Vec3};
use lc_anim::AnimationClip;
use lc_geom::Spline;
use lc_shading::MaterialId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub type NodeId = u64;

fn default_true() -> bool {
    true
}

fn one() -> f32 {
    1.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Tag {
    LookAt {
        target: NodeId,
        #[serde(default = "default_up")]
        up: Vec3,
    },
    ClampRotation {
        min: Vec3,
        max: Vec3,
    },
    CopyLocation {
        source: NodeId,
        offset: Vec3,
        #[serde(default = "one")]
        weight: f32,
    },
    RenderOverride {
        visible: bool,
        casts_shadow: bool,
    },
    MaterialOverride {
        material: MaterialId,
    },
    Custom {
        name: String,
        values: BTreeMap<String, f32>,
    },
}

fn default_up() -> Vec3 {
    Vec3::Y
}

impl Tag {
    pub fn label(&self) -> &'static str {
        match self {
            Self::LookAt { .. } => "Look At",
            Self::ClampRotation { .. } => "Clamp Rotation",
            Self::CopyLocation { .. } => "Copy Location",
            Self::RenderOverride { .. } => "Render Override",
            Self::MaterialOverride { .. } => "Material Override",
            Self::Custom { .. } => "Custom Data",
        }
    }

    pub fn look_at(target: NodeId) -> Self {
        Self::LookAt {
            target,
            up: Vec3::Y,
        }
    }

    pub fn defaults_for(node: NodeId) -> Vec<Tag> {
        vec![
            Tag::LookAt {
                target: node,
                up: Vec3::Y,
            },
            Tag::ClampRotation {
                min: Vec3::splat(-180.0),
                max: Vec3::splat(180.0),
            },
            Tag::CopyLocation {
                source: node,
                offset: Vec3::ZERO,
                weight: 1.0,
            },
            Tag::RenderOverride {
                visible: true,
                casts_shadow: true,
            },
            Tag::Custom {
                name: "Meta".into(),
                values: BTreeMap::new(),
            },
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Skin {
    /// Joint nodes in the same scene.
    pub joints: Vec<NodeId>,
    /// Bind-space inverse per joint.
    pub inverse_bind: Vec<Mat4>,
    /// Up to four joint indices into `joints` per mesh vertex.
    pub joint_indices: Vec<[u32; 4]>,
    /// Up to four corresponding weights per vertex.
    pub weights: Vec<[f32; 4]>,
}

/// Controls for the proximity auto-weight pass. Cheetah3D documents a bind
/// dropoff rate and a maximum influence count; the exponent blends those into
/// one deterministic inverse-distance falloff.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProximityWeightOptions {
    pub dropoff: f32,
    pub strength: f32,
    pub max_influences: usize,
}

impl Default for ProximityWeightOptions {
    fn default() -> Self {
        Self {
            dropoff: 2.0,
            strength: 0.6,
            max_influences: 4,
        }
    }
}

impl Skin {
    pub fn new(joints: Vec<NodeId>, inverse_bind: Vec<Mat4>) -> Self {
        Self {
            joints,
            inverse_bind,
            joint_indices: Vec::new(),
            weights: Vec::new(),
        }
    }

    pub fn vertex_count(&self) -> usize {
        self.weights.len().min(self.joint_indices.len())
    }

    /// Bind every vertex fully to the first joint (a starting point for
    /// hand-painted weights or imported rigs).
    pub fn bind_all_to_first(&mut self, vertex_count: usize) {
        self.joint_indices = vec![[0, 0, 0, 0]; vertex_count];
        self.weights = vec![[1.0, 0.0, 0.0, 0.0]; vertex_count];
    }

    /// Leave every vertex unweighted. Such vertices deliberately stay at their
    /// rest positions until painted; they do not silently fall back to joint 0.
    pub fn clear_weights(&mut self, vertex_count: usize) {
        self.joint_indices = vec![[0, 0, 0, 0]; vertex_count];
        self.weights = vec![[0.0, 0.0, 0.0, 0.0]; vertex_count];
    }

    /// Replace all weights with normalized proximity weights to the joint bone
    /// segments. Each `segments` entry must correspond to the same `joints`
    /// index and is `(head, tail)` in mesh space. `dropoff` and `strength`
    /// increase how quickly distant joints lose influence; `max_influences` is
    /// clamped to the four available slots.
    pub fn auto_weight_proximity(
        &mut self,
        positions: &[Vec3],
        segments: &[(Vec3, Vec3)],
        options: ProximityWeightOptions,
    ) -> bool {
        if self.joints.is_empty() || segments.len() != self.joints.len() {
            return false;
        }
        let max_influences = options.max_influences.clamp(1, 4);
        let dropoff = if options.dropoff.is_finite() {
            options.dropoff.clamp(0.05, 16.0)
        } else {
            ProximityWeightOptions::default().dropoff
        };
        let strength = if options.strength.is_finite() {
            options.strength.clamp(0.0, 1.0)
        } else {
            ProximityWeightOptions::default().strength
        };
        let exponent = dropoff * (0.35 + 1.65 * strength);
        self.joint_indices = vec![[0; 4]; positions.len()];
        self.weights = vec![[0.0; 4]; positions.len()];
        for (vertex, position) in positions.iter().enumerate() {
            let mut ranked: Vec<(usize, f32)> = segments
                .iter()
                .enumerate()
                .map(|(joint, (head, tail))| {
                    let distance = point_segment_distance(*position, *head, *tail);
                    (joint, distance)
                })
                .collect();
            let closest = ranked
                .iter()
                .map(|(_, distance)| *distance)
                .fold(f32::INFINITY, f32::min);
            for (_, distance) in &mut ranked {
                let ratio = (closest + 0.05) / (*distance + 0.05);
                *distance = ratio.max(0.0).powf(exponent);
            }
            ranked.sort_by(|(joint_a, score_a), (joint_b, score_b)| {
                score_b
                    .total_cmp(score_a)
                    .then_with(|| joint_a.cmp(joint_b))
            });
            let mut total = 0.0;
            for (slot, (joint, score)) in ranked.into_iter().take(max_influences).enumerate() {
                self.joint_indices[vertex][slot] = joint as u32;
                self.weights[vertex][slot] = score.max(0.0);
                total += score.max(0.0);
            }
            if total > 1e-8 {
                for weight in &mut self.weights[vertex] {
                    *weight /= total;
                }
            } else {
                self.joint_indices[vertex] = [0; 4];
                self.weights[vertex] = [0.0; 4];
            }
            self.compact_vertex(vertex);
        }
        true
    }

    /// Weight assigned to `joint` (an index into `joints`) for `vertex`.
    pub fn influence(&self, vertex: usize, joint: usize) -> f32 {
        if joint >= self.joints.len() {
            return 0.0;
        }
        let Some(indices) = self.joint_indices.get(vertex) else {
            return 0.0;
        };
        let weights = self.weights.get(vertex).copied().unwrap_or_default();
        indices
            .iter()
            .zip(weights)
            .find(|(index, _)| **index as usize == joint)
            .map(|(_, weight)| weight)
            .unwrap_or(0.0)
    }

    /// Set one joint's influence on a vertex and renormalize the remaining
    /// four slots. A new influence takes a free slot or replaces the smallest
    /// one, so a vertex never carries more than four joints.
    pub fn set_influence(&mut self, vertex: usize, joint: usize, weight: f32) -> bool {
        if joint >= self.joints.len() || vertex >= self.vertex_count() {
            return false;
        }
        let weight = if weight.is_finite() {
            weight.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let indices = &mut self.joint_indices[vertex];
        let weights = &mut self.weights[vertex];
        if let Some(slot) = indices.iter().position(|index| *index as usize == joint) {
            weights[slot] = weight;
        } else if weight > 0.0 {
            let slot = weights
                .iter()
                .position(|weight| *weight <= 0.0)
                .or_else(|| {
                    weights
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| a.total_cmp(b))
                        .map(|(slot, _)| slot)
                })
                .unwrap_or(0);
            indices[slot] = joint as u32;
            weights[slot] = weight;
        }
        self.compact_vertex(vertex);
        self.normalize_vertex(vertex);
        true
    }

    /// Replace every influence on a vertex with one joint. Used by the
    /// replace brush so a painted area does not retain unrelated joints.
    pub fn set_exclusive_influence(&mut self, vertex: usize, joint: usize, weight: f32) -> bool {
        if joint >= self.joints.len() || vertex >= self.vertex_count() {
            return false;
        }
        let weight = if weight.is_finite() {
            weight.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.joint_indices[vertex] = [joint as u32, 0, 0, 0];
        self.weights[vertex] = [weight, 0.0, 0.0, 0.0];
        self.compact_vertex(vertex);
        self.normalize_vertex(vertex);
        true
    }

    /// Drop zero and out-of-range slots, merge duplicate joints, keep the four
    /// strongest influences and sort them for deterministic serialization.
    pub fn compact_vertex(&mut self, vertex: usize) {
        if vertex >= self.vertex_count() {
            return;
        }
        let mut pairs: Vec<(u32, f32)> = self.joint_indices[vertex]
            .iter()
            .copied()
            .zip(self.weights[vertex])
            .filter(|(index, weight)| {
                (*index as usize) < self.joints.len() && weight.is_finite() && *weight > 0.0
            })
            .collect();
        pairs.sort_by(|(joint_a, weight_a), (joint_b, weight_b)| {
            weight_b
                .total_cmp(weight_a)
                .then_with(|| joint_a.cmp(joint_b))
        });
        let mut merged: Vec<(u32, f32)> = Vec::with_capacity(4);
        for (joint, weight) in pairs {
            if let Some((_, total)) = merged.iter_mut().find(|(other, _)| *other == joint) {
                *total += weight;
            } else if merged.len() < 4 {
                merged.push((joint, weight));
            }
        }
        merged.sort_by(|(joint_a, weight_a), (joint_b, weight_b)| {
            weight_b
                .total_cmp(weight_a)
                .then_with(|| joint_a.cmp(joint_b))
        });
        let mut indices = [0u32; 4];
        let mut weights = [0.0f32; 4];
        for (slot, (joint, weight)) in merged.into_iter().enumerate() {
            indices[slot] = joint;
            weights[slot] = weight;
        }
        self.joint_indices[vertex] = indices;
        self.weights[vertex] = weights;
    }

    /// Normalize a vertex to unit total influence when it has any influence.
    /// A fully cleared vertex remains unweighted and stays at its rest position.
    pub fn normalize_vertex(&mut self, vertex: usize) {
        if vertex >= self.vertex_count() {
            return;
        }
        if self.joints.is_empty() {
            self.weights[vertex] = [0.0; 4];
            return;
        }
        let mut sum = 0.0;
        for weight in &mut self.weights[vertex] {
            if !weight.is_finite() || *weight <= 0.0 {
                *weight = 0.0;
            } else {
                sum += *weight;
            }
        }
        if sum > 1e-8 {
            for weight in &mut self.weights[vertex] {
                *weight /= sum;
            }
        } else {
            self.joint_indices[vertex] = [0, 0, 0, 0];
            self.weights[vertex] = [0.0, 0.0, 0.0, 0.0];
        }
    }

    /// Repair imported or loaded weights: lengths match the mesh, joint indices
    /// are in range, duplicate/zero influences are removed, and every vertex is
    /// normalized to four or fewer influences.
    pub fn sanitize(&mut self, vertex_count: usize) {
        if self.joints.is_empty() {
            self.joint_indices.clear();
            self.weights.clear();
            return;
        }
        self.joint_indices.resize(vertex_count, [0; 4]);
        self.weights.resize(vertex_count, [0.0; 4]);
        self.joint_indices.truncate(vertex_count);
        self.weights.truncate(vertex_count);
        for vertex in 0..vertex_count {
            self.compact_vertex(vertex);
            self.normalize_vertex(vertex);
        }
    }
}

/// One recorded vertex-offset pose for a mesh. Cheetah3D calls these morph
/// targets. The manual does not document a numeric weight range; LeopardCraft's
/// clean-room choice is to clamp to `0..=1` when the pose is applied.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MorphTarget {
    pub name: String,
    #[serde(default)]
    pub deltas: Vec<Vec3>,
    #[serde(default)]
    pub weight: f32,
}

impl MorphTarget {
    pub fn new(name: impl Into<String>, deltas: Vec<Vec3>) -> Self {
        Self {
            name: name.into(),
            deltas,
            weight: 0.0,
        }
    }
}

/// A mesh's morph base and additive target poses. The base is the rest shape
/// recorded before the first target; deltas are relative to it, so a target
/// keeps its meaning while the live mesh is restored and posed again.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MorphSet {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub base: Vec<Vec3>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<MorphTarget>,
}

impl MorphSet {
    pub fn is_empty(&self) -> bool {
        self.base.is_empty() && self.targets.is_empty()
    }

    pub fn has_base(&self) -> bool {
        !self.base.is_empty()
    }

    /// First unused `Morph N` name for a newly captured target.
    pub fn unique_target_name(&self) -> String {
        for i in 1..10_000 {
            let name = format!("Morph {i}");
            if !self.targets.iter().any(|target| target.name == name) {
                return name;
            }
        }
        "Morph".into()
    }

    /// Add every target's weighted offsets to `positions`. LeopardCraft clamps
    /// weights to `0..=1` as a clean-room range choice (the Cheetah3D Morph
    /// manual leaves the range unstated); targets recorded against another
    /// vertex count are skipped so a topology change cannot corrupt evaluation.
    pub fn apply(&self, positions: &mut [Vec3]) {
        self.apply_weighted(positions, &[]);
    }

    /// Apply targets using explicit weights (sampled from a take); missing
    /// entries fall back to each target's authored weight.
    pub fn apply_weighted(&self, positions: &mut [Vec3], weights: &[f32]) {
        for (index, target) in self.targets.iter().enumerate() {
            let authored = weights.get(index).copied().unwrap_or(target.weight);
            if !authored.is_finite() {
                continue;
            }
            let weight = authored.clamp(0.0, 1.0);
            if weight <= 0.0 || target.deltas.len() != positions.len() {
                continue;
            }
            for (position, delta) in positions.iter_mut().zip(&target.deltas) {
                if delta.is_finite() {
                    *position += *delta * weight;
                }
            }
        }
    }

    /// Restore the recorded rest shape. Returns false when the live mesh no
    /// longer has the recorded vertex count.
    pub fn restore_base(&self, positions: &mut [Vec3]) -> bool {
        if self.base.len() != positions.len() {
            return false;
        }
        positions.copy_from_slice(&self.base);
        true
    }

    /// Repair imported data: finite poses, clamped weights, unique non-empty
    /// names and target topology matching the recorded base and live mesh.
    pub fn sanitize(&mut self, vertex_count: Option<usize>) {
        if !self.base.iter().all(|position| position.is_finite()) {
            self.base.clear();
            self.targets.clear();
            return;
        }
        if self.base.is_empty() {
            // Targets without a recorded base have no reference shape.
            self.targets.clear();
            return;
        }
        if !self.base.is_empty() {
            self.targets
                .retain(|target| target.deltas.len() == self.base.len());
        }
        if let Some(count) = vertex_count {
            if !self.base.is_empty() && self.base.len() != count {
                self.base.clear();
                self.targets.clear();
                return;
            }
            self.targets.retain(|target| target.deltas.len() == count);
        }
        self.targets
            .retain(|target| target.deltas.iter().all(|delta| delta.is_finite()));
        let mut used = HashSet::new();
        for (index, target) in self.targets.iter_mut().enumerate() {
            target.weight = if target.weight.is_finite() {
                target.weight.clamp(0.0, 1.0)
            } else {
                0.0
            };
            let base = if target.name.trim().is_empty() {
                format!("Morph {}", index + 1)
            } else {
                target.name.trim().to_string()
            };
            let mut name = base.clone();
            let mut suffix = 2;
            while used.contains(&name) {
                name = format!("{base} {suffix}");
                suffix += 1;
            }
            used.insert(name.clone());
            target.name = name;
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeKind {
    #[default]
    Null,
    Mesh {
        geometry: GeometrySource,
    },
    Spline {
        spline: Spline,
    },
    Camera {
        camera: Camera,
    },
    Light {
        light: Light,
    },
    Joint {
        #[serde(default = "default_length")]
        length: f32,
        #[serde(default = "default_width")]
        width: f32,
    },
}

fn default_length() -> f32 {
    0.8
}

fn default_width() -> f32 {
    0.08
}

impl NodeKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Null => "Null",
            Self::Mesh { geometry } => geometry.label(),
            Self::Spline { .. } => "Spline",
            Self::Camera { .. } => "Camera",
            Self::Light { .. } => "Light",
            Self::Joint { .. } => "Joint",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Null => "○",
            Self::Mesh { .. } => "◆",
            Self::Spline { .. } => "∿",
            Self::Camera { .. } => "▣",
            Self::Light { .. } => "☀",
            Self::Joint { .. } => "⌁",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    #[serde(default)]
    pub parent: Option<NodeId>,
    #[serde(default)]
    pub children: Vec<NodeId>,
    #[serde(default)]
    pub transform: lc_core::Transform,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_true")]
    pub renderable: bool,
    #[serde(default)]
    pub layer: u8,
    #[serde(default)]
    pub kind: NodeKind,
    #[serde(default)]
    pub material: Option<MaterialId>,
    /// Sparse per-slot material overrides (OBJ/glTF material slots).
    #[serde(default)]
    pub material_slots: BTreeMap<u16, MaterialId>,
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(default)]
    pub animation: AnimationClip,
    /// Per-tag clips keyed by tag index. Tag scope channels live here so two
    /// Copy Location tags on one object animate independently.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tag_animations: BTreeMap<u32, AnimationClip>,
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
    #[serde(default)]
    pub skin: Option<Skin>,
    #[serde(default, skip_serializing_if = "MorphSet::is_empty")]
    pub morph: MorphSet,
    /// Optional IK chain driven from this joint's origin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ik: Option<IkChain>,
    /// Optional local rotation limits for this Joint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_limits: Option<JointLimits>,
    #[serde(default)]
    pub locked: bool,
}

impl Node {
    pub fn new(id: NodeId, name: impl Into<String>, kind: NodeKind) -> Self {
        Self {
            id,
            name: name.into(),
            parent: None,
            children: Vec::new(),
            transform: lc_core::Transform::IDENTITY,
            visible: true,
            renderable: true,
            layer: 0,
            kind,
            material: None,
            material_slots: BTreeMap::new(),
            tags: Vec::new(),
            animation: AnimationClip::default(),
            tag_animations: BTreeMap::new(),
            modifiers: Vec::new(),
            skin: None,
            morph: MorphSet::default(),
            ik: None,
            rotation_limits: None,
            locked: false,
        }
    }

    pub fn with_transform(mut self, transform: lc_core::Transform) -> Self {
        self.transform = transform;
        self
    }

    pub fn with_material(mut self, material: MaterialId) -> Self {
        self.material = Some(material);
        self
    }

    pub fn with_tag(mut self, tag: Tag) -> Self {
        self.tags.push(tag);
        self
    }

    /// Remove a tag and shift the per-tag clips that followed it so animation
    /// keeps pointing at the same tag.
    pub fn remove_tag(&mut self, index: usize) -> Option<Tag> {
        if index >= self.tags.len() {
            return None;
        }
        let removed = self.tags.remove(index);
        let mut remapped = BTreeMap::new();
        for (key, clip) in std::mem::take(&mut self.tag_animations) {
            if key < index as u32 {
                remapped.insert(key, clip);
            } else if key > index as u32 {
                remapped.insert(key - 1, clip);
            }
        }
        self.tag_animations = remapped;
        Some(removed)
    }

    pub fn mesh(id: NodeId, name: impl Into<String>, geometry: GeometrySource) -> Self {
        Self::new(id, name, NodeKind::Mesh { geometry })
    }

    pub fn material_for_slot(&self, slot: u16) -> Option<MaterialId> {
        self.material_slots.get(&slot).copied().or(self.material)
    }

    pub fn effective_material(&self) -> Option<MaterialId> {
        self.tags
            .iter()
            .find_map(|t| match t {
                Tag::MaterialOverride { material } => Some(*material),
                _ => None,
            })
            .or(self.material)
    }

    pub fn is_mesh(&self) -> bool {
        matches!(self.kind, NodeKind::Mesh { .. })
    }

    pub fn is_spline(&self) -> bool {
        matches!(self.kind, NodeKind::Spline { .. })
    }

    pub fn has_animation(&self) -> bool {
        !self.animation.is_empty()
    }
}

/// Editable subset of node fields, used by undo commands and the inspector.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeProps {
    pub visible: bool,
    pub renderable: bool,
    pub layer: u8,
    pub locked: bool,
    pub material: Option<MaterialId>,
    pub material_slots: BTreeMap<u16, MaterialId>,
    pub tags: Vec<Tag>,
    /// Per-tag clips keyed by tag index. They travel with the tag list so
    /// removing a tag in the inspector (or undoing that removal) keeps every
    /// clip attached to the tag it was recorded on.
    pub tag_animations: BTreeMap<u32, AnimationClip>,
    pub ik: Option<IkChain>,
    pub rotation_limits: Option<JointLimits>,
}

impl NodeProps {
    pub fn of(node: &Node) -> Self {
        Self {
            visible: node.visible,
            renderable: node.renderable,
            layer: node.layer,
            locked: node.locked,
            material: node.material,
            material_slots: node.material_slots.clone(),
            tags: node.tags.clone(),
            tag_animations: node.tag_animations.clone(),
            ik: node.ik.clone(),
            rotation_limits: node.rotation_limits.clone(),
        }
    }

    pub fn apply(&self, node: &mut Node) {
        node.visible = self.visible;
        node.renderable = self.renderable;
        node.layer = self.layer;
        node.locked = self.locked;
        node.material = self.material;
        node.material_slots = self.material_slots.clone();
        node.tags = self.tags.clone();
        node.tag_animations = self.tag_animations.clone();
        node.ik = self.ik.clone();
        node.rotation_limits = self.rotation_limits.clone();
    }
}

#[derive(Clone, Debug)]
pub struct Subtree {
    pub root: NodeId,
    pub nodes: Vec<Node>,
    pub parent: Option<NodeId>,
    pub index: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub nodes: BTreeMap<NodeId, Node>,
    #[serde(default)]
    pub roots: Vec<NodeId>,
    #[serde(default)]
    pub next_id: NodeId,
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

impl Scene {
    pub fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            roots: Vec::new(),
            next_id: 1,
        }
    }

    pub fn alloc_id(&mut self) -> NodeId {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        id
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn insert_node(
        &mut self,
        mut node: Node,
        parent: Option<NodeId>,
        index: Option<usize>,
    ) -> Result<NodeId, String> {
        if node.id == 0 {
            node.id = self.alloc_id();
        }
        let id = node.id;
        if self.nodes.contains_key(&id) {
            return Err(format!("node {id} already exists"));
        }
        if !node.children.is_empty() {
            return Err("use insert_subtree for nodes with children".into());
        }
        if let Some(p) = parent {
            let parent_node = self
                .nodes
                .get_mut(&p)
                .ok_or_else(|| format!("parent {p} does not exist"))?;
            let idx = index.unwrap_or(parent_node.children.len());
            let idx = idx.min(parent_node.children.len());
            parent_node.children.insert(idx, node.id);
            node.parent = Some(p);
        } else {
            let idx = index.unwrap_or(self.roots.len());
            let idx = idx.min(self.roots.len());
            self.roots.insert(idx, node.id);
            node.parent = None;
        }
        self.next_id = self.next_id.max(id + 1);
        self.nodes.insert(id, node);
        Ok(id)
    }

    /// Insert a detached subtree (or the subtree returned by
    /// [`Scene::remove_subtree`] / duplicate) preserving ids.
    pub fn insert_subtree(
        &mut self,
        nodes: Vec<Node>,
        parent: Option<NodeId>,
        index: Option<usize>,
    ) -> Result<NodeId, String> {
        let Some(root) = nodes.first().map(|n| n.id) else {
            return Err("empty subtree".into());
        };
        let ids: HashSet<NodeId> = nodes.iter().map(|n| n.id).collect();
        if ids.len() != nodes.len() {
            return Err("subtree has duplicate ids".into());
        }
        for node in &nodes {
            if self.nodes.contains_key(&node.id) {
                return Err(format!("node {} already exists", node.id));
            }
        }
        // Validate every reference before touching the scene so a rejected
        // insert cannot leave a dangling child link behind.
        for node in &nodes {
            for child in &node.children {
                if !ids.contains(child) && !self.nodes.contains_key(child) {
                    return Err(format!("subtree references missing child {child}"));
                }
            }
        }
        if let Some(p) = parent {
            let parent_node = self
                .nodes
                .get_mut(&p)
                .ok_or_else(|| format!("parent {p} does not exist"))?;
            let idx = index.unwrap_or(parent_node.children.len());
            let idx = idx.min(parent_node.children.len());
            parent_node.children.insert(idx, root);
        } else {
            let idx = index.unwrap_or(self.roots.len());
            let idx = idx.min(self.roots.len());
            self.roots.insert(idx, root);
        }
        let mut inserted = nodes;
        // Repair parent pointers from the authoritative child lists.
        let mut parent_map: std::collections::HashMap<NodeId, NodeId> =
            std::collections::HashMap::new();
        for n in &inserted {
            for c in &n.children {
                parent_map.insert(*c, n.id);
            }
        }
        for node in &mut inserted {
            node.parent = if node.id == root {
                parent
            } else {
                parent_map.get(&node.id).copied()
            };
            self.next_id = self.next_id.max(node.id + 1);
        }
        for node in inserted {
            self.nodes.insert(node.id, node);
        }
        Ok(root)
    }

    /// Clone a subtree without removing it (used by undo delete commands).
    pub fn collect_subtree(&self, id: NodeId) -> Option<Subtree> {
        let node = self.nodes.get(&id)?;
        let parent = node.parent;
        let index = if let Some(p) = parent {
            self.nodes
                .get(&p)
                .and_then(|p| p.children.iter().position(|&c| c == id))
                .unwrap_or(0)
        } else {
            self.roots.iter().position(|&r| r == id).unwrap_or(0)
        };
        let nodes = self
            .depth_first_from(id)
            .into_iter()
            .filter_map(|nid| self.nodes.get(&nid).cloned())
            .collect();
        Some(Subtree {
            root: id,
            nodes,
            parent,
            index,
        })
    }

    pub fn remove_subtree(&mut self, id: NodeId) -> Option<Subtree> {
        let node = self.nodes.get(&id)?;
        let parent = node.parent;
        let index = if let Some(p) = parent {
            self.nodes
                .get(&p)
                .and_then(|p| p.children.iter().position(|&c| c == id))
                .unwrap_or(0)
        } else {
            self.roots.iter().position(|&r| r == id).unwrap_or(0)
        };
        if let Some(p) = parent {
            if let Some(pn) = self.nodes.get_mut(&p) {
                pn.children.retain(|&c| c != id);
            }
        } else {
            self.roots.retain(|&r| r != id);
        }
        let ids = self.depth_first_from(id);
        let mut nodes = Vec::with_capacity(ids.len());
        for nid in ids {
            if let Some(n) = self.nodes.remove(&nid) {
                nodes.push(n);
            }
        }
        Some(Subtree {
            root: id,
            nodes,
            parent,
            index,
        })
    }

    /// Deterministic depth-first order, roots in authored order.
    pub fn depth_first(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut visited = HashSet::new();
        let mut stack: Vec<NodeId> = self.roots.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            let Some(node) = self.nodes.get(&id) else {
                continue;
            };
            out.push(id);
            for &child in node.children.iter().rev() {
                stack.push(child);
            }
        }
        // Include orphaned nodes defensively so the UI never hides data.
        let mut orphans: Vec<NodeId> = self
            .nodes
            .keys()
            .copied()
            .filter(|id| !visited.contains(id))
            .collect();
        orphans.sort();
        for id in orphans {
            if visited.insert(id) {
                out.push(id);
            }
        }
        out
    }

    pub fn depth_first_from(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut visited = HashSet::new();
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            let Some(node) = self.nodes.get(&id) else {
                continue;
            };
            out.push(id);
            for &child in node.children.iter().rev() {
                stack.push(child);
            }
        }
        out
    }

    pub fn parent_chain(&self, id: NodeId) -> Vec<NodeId> {
        let mut chain = Vec::new();
        let mut current = self.get(id).and_then(|n| n.parent);
        let mut guard = 0;
        while let Some(p) = current {
            if !chain.contains(&p) {
                chain.push(p);
            } else {
                break;
            }
            current = self.get(p).and_then(|n| n.parent);
            guard += 1;
            if guard > 10_000 {
                break;
            }
        }
        chain
    }

    pub fn is_ancestor(&self, ancestor: NodeId, node: NodeId) -> bool {
        if ancestor == node {
            return true;
        }
        self.parent_chain(node).contains(&ancestor)
    }

    pub fn world_matrix_plain(&self, id: NodeId) -> Mat4 {
        let mut chain = Vec::new();
        let mut current = Some(id);
        let mut guard = 0;
        while let Some(nid) = current {
            let Some(node) = self.get(nid) else {
                break;
            };
            chain.push(node.transform.matrix());
            current = node.parent;
            guard += 1;
            if guard > 10_000 || chain.len() > 10_000 {
                break;
            }
        }
        let mut m = Mat4::IDENTITY;
        for local in chain.iter().rev() {
            m *= *local;
        }
        m
    }

    pub fn reparent(
        &mut self,
        id: NodeId,
        new_parent: Option<NodeId>,
        index: Option<usize>,
    ) -> Result<(), String> {
        if let Some(p) = new_parent {
            if !self.nodes.contains_key(&p) {
                return Err("target parent does not exist".into());
            }
            if p == id || self.is_ancestor(id, p) {
                return Err("cannot parent a node to itself or its descendant".into());
            }
        }
        let old_parent = self.nodes.get(&id).and_then(|n| n.parent);
        if let Some(p) = old_parent {
            if let Some(pn) = self.nodes.get_mut(&p) {
                pn.children.retain(|&c| c != id);
            }
        } else {
            self.roots.retain(|&r| r != id);
        }
        if let Some(p) = new_parent {
            if let Some(pn) = self.nodes.get_mut(&p) {
                let idx = index.unwrap_or(pn.children.len()).min(pn.children.len());
                pn.children.insert(idx, id);
            }
        } else {
            let idx = index.unwrap_or(self.roots.len()).min(self.roots.len());
            self.roots.insert(idx, id);
        }
        if let Some(node) = self.nodes.get_mut(&id) {
            node.parent = new_parent;
        }
        Ok(())
    }

    pub fn duplicate_subtree(&mut self, id: NodeId) -> Option<NodeId> {
        let original = self.nodes.get(&id)?;
        let parent = original.parent;
        let index = if let Some(p) = parent {
            self.nodes
                .get(&p)
                .and_then(|p| p.children.iter().position(|&c| c == id))
                .map(|i| i + 1)
                .unwrap_or(0)
        } else {
            self.roots
                .iter()
                .position(|&r| r == id)
                .map(|i| i + 1)
                .unwrap_or(0)
        };
        let ids = self.depth_first_from(id);
        let mut map: BTreeMap<NodeId, NodeId> = BTreeMap::new();
        for nid in &ids {
            map.insert(*nid, self.alloc_id());
        }
        let root_new = *map.get(&id)?;
        let mut clones = Vec::with_capacity(ids.len());
        for nid in &ids {
            let mut node = self.nodes.get(nid)?.clone();
            node.id = *map.get(nid)?;
            node.children = node
                .children
                .iter()
                .filter_map(|c| map.get(c).copied())
                .collect();
            node.parent = node.parent.and_then(|p| map.get(&p).copied());
            clones.push(node);
        }
        if let Some(root) = clones.first_mut() {
            root.name = self.unique_name(&format!("{} copy", root.name));
            root.parent = parent;
        }
        let fallback_name = self
            .nodes
            .get(&id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Object".into());
        let _ = (root_new, fallback_name);
        self.insert_subtree(clones, parent, Some(index)).ok()
    }

    pub fn unique_name(&self, base: &str) -> String {
        let base = base.trim();
        let base = if base.is_empty() { "Object" } else { base };
        if !self.nodes.values().any(|n| n.name == base) {
            return base.to_string();
        }
        for i in 2..10_000 {
            let candidate = format!("{base} {i}");
            if !self.nodes.values().any(|n| n.name == candidate) {
                return candidate;
            }
        }
        format!("{base} copy")
    }

    pub fn find_by_name(&self, name: &str) -> Option<NodeId> {
        self.nodes.values().find(|n| n.name == name).map(|n| n.id)
    }

    pub fn signature(&self) -> u64 {
        use lc_core::StableHasher;
        let mut h = StableHasher::new();
        for id in self.depth_first() {
            if let Some(n) = self.get(id) {
                h.write_u64(n.id);
                h.write_str(&n.name);
                h.write_u64(n.parent.unwrap_or(0));
                h.write_u64(n.animation.signature());
                for (tag_index, clip) in &n.tag_animations {
                    h.write_u32(*tag_index);
                    h.write_u64(clip.signature());
                }
                if let Some(ik) = &n.ik {
                    h.write_u64(ik.target);
                    h.write_u64(ik.pole.unwrap_or(0));
                    h.write_u64(ik.chain_length as u64);
                    h.write_u64(ik.iterations as u64);
                    h.write_f32(ik.tolerance);
                    h.write_f32(ik.damping);
                    h.write_u64(ik.enabled as u64);
                }
                if let Some(limits) = &n.rotation_limits {
                    h.write_u64(limits.enabled as u64);
                    for value in [
                        limits.min.x,
                        limits.min.y,
                        limits.min.z,
                        limits.max.x,
                        limits.max.y,
                        limits.max.z,
                    ] {
                        h.write_f32(value);
                    }
                }
            }
        }
        h.finish()
    }
}

fn point_segment_distance(point: Vec3, head: Vec3, tail: Vec3) -> f32 {
    let segment = tail - head;
    let length_sq = segment.length_squared();
    if length_sq <= 1e-12 {
        return (point - head).length();
    }
    let t = ((point - head).dot(segment) / length_sq).clamp(0.0, 1.0);
    (point - (head + segment * t)).length()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Primitive;
    use lc_core::Transform;

    fn mesh_node(scene: &mut Scene, name: &str, parent: Option<NodeId>) -> NodeId {
        let id = scene.alloc_id();
        let node = Node::mesh(
            id,
            name,
            GeometrySource::from_primitive(Primitive::Box {
                width: 1.0,
                height: 1.0,
                depth: 1.0,
            }),
        );
        scene.insert_node(node, parent, None).unwrap();
        id
    }

    #[test]
    fn hierarchy_orders_and_world_matrices() {
        let mut scene = Scene::new();
        let root = mesh_node(&mut scene, "Root", None);
        let child = mesh_node(&mut scene, "Child", Some(root));
        scene.get_mut(root).unwrap().transform =
            Transform::from_translation(Vec3::new(1.0, 0.0, 0.0));
        scene.get_mut(child).unwrap().transform =
            Transform::from_translation(Vec3::new(0.0, 2.0, 0.0));
        assert_eq!(scene.depth_first(), vec![root, child]);
        let world = scene.world_matrix_plain(child);
        assert!((world.w_axis.truncate() - Vec3::new(1.0, 2.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn reparent_rejects_cycles() {
        let mut scene = Scene::new();
        let a = mesh_node(&mut scene, "A", None);
        let b = mesh_node(&mut scene, "B", Some(a));
        assert!(scene.reparent(a, Some(b), None).is_err());
        assert!(scene.reparent(b, None, None).is_ok());
        assert_eq!(scene.roots.len(), 2);
        assert_eq!(scene.get(b).unwrap().parent, None);
    }

    #[test]
    fn remove_and_reinsert_subtree() {
        let mut scene = Scene::new();
        let a = mesh_node(&mut scene, "A", None);
        let b = mesh_node(&mut scene, "B", Some(a));
        let subtree = scene.remove_subtree(a).unwrap();
        assert_eq!(subtree.nodes.len(), 2);
        assert!(scene.get(a).is_none());
        let root = scene.insert_subtree(subtree.nodes, None, Some(0)).unwrap();
        assert_eq!(root, a);
        assert!(scene.get(b).is_some());
        assert_eq!(scene.depth_first(), vec![a, b]);
    }

    #[test]
    fn rejected_subtree_insert_leaves_the_scene_untouched() {
        let mut scene = Scene::new();
        let root = mesh_node(&mut scene, "Root", None);
        let mut orphan = Node::new(scene.alloc_id(), "Orphan", NodeKind::Null);
        orphan.children.push(999);
        let before = scene.clone();

        assert!(scene
            .insert_subtree(vec![orphan], Some(root), None)
            .is_err());
        assert_eq!(scene, before, "a rejected insert must not mutate the scene");
        assert!(scene.get(root).unwrap().children.is_empty());
        assert_eq!(scene.roots, vec![root]);
    }

    #[test]
    fn duplicate_remaps_ids_and_names() {
        let mut scene = Scene::new();
        let a = mesh_node(&mut scene, "Cube", None);
        let b = mesh_node(&mut scene, "Child", Some(a));
        let copy = scene.duplicate_subtree(a).unwrap();
        assert_ne!(copy, a);
        assert_eq!(scene.nodes.len(), 4);
        assert_ne!(scene.get(copy).unwrap().name, "Cube");
        assert_eq!(scene.get(copy).unwrap().children.len(), 1);
        let copied_child = scene.get(copy).unwrap().children[0];
        assert_ne!(copied_child, b);
        assert_eq!(scene.get(copied_child).unwrap().parent, Some(copy));
    }

    #[test]
    fn skin_painting_normalizes_influences_and_keeps_four_slots() {
        let mut skin = Skin::new(vec![10, 11, 12, 13, 14], vec![Mat4::IDENTITY; 5]);
        skin.bind_all_to_first(2);
        assert_eq!(skin.influence(0, 0), 1.0);
        assert_eq!(skin.influence(0, 1), 0.0);

        assert!(skin.set_influence(0, 1, 1.0));
        assert!((skin.influence(0, 0) - 0.5).abs() < 1e-6);
        assert!((skin.influence(0, 1) - 0.5).abs() < 1e-6);

        for joint in 2..5 {
            assert!(skin.set_influence(0, joint, 0.4));
        }
        let mut total = 0.0;
        let mut used = std::collections::BTreeSet::new();
        for (index, weight) in skin.joint_indices[0].iter().zip(skin.weights[0]) {
            assert!((*index as usize) < skin.joints.len());
            assert!(used.insert(*index), "a joint cannot occupy two slots");
            assert!(weight >= 0.0);
            total += weight;
        }
        assert!((total - 1.0).abs() < 1e-6);
        assert_eq!(used.len(), 4);
    }

    #[test]
    fn skin_sanitize_repairs_lengths_indices_weights_and_duplicates() {
        let mut skin = Skin::new(vec![10, 11, 12], vec![Mat4::IDENTITY; 3]);
        skin.joint_indices = vec![[0, 0, 1, 0], [9, 2, 2, 0], [1, 0, 0, 0], [0, 0, 0, 0]];
        skin.weights = vec![
            [0.25, 0.25, 0.5, 0.0],
            [1.0, 0.5, 0.5, 0.0],
            [-1.0, 0.0, 0.0, 0.0],
            [f32::NAN, 0.0, 0.0, 0.0],
        ];
        skin.sanitize(3);
        assert_eq!(skin.joint_indices.len(), 3);
        assert_eq!(skin.weights.len(), 3);
        for (indices, weights) in skin.joint_indices.iter().zip(&skin.weights) {
            let total: f32 = weights.iter().sum();
            assert!(total < 1e-6 || (total - 1.0).abs() < 1e-6);
            let mut used = std::collections::BTreeSet::new();
            for (index, weight) in indices.iter().zip(weights) {
                if *weight <= 0.0 {
                    continue;
                }
                assert!((*index as usize) < skin.joints.len());
                assert!(used.insert(*index), "duplicate joint slots must merge");
            }
        }
        assert_eq!(skin.influence(2, 0), 0.0);
    }

    #[test]
    fn proximity_weights_are_normalized_bounded_and_nearest() {
        let mut skin = Skin::new(vec![10, 11, 12], vec![Mat4::IDENTITY; 3]);
        let positions = [
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 2.5, 0.0),
        ];
        let segments = [
            (Vec3::new(-2.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0)),
            (Vec3::new(1.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0)),
            (Vec3::new(0.0, 2.0, 0.0), Vec3::new(0.0, 3.0, 0.0)),
        ];
        assert!(skin.auto_weight_proximity(
            &positions,
            &segments,
            ProximityWeightOptions {
                dropoff: 2.0,
                strength: 0.6,
                max_influences: 2,
            },
        ));
        for weights in skin.weights.iter().take(positions.len()) {
            let total: f32 = weights.iter().sum();
            assert!((total - 1.0).abs() < 1e-5);
            assert!(weights.iter().filter(|w| **w > 0.0).count() <= 2);
        }
        assert!(skin.influence(0, 0) > skin.influence(0, 1));
        assert!(skin.influence(1, 1) > skin.influence(1, 0));
        assert!(skin.influence(2, 2) > skin.influence(2, 0));

        let bind = |dropoff: f32, strength: f32, max_influences: usize| {
            let mut variant = Skin::new(vec![10, 11, 12], vec![Mat4::IDENTITY; 3]);
            assert!(variant.auto_weight_proximity(
                &positions,
                &segments,
                ProximityWeightOptions {
                    dropoff,
                    strength,
                    max_influences,
                },
            ));
            variant
        };

        let single = bind(2.0, 0.6, 1);
        for weights in single.weights.iter().take(positions.len()) {
            assert_eq!(weights.iter().filter(|weight| **weight > 0.0).count(), 1);
        }
        assert_ne!(
            single.weights[0], skin.weights[0],
            "max influences must change how many joints contribute"
        );

        let soft = bind(2.0, 0.1, 2);
        let sharp = bind(2.0, 0.9, 2);
        assert!(
            soft.influence(0, 1) > sharp.influence(0, 1),
            "higher strength must reduce a distant joint's influence"
        );

        let shallow = bind(0.5, 0.6, 2);
        let steep = bind(4.0, 0.6, 2);
        assert!(
            shallow.influence(0, 1) > steep.influence(0, 1),
            "higher dropoff must reduce a distant joint's influence"
        );
    }

    #[test]
    fn cleared_weights_stay_unweighted_instead_of_falling_to_the_root() {
        let mut skin = Skin::new(vec![10, 11], vec![Mat4::IDENTITY; 2]);
        skin.clear_weights(2);
        assert_eq!(skin.influence(0, 0), 0.0);
        skin.normalize_vertex(0);
        assert_eq!(skin.influence(0, 0), 0.0);
        assert_eq!(skin.weights[0], [0.0; 4]);
        skin.clear_weights(2);
        skin.sanitize(2);
        assert_eq!(skin.influence(0, 0), 0.0);
        assert_eq!(skin.weights[0], [0.0; 4]);
    }
}
