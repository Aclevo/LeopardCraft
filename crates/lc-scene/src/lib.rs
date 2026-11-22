//! Scene graph, tags, parametric geometry stacks, deterministic evaluation,
//! undo/redo and the native `.lcscene` document format.

pub mod camera;
pub mod demo;
pub mod document;
pub mod eval;
pub mod format;
pub mod geometry;
pub mod light;
pub mod scene;
pub mod undo;

pub use camera::{Camera, ProjectionMode};
pub use document::{
    Document, DocumentSettings, Environment, Layer, RenderSettings, FORMAT_NAME, FORMAT_VERSION,
};
pub use eval::{
    aim_at, EvaluatedCamera, EvaluatedGeometry, EvaluatedKind, EvaluatedLight, EvaluatedNode,
    EvaluatedScene,
};
pub use format::{
    from_json, load_native, recover_interrupted_save, recover_interrupted_saves, save_native,
    take_warnings, to_json,
};
pub use geometry::{CsgOperation, GeometrySource, Modifier, ModifierInput, Primitive};
pub use light::{Light, LightKind};
pub use scene::{Node, NodeId, NodeKind, NodeProps, Scene, Skin, Subtree, Tag};
pub use undo::{
    AddMaterialCmd, AddNodeCmd, Command, DeleteNodeCmd, History, RemoveMaterialCmd, RenameNodeCmd,
    ReparentCmd, ReplaceDocumentCmd, SetAnimationCmd, SetAnimationsCmd, SetMaterialCmd,
    SetNodePropsCmd, SetNodesPropsCmd, SetSettingsCmd, SetShapeCmd, SetShapesCmd,
    SetTagAnimationCmd, SetTransformCmd, SetTransformsCmd, Shape,
};
