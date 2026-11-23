//! Shared math, color, random and hashing primitives for LeopardCraft.
//!
//! Everything in this crate is deterministic and headless: no windowing, no
//! GPU, no clock access. That keeps scene evaluation reproducible.

use glam::{Mat3, Mat4, Quat, Vec2, Vec3, Vec4};
use serde::{Deserialize, Serialize};

/// Rigid-ish local transform: translation, rotation, scale.
///
/// A node's world matrix is `parent_world * local.matrix()`, so non-uniform
/// parent scale is inherited, matching common modelling expectations.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    pub const fn from_translation(translation: Vec3) -> Self {
        Self {
            translation,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }

    pub fn from_rotation_y(radians: f32) -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::from_rotation_y(radians),
            scale: Vec3::ONE,
        }
    }

    pub fn from_scale(scale: Vec3) -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale,
        }
    }

    pub fn new(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self {
            translation,
            rotation,
            scale,
        }
    }

    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    pub fn inverse_matrix(&self) -> Mat4 {
        self.matrix().inverse()
    }

    pub fn transform_point(&self, point: Vec3) -> Vec3 {
        self.matrix().transform_point3(point)
    }

    pub fn transform_vector(&self, vector: Vec3) -> Vec3 {
        self.rotation * (vector * self.scale)
    }

    /// Compose `self` with a child local transform.
    pub fn compose(&self, child: &Transform) -> Transform {
        let parent = self.matrix();
        let local = child.matrix();
        let m = parent * local;
        let (scale, rotation, translation) = m.to_scale_rotation_translation();
        Transform {
            translation,
            rotation,
            scale,
        }
    }

    pub fn lerp(&self, other: &Transform, t: f32) -> Transform {
        Transform {
            translation: self.translation.lerp(other.translation, t),
            rotation: self.rotation.slerp(other.rotation, t),
            scale: self.scale.lerp(other.scale, t),
        }
    }

    /// Euler angles in degrees (XYZ order), useful for the inspector and
    /// for Euler-based animation channels. This is the inverse of
    /// [`Self::from_euler_degrees`]; glam's `to_euler` returns radians, so the
    /// components are converted here.
    pub fn euler_degrees(&self) -> Vec3 {
        let radians: Vec3 = self.rotation.to_euler(glam::EulerRot::XYZ).into();
        Vec3::from_array(radians.to_array().map(f32::to_degrees))
    }

    pub fn from_euler_degrees(euler: Vec3) -> Self {
        let r = glam::EulerRot::XYZ;
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::from_euler(
                r,
                euler.x.to_radians(),
                euler.y.to_radians(),
                euler.z.to_radians(),
            ),
            scale: Vec3::ONE,
        }
    }

    pub fn normal_matrix(&self) -> Mat3 {
        Mat3::from_mat4(self.matrix()).inverse().transpose()
    }
}

/// Axis-aligned bounding box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub const EMPTY: Self = Self {
        min: Vec3::splat(f32::INFINITY),
        max: Vec3::splat(f32::NEG_INFINITY),
    };

    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    pub fn from_point(p: Vec3) -> Self {
        Self { min: p, max: p }
    }

    pub fn from_points(points: impl IntoIterator<Item = Vec3>) -> Self {
        let mut b = Self::EMPTY;
        for p in points {
            b = b.expand(p);
        }
        b
    }

    pub fn is_valid(&self) -> bool {
        self.min.x <= self.max.x && self.min.y <= self.max.y && self.min.z <= self.max.z
    }

    pub fn expand(&self, p: Vec3) -> Self {
        Self {
            min: self.min.min(p),
            max: self.max.max(p),
        }
    }

    pub fn union(&self, other: &Aabb) -> Self {
        if !self.is_valid() {
            return *other;
        }
        if !other.is_valid() {
            return *self;
        }
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn radius(&self) -> f32 {
        if self.is_valid() {
            self.size().length() * 0.5
        } else {
            0.0
        }
    }

    pub fn expanded(&self, amount: f32) -> Self {
        Self {
            min: self.min - Vec3::splat(amount),
            max: self.max + Vec3::splat(amount),
        }
    }

    pub fn contains_point(&self, p: Vec3) -> bool {
        p.cmpge(self.min).all() && p.cmple(self.max).all()
    }

    pub fn intersects(&self, other: &Aabb) -> bool {
        self.min.cmple(other.max).all() && other.min.cmple(self.max).all()
    }

    pub fn transformed(&self, m: Mat4) -> Aabb {
        let corners = [
            Vec3::new(self.min.x, self.min.y, self.min.z),
            Vec3::new(self.max.x, self.min.y, self.min.z),
            Vec3::new(self.min.x, self.max.y, self.min.z),
            Vec3::new(self.max.x, self.max.y, self.min.z),
            Vec3::new(self.min.x, self.min.y, self.max.z),
            Vec3::new(self.max.x, self.min.y, self.max.z),
            Vec3::new(self.min.x, self.max.y, self.max.z),
            Vec3::new(self.max.x, self.max.y, self.max.z),
        ];
        Aabb::from_points(corners.map(|c| m.transform_point3(c)))
    }

    /// Slab test. Returns `None` when the ray misses, otherwise near/far
    /// parameters clipped to the positive ray range.
    pub fn intersect_ray(&self, ray: &Ray) -> Option<(f32, f32)> {
        if !self.is_valid() {
            return None;
        }
        let inv = Vec3::ONE / ray.dir;
        let t0 = (self.min - ray.origin) * inv;
        let t1 = (self.max - ray.origin) * inv;
        let tmin = t0.min(t1);
        let tmax = t0.max(t1);
        let near = tmin.max_element().max(0.0);
        let far = tmax.min_element();
        if near <= far {
            Some((near, far))
        } else {
            None
        }
    }
}

/// Ray with a normalized direction.
#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

impl Ray {
    pub fn new(origin: Vec3, dir: Vec3) -> Self {
        Self {
            origin,
            dir: dir.normalize_or_zero(),
        }
    }

    pub fn at(&self, t: f32) -> Vec3 {
        self.origin + self.dir * t
    }
}

/// Möller–Trumbore ray/triangle intersection (two-sided).
pub fn ray_triangle(ray: &Ray, a: Vec3, b: Vec3, c: Vec3) -> Option<(f32, f32, f32)> {
    let e1 = b - a;
    let e2 = c - a;
    let p = ray.dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv_det = 1.0 / det;
    let tvec = ray.origin - a;
    let u = tvec.dot(p) * inv_det;
    if !(-1e-6..=1.0 + 1e-6).contains(&u) {
        return None;
    }
    let q = tvec.cross(e1);
    let v = ray.dir.dot(q) * inv_det;
    if v < -1e-6 || u + v > 1.0 + 1e-6 {
        return None;
    }
    let t = e2.dot(q) * inv_det;
    if t > 1e-5 {
        Some((t, u, v))
    } else {
        None
    }
}

pub fn ray_plane(ray: &Ray, point: Vec3, normal: Vec3) -> Option<f32> {
    let denom = ray.dir.dot(normal);
    if denom.abs() < 1e-8 {
        return None;
    }
    let t = (point - ray.origin).dot(normal) / denom;
    (t > 0.0).then_some(t)
}

/// Closest points between two lines, returned as parameters on each line.
/// Used by the translate gizmo so dragging feels exact.
pub fn closest_line_params(p1: Vec3, d1: Vec3, p2: Vec3, d2: Vec3) -> (f32, f32) {
    let r = p1 - p2;
    let a = d1.dot(d1);
    let b = d1.dot(d2);
    let c = d2.dot(d2);
    let d = d1.dot(r);
    let e = d2.dot(r);
    let denom = a * c - b * b;
    if denom.abs() < 1e-8 {
        (0.0, e / c.max(1e-8))
    } else {
        ((b * e - c * d) / denom, (a * e - b * d) / denom)
    }
}

/// Orthonormal tangent frame for a unit normal.
pub fn orthonormal_basis(normal: Vec3) -> (Vec3, Vec3) {
    let n = normal.normalize_or(Vec3::Y);
    let helper = if n.x.abs() > 0.9 { Vec3::Y } else { Vec3::X };
    let tangent = helper.cross(n).normalize();
    let bitangent = n.cross(tangent);
    (tangent, bitangent)
}

/// Produce a quaternion that points an object's -Z axis at `target`
/// (matching the convention used by cameras and lights).
pub fn look_at_quat(eye: Vec3, target: Vec3, up: Vec3) -> Quat {
    let forward = (target - eye).normalize_or(Vec3::NEG_Z);
    let up = if forward.dot(up).abs() > 0.999 {
        Vec3::Z
    } else {
        up
    };
    let right = forward.cross(up).normalize_or(Vec3::X);
    let true_up = right.cross(forward);
    let m = Mat3::from_cols(right, true_up, -forward);
    Quat::from_mat3(&m)
}

// ---------------------------------------------------------------------------
// Deterministic random numbers
// ---------------------------------------------------------------------------

/// Small, fast, deterministic PCG32 generator.
///
/// Seeding is explicit everywhere (pixel index, sample index, frame seed) so
/// renders and tests reproduce bit-for-bit regardless of thread scheduling.
#[derive(Clone, Debug)]
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    pub fn new(seed: u64) -> Self {
        let mut rng = Self {
            state: 0,
            inc: (seed << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    pub fn from_parts(seed: u64, stream: u64) -> Self {
        let mut rng = Self {
            state: 0,
            inc: (stream << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(6364136223846793005).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform in `[0, 1)`.
    pub fn next_f32(&mut self) -> f32 {
        // 24 bits gives exact uniform floats without rounding to 1.0.
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }

    pub fn next_usize(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u32() as usize) % n
        }
    }
}

/// Cosine-weighted hemisphere sample around +Z.
pub fn sample_cosine_hemisphere(rng: &mut Pcg32) -> Vec3 {
    let u = rng.next_f32();
    let v = rng.next_f32();
    let r = u.sqrt();
    let phi = std::f32::consts::TAU * v;
    Vec3::new(r * phi.cos(), r * phi.sin(), (1.0 - u).max(0.0).sqrt())
}

/// Uniform sample on a unit disk.
pub fn sample_disk(rng: &mut Pcg32) -> Vec2 {
    let r = rng.next_f32().sqrt();
    let phi = std::f32::consts::TAU * rng.next_f32();
    Vec2::new(r * phi.cos(), r * phi.sin())
}

/// Build a world-space direction from a local (tangent-space) direction.
pub fn tangent_to_world(local: Vec3, normal: Vec3) -> Vec3 {
    let (t, b) = orthonormal_basis(normal);
    (t * local.x + b * local.y + normal * local.z).normalize_or(normal)
}

// ---------------------------------------------------------------------------
// Color
// ---------------------------------------------------------------------------

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

pub fn srgb_vec3_to_linear(c: Vec3) -> Vec3 {
    Vec3::new(
        srgb_to_linear(c.x),
        srgb_to_linear(c.y),
        srgb_to_linear(c.z),
    )
}

pub fn linear_vec3_to_srgb(c: Vec3) -> Vec3 {
    Vec3::new(
        linear_to_srgb(c.x),
        linear_to_srgb(c.y),
        linear_to_srgb(c.z),
    )
}

/// Narkowicz-style ACES filmic tone mapping.
pub fn aces_tonemap(x: Vec3) -> Vec3 {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    (x * (x * a + Vec3::splat(b)) / (x * (x * c + Vec3::splat(d)) + Vec3::splat(e)))
        .clamp(Vec3::ZERO, Vec3::ONE)
}

/// Convert a linear HDR color to 8-bit sRGB with exposure and tone mapping.
pub fn to_srgb8(color: Vec3, exposure: f32) -> [u8; 3] {
    // Exposure is in stops: 0 EV is neutral, +1 EV doubles the linear value.
    let c = aces_tonemap(color * exposure.exp2());
    let s = linear_vec3_to_srgb(c).clamp(Vec3::ZERO, Vec3::ONE);
    [
        (s.x * 255.0 + 0.5) as u8,
        (s.y * 255.0 + 0.5) as u8,
        (s.z * 255.0 + 0.5) as u8,
    ]
}

pub fn luminance(c: Vec3) -> f32 {
    c.dot(Vec3::new(0.2126, 0.7152, 0.0722))
}

#[cfg(test)]
mod exposure_tests {
    use super::*;

    #[test]
    fn zero_exposure_is_neutral_and_positive_exposure_brightens() {
        let neutral = to_srgb8(Vec3::splat(1.0), 0.0);
        let brighter = to_srgb8(Vec3::splat(1.0), 1.0);
        let darker = to_srgb8(Vec3::splat(1.0), -1.0);
        assert!(neutral[0] > 200);
        assert!(brighter[0] >= neutral[0]);
        assert!(darker[0] < neutral[0]);
    }
}

// ---------------------------------------------------------------------------
// Stable hashing (test signatures, cache keys)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
pub struct StableHasher(u64);

impl StableHasher {
    pub fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    pub fn write_u8(&mut self, v: u8) {
        self.0 ^= v as u64;
        self.0 = self.0.wrapping_mul(0x100_0000_01b3);
    }

    pub fn write_u32(&mut self, v: u32) {
        for b in v.to_le_bytes() {
            self.write_u8(b);
        }
    }

    pub fn write_u64(&mut self, v: u64) {
        for b in v.to_le_bytes() {
            self.write_u8(b);
        }
    }

    pub fn write_f32(&mut self, v: f32) {
        // Normalize -0.0 so signatures do not depend on signed zero.
        let v = if v == 0.0 { 0.0 } else { v };
        self.write_u32(v.to_bits());
    }

    pub fn write_vec3(&mut self, v: Vec3) {
        self.write_f32(v.x);
        self.write_f32(v.y);
        self.write_f32(v.z);
    }

    pub fn write_vec2(&mut self, v: Vec2) {
        self.write_f32(v.x);
        self.write_f32(v.y);
    }

    pub fn write_str(&mut self, s: &str) {
        self.write_u64(s.len() as u64);
        for b in s.as_bytes() {
            self.write_u8(*b);
        }
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

/// Iterate samples in a deterministic low-discrepancy order.
pub fn van_der_corput(index: u32, base: u32) -> f32 {
    let mut i = index;
    let mut denom = 1.0f32;
    let mut result = 0.0f32;
    while i > 0 {
        denom *= base as f32;
        result += (i % base) as f32 / denom;
        i /= base;
    }
    result
}

pub fn vec3_from_array(a: [f32; 3]) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}

pub fn vec2_from_array(a: [f32; 2]) -> Vec2 {
    Vec2::new(a[0], a[1])
}

pub fn vec4_array(v: Vec4) -> [f32; 4] {
    [v.x, v.y, v.z, v.w]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic_and_bounded() {
        let mut a = Pcg32::new(7);
        let mut b = Pcg32::new(7);
        for _ in 0..1000 {
            let x = a.next_f32();
            assert_eq!(x, b.next_f32());
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn euler_degrees_round_trips_with_from_euler_degrees() {
        let euler = Vec3::new(15.0, 45.0, -30.0);
        let round_trip = Transform::from_euler_degrees(euler).euler_degrees();
        assert!(
            (round_trip - euler).abs().max_element() < 1e-4,
            "got {round_trip:?}"
        );
        // A quarter turn about Y must read back as 90 degrees, not 1.5708.
        let y = Transform::from_rotation_y(std::f32::consts::FRAC_PI_2).euler_degrees();
        assert!((y.y - 90.0).abs() < 1e-4, "got {y:?}");
    }

    #[test]
    fn transform_hierarchy_matches_matrix() {
        let parent = Transform::new(
            Vec3::new(3.0, 1.0, -2.0),
            Quat::from_rotation_y(0.5),
            Vec3::splat(2.0),
        );
        let child = Transform::new(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY, Vec3::ONE);
        let composed = parent.compose(&child);
        let expected = parent.matrix() * child.matrix();
        let p = Vec3::new(1.0, 2.0, 3.0);
        let got = composed.matrix().transform_point3(p);
        let want = expected.transform_point3(p);
        assert!((got - want).length() < 1e-4);
    }

    #[test]
    fn ray_triangle_hits_and_misses() {
        let ray = Ray::new(Vec3::new(0.2, 0.2, 1.0), Vec3::NEG_Z);
        let hit = ray_triangle(&ray, Vec3::ZERO, Vec3::X, Vec3::Y);
        assert!(hit.is_some());
        let miss = Ray::new(Vec3::new(2.0, 2.0, 1.0), Vec3::NEG_Z);
        assert!(ray_triangle(&miss, Vec3::ZERO, Vec3::X, Vec3::Y).is_none());
    }

    #[test]
    fn aabb_ray_clips() {
        let a = Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0));
        let ray = Ray::new(Vec3::new(0.0, 0.0, 5.0), Vec3::NEG_Z);
        let (near, far) = a.intersect_ray(&ray).unwrap();
        assert!((near - 4.0).abs() < 1e-5);
        assert!((far - 6.0).abs() < 1e-5);
    }

    #[test]
    fn cosine_samples_are_hemispheric() {
        let mut rng = Pcg32::new(3);
        for _ in 0..500 {
            let s = sample_cosine_hemisphere(&mut rng);
            assert!(s.z >= 0.0);
            assert!((s.length() - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn color_roundtrip_is_close() {
        for c in [0.0, 0.02, 0.2, 0.5, 0.9, 1.0] {
            assert!((linear_to_srgb(srgb_to_linear(c)) - c).abs() < 1e-5);
        }
    }
}
