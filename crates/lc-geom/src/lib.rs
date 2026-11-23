//! Geometry for LeopardCraft: meshes, primitives, splines, modelling tools,
//! subdivision, UV mapping, boolean operations and ray acceleration.
//!
//! The crate is pure Rust and headless so it can be tested and used by the
//! renderer, exporters and the UI alike.

pub mod bvh;
pub mod csg;
pub mod mesh;
pub mod ops;
pub mod primitives;
pub mod spline;
pub mod subdiv;
pub mod uv;

pub use bvh::{Bvh, BvhHit, BvhObject};
pub use csg::{boolean, intersect, subtract, union, BooleanOp};
pub use mesh::{Corner, Mesh, MeshEdge, Polygon, RenderTriangle, RenderVertex};
pub use ops::{
    array_mesh, bend, bevel_faces, bridge_loops, bulge, can_disconnect, can_optimize,
    can_slide_points, collapse_points, cover_faces, coverable_holes, create_polygon_faces,
    disconnect_faces, extract_faces, extrude_faces, extrude_profile, faceable_loops, fill_holes,
    flip_faces, inset_faces, lathe, open_holes, optimize_points, ring_cut, shear, shell_faces,
    shell_region, slide_points, spherify, sweep, sweep_spline, symmetry_mesh, taper,
    triangulate_faces, twist,
};
pub use primitives::{
    box_mesh, capsule_mesh, capsule_profile, cone_mesh, cylinder_mesh, disc_mesh, disc_profile,
    plane_mesh, sphere_mesh, torus_mesh,
};
pub use spline::{Spline, SplineKind, SplinePoint};
pub use subdiv::{catmull_clark, linear as linear_subdivision, stam_loop};
pub use uv::{
    automatic_unwrap, box_projection, cylindrical_projection, normalize_uvs, planar_projection,
    relax_uvs, spherical_projection,
};

/// Crease angle used for viewport shading and export normals.
pub const CREASE_ANGLE_DEG: f32 = 40.0;
