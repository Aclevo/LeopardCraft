//! Camera parameters. Cameras look down their local -Z axis.

use glam::{Mat4, Vec3, Vec4};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionMode {
    #[default]
    Perspective,
    Orthographic,
    /// Equirectangular panorama for 360° renders.
    Panoramic,
}

impl ProjectionMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Perspective => "Perspective",
            Self::Orthographic => "Orthographic",
            Self::Panoramic => "Panoramic (360°)",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub fov_y_deg: f32,
    pub projection: ProjectionMode,
    pub ortho_scale: f32,
    pub near: f32,
    pub far: f32,
    /// Lens aperture in scene units; 0 disables depth of field.
    pub aperture: f32,
    /// Distance from the lens to the focus plane.
    pub focus_distance: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            fov_y_deg: 45.0,
            projection: ProjectionMode::Perspective,
            ortho_scale: 4.0,
            near: 0.05,
            far: 1000.0,
            aperture: 0.0,
            focus_distance: 8.0,
        }
    }
}

impl Camera {
    pub fn view_matrix(world: Mat4) -> Mat4 {
        world.inverse()
    }

    /// Right-handed projection with a 0..1 depth range (WebGPU/Vulkan style).
    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        let aspect = aspect.max(1e-4);
        let near = self.near.max(0.0001);
        let far = self.far.max(near + 0.001);
        match self.projection {
            ProjectionMode::Orthographic => {
                let h = self.ortho_scale.max(0.001) * 0.5;
                let w = h * aspect;
                Mat4::from_cols(
                    Vec4::new(1.0 / w, 0.0, 0.0, 0.0),
                    Vec4::new(0.0, 1.0 / h, 0.0, 0.0),
                    Vec4::new(0.0, 0.0, 1.0 / (near - far), 0.0),
                    Vec4::new(0.0, 0.0, near / (near - far), 1.0),
                )
            }
            _ => {
                let f = 1.0 / (self.fov_y_deg.clamp(1.0, 179.0).to_radians() * 0.5).tan();
                Mat4::from_cols(
                    Vec4::new(f / aspect, 0.0, 0.0, 0.0),
                    Vec4::new(0.0, f, 0.0, 0.0),
                    Vec4::new(0.0, 0.0, far / (near - far), -1.0),
                    Vec4::new(0.0, 0.0, near * far / (near - far), 0.0),
                )
            }
        }
    }

    pub fn view_projection(&self, world: Mat4, aspect: f32) -> Mat4 {
        self.projection_matrix(aspect) * Self::view_matrix(world)
    }

    pub fn eye(world: Mat4) -> Vec3 {
        world.w_axis.truncate()
    }

    /// Forward direction (-Z).
    pub fn forward(world: Mat4) -> Vec3 {
        -world.z_axis.truncate().normalize_or(Vec3::Z)
    }

    pub fn right(world: Mat4) -> Vec3 {
        world.x_axis.truncate().normalize_or(Vec3::X)
    }

    pub fn up(world: Mat4) -> Vec3 {
        world.y_axis.truncate().normalize_or(Vec3::Y)
    }
}
