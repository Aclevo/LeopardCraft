//! Render-ready scene: world-space triangles, BVH, lights, environment and
//! material access.

use crate::camera::RenderCamera;
use glam::{Mat3, Vec2, Vec3};
use lc_core::{orthonormal_basis, ray_triangle, Aabb, Ray};
use lc_geom::{Bvh, BvhObject};
use lc_scene::{Document, Environment, EvaluatedScene, LightKind, ProjectionMode};
use lc_shading::{Material, MaterialLibrary, ShadingContext, Surface, TextureCache};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Triangle {
    pub p: [Vec3; 3],
    pub n: [Vec3; 3],
    pub uv: [Vec2; 3],
    /// Additional UV sets (UV1, …) at the same three corners.
    pub extra_uv: Vec<[Vec2; 3]>,
    pub tangent: Vec3,
    pub material: u32,
}

impl Triangle {
    pub fn geometric_normal(&self) -> Vec3 {
        (self.p[1] - self.p[0])
            .cross(self.p[2] - self.p[0])
            .normalize_or(Vec3::Y)
    }
}

impl BvhObject for Triangle {
    fn bounds(&self) -> Aabb {
        Aabb::from_points(self.p)
    }

    fn centroid(&self) -> Vec3 {
        (self.p[0] + self.p[1] + self.p[2]) / 3.0
    }

    fn ray_hit(&self, ray: &Ray) -> Option<(f32, f32, f32)> {
        ray_triangle(ray, self.p[0], self.p[1], self.p[2])
    }
}

#[derive(Clone, Debug)]
pub enum SceneLightKind {
    /// Direction points *toward* the light.
    Directional {
        direction: Vec3,
    },
    Point {
        position: Vec3,
        radius: f32,
    },
    Area {
        center: Vec3,
        /// Full width/height edge vectors.
        u: Vec3,
        v: Vec3,
        normal: Vec3,
    },
    Spot {
        position: Vec3,
        radius: f32,
        /// Cone axis, pointing away from the light.
        direction: Vec3,
        /// Cosine of the full-intensity cone angle (the larger of the two).
        cos_inner: f32,
        /// Cosine of the cone edge angle (the smaller of the two).
        cos_outer: f32,
    },
}

#[derive(Clone, Debug)]
pub struct SceneLight {
    pub kind: SceneLightKind,
    pub emission: Vec3,
    pub casts_shadow: bool,
}

impl SceneLight {
    pub fn area(&self) -> f32 {
        match &self.kind {
            SceneLightKind::Area { u, v, .. } => u.cross(*v).length(),
            _ => 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RenderScene {
    pub triangles: Vec<Triangle>,
    pub bvh: Bvh,
    pub materials: MaterialLibrary,
    pub textures: Arc<TextureCache>,
    pub lights: Vec<SceneLight>,
    pub environment: Environment,
    pub background: Vec3,
    pub camera: RenderCamera,
    pub time: f32,
    pub bounds: Aabb,
    pub max_bounces: u32,
    pub exposure: f32,
    pub transparent_background: bool,
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub t: f32,
    pub position: Vec3,
    pub uv: Vec2,
    /// Interpolated additional UV sets, starting at UV1.
    pub extra_uvs: Vec<Vec2>,
    /// Interpolated, outward facing normal.
    pub normal: Vec3,
    pub geometric_normal: Vec3,
    pub tangent: Vec3,
    pub material: u32,
}

impl RenderScene {
    pub fn from_document(
        doc: &Document,
        time: f32,
        base_dir: impl Into<PathBuf>,
    ) -> Result<Self, String> {
        let evaluated = doc.evaluate(time);
        let camera = evaluated.active_camera.as_ref().ok_or_else(|| {
            "scene has no active camera; add a camera or set one as active".to_string()
        })?;
        let mut textures = TextureCache::new(base_dir);
        doc.materials.preload_textures(&mut textures);
        if let Environment::Hdri { path, .. } = &evaluated.environment {
            textures.ensure(path);
        }
        let render_settings = evaluated.settings.clone();
        let mut triangles = Vec::new();
        for node in evaluated.mesh_nodes() {
            let Some(mesh) = node.mesh() else {
                continue;
            };
            let normal_matrix = Mat3::from_mat4(node.world).inverse().transpose();
            let world_dir = Mat3::from_mat4(node.world);
            for tri in mesh.render_triangles(40.0) {
                let p = tri.v.map(|v| node.world.transform_point3(v.position));
                let mut n = tri
                    .v
                    .map(|v| (normal_matrix * v.normal).normalize_or(Vec3::Y));
                let uv = tri.v.map(|v| v.uv);
                let extra_uv = (1..mesh.uv_set_count())
                    .map(|set| {
                        [0usize, 1, 2].map(|k| {
                            let corner = &mesh.polygons[tri.polygon as usize].corners
                                [tri.v[k].corner as usize];
                            mesh.uv_of_set(corner, set)
                        })
                    })
                    .collect();
                let mut tangent = triangle_tangent(p, uv);
                tangent = (world_dir * tangent).normalize_or(orthonormal_basis(n[0]).0);
                if !n.iter().all(|v| v.length_squared() > 1e-12) {
                    n = [Vec3::Y; 3];
                }
                let material = node.material_for_slot(tri.material_slot).unwrap_or(0);
                triangles.push(Triangle {
                    p,
                    n,
                    uv,
                    extra_uv,
                    tangent,
                    material,
                });
            }
        }
        let bvh = Bvh::build(&triangles);
        let mut lights = Vec::new();
        for light in &evaluated.lights {
            let emission = light.light.color.max(Vec3::ZERO) * light.light.intensity.max(0.0);
            if emission.max_element() <= 0.0 {
                continue;
            }
            let kind = match light.light.kind {
                LightKind::Directional => SceneLightKind::Directional {
                    direction: light.world.z_axis.truncate().normalize_or(Vec3::Y),
                },
                LightKind::Point { radius } => SceneLightKind::Point {
                    position: light.world.w_axis.truncate(),
                    radius,
                },
                LightKind::Spot {
                    radius,
                    inner_deg,
                    outer_deg,
                } => {
                    let inner = inner_deg.clamp(0.0, 90.0);
                    let outer = outer_deg.clamp(0.0, 90.0).max(inner);
                    SceneLightKind::Spot {
                        position: light.world.w_axis.truncate(),
                        radius,
                        direction: -light.world.z_axis.truncate().normalize_or(Vec3::Z),
                        cos_inner: inner.to_radians().cos(),
                        cos_outer: outer.to_radians().cos(),
                    }
                }
                LightKind::Area { width, height } => {
                    let u = light.world.x_axis.truncate() * width;
                    let v = light.world.y_axis.truncate() * height;
                    SceneLightKind::Area {
                        center: light.world.w_axis.truncate(),
                        u,
                        v,
                        normal: -light.world.z_axis.truncate().normalize_or(Vec3::Z),
                    }
                }
            };
            lights.push(SceneLight {
                kind,
                emission,
                casts_shadow: light.casts_shadow,
            });
        }
        if let Some((direction, color, intensity)) = evaluated.environment.sun() {
            let toward_sun = -direction.normalize_or(Vec3::Y);
            lights.push(SceneLight {
                kind: SceneLightKind::Directional {
                    direction: toward_sun,
                },
                emission: color.max(Vec3::ZERO) * intensity.max(0.0),
                casts_shadow: true,
            });
        }
        let bounds = if evaluated.bounds.is_valid() {
            evaluated.bounds
        } else {
            Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0))
        };
        Ok(Self {
            triangles,
            bvh,
            materials: doc.materials.clone(),
            textures: Arc::new(textures),
            lights,
            environment: evaluated.environment.clone(),
            background: evaluated.background,
            camera: RenderCamera {
                world: camera.world,
                camera: camera.camera.clone(),
            },
            time,
            bounds,
            max_bounces: render_settings.max_bounces.clamp(1, 32),
            exposure: render_settings.exposure,
            transparent_background: render_settings.transparent_background,
        })
    }

    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    pub fn intersect(&self, ray: &Ray, max_t: f32) -> Option<Hit> {
        let hit = self.bvh.nearest(&self.triangles, ray, max_t)?;
        let tri = &self.triangles[hit.prim as usize];
        let w = 1.0 - hit.u - hit.v;
        let position = ray.at(hit.t);
        let normal = (tri.n[0] * w + tri.n[1] * hit.u + tri.n[2] * hit.v)
            .normalize_or(tri.geometric_normal());
        let uv = tri.uv[0] * w + tri.uv[1] * hit.u + tri.uv[2] * hit.v;
        let extra_uvs = tri
            .extra_uv
            .iter()
            .map(|set| set[0] * w + set[1] * hit.u + set[2] * hit.v)
            .collect();
        Some(Hit {
            t: hit.t,
            position,
            uv,
            extra_uvs,
            normal,
            geometric_normal: tri.geometric_normal(),
            tangent: tri.tangent,
            material: tri.material,
        })
    }

    pub fn occluded(&self, ray: &Ray, max_t: f32) -> bool {
        self.bvh.any_hit(&self.triangles, ray, max_t)
    }

    pub fn material(&self, id: u32) -> Option<&Material> {
        self.materials
            .get(id)
            .or_else(|| self.materials.materials.values().next())
    }

    pub fn surface(&self, hit: &Hit, view_dir: Vec3) -> Surface {
        let faceforward = if hit.normal.dot(view_dir) > 0.0 {
            -hit.normal
        } else {
            hit.normal
        };
        let fallback = Surface {
            normal: faceforward,
            ..Default::default()
        };
        let Some(material) = self.material(hit.material) else {
            return fallback;
        };
        let tangent = if hit.tangent.length_squared() > 1e-10 {
            (hit.tangent - faceforward * hit.tangent.dot(faceforward))
                .normalize_or(lc_core::orthonormal_basis(faceforward).0)
        } else {
            lc_core::orthonormal_basis(faceforward).0
        };
        let ctx = ShadingContext {
            uv: hit.uv,
            extra_uvs: &hit.extra_uvs,
            position: hit.position,
            normal: faceforward,
            tangent,
            view_dir,
            time: self.time,
            textures: self.textures.as_ref(),
        };
        let mut surface = material.evaluate(&ctx);
        if !surface.normal.is_finite() || surface.normal.length_squared() < 1e-12 {
            surface.normal = faceforward;
        }
        if surface.normal.dot(faceforward) < 0.0 {
            surface.normal = -surface.normal;
        }
        if !surface.base_color.is_finite() {
            surface.base_color = Vec3::splat(0.8);
        }
        surface.base_color = surface.base_color.clamp(Vec3::ZERO, Vec3::splat(10.0));
        surface
    }

    pub fn environment_radiance(&self, direction: Vec3) -> Vec3 {
        match &self.environment {
            Environment::Solid { color } => *color,
            Environment::Hdri {
                path,
                intensity,
                rotation_deg,
            } => {
                if let Some(tex) = self.textures.get(path) {
                    let (sin_r, cos_r) = rotation_deg.to_radians().sin_cos();
                    let d = Vec3::new(
                        direction.x * cos_r - direction.z * sin_r,
                        direction.y,
                        direction.x * sin_r + direction.z * cos_r,
                    );
                    let u = d.z.atan2(d.x) / std::f32::consts::TAU + 0.5;
                    let v = d.y.clamp(-1.0, 1.0).acos() / std::f32::consts::PI;
                    tex.sample(Vec2::new(u, v)) * *intensity
                } else {
                    Vec3::splat(0.2)
                }
            }
            Environment::Sky {
                top,
                horizon,
                ground,
                ..
            } => {
                let y = direction.y;
                if y >= 0.0 {
                    let t = (y / 0.6).clamp(0.0, 1.0);
                    let t = t * t * (3.0 - 2.0 * t);
                    horizon.lerp(*top, t)
                } else {
                    let t = (-y / 0.6).clamp(0.0, 1.0);
                    let t = t * t * (3.0 - 2.0 * t);
                    horizon.lerp(*ground, t)
                }
            }
        }
    }
}

fn triangle_tangent(p: [Vec3; 3], uv: [Vec2; 3]) -> Vec3 {
    let edge1 = p[1] - p[0];
    let edge2 = p[2] - p[0];
    let duv1 = uv[1] - uv[0];
    let duv2 = uv[2] - uv[0];
    let det = duv1.x * duv2.y - duv1.y * duv2.x;
    if det.abs() < 1e-12 {
        orthonormal_basis((p[1] - p[0]).cross(p[2] - p[0]).normalize_or(Vec3::Y)).0
    } else {
        let t = (edge1 * duv2.y - edge2 * duv1.y) / det;
        t.normalize_or(Vec3::X)
    }
}

/// Camera parameters selected by evaluation, useful for UI overlays.
pub fn camera_projection_label(scene: &EvaluatedScene) -> &'static str {
    scene
        .active_camera
        .as_ref()
        .map(|c| c.camera.projection.label())
        .unwrap_or(ProjectionMode::Perspective.label())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lc_core::Pcg32;

    #[test]
    fn demo_builds_triangles_and_lights() {
        let doc = Document::demo();
        let scene = RenderScene::from_document(&doc, 0.0, ".").unwrap();
        assert!(scene.triangle_count() > 100);
        assert!(!scene.lights.is_empty());
        assert!(scene.bounds.is_valid());
        assert!(scene.materials.len() >= 6);
    }

    #[test]
    fn rays_intersect_scene_and_sky() {
        let doc = Document::demo();
        let scene = RenderScene::from_document(&doc, 0.0, ".").unwrap();
        let mut rng = Pcg32::new(9);
        let w = 320.0;
        let h = 200.0;
        let mut hits = 0;
        for y in 0..20 {
            for x in 0..32 {
                let ray = scene.camera.ray(
                    Vec2::new(x as f32 * 10.0 + 5.0, y as f32 * 10.0 + 5.0),
                    Vec2::new(w, h),
                    &mut rng,
                );
                if scene.intersect(&ray, f32::INFINITY).is_some() {
                    hits += 1;
                }
            }
        }
        assert!(hits > 0, "camera sees no geometry");
        assert!(scene
            .environment_radiance(Vec3::new(0.0, 1.0, 0.0))
            .is_finite());
    }

    #[test]
    fn hits_carry_interpolated_additional_uv_sets() {
        use crate::camera::RenderCamera;
        use lc_geom::Bvh;
        use lc_scene::Camera;
        use lc_shading::{MaterialLibrary, TextureCache};
        use std::sync::Arc;
        let tri = Triangle {
            p: [
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            n: [Vec3::Z; 3],
            uv: [Vec2::ZERO; 3],
            extra_uv: vec![[
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.0, 1.0),
            ]],
            tangent: Vec3::X,
            material: 0,
        };
        let scene = RenderScene {
            bvh: Bvh::build(std::slice::from_ref(&tri)),
            triangles: vec![tri],
            materials: MaterialLibrary::default(),
            textures: Arc::new(TextureCache::default()),
            lights: Vec::new(),
            environment: lc_scene::Environment::default(),
            background: Vec3::ZERO,
            camera: RenderCamera::new(glam::Mat4::IDENTITY, Camera::default()),
            time: 0.0,
            bounds: lc_core::Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0)),
            max_bounces: 1,
            exposure: 0.0,
            transparent_background: false,
        };
        let ray = lc_core::Ray::new(Vec3::new(0.25, 0.25, 1.0), -Vec3::Z);
        let hit = scene.intersect(&ray, f32::INFINITY).unwrap();
        assert_eq!(hit.extra_uvs.len(), 1);
        assert!((hit.extra_uvs[0] - Vec2::new(0.25, 0.25)).length() < 1e-5);
    }

    #[test]
    fn surface_evaluation_is_finite() {
        let doc = Document::demo();
        let scene = RenderScene::from_document(&doc, 0.0, ".").unwrap();
        let mut rng = Pcg32::new(11);
        for _ in 0..50 {
            let ray = scene.camera.ray(
                Vec2::new(rng.range(0.0, 640.0), rng.range(0.0, 360.0)),
                Vec2::new(640.0, 360.0),
                &mut rng,
            );
            if let Some(hit) = scene.intersect(&ray, f32::INFINITY) {
                let s = scene.surface(&hit, -ray.dir);
                assert!(s.base_color.is_finite());
                assert!(s.roughness.is_finite() && s.ior.is_finite());
                assert!(s.normal.is_finite());
            }
        }
    }
}
