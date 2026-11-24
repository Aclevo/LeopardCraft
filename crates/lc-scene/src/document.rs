//! The serializable document: scene graph, materials and render settings.

use crate::{Camera, Node, NodeId, NodeKind, Scene};
use glam::Vec3;
use lc_anim::AnimationClip;
use lc_core::StableHasher;
use lc_shading::{MaterialId, MaterialLibrary};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const FORMAT_NAME: &str = "leopardcraft-scene";
pub const FORMAT_VERSION: u32 = 1;

fn default_format() -> String {
    FORMAT_NAME.to_string()
}

fn default_version() -> u32 {
    FORMAT_VERSION
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    pub color: [u8; 3],
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            name: "Default".into(),
            visible: true,
            color: [110, 140, 190],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Environment {
    Sky {
        top: Vec3,
        horizon: Vec3,
        ground: Vec3,
        sun_direction: Vec3,
        sun_color: Vec3,
        sun_intensity: f32,
    },
    Hdri {
        path: String,
        intensity: f32,
        rotation_deg: f32,
    },
    Solid {
        color: Vec3,
    },
}

impl Default for Environment {
    fn default() -> Self {
        Self::Sky {
            top: Vec3::new(0.10, 0.16, 0.28),
            horizon: Vec3::new(0.55, 0.62, 0.72),
            ground: Vec3::new(0.10, 0.09, 0.08),
            sun_direction: Vec3::new(-0.45, -0.8, -0.35).normalize(),
            sun_color: Vec3::new(1.0, 0.93, 0.82),
            sun_intensity: 1.4,
        }
    }
}

impl Environment {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Sky { .. } => "Procedural Sky",
            Self::Hdri { .. } => "HDRI",
            Self::Solid { .. } => "Solid Color",
        }
    }

    pub fn sun(&self) -> Option<(Vec3, Vec3, f32)> {
        match self {
            Self::Sky {
                sun_direction,
                sun_color,
                sun_intensity,
                ..
            } if *sun_intensity > 0.0 => Some((
                sun_direction.normalize_or(Vec3::new(-0.45, -0.8, -0.35)),
                *sun_color,
                *sun_intensity,
            )),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RenderSettings {
    pub width: u32,
    pub height: u32,
    pub samples: u32,
    pub max_bounces: u32,
    pub exposure: f32,
    pub transparent_background: bool,
    pub seed: u64,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            width: 960,
            height: 540,
            samples: 64,
            max_bounces: 6,
            exposure: 0.0,
            transparent_background: false,
            seed: 2024,
        }
    }
}

impl RenderSettings {
    pub fn aspect(&self) -> f32 {
        self.width.max(1) as f32 / self.height.max(1) as f32
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentSettings {
    #[serde(default = "default_layers")]
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub active_camera: Option<NodeId>,
    #[serde(default)]
    pub environment: Environment,
    #[serde(default)]
    pub render: RenderSettings,
    #[serde(default = "default_fps")]
    pub fps: f32,
    #[serde(default)]
    pub frame_start: f32,
    #[serde(default = "default_frame_end")]
    pub frame_end: f32,
    #[serde(default = "default_background")]
    pub background: Vec3,
    #[serde(default = "default_units")]
    pub units: String,
}

fn default_layers() -> Vec<Layer> {
    vec![Layer::default()]
}

fn default_fps() -> f32 {
    24.0
}

fn default_frame_end() -> f32 {
    96.0
}

fn default_background() -> Vec3 {
    Vec3::new(0.035, 0.04, 0.05)
}

fn default_units() -> String {
    "metres".into()
}

impl Default for DocumentSettings {
    fn default() -> Self {
        Self {
            layers: default_layers(),
            active_camera: None,
            environment: Environment::default(),
            render: RenderSettings::default(),
            fps: default_fps(),
            frame_start: 0.0,
            frame_end: default_frame_end(),
            background: default_background(),
            units: default_units(),
        }
    }
}

impl DocumentSettings {
    pub fn layer_visible(&self, index: u8) -> bool {
        self.layers
            .get(index as usize)
            .map(|l| l.visible)
            .unwrap_or(true)
    }

    pub fn ensure_valid_layers(&mut self) {
        if self.layers.is_empty() {
            self.layers.push(Layer::default());
        }
        self.render.width = self.render.width.clamp(16, 8192);
        self.render.height = self.render.height.clamp(16, 8192);
        self.render.samples = self.render.samples.clamp(1, 4096);
        self.render.max_bounces = self.render.max_bounces.clamp(1, 64);
        self.fps = if self.fps.is_finite() {
            self.fps.clamp(1.0, 240.0)
        } else {
            24.0
        };
        if !self.frame_start.is_finite() {
            self.frame_start = 0.0;
        }
        if !self.frame_end.is_finite() || self.frame_end < self.frame_start {
            self.frame_end = self.frame_start + 1.0;
        }
        if !self.background.is_finite() {
            self.background = default_background();
        }
    }
}

/// One animation take. Takes live on the document, not on individual objects:
/// the active take's clips are mirrored into `Node::animation`,
/// `Node::tag_animations` and `Material::animation` so evaluation and the
/// existing animation editors keep reading one live view. Inactive takes keep
/// their clips here and are restored when activated.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Take {
    pub name: String,
    #[serde(default = "default_fps")]
    pub fps: f32,
    #[serde(default, skip_serializing_if = "TakeClips::is_empty")]
    pub clips: TakeClips,
}

impl Take {
    pub fn new(name: impl Into<String>, fps: f32) -> Self {
        Self {
            name: name.into(),
            fps: sanitize_take_fps(fps).unwrap_or(default_fps()),
            clips: TakeClips::default(),
        }
    }
}

/// A per-tag clip stored inside a take. Tag clips are keyed by tag index on the
/// owning node, so they are kept in a list to serialize without string keys.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TakeTagClip {
    pub node: NodeId,
    pub tag: u32,
    #[serde(default)]
    pub clip: AnimationClip,
}

/// The clips owned by one take. Empty clips are omitted from the file; applying
/// a take clears any live clip the take does not contain.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TakeClips {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub objects: BTreeMap<NodeId, AnimationClip>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub materials: BTreeMap<MaterialId, AnimationClip>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TakeTagClip>,
}

impl TakeClips {
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty() && self.materials.is_empty() && self.tags.is_empty()
    }
}

fn sanitize_take_fps(fps: f32) -> Option<f32> {
    fps.is_finite().then(|| fps.clamp(1.0, 240.0))
}

/// Exact take-related document state captured for one undo step. Live clips are
/// included because switching takes replaces them.
#[derive(Clone, Debug, PartialEq)]
pub struct TakeState {
    pub takes: Vec<Take>,
    pub active_take: usize,
    pub settings_fps: f32,
    pub live: TakeClips,
}

impl TakeState {
    pub fn capture(doc: &Document) -> Self {
        let live = doc.capture_live_clips();
        let mut takes = doc.takes.clone();
        if let Some(active) = takes.get_mut(doc.active_take) {
            active.clips = live.clone();
        }
        Self {
            takes,
            active_take: doc.active_take,
            settings_fps: doc.settings.fps,
            live,
        }
    }

    pub fn restore(&self, doc: &mut Document) {
        doc.takes = self.takes.clone();
        doc.active_take = self.active_take;
        doc.settings.fps = self.settings_fps;
        doc.apply_clips(&self.live);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub scene: Scene,
    #[serde(default)]
    pub materials: MaterialLibrary,
    #[serde(default)]
    pub settings: DocumentSettings,
    /// Document-level animation takes. Files written before takes existed omit
    /// this list and migrate into one default take holding the live clips.
    #[serde(default)]
    pub takes: Vec<Take>,
    #[serde(default)]
    pub active_take: usize,
}

impl Default for Document {
    fn default() -> Self {
        Self::empty()
    }
}

impl Document {
    /// An empty but usable document (one layer, default materials).
    pub fn empty() -> Self {
        let mut doc = Self {
            format: FORMAT_NAME.to_string(),
            version: FORMAT_VERSION,
            scene: Scene::new(),
            materials: MaterialLibrary::default(),
            settings: DocumentSettings::default(),
            takes: Vec::new(),
            active_take: 0,
        };
        doc.settings.ensure_valid_layers();
        doc.takes.push(Take::new("Take 1", doc.settings.fps));
        doc
    }

    pub fn create_node(&mut self, kind: NodeKind, name_base: &str) -> Node {
        let id = self.scene.alloc_id();
        let name = self.scene.unique_name(name_base);
        Node::new(id, name, kind)
    }

    pub fn unique_name(&self, base: &str) -> String {
        self.scene.unique_name(base)
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.scene.get(id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.scene.get_mut(id)
    }

    pub fn active_camera_id(&self) -> Option<NodeId> {
        if let Some(id) = self.settings.active_camera {
            if matches!(
                self.scene.get(id).map(|n| &n.kind),
                Some(NodeKind::Camera { .. })
            ) {
                return Some(id);
            }
        }
        self.scene.depth_first().into_iter().find(|id| {
            matches!(
                self.scene.get(*id).map(|n| &n.kind),
                Some(NodeKind::Camera { .. })
            )
        })
    }

    pub fn camera(&self, id: NodeId) -> Option<&Camera> {
        match &self.scene.get(id)?.kind {
            NodeKind::Camera { camera } => Some(camera),
            _ => None,
        }
    }

    pub fn layer_count(&self) -> usize {
        self.settings.layers.len()
    }

    /// The active take's clips, or an empty set when the document has no takes.
    pub fn active_take(&self) -> Option<&Take> {
        self.takes.get(self.active_take)
    }

    /// FPS of the active take. This drives playback, timelines and animation
    /// export; `DocumentSettings::fps` stays as a legacy mirror for readers that
    /// do not know about takes.
    pub fn active_take_fps(&self) -> f32 {
        self.active_take()
            .and_then(|take| sanitize_take_fps(take.fps))
            .or_else(|| sanitize_take_fps(self.settings.fps))
            .unwrap_or(default_fps())
    }

    pub fn unique_take_name(&self, base: &str) -> String {
        let base = base.trim();
        let base = if base.is_empty() { "Take" } else { base };
        if !self.takes.iter().any(|take| take.name == base) {
            return base.to_string();
        }
        let mut index = 2;
        loop {
            let name = format!("{base} {index}");
            if !self.takes.iter().any(|take| take.name == name) {
                return name;
            }
            index += 1;
        }
    }

    /// Capture the live animation state into one snapshot for a take.
    pub fn capture_live_clips(&self) -> TakeClips {
        let mut clips = TakeClips::default();
        for (&id, node) in &self.scene.nodes {
            if !node.animation.is_empty() {
                clips.objects.insert(id, node.animation.clone());
            }
            for (&tag, clip) in &node.tag_animations {
                if !clip.is_empty() {
                    clips.tags.push(TakeTagClip {
                        node: id,
                        tag,
                        clip: clip.clone(),
                    });
                }
            }
        }
        for (&id, material) in &self.materials.materials {
            if !material.animation.is_empty() {
                clips.materials.insert(id, material.animation.clone());
            }
        }
        clips
    }

    /// Replace every live clip with the snapshot. Clips absent from the
    /// snapshot are cleared, so switching takes cannot leak the previous one.
    pub fn apply_clips(&mut self, clips: &TakeClips) {
        for node in self.scene.nodes.values_mut() {
            node.animation = clips.objects.get(&node.id).cloned().unwrap_or_default();
            node.tag_animations.clear();
            for tag_clip in &clips.tags {
                if tag_clip.node == node.id {
                    node.tag_animations
                        .insert(tag_clip.tag, tag_clip.clip.clone());
                }
            }
        }
        for material in self.materials.materials.values_mut() {
            material.animation = clips
                .materials
                .get(&material.id)
                .cloned()
                .unwrap_or_default();
        }
    }

    /// Commit the live clips into the active take. Save, signature, switching
    /// and take-management operations call this; the live clips remain the
    /// working view while editing.
    pub fn store_active_clips(&mut self) {
        let clips = self.capture_live_clips();
        if let Some(take) = self.takes.get_mut(self.active_take) {
            take.clips = clips;
        }
    }

    pub fn take_state(&self) -> TakeState {
        TakeState::capture(self)
    }

    pub fn restore_take_state(&mut self, state: &TakeState) {
        state.restore(self);
    }

    /// Create a new empty take, make it active and clear the live clips.
    pub fn create_take(&mut self, base: &str) -> usize {
        self.store_active_clips();
        let name = self.unique_take_name(base);
        let fps = self.active_take_fps();
        self.takes.push(Take::new(name, fps));
        self.active_take = self.takes.len() - 1;
        self.apply_clips(&TakeClips::default());
        self.settings.fps = fps;
        self.active_take
    }

    /// Duplicate the active take, including its clips, and activate the copy.
    pub fn duplicate_active_take(&mut self) -> usize {
        self.store_active_clips();
        let source = self
            .takes
            .get(self.active_take)
            .cloned()
            .unwrap_or_else(|| Take::new("Take", self.settings.fps));
        let name = self.unique_take_name(&format!("{} Copy", source.name));
        let mut copy = source;
        copy.name = name;
        let clips = copy.clips.clone();
        self.takes.push(copy);
        self.active_take = self.takes.len() - 1;
        self.apply_clips(&clips);
        self.settings.fps = self.active_take_fps();
        self.active_take
    }

    /// Delete a take. Deleting the active take activates a neighbour; deleting
    /// the last remaining take replaces it with a fresh default take.
    pub fn delete_take(&mut self, index: usize) -> bool {
        if index >= self.takes.len() {
            return false;
        }
        self.store_active_clips();
        let was_active = index == self.active_take;
        let active_fps = self.active_take_fps();
        self.takes.remove(index);
        if self.takes.is_empty() {
            self.takes.push(Take::new("Take 1", active_fps));
            self.active_take = 0;
            self.apply_clips(&TakeClips::default());
        } else if was_active {
            self.active_take = index.min(self.takes.len() - 1);
            let clips = self.takes[self.active_take].clips.clone();
            self.apply_clips(&clips);
        } else if index < self.active_take {
            self.active_take -= 1;
        }
        self.settings.fps = self.active_take_fps();
        true
    }

    /// Activate a take by index, storing the current live clips first.
    pub fn activate_take(&mut self, index: usize) -> bool {
        if index >= self.takes.len() || index == self.active_take {
            return false;
        }
        self.store_active_clips();
        self.active_take = index;
        let clips = self.takes[index].clips.clone();
        self.apply_clips(&clips);
        self.settings.fps = self.active_take_fps();
        true
    }

    /// Rename a take. Empty names and duplicates are refused so the Takes tab
    /// can report a clear failure without creating an undo entry.
    pub fn rename_take(&mut self, index: usize, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || index >= self.takes.len() {
            return false;
        }
        if self.takes[index].name == name {
            return false;
        }
        if self
            .takes
            .iter()
            .enumerate()
            .any(|(other, take)| other != index && take.name == name)
        {
            return false;
        }
        self.takes[index].name = name.to_string();
        true
    }

    /// Set a take's fps. Non-finite values are refused; finite values clamp to
    /// the supported 1–240 range. Editing the active take also updates the
    /// legacy settings mirror used by playback.
    pub fn set_take_fps(&mut self, index: usize, fps: f32) -> bool {
        let Some(fps) = sanitize_take_fps(fps) else {
            return false;
        };
        if index >= self.takes.len() || (self.takes[index].fps - fps).abs() <= 1e-6 {
            return false;
        }
        self.takes[index].fps = fps;
        if index == self.active_take {
            self.settings.fps = fps;
        }
        true
    }

    /// Ensure every document has at least one take, sanitize stored clips and
    /// active indices, and mirror the active fps into legacy settings.
    pub fn ensure_takes(&mut self) {
        if self.takes.is_empty() {
            let mut take = Take::new("Take 1", self.settings.fps);
            take.clips = self.capture_live_clips();
            self.takes.push(take);
            self.active_take = 0;
        }
        let node_tags: BTreeMap<NodeId, usize> = self
            .scene
            .nodes
            .iter()
            .map(|(&id, node)| (id, node.tags.len()))
            .collect();
        let material_ids: BTreeSet<MaterialId> = self.materials.materials.keys().copied().collect();
        for (index, take) in self.takes.iter_mut().enumerate() {
            if take.name.trim().is_empty() {
                take.name = format!("Take {}", index + 1);
            }
            take.fps = sanitize_take_fps(take.fps).unwrap_or(default_fps());
            take.clips.objects.retain(|id, clip| {
                if !node_tags.contains_key(id) {
                    return false;
                }
                crate::format::sanitize_animation(clip);
                !clip.is_empty()
            });
            take.clips.materials.retain(|id, clip| {
                if !material_ids.contains(id) {
                    return false;
                }
                crate::format::sanitize_animation(clip);
                !clip.is_empty()
            });
            let mut seen = BTreeSet::new();
            take.clips.tags.retain_mut(|tag_clip| {
                let Some(tag_count) = node_tags.get(&tag_clip.node) else {
                    return false;
                };
                if tag_clip.tag as usize >= *tag_count
                    || !seen.insert((tag_clip.node, tag_clip.tag))
                {
                    return false;
                }
                crate::format::sanitize_animation(&mut tag_clip.clip);
                !tag_clip.clip.is_empty()
            });
        }
        if self.active_take >= self.takes.len() {
            self.active_take = 0;
        }
        self.settings.fps = self.active_take_fps();
    }

    /// Restore the active take into the live clip view. Called after loading so
    /// files with several takes open on their stored active take.
    pub fn load_active_take(&mut self) {
        self.ensure_takes();
        let clips = self.takes[self.active_take].clips.clone();
        self.apply_clips(&clips);
        self.settings.fps = self.active_take_fps();
    }

    /// Stable semantic signature. The id allocator is deliberately ignored:
    /// undo restores scene content, not the monotonic allocation counter.
    pub fn signature(&self) -> u64 {
        let mut h = StableHasher::new();
        let mut normalized = self.clone();
        normalized.store_active_clips();
        normalized.settings.fps = normalized.active_take_fps();
        let mut value = match serde_json::to_value(&normalized) {
            Ok(v) => v,
            Err(_) => {
                h.write_u64(self.scene.signature());
                h.write_u64(self.materials.signature());
                return h.finish();
            }
        };
        if let Some(scene) = value.get_mut("scene").and_then(|s| s.as_object_mut()) {
            scene.insert("next_id".into(), serde_json::Value::from(0));
        }
        if let Ok(bytes) = serde_json::to_vec(&value) {
            for b in bytes {
                h.write_u8(b);
            }
        }
        h.finish()
    }

    pub fn mesh_node_count(&self) -> usize {
        self.scene
            .nodes
            .values()
            .filter(|n| matches!(n.kind, NodeKind::Mesh { .. } | NodeKind::Spline { .. }))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::undo::History;
    use crate::{format::from_json, NodeKind, SetTakesCmd, Tag};
    use lc_anim::{AnimChannel, ChannelAddress, Interpolation};

    #[test]
    fn empty_document_is_valid_shape() {
        let doc = Document::empty();
        assert_eq!(doc.format, FORMAT_NAME);
        assert!(!doc.settings.layers.is_empty());
        assert!(!doc.materials.is_empty());
        assert!(doc.scene.is_empty());
    }

    #[test]
    fn settings_are_sanitized() {
        let mut s = DocumentSettings::default();
        s.render.width = 0;
        s.render.height = 100_000;
        s.fps = f32::NAN;
        s.layers.clear();
        s.ensure_valid_layers();
        assert!(s.render.width >= 16);
        assert!(s.render.height <= 8192);
        assert_eq!(s.fps, 24.0);
        assert!(!s.layers.is_empty());
    }

    fn animated_document() -> (Document, NodeId) {
        let mut doc = Document::empty();
        let id = doc.append_node(NodeKind::Null, "Animated", None);
        doc.scene.get_mut(id).unwrap().animation.set_key(
            ChannelAddress::owner_component(AnimChannel::Translation, 0),
            0.0,
            1.0,
            Interpolation::Linear,
        );
        doc.store_active_clips();
        (doc, id)
    }

    fn run_take_edit(
        doc: &mut Document,
        history: &mut History,
        label: &str,
        edit: impl FnOnce(&mut Document) -> bool,
    ) -> bool {
        let before = doc.take_state();
        if !edit(doc) {
            return false;
        }
        let after = doc.take_state();
        history.execute(
            doc,
            Box::new(SetTakesCmd {
                label: label.into(),
                before,
                after,
            }),
        );
        true
    }

    #[test]
    fn legacy_documents_migrate_into_one_default_take() {
        let (doc, id) = animated_document();
        let mut legacy = serde_json::to_value(&doc).unwrap();
        legacy.as_object_mut().unwrap().remove("takes");
        legacy.as_object_mut().unwrap().remove("active_take");

        let loaded = from_json(&legacy.to_string()).expect("legacy file loads");
        assert_eq!(loaded.takes.len(), 1);
        assert_eq!(loaded.active_take, 0);
        assert_eq!(loaded.takes[0].name, "Take 1");
        assert_eq!(loaded.takes[0].fps, 24.0);
        assert_eq!(loaded.active_take_fps(), 24.0);
        let clip = loaded.takes[0]
            .clips
            .objects
            .get(&id)
            .expect("the live clip migrated into the default take");
        assert!(
            (clip
                .sample(
                    ChannelAddress::owner_component(AnimChannel::Translation, 0),
                    0.0
                )
                .unwrap()
                - 1.0)
                .abs()
                < 1e-6
        );
        assert!(
            (loaded
                .scene
                .get(id)
                .unwrap()
                .animation
                .sample(
                    ChannelAddress::owner_component(AnimChannel::Translation, 0),
                    0.0
                )
                .unwrap()
                - 1.0)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn switching_takes_restores_node_tag_and_material_clips() {
        let (mut doc, id) = animated_document();
        let material = doc.materials.get(0).unwrap().id;
        {
            let node = doc.scene.get_mut(id).unwrap();
            node.tags = vec![Tag::RenderOverride {
                visible: true,
                casts_shadow: true,
            }];
            let mut tag_clip = AnimationClip::default();
            tag_clip.set_key(
                ChannelAddress::owner_component(AnimChannel::CopyLocationWeight, 0),
                0.0,
                0.25,
                Interpolation::Linear,
            );
            node.tag_animations.insert(0, tag_clip);
        }
        doc.materials
            .materials
            .get_mut(&material)
            .unwrap()
            .animation
            .set_key(
                ChannelAddress::owner_component(AnimChannel::MaterialRoughness, 0),
                0.0,
                0.25,
                Interpolation::Linear,
            );
        doc.store_active_clips();

        assert_eq!(doc.create_take("Action"), 1);
        assert!(doc.scene.get(id).unwrap().animation.is_empty());
        assert!(doc.scene.get(id).unwrap().tag_animations.is_empty());
        assert!(doc.materials.get(material).unwrap().animation.is_empty());

        {
            let node = doc.scene.get_mut(id).unwrap();
            node.animation.set_key(
                ChannelAddress::owner_component(AnimChannel::Translation, 0),
                0.0,
                3.0,
                Interpolation::Linear,
            );
            node.tag_animations.insert(0, {
                let mut clip = AnimationClip::default();
                clip.set_key(
                    ChannelAddress::owner_component(AnimChannel::CopyLocationWeight, 0),
                    0.0,
                    0.75,
                    Interpolation::Linear,
                );
                clip
            });
        }
        doc.materials
            .materials
            .get_mut(&material)
            .unwrap()
            .animation
            .set_key(
                ChannelAddress::owner_component(AnimChannel::MaterialRoughness, 0),
                0.0,
                0.75,
                Interpolation::Linear,
            );
        doc.store_active_clips();

        assert!(doc.activate_take(0));
        let node = doc.scene.get(id).unwrap();
        assert!(
            (node
                .animation
                .sample(
                    ChannelAddress::owner_component(AnimChannel::Translation, 0),
                    0.0
                )
                .unwrap()
                - 1.0)
                .abs()
                < 1e-6
        );
        assert!(
            (node.tag_animations[&0]
                .sample(
                    ChannelAddress::owner_component(AnimChannel::CopyLocationWeight, 0),
                    0.0
                )
                .unwrap()
                - 0.25)
                .abs()
                < 1e-6
        );
        assert!(
            (doc.materials
                .get(material)
                .unwrap()
                .animation
                .sample(
                    ChannelAddress::owner_component(AnimChannel::MaterialRoughness, 0),
                    0.0
                )
                .unwrap()
                - 0.25)
                .abs()
                < 1e-6
        );

        assert!(doc.activate_take(1));
        let node = doc.scene.get(id).unwrap();
        assert!(
            (node
                .animation
                .sample(
                    ChannelAddress::owner_component(AnimChannel::Translation, 0),
                    0.0
                )
                .unwrap()
                - 3.0)
                .abs()
                < 1e-6
        );
        assert!(
            (doc.materials
                .get(material)
                .unwrap()
                .animation
                .sample(
                    ChannelAddress::owner_component(AnimChannel::MaterialRoughness, 0),
                    0.0
                )
                .unwrap()
                - 0.75)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn per_take_fps_clamps_finite_values_and_refuses_non_finite() {
        let (mut doc, _) = animated_document();
        assert!(doc.set_take_fps(0, 0.0));
        assert_eq!(doc.takes[0].fps, 1.0);
        assert_eq!(doc.settings.fps, 1.0);
        assert!(doc.set_take_fps(0, 1000.0));
        assert_eq!(doc.takes[0].fps, 240.0);
        assert_eq!(doc.active_take_fps(), 240.0);
        assert!(!doc.set_take_fps(0, f32::NAN));
        assert!(!doc.set_take_fps(0, f32::INFINITY));
        assert!(!doc.set_take_fps(9, 24.0));
        assert_eq!(doc.takes[0].fps, 240.0);
    }

    #[test]
    fn take_operations_are_one_undo_step_each() {
        let (mut doc, id) = animated_document();
        let mut history = History::new(20);
        let original = doc.signature();

        assert!(run_take_edit(&mut doc, &mut history, "New Take", |doc| {
            doc.create_take("Action");
            true
        }));
        assert_eq!(history.depth(), 1);
        assert_eq!(doc.takes.len(), 2);
        assert_eq!(doc.active_take, 1);
        assert!(history.undo(&mut doc).is_some());
        assert_eq!(doc.takes.len(), 1);
        assert_eq!(doc.active_take, 0);
        assert_eq!(doc.signature(), original);
        assert!(history.redo(&mut doc).is_some());
        assert_eq!(doc.active_take, 1);

        assert!(run_take_edit(
            &mut doc,
            &mut history,
            "Rename Take",
            |doc| { doc.rename_take(1, "Run") }
        ));
        assert_eq!(history.depth(), 2);
        assert_eq!(doc.takes[1].name, "Run");
        assert!(!run_take_edit(
            &mut doc,
            &mut history,
            "Rename Take",
            |doc| { doc.rename_take(1, "  ") }
        ));
        assert_eq!(history.depth(), 2);
        assert!(!run_take_edit(
            &mut doc,
            &mut history,
            "Rename Take",
            |doc| { doc.rename_take(1, "Take 1") }
        ));
        assert_eq!(history.depth(), 2);

        assert!(run_take_edit(&mut doc, &mut history, "Take FPS", |doc| {
            doc.set_take_fps(1, 12.0)
        }));
        assert_eq!(history.depth(), 3);
        assert!(!run_take_edit(&mut doc, &mut history, "Take FPS", |doc| {
            doc.set_take_fps(1, f32::NAN)
        }));
        assert_eq!(history.depth(), 3);

        assert!(run_take_edit(
            &mut doc,
            &mut history,
            "Activate Take",
            |doc| { doc.activate_take(0) }
        ));
        assert_eq!(history.depth(), 4);
        assert_eq!(doc.active_take, 0);
        assert!(history.undo(&mut doc).is_some());
        assert_eq!(doc.active_take, 1);
        assert_eq!(doc.takes[1].fps, 12.0);

        assert!(run_take_edit(
            &mut doc,
            &mut history,
            "Duplicate Take",
            |doc| {
                doc.duplicate_active_take();
                true
            }
        ));
        assert_eq!(doc.takes.len(), 3);
        assert_eq!(doc.takes[2].name, "Run Copy");
        assert!(history.undo(&mut doc).is_some());
        assert_eq!(doc.takes.len(), 2);

        assert!(run_take_edit(
            &mut doc,
            &mut history,
            "Delete Take",
            |doc| { doc.delete_take(1) }
        ));
        assert_eq!(doc.takes.len(), 1);
        assert_eq!(doc.active_take, 0);
        assert!(doc.scene.get(id).is_some());
        assert!(history.undo(&mut doc).is_some());
        assert_eq!(doc.takes.len(), 2);
        assert_eq!(doc.takes[1].name, "Run");
    }

    #[test]
    fn active_take_round_trips_and_signature_is_stable() {
        let (mut doc, id) = animated_document();
        doc.create_take("Action");
        doc.set_take_fps(1, 12.0);
        doc.scene.get_mut(id).unwrap().animation.set_key(
            ChannelAddress::owner_component(AnimChannel::Translation, 0),
            0.0,
            2.5,
            Interpolation::Linear,
        );
        doc.store_active_clips();
        let signature = doc.signature();

        let json = crate::format::to_json(&doc).unwrap();
        assert!(json.contains("\"takes\""));
        let back = from_json(&json).expect("take document round trips");
        assert_eq!(back.takes.len(), 2);
        assert_eq!(back.active_take, 1);
        assert_eq!(back.takes[1].name, "Action");
        assert_eq!(back.active_take_fps(), 12.0);
        assert!(
            (back
                .scene
                .get(id)
                .unwrap()
                .animation
                .sample(
                    ChannelAddress::owner_component(AnimChannel::Translation, 0),
                    0.0
                )
                .unwrap()
                - 2.5)
                .abs()
                < 1e-6
        );
        assert_eq!(back.signature(), signature);
    }

    #[test]
    fn deleting_the_only_take_replaces_it_with_a_default() {
        let (mut doc, id) = animated_document();
        let fps = doc.active_take_fps();
        assert!(doc.delete_take(0));
        assert_eq!(doc.takes.len(), 1);
        assert_eq!(doc.active_take, 0);
        assert_eq!(doc.takes[0].name, "Take 1");
        assert_eq!(doc.takes[0].fps, fps);
        assert_eq!(doc.active_take_fps(), fps);
        assert!(doc.scene.get(id).unwrap().animation.is_empty());
    }
}
