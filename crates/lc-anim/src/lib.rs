//! Animation curves, tracks, playback, and deformation helpers.
//!
//! All sampling is pure and deterministic: a clip sampled at the same time
//! always yields the same transform. The timeline UI only supplies time.

use glam::{Mat4, Vec3};
use lc_core::{StableHasher, Transform};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interpolation {
    Linear,
    Step,
    /// Cubic Hermite with automatically derived tangents (smooth F-curve).
    Bezier,
}

impl Default for Interpolation {
    fn default() -> Self {
        Self::Bezier
    }
}

impl Interpolation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Step => "Stepped",
            Self::Bezier => "Smooth",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub time: f32,
    pub value: f32,
    #[serde(default)]
    pub interpolation: Interpolation,
    /// Slope in value units per second entering this key.
    #[serde(default)]
    pub tangent_in: f32,
    /// Slope leaving this key.
    #[serde(default)]
    pub tangent_out: f32,
}

impl Keyframe {
    pub fn new(time: f32, value: f32, interpolation: Interpolation) -> Self {
        Self {
            time,
            value,
            interpolation,
            tangent_in: 0.0,
            tangent_out: 0.0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    pub keys: Vec<Keyframe>,
}

impl Curve {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn insert(&mut self, time: f32, value: f32, interpolation: Interpolation) -> usize {
        let time = time.max(0.0);
        let idx = match self.keys.binary_search_by(|k| {
            k.time
                .partial_cmp(&time)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            Ok(i) => {
                self.keys[i].value = value;
                self.keys[i].interpolation = interpolation;
                i
            }
            Err(i) => {
                self.keys
                    .insert(i, Keyframe::new(time, value, interpolation));
                i
            }
        };
        self.update_tangents();
        idx
    }

    pub fn remove_nearest(&mut self, time: f32) -> Option<Keyframe> {
        if self.keys.is_empty() {
            return None;
        }
        let mut best = 0;
        let mut dist = f32::INFINITY;
        for (i, k) in self.keys.iter().enumerate() {
            let d = (k.time - time).abs();
            if d < dist {
                dist = d;
                best = i;
            }
        }
        let removed = self.keys.remove(best);
        self.update_tangents();
        Some(removed)
    }

    /// Recompute auto tangents from neighbouring keys (Catmull-Rom slopes).
    pub fn update_tangents(&mut self) {
        let n = self.keys.len();
        if n == 0 {
            return;
        }
        // Read values first to avoid borrow conflicts.
        let times: Vec<f32> = self.keys.iter().map(|k| k.time).collect();
        let values: Vec<f32> = self.keys.iter().map(|k| k.value).collect();
        for i in 0..n {
            let (prev_t, prev_v) = if i == 0 {
                (times[0], values[0])
            } else {
                (times[i - 1], values[i - 1])
            };
            let (next_t, next_v) = if i + 1 == n {
                (times[n - 1], values[n - 1])
            } else {
                (times[i + 1], values[i + 1])
            };
            let dt = next_t - prev_t;
            let slope = if dt.abs() > 1e-6 {
                (next_v - prev_v) / dt
            } else {
                0.0
            };
            self.keys[i].tangent_in = slope;
            self.keys[i].tangent_out = slope;
        }
    }

    pub fn sample(&self, time: f32) -> f32 {
        if self.keys.is_empty() {
            return 0.0;
        }
        if self.keys.len() == 1 {
            return self.keys[0].value;
        }
        if time <= self.keys[0].time {
            return self.keys[0].value;
        }
        if time >= self.keys[self.keys.len() - 1].time {
            return self.keys[self.keys.len() - 1].value;
        }
        let mut lo = 0usize;
        let mut hi = self.keys.len() - 1;
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.keys[mid].time <= time {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let k0 = self.keys[lo];
        let k1 = self.keys[lo + 1];
        let dt = (k1.time - k0.time).max(1e-6);
        let t = ((time - k0.time) / dt).clamp(0.0, 1.0);
        match k0.interpolation {
            Interpolation::Step => k0.value,
            Interpolation::Linear => k0.value + (k1.value - k0.value) * t,
            Interpolation::Bezier => {
                let t2 = t * t;
                let t3 = t2 * t;
                let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
                let h10 = t3 - 2.0 * t2 + t;
                let h01 = -2.0 * t3 + 3.0 * t2;
                let h11 = t3 - t2;
                h00 * k0.value
                    + h10 * dt * k0.tangent_out
                    + h01 * k1.value
                    + h11 * dt * k1.tangent_in
            }
        }
    }

    pub fn value_range(&self) -> (f32, f32) {
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for k in &self.keys {
            min = min.min(k.value);
            max = max.max(k.value);
        }
        if min.is_finite() {
            (min, max)
        } else {
            (0.0, 0.0)
        }
    }

    pub fn signature(&self) -> u64 {
        let mut h = StableHasher::new();
        for k in &self.keys {
            h.write_f32(k.time);
            h.write_f32(k.value);
            h.write_u8(match k.interpolation {
                Interpolation::Linear => 0,
                Interpolation::Step => 1,
                Interpolation::Bezier => 2,
            });
        }
        h.finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimProperty {
    TranslationX,
    TranslationY,
    TranslationZ,
    RotationX,
    RotationY,
    RotationZ,
    ScaleX,
    ScaleY,
    ScaleZ,
    Fov,
    LightIntensity,
    LightColorR,
    LightColorG,
    LightColorB,
}

impl AnimProperty {
    pub fn label(self) -> &'static str {
        match self {
            Self::TranslationX => "Location X",
            Self::TranslationY => "Location Y",
            Self::TranslationZ => "Location Z",
            Self::RotationX => "Rotation X",
            Self::RotationY => "Rotation Y",
            Self::RotationZ => "Rotation Z",
            Self::ScaleX => "Scale X",
            Self::ScaleY => "Scale Y",
            Self::ScaleZ => "Scale Z",
            Self::Fov => "Camera FOV",
            Self::LightIntensity => "Light Intensity",
            Self::LightColorR => "Light Color R",
            Self::LightColorG => "Light Color G",
            Self::LightColorB => "Light Color B",
        }
    }

    pub fn is_rotation(self) -> bool {
        matches!(self, Self::RotationX | Self::RotationY | Self::RotationZ)
    }

    pub fn default_value(self) -> f32 {
        match self {
            Self::ScaleX | Self::ScaleY | Self::ScaleZ => 1.0,
            Self::LightIntensity => 1.0,
            Self::LightColorR | Self::LightColorG | Self::LightColorB => 1.0,
            Self::RotationX | Self::RotationY | Self::RotationZ => 0.0,
            Self::Fov => 45.0,
            _ => 0.0,
        }
    }

    pub const ALL_TRANSFORM: [AnimProperty; 9] = [
        Self::TranslationX,
        Self::TranslationY,
        Self::TranslationZ,
        Self::RotationX,
        Self::RotationY,
        Self::RotationZ,
        Self::ScaleX,
        Self::ScaleY,
        Self::ScaleZ,
    ];
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimationTrack {
    pub property: AnimProperty,
    pub curve: Curve,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AnimationClip {
    pub tracks: Vec<AnimationTrack>,
}

impl AnimationClip {
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn track(&self, property: AnimProperty) -> Option<&AnimationTrack> {
        self.tracks.iter().find(|t| t.property == property)
    }

    pub fn track_mut(&mut self, property: AnimProperty) -> Option<&mut AnimationTrack> {
        self.tracks.iter_mut().find(|t| t.property == property)
    }

    pub fn set_key(
        &mut self,
        property: AnimProperty,
        time: f32,
        value: f32,
        interpolation: Interpolation,
    ) {
        if let Some(track) = self.track_mut(property) {
            track.curve.insert(time, value, interpolation);
        } else {
            let mut curve = Curve::new();
            curve.insert(time, value, interpolation);
            self.tracks.push(AnimationTrack { property, curve });
        }
    }

    pub fn remove_key(&mut self, property: AnimProperty, time: f32) {
        if let Some(track) = self.track_mut(property) {
            track.curve.remove_nearest(time);
        }
        self.tracks.retain(|t| !t.curve.keys.is_empty());
    }

    pub fn remove_track(&mut self, property: AnimProperty) {
        self.tracks.retain(|t| t.property != property);
    }

    pub fn sample(&self, property: AnimProperty, time: f32) -> Option<f32> {
        self.track(property).map(|t| t.curve.sample(time))
    }

    pub fn sample_or(&self, property: AnimProperty, time: f32, fallback: f32) -> f32 {
        self.sample(property, time).unwrap_or(fallback)
    }

    /// Apply all transform channels to a base transform at `time`.
    pub fn apply_transform(&self, base: Transform, time: f32) -> Transform {
        let mut t = base;
        t.translation.x = self.sample_or(AnimProperty::TranslationX, time, base.translation.x);
        t.translation.y = self.sample_or(AnimProperty::TranslationY, time, base.translation.y);
        t.translation.z = self.sample_or(AnimProperty::TranslationZ, time, base.translation.z);
        let euler = base.euler_degrees();
        let rx = self
            .sample(AnimProperty::RotationX, time)
            .unwrap_or(euler.x);
        let ry = self
            .sample(AnimProperty::RotationY, time)
            .unwrap_or(euler.y);
        let rz = self
            .sample(AnimProperty::RotationZ, time)
            .unwrap_or(euler.z);
        if self.sample(AnimProperty::RotationX, time).is_some()
            || self.sample(AnimProperty::RotationY, time).is_some()
            || self.sample(AnimProperty::RotationZ, time).is_some()
        {
            t.rotation = Transform::from_euler_degrees(Vec3::new(rx, ry, rz)).rotation;
        }
        t.scale.x = self.sample_or(AnimProperty::ScaleX, time, base.scale.x);
        t.scale.y = self.sample_or(AnimProperty::ScaleY, time, base.scale.y);
        t.scale.z = self.sample_or(AnimProperty::ScaleZ, time, base.scale.z);
        t
    }

    pub fn key_times(&self) -> Vec<f32> {
        let mut times: Vec<f32> = self
            .tracks
            .iter()
            .flat_map(|t| t.curve.keys.iter().map(|k| k.time))
            .collect();
        times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        times.dedup_by(|a, b| (*a - *b).abs() < 1e-5);
        times
    }

    pub fn duration(&self) -> f32 {
        self.key_times().last().copied().unwrap_or(0.0)
    }

    pub fn signature(&self) -> u64 {
        let mut h = StableHasher::new();
        for t in &self.tracks {
            h.write_u32(t.property as u32);
            h.write_u64(t.curve.signature());
        }
        h.finish()
    }
}

/// Deterministic playback state, independent of wall-clock and UI.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Playback {
    pub start: f32,
    pub end: f32,
    pub fps: f32,
    pub time: f32,
    pub playing: bool,
    pub looping: bool,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            start: 0.0,
            end: 120.0,
            fps: 24.0,
            time: 0.0,
            playing: false,
            looping: true,
        }
    }
}

impl Playback {
    pub fn frame(&self) -> i32 {
        (self.time * self.fps).round() as i32
    }

    pub fn advance(&mut self, delta_seconds: f32) {
        if !self.playing {
            return;
        }
        let end = self.end.max(self.start);
        self.time += delta_seconds.clamp(0.0, 1.0);
        if self.time > end {
            if self.looping {
                let span = (end - self.start).max(1e-3);
                self.time = self.start + (self.time - self.start) % span;
            } else {
                self.time = end;
                self.playing = false;
            }
        }
    }

    pub fn seek(&mut self, time: f32) {
        self.time = time.clamp(self.start, self.end.max(self.start));
    }
}

/// Linear blend skinning. Joint matrices are expected in the mesh's space.
/// Invalid or zero weights leave the vertex unchanged. This function never
/// panics on mismatched buffers from imported files.
pub fn skin_positions_indexed(
    positions: &[Vec3],
    joints: &[[u32; 4]],
    weights: &[[f32; 4]],
    joint_matrices: &[Mat4],
) -> Vec<Vec3> {
    let mut out = Vec::with_capacity(positions.len());
    for (i, p) in positions.iter().enumerate() {
        let (Some(js), Some(ws)) = (joints.get(i), weights.get(i)) else {
            out.push(*p);
            continue;
        };
        let mut total = 0.0;
        let mut acc = Vec3::ZERO;
        for k in 0..4 {
            let w = ws[k];
            if w <= 0.0 {
                continue;
            }
            if let Some(m) = joint_matrices.get(js[k] as usize) {
                acc += m.transform_point3(*p) * w;
                total += w;
            }
        }
        if total > 1e-6 {
            out.push(acc / total);
        } else {
            out.push(*p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_interpolates_linearly() {
        let mut c = Curve::new();
        c.insert(0.0, 0.0, Interpolation::Linear);
        c.insert(2.0, 10.0, Interpolation::Linear);
        assert!((c.sample(1.0) - 5.0).abs() < 1e-4);
        assert!((c.sample(-1.0) - 0.0).abs() < 1e-6);
        assert!((c.sample(3.0) - 10.0).abs() < 1e-6);
    }

    #[test]
    fn curve_step_holds() {
        let mut c = Curve::new();
        c.insert(0.0, 2.0, Interpolation::Step);
        c.insert(1.0, 9.0, Interpolation::Step);
        assert!((c.sample(0.9) - 2.0).abs() < 1e-6);
        assert!((c.sample(1.0) - 9.0).abs() < 1e-6);
    }

    #[test]
    fn bezier_passes_through_keys_and_is_smooth() {
        let mut c = Curve::new();
        c.insert(0.0, 0.0, Interpolation::Bezier);
        c.insert(1.0, 1.0, Interpolation::Bezier);
        c.insert(2.0, 0.0, Interpolation::Bezier);
        assert!((c.sample(0.0) - 0.0).abs() < 1e-5);
        assert!((c.sample(1.0) - 1.0).abs() < 1e-5);
        assert!((c.sample(2.0) - 0.0).abs() < 1e-5);
        assert!(c.sample(0.5) > 0.0 && c.sample(0.5) < 1.0);
    }

    #[test]
    fn inserting_out_of_order_replaces_duplicate_time() {
        let mut c = Curve::new();
        c.insert(2.0, 2.0, Interpolation::Linear);
        c.insert(0.0, 0.0, Interpolation::Linear);
        c.insert(1.0, 1.0, Interpolation::Linear);
        c.insert(1.0, 5.0, Interpolation::Step);
        assert_eq!(c.keys.len(), 3);
        assert!((c.sample(1.0) - 5.0).abs() < 1e-5);
        assert!(c.keys.windows(2).all(|w| w[0].time <= w[1].time));
    }

    #[test]
    fn clip_applies_transform_channels() {
        let base = Transform::from_translation(Vec3::ZERO);
        let mut clip = AnimationClip::default();
        clip.set_key(AnimProperty::TranslationX, 0.0, 0.0, Interpolation::Linear);
        clip.set_key(
            AnimProperty::TranslationX,
            10.0,
            20.0,
            Interpolation::Linear,
        );
        clip.set_key(AnimProperty::RotationZ, 0.0, 0.0, Interpolation::Linear);
        clip.set_key(AnimProperty::RotationZ, 10.0, 90.0, Interpolation::Linear);
        let t = clip.apply_transform(base, 5.0);
        assert!((t.translation.x - 10.0).abs() < 1e-5);
        let rotated = t.transform_vector(Vec3::X);
        let expected = Vec3::new(45f32.to_radians().cos(), 45f32.to_radians().sin(), 0.0);
        assert!((rotated - expected).length() < 1e-4, "rotated {rotated:?}");
    }

    #[test]
    fn skinning_translates_weighted_vertices() {
        let positions = vec![Vec3::ZERO, Vec3::X];
        let joints = [[0u32, 0, 0, 0], [1, 0, 0, 0]];
        let weights = [[1.0f32, 0.0, 0.0, 0.0], [0.5, 0.5, 0.0, 0.0]];
        let transforms = vec![
            Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0)),
            Mat4::from_translation(Vec3::new(0.0, 2.0, 0.0)),
        ];
        let out = skin_positions_indexed(&positions, &joints, &weights, &transforms);
        assert!((out[0] - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-5);
        assert!((out[1] - Vec3::new(1.5, 1.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn playback_loops_deterministically() {
        let mut p = Playback {
            start: 0.0,
            end: 2.0,
            fps: 24.0,
            ..Default::default()
        };
        p.playing = true;
        p.advance(0.5);
        p.advance(0.5);
        p.advance(0.5);
        p.advance(0.5);
        p.advance(0.5);
        assert!(p.time >= 0.0 && p.time <= 2.0);
        assert_eq!(p.frame(), (p.time * 24.0).round() as i32);
    }
}
