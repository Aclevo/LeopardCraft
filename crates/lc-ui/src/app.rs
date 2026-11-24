//! The LeopardCraft editor: Cheetah-style fixed workspace, viewport,
//! inspectors, material graph, UV view, timeline and background renderer.

use self::lifecycle::{
    is_native_scene, AutosaveOutcome, DocumentLifecycle, GuardChoice, GuardResult, PendingAction,
    SaveKind, StartupOutcome,
};
use self::recovery::RecoveryStore;
use self::selection::Selection;
use crate::icons::{self, Icon};
use crate::theme;
use crate::viewport::{FrameData, Vertex, ViewportRenderer};
use eframe::egui;
use egui::{Color32, Pos2, Rect, Sense, Vec2};
use glam::{Mat4, Quat, Vec2 as V2, Vec3};
use lc_anim::{
    AnimChannel, AnimationClip, ChannelAddress, ChannelAppliesTo, ChannelKind, ChannelTarget,
    Interpolation, Playback, TangentType,
};
use lc_core::{look_at_quat, ray_triangle, Pcg32, Transform};
use lc_geom::Mesh;
use lc_render::{PathTracer, RenderScene};
use lc_scene::{
    bake_ik_range_edits, ik_chain_joints, AddNodeCmd, Camera, Command, Document, Environment,
    EvaluatedScene, GeometrySource, History, Light, LightKind, Modifier, MorphSet, MorphTarget,
    Node, NodeId, NodeKind, NodeProps, Primitive, ProjectionMode, ProximityWeightOptions,
    RenameNodeCmd, ReparentCmd, ReplaceDocumentCmd, SetAnimationCmd, SetAnimationsCmd,
    SetMaterialCmd, SetMorphCmd, SetNodePropsCmd, SetNodesPropsCmd, SetShapeCmd, SetShapesCmd,
    SetTagAnimationCmd, SetTakesCmd, SetTransformsCmd, Shape, Skin, Tag,
};
use lc_shading::{Material, MaterialId, ShaderNodeKind, ShaderValue, TextureCache};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[path = "animation.rs"]
mod animation;
#[path = "deform.rs"]
mod deform;
#[path = "dialogs.rs"]
mod dialogs;
#[path = "documents.rs"]
mod documents;
#[path = "lifecycle.rs"]
mod lifecycle;
#[path = "lifecycle_harness.rs"]
mod lifecycle_harness;
#[path = "mesh_edit.rs"]
mod mesh_edit;
#[path = "panels.rs"]
mod panels;
#[path = "panels_more.rs"]
mod panels_more;
#[path = "recovery.rs"]
mod recovery;
#[path = "render_jobs.rs"]
mod render_jobs;
#[path = "scene_edit.rs"]
mod scene_edit;
#[path = "selection.rs"]
mod selection;
#[cfg(test)]
#[path = "selection_tests.rs"]
mod selection_tests;
#[path = "shell.rs"]
mod shell;
#[path = "viewport_ui.rs"]
mod viewport_ui;
#[cfg(test)]
#[path = "warning_surface_tests.rs"]
mod warning_surface_tests;

#[cfg(test)]
mod ui_test_support {
    use super::*;
    use std::cell::RefCell;

    #[derive(Clone, Debug)]
    pub(super) struct MenuHit {
        pub key: String,
        pub rect: Rect,
        pub enabled: bool,
    }

    thread_local! {
        static MENU_HITS: RefCell<Vec<MenuHit>> = const { RefCell::new(Vec::new()) };
        static VIEWPORT_HITS: RefCell<(Vec<NodeId>, Vec<NodeId>)> =
            const { RefCell::new((Vec::new(), Vec::new())) };
    }

    pub(super) fn clear_menu_hits() {
        MENU_HITS.with(|hits| hits.borrow_mut().clear());
    }

    pub(super) fn record_menu_hit(key: &str, response: &egui::Response, enabled: bool) {
        MENU_HITS.with(|hits| {
            hits.borrow_mut().push(MenuHit {
                key: key.to_owned(),
                rect: response.rect,
                enabled,
            });
        });
    }

    pub(super) fn menu_hit(key: &str) -> Option<(Rect, bool)> {
        MENU_HITS.with(|hits| {
            hits.borrow()
                .iter()
                .find(|hit| hit.key == key)
                .map(|hit| (hit.rect, hit.enabled))
        })
    }

    pub(super) fn record_viewport(visible: Vec<NodeId>, selected: Vec<NodeId>) {
        VIEWPORT_HITS.with(|hits| *hits.borrow_mut() = (visible, selected));
    }

    pub(super) fn viewport_hits() -> (Vec<NodeId>, Vec<NodeId>) {
        VIEWPORT_HITS.with(|hits| hits.borrow().clone())
    }
}

#[cfg(test)]
fn record_menu_item(key: &str, response: &egui::Response) {
    ui_test_support::record_menu_hit(key, response, response.enabled());
}

#[cfg(not(test))]
fn record_menu_item(_key: &str, _response: &egui::Response) {}

#[cfg(test)]
fn record_menu_item_state(key: &str, response: &egui::Response, enabled: bool) {
    ui_test_support::record_menu_hit(key, response, enabled);
}

#[cfg(not(test))]
fn record_menu_item_state(_key: &str, _response: &egui::Response, _enabled: bool) {}

fn recorded_button(ui: &mut egui::Ui, key: &str, text: impl Into<String>) -> egui::Response {
    let text = text.into();
    let response = ui.button(text.as_str());
    record_menu_item(key, &response);
    response
}

fn recorded_button_enabled(
    ui: &mut egui::Ui,
    key: &str,
    enabled: bool,
    text: impl Into<String>,
) -> egui::Response {
    let text = text.into();
    let response = ui.add_enabled(enabled, egui::Button::new(text.as_str()));
    record_menu_item(key, &response);
    response
}

fn recorded_selectable(
    ui: &mut egui::Ui,
    key: &str,
    selected: bool,
    text: impl Into<String>,
) -> egui::Response {
    let text = text.into();
    let response = ui.selectable_label(selected, text.as_str());
    record_menu_item(key, &response);
    response
}

fn recorded_submenu<R>(
    ui: &mut egui::Ui,
    key: &str,
    text: impl Into<String>,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let text = text.into();
    let response = ui.menu_button(text.as_str(), add_contents);
    record_menu_item(key, &response.response);
    response
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorView {
    #[default]
    Scene3d,
    Uv,
    Nodes,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum InspectorTab {
    #[default]
    Object,
    Material,
    Render,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowserTab {
    #[default]
    Objects,
    Morphs,
    Takes,
    Layers,
    Console,
}

/// One row in the scene-wide Morphs browser tab. The target index points into
/// its owner's `MorphSet`, so the tab never copies morph state.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct MorphBrowserRow {
    pub(super) object: NodeId,
    pub(super) object_name: String,
    pub(super) target: usize,
    pub(super) target_name: String,
    pub(super) weight: f32,
    pub(super) active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewportNav {
    Orbit,
    Pan,
}

fn alt_navigation(modifiers: egui::Modifiers, button: egui::PointerButton) -> Option<ViewportNav> {
    if !modifiers.alt {
        return None;
    }
    match button {
        egui::PointerButton::Primary => Some(if modifiers.shift {
            ViewportNav::Pan
        } else {
            ViewportNav::Orbit
        }),
        egui::PointerButton::Middle | egui::PointerButton::Secondary => Some(ViewportNav::Pan),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum ShadingMode {
    #[default]
    Shaded,
    Wireframe,
    ShadedWireframe,
}

impl ShadingMode {
    fn label(self) -> &'static str {
        match self {
            Self::Shaded => "Shaded",
            Self::Wireframe => "Wireframe",
            Self::ShadedWireframe => "Shaded + Wireframe",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum GizmoMode {
    Off,
    #[default]
    Translate,
    Rotate,
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NodeFlag {
    Visible,
    Renderable,
    Locked,
}

impl NodeFlag {
    fn get(self, props: &NodeProps) -> bool {
        match self {
            Self::Visible => props.visible,
            Self::Renderable => props.renderable,
            Self::Locked => props.locked,
        }
    }

    fn set(self, props: &mut NodeProps, value: bool) {
        match self {
            Self::Visible => props.visible = value,
            Self::Renderable => props.renderable = value,
            Self::Locked => props.locked = value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MeshSelectMode {
    Points,
    Edges,
    Polygons,
}

impl MeshSelectMode {
    fn label(self) -> &'static str {
        match self {
            Self::Points => "Points",
            Self::Edges => "Edges",
            Self::Polygons => "Polygons",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MeshTransformMode {
    Move,
    Rotate,
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MeshTransformSpace {
    World,
    Local,
    Normal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MeshAction {
    Enter,
    Exit,
    SelectAll,
    SelectNone,
    Transform,
    Extrude,
    Inset,
    Bevel,
    RingCut,
    Weld,
    Collapse,
    Optimize,
    PointSlide,
    Bridge,
    ToggleCrease,
    SetCreaseWeight,
    Subdivide,
    Clone,
    ToggleSeam,
    TogglePinnedUv,
    FillHole,
    Split,
    Shell,
    Triangulate,
    Scalpel,
    FlipNormal,
    Disconnect,
    Cover,
    CreatePolygon,
    AssignMaterial,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum MorphAction {
    RecordBase,
    Capture,
    UpdateBase,
    Restore,
    Remove(usize),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct OrbitCamera {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
    fov: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::new(0.0, 1.1, 0.0),
            yaw: 0.65,
            pitch: 0.42,
            distance: 11.0,
            fov: 45.0,
        }
    }
}

const WORKSPACE_STORAGE_KEY: &str = "leopardcraft.workspace.v1";
/// Recovery snapshots are written at most this often while the scene is dirty.
const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(15);
/// Upper bound for the playback range, matching the timeline's frame fields.
const MAX_PLAYBACK_FRAME: f32 = 100_000.0;

/// Layout and view choices that should return when the editor is reopened.
/// Scene contents themselves remain in the user's `.lcscene` file.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct WorkspaceState {
    inspector_tab: InspectorTab,
    browser_tab: BrowserTab,
    editor_view: EditorView,
    shading: ShadingMode,
    gizmo: GizmoMode,
    gizmo_world: bool,
    show_grid: bool,
    show_helpers: bool,
    show_curves: bool,
    hierarchy_filter: String,
    view_label: String,
    orbit: OrbitCamera,
    /// Kept so workspaces written by older builds still restore their object.
    #[serde(default)]
    selected_name: Option<String>,
    #[serde(default)]
    selected_names: Vec<String>,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            inspector_tab: InspectorTab::default(),
            browser_tab: BrowserTab::default(),
            editor_view: EditorView::default(),
            shading: ShadingMode::default(),
            gizmo: GizmoMode::default(),
            gizmo_world: true,
            show_grid: true,
            show_helpers: true,
            show_curves: false,
            hierarchy_filter: String::new(),
            view_label: "Viewport".into(),
            orbit: OrbitCamera::default(),
            selected_name: None,
            selected_names: Vec::new(),
        }
    }
}

impl OrbitCamera {
    fn eye(&self) -> Vec3 {
        self.target
            + Vec3::new(
                self.distance * self.pitch.cos() * self.yaw.sin(),
                self.distance * self.pitch.sin(),
                self.distance * self.pitch.cos() * self.yaw.cos(),
            )
    }

    fn world(&self) -> Mat4 {
        let eye = self.eye();
        Transform::new(eye, look_at_quat(eye, self.target, Vec3::Y), Vec3::ONE).matrix()
    }

    fn camera(&self) -> Camera {
        Camera {
            fov_y_deg: self.fov,
            ..Default::default()
        }
    }

    fn orbit(&mut self, delta: Vec2) {
        self.yaw -= delta.x * 0.008;
        self.pitch = (self.pitch + delta.y * 0.008).clamp(-1.48, 1.48);
    }

    fn pan(&mut self, delta: Vec2) {
        let forward = (self.target - self.eye()).normalize_or(Vec3::NEG_Z);
        let right = forward.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = right.cross(forward).normalize_or(Vec3::Y);
        let scale = self.distance * 0.0018;
        self.target += (-right * delta.x + up * delta.y) * scale;
    }

    fn zoom(&mut self, scroll: f32) {
        self.distance = (self.distance * (-scroll * 0.0018).exp()).clamp(0.2, 500.0);
    }

    fn frame(&mut self, bounds: lc_core::Aabb) {
        if bounds.is_valid() {
            let center = bounds.center();
            let radius = (bounds.max - bounds.min).length() * 0.5;
            self.target = center;
            self.distance = (radius * 2.9).clamp(0.5, 500.0);
        }
    }
}

#[derive(Clone, Debug)]
struct GizmoDrag {
    /// Top-level selected nodes and their transforms when the drag began.
    nodes: Vec<(NodeId, Transform)>,
    /// Shared world-space pivot used by translate, rotate and scale.
    pivot: Vec3,
    mode: GizmoMode,
    axis: usize,
    start_pointer: Pos2,
    screen_dir: Vec2,
    screen_len: f32,
    world_dir: Vec3,
}

/// An in-progress viewport marquee. `base` is restored when the drag is
/// additive (Shift or Cmd/Ctrl) and the marquee misses every object.
struct BoxSelect {
    start: Pos2,
    current: Pos2,
    base: Vec<NodeId>,
    additive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileMode {
    Open,
    SaveAs,
    Import,
    Export,
}

struct FileDialog {
    mode: FileMode,
    path: String,
}

/// A glTF import that produced a skipped-data report. The parsed document is
/// held until the user confirms, so the live scene is untouched while the
/// report is on screen.
pub(super) struct PendingImport {
    path: PathBuf,
    doc: Document,
    summary: String,
    replace_scene: bool,
}

struct RenderResult {
    width: u32,
    height: u32,
    pixels: Vec<Vec3>,
    rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderJobStatus {
    Finished,
    Cancelled,
}

struct RenderJob {
    id: u32,
    name: String,
    width: u32,
    height: u32,
    samples: u32,
    samples_done: u32,
    seconds: f32,
    exposure: f32,
    date: String,
    file: String,
    status: RenderJobStatus,
    texture: Option<egui::TextureHandle>,
    pixels: Option<Vec<Vec3>>,
}

impl RenderJob {
    fn is_finished(&self) -> bool {
        self.status == RenderJobStatus::Finished
    }
}

fn cancelled_render_job(active: &ActiveRender) -> RenderJob {
    RenderJob {
        id: active.id,
        name: active.name.clone(),
        width: active.width,
        height: active.height,
        samples: active.total,
        samples_done: active.done,
        seconds: active.started.elapsed().as_secs_f32(),
        exposure: active.exposure,
        date: utc_now_string(),
        file: active.file.clone(),
        status: RenderJobStatus::Cancelled,
        texture: None,
        pixels: None,
    }
}

struct ActiveRender {
    rx: Receiver<RenderMessage>,
    cancel: Arc<AtomicBool>,
    id: u32,
    name: String,
    file: String,
    width: u32,
    height: u32,
    total: u32,
    done: u32,
    exposure: f32,
    started: Instant,
}

enum RenderMessage {
    Progress { done: u32, total: u32 },
    Done(Box<RenderResult>),
    Failed(String),
    Cancelled,
}

/// Run the non-interactive document-lifecycle harness. This is only invoked
/// explicitly by `--headless lifecycle` and the integration test; normal
/// editor startup never enters it.
pub fn run_lifecycle_selftest() -> Result<(), String> {
    lifecycle_harness::run()
}

fn reconcile_native_scene(path: &Path) {
    if is_native_scene(path) {
        let _ = lc_scene::recover_interrupted_save(path);
    }
}

/// Tolerance used to identify one F-curve key by its track and time.
const KEY_TIME_EPSILON: f32 = 1e-3;

/// Which kind of channel track a curve key belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CurveTrackId {
    Scalar(ChannelAddress),
    Rotation,
    MorphWeight(u32),
}

/// The object, tag or material whose animation the F-curve editor is editing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AnimTarget {
    Object(NodeId),
    Tag(NodeId, usize),
    Material(MaterialId),
}

/// One row in the registry-driven F-curve editor. Quaternion channels use
/// `property: None`; every other row addresses one scalar component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CurveRow {
    pub(super) channel: AnimChannel,
    pub(super) address: Option<ChannelAddress>,
    pub(super) morph_target: Option<u32>,
}

/// One target's channels in the grouped F-curve row list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CurveRowGroup {
    pub(super) target: AnimTarget,
    pub(super) label: String,
    pub(super) rows: Vec<CurveRow>,
}

/// Which registry rows and curves the F-curve list shows. `Selected` keeps
/// only channels that own a selected key; `Transform` keeps Location, Rotation
/// and Scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CurveFilter {
    All,
    Selected,
    Transform,
}

impl CurveFilter {
    pub(super) const ALL: [Self; 3] = [Self::All, Self::Selected, Self::Transform];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::All => "All channels",
            Self::Selected => "Selected keys",
            Self::Transform => "Transforms only",
        }
    }
}

/// A single F-curve key identified by its target, track and time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SelectedKey {
    target: AnimTarget,
    track: CurveTrackId,
    time: f32,
}

/// In-flight horizontal key drag in the F-curve editor. `originals` is the
/// whole selection at drag start so multi-key drags preserve spacing.
#[derive(Clone, Debug)]
pub(super) struct CurveKeyDrag {
    key: SelectedKey,
    original_time: f32,
    current_time: f32,
    original_selection: Vec<SelectedKey>,
}

/// In-flight drag of one Bézier tangent handle. The key's screen-space
/// time/value anchor and the signed time offset to the handle are kept so the
/// pointer's vertical position can be converted back into a slope.
#[derive(Clone, Copy, Debug)]
pub(super) struct CurveTangentDrag {
    pub(super) target: AnimTarget,
    pub(super) track: CurveTrackId,
    pub(super) time: f32,
    pub(super) incoming: bool,
    pub(super) key_value: f32,
    pub(super) time_offset: f32,
    pub(super) original_slope: f32,
    pub(super) current_slope: f32,
}

/// In-flight marquee over the curve canvas.
#[derive(Clone, Copy, Debug)]
pub(super) struct CurveBoxSelect {
    pub(super) start: Pos2,
    pub(super) current: Pos2,
    pub(super) additive: bool,
}

#[derive(Clone, Copy, Debug)]
enum CurveKeyValue {
    Scalar(f32),
    Rotation(Quat),
}

#[derive(Clone, Copy, Debug)]
struct CurveClipboardKey {
    track: CurveTrackId,
    offset: f32,
    value: CurveKeyValue,
    interpolation: Interpolation,
}

/// A paste's document edit plus the key selections before and after it. This
/// rides on the same History entry as the clip edit, so undo and redo restore
/// both the document and the visible F-curve selection.
struct PasteCurveKeysCmd {
    inner: Box<dyn Command>,
    target: AnimTarget,
    before_selection: Vec<SelectedKey>,
    after_selection: Vec<SelectedKey>,
}

impl Command for PasteCurveKeysCmd {
    fn label(&self) -> String {
        self.inner.label()
    }

    fn apply(&mut self, doc: &mut Document) {
        self.inner.apply(doc);
    }

    fn undo(&mut self, doc: &mut Document) {
        self.inner.undo(doc);
    }

    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
}

/// Keys copied from the F-curve editor. Offsets are relative to the earliest
/// copied key, so paste starts at the playhead and preserves spacing.
#[derive(Clone, Debug, Default)]
pub(super) struct CurveClipboard {
    keys: Vec<CurveClipboardKey>,
}

/// UV view state: multi-point selection, the active UV set, chart tool values
/// and the checker/distortion overlays. None of this is document state or an
/// undo entry; only the UV edits themselves go through the history.
#[derive(Clone, Debug)]
pub(super) struct UvEditorState {
    pub selection: BTreeSet<u32>,
    pub set: usize,
    pub checker: bool,
    pub distortion: bool,
    pub pack_padding: f32,
    pub pack_rotate: bool,
    pub pack_align: lc_geom::UvPackAlign,
    pub chart_move: V2,
    pub chart_rotate: f32,
    pub chart_scale: f32,
    pub numeric: V2,
}

impl Default for UvEditorState {
    fn default() -> Self {
        Self {
            selection: BTreeSet::new(),
            set: 0,
            checker: true,
            distortion: false,
            pack_padding: 0.01,
            pack_rotate: false,
            pack_align: lc_geom::UvPackAlign::BottomLeft,
            chart_move: V2::ZERO,
            chart_rotate: 0.0,
            chart_scale: 1.0,
            numeric: V2::ZERO,
        }
    }
}

/// UV edits that work on the active set's slots. Weld and Split change the
/// shared slot topology, so they affect every set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UvSlotOp {
    Weld,
    Split,
    AlignU,
    AlignV,
    DistributeU,
    DistributeV,
    FlipU,
    FlipV,
    Rotate90,
    Numeric,
}

pub struct LeopardApp {
    doc: Document,
    history: History,
    editor_view: EditorView,
    inspector_tab: InspectorTab,
    browser_tab: BrowserTab,
    show_curves: bool,
    show_render_manager: bool,
    render_info_open: bool,
    view_label: String,
    last_title: String,
    selection: Selection,
    selected_material: Option<MaterialId>,
    selected_shader_node: Option<u32>,
    selected_uv: Option<usize>,
    uv_editor: UvEditorState,
    /// Explicit F-curve target. `None` follows the primary object.
    anim_target: Option<AnimTarget>,
    /// Selected F-curve keys, on whichever target owns them.
    selected_keys: Vec<SelectedKey>,
    /// Shared numeric factor used by selected-key and whole-take retiming.
    retime_factor: f32,
    /// Text buffers for the document Takes tab. They are committed by an
    /// explicit button so each rename/fps edit is exactly one undo step.
    take_name_buffer: String,
    take_fps_buffer: f32,
    take_editor_index: Option<usize>,
    curve_key_drag: Option<CurveKeyDrag>,
    curve_tangent_drag: Option<CurveTangentDrag>,
    curve_box_select: Option<CurveBoxSelect>,
    curve_clipboard: CurveClipboard,
    /// Registry row selected in the F-curve list, for keying.
    selected_curve_channel: Option<(AnimTarget, CurveRow)>,
    /// View filter for the F-curve row list, drawn curves and key picking.
    curve_filter: CurveFilter,
    /// A native clipboard paste was handled and its V-key release still needs
    /// to be swallowed so the same chord is not pasted twice.
    native_paste_pending: bool,
    playback: Playback,
    orbit: OrbitCamera,
    shading: ShadingMode,
    gizmo: GizmoMode,
    gizmo_world: bool,
    show_grid: bool,
    show_helpers: bool,
    viewport: ViewportRenderer,
    texture_cache: TextureCache,
    dialog: Option<FileDialog>,
    pending_import: Option<PendingImport>,
    lifecycle: DocumentLifecycle,
    allow_close: bool,
    status: String,
    show_about: bool,
    hierarchy_filter: String,
    focus_search: bool,
    reveal_selected_in_browser: bool,
    render_path: String,
    render_jobs: Vec<RenderJob>,
    active_render: Option<ActiveRender>,
    selected_render_job: Option<u32>,
    next_job_id: u32,
    console: VecDeque<String>,
    last_status: String,
    collapsed: HashSet<NodeId>,
    started_at: Instant,
    pending_transform: Option<Vec<(NodeId, Transform)>>,
    pending_shape: Option<(NodeId, Shape)>,
    pending_material: Option<(MaterialId, Material)>,
    gizmo_drag: Option<GizmoDrag>,
    box_select: Option<BoxSelect>,
    mesh_edit: bool,
    mesh_select_mode: MeshSelectMode,
    triangulate_mode: lc_geom::TriangulateMode,
    mesh_crease_weight: f32,
    mesh_clone: lc_geom::CloneOptions,
    /// Click a point on an open border to fill that hole, like the manual's
    /// Fill Hole tool. The selection-driven command remains available too.
    fill_hole_tool: bool,
    /// Click a point or edge to cut, like the manual's Scalpel tool: the second
    /// click splits the polygon holding both picks, and that cut's end point
    /// becomes the next start until ESC resets it.
    scalpel_tool: bool,
    scalpel_start: Option<lc_geom::ScalpelPick>,
    mesh_subdivide_levels: u32,
    mesh_subdivide_scheme: lc_geom::SubdivisionKind,
    mesh_selected: Vec<u32>,
    /// Component selection for every mesh taking part in a multi-mesh edit.
    /// `mesh_selected` remains the primary mesh's selection for the viewport
    /// overlay and the component transform controls.
    mesh_components: BTreeMap<NodeId, Vec<u32>>,
    mesh_transform: MeshTransformMode,
    mesh_space: MeshTransformSpace,
    mesh_axis: usize,
    mesh_value: f32,
    mesh_depth: f32,
    mesh_offset: f32,
    mesh_extrude_individual: bool,
    mesh_shell_thickness: f32,
    mesh_shell_inside: bool,
    mesh_shell_angle: f32,
    mesh_shell_quantize: f32,
    mesh_optimize_tolerance: f32,
    mesh_slide_fraction: f32,
    weight_paint: bool,
    weight_joint: Option<usize>,
    weight_brush: WeightBrush,
    bind_joints: Option<BTreeSet<NodeId>>,
    bind_auto_weights: bool,
    bind_dropoff: f32,
    bind_strength: f32,
    bind_max_influences: usize,
    last_viewport_rect: Rect,
    last_viewport_size: Vec2,
    /// Actual row rectangles and highlight decisions from the last Object
    /// Browser frame, so input tests can click the same widgets a user sees.
    #[cfg(test)]
    object_row_rects: Vec<(NodeId, Rect)>,
    #[cfg(test)]
    object_row_highlights: Vec<(NodeId, bool)>,
    /// Eye, render and lock hit rectangles for each object-browser row.
    #[cfg(test)]
    object_row_icons: Vec<(NodeId, Rect, Rect, Rect)>,
    /// Visible, renderable and locked flags reflected by each browser row.
    #[cfg(test)]
    object_row_flags: Vec<(NodeId, bool, bool, bool)>,
    /// Target-row rectangles from the last Morphs browser frame.
    #[cfg(test)]
    morph_row_rects: Vec<(NodeId, usize, Rect)>,
    /// Take-row rectangles from the last Takes browser frame.
    #[cfg(test)]
    take_row_rects: Vec<(usize, Rect)>,
}

impl LeopardApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::theme::install(&cc.egui_ctx);
        let doc = Document::demo();
        let recovery = RecoveryStore::system();
        let mut lifecycle = DocumentLifecycle::new(recovery, AUTOSAVE_INTERVAL);
        let startup = lifecycle.startup(&doc);
        let fps = doc.settings.fps.max(1.0);
        let mut selection = Selection::default();
        if let Some(id) = doc.scene.find_by_name("Cube") {
            selection.select_one(id);
        }
        let mut app = Self {
            playback: Playback {
                start: doc.settings.frame_start / fps,
                end: doc.settings.frame_end / fps,
                fps,
                looping: true,
                ..Default::default()
            },
            selection,
            selected_material: Some(0),
            selected_shader_node: None,
            selected_uv: None,
            uv_editor: UvEditorState::default(),
            anim_target: None,
            selected_keys: Vec::new(),
            retime_factor: 1.0,
            take_name_buffer: doc
                .active_take()
                .map(|t| t.name.clone())
                .unwrap_or_default(),
            take_fps_buffer: doc.active_take_fps(),
            take_editor_index: Some(doc.active_take),
            curve_key_drag: None,
            curve_tangent_drag: None,
            curve_box_select: None,
            curve_clipboard: CurveClipboard::default(),
            selected_curve_channel: None,
            curve_filter: CurveFilter::All,
            native_paste_pending: false,
            doc,
            history: History::new(200),
            editor_view: EditorView::Scene3d,
            inspector_tab: InspectorTab::Object,
            browser_tab: BrowserTab::Objects,
            show_curves: false,
            show_render_manager: false,
            render_info_open: true,
            view_label: "Viewport".into(),
            last_title: String::new(),
            orbit: OrbitCamera::default(),
            shading: ShadingMode::Shaded,
            gizmo: GizmoMode::Translate,
            gizmo_world: true,
            show_grid: true,
            show_helpers: true,
            viewport: ViewportRenderer::new(cc.wgpu_render_state.clone()),
            texture_cache: TextureCache::default(),
            dialog: None,
            pending_import: None,
            lifecycle,
            allow_close: false,
            status: "Ready — demo studio loaded".into(),
            show_about: false,
            hierarchy_filter: String::new(),
            focus_search: false,
            reveal_selected_in_browser: false,
            render_path: "render.png".into(),
            render_jobs: Vec::new(),
            active_render: None,
            selected_render_job: None,
            next_job_id: 1,
            console: VecDeque::new(),
            last_status: String::new(),
            collapsed: HashSet::new(),
            started_at: Instant::now(),
            pending_transform: None,
            pending_shape: None,
            pending_material: None,
            gizmo_drag: None,
            box_select: None,
            mesh_edit: false,
            mesh_select_mode: MeshSelectMode::Polygons,
            triangulate_mode: lc_geom::TriangulateMode::default(),
            mesh_crease_weight: 1.0,
            mesh_clone: lc_geom::CloneOptions::default(),
            fill_hole_tool: false,
            scalpel_tool: false,
            scalpel_start: None,
            mesh_subdivide_levels: 1,
            mesh_subdivide_scheme: lc_geom::SubdivisionKind::default(),
            mesh_selected: Vec::new(),
            mesh_components: BTreeMap::new(),
            mesh_transform: MeshTransformMode::Move,
            mesh_space: MeshTransformSpace::World,
            mesh_axis: 1,
            mesh_value: 0.25,
            mesh_depth: 0.05,
            mesh_offset: 0.0,
            mesh_extrude_individual: false,
            mesh_shell_thickness: 0.1,
            mesh_shell_inside: true,
            mesh_shell_angle: 30.0,
            mesh_shell_quantize: 0.0,
            mesh_optimize_tolerance: 0.001,
            mesh_slide_fraction: 0.5,
            weight_paint: false,
            weight_joint: None,
            weight_brush: WeightBrush::default(),
            bind_joints: None,
            bind_auto_weights: true,
            bind_dropoff: 2.0,
            bind_strength: 0.6,
            bind_max_influences: 4,
            last_viewport_rect: Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)),
            last_viewport_size: Vec2::splat(100.0),
            #[cfg(test)]
            object_row_rects: Vec::new(),
            #[cfg(test)]
            object_row_highlights: Vec::new(),
            #[cfg(test)]
            object_row_icons: Vec::new(),
            #[cfg(test)]
            object_row_flags: Vec::new(),
            #[cfg(test)]
            morph_row_rects: Vec::new(),
            #[cfg(test)]
            take_row_rects: Vec::new(),
        };
        if let Some(storage) = cc.storage {
            if let Some(state) = eframe::get_value::<WorkspaceState>(storage, WORKSPACE_STORAGE_KEY)
            {
                app.apply_workspace_state(state);
            }
        }
        match startup {
            StartupOutcome::Recoverable => {
                let origin = app
                    .lifecycle
                    .recovery_prompt()
                    .and_then(|recovered| recovered.origin.as_ref())
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "an untitled scene".into());
                app.status = format!("Found unsaved work for {origin} from a previous session");
            }
            StartupOutcome::Unreadable(error) => {
                app.status = format!("Discarded unreadable recovery snapshot: {error}");
            }
            StartupOutcome::Clean => {}
        }
        app
    }

    fn workspace_state(&self) -> WorkspaceState {
        WorkspaceState {
            inspector_tab: self.inspector_tab,
            browser_tab: self.browser_tab,
            editor_view: self.editor_view,
            shading: self.shading,
            gizmo: self.gizmo,
            gizmo_world: self.gizmo_world,
            show_grid: self.show_grid,
            show_helpers: self.show_helpers,
            show_curves: self.show_curves,
            hierarchy_filter: self.hierarchy_filter.clone(),
            view_label: self.view_label.clone(),
            orbit: self.orbit,
            selected_name: self
                .selection
                .primary()
                .and_then(|id| self.doc.scene.get(id))
                .map(|node| node.name.clone()),
            selected_names: self
                .selection
                .ids()
                .iter()
                .filter_map(|id| self.doc.scene.get(*id))
                .map(|node| node.name.clone())
                .collect(),
        }
    }

    fn apply_workspace_state(&mut self, state: WorkspaceState) {
        self.inspector_tab = state.inspector_tab;
        self.browser_tab = state.browser_tab;
        self.editor_view = state.editor_view;
        self.shading = state.shading;
        self.gizmo = state.gizmo;
        self.gizmo_world = state.gizmo_world;
        self.show_grid = state.show_grid;
        self.show_helpers = state.show_helpers;
        self.show_curves = state.show_curves;
        self.hierarchy_filter = state.hierarchy_filter.chars().take(120).collect();
        self.view_label = if matches!(
            state.view_label.as_str(),
            "Viewport" | "Front" | "Top" | "Right" | "Left"
        ) {
            state.view_label
        } else {
            "Viewport".into()
        };
        let orbit = state.orbit;
        if orbit.target.is_finite()
            && orbit.yaw.is_finite()
            && orbit.pitch.is_finite()
            && orbit.distance.is_finite()
            && orbit.fov.is_finite()
        {
            self.orbit = OrbitCamera {
                target: orbit.target,
                yaw: orbit.yaw,
                pitch: orbit.pitch.clamp(-1.5, 1.5),
                distance: orbit.distance.clamp(0.2, 500.0),
                fov: orbit.fov.clamp(10.0, 120.0),
            };
        }
        let names: Vec<String> = if state.selected_names.is_empty() {
            state.selected_name.into_iter().collect()
        } else {
            state.selected_names
        };
        let ids: Vec<NodeId> = names
            .iter()
            .filter_map(|name| self.doc.scene.find_by_name(name))
            .collect();
        self.selection.set(ids);
        self.sync_selection_material();
    }

    /// Adopt the loaded document's frame range. Documents store frames and
    /// fps; the playback clock runs in seconds, so both bounds are divided.
    fn sync_playback_to_document(&mut self) {
        self.playback.fps = self.doc.active_take_fps();
        self.playback.start = self.doc.settings.frame_start / self.playback.fps;
        self.playback.end = self.doc.settings.frame_end / self.playback.fps;
        self.playback.time = self.playback.start;
        self.normalize_playback();
    }

    fn sync_take_editor(&mut self) {
        let index = self.doc.active_take;
        self.take_name_buffer = self
            .doc
            .active_take()
            .map(|take| take.name.clone())
            .unwrap_or_default();
        self.take_fps_buffer = self.doc.active_take_fps();
        self.take_editor_index = Some(index);
    }

    /// Drop key selections whose clips were replaced by a take operation.
    fn clear_take_dependent_edits(&mut self) {
        self.selected_keys.clear();
        self.curve_key_drag = None;
        self.curve_tangent_drag = None;
        self.curve_box_select = None;
    }

    fn run_take_edit(&mut self, label: &str, edit: impl FnOnce(&mut Document) -> bool) -> bool {
        let before = self.doc.take_state();
        if !edit(&mut self.doc) {
            self.sync_take_editor();
            return false;
        }
        let after = self.doc.take_state();
        self.history.execute(
            &mut self.doc,
            Box::new(SetTakesCmd {
                label: label.into(),
                before,
                after,
            }),
        );
        self.sync_playback_to_document();
        self.sync_take_editor();
        self.clear_take_dependent_edits();
        self.invalidate_pending();
        true
    }

    pub(super) fn create_take(&mut self) -> bool {
        if !self.run_take_edit("New Take", |doc| {
            doc.create_take("Take");
            true
        }) {
            return false;
        }
        self.status = format!("Created {}", self.take_name_buffer);
        true
    }

    pub(super) fn duplicate_take(&mut self) -> bool {
        if !self.run_take_edit("Duplicate Take", |doc| {
            doc.duplicate_active_take();
            true
        }) {
            return false;
        }
        self.status = format!("Duplicated take as {}", self.take_name_buffer);
        true
    }

    pub(super) fn delete_active_take(&mut self) -> bool {
        let index = self.doc.active_take;
        if !self.run_take_edit("Delete Take", |doc| doc.delete_take(index)) {
            return false;
        }
        self.status = format!("Deleted take; active is {}", self.take_name_buffer);
        true
    }

    pub(super) fn activate_take(&mut self, index: usize) -> bool {
        if !self.run_take_edit("Activate Take", |doc| doc.activate_take(index)) {
            return false;
        }
        self.status = format!("Activated {}", self.take_name_buffer);
        true
    }

    pub(super) fn rename_active_take(&mut self, name: String) -> bool {
        let index = self.doc.active_take;
        if !self.run_take_edit("Rename Take", |doc| doc.rename_take(index, &name)) {
            self.status = "Take names must be unique and non-empty".into();
            return false;
        }
        self.status = format!("Renamed take to {}", self.take_name_buffer);
        true
    }

    pub(super) fn set_active_take_fps(&mut self, fps: f32) -> bool {
        let index = self.doc.active_take;
        if !self.run_take_edit("Take FPS", |doc| doc.set_take_fps(index, fps)) {
            self.status = if fps.is_finite() {
                "Take fps is unchanged".into()
            } else {
                "Take fps must be a finite number".into()
            };
            return false;
        }
        self.status = format!("Take fps set to {}", self.playback.fps);
        true
    }

    /// Keep playback bounds finite, ordered and inside the timeline's frame
    /// limit. Playback is view state, so this never touches the document or
    /// history.
    fn normalize_playback(&mut self) {
        let fps = if self.playback.fps.is_finite() {
            self.playback.fps.clamp(1.0, 240.0)
        } else {
            24.0
        };
        self.playback.fps = fps;
        let max = MAX_PLAYBACK_FRAME / fps;
        if !self.playback.start.is_finite() {
            self.playback.start = 0.0;
        }
        if !self.playback.end.is_finite() {
            self.playback.end = self.playback.start;
        }
        self.playback.start = self.playback.start.clamp(0.0, max);
        self.playback.end = self.playback.end.clamp(self.playback.start, max);
        if !self.playback.time.is_finite() {
            self.playback.time = self.playback.start;
        }
        self.playback.time = self
            .playback
            .time
            .clamp(self.playback.start, self.playback.end);
    }

    pub(super) fn set_playback_start(&mut self, start: f32) -> bool {
        if !start.is_finite() {
            return false;
        }
        self.normalize_playback();
        let start = start.clamp(0.0, self.playback.end);
        if (start - self.playback.start).abs() <= 1e-6 {
            return false;
        }
        self.playback.start = start;
        self.playback.time = self.playback.time.clamp(start, self.playback.end);
        true
    }

    pub(super) fn set_playback_end(&mut self, end: f32) -> bool {
        if !end.is_finite() {
            return false;
        }
        self.normalize_playback();
        let max = MAX_PLAYBACK_FRAME / self.playback.fps;
        let end = end.clamp(self.playback.start, max);
        if (end - self.playback.end).abs() <= 1e-6 {
            return false;
        }
        self.playback.end = end;
        self.playback.time = self.playback.time.clamp(self.playback.start, end);
        true
    }

    pub(super) fn set_playback_looping(&mut self, looping: bool) -> bool {
        if self.playback.looping == looping {
            return false;
        }
        self.playback.looping = looping;
        true
    }

    pub(super) fn set_playback_fps(&mut self, fps: f32) -> bool {
        if !fps.is_finite() {
            return false;
        }
        let fps = fps.clamp(1.0, 240.0);
        if (fps - self.playback.fps).abs() <= 1e-6 {
            return false;
        }
        self.playback.fps = fps;
        self.normalize_playback();
        true
    }

    pub fn load_scene(&mut self, doc: Document, path: Option<PathBuf>) {
        if let Some(active) = self.active_render.take() {
            active.cancel.store(true, Ordering::Relaxed);
        }
        self.doc = doc;
        self.sync_playback_to_document();
        self.sync_take_editor();
        let initial = self.doc.scene.find_by_name("Cube").or_else(|| {
            self.doc.scene.depth_first().into_iter().find(|id| {
                self.doc
                    .scene
                    .get(*id)
                    .map(|n| n.is_mesh())
                    .unwrap_or(false)
            })
        });
        self.selection.clear();
        if let Some(id) = initial {
            self.selection.select_one(id);
        }
        self.history.clear();
        self.lifecycle.loaded(&self.doc, path);
        self.pending_transform = None;
        self.pending_shape = None;
        self.pending_material = None;
        self.gizmo_drag = None;
        self.box_select = None;
        self.mesh_edit = false;
        self.clear_mesh_component_selection();
        self.collapsed.clear();
        self.hierarchy_filter.clear();
        self.browser_tab = BrowserTab::Objects;
        self.reveal_selected_in_browser = true;
        self.editor_view = EditorView::Scene3d;
        self.inspector_tab = InspectorTab::Object;
        self.render_jobs.clear();
        self.selected_render_job = None;
        self.show_render_manager = false;
    }

    fn sync_selection_material(&mut self) {
        if let Some(id) = self.selection.primary() {
            if let Some(mat) = self.doc.scene.get(id).and_then(|n| n.effective_material()) {
                self.selected_material = Some(mat);
            }
        }
        if let Some(mat) = self.selected_material {
            if self.doc.materials.get(mat).is_none() {
                self.selected_material = self.doc.materials.iter().next().map(|m| m.id);
            }
        }
    }

    fn evaluate(&self) -> EvaluatedScene {
        self.doc.evaluate(self.playback.time)
    }

    fn scene_bounds(&self) -> lc_core::Aabb {
        self.doc.evaluate(self.playback.time).bounds
    }

    fn frame_selected(&mut self) {
        let evaluated = self.evaluate();
        let mut bounds = lc_core::Aabb::EMPTY;
        let mut center = Vec3::ZERO;
        let mut count = 0usize;
        for id in self.selection.ids() {
            let Some(node) = evaluated.node(*id) else {
                continue;
            };
            if let Some(mesh) = node.mesh() {
                bounds = bounds.union(&mesh.bounds().transformed(node.world));
            } else {
                center += node.world.w_axis.truncate();
                count += 1;
            }
        }
        if bounds.is_valid() {
            self.orbit.frame(bounds);
        } else if count > 0 {
            self.orbit.target = center / count as f32;
        } else {
            self.orbit.frame(self.scene_bounds());
        }
    }

    fn undo(&mut self) {
        if let Some(label) = self.history.undo(&mut self.doc) {
            let take_changed = self
                .history
                .redo_top()
                .and_then(|command| command.as_any())
                .is_some_and(|state| state.downcast_ref::<SetTakesCmd>().is_some());
            let restored = self
                .history
                .redo_top()
                .and_then(|command| command.as_any())
                .and_then(|state| state.downcast_ref::<PasteCurveKeysCmd>())
                .map(|paste| (paste.target, paste.before_selection.clone()));
            if let Some((target, selection)) = restored {
                self.restore_paste_state(target, selection);
            }
            if take_changed {
                self.sync_playback_to_document();
                self.sync_take_editor();
                self.clear_take_dependent_edits();
            }
            self.status = format!("Undo: {label}");
            self.selection.retain_existing(&self.doc.scene);
            self.invalidate_pending();
        }
    }

    fn redo(&mut self) {
        if let Some(label) = self.history.redo(&mut self.doc) {
            let take_changed = self
                .history
                .undo_top()
                .and_then(|command| command.as_any())
                .is_some_and(|state| state.downcast_ref::<SetTakesCmd>().is_some());
            let restored = self
                .history
                .undo_top()
                .and_then(|command| command.as_any())
                .and_then(|state| state.downcast_ref::<PasteCurveKeysCmd>())
                .map(|paste| (paste.target, paste.after_selection.clone()));
            if let Some((target, selection)) = restored {
                self.restore_paste_state(target, selection);
            }
            if take_changed {
                self.sync_playback_to_document();
                self.sync_take_editor();
                self.clear_take_dependent_edits();
            }
            self.status = format!("Redo: {label}");
            self.selection.retain_existing(&self.doc.scene);
            self.invalidate_pending();
        }
    }

    /// Restore the F-curve target and its key selection for a paste command.
    /// A target that has disappeared since the paste is skipped; the document
    /// edit and any selection whose target still exists remain safe.
    fn restore_paste_state(&mut self, target: AnimTarget, selection: Vec<SelectedKey>) {
        if self.set_anim_target(target) {
            match target {
                AnimTarget::Object(id) => {
                    self.selection.select_one(id);
                    self.sync_selection_material();
                }
                AnimTarget::Tag(node, _) => {
                    self.selection.select_one(node);
                    self.sync_selection_material();
                }
                AnimTarget::Material(_) => {}
            }
            self.selected_keys = selection;
        } else {
            let valid: Vec<SelectedKey> = selection
                .into_iter()
                .filter(|key| self.target_is_valid(key.target))
                .collect();
            if !valid.is_empty() {
                self.selected_keys = valid;
            }
        }
    }

    fn invalidate_pending(&mut self) {
        self.pending_transform = None;
        self.pending_shape = None;
        self.pending_material = None;
        self.gizmo_drag = None;
        // A pending Scalpel cut names points of the current topology, so undo,
        // redo and a replaced document must drop it rather than leave a start
        // the mesh no longer has.
        self.scalpel_start = None;
    }

    fn commit_pending(&mut self) {
        if let Some(before) = self.pending_transform.take() {
            let edits: Vec<(NodeId, Transform, Transform)> = before
                .into_iter()
                .filter_map(|(id, before)| {
                    let after = self.doc.scene.get(id)?.transform;
                    (after != before).then_some((id, before, after))
                })
                .collect();
            if !edits.is_empty() {
                self.history
                    .execute(&mut self.doc, Box::new(SetTransformsCmd { edits }));
            }
        }
        if let Some((id, before)) = self.pending_shape.take() {
            if let Some(node) = self.doc.scene.get(id) {
                let after = Shape::of(node);
                let shape_changed = after.kind != before.kind
                    || after.modifiers != before.modifiers
                    || after.skin != before.skin;
                let morph_changed = after.morph != before.morph;
                if shape_changed {
                    self.history
                        .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
                } else if morph_changed {
                    self.history.execute(
                        &mut self.doc,
                        Box::new(SetMorphCmd {
                            id,
                            before: before.morph,
                            after: after.morph,
                        }),
                    );
                }
            }
        }
        if let Some((id, before)) = self.pending_material.take() {
            if let Some(after) = self.doc.materials.get(id).cloned() {
                if after != before {
                    self.history
                        .execute(&mut self.doc, Box::new(SetMaterialCmd { before, after }));
                }
            }
        }
    }
}

enum UvOp {
    Planar,
    Box,
    Cylindrical,
    Spherical,
    Unwrap,
    Relax,
    Normalize,
}

impl eframe::App for LeopardApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, WORKSPACE_STORAGE_KEY, &self.workspace_state());
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        self.lifecycle.refresh_dirty(&self.doc, false);
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_action(&ctx, PendingAction::Quit);
        }
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        if self.playback.playing {
            self.playback.advance(dt);
            ctx.request_repaint();
        }
        self.handle_shortcuts(&ctx);

        self.poll_render(&ctx);
        self.log_status();
        self.autosave_tick(&ctx);
        self.shell(root);

        self.show_file_dialog(&ctx);
        self.show_import_report_dialog(&ctx);
        self.show_recovery_dialog(&ctx);
        self.show_guard_dialog(&ctx);
        self.show_about(&ctx);
        self.update_window_title(&ctx);

        if ctx.input(|i| i.pointer.any_released()) || ctx.input(|i| i.key_pressed(egui::Key::Enter))
        {
            self.commit_pending();
        }
        if self.active_render.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

// ---------------------------------------------------------------------------
// Menu, dialogs and status bar
// ---------------------------------------------------------------------------

fn format_age(when: SystemTime) -> String {
    let seconds = SystemTime::now()
        .duration_since(when)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    if seconds < 60 {
        "less than a minute ago".into()
    } else if seconds < 3600 {
        let minutes = seconds / 60;
        if minutes == 1 {
            "1 minute ago".into()
        } else {
            format!("{minutes} minutes ago")
        }
    } else {
        let hours = seconds / 3600;
        if hours == 1 {
            "1 hour ago".into()
        } else {
            format!("{hours} hours ago")
        }
    }
}

// ---------------------------------------------------------------------------
// Viewport
// ---------------------------------------------------------------------------

fn utc_now_string() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rem / 3_600,
        (rem % 3_600) / 60
    )
}

fn axis_vector(axis: usize) -> Vec3 {
    match axis {
        0 => Vec3::X,
        1 => Vec3::Y,
        _ => Vec3::Z,
    }
}

fn mesh_affected_points(mesh: &Mesh, mode: MeshSelectMode, selected: &[u32]) -> Vec<u32> {
    let mut points = Vec::new();
    match mode {
        MeshSelectMode::Points => {
            for &p in selected {
                if (p as usize) < mesh.positions.len() {
                    points.push(p);
                }
            }
        }
        MeshSelectMode::Edges => {
            let edges = mesh.edges();
            for &index in selected {
                if let Some(edge) = edges.get(index as usize) {
                    points.push(edge.a);
                    points.push(edge.b);
                }
            }
        }
        MeshSelectMode::Polygons => {
            for &index in selected {
                if let Some(poly) = mesh.polygons.get(index as usize) {
                    points.extend(poly.corners.iter().map(|c| c.point));
                }
            }
        }
    }
    points.sort_unstable();
    points.dedup();
    points
}

fn point_segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq <= 1e-8 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

fn point_in_polygon(p: Pos2, polygon: &[Pos2]) -> bool {
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[j];
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

struct SkinTarget {
    vertex_count: usize,
    joints: Vec<NodeId>,
    inverse_bind: Vec<Mat4>,
    segments: Vec<(Vec3, Vec3)>,
    positions: Vec<Vec3>,
    existing: Option<Skin>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WeightBrushMode {
    Add,
    Subtract,
    Replace,
    Smooth,
}

impl WeightBrushMode {
    fn label(self) -> &'static str {
        match self {
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Replace => "Replace",
            Self::Smooth => "Smooth",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WeightFalloff {
    Constant,
    Linear,
    Smooth,
}

impl WeightFalloff {
    fn label(self) -> &'static str {
        match self {
            Self::Constant => "Constant",
            Self::Linear => "Linear",
            Self::Smooth => "Smooth",
        }
    }

    fn factor(self, distance: f32, radius: f32) -> f32 {
        if radius <= 0.0 {
            return 0.0;
        }
        let t = (distance / radius).clamp(0.0, 1.0);
        match self {
            Self::Constant => 1.0,
            Self::Linear => 1.0 - t,
            Self::Smooth => {
                let s = 1.0 - t;
                s * s * (3.0 - 2.0 * s)
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct WeightBrush {
    mode: WeightBrushMode,
    falloff: WeightFalloff,
    radius: f32,
    strength: f32,
}

impl Default for WeightBrush {
    fn default() -> Self {
        Self {
            mode: WeightBrushMode::Add,
            falloff: WeightFalloff::Smooth,
            radius: 44.0,
            strength: 0.3,
        }
    }
}

fn skin_binding_ready(_node: &Node, target: &SkinTarget) -> bool {
    target.vertex_count > 0 && !target.joints.is_empty()
}

fn skin_unbind_ready(node: &Node) -> bool {
    node.skin.is_some()
}

fn modifier_applies_to(node: &Node, modifier: &Modifier) -> bool {
    match modifier {
        Modifier::Sweep { .. } => matches!(node.kind, NodeKind::Spline { .. }),
        Modifier::Lathe { .. } => true,
        _ => node.is_mesh(),
    }
}

fn mesh_transform_axis(
    mesh: &Mesh,
    affected: &HashSet<u32>,
    space: MeshTransformSpace,
    axis: usize,
    rotation: Quat,
) -> Vec3 {
    match space {
        MeshTransformSpace::Local => axis_vector(axis),
        MeshTransformSpace::World => (rotation.inverse() * axis_vector(axis)).normalize_or(Vec3::Y),
        MeshTransformSpace::Normal => {
            let mut normal = Vec3::ZERO;
            for (fi, poly) in mesh.polygons.iter().enumerate() {
                if poly.corners.iter().any(|c| affected.contains(&c.point)) {
                    normal += mesh.face_normal(fi);
                }
            }
            normal.normalize_or(Vec3::Y)
        }
    }
}

fn mesh_transform_would_change(
    mesh: &Mesh,
    affected: &HashSet<u32>,
    mode: MeshTransformMode,
    local_axis: Vec3,
    value: f32,
) -> bool {
    let center = affected
        .iter()
        .filter_map(|p| mesh.positions.get(*p as usize).copied())
        .sum::<Vec3>()
        / affected.len() as f32;
    match mode {
        MeshTransformMode::Move => value.abs() > 1e-6,
        MeshTransformMode::Rotate => {
            value.rem_euclid(360.0).abs() > 1e-6
                && affected.iter().any(|p| {
                    mesh.positions
                        .get(*p as usize)
                        .map(|position| (*position - center).length_squared() > 1e-12)
                        .unwrap_or(false)
                })
        }
        MeshTransformMode::Scale => {
            value.abs() > 1e-6
                && affected.iter().any(|p| {
                    mesh.positions
                        .get(*p as usize)
                        .map(|position| (*position - center).dot(local_axis).abs() > 1e-6)
                        .unwrap_or(false)
                })
        }
    }
}

fn selected_valid_points(mesh: &Mesh, selected: &[u32]) -> Vec<u32> {
    let mut unique: Vec<u32> = selected
        .iter()
        .copied()
        .filter(|p| (*p as usize) < mesh.positions.len())
        .collect();
    unique.sort_unstable();
    unique.dedup();
    unique
}

/// Resolve the component selection into one Scalpel cut: two edges or two
/// points that share a polygon, with edge picks cutting at their midpoints.
/// Mesh Editing's predicate and handler both use this so enablement and the
/// action cannot disagree.
fn scalpel_cut(
    mesh: &Mesh,
    mode: MeshSelectMode,
    selected: &[u32],
) -> Result<(u32, lc_geom::ScalpelPick, lc_geom::ScalpelPick), String> {
    let on_face = |face: &lc_geom::Polygon, a: u32, b: u32| {
        let count = face.corners.len();
        (0..count).any(|index| {
            let start = face.corners[index].point;
            let end = face.corners[(index + 1) % count].point;
            (start == a && end == b) || (start == b && end == a)
        })
    };
    match mode {
        MeshSelectMode::Edges => {
            let edges = mesh.edges();
            let mut picks: Vec<(u32, u32)> = Vec::new();
            for index in selected {
                let Some(edge) = edges.get(*index as usize) else {
                    return Err("select two edges of one polygon".into());
                };
                if edge.a == edge.b {
                    return Err("an edge needs two different points".into());
                }
                picks.push((edge.a.min(edge.b), edge.a.max(edge.b)));
            }
            picks.sort_unstable();
            picks.dedup();
            if picks.len() != 2 {
                return Err("select exactly two edges of the same polygon".into());
            }
            let face = (0..mesh.polygons.len())
                .find(|face| {
                    on_face(&mesh.polygons[*face], picks[0].0, picks[0].1)
                        && on_face(&mesh.polygons[*face], picks[1].0, picks[1].1)
                })
                .ok_or_else(|| "the two edges must belong to the same polygon".to_string())?;
            Ok((
                face as u32,
                lc_geom::ScalpelPick::Edge {
                    a: picks[0].0,
                    b: picks[0].1,
                    t: 0.5,
                },
                lc_geom::ScalpelPick::Edge {
                    a: picks[1].0,
                    b: picks[1].1,
                    t: 0.5,
                },
            ))
        }
        MeshSelectMode::Points => {
            let points = selected_valid_points(mesh, selected);
            if points.len() != 2 {
                return Err("select exactly two points of the same polygon".into());
            }
            let face = (0..mesh.polygons.len())
                .find(|face| {
                    let poly = &mesh.polygons[*face];
                    points
                        .iter()
                        .all(|point| poly.corners.iter().any(|corner| corner.point == *point))
                })
                .ok_or_else(|| "the two points must belong to the same polygon".to_string())?;
            Ok((
                face as u32,
                lc_geom::ScalpelPick::Point(points[0]),
                lc_geom::ScalpelPick::Point(points[1]),
            ))
        }
        _ => Err("Scalpel works in point or edge mode".into()),
    }
}

/// Parameter along the segment `a -> b` of the point closest to `ray`, clamped
/// to the segment. `None` when the ray is parallel to the edge.
fn closest_segment_parameter(ray: &lc_core::Ray, a: Vec3, b: Vec3) -> Option<f32> {
    let direction = ray.dir.normalize_or_zero();
    if direction.length_squared() <= 1e-12 {
        return None;
    }
    let segment = b - a;
    let offset = a - ray.origin;
    let denominator = segment.length_squared() - segment.dot(direction).powi(2);
    if denominator.abs() <= 1e-8 || segment.length_squared() <= 1e-12 {
        return None;
    }
    let t = (segment.dot(direction) * direction.dot(offset) - segment.dot(offset)) / denominator;
    Some(t.clamp(0.0, 1.0))
}

/// Whether a Scalpel pick lands on one polygon's corner ring.
fn scalpel_pick_on_face(mesh: &Mesh, face: usize, pick: lc_geom::ScalpelPick) -> bool {
    let Some(poly) = mesh.polygons.get(face) else {
        return false;
    };
    match pick {
        lc_geom::ScalpelPick::Point(point) => {
            poly.corners.iter().any(|corner| corner.point == point)
        }
        lc_geom::ScalpelPick::Edge { a, b, .. } => {
            let count = poly.corners.len();
            (0..count).any(|index| {
                let start = poly.corners[index].point;
                let end = poly.corners[(index + 1) % count].point;
                (start == a && end == b) || (start == b && end == a)
            })
        }
    }
}

fn scalpel_pick_on_any_face(mesh: &Mesh, pick: lc_geom::ScalpelPick) -> bool {
    (0..mesh.polygons.len()).any(|face| scalpel_pick_on_face(mesh, face, pick))
}

/// The polygon a chained Scalpel cut splits: the polygon under the pointer when
/// it holds both picks, otherwise the first polygon that does. Two points that
/// share an edge belong to both faces, so the pointer decides the intended one.
fn scalpel_chain_face(
    mesh: &Mesh,
    under_pointer: Option<u32>,
    start: lc_geom::ScalpelPick,
    end: lc_geom::ScalpelPick,
) -> Option<u32> {
    let holds = |face: usize| {
        scalpel_pick_on_face(mesh, face, start) && scalpel_pick_on_face(mesh, face, end)
    };
    under_pointer
        .filter(|face| holds(*face as usize))
        .or_else(|| {
            (0..mesh.polygons.len())
                .find(|face| holds(*face))
                .map(|face| face as u32)
        })
}

fn selected_valid_polygons(mesh: &Mesh, selected: &[u32]) -> Vec<u32> {
    let mut unique: Vec<u32> = selected
        .iter()
        .copied()
        .filter(|p| (*p as usize) < mesh.polygons.len())
        .collect();
    unique.sort_unstable();
    unique.dedup();
    unique
}

/// The cut plane must strictly cross a polygon; that is exactly when
/// `lc_geom::ring_cut` is guaranteed to change the mesh.
fn ring_cut_would_split(mesh: &Mesh, axis: usize, offset: f32) -> bool {
    let normal = match axis {
        0 => Vec3::X,
        1 => Vec3::Y,
        _ => Vec3::Z,
    };
    mesh.polygons.iter().any(|poly| {
        let mut front = false;
        let mut back = false;
        for corner in &poly.corners {
            let Some(position) = mesh.positions.get(corner.point as usize) else {
                continue;
            };
            let distance = position.dot(normal) - offset;
            front |= distance > 1e-6;
            back |= distance < -1e-6;
        }
        front && back
    })
}

/// Weld is the editor's Collapse: both merge the selected points into one at
/// their centroid, so they share the geometry call in `lc-geom`.
fn weld_selected_points(mesh: &Mesh, selected: &[u32]) -> Mesh {
    lc_geom::collapse_points(mesh, selected).0
}

/// Orders the selected edges into exactly the two clean closed loops Bridge
/// needs. The menu enablement and the handler both call this, so they cannot
/// disagree about whether a bridge is possible.
fn selected_bridge_loops(mesh: &Mesh, selected: &[u32]) -> Result<(Vec<u32>, Vec<u32>), String> {
    let loops = selected_edge_loops(mesh, selected)?;
    match loops.as_slice() {
        [a, b] if a.len() == b.len() => Ok((a.clone(), b.clone())),
        [_, _] => Err("the two loops must have the same number of points".into()),
        other => Err(format!(
            "select exactly two closed edge loops (found {})",
            other.len()
        )),
    }
}

fn selected_edge_loops(mesh: &Mesh, selected: &[u32]) -> Result<Vec<Vec<u32>>, String> {
    let edges = mesh.edges();
    let mut adjacency: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for &index in selected {
        let Some(edge) = edges.get(index as usize) else {
            continue;
        };
        if edge.a == edge.b {
            continue;
        }
        adjacency.entry(edge.a).or_default().push(edge.b);
        adjacency.entry(edge.b).or_default().push(edge.a);
    }
    if adjacency.is_empty() {
        return Err("no edges selected".into());
    }
    for neighbors in adjacency.values_mut() {
        neighbors.sort_unstable();
        neighbors.dedup();
    }
    let mut visited = HashSet::new();
    let mut components: Vec<Vec<u32>> = Vec::new();
    for &start in adjacency.keys() {
        if !visited.insert(start) {
            continue;
        }
        let mut stack = vec![start];
        let mut component = Vec::new();
        while let Some(vertex) = stack.pop() {
            component.push(vertex);
            if let Some(neighbors) = adjacency.get(&vertex) {
                for &next in neighbors {
                    if visited.insert(next) {
                        stack.push(next);
                    }
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }
    let mut loops = Vec::with_capacity(components.len());
    for component in components {
        if component.len() < 3 {
            return Err("selected edges must form closed loops".into());
        }
        if component
            .iter()
            .any(|v| adjacency.get(v).map(|n| n.len()).unwrap_or(0) != 2)
        {
            return Err("selected edges must form clean closed loops (two per point)".into());
        }
        let start = component[0];
        let mut ordered = vec![start];
        let mut previous = None;
        let mut current = start;
        loop {
            let neighbors = &adjacency[&current];
            let next = neighbors
                .iter()
                .copied()
                .find(|n| Some(*n) != previous)
                .unwrap_or(start);
            if next == start {
                break;
            }
            if ordered.contains(&next) {
                return Err("selected edges contain a figure-eight loop".into());
            }
            ordered.push(next);
            previous = Some(current);
            current = next;
        }
        if ordered.len() != component.len() {
            return Err("could not order the selected edge loop".into());
        }
        loops.push(ordered);
    }
    Ok(loops)
}

#[cfg(test)]
mod mesh_edit_tests {
    use super::*;

    #[test]
    fn welds_selected_points_and_keeps_a_valid_mesh() {
        let mesh = Mesh::from_polygons(
            "quad",
            vec![
                Vec3::new(-1.0, 0.0, -1.0),
                Vec3::new(1.0, 0.0, -1.0),
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(-1.0, 0.0, 1.0),
            ],
            vec![vec![0, 1, 2, 3]],
        );
        let welded = weld_selected_points(&mesh, &[0, 1]);
        assert!(welded.validate().is_ok());
        assert_eq!(welded.positions.len(), 3);
        assert!((welded.positions[0].x - 0.0).abs() < 1e-6);
    }

    #[test]
    fn separates_two_selected_edge_loops() {
        let mesh = Mesh::from_polygons(
            "two loops",
            vec![
                Vec3::ZERO,
                Vec3::X,
                Vec3::Y,
                Vec3::new(0.0, 0.0, 3.0),
                Vec3::new(1.0, 0.0, 3.0),
                Vec3::new(0.0, 1.0, 3.0),
            ],
            vec![vec![0, 1, 2], vec![3, 4, 5]],
        );
        let selected: Vec<u32> = (0..mesh.edges().len() as u32).collect();
        let loops = selected_edge_loops(&mesh, &selected).expect("two loops");
        assert_eq!(loops.len(), 2);
        assert!(loops.iter().all(|l| l.len() == 3));
        assert!(point_in_polygon(
            Pos2::new(0.25, 0.25),
            &[
                Pos2::new(0.0, 0.0),
                Pos2::new(1.0, 0.0),
                Pos2::new(0.0, 1.0)
            ]
        ));
    }

    #[test]
    fn bridge_rejects_two_loops_that_cannot_pair() {
        let mesh = Mesh::from_polygons(
            "triangle and quad",
            vec![
                Vec3::ZERO,
                Vec3::X,
                Vec3::Y,
                Vec3::new(0.0, 0.0, 3.0),
                Vec3::new(1.0, 0.0, 3.0),
                Vec3::new(1.0, 1.0, 3.0),
                Vec3::new(0.0, 1.0, 3.0),
            ],
            vec![vec![0, 1, 2], vec![3, 4, 5, 6]],
        );
        let selected: Vec<u32> = (0..mesh.edges().len() as u32).collect();
        let error = selected_bridge_loops(&mesh, &selected).expect_err("unequal loops");
        assert!(error.contains("same number of points"), "{error}");
    }
}

#[cfg(test)]
mod viewport_and_render_tests {
    use super::*;

    #[test]
    fn alt_navigation_maps_drags_to_the_documented_actions() {
        let alt = egui::Modifiers {
            alt: true,
            ..Default::default()
        };
        let alt_shift = egui::Modifiers {
            alt: true,
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            alt_navigation(alt, egui::PointerButton::Primary),
            Some(ViewportNav::Orbit)
        );
        assert_eq!(
            alt_navigation(alt, egui::PointerButton::Middle),
            Some(ViewportNav::Pan)
        );
        assert_eq!(
            alt_navigation(alt, egui::PointerButton::Secondary),
            Some(ViewportNav::Pan)
        );
        assert_eq!(
            alt_navigation(alt_shift, egui::PointerButton::Primary),
            Some(ViewportNav::Pan)
        );
        assert_eq!(
            alt_navigation(egui::Modifiers::default(), egui::PointerButton::Primary),
            None
        );
    }

    #[test]
    fn scroll_zoom_moves_the_camera_both_ways() {
        let mut orbit = OrbitCamera::default();
        let start = orbit.distance;
        orbit.zoom(120.0);
        assert!(orbit.distance < start);
        let zoomed_in = orbit.distance;
        orbit.zoom(-120.0);
        assert!(orbit.distance > zoomed_in);
    }

    #[test]
    fn cancelled_render_is_recorded_as_cancelled_without_pixels() {
        let (_tx, rx) = channel();
        let active = ActiveRender {
            rx,
            cancel: Arc::new(AtomicBool::new(true)),
            id: 11,
            name: "render".into(),
            file: "render.png".into(),
            width: 64,
            height: 48,
            total: 64,
            done: 12,
            exposure: 1.0,
            started: Instant::now(),
        };
        let job = cancelled_render_job(&active);
        assert_eq!(job.id, 11);
        assert_eq!(job.status, RenderJobStatus::Cancelled);
        assert!(!job.is_finished());
        assert_eq!(job.samples_done, 12);
        assert_eq!(job.samples, 64);
        assert!(job.texture.is_none());
        assert!(job.pixels.is_none());
    }
}

#[cfg(test)]
mod workspace_persistence_tests {
    use super::{WorkspaceState, WORKSPACE_STORAGE_KEY};
    use eframe::Storage as _;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemoryStorage(HashMap<String, String>);

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }

        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn old_pane_fields_are_ignored_without_losing_workspace_state() {
        let mut storage = MemoryStorage::default();
        storage.set_string(
            WORKSPACE_STORAGE_KEY,
            "(inspector_width:Some(360.0),properties_height:Some(523.0),inspector_tab:Material)"
                .into(),
        );
        let restored = eframe::get_value::<WorkspaceState>(&storage, WORKSPACE_STORAGE_KEY)
            .expect("workspace state containing removed pane fields should deserialize");
        assert_eq!(restored.inspector_tab, super::InspectorTab::Material);
        assert_eq!(restored.browser_tab, super::BrowserTab::Objects);
    }
}

#[cfg(test)]
mod scene_open_recovery_tests {
    use super::*;

    #[test]
    fn native_scene_open_reports_staging_cleanup_warning() {
        let dir = std::env::temp_dir().join(format!(
            "leopardcraft-ui-open-recovery-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scene.lcscene");
        let stale_file = dir.join(format!(".scene.lcscene.tmp-{}-1", std::process::id()));
        std::fs::write(&stale_file, b"partial save").unwrap();
        let _ = lc_scene::take_warnings();

        reconcile_native_scene(&path);
        assert!(!stale_file.exists());
        assert!(lc_scene::take_warnings().is_empty());

        let stale = dir.join(format!(".scene.lcscene.tmp-{}-2", std::process::id()));
        std::fs::create_dir(&stale).unwrap();
        std::fs::write(stale.join("blocker"), b"keep").unwrap();
        let _ = lc_scene::take_warnings();

        reconcile_native_scene(&path);
        let warnings = lc_scene::take_warnings();
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("failed to reconcile")),
            "expected a scene-open recovery warning, got {warnings:?}"
        );
        assert!(stale.is_dir());

        let obj = dir.join("model.obj");
        std::fs::write(&obj, b"not a scene").unwrap();
        let obj_stale = dir.join(format!(".model.obj.tmp-{}-3", std::process::id()));
        std::fs::write(&obj_stale, b"OBJ recovery is handled elsewhere").unwrap();
        reconcile_native_scene(&obj);
        assert!(lc_scene::take_warnings().is_empty());
        assert!(obj.exists());
        assert!(
            obj_stale.exists(),
            "opening an OBJ must not run scene staging cleanup"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
