//! Non-destructive undo/redo command stack.
//!
//! Commands carry the exact data needed to reverse themselves, so undo never
//! rebuilds a scene from a lossy log. Subtree deletes preserve ids, parenting
//! and ordering.

use crate::{
    Document, DocumentSettings, Modifier, MorphSet, Node, NodeId, NodeKind, NodeProps, Skin,
    Subtree,
};
use lc_anim::AnimationClip;
use lc_core::Transform;
use lc_shading::{Material, MaterialId};

pub trait Command: Send {
    fn label(&self) -> String;
    fn apply(&mut self, doc: &mut Document);
    fn undo(&mut self, doc: &mut Document);

    /// Optional editor metadata carried by a command, so UI state such as a
    /// key selection can be restored alongside the document edit.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }
}

#[derive(Default)]
pub struct History {
    undo_stack: Vec<Box<dyn Command>>,
    redo_stack: Vec<Box<dyn Command>>,
    limit: usize,
}

impl History {
    pub fn new(limit: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            limit,
        }
    }

    pub fn execute(&mut self, doc: &mut Document, mut command: Box<dyn Command>) -> String {
        command.apply(doc);
        let label = command.label();
        self.undo_stack.push(command);
        self.redo_stack.clear();
        while self.undo_stack.len() > self.limit.max(1) {
            self.undo_stack.remove(0);
        }
        label
    }

    pub fn undo(&mut self, doc: &mut Document) -> Option<String> {
        let mut command = self.undo_stack.pop()?;
        command.undo(doc);
        let label = command.label();
        self.redo_stack.push(command);
        Some(label)
    }

    pub fn redo(&mut self, doc: &mut Document) -> Option<String> {
        let mut command = self.redo_stack.pop()?;
        command.apply(doc);
        let label = command.label();
        self.undo_stack.push(command);
        Some(label)
    }

    /// The command at the top of the undo stack. After a redo, this is the
    /// command that was just reapplied.
    pub fn undo_top(&self) -> Option<&dyn Command> {
        self.undo_stack.last().map(|command| command.as_ref())
    }

    /// The command at the top of the redo stack. After an undo, this is the
    /// command that was just reversed.
    pub fn redo_top(&self) -> Option<&dyn Command> {
        self.redo_stack.last().map(|command| command.as_ref())
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo_label(&self) -> Option<String> {
        self.undo_stack.last().map(|c| c.label())
    }

    pub fn redo_label(&self) -> Option<String> {
        self.redo_stack.last().map(|c| c.label())
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn depth(&self) -> usize {
        self.undo_stack.len()
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.max(1);
        while self.undo_stack.len() > self.limit {
            self.undo_stack.remove(0);
        }
    }
}

pub struct AddNodeCmd {
    node: Node,
    parent: Option<NodeId>,
    index: usize,
}

impl AddNodeCmd {
    pub fn new(doc: &Document, node: Node, parent: Option<NodeId>) -> Self {
        let index = parent
            .and_then(|p| doc.scene.get(p).map(|n| n.children.len()))
            .unwrap_or(doc.scene.roots.len());
        Self {
            node,
            parent,
            index,
        }
    }
}

impl Command for AddNodeCmd {
    fn label(&self) -> String {
        format!("Add {}", self.node.name)
    }

    fn apply(&mut self, doc: &mut Document) {
        let _ = doc
            .scene
            .insert_node(self.node.clone(), self.parent, Some(self.index));
    }

    fn undo(&mut self, doc: &mut Document) {
        doc.scene.remove_subtree(self.node.id);
    }
}

/// One undo step for an action that inserts several objects at once, such as
/// splitting a component selection into new meshes.
pub struct AddNodesCmd {
    pub label: String,
    /// Nodes paired with the parent each one is appended to.
    pub nodes: Vec<(Node, Option<NodeId>)>,
}

impl Command for AddNodesCmd {
    fn label(&self) -> String {
        self.label.clone()
    }

    fn apply(&mut self, doc: &mut Document) {
        for (node, parent) in &self.nodes {
            let _ = doc.scene.insert_node(node.clone(), *parent, None);
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        for (node, _) in self.nodes.iter().rev() {
            doc.scene.remove_subtree(node.id);
        }
    }
}

pub struct DeleteNodeCmd {
    id: NodeId,
    subtree: Option<Subtree>,
}

impl DeleteNodeCmd {
    pub fn new(doc: &Document, id: NodeId) -> Self {
        Self {
            id,
            subtree: doc.scene.collect_subtree(id),
        }
    }
}

impl Command for DeleteNodeCmd {
    fn label(&self) -> String {
        self.subtree
            .as_ref()
            .and_then(|s| s.nodes.first())
            .map(|n| format!("Delete {}", n.name))
            .unwrap_or_else(|| "Delete".into())
    }

    fn apply(&mut self, doc: &mut Document) {
        if self.subtree.is_none() {
            self.subtree = doc.scene.collect_subtree(self.id);
        }
        doc.scene.remove_subtree(self.id);
    }

    fn undo(&mut self, doc: &mut Document) {
        if let Some(subtree) = &self.subtree {
            let _ = doc.scene.insert_subtree(
                subtree.nodes.clone(),
                subtree.parent,
                Some(subtree.index),
            );
        }
    }
}

pub struct SetTransformCmd {
    pub id: NodeId,
    pub before: Transform,
    pub after: Transform,
}

impl SetTransformCmd {
    pub fn new(doc: &Document, id: NodeId, after: Transform) -> Option<Self> {
        let before = doc.scene.get(id)?.transform;
        Some(Self { id, before, after })
    }
}

impl Command for SetTransformCmd {
    fn label(&self) -> String {
        "Transform".into()
    }

    fn apply(&mut self, doc: &mut Document) {
        if let Some(n) = doc.scene.get_mut(self.id) {
            n.transform = self.after;
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        if let Some(n) = doc.scene.get_mut(self.id) {
            n.transform = self.before;
        }
    }
}

/// One undo step for a shared-pivot or multi-object transform gesture.
pub struct SetTransformsCmd {
    pub edits: Vec<(NodeId, Transform, Transform)>,
}

impl Command for SetTransformsCmd {
    fn label(&self) -> String {
        match self.edits.len() {
            0 | 1 => "Transform".into(),
            count => format!("Transform {count} Objects"),
        }
    }

    fn apply(&mut self, doc: &mut Document) {
        for (id, _, after) in &self.edits {
            if let Some(n) = doc.scene.get_mut(*id) {
                n.transform = *after;
            }
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        for (id, before, _) in &self.edits {
            if let Some(n) = doc.scene.get_mut(*id) {
                n.transform = *before;
            }
        }
    }
}

/// One undo step for a batch visibility, render flag or lock change.
pub struct SetNodesPropsCmd {
    pub edits: Vec<(NodeId, NodeProps, NodeProps)>,
}

impl Command for SetNodesPropsCmd {
    fn label(&self) -> String {
        match self.edits.len() {
            0 | 1 => "Edit Properties".into(),
            count => format!("Edit Properties on {count} Objects"),
        }
    }

    fn apply(&mut self, doc: &mut Document) {
        for (id, _, after) in &self.edits {
            if let Some(n) = doc.scene.get_mut(*id) {
                after.apply(n);
            }
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        for (id, before, _) in &self.edits {
            if let Some(n) = doc.scene.get_mut(*id) {
                before.apply(n);
            }
        }
    }
}

pub struct RenameNodeCmd {
    pub id: NodeId,
    pub before: String,
    pub after: String,
}

impl RenameNodeCmd {
    pub fn new(doc: &Document, id: NodeId, after: String) -> Option<Self> {
        let before = doc.scene.get(id)?.name.clone();
        Some(Self { id, before, after })
    }
}

impl Command for RenameNodeCmd {
    fn label(&self) -> String {
        "Rename".into()
    }

    fn apply(&mut self, doc: &mut Document) {
        if let Some(n) = doc.scene.get_mut(self.id) {
            n.name = self.after.clone();
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        if let Some(n) = doc.scene.get_mut(self.id) {
            n.name = self.before.clone();
        }
    }
}

pub struct SetNodePropsCmd {
    pub id: NodeId,
    pub before: NodeProps,
    pub after: NodeProps,
}

impl SetNodePropsCmd {
    pub fn new(doc: &Document, id: NodeId, after: NodeProps) -> Option<Self> {
        let before = NodeProps::of(doc.scene.get(id)?);
        Some(Self { id, before, after })
    }
}

impl Command for SetNodePropsCmd {
    fn label(&self) -> String {
        "Edit Properties".into()
    }

    fn apply(&mut self, doc: &mut Document) {
        if let Some(n) = doc.scene.get_mut(self.id) {
            self.after.apply(n);
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        if let Some(n) = doc.scene.get_mut(self.id) {
            self.before.apply(n);
        }
    }
}

#[derive(Clone, Debug)]
pub struct Shape {
    pub kind: NodeKind,
    pub modifiers: Vec<Modifier>,
    pub skin: Option<Skin>,
    pub morph: MorphSet,
}

impl Shape {
    pub fn of(node: &Node) -> Self {
        Self {
            kind: node.kind.clone(),
            modifiers: node.modifiers.clone(),
            skin: node.skin.clone(),
            morph: node.morph.clone(),
        }
    }
}

pub struct SetShapeCmd {
    pub id: NodeId,
    pub before: Shape,
    pub after: Shape,
}

impl Command for SetShapeCmd {
    fn label(&self) -> String {
        "Edit Geometry".into()
    }

    fn apply(&mut self, doc: &mut Document) {
        if let Some(n) = doc.scene.get_mut(self.id) {
            n.kind = self.after.kind.clone();
            n.modifiers = self.after.modifiers.clone();
            n.skin = self.after.skin.clone();
            n.morph = self.after.morph.clone();
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        if let Some(n) = doc.scene.get_mut(self.id) {
            n.kind = self.before.kind.clone();
            n.modifiers = self.before.modifiers.clone();
            n.skin = self.before.skin.clone();
            n.morph = self.before.morph.clone();
        }
    }
}

/// One undo step for morph targets, names and weights.
pub struct SetMorphCmd {
    pub id: NodeId,
    pub before: MorphSet,
    pub after: MorphSet,
}

impl Command for SetMorphCmd {
    fn label(&self) -> String {
        "Edit Morph".into()
    }

    fn apply(&mut self, doc: &mut Document) {
        if let Some(node) = doc.scene.get_mut(self.id) {
            node.morph = self.after.clone();
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        if let Some(node) = doc.scene.get_mut(self.id) {
            node.morph = self.before.clone();
        }
    }
}

/// One undo step for a shape change that covers every object in a shared
/// selection. Single-object edits keep the historic `"Edit Geometry"` label.
pub struct SetShapesCmd {
    pub edits: Vec<(NodeId, Shape, Shape)>,
}

impl Command for SetShapesCmd {
    fn label(&self) -> String {
        match self.edits.len() {
            0 | 1 => "Edit Geometry".into(),
            count => format!("Edit Geometry on {count} Objects"),
        }
    }

    fn apply(&mut self, doc: &mut Document) {
        for (id, _, after) in &self.edits {
            if let Some(n) = doc.scene.get_mut(*id) {
                n.kind = after.kind.clone();
                n.modifiers = after.modifiers.clone();
                n.skin = after.skin.clone();
                n.morph = after.morph.clone();
            }
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        for (id, before, _) in &self.edits {
            if let Some(n) = doc.scene.get_mut(*id) {
                n.kind = before.kind.clone();
                n.modifiers = before.modifiers.clone();
                n.skin = before.skin.clone();
                n.morph = before.morph.clone();
            }
        }
    }
}

pub struct SetAnimationCmd {
    pub id: NodeId,
    pub before: AnimationClip,
    pub after: AnimationClip,
}

impl SetAnimationCmd {
    pub fn new(doc: &Document, id: NodeId, after: AnimationClip) -> Option<Self> {
        let before = doc.scene.get(id)?.animation.clone();
        Some(Self { id, before, after })
    }
}

impl Command for SetAnimationCmd {
    fn label(&self) -> String {
        "Edit Animation".into()
    }

    fn apply(&mut self, doc: &mut Document) {
        if let Some(n) = doc.scene.get_mut(self.id) {
            n.animation = self.after.clone();
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        if let Some(n) = doc.scene.get_mut(self.id) {
            n.animation = self.before.clone();
        }
    }
}

/// One undo step for a tag-scope animation clip.
pub struct SetTagAnimationCmd {
    pub node: NodeId,
    pub tag: usize,
    pub before: AnimationClip,
    pub after: AnimationClip,
}

impl SetTagAnimationCmd {
    pub fn new(doc: &Document, node: NodeId, tag: usize, after: AnimationClip) -> Option<Self> {
        let before = doc
            .scene
            .get(node)?
            .tag_animations
            .get(&(tag as u32))
            .cloned()
            .unwrap_or_default();
        Some(Self {
            node,
            tag,
            before,
            after,
        })
    }
}

impl Command for SetTagAnimationCmd {
    fn label(&self) -> String {
        "Edit Tag Animation".into()
    }

    fn apply(&mut self, doc: &mut Document) {
        let Some(node) = doc.scene.get_mut(self.node) else {
            return;
        };
        if self.after.is_empty() {
            node.tag_animations.remove(&(self.tag as u32));
        } else {
            node.tag_animations
                .insert(self.tag as u32, self.after.clone());
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        let Some(node) = doc.scene.get_mut(self.node) else {
            return;
        };
        if self.before.is_empty() {
            node.tag_animations.remove(&(self.tag as u32));
        } else {
            node.tag_animations
                .insert(self.tag as u32, self.before.clone());
        }
    }
}

/// One undo step for keying transforms on a shared multi-object selection.
pub struct SetAnimationsCmd {
    pub edits: Vec<(NodeId, AnimationClip, AnimationClip)>,
}

impl Command for SetAnimationsCmd {
    fn label(&self) -> String {
        match self.edits.len() {
            0 | 1 => "Edit Animation".into(),
            count => format!("Edit Animation on {count} Objects"),
        }
    }

    fn apply(&mut self, doc: &mut Document) {
        for (id, _, after) in &self.edits {
            if let Some(n) = doc.scene.get_mut(*id) {
                n.animation = after.clone();
            }
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        for (id, before, _) in &self.edits {
            if let Some(n) = doc.scene.get_mut(*id) {
                n.animation = before.clone();
            }
        }
    }
}

pub struct SetMaterialCmd {
    pub before: Material,
    pub after: Material,
}

impl Command for SetMaterialCmd {
    fn label(&self) -> String {
        format!("Edit Material {}", self.after.name)
    }

    fn apply(&mut self, doc: &mut Document) {
        doc.materials.insert(self.after.clone());
    }

    fn undo(&mut self, doc: &mut Document) {
        doc.materials.insert(self.before.clone());
    }
}

pub struct AddMaterialCmd {
    pub material: Material,
}

impl Command for AddMaterialCmd {
    fn label(&self) -> String {
        "Add Material".into()
    }

    fn apply(&mut self, doc: &mut Document) {
        doc.materials.insert(self.material.clone());
    }

    fn undo(&mut self, doc: &mut Document) {
        doc.materials.materials.remove(&self.material.id);
    }
}

pub struct RemoveMaterialCmd {
    pub material: Material,
}

impl Command for RemoveMaterialCmd {
    fn label(&self) -> String {
        "Remove Material".into()
    }

    fn apply(&mut self, doc: &mut Document) {
        doc.materials.materials.remove(&self.material.id);
    }

    fn undo(&mut self, doc: &mut Document) {
        doc.materials.insert(self.material.clone());
    }
}

pub struct ReparentCmd {
    pub id: NodeId,
    pub old_parent: Option<NodeId>,
    pub old_index: usize,
    pub new_parent: Option<NodeId>,
    pub new_index: Option<usize>,
}

impl ReparentCmd {
    pub fn new(doc: &Document, id: NodeId, new_parent: Option<NodeId>) -> Option<Self> {
        let node = doc.scene.get(id)?;
        let old_parent = node.parent;
        let old_index = if let Some(p) = old_parent {
            doc.scene
                .get(p)
                .and_then(|p| p.children.iter().position(|&c| c == id))
                .unwrap_or(0)
        } else {
            doc.scene.roots.iter().position(|&r| r == id).unwrap_or(0)
        };
        Some(Self {
            id,
            old_parent,
            old_index,
            new_parent,
            new_index: None,
        })
    }
}

impl Command for ReparentCmd {
    fn label(&self) -> String {
        "Reparent".into()
    }

    fn apply(&mut self, doc: &mut Document) {
        let _ = doc.scene.reparent(self.id, self.new_parent, self.new_index);
    }

    fn undo(&mut self, doc: &mut Document) {
        let _ = doc
            .scene
            .reparent(self.id, self.old_parent, Some(self.old_index));
    }
}

pub struct SetSettingsCmd {
    pub before: DocumentSettings,
    pub after: DocumentSettings,
}

impl Command for SetSettingsCmd {
    fn label(&self) -> String {
        "Edit Render Settings".into()
    }

    fn apply(&mut self, doc: &mut Document) {
        doc.settings = self.after.clone();
    }

    fn undo(&mut self, doc: &mut Document) {
        doc.settings = self.before.clone();
    }
}

/// Whole-document replacement for destructive operations such as opening or
/// importing a file. Undo restores the exact previous document.
pub struct ReplaceDocumentCmd {
    pub label: String,
    pub before: Document,
    pub after: Document,
}

impl Command for ReplaceDocumentCmd {
    fn label(&self) -> String {
        self.label.clone()
    }

    fn apply(&mut self, doc: &mut Document) {
        *doc = self.after.clone();
    }

    fn undo(&mut self, doc: &mut Document) {
        *doc = self.before.clone();
    }
}

pub fn assign_material_boxes(doc: &mut Document, ids: &[NodeId], material: Option<MaterialId>) {
    for id in ids {
        if let Some(n) = doc.scene.get_mut(*id) {
            n.material = material;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GeometrySource, NodeKind, Primitive};
    use lc_core::Transform;

    fn demo() -> Document {
        Document::demo()
    }

    #[test]
    fn add_and_undo_restore_signature() {
        let mut doc = demo();
        let before = doc.signature();
        let mut history = History::new(100);
        let node = doc.create_node(
            NodeKind::Mesh {
                geometry: GeometrySource::from_primitive(Primitive::Box {
                    width: 1.0,
                    height: 1.0,
                    depth: 1.0,
                }),
            },
            "New Cube",
        );
        let node_id = node.id;
        let cmd = AddNodeCmd::new(&doc, node, None);
        history.execute(&mut doc, Box::new(cmd));
        assert!(doc.scene.get(node_id).is_some());
        assert!(doc.signature() != before);
        assert!(history.undo(&mut doc).is_some());
        assert!(doc.scene.get(node_id).is_none());
        assert_eq!(doc.signature(), before);
        assert!(history.redo(&mut doc).is_some());
        assert!(doc.scene.get(node_id).is_some());
    }

    #[test]
    fn delete_subtree_restores_hierarchy() {
        let mut doc = demo();
        let before = doc.signature();
        let cube = doc.scene.find_by_name("Cube").unwrap();
        let mut history = History::new(100);
        let cmd = DeleteNodeCmd::new(&doc, cube);
        history.execute(&mut doc, Box::new(cmd));
        assert!(doc.scene.get(cube).is_none());
        history.undo(&mut doc);
        assert_eq!(doc.signature(), before);
    }

    #[test]
    fn transform_and_props_undo() {
        let mut doc = demo();
        let id = doc.scene.find_by_name("Torus").unwrap();
        let before = doc.signature();
        let mut history = History::new(100);
        let cmd = SetTransformCmd::new(
            &doc,
            id,
            Transform::from_translation(glam::Vec3::new(1.0, 2.0, 3.0)),
        )
        .unwrap();
        history.execute(&mut doc, Box::new(cmd));
        assert!((doc.scene.get(id).unwrap().transform.translation.y - 2.0).abs() < 1e-5);
        history.undo(&mut doc);
        assert_eq!(doc.signature(), before);

        let mut props = NodeProps::of(doc.scene.get(id).unwrap());
        props.visible = false;
        props.material = Some(1);
        let cmd = SetNodePropsCmd::new(&doc, id, props).unwrap();
        history.execute(&mut doc, Box::new(cmd));
        assert!(!doc.scene.get(id).unwrap().visible);
        history.undo(&mut doc);
        assert!(doc.scene.get(id).unwrap().visible);
    }

    #[test]
    fn material_commands_undo() {
        let mut doc = demo();
        let before = doc.signature();
        let mat = doc.materials.get(0).unwrap().clone();
        let mut changed = mat.clone();
        changed.name = "Edited Grey".into();
        let mut history = History::new(10);
        history.execute(
            &mut doc,
            Box::new(SetMaterialCmd {
                before: mat,
                after: changed,
            }),
        );
        assert_eq!(doc.materials.get(0).unwrap().name, "Edited Grey");
        history.undo(&mut doc);
        assert_eq!(doc.signature(), before);
    }

    #[test]
    fn removing_a_tag_keeps_per_tag_clips_aligned_and_undoable() {
        use crate::Tag;
        use lc_anim::{AnimProperty, Interpolation};

        let mut doc = demo();
        let id = doc.scene.find_by_name("Cube").unwrap();
        let clip = {
            let node = doc.scene.get_mut(id).unwrap();
            node.tags = vec![
                Tag::RenderOverride {
                    visible: true,
                    casts_shadow: true,
                },
                Tag::CopyLocation {
                    source: id,
                    offset: glam::Vec3::ZERO,
                    weight: 1.0,
                },
            ];
            node.tag_animations.clear();
            let mut clip = AnimationClip::default();
            clip.set_key(
                AnimProperty::CopyLocationWeight,
                0.0,
                0.25,
                Interpolation::Linear,
            );
            node.tag_animations.insert(1, clip.clone());
            clip
        };

        // The inspector edits a clone and commits through NodeProps, so the
        // remapped clip map has to travel with the tag list.
        let mut edited = doc.scene.get(id).unwrap().clone();
        assert!(edited.remove_tag(0).is_some());
        let mut history = History::new(4);
        let cmd = SetNodePropsCmd::new(&doc, id, NodeProps::of(&edited)).unwrap();
        history.execute(&mut doc, Box::new(cmd));

        let node = doc.scene.get(id).unwrap();
        assert_eq!(node.tags.len(), 1);
        assert_eq!(node.tag_animations.get(&0), Some(&clip));
        assert!(!node.tag_animations.contains_key(&1));

        history.undo(&mut doc);
        let node = doc.scene.get(id).unwrap();
        assert_eq!(node.tags.len(), 2);
        assert_eq!(node.tag_animations.get(&1), Some(&clip));
    }

    #[test]
    fn history_limit_drops_oldest() {
        let mut doc = doc_empty();
        let mut history = History::new(2);
        for i in 0..5 {
            let node = doc.create_node(
                NodeKind::Mesh {
                    geometry: GeometrySource::from_primitive(Primitive::default()),
                },
                &format!("Node {i}"),
            );
            let cmd = AddNodeCmd::new(&doc, node, None);
            history.execute(&mut doc, Box::new(cmd));
        }
        assert_eq!(history.depth(), 2);
    }

    fn doc_empty() -> Document {
        Document::empty()
    }
}
