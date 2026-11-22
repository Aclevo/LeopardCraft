//! Scene graph: hierarchical nodes, transforms, tags and skins.

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
        for node in &inserted {
            for child in &node.children {
                if !ids.contains(child) && !self.nodes.contains_key(child) {
                    return Err(format!("subtree references missing child {child}"));
                }
            }
        }
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
            }
        }
        h.finish()
    }
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
}
