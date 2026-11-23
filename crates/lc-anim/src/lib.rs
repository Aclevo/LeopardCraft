//! Animation curves, tracks, playback, and deformation helpers.
//!
//! All sampling is pure and deterministic: a clip sampled at the same time
//! always yields the same transform. The timeline UI only supplies time.

use glam::{Mat4, Quat, Vec3};
use lc_core::{StableHasher, Transform};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    /// Hold the left key's value until the next key.
    #[serde(alias = "step")]
    Constant,
    /// Interpolate with constant speed between the two keys.
    Linear,
    /// Recompute Catmull-Rom tangents and run a cubic Hermite between keys.
    /// Smooth owns its tangents, so selecting it replaces hand-authored slopes.
    #[serde(alias = "bezier")]
    #[default]
    Smooth,
    /// Run a cubic Hermite through the spline keys and expose their tangent
    /// handles. Spline preserves hand-authored slopes and tangent types.
    Spline,
}

impl Interpolation {
    pub const ALL: [Self; 4] = [Self::Constant, Self::Linear, Self::Smooth, Self::Spline];

    pub fn label(self) -> &'static str {
        match self {
            Self::Constant => "Constant",
            Self::Linear => "Linear",
            Self::Smooth => "Smooth",
            Self::Spline => "Spline",
        }
    }
}

/// How one side of a key's tangent is derived. Cheetah3D exposes the same
/// interpolation vocabulary for its `Animation -> Tangent -> Type` commands;
/// the manual does not spell out the derivation formulas, so LeopardCraft keeps
/// the mapping explicit:
///
/// - Constant: a flat (zero) slope, used by hold-style tangents;
/// - Linear: the slope of the adjacent segment;
/// - Smooth: the Catmull-Rom slope through the neighbouring keys;
/// - Spline: keep a hand-authored slope, or fall back to the adjacent
///   segment's secant slope. This makes an untouched Spline key visibly
///   different from Smooth, which owns Catmull-Rom tangents.
///
/// Spline is the serde default so a key written before tangent types existed,
/// including its manual slopes, round-trips unchanged through `.lcscene`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TangentType {
    Constant,
    Linear,
    Smooth,
    #[default]
    Spline,
}

impl TangentType {
    pub const ALL: [Self; 4] = [Self::Constant, Self::Linear, Self::Smooth, Self::Spline];

    pub fn label(self) -> &'static str {
        match self {
            Self::Constant => "Constant",
            Self::Linear => "Linear",
            Self::Smooth => "Smooth",
            Self::Spline => "Spline",
        }
    }
}

/// The tangent type a freshly recorded key uses. Constant and Linear keys do
/// not consult their tangents, but recording a matching type keeps the editor's
/// Tangent Type control truthful. Smooth is fully automatic. Spline seeds the
/// adjacent secant slope but allows a later drag to make it hand-authored.
fn default_tangent_type(interpolation: Interpolation) -> TangentType {
    match interpolation {
        Interpolation::Constant => TangentType::Constant,
        Interpolation::Linear => TangentType::Linear,
        Interpolation::Smooth => TangentType::Smooth,
        Interpolation::Spline => TangentType::Spline,
    }
}

/// Tolerance used to identify one key by its time in the editor.
pub const KEY_TIME_EPSILON: f32 = 1e-3;

/// Normalize a quaternion, falling back to identity for degenerate input.
/// Shared with the native format's load-time repair of rotation keys.
pub fn unit_quaternion(q: Quat) -> Quat {
    let length = q.length_squared().sqrt();
    if length > 1e-12 && length.is_finite() {
        Quat::from_xyzw(q.x / length, q.y / length, q.z / length, q.w / length)
    } else {
        Quat::IDENTITY
    }
}

fn negate_quaternion(q: Quat) -> Quat {
    Quat::from_xyzw(-q.x, -q.y, -q.z, -q.w)
}

/// Flip `b` into the same hemisphere as `a` when the dot product is negative.
fn align_quaternion(a: Quat, b: Quat) -> Quat {
    if a.dot(b) < 0.0 {
        negate_quaternion(b)
    } else {
        b
    }
}

/// Shortest-path spherical linear interpolation, as glTF requires for LINEAR
/// rotation channels. Antipodal keys (`q` and `-q`) represent the same
/// orientation and must not interpolate through the zero quaternion.
fn slerp_shortest(a: Quat, b: Quat, t: f32) -> Quat {
    let a = unit_quaternion(a);
    let b = align_quaternion(a, unit_quaternion(b));
    let mut dot = a.dot(b).clamp(-1.0, 1.0);
    if dot < 0.0 {
        dot = -dot;
    }
    if dot > 0.9995 {
        let lerp = Quat::from_xyzw(
            a.x + (b.x - a.x) * t,
            a.y + (b.y - a.y) * t,
            a.z + (b.z - a.z) * t,
            a.w + (b.w - a.w) * t,
        );
        return unit_quaternion(lerp);
    }
    let theta = dot.acos();
    let sin_theta = theta.sin();
    if sin_theta.abs() < 1e-6 {
        return a;
    }
    let w0 = ((1.0 - t) * theta).sin() / sin_theta;
    let w1 = (t * theta).sin() / sin_theta;
    unit_quaternion(Quat::from_xyzw(
        a.x * w0 + b.x * w1,
        a.y * w0 + b.y * w1,
        a.z * w0 + b.z * w1,
        a.w * w0 + b.w * w1,
    ))
}

/// One quaternion rotation key. Rotation is a four-component channel, so it
/// has its own track instead of pretending to be three scalar Euler curves.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RotationKey {
    pub time: f32,
    /// Stored as a unit quaternion; sampling re-normalises the result.
    pub value: Quat,
    #[serde(default)]
    pub interpolation: Interpolation,
    /// Quaternion keys have no scalar tangent handles, but they retain the same
    /// tangent-type metadata so a mixed selection applies and round-trips the
    /// same command consistently. During cubic sampling each side's type picks
    /// a flat, secant or Catmull-Rom derivative.
    #[serde(default)]
    pub tangent_in_type: TangentType,
    #[serde(default)]
    pub tangent_out_type: TangentType,
}

impl RotationKey {
    pub fn new(time: f32, value: Quat, interpolation: Interpolation) -> Self {
        let tangent_type = default_tangent_type(interpolation);
        Self {
            time,
            value: unit_quaternion(value),
            interpolation,
            tangent_in_type: tangent_type,
            tangent_out_type: tangent_type,
        }
    }

    /// Apply the same interpolation rule used by scalar keys. Rotation keys do
    /// not store scalar slopes, so entering Smooth or Spline seeds the matching
    /// automatic tangent types; the per-side control is then read by the cubic
    /// quaternion sampler.
    pub fn set_interpolation(&mut self, interpolation: Interpolation) -> bool {
        if self.interpolation == interpolation {
            if interpolation != Interpolation::Smooth {
                return false;
            }
            let changed = self.tangent_in_type != TangentType::Smooth
                || self.tangent_out_type != TangentType::Smooth;
            self.tangent_in_type = TangentType::Smooth;
            self.tangent_out_type = TangentType::Smooth;
            return changed;
        }
        self.interpolation = interpolation;
        if interpolation == Interpolation::Smooth {
            self.tangent_in_type = TangentType::Smooth;
            self.tangent_out_type = TangentType::Smooth;
        } else if interpolation == Interpolation::Spline {
            self.tangent_in_type = TangentType::Spline;
            self.tangent_out_type = TangentType::Spline;
        }
        true
    }

    pub fn set_tangent_type(
        &mut self,
        tangent_type: TangentType,
        incoming: bool,
        outgoing: bool,
    ) -> bool {
        let mut changed = false;
        if incoming && self.tangent_in_type != tangent_type {
            self.tangent_in_type = tangent_type;
            changed = true;
        }
        if outgoing && self.tangent_out_type != tangent_type {
            self.tangent_out_type = tangent_type;
            changed = true;
        }
        changed
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RotationTrack {
    pub keys: Vec<RotationKey>,
}

impl RotationTrack {
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn insert(&mut self, time: f32, value: Quat, interpolation: Interpolation) -> usize {
        let time = time.max(0.0);
        let key = RotationKey::new(time, value, interpolation);
        match self.keys.binary_search_by(|other| {
            other
                .time
                .partial_cmp(&time)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            Ok(i) => {
                self.keys[i] = key;
                i
            }
            Err(i) => {
                self.keys.insert(i, key);
                i
            }
        }
    }

    pub fn remove_nearest(&mut self, time: f32) -> Option<RotationKey> {
        if self.keys.is_empty() {
            return None;
        }
        let mut best = 0;
        let mut distance = f32::INFINITY;
        for (i, key) in self.keys.iter().enumerate() {
            let d = (key.time - time).abs();
            if d < distance {
                distance = d;
                best = i;
            }
        }
        Some(self.keys.remove(best))
    }

    pub fn key_index(&self, time: f32) -> Option<usize> {
        self.keys
            .iter()
            .position(|key| (key.time - time).abs() <= KEY_TIME_EPSILON)
    }

    pub fn set_interpolation(&mut self, time: f32, interpolation: Interpolation) -> bool {
        let Some(index) = self.key_index(time) else {
            return false;
        };
        self.keys[index].set_interpolation(interpolation)
    }

    pub fn set_tangent_type(
        &mut self,
        time: f32,
        tangent_type: TangentType,
        incoming: bool,
        outgoing: bool,
    ) -> bool {
        let Some(index) = self.key_index(time) else {
            return false;
        };
        self.keys[index].set_tangent_type(tangent_type, incoming, outgoing)
    }

    /// Retime quaternion keys around `pivot`. Rotation keys have no stored
    /// scalar tangents: smooth quaternion tangents are derived from the new
    /// neighbouring times during sampling, so moving the times in this same
    /// pass keeps them coherent. Collisions, negative times and non-finite
    /// factors abort the whole operation.
    pub fn retime_selected_keys(
        &mut self,
        pivot: f32,
        factor: f32,
        selected_times: &[f32],
    ) -> bool {
        if !valid_retime(pivot, factor) || self.keys.is_empty() {
            return false;
        }
        let times: Vec<f32> = self.keys.iter().map(|key| key.time).collect();
        let Some(mask) = selected_key_mask(&times, selected_times) else {
            return false;
        };
        let old_keys = self.keys.clone();
        let mut new_times = times.clone();
        for (index, selected) in mask.iter().enumerate() {
            if !selected {
                continue;
            }
            let Some(new_time) = retimed_time(pivot, factor, old_keys[index].time) else {
                return false;
            };
            new_times[index] = new_time;
        }
        let mut order: Vec<usize> = (0..old_keys.len()).collect();
        order.sort_by(|a, b| new_times[*a].total_cmp(&new_times[*b]));
        if order
            .windows(2)
            .any(|pair| (new_times[pair[0]] - new_times[pair[1]]).abs() <= KEY_TIME_EPSILON)
        {
            return false;
        }
        let mut keys = Vec::with_capacity(old_keys.len());
        for index in order {
            let mut key = old_keys[index];
            key.time = new_times[index];
            keys.push(key);
        }
        self.keys = keys;
        true
    }

    /// Retime every key on this track. Whole-take retiming pivots at zero.
    pub fn retime_all_keys(&mut self, pivot: f32, factor: f32) -> bool {
        let times: Vec<f32> = self.keys.iter().map(|key| key.time).collect();
        self.retime_selected_keys(pivot, factor, &times)
    }

    /// Sample the track. Constant holds the left key, Linear uses shortest-path
    /// slerp, and Smooth/Spline run a component-wise cubic Hermite with
    /// hemisphere alignment and a final normalisation (the same rule glTF uses
    /// for CUBICSPLINE rotation). Each side of a cubic uses its Tangent Type:
    /// Constant is flat, Linear is the adjacent secant, Smooth is Catmull-Rom,
    /// and Spline falls back to the secant because quaternions have no scalar
    /// handle to author a manual slope.
    pub fn sample(&self, time: f32) -> Option<Quat> {
        let keys = &self.keys;
        if keys.is_empty() {
            return None;
        }
        if keys.len() == 1 {
            return Some(unit_quaternion(keys[0].value));
        }
        let last = keys.len() - 1;
        if time <= keys[0].time {
            return Some(unit_quaternion(keys[0].value));
        }
        if time >= keys[last].time {
            return Some(unit_quaternion(keys[last].value));
        }
        let mut lo = 0usize;
        let mut hi = last;
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if keys[mid].time <= time {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let k0 = &keys[lo];
        let k1 = &keys[hi];
        let dt = (k1.time - k0.time).max(1e-6);
        let t = ((time - k0.time) / dt).clamp(0.0, 1.0);
        match k0.interpolation {
            Interpolation::Constant => Some(unit_quaternion(k0.value)),
            Interpolation::Linear => Some(slerp_shortest(k0.value, k1.value, t)),
            Interpolation::Smooth | Interpolation::Spline => {
                let q0 = unit_quaternion(k0.value);
                let q1 = align_quaternion(q0, unit_quaternion(k1.value));
                let has_prev = lo > 0;
                let has_next = hi < last;
                let qp = if has_prev {
                    align_quaternion(q0, unit_quaternion(keys[lo - 1].value))
                } else {
                    q0
                };
                let qn = if has_next {
                    align_quaternion(q1, unit_quaternion(keys[hi + 1].value))
                } else {
                    q1
                };
                let dt_prev = if has_prev {
                    (k0.time - keys[lo - 1].time).max(1e-6)
                } else {
                    dt
                };
                let dt_next = if has_next {
                    (keys[hi + 1].time - k1.time).max(1e-6)
                } else {
                    dt
                };
                let a = [q0.x, q0.y, q0.z, q0.w];
                let b = [q1.x, q1.y, q1.z, q1.w];
                let p = [qp.x, qp.y, qp.z, qp.w];
                let n = [qn.x, qn.y, qn.z, qn.w];
                let t2 = t * t;
                let t3 = t2 * t;
                let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
                let h10 = t3 - 2.0 * t2 + t;
                let h01 = -2.0 * t3 + 3.0 * t2;
                let h11 = t3 - t2;
                let mut value = [0.0f32; 4];
                for i in 0..4 {
                    let m0 = match k0.tangent_out_type {
                        TangentType::Constant => 0.0,
                        TangentType::Linear | TangentType::Spline => (b[i] - a[i]) / dt,
                        TangentType::Smooth => {
                            if has_prev {
                                (b[i] - p[i]) / (dt + dt_prev)
                            } else {
                                (b[i] - a[i]) / dt
                            }
                        }
                    };
                    let m1 = match k1.tangent_in_type {
                        TangentType::Constant => 0.0,
                        TangentType::Linear | TangentType::Spline => (b[i] - a[i]) / dt,
                        TangentType::Smooth => {
                            if has_next {
                                (n[i] - a[i]) / (dt + dt_next)
                            } else {
                                (b[i] - a[i]) / dt
                            }
                        }
                    };
                    value[i] = h00 * a[i] + h10 * dt * m0 + h01 * b[i] + h11 * dt * m1;
                }
                let cubic = Quat::from_xyzw(value[0], value[1], value[2], value[3]);
                if cubic.length_squared() < 1e-12 || !cubic.is_finite() {
                    Some(slerp_shortest(q0, q1, t))
                } else {
                    Some(unit_quaternion(cubic))
                }
            }
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
    /// Whether the tangent slopes are hand-authored. Auto tangents are
    /// recalculated from neighbours whenever the curve changes; manual ones
    /// survive edits until the key is re-recorded.
    #[serde(default, skip_serializing_if = "is_false")]
    pub tangent_manual: bool,
    /// How the incoming tangent is derived. Spline retires the hand-authored
    /// slope; other types derive the slope again from the neighbours.
    #[serde(default)]
    pub tangent_in_type: TangentType,
    /// How the outgoing tangent is derived, independently of the incoming side.
    #[serde(default)]
    pub tangent_out_type: TangentType,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Validate a retime request. A retime must be a positive finite scale around
/// a finite pivot; zero would collapse every selected interval into a single
/// instant and negative factors would reverse time.
fn valid_retime(pivot: f32, factor: f32) -> bool {
    pivot.is_finite() && factor.is_finite() && factor > 0.0
}

/// Map one key time through a retime. Negative results are rejected instead of
/// being silently clamped, so the caller can abort atomically.
fn retimed_time(pivot: f32, factor: f32, time: f32) -> Option<f32> {
    let result = pivot + (time - pivot) * factor;
    (result.is_finite() && result >= 0.0).then_some(result)
}

/// Rescale one side of a hand-authored tangent so a segment keeps the same
/// normalised Hermite shape. A tangent is a value change per second, while the
/// sampler multiplies it by the segment duration; multiplying by old_dt/new_dt
/// therefore cancels the changed duration. This is applied independently to
/// the incoming and outgoing sides because a selected key can have only one
/// neighbour selected.
fn rescale_tangent(slope: f32, old_dt: f32, new_dt: f32) -> f32 {
    if old_dt > 1e-6 && new_dt > 1e-6 {
        slope * old_dt / new_dt
    } else {
        slope
    }
}

fn selected_key_mask(times: &[f32], selected_times: &[f32]) -> Option<Vec<bool>> {
    if selected_times.is_empty() {
        return None;
    }
    let mut mask = vec![false; times.len()];
    for selected in selected_times {
        if !selected.is_finite() {
            return None;
        }
        let index = times
            .iter()
            .position(|time| (time - selected).abs() <= KEY_TIME_EPSILON)?;
        mask[index] = true;
    }
    mask.iter().any(|selected| *selected).then_some(mask)
}

impl Keyframe {
    pub fn new(time: f32, value: f32, interpolation: Interpolation) -> Self {
        let tangent_type = default_tangent_type(interpolation);
        Self {
            time,
            value,
            interpolation,
            tangent_in: 0.0,
            tangent_out: 0.0,
            tangent_manual: false,
            tangent_in_type: tangent_type,
            tangent_out_type: tangent_type,
        }
    }

    /// Set this key's interpolation. Smooth genuinely redefines both tangents
    /// as automatic, so it clears hand-authored slopes. Constant, Linear and
    /// Spline leave the stored slopes and tangent types intact: Constant and
    /// Linear do not read them, and Spline may start using them again when the
    /// user drags a handle. Re-deriving the automatic sides happens in
    /// `Curve::update_tangents` after the caller finishes editing the curve.
    pub fn set_interpolation(&mut self, interpolation: Interpolation) -> bool {
        if self.interpolation == interpolation {
            if interpolation != Interpolation::Smooth {
                return false;
            }
            // Re-selecting Smooth must still clear a hand-authored flag, even
            // when the interpolation field itself does not change.
            let changed = self.tangent_manual
                || self.tangent_in_type != TangentType::Smooth
                || self.tangent_out_type != TangentType::Smooth;
            self.tangent_manual = false;
            self.tangent_in_type = TangentType::Smooth;
            self.tangent_out_type = TangentType::Smooth;
            return changed;
        }
        self.interpolation = interpolation;
        if interpolation == Interpolation::Smooth {
            self.tangent_manual = false;
            self.tangent_in_type = TangentType::Smooth;
            self.tangent_out_type = TangentType::Smooth;
        } else if interpolation == Interpolation::Spline && !self.tangent_manual {
            // Spline is the editable form of the same cubic Hermite. Without
            // an authored slope, seed both sides with the automatic fallback
            // so the Tangent Type control describes the active derivation.
            self.tangent_in_type = TangentType::Spline;
            self.tangent_out_type = TangentType::Spline;
        }
        true
    }

    /// Set the derivation rule for either or both tangent sides. A non-Spline
    /// type genuinely redefines that side's slope, so the side is re-derived by
    /// `update_tangents`; a hand-authored value is only preserved when the side
    /// is switched to Spline. When both sides move away from Spline the key's
    /// manual flag is cleared too. Leaving one side on Spline keeps the flag so
    /// that side's authored slope survives.
    pub fn set_tangent_type(
        &mut self,
        tangent_type: TangentType,
        incoming: bool,
        outgoing: bool,
    ) -> bool {
        let clears_manual = tangent_type != TangentType::Spline && incoming && outgoing;
        let mut changed = clears_manual && self.tangent_manual;
        if incoming && self.tangent_in_type != tangent_type {
            self.tangent_in_type = tangent_type;
            changed = true;
        }
        if outgoing && self.tangent_out_type != tangent_type {
            self.tangent_out_type = tangent_type;
            changed = true;
        }
        if clears_manual {
            self.tangent_manual = false;
        }
        changed
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
                self.keys[i].tangent_manual = false;
                let tangent_type = default_tangent_type(interpolation);
                self.keys[i].tangent_in_type = tangent_type;
                self.keys[i].tangent_out_type = tangent_type;
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

    /// Recompute automatic tangents from neighbouring keys. Each side is
    /// derived independently according to its `TangentType`; a hand-authored
    /// Spline side is left exactly as stored. Smooth uses the Catmull-Rom
    /// slope, Linear uses the adjacent segment's secant, Constant is flat, and
    /// Spline uses the adjacent secant until the user drags that handle.
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
            let central_dt = next_t - prev_t;
            let central = if central_dt.abs() > 1e-6 {
                (next_v - prev_v) / central_dt
            } else {
                0.0
            };
            let incoming_secant = if i > 0 && (times[i] - times[i - 1]).abs() > 1e-6 {
                (values[i] - values[i - 1]) / (times[i] - times[i - 1])
            } else {
                central
            };
            let outgoing_secant = if i + 1 < n && (times[i + 1] - times[i]).abs() > 1e-6 {
                (values[i + 1] - values[i]) / (times[i + 1] - times[i])
            } else {
                central
            };
            let key = &mut self.keys[i];
            if !(key.tangent_manual && key.tangent_in_type == TangentType::Spline) {
                key.tangent_in = match key.tangent_in_type {
                    TangentType::Constant => 0.0,
                    TangentType::Linear => incoming_secant,
                    TangentType::Smooth => central,
                    TangentType::Spline => incoming_secant,
                };
            }
            if !(key.tangent_manual && key.tangent_out_type == TangentType::Spline) {
                key.tangent_out = match key.tangent_out_type {
                    TangentType::Constant => 0.0,
                    TangentType::Linear => outgoing_secant,
                    TangentType::Smooth => central,
                    TangentType::Spline => outgoing_secant,
                };
            }
        }
    }

    /// Change one key's interpolation and re-derive its automatic tangents.
    /// The key is identified by time within `KEY_TIME_EPSILON`. Returns false
    /// when the key is absent or already has the requested type and no tangent
    /// redefinition is needed; re-selecting Smooth on a manual key is a real
    /// edit because it returns the key to automatic tangents.
    pub fn set_interpolation(&mut self, time: f32, interpolation: Interpolation) -> bool {
        let Some(key) = self
            .keys
            .iter_mut()
            .find(|key| (key.time - time).abs() <= KEY_TIME_EPSILON)
        else {
            return false;
        };
        if !key.set_interpolation(interpolation) {
            return false;
        }
        self.update_tangents();
        true
    }

    /// Change the tangent derivation for one key, on either or both sides.
    /// Constant, Linear and Smooth re-derive the tangent; Spline keeps an
    /// existing hand-authored slope and uses the automatic secant slope
    /// otherwise. Changing both sides away from Spline clears `tangent_manual`.
    pub fn set_tangent_type(
        &mut self,
        time: f32,
        tangent_type: TangentType,
        incoming: bool,
        outgoing: bool,
    ) -> bool {
        let Some(key) = self
            .keys
            .iter_mut()
            .find(|key| (key.time - time).abs() <= KEY_TIME_EPSILON)
        else {
            return false;
        };
        if !key.set_tangent_type(tangent_type, incoming, outgoing) {
            return false;
        }
        self.update_tangents();
        true
    }

    /// Retime selected scalar keys around `pivot` by `factor`.
    ///
    /// `pivot` is chosen by the editor: selected-key retiming uses the earliest
    /// selected key, so that key keeps its time; whole-take retiming uses zero.
    /// Times are not frame-snapped, and negative or non-finite results abort
    /// the operation. A retime that would put two keys on the same track within
    /// `KEY_TIME_EPSILON` also aborts. The caller decides how to report this;
    /// the curve remains untouched on failure.
    ///
    /// Hand-authored tangents are rescaled against their old and new incident
    /// segment durations, which preserves each Hermite segment's normalised
    /// shape. Automatic tangents are re-derived from the new times. Rotation
    /// tracks move their keys in the same retime pass but have no stored scalar
    /// slopes to rescale.
    pub fn retime_selected_keys(
        &mut self,
        pivot: f32,
        factor: f32,
        selected_times: &[f32],
    ) -> bool {
        if !valid_retime(pivot, factor) || self.keys.is_empty() {
            return false;
        }
        let times: Vec<f32> = self.keys.iter().map(|key| key.time).collect();
        let Some(mask) = selected_key_mask(&times, selected_times) else {
            return false;
        };
        let old_keys = self.keys.clone();
        let mut new_times = times.clone();
        for (index, selected) in mask.iter().enumerate() {
            if !selected {
                continue;
            }
            let Some(new_time) = retimed_time(pivot, factor, old_keys[index].time) else {
                return false;
            };
            new_times[index] = new_time;
        }

        let mut order: Vec<usize> = (0..old_keys.len()).collect();
        order.sort_by(|a, b| new_times[*a].total_cmp(&new_times[*b]));
        if order
            .windows(2)
            .any(|pair| (new_times[pair[0]] - new_times[pair[1]]).abs() <= KEY_TIME_EPSILON)
        {
            return false;
        }

        let mut keys = Vec::with_capacity(old_keys.len());
        for (position, index) in order.iter().copied().enumerate() {
            let mut key = old_keys[index];
            key.time = new_times[index];
            if key.tangent_manual {
                let old_prev = if index > 0 {
                    old_keys[index].time - old_keys[index - 1].time
                } else {
                    0.0
                };
                let old_next = if index + 1 < old_keys.len() {
                    old_keys[index + 1].time - old_keys[index].time
                } else {
                    0.0
                };
                let new_prev = if position > 0 {
                    new_times[index] - new_times[order[position - 1]]
                } else {
                    0.0
                };
                let new_next = if position + 1 < order.len() {
                    new_times[order[position + 1]] - new_times[index]
                } else {
                    0.0
                };
                key.tangent_in = rescale_tangent(old_keys[index].tangent_in, old_prev, new_prev);
                key.tangent_out = rescale_tangent(old_keys[index].tangent_out, old_next, new_next);
            }
            keys.push(key);
        }
        self.keys = keys;
        // Automatic keys follow the new time axis; manual keys were rescaled
        // above and are intentionally skipped by this pass.
        self.update_tangents();
        true
    }

    /// Retime every key on this curve. Whole-take retiming pivots at zero.
    pub fn retime_all_keys(&mut self, pivot: f32, factor: f32) -> bool {
        let times: Vec<f32> = self.keys.iter().map(|key| key.time).collect();
        self.retime_selected_keys(pivot, factor, &times)
    }

    /// Mark one key's tangents as hand-authored. The editor uses the incoming
    /// and outgoing slopes independently, so both are stored on the key.
    pub fn set_manual_tangents(&mut self, time: f32, tangent_in: f32, tangent_out: f32) -> bool {
        let Some(key) = self
            .keys
            .iter_mut()
            .find(|key| (key.time - time).abs() <= 1e-3)
        else {
            return false;
        };
        if !tangent_in.is_finite() || !tangent_out.is_finite() {
            return false;
        }
        key.tangent_in = tangent_in;
        key.tangent_out = tangent_out;
        key.tangent_manual = true;
        // Hand-authoring is the Spline tangent rule. Explicitly selecting this
        // type on a drag keeps one side editable without disturbing the other
        // side's derivation rule.
        key.tangent_in_type = TangentType::Spline;
        key.tangent_out_type = TangentType::Spline;
        true
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
            Interpolation::Constant => k0.value,
            Interpolation::Linear => k0.value + (k1.value - k0.value) * t,
            // Smooth and Spline share the cubic Hermite evaluator. They differ
            // in tangent ownership: Smooth re-derives both sides as automatic,
            // while Spline preserves hand-authored slopes and handles.
            Interpolation::Smooth | Interpolation::Spline => {
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
            h.write_f32(k.tangent_in);
            h.write_f32(k.tangent_out);
            h.write_u8(k.tangent_manual as u8);
            h.write_u8(match k.tangent_in_type {
                TangentType::Constant => 0,
                TangentType::Linear => 1,
                TangentType::Smooth => 2,
                TangentType::Spline => 3,
            });
            h.write_u8(match k.tangent_out_type {
                TangentType::Constant => 0,
                TangentType::Linear => 1,
                TangentType::Smooth => 2,
                TangentType::Spline => 3,
            });
            h.write_u8(match k.interpolation {
                Interpolation::Constant => 0,
                Interpolation::Linear => 1,
                Interpolation::Smooth => 2,
                Interpolation::Spline => 3,
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
    /// Weight of the object's Copy Location tag(s).
    CopyLocationWeight,
    /// Surface roughness of the material the clip belongs to.
    MaterialRoughness,
    /// Surface metallic value of the material the clip belongs to.
    MaterialMetallic,
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
            Self::CopyLocationWeight => "Copy Location Weight",
            Self::MaterialRoughness => "Material Roughness",
            Self::MaterialMetallic => "Material Metallic",
        }
    }

    pub fn is_rotation(self) -> bool {
        matches!(self, Self::RotationX | Self::RotationY | Self::RotationZ)
    }

    pub fn default_value(self) -> f32 {
        match self {
            Self::ScaleX | Self::ScaleY | Self::ScaleZ => 1.0,
            Self::LightIntensity => 1.0,
            Self::CopyLocationWeight => 1.0,
            Self::MaterialRoughness => 0.5,
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

/// Where an animatable channel lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelScope {
    Object,
    Tag,
    Material,
}

/// Which kind of entity owns a channel, used by the editor to decide whether a
/// channel is offered for the current object, tag or material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelAppliesTo {
    AnyObject,
    Camera,
    Light,
    Morph,
    Tag,
    Material,
}

/// How many values a channel owns and how they are sampled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelKind {
    Scalar,
    Vector3,
    Quaternion,
}

impl ChannelKind {
    pub fn component_count(self) -> usize {
        match self {
            Self::Scalar => 1,
            Self::Vector3 => 3,
            Self::Quaternion => 4,
        }
    }
}

/// The single source of truth for what can be animated, how many components it
/// has, and whether it belongs to an object, a tag or a material. The recorder
/// and the F-curve editor both resolve channels through this registry instead of
/// matching ad-hoc track names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimChannel {
    Translation,
    Rotation,
    Scale,
    Fov,
    LightIntensity,
    LightColor,
    CopyLocationWeight,
    MorphWeight,
    MaterialRoughness,
    MaterialMetallic,
}

impl AnimChannel {
    pub const ALL: [Self; 10] = [
        Self::Translation,
        Self::Rotation,
        Self::Scale,
        Self::Fov,
        Self::LightIntensity,
        Self::LightColor,
        Self::CopyLocationWeight,
        Self::MorphWeight,
        Self::MaterialRoughness,
        Self::MaterialMetallic,
    ];

    /// Channels the transform recorder writes, in registry order.
    pub const TRANSFORMS: [Self; 3] = [Self::Translation, Self::Rotation, Self::Scale];

    pub fn label(self) -> &'static str {
        match self {
            Self::Translation => "Location",
            Self::Rotation => "Rotation",
            Self::Scale => "Scale",
            Self::Fov => "Camera FOV",
            Self::LightIntensity => "Light Intensity",
            Self::LightColor => "Light Color",
            Self::CopyLocationWeight => "Copy Location Weight",
            Self::MorphWeight => "Morph Weight",
            Self::MaterialRoughness => "Material Roughness",
            Self::MaterialMetallic => "Material Metallic",
        }
    }

    pub fn kind(self) -> ChannelKind {
        match self {
            Self::Translation | Self::Scale | Self::LightColor => ChannelKind::Vector3,
            Self::Rotation => ChannelKind::Quaternion,
            _ => ChannelKind::Scalar,
        }
    }

    pub fn component_count(self) -> usize {
        self.kind().component_count()
    }

    pub fn scope(self) -> ChannelScope {
        match self {
            Self::CopyLocationWeight => ChannelScope::Tag,
            Self::MaterialRoughness | Self::MaterialMetallic => ChannelScope::Material,
            _ => ChannelScope::Object,
        }
    }

    pub fn applies_to(self) -> ChannelAppliesTo {
        match self {
            Self::Translation | Self::Rotation | Self::Scale => ChannelAppliesTo::AnyObject,
            Self::Fov => ChannelAppliesTo::Camera,
            Self::LightIntensity | Self::LightColor => ChannelAppliesTo::Light,
            Self::MorphWeight => ChannelAppliesTo::Morph,
            Self::CopyLocationWeight => ChannelAppliesTo::Tag,
            Self::MaterialRoughness | Self::MaterialMetallic => ChannelAppliesTo::Material,
        }
    }

    /// Scalar tracks backing this channel. Rotation has none: it owns a
    /// quaternion track.
    pub fn scalar_properties(self) -> &'static [AnimProperty] {
        match self {
            Self::Translation => &[
                AnimProperty::TranslationX,
                AnimProperty::TranslationY,
                AnimProperty::TranslationZ,
            ],
            Self::Scale => &[
                AnimProperty::ScaleX,
                AnimProperty::ScaleY,
                AnimProperty::ScaleZ,
            ],
            Self::LightColor => &[
                AnimProperty::LightColorR,
                AnimProperty::LightColorG,
                AnimProperty::LightColorB,
            ],
            Self::Fov => &[AnimProperty::Fov],
            Self::LightIntensity => &[AnimProperty::LightIntensity],
            Self::CopyLocationWeight => &[AnimProperty::CopyLocationWeight],
            Self::MaterialRoughness => &[AnimProperty::MaterialRoughness],
            Self::MaterialMetallic => &[AnimProperty::MaterialMetallic],
            Self::Rotation | Self::MorphWeight => &[],
        }
    }

    pub fn from_scalar_property(property: AnimProperty) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|channel| channel.scalar_properties().contains(&property))
    }

    /// Human label for a tracked scalar component, derived from its registry
    /// channel (for example "Location X" or "Light Color R").
    pub fn scalar_label(property: AnimProperty) -> Option<String> {
        let channel = Self::from_scalar_property(property)?;
        let index = channel
            .scalar_properties()
            .iter()
            .position(|candidate| *candidate == property)?;
        let suffix = match channel {
            Self::LightColor => [" R", " G", " B"][index],
            _ if channel.component_count() > 1 => [" X", " Y", " Z"][index],
            _ => "",
        };
        Some(format!("{}{}", channel.label(), suffix))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimationTrack {
    pub property: AnimProperty,
    pub curve: Curve,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AnimationClip {
    pub tracks: Vec<AnimationTrack>,
    /// Quaternion rotation channel. Kept separate from the scalar tracks so its
    /// four components are sampled together with shortest-path alignment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<RotationTrack>,
    /// Per-morph-target weight curves. Target indices match `MorphSet.targets`;
    /// a target without a curve keeps its authored weight.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub morph_weights: BTreeMap<u32, Curve>,
}

impl AnimationClip {
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
            && self.rotation.as_ref().is_none_or(RotationTrack::is_empty)
            && self.morph_weights.is_empty()
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

    pub fn set_rotation_key(
        &mut self,
        time: f32,
        value: Quat,
        interpolation: Interpolation,
    ) -> usize {
        self.rotation
            .get_or_insert_with(RotationTrack::default)
            .insert(time, value, interpolation)
    }

    pub fn remove_rotation_key(&mut self, time: f32) {
        if let Some(track) = self.rotation.as_mut() {
            track.remove_nearest(time);
            if track.is_empty() {
                self.rotation = None;
            }
        }
    }

    pub fn remove_rotation_track(&mut self) {
        self.rotation = None;
    }

    /// Record a morph-target weight key. Weights are stored without clamping
    /// so the curve editor keeps the authored value; evaluation and export
    /// clamp to 0..=1.
    pub fn set_morph_weight_key(
        &mut self,
        target: u32,
        time: f32,
        value: f32,
        interpolation: Interpolation,
    ) {
        self.morph_weights
            .entry(target)
            .or_default()
            .insert(time, value, interpolation);
    }

    pub fn remove_morph_weight_key(&mut self, target: u32, time: f32) {
        if let Some(curve) = self.morph_weights.get_mut(&target) {
            curve.remove_nearest(time);
        }
        self.morph_weights.retain(|_, curve| !curve.keys.is_empty());
    }

    pub fn remove_morph_weight_track(&mut self, target: u32) {
        self.morph_weights.remove(&target);
    }

    pub fn sample_morph_weight(&self, target: u32, time: f32) -> Option<f32> {
        self.morph_weights
            .get(&target)
            .map(|curve| curve.sample(time))
    }

    pub fn rotation_key_index(&self, time: f32) -> Option<usize> {
        self.rotation
            .as_ref()
            .and_then(|track| track.key_index(time))
    }

    pub fn sample_rotation(&self, time: f32) -> Option<Quat> {
        self.rotation.as_ref().and_then(|track| track.sample(time))
    }

    /// Record one scalar-component channel from registry values. The number of
    /// values must match the channel's declared component count.
    pub fn record_channel(
        &mut self,
        channel: AnimChannel,
        values: &[f32],
        time: f32,
        interpolation: Interpolation,
    ) -> bool {
        let properties = channel.scalar_properties();
        if properties.is_empty() || properties.len() != values.len() {
            return false;
        }
        for (property, value) in properties.iter().zip(values) {
            self.set_key(*property, time, *value, interpolation);
        }
        true
    }

    /// Record a transform through the channel registry. The registry decides
    /// which channels exist, their component counts and whether they need a
    /// quaternion track or scalar curves.
    pub fn record_transform(
        &mut self,
        transform: &Transform,
        time: f32,
        interpolation: Interpolation,
    ) {
        for channel in AnimChannel::TRANSFORMS {
            match channel {
                AnimChannel::Rotation => {
                    self.set_rotation_key(time, transform.rotation, interpolation);
                }
                AnimChannel::Translation => {
                    let values = [
                        transform.translation.x,
                        transform.translation.y,
                        transform.translation.z,
                    ];
                    for (property, value) in channel.scalar_properties().iter().zip(values) {
                        self.set_key(*property, time, value, interpolation);
                    }
                }
                AnimChannel::Scale => {
                    let values = [transform.scale.x, transform.scale.y, transform.scale.z];
                    for (property, value) in channel.scalar_properties().iter().zip(values) {
                        self.set_key(*property, time, value, interpolation);
                    }
                }
                _ => {}
            }
        }
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
        if let Some(rotation) = self.sample_rotation(time) {
            t.rotation = rotation;
        } else if self.sample(AnimProperty::RotationX, time).is_some()
            || self.sample(AnimProperty::RotationY, time).is_some()
            || self.sample(AnimProperty::RotationZ, time).is_some()
        {
            // Legacy documents store rotation as three Euler curves. They stay
            // readable, but new recordings use the quaternion track above.
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
        if let Some(rotation) = &self.rotation {
            times.extend(rotation.keys.iter().map(|key| key.time));
        }
        times.extend(
            self.morph_weights
                .values()
                .flat_map(|curve| curve.keys.iter().map(|key| key.time)),
        );
        times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        times.dedup_by(|a, b| (*a - *b).abs() < 1e-5);
        times
    }

    pub fn duration(&self) -> f32 {
        self.key_times().last().copied().unwrap_or(0.0)
    }

    /// Retime every key on every channel in this clip around `pivot`, including
    /// scalar, morph-weight and quaternion rotation channels. Whole-take
    /// retiming passes `pivot = 0.0`, so the take origin stays fixed and every
    /// key scales away from zero (a first key already at zero does not move).
    ///
    /// This is atomic: an invalid factor, a collision or a negative implied time
    /// leaves the clip untouched. Manual scalar tangents are rescaled by their
    /// incident segment durations; auto tangents are re-derived; quaternion
    /// tangents are implicit in sampling and therefore follow the moved times.
    pub fn retime_all(&mut self, pivot: f32, factor: f32) -> bool {
        if !valid_retime(pivot, factor) || (factor - 1.0).abs() <= f32::EPSILON || self.is_empty() {
            return false;
        }
        let mut after = self.clone();
        let mut saw_curve = false;
        for track in &mut after.tracks {
            if track.curve.is_empty() {
                continue;
            }
            saw_curve = true;
            let times: Vec<f32> = track.curve.keys.iter().map(|key| key.time).collect();
            if times.iter().all(|time| (*time - pivot).abs() <= 1e-6) {
                continue;
            }
            if !track.curve.retime_all_keys(pivot, factor) {
                return false;
            }
        }
        if let Some(track) = after.rotation.as_mut() {
            if !track.is_empty() {
                saw_curve = true;
                let times: Vec<f32> = track.keys.iter().map(|key| key.time).collect();
                if !times.iter().all(|time| (*time - pivot).abs() <= 1e-6)
                    && !track.retime_all_keys(pivot, factor)
                {
                    return false;
                }
            }
        }
        for curve in after.morph_weights.values_mut() {
            if curve.is_empty() {
                continue;
            }
            saw_curve = true;
            let times: Vec<f32> = curve.keys.iter().map(|key| key.time).collect();
            if times.iter().all(|time| (*time - pivot).abs() <= 1e-6) {
                continue;
            }
            if !curve.retime_all_keys(pivot, factor) {
                return false;
            }
        }
        if !saw_curve {
            return false;
        }
        *self = after;
        true
    }

    pub fn signature(&self) -> u64 {
        let mut h = StableHasher::new();
        for t in &self.tracks {
            h.write_u32(t.property as u32);
            h.write_u64(t.curve.signature());
        }
        if let Some(rotation) = &self.rotation {
            h.write_u8(0xA7);
            for key in &rotation.keys {
                h.write_f32(key.time);
                h.write_f32(key.value.x);
                h.write_f32(key.value.y);
                h.write_f32(key.value.z);
                h.write_f32(key.value.w);
                h.write_u8(match key.interpolation {
                    Interpolation::Constant => 0,
                    Interpolation::Linear => 1,
                    Interpolation::Smooth => 2,
                    Interpolation::Spline => 3,
                });
                h.write_u8(match key.tangent_in_type {
                    TangentType::Constant => 0,
                    TangentType::Linear => 1,
                    TangentType::Smooth => 2,
                    TangentType::Spline => 3,
                });
                h.write_u8(match key.tangent_out_type {
                    TangentType::Constant => 0,
                    TangentType::Linear => 1,
                    TangentType::Smooth => 2,
                    TangentType::Spline => 3,
                });
            }
        }
        for (target, curve) in &self.morph_weights {
            h.write_u8(0xB3);
            h.write_u32(*target);
            h.write_u64(curve.signature());
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
    fn legacy_euler_curves_use_degrees_alongside_the_base_rotation() {
        // Old documents key rotation as three degree-valued Euler curves. A
        // partially keyed clip must keep the other authored axes in the same
        // unit instead of falling back to radians.
        let mut clip = AnimationClip::default();
        clip.set_key(AnimProperty::RotationY, 0.0, 90.0, Interpolation::Linear);
        let base = Transform::from_euler_degrees(Vec3::new(30.0, 0.0, 0.0));
        let applied = clip.apply_transform(base, 0.0).euler_degrees();
        assert!((applied.x - 30.0).abs() < 1e-3, "x {applied:?}");
        assert!((applied.y - 90.0).abs() < 1e-3, "y {applied:?}");
        assert!(applied.z.abs() < 1e-3, "z {applied:?}");
    }

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
        c.insert(0.0, 2.0, Interpolation::Constant);
        c.insert(1.0, 9.0, Interpolation::Constant);
        assert!((c.sample(0.9) - 2.0).abs() < 1e-6);
        assert!((c.sample(1.0) - 9.0).abs() < 1e-6);
    }

    #[test]
    fn bezier_passes_through_keys_and_is_smooth() {
        let mut c = Curve::new();
        c.insert(0.0, 0.0, Interpolation::Smooth);
        c.insert(1.0, 1.0, Interpolation::Smooth);
        c.insert(2.0, 0.0, Interpolation::Smooth);
        assert!((c.sample(0.0) - 0.0).abs() < 1e-5);
        assert!((c.sample(1.0) - 1.0).abs() < 1e-5);
        assert!((c.sample(2.0) - 0.0).abs() < 1e-5);
        assert!(c.sample(0.5) > 0.0 && c.sample(0.5) < 1.0);
    }

    #[test]
    fn interpolation_types_resample_to_distinct_curve_shapes() {
        let mut constant = Curve::new();
        let mut linear = Curve::new();
        let mut smooth = Curve::new();
        let mut spline = Curve::new();
        let mut spline_auto = Curve::new();
        for (time, value) in [(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)] {
            constant.insert(time, value, Interpolation::Constant);
            linear.insert(time, value, Interpolation::Linear);
            smooth.insert(time, value, Interpolation::Smooth);
            spline.insert(time, value, Interpolation::Spline);
            spline_auto.insert(time, value, Interpolation::Spline);
        }
        // Spline is the editable Bezier form. Give it a deliberately authored
        // shape so it differs from Smooth's automatic Catmull-Rom tangents.
        assert!(spline.set_manual_tangents(0.0, 0.0, 2.0));
        assert!(spline.set_manual_tangents(1.0, 2.0, -2.0));
        assert!(spline.set_manual_tangents(2.0, -2.0, 0.0));

        // These are sampled between keys, not merely stored field checks.
        assert!((constant.sample(0.25) - 0.0).abs() < 1e-5);
        assert!((linear.sample(0.25) - 0.25).abs() < 1e-5);
        assert!((smooth.sample(0.25) - 0.296875).abs() < 1e-4);
        assert!((spline.sample(0.25) - 0.34375).abs() < 1e-4);
        assert!(
            (spline_auto.sample(0.25) - smooth.sample(0.25)).abs() > 1e-3,
            "an untouched Spline must not collapse into Smooth"
        );
        assert!((constant.sample(1.5) - 1.0).abs() < 1e-5);
        assert!((linear.sample(1.5) - 0.5).abs() < 1e-5);
        assert!((smooth.sample(1.5) - 0.625).abs() < 1e-4);
        assert!((spline.sample(1.5) - 0.5).abs() < 1e-4);
        let quarter_points = [
            constant.sample(0.25),
            linear.sample(0.25),
            smooth.sample(0.25),
            spline.sample(0.25),
        ];
        for (index, left) in quarter_points.iter().enumerate() {
            for right in &quarter_points[index + 1..] {
                assert!(
                    (left - right).abs() > 1e-3,
                    "{left} and {right} must not share one implicit curve"
                );
            }
        }
    }

    #[test]
    fn interpolation_changes_rederive_auto_tangents_but_spline_keeps_manual_slopes() {
        let mut curve = Curve::new();
        curve.insert(0.0, 0.0, Interpolation::Linear);
        curve.insert(1.0, 1.0, Interpolation::Linear);
        curve.insert(2.0, 0.0, Interpolation::Linear);
        assert!(curve.set_manual_tangents(0.0, 0.0, 2.0));

        assert!(curve.set_interpolation(0.0, Interpolation::Spline));
        assert!(curve.keys[0].tangent_manual);
        assert!((curve.keys[0].tangent_out - 2.0).abs() < 1e-5);
        assert_eq!(curve.keys[0].tangent_out_type, TangentType::Spline);

        // Smooth owns automatic tangents, so changing a key to Smooth redefines
        // its hand-authored slopes rather than silently keeping them.
        assert!(curve.set_interpolation(1.0, Interpolation::Smooth));
        assert!(!curve.keys[1].tangent_manual);
        assert!((curve.keys[1].tangent_in).abs() < 1e-5);
        assert_eq!(curve.keys[1].tangent_in_type, TangentType::Smooth);

        // Re-selecting Smooth is a real edit when the key became manual again;
        // it must clear the flag rather than silently keeping stale slopes.
        assert!(curve.set_manual_tangents(1.0, 1.0, -1.0));
        assert!(curve.set_interpolation(1.0, Interpolation::Smooth));
        assert!(!curve.keys[1].tangent_manual);
        assert_eq!(curve.keys[1].tangent_in_type, TangentType::Smooth);

        // A non-Spline tangent type redefines that side with the linear secant.
        let before_tangent_type = curve.sample(0.5);
        assert!(curve.set_tangent_type(0.0, TangentType::Linear, true, true));
        assert!(!curve.keys[0].tangent_manual);
        assert!((curve.keys[0].tangent_out - 1.0).abs() < 1e-5);
        assert_eq!(curve.keys[0].tangent_out_type, TangentType::Linear);
        assert!(
            (curve.sample(0.5) - before_tangent_type).abs() > 1e-3,
            "the tangent type must change the sampled curve, not just metadata"
        );
    }

    #[test]
    fn retiming_keeps_interpolation_and_tangent_type_meaning() {
        let mut constant = Curve::new();
        constant.insert(0.0, 2.0, Interpolation::Constant);
        constant.insert(1.0, 9.0, Interpolation::Constant);
        assert!(constant.set_tangent_type(0.0, TangentType::Linear, true, true));
        let mut linear = Curve::new();
        linear.insert(0.0, 2.0, Interpolation::Linear);
        linear.insert(2.0, 8.0, Interpolation::Linear);
        assert!(constant.retime_all_keys(0.0, 2.0));
        assert!(linear.retime_all_keys(0.0, 2.0));

        assert_eq!(constant.keys[0].interpolation, Interpolation::Constant);
        assert_eq!(constant.keys[0].tangent_out_type, TangentType::Linear);
        assert!((constant.sample(1.0) - 2.0).abs() < 1e-5);
        assert!((constant.sample(3.9) - 9.0).abs() < 1e-5);
        assert!((constant.sample(4.0) - 9.0).abs() < 1e-5);
        assert_eq!(linear.keys[0].interpolation, Interpolation::Linear);
        assert_eq!(linear.keys[1].tangent_out_type, TangentType::Linear);
        assert!((linear.sample(2.0) - 5.0).abs() < 1e-5);
        assert!((linear.sample(4.0) - 8.0).abs() < 1e-5);
    }

    #[test]
    fn manual_tangents_override_auto_and_survive_neighbour_edits() {
        let mut c = Curve::new();
        c.insert(0.0, 0.0, Interpolation::Smooth);
        c.insert(1.0, 1.0, Interpolation::Smooth);
        assert!(c.set_manual_tangents(0.0, 0.0, 2.0));
        assert!(c.set_manual_tangents(1.0, 0.0, 0.0));
        assert!((c.sample(0.5) - 0.75).abs() < 1e-4);
        let signature = c.signature();
        c.keys[0].tangent_out = 3.0;
        assert_ne!(c.signature(), signature);

        c.insert(2.0, 0.0, Interpolation::Smooth);
        assert!(c.keys[0].tangent_manual);
        assert!((c.keys[0].tangent_out - 3.0).abs() < 1e-5);
        assert!((c.sample(0.5) - 0.875).abs() < 1e-4);

        // Re-recording a key returns it to automatic tangents.
        c.insert(0.0, 0.0, Interpolation::Smooth);
        assert!(!c.keys[0].tangent_manual);
    }

    #[test]
    fn retime_selected_keys_preserves_manual_shape_and_rescales_slopes() {
        let mut curve = Curve::new();
        curve.insert(0.0, 0.0, Interpolation::Smooth);
        curve.insert(1.0, 1.0, Interpolation::Smooth);
        curve.insert(2.0, 0.0, Interpolation::Smooth);
        assert!(curve.set_manual_tangents(0.0, 0.0, 1.0));
        assert!(curve.set_manual_tangents(1.0, 1.0, -1.0));
        assert!(curve.set_manual_tangents(2.0, -1.0, 0.0));
        let half = curve.sample(0.5);
        let one_and_a_half = curve.sample(1.5);

        assert!(curve.retime_selected_keys(0.0, 2.0, &[0.0, 1.0, 2.0]));
        assert_eq!(curve.keys.len(), 3);
        assert!((curve.keys[0].time - 0.0).abs() < 1e-6);
        assert!((curve.keys[1].time - 2.0).abs() < 1e-6);
        assert!((curve.keys[2].time - 4.0).abs() < 1e-6);
        assert!(curve.keys.iter().all(|key| key.tangent_manual));
        assert!((curve.keys[0].tangent_out - 0.5).abs() < 1e-6);
        assert!((curve.keys[1].tangent_in - 0.5).abs() < 1e-6);
        assert!((curve.keys[1].tangent_out + 0.5).abs() < 1e-6);
        assert!((curve.keys[2].tangent_in + 0.5).abs() < 1e-6);
        assert!((curve.sample(1.0) - half).abs() < 1e-4);
        assert!((curve.sample(3.0) - one_and_a_half).abs() < 1e-4);
    }

    #[test]
    fn retime_keeps_fractional_times_and_rejects_collisions_atomically() {
        let mut curve = Curve::new();
        curve.insert(0.25, 0.0, Interpolation::Smooth);
        curve.insert(0.75, 1.0, Interpolation::Smooth);
        let before_half = curve.sample(0.5);
        assert!(curve.retime_selected_keys(0.25, 2.0, &[0.25, 0.75]));
        assert!((curve.keys[0].time - 0.25).abs() < 1e-6);
        assert!((curve.keys[1].time - 1.25).abs() < 1e-6);
        assert!((curve.keys[1].time - curve.keys[1].time.round()).abs() > 0.1);
        assert!((curve.sample(0.75) - before_half).abs() < 1e-4);

        let mut collision = Curve::new();
        for (time, value) in [(0.0, 0.0), (1.0, 1.0), (2.0, 2.0), (3.0, 3.0)] {
            collision.insert(time, value, Interpolation::Smooth);
        }
        assert!(collision.set_manual_tangents(2.0, 1.0, 2.0));
        let before = collision.clone();
        assert!(!collision.retime_selected_keys(0.0, 1.5, &[0.0, 2.0]));
        assert_eq!(collision, before);
        assert!(!collision.retime_selected_keys(0.0, 0.0, &[0.0]));
        assert!(!collision.retime_selected_keys(0.0, -1.0, &[0.0]));
    }

    #[test]
    fn retime_rederives_auto_tangents_without_changing_curve_shape() {
        let mut curve = Curve::new();
        curve.insert(0.0, 0.0, Interpolation::Smooth);
        curve.insert(1.0, 1.0, Interpolation::Smooth);
        curve.insert(2.0, 0.0, Interpolation::Smooth);
        let old_half = curve.sample(0.5);
        let old_three_halves = curve.sample(1.5);
        assert!(curve.retime_selected_keys(0.0, 2.0, &[0.0, 1.0, 2.0]));
        assert!((curve.sample(1.0) - old_half).abs() < 1e-4);
        assert!((curve.sample(3.0) - old_three_halves).abs() < 1e-4);
        assert!(!curve.keys.iter().any(|key| key.tangent_manual));
    }

    #[test]
    fn retime_rescales_manual_tangents_across_the_selection_boundary() {
        let mut curve = Curve::new();
        curve.insert(0.0, 0.0, Interpolation::Smooth);
        curve.insert(1.0, 1.0, Interpolation::Smooth);
        curve.insert(4.0, 0.0, Interpolation::Smooth);
        assert!(curve.set_manual_tangents(1.0, 0.0, -1.0));
        assert!(curve.set_manual_tangents(4.0, 1.0, 0.0));
        let old_mid = curve.sample(2.5);

        // Only the last key moves. Its unselected neighbour still has a manual
        // slope on the same stretched segment, so that slope must be rescaled
        // too or the segment silently changes shape.
        assert!(curve.retime_selected_keys(0.0, 2.0, &[0.0, 4.0]));
        assert!((curve.keys[1].time - 1.0).abs() < 1e-6);
        assert!((curve.keys[2].time - 8.0).abs() < 1e-6);
        assert!(curve.keys[1].tangent_manual);
        assert!((curve.keys[1].tangent_out + 3.0 / 7.0).abs() < 1e-5);
        assert!((curve.keys[2].tangent_in - 3.0 / 7.0).abs() < 1e-5);
        assert!((curve.sample(4.5) - old_mid).abs() < 1e-4);
    }

    #[test]
    fn retime_all_moves_rotation_morph_and_scalar_channels_together() {
        let mut clip = AnimationClip::default();
        clip.set_key(AnimProperty::TranslationX, 0.0, 0.0, Interpolation::Smooth);
        clip.set_key(AnimProperty::TranslationX, 1.0, 1.0, Interpolation::Smooth);
        clip.set_morph_weight_key(0, 0.0, 0.0, Interpolation::Linear);
        clip.set_morph_weight_key(0, 2.0, 1.0, Interpolation::Linear);
        clip.set_rotation_key(0.0, Quat::IDENTITY, Interpolation::Linear);
        clip.set_rotation_key(
            2.0,
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            Interpolation::Linear,
        );
        let curve = &mut clip.track_mut(AnimProperty::TranslationX).unwrap().curve;
        assert!(curve.set_manual_tangents(0.0, 0.0, 1.0));
        let old_rotation_mid = clip.sample_rotation(1.0).unwrap();

        assert!(clip.retime_all(0.0, 2.0));
        let scalar = clip.track(AnimProperty::TranslationX).unwrap();
        assert!((scalar.curve.keys[1].time - 2.0).abs() < 1e-6);
        assert!((scalar.curve.keys[0].tangent_out - 0.5).abs() < 1e-6);
        assert!((clip.sample_morph_weight(0, 2.0).unwrap() - 0.5).abs() < 1e-5);
        assert!(clip.rotation.as_ref().unwrap().keys[1].time == 4.0);
        let new_rotation_mid = clip.sample_rotation(2.0).unwrap();
        assert!((new_rotation_mid.dot(old_rotation_mid).abs() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn inserting_out_of_order_replaces_duplicate_time() {
        let mut c = Curve::new();
        c.insert(2.0, 2.0, Interpolation::Linear);
        c.insert(0.0, 0.0, Interpolation::Linear);
        c.insert(1.0, 1.0, Interpolation::Linear);
        c.insert(1.0, 5.0, Interpolation::Constant);
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
    fn morph_weight_curves_sample_and_extend_the_clip_range() {
        let mut clip = AnimationClip::default();
        assert!(clip.is_empty());
        clip.set_morph_weight_key(2, 0.0, 0.0, Interpolation::Linear);
        clip.set_morph_weight_key(2, 2.0, 1.0, Interpolation::Linear);
        assert!(!clip.is_empty());
        assert!((clip.sample_morph_weight(2, 1.0).unwrap() - 0.5).abs() < 1e-5);
        assert!((clip.duration() - 2.0).abs() < 1e-5);
        clip.remove_morph_weight_track(2);
        assert!(clip.is_empty());
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

    fn quat_angle(q: Quat) -> f32 {
        2.0 * q.w.abs().clamp(-1.0, 1.0).acos()
    }

    #[test]
    fn sign_opposed_quaternions_interpolate_without_a_flip() {
        let angle = 0.7f32;
        let q = Quat::from_rotation_y(angle);
        let negated = Quat::from_xyzw(-q.x, -q.y, -q.z, -q.w);
        for interpolation in [Interpolation::Linear, Interpolation::Smooth] {
            let mut clip = AnimationClip::default();
            clip.set_rotation_key(0.0, q, interpolation);
            clip.set_rotation_key(1.0, negated, interpolation);
            for time in [0.25, 0.5, 0.75] {
                let sampled = clip.sample_rotation(time).expect("rotation sample");
                // q and -q are the same orientation, so every sample must keep it.
                assert!(
                    sampled.dot(q).abs() > 0.9999,
                    "{interpolation:?} at {time} flipped to {sampled:?}"
                );
                assert!((sampled.length() - 1.0).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn multi_key_rotation_track_samples_non_key_frames() {
        let mut clip = AnimationClip::default();
        clip.set_rotation_key(0.0, Quat::IDENTITY, Interpolation::Linear);
        clip.set_rotation_key(
            1.0,
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            Interpolation::Linear,
        );
        clip.set_rotation_key(
            2.0,
            Quat::from_rotation_y(std::f32::consts::PI),
            Interpolation::Linear,
        );
        for (time, expected_degrees) in [(0.5, 45.0), (1.5, 135.0)] {
            let sampled = clip.sample_rotation(time).expect("rotation sample");
            assert!((sampled.length() - 1.0).abs() < 1e-4);
            let degrees = quat_angle(sampled).to_degrees();
            assert!(
                (degrees - expected_degrees).abs() < 0.5,
                "at {time}s expected {expected_degrees}°, got {degrees}°"
            );
        }
        // The track is continuous across both segments, not a repeated hold.
        let a = clip.sample_rotation(0.5).unwrap();
        let b = clip.sample_rotation(1.5).unwrap();
        assert!(a.dot(b).abs() < 0.9999);

        // STEP holds the left key instead of interpolating.
        let mut stepped = clip.clone();
        if let Some(track) = stepped.rotation.as_mut() {
            track.keys[0].interpolation = Interpolation::Constant;
        }
        let held = stepped.sample_rotation(0.5).unwrap();
        assert!(held.dot(Quat::IDENTITY).abs() > 0.9999);
    }

    #[test]
    fn rotation_tangent_types_change_the_cubic_derivation() {
        let mut smooth = AnimationClip::default();
        for (time, angle) in [(0.0, 0.0), (1.0, 0.4), (3.0, 0.8)] {
            smooth.set_rotation_key(time, Quat::from_rotation_y(angle), Interpolation::Smooth);
        }
        let smooth_half = quat_angle(smooth.sample_rotation(0.5).unwrap());

        let mut spline = smooth.clone();
        let track = spline.rotation.as_mut().unwrap();
        for key in &mut track.keys {
            key.interpolation = Interpolation::Spline;
            key.tangent_in_type = TangentType::Spline;
            key.tangent_out_type = TangentType::Spline;
        }
        let spline_half = quat_angle(spline.sample_rotation(0.5).unwrap());
        assert!(
            (spline_half - smooth_half).abs() > 1e-3,
            "Spline's secant fallback must differ from Smooth's Catmull-Rom control"
        );

        let mut flat = smooth.clone();
        let track = flat.rotation.as_mut().unwrap();
        assert!(track.keys[0].set_tangent_type(TangentType::Constant, false, true));
        let flat_half = quat_angle(flat.sample_rotation(0.5).unwrap());
        assert!(
            (flat_half - smooth_half).abs() > 1e-3,
            "a flat outgoing tangent must change the cubic"
        );
    }

    #[test]
    fn registry_declares_channel_scopes_and_component_counts() {
        assert_eq!(AnimChannel::Translation.component_count(), 3);
        assert_eq!(AnimChannel::Scale.component_count(), 3);
        assert_eq!(AnimChannel::Rotation.component_count(), 4);
        assert_eq!(AnimChannel::Fov.component_count(), 1);
        assert_eq!(AnimChannel::Rotation.kind(), ChannelKind::Quaternion);
        assert_eq!(AnimChannel::Translation.kind(), ChannelKind::Vector3);
        assert_eq!(AnimChannel::Fov.kind(), ChannelKind::Scalar);
        assert_eq!(AnimChannel::Translation.scope(), ChannelScope::Object);
        assert_eq!(AnimChannel::CopyLocationWeight.scope(), ChannelScope::Tag);
        assert_eq!(
            AnimChannel::MaterialRoughness.scope(),
            ChannelScope::Material
        );
        assert_eq!(AnimChannel::Translation.scalar_properties().len(), 3);
        assert!(AnimChannel::Rotation.scalar_properties().is_empty());
        assert_eq!(
            AnimChannel::from_scalar_property(AnimProperty::ScaleZ),
            Some(AnimChannel::Scale)
        );
        assert_eq!(
            AnimChannel::from_scalar_property(AnimProperty::MaterialMetallic),
            Some(AnimChannel::MaterialMetallic)
        );
    }

    #[test]
    fn recorded_transform_uses_the_quaternion_rotation_channel() {
        let mut clip = AnimationClip::default();
        let transform = Transform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::from_rotation_z(0.5),
            scale: Vec3::splat(2.0),
        };
        clip.record_transform(&transform, 0.25, Interpolation::Linear);
        let rotation = clip.rotation.as_ref().expect("quaternion track");
        assert_eq!(rotation.keys.len(), 1);
        assert!(rotation.keys[0].value.dot(transform.rotation).abs() > 0.9999);
        assert!(clip.track(AnimProperty::RotationX).is_none());
        assert_eq!(
            clip.track(AnimProperty::TranslationY).unwrap().curve.keys[0].value,
            2.0
        );
        let applied = clip.apply_transform(Transform::IDENTITY, 0.25);
        assert!(applied.rotation.dot(transform.rotation).abs() > 0.9999);
    }
}
