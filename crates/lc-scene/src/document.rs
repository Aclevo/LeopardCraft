//! The serializable document: scene graph, materials and render settings.

use crate::{Camera, Node, NodeId, NodeKind, Scene};
use glam::Vec3;
use lc_core::StableHasher;
use lc_shading::MaterialLibrary;
use serde::{Deserialize, Serialize};

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
        };
        doc.settings.ensure_valid_layers();
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

    /// Stable semantic signature. The id allocator is deliberately ignored:
    /// undo restores scene content, not the monotonic allocation counter.
    pub fn signature(&self) -> u64 {
        let mut h = StableHasher::new();
        let mut value = match serde_json::to_value(self) {
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
}
