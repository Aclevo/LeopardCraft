//! Node-based material graphs. Nodes are data, links connect a node's single
//! output to a named input of another node, and evaluation is a pure function
//! of the shading context.

use crate::noise;
use crate::texture::TextureCache;
use glam::{Vec2, Vec3};
use lc_core::{orthonormal_basis, StableHasher};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ShaderValue {
    Float(f32),
    Color(Vec3),
    Vector(Vec3),
}

impl Default for ShaderValue {
    fn default() -> Self {
        Self::Float(0.0)
    }
}

impl ShaderValue {
    pub fn as_float(self) -> f32 {
        match self {
            Self::Float(v) => v,
            Self::Color(c) | Self::Vector(c) => (c.x + c.y + c.z) / 3.0,
        }
    }

    pub fn as_color(self) -> Vec3 {
        match self {
            Self::Float(v) => Vec3::splat(v),
            Self::Color(c) | Self::Vector(c) => c,
        }
    }

    pub fn as_vector(self) -> Vec3 {
        match self {
            Self::Float(v) => Vec3::splat(v),
            Self::Color(c) | Self::Vector(c) => c,
        }
    }

    pub fn color(c: Vec3) -> Self {
        Self::Color(c)
    }

    pub fn float(v: f32) -> Self {
        Self::Float(v)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoiseKind {
    Value,
    Fractal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeometryAttribute {
    Position,
    Normal,
    Tangent,
    ViewDirection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MathOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
    Min,
    Max,
    Sine,
    Cosine,
    Absolute,
    Fraction,
}

impl MathOp {
    pub fn label(self) -> &'static str {
        match self {
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Multiply => "Multiply",
            Self::Divide => "Divide",
            Self::Power => "Power",
            Self::Min => "Min",
            Self::Max => "Max",
            Self::Sine => "Sine",
            Self::Cosine => "Cosine",
            Self::Absolute => "Absolute",
            Self::Fraction => "Fraction",
        }
    }

    pub fn apply(self, a: f32, b: f32) -> f32 {
        match self {
            Self::Add => a + b,
            Self::Subtract => a - b,
            Self::Multiply => a * b,
            Self::Divide => {
                if b.abs() < 1e-8 {
                    0.0
                } else {
                    a / b
                }
            }
            Self::Power => a.max(0.0).powf(b),
            Self::Min => a.min(b),
            Self::Max => a.max(b),
            Self::Sine => a.sin(),
            Self::Cosine => a.cos(),
            Self::Absolute => a.abs(),
            Self::Fraction => a.fract().abs(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MixMode {
    Mix,
    Multiply,
    Add,
    Screen,
    Overlay,
}

impl MixMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mix => "Mix",
            Self::Multiply => "Multiply",
            Self::Add => "Add",
            Self::Screen => "Screen",
            Self::Overlay => "Overlay",
        }
    }

    pub fn apply(self, a: Vec3, b: Vec3, factor: f32) -> Vec3 {
        let f = factor.clamp(0.0, 1.0);
        let blended = match self {
            Self::Mix => b,
            Self::Multiply => a * b,
            Self::Add => a + b,
            Self::Screen => Vec3::ONE - (Vec3::ONE - a) * (Vec3::ONE - b),
            Self::Overlay => {
                let low = a * 2.0 * b;
                let high = Vec3::ONE - (Vec3::ONE - a) * (Vec3::ONE - b) * 2.0;
                Vec3::new(
                    if a.x < 0.5 { low.x } else { high.x },
                    if a.y < 0.5 { low.y } else { high.y },
                    if a.z < 0.5 { low.z } else { high.z },
                )
            }
        };
        a.lerp(blended, f)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RampStop {
    pub position: f32,
    pub color: Vec3,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShaderNodeKind {
    MaterialOutput,
    Principled,
    Value {
        value: f32,
    },
    Rgb {
        color: Vec3,
    },
    Noise {
        kind: NoiseKind,
        scale: f32,
        detail: u32,
        roughness: f32,
        distortion: f32,
    },
    Voronoi {
        scale: f32,
        jitter: f32,
    },
    Checker {
        scale: f32,
        color_a: Vec3,
        color_b: Vec3,
    },
    Gradient {
        axis: u8,
        scale: f32,
    },
    Uv,
    Geometry {
        attribute: GeometryAttribute,
    },
    Fresnel {
        ior: f32,
    },
    Bump {
        strength: f32,
        distance: f32,
    },
    NormalMap {
        strength: f32,
    },
    Mapping {
        scale: Vec3,
        offset: Vec3,
        rotation_z_deg: f32,
    },
    Math {
        op: MathOp,
    },
    Mix {
        mode: MixMode,
    },
    ColorRamp {
        stops: Vec<RampStop>,
    },
    Texture {
        path: String,
        /// Which UV set the image is sampled with. 0 is UV0; higher values use
        /// the hit's additional sets, falling back to UV0 when the mesh has no
        /// such set. Old files default to UV0.
        #[serde(default)]
        uv_set: u32,
    },
}

impl ShaderNodeKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::MaterialOutput => "Material Output",
            Self::Principled => "Principled BSDF",
            Self::Value { .. } => "Value",
            Self::Rgb { .. } => "RGB",
            Self::Noise { .. } => "Noise",
            Self::Voronoi { .. } => "Voronoi",
            Self::Checker { .. } => "Checker",
            Self::Gradient { .. } => "Gradient",
            Self::Uv => "Texture Coordinate",
            Self::Geometry { .. } => "Geometry",
            Self::Fresnel { .. } => "Fresnel",
            Self::Bump { .. } => "Bump",
            Self::NormalMap { .. } => "Normal Map",
            Self::Mapping { .. } => "Mapping",
            Self::Math { .. } => "Math",
            Self::Mix { .. } => "Mix Color",
            Self::ColorRamp { .. } => "Color Ramp",
            Self::Texture { .. } => "Image Texture",
        }
    }

    pub fn input_names(&self) -> &'static [&'static str] {
        match self {
            Self::Principled => &[
                "Base Color",
                "Metallic",
                "Roughness",
                "IOR",
                "Transmission",
                "Emission",
                "Emission Strength",
                "Alpha",
                "Normal",
            ],
            Self::Bump { .. } => &["Height"],
            Self::NormalMap { .. } => &["Color"],
            Self::Mapping { .. } => &["Vector"],
            Self::Math { .. } => &["A", "B"],
            Self::Mix { .. } => &["Factor", "A", "B"],
            Self::ColorRamp { .. } => &["Factor"],
            Self::Texture { .. } => &["Vector"],
            Self::MaterialOutput => &["Surface"],
            _ => &[],
        }
    }

    pub fn default_inputs(&self) -> Vec<ShaderValue> {
        match self {
            Self::Principled => vec![
                ShaderValue::Color(Vec3::new(0.8, 0.8, 0.8)),
                ShaderValue::Float(0.0),
                ShaderValue::Float(0.45),
                ShaderValue::Float(1.45),
                ShaderValue::Float(0.0),
                ShaderValue::Color(Vec3::ZERO),
                ShaderValue::Float(0.0),
                ShaderValue::Float(1.0),
                ShaderValue::Vector(Vec3::ZERO),
            ],
            Self::MaterialOutput => vec![ShaderValue::Color(Vec3::ZERO)],
            Self::Bump { .. } => vec![ShaderValue::Float(0.0)],
            Self::NormalMap { .. } => vec![ShaderValue::Color(Vec3::new(0.5, 0.5, 1.0))],
            Self::Mapping { .. } => vec![ShaderValue::Vector(Vec3::ZERO)],
            Self::Math { .. } => vec![ShaderValue::Float(0.0), ShaderValue::Float(1.0)],
            Self::Mix { .. } => vec![
                ShaderValue::Float(0.5),
                ShaderValue::Color(Vec3::splat(0.8)),
                ShaderValue::Color(Vec3::splat(0.2)),
            ],
            Self::ColorRamp { .. } => vec![ShaderValue::Float(0.5)],
            Self::Texture { .. } => vec![ShaderValue::Vector(Vec3::ZERO)],
            Self::Value { value } => vec![ShaderValue::Float(*value)],
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShaderNode {
    pub id: u32,
    pub kind: ShaderNodeKind,
    pub position: Vec2,
    pub inputs: Vec<ShaderValue>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShaderLink {
    pub from: u32,
    pub to: u32,
    pub to_input: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ShaderGraph {
    pub nodes: Vec<ShaderNode>,
    pub links: Vec<ShaderLink>,
    pub next_id: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct ShadingContext<'a> {
    pub uv: Vec2,
    /// Additional UV sets at the shaded point, starting at UV1. Empty when the
    /// hit carries no extra sets, in which case every texture falls back to
    /// `uv`.
    pub extra_uvs: &'a [Vec2],
    pub position: Vec3,
    pub normal: Vec3,
    pub tangent: Vec3,
    pub view_dir: Vec3,
    pub time: f32,
    pub textures: &'a TextureCache,
}

impl<'a> ShadingContext<'a> {
    pub fn new(textures: &'a TextureCache) -> Self {
        Self {
            uv: Vec2::ZERO,
            extra_uvs: &[],
            position: Vec3::ZERO,
            normal: Vec3::Y,
            tangent: Vec3::X,
            view_dir: Vec3::Z,
            time: 0.0,
            textures,
        }
    }

    pub fn at_uv(mut self, uv: Vec2, offset: Vec3) -> Self {
        self.uv = uv;
        self.position += offset;
        self
    }

    /// The coordinate a texture bound to `set` should sample. UV0 and missing
    /// sets read the primary coordinate, so a material never samples a stale
    /// set just because the mesh lost one.
    pub fn uv_for_set(&self, set: u32) -> Vec2 {
        if set == 0 {
            self.uv
        } else {
            self.extra_uvs
                .get(set as usize - 1)
                .copied()
                .unwrap_or(self.uv)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Surface {
    pub base_color: Vec3,
    pub roughness: f32,
    pub metallic: f32,
    pub ior: f32,
    pub transmission: f32,
    pub emission: Vec3,
    pub alpha: f32,
    pub normal: Vec3,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            base_color: Vec3::splat(0.8),
            roughness: 0.5,
            metallic: 0.0,
            ior: 1.45,
            transmission: 0.0,
            emission: Vec3::ZERO,
            alpha: 1.0,
            normal: Vec3::ZERO,
        }
    }
}

impl ShaderValue {
    pub fn is_finite(self) -> bool {
        match self {
            Self::Float(value) => value.is_finite(),
            Self::Color(value) | Self::Vector(value) => value.is_finite(),
        }
    }
}

fn shader_kind_is_finite(kind: &ShaderNodeKind) -> bool {
    match kind {
        ShaderNodeKind::MaterialOutput | ShaderNodeKind::Principled | ShaderNodeKind::Uv => true,
        ShaderNodeKind::Value { value } => value.is_finite(),
        ShaderNodeKind::Rgb { color } => color.is_finite(),
        ShaderNodeKind::Noise {
            scale,
            roughness,
            distortion,
            ..
        } => scale.is_finite() && roughness.is_finite() && distortion.is_finite(),
        ShaderNodeKind::Voronoi { scale, jitter } => scale.is_finite() && jitter.is_finite(),
        ShaderNodeKind::Checker {
            scale,
            color_a,
            color_b,
        } => scale.is_finite() && color_a.is_finite() && color_b.is_finite(),
        ShaderNodeKind::Gradient { scale, .. } => scale.is_finite(),
        ShaderNodeKind::Geometry { .. } => true,
        ShaderNodeKind::Fresnel { ior } => ior.is_finite(),
        ShaderNodeKind::Bump { strength, distance } => strength.is_finite() && distance.is_finite(),
        ShaderNodeKind::NormalMap { strength } => strength.is_finite(),
        ShaderNodeKind::Mapping {
            scale,
            offset,
            rotation_z_deg,
        } => scale.is_finite() && offset.is_finite() && rotation_z_deg.is_finite(),
        ShaderNodeKind::Math { .. } | ShaderNodeKind::Mix { .. } => true,
        ShaderNodeKind::ColorRamp { stops } => stops
            .iter()
            .all(|stop| stop.position.is_finite() && stop.color.is_finite()),
        ShaderNodeKind::Texture { .. } => true,
    }
}

impl ShaderGraph {
    /// Check the invariants that the editor's link/add operations maintain.
    /// Serialized graphs arrive from outside the process, so a graph with a
    /// duplicate node id, a dangling link or a non-finite parameter must be
    /// rejected before it can replace a material.
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = std::collections::BTreeSet::new();
        let mut outputs = 0usize;
        let mut max_id = None;
        for node in &self.nodes {
            if !ids.insert(node.id) {
                return Err(format!("duplicate shader node id {}", node.id));
            }
            max_id = Some(max_id.map_or(node.id, |max: u32| max.max(node.id)));
            if matches!(node.kind, ShaderNodeKind::MaterialOutput) {
                outputs += 1;
            }
            if !node.position.is_finite() {
                return Err(format!("shader node {} has a non-finite position", node.id));
            }
            if node.inputs.iter().any(|value| !value.is_finite()) {
                return Err(format!("shader node {} has a non-finite input", node.id));
            }
            if !shader_kind_is_finite(&node.kind) {
                return Err(format!("shader node {} has non-finite parameters", node.id));
            }
        }
        if outputs > 1 {
            return Err("a shader graph can contain only one Material Output node".into());
        }
        if let Some(max_id) = max_id {
            if self.next_id <= max_id {
                return Err(format!(
                    "shader graph next_id {} must be greater than the highest node id {max_id}",
                    self.next_id
                ));
            }
        }
        let mut targets = std::collections::BTreeSet::new();
        for link in &self.links {
            let Some(from) = self.node(link.from) else {
                return Err(format!("shader link references missing node {}", link.from));
            };
            let Some(to) = self.node(link.to) else {
                return Err(format!("shader link references missing node {}", link.to));
            };
            if from.id == to.id {
                return Err(format!("shader node {} links to itself", link.from));
            }
            if link.to_input >= to.kind.input_names().len() {
                return Err(format!(
                    "shader node {} has no input {}",
                    link.to, link.to_input
                ));
            }
            if !targets.insert((link.to, link.to_input)) {
                return Err(format!(
                    "shader node {} input {} is linked more than once",
                    link.to, link.to_input
                ));
            }
        }
        Ok(())
    }

    pub fn empty() -> Self {
        Self::default()
    }

    /// A graph with a Principled BSDF already wired to the output.
    pub fn new_principled() -> Self {
        let mut g = Self::default();
        let output = g.add_node(ShaderNodeKind::MaterialOutput, Vec2::new(360.0, 60.0));
        let principled = g.add_node(ShaderNodeKind::Principled, Vec2::new(60.0, 40.0));
        g.link(principled, output, 0);
        g
    }

    pub fn add_node(&mut self, kind: ShaderNodeKind, position: Vec2) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let inputs = kind.default_inputs();
        self.nodes.push(ShaderNode {
            id,
            kind,
            position,
            inputs,
        });
        id
    }

    pub fn remove_node(&mut self, id: u32) {
        self.nodes.retain(|n| n.id != id);
        self.links.retain(|l| l.from != id && l.to != id);
    }

    pub fn node(&self, id: u32) -> Option<&ShaderNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: u32) -> Option<&mut ShaderNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn output_node(&self) -> Option<&ShaderNode> {
        self.nodes
            .iter()
            .find(|n| matches!(n.kind, ShaderNodeKind::MaterialOutput))
    }

    pub fn has_link(&self, to: u32, to_input: usize) -> bool {
        self.links
            .iter()
            .any(|l| l.to == to && l.to_input == to_input)
    }

    pub fn link(&mut self, from: u32, to: u32, to_input: usize) -> bool {
        if from == to || self.node(from).is_none() || self.node(to).is_none() {
            return false;
        }
        let names = self
            .node(to)
            .map(|n| n.kind.input_names().len())
            .unwrap_or(0);
        if to_input >= names {
            return false;
        }
        self.links
            .retain(|l| !(l.to == to && l.to_input == to_input));
        self.links.push(ShaderLink { from, to, to_input });
        true
    }

    pub fn unlink(&mut self, to: u32, to_input: usize) {
        self.links
            .retain(|l| !(l.to == to && l.to_input == to_input));
    }

    pub fn texture_paths(&self) -> Vec<String> {
        self.nodes
            .iter()
            .filter_map(|n| match &n.kind {
                ShaderNodeKind::Texture { path, .. } if !path.is_empty() => Some(path.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn signature(&self) -> u64 {
        let mut h = StableHasher::new();
        for n in &self.nodes {
            h.write_u32(n.id);
            h.write_str(n.kind.label());
            h.write_vec2(n.position);
            for i in &n.inputs {
                match i {
                    ShaderValue::Float(v) => h.write_f32(*v),
                    ShaderValue::Color(c) | ShaderValue::Vector(c) => h.write_vec3(*c),
                }
            }
            if let ShaderNodeKind::Texture { path, uv_set } = &n.kind {
                h.write_str(path);
                h.write_u32(*uv_set);
            }
            if let ShaderNodeKind::ColorRamp { stops } = &n.kind {
                for s in stops {
                    h.write_f32(s.position);
                    h.write_vec3(s.color);
                }
            }
        }
        h.write_u64(self.links.len() as u64);
        for l in &self.links {
            h.write_u32(l.from);
            h.write_u32(l.to);
            h.write_u32(l.to_input as u32);
        }
        h.finish()
    }

    fn input_value(
        &self,
        node_id: u32,
        index: usize,
        ctx: &ShadingContext,
        depth: usize,
        stack: &mut Vec<u32>,
    ) -> ShaderValue {
        let Some(node) = self.node(node_id) else {
            return ShaderValue::Float(0.0);
        };
        if let Some(link) = self
            .links
            .iter()
            .find(|l| l.to == node_id && l.to_input == index)
        {
            self.eval_node(link.from, ctx, depth + 1, stack)
        } else {
            node.inputs.get(index).cloned().unwrap_or_default()
        }
    }

    fn eval_node(
        &self,
        id: u32,
        ctx: &ShadingContext,
        depth: usize,
        stack: &mut Vec<u32>,
    ) -> ShaderValue {
        if depth > 64 || stack.contains(&id) {
            return ShaderValue::Float(0.0); // graph cycle guard
        }
        let Some(node) = self.node(id) else {
            return ShaderValue::Float(0.0);
        };
        stack.push(id);
        let value = match &node.kind {
            ShaderNodeKind::MaterialOutput => ShaderValue::Color(Vec3::ZERO),
            ShaderNodeKind::Principled => {
                let c = self.input_value(id, 0, ctx, depth, stack);
                ShaderValue::Color(c.as_color())
            }
            ShaderNodeKind::Value { value } => ShaderValue::Float(*value),
            ShaderNodeKind::Rgb { color } => ShaderValue::Color(*color),
            ShaderNodeKind::Noise {
                kind,
                scale,
                detail,
                roughness,
                distortion,
            } => {
                let p = Vec3::new(ctx.uv.x, ctx.uv.y, 0.37) * scale.max(0.0001);
                let n = match kind {
                    NoiseKind::Value => noise::value_noise(p, 17),
                    NoiseKind::Fractal => {
                        let warp = if *distortion > 0.0 {
                            Vec3::new(
                                noise::value_noise(p * 0.5, 3),
                                noise::value_noise(p * 0.5, 5),
                                noise::value_noise(p * 0.5, 7),
                            ) * *distortion
                        } else {
                            Vec3::ZERO
                        };
                        noise::fbm(p + warp, *detail, *roughness, 2.0, 11)
                    }
                };
                ShaderValue::Float(n * 0.5 + 0.5)
            }
            ShaderNodeKind::Voronoi { scale, jitter } => {
                let p = Vec3::new(ctx.uv.x, ctx.uv.y, 0.21) * scale.max(0.0001);
                ShaderValue::Float(noise::voronoi(p, *jitter, 5))
            }
            ShaderNodeKind::Checker {
                scale,
                color_a,
                color_b,
            } => {
                let s = scale.max(0.0001);
                let x = (ctx.uv.x * s).floor() as i64;
                let y = (ctx.uv.y * s).floor() as i64;
                let c = if (x + y).rem_euclid(2) == 0 {
                    *color_a
                } else {
                    *color_b
                };
                ShaderValue::Color(c)
            }
            ShaderNodeKind::Gradient { axis, scale } => {
                let v = match axis {
                    0 => ctx.uv.x,
                    _ => ctx.uv.y,
                };
                ShaderValue::Float((v * scale.max(0.0001)).clamp(0.0, 1.0))
            }
            ShaderNodeKind::Uv => ShaderValue::Vector(Vec3::new(ctx.uv.x, ctx.uv.y, 0.0)),
            ShaderNodeKind::Geometry { attribute } => match attribute {
                GeometryAttribute::Position => ShaderValue::Vector(ctx.position),
                GeometryAttribute::Normal => ShaderValue::Vector(ctx.normal),
                GeometryAttribute::Tangent => ShaderValue::Vector(ctx.tangent),
                GeometryAttribute::ViewDirection => ShaderValue::Vector(ctx.view_dir),
            },
            ShaderNodeKind::Fresnel { ior } => {
                let r0 = ((ior - 1.0) / (ior + 1.0)).powi(2);
                let cos = ctx.normal.dot(ctx.view_dir).abs().clamp(0.0, 1.0);
                ShaderValue::Float(r0 + (1.0 - r0) * (1.0 - cos).powi(5))
            }
            ShaderNodeKind::Bump { strength, distance } => {
                let n = ctx.normal.normalize_or(Vec3::Y);
                let (t, b) = orthonormal_basis(n);
                let eps = 0.01f32.max(*distance * 0.05);
                let h = |c: &ShadingContext| -> f32 {
                    self.input_value(id, 0, c, depth + 1, &mut stack.clone())
                        .as_float()
                };
                let h0 = h(ctx);
                let hx = h(&ctx.at_uv(ctx.uv + Vec2::new(eps, 0.0), t * eps));
                let hy = h(&ctx.at_uv(ctx.uv + Vec2::new(0.0, eps), b * eps));
                let slope = t * ((hx - h0) / eps) + b * ((hy - h0) / eps);
                let perturbed = (n - slope * (*strength) / distance.max(0.001)).normalize_or(n);
                ShaderValue::Vector(perturbed)
            }
            ShaderNodeKind::NormalMap { strength } => {
                let c = self.input_value(id, 0, ctx, depth + 1, stack).as_color();
                let local = Vec3::new(c.x * 2.0 - 1.0, c.y * 2.0 - 1.0, c.z * 2.0 - 1.0);
                let n = ctx.normal.normalize_or(Vec3::Y);
                let tangent = if ctx.tangent.length_squared() > 1e-8 {
                    (ctx.tangent - n * ctx.tangent.dot(n)).normalize_or(Vec3::X)
                } else {
                    orthonormal_basis(n).0
                };
                let bitangent = n.cross(tangent);
                let world = (tangent * local.x + bitangent * local.y + n * local.z).normalize_or(n);
                ShaderValue::Vector(n.lerp(world, strength.clamp(0.0, 1.0)).normalize_or(n))
            }
            ShaderNodeKind::Mapping {
                scale,
                offset,
                rotation_z_deg,
            } => {
                let v = self.input_value(id, 0, ctx, depth, stack).as_vector();
                let v = Vec3::new(ctx.uv.x, ctx.uv.y, 0.0) + v;
                let a = rotation_z_deg.to_radians();
                let rotated = Vec3::new(
                    v.x * a.cos() - v.y * a.sin(),
                    v.x * a.sin() + v.y * a.cos(),
                    v.z,
                );
                ShaderValue::Vector(rotated * *scale + *offset)
            }
            ShaderNodeKind::Math { op } => {
                let a = self.input_value(id, 0, ctx, depth, stack).as_float();
                let b = self.input_value(id, 1, ctx, depth, stack).as_float();
                ShaderValue::Float(op.apply(a, b))
            }
            ShaderNodeKind::Mix { mode } => {
                let f = self.input_value(id, 0, ctx, depth, stack).as_float();
                let a = self.input_value(id, 1, ctx, depth, stack).as_color();
                let b = self.input_value(id, 2, ctx, depth, stack).as_color();
                ShaderValue::Color(mode.apply(a, b, f))
            }
            ShaderNodeKind::ColorRamp { stops } => {
                let f = self
                    .input_value(id, 0, ctx, depth, stack)
                    .as_float()
                    .clamp(0.0, 1.0);
                let mut sorted = stops.clone();
                sorted.sort_by(|a, b| {
                    a.position
                        .partial_cmp(&b.position)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let c = if sorted.is_empty() {
                    Vec3::splat(f)
                } else if f <= sorted[0].position {
                    sorted[0].color
                } else if f >= sorted[sorted.len() - 1].position {
                    sorted[sorted.len() - 1].color
                } else {
                    let mut result = sorted[0].color;
                    for w in sorted.windows(2) {
                        if f >= w[0].position && f <= w[1].position {
                            let span = (w[1].position - w[0].position).max(1e-6);
                            result = w[0].color.lerp(w[1].color, (f - w[0].position) / span);
                            break;
                        }
                    }
                    result
                };
                ShaderValue::Color(c)
            }
            ShaderNodeKind::Texture { path, uv_set } => {
                let uv = if self.has_link(id, 0) {
                    let v = self.input_value(id, 0, ctx, depth, stack).as_vector();
                    Vec2::new(v.x, v.y)
                } else {
                    ctx.uv_for_set(*uv_set)
                };
                let color = if path.is_empty() {
                    Vec3::splat(0.8)
                } else {
                    ctx.textures
                        .get(path)
                        .map(|t| t.sample(uv))
                        .unwrap_or(Vec3::new(1.0, 0.0, 1.0))
                };
                ShaderValue::Color(color)
            }
        };
        stack.pop();
        value
    }

    /// Evaluate the surface shader for a hit point.
    pub fn evaluate(&self, ctx: &ShadingContext) -> Surface {
        let Some(output) = self.output_node() else {
            return Surface {
                normal: ctx.normal,
                ..Default::default()
            };
        };
        let Some(link) = self
            .links
            .iter()
            .find(|l| l.to == output.id && l.to_input == 0)
        else {
            return Surface {
                normal: ctx.normal,
                ..Default::default()
            };
        };
        let Some(node) = self.node(link.from) else {
            return Surface {
                normal: ctx.normal,
                ..Default::default()
            };
        };
        let mut stack = Vec::new();
        let mut surface = Surface {
            normal: ctx.normal,
            ..Default::default()
        };
        match &node.kind {
            ShaderNodeKind::Principled => {
                let base = self.input_value(node.id, 0, ctx, 0, &mut stack).as_color();
                surface.base_color = base.max(Vec3::ZERO);
                surface.metallic = self
                    .input_value(node.id, 1, ctx, 0, &mut stack)
                    .as_float()
                    .clamp(0.0, 1.0);
                surface.roughness = self
                    .input_value(node.id, 2, ctx, 0, &mut stack)
                    .as_float()
                    .clamp(0.02, 1.0);
                surface.ior = self
                    .input_value(node.id, 3, ctx, 0, &mut stack)
                    .as_float()
                    .clamp(1.0, 3.0);
                surface.transmission = self
                    .input_value(node.id, 4, ctx, 0, &mut stack)
                    .as_float()
                    .clamp(0.0, 1.0);
                let emission = self.input_value(node.id, 5, ctx, 0, &mut stack).as_color();
                let strength = self
                    .input_value(node.id, 6, ctx, 0, &mut stack)
                    .as_float()
                    .max(0.0);
                surface.emission = emission.max(Vec3::ZERO) * strength;
                surface.alpha = self
                    .input_value(node.id, 7, ctx, 0, &mut stack)
                    .as_float()
                    .clamp(0.0, 1.0);
                if self.has_link(node.id, 8) {
                    let n = self.input_value(node.id, 8, ctx, 0, &mut stack).as_vector();
                    if n.length_squared() > 1e-10 {
                        surface.normal = n.normalize();
                    }
                }
            }
            ShaderNodeKind::MaterialOutput => {}
            _ => {
                let value = self.eval_node(node.id, ctx, 0, &mut stack);
                surface.base_color = value.as_color().max(Vec3::ZERO);
            }
        }
        if !surface.base_color.is_finite() {
            surface.base_color = Vec3::splat(0.8);
        }
        surface
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::TextureCache;

    fn ctx() -> (TextureCache, Vec2, Vec3, Vec3) {
        (
            TextureCache::default(),
            Vec2::new(0.25, 0.75),
            Vec3::ZERO,
            Vec3::Y,
        )
    }

    #[test]
    fn principled_defaults_are_evaluated() {
        let graph = ShaderGraph::new_principled();
        let (tex, uv, pos, normal) = ctx();
        let c = ShadingContext {
            uv,
            position: pos,
            normal,
            tangent: Vec3::X,
            view_dir: Vec3::Z,
            time: 0.0,
            extra_uvs: &[],
            textures: &tex,
        };
        let s = graph.evaluate(&c);
        assert!((s.base_color - Vec3::splat(0.8)).length() < 1e-5);
        assert!((s.roughness - 0.45).abs() < 1e-5);
        assert!(s.normal.dot(normal) > 0.99);
    }

    #[test]
    fn links_override_inputs() {
        let (tex, uv, pos, normal) = ctx();
        let c = ShadingContext {
            uv,
            position: pos,
            normal,
            tangent: Vec3::X,
            view_dir: Vec3::Z,
            time: 0.0,
            extra_uvs: &[],
            textures: &tex,
        };
        let mut graph = ShaderGraph::new_principled();
        let principled = graph
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ShaderNodeKind::Principled))
            .unwrap()
            .id;
        let value = graph.add_node(
            ShaderNodeKind::Rgb {
                color: Vec3::new(1.0, 0.0, 0.0),
            },
            Vec2::ZERO,
        );
        assert!(graph.link(value, principled, 0));
        let s = graph.evaluate(&c);
        assert!((s.base_color - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn texture_uv_set_selects_the_coordinate() {
        use crate::texture::{Texture2D, TextureCache};
        let mut tex = TextureCache::default();
        tex.insert(
            "two",
            Texture2D::from_pixels(2, 1, vec![Vec3::ZERO, Vec3::ONE]),
        );
        let mut graph = ShaderGraph::new_principled();
        let principled = graph.nodes[1].id;
        let node = graph.add_node(
            ShaderNodeKind::Texture {
                path: "two".into(),
                uv_set: 1,
            },
            Vec2::ZERO,
        );
        assert!(graph.link(node, principled, 0));
        let sample = |uv: Vec2, extra: &[Vec2]| {
            let ctx = ShadingContext {
                uv,
                extra_uvs: extra,
                position: Vec3::ZERO,
                normal: Vec3::Y,
                tangent: Vec3::X,
                view_dir: Vec3::Z,
                time: 0.0,
                textures: &tex,
            };
            graph.evaluate(&ctx).base_color.x
        };
        // UV0 is the black texel; UV1 is the white one, so the selected set
        // decides the sampled colour.
        assert!(sample(Vec2::new(0.25, 0.5), &[Vec2::new(0.75, 0.5)]) > 0.9);
        // A mesh without UV1 falls back to UV0 instead of a stale coordinate.
        assert!(sample(Vec2::new(0.25, 0.5), &[]) < 0.1);
        let sig = graph.signature();
        if let ShaderNodeKind::Texture { uv_set, .. } = &mut graph.nodes[2].kind {
            *uv_set = 0;
        }
        assert_ne!(graph.signature(), sig);
    }

    #[test]
    fn cycles_do_not_hang_or_panic() {
        let (tex, uv, pos, normal) = ctx();
        let c = ShadingContext {
            uv,
            position: pos,
            normal,
            tangent: Vec3::X,
            view_dir: Vec3::Z,
            time: 0.0,
            extra_uvs: &[],
            textures: &tex,
        };
        let mut graph = ShaderGraph::new_principled();
        let a = graph.add_node(ShaderNodeKind::Math { op: MathOp::Add }, Vec2::ZERO);
        let b = graph.add_node(
            ShaderNodeKind::Math {
                op: MathOp::Multiply,
            },
            Vec2::ZERO,
        );
        graph.link(a, b, 0);
        graph.link(b, a, 0);
        let _ = graph.evaluate(&c);
    }

    #[test]
    fn procedural_nodes_are_bounded() {
        let (tex, uv, pos, normal) = ctx();
        let c = ShadingContext {
            uv,
            position: pos,
            normal,
            tangent: Vec3::X,
            view_dir: Vec3::Z,
            time: 0.0,
            extra_uvs: &[],
            textures: &tex,
        };
        let mut graph = ShaderGraph::new_principled();
        let principled = graph.nodes[1].id;
        let n = graph.add_node(
            ShaderNodeKind::Noise {
                kind: NoiseKind::Fractal,
                scale: 8.0,
                detail: 4,
                roughness: 0.5,
                distortion: 0.1,
            },
            Vec2::ZERO,
        );
        graph.link(n, principled, 2);
        let s = graph.evaluate(&c);
        assert!((0.02..=1.0).contains(&s.roughness));
    }

    #[test]
    fn graph_serde_roundtrip() {
        let mut g = ShaderGraph::new_principled();
        let ramp = g.add_node(
            ShaderNodeKind::ColorRamp {
                stops: vec![
                    RampStop {
                        position: 0.0,
                        color: Vec3::ZERO,
                    },
                    RampStop {
                        position: 1.0,
                        color: Vec3::ONE,
                    },
                ],
            },
            Vec2::new(10.0, 20.0),
        );
        g.link(ramp, g.nodes[1].id, 0);
        let json = serde_json::to_string(&g).unwrap();
        let back: ShaderGraph = serde_json::from_str(&json).unwrap();
        assert_eq!(g, back);
        assert_eq!(g.signature(), back.signature());
    }
    #[test]
    fn validate_rejects_malformed_serialized_graphs() {
        let graph = ShaderGraph::new_principled();
        assert!(graph.validate().is_ok());

        let mut duplicate = graph.clone();
        duplicate.nodes[1].id = duplicate.nodes[0].id;
        assert!(duplicate
            .validate()
            .unwrap_err()
            .contains("duplicate shader node id"));

        let mut dangling = graph.clone();
        dangling.links[0].from = 99;
        assert!(dangling.validate().unwrap_err().contains("missing node 99"));

        let mut bad_input = graph.clone();
        bad_input.links[0].to_input = 9;
        assert!(bad_input.validate().is_err());

        let mut repeated = graph.clone();
        repeated.links.push(repeated.links[0].clone());
        assert!(repeated
            .validate()
            .unwrap_err()
            .contains("linked more than once"));

        let mut stale_next = graph.clone();
        stale_next.next_id = 0;
        assert!(stale_next.validate().is_err());

        let mut non_finite = graph.clone();
        non_finite.nodes[0].position.x = f32::NAN;
        assert!(non_finite.validate().is_err());
    }
}
