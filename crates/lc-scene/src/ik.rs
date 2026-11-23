//! Inverse kinematics for joint chains.
//!
//! A chain is attached to the selected tip joint. `chain_length` counts the
//! consecutive movable Joint ancestors above that tip; the tip's origin is the
//! end effector. Exactly two movable joints use a closed-form two-bone solve
//! with an optional pole. Longer (and shorter) chains use damped cyclic
//! coordinate descent. Neither solver stretches bones: unreachable targets are
//! clamped to the chain's reach.

use crate::{Document, NodeId, NodeKind, Scene};
use glam::{Mat4, Quat, Vec3};
use lc_anim::{AnimationClip, Interpolation};
use lc_core::Transform;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const EPSILON: f32 = 1e-6;
const MAX_ITERATIONS: usize = 128;
const DEFAULT_CHAIN_LENGTH: usize = 2;
const DEFAULT_ITERATIONS: usize = 24;
const DEFAULT_TOLERANCE: f32 = 1e-3;
const DEFAULT_DAMPING: f32 = 1.0;
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
        if !chain.enabled || doc.scene.get(chain.target).is_none() {
            continue;
        }
        joints.extend(ik_chain_joints(&doc.scene, tip, chain));
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
                after.set_rotation_key(time, node.local.rotation, Interpolation::Smooth);
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
    use lc_anim::AnimProperty;

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
            AnimProperty::TranslationX,
            0.0,
            1.0,
            Interpolation::Linear,
        );
        doc.scene.get_mut(target).unwrap().animation.set_key(
            AnimProperty::TranslationX,
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
}
