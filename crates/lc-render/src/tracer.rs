//! Deterministic progressive path tracer.
//!
//! Each sample is seeded from (film seed, x, y, sample index), so results do
//! not depend on thread scheduling or on how many passes the caller splits the
//! work into. Rayon is only used to distribute pixels.

use crate::scene::{Hit, RenderScene, SceneLightKind};
use glam::{Vec2, Vec3};
use lc_core::{luminance, sample_cosine_hemisphere, tangent_to_world, Pcg32, Ray};
use lc_shading::Surface;
use rayon::prelude::*;

#[derive(Clone, Copy, Debug)]
struct BsdfSample {
    direction: Vec3,
    weight: Vec3,
}

#[derive(Clone, Debug)]
pub struct PathTracer {
    pub width: u32,
    pub height: u32,
    pub accum: Vec<Vec3>,
    pub coverage: Vec<f32>,
    pub samples: u32,
    pub seed: u64,
}

impl PathTracer {
    pub fn new(width: u32, height: u32, seed: u64) -> Self {
        let width = width.clamp(1, 8192);
        let height = height.clamp(1, 8192);
        Self {
            width,
            height,
            accum: vec![Vec3::ZERO; width as usize * height as usize],
            coverage: vec![0.0; width as usize * height as usize],
            samples: 0,
            seed,
        }
    }

    pub fn reset(&mut self, width: u32, height: u32, seed: u64) {
        *self = Self::new(width, height, seed);
    }

    pub fn clear(&mut self) {
        self.accum.fill(Vec3::ZERO);
        self.coverage.fill(0.0);
        self.samples = 0;
    }

    /// Add `samples_per_pixel` more samples to the accumulation buffer.
    pub fn render_pass(&mut self, scene: &RenderScene, samples_per_pixel: u32) {
        let width = self.width;
        let height = self.height;
        if width == 0 || height == 0 || samples_per_pixel == 0 {
            return;
        }
        let spp = samples_per_pixel.min(4096);
        let sample_base = self.samples;
        let seed = self.seed;
        let resolution = Vec2::new(width as f32, height as f32);
        let max_bounces = scene.max_bounces;
        let accum = &mut self.accum;
        let coverage = &mut self.coverage;
        accum
            .par_chunks_mut(width as usize)
            .zip(coverage.par_chunks_mut(width as usize))
            .enumerate()
            .for_each(|(y, (row, coverage_row))| {
                for x in 0..width as usize {
                    let mut covered = 0.0;
                    for s in 0..spp {
                        let sample_index = sample_base + s;
                        let mut rng = Pcg32::from_parts(
                            hash_seed(seed, x as u32, y as u32, sample_index),
                            sample_index as u64 ^ (y as u64).rotate_left(21),
                        );
                        let jitter = Vec2::new(rng.next_f32(), rng.next_f32());
                        let ray = scene.camera.ray(
                            Vec2::new(x as f32 + jitter.x, y as f32 + jitter.y),
                            resolution,
                            &mut rng,
                        );
                        let (radiance, hit) = trace_path(scene, ray, max_bounces, &mut rng);
                        if radiance.is_finite() {
                            // Accumulate per sample, in sample order, so splitting
                            // a render into passes stays bit-identical to one pass.
                            row[x] += radiance.max(Vec3::ZERO).min(Vec3::splat(1e6));
                        }
                        covered += hit;
                    }
                    coverage_row[x] += covered;
                }
            });
        self.samples = self.samples.saturating_add(spp);
    }

    pub fn hdr_pixels(&self) -> Vec<Vec3> {
        if self.samples == 0 {
            return vec![Vec3::ZERO; self.accum.len()];
        }
        self.accum
            .iter()
            .map(|c| *c / self.samples as f32)
            .collect()
    }

    pub fn rgba8(&self, exposure: f32, transparent_background: bool) -> Vec<[u8; 4]> {
        let inv = if self.samples == 0 {
            0.0
        } else {
            1.0 / self.samples as f32
        };
        self.accum
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let color = *c * inv;
                let [r, g, b] = lc_core::to_srgb8(color, exposure);
                let a = if transparent_background {
                    (self.coverage[i] * inv * 255.0).clamp(0.0, 255.0) as u8
                } else {
                    255
                };
                [r, g, b, a]
            })
            .collect()
    }

    /// A small CPU preview independent of sample count (one sample path).
    pub fn quick_preview(
        scene: &RenderScene,
        width: u32,
        height: u32,
        exposure: f32,
    ) -> Vec<[u8; 4]> {
        let mut tracer = Self::new(width, height, scene_hash(scene));
        tracer.render_pass(scene, 1);
        tracer.rgba8(exposure, false)
    }
}

fn hash_seed(seed: u64, x: u32, y: u32, sample: u32) -> u64 {
    let mut h = seed
        .wrapping_add((x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
        .wrapping_add((y as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f))
        .wrapping_add((sample as u64).wrapping_mul(0x1656_67b1_9e37_79f9));
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^ (h >> 31)
}

fn scene_hash(scene: &RenderScene) -> u64 {
    scene.triangle_count() as u64 ^ scene.lights.len() as u64 ^ scene.time.to_bits() as u64
}

fn trace_path(scene: &RenderScene, mut ray: Ray, max_bounces: u32, rng: &mut Pcg32) -> (Vec3, f32) {
    let mut radiance = Vec3::ZERO;
    let mut throughput = Vec3::ONE;
    let mut hit_any = 0.0;
    for depth in 0..max_bounces.max(1) {
        let Some(hit) = scene.intersect(&ray, f32::INFINITY) else {
            radiance += throughput * scene.environment_radiance(ray.dir);
            break;
        };
        if depth == 0 {
            hit_any = 1.0;
        }
        let surface = scene.surface(&hit, -ray.dir);
        radiance += throughput * surface.emission;
        if surface.transmission < 0.5 {
            radiance += throughput * direct_light(scene, &hit, &surface, -ray.dir, rng);
        }
        let entering = ray.dir.dot(hit.geometric_normal) < 0.0;
        let Some(sample) = sample_bsdf(&surface, -ray.dir, entering, rng) else {
            break;
        };
        if sample.weight.max_element() <= 0.0 || !sample.weight.is_finite() {
            break;
        }
        throughput *= sample.weight;
        if !throughput.is_finite() {
            break;
        }
        let side = if sample.direction.dot(surface.normal) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let origin = hit.position + surface.normal * (2e-4 * side);
        ray = Ray::new(origin, sample.direction);
        if depth >= 2 {
            let p = luminance(throughput).clamp(0.05, 0.95);
            if rng.next_f32() > p {
                break;
            }
            throughput /= p;
        }
        if throughput.max_element() < 1e-4 {
            break;
        }
    }
    (radiance, hit_any)
}

/// Sample a point on the light's spherical shell for soft shadows. A zero or
/// non-finite radius returns the centre and consumes no randomness, so the
/// hard-shadow path stays byte-identical.
fn sample_light_position(center: Vec3, radius: f32, rng: &mut Pcg32) -> Vec3 {
    let radius = if radius.is_finite() {
        radius.max(0.0)
    } else {
        0.0
    };
    if radius <= 0.0 {
        return center;
    }
    let z = rng.next_f32() * 2.0 - 1.0;
    let phi = std::f32::consts::TAU * rng.next_f32();
    let ring = (1.0 - z * z).max(0.0).sqrt();
    center + Vec3::new(ring * phi.cos(), ring * phi.sin(), z) * radius
}

fn direct_light(
    scene: &RenderScene,
    hit: &Hit,
    surface: &Surface,
    wo: Vec3,
    rng: &mut Pcg32,
) -> Vec3 {
    if scene.lights.is_empty() {
        return Vec3::ZERO;
    }
    let idx = rng.next_usize(scene.lights.len());
    let light = &scene.lights[idx];
    let n = surface.normal;
    let shadow_eps = 2e-4;
    let count = scene.lights.len() as f32;
    match &light.kind {
        SceneLightKind::Directional { direction } => {
            let wi = *direction;
            let n_dot_l = n.dot(wi);
            if n_dot_l <= 0.0 {
                return Vec3::ZERO;
            }
            if light.casts_shadow {
                let ray = Ray::new(hit.position + n * shadow_eps, wi);
                if scene.occluded(&ray, f32::INFINITY) {
                    return Vec3::ZERO;
                }
            }
            let f = evaluate_bsdf(surface, wo, wi, n_dot_l);
            f * light.emission * n_dot_l / count
        }
        SceneLightKind::Point { position, radius } => {
            // A light with a radius is a sphere: sampling its shell widens the
            // penumbra instead of casting a hard-edged shadow.
            let to_light = sample_light_position(*position, *radius, rng) - hit.position;
            let dist2 = to_light.length_squared().max(1e-6);
            let wi = to_light / dist2.sqrt();
            let n_dot_l = n.dot(wi);
            if n_dot_l <= 0.0 {
                return Vec3::ZERO;
            }
            if light.casts_shadow {
                let ray = Ray::new(hit.position + n * shadow_eps, wi);
                if scene.occluded(&ray, dist2.sqrt() - shadow_eps) {
                    return Vec3::ZERO;
                }
            }
            let f = evaluate_bsdf(surface, wo, wi, n_dot_l);
            f * light.emission * (n_dot_l / dist2) / count
        }
        SceneLightKind::Spot {
            position,
            radius,
            direction,
            cos_inner,
            cos_outer,
        } => {
            let to_light = sample_light_position(*position, *radius, rng) - hit.position;
            let dist2 = to_light.length_squared().max(1e-6);
            let dist = dist2.sqrt();
            let wi = to_light / dist;
            let n_dot_l = n.dot(wi);
            if n_dot_l <= 0.0 {
                return Vec3::ZERO;
            }
            // The cone axis points away from the light, so the ray that reaches
            // this surface must travel along it.
            let cos_theta = (-wi).dot(*direction);
            if cos_theta <= *cos_outer {
                return Vec3::ZERO;
            }
            let falloff = if cos_theta >= *cos_inner {
                1.0
            } else {
                let span = (*cos_inner - *cos_outer).max(1e-5);
                ((cos_theta - *cos_outer) / span).clamp(0.0, 1.0)
            };
            if light.casts_shadow {
                let ray = Ray::new(hit.position + n * shadow_eps, wi);
                if scene.occluded(&ray, dist - shadow_eps) {
                    return Vec3::ZERO;
                }
            }
            let f = evaluate_bsdf(surface, wo, wi, n_dot_l);
            f * light.emission * (n_dot_l / dist2) * falloff / count
        }
        SceneLightKind::Area {
            center,
            u,
            v,
            normal,
        } => {
            let a = rng.next_f32();
            let b = rng.next_f32();
            let point = *center + *u * (a - 0.5) + *v * (b - 0.5);
            let to_light = point - hit.position;
            let dist2 = to_light.length_squared().max(1e-6);
            let wi = to_light / dist2.sqrt();
            let n_dot_l = n.dot(wi);
            if n_dot_l <= 0.0 {
                return Vec3::ZERO;
            }
            let cos_light = normal.dot(-wi).abs();
            if cos_light <= 1e-4 {
                return Vec3::ZERO;
            }
            if light.casts_shadow {
                let ray = Ray::new(hit.position + n * shadow_eps, wi);
                if scene.occluded(&ray, dist2.sqrt() - shadow_eps) {
                    return Vec3::ZERO;
                }
            }
            let area = light.area();
            let f = evaluate_bsdf(surface, wo, wi, n_dot_l);
            f * light.emission * (n_dot_l * cos_light * area / dist2) / count
        }
    }
}

fn f0(surface: &Surface) -> Vec3 {
    Vec3::splat(0.04).lerp(surface.base_color, surface.metallic)
}

fn fresnel_schlick(f0: Vec3, cos_theta: f32) -> Vec3 {
    f0 + (Vec3::ONE - f0) * ((1.0 - cos_theta).clamp(0.0, 1.0)).powi(5)
}

fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = (roughness * roughness).clamp(0.001, 1.0);
    let a2 = a * a;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    a2 / (std::f32::consts::PI * d * d).max(1e-7)
}

fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let a = (roughness * roughness).clamp(0.001, 1.0);
    let a2 = a * a;
    let gv = n_dot_v / (n_dot_v + (a2 + (1.0 - a2) * n_dot_v * n_dot_v).sqrt());
    let gl = n_dot_l / (n_dot_l + (a2 + (1.0 - a2) * n_dot_l * n_dot_l).sqrt());
    gv * gl
}

fn evaluate_bsdf(surface: &Surface, wo: Vec3, wi: Vec3, n_dot_l: f32) -> Vec3 {
    let n = surface.normal;
    let n_dot_v = n.dot(wo).max(1e-4);
    if n_dot_l <= 0.0 || n_dot_v <= 0.0 {
        return Vec3::ZERO;
    }
    let h = (wo + wi).normalize_or(n);
    let n_dot_h = n.dot(h).max(0.0);
    let v_dot_h = wo.dot(h).max(0.0);
    let f = fresnel_schlick(f0(surface), v_dot_h);
    let d = distribution_ggx(n_dot_h, surface.roughness);
    let g = geometry_smith(n_dot_v, n_dot_l, surface.roughness);
    let spec = f * (d * g / (4.0 * n_dot_v * n_dot_l).max(1e-5));
    let diffuse_weight = (1.0 - surface.metallic) * (1.0 - surface.transmission);
    let diffuse = surface.base_color * diffuse_weight / std::f32::consts::PI;
    diffuse + spec
}

fn sample_bsdf(surface: &Surface, wo: Vec3, entering: bool, rng: &mut Pcg32) -> Option<BsdfSample> {
    let n = surface.normal.normalize_or(Vec3::Y);
    let ior = surface.ior.max(1.0001);
    if surface.transmission > 0.5 {
        // `n` faces the incoming side. `entering` decides the IOR ratio and
        // total internal reflection only happens on the way out.
        let cos_i = (-wo).dot(n).clamp(1e-4, 1.0);
        let eta = if entering { 1.0 / ior } else { ior };
        let sin2_t = eta * eta * (1.0 - cos_i * cos_i);
        let fresnel = fresnel_dielectric(cos_i, entering, ior);
        if sin2_t >= 1.0 || rng.next_f32() < fresnel {
            let wi = reflect(wo, n);
            return Some(BsdfSample {
                direction: wi,
                weight: Vec3::ONE,
            });
        }
        let cos_t = (1.0 - sin2_t).max(0.0).sqrt();
        let wi = (wo * eta + n * (eta * cos_i - cos_t)).normalize_or(n);
        return Some(BsdfSample {
            direction: wi,
            weight: surface.base_color,
        });
    }

    let f0 = f0(surface);
    let spec_prob = (f0.x.max(f0.y).max(f0.z) * 2.0 + surface.metallic * 0.5).clamp(0.05, 0.9);
    if rng.next_f32() < spec_prob {
        let wi = match sample_ggx_direction(n, wo, surface.roughness, rng) {
            Some(w) => w,
            None => tangent_to_world(sample_cosine_hemisphere(rng), n),
        };
        let n_dot_l = n.dot(wi).max(0.0);
        if n_dot_l <= 0.0 {
            return None;
        }
        let f = evaluate_bsdf(surface, wo, wi, n_dot_l);
        let n_dot_h = n.dot((wo + wi).normalize_or(n)).clamp(0.0, 1.0);
        let v_dot_h = wo.dot((wo + wi).normalize_or(n)).clamp(0.0, 1.0);
        let pdf =
            distribution_ggx(n_dot_h, surface.roughness) * n_dot_h / (4.0 * v_dot_h).max(1e-5);
        if pdf <= 1e-6 {
            return None;
        }
        Some(BsdfSample {
            direction: wi,
            weight: f * n_dot_l / (pdf * spec_prob),
        })
    } else {
        let wi = tangent_to_world(sample_cosine_hemisphere(rng), n);
        let n_dot_l = n.dot(wi).max(0.0);
        if n_dot_l <= 0.0 {
            return None;
        }
        let diffuse_weight = (1.0 - surface.metallic) * (1.0 - surface.transmission);
        let weight = surface.base_color * diffuse_weight / (1.0 - spec_prob);
        Some(BsdfSample {
            direction: wi,
            weight,
        })
    }
}

fn reflect(v: Vec3, n: Vec3) -> Vec3 {
    (v - n * (2.0 * v.dot(n))).normalize_or(n)
}

fn fresnel_dielectric(cos_i: f32, entering: bool, ior: f32) -> f32 {
    let eta_i = if entering { 1.0 } else { ior };
    let eta_t = if entering { ior } else { 1.0 };
    let sin_t = eta_i / eta_t * (1.0 - cos_i * cos_i).max(0.0).sqrt();
    if sin_t >= 1.0 {
        return 1.0;
    }
    let cos_t = (1.0 - sin_t * sin_t).max(0.0).sqrt();
    let rs = (eta_i * cos_i - eta_t * cos_t) / (eta_i * cos_i + eta_t * cos_t).max(1e-5);
    let rp = (eta_t * cos_i - eta_i * cos_t) / (eta_t * cos_i + eta_i * cos_t).max(1e-5);
    ((rs * rs + rp * rp) * 0.5).clamp(0.0, 1.0)
}

fn sample_ggx_direction(n: Vec3, wo: Vec3, roughness: f32, rng: &mut Pcg32) -> Option<Vec3> {
    let alpha = (roughness * roughness).clamp(0.001, 1.0);
    let u1 = rng.next_f32();
    let u2 = rng.next_f32();
    let phi = std::f32::consts::TAU * u2;
    let cos_theta = ((1.0 - u1) / (1.0 + (alpha * alpha - 1.0) * u1))
        .max(0.0)
        .sqrt();
    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
    let (t, b) = lc_core::orthonormal_basis(n);
    let h =
        (t * (sin_theta * phi.cos()) + b * (sin_theta * phi.sin()) + n * cos_theta).normalize_or(n);
    let wi = reflect(wo, h);
    if wi.dot(n) <= 0.0 || !wi.is_finite() {
        None
    } else {
        Some(wi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lc_scene::Document;

    #[test]
    fn renders_are_deterministic_and_illuminated() {
        let doc = Document::demo();
        let scene = RenderScene::from_document(&doc, 0.0, ".").unwrap();
        let mut a = PathTracer::new(32, 20, 42);
        let mut b = PathTracer::new(32, 20, 42);
        a.render_pass(&scene, 4);
        b.render_pass(&scene, 4);
        assert_eq!(a.accum, b.accum);
        let pixels = a.hdr_pixels();
        let total: f32 = pixels.iter().map(|p| luminance(*p)).sum();
        assert!(total > 1.0, "render is black (total {total})");
    }

    #[test]
    fn progressive_accumulation_matches_single_pass() {
        let doc = Document::demo();
        let scene = RenderScene::from_document(&doc, 0.0, ".").unwrap();
        let mut a = PathTracer::new(24, 16, 7);
        let mut b = PathTracer::new(24, 16, 7);
        a.render_pass(&scene, 2);
        a.render_pass(&scene, 2);
        b.render_pass(&scene, 4);
        assert_eq!(a.samples, 4);
        let pa = a.hdr_pixels();
        let pb = b.hdr_pixels();
        for (x, y) in pa.iter().zip(&pb) {
            assert!((*x - *y).length() < 1e-4, "{x:?} vs {y:?}");
        }
    }

    #[test]
    fn alpha_and_tonemap_are_bounded() {
        let doc = Document::demo();
        let scene = RenderScene::from_document(&doc, 0.0, ".").unwrap();
        let mut tracer = PathTracer::new(16, 12, 3);
        tracer.render_pass(&scene, 1);
        let rgba = tracer.rgba8(0.0, true);
        assert_eq!(rgba.len(), 16 * 12);
        assert_eq!(rgba.len(), 16 * 12);
    }

    #[test]
    fn light_radius_creates_a_deterministic_penumbra_and_stays_finite() {
        use crate::camera::RenderCamera;
        use crate::scene::{SceneLight, Triangle};
        use lc_geom::Bvh;
        use lc_scene::Camera;
        use lc_shading::{MaterialLibrary, TextureCache};

        fn scene_with_radius(radius: f32) -> RenderScene {
            // A small blocker one metre above the shaded point. A radius-zero
            // point light is fully blocked; sampling a spherical shell lets
            // light around it, and a larger radius lets more through.
            let blocker = [
                Triangle {
                    p: [
                        Vec3::new(-0.1, 1.0, -0.1),
                        Vec3::new(0.1, 1.0, -0.1),
                        Vec3::new(0.1, 1.0, 0.1),
                    ],
                    n: [Vec3::Y; 3],
                    uv: [Vec2::ZERO; 3],
                    extra_uv: Vec::new(),
                    tangent: Vec3::X,
                    material: 0,
                },
                Triangle {
                    p: [
                        Vec3::new(-0.1, 1.0, -0.1),
                        Vec3::new(0.1, 1.0, 0.1),
                        Vec3::new(-0.1, 1.0, 0.1),
                    ],
                    n: [Vec3::Y; 3],
                    uv: [Vec2::ZERO; 3],
                    extra_uv: Vec::new(),
                    tangent: Vec3::X,
                    material: 0,
                },
            ];
            RenderScene {
                bvh: Bvh::build(&blocker),
                triangles: blocker.to_vec(),
                materials: MaterialLibrary::default(),
                textures: std::sync::Arc::new(TextureCache::default()),
                lights: vec![SceneLight {
                    kind: SceneLightKind::Point {
                        position: Vec3::new(0.0, 2.0, 0.0),
                        radius,
                    },
                    emission: Vec3::ONE,
                    casts_shadow: true,
                }],
                environment: lc_scene::Environment::default(),
                background: Vec3::ZERO,
                camera: RenderCamera::new(glam::Mat4::IDENTITY, Camera::default()),
                time: 0.0,
                bounds: lc_core::Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0)),
                max_bounces: 1,
                exposure: 0.0,
                transparent_background: false,
            }
        }

        fn shadowed_fraction(radius: f32, samples: usize) -> f32 {
            let scene = scene_with_radius(radius);
            let hit = Hit {
                t: 0.0,
                position: Vec3::ZERO,
                uv: Vec2::ZERO,
                extra_uvs: Vec::new(),
                normal: Vec3::Y,
                geometric_normal: Vec3::Y,
                tangent: Vec3::X,
                material: 0,
            };
            let surface = Surface {
                normal: Vec3::Y,
                ..Default::default()
            };
            let mut shadowed = 0usize;
            for sample in 0..samples {
                let mut rng = Pcg32::new(sample as u64 + 17);
                let value = direct_light(&scene, &hit, &surface, Vec3::Y, &mut rng);
                assert!(value.is_finite(), "radius {radius} produced {value}");
                if value.max_element() <= 0.0 {
                    shadowed += 1;
                }
            }
            shadowed as f32 / samples as f32
        }

        // The shaded point sits in the hard-shadow umbra. Sampling a spherical
        // shell lets light around the blocker; a wider radius leaves a smaller
        // fully-shadowed core, which is a wider penumbra.
        let hard = shadowed_fraction(0.0, 256);
        let small = shadowed_fraction(0.5, 256);
        let large = shadowed_fraction(2.0, 256);
        assert!(hard > 0.99, "a radius-zero light is fully blocked: {hard}");
        assert!(
            small < hard,
            "a small radius must light part of the umbra: {small} vs {hard}"
        );
        assert!(
            large < small,
            "a larger radius must widen the penumbra: {small} vs {large}"
        );
        for radius in [f32::NAN, f32::INFINITY, -1.0] {
            let value = shadowed_fraction(radius, 8);
            assert!(value.is_finite(), "radius {radius} produced {value}");
        }
    }
}
