//! Inverse kinematics for joint chains.
//!
//! A chain is attached to the selected tip joint. `chain_length` counts the
//! consecutive movable Joint ancestors above that tip; the tip's origin is the
//! end effector. Exactly two movable joints use a closed-form two-bone solve
//! with an optional pole. Longer (and shorter) chains use damped cyclic
//! coordinate descent. Neither solver stretches bones: unreachable targets are
//! clamped to the chain's reach.
//!
//! A chain may also name a Spline node instead of relying on its point target.
//! Spline IK samples that curve into a world-space polyline and aims each bone
//! at a point one bone length further along it, so the chain hugs the curve as
//! a chain of chords without stretching; see [`solve_spline_chain`].

use crate::{Document, NodeId, NodeKind, Scene};
use glam::{Mat4, Quat, Vec3};
use lc_anim::{AnimationClip, ChannelTarget, Interpolation};
use lc_core::Transform;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const EPSILON: f32 = 1e-6;
const MAX_ITERATIONS: usize = 128;
const DEFAULT_CHAIN_LENGTH: usize = 2;
const DEFAULT_ITERATIONS: usize = 24;
const DEFAULT_TOLERANCE: f32 = 1e-3;
const DEFAULT_DAMPING: f32 = 1.0;
const DEFAULT_STIFFNESS: f32 = 1.0;
const DEFAULT_LIMIT_DEGREES: f32 = 180.0;
const MAX_LIMIT_DEGREES: f32 = 360.0;

/// Clean-room per-joint rotation limits in local XYZ Euler degrees.
///
/// Cheetah3D documents rotation limits through an IK Constraint tag with
/// heading/pitch/bank coordinates; this is a LeopardCraft approximation, not a
/// claim about Cheetah3D's tag storage or angle convention. A zero-width axis
/// (`min == max`) locks that axis. Limits are applied to direct joint rotation
/// as well as to both IK solvers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JointLimits {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_limit_min")]
    pub min: Vec3,
    #[serde(default = "default_limit_max")]
    pub max: Vec3,
}

fn default_limit_min() -> Vec3 {
    Vec3::splat(-DEFAULT_LIMIT_DEGREES)
}

fn default_limit_max() -> Vec3 {
    Vec3::splat(DEFAULT_LIMIT_DEGREES)
}

impl Default for JointLimits {
    fn default() -> Self {
        Self {
            enabled: true,
            min: default_limit_min(),
            max: default_limit_max(),
        }
    }
}

fn rotation_euler_degrees(rotation: Quat) -> Vec3 {
    Transform::new(Vec3::ZERO, rotation, Vec3::ONE).euler_degrees()
}

impl JointLimits {
    /// Keep all bounds finite, ordered per axis, and in a range where the local
    /// Euler representation remains useful. Reversed bounds are swapped.
    pub fn sanitized(&self) -> Self {
        let finite = |value: f32, fallback: f32| {
            if value.is_finite() {
                value.clamp(-MAX_LIMIT_DEGREES, MAX_LIMIT_DEGREES)
            } else {
                fallback
            }
        };
        let mut min = Vec3::new(
            finite(self.min.x, -DEFAULT_LIMIT_DEGREES),
            finite(self.min.y, -DEFAULT_LIMIT_DEGREES),
            finite(self.min.z, -DEFAULT_LIMIT_DEGREES),
        );
        let mut max = Vec3::new(
            finite(self.max.x, DEFAULT_LIMIT_DEGREES),
            finite(self.max.y, DEFAULT_LIMIT_DEGREES),
            finite(self.max.z, DEFAULT_LIMIT_DEGREES),
        );
        for axis in 0..3 {
            if min[axis] > max[axis] {
                std::mem::swap(&mut min[axis], &mut max[axis]);
            }
        }
        Self {
            enabled: self.enabled,
            min,
            max,
        }
    }

    /// Clamp a local rotation to the limits. Disabled limits leave the rotation
    /// untouched; a NaN input becomes identity rather than leaking non-finite
    /// data into the pose.
    pub fn clamp_rotation(&self, rotation: Quat) -> Quat {
        if !self.enabled {
            return rotation;
        }
        if !rotation.is_finite() {
            return Quat::IDENTITY;
        }
        let limits = self.sanitized();
        let euler = rotation_euler_degrees(rotation);
        if !euler.is_finite() {
            return Quat::IDENTITY;
        }
        let clamped = Vec3::new(
            euler.x.clamp(limits.min.x, limits.max.x),
            euler.y.clamp(limits.min.y, limits.max.y),
            euler.z.clamp(limits.min.z, limits.max.z),
        );
        Transform::from_euler_degrees(clamped).rotation
    }
}

/// Apply a Joint's limits to a local transform. Non-joints and joints without
/// limits are unchanged. Returns whether the rotation was clamped.
pub(crate) fn clamp_joint_local(scene: &Scene, id: NodeId, local: &mut Transform) -> bool {
    let Some(node) = scene.get(id) else {
        return false;
    };
    if !matches!(&node.kind, NodeKind::Joint { .. }) {
        return false;
    }
    let Some(limits) = node.rotation_limits.as_ref() else {
        return false;
    };
    let before = local.rotation;
    local.rotation = limits.clamp_rotation(local.rotation);
    local.rotation != before
}

fn clamp_chain_locals(scene: &Scene, joints: &[NodeId], locals: &mut BTreeMap<NodeId, Transform>) {
    for id in joints {
        if let Some(local) = locals.get_mut(id) {
            clamp_joint_local(scene, *id, local);
        }
    }
}

/// Serializable IK settings stored on the tip joint.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IkChain {
    /// Node whose world-space origin is the goal.
    pub target: NodeId,
    /// Optional node that chooses the bend plane for a two-bone chain and acts
    /// as a preferred plane for longer chains.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pole: Option<NodeId>,
    /// Number of movable Joint ancestors above the tip.
    #[serde(default = "default_chain_length")]
    pub chain_length: usize,
    /// Maximum CCD passes. The analytic two-bone path uses one pass.
    #[serde(default = "default_iterations")]
    pub iterations: usize,
    /// Distance at which the solve is considered converged.
    #[serde(default = "default_tolerance")]
    pub tolerance: f32,
    /// Fraction of each CCD correction applied per pass. 1 is a full step.
    #[serde(default = "default_damping")]
    pub damping: f32,
    /// Optional Spline node whose curve drives the chain in place of the point
    /// target. A missing or non-spline node falls back to the point target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spline: Option<NodeId>,
    /// Fraction of each solver correction applied per pass. 1 is a full solve,
    /// 0 leaves the rest pose untouched and values between land in the middle.
    #[serde(
        default = "default_stiffness",
        skip_serializing_if = "stiffness_is_default"
    )]
    pub stiffness: f32,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_chain_length() -> usize {
    DEFAULT_CHAIN_LENGTH
}

fn default_iterations() -> usize {
    DEFAULT_ITERATIONS
}

fn default_tolerance() -> f32 {
    DEFAULT_TOLERANCE
}

fn default_damping() -> f32 {
    DEFAULT_DAMPING
}

fn default_stiffness() -> f32 {
    DEFAULT_STIFFNESS
}

fn stiffness_is_default(stiffness: &f32) -> bool {
    (*stiffness - DEFAULT_STIFFNESS).abs() <= f32::EPSILON
}

/// Blend every solved joint back toward its pre-solve rotation by the chain's
/// stiffness. Doing this once, after the solve, makes stiffness mean the same
/// thing on the analytic, CCD and spline paths: 1 keeps the full solve, 0 keeps
/// the rest pose and values between land partway. Limits are re-clamped after
/// the blend so a soft solve never escapes them.
fn soften_pose(
    scene: &Scene,
    chain: &IkChain,
    base: &[(NodeId, Transform)],
    locals: &mut BTreeMap<NodeId, Transform>,
) {
    let stiffness = chain.stiffness.clamp(0.0, 1.0);
    if !stiffness.is_finite() || stiffness >= 1.0 {
        return;
    }
    for (id, base) in base {
        if let Some(local) = locals.get_mut(id) {
            local.rotation = base.rotation.slerp(local.rotation, stiffness);
            let _ = clamp_joint_local(scene, *id, local);
        }
    }
}

/// Local rotations of `joints` as they are before a solve, so a soft solve can
/// blend back toward the authored pose.
fn pre_solve_rotations(
    joints: &[NodeId],
    locals: &BTreeMap<NodeId, Transform>,
) -> Vec<(NodeId, Transform)> {
    joints
        .iter()
        .filter_map(|id| locals.get(id).map(|local| (*id, *local)))
        .collect()
}

fn default_true() -> bool {
    true
}

impl IkChain {
    pub fn new(target: NodeId) -> Self {
        Self {
            target,
            pole: None,
            chain_length: DEFAULT_CHAIN_LENGTH,
            iterations: DEFAULT_ITERATIONS,
            tolerance: DEFAULT_TOLERANCE,
            damping: DEFAULT_DAMPING,
            spline: None,
            stiffness: DEFAULT_STIFFNESS,
            enabled: true,
        }
    }

    pub fn sanitized(&self) -> Self {
        Self {
            target: self.target,
            pole: self.pole,
            chain_length: self.chain_length.clamp(1, 64),
            iterations: self.iterations.clamp(1, MAX_ITERATIONS),
            tolerance: if self.tolerance.is_finite() {
                self.tolerance.clamp(1e-5, 1.0)
            } else {
                DEFAULT_TOLERANCE
            },
            damping: if self.damping.is_finite() {
                self.damping.clamp(0.0, 1.0)
            } else {
                DEFAULT_DAMPING
            },
            spline: self.spline,
            stiffness: if self.stiffness.is_finite() {
                self.stiffness.clamp(0.0, 1.0)
            } else {
                DEFAULT_STIFFNESS
            },
            enabled: self.enabled,
        }
    }
}

/// Result of solving one chain. `rotations` are local rotations for the
/// movable joints, root first.
#[derive(Clone, Debug)]
pub struct IkSolution {
    pub tip: NodeId,
    pub joints: Vec<NodeId>,
    pub rotations: Vec<(NodeId, Quat)>,
    pub tip_position: Vec3,
    pub target_position: Vec3,
    pub error: f32,
    pub iterations: usize,
    pub converged: bool,
    /// True when the requested target was outside the chain's reach and the
    /// solve used the nearest reachable point.
    pub clamped: bool,
}

/// Consecutive movable Joint ancestors above `tip`, root first.
pub fn ik_chain_joints(scene: &Scene, tip: NodeId, chain: &IkChain) -> Vec<NodeId> {
    let Some(tip_node) = scene.get(tip) else {
        return Vec::new();
    };
    if !matches!(tip_node.kind, NodeKind::Joint { .. }) {
        return Vec::new();
    }
    let count = chain.chain_length.clamp(1, 64);
    let mut joints = Vec::with_capacity(count);
    let mut current = tip_node.parent;
    while joints.len() < count {
        let Some(node) = current.and_then(|id| scene.get(id)) else {
            break;
        };
        if !matches!(node.kind, NodeKind::Joint { .. }) {
            break;
        }
        joints.push(node.id);
        current = node.parent;
    }
    joints.reverse();
    joints
}

fn local_transform(
    scene: &Scene,
    locals: &BTreeMap<NodeId, Transform>,
    id: NodeId,
) -> Option<Transform> {
    locals
        .get(&id)
        .copied()
        .or_else(|| scene.get(id).map(|node| node.transform))
}

struct Pose {
    joints: Vec<Mat4>,
    tip: Mat4,
}

struct SolveContext<'a> {
    scene: &'a Scene,
    tip: NodeId,
    joints: &'a [NodeId],
    chain: &'a IkChain,
    root_parent_world: Mat4,
    target_position: Vec3,
    pole_position: Option<Vec3>,
}

fn pose_chain(
    scene: &Scene,
    joints: &[NodeId],
    tip: NodeId,
    locals: &BTreeMap<NodeId, Transform>,
    root_parent_world: Mat4,
) -> Option<Pose> {
    let mut parent_world = root_parent_world;
    let mut joint_worlds = Vec::with_capacity(joints.len());
    for id in joints {
        let local = local_transform(scene, locals, *id)?;
        parent_world *= local.matrix();
        joint_worlds.push(parent_world);
    }
    let tip_local = local_transform(scene, locals, tip)?;
    Some(Pose {
        joints: joint_worlds,
        tip: parent_world * tip_local.matrix(),
    })
}

fn world_position(world: Mat4) -> Vec3 {
    world.w_axis.truncate()
}

fn apply_world_rotation(parent_world: Mat4, local: &mut Transform, q_world: Quat) {
    let parent_rotation = crate::eval::world_rotation(parent_world);
    let q_local = parent_rotation.inverse() * q_world * parent_rotation;
    local.rotation = (q_local * local.rotation).normalize();
}

/// Unit direction that survives components large enough to overflow a squared
/// length, where `Vec3::normalize` divides by an infinite length and returns a
/// zero vector (which then turns later angles into NaN).
fn unit_direction(v: Vec3) -> Option<Vec3> {
    if !v.is_finite() {
        return None;
    }
    let scale = v.abs().max_element();
    if scale <= EPSILON {
        return None;
    }
    Some((v / scale).normalize())
}

fn direction_from_fallback(primary: Vec3, fallback: Vec3) -> Vec3 {
    unit_direction(primary)
        .or_else(|| unit_direction(fallback))
        .unwrap_or(Vec3::Y)
}

/// Solve one chain. `worlds` supplies the parent frame and `target_world` /
/// `pole_world` are the current world matrices of the goal objects.
pub fn solve_chain(
    scene: &Scene,
    tip: NodeId,
    chain: &IkChain,
    locals: &BTreeMap<NodeId, Transform>,
    worlds: &BTreeMap<NodeId, Mat4>,
    target_world: Mat4,
    pole_world: Option<Mat4>,
) -> Option<IkSolution> {
    if !chain.enabled || !matches!(scene.get(tip)?.kind, NodeKind::Joint { .. }) {
        return None;
    }
    let chain = chain.sanitized();
    let joints = ik_chain_joints(scene, tip, &chain);
    if joints.is_empty() {
        return None;
    }
    let target_position = world_position(target_world);
    if !target_position.is_finite() {
        return None;
    }
    let pole_position = pole_world
        .map(world_position)
        .filter(|position| position.is_finite());
    let root_parent_world = scene
        .get(joints[0])
        .and_then(|node| node.parent)
        .and_then(|parent| worlds.get(&parent).copied())
        .unwrap_or(Mat4::IDENTITY);

    let mut chain_locals = BTreeMap::new();
    for id in &joints {
        chain_locals.insert(*id, local_transform(scene, locals, *id)?);
    }
    // Start from an allowed pose even when a caller passed unclamped locals.
    clamp_chain_locals(scene, &joints, &mut chain_locals);

    let context = SolveContext {
        scene,
        tip,
        joints: &joints,
        chain: &chain,
        root_parent_world,
        target_position,
        pole_position,
    };
    if joints.len() == 2 {
        solve_two_bone(&context, &mut chain_locals)
    } else {
        solve_ccd(&context, &mut chain_locals)
    }
}

fn solve_two_bone(
    context: &SolveContext,
    locals: &mut BTreeMap<NodeId, Transform>,
) -> Option<IkSolution> {
    let SolveContext {
        scene,
        tip,
        joints,
        chain,
        root_parent_world,
        target_position,
        pole_position,
    } = *context;
    let base_locals = pre_solve_rotations(joints, locals);
    let initial_pose = pose_chain(scene, joints, tip, locals, root_parent_world)?;
    let a = world_position(initial_pose.joints[0]);
    let b = world_position(initial_pose.joints[1]);
    let e = world_position(initial_pose.tip);
    let requested_error = (e - target_position).length();
    let l1 = (b - a).length();
    let l2 = (e - b).length();
    let to_target = target_position - a;
    let max_reach = l1 + l2;
    let min_reach = (l1 - l2).abs();
    // Non-finite bones from a damaged scene leave the reach bounds NaN or
    // infinite, which would make the reach clamp below panic.
    if !min_reach.is_finite() || !max_reach.is_finite() {
        return None;
    }

    let mut solution = IkSolution {
        tip,
        joints: joints.to_vec(),
        rotations: Vec::new(),
        tip_position: e,
        target_position,
        error: requested_error,
        iterations: 1,
        converged: false,
        clamped: false,
    };

    if l1 <= EPSILON || l2 <= EPSILON || max_reach <= EPSILON {
        clamp_chain_locals(scene, joints, locals);
        let pose = pose_chain(scene, joints, tip, locals, root_parent_world)?;
        solution.tip_position = world_position(pose.tip);
        solution.error = (solution.tip_position - target_position).length();
        solution.rotations = joints
            .iter()
            .filter_map(|id| locals.get(id).map(|local| (*id, local.rotation)))
            .collect();
        solution.converged = solution.error <= chain.tolerance;
        return Some(solution);
    }

    let requested_distance = to_target.length();
    let base = direction_from_fallback(to_target, b - a);
    let target_distance = if requested_distance.is_finite() {
        requested_distance
    } else {
        max_reach
    };
    let desired_distance = target_distance.clamp(min_reach, max_reach);
    solution.clamped = !requested_distance.is_finite()
        || requested_distance > max_reach + 1e-4
        || requested_distance < min_reach - 1e-4;

    let mut side = Vec3::ZERO;
    if let Some(pole) = pole_position {
        let pole_vector = pole - a;
        side = pole_vector - base * pole_vector.dot(base);
    }
    if side.length_squared() <= EPSILON {
        let current_root = b - a;
        side = current_root - base * current_root.dot(base);
    }
    if side.length_squared() <= EPSILON {
        side = base.cross(Vec3::Y);
    }
    if side.length_squared() <= EPSILON {
        side = base.cross(Vec3::X);
    }
    if side.length_squared() <= EPSILON {
        side = Vec3::Z;
    }
    let side = unit_direction(side).unwrap_or(Vec3::Y);

    let desired_tip = a + base * desired_distance;
    let (root_dir, desired_tip) = if desired_distance <= EPSILON {
        // The target is at the root. Folding equal-length bones reaches it;
        // otherwise the minimum reach is above zero and this branch is unused.
        (direction_from_fallback(b - a, Vec3::Y), a)
    } else {
        // Bones and target distances can exceed the square root of f32::MAX,
        // so squaring or multiplying them overflows to infinity and the law of
        // cosines yields inf/inf. Measuring every side against the longest one
        // leaves the cosine unchanged while keeping the terms finite.
        let scale = l1.max(desired_distance);
        let (a_side, reach, b_side) = (l1 / scale, desired_distance / scale, l2 / scale);
        let cos_a = ((a_side * a_side + reach * reach - b_side * b_side) / (2.0 * a_side * reach))
            .clamp(-1.0, 1.0);
        let angle = cos_a.acos();
        let root_dir = unit_direction(base * angle.cos() + side * angle.sin()).unwrap_or(base);
        (root_dir, desired_tip)
    };

    let current_root = b - a;
    if current_root.length_squared() > EPSILON {
        let current_dir = unit_direction(current_root).unwrap_or(root_dir);
        let q_root = Quat::from_rotation_arc(current_dir, root_dir);
        if let Some(local) = locals.get_mut(&joints[0]) {
            apply_world_rotation(root_parent_world, local, q_root);
            clamp_joint_local(scene, joints[0], local);
        }
    }

    let after_root = pose_chain(scene, joints, tip, locals, root_parent_world)?;
    let current_mid = world_position(after_root.tip) - world_position(after_root.joints[1]);
    let desired_mid = desired_tip - world_position(after_root.joints[1]);
    let mid_parent_world = after_root.joints[0];
    if current_mid.length_squared() > EPSILON && desired_mid.length_squared() > EPSILON {
        if let (Some(current_dir), Some(desired_dir)) =
            (unit_direction(current_mid), unit_direction(desired_mid))
        {
            let q_mid = Quat::from_rotation_arc(current_dir, desired_dir);
            if let Some(local) = locals.get_mut(&joints[1]) {
                apply_world_rotation(mid_parent_world, local, q_mid);
                clamp_joint_local(scene, joints[1], local);
            }
        }
    }

    soften_pose(scene, chain, &base_locals, locals);
    clamp_chain_locals(scene, joints, locals);
    let final_pose = pose_chain(scene, joints, tip, locals, root_parent_world)?;
    let final_tip = world_position(final_pose.tip);
    solution.tip_position = final_tip;
    solution.error = (final_tip - target_position).length();
    solution.converged = solution.error <= chain.tolerance;
    solution.rotations = joints
        .iter()
        .filter_map(|id| locals.get(id).map(|local| (*id, local.rotation)))
        .collect();
    Some(solution)
}

fn solve_ccd(
    context: &SolveContext,
    locals: &mut BTreeMap<NodeId, Transform>,
) -> Option<IkSolution> {
    let SolveContext {
        scene,
        tip,
        joints,
        chain,
        root_parent_world,
        target_position,
        pole_position,
    } = *context;
    let base_locals = pre_solve_rotations(joints, locals);
    let initial = pose_chain(scene, joints, tip, locals, root_parent_world)?;
    let root_position = world_position(initial.joints[0]);
    let mut max_reach = 0.0;
    let mut previous = root_position;
    for joint in &initial.joints[1..] {
        let position = world_position(*joint);
        max_reach += (position - previous).length();
        previous = position;
    }
    max_reach += (world_position(initial.tip) - previous).length();
    let target_distance = (target_position - root_position).length();
    let clamped = if target_distance.is_finite() {
        target_distance > max_reach + 1e-4
    } else {
        true
    };
    let mut solution = IkSolution {
        tip,
        joints: joints.to_vec(),
        rotations: Vec::new(),
        tip_position: Vec3::ZERO,
        target_position,
        error: f32::INFINITY,
        iterations: 0,
        converged: false,
        clamped,
    };

    for pass in 0..chain.iterations {
        solution.iterations = pass + 1;
        for index in (0..joints.len()).rev() {
            let pose = pose_chain(scene, joints, tip, locals, root_parent_world)?;
            let pivot = world_position(pose.joints[index]);
            let end = world_position(pose.tip);
            let to_end = end - pivot;
            let to_target = target_position - pivot;
            if to_end.length_squared() <= EPSILON
                || !to_target.is_finite()
                || to_target.length_squared() <= EPSILON
            {
                continue;
            }
            let q_world = Quat::from_rotation_arc(to_end.normalize(), to_target.normalize());
            let q_world = if chain.damping < 0.999 {
                Quat::IDENTITY.slerp(q_world, chain.damping)
            } else {
                q_world
            };

            let parent_world = if index == 0 {
                root_parent_world
            } else {
                pose.joints[index - 1]
            };
            if let Some(local) = locals.get_mut(&joints[index]) {
                apply_world_rotation(parent_world, local, q_world);
                clamp_joint_local(scene, joints[index], local);
            }
        }

        let pose = pose_chain(scene, joints, tip, locals, root_parent_world)?;
        solution.tip_position = world_position(pose.tip);
        solution.error = (solution.tip_position - target_position).length();
        if solution.error <= chain.tolerance {
            solution.converged = true;
            break;
        }
    }

    // A fully converged multi-bone solve can still leave an arbitrary twist.
    // Rotate the whole chain around the root-to-target axis to put the middle
    // joint on the pole side; points on that axis, including the reached tip,
    // do not move.
    if solution.converged {
        if let Some(pole) = pole_position {
            let pose = pose_chain(scene, joints, tip, locals, root_parent_world)?;
            let root = world_position(pose.joints[0]);
            let axis = (target_position - root).normalize_or_zero();
            if axis.length_squared() > EPSILON {
                let middle = world_position(pose.joints[joints.len() / 2]);
                let radial = middle - root;
                let radial = radial - axis * radial.dot(axis);
                let pole_radial = pole - root;
                let pole_radial = pole_radial - axis * pole_radial.dot(axis);
                if radial.length_squared() > EPSILON && pole_radial.length_squared() > EPSILON {
                    let angle = radial
                        .cross(pole_radial)
                        .dot(axis)
                        .atan2(radial.dot(pole_radial));
                    if angle.abs() > 1e-4 {
                        let q_world = Quat::from_axis_angle(axis, angle);
                        if let Some(local) = locals.get_mut(&joints[0]) {
                            apply_world_rotation(root_parent_world, local, q_world);
                            clamp_joint_local(scene, joints[0], local);
                        }
                        let pose = pose_chain(scene, joints, tip, locals, root_parent_world)?;
                        solution.tip_position = world_position(pose.tip);
                        solution.error = (solution.tip_position - target_position).length();
                        solution.converged = solution.error <= chain.tolerance;
                    }
                }
            }
        }
    }

    if solution.iterations == 0 {
        solution.iterations = 1;
    }
    soften_pose(scene, chain, &base_locals, locals);
    clamp_chain_locals(scene, joints, locals);
    let pose = pose_chain(scene, joints, tip, locals, root_parent_world)?;
    solution.tip_position = world_position(pose.tip);
    solution.error = (solution.tip_position - target_position).length();
    solution.rotations = joints
        .iter()
        .filter_map(|id| locals.get(id).map(|local| (*id, local.rotation)))
        .collect();
    Some(solution)
}

/// World-space polyline of a Spline node with each point's cumulative arc
/// length. A non-spline node, non-finite transform or repeated points that
/// collapse the curve yield fewer than two points, which disables the spline
/// path.
fn spline_polyline(scene: &Scene, spline: NodeId, spline_world: Mat4) -> Vec<(Vec3, f32)> {
    let Some(node) = scene.get(spline) else {
        return Vec::new();
    };
    let NodeKind::Spline { spline: definition } = &node.kind else {
        return Vec::new();
    };
    let mut polyline: Vec<(Vec3, f32)> = Vec::new();
    let mut length = 0.0f32;
    for point in definition.sample() {
        let world = spline_world.transform_point3(point);
        if !world.is_finite() {
            return Vec::new();
        }
        if let Some((previous, _)) = polyline.last() {
            let step = (world - *previous).length();
            if !step.is_finite() {
                return Vec::new();
            }
            if step <= EPSILON {
                continue;
            }
            length += step;
        }
        polyline.push((world, length));
    }
    polyline
}

/// Point on the polyline at `arc`, clamped to its ends.
fn polyline_position(polyline: &[(Vec3, f32)], arc: f32) -> Option<Vec3> {
    let last = polyline.len().checked_sub(1)?;
    if last == 0 {
        return None;
    }
    if arc >= polyline[last].1 {
        return Some(polyline[last].0);
    }
    let mut index = 0;
    while index + 1 < last && polyline[index + 1].1 < arc {
        index += 1;
    }
    let (start, start_arc) = polyline[index];
    let (end, end_arc) = polyline[index + 1];
    let span = end_arc - start_arc;
    let t = if span > EPSILON {
        ((arc - start_arc) / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    Some(start + (end - start) * t)
}

/// Arc length and position of the polyline point closest to `point`.
fn nearest_polyline_point(polyline: &[(Vec3, f32)], point: Vec3) -> Option<(f32, Vec3)> {
    let mut best: Option<(f32, f32, Vec3)> = None;
    for window in polyline.windows(2) {
        let (start, start_arc) = window[0];
        let (end, end_arc) = window[1];
        let segment = end - start;
        let length_squared = segment.length_squared();
        if !length_squared.is_finite() || length_squared <= EPSILON {
            continue;
        }
        let t = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
        let closest = start + segment * t;
        let distance = (point - closest).length_squared();
        if !distance.is_finite() {
            continue;
        }
        let better = match best {
            Some((best_distance, _, _)) => distance < best_distance,
            None => true,
        };
        if better {
            best = Some((distance, start_arc + (end_arc - start_arc) * t, closest));
        }
    }
    best.map(|(_, arc, closest)| (arc, closest))
}

/// Number of world-space passes over a spline chain. Each bone is aimed from
/// its parent's frame, so a second pass removes most of the residual chord
/// error that a sharp curve or a joint limit introduces.
const SPLINE_PASSES: usize = 2;

/// Solve a chain against a Spline node instead of a point goal.
///
/// Clean-room semantics (the public Cheetah3D pages advertise Spline IK without
/// defining its solver): sample the spline into a world-space polyline, then
/// aim every bone at the curve point one bone length further along from that
/// joint's projection onto the curve. Bones keep their exact length because the
/// solve only rotates, so the chain approximates the curve as a chain of chords
/// and the tip reports its distance from the nearest curve point. A curve
/// shorter than the chain leaves the remaining bones aimed at its end and the
/// solution is marked clamped. Joint rotation limits clamp after every
/// correction. Returns `None` when the node is not a Spline, the curve has
/// fewer than two distinct samples, or the chain has no movable joints.
pub fn solve_spline_chain(
    scene: &Scene,
    tip: NodeId,
    chain: &IkChain,
    locals: &BTreeMap<NodeId, Transform>,
    worlds: &BTreeMap<NodeId, Mat4>,
    spline: NodeId,
    spline_world: Mat4,
) -> Option<IkSolution> {
    if !chain.enabled || !matches!(scene.get(tip)?.kind, NodeKind::Joint { .. }) {
        return None;
    }
    let chain = chain.sanitized();
    let joints = ik_chain_joints(scene, tip, &chain);
    if joints.is_empty() || !spline_world.is_finite() {
        return None;
    }
    let polyline = spline_polyline(scene, spline, spline_world);
    if polyline.len() < 2 {
        return None;
    }
    let curve_end = polyline[polyline.len() - 1].1;
    if !curve_end.is_finite() || curve_end <= EPSILON {
        return None;
    }
    let root_parent_world = scene
        .get(joints[0])
        .and_then(|node| node.parent)
        .and_then(|parent| worlds.get(&parent).copied())
        .unwrap_or(Mat4::IDENTITY);

    let mut chain_locals = BTreeMap::new();
    for id in &joints {
        chain_locals.insert(*id, local_transform(scene, locals, *id)?);
    }
    clamp_chain_locals(scene, &joints, &mut chain_locals);

    // Bone lengths from the authored pose: the solve rotates only, so these
    // stay exact and give every joint its place along the curve.
    let base_locals = pre_solve_rotations(&joints, &chain_locals);
    let initial_pose = pose_chain(scene, &joints, tip, &chain_locals, root_parent_world)?;
    let mut bone_lengths = Vec::with_capacity(joints.len());
    for index in 0..joints.len() {
        let start = world_position(initial_pose.joints[index]);
        let end = if index + 1 < joints.len() {
            world_position(initial_pose.joints[index + 1])
        } else {
            world_position(initial_pose.tip)
        };
        let length = (end - start).length();
        if !length.is_finite() {
            return None;
        }
        bone_lengths.push(length);
    }
    let chain_length: f32 = bone_lengths.iter().sum();

    for _ in 0..SPLINE_PASSES {
        for index in 0..joints.len() {
            let pose = pose_chain(scene, &joints, tip, &chain_locals, root_parent_world)?;
            let start = world_position(pose.joints[index]);
            let child = if index + 1 < joints.len() {
                world_position(pose.joints[index + 1])
            } else {
                world_position(pose.tip)
            };
            let Some((arc, _)) = nearest_polyline_point(&polyline, start) else {
                continue;
            };
            let goal_arc = (arc + bone_lengths[index]).min(curve_end);
            let Some(goal) = polyline_position(&polyline, goal_arc) else {
                continue;
            };
            let Some(current) = unit_direction(child - start) else {
                continue;
            };
            let Some(desired) = unit_direction(goal - start) else {
                continue;
            };
            let rotation = Quat::from_rotation_arc(current, desired);
            let parent_world = if index == 0 {
                root_parent_world
            } else {
                pose.joints[index - 1]
            };
            if let Some(local) = chain_locals.get_mut(&joints[index]) {
                apply_world_rotation(parent_world, local, rotation);
                clamp_joint_local(scene, joints[index], local);
            }
        }
    }

    soften_pose(scene, &chain, &base_locals, &mut chain_locals);
    clamp_chain_locals(scene, &joints, &mut chain_locals);
    let final_pose = pose_chain(scene, &joints, tip, &chain_locals, root_parent_world)?;
    let tip_position = world_position(final_pose.tip);
    let (_, nearest) =
        nearest_polyline_point(&polyline, tip_position).unwrap_or((0.0, tip_position));
    let error = (tip_position - nearest).length();
    Some(IkSolution {
        tip,
        joints: joints.clone(),
        rotations: joints
            .iter()
            .filter_map(|id| chain_locals.get(id).map(|local| (*id, local.rotation)))
            .collect(),
        tip_position,
        target_position: nearest,
        error: if error.is_finite() { error } else { f32::MAX },
        iterations: SPLINE_PASSES,
        converged: error.is_finite() && error <= chain.tolerance,
        clamped: chain_length > curve_end + 1e-4,
    })
}

/// Solve and apply every enabled chain in scene order. Solving before the
/// normal tag-constraint pass lets skinning see the IK pose.
pub fn solve_ik_chains(
    scene: &Scene,
    locals: &mut BTreeMap<NodeId, Transform>,
    worlds: &BTreeMap<NodeId, Mat4>,
) -> Vec<IkSolution> {
    let mut solutions = Vec::new();
    for tip in scene.depth_first() {
        let Some(node) = scene.get(tip) else {
            continue;
        };
        let Some(chain) = node.ik.as_ref() else {
            continue;
        };
        if !chain.enabled {
            continue;
        }
        // A usable Spline target drives the chain along its curve; a missing or
        // non-spline node falls through to the point target below.
        if let Some(spline) = chain.spline {
            if let Some(spline_world) = worlds.get(&spline).copied() {
                if let Some(solution) =
                    solve_spline_chain(scene, tip, chain, locals, worlds, spline, spline_world)
                {
                    for (id, rotation) in &solution.rotations {
                        if let Some(local) = locals.get_mut(id) {
                            local.rotation = *rotation;
                        }
                    }
                    solutions.push(solution);
                    continue;
                }
            }
        }
        let Some(target_world) = worlds.get(&chain.target).copied() else {
            continue;
        };
        let pole_world = chain.pole.and_then(|pole| worlds.get(&pole).copied());
        let Some(solution) =
            solve_chain(scene, tip, chain, locals, worlds, target_world, pole_world)
        else {
            continue;
        };
        for (id, rotation) in &solution.rotations {
            if let Some(local) = locals.get_mut(id) {
                local.rotation = *rotation;
            }
        }
        solutions.push(solution);
    }
    solutions
}

/// Maximum number of frames one bake request may write. This keeps a bad scene
/// range from stalling the editor; a full take normally has far fewer frames.
pub const MAX_IK_BAKE_FRAMES: usize = 10_000;

fn baked_joint_ids(doc: &Document) -> BTreeSet<NodeId> {
    let mut joints = BTreeSet::new();
    for tip in doc.scene.depth_first() {
        let Some(node) = doc.scene.get(tip) else {
            continue;
        };
        let Some(chain) = node.ik.as_ref() else {
            continue;
        };
        if !chain.enabled {
            continue;
        }
        let has_goal = doc.scene.get(chain.target).is_some()
            || chain
                .spline
                .is_some_and(|spline| doc.scene.get(spline).is_some());
        if !has_goal {
            continue;
        }
        joints.extend(ik_chain_joints(&doc.scene, tip, chain));
    }
    // A constraint tag drives its joint without any chain, so a bake that only
    // walked chains would silently drop those poses.
    for node in doc.scene.nodes.values() {
        if matches!(node.kind, NodeKind::Joint { .. })
            && node.tags.iter().any(|tag| tag.is_ik_constraint())
        {
            joints.insert(node.id);
        }
    }
    joints
}

fn bake_ik_edits_for_times(
    doc: &Document,
    times: &[f32],
) -> Vec<(NodeId, AnimationClip, AnimationClip)> {
    if times.is_empty() {
        return Vec::new();
    }
    let joints = baked_joint_ids(doc);
    if joints.is_empty() {
        return Vec::new();
    }
    let mut edits: BTreeMap<NodeId, (AnimationClip, AnimationClip)> = joints
        .into_iter()
        .filter_map(|id| {
            let node = doc.scene.get(id)?;
            Some((id, (node.animation.clone(), node.animation.clone())))
        })
        .collect();
    for &time in times {
        let time = if time.is_finite() { time.max(0.0) } else { 0.0 };
        let evaluated = doc.evaluate(time);
        for (id, (_, after)) in &mut edits {
            let Some(node) = evaluated.node(*id) else {
                continue;
            };
            if node.local.rotation.is_finite() {
                after.set_rotation_key(
                    ChannelTarget::Object { id: *id },
                    time,
                    node.local.rotation,
                    Interpolation::Smooth,
                );
            }
        }
    }
    edits
        .into_iter()
        .filter(|(_, (before, after))| before != after)
        .map(|(id, (before, after))| (id, before, after))
        .collect()
}

/// Build animation edits that key the current solved rotation of every joint in
/// every enabled chain at `time`. The caller applies these through
/// `SetAnimationsCmd`, preserving one undo entry for the bake.
pub fn bake_ik_edits(doc: &Document, time: f32) -> Vec<(NodeId, AnimationClip, AnimationClip)> {
    let time = if time.is_finite() { time.max(0.0) } else { 0.0 };
    bake_ik_edits_for_times(doc, &[time])
}

/// Bake every frame in `[start, end]` at `fps`, matching Cheetah3D's documented
/// IK bake behavior of keying every frame rather than only the playhead. The
/// returned clips replace the originals through one `SetAnimationsCmd`.
pub fn bake_ik_range_edits(
    doc: &Document,
    start: f32,
    end: f32,
    fps: f32,
) -> Vec<(NodeId, AnimationClip, AnimationClip)> {
    let fps = if fps.is_finite() && fps > 0.0 {
        fps.clamp(1.0, 240.0)
    } else {
        30.0
    };
    let start = if start.is_finite() {
        start.max(0.0)
    } else {
        0.0
    };
    let end = if end.is_finite() {
        end.max(start)
    } else {
        start
    };
    let start_frame = (start * fps).round() as i64;
    let end_frame = (end * fps).round() as i64;
    let count = (end_frame - start_frame + 1).max(0) as usize;
    if count == 0 || count > MAX_IK_BAKE_FRAMES {
        return Vec::new();
    }
    let times: Vec<f32> = (0..count)
        .map(|index| (start_frame + index as i64) as f32 / fps)
        .collect();
    bake_ik_edits_for_times(doc, &times)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GeometrySource, History, Node, NodeProps, SetNodePropsCmd, Skin};
    use glam::Vec3;
    use lc_anim::{AnimChannel, ChannelAddress};

    fn joint(doc: &mut Document, name: &str, parent: Option<NodeId>, translation: Vec3) -> NodeId {
        let id = doc.append_node(
            NodeKind::Joint {
                length: 1.0,
                width: 0.08,
            },
            name,
            parent,
        );
        doc.scene.get_mut(id).unwrap().transform = Transform::from_translation(translation);
        id
    }

    fn target(doc: &mut Document, name: &str, translation: Vec3) -> NodeId {
        let id = doc.append_node(NodeKind::Null, name, None);
        doc.scene.get_mut(id).unwrap().transform = Transform::from_translation(translation);
        id
    }

    fn two_bone_setup(name: &str, goal: Vec3) -> (Document, NodeId, NodeId, NodeId) {
        let mut doc = Document::empty();
        let root = joint(&mut doc, "Shoulder", None, Vec3::ZERO);
        let middle = joint(&mut doc, "Elbow", Some(root), Vec3::X);
        let tip = joint(&mut doc, "Wrist", Some(middle), Vec3::X);
        let target = target(&mut doc, &format!("{name} Goal"), goal);
        doc.scene.get_mut(tip).unwrap().ik = Some(IkChain::new(target));
        (doc, root, middle, tip)
    }

    fn all_limits(degrees: f32) -> JointLimits {
        JointLimits {
            enabled: true,
            min: Vec3::splat(-degrees),
            max: Vec3::splat(degrees),
        }
    }

    fn assert_rotation_within(rotation: Quat, limits: &JointLimits, label: &str) {
        let limits = limits.sanitized();
        let euler = rotation_euler_degrees(rotation);
        for (axis, value) in [("X", euler.x), ("Y", euler.y), ("Z", euler.z)] {
            assert!(
                value >= limits.min[axis_index(axis)] - 1e-3
                    && value <= limits.max[axis_index(axis)] + 1e-3,
                "{label} {axis} rotation {value}° escaped limits [{}, {}]°",
                limits.min[axis_index(axis)],
                limits.max[axis_index(axis)]
            );
        }
    }

    fn axis_index(axis: &str) -> usize {
        match axis {
            "X" => 0,
            "Y" => 1,
            _ => 2,
        }
    }

    #[test]
    fn analytic_solve_respects_joint_rotation_limits() {
        let (mut doc, root, middle, tip) =
            two_bone_setup("limited-analytic", Vec3::new(1.0, 1.2, 0.0));
        let limits = all_limits(10.0);
        doc.scene.get_mut(root).unwrap().rotation_limits = Some(limits.clone());
        doc.scene.get_mut(middle).unwrap().rotation_limits = Some(limits.clone());
        let solved = doc.evaluate(0.0);
        let root_rotation = solved.node(root).unwrap().local.rotation;
        let middle_rotation = solved.node(middle).unwrap().local.rotation;
        assert_rotation_within(root_rotation, &limits, "shoulder");
        assert_rotation_within(middle_rotation, &limits, "elbow");
        assert!(
            root_rotation.angle_between(Quat::IDENTITY) > 0.05,
            "the limited analytic solve must still attempt the bend"
        );
        assert!(solved
            .node(tip)
            .unwrap()
            .world
            .w_axis
            .truncate()
            .is_finite());
    }

    #[test]
    fn ccd_solve_respects_zero_joint_rotation_limits() {
        let mut doc = Document::empty();
        let root = joint(&mut doc, "Root", None, Vec3::ZERO);
        let a = joint(&mut doc, "A", Some(root), Vec3::X);
        let b = joint(&mut doc, "B", Some(a), Vec3::X);
        let tip = joint(&mut doc, "Tip", Some(b), Vec3::X);
        let goal = target(&mut doc, "Locked Goal", Vec3::new(2.0, 1.0, 0.5));
        let mut chain = IkChain::new(goal);
        chain.chain_length = 3;
        chain.iterations = 64;
        doc.scene.get_mut(tip).unwrap().ik = Some(chain);
        let locked = all_limits(0.0);
        for id in [root, a, b] {
            doc.scene.get_mut(id).unwrap().rotation_limits = Some(locked.clone());
        }

        let solved = doc.evaluate(0.0);
        for (id, label) in [(root, "root"), (a, "A"), (b, "B")] {
            let rotation = solved.node(id).unwrap().local.rotation;
            assert_rotation_within(rotation, &locked, label);
            assert!(
                rotation.angle_between(Quat::IDENTITY) < 1e-3,
                "a zero-width limit must lock {label}, got {rotation:?}"
            );
        }
        assert!(solved
            .node(tip)
            .unwrap()
            .world
            .w_axis
            .truncate()
            .is_finite());
    }

    #[test]
    fn direct_joint_rotation_respects_limits_without_ik() {
        let (mut doc, root, _middle, tip) =
            two_bone_setup("limited-direct", Vec3::new(1.0, 1.0, 0.0));
        doc.scene.get_mut(tip).unwrap().ik = None;
        doc.scene.get_mut(root).unwrap().transform =
            Transform::from_euler_degrees(Vec3::new(0.0, 0.0, 45.0));
        let limits = all_limits(10.0);
        doc.scene.get_mut(root).unwrap().rotation_limits = Some(limits.clone());
        let solved = doc.evaluate(0.0);
        let rotation = solved.node(root).unwrap().local.rotation;
        assert_rotation_within(rotation, &limits, "direct shoulder");
        let euler = rotation_euler_degrees(rotation);
        assert!(
            (euler.z - 10.0).abs() < 1e-3,
            "direct 45° rotation should clamp to the 10° limit, got {euler:?}"
        );
    }

    #[test]
    fn joint_limits_round_trip_and_undo() {
        let (mut doc, root, _middle, _tip) =
            two_bone_setup("limited-props", Vec3::new(1.0, 1.0, 0.0));
        let limits = JointLimits {
            enabled: true,
            min: Vec3::new(-20.0, -10.0, -5.0),
            max: Vec3::new(20.0, 30.0, 40.0),
        };
        doc.scene.get_mut(root).unwrap().rotation_limits = Some(limits.clone());
        let json = crate::to_json(&doc).unwrap();
        let loaded = crate::from_json(&json).unwrap();
        assert_eq!(
            loaded.scene.get(root).unwrap().rotation_limits.as_ref(),
            Some(&limits),
            "joint limits must survive the native format"
        );

        let mut history = History::new(4);
        let mut props = NodeProps::of(doc.scene.get(root).unwrap());
        props.rotation_limits = Some(all_limits(0.0));
        let cmd = SetNodePropsCmd::new(&doc, root, props).unwrap();
        let before = doc.signature();
        history.execute(&mut doc, Box::new(cmd));
        assert_eq!(
            doc.scene.get(root).unwrap().rotation_limits,
            Some(all_limits(0.0))
        );
        history.undo(&mut doc);
        assert_eq!(
            doc.scene.get(root).unwrap().rotation_limits,
            Some(limits),
            "undo must restore the authored limits"
        );
        assert_eq!(doc.signature(), before);
    }

    #[test]
    fn non_finite_bone_lengths_do_not_panic_evaluation() {
        let mut doc = Document::empty();
        let root = joint(&mut doc, "Root", None, Vec3::ZERO);
        let middle = joint(
            &mut doc,
            "Middle",
            Some(root),
            Vec3::new(f32::INFINITY, 0.0, 0.0),
        );
        let tip = joint(&mut doc, "Tip", Some(middle), Vec3::X);
        let goal = target(&mut doc, "Goal", Vec3::X);
        doc.scene.get_mut(tip).unwrap().ik = Some(IkChain::new(goal));

        let solved = doc.evaluate(0.0);
        for id in [root, middle, tip] {
            assert!(
                solved.node(id).unwrap().local.rotation.is_finite(),
                "a non-finite bone must not leak a NaN rotation or panic"
            );
        }
    }

    #[test]
    fn huge_bones_solve_without_overflowing_the_law_of_cosines() {
        let mut doc = Document::empty();
        let root = joint(&mut doc, "Root", None, Vec3::ZERO);
        let middle = joint(&mut doc, "Middle", Some(root), Vec3::new(1.8e19, 0.0, 0.0));
        let tip = joint(&mut doc, "Tip", Some(middle), Vec3::new(1.8e19, 0.0, 0.0));
        // Reachable, but the squared side lengths and the doubled `l1 * d`
        // product all overflow f32, so the raw law of cosines is inf/inf and a
        // NaN-driven fallback would straighten the chain instead of bending it.
        let goal = target(&mut doc, "Goal", Vec3::new(1e19, 0.0, 0.0));
        doc.scene.get_mut(tip).unwrap().ik = Some(IkChain::new(goal));

        let solved = doc.evaluate(0.0);
        for id in [root, middle, tip] {
            let node = solved.node(id).unwrap();
            assert!(
                node.local.rotation.is_finite(),
                "huge bones must not write a NaN rotation into the chain"
            );
            assert!(node.world.w_axis.truncate().is_finite());
        }
        assert!(
            solved
                .node(middle)
                .unwrap()
                .local
                .rotation
                .angle_between(Quat::IDENTITY)
                > 0.1,
            "the elbow must bend instead of falling back to a straight chain"
        );
        let error = (solved.node(tip).unwrap().world.w_axis.truncate() - Vec3::new(1e19, 0.0, 0.0))
            .length();
        assert!(
            error < 1e16,
            "huge bones should still reach the target, missed by {error}"
        );
    }

    #[test]
    fn two_bone_analytic_solve_reaches_target_and_changes_rotations() {
        let (doc, root, middle, tip) = two_bone_setup("analytic", Vec3::new(1.0, 1.2, 0.0));
        let solved = doc.evaluate(0.0);
        let error =
            (solved.node(tip).unwrap().world.w_axis.truncate() - Vec3::new(1.0, 1.2, 0.0)).length();
        assert!(error < 1e-3, "two-bone tip missed target by {error}");
        let root_rotation = solved.node(root).unwrap().local.rotation;
        let middle_rotation = solved.node(middle).unwrap().local.rotation;
        assert!(
            root_rotation.angle_between(Quat::IDENTITY) > 0.1,
            "the analytic solve must rotate the shoulder"
        );
        assert!(
            middle_rotation.angle_between(Quat::IDENTITY) > 0.1,
            "the analytic solve must rotate the elbow"
        );
    }

    #[test]
    fn pole_controls_the_two_bone_bend_plane() {
        let (mut doc, _root, middle, tip) = two_bone_setup("pole", Vec3::new(1.0, 0.7, 0.0));
        let pole = target(&mut doc, "Pole", Vec3::new(0.2, -2.0, 0.0));
        doc.scene.get_mut(tip).unwrap().ik.as_mut().unwrap().pole = Some(pole);
        let solved = doc.evaluate(0.0);
        assert!(
            solved.node(middle).unwrap().world.w_axis.y < -0.1,
            "the pole below the chain should pull the elbow downward"
        );
        assert!(
            (solved.node(tip).unwrap().world.w_axis.truncate() - Vec3::new(1.0, 0.7, 0.0)).length()
                < 1e-3
        );
    }

    #[test]
    fn multi_bone_ccd_reaches_a_reachable_target() {
        let mut doc = Document::empty();
        let root = joint(&mut doc, "Root", None, Vec3::ZERO);
        let a = joint(&mut doc, "A", Some(root), Vec3::X);
        let b = joint(&mut doc, "B", Some(a), Vec3::X);
        let tip = joint(&mut doc, "Tip", Some(b), Vec3::X);
        let goal = target(&mut doc, "Goal", Vec3::new(2.0, 1.0, 0.5));
        let mut chain = IkChain::new(goal);
        chain.chain_length = 3;
        chain.iterations = 64;
        chain.tolerance = 1e-3;
        doc.scene.get_mut(tip).unwrap().ik = Some(chain);
        let solved = doc.evaluate(0.0);
        let error =
            (solved.node(tip).unwrap().world.w_axis.truncate() - Vec3::new(2.0, 1.0, 0.5)).length();
        assert!(error < 5e-3, "CCD tip missed target by {error}");
        assert!(
            solved
                .node(b)
                .unwrap()
                .local
                .rotation
                .angle_between(Quat::IDENTITY)
                > 0.05
        );
    }

    #[test]
    fn ccd_pole_prefers_a_bend_plane_without_losing_the_target() {
        let mut doc = Document::empty();
        let root = joint(&mut doc, "Root", None, Vec3::ZERO);
        let a = joint(&mut doc, "A", Some(root), Vec3::X);
        let b = joint(&mut doc, "B", Some(a), Vec3::X);
        let tip = joint(&mut doc, "Tip", Some(b), Vec3::X);
        let goal = target(&mut doc, "Goal", Vec3::new(2.0, 1.0, 0.5));
        let mut chain = IkChain::new(goal);
        chain.chain_length = 3;
        chain.iterations = 64;
        chain.tolerance = 5e-3;
        doc.scene.get_mut(tip).unwrap().ik = Some(chain.clone());
        let no_pole = doc.evaluate(0.0);
        let no_pole_middle = no_pole.node(b).unwrap().world.w_axis.truncate();
        let pole = target(&mut doc, "Pole", Vec3::new(0.0, 3.0, 0.0));
        chain.pole = Some(pole);
        doc.scene.get_mut(tip).unwrap().ik = Some(chain);
        let with_pole = doc.evaluate(0.0);
        let error = (with_pole.node(tip).unwrap().world.w_axis.truncate()
            - Vec3::new(2.0, 1.0, 0.5))
        .length();
        assert!(error < 5e-3, "pole changed reach by {error}");
        let pole_middle = with_pole.node(b).unwrap().world.w_axis.truncate();
        assert!(
            (pole_middle - no_pole_middle).length() > 1e-3,
            "the optional pole should influence the multi-bone bend plane"
        );
    }

    #[test]
    fn unreachable_targets_clamp_without_stretching_or_nans() {
        let goal = Vec3::new(9.0, 3.0, -2.0);
        let (doc, root, middle, tip) = two_bone_setup("unreachable", goal);
        let solved = doc.evaluate(0.0);
        let tip_position = solved.node(tip).unwrap().world.w_axis.truncate();
        let root_position = solved.node(root).unwrap().world.w_axis.truncate();
        assert!(tip_position.is_finite(), "tip position must stay finite");
        assert!(
            ((tip_position - root_position).length() - 2.0).abs() < 1e-3,
            "clamped tip should sit at the two-unit reach, got {}",
            (tip_position - root_position).length()
        );
        let tip_direction = (tip_position - root_position).normalize();
        let target_direction = (goal - root_position).normalize();
        assert!(
            tip_direction.dot(target_direction) > 0.999,
            "an unreachable chain must point at the target, not stay at rest"
        );
        assert!(
            solved
                .node(root)
                .unwrap()
                .local
                .rotation
                .angle_between(Quat::IDENTITY)
                > 0.05
                || solved
                    .node(middle)
                    .unwrap()
                    .local
                    .rotation
                    .angle_between(Quat::IDENTITY)
                    > 0.05,
            "the clamped solve must still rotate the chain"
        );
        for id in solved.nodes.iter().map(|node| node.id) {
            let node = solved.node(id).unwrap();
            assert!(node.world.w_axis.truncate().is_finite());
            assert!(node.local.rotation.is_finite());
        }
    }

    #[test]
    fn multi_bone_unreachable_targets_stay_finite_and_do_not_stretch() {
        let mut doc = Document::empty();
        let root = joint(&mut doc, "Root", None, Vec3::ZERO);
        let a = joint(&mut doc, "A", Some(root), Vec3::X);
        let b = joint(&mut doc, "B", Some(a), Vec3::X);
        let tip = joint(&mut doc, "Tip", Some(b), Vec3::X);
        let goal = target(&mut doc, "Far Goal", Vec3::new(50.0, 30.0, -20.0));
        let mut chain = IkChain::new(goal);
        chain.chain_length = 3;
        chain.iterations = 64;
        doc.scene.get_mut(tip).unwrap().ik = Some(chain);
        let solved = doc.evaluate(0.0);
        let tip_position = solved.node(tip).unwrap().world.w_axis.truncate();
        let root_position = solved.node(root).unwrap().world.w_axis.truncate();
        assert!(tip_position.is_finite());
        assert!((tip_position - root_position).length() <= 3.0 + 1e-3);
        assert!(
            (tip_position - root_position)
                .normalize()
                .dot((Vec3::new(50.0, 30.0, -20.0) - root_position).normalize())
                > 0.95,
            "CCD should orient an unreachable chain toward the target"
        );
        assert!(
            solved
                .node(root)
                .unwrap()
                .local
                .rotation
                .angle_between(Quat::IDENTITY)
                > 0.05
                || solved
                    .node(a)
                    .unwrap()
                    .local
                    .rotation
                    .angle_between(Quat::IDENTITY)
                    > 0.05
                || solved
                    .node(b)
                    .unwrap()
                    .local
                    .rotation
                    .angle_between(Quat::IDENTITY)
                    > 0.05,
            "the clamped CCD solve must rotate the chain"
        );
        for node in &solved.nodes {
            assert!(node.world.w_axis.truncate().is_finite());
            assert!(node.local.rotation.is_finite());
        }
    }

    #[test]
    fn zero_length_and_missing_ancestor_chains_are_safe() {
        let mut doc = Document::empty();
        let root = joint(&mut doc, "Root", None, Vec3::ZERO);
        let middle = joint(&mut doc, "Middle", Some(root), Vec3::ZERO);
        let tip = joint(&mut doc, "Tip", Some(middle), Vec3::ZERO);
        let goal = target(&mut doc, "Goal", Vec3::new(1.0, 0.0, 0.0));
        let mut chain = IkChain::new(goal);
        chain.chain_length = 2;
        doc.scene.get_mut(tip).unwrap().ik = Some(chain);
        let solved = doc.evaluate(0.0);
        assert!(solved
            .node(tip)
            .unwrap()
            .world
            .w_axis
            .truncate()
            .is_finite());
        assert!(solved.node(root).unwrap().local.rotation.is_finite());
        assert!(
            solved
                .node(root)
                .unwrap()
                .local
                .rotation
                .angle_between(Quat::IDENTITY)
                < 1e-6
        );
        assert!(
            solved
                .node(middle)
                .unwrap()
                .local
                .rotation
                .angle_between(Quat::IDENTITY)
                < 1e-6
        );

        let alone = joint(&mut doc, "Alone", None, Vec3::ZERO);
        let goal = target(&mut doc, "Alone Goal", Vec3::X);
        doc.scene.get_mut(alone).unwrap().ik = Some(IkChain::new(goal));
        let solved = doc.evaluate(0.0);
        assert!(solved
            .node(alone)
            .unwrap()
            .world
            .w_axis
            .truncate()
            .is_finite());
        assert!(
            solved
                .node(alone)
                .unwrap()
                .local
                .rotation
                .angle_between(Quat::IDENTITY)
                < 1e-6
        );
    }

    #[test]
    fn ik_rotations_deform_a_skinned_mesh() {
        let (mut doc, root, middle, tip) = two_bone_setup("skinned", Vec3::new(1.0, 1.2, 0.0));
        let mesh = lc_geom::box_mesh(1.0, 1.0, 1.0);
        let vertex_count = mesh.positions.len();
        let mesh_id = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::editable(mesh),
            },
            "IK Mesh",
            None,
        );
        // Bind against the unsolved rest pose, then evaluate the same skin with
        // the IK chain enabled.
        doc.scene.get_mut(tip).unwrap().ik.as_mut().unwrap().enabled = false;
        let bind = doc.evaluate(0.0);
        let middle_world = bind.node(middle).unwrap().world;
        let mut skin = Skin::new(vec![middle], vec![middle_world.inverse()]);
        skin.clear_weights(vertex_count);
        for vertex in 0..vertex_count {
            assert!(skin.set_influence(vertex, 0, 1.0));
        }
        doc.scene.get_mut(mesh_id).unwrap().skin = Some(skin.clone());
        let rest = doc
            .evaluate(0.0)
            .node(mesh_id)
            .unwrap()
            .mesh()
            .unwrap()
            .positions
            .clone();
        assert!(
            rest.iter().all(|position| position.is_finite()),
            "binding at the rest pose must not move vertices"
        );
        doc.scene.get_mut(tip).unwrap().ik.as_mut().unwrap().enabled = true;
        let posed = doc
            .evaluate(0.0)
            .node(mesh_id)
            .unwrap()
            .mesh()
            .unwrap()
            .positions
            .clone();
        assert!(
            doc.evaluate(0.0)
                .node(root)
                .unwrap()
                .local
                .rotation
                .angle_between(Quat::IDENTITY)
                > 0.05,
            "the mesh must be posed through an actually rotated IK joint"
        );
        let displacement = rest
            .iter()
            .zip(&posed)
            .map(|(a, b)| (*a - *b).length())
            .fold(0.0_f32, f32::max);
        assert!(
            displacement > 0.1,
            "the skinned mesh should follow the IK-driven elbow, got {displacement}"
        );
        assert_eq!(
            doc.scene.get(mesh_id).unwrap().skin.as_ref().unwrap(),
            &skin,
            "solving must not rewrite the authored weights"
        );
        assert!(doc.scene.get(root).unwrap().ik.is_none());
    }

    #[test]
    fn baking_writes_rotation_keys_that_reproduce_the_solved_pose() {
        let (mut doc, root, middle, tip) = two_bone_setup("bake", Vec3::new(1.0, 1.2, 0.0));
        let solved = doc.evaluate(1.0);
        let solved_root = solved.node(root).unwrap().local.rotation;
        let solved_middle = solved.node(middle).unwrap().local.rotation;
        let solved_tip = solved.node(tip).unwrap().world.w_axis.truncate();
        assert!(
            (solved_tip - Vec3::new(1.0, 1.2, 0.0)).length() < 1e-3,
            "the pose being baked must be solved, not the rest pose"
        );
        assert!(
            solved_root.angle_between(Quat::IDENTITY) > 0.05
                && solved_middle.angle_between(Quat::IDENTITY) > 0.05,
            "baking the rest pose would not prove IK baking"
        );

        let edits = bake_ik_edits(&doc, 1.0);
        assert_eq!(edits.len(), 2, "both movable joints should be keyed");
        for (id, _, after) in edits {
            assert!(
                after
                    .rotation
                    .as_ref()
                    .is_some_and(|track| !track.is_empty()),
                "bake must create a rotation F-curve on joint {id}"
            );
            doc.scene.get_mut(id).unwrap().animation = after;
        }
        doc.scene.get_mut(tip).unwrap().ik.as_mut().unwrap().enabled = false;
        let played = doc.evaluate(1.0);
        assert!(
            played
                .node(root)
                .unwrap()
                .local
                .rotation
                .angle_between(solved_root)
                < 1e-4
        );
        assert!(
            played
                .node(middle)
                .unwrap()
                .local
                .rotation
                .angle_between(solved_middle)
                < 1e-4
        );
        assert!((played.node(tip).unwrap().world.w_axis.truncate() - solved_tip).length() < 1e-4);
    }

    #[test]
    fn range_bake_reproduces_solved_frames_without_ik() {
        let (mut doc, _root, _middle, tip) = two_bone_setup("range-bake", Vec3::new(1.0, 1.2, 0.0));
        let target = doc.scene.get(tip).unwrap().ik.as_ref().unwrap().target;
        doc.scene.get_mut(target).unwrap().animation.set_key(
            ChannelAddress::owner_component(AnimChannel::Translation, 0),
            0.0,
            1.0,
            Interpolation::Linear,
        );
        doc.scene.get_mut(target).unwrap().animation.set_key(
            ChannelAddress::owner_component(AnimChannel::Translation, 0),
            1.0,
            0.5,
            Interpolation::Linear,
        );
        let solved_start = doc.evaluate(0.0).node(tip).unwrap().world.w_axis.truncate();
        let solved_end = doc.evaluate(1.0).node(tip).unwrap().world.w_axis.truncate();
        assert!((solved_start - solved_end).length() > 0.1);

        let edits = bake_ik_range_edits(&doc, 0.0, 1.0, 1.0);
        assert_eq!(edits.len(), 2);
        for (id, _, after) in edits {
            assert!(
                after
                    .rotation
                    .as_ref()
                    .is_some_and(|track| track.keys.len() == 2),
                "range bake should key both ends of the take for joint {id}"
            );
            doc.scene.get_mut(id).unwrap().animation = after;
        }
        doc.scene.get_mut(tip).unwrap().ik.as_mut().unwrap().enabled = false;
        let played_start = doc.evaluate(0.0).node(tip).unwrap().world.w_axis.truncate();
        let played_end = doc.evaluate(1.0).node(tip).unwrap().world.w_axis.truncate();
        assert!((played_start - solved_start).length() < 1e-3);
        assert!((played_end - solved_end).length() < 1e-3);
    }

    #[test]
    fn ik_settings_survive_node_props_round_trip() {
        let (doc, _root, _middle, tip) = two_bone_setup("props", Vec3::new(1.0, 1.0, 0.0));
        let node = doc.scene.get(tip).unwrap();
        let props = crate::NodeProps::of(node);
        assert!(props.ik.is_some());
        let mut copy = Node::new(
            tip + 100,
            "Copy",
            NodeKind::Joint {
                length: 1.0,
                width: 0.08,
            },
        );
        props.apply(&mut copy);
        assert_eq!(copy.ik, node.ik);
    }

    fn guide_spline(doc: &mut Document, points: &[Vec3]) -> NodeId {
        let spline = lc_geom::Spline {
            points: points
                .iter()
                .map(|point| lc_geom::SplinePoint::new(*point))
                .collect(),
            kind: lc_geom::SplineKind::Linear,
            closed: false,
            resolution: 4,
        };
        doc.append_node(NodeKind::Spline { spline }, "Guide", None)
    }

    fn spline_chain_setup(
        goal: Vec3,
        points: &[Vec3],
    ) -> (Document, NodeId, NodeId, NodeId, NodeId) {
        let mut doc = Document::empty();
        let root = joint(&mut doc, "Hip", None, Vec3::ZERO);
        let middle = joint(&mut doc, "Knee", Some(root), Vec3::X);
        let tip = joint(&mut doc, "Ankle", Some(middle), Vec3::X);
        let point_target = target(&mut doc, "Goal", goal);
        let spline = guide_spline(&mut doc, points);
        let mut chain = IkChain::new(point_target);
        chain.spline = Some(spline);
        doc.scene.get_mut(tip).unwrap().ik = Some(chain);
        (doc, root, middle, tip, spline)
    }

    fn world_position_of(solved: &crate::EvaluatedScene, id: NodeId) -> Vec3 {
        solved.node(id).unwrap().world.w_axis.truncate()
    }

    #[test]
    fn spline_ik_follows_the_guide_curve_without_stretching() {
        // A guide that rises at 45° for 1.41 m and then runs horizontally.
        let (doc, root, middle, tip, _spline) = spline_chain_setup(
            Vec3::new(2.0, 0.0, 0.0),
            &[
                Vec3::ZERO,
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(2.0, 1.0, 0.0),
            ],
        );
        let solved = doc.evaluate(0.0);
        let hip = world_position_of(&solved, root);
        let knee = world_position_of(&solved, middle);
        let ankle = world_position_of(&solved, tip);
        // Rotating bones keeps their authored length exactly.
        assert!(((knee - hip).length() - 1.0).abs() < 1e-3, "knee {knee}");
        assert!(
            ((ankle - knee).length() - 1.0).abs() < 1e-3,
            "ankle {ankle}"
        );
        // The first bone follows the 45° opening segment of the guide.
        let first = (knee - hip).normalize();
        assert!(
            first.dot(Vec3::new(1.0, 1.0, 0.0).normalize()) > 0.99,
            "first bone {first} does not follow the guide's opening tangent"
        );
        // The chain climbs to the horizontal part instead of ignoring the curve.
        assert!(
            ankle.y > 0.9,
            "the chain must follow the curve upward: {ankle}"
        );
        assert!(
            ankle.x > 1.4,
            "the chain must progress along the curve: {ankle}"
        );
    }

    #[test]
    fn spline_chain_reports_curve_error_and_short_curve_clamp() {
        let (doc, root, middle, tip, spline) = spline_chain_setup(
            Vec3::new(2.0, 0.0, 0.0),
            &[
                Vec3::ZERO,
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(2.0, 1.0, 0.0),
            ],
        );
        let chain = doc.scene.get(tip).unwrap().ik.clone().unwrap();
        let locals: BTreeMap<NodeId, Transform> = [root, middle, tip]
            .iter()
            .map(|id| (*id, doc.scene.get(*id).unwrap().transform))
            .collect();
        let solution = solve_spline_chain(
            &doc.scene,
            tip,
            &chain,
            &locals,
            &BTreeMap::new(),
            spline,
            Mat4::IDENTITY,
        )
        .expect("a linear guide solves the chain");
        assert!(
            solution.error < 0.05,
            "the tip must stay near the guide, error {}",
            solution.error
        );
        assert!(!solution.clamped, "the guide is longer than the chain");
        assert_eq!(solution.iterations, SPLINE_PASSES);
        assert_eq!(solution.joints, vec![root, middle]);
        assert!(solution.tip_position.is_finite());

        // A guide shorter than the chain clamps instead of stretching.
        let (short, _root, _middle, short_tip, short_spline) = spline_chain_setup(
            Vec3::new(2.0, 0.0, 0.0),
            &[Vec3::ZERO, Vec3::new(0.5, 0.0, 0.0)],
        );
        let short_chain = short.scene.get(short_tip).unwrap().ik.clone().unwrap();
        let short_locals: BTreeMap<NodeId, Transform> = short
            .scene
            .nodes
            .values()
            .filter(|node| matches!(node.kind, NodeKind::Joint { .. }))
            .map(|node| (node.id, node.transform))
            .collect();
        let solution = solve_spline_chain(
            &short.scene,
            short_tip,
            &short_chain,
            &short_locals,
            &BTreeMap::new(),
            short_spline,
            Mat4::IDENTITY,
        )
        .expect("the short guide still produces a finite solve");
        assert!(solution.clamped, "a short guide must report clamping");
        assert!(solution.tip_position.is_finite());
        for (_, rotation) in &solution.rotations {
            assert!(rotation.is_finite(), "{rotation:?}");
        }
    }

    #[test]
    fn spline_ik_falls_back_to_the_point_target_for_a_non_spline_node() {
        let (mut doc, _root, _middle, tip, _spline) = spline_chain_setup(
            Vec3::new(1.5, 0.0, 0.5),
            &[Vec3::ZERO, Vec3::new(1.0, 1.0, 0.0)],
        );
        // A null object in the spline slot is not a curve, so the point target
        // keeps driving the chain instead of freezing it.
        let null = target(&mut doc, "Not A Curve", Vec3::new(0.0, 2.0, 0.0));
        doc.scene.get_mut(tip).unwrap().ik.as_mut().unwrap().spline = Some(null);
        let solved = doc.evaluate(0.0);
        let ankle = world_position_of(&solved, tip);
        let goal = Vec3::new(1.5, 0.0, 0.5);
        assert!(
            (ankle - goal).length() < 0.05,
            "the point target must still drive the chain, got {ankle}"
        );
    }

    #[test]
    fn spline_solve_is_deterministic() {
        let (doc, _root, _middle, tip, _spline) = spline_chain_setup(
            Vec3::new(2.0, 0.0, 0.0),
            &[
                Vec3::ZERO,
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(2.0, 1.0, 0.0),
            ],
        );
        let first = doc.evaluate(0.0);
        let second = doc.evaluate(0.0);
        assert_eq!(
            first.node(tip).unwrap().local.rotation,
            second.node(tip).unwrap().local.rotation
        );
        assert_eq!(
            first.node(tip).unwrap().world,
            second.node(tip).unwrap().world
        );
    }

    #[test]
    fn bake_includes_a_spline_chain_whose_point_target_is_gone() {
        let (mut doc, root, middle, _tip, _spline) = spline_chain_setup(
            Vec3::new(2.0, 0.0, 0.0),
            &[
                Vec3::ZERO,
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(2.0, 1.0, 0.0),
            ],
        );
        // The guide drives the chain, so a stale point target must not stop a bake.
        doc.scene.get_mut(_tip).unwrap().ik.as_mut().unwrap().target = 9_999;
        let edits = bake_ik_edits(&doc, 0.0);
        for id in [root, middle] {
            let clip = edits
                .iter()
                .find(|(edited, _, _)| *edited == id)
                .map(|(_, _, after)| after)
                .unwrap_or_else(|| panic!("joint {id} was not baked"));
            assert!(
                clip.sample_rotation(0.0).is_some(),
                "the baked joint {id} needs a rotation key"
            );
        }
    }

    #[test]
    fn ik_chain_reads_legacy_json_without_a_spline_target() {
        let chain: IkChain = serde_json::from_str(r#"{ "target": 7 }"#)
            .expect("legacy chains without a spline field still load");
        assert_eq!(chain.target, 7);
        assert_eq!(chain.spline, None);

        let mut chain = chain;
        chain.spline = Some(3);
        let json = serde_json::to_string(&chain).expect("chains serialize");
        assert!(json.contains("\"spline\":3"), "{json}");
        let round: IkChain = serde_json::from_str(&json).expect("chains round-trip");
        assert_eq!(round.spline, Some(3));

        chain.spline = None;
        let json = serde_json::to_string(&chain).expect("chains serialize");
        assert!(
            !json.contains("spline"),
            "an unset spline stays out of the file"
        );
    }

    fn rotated_null(
        doc: &mut Document,
        name: &str,
        translation: Vec3,
        euler_degrees: Vec3,
    ) -> NodeId {
        let id = target(doc, name, translation);
        doc.scene.get_mut(id).unwrap().transform.rotation =
            Transform::from_euler_degrees(euler_degrees).rotation;
        id
    }

    fn world_aim(solved: &crate::EvaluatedScene, id: NodeId) -> Vec3 {
        solved
            .node(id)
            .unwrap()
            .world
            .transform_vector3(Vec3::X)
            .normalize_or_zero()
    }

    #[test]
    fn stiffness_lands_between_the_rest_pose_and_a_full_solve() {
        let goal = Vec3::new(1.2, 0.8, 0.0);
        let (mut doc, _root, middle, tip) = two_bone_setup("Soft", goal);
        let pole = target(&mut doc, "Pole", Vec3::new(0.0, 0.0, 1.0));
        doc.scene.get_mut(tip).unwrap().ik.as_mut().unwrap().pole = Some(pole);
        // Zero stiffness is the rest pose; the default is a full solve.
        doc.scene
            .get_mut(tip)
            .unwrap()
            .ik
            .as_mut()
            .unwrap()
            .stiffness = 0.0;
        let rest = doc.evaluate(0.0);
        let rest_tip = world_position_of(&rest, tip);
        let rest_middle = world_position_of(&rest, middle);

        doc.scene
            .get_mut(tip)
            .unwrap()
            .ik
            .as_mut()
            .unwrap()
            .stiffness = 1.0;
        let full = doc.evaluate(0.0);
        let full_tip = world_position_of(&full, tip);
        let full_middle = world_position_of(&full, middle);

        doc.scene
            .get_mut(tip)
            .unwrap()
            .ik
            .as_mut()
            .unwrap()
            .stiffness = 0.5;
        let soft = doc.evaluate(0.0);
        let soft_tip = world_position_of(&soft, tip);
        let soft_middle = world_position_of(&soft, middle);

        doc.scene
            .get_mut(tip)
            .unwrap()
            .ik
            .as_mut()
            .unwrap()
            .stiffness = 0.0;
        let frozen_tip = world_position_of(&doc.evaluate(0.0), tip);

        assert!(
            (frozen_tip - rest_tip).length() < 1e-3,
            "zero stiffness must keep the rest pose, got {frozen_tip}"
        );
        assert!(
            (full_tip - goal).length() < 0.02,
            "a full solve still reaches the goal, got {full_tip}"
        );
        let rest_residual = (rest_tip - goal).length();
        let soft_residual = (soft_tip - goal).length();
        let full_residual = (full_tip - goal).length();
        assert!(
            soft_residual < rest_residual - 0.02,
            "half stiffness must correct: {soft_residual} vs rest {rest_residual}"
        );
        assert!(
            soft_residual > full_residual + 0.01,
            "half stiffness must trail the full solve: {soft_residual} vs {full_residual}"
        );
        // The pole still chooses the bend plane, just at a partial correction.
        assert!((soft_middle - rest_middle).length() > 0.02, "{soft_middle}");
        assert!((soft_middle - full_middle).length() > 0.02, "{soft_middle}");
    }

    #[test]
    fn stiffness_softens_ccd_and_spline_chains() {
        let mut doc = Document::empty();
        let a = joint(&mut doc, "A", None, Vec3::ZERO);
        let b = joint(&mut doc, "B", Some(a), Vec3::X);
        let c = joint(&mut doc, "C", Some(b), Vec3::X);
        let tip = joint(&mut doc, "Tip", Some(c), Vec3::X);
        let goal_node = target(&mut doc, "Goal", Vec3::new(2.0, 1.2, 0.0));
        let goal = Vec3::new(2.0, 1.2, 0.0);
        let mut chain = IkChain::new(goal_node);
        chain.chain_length = 3;
        doc.scene.get_mut(tip).unwrap().ik = Some(chain);
        // Zero stiffness is the rest pose; the default is a full solve.
        doc.scene
            .get_mut(tip)
            .unwrap()
            .ik
            .as_mut()
            .unwrap()
            .stiffness = 0.0;
        let rest_tip = world_position_of(&doc.evaluate(0.0), tip);
        assert!(
            (rest_tip - Vec3::new(3.0, 0.0, 0.0)).length() < 1e-3,
            "{rest_tip}"
        );

        doc.scene
            .get_mut(tip)
            .unwrap()
            .ik
            .as_mut()
            .unwrap()
            .stiffness = 1.0;
        let full_tip = world_position_of(&doc.evaluate(0.0), tip);
        doc.scene
            .get_mut(tip)
            .unwrap()
            .ik
            .as_mut()
            .unwrap()
            .stiffness = 0.4;
        let soft_tip = world_position_of(&doc.evaluate(0.0), tip);
        assert!(
            (soft_tip - goal).length() > (full_tip - goal).length() + 0.02,
            "a soft CCD solve must trail the full one: {soft_tip}"
        );
        assert!(
            (soft_tip - goal).length() < (rest_tip - goal).length() - 0.02,
            "a soft CCD solve must still correct: {soft_tip}"
        );

        let points = [
            Vec3::ZERO,
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(2.0, 1.0, 0.0),
        ];
        let (mut doc, _root, _middle, tip, _spline) = spline_chain_setup(Vec3::ZERO, &points);
        doc.scene
            .get_mut(tip)
            .unwrap()
            .ik
            .as_mut()
            .unwrap()
            .stiffness = 0.0;
        let rest_tip = world_position_of(&doc.evaluate(0.0), tip);
        assert!(
            (rest_tip - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-3,
            "{rest_tip}"
        );
        doc.scene
            .get_mut(tip)
            .unwrap()
            .ik
            .as_mut()
            .unwrap()
            .stiffness = 1.0;
        let full_tip = world_position_of(&doc.evaluate(0.0), tip);
        doc.scene
            .get_mut(tip)
            .unwrap()
            .ik
            .as_mut()
            .unwrap()
            .stiffness = 0.5;
        let soft_tip = world_position_of(&doc.evaluate(0.0), tip);
        assert!(
            (soft_tip - rest_tip).length() > 0.05,
            "a soft spline solve must leave the rest pose: {soft_tip}"
        );
        assert!(
            (soft_tip - full_tip).length() > 0.05,
            "a soft spline solve must trail the guide: {soft_tip}"
        );
    }

    #[test]
    fn ik_point_constraint_pulls_the_owner_toward_its_target() {
        let mut doc = Document::empty();
        let owner = target(&mut doc, "Owner", Vec3::ZERO);
        let goal = target(&mut doc, "Goal", Vec3::new(2.0, 0.0, 0.0));

        doc.scene.get_mut(owner).unwrap().tags = vec![crate::Tag::IkPoint {
            target: goal,
            weight: 0.5,
        }];
        let half = world_position_of(&doc.evaluate(0.0), owner);
        assert!(
            (half - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-3,
            "a half-weight point constraint lands halfway, got {half}"
        );

        doc.scene.get_mut(owner).unwrap().tags = vec![crate::Tag::IkPoint {
            target: goal,
            weight: 1.0,
        }];
        let full = world_position_of(&doc.evaluate(0.0), owner);
        assert!((full - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-3, "{full}");

        doc.scene.get_mut(owner).unwrap().tags = vec![crate::Tag::IkPoint {
            target: goal,
            weight: 0.0,
        }];
        let none = world_position_of(&doc.evaluate(0.0), owner);
        assert!(
            none.length() < 1e-3,
            "weight zero leaves the pose alone: {none}"
        );
    }

    #[test]
    fn ik_angle_constraint_keeps_the_aim_inside_its_cone() {
        let mut doc = Document::empty();
        let owner = rotated_null(&mut doc, "Aim", Vec3::ZERO, Vec3::new(0.0, 90.0, 0.0));
        let goal = target(&mut doc, "Goal", Vec3::new(2.0, 0.0, 0.0));
        doc.scene.get_mut(owner).unwrap().tags = vec![crate::Tag::IkAngle {
            target: goal,
            max_degrees: 10.0,
        }];
        let aim = world_aim(&doc.evaluate(0.0), owner);
        let angle = aim.angle_between(Vec3::X).to_degrees();
        assert!(
            (angle - 10.0).abs() < 0.2,
            "the aim must stop on the 10° cone, got {angle}°"
        );

        // An aim already inside the cone keeps its pose.
        let mut inside = Document::empty();
        let owner = rotated_null(&mut inside, "Aim", Vec3::ZERO, Vec3::new(0.0, 5.0, 0.0));
        let goal = target(&mut inside, "Goal", Vec3::new(2.0, 0.0, 0.0));
        inside.scene.get_mut(owner).unwrap().tags = vec![crate::Tag::IkAngle {
            target: goal,
            max_degrees: 10.0,
        }];
        let aim = world_aim(&inside.evaluate(0.0), owner);
        assert!((aim.angle_between(Vec3::X).to_degrees() - 5.0).abs() < 0.2);

        // A clamp tag later in the list still wins over the cone correction.
        let mut clamped = Document::empty();
        let owner = rotated_null(&mut clamped, "Aim", Vec3::ZERO, Vec3::new(0.0, 90.0, 0.0));
        let goal = target(&mut clamped, "Goal", Vec3::new(2.0, 0.0, 0.0));
        clamped.scene.get_mut(owner).unwrap().tags = vec![
            crate::Tag::IkAngle {
                target: goal,
                max_degrees: 60.0,
            },
            crate::Tag::ClampRotation {
                min: Vec3::splat(-10.0),
                max: Vec3::splat(10.0),
            },
        ];
        let solved = clamped.evaluate(0.0);
        assert_rotation_within(
            solved.node(owner).unwrap().local.rotation,
            &all_limits(10.0),
            "angle constraint",
        );
        // Euler clamping reconstructs from the clamped triple, which moves the
        // aim away from what the 60° cone alone would have produced.
        let angle = world_aim(&solved, owner)
            .angle_between(Vec3::X)
            .to_degrees();
        assert!(
            (angle - 60.0).abs() > 5.0,
            "the clamp must override the cone result, got {angle}°"
        );
    }

    #[test]
    fn orient_and_parent_constraints_blend_by_weight() {
        let mut doc = Document::empty();
        let owner = target(&mut doc, "Owner", Vec3::ZERO);
        let source = rotated_null(
            &mut doc,
            "Source",
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, 90.0, 0.0),
        );
        let diagonal = std::f32::consts::FRAC_1_SQRT_2;
        let halfway = Vec3::new(diagonal, 0.0, -diagonal);

        doc.scene.get_mut(owner).unwrap().tags = vec![crate::Tag::Orient {
            target: source,
            weight: 0.5,
        }];
        let solved = doc.evaluate(0.0);
        assert!(
            (world_aim(&solved, owner) - halfway).length() < 1e-2,
            "a half orient lands 45° toward the source, got {:?}",
            world_aim(&solved, owner)
        );
        assert!(
            world_position_of(&solved, owner).length() < 1e-4,
            "orient leaves the position alone"
        );

        doc.scene.get_mut(owner).unwrap().tags = vec![crate::Tag::Orient {
            target: source,
            weight: 1.0,
        }];
        let aim = world_aim(&doc.evaluate(0.0), owner);
        assert!((aim - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-3, "{aim}");

        doc.scene.get_mut(owner).unwrap().tags = vec![crate::Tag::Parent {
            target: source,
            weight: 0.5,
        }];
        let solved = doc.evaluate(0.0);
        assert!(
            (world_position_of(&solved, owner) - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-3,
            "parent blends the position"
        );
        assert!((world_aim(&solved, owner) - halfway).length() < 1e-2);

        doc.scene.get_mut(owner).unwrap().tags = vec![crate::Tag::Parent {
            target: source,
            weight: 1.0,
        }];
        let solved = doc.evaluate(0.0);
        assert!((world_position_of(&solved, owner) - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-3);
        assert!((world_aim(&solved, owner) - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-3);
    }

    #[test]
    fn constraint_driven_joint_bakes_and_evaluates_deterministically() {
        let mut doc = Document::empty();
        let source = rotated_null(
            &mut doc,
            "Source",
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 45.0),
        );
        let driven = joint(&mut doc, "Driven", None, Vec3::ZERO);
        doc.scene.get_mut(driven).unwrap().tags = vec![crate::Tag::Orient {
            target: source,
            weight: 1.0,
        }];
        let first = doc.evaluate(0.0);
        let second = doc.evaluate(0.0);
        assert_eq!(
            first.node(driven).unwrap().world,
            second.node(driven).unwrap().world
        );

        let expected = first.node(driven).unwrap().local.rotation;
        let edits = bake_ik_edits(&doc, 0.0);
        let (_, _, after) = edits
            .iter()
            .find(|(id, _, _)| *id == driven)
            .unwrap_or_else(|| panic!("a constraint-only joint must bake, got {edits:?}"));
        let baked = after
            .sample_rotation(0.0)
            .expect("the bake writes a rotation key");
        assert!(
            baked.dot(expected).abs() > 0.999,
            "the bake must key the constrained pose, got {baked:?} want {expected:?}"
        );
    }

    #[test]
    fn constraint_tags_read_legacy_json_and_keep_defaults_out_of_the_file() {
        let tag: crate::Tag = serde_json::from_str(r#"{ "type": "orient", "target": 3 }"#)
            .expect("an orient tag without a weight loads");
        assert!(
            matches!(tag, crate::Tag::Orient { target: 3, weight } if (weight - 1.0).abs() < 1e-6),
            "{tag:?}"
        );
        let tag: crate::Tag = serde_json::from_str(r#"{ "type": "ik_angle", "target": 2 }"#)
            .expect("an angle tag without a cone loads");
        assert!(
            matches!(tag, crate::Tag::IkAngle { target: 2, max_degrees } if (max_degrees - 45.0).abs() < 1e-6),
            "{tag:?}"
        );
        let tag: crate::Tag = serde_json::from_str(r#"{ "type": "parent", "target": 4 }"#)
            .expect("a parent tag without a weight loads");
        assert!(matches!(tag, crate::Tag::Parent { target: 4, .. }));
        let tag: crate::Tag = serde_json::from_str(
            r#"{ "type": "copy_location", "source": 5, "offset": [0, 0, 0] }"#,
        )
        .expect("legacy tags still load");
        assert!(
            matches!(tag, crate::Tag::CopyLocation { source: 5, weight, .. } if (weight - 1.0).abs() < 1e-6)
        );

        let json = serde_json::to_string(&crate::Tag::Orient {
            target: 1,
            weight: 0.5,
        })
        .expect("tags serialize");
        assert!(json.contains("\"weight\":0.5"), "{json}");
        let round: crate::Tag = serde_json::from_str(&json).expect("orient tags round-trip");
        assert!(
            matches!(round, crate::Tag::Orient { target: 1, weight } if (weight - 0.5).abs() < 1e-6)
        );

        let chain: IkChain = serde_json::from_str(r#"{ "target": 7 }"#)
            .expect("legacy chains without stiffness still load");
        assert!((chain.stiffness - 1.0).abs() < 1e-6);
        let json = serde_json::to_string(&chain).expect("chains serialize");
        assert!(
            !json.contains("stiffness"),
            "a full solve stays out: {json}"
        );
    }
}
