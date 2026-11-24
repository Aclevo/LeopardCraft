//! Node-based shading for LeopardCraft.
//!
//! Materials are small dataflow graphs evaluated on the CPU for the path
//! tracer and for viewport vertex colours. No shader compilation or external
//! service is involved, which keeps results deterministic and testable.

pub mod graph;
pub mod noise;
pub mod texture;

pub use graph::{
    GeometryAttribute, MathOp, MixMode, NoiseKind, RampStop, ShaderGraph, ShaderLink, ShaderNode,
    ShaderNodeKind, ShaderValue, ShadingContext, Surface,
};
pub use texture::{Texture2D, TextureCache};

use glam::Vec2;
use lc_anim::{AnimChannel, AnimationClip, ChannelTarget};
use lc_core::StableHasher;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type MaterialId = u32;

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub id: MaterialId,
    pub name: String,
    pub graph: ShaderGraph,
    #[serde(default = "default_true")]
    pub double_sided: bool,
    #[serde(default = "default_true")]
    pub casts_shadow: bool,
    /// Material-scope animation (roughness, metallic).
    #[serde(default)]
    pub animation: AnimationClip,
}

impl Material {
    pub fn new(id: MaterialId, name: impl Into<String>) -> Self {
        Self::pbr(id, name, glam::Vec3::splat(0.8), 0.5, 0.0)
    }

    pub fn pbr(
        id: MaterialId,
        name: impl Into<String>,
        base_color: glam::Vec3,
        roughness: f32,
        metallic: f32,
    ) -> Self {
        let mut graph = ShaderGraph::new_principled();
        if let Some(p) = graph
            .nodes
            .iter_mut()
            .find(|n| matches!(n.kind, ShaderNodeKind::Principled))
        {
            if let Some(input) = p.inputs.get_mut(0) {
                *input = ShaderValue::Color(base_color);
            }
            if let Some(input) = p.inputs.get_mut(1) {
                *input = ShaderValue::Float(metallic);
            }
            if let Some(input) = p.inputs.get_mut(2) {
                *input = ShaderValue::Float(roughness);
            }
        }
        Self {
            id,
            name: name.into(),
            graph,
            double_sided: true,
            casts_shadow: true,
            animation: AnimationClip::default(),
        }
    }

    pub fn with_transmission(mut self, transmission: f32, ior: f32) -> Self {
        if let Some(p) = self
            .graph
            .nodes
            .iter_mut()
            .find(|n| matches!(n.kind, ShaderNodeKind::Principled))
        {
            if let Some(input) = p.inputs.get_mut(3) {
                *input = ShaderValue::Float(ior);
            }
            if let Some(input) = p.inputs.get_mut(4) {
                *input = ShaderValue::Float(transmission);
            }
        }
        self
    }

    pub fn evaluate(&self, ctx: &ShadingContext) -> Surface {
        self.evaluate_at(ctx, ctx.time)
    }

    /// Evaluate the material at an explicit time so material-scope animation
    /// applies consistently in previews and offline renders.
    pub fn evaluate_at(&self, ctx: &ShadingContext, time: f32) -> Surface {
        let mut surface = self.graph.evaluate(ctx);
        if self.animation.is_empty() {
            return surface;
        }
        // One scratch buffer per call samples each material channel once, even
        // though several surface fields are read from it.
        let samples = self.animation.sample_channels(time);
        let target = ChannelTarget::Material { id: self.id };
        if let Some(value) = samples.scalar_for(target, AnimChannel::MaterialRoughness, 0) {
            surface.roughness = value.clamp(0.0, 1.0);
        }
        if let Some(value) = samples.scalar_for(target, AnimChannel::MaterialMetallic, 0) {
            surface.metallic = value.clamp(0.0, 1.0);
        }
        surface
    }

    /// A representative base colour, used by the real-time viewport and
    /// material swatches. UV (0.37, 0.61) avoids noisy seams on most graphs.
    pub fn preview_color(&self, textures: &TextureCache) -> glam::Vec3 {
        let ctx = ShadingContext {
            uv: Vec2::new(0.37, 0.61),
            position: glam::Vec3::ZERO,
            normal: glam::Vec3::Y,
            tangent: glam::Vec3::X,
            view_dir: glam::Vec3::Z,
            time: 0.0,
            extra_uvs: &[],
            textures,
        };
        self.evaluate(&ctx)
            .base_color
            .clamp(glam::Vec3::ZERO, glam::Vec3::ONE)
    }

    pub fn preview_roughness(&self, textures: &TextureCache) -> f32 {
        let ctx = ShadingContext {
            uv: Vec2::new(0.37, 0.61),
            position: glam::Vec3::ZERO,
            normal: glam::Vec3::Y,
            tangent: glam::Vec3::X,
            view_dir: glam::Vec3::Z,
            time: 0.0,
            extra_uvs: &[],
            textures,
        };
        self.evaluate(&ctx).roughness
    }

    pub fn preview_metallic(&self, textures: &TextureCache) -> f32 {
        let ctx = ShadingContext {
            uv: Vec2::new(0.37, 0.61),
            position: glam::Vec3::ZERO,
            normal: glam::Vec3::Y,
            tangent: glam::Vec3::X,
            view_dir: glam::Vec3::Z,
            time: 0.0,
            extra_uvs: &[],
            textures,
        };
        self.evaluate(&ctx).metallic
    }

    /// Graph roughness without any animation applied. Recording a new key must
    /// capture the authored graph value, not the currently evaluated key.
    pub fn base_roughness(&self, textures: &TextureCache) -> f32 {
        let ctx = ShadingContext {
            uv: Vec2::new(0.37, 0.61),
            position: glam::Vec3::ZERO,
            normal: glam::Vec3::Y,
            tangent: glam::Vec3::X,
            view_dir: glam::Vec3::Z,
            time: 0.0,
            extra_uvs: &[],
            textures,
        };
        self.graph.evaluate(&ctx).roughness
    }

    /// Graph metallic without any animation applied.
    pub fn base_metallic(&self, textures: &TextureCache) -> f32 {
        let ctx = ShadingContext {
            uv: Vec2::new(0.37, 0.61),
            position: glam::Vec3::ZERO,
            normal: glam::Vec3::Y,
            tangent: glam::Vec3::X,
            view_dir: glam::Vec3::Z,
            time: 0.0,
            extra_uvs: &[],
            textures,
        };
        self.graph.evaluate(&ctx).metallic
    }

    pub fn signature(&self) -> u64 {
        let mut h = StableHasher::new();
        h.write_u32(self.id);
        h.write_str(&self.name);
        h.write_u64(self.graph.signature());
        h.write_u64(self.animation.signature());
        h.finish()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaterialLibrary {
    pub materials: BTreeMap<MaterialId, Material>,
    #[serde(default)]
    pub next_id: MaterialId,
}

impl Default for MaterialLibrary {
    fn default() -> Self {
        let mut library = Self {
            materials: BTreeMap::new(),
            next_id: 0,
        };
        library.insert(Material::pbr(
            0,
            "Studio Grey",
            glam::Vec3::new(0.62, 0.62, 0.65),
            0.5,
            0.0,
        ));
        library.insert(Material::pbr(
            1,
            "Brushed Copper",
            glam::Vec3::new(0.95, 0.55, 0.30),
            0.28,
            1.0,
        ));
        library.insert(
            Material::pbr(2, "Clear Glass", glam::Vec3::ONE, 0.03, 0.0)
                .with_transmission(1.0, 1.45),
        );
        library.insert(Material::pbr(
            3,
            "Cobalt",
            glam::Vec3::new(0.06, 0.16, 0.58),
            0.24,
            0.15,
        ));
        library.insert(Material::pbr(
            4,
            "Warm Clay",
            glam::Vec3::new(0.85, 0.42, 0.30),
            0.65,
            0.0,
        ));

        // A graph-driven material demonstrating procedural nodes.
        let mut checker = Material::pbr(5, "Checker Tiles", glam::Vec3::ONE, 0.42, 0.0);
        let principled = checker
            .graph
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ShaderNodeKind::Principled))
            .map(|n| n.id)
            .unwrap_or(0);
        let checker_node = checker.graph.add_node(
            ShaderNodeKind::Checker {
                scale: 6.0,
                color_a: glam::Vec3::new(0.92, 0.90, 0.86),
                color_b: glam::Vec3::new(0.16, 0.18, 0.20),
            },
            Vec2::new(40.0, 300.0),
        );
        checker.graph.link(checker_node, principled, 0);
        let noise_node = checker.graph.add_node(
            ShaderNodeKind::Noise {
                kind: NoiseKind::Fractal,
                scale: 12.0,
                detail: 4,
                roughness: 0.55,
                distortion: 0.2,
            },
            Vec2::new(40.0, 470.0),
        );
        let ramp = checker.graph.add_node(
            ShaderNodeKind::ColorRamp {
                stops: vec![
                    RampStop {
                        position: 0.0,
                        color: glam::Vec3::splat(0.18),
                    },
                    RampStop {
                        position: 1.0,
                        color: glam::Vec3::splat(0.72),
                    },
                ],
            },
            Vec2::new(240.0, 470.0),
        );
        checker.graph.link(noise_node, ramp, 0);
        checker.graph.link(ramp, principled, 2);
        library.insert(checker);
        library.next_id = 6;
        library
    }
}

impl MaterialLibrary {
    pub fn get(&self, id: MaterialId) -> Option<&Material> {
        self.materials.get(&id)
    }

    pub fn get_mut(&mut self, id: MaterialId) -> Option<&mut Material> {
        self.materials.get_mut(&id)
    }

    pub fn insert(&mut self, material: Material) {
        self.next_id = self.next_id.max(material.id + 1);
        self.materials.insert(material.id, material);
    }

    pub fn add(&mut self, mut material: Material) -> MaterialId {
        let id = self.next_id.max(
            self.materials
                .keys()
                .next_back()
                .map(|id| id + 1)
                .unwrap_or(0),
        );
        material.id = id;
        self.next_id = id + 1;
        self.materials.insert(id, material);
        id
    }

    pub fn remove(&mut self, id: MaterialId) -> Option<Material> {
        if self.materials.len() <= 1 {
            return None;
        }
        self.materials.remove(&id)
    }

    pub fn len(&self) -> usize {
        self.materials.len()
    }

    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Material> {
        self.materials.values()
    }

    pub fn texture_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .materials
            .values()
            .flat_map(|m| m.graph.texture_paths())
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    pub fn preload_textures(&self, cache: &mut TextureCache) {
        for path in self.texture_paths() {
            cache.ensure(&path);
        }
    }

    pub fn signature(&self) -> u64 {
        let mut h = StableHasher::new();
        for m in self.materials.values() {
            h.write_u64(m.signature());
        }
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_library_has_named_materials() {
        let lib = MaterialLibrary::default();
        assert!(lib.len() >= 6);
        assert_eq!(lib.get(1).unwrap().name, "Brushed Copper");
        assert!(lib.get(2).unwrap().graph.nodes.len() >= 2);
        assert!(lib.get(5).unwrap().graph.nodes.len() >= 5);
    }

    #[test]
    fn transmission_material_evaluates_glass() {
        let lib = MaterialLibrary::default();
        let cache = TextureCache::default();
        let glass = lib.get(2).unwrap();
        let ctx = ShadingContext::new(&cache);
        let s = glass.evaluate(&ctx);
        assert!(s.transmission > 0.99);
        assert!((s.ior - 1.45).abs() < 1e-4);
    }

    #[test]
    fn animated_material_scalars_apply_at_time() {
        let mut material = Material::pbr(7, "Animated", glam::Vec3::splat(0.5), 0.2, 0.1);
        let roughness = lc_anim::ChannelAddress::owner(AnimChannel::MaterialRoughness, 0).unwrap();
        material
            .animation
            .set_key(roughness, 0.0, 0.2, lc_anim::Interpolation::Linear);
        material
            .animation
            .set_key(roughness, 1.0, 0.8, lc_anim::Interpolation::Linear);
        let cache = TextureCache::default();
        let ctx = ShadingContext::new(&cache);
        assert!((material.evaluate_at(&ctx, 0.5).roughness - 0.5).abs() < 1e-4);
        assert!((material.evaluate_at(&ctx, 0.5).metallic - 0.1).abs() < 1e-4);
    }

    #[test]
    fn adding_materials_allocates_unique_ids() {
        let mut lib = MaterialLibrary::default();
        let id = lib.add(Material::new(99, "Test"));
        assert_ne!(id, 1);
        assert!(lib.get(id).is_some());
        assert_eq!(lib.get(id).unwrap().id, id);
    }
}
