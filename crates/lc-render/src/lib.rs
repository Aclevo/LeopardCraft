//! Offline CPU path tracer and image output.
//!
//! The renderer is separate from the viewport: it takes a deterministic
//! [`lc_scene::EvaluatedScene`], flattens geometry to world space, builds a
//! BVH, and progressively accumulates radiance with per-pixel deterministic
//! seeds. PNG and EXR export are self-contained.

pub mod camera;
pub mod exr;
pub mod output;
pub mod scene;
pub mod tracer;

pub use camera::RenderCamera;
pub use exr::{encode_exr, write_exr};
pub use output::{save_linear_exr, save_linear_png, save_rgba_png, save_tracer_png};
pub use scene::{Hit, RenderScene, SceneLight, SceneLightKind, Triangle};
pub use tracer::PathTracer;
