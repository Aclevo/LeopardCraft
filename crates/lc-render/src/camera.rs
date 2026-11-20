//! Ray generation for perspective, orthographic and panoramic cameras.

use glam::{Mat4, Vec2, Vec3};
use lc_core::{sample_disk, Pcg32, Ray};
use lc_scene::{Camera, ProjectionMode};

#[derive(Clone, Debug)]
pub struct RenderCamera {
    pub world: Mat4,
    pub camera: Camera,
}

impl RenderCamera {
    pub fn new(world: Mat4, camera: Camera) -> Self {
        Self { world, camera }
    }

    pub fn eye(&self) -> Vec3 {
        self.world.w_axis.truncate()
    }

    pub fn forward(&self) -> Vec3 {
        -self.world.z_axis.truncate().normalize_or(Vec3::Z)
    }

    pub fn right(&self) -> Vec3 {
        self.world.x_axis.truncate().normalize_or(Vec3::X)
    }

    pub fn up(&self) -> Vec3 {
        self.world.y_axis.truncate().normalize_or(Vec3::Y)
    }

    pub fn aspect(&self) -> f32 {
        // Aspect is supplied by the film size in ray generation.
        1.0
    }

    /// Generate a primary ray. `pixel` is in continuous pixel coordinates
    /// (0..width, 0..height), already optionally jittered by the caller.
    pub fn ray(&self, pixel: Vec2, resolution: Vec2, rng: &mut Pcg32) -> Ray {
        let w = resolution.x.max(1.0);
        let h = resolution.y.max(1.0);
        let aspect = w / h;
        let ndc = Vec2::new((pixel.x / w) * 2.0 - 1.0, 1.0 - (pixel.y / h) * 2.0);
        let eye = self.eye();
        let forward = self.forward();
        let right = self.right();
        let up = self.up();
        let mut origin = eye;
        let mut dir;
        match self.camera.projection {
            ProjectionMode::Orthographic => {
                let half_h = self.camera.ortho_scale.max(0.001) * 0.5;
                let half_w = half_h * aspect;
                origin += right * (ndc.x * half_w) + up * (ndc.y * half_h);
                dir = forward;
            }
            ProjectionMode::Panoramic => {
                let lon = ndc.x * std::f32::consts::PI;
                let lat = ndc.y * std::f32::consts::FRAC_PI_2;
                let (sin_lat, cos_lat) = lat.sin_cos();
                let (sin_lon, cos_lon) = lon.sin_cos();
                dir = (forward * (cos_lat * cos_lon) + right * (cos_lat * sin_lon) + up * sin_lat)
                    .normalize_or(forward);
            }
            ProjectionMode::Perspective => {
                let tan_half = (self.camera.fov_y_deg.clamp(1.0, 179.0).to_radians() * 0.5).tan();
                dir = (forward + right * (ndc.x * tan_half * aspect) + up * (ndc.y * tan_half))
                    .normalize_or(forward);
                let aperture = self.camera.aperture.max(0.0);
                if aperture > 1e-5 {
                    let focus = self.camera.focus_distance.max(self.camera.near);
                    let target = origin + dir * focus;
                    let lens = sample_disk(rng) * (aperture * 0.5);
                    origin += right * lens.x + up * lens.y;
                    dir = (target - origin).normalize_or(dir);
                }
            }
        }
        Ray::new(origin, dir)
    }

    /// Project a world position to continuous pixel coordinates (used by
    /// viewport helpers and picking overlays).
    pub fn project(&self, world_point: Vec3, resolution: Vec2) -> Option<Vec2> {
        let vp = self
            .camera
            .view_projection(self.world, resolution.x.max(1.0) / resolution.y.max(1.0));
        let clip = vp * world_point.extend(1.0);
        if clip.w.abs() < 1e-7 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        if ndc.z < 0.0 || ndc.z > 1.0 {
            return None;
        }
        Some(Vec2::new(
            (ndc.x * 0.5 + 0.5) * resolution.x,
            (0.5 - ndc.y * 0.5) * resolution.y,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lc_core::look_at_quat;
    use lc_core::Transform;

    fn camera() -> RenderCamera {
        let eye = Vec3::new(3.0, 2.0, 5.0);
        let t = Transform::new(eye, look_at_quat(eye, Vec3::ZERO, Vec3::Y), Vec3::ONE);
        RenderCamera::new(t.matrix(), Camera::default())
    }

    #[test]
    fn center_ray_points_at_target() {
        let cam = camera();
        let mut rng = Pcg32::new(1);
        let ray = cam.ray(Vec2::new(400.0, 300.0), Vec2::new(800.0, 600.0), &mut rng);
        let to_target = (Vec3::ZERO - cam.eye()).normalize();
        assert!(ray.dir.dot(to_target) > 0.999, "dir {:?}", ray.dir);
    }

    #[test]
    fn orthographic_rays_are_parallel() {
        let mut cam = camera();
        cam.camera.projection = ProjectionMode::Orthographic;
        let mut a = Pcg32::new(2);
        let mut b = Pcg32::new(3);
        let r1 = cam.ray(Vec2::new(10.0, 10.0), Vec2::new(100.0, 100.0), &mut a);
        let r2 = cam.ray(Vec2::new(90.0, 80.0), Vec2::new(100.0, 100.0), &mut b);
        assert!(r1.dir.dot(r2.dir) > 0.9999);
    }

    #[test]
    fn panoramic_rays_cover_sphere() {
        let mut cam = camera();
        cam.camera.projection = ProjectionMode::Panoramic;
        let mut rng = Pcg32::new(4);
        let mut dots = Vec::new();
        for x in 0..4 {
            for y in 0..4 {
                let r = cam.ray(
                    Vec2::new(x as f32 * 25.0 + 12.5, y as f32 * 25.0 + 12.5),
                    Vec2::new(100.0, 100.0),
                    &mut rng,
                );
                dots.push(r.dir.dot(cam.forward()));
            }
        }
        assert!(dots.iter().any(|d| *d > 0.5));
        assert!(dots.iter().any(|d| *d < -0.5));
    }
}
