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
pub use mesh::{Corner, Mesh, MeshEdge, Polygon, RenderTriangle, RenderVertex, WeldTarget};
pub use ops::{
    array_mesh, bend, bevel_faces, bridge_loops, bulge, can_disconnect, can_optimize,
    can_slide_points, clone_faces, clone_points, collapse_points, cover_faces, coverable_holes,
    create_polygon_faces, disconnect_faces, displace, extract_faces, extrude_faces,
    extrude_profile, faceable_loops, fill_holes, flip_faces, formula, formula_expression_is_valid,
    insert_point, inset_faces, laplacian_smooth, lathe, magnet, open_holes, optimize_points,
    reduce_polygons, ring_cut, scalpel_face, scalpel_face_ends, shear, shell_faces, shell_region,
    slide_points, spherify, sweep, sweep_spline, symmetry_mesh, taper, transform_points,
    triangulate_faces, triangulate_faces_with, twist, wrap_mesh, CloneOptions, InsertPointPick,
    ScalpelPick, TriangulateMode,
};
pub use primitives::{
    box_mesh, capsule_mesh, capsule_profile, cone_mesh, cylinder_mesh, disc_mesh, disc_profile,
    plane_mesh, sphere_mesh, torus_mesh,
};
pub use spline::{Spline, SplineKind, SplinePoint};
pub use subdiv::{
    catmull_clark, linear as linear_subdivision, stam_loop, subdivide_faces, SubdivisionKind,
};
pub use uv::{
    align_uv_slots, automatic_unwrap, box_projection, box_projection_in_set, charts_for_uv_slots,
    cylindrical_projection, cylindrical_projection_in_set, distribute_uv_slots, flip_uv_slots,
    normalize_uvs, pack_uv_charts, planar_projection, planar_projection_in_set, relax_uvs,
    rotate_uv_slots, spherical_projection, spherical_projection_in_set, split_uv_slots,
    transform_uv_charts, uv_chart_indices, uv_charts, uv_distortion_stats, uv_face_distortion,
    uv_island_slots, weld_uv_slots, UvAxis, UvChart, UvDistortionStats, UvFaceDistortion,
    UvPackAlign, UvPackOptions, UvPackReport,
};

/// Crease angle used for viewport shading and export normals.
pub const CREASE_ANGLE_DEG: f32 = 40.0;
