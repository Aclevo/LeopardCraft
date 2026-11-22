//! Animation curves, tracks, playback, and deformation helpers.
//!
//! All sampling is pure and deterministic: a clip sampled at the same time
//! always yields the same transform. The timeline UI only supplies time.

use glam::{Mat4, Quat, Vec3};
use lc_core::{StableHasher, Transform};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interpolation {
    Linear,
    Step,
    /// Cubic Hermite with automatically derived tangents (smooth F-curve).
    #[default]
    Bezier,
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

/// Tolerance used to identify one key by its time in the editor.
pub const KEY_TIME_EPSILON: f32 = 1e-3;

fn unit_quaternion(q: Quat) -> Quat {
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
}

impl RotationKey {
    pub fn new(time: f32, value: Quat, interpolation: Interpolation) -> Self {
        Self {
            time,
            value: unit_quaternion(value),
            interpolation,
        }
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

    /// Sample the track. LINEAR uses shortest-path slerp, STEP holds the left
    /// key, and smooth interpolation runs a component-wise cubic Hermite with
    /// hemisphere alignment and a final normalisation (the same rule glTF uses
    /// for CUBICSPLINE rotation).
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
            Interpolation::Step => Some(unit_quaternion(k0.value)),
            Interpolation::Linear => Some(slerp_shortest(k0.value, k1.value, t)),
            Interpolation::Bezier => {
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
                    let m0 = if has_prev {
                        (b[i] - p[i]) / (dt + dt_prev)
                    } else {
                        (b[i] - a[i]) / dt
                    };
                    let m1 = if has_next {
                        (n[i] - a[i]) / (dt + dt_next)
                    } else {
                        (b[i] - a[i]) / dt
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
    MaterialRoughness,
    MaterialMetallic,
}

impl AnimChannel {
    pub const ALL: [Self; 9] = [
        Self::Translation,
        Self::Rotation,
        Self::Scale,
        Self::Fov,
        Self::LightIntensity,
        Self::LightColor,
        Self::CopyLocationWeight,
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
            Self::Rotation => &[],
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
}

impl AnimationClip {
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty() && self.rotation.as_ref().is_none_or(RotationTrack::is_empty)
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
        if let Some(rotation) = &self.rotation {
            h.write_u8(0xA7);
            for key in &rotation.keys {
                h.write_f32(key.time);
                h.write_f32(key.value.x);
                h.write_f32(key.value.y);
                h.write_f32(key.value.z);
                h.write_f32(key.value.w);
                h.write_u8(match key.interpolation {
                    Interpolation::Linear => 0,
                    Interpolation::Step => 1,
                    Interpolation::Bezier => 2,
                });
            }
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

    fn quat_angle(q: Quat) -> f32 {
        2.0 * q.w.abs().clamp(-1.0, 1.0).acos()
    }

    #[test]
    fn sign_opposed_quaternions_interpolate_without_a_flip() {
        let angle = 0.7f32;
        let q = Quat::from_rotation_y(angle);
        let negated = Quat::from_xyzw(-q.x, -q.y, -q.z, -q.w);
        for interpolation in [Interpolation::Linear, Interpolation::Bezier] {
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
            track.keys[0].interpolation = Interpolation::Step;
        }
        let held = stepped.sample_rotation(0.5).unwrap();
        assert!(held.dot(Quat::IDENTITY).abs() > 0.9999);
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
