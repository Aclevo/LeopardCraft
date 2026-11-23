//! Parametric geometry sources and non-destructive modifier stacks.

use crate::NodeId;
use glam::{Vec2, Vec3};
use lc_geom::{BooleanOp as GeomBooleanOp, Mesh, Spline};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Primitive {
    Box {
        width: f32,
        height: f32,
        depth: f32,
    },
    Sphere {
        radius: f32,
        segments: u32,
        rings: u32,
    },
    Cylinder {
        radius: f32,
        height: f32,
        segments: u32,
    },
    Capsule {
        radius: f32,
        height: f32,
        segments: u32,
        rings: u32,
    },
    Disc {
        radius: f32,
        segments: u32,
    },
    Cone {
        radius: f32,
        height: f32,
        segments: u32,
    },
    Torus {
        major_radius: f32,
        minor_radius: f32,
        segments: u32,
        rings: u32,
    },
    Plane {
        width: f32,
        depth: f32,
        segments_x: u32,
        segments_z: u32,
    },
}

impl Default for Primitive {
    fn default() -> Self {
        Self::Box {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }
    }
}

impl Primitive {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Box { .. } => "Box",
            Self::Sphere { .. } => "Sphere",
            Self::Cylinder { .. } => "Cylinder",
            Self::Capsule { .. } => "Capsule",
            Self::Disc { .. } => "Disc",
            Self::Cone { .. } => "Cone",
            Self::Torus { .. } => "Torus",
            Self::Plane { .. } => "Plane",
        }
    }

    /// Repair non-finite dimensions to zero, matching the load-time mesh
    /// repair. Every builder clamps its magnitude to a tiny positive value, so
    /// a repaired dimension still produces finite geometry.
    pub fn sanitize(&mut self) {
        fn repair(value: &mut f32) {
            if !value.is_finite() {
                *value = 0.0;
            }
        }
        match self {
            Self::Box {
                width,
                height,
                depth,
            } => {
                repair(width);
                repair(height);
                repair(depth);
            }
            Self::Sphere { radius, .. } => repair(radius),
            Self::Cylinder { radius, height, .. } => {
                repair(radius);
                repair(height);
            }
            Self::Capsule { radius, height, .. } => {
                repair(radius);
                repair(height);
            }
            Self::Disc { radius, .. } => repair(radius),
            Self::Cone { radius, height, .. } => {
                repair(radius);
                repair(height);
            }
            Self::Torus {
                major_radius,
                minor_radius,
                ..
            } => {
                repair(major_radius);
                repair(minor_radius);
            }
            Self::Plane { width, depth, .. } => {
                repair(width);
                repair(depth);
            }
        }
    }

    pub fn build(&self) -> Mesh {
        match *self {
            Self::Box {
                width,
                height,
                depth,
            } => lc_geom::box_mesh(width, height, depth),
            Self::Sphere {
                radius,
                segments,
                rings,
            } => {
                let mut m = lc_geom::sphere_mesh(radius, segments, rings);
                m.name = "Sphere".into();
                m
            }
            Self::Cylinder {
                radius,
                height,
                segments,
            } => lc_geom::cylinder_mesh(radius, height, segments),
            Self::Capsule {
                radius,
                height,
                segments,
                rings,
            } => lc_geom::capsule_mesh(radius, height, segments, rings),
            Self::Disc { radius, segments } => lc_geom::disc_mesh(radius, segments),
            Self::Cone {
                radius,
                height,
                segments,
            } => lc_geom::cone_mesh(radius, height, segments),
            Self::Torus {
                major_radius,
                minor_radius,
                segments,
                rings,
            } => lc_geom::torus_mesh(major_radius, minor_radius, segments, rings),
            Self::Plane {
                width,
                depth,
                segments_x,
                segments_z,
            } => lc_geom::plane_mesh(width, depth, segments_x, segments_z),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GeometrySource {
    Primitive {
        primitive: Primitive,
    },
    /// Directly editable n-gon mesh stored in the document.
    Editable {
        mesh: Mesh,
        /// The primitive this mesh was converted from, retained as display
        /// metadata so the object browser can keep the familiar shape icon.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<Primitive>,
    },
}

impl Default for GeometrySource {
    fn default() -> Self {
        Self::Primitive {
            primitive: Primitive::default(),
        }
    }
}

impl GeometrySource {
    pub fn from_primitive(primitive: Primitive) -> Self {
        Self::Primitive { primitive }
    }

    pub fn editable(mesh: Mesh) -> Self {
        Self::Editable { mesh, origin: None }
    }

    pub fn editable_from_primitive(mesh: Mesh, primitive: Primitive) -> Self {
        Self::Editable {
            mesh,
            origin: Some(primitive),
        }
    }

    /// Replace the editable mesh while preserving any primitive provenance.
    /// Primitive sources become editable on demand, keeping their shape hint.
    pub fn with_mesh(&self, mesh: Mesh) -> Self {
        Self::Editable {
            mesh,
            origin: self.origin_copied(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Primitive { primitive } => primitive.label(),
            Self::Editable { .. } => "Editable Mesh",
        }
    }

    pub fn primitive(&self) -> Option<&Primitive> {
        match self {
            Self::Primitive { primitive } => Some(primitive),
            _ => None,
        }
    }

    /// The primitive this geometry came from, if it is still a live primitive
    /// or an editable mesh converted from one.
    pub fn origin(&self) -> Option<&Primitive> {
        match self {
            Self::Primitive { primitive } => Some(primitive),
            Self::Editable { origin, .. } => origin.as_ref(),
        }
    }

    pub fn origin_copied(&self) -> Option<Primitive> {
        self.origin().copied()
    }

    pub fn mesh(&self) -> Option<&Mesh> {
        match self {
            Self::Editable { mesh, .. } => Some(mesh),
            _ => None,
        }
    }

    pub fn build(&self) -> Mesh {
        match self {
            // The inspector and undo write primitive fields in place, so the
            // document geometry entry repairs a copy before evaluation and
            // export build it.
            Self::Primitive { primitive } => {
                let mut repaired = *primitive;
                repaired.sanitize();
                repaired.build()
            }
            Self::Editable { mesh, .. } => mesh.clone(),
        }
    }

    /// Convert a primitive to a directly editable mesh (used by mesh tools).
    pub fn make_editable(&mut self) -> bool {
        match self {
            Self::Primitive { primitive } => {
                let mut repaired = *primitive;
                repaired.sanitize();
                *self = Self::editable_from_primitive(repaired.build(), repaired);
                true
            }
            Self::Editable { .. } => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CsgOperation {
    Union,
    Subtract,
    Intersect,
}

impl CsgOperation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Union => "Union",
            Self::Subtract => "Subtract",
            Self::Intersect => "Intersect",
        }
    }

    pub fn to_geom(self) -> GeomBooleanOp {
        match self {
            Self::Union => GeomBooleanOp::Union,
            Self::Subtract => GeomBooleanOp::Subtract,
            Self::Intersect => GeomBooleanOp::Intersect,
        }
    }
}

/// Subdivision surface schemes documented for Cheetah3D's Subdivision modifier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubdivisionScheme {
    #[default]
    CatmullClark,
    StamLoop,
    Linear,
}

impl SubdivisionScheme {
    pub fn label(self) -> &'static str {
        match self {
            Self::CatmullClark => "Catmull-Clark",
            Self::StamLoop => "Stam-Loop",
            Self::Linear => "Linear",
        }
    }
}

/// A modifier in a node's non-destructive stack.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Modifier {
    Subdivision {
        #[serde(default = "default_levels")]
        levels: u32,
        #[serde(default)]
        scheme: SubdivisionScheme,
    },
    ExtrudeFaces {
        distance: f32,
        #[serde(default)]
        individual: bool,
        /// Polygon indices; empty means all polygons.
        #[serde(default)]
        faces: Vec<u32>,
    },
    BevelFaces {
        amount: f32,
        depth: f32,
        #[serde(default)]
        faces: Vec<u32>,
    },
    Array {
        count: u32,
        offset: Vec3,
        #[serde(default)]
        rotate_y_deg: f32,
    },
    Symmetry {
        axis: u8,
        threshold: f32,
    },
    Boolean {
        operand: NodeId,
        operation: CsgOperation,
    },
    Bend {
        angle_deg: f32,
    },
    Taper {
        axis: u8,
        amount: f32,
    },
    Twist {
        angle_deg: f32,
        #[serde(default = "default_axis_y")]
        axis: u8,
    },
    Shear {
        distance: f32,
        #[serde(default)]
        direction_deg: f32,
        #[serde(default = "default_axis_y")]
        axis: u8,
    },
    Bulge {
        strength: f32,
        #[serde(default = "default_true")]
        x_axis: bool,
        #[serde(default = "default_true")]
        z_axis: bool,
    },
    Spherify {
        radius: f32,
        strength: f32,
    },
    Lathe {
        profile: Vec<Vec2>,
        segments: u32,
        angle_deg: f32,
        #[serde(default = "default_true")]
        caps: bool,
    },
    Sweep {
        profile: Vec<Vec2>,
        #[serde(default = "default_true")]
        caps: bool,
    },
    RingCut {
        axis: u8,
        offset: f32,
    },
}

fn default_levels() -> u32 {
    2
}

fn default_axis_y() -> u8 {
    1
}

fn default_true() -> bool {
    true
}

impl Modifier {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Subdivision { .. } => "Subdivision Surface",
            Self::ExtrudeFaces { .. } => "Extrude Faces",
            Self::BevelFaces { .. } => "Bevel Faces",
            Self::Array { .. } => "Array",
            Self::Symmetry { .. } => "Symmetry",
            Self::Boolean { .. } => "Boolean",
            Self::Bend { .. } => "Bend",
            Self::Taper { .. } => "Taper",
            Self::Twist { .. } => "Twist",
            Self::Shear { .. } => "Shear",
            Self::Bulge { .. } => "Bulge",
            Self::Spherify { .. } => "Spherify",
            Self::Lathe { .. } => "Lathe",
            Self::Sweep { .. } => "Sweep",
            Self::RingCut { .. } => "Ring Cut",
        }
    }

    pub fn defaults() -> Vec<Modifier> {
        vec![
            Modifier::Subdivision {
                levels: 2,
                scheme: SubdivisionScheme::CatmullClark,
            },
            Modifier::ExtrudeFaces {
                distance: 0.2,
                individual: false,
                faces: Vec::new(),
            },
            Modifier::BevelFaces {
                amount: 0.08,
                depth: 0.02,
                faces: Vec::new(),
            },
            Modifier::Array {
                count: 3,
                offset: Vec3::new(1.5, 0.0, 0.0),
                rotate_y_deg: 0.0,
            },
            Modifier::Symmetry {
                axis: 0,
                threshold: 1e-4,
            },
            Modifier::Boolean {
                operand: 0,
                operation: CsgOperation::Subtract,
            },
            Modifier::Bend { angle_deg: 30.0 },
            Modifier::Taper {
                axis: 1,
                amount: 0.4,
            },
            Modifier::Twist {
                angle_deg: 45.0,
                axis: 1,
            },
            Modifier::Shear {
                distance: 0.4,
                direction_deg: 0.0,
                axis: 1,
            },
            Modifier::Bulge {
                strength: 0.3,
                x_axis: true,
                z_axis: true,
            },
            Modifier::Spherify {
                radius: 1.0,
                strength: 0.5,
            },
            Modifier::Lathe {
                profile: vec![
                    Vec2::new(0.0, -1.0),
                    Vec2::new(0.8, -1.0),
                    Vec2::new(1.0, 0.0),
                    Vec2::new(0.8, 1.0),
                    Vec2::new(0.0, 1.0),
                ],
                segments: 32,
                angle_deg: 360.0,
                caps: true,
            },
            Modifier::Sweep {
                profile: lc_geom::disc_profile(0.18, 16),
                caps: true,
            },
            Modifier::RingCut {
                axis: 1,
                offset: 0.0,
            },
        ]
    }
}

impl Modifier {
    /// Whether every numeric parameter is finite. A non-finite parameter would
    /// otherwise be multiplied into vertex positions during evaluation.
    pub fn parameters_are_finite(&self) -> bool {
        match self {
            Self::Subdivision { .. } | Self::Boolean { .. } => true,
            Self::ExtrudeFaces { distance, .. } => distance.is_finite(),
            Self::BevelFaces { amount, depth, .. } => amount.is_finite() && depth.is_finite(),
            Self::Array {
                offset,
                rotate_y_deg,
                ..
            } => offset.is_finite() && rotate_y_deg.is_finite(),
            Self::Symmetry { threshold, .. } => threshold.is_finite(),
            Self::Bend { angle_deg } => angle_deg.is_finite(),
            Self::Taper { amount, .. } => amount.is_finite(),
            Self::Twist { angle_deg, .. } => angle_deg.is_finite(),
            Self::Shear {
                distance,
                direction_deg,
                ..
            } => distance.is_finite() && direction_deg.is_finite(),
            Self::Bulge { strength, .. } => strength.is_finite(),
            Self::Spherify { radius, strength } => radius.is_finite() && strength.is_finite(),
            Self::Lathe {
                profile, angle_deg, ..
            } => angle_deg.is_finite() && profile.iter().all(|point| point.is_finite()),
            Self::Sweep { profile, .. } => profile.iter().all(|point| point.is_finite()),
            Self::RingCut { offset, .. } => offset.is_finite(),
        }
    }

    /// Repair non-finite numeric parameters to zero, matching the load-time
    /// mesh repair. Applied on load and again before evaluation so a value
    /// written directly into the modifier stack cannot turn finite geometry
    /// into NaN.
    pub fn sanitize(&mut self) {
        fn repair(value: &mut f32) {
            if !value.is_finite() {
                *value = 0.0;
            }
        }
        fn repair_vec2(value: &mut Vec2) {
            repair(&mut value.x);
            repair(&mut value.y);
        }
        fn repair_vec3(value: &mut Vec3) {
            repair(&mut value.x);
            repair(&mut value.y);
            repair(&mut value.z);
        }
        match self {
            Self::Subdivision { .. } | Self::Boolean { .. } => {}
            Self::ExtrudeFaces { distance, .. } => repair(distance),
            Self::BevelFaces { amount, depth, .. } => {
                repair(amount);
                repair(depth);
            }
            Self::Array {
                offset,
                rotate_y_deg,
                ..
            } => {
                repair_vec3(offset);
                repair(rotate_y_deg);
            }
            Self::Symmetry { threshold, .. } => repair(threshold),
            Self::Bend { angle_deg } => repair(angle_deg),
            Self::Taper { amount, .. } => repair(amount),
            Self::Twist { angle_deg, .. } => repair(angle_deg),
            Self::Shear {
                distance,
                direction_deg,
                ..
            } => {
                repair(distance);
                repair(direction_deg);
            }
            Self::Bulge { strength, .. } => repair(strength),
            Self::Spherify { radius, strength } => {
                repair(radius);
                repair(strength);
            }
            Self::Lathe {
                profile, angle_deg, ..
            } => {
                repair(angle_deg);
                for point in profile {
                    repair_vec2(point);
                }
            }
            Self::Sweep { profile, .. } => {
                for point in profile {
                    repair_vec2(point);
                }
            }
            Self::RingCut { offset, .. } => repair(offset),
        }
    }
}

/// Inputs available to a modifier during evaluation.
pub struct ModifierInput<'a> {
    pub mesh: Option<&'a Mesh>,
    /// The node's spline, when it is a spline object (for Sweep).
    pub spline: Option<&'a Spline>,
    /// Boolean operand already transformed into the target's local space.
    pub operand: Option<&'a Mesh>,
}

impl Modifier {
    /// Apply this stack entry. Returns `None` when the modifier has no effect.
    pub fn apply(&self, input: ModifierInput) -> Option<Mesh> {
        match self {
            Self::Subdivision { levels, scheme } => {
                let mesh = input.mesh?;
                if *levels == 0 || mesh.polygons.is_empty() {
                    return None;
                }
                Some(match scheme {
                    SubdivisionScheme::CatmullClark => lc_geom::catmull_clark(mesh, *levels),
                    SubdivisionScheme::StamLoop => lc_geom::stam_loop(mesh, *levels),
                    SubdivisionScheme::Linear => lc_geom::linear_subdivision(mesh, *levels),
                })
            }
            Self::ExtrudeFaces {
                distance,
                individual,
                faces,
            } => {
                let mesh = input.mesh?;
                let faces = if faces.is_empty() {
                    (0..mesh.polygons.len() as u32).collect()
                } else {
                    faces.clone()
                };
                Some(lc_geom::extrude_faces(mesh, &faces, *distance, *individual))
            }
            Self::BevelFaces {
                amount,
                depth,
                faces,
            } => {
                let mesh = input.mesh?;
                let faces = if faces.is_empty() {
                    (0..mesh.polygons.len() as u32).collect()
                } else {
                    faces.clone()
                };
                Some(lc_geom::bevel_faces(mesh, &faces, *amount, *depth))
            }
            Self::Array {
                count,
                offset,
                rotate_y_deg,
            } => {
                let mesh = input.mesh?;
                Some(lc_geom::array_mesh(mesh, *count, *offset, *rotate_y_deg))
            }
            Self::Symmetry { axis, threshold } => {
                let mesh = input.mesh?;
                Some(lc_geom::symmetry_mesh(
                    mesh,
                    (*axis).min(2) as usize,
                    *threshold,
                ))
            }
            Self::Boolean { operation, .. } => {
                let mesh = input.mesh?;
                let operand = input.operand?;
                if operand.is_empty() || mesh.is_empty() {
                    return None;
                }
                Some(lc_geom::boolean(mesh, operand, operation.to_geom()))
            }
            Self::Bend { angle_deg } => {
                let mesh = input.mesh?;
                let center = mesh.bounds().center();
                Some(lc_geom::bend(mesh, *angle_deg, center))
            }
            Self::Taper { axis, amount } => {
                let mesh = input.mesh?;
                let center = mesh.bounds().center();
                Some(lc_geom::taper(
                    mesh,
                    (*axis).min(2) as usize,
                    *amount,
                    center,
                ))
            }
            Self::Twist { angle_deg, axis } => {
                let mesh = input.mesh?;
                let center = mesh.bounds().center();
                Some(lc_geom::twist(
                    mesh,
                    (*axis).min(2) as usize,
                    *angle_deg,
                    center,
                ))
            }
            Self::Shear {
                distance,
                direction_deg,
                axis,
            } => {
                let mesh = input.mesh?;
                let center = mesh.bounds().center();
                Some(lc_geom::shear(
                    mesh,
                    (*axis).min(2) as usize,
                    *distance,
                    *direction_deg,
                    center,
                ))
            }
            Self::Bulge {
                strength,
                x_axis,
                z_axis,
            } => {
                let mesh = input.mesh?;
                Some(lc_geom::bulge(mesh, *strength, *x_axis, *z_axis))
            }
            Self::Spherify { radius, strength } => {
                let mesh = input.mesh?;
                Some(lc_geom::spherify(mesh, *radius, *strength))
            }
            Self::Lathe {
                profile,
                segments,
                angle_deg,
                caps,
            } => {
                if profile.len() < 2 {
                    return None;
                }
                Some(lc_geom::lathe(profile, *segments, *angle_deg, *caps))
            }
            Self::Sweep { profile, caps } => {
                let spline = input.spline?;
                if profile.len() < 3 {
                    return None;
                }
                let path = spline.sample();
                if path.len() < 2 {
                    return None;
                }
                Some(lc_geom::sweep(profile, &path, spline.closed, *caps))
            }
            Self::RingCut { axis, offset } => {
                let mesh = input.mesh?;
                Some(lc_geom::ring_cut(mesh, (*axis).min(2) as usize, *offset))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_builders_are_valid() {
        for p in [
            Primitive::default(),
            Primitive::Sphere {
                radius: 1.0,
                segments: 16,
                rings: 8,
            },
            Primitive::Torus {
                major_radius: 1.0,
                minor_radius: 0.2,
                segments: 16,
                rings: 8,
            },
            Primitive::Capsule {
                radius: 0.5,
                height: 2.0,
                segments: 16,
                rings: 8,
            },
            Primitive::Disc {
                radius: 1.0,
                segments: 24,
            },
        ] {
            let mesh = p.build();
            assert!(mesh.validate().is_ok());
            assert!(!mesh.polygons.is_empty());
        }
    }

    #[test]
    fn editing_converts_primitive() {
        let mut src = GeometrySource::default();
        let original = src.origin_copied();
        assert!(src.make_editable());
        assert!(matches!(src, GeometrySource::Editable { .. }));
        assert_eq!(src.origin_copied(), original);
        assert!(!src.make_editable());
    }

    #[test]
    fn replacing_an_editable_mesh_keeps_its_origin() {
        let sphere = Primitive::Sphere {
            radius: 2.0,
            segments: 16,
            rings: 8,
        };
        let mut src = GeometrySource::from_primitive(sphere);
        assert!(src.make_editable());
        let replaced = src.with_mesh(sphere.build());
        assert_eq!(replaced.origin_copied(), Some(sphere));
        assert!(replaced.mesh().is_some());
    }

    #[test]
    fn subdivision_schemes_produce_distinct_shapes() {
        let cube = Primitive::Box {
            width: 2.0,
            height: 2.0,
            depth: 2.0,
        }
        .build();
        let input = || ModifierInput {
            mesh: Some(&cube),
            spline: None,
            operand: None,
        };
        let smooth = Modifier::Subdivision {
            levels: 2,
            scheme: SubdivisionScheme::CatmullClark,
        }
        .apply(input())
        .unwrap();
        let linear = Modifier::Subdivision {
            levels: 2,
            scheme: SubdivisionScheme::Linear,
        }
        .apply(input())
        .unwrap();
        assert_eq!(smooth.polygons.len(), linear.polygons.len());
        assert!(smooth.bounds().size().x < linear.bounds().size().x - 1e-3);
        assert!(linear.validate().is_ok());
    }

    #[test]
    fn deformer_modifiers_apply_to_meshes() {
        let cube = Primitive::Box {
            width: 2.0,
            height: 4.0,
            depth: 2.0,
        }
        .build();
        let input = || ModifierInput {
            mesh: Some(&cube),
            spline: None,
            operand: None,
        };
        for modifier in [
            Modifier::Twist {
                angle_deg: 90.0,
                axis: 1,
            },
            Modifier::Shear {
                distance: 1.0,
                direction_deg: 0.0,
                axis: 1,
            },
            Modifier::Bulge {
                strength: 0.4,
                x_axis: true,
                z_axis: true,
            },
            Modifier::Spherify {
                radius: 1.0,
                strength: 1.0,
            },
        ] {
            let out = modifier.apply(input()).expect("deformer output");
            assert!(out.validate().is_ok(), "{}", modifier.label());
            assert!(out.positions.iter().all(|p| p.is_finite()));
        }
    }

    #[test]
    fn extrude_and_bevel_modifiers_repair_non_finite_primitive_positions() {
        // `serde_json` casts an out-of-range JSON float to f32 infinity, and
        // primitive dimensions are not touched by `Document::sanitize`, so a
        // crafted native file can build a mesh with non-finite positions.
        let primitive: Primitive =
            serde_json::from_str(r#"{"type":"box","width":1e39,"height":1.0,"depth":1.0}"#)
                .expect("out-of-range primitive dimension deserializes");
        let mesh = primitive.build();
        assert!(
            mesh.positions.iter().any(|p| !p.is_finite()),
            "the crafted primitive must produce non-finite positions",
        );
        let input = || ModifierInput {
            mesh: Some(&mesh),
            spline: None,
            operand: None,
        };
        let extruded = Modifier::ExtrudeFaces {
            distance: 0.5,
            individual: false,
            faces: Vec::new(),
        }
        .apply(input())
        .expect("extrude applies");
        assert!(
            extruded.positions.iter().all(|p| p.is_finite()),
            "extrude modifier emitted non-finite positions",
        );
        assert!(extruded.validate().is_ok());

        let beveled = Modifier::BevelFaces {
            amount: 0.1,
            depth: 0.0,
            faces: Vec::new(),
        }
        .apply(input())
        .expect("bevel applies");
        assert!(
            beveled.positions.iter().all(|p| p.is_finite()),
            "bevel modifier emitted non-finite positions",
        );
        assert!(beveled.validate().is_ok());
    }

    #[test]
    fn modifier_stack_roundtrips_through_serde() {
        let stack = vec![
            Modifier::Subdivision {
                levels: 2,
                scheme: SubdivisionScheme::Linear,
            },
            Modifier::Twist {
                angle_deg: 30.0,
                axis: 2,
            },
            Modifier::Shear {
                distance: 0.5,
                direction_deg: 20.0,
                axis: 1,
            },
            Modifier::Bulge {
                strength: -0.2,
                x_axis: true,
                z_axis: false,
            },
            Modifier::Spherify {
                radius: 2.0,
                strength: 0.25,
            },
        ];
        let json = serde_json::to_string(&stack).unwrap();
        let back: Vec<Modifier> = serde_json::from_str(&json).unwrap();
        assert_eq!(stack, back);
    }

    #[test]
    fn legacy_subdivision_without_scheme_defaults_to_catmull_clark() {
        let json = r#"[{"type":"subdivision","levels":2}]"#;
        let stack: Vec<Modifier> = serde_json::from_str(json).unwrap();
        assert!(matches!(
            stack[0],
            Modifier::Subdivision {
                levels: 2,
                scheme: SubdivisionScheme::CatmullClark
            }
        ));
    }

    #[test]
    fn modifiers_apply_without_panicking_on_missing_inputs() {
        for m in Modifier::defaults() {
            let _ = m.apply(ModifierInput {
                mesh: None,
                spline: None,
                operand: None,
            });
        }
    }
}
