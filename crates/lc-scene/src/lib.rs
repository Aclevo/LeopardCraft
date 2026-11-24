//! Scene graph, tags, parametric geometry stacks, deterministic evaluation,
//! undo/redo and the native `.lcscene` document format.

pub mod camera;
pub mod demo;
pub mod document;
pub mod eval;
pub mod format;
pub mod geometry;
pub mod ik;
pub mod light;
pub mod scene;
pub mod undo;

pub use camera::{Camera, ProjectionMode};
pub use document::{
    Document, DocumentSettings, Environment, Layer, RenderSettings, Take, TakeClips, TakeState,
    TakeTagClip, FORMAT_NAME, FORMAT_VERSION,
};
pub use eval::{
    aim_at, EvaluatedCamera, EvaluatedGeometry, EvaluatedKind, EvaluatedLight, EvaluatedNode,
    EvaluatedScene,
};
pub use format::{
    from_json, load_native, recover_interrupted_save, recover_interrupted_saves, save_native,
    take_warnings, to_json,
};
pub use geometry::{
    CsgOperation, DeformCage, DeformLimit, GeometrySource, Modifier, ModifierInput, Primitive,
    SubdivisionScheme,
};
pub use ik::{
    bake_ik_edits, bake_ik_range_edits, ik_chain_joints, solve_chain, IkChain, IkSolution,
    JointLimits,
};
pub use light::{Light, LightKind};
pub use scene::{
    MorphSet, MorphTarget, Node, NodeId, NodeKind, NodeProps, ProximityWeightOptions, Scene, Skin,
    Subtree, Tag,
};
pub use undo::{
    AddMaterialCmd, AddNodeCmd, AddNodesCmd, Command, DeleteNodeCmd, History, RemoveMaterialCmd,
    RenameNodeCmd, ReparentCmd, ReplaceDocumentCmd, SetAnimationCmd, SetAnimationsCmd,
    SetMaterialCmd, SetMorphCmd, SetNodePropsCmd, SetNodesPropsCmd, SetSettingsCmd, SetShapeCmd,
    SetShapesCmd, SetTagAnimationCmd, SetTakesCmd, SetTransformCmd, SetTransformsCmd, Shape,
};
