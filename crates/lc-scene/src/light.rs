//! Scene lights. A light points down its local -Z axis.

use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LightKind {
    Directional,
    Point { radius: f32 },
    Area { width: f32, height: f32 },
}

impl Default for LightKind {
    fn default() -> Self {
        Self::Directional
    }
}

impl LightKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Directional => "Directional",
            Self::Point { .. } => "Point",
            Self::Area { .. } => "Area",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Light {
    pub kind: LightKind,
    pub color: Vec3,
    pub intensity: f32,
    pub cast_shadows: bool,
}

impl Default for Light {
    fn default() -> Self {
        Self {
            kind: LightKind::Directional,
            color: Vec3::ONE,
            intensity: 3.0,
            cast_shadows: true,
        }
    }
}

impl Light {
    pub fn directional(intensity: f32) -> Self {
        Self {
            intensity,
            ..Default::default()
        }
    }

    pub fn point(intensity: f32, radius: f32) -> Self {
        Self {
            kind: LightKind::Point {
                radius: radius.max(0.001),
            },
            intensity,
            ..Default::default()
        }
    }

    pub fn area(width: f32, height: f32, intensity: f32) -> Self {
        Self {
            kind: LightKind::Area {
                width: width.max(0.001),
                height: height.max(0.001),
            },
            intensity,
            ..Default::default()
        }
    }

    pub fn position(world: Mat4) -> Vec3 {
        world.w_axis.truncate()
    }

    pub fn direction(world: Mat4) -> Vec3 {
        -world.z_axis.truncate().normalize_or(Vec3::NEG_Z)
    }

    /// Width/height in world units (radians for area lights; scaled by node).
    pub fn area_size(&self, world: Mat4) -> (f32, f32) {
        match self.kind {
            LightKind::Area { width, height } => {
                let sx = world.x_axis.truncate().length();
                let sy = world.y_axis.truncate().length();
                (width * sx, height * sy)
            }
            _ => (0.0, 0.0),
        }
    }
}
