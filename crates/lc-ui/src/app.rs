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
    AnimChannel, AnimProperty, AnimationClip, ChannelAppliesTo, ChannelKind, Interpolation,
    Playback, TangentType,
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
    SetTagAnimationCmd, SetTransformsCmd, Shape, Skin, Tag,
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

#[path = "lifecycle.rs"]
mod lifecycle;
#[path = "lifecycle_harness.rs"]
mod lifecycle_harness;
#[path = "panels.rs"]
mod panels;
#[path = "panels_more.rs"]
mod panels_more;
#[path = "recovery.rs"]
mod recovery;
#[path = "selection.rs"]
mod selection;
#[cfg(test)]
#[path = "selection_tests.rs"]
mod selection_tests;
#[path = "shell.rs"]
mod shell;
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
    Takes,
    Layers,
    Console,
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
    ToggleSeam,
    TogglePinnedUv,
    FillHole,
    Split,
    Shell,
    Triangulate,
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
    Scalar(AnimProperty),
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
    pub(super) property: Option<AnimProperty>,
    pub(super) morph_target: Option<u32>,
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
    /// Explicit F-curve target. `None` follows the primary object.
    anim_target: Option<AnimTarget>,
    /// Selected F-curve keys, on whichever target owns them.
    selected_keys: Vec<SelectedKey>,
    /// Shared numeric factor used by selected-key and whole-take retiming.
    retime_factor: f32,
    curve_key_drag: Option<CurveKeyDrag>,
    curve_tangent_drag: Option<CurveTangentDrag>,
    curve_box_select: Option<CurveBoxSelect>,
    curve_clipboard: CurveClipboard,
    /// Registry row selected in the F-curve list, for keying.
    selected_curve_channel: Option<(AnimTarget, CurveRow)>,
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
            anim_target: None,
            selected_keys: Vec::new(),
            retime_factor: 1.0,
            curve_key_drag: None,
            curve_tangent_drag: None,
            curve_box_select: None,
            curve_clipboard: CurveClipboard::default(),
            selected_curve_channel: None,
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
        self.playback.fps = self.doc.settings.fps.max(1.0);
        self.playback.start = self.doc.settings.frame_start / self.playback.fps;
        self.playback.end = self.doc.settings.frame_end / self.playback.fps;
        self.playback.time = self.playback.start;
    }

    pub fn load_scene(&mut self, doc: Document, path: Option<PathBuf>) {
        if let Some(active) = self.active_render.take() {
            active.cancel.store(true, Ordering::Relaxed);
        }
        self.doc = doc;
        self.sync_playback_to_document();
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

    fn add_node(&mut self, node: Node, parent: Option<NodeId>) {
        let id = node.id;
        let cmd = AddNodeCmd::new(&self.doc, node, parent);
        self.history.execute(&mut self.doc, Box::new(cmd));
        self.selection.select_one(id);
        self.mesh_edit = false;
        self.clear_mesh_component_selection();
        self.browser_tab = BrowserTab::Objects;
        self.reveal_selected_in_browser = true;
        let cleared_filter = !self.hierarchy_filter.is_empty();
        if cleared_filter {
            self.hierarchy_filter.clear();
        }
        let name = self
            .doc
            .scene
            .get(id)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        self.status = if cleared_filter {
            format!("Added {name} · search cleared")
        } else {
            format!("Added {name}")
        };
    }

    fn add_primitive(&mut self, primitive: Primitive) {
        let node = self.doc.create_node(
            NodeKind::Mesh {
                geometry: GeometrySource::from_primitive(primitive),
            },
            primitive.label(),
        );
        self.add_node(node, None);
        if let Some(id) = self.selection.primary() {
            if let Some(n) = self.doc.scene.get_mut(id) {
                n.material = Some(0);
            }
        }
        self.sync_selection_material();
    }

    fn add_light(&mut self, kind: LightKind) {
        let mut node = self.doc.create_node(
            NodeKind::Light {
                light: Light {
                    kind,
                    color: Vec3::new(1.0, 0.92, 0.82),
                    intensity: 8.0,
                    cast_shadows: true,
                },
            },
            "Light",
        );
        node.transform = Transform::from_translation(Vec3::new(2.5, 3.2, 2.0));
        self.add_node(node, None);
    }

    fn add_camera(&mut self) {
        let eye = Vec3::new(5.2, 3.4, 6.4);
        let mut node = self.doc.create_node(
            NodeKind::Camera {
                camera: Camera::default(),
            },
            "Camera",
        );
        node.transform = Transform::new(
            eye,
            look_at_quat(eye, Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
            Vec3::ONE,
        );
        self.add_node(node, None);
    }

    fn add_null(&mut self) {
        let node = self.doc.create_node(NodeKind::Null, "Null");
        self.add_node(node, None);
    }

    /// Bake the selected tip's solved chain over the playback range as one undo
    /// step, keying every frame like Cheetah3D's IK bake command.
    fn bake_ik_chain(&mut self, tip: NodeId) -> bool {
        let Some(chain) = self
            .doc
            .scene
            .get(tip)
            .and_then(|node| node.ik.as_ref())
            .cloned()
        else {
            self.status = "This joint has no IK chain to bake".into();
            return false;
        };
        if !chain.enabled {
            self.status = "Enable the IK chain before baking".into();
            return false;
        }
        let joints: BTreeSet<NodeId> = ik_chain_joints(&self.doc.scene, tip, &chain)
            .into_iter()
            .collect();
        if joints.is_empty() {
            self.status = "The IK chain has no movable joints to bake".into();
            return false;
        }
        let start = self.playback.start;
        let end = self.playback.end.max(start);
        let edits: Vec<_> = bake_ik_range_edits(&self.doc, start, end, self.playback.fps)
            .into_iter()
            .filter(|(id, _, _)| joints.contains(id))
            .collect();
        if edits.is_empty() {
            self.status =
                "The IK bake range is empty, too long, or already has this pose keyed".into();
            return false;
        }
        let count = edits.len();
        self.history
            .execute(&mut self.doc, Box::new(SetAnimationsCmd { edits }));
        self.status = format!(
            "Baked IK on {count} joint{} over frames {}–{}",
            if count == 1 { "" } else { "s" },
            (start * self.playback.fps).round(),
            (end * self.playback.fps).round()
        );
        true
    }

    fn add_joint(&mut self) {
        let node = self.doc.create_node(
            NodeKind::Joint {
                length: 0.8,
                width: 0.08,
            },
            "Joint",
        );
        self.add_node(node, None);
    }

    fn add_spline(&mut self) {
        let node = self.doc.create_node(
            NodeKind::Spline {
                spline: lc_geom::Spline::new(
                    lc_geom::SplineKind::Bezier,
                    vec![
                        Vec3::new(-2.0, 0.4, 0.0),
                        Vec3::new(-1.0, 1.5, 0.0),
                        Vec3::new(0.0, 0.5, 0.0),
                        Vec3::new(1.0, 1.5, 0.0),
                        Vec3::new(2.0, 0.4, 0.0),
                    ],
                    false,
                ),
            },
            "Spline",
        );
        self.add_node(node, None);
    }

    fn delete_selected(&mut self) {
        let roots = self.selection_roots();
        if roots.is_empty() {
            return;
        }
        let count = roots.len();
        let single_name = (count == 1)
            .then(|| self.doc.scene.get(roots[0]).map(|node| node.name.clone()))
            .flatten();
        let before = self.doc.clone();
        for id in roots {
            self.doc.scene.remove_subtree(id);
        }
        let after = self.doc.clone();
        self.history.execute(
            &mut self.doc,
            Box::new(lc_scene::ReplaceDocumentCmd {
                label: match single_name {
                    Some(name) => format!("Delete {name}"),
                    None => format!("Delete {count} Objects"),
                },
                before,
                after,
            }),
        );
        self.selection.clear();
        self.mesh_edit = false;
        self.clear_mesh_component_selection();
        self.status = if count == 1 {
            "Deleted object".into()
        } else {
            format!("Deleted {count} objects")
        };
    }

    fn duplicate_selected(&mut self) {
        let roots = self.selection_roots();
        if roots.is_empty() {
            return;
        }
        let count = roots.len();
        let before = self.doc.clone();
        let mut copies = Vec::new();
        for id in roots {
            if let Some(new_id) = self.doc.scene.duplicate_subtree(id) {
                copies.push(new_id);
            }
        }
        if copies.is_empty() {
            return;
        }
        let after = self.doc.clone();
        self.history.execute(
            &mut self.doc,
            Box::new(lc_scene::ReplaceDocumentCmd {
                label: if count == 1 {
                    "Duplicate".into()
                } else {
                    format!("Duplicate {count} Objects")
                },
                before,
                after,
            }),
        );
        self.selection.set(copies);
        self.status = if count == 1 {
            "Duplicated subtree".into()
        } else {
            format!("Duplicated {count} objects")
        };
    }

    /// Selected nodes without any selected ancestor. Moving a parent already
    /// moves its children, and deleting or duplicating a parent covers them.
    fn selection_roots(&self) -> Vec<NodeId> {
        let selected: HashSet<NodeId> = self.selection.ids().iter().copied().collect();
        if selected.is_empty() {
            return Vec::new();
        }
        self.doc
            .scene
            .depth_first()
            .into_iter()
            .filter(|id| selected.contains(id))
            .filter(|id| {
                !self
                    .doc
                    .scene
                    .parent_chain(*id)
                    .iter()
                    .any(|parent| selected.contains(parent))
            })
            .collect()
    }

    /// Roots that accept a gizmo transform. Locked objects stay selected but
    /// are not edited by the shared pivot.
    fn selection_transform_roots(&self) -> Vec<NodeId> {
        self.selection_roots()
            .into_iter()
            .filter(|id| {
                self.doc
                    .scene
                    .get(*id)
                    .map(|node| !node.locked)
                    .unwrap_or(false)
            })
            .collect()
    }

    fn selection_pivot(&self, evaluated: &EvaluatedScene) -> Option<Vec3> {
        let roots = self.selection_transform_roots();
        let mut sum = Vec3::ZERO;
        let mut count = 0usize;
        for id in roots {
            if let Some(node) = evaluated.node(id) {
                sum += node.world.w_axis.truncate();
                count += 1;
            }
        }
        (count > 0).then_some(sum / count as f32)
    }

    pub(super) fn selection_flag_ready(&self, flag: NodeFlag, value: bool) -> bool {
        self.selection.ids().iter().any(|id| {
            self.doc
                .scene
                .get(*id)
                .map(|node| flag.get(&NodeProps::of(node)) != value)
                .unwrap_or(false)
        })
    }

    fn set_selection_flag(&mut self, flag: NodeFlag, value: bool) {
        if !self.selection_flag_ready(flag, value) {
            self.status = "Selected objects already have that setting".into();
            return;
        }
        let ids: Vec<NodeId> = self.selection.ids().to_vec();
        self.set_nodes_flag(&ids, flag, value);
    }

    fn set_nodes_flag(&mut self, ids: &[NodeId], flag: NodeFlag, value: bool) {
        let mut edits = Vec::new();
        for id in ids {
            let Some(node) = self.doc.scene.get(*id) else {
                continue;
            };
            let before = NodeProps::of(node);
            if flag.get(&before) == value {
                continue;
            }
            let mut after = before.clone();
            flag.set(&mut after, value);
            edits.push((*id, before, after));
        }
        if edits.is_empty() {
            return;
        }
        let count = edits.len();
        self.history
            .execute(&mut self.doc, Box::new(SetNodesPropsCmd { edits }));
        let label = match flag {
            NodeFlag::Visible => "visibility",
            NodeFlag::Renderable => "render flag",
            NodeFlag::Locked => "lock",
        };
        self.status = if count == 1 {
            format!("Changed {label}")
        } else {
            format!("Changed {label} on {count} objects")
        };
    }

    /// Group the selected roots under a new Null at their shared pivot while
    /// preserving every object's world transform.
    fn group_selection(&mut self) {
        let roots = self.selection_roots();
        if roots.is_empty() {
            return;
        }
        let pivot = self.selection_pivot(&self.evaluate()).unwrap_or(Vec3::ZERO);
        let parents: HashSet<Option<NodeId>> = roots
            .iter()
            .map(|id| self.doc.scene.get(*id).and_then(|node| node.parent))
            .collect();
        let group_parent = if parents.len() == 1 {
            parents.into_iter().next().flatten()
        } else {
            None
        };
        let parent_world = group_parent
            .map(|parent| self.doc.scene.world_matrix_plain(parent))
            .unwrap_or(Mat4::IDENTITY);
        let before = self.doc.clone();
        let mut group = self.doc.create_node(NodeKind::Null, "Group");
        let (scale, rotation, translation) = (parent_world.inverse()
            * Mat4::from_translation(pivot))
        .to_scale_rotation_translation();
        group.transform = Transform::new(translation, rotation, scale);
        let group_id = group.id;
        if let Err(error) = self.doc.scene.insert_node(group, group_parent, None) {
            self.doc = before;
            self.status = format!("Group failed: {error}");
            return;
        }
        let group_inverse = self.doc.scene.world_matrix_plain(group_id).inverse();
        for id in &roots {
            let old_world = self.doc.scene.world_matrix_plain(*id);
            if let Err(error) = self.doc.scene.reparent(*id, Some(group_id), None) {
                self.doc = before;
                self.status = format!("Group failed: {error}");
                return;
            }
            let (scale, rotation, translation) =
                (group_inverse * old_world).to_scale_rotation_translation();
            if let Some(node) = self.doc.scene.get_mut(*id) {
                node.transform = Transform::new(translation, rotation, scale);
            }
        }
        let after = self.doc.clone();
        self.history.execute(
            &mut self.doc,
            Box::new(ReplaceDocumentCmd {
                label: format!("Group {} Objects", roots.len()),
                before,
                after,
            }),
        );
        self.selection.select_one(group_id);
        self.mesh_edit = false;
        self.clear_mesh_component_selection();
        self.status = format!("Grouped {} objects", roots.len());
    }

    /// Reparent selected roots under `new_parent` (or the scene root) while
    /// preserving their world transforms.
    fn reparent_selection(&mut self, new_parent: Option<NodeId>) -> bool {
        let roots = self.selection_roots();
        if roots.is_empty() {
            return false;
        }
        if let Some(parent) = new_parent {
            if roots
                .iter()
                .any(|id| *id == parent || self.doc.scene.is_ancestor(*id, parent))
            {
                self.status = "Cannot parent an object to itself or its descendant".into();
                return false;
            }
        }
        let before = self.doc.clone();
        for id in &roots {
            let old_world = self.doc.scene.world_matrix_plain(*id);
            if let Err(error) = self.doc.scene.reparent(*id, new_parent, None) {
                self.doc = before;
                self.status = format!("Reparent failed: {error}");
                return false;
            }
            let parent_world = new_parent
                .map(|parent| self.doc.scene.world_matrix_plain(parent))
                .unwrap_or(Mat4::IDENTITY);
            let (scale, rotation, translation) =
                (parent_world.inverse() * old_world).to_scale_rotation_translation();
            if let Some(node) = self.doc.scene.get_mut(*id) {
                node.transform = Transform::new(translation, rotation, scale);
            }
        }
        let after = self.doc.clone();
        self.history.execute(
            &mut self.doc,
            Box::new(ReplaceDocumentCmd {
                label: if roots.len() == 1 {
                    "Reparent".into()
                } else {
                    format!("Reparent {} Objects", roots.len())
                },
                before,
                after,
            }),
        );
        self.status = if roots.len() == 1 {
            "Reparented object".into()
        } else {
            format!("Reparented {} objects", roots.len())
        };
        true
    }

    fn undo(&mut self) {
        if let Some(label) = self.history.undo(&mut self.doc) {
            let restored = self
                .history
                .redo_top()
                .and_then(|command| command.as_any())
                .and_then(|state| state.downcast_ref::<PasteCurveKeysCmd>())
                .map(|paste| (paste.target, paste.before_selection.clone()));
            if let Some((target, selection)) = restored {
                self.restore_paste_state(target, selection);
            }
            self.status = format!("Undo: {label}");
            self.selection.retain_existing(&self.doc.scene);
            self.invalidate_pending();
        }
    }

    fn redo(&mut self) {
        if let Some(label) = self.history.redo(&mut self.doc) {
            let restored = self
                .history
                .undo_top()
                .and_then(|command| command.as_any())
                .and_then(|state| state.downcast_ref::<PasteCurveKeysCmd>())
                .map(|paste| (paste.target, paste.after_selection.clone()));
            if let Some((target, selection)) = restored {
                self.restore_paste_state(target, selection);
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
    }

    fn new_scene(&mut self) {
        self.load_scene(Document::demo(), None);
        self.lifecycle.discard_recovery();
        self.status = "New scene from studio template".into();
    }

    fn open_file(&mut self, path: PathBuf) -> bool {
        reconcile_native_scene(&path);
        let opened = match lc_io::import_with_report(&path) {
            Ok((doc, report)) => {
                let name = path.display().to_string();
                self.load_scene(doc, Some(path));
                self.lifecycle.discard_recovery();
                self.status = match report.summary() {
                    Some(summary) => format!("Opened {name} — {summary}"),
                    None => format!("Opened {name}"),
                };
                true
            }
            Err(e) => {
                self.status = format!("Open failed: {e}");
                false
            }
        };
        self.drain_scene_warnings();
        opened
    }

    fn import_file(&mut self, path: PathBuf) {
        reconcile_native_scene(&path);
        match lc_io::import_with_report(&path) {
            Ok((mut doc, report)) => {
                doc.sanitize();
                let before = self.doc.clone();
                let after = doc;
                self.history.execute(
                    &mut self.doc,
                    Box::new(lc_scene::ReplaceDocumentCmd {
                        label: "Import".into(),
                        before,
                        after,
                    }),
                );
                self.selection.set(self.doc.scene.roots.first().copied());
                self.sync_playback_to_document();
                self.status = match report.summary() {
                    Some(summary) => format!("Imported {} — {summary}", path.display()),
                    None => format!("Imported {}", path.display()),
                };
            }
            Err(e) => self.status = format!("Import failed: {e}"),
        }
        self.drain_scene_warnings();
    }

    fn save_file(&mut self, path: PathBuf) -> Result<Option<PendingAction>, ()> {
        let result = match self.lifecycle.save_and_take_action(&self.doc, path.clone()) {
            Ok((SaveKind::Native, action)) => {
                self.status = format!("Saved {}", path.display());
                Ok(action)
            }
            Ok((SaveKind::Export, action)) => {
                self.status = format!("Exported {}", path.display());
                Ok(action)
            }
            Err(error) => {
                self.status = format!("Save failed: {error}");
                Err(())
            }
        };
        self.drain_scene_warnings();
        result
    }

    fn apply_file_dialog(&mut self, ctx: &egui::Context, mode: FileMode, text: String) {
        match mode {
            FileMode::SaveAs => match self.lifecycle.save_as_path(&text) {
                Ok(path) => {
                    if let Ok(Some(action)) = self.save_file(path) {
                        self.execute_action(ctx, action);
                    }
                }
                Err(message) => self.status = message,
            },
            FileMode::Open => {
                let Some(path) = self.dialog_path(&text) else {
                    return;
                };
                self.request_action(ctx, PendingAction::OpenPath(path));
            }
            FileMode::Import => {
                let Some(path) = self.dialog_path(&text) else {
                    return;
                };
                self.import_file(path);
            }
            FileMode::Export => {
                let Some(path) = self.dialog_path(&text) else {
                    return;
                };
                let _ = self.save_file(path);
            }
        }
    }

    fn dialog_path(&mut self, text: &str) -> Option<PathBuf> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            self.status = "No file path entered".into();
            None
        } else {
            Some(PathBuf::from(trimmed))
        }
    }

    pub(super) fn save_or_prompt(&mut self, ctx: &egui::Context) {
        if self.lifecycle.needs_save_as() {
            self.dialog = Some(FileDialog {
                mode: FileMode::SaveAs,
                path: self.save_as_default(),
            });
        } else {
            let path = self
                .lifecycle
                .path()
                .expect("a native document has a path")
                .to_path_buf();
            if let Ok(Some(action)) = self.save_file(path) {
                self.execute_action(ctx, action);
            }
        }
    }

    fn save_as_default(&self) -> String {
        self.lifecycle
            .path()
            .map(|path| path.with_extension("lcscene").display().to_string())
            .unwrap_or_else(|| "scene.lcscene".into())
    }

    fn drain_scene_warnings(&mut self) {
        self.report_scene_warnings(lc_scene::take_warnings());
    }

    /// Show non-fatal scene recovery or directory-flush warnings in the editor.
    pub fn report_scene_warnings(&mut self, warnings: Vec<String>) {
        if warnings.is_empty() {
            return;
        }
        let stamp = self.started_at.elapsed().as_secs_f32();
        for warning in &warnings {
            self.console
                .push_back(format!("[{stamp:6.1}s] warning: {warning}"));
        }
        while self.console.len() > 250 {
            self.console.pop_front();
        }
        let summary = if warnings.len() == 1 {
            format!("Scene warning: {}", warnings[0])
        } else {
            format!("{} scene warnings — see Console", warnings.len())
        };
        self.status = if self.status.is_empty() {
            summary
        } else {
            format!("{} — {summary}", self.status)
        };
    }

    fn document_name(&self) -> String {
        self.lifecycle
            .path()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| "Untitled".into())
    }

    fn request_action(&mut self, ctx: &egui::Context, action: PendingAction) {
        match self.lifecycle.request(&self.doc, action) {
            GuardResult::Run(action) => self.execute_action(ctx, action),
            GuardResult::Confirm => {}
            GuardResult::Save | GuardResult::Cancelled => {}
        }
    }

    fn execute_action(&mut self, ctx: &egui::Context, action: PendingAction) {
        match action {
            PendingAction::NewScene => self.new_scene(),
            PendingAction::OpenScene => {
                self.dialog = Some(FileDialog {
                    mode: FileMode::Open,
                    path: self
                        .lifecycle
                        .path()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "scene.lcscene".into()),
                });
            }
            PendingAction::OpenPath(path) => {
                self.open_file(path);
            }
            PendingAction::Quit => {
                self.lifecycle.finish_quit();
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn autosave_tick(&mut self, ctx: &egui::Context) {
        match self.lifecycle.autosave_tick(&self.doc) {
            AutosaveOutcome::Idle | AutosaveOutcome::Clean | AutosaveOutcome::Saved => {}
            AutosaveOutcome::Failed(error) => {
                self.status = format!("Autosave failed: {error}");
            }
        }
        self.drain_scene_warnings();
        // Autosave needs a wake-up even when the user is idle and no input
        // events would otherwise trigger another frame.
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    fn push_render_job(&mut self, job: RenderJob) {
        let id = job.id;
        self.render_jobs.push(job);
        while self.render_jobs.len() > 8 {
            let removed = self.render_jobs.remove(0);
            if self.selected_render_job == Some(removed.id) {
                self.selected_render_job = None;
            }
        }
        self.selected_render_job = Some(id);
    }

    fn selected_render(&self) -> Option<&RenderJob> {
        self.selected_render_job
            .and_then(|id| self.render_jobs.iter().find(|job| job.id == id))
    }

    fn selected_finished_render(&self) -> Option<&RenderJob> {
        self.selected_render().filter(|job| job.is_finished())
    }

    fn poll_render(&mut self, ctx: &egui::Context) {
        let polled = self
            .active_render
            .as_ref()
            .map(|active| active.rx.try_recv());
        match polled {
            None => {}
            Some(Ok(RenderMessage::Progress { done, total })) => {
                if let Some(active) = self.active_render.as_mut() {
                    active.done = done;
                    active.total = total;
                }
                ctx.request_repaint_after(Duration::from_millis(120));
            }
            Some(Ok(RenderMessage::Done(result))) => {
                let Some(active) = self.active_render.take() else {
                    return;
                };
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [result.width as usize, result.height as usize],
                    &result.rgba,
                );
                let texture = ctx.load_texture(
                    format!("render-job-{}", active.id),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                let job_id = active.id;
                self.push_render_job(RenderJob {
                    id: job_id,
                    name: active.name,
                    width: result.width,
                    height: result.height,
                    samples: active.total,
                    samples_done: active.total,
                    seconds: active.started.elapsed().as_secs_f32(),
                    exposure: active.exposure,
                    date: utc_now_string(),
                    file: active.file,
                    status: RenderJobStatus::Finished,
                    texture: Some(texture),
                    pixels: Some(result.pixels),
                });
                self.status = format!(
                    "Rendered {}×{} ({} spp)",
                    result.width, result.height, active.total
                );
                self.show_render_manager = true;
            }
            Some(Ok(RenderMessage::Failed(error))) => {
                self.active_render = None;
                self.status = format!("Render failed: {error}");
            }
            Some(Ok(RenderMessage::Cancelled)) => {
                if let Some(active) = self.active_render.take() {
                    self.push_render_job(cancelled_render_job(&active));
                }
                self.status = "Render cancelled".into();
                self.show_render_manager = true;
            }
            Some(Err(std::sync::mpsc::TryRecvError::Empty)) => {
                ctx.request_repaint_after(Duration::from_millis(120));
            }
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                self.active_render = None;
                self.status = "Render worker stopped unexpectedly".into();
            }
        }
    }

    fn start_render(&mut self, ctx: &egui::Context) {
        if self.active_render.is_some() {
            self.status = "A render is already running".into();
            self.show_render_manager = true;
            return;
        }
        let doc = self.doc.clone();
        let time = self.playback.time;
        let settings = self.doc.settings.render.clone();
        let base_dir = self
            .lifecycle
            .path()
            .and_then(|path| path.parent())
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let output = if self.render_path.trim().is_empty() {
            "render.png".to_string()
        } else {
            self.render_path.clone()
        };
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let exposure = settings.exposure;
        std::thread::spawn(move || {
            let result = (|| -> Result<RenderResult, String> {
                let scene = RenderScene::from_document(&doc, time, base_dir)?;
                let mut tracer = PathTracer::new(settings.width, settings.height, settings.seed);
                let mut done = 0u32;
                while done < settings.samples {
                    if worker_cancel.load(Ordering::Relaxed) {
                        return Err("__cancelled__".into());
                    }
                    let chunk = (settings.samples - done).min(4);
                    tracer.render_pass(&scene, chunk);
                    done += chunk;
                    let _ = tx.send(RenderMessage::Progress {
                        done,
                        total: settings.samples,
                    });
                }
                let pixels = tracer.hdr_pixels();
                let rgba = tracer
                    .rgba8(settings.exposure, settings.transparent_background)
                    .into_iter()
                    .flatten()
                    .collect();
                Ok(RenderResult {
                    width: settings.width,
                    height: settings.height,
                    pixels,
                    rgba,
                })
            })();
            match result {
                Ok(result) => {
                    let _ = tx.send(RenderMessage::Done(Box::new(result)));
                }
                Err(error) if error == "__cancelled__" => {
                    let _ = tx.send(RenderMessage::Cancelled);
                }
                Err(error) => {
                    let _ = tx.send(RenderMessage::Failed(error));
                }
            }
        });
        let name = Path::new(&output)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Untitled")
            .to_string();
        self.active_render = Some(ActiveRender {
            rx,
            cancel,
            id: self.next_job_id,
            name,
            file: output,
            width: settings.width,
            height: settings.height,
            total: settings.samples,
            done: 0,
            exposure,
            started: Instant::now(),
        });
        self.next_job_id += 1;
        self.show_render_manager = true;
        self.status = "Rendering on CPU…".into();
        ctx.request_repaint();
    }

    fn save_render(&mut self, exr: bool) {
        let Some(id) = self.selected_render_job else {
            self.status = "Render a frame before saving".into();
            return;
        };
        let outcome = {
            let Some(job) = self.render_jobs.iter().find(|job| job.id == id) else {
                self.status = "Select a finished render job".into();
                return;
            };
            if !job.is_finished() {
                self.status = "Select a finished render job".into();
                return;
            }
            let Some(pixels) = job.pixels.as_ref() else {
                self.status = "Select a finished render job".into();
                return;
            };
            let mut path = PathBuf::from(if job.file.trim().is_empty() {
                self.render_path.trim()
            } else {
                job.file.trim()
            });
            if path.as_os_str().is_empty() {
                path = PathBuf::from("render.png");
            }
            if exr {
                path.set_extension("exr");
                lc_render::save_linear_exr(&path, pixels, job.width, job.height)
                    .map(|()| path.clone())
                    .map_err(|e| format!("EXR save failed: {e}"))
            } else {
                path.set_extension("png");
                lc_render::save_linear_png(&path, pixels, job.width, job.height, job.exposure)
                    .map(|()| path.clone())
                    .map_err(|e| format!("PNG save failed: {e}"))
            }
        };
        self.status = match outcome {
            Ok(path) => format!("Saved {}", path.display()),
            Err(error) => error,
        };
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

    /// Whether the inspector's modifier stack can meaningfully add this
    /// modifier to the primary object. The handler that pushes it calls the
    /// same predicate.
    fn modifier_ready(&self, modifier: &Modifier) -> bool {
        let Some(node) = self
            .selection
            .primary()
            .and_then(|id| self.doc.scene.get(id))
        else {
            return false;
        };
        if !modifier_applies_to(node, modifier) {
            return false;
        }
        if let Modifier::Boolean { operand, .. } = modifier {
            return *operand != node.id
                && self
                    .doc
                    .scene
                    .get(*operand)
                    .map(|operand| operand.is_mesh())
                    .unwrap_or(false);
        }
        true
    }

    fn add_modifier_to_primary(&mut self, modifier: Modifier) -> bool {
        let Some(id) = self.selection.primary() else {
            self.status = "Select an object before adding a modifier".into();
            return false;
        };
        if !self.modifier_ready(&modifier) {
            self.status = format!("{} needs a compatible object", modifier.label());
            return false;
        }
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        let before = Shape::of(node);
        let mut after = before.clone();
        let label = modifier.label();
        after.modifiers.push(modifier);
        self.history
            .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
        self.status = format!("Added {label}");
        true
    }

    fn modifier_remove_ready(&self, index: usize) -> bool {
        self.selection
            .primary()
            .and_then(|id| self.doc.scene.get(id))
            .map(|node| index < node.modifiers.len())
            .unwrap_or(false)
    }

    fn remove_modifier_from_primary(&mut self, index: usize) -> bool {
        let Some(id) = self.selection.primary() else {
            return false;
        };
        if !self.modifier_remove_ready(index) {
            self.status = "That modifier no longer exists".into();
            return false;
        }
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        let before = Shape::of(node);
        let mut after = before.clone();
        let label = after.modifiers.remove(index).label();
        self.history
            .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
        self.status = format!("Removed {label}");
        true
    }

    fn all_joint_ids(&self) -> Vec<NodeId> {
        self.doc
            .scene
            .depth_first()
            .into_iter()
            .filter(|id| {
                matches!(
                    self.doc.scene.get(*id).map(|node| &node.kind),
                    Some(NodeKind::Joint { .. })
                )
            })
            .collect()
    }

    /// The joints selected for binding. `None` means every joint in the scene.
    fn bind_joint_ids(&self) -> Vec<NodeId> {
        self.all_joint_ids()
            .into_iter()
            .filter(|id| {
                self.bind_joints
                    .as_ref()
                    .is_none_or(|joints| joints.contains(id))
            })
            .collect()
    }

    fn set_all_bind_joints(&mut self, all: bool) {
        self.bind_joints = if all { None } else { Some(BTreeSet::new()) };
    }

    fn set_bind_joint(&mut self, id: NodeId, enabled: bool) {
        let all = self.all_joint_ids();
        let mut joints = self
            .bind_joints
            .clone()
            .unwrap_or_else(|| all.iter().copied().collect());
        if enabled {
            joints.insert(id);
        } else {
            joints.remove(&id);
        }
        self.bind_joints = if joints.len() == all.len() {
            None
        } else {
            Some(joints)
        };
    }

    /// The evaluated mesh and chosen joints a bind operation would use.
    fn skin_target(&self) -> Option<SkinTarget> {
        let id = self.selection.primary()?;
        let node = self.doc.scene.get(id)?;
        if !node.is_mesh() {
            return None;
        }
        let evaluated = self.evaluate();
        let vertex_count = evaluated
            .node(id)
            .and_then(|node| node.mesh())
            .map(|mesh| mesh.positions.len())
            .unwrap_or(0);
        let positions = evaluated
            .node(id)
            .and_then(|node| node.mesh())
            .map(|mesh| mesh.positions.clone())
            .unwrap_or_default();
        let mesh_inverse = evaluated
            .node(id)
            .map(|node| node.world.inverse())
            .unwrap_or(Mat4::IDENTITY);
        let joints: Vec<(NodeId, Mat4, f32)> = self
            .bind_joint_ids()
            .into_iter()
            .filter_map(|joint_id| {
                let node = self.doc.scene.get(joint_id)?;
                let length = match &node.kind {
                    NodeKind::Joint { length, .. } => *length,
                    _ => return None,
                };
                let world = evaluated.node(joint_id)?.world;
                Some((joint_id, world, length.max(0.0)))
            })
            .collect();
        Some(SkinTarget {
            vertex_count,
            joints: joints.iter().map(|(joint, _, _)| *joint).collect(),
            inverse_bind: joints.iter().map(|(_, world, _)| world.inverse()).collect(),
            segments: joints
                .iter()
                .map(|(_, world, length)| {
                    (
                        mesh_inverse.transform_point3(world.transform_point3(Vec3::ZERO)),
                        mesh_inverse.transform_point3(world.transform_point3(Vec3::Y * *length)),
                    )
                })
                .collect(),
            positions,
            existing: node.skin.clone(),
        })
    }

    fn build_bound_skin(&self, target: &SkinTarget) -> Option<Skin> {
        if target.vertex_count == 0 || target.joints.is_empty() {
            return None;
        }
        let mut skin = Skin::new(target.joints.clone(), target.inverse_bind.clone());
        skin.clear_weights(target.vertex_count);
        let auto: Option<Skin> = if self.bind_auto_weights {
            let mut auto = Skin::new(target.joints.clone(), target.inverse_bind.clone());
            if auto.auto_weight_proximity(
                &target.positions,
                &target.segments,
                ProximityWeightOptions {
                    dropoff: self.bind_dropoff,
                    strength: self.bind_strength,
                    max_influences: self.bind_max_influences,
                },
            ) {
                Some(auto)
            } else {
                None
            }
        } else {
            None
        };
        for vertex in 0..target.vertex_count {
            let mut carried: Vec<(usize, f32)> = Vec::new();
            if let Some(existing) = &target.existing {
                let old_indices = existing.joint_indices.get(vertex);
                let old_weights = existing.weights.get(vertex);
                if let (Some(old_indices), Some(old_weights)) = (old_indices, old_weights) {
                    for (old_index, weight) in old_indices.iter().zip(old_weights) {
                        if *weight <= 0.0 || !weight.is_finite() {
                            continue;
                        }
                        let Some(old_joint) = existing.joints.get(*old_index as usize) else {
                            continue;
                        };
                        let Some(new_index) =
                            target.joints.iter().position(|joint| joint == old_joint)
                        else {
                            continue;
                        };
                        if let Some((_, total)) =
                            carried.iter_mut().find(|(index, _)| *index == new_index)
                        {
                            *total += *weight;
                        } else {
                            carried.push((new_index, *weight));
                        }
                    }
                }
            }
            if !carried.is_empty() {
                for (slot, (index, weight)) in carried.into_iter().take(4).enumerate() {
                    skin.joint_indices[vertex][slot] = index as u32;
                    skin.weights[vertex][slot] = weight;
                }
                skin.compact_vertex(vertex);
                skin.normalize_vertex(vertex);
            } else if let Some(auto) = &auto {
                skin.joint_indices[vertex] = auto.joint_indices[vertex];
                skin.weights[vertex] = auto.weights[vertex];
            }
        }
        Some(skin)
    }

    fn bind_skin_to_joints(&mut self) -> bool {
        let Some(id) = self.selection.primary() else {
            return false;
        };
        let Some(target) = self.skin_target() else {
            self.status = "Select a mesh to bind".into();
            return false;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        let Some(skin) = self.build_bound_skin(&target) else {
            self.status = "Choose at least one joint and a non-empty mesh first".into();
            return false;
        };
        let active_joint = self
            .weight_joint
            .and_then(|index| target.existing.as_ref()?.joints.get(index).copied());
        let next_weight_joint = active_joint.map(|joint| {
            skin.joints
                .iter()
                .position(|candidate| *candidate == joint)
                .unwrap_or(0)
        });
        if node.skin.as_ref() == Some(&skin) {
            self.status = "Skin already matches the selected joints and weights".into();
            return false;
        }
        let before = Shape::of(node);
        let mut after = before.clone();
        after.skin = Some(skin);
        self.history
            .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
        self.status = if target.existing.is_some() {
            "Rebound skin, preserving covered weights".into()
        } else {
            "Bound skin to the selected joints".into()
        };
        if active_joint.is_some() {
            self.weight_joint = next_weight_joint;
        }
        true
    }

    fn unbind_skin_from_primary(&mut self) -> bool {
        let Some(id) = self.selection.primary() else {
            return false;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        if !skin_unbind_ready(node) {
            self.status = "This object has no skin to unbind".into();
            return false;
        }
        let before = Shape::of(node);
        let mut after = before.clone();
        after.skin = None;
        self.history
            .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
        self.status = "Unbound skin".into();
        true
    }

    fn active_weight_joint(&self) -> Option<usize> {
        let id = self.selection.primary()?;
        let skin = self.doc.scene.get(id)?.skin.as_ref()?;
        if skin.joints.is_empty() {
            return None;
        }
        Some(self.weight_joint.unwrap_or(0).min(skin.joints.len() - 1))
    }

    fn weight_paint_ready(&self) -> bool {
        let Some((id, mesh)) = self.selected_editable_mesh() else {
            return false;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        let Some(skin) = node.skin.as_ref() else {
            return false;
        };
        node.modifiers.is_empty()
            && skin.vertex_count() == mesh.positions.len()
            && !skin.joints.is_empty()
    }

    fn set_weight_paint(&mut self, enabled: bool) {
        if enabled && !self.weight_paint_ready() {
            self.status = "Weight painting needs a bound, modifier-free editable mesh".into();
            self.weight_paint = false;
            return;
        }
        self.weight_paint = enabled;
        if enabled {
            self.mesh_edit = false;
            self.clear_mesh_component_selection();
            self.gizmo_drag = None;
            self.status = "Weight paint mode".into();
        }
    }

    fn begin_weight_stroke(&mut self) -> bool {
        if !self.weight_paint_ready() {
            return false;
        }
        let Some(id) = self.selection.primary() else {
            return false;
        };
        if self.pending_shape.is_none() {
            if let Some(node) = self.doc.scene.get(id) {
                self.pending_shape = Some((id, Shape::of(node)));
            }
        }
        true
    }

    fn paint_weight_brush(
        &mut self,
        evaluated: &EvaluatedScene,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
        pointer: Pos2,
    ) -> bool {
        if !self.weight_paint_ready() {
            return false;
        }
        let Some(id) = self.selection.primary() else {
            return false;
        };
        let Some(joint) = self.active_weight_joint() else {
            return false;
        };
        let Some(node) = evaluated.node(id) else {
            return false;
        };
        let Some(mesh) = node.mesh() else {
            return false;
        };
        let Some(mut skin) = self.doc.scene.get(id).and_then(|node| node.skin.clone()) else {
            return false;
        };
        if skin.vertex_count() < mesh.positions.len() {
            return false;
        }
        let radius = self.weight_brush.radius.max(1.0);
        let strength = self.weight_brush.strength.clamp(0.0, 1.0);
        let mode = self.weight_brush.mode;
        let falloff = self.weight_brush.falloff;
        let before_weights: Vec<f32> = (0..mesh.positions.len())
            .map(|vertex| skin.influence(vertex, joint))
            .collect();
        let adjacency: Vec<Vec<u32>> = if mode == WeightBrushMode::Smooth {
            let mut adjacency = vec![Vec::new(); mesh.positions.len()];
            for edge in mesh.edges() {
                let a = edge.a as usize;
                let b = edge.b as usize;
                if a < adjacency.len() && b < adjacency.len() {
                    adjacency[a].push(edge.b);
                    adjacency[b].push(edge.a);
                }
            }
            adjacency
        } else {
            Vec::new()
        };
        let mut painted = 0usize;
        for (vertex, position) in mesh.positions.iter().enumerate() {
            let Some(screen) = self.project(
                node.world.transform_point3(*position),
                rect,
                pixel_w,
                pixel_h,
            ) else {
                continue;
            };
            let distance = (screen - pointer).length();
            if distance > radius {
                continue;
            }
            let factor = strength * falloff.factor(distance, radius);
            if factor <= 0.0 {
                continue;
            }
            let current = before_weights[vertex];
            let changed = match mode {
                WeightBrushMode::Add => skin.set_influence(vertex, joint, current + factor),
                WeightBrushMode::Subtract => skin.set_influence(vertex, joint, current - factor),
                WeightBrushMode::Replace => skin.set_exclusive_influence(vertex, joint, factor),
                WeightBrushMode::Smooth => {
                    let neighbours = &adjacency[vertex];
                    let average = if neighbours.is_empty() {
                        current
                    } else {
                        neighbours
                            .iter()
                            .map(|neighbour| before_weights[*neighbour as usize])
                            .sum::<f32>()
                            / neighbours.len() as f32
                    };
                    skin.set_influence(vertex, joint, current + (average - current) * factor)
                }
            };
            painted += usize::from(changed);
        }
        if painted == 0 {
            return false;
        }
        if let Some(node) = self.doc.scene.get_mut(id) {
            node.skin = Some(skin);
        }
        self.status = format!("Painted {painted} weight vertices");
        true
    }

    fn perform_morph_action(&mut self, id: NodeId, action: MorphAction) {
        if self.selection.primary() != Some(id) {
            return;
        }
        match action {
            MorphAction::RecordBase => {
                self.record_morph_base();
            }
            MorphAction::Capture => {
                self.capture_morph_target();
            }
            MorphAction::Restore => {
                self.restore_morph_base();
            }
            MorphAction::Remove(index) => {
                self.remove_morph_target(index);
            }
        }
    }

    fn record_morph_base(&mut self) -> bool {
        let Some((id, mesh)) = self.selected_editable_mesh() else {
            self.status = "Morph targets need an editable mesh".into();
            return false;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        if node.morph.has_base() || !node.morph.targets.is_empty() {
            self.status = "This mesh already has a morph base".into();
            return false;
        }
        if mesh.positions.is_empty() {
            self.status = "The mesh has no vertices to record".into();
            return false;
        }
        let before = node.morph.clone();
        let vertices = mesh.positions.len();
        self.history.execute(
            &mut self.doc,
            Box::new(SetMorphCmd {
                id,
                before,
                after: MorphSet {
                    base: mesh.positions,
                    targets: Vec::new(),
                },
            }),
        );
        self.status = format!("Recorded morph base ({vertices} vertices)");
        true
    }

    fn capture_morph_target(&mut self) -> bool {
        let Some((id, mesh)) = self.selected_editable_mesh() else {
            self.status = "Morph targets need an editable mesh".into();
            return false;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        if !node.morph.has_base() {
            self.status = "Record a morph base before capturing a target".into();
            return false;
        }
        if node.morph.base.len() != mesh.positions.len() {
            self.status = "The morph base does not match the current mesh topology".into();
            return false;
        }
        let deltas: Vec<Vec3> = mesh
            .positions
            .iter()
            .zip(&node.morph.base)
            .map(|(position, base)| *position - *base)
            .collect();
        if deltas.iter().all(|delta| delta.length_squared() < 1e-10) {
            self.status = "Move mesh vertices before capturing a morph target".into();
            return false;
        }
        let name = node.morph.unique_target_name();
        let before = node.morph.clone();
        let mut after = before.clone();
        after.targets.push(MorphTarget::new(name.clone(), deltas));
        self.history
            .execute(&mut self.doc, Box::new(SetMorphCmd { id, before, after }));
        self.status = format!("Captured morph target {name}");
        true
    }

    fn restore_morph_base(&mut self) -> bool {
        let Some((id, mesh)) = self.selected_editable_mesh() else {
            self.status = "Morph targets need an editable mesh".into();
            return false;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        if !node.morph.has_base() {
            self.status = "Record a morph base first".into();
            return false;
        }
        let mut positions = mesh.positions.clone();
        if !node.morph.restore_base(&mut positions) {
            self.status = "The morph base does not match the current mesh topology".into();
            return false;
        }
        if positions == mesh.positions {
            self.status = "The mesh is already at the morph base".into();
            return false;
        }
        let before = Shape::of(node);
        let mut after = before.clone();
        let NodeKind::Mesh {
            geometry: GeometrySource::Editable { mesh, .. },
        } = &mut after.kind
        else {
            return false;
        };
        mesh.positions = positions;
        self.history
            .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
        self.status = "Restored morph base".into();
        true
    }

    fn remove_morph_target(&mut self, index: usize) -> bool {
        let Some((id, _)) = self.selected_editable_mesh() else {
            return false;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        let Some(removed) = node.morph.targets.get(index) else {
            self.status = "That morph target no longer exists".into();
            return false;
        };
        let name = removed.name.clone();
        let before = node.morph.clone();
        let mut after = before.clone();
        after.targets.remove(index);
        self.history
            .execute(&mut self.doc, Box::new(SetMorphCmd { id, before, after }));
        self.status = format!("Removed morph target {name}");
        true
    }

    fn apply_node_edits(&mut self, id: NodeId, original: &Node, edited: &Node) {
        let Some(current) = self.doc.scene.get(id).cloned() else {
            return;
        };
        if edited.name != original.name {
            if let Some(cmd) = RenameNodeCmd::new(&self.doc, id, edited.name.clone()) {
                self.history.execute(&mut self.doc, Box::new(cmd));
            }
        }
        let before_props = NodeProps::of(&current);
        let after_props = NodeProps::of(edited);
        if before_props != after_props {
            self.history.execute(
                &mut self.doc,
                Box::new(SetNodePropsCmd {
                    id,
                    before: before_props,
                    after: after_props,
                }),
            );
        }
        let before_shape = Shape::of(&current);
        let after_shape = Shape::of(edited);
        if before_shape.kind != after_shape.kind
            || before_shape.modifiers != after_shape.modifiers
            || before_shape.skin != after_shape.skin
            || before_shape.morph != after_shape.morph
        {
            if self.pending_shape.is_none() {
                self.pending_shape = Some((id, before_shape.clone()));
            }
            if let Some(node) = self.doc.scene.get_mut(id) {
                node.kind = edited.kind.clone();
                node.modifiers = edited.modifiers.clone();
                node.skin = edited.skin.clone();
                node.morph = edited.morph.clone();
            }
        }
        if edited.transform != original.transform {
            let pending = self.pending_transform.get_or_insert_with(Vec::new);
            if !pending.iter().any(|(pending_id, _)| *pending_id == id) {
                pending.push((id, current.transform));
            }
            if let Some(node) = self.doc.scene.get_mut(id) {
                node.transform = edited.transform;
            }
        }
        self.sync_selection_material();
    }

    pub(super) fn selected_is_mesh(&self) -> bool {
        self.selection
            .primary()
            .and_then(|id| self.doc.scene.get(id))
            .map(|node| node.is_mesh())
            .unwrap_or(false)
    }

    /// Component mode needs a mesh-only object selection: every selected
    /// object must be a mesh, and primitives are eligible because entering
    /// component mode can convert them in one batch.
    pub(super) fn mesh_mode_ready(&self) -> bool {
        !self.selection.is_empty()
            && self.selection.ids().iter().all(|id| {
                self.doc
                    .scene
                    .get(*id)
                    .map(|node| node.is_mesh())
                    .unwrap_or(false)
            })
    }

    fn selected_meshes_editable(&self) -> bool {
        !self.selection.is_empty()
            && self.selection.ids().iter().all(|id| {
                matches!(
                    self.doc.scene.get(*id).map(|node| &node.kind),
                    Some(NodeKind::Mesh {
                        geometry: GeometrySource::Editable { .. }
                    })
                )
            })
    }

    pub(super) fn mesh_component_selection_ready(&self) -> bool {
        self.mesh_edit && self.selected_meshes_editable()
    }

    /// True when every selected mesh is editable, in the required component
    /// mode, and has at least `minimum` selected components. This is the single
    /// acceptance rule for every component tool: menu enablement and the
    /// handlers both go through it.
    pub(super) fn mesh_topology_ready(&self, mode: MeshSelectMode, minimum: usize) -> bool {
        self.mesh_component_selection_ready()
            && self.mesh_select_mode == mode
            && self
                .selection
                .ids()
                .iter()
                .all(|id| self.mesh_components_for(*id).len() >= minimum)
    }

    fn editable_mesh(&self, id: NodeId) -> Option<&Mesh> {
        match self.doc.scene.get(id).map(|node| &node.kind) {
            Some(NodeKind::Mesh {
                geometry: GeometrySource::Editable { mesh, .. },
            }) => Some(mesh),
            _ => None,
        }
    }

    /// Weld needs two distinct, in-range points on every selected mesh.
    pub(super) fn mesh_weld_ready(&self) -> bool {
        self.mesh_component_selection_ready()
            && self.mesh_select_mode == MeshSelectMode::Points
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        selected_valid_points(mesh, self.mesh_components_for(*id)).len() >= 2
                    })
                    .unwrap_or(false)
            })
    }

    /// Optimize welds the selected points that sit within the tolerance, so it
    /// needs point mode, a positive tolerance and at least one cluster on every
    /// selected mesh.
    pub(super) fn mesh_optimize_ready(&self) -> bool {
        self.mesh_component_selection_ready()
            && self.mesh_select_mode == MeshSelectMode::Points
            && self.mesh_optimize_tolerance.is_finite()
            && self.mesh_optimize_tolerance > 0.0
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        lc_geom::can_optimize(
                            mesh,
                            self.mesh_components_for(*id),
                            self.mesh_optimize_tolerance,
                        )
                    })
                    .unwrap_or(false)
            })
    }

    /// Point Slide moves the selected points toward their neighbours, so it
    /// needs point mode, a fraction above zero and at least one selected point
    /// that an edge connects to something.
    pub(super) fn mesh_point_slide_ready(&self) -> bool {
        self.mesh_component_selection_ready()
            && self.mesh_select_mode == MeshSelectMode::Points
            && self.mesh_slide_fraction.is_finite()
            && self.mesh_slide_fraction > 0.0
            && self.mesh_slide_fraction <= 1.0
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        lc_geom::can_slide_points(
                            mesh,
                            self.mesh_components_for(*id),
                            self.mesh_slide_fraction,
                        )
                    })
                    .unwrap_or(false)
            })
    }

    /// Collapse welds the selected points into one at their centroid, so it
    /// needs point mode and two distinct selected points on every selected mesh
    /// — the same rule as Weld, which shares its geometry call.
    pub(super) fn mesh_collapse_ready(&self) -> bool {
        self.mesh_weld_ready()
    }

    /// Ring Cut is a through-plane cut: it must cross geometry on every
    /// selected mesh, otherwise the handler would report "no change".
    pub(super) fn mesh_ring_cut_ready(&self) -> bool {
        self.mesh_component_selection_ready()
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| ring_cut_would_split(mesh, self.mesh_axis, self.mesh_offset))
                    .unwrap_or(false)
            })
    }

    /// Component transforms need at least one affected point that the current
    /// mode, axis and value would actually move on every selected mesh.
    pub(super) fn mesh_transform_ready(&self) -> bool {
        self.mesh_component_selection_ready()
            && self.selection.ids().iter().all(|id| {
                let Some(mesh) = self.editable_mesh(*id) else {
                    return false;
                };
                let affected: HashSet<u32> = mesh_affected_points(
                    mesh,
                    self.mesh_select_mode,
                    self.mesh_components_for(*id),
                )
                .into_iter()
                .collect();
                if affected.is_empty() {
                    return false;
                }
                let rotation = self
                    .doc
                    .scene
                    .world_matrix_plain(*id)
                    .to_scale_rotation_translation()
                    .1;
                let local_axis =
                    mesh_transform_axis(mesh, &affected, self.mesh_space, self.mesh_axis, rotation);
                mesh_transform_would_change(
                    mesh,
                    &affected,
                    self.mesh_transform,
                    local_axis,
                    self.mesh_value,
                )
            })
    }

    /// Material assignment needs a material and selected polygons on every
    /// selected mesh so one undo cannot leave part of the selection untouched.
    pub(super) fn mesh_assign_material_ready(&self) -> bool {
        self.mesh_component_selection_ready()
            && self.mesh_select_mode == MeshSelectMode::Polygons
            && self.selected_material_id().is_some()
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        self.mesh_components_for(*id)
                            .iter()
                            .any(|index| mesh.polygons.get(*index as usize).is_some())
                    })
                    .unwrap_or(false)
            })
    }

    fn selected_material_id(&self) -> Option<MaterialId> {
        self.selected_material
            .filter(|id| self.doc.materials.get(*id).is_some())
    }

    /// Bridge Loops needs every selected mesh to carry exactly two clean
    /// closed edge loops of equal length. This one check backs both the menu
    /// button and the handler.
    pub(super) fn mesh_bridge_ready(&self) -> bool {
        self.mesh_bridge_failure().is_none()
    }

    fn mesh_bridge_failure(&self) -> Option<String> {
        if !self.mesh_topology_ready(MeshSelectMode::Edges, 1) {
            return Some(self.mesh_tool_refusal(MeshSelectMode::Edges, 1, "bridge"));
        }
        for id in self.selection.ids() {
            if let Err(error) = self.check_bridge_loops(*id) {
                let target = if self.selection.len() == 1 {
                    String::new()
                } else {
                    let name = self
                        .doc
                        .scene
                        .get(*id)
                        .map(|node| node.name.clone())
                        .unwrap_or_default();
                    format!(" on \"{name}\"")
                };
                return Some(format!(
                    "Bridge Loops needs exactly two closed edge loops of equal length{target}: {error}"
                ));
            }
        }
        None
    }

    fn check_bridge_loops(&self, id: NodeId) -> Result<(), String> {
        let mesh = self.editable_mesh(id).ok_or("not an editable mesh")?;
        selected_bridge_loops(mesh, self.mesh_components_for(id)).map(|_| ())
    }

    /// Fill Hole accepts a selection in point, edge or polygon mode and only
    /// needs part of one open border, matching the documented tool (source 38).
    pub(super) fn mesh_fill_hole_ready(&self) -> bool {
        if !self.mesh_component_selection_ready() {
            return false;
        }
        let mode = self.mesh_select_mode;
        self.selection.ids().iter().all(|id| {
            self.editable_mesh(*id)
                .map(|mesh| {
                    let components = self.mesh_components_for(*id);
                    if components.is_empty() {
                        return false;
                    }
                    let points = mesh_affected_points(mesh, mode, components);
                    !lc_geom::open_holes(mesh, &points).is_empty()
                })
                .unwrap_or(false)
        })
    }

    /// Open hole borders under the primary mesh's component selection, used by
    /// the Fill Hole count label.
    pub(super) fn selected_hole_count(&self) -> usize {
        let Some((_, mesh)) = self.selected_editable_mesh() else {
            return 0;
        };
        let points = mesh_affected_points(&mesh, self.mesh_select_mode, &self.mesh_selected);
        lc_geom::open_holes(&mesh, &points).len()
    }

    /// Split copies the selected polygons into a new object, so like the
    /// documented tool (source 39) it is only available in polygon mode on raw
    /// polygon objects.
    pub(super) fn mesh_split_ready(&self) -> bool {
        self.mesh_topology_ready(MeshSelectMode::Polygons, 1)
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        !selected_valid_polygons(mesh, self.mesh_components_for(*id)).is_empty()
                    })
                    .unwrap_or(false)
            })
    }

    /// Triangulate needs polygon mode and at least one selected polygon that
    /// actually has more than three corners, so it never reports a no-op.
    pub(super) fn mesh_triangulate_ready(&self) -> bool {
        self.mesh_topology_ready(MeshSelectMode::Polygons, 1)
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        selected_valid_polygons(mesh, self.mesh_components_for(*id))
                            .iter()
                            .any(|face| {
                                mesh.polygons
                                    .get(*face as usize)
                                    .is_some_and(|polygon| polygon.corners.len() > 3)
                            })
                    })
                    .unwrap_or(false)
            })
    }

    /// Flip Normal reverses the winding of the selected polygons, so like Split
    /// and Shell it needs polygon mode on raw polygon objects plus at least one
    /// usable selected polygon.
    pub(super) fn mesh_flip_normal_ready(&self) -> bool {
        self.mesh_topology_ready(MeshSelectMode::Polygons, 1)
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        !selected_valid_polygons(mesh, self.mesh_components_for(*id)).is_empty()
                    })
                    .unwrap_or(false)
            })
    }

    /// Disconnect needs polygon mode and a selected face that still shares a
    /// point with the rest of the mesh; a whole-mesh selection is already one
    /// island, so the command would be a no-op.
    pub(super) fn mesh_disconnect_ready(&self) -> bool {
        self.mesh_topology_ready(MeshSelectMode::Polygons, 1)
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| lc_geom::can_disconnect(mesh, self.mesh_components_for(*id)))
                    .unwrap_or(false)
            })
    }

    /// Cover fills the holes the selected faces border on every side, so it
    /// needs polygon mode and at least one closed open boundary loop the
    /// selection rings completely.
    pub(super) fn mesh_cover_ready(&self) -> bool {
        self.mesh_topology_ready(MeshSelectMode::Polygons, 1)
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        !lc_geom::coverable_holes(mesh, self.mesh_components_for(*id)).is_empty()
                    })
                    .unwrap_or(false)
            })
    }

    /// Create Polygon turns the selected boundary points into a new face, so it
    /// needs point mode and at least one open boundary loop fully selected.
    pub(super) fn mesh_create_polygon_ready(&self) -> bool {
        self.mesh_component_selection_ready()
            && self.mesh_select_mode == MeshSelectMode::Points
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        !lc_geom::faceable_loops(mesh, self.mesh_components_for(*id)).is_empty()
                    })
                    .unwrap_or(false)
            })
    }

    /// Shell needs polygon mode on raw polygon objects plus a usable distance,
    /// following the documented modes and properties (source 41).
    pub(super) fn mesh_shell_ready(&self) -> bool {
        self.mesh_shell_thickness.is_finite()
            && self.mesh_shell_thickness > 1e-6
            && self.mesh_topology_ready(MeshSelectMode::Polygons, 1)
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        !selected_valid_polygons(mesh, self.mesh_components_for(*id)).is_empty()
                    })
                    .unwrap_or(false)
            })
    }

    /// Polygons the documented Angle property groups into the shell region
    /// under the primary mesh's selection.
    pub(super) fn selected_shell_face_count(&self) -> usize {
        match self.selected_editable_mesh() {
            Some((_, mesh)) if self.mesh_select_mode == MeshSelectMode::Polygons => {
                let faces = selected_valid_polygons(&mesh, &self.mesh_selected);
                lc_geom::shell_region(&mesh, &faces, self.mesh_shell_angle).len()
            }
            _ => 0,
        }
    }

    /// Polygons the Split button would copy out of the primary mesh.
    pub(super) fn selected_split_polygon_count(&self) -> usize {
        match self.selected_editable_mesh() {
            Some((_, mesh)) if self.mesh_select_mode == MeshSelectMode::Polygons => {
                selected_valid_polygons(&mesh, &self.mesh_selected).len()
            }
            _ => 0,
        }
    }

    fn mesh_components_for(&self, id: NodeId) -> &[u32] {
        self.mesh_components
            .get(&id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn clear_mesh_component_selection(&mut self) {
        self.mesh_selected.clear();
        self.mesh_components.clear();
    }

    /// Direct UV editing manipulates the primary mesh only and needs at least
    /// one UV point to pick and drag.
    pub(super) fn uv_point_edit_ready(&self) -> bool {
        self.selected_editable_mesh()
            .map(|(_, mesh)| !mesh.uvs.is_empty())
            .unwrap_or(false)
    }

    fn set_uv_point(&mut self, id: NodeId, index: usize, uv: V2) -> bool {
        let Some((primary, _)) = self.selected_editable_mesh() else {
            self.status = "UV editing requires an editable mesh".into();
            return false;
        };
        if !self.uv_point_edit_ready() || primary != id {
            self.status = "Select the editable mesh that owns the UV point".into();
            return false;
        }
        let Some(before) = self.doc.scene.get(id).map(Shape::of) else {
            return false;
        };
        let Some(node) = self.doc.scene.get_mut(id) else {
            return false;
        };
        let NodeKind::Mesh { geometry } = &mut node.kind else {
            return false;
        };
        let GeometrySource::Editable { mesh, .. } = geometry else {
            return false;
        };
        let Some(point) = mesh.uvs.get_mut(index) else {
            self.status = "That UV point no longer exists".into();
            return false;
        };
        if *point != uv {
            *point = uv;
            if self.pending_shape.is_none() {
                self.pending_shape = Some((id, before));
            }
        }
        self.selected_uv = Some(index);
        true
    }

    /// The pin command edits the UV point selected in the UV view, on the
    /// primary editable mesh.
    pub(super) fn uv_pin_ready(&self) -> bool {
        self.selected_editable_mesh()
            .zip(self.selected_uv)
            .is_some_and(|((_, mesh), index)| index < mesh.uvs.len())
    }

    fn toggle_selected_uv_pin(&mut self) {
        if !self.uv_pin_ready() {
            self.status = if self.selected_editable_mesh().is_some() {
                "Select a UV point in the UV view first".into()
            } else {
                "Pin UVs on an editable mesh with a UV set".into()
            };
            return;
        }
        let (Some((id, mesh)), Some(index)) = (self.selected_editable_mesh(), self.selected_uv)
        else {
            return;
        };
        let Some(before) = self.doc.scene.get(id).map(Shape::of) else {
            return;
        };
        let mut updated = mesh;
        let pinned = updated.toggle_uv_pinned(index as u32);
        let NodeKind::Mesh { geometry } = &before.kind else {
            return;
        };
        let mut after = before.clone();
        after.kind = NodeKind::Mesh {
            geometry: geometry.with_mesh(updated),
        };
        self.history.execute(
            &mut self.doc,
            Box::new(SetShapesCmd {
                edits: vec![(id, before, after)],
            }),
        );
        self.status = if pinned {
            format!("Pinned UV point {index}")
        } else {
            format!("Unpinned UV point {index}")
        };
    }

    fn selected_editable_mesh(&self) -> Option<(NodeId, Mesh)> {
        let id = self.selection.primary()?;
        let node = self.doc.scene.get(id)?;
        match &node.kind {
            NodeKind::Mesh {
                geometry: GeometrySource::Editable { mesh, .. },
            } => Some((id, mesh.clone())),
            _ => None,
        }
    }

    pub(super) fn selection_has_mesh(&self) -> bool {
        self.selection.ids().iter().any(|id| {
            self.doc
                .scene
                .get(*id)
                .map(|node| node.is_mesh())
                .unwrap_or(false)
        })
    }

    /// Any selected mesh that still needs converting to an editable mesh.
    pub(super) fn selection_convert_ready(&self) -> bool {
        self.selection.ids().iter().any(|id| {
            matches!(
                self.doc.scene.get(*id).map(|node| &node.kind),
                Some(NodeKind::Mesh { geometry }) if !matches!(geometry, GeometrySource::Editable { .. })
            )
        })
    }

    fn boolean_operand(&self) -> Option<NodeId> {
        let selected: HashSet<NodeId> = self.selection.ids().iter().copied().collect();
        self.doc.scene.depth_first().into_iter().find(|id| {
            !selected.contains(id)
                && self
                    .doc
                    .scene
                    .get(*id)
                    .map(|node| node.is_mesh())
                    .unwrap_or(false)
        })
    }

    pub(super) fn boolean_ready(&self) -> bool {
        self.selection_has_mesh() && self.boolean_operand().is_some()
    }

    /// UV projection can only touch editable meshes. Require every selected
    /// mesh to be editable so a multi-selection action never silently skips a
    /// selected primitive; non-mesh objects are ignored.
    pub(super) fn selection_uv_ready(&self) -> bool {
        let mut has_mesh = false;
        for id in self.selection.ids() {
            let Some(node) = self.doc.scene.get(*id) else {
                continue;
            };
            if !node.is_mesh() {
                continue;
            }
            has_mesh = true;
            if !matches!(
                node.kind,
                NodeKind::Mesh {
                    geometry: GeometrySource::Editable { .. }
                }
            ) {
                return false;
            }
        }
        has_mesh
    }

    fn make_selected_editable(&mut self) {
        if !self.selection_convert_ready() {
            self.status = if self.selection_has_mesh() {
                "Selected meshes are already editable".into()
            } else {
                "Select a mesh object first".into()
            };
            return;
        }
        let mut edits = Vec::new();
        for id in self.selection.ids() {
            let Some(node) = self.doc.scene.get(*id) else {
                continue;
            };
            let before = Shape::of(node);
            let mut after = before.clone();
            if let NodeKind::Mesh { geometry } = &mut after.kind {
                geometry.make_editable();
            }
            if before.kind != after.kind {
                edits.push((*id, before, after));
            }
        }
        let count = edits.len();
        self.history
            .execute(&mut self.doc, Box::new(SetShapesCmd { edits }));
        self.status = if count == 1 {
            "Converted primitive to editable mesh".into()
        } else {
            format!("Converted {count} primitives to editable meshes")
        };
    }

    fn apply_uv_op(&mut self, op: UvOp) {
        if !self.selection_uv_ready() {
            self.status = if self.selection_has_mesh() {
                "Convert all selected meshes to editable first".into()
            } else {
                "Select an editable mesh first".into()
            };
            return;
        }
        let mut edits = Vec::new();
        for id in self.selection.ids() {
            let Some(node) = self.doc.scene.get(*id) else {
                continue;
            };
            let before = Shape::of(node);
            let NodeKind::Mesh { geometry } = &before.kind else {
                continue;
            };
            let GeometrySource::Editable { mesh, .. } = geometry else {
                continue;
            };
            let mut mesh = mesh.clone();
            match op {
                UvOp::Planar => lc_geom::planar_projection(&mut mesh, 1, 1.0),
                UvOp::Box => lc_geom::box_projection(&mut mesh, 1.0),
                UvOp::Cylindrical => lc_geom::cylindrical_projection(&mut mesh, 1.0, 1.0),
                UvOp::Spherical => lc_geom::spherical_projection(&mut mesh, 1.0),
                UvOp::Unwrap => lc_geom::automatic_unwrap(&mut mesh, 45.0, 0.01),
                UvOp::Relax => lc_geom::relax_uvs(&mut mesh, 8, 0.5, true),
                UvOp::Normalize => lc_geom::normalize_uvs(&mut mesh),
            }
            let mut after = before.clone();
            after.kind = NodeKind::Mesh {
                geometry: geometry.with_mesh(mesh),
            };
            edits.push((*id, before, after));
        }
        if edits.is_empty() {
            self.status = "Select an editable mesh first".into();
            return;
        }
        let count = edits.len();
        self.history
            .execute(&mut self.doc, Box::new(SetShapesCmd { edits }));
        self.status = if count == 1 {
            "Updated UVs".into()
        } else {
            format!("Updated UVs on {count} objects")
        };
    }

    fn perform_mesh_action(&mut self, action: MeshAction) {
        match action {
            MeshAction::Enter => {
                if !self.mesh_mode_ready() {
                    self.status = if self.selection.is_empty() {
                        "Select a mesh object to edit components".into()
                    } else {
                        "Component editing needs a mesh-only selection".into()
                    };
                    return;
                }
                if !self.selected_meshes_editable() {
                    self.make_selected_editable();
                }
                if self.selected_meshes_editable() {
                    self.mesh_edit = true;
                    self.clear_mesh_component_selection();
                    self.status = "Mesh edit mode — click components in the viewport".into();
                } else if !self.selection.is_empty() {
                    self.status = "Select a mesh object first".into();
                }
            }
            MeshAction::Exit => {
                self.mesh_edit = false;
                self.clear_mesh_component_selection();
                self.status = "Object mode".into();
            }
            MeshAction::SelectAll => {
                if !self.mesh_component_selection_ready() {
                    self.status = "Enter component mode with editable meshes first".into();
                    return;
                }
                let ids = self.selection.ids().to_vec();
                let mut primary = None;
                for id in ids {
                    let Some(node) = self.doc.scene.get(id) else {
                        continue;
                    };
                    let NodeKind::Mesh {
                        geometry: GeometrySource::Editable { mesh, .. },
                    } = &node.kind
                    else {
                        continue;
                    };
                    let components: Vec<u32> = match self.mesh_select_mode {
                        MeshSelectMode::Points => (0..mesh.positions.len() as u32).collect(),
                        MeshSelectMode::Edges => (0..mesh.edges().len() as u32).collect(),
                        MeshSelectMode::Polygons => (0..mesh.polygons.len() as u32).collect(),
                    };
                    if self.selection.primary() == Some(id) {
                        primary = Some(components.clone());
                    }
                    self.mesh_components.insert(id, components);
                }
                self.mesh_selected = primary.unwrap_or_default();
            }
            MeshAction::SelectNone => {
                if !self.mesh_component_selection_ready() {
                    self.status = "Enter component mode with editable meshes first".into();
                    return;
                }
                self.clear_mesh_component_selection();
            }
            MeshAction::Transform => self.mesh_transform_selection(),
            MeshAction::Extrude => {
                if !self.require_mesh_selection(MeshSelectMode::Polygons, 1, "extrude") {
                    return;
                }
                let distance = self.mesh_value;
                let individual = self.mesh_extrude_individual;
                self.apply_mesh_operation("Extruded faces", move |_, mesh, faces| {
                    Ok(lc_geom::extrude_faces(mesh, faces, distance, individual))
                });
            }
            MeshAction::Inset => {
                if !self.require_mesh_selection(MeshSelectMode::Polygons, 1, "inset") {
                    return;
                }
                let amount = self.mesh_value.abs().max(1e-4);
                self.apply_mesh_operation("Inset faces", move |_, mesh, faces| {
                    Ok(lc_geom::bevel_faces(mesh, faces, amount, 0.0))
                });
            }
            MeshAction::Bevel => {
                if !self.require_mesh_selection(MeshSelectMode::Polygons, 1, "bevel") {
                    return;
                }
                let amount = self.mesh_value.abs().max(1e-4);
                let depth = self.mesh_depth;
                self.apply_mesh_operation("Bevelled faces", move |_, mesh, faces| {
                    Ok(lc_geom::bevel_faces(mesh, faces, amount, depth))
                });
            }
            MeshAction::RingCut => {
                if !self.mesh_ring_cut_ready() {
                    self.status = if self.mesh_component_selection_ready() {
                        "Ring cut needs the cut plane to cross every selected mesh".into()
                    } else {
                        "Enter component mode with editable meshes first".into()
                    };
                    return;
                }
                let axis = self.mesh_axis;
                let offset = self.mesh_offset;
                self.apply_mesh_operation("Ring cut", move |_, mesh, _| {
                    Ok(lc_geom::ring_cut(mesh, axis, offset))
                });
            }
            MeshAction::Weld => {
                if !self.mesh_weld_ready() {
                    self.status = if !self.mesh_topology_ready(MeshSelectMode::Points, 2) {
                        self.mesh_tool_refusal(MeshSelectMode::Points, 2, "weld")
                    } else if self.selection.len() == 1 {
                        "Select at least two distinct points to weld".into()
                    } else {
                        "Select at least two distinct points on every selected mesh to weld".into()
                    };
                    return;
                }
                self.apply_mesh_operation("Welded points", |_, mesh, points| {
                    Ok(weld_selected_points(mesh, points))
                });
            }
            MeshAction::Collapse => {
                if !self.mesh_collapse_ready() {
                    self.status = if !self.mesh_topology_ready(MeshSelectMode::Points, 2) {
                        self.mesh_tool_refusal(MeshSelectMode::Points, 2, "collapse")
                    } else {
                        "Select at least two distinct points to collapse".into()
                    };
                    return;
                }
                // The merged point can land on a new index after compaction, so
                // the collapse reports its survivor and the selection follows it.
                let survivors = std::cell::RefCell::new(Vec::new());
                let depth = self.history.depth();
                self.apply_mesh_operation_selection(
                    "Collapsed points",
                    true,
                    |id, mesh, points| {
                        let (out, survivor) = lc_geom::collapse_points(mesh, points);
                        if let Some(survivor) = survivor {
                            survivors.borrow_mut().push((id, survivor));
                        }
                        Ok(out)
                    },
                );
                if self.history.depth() == depth {
                    return;
                }
                let survivors = survivors.into_inner();
                for (id, survivor) in &survivors {
                    self.mesh_components.insert(*id, vec![*survivor]);
                }
                self.mesh_selected = self
                    .selection
                    .primary()
                    .and_then(|primary| {
                        survivors
                            .iter()
                            .find(|(id, _)| *id == primary)
                            .map(|(_, survivor)| vec![*survivor])
                    })
                    .unwrap_or_default();
            }
            MeshAction::Optimize => {
                if !self.mesh_optimize_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Points {
                        "Optimize is only available in point mode".into()
                    } else if !self.mesh_optimize_tolerance.is_finite()
                        || self.mesh_optimize_tolerance <= 0.0
                    {
                        "Set a positive optimize tolerance first".into()
                    } else if self
                        .selection
                        .ids()
                        .iter()
                        .any(|id| self.mesh_components_for(*id).len() < 2)
                    {
                        "Select at least two points to optimize".into()
                    } else {
                        "No selected points sit within the optimize tolerance".into()
                    };
                    return;
                }
                let tolerance = self.mesh_optimize_tolerance;
                let survivors = std::cell::RefCell::new(Vec::new());
                let depth = self.history.depth();
                self.apply_mesh_operation_selection(
                    "Optimized points",
                    true,
                    |id, mesh, components| {
                        let points = selected_valid_points(mesh, components);
                        if points.len() < 2 {
                            return Err("select at least two points to optimize".into());
                        }
                        let Some((out, kept)) = lc_geom::optimize_points(mesh, &points, tolerance)
                        else {
                            return Err(
                                "no selected points sit within the optimize tolerance".into()
                            );
                        };
                        survivors.borrow_mut().push((id, kept));
                        Ok(out)
                    },
                );
                if self.history.depth() == depth {
                    return;
                }
                let survivors = survivors.into_inner();
                for (id, kept) in &survivors {
                    self.mesh_components.insert(*id, kept.clone());
                }
                self.mesh_selected = self
                    .selection
                    .primary()
                    .and_then(|primary| {
                        survivors
                            .iter()
                            .find(|(id, _)| *id == primary)
                            .map(|(_, kept)| kept.clone())
                    })
                    .unwrap_or_default();
            }
            MeshAction::PointSlide => {
                if !self.mesh_point_slide_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Points {
                        "Point Slide is only available in point mode".into()
                    } else if !self.mesh_slide_fraction.is_finite()
                        || self.mesh_slide_fraction < 0.0
                        || self.mesh_slide_fraction > 1.0
                    {
                        "Set a slide fraction between 0 and 1 first".into()
                    } else if self.mesh_slide_fraction == 0.0 {
                        "Set a slide fraction above 0 to slide points".into()
                    } else if self
                        .selection
                        .ids()
                        .iter()
                        .all(|id| self.mesh_components_for(*id).is_empty())
                    {
                        "Select at least one point to slide".into()
                    } else {
                        "No selected point has neighbours to slide toward".into()
                    };
                    return;
                }
                let fraction = self.mesh_slide_fraction;
                self.apply_mesh_operation_selection("Slid points", false, |_, mesh, components| {
                    let points = selected_valid_points(mesh, components);
                    if points.is_empty() {
                        return Err("select at least one point to slide".into());
                    }
                    lc_geom::slide_points(mesh, &points, fraction)
                        .ok_or_else(|| "no selected point has neighbours to slide toward".into())
                });
            }
            MeshAction::Bridge => {
                if let Some(status) = self.mesh_bridge_failure() {
                    self.status = status;
                    return;
                }
                self.apply_mesh_operation("Bridged edge loops", |_, mesh, edges| {
                    let (loop_a, loop_b) = selected_bridge_loops(mesh, edges)?;
                    lc_geom::bridge_loops(mesh, &loop_a, &loop_b)
                });
            }
            MeshAction::FillHole => {
                if !self.mesh_fill_hole_ready() {
                    self.status = if self.mesh_component_selection_ready() {
                        "Select a point on a hole border first".into()
                    } else {
                        "Enter component mode with editable meshes first".into()
                    };
                    return;
                }
                let mode = self.mesh_select_mode;
                self.apply_mesh_operation("Filled hole", move |_, mesh, components| {
                    let points = mesh_affected_points(mesh, mode, components);
                    let holes = lc_geom::open_holes(mesh, &points);
                    if holes.is_empty() {
                        return Err("select a point on a hole border".into());
                    }
                    lc_geom::fill_holes(mesh, &holes)
                });
            }
            MeshAction::Split => self.split_selected_polygons(),
            MeshAction::Shell => {
                if !self.mesh_shell_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Polygons {
                        "Shell is only available in polygon mode".into()
                    } else if self.mesh_shell_thickness <= 1e-6 {
                        "Set a positive shell thickness first; Inside picks the side".into()
                    } else {
                        "Select at least one polygon to shell".into()
                    };
                    return;
                }
                let thickness = self.mesh_shell_thickness;
                let inside = self.mesh_shell_inside;
                let angle = self.mesh_shell_angle;
                let quantize = self.mesh_shell_quantize;
                self.apply_mesh_operation("Shelled polygons", move |_, mesh, components| {
                    let faces = selected_valid_polygons(mesh, components);
                    lc_geom::shell_faces(mesh, &faces, thickness, inside, angle, quantize)
                });
            }
            MeshAction::ToggleCrease => {
                if !self.require_mesh_selection(MeshSelectMode::Edges, 1, "toggle creases") {
                    return;
                }
                self.apply_mesh_operation_selection(
                    "Toggled edge creases",
                    false,
                    |_, mesh, selected| {
                        let edges = mesh.edges();
                        let mut updated = mesh.clone();
                        for &index in selected {
                            if let Some(edge) = edges.get(index as usize) {
                                updated.toggle_crease(edge.a, edge.b);
                            }
                        }
                        Ok(updated)
                    },
                );
            }
            MeshAction::ToggleSeam => {
                if !self.require_mesh_selection(MeshSelectMode::Edges, 1, "toggle UV seams") {
                    return;
                }
                self.apply_mesh_operation_selection(
                    "Toggled UV seams",
                    false,
                    |_, mesh, selected| {
                        let edges = mesh.edges();
                        let mut updated = mesh.clone();
                        for &index in selected {
                            if let Some(edge) = edges.get(index as usize) {
                                updated.toggle_seam(edge.a, edge.b);
                            }
                        }
                        Ok(updated)
                    },
                );
            }
            MeshAction::Triangulate => {
                if !self.mesh_triangulate_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Polygons {
                        "Triangulate is only available in polygon mode".into()
                    } else {
                        "Select at least one polygon with more than three corners to triangulate"
                            .into()
                    };
                    return;
                }
                self.apply_mesh_operation("Triangulated polygons", |_, mesh, components| {
                    let faces = selected_valid_polygons(mesh, components);
                    if faces.is_empty() {
                        return Err(
                            "select at least one polygon with more than three corners to triangulate"
                                .into(),
                        );
                    }
                    Ok(lc_geom::triangulate_faces(mesh, &faces))
                });
            }
            MeshAction::FlipNormal => {
                if !self.mesh_flip_normal_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Polygons {
                        "Flip Normal is only available in polygon mode".into()
                    } else {
                        "Select at least one polygon to flip".into()
                    };
                    return;
                }
                // Face indices do not change, so the selection stays and the
                // same command flips the faces back.
                self.apply_mesh_operation_selection(
                    "Flipped polygon normals",
                    false,
                    |_, mesh, components| {
                        let faces = selected_valid_polygons(mesh, components);
                        if faces.is_empty() {
                            return Err("select at least one polygon to flip".into());
                        }
                        Ok(lc_geom::flip_faces(mesh, &faces))
                    },
                );
            }
            MeshAction::Disconnect => {
                if !self.mesh_disconnect_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Polygons {
                        "Disconnect is only available in polygon mode".into()
                    } else {
                        "Select polygons that border the rest of the mesh to disconnect".into()
                    };
                    return;
                }
                // Face indices do not change, so the selection stays exactly as
                // it was and now names the detached island.
                self.apply_mesh_operation_selection(
                    "Disconnected polygons",
                    false,
                    |_, mesh, components| {
                        let faces = selected_valid_polygons(mesh, components);
                        if faces.is_empty() {
                            return Err("select at least one polygon to disconnect".into());
                        }
                        Ok(lc_geom::disconnect_faces(mesh, &faces))
                    },
                );
            }
            MeshAction::Cover => {
                if !self.mesh_cover_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Polygons {
                        "Cover is only available in polygon mode".into()
                    } else {
                        "Select the polygons that border an open hole on every side to cover it"
                            .into()
                    };
                    return;
                }
                // The fill appends faces, so the handler selects them once the
                // command has actually been pushed.
                let created = std::cell::RefCell::new(Vec::new());
                let depth = self.history.depth();
                self.apply_mesh_operation_selection(
                    "Covered holes",
                    true,
                    |id, mesh, components| {
                        let faces = selected_valid_polygons(mesh, components);
                        if faces.is_empty() {
                            return Err("select at least one polygon to cover".into());
                        }
                        let (out, new_faces) = lc_geom::cover_faces(mesh, &faces)?;
                        created.borrow_mut().push((id, new_faces));
                        Ok(out)
                    },
                );
                if self.history.depth() == depth {
                    return;
                }
                let created = created.into_inner();
                for (id, faces) in &created {
                    if !faces.is_empty() {
                        self.mesh_components.insert(*id, faces.clone());
                    }
                }
                self.mesh_selected = self
                    .selection
                    .primary()
                    .and_then(|primary| {
                        created
                            .iter()
                            .find(|(id, _)| *id == primary)
                            .map(|(_, faces)| faces.clone())
                    })
                    .unwrap_or_default();
            }
            MeshAction::CreatePolygon => {
                if !self.mesh_create_polygon_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Points {
                        "Create Polygon is only available in point mode".into()
                    } else {
                        "Select the boundary points that close an open region to create a polygon"
                            .into()
                    };
                    return;
                }
                // New faces are appended, so the handler selects them once the
                // command has actually been pushed.
                let created = std::cell::RefCell::new(Vec::new());
                let depth = self.history.depth();
                self.apply_mesh_operation_selection(
                    "Created polygons",
                    true,
                    |id, mesh, components| {
                        let points = selected_valid_points(mesh, components);
                        if points.len() < 3 {
                            return Err(
                                "select at least three boundary points that close an open region"
                                    .into(),
                            );
                        }
                        let (out, new_faces) = lc_geom::create_polygon_faces(mesh, &points)?;
                        created.borrow_mut().push((id, new_faces));
                        Ok(out)
                    },
                );
                if self.history.depth() == depth {
                    return;
                }
                let created = created.into_inner();
                for (id, faces) in &created {
                    if !faces.is_empty() {
                        self.mesh_components.insert(*id, faces.clone());
                    }
                }
                self.mesh_selected = self
                    .selection
                    .primary()
                    .and_then(|primary| {
                        created
                            .iter()
                            .find(|(id, _)| *id == primary)
                            .map(|(_, faces)| faces.clone())
                    })
                    .unwrap_or_default();
            }
            MeshAction::TogglePinnedUv => self.toggle_selected_uv_pin(),
            MeshAction::AssignMaterial => self.assign_material_to_selected_faces(),
        }
    }

    fn require_mesh_selection(
        &mut self,
        mode: MeshSelectMode,
        minimum: usize,
        action: &str,
    ) -> bool {
        if self.mesh_topology_ready(mode, minimum) {
            return true;
        }
        self.status = self.mesh_tool_refusal(mode, minimum, action);
        false
    }

    /// Explains why the shared component-tool rule rejected the selection.
    fn mesh_tool_refusal(&self, mode: MeshSelectMode, minimum: usize, action: &str) -> String {
        if !self.selected_meshes_editable() {
            return if self.selection.is_empty() {
                "Select an editable mesh first".into()
            } else {
                "Select only editable mesh objects first".into()
            };
        }
        if !self.mesh_edit {
            return "Enter component mode first".into();
        }
        if self.mesh_select_mode != mode {
            return format!("Switch to {} selection to {action}", mode.label());
        }
        let label = mode.label().to_lowercase();
        if minimum <= 1 {
            if self.selection.len() == 1 {
                format!("Select {label} first")
            } else {
                format!("Select {label} on every selected mesh first")
            }
        } else if self.selection.len() == 1 {
            format!("Select at least {minimum} {label} to {action}")
        } else {
            format!("Select at least {minimum} {label} on every selected mesh to {action}")
        }
    }

    fn apply_mesh_operation(
        &mut self,
        message: &str,
        operation: impl Fn(NodeId, &Mesh, &[u32]) -> Result<Mesh, String>,
    ) {
        self.apply_mesh_operation_selection(message, true, operation);
    }

    fn apply_mesh_operation_selection(
        &mut self,
        message: &str,
        clear_selection: bool,
        operation: impl Fn(NodeId, &Mesh, &[u32]) -> Result<Mesh, String>,
    ) {
        if !self.mesh_edit {
            self.status = "Enter component mode first".into();
            return;
        }
        if !self.selected_meshes_editable() {
            self.status = if self.selection.is_empty() {
                "Select an editable mesh first".into()
            } else {
                "Select only editable mesh objects first".into()
            };
            return;
        }
        let mut edits = Vec::new();
        for id in self.selection.ids().to_vec() {
            let Some(node) = self.doc.scene.get(id) else {
                return;
            };
            let before = Shape::of(node);
            let NodeKind::Mesh {
                geometry: GeometrySource::Editable { mesh, .. },
            } = &node.kind
            else {
                return;
            };
            let mesh = mesh.clone();
            let components = self.mesh_components_for(id).to_vec();
            let updated = match operation(id, &mesh, &components) {
                Ok(updated) if updated == mesh => {
                    self.status = format!("{message}: no change");
                    return;
                }
                Ok(updated) => updated,
                Err(error) => {
                    self.status = format!("{message} failed: {error}");
                    return;
                }
            };
            if let Err(errors) = updated.validate() {
                self.status = format!("Mesh edit rejected: {}", errors.join("; "));
                return;
            }
            let mut after = before.clone();
            after.kind = match after.kind {
                NodeKind::Mesh { geometry } => NodeKind::Mesh {
                    geometry: geometry.with_mesh(updated),
                },
                other => other,
            };
            edits.push((id, before, after));
        }
        if edits.is_empty() {
            self.status = "Select an editable mesh first".into();
            return;
        }
        self.history
            .execute(&mut self.doc, Box::new(SetShapesCmd { edits }));
        if clear_selection {
            self.clear_mesh_component_selection();
        }
        self.status = message.to_string();
    }

    /// Copy the selected polygons into a new object and leave the source mesh
    /// untouched, exactly the documented Split behaviour (sources 39 and 40).
    fn split_selected_polygons(&mut self) {
        if !self.mesh_split_ready() {
            self.status = if !self.mesh_component_selection_ready() {
                "Enter component mode with editable meshes first".into()
            } else if self.mesh_select_mode != MeshSelectMode::Polygons {
                "Split is only available in polygon mode".into()
            } else {
                "Select at least one polygon to split".into()
            };
            return;
        }
        let mut nodes: Vec<(Node, Option<NodeId>)> = Vec::new();
        let mut polygons = 0usize;
        for id in self.selection.ids().to_vec() {
            let Some(source) = self.doc.scene.get(id).cloned() else {
                continue;
            };
            let NodeKind::Mesh {
                geometry: GeometrySource::Editable { mesh, .. },
            } = &source.kind
            else {
                continue;
            };
            let faces = selected_valid_polygons(mesh, self.mesh_components_for(id));
            let part = match lc_geom::extract_faces(mesh, &faces) {
                Ok(part) => part,
                Err(error) => {
                    self.status = format!("Split failed: {error}");
                    return;
                }
            };
            polygons += part.polygons.len();
            // Source 39 copies "all tags and selections" to the new object. The
            // modifier stack, skin, morph targets and object animation stay
            // behind: the manual is silent and the new object holds raw geometry.
            let mut node = self.doc.create_node(
                NodeKind::Mesh {
                    geometry: GeometrySource::editable(part),
                },
                &format!("{} Split", source.name),
            );
            node.transform = source.transform;
            node.visible = source.visible;
            node.renderable = source.renderable;
            node.locked = source.locked;
            node.layer = source.layer;
            node.material = source.material;
            node.material_slots = source.material_slots.clone();
            node.tags = source.tags.clone();
            node.tag_animations = source.tag_animations.clone();
            nodes.push((node, source.parent));
        }
        if nodes.is_empty() {
            self.status = "Select an editable mesh first".into();
            return;
        }
        let ids: Vec<NodeId> = nodes.iter().map(|(node, _)| node.id).collect();
        let name = nodes[0].0.name.clone();
        let objects = nodes.len();
        self.history.execute(
            &mut self.doc,
            Box::new(lc_scene::AddNodesCmd {
                label: if objects == 1 {
                    "Split".into()
                } else {
                    format!("Split {objects} Objects")
                },
                nodes,
            }),
        );
        self.selection.set(ids);
        self.clear_mesh_component_selection();
        self.status = if objects == 1 {
            format!(
                "Split {polygons} polygon{} into \"{name}\"",
                if polygons == 1 { "" } else { "s" }
            )
        } else {
            format!("Split {polygons} polygons into {objects} new objects")
        };
    }

    fn mesh_transform_selection(&mut self) {
        if !self.mesh_transform_ready() {
            self.status = if self.mesh_component_selection_ready() {
                "Select components to transform on every selected mesh".into()
            } else {
                "Enter component mode with editable meshes first".into()
            };
            return;
        }
        let mode = self.mesh_select_mode;
        let space = self.mesh_space;
        let axis = self.mesh_axis;
        let transform = self.mesh_transform;
        let value = self.mesh_value;
        let rotations: HashMap<NodeId, Quat> = self
            .selection
            .ids()
            .iter()
            .map(|id| {
                (
                    *id,
                    self.doc
                        .scene
                        .world_matrix_plain(*id)
                        .to_scale_rotation_translation()
                        .1,
                )
            })
            .collect();
        self.apply_mesh_operation_selection(
            &format!("Transformed selected {}", mode.label().to_lowercase()),
            false,
            move |id, mesh, components| {
                let affected: HashSet<u32> = mesh_affected_points(mesh, mode, components)
                    .into_iter()
                    .collect();
                if affected.is_empty() {
                    return Err("select mesh components to transform".into());
                }
                let center = affected
                    .iter()
                    .filter_map(|p| mesh.positions.get(*p as usize).copied())
                    .sum::<Vec3>()
                    / affected.len() as f32;
                let local_axis = mesh_transform_axis(
                    mesh,
                    &affected,
                    space,
                    axis,
                    rotations.get(&id).copied().unwrap_or(Quat::IDENTITY),
                );
                let mut out = mesh.clone();
                match transform {
                    MeshTransformMode::Move => {
                        for &p in &affected {
                            if let Some(position) = out.positions.get_mut(p as usize) {
                                *position += local_axis * value;
                            }
                        }
                    }
                    MeshTransformMode::Rotate => {
                        let rotation = Quat::from_axis_angle(local_axis, value.to_radians());
                        for &p in &affected {
                            if let Some(position) = out.positions.get_mut(p as usize) {
                                *position = center + rotation * (*position - center);
                            }
                        }
                    }
                    MeshTransformMode::Scale => {
                        for &p in &affected {
                            if let Some(position) = out.positions.get_mut(p as usize) {
                                let offset = *position - center;
                                *position =
                                    center + offset + local_axis * offset.dot(local_axis) * value;
                            }
                        }
                    }
                }
                Ok(out)
            },
        );
    }

    fn toggle_mesh_selection(&mut self, index: u32, additive: bool) {
        if additive {
            if let Some(position) = self.mesh_selected.iter().position(|i| *i == index) {
                self.mesh_selected.remove(position);
            } else {
                self.mesh_selected.push(index);
            }
        } else {
            self.mesh_selected = vec![index];
        }
        if let Some(id) = self.selection.primary() {
            self.mesh_components.insert(id, self.mesh_selected.clone());
        }
    }

    fn assign_material_to_selected_faces(&mut self) {
        if !self.mesh_assign_material_ready() {
            self.status = if !self.mesh_component_selection_ready() {
                "Enter component mode with editable meshes first".into()
            } else if self.mesh_select_mode != MeshSelectMode::Polygons {
                "Switch to Polygon selection to assign a material".into()
            } else if self.selected_material_id().is_none() {
                "Select a material in the Materials tab first".into()
            } else if self.selection.len() == 1 {
                "Select polygons before assigning a material".into()
            } else {
                "Select polygons on every selected mesh before assigning a material".into()
            };
            return;
        }
        let Some(material_id) = self.selected_material_id() else {
            return;
        };
        let before = self.doc.clone();
        for id in self.selection.ids().to_vec() {
            let components = self.mesh_components_for(id).to_vec();
            let Some(node) = self.doc.scene.get_mut(id) else {
                self.doc = before;
                self.status = "Assign material failed: a mesh left the scene".into();
                return;
            };
            let NodeKind::Mesh {
                geometry: GeometrySource::Editable { mesh, .. },
            } = &mut node.kind
            else {
                self.doc = before;
                self.status = "Assign material failed: not an editable mesh".into();
                return;
            };
            let slot = node
                .material_slots
                .iter()
                .find(|(_, material)| **material == material_id)
                .map(|(slot, _)| *slot)
                .unwrap_or_else(|| {
                    node.material_slots
                        .keys()
                        .next_back()
                        .map(|slot| slot.saturating_add(1))
                        .unwrap_or(0)
                });
            node.material_slots.insert(slot, material_id);
            for index in components {
                if let Some(poly) = mesh.polygons.get_mut(index as usize) {
                    poly.material_slot = slot;
                }
            }
        }
        let after = self.doc.clone();
        let count = self.selection.len();
        self.history.execute(
            &mut self.doc,
            Box::new(ReplaceDocumentCmd {
                label: if count == 1 {
                    "Assign polygon material".into()
                } else {
                    format!("Assign material on {count} Objects")
                },
                before,
                after,
            }),
        );
        self.status = if count == 1 {
            format!("Assigned material {material_id} to selected polygons")
        } else {
            format!("Assigned material {material_id} on {count} objects")
        };
    }

    fn pick_mesh_component(
        &self,
        mesh: &Mesh,
        world: Mat4,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
        pointer: Pos2,
    ) -> Option<u32> {
        match self.mesh_select_mode {
            MeshSelectMode::Points => {
                let mut best: Option<(f32, u32)> = None;
                for (i, p) in mesh.positions.iter().enumerate() {
                    let Some(screen) =
                        self.project(world.transform_point3(*p), rect, pixel_w, pixel_h)
                    else {
                        continue;
                    };
                    let distance = (screen - pointer).length();
                    if distance < 10.0 && best.map(|(d, _)| distance < d).unwrap_or(true) {
                        best = Some((distance, i as u32));
                    }
                }
                best.map(|(_, i)| i)
            }
            MeshSelectMode::Edges => {
                let mut best: Option<(f32, u32)> = None;
                for (i, edge) in mesh.edges().iter().enumerate() {
                    let Some(a) = mesh.positions.get(edge.a as usize) else {
                        continue;
                    };
                    let Some(b) = mesh.positions.get(edge.b as usize) else {
                        continue;
                    };
                    let Some(a) = self.project(world.transform_point3(*a), rect, pixel_w, pixel_h)
                    else {
                        continue;
                    };
                    let Some(b) = self.project(world.transform_point3(*b), rect, pixel_w, pixel_h)
                    else {
                        continue;
                    };
                    let distance = point_segment_distance(pointer, a, b);
                    if distance < 8.0 && best.map(|(d, _)| distance < d).unwrap_or(true) {
                        best = Some((distance, i as u32));
                    }
                }
                best.map(|(_, i)| i)
            }
            MeshSelectMode::Polygons => {
                let mut best: Option<(f32, u32)> = None;
                for (i, poly) in mesh.polygons.iter().enumerate() {
                    let points: Vec<Pos2> = poly
                        .corners
                        .iter()
                        .filter_map(|c| mesh.positions.get(c.point as usize).copied())
                        .filter_map(|p| {
                            self.project(world.transform_point3(p), rect, pixel_w, pixel_h)
                        })
                        .collect();
                    if points.len() < 3 || !point_in_polygon(pointer, &points) {
                        continue;
                    }
                    let center = points.iter().fold(Vec2::ZERO, |acc, p| acc + p.to_vec2())
                        / points.len() as f32;
                    let distance = (center - pointer.to_vec2()).length();
                    if best.map(|(d, _)| distance < d).unwrap_or(true) {
                        best = Some((distance, i as u32));
                    }
                }
                best.map(|(_, i)| i)
            }
        }
    }

    fn draw_mesh_edit_overlay(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        evaluated: &EvaluatedScene,
        pixel_w: u32,
        pixel_h: u32,
    ) {
        let Some((id, mesh)) = self.selected_editable_mesh() else {
            return;
        };
        let Some(node) = evaluated.node(id) else {
            return;
        };
        let painter = ui.painter_at(rect);
        let selected: HashSet<u32> = self.mesh_selected.iter().copied().collect();
        let selected_color = Color32::from_rgb(255, 164, 64);
        let points = |points: &mut Vec<Pos2>, indices: &[u32]| {
            points.clear();
            for index in indices {
                if let Some(p) = mesh.positions.get(*index as usize) {
                    if let Some(screen) =
                        self.project(node.world.transform_point3(*p), rect, pixel_w, pixel_h)
                    {
                        points.push(screen);
                    }
                }
            }
        };
        match self.mesh_select_mode {
            MeshSelectMode::Points => {
                for (i, p) in mesh.positions.iter().enumerate() {
                    if let Some(screen) =
                        self.project(node.world.transform_point3(*p), rect, pixel_w, pixel_h)
                    {
                        let chosen = selected.contains(&(i as u32));
                        painter.circle_filled(
                            screen,
                            if chosen { 4.2 } else { 2.3 },
                            if chosen {
                                selected_color
                            } else {
                                Color32::from_rgb(130, 205, 250)
                            },
                        );
                    }
                }
            }
            MeshSelectMode::Edges => {
                for (i, edge) in mesh.edges().iter().enumerate() {
                    let Some(a) = mesh.positions.get(edge.a as usize) else {
                        continue;
                    };
                    let Some(b) = mesh.positions.get(edge.b as usize) else {
                        continue;
                    };
                    let (Some(a), Some(b)) = (
                        self.project(node.world.transform_point3(*a), rect, pixel_w, pixel_h),
                        self.project(node.world.transform_point3(*b), rect, pixel_w, pixel_h),
                    ) else {
                        continue;
                    };
                    let chosen = selected.contains(&(i as u32));
                    let seam = mesh.is_seam(edge.a, edge.b);
                    let creased = mesh.is_crease(edge.a, edge.b);
                    painter.line_segment(
                        [a, b],
                        egui::Stroke::new(
                            if chosen {
                                3.0
                            } else if seam {
                                2.5
                            } else if creased {
                                2.0
                            } else {
                                1.0
                            },
                            if chosen {
                                selected_color
                            } else if seam {
                                // Seam edges are green, as in Cheetah3D's UV tutorial.
                                Color32::from_rgb(90, 200, 110)
                            } else if creased {
                                Color32::from_rgb(90, 165, 255)
                            } else {
                                Color32::from_rgb(120, 135, 155)
                            },
                        ),
                    );
                }
            }
            MeshSelectMode::Polygons => {
                for (i, poly) in mesh.polygons.iter().enumerate() {
                    let poly_points: Vec<Pos2> = poly
                        .corners
                        .iter()
                        .filter_map(|c| mesh.positions.get(c.point as usize).copied())
                        .filter_map(|p| {
                            self.project(node.world.transform_point3(p), rect, pixel_w, pixel_h)
                        })
                        .collect();
                    if poly_points.len() < 3 {
                        continue;
                    }
                    let chosen = selected.contains(&(i as u32));
                    if chosen {
                        painter.add(egui::Shape::convex_polygon(
                            poly_points.clone(),
                            Color32::from_rgba_unmultiplied(255, 150, 50, 55),
                            egui::Stroke::new(2.0, selected_color),
                        ));
                    } else {
                        for k in 0..poly_points.len() {
                            painter.line_segment(
                                [poly_points[k], poly_points[(k + 1) % poly_points.len()]],
                                egui::Stroke::new(0.8, Color32::from_rgb(90, 105, 125)),
                            );
                        }
                    }
                }
            }
        }
        let mut handles = Vec::new();
        points(
            &mut handles,
            &mesh_affected_points(&mesh, self.mesh_select_mode, &self.mesh_selected),
        );
        for p in handles {
            painter.circle_stroke(p, 3.5, egui::Stroke::new(1.0, selected_color));
        }
    }

    /// Keys every selected object whose transform is not already keyed at the
    /// current frame.
    fn key_transform_edits(&self) -> Vec<(NodeId, AnimationClip, AnimationClip)> {
        let time = self.playback.time;
        let mut edits = Vec::new();
        for id in self.selection.ids() {
            let Some(node) = self.doc.scene.get(*id) else {
                continue;
            };
            let before = node.animation.clone();
            let mut after = before.clone();
            after.record_transform(&node.transform, time, Interpolation::Smooth);
            if after != before {
                edits.push((*id, before, after));
            }
        }
        edits
    }

    /// Whether a target still names a live entity that owns animatable data.
    fn target_is_valid(&self, target: AnimTarget) -> bool {
        match target {
            AnimTarget::Object(id) => self.doc.scene.get(id).is_some(),
            AnimTarget::Tag(node, index) => self
                .doc
                .scene
                .get(node)
                .and_then(|n| n.tags.get(index))
                .is_some_and(|tag| Self::tag_animation_channel(tag).is_some()),
            AnimTarget::Material(id) => self.doc.materials.get(id).is_some(),
        }
    }

    /// Map a scene tag to the registry channel that drives it.
    pub(super) fn tag_animation_channel(tag: &Tag) -> Option<AnimChannel> {
        match tag {
            Tag::CopyLocation { .. } => Some(AnimChannel::CopyLocationWeight),
            _ => None,
        }
    }

    /// The explicit F-curve target while it still follows the selection,
    /// otherwise the primary object. Tag targets follow their owning object;
    /// material targets are independent because materials have their own
    /// selection.
    pub(super) fn current_anim_target(&self) -> Option<AnimTarget> {
        if let Some(target) = self.anim_target {
            let follows_selection = match target {
                AnimTarget::Object(id) => self.selection.primary() == Some(id),
                AnimTarget::Tag(node, _) => self.selection.primary() == Some(node),
                AnimTarget::Material(_) => true,
            };
            if follows_selection && self.target_is_valid(target) {
                return Some(target);
            }
        }
        self.selection.primary().map(AnimTarget::Object)
    }

    /// Choose what the F-curve editor edits. Changing target drops key and row
    /// selections that belonged to the previous entity.
    pub(super) fn set_anim_target(&mut self, target: AnimTarget) -> bool {
        if !self.target_is_valid(target) {
            return false;
        }
        if self.anim_target != Some(target) {
            self.selected_keys.clear();
            self.curve_key_drag = None;
            self.curve_tangent_drag = None;
            self.curve_box_select = None;
            self.selected_curve_channel = None;
        }
        self.anim_target = Some(target);
        true
    }

    /// Current clip for a target. Tag targets without keys still yield an
    /// empty clip so the editor can show their registry rows and record into
    /// them; object and material targets require a live entity.
    pub(super) fn target_clip(&self, target: AnimTarget) -> Option<AnimationClip> {
        match target {
            AnimTarget::Object(id) => Some(self.doc.scene.get(id)?.animation.clone()),
            AnimTarget::Tag(node, index) => {
                let owner = self.doc.scene.get(node)?;
                let tag = owner.tags.get(index)?;
                Self::tag_animation_channel(tag)?;
                Some(
                    owner
                        .tag_animations
                        .get(&(index as u32))
                        .cloned()
                        .unwrap_or_default(),
                )
            }
            AnimTarget::Material(id) => Some(self.doc.materials.get(id)?.animation.clone()),
        }
    }

    pub(super) fn target_label(&self, target: AnimTarget) -> String {
        match target {
            AnimTarget::Object(id) => format!(
                "Object: {}",
                self.doc
                    .scene
                    .get(id)
                    .map(|n| n.name.as_str())
                    .unwrap_or("Object")
            ),
            AnimTarget::Tag(node, index) => {
                let tag_name = self
                    .doc
                    .scene
                    .get(node)
                    .and_then(|n| n.tags.get(index))
                    .map(|tag| tag.label())
                    .unwrap_or("Tag");
                format!("Tag: {tag_name} #{}", index + 1)
            }
            AnimTarget::Material(id) => format!(
                "Material: {}",
                self.doc
                    .materials
                    .get(id)
                    .map(|m| m.name.as_str())
                    .unwrap_or("Material")
            ),
        }
    }

    fn channel_applies_to_target(&self, channel: AnimChannel, target: AnimTarget) -> bool {
        let scope_matches = match target {
            AnimTarget::Object(_) => channel.scope() == lc_anim::ChannelScope::Object,
            AnimTarget::Tag(..) => channel.scope() == lc_anim::ChannelScope::Tag,
            AnimTarget::Material(_) => channel.scope() == lc_anim::ChannelScope::Material,
        };
        if !scope_matches {
            return false;
        }
        let AnimTarget::Object(id) = target else {
            return true;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        match channel.applies_to() {
            ChannelAppliesTo::AnyObject => true,
            ChannelAppliesTo::Camera => matches!(node.kind, NodeKind::Camera { .. }),
            ChannelAppliesTo::Light => matches!(node.kind, NodeKind::Light { .. }),
            ChannelAppliesTo::Morph => {
                matches!(node.kind, NodeKind::Mesh { .. }) && !node.morph.targets.is_empty()
            }
            ChannelAppliesTo::Tag | ChannelAppliesTo::Material => false,
        }
    }

    /// Every registry channel offered for a target, expanded into one row per
    /// scalar component. Quaternion channels stay a single row, and a mesh with
    /// morph targets gets one dynamic row per target weight.
    pub(super) fn curve_rows(&self, target: AnimTarget) -> Vec<CurveRow> {
        let mut rows = Vec::new();
        for channel in AnimChannel::ALL {
            if channel == AnimChannel::MorphWeight
                || !self.channel_applies_to_target(channel, target)
            {
                continue;
            }
            match channel.kind() {
                ChannelKind::Quaternion => rows.push(CurveRow {
                    channel,
                    property: None,
                    morph_target: None,
                }),
                _ => {
                    for property in channel.scalar_properties() {
                        rows.push(CurveRow {
                            channel,
                            property: Some(*property),
                            morph_target: None,
                        });
                    }
                }
            }
        }
        if let AnimTarget::Object(id) = target {
            if let Some(node) = self.doc.scene.get(id) {
                if matches!(node.kind, NodeKind::Mesh { .. }) {
                    for index in 0..node.morph.targets.len() {
                        rows.push(CurveRow {
                            channel: AnimChannel::MorphWeight,
                            property: None,
                            morph_target: Some(index as u32),
                        });
                    }
                }
            }
        }
        rows
    }

    pub(super) fn curve_row_track(row: CurveRow) -> CurveTrackId {
        if let Some(index) = row.morph_target {
            CurveTrackId::MorphWeight(index)
        } else if let Some(property) = row.property {
            CurveTrackId::Scalar(property)
        } else {
            CurveTrackId::Rotation
        }
    }

    pub(super) fn curve_row_label(&self, target: AnimTarget, row: CurveRow) -> String {
        if let Some(index) = row.morph_target {
            let name = match target {
                AnimTarget::Object(id) => self
                    .doc
                    .scene
                    .get(id)
                    .and_then(|node| node.morph.targets.get(index as usize))
                    .map(|target| target.name.as_str())
                    .unwrap_or("Morph"),
                _ => "Morph",
            };
            return format!("{name} Weight");
        }
        match row.property {
            Some(property) => {
                AnimChannel::scalar_label(property).unwrap_or_else(|| property.label().to_string())
            }
            None => row.channel.label().to_string(),
        }
    }

    /// Live values for a registry channel, in registry component order.
    fn channel_values(&self, target: AnimTarget, channel: AnimChannel) -> Option<Vec<f32>> {
        match (target, channel) {
            (AnimTarget::Object(id), AnimChannel::Translation) => {
                let node = self.doc.scene.get(id)?;
                Some(node.transform.translation.to_array().to_vec())
            }
            (AnimTarget::Object(id), AnimChannel::Scale) => {
                let node = self.doc.scene.get(id)?;
                Some(node.transform.scale.to_array().to_vec())
            }
            (AnimTarget::Object(id), AnimChannel::Fov) => match &self.doc.scene.get(id)?.kind {
                NodeKind::Camera { camera } => Some(vec![camera.fov_y_deg]),
                _ => None,
            },
            (AnimTarget::Object(id), AnimChannel::LightIntensity) => {
                match &self.doc.scene.get(id)?.kind {
                    NodeKind::Light { light } => Some(vec![light.intensity]),
                    _ => None,
                }
            }
            (AnimTarget::Object(id), AnimChannel::LightColor) => {
                match &self.doc.scene.get(id)?.kind {
                    NodeKind::Light { light } => Some(light.color.to_array().to_vec()),
                    _ => None,
                }
            }
            (AnimTarget::Tag(node, index), AnimChannel::CopyLocationWeight) => {
                match self.doc.scene.get(node)?.tags.get(index)? {
                    Tag::CopyLocation { weight, .. } => Some(vec![*weight]),
                    _ => None,
                }
            }
            (AnimTarget::Material(id), AnimChannel::MaterialRoughness) => Some(vec![self
                .doc
                .materials
                .get(id)?
                .base_roughness(&self.texture_cache)]),
            (AnimTarget::Material(id), AnimChannel::MaterialMetallic) => Some(vec![self
                .doc
                .materials
                .get(id)?
                .base_metallic(&self.texture_cache)]),
            _ => None,
        }
    }

    fn morph_weight_value(&self, target: AnimTarget, index: u32) -> Option<f32> {
        let AnimTarget::Object(id) = target else {
            return None;
        };
        self.doc
            .scene
            .get(id)?
            .morph
            .targets
            .get(index as usize)
            .map(|target| target.weight)
    }

    /// Record one registry row at the playhead. Morph-weight rows need their
    /// target index in addition to the channel, so the F-curve editor uses this
    /// entry point for both dynamic and ordinary rows.
    pub(super) fn record_curve_row_key(&mut self, target: AnimTarget, row: CurveRow) -> bool {
        if row.channel == AnimChannel::MorphWeight {
            let Some(index) = row.morph_target else {
                return false;
            };
            if !self.target_is_valid(target) || !self.channel_applies_to_target(row.channel, target)
            {
                return false;
            }
            let Some(value) = self.morph_weight_value(target, index) else {
                return false;
            };
            let time = self.playback.time;
            let Some(mut after) = self.target_clip(target) else {
                return false;
            };
            after.set_morph_weight_key(index, time, value, Interpolation::Smooth);
            if !self.commit_target_clip(target, after) {
                return false;
            }
            self.anim_target = Some(target);
            self.selected_curve_channel = Some((target, row));
            self.status = format!(
                "Keyed {} at frame {}",
                self.curve_row_label(target, row),
                (time * self.playback.fps).round()
            );
            return true;
        }
        if self.record_channel_key(target, row.channel) {
            self.selected_curve_channel = Some((target, row));
            true
        } else {
            false
        }
    }

    /// Record one registry channel at the playhead as one undoable command.
    pub(super) fn record_channel_key(&mut self, target: AnimTarget, channel: AnimChannel) -> bool {
        if !self.target_is_valid(target) || !self.channel_applies_to_target(channel, target) {
            return false;
        }
        let time = self.playback.time;
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        if channel.kind() == ChannelKind::Quaternion {
            let AnimTarget::Object(id) = target else {
                return false;
            };
            let Some(node) = self.doc.scene.get(id) else {
                return false;
            };
            after.set_rotation_key(time, node.transform.rotation, Interpolation::Smooth);
        } else {
            let Some(values) = self.channel_values(target, channel) else {
                return false;
            };
            if !after.record_channel(channel, &values, time, Interpolation::Smooth) {
                return false;
            }
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.anim_target = Some(target);
        self.selected_curve_channel = Some((
            target,
            CurveRow {
                channel,
                property: channel.scalar_properties().first().copied(),
                morph_target: None,
            },
        ));
        self.status = format!(
            "Keyed {} at frame {}",
            channel.label(),
            (time * self.playback.fps).round()
        );
        true
    }

    /// Build the single command that writes a clip to the entity that owns it.
    fn target_clip_command(
        &self,
        target: AnimTarget,
        after: AnimationClip,
    ) -> Option<Box<dyn Command>> {
        match target {
            AnimTarget::Object(id) => Some(Box::new(SetAnimationCmd::new(&self.doc, id, after)?)),
            AnimTarget::Tag(node, index) => Some(Box::new(SetTagAnimationCmd::new(
                &self.doc, node, index, after,
            )?)),
            AnimTarget::Material(id) => {
                let before = self.doc.materials.get(id).cloned()?;
                let mut updated = before.clone();
                updated.animation = after;
                if updated == before {
                    return None;
                }
                Some(Box::new(SetMaterialCmd {
                    before,
                    after: updated,
                }))
            }
        }
    }

    /// Commit a clip to the entity that owns it as exactly one undo step.
    fn commit_target_clip(&mut self, target: AnimTarget, after: AnimationClip) -> bool {
        let Some(cmd) = self.target_clip_command(target, after) else {
            return false;
        };
        self.history.execute(&mut self.doc, cmd);
        true
    }

    /// Commit a paste and carry its before/after key selection on the same
    /// history entry. Undo and redo can then restore the editor selection
    /// without a second history stack.
    fn commit_pasted_target_clip(
        &mut self,
        target: AnimTarget,
        after: AnimationClip,
        before_selection: Vec<SelectedKey>,
        after_selection: Vec<SelectedKey>,
    ) -> bool {
        let Some(inner) = self.target_clip_command(target, after) else {
            return false;
        };
        self.history.execute(
            &mut self.doc,
            Box::new(PasteCurveKeysCmd {
                inner,
                target,
                before_selection,
                after_selection,
            }),
        );
        true
    }

    /// Index of the key at `time` on `property`, using the editor's key
    /// tolerance so selection survives frame snapping.
    pub(super) fn curve_key_index(
        clip: &AnimationClip,
        property: AnimProperty,
        time: f32,
    ) -> Option<usize> {
        let track = clip.track(property)?;
        track
            .curve
            .keys
            .iter()
            .position(|key| (key.time - time).abs() <= KEY_TIME_EPSILON)
    }

    pub(super) fn morph_curve_key_index(
        clip: &AnimationClip,
        target: u32,
        time: f32,
    ) -> Option<usize> {
        clip.morph_weights
            .get(&target)?
            .keys
            .iter()
            .position(|key| (key.time - time).abs() <= KEY_TIME_EPSILON)
    }

    /// Select the F-curve key at `time` on the primary object.
    #[cfg(test)]
    pub(super) fn select_curve_key(&mut self, property: AnimProperty, time: f32) -> bool {
        self.select_track_key(CurveTrackId::Scalar(property), time)
    }

    /// Select one quaternion rotation key on the primary object.
    #[cfg(test)]
    pub(super) fn select_rotation_key(&mut self, time: f32) -> bool {
        self.select_track_key(CurveTrackId::Rotation, time)
    }

    #[cfg(test)]
    pub(super) fn select_track_key(&mut self, track: CurveTrackId, time: f32) -> bool {
        let Some(target) = self.current_anim_target() else {
            return false;
        };
        self.select_target_track_key(target, track, time)
    }

    /// Select a key on an explicit object, tag or material target.
    #[cfg(test)]
    pub(super) fn select_target_track_key(
        &mut self,
        target: AnimTarget,
        track: CurveTrackId,
        time: f32,
    ) -> bool {
        self.select_target_track_key_additive(target, track, time, false)
    }

    /// Add or replace one F-curve key in the current selection.
    pub(super) fn select_target_track_key_additive(
        &mut self,
        target: AnimTarget,
        track: CurveTrackId,
        time: f32,
        additive: bool,
    ) -> bool {
        let Some(clip) = self.target_clip(target) else {
            return false;
        };
        let key = SelectedKey {
            target,
            track,
            time,
        };
        if !Self::curve_key_exists(&clip, &key) {
            return false;
        }
        if !additive {
            self.selected_keys.clear();
        }
        if !self
            .selected_keys
            .iter()
            .any(|existing| Self::same_curve_key(*existing, key))
        {
            self.selected_keys.push(key);
        }
        true
    }

    /// Toggle a key in the selection for shift/command clicks, matching the
    /// object selection semantics.
    pub(super) fn toggle_target_track_key(
        &mut self,
        target: AnimTarget,
        track: CurveTrackId,
        time: f32,
    ) -> bool {
        let Some(clip) = self.target_clip(target) else {
            return false;
        };
        let key = SelectedKey {
            target,
            track,
            time,
        };
        if !Self::curve_key_exists(&clip, &key) {
            return false;
        }
        if let Some(index) = self
            .selected_keys
            .iter()
            .position(|existing| Self::same_curve_key(*existing, key))
        {
            self.selected_keys.remove(index);
        } else {
            self.selected_keys.push(key);
        }
        true
    }

    fn same_curve_key(a: SelectedKey, b: SelectedKey) -> bool {
        a.target == b.target && a.track == b.track && (a.time - b.time).abs() <= KEY_TIME_EPSILON
    }

    fn curve_key_exists(clip: &AnimationClip, key: &SelectedKey) -> bool {
        match key.track {
            CurveTrackId::Scalar(property) => {
                Self::curve_key_index(clip, property, key.time).is_some()
            }
            CurveTrackId::Rotation => clip.rotation_key_index(key.time).is_some(),
            CurveTrackId::MorphWeight(target) => {
                Self::morph_curve_key_index(clip, target, key.time).is_some()
            }
        }
    }

    /// All selected keys that still exist on the current target.
    pub(super) fn selected_curve_keys(&self) -> Vec<SelectedKey> {
        let Some(target) = self.current_anim_target() else {
            return Vec::new();
        };
        let Some(clip) = self.target_clip(target) else {
            return Vec::new();
        };
        let mut keys = Vec::new();
        for key in &self.selected_keys {
            if key.target != target || !Self::curve_key_exists(&clip, key) {
                continue;
            }
            if !keys
                .iter()
                .any(|existing| Self::same_curve_key(*existing, *key))
            {
                keys.push(*key);
            }
        }
        keys
    }

    /// The last selected key, used by single-key controls and tests.
    #[cfg(test)]
    pub(super) fn selected_curve_key(&self) -> Option<SelectedKey> {
        self.selected_curve_keys().last().copied()
    }

    pub(super) fn is_curve_key_selected(&self, key: &SelectedKey) -> bool {
        self.selected_keys
            .iter()
            .any(|existing| Self::same_curve_key(*existing, *key))
    }

    pub(super) fn clear_curve_key_selection(&mut self) {
        self.selected_keys.clear();
    }

    /// Select every scalar or quaternion key whose frame and displayed value
    /// fall inside the marquee. Additive selection keeps what was already
    /// selected, matching shift/command selection elsewhere in the editor.
    pub(super) fn select_keys_in_box(
        &mut self,
        target: AnimTarget,
        clip: &AnimationClip,
        time_range: (f32, f32),
        value_range: (f32, f32),
        additive: bool,
    ) -> bool {
        let (time_min, time_max) = time_range;
        let (value_min, value_max) = value_range;
        if !time_min.is_finite()
            || !time_max.is_finite()
            || !value_min.is_finite()
            || !value_max.is_finite()
        {
            return false;
        }
        let mut hits = Vec::new();
        for track in &clip.tracks {
            for key in &track.curve.keys {
                if key.time >= time_min
                    && key.time <= time_max
                    && key.value >= value_min
                    && key.value <= value_max
                {
                    hits.push(SelectedKey {
                        target,
                        track: CurveTrackId::Scalar(track.property),
                        time: key.time,
                    });
                }
            }
        }
        if let Some(track) = &clip.rotation {
            for key in &track.keys {
                if key.time >= time_min
                    && key.time <= time_max
                    && key.value.w >= value_min
                    && key.value.w <= value_max
                {
                    hits.push(SelectedKey {
                        target,
                        track: CurveTrackId::Rotation,
                        time: key.time,
                    });
                }
            }
        }
        for (target_index, curve) in &clip.morph_weights {
            for key in &curve.keys {
                if key.time >= time_min
                    && key.time <= time_max
                    && key.value >= value_min
                    && key.value <= value_max
                {
                    hits.push(SelectedKey {
                        target,
                        track: CurveTrackId::MorphWeight(*target_index),
                        time: key.time,
                    });
                }
            }
        }
        if !additive {
            self.selected_keys.clear();
        }
        for hit in hits {
            if !self
                .selected_keys
                .iter()
                .any(|existing| Self::same_curve_key(*existing, hit))
            {
                self.selected_keys.push(hit);
            }
        }
        !self.selected_keys.is_empty()
    }

    /// Move one scalar key on any object, tag or material target in time as a
    /// single undo step.
    #[cfg(test)]
    pub(super) fn move_target_curve_key(
        &mut self,
        target: AnimTarget,
        property: AnimProperty,
        from: f32,
        to: f32,
    ) -> bool {
        if !to.is_finite() {
            return false;
        }
        let to = to.max(0.0);
        let Some(before) = self.target_clip(target) else {
            return false;
        };
        let mut after = before.clone();
        let Some(track) = after.track_mut(property) else {
            return false;
        };
        let Some(index) = track
            .curve
            .keys
            .iter()
            .position(|key| (key.time - from).abs() <= KEY_TIME_EPSILON)
        else {
            return false;
        };
        if (to - from).abs() <= 1e-4 {
            return false;
        }
        if track
            .curve
            .keys
            .iter()
            .enumerate()
            .any(|(i, key)| i != index && (key.time - to).abs() <= KEY_TIME_EPSILON)
        {
            self.status = "Another key already sits at that frame".into();
            return false;
        }
        track.curve.keys[index].time = to;
        track.curve.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        track.curve.update_tangents();
        if after == before {
            return false;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.selected_keys = vec![SelectedKey {
            target,
            track: CurveTrackId::Scalar(property),
            time: to,
        }];
        self.status = format!("Moved key to frame {}", (to * self.playback.fps).round());
        true
    }

    /// Delete one scalar key on any target as a single undo step, dropping the
    /// track when its last key goes away.
    pub(super) fn delete_target_curve_key(
        &mut self,
        target: AnimTarget,
        property: AnimProperty,
        time: f32,
    ) -> bool {
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let Some(track) = after.track_mut(property) else {
            return false;
        };
        let Some(index) = track
            .curve
            .keys
            .iter()
            .position(|key| (key.time - time).abs() <= KEY_TIME_EPSILON)
        else {
            return false;
        };
        track.curve.keys.remove(index);
        after.tracks.retain(|track| !track.curve.keys.is_empty());
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.selected_keys.clear();
        self.status = "Deleted key".into();
        true
    }

    /// Change interpolation for exactly one scalar key on any target.
    #[cfg(test)]
    pub(super) fn set_target_curve_key_interpolation(
        &mut self,
        target: AnimTarget,
        property: AnimProperty,
        time: f32,
        interpolation: Interpolation,
    ) -> bool {
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let Some(track) = after.track_mut(property) else {
            return false;
        };
        let Some(index) = track
            .curve
            .keys
            .iter()
            .position(|key| (key.time - time).abs() <= KEY_TIME_EPSILON)
        else {
            return false;
        };
        if track.curve.keys[index].interpolation == interpolation {
            // Reselecting Smooth can still be a real edit when a legacy key
            // retains manual slopes; let the curve normalize it.
            if interpolation != Interpolation::Smooth {
                return false;
            }
        }
        if !track.curve.set_interpolation(time, interpolation) {
            return false;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.selected_keys = vec![SelectedKey {
            target,
            track: CurveTrackId::Scalar(property),
            time,
        }];
        self.status = format!("Set key interpolation to {}", interpolation.label());
        true
    }

    /// Apply an interpolation change from one channel row. When keys in that
    /// row are selected, only those keys change; otherwise the entire row
    /// changes, matching the old per-row control's intent while making the edit
    /// undoable and applying the same tangent rules as the selection commands.
    pub(super) fn set_curve_row_interpolation(
        &mut self,
        target: AnimTarget,
        row: CurveRow,
        interpolation: Interpolation,
    ) -> bool {
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let track_id = Self::curve_row_track(row);
        let selected_times: Vec<f32> = self
            .selected_curve_keys()
            .into_iter()
            .filter(|key| key.target == target && key.track == track_id)
            .map(|key| key.time)
            .collect();
        let mut changed = 0;
        if let Some(property) = row.property {
            if let Some(track) = after.track_mut(property) {
                let times: Vec<f32> = track
                    .curve
                    .keys
                    .iter()
                    .filter(|key| {
                        selected_times.is_empty()
                            || selected_times
                                .iter()
                                .any(|time| (key.time - time).abs() <= KEY_TIME_EPSILON)
                    })
                    .map(|key| key.time)
                    .collect();
                for time in times {
                    if track.curve.set_interpolation(time, interpolation) {
                        changed += 1;
                    }
                }
            }
        } else if let Some(index) = row.morph_target {
            if let Some(curve) = after.morph_weights.get_mut(&index) {
                let times: Vec<f32> = curve
                    .keys
                    .iter()
                    .filter(|key| {
                        selected_times.is_empty()
                            || selected_times
                                .iter()
                                .any(|time| (key.time - time).abs() <= KEY_TIME_EPSILON)
                    })
                    .map(|key| key.time)
                    .collect();
                for time in times {
                    if curve.set_interpolation(time, interpolation) {
                        changed += 1;
                    }
                }
            }
        } else if let Some(track) = after.rotation.as_mut() {
            let times: Vec<f32> = track
                .keys
                .iter()
                .filter(|key| {
                    selected_times.is_empty()
                        || selected_times
                            .iter()
                            .any(|time| (key.time - time).abs() <= KEY_TIME_EPSILON)
                })
                .map(|key| key.time)
                .collect();
            for time in times {
                if track.set_interpolation(time, interpolation) {
                    changed += 1;
                }
            }
        }
        if changed == 0 {
            return false;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.status = format!("Set key interpolation to {}", interpolation.label());
        true
    }

    pub(super) fn delete_target_morph_weight_curve_key(
        &mut self,
        target: AnimTarget,
        target_index: u32,
        time: f32,
    ) -> bool {
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let Some(index) = Self::morph_curve_key_index(&after, target_index, time) else {
            return false;
        };
        let Some(curve) = after.morph_weights.get_mut(&target_index) else {
            return false;
        };
        curve.keys.remove(index);
        after
            .morph_weights
            .retain(|_, curve| !curve.keys.is_empty());
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.selected_keys.clear();
        self.status = "Deleted key".into();
        true
    }

    pub(super) fn delete_target_rotation_curve_key(
        &mut self,
        target: AnimTarget,
        time: f32,
    ) -> bool {
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let Some(track) = after.rotation.as_mut() else {
            return false;
        };
        if track.key_index(time).is_none() {
            return false;
        }
        track.remove_nearest(time);
        if track.is_empty() {
            after.rotation = None;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.selected_keys.clear();
        self.status = "Deleted key".into();
        true
    }

    /// Retime the selected F-curve keys around their earliest selected time.
    ///
    /// The pivot is the earliest selected key, so the selection's leading edge
    /// stays fixed and later keys move toward or away from it. Times are not
    /// frame-snapped. Positive factors are required; a collision with an
    /// unselected key or an implied negative time rejects the whole edit without
    /// changing the document. Auto tangents are re-derived and manual tangents
    /// are rescaled per incident segment by the anim crate. One target commit
    /// makes the retime one undo step.
    pub(super) fn retime_selected_curve_keys(&mut self, factor: f32) -> bool {
        if !factor.is_finite() || factor <= 0.0 || (factor - 1.0).abs() <= 1e-6 {
            self.status = "Enter a positive retime factor other than 1".into();
            return false;
        }
        let keys = self.selected_curve_keys();
        let Some(first) = keys.first() else {
            self.status = "Select F-curve keys to retime".into();
            return false;
        };
        let target = first.target;
        if keys.iter().any(|key| key.target != target) {
            return false;
        }
        let pivot = keys
            .iter()
            .map(|key| key.time)
            .fold(f32::INFINITY, f32::min);
        let Some(before) = self.target_clip(target) else {
            return false;
        };
        let mut after = before.clone();
        let mut scalar_tracks: Vec<(AnimProperty, Vec<f32>)> = Vec::new();
        let mut morph_tracks: Vec<(u32, Vec<f32>)> = Vec::new();
        let mut rotation_times = Vec::new();
        for key in &keys {
            match key.track {
                CurveTrackId::Scalar(property) => {
                    if let Some((_, times)) = scalar_tracks
                        .iter_mut()
                        .find(|(candidate, _)| *candidate == property)
                    {
                        times.push(key.time);
                    } else {
                        scalar_tracks.push((property, vec![key.time]));
                    }
                }
                CurveTrackId::Rotation => rotation_times.push(key.time),
                CurveTrackId::MorphWeight(target) => {
                    if let Some((_, times)) = morph_tracks
                        .iter_mut()
                        .find(|(candidate, _)| *candidate == target)
                    {
                        times.push(key.time);
                    } else {
                        morph_tracks.push((target, vec![key.time]));
                    }
                }
            }
        }
        for (property, times) in &scalar_tracks {
            let Some(track) = after.track_mut(*property) else {
                return false;
            };
            if !track.curve.retime_selected_keys(pivot, factor, times) {
                self.status = "Retime would collide with another key or leave the timeline".into();
                return false;
            }
        }
        if !rotation_times.is_empty() {
            let Some(track) = after.rotation.as_mut() else {
                return false;
            };
            if !track.retime_selected_keys(pivot, factor, &rotation_times) {
                self.status = "Retime would collide with another key or leave the timeline".into();
                return false;
            }
        }
        for (target_index, times) in &morph_tracks {
            let Some(curve) = after.morph_weights.get_mut(target_index) else {
                return false;
            };
            if !curve.retime_selected_keys(pivot, factor, times) {
                self.status = "Retime would collide with another key or leave the timeline".into();
                return false;
            }
        }
        if after == before {
            self.status = "Those keys already have the requested timing".into();
            return false;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.selected_keys = keys
            .into_iter()
            .map(|mut key| {
                key.time = pivot + (key.time - pivot) * factor;
                key
            })
            .collect();
        self.status = format!("Retimed {} key(s) by ×{factor}", self.selected_keys.len());
        true
    }

    /// Retime every key in one object's take around zero seconds. The take
    /// manager uses this path; it is one `SetAnimationCmd` undo step and covers
    /// scalar, morph-weight and quaternion rotation channels. Tag and material
    /// clips are not part of a per-object take; retime their selected keys from
    /// the F-curve editor instead.
    pub(super) fn retime_animation_take(&mut self, id: NodeId, factor: f32) -> bool {
        if !factor.is_finite() || factor <= 0.0 || (factor - 1.0).abs() <= 1e-6 {
            self.status = "Enter a positive retime factor other than 1".into();
            return false;
        }
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        let name = node.name.clone();
        let before = node.animation.clone();
        if before.is_empty() {
            self.status = "This take has no keys to retime".into();
            return false;
        }
        let mut after = before.clone();
        if !after.retime_all(0.0, factor) {
            self.status = "Retime would collide with another key or leave the timeline".into();
            return false;
        }
        if after == before {
            self.status = "That factor does not change any key times in this take".into();
            return false;
        }
        let key_count = after
            .tracks
            .iter()
            .map(|track| track.curve.keys.len())
            .sum::<usize>()
            + after.rotation.as_ref().map_or(0, |track| track.keys.len())
            + after
                .morph_weights
                .values()
                .map(|curve| curve.keys.len())
                .sum::<usize>();
        let Some(command) = SetAnimationCmd::new(&self.doc, id, after) else {
            return false;
        };
        self.history.execute(&mut self.doc, Box::new(command));
        for key in &mut self.selected_keys {
            if key.target == AnimTarget::Object(id) {
                key.time *= factor;
            }
        }
        self.status = format!("Retimed take {name} by ×{factor} ({key_count} keys)");
        true
    }

    /// Move every selected key by one frame delta as a single undo step.
    pub(super) fn move_selected_curve_keys_by(&mut self, delta: f32) -> bool {
        if !delta.is_finite() || delta.abs() <= 1e-4 {
            return false;
        }
        let keys = self.selected_curve_keys();
        if keys.is_empty() {
            return false;
        }
        let target = keys[0].target;
        if keys.iter().any(|key| key.target != target) {
            return false;
        }
        let Some(before) = self.target_clip(target) else {
            return false;
        };
        // Clamp the whole selection at frame zero so spacing is preserved even
        // when the drag would push the earliest key before the timeline start.
        let earliest = keys
            .iter()
            .map(|key| key.time)
            .fold(f32::INFINITY, f32::min);
        let delta = delta.max(-earliest);
        if delta.abs() <= 1e-4 {
            return false;
        }
        let mut after = before.clone();
        let mut moved = Vec::with_capacity(keys.len());
        for key in &keys {
            moved.push((*key, key.time + delta));
        }
        for (key, new_time) in &moved {
            let selected_times: Vec<f32> = keys
                .iter()
                .filter(|other| other.track == key.track)
                .map(|other| other.time)
                .collect();
            let collides = match key.track {
                CurveTrackId::Scalar(property) => after.track(property).is_some_and(|track| {
                    track.curve.keys.iter().any(|candidate| {
                        (candidate.time - *new_time).abs() <= KEY_TIME_EPSILON
                            && !selected_times
                                .iter()
                                .any(|time| (candidate.time - time).abs() <= KEY_TIME_EPSILON)
                    })
                }),
                CurveTrackId::Rotation => after.rotation.as_ref().is_some_and(|track| {
                    track.keys.iter().any(|candidate| {
                        (candidate.time - *new_time).abs() <= KEY_TIME_EPSILON
                            && !selected_times
                                .iter()
                                .any(|time| (candidate.time - time).abs() <= KEY_TIME_EPSILON)
                    })
                }),
                CurveTrackId::MorphWeight(target) => {
                    after.morph_weights.get(&target).is_some_and(|curve| {
                        curve.keys.iter().any(|candidate| {
                            (candidate.time - *new_time).abs() <= KEY_TIME_EPSILON
                                && !selected_times
                                    .iter()
                                    .any(|time| (candidate.time - time).abs() <= KEY_TIME_EPSILON)
                        })
                    })
                }
            };
            if collides {
                self.status = "Another key already sits at that frame".into();
                return false;
            }
        }
        for (index, (key, new_time)) in moved.iter().enumerate() {
            for (other, other_time) in moved.iter().skip(index + 1) {
                if key.track == other.track && (*new_time - *other_time).abs() <= KEY_TIME_EPSILON {
                    self.status = "Two selected keys would land on the same frame".into();
                    return false;
                }
            }
        }
        for (key, new_time) in &moved {
            match key.track {
                CurveTrackId::Scalar(property) => {
                    let Some(track) = after.track_mut(property) else {
                        return false;
                    };
                    let Some(index) = track.curve.keys.iter().position(|candidate| {
                        (candidate.time - key.time).abs() <= KEY_TIME_EPSILON
                    }) else {
                        return false;
                    };
                    track.curve.keys[index].time = *new_time;
                }
                CurveTrackId::Rotation => {
                    let Some(track) = after.rotation.as_mut() else {
                        return false;
                    };
                    let Some(index) = track.key_index(key.time) else {
                        return false;
                    };
                    track.keys[index].time = *new_time;
                }
                CurveTrackId::MorphWeight(target) => {
                    let Some(curve) = after.morph_weights.get_mut(&target) else {
                        return false;
                    };
                    let Some(index) = curve.keys.iter().position(|candidate| {
                        (candidate.time - key.time).abs() <= KEY_TIME_EPSILON
                    }) else {
                        return false;
                    };
                    curve.keys[index].time = *new_time;
                }
            }
        }
        for track in &mut after.tracks {
            track.curve.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            track.curve.update_tangents();
        }
        if let Some(track) = after.rotation.as_mut() {
            track.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        }
        for curve in after.morph_weights.values_mut() {
            curve.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            curve.update_tangents();
        }
        if after == before {
            return false;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.selected_keys = moved
            .into_iter()
            .map(|(mut key, time)| {
                key.time = time;
                key
            })
            .collect();
        self.status = format!(
            "Moved {} key(s) by {} frame(s)",
            self.selected_keys.len(),
            (delta * self.playback.fps).round()
        );
        true
    }

    /// Move the selected anchor key to `to`, carrying the whole multi-key
    /// selection with it.
    #[cfg(test)]
    pub(super) fn move_selected_curve_key(&mut self, to: f32) -> bool {
        let Some(key) = self.selected_curve_key() else {
            return false;
        };
        self.move_selected_curve_keys_by(to - key.time)
    }

    /// Set one hand-authored Bezier tangent slope for a scalar or morph key.
    /// One drag becomes one undo step; the editor keeps the other side of the
    /// key exactly as it was.
    pub(super) fn set_target_curve_tangent(
        &mut self,
        target: AnimTarget,
        track: CurveTrackId,
        time: f32,
        incoming: bool,
        slope: f32,
    ) -> bool {
        if !slope.is_finite() {
            return false;
        }
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let changed = match track {
            CurveTrackId::Scalar(property) => {
                let Some(track) = after.track_mut(property) else {
                    return false;
                };
                let Some(index) = track
                    .curve
                    .keys
                    .iter()
                    .position(|key| (key.time - time).abs() <= KEY_TIME_EPSILON)
                else {
                    return false;
                };
                let key = &mut track.curve.keys[index];
                let before = (key.tangent_in, key.tangent_out, key.tangent_manual);
                if incoming {
                    key.tangent_in = slope;
                    key.tangent_in_type = TangentType::Spline;
                } else {
                    key.tangent_out = slope;
                    key.tangent_out_type = TangentType::Spline;
                }
                key.tangent_manual = true;
                before != (key.tangent_in, key.tangent_out, key.tangent_manual)
            }
            CurveTrackId::MorphWeight(target_index) => {
                let Some(curve) = after.morph_weights.get_mut(&target_index) else {
                    return false;
                };
                let Some(index) = curve
                    .keys
                    .iter()
                    .position(|key| (key.time - time).abs() <= KEY_TIME_EPSILON)
                else {
                    return false;
                };
                let key = &mut curve.keys[index];
                let before = (key.tangent_in, key.tangent_out, key.tangent_manual);
                if incoming {
                    key.tangent_in = slope;
                    key.tangent_in_type = TangentType::Spline;
                } else {
                    key.tangent_out = slope;
                    key.tangent_out_type = TangentType::Spline;
                }
                key.tangent_manual = true;
                before != (key.tangent_in, key.tangent_out, key.tangent_manual)
            }
            CurveTrackId::Rotation => return false,
        };
        if !changed {
            return false;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.status = "Adjusted tangent".into();
        true
    }

    /// Delete every selected key as a single undo step. A track disappears
    /// when its last key is removed, and a quaternion track is dropped with it.
    pub(super) fn delete_selected_curve_key(&mut self) -> bool {
        let keys = self.selected_curve_keys();
        if keys.is_empty() {
            return false;
        }
        let target = keys[0].target;
        if keys.iter().any(|key| key.target != target) {
            return false;
        }
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let count = keys.len();
        for key in &keys {
            match key.track {
                CurveTrackId::Scalar(property) => {
                    if let Some(track) = after.track_mut(property) {
                        track.curve.remove_nearest(key.time);
                    }
                }
                CurveTrackId::Rotation => {
                    if let Some(track) = after.rotation.as_mut() {
                        track.remove_nearest(key.time);
                        if track.is_empty() {
                            after.rotation = None;
                        }
                    }
                }
                CurveTrackId::MorphWeight(target) => {
                    if let Some(curve) = after.morph_weights.get_mut(&target) {
                        curve.remove_nearest(key.time);
                    }
                }
            }
        }
        after.tracks.retain(|track| !track.curve.keys.is_empty());
        after
            .morph_weights
            .retain(|_, curve| !curve.keys.is_empty());
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.selected_keys.clear();
        self.status = if count == 1 {
            "Deleted key".into()
        } else {
            format!("Deleted {count} keys")
        };
        true
    }

    /// Set interpolation for every selected key as one undo step. The anim
    /// crate applies the tangent-ownership rule: Smooth replaces hand-authored
    /// slopes with automatic ones, while Spline preserves them. Scalar, morph
    /// and quaternion keys all pass through this same selection.
    pub(super) fn set_selected_curve_key_interpolation(
        &mut self,
        interpolation: Interpolation,
    ) -> bool {
        let keys = self.selected_curve_keys();
        if keys.is_empty() {
            return false;
        }
        let target = keys[0].target;
        if keys.iter().any(|key| key.target != target) {
            return false;
        }
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let mut changed = 0;
        for key in &keys {
            match key.track {
                CurveTrackId::Scalar(property) => {
                    if let Some(track) = after.track_mut(property) {
                        if track.curve.set_interpolation(key.time, interpolation) {
                            changed += 1;
                        }
                    }
                }
                CurveTrackId::Rotation => {
                    if let Some(track) = after.rotation.as_mut() {
                        if track.set_interpolation(key.time, interpolation) {
                            changed += 1;
                        }
                    }
                }
                CurveTrackId::MorphWeight(target) => {
                    if let Some(curve) = after.morph_weights.get_mut(&target) {
                        if curve.set_interpolation(key.time, interpolation) {
                            changed += 1;
                        }
                    }
                }
            }
        }
        if changed == 0 {
            return false;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.status = format!("Set key interpolation to {}", interpolation.label());
        true
    }

    /// Set the same tangent derivation rule on the selected keys. `Tangent >
    /// Type` passes both sides; the left/right menu variants set only one. The
    /// whole selection is committed once, so types and overwritten slopes undo
    /// together.
    pub(super) fn set_selected_curve_tangent_type(
        &mut self,
        tangent_type: TangentType,
        incoming: bool,
        outgoing: bool,
    ) -> bool {
        let keys = self.selected_curve_keys();
        if keys.is_empty() {
            self.status = "Select F-curve keys to change their tangent type".into();
            return false;
        }
        let target = keys[0].target;
        if keys.iter().any(|key| key.target != target) {
            return false;
        }
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let mut changed = 0;
        for key in &keys {
            match key.track {
                CurveTrackId::Scalar(property) => {
                    if let Some(track) = after.track_mut(property) {
                        if track
                            .curve
                            .set_tangent_type(key.time, tangent_type, incoming, outgoing)
                        {
                            changed += 1;
                        }
                    }
                }
                CurveTrackId::Rotation => {
                    if let Some(track) = after.rotation.as_mut() {
                        if track.set_tangent_type(key.time, tangent_type, incoming, outgoing) {
                            changed += 1;
                        }
                    }
                }
                CurveTrackId::MorphWeight(target) => {
                    if let Some(curve) = after.morph_weights.get_mut(&target) {
                        if curve.set_tangent_type(key.time, tangent_type, incoming, outgoing) {
                            changed += 1;
                        }
                    }
                }
            }
        }
        if changed == 0 {
            self.status = "Those keys already use that tangent type".into();
            return false;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.status = format!("Set tangent type to {}", tangent_type.label());
        true
    }

    /// The common interpolation of the current key selection, or `None` for a
    /// mixed selection. The toolbar uses this to show `Mixed` rather than
    /// implying that the first key speaks for the rest.
    pub(super) fn selected_curve_interpolation(&self) -> Option<Interpolation> {
        let keys = self.selected_curve_keys();
        let clip = self.target_clip(self.current_anim_target()?)?;
        let mut common = None;
        for key in keys {
            let interpolation = match key.track {
                CurveTrackId::Scalar(property) => {
                    clip.track(property)?
                        .curve
                        .keys
                        .iter()
                        .find(|candidate| (candidate.time - key.time).abs() <= KEY_TIME_EPSILON)?
                        .interpolation
                }
                CurveTrackId::Rotation => {
                    clip.rotation
                        .as_ref()?
                        .keys
                        .iter()
                        .find(|candidate| (candidate.time - key.time).abs() <= KEY_TIME_EPSILON)?
                        .interpolation
                }
                CurveTrackId::MorphWeight(target) => {
                    clip.morph_weights
                        .get(&target)?
                        .keys
                        .iter()
                        .find(|candidate| (candidate.time - key.time).abs() <= KEY_TIME_EPSILON)?
                        .interpolation
                }
            };
            match common {
                Some(existing) if existing != interpolation => return None,
                None => common = Some(interpolation),
                _ => {}
            }
        }
        common
    }

    /// The common tangent type across every selected side, or `None` when the
    /// selection is mixed (including a key whose two sides differ).
    pub(super) fn selected_curve_tangent_type(&self) -> Option<TangentType> {
        let keys = self.selected_curve_keys();
        let clip = self.target_clip(self.current_anim_target()?)?;
        let mut common = None;
        for key in keys {
            let (incoming, outgoing) = match key.track {
                CurveTrackId::Scalar(property) => {
                    let key =
                        clip.track(property)?.curve.keys.iter().find(|candidate| {
                            (candidate.time - key.time).abs() <= KEY_TIME_EPSILON
                        })?;
                    (key.tangent_in_type, key.tangent_out_type)
                }
                CurveTrackId::Rotation => {
                    let key =
                        clip.rotation.as_ref()?.keys.iter().find(|candidate| {
                            (candidate.time - key.time).abs() <= KEY_TIME_EPSILON
                        })?;
                    (key.tangent_in_type, key.tangent_out_type)
                }
                CurveTrackId::MorphWeight(target) => {
                    let key = clip
                        .morph_weights
                        .get(&target)?
                        .keys
                        .iter()
                        .find(|candidate| (candidate.time - key.time).abs() <= KEY_TIME_EPSILON)?;
                    (key.tangent_in_type, key.tangent_out_type)
                }
            };
            if incoming != outgoing {
                return None;
            }
            match common {
                Some(existing) if existing != incoming => return None,
                None => common = Some(incoming),
                _ => {}
            }
        }
        common
    }

    /// Copy the current key selection, preserving each key's offset from the
    /// earliest selected key.
    pub(super) fn copy_selected_curve_keys(&mut self) -> bool {
        let keys = self.selected_curve_keys();
        let Some(first) = keys.first() else {
            self.status = "Select F-curve keys to copy".into();
            return false;
        };
        let target = first.target;
        let Some(clip) = self.target_clip(target) else {
            return false;
        };
        let anchor = keys
            .iter()
            .map(|key| key.time)
            .fold(f32::INFINITY, f32::min);
        let mut clipboard = Vec::with_capacity(keys.len());
        for key in &keys {
            match key.track {
                CurveTrackId::Scalar(property) => {
                    let Some(track) = clip.track(property) else {
                        continue;
                    };
                    let Some(value) =
                        track.curve.keys.iter().find(|candidate| {
                            (candidate.time - key.time).abs() <= KEY_TIME_EPSILON
                        })
                    else {
                        continue;
                    };
                    clipboard.push(CurveClipboardKey {
                        track: key.track,
                        offset: key.time - anchor,
                        value: CurveKeyValue::Scalar(value.value),
                        interpolation: value.interpolation,
                    });
                }
                CurveTrackId::Rotation => {
                    let Some(track) = clip.rotation.as_ref() else {
                        continue;
                    };
                    let Some(value) = track
                        .keys
                        .iter()
                        .find(|candidate| (candidate.time - key.time).abs() <= KEY_TIME_EPSILON)
                    else {
                        continue;
                    };
                    clipboard.push(CurveClipboardKey {
                        track: key.track,
                        offset: key.time - anchor,
                        value: CurveKeyValue::Rotation(value.value),
                        interpolation: value.interpolation,
                    });
                }
                CurveTrackId::MorphWeight(target) => {
                    let Some(curve) = clip.morph_weights.get(&target) else {
                        continue;
                    };
                    let Some(value) = curve
                        .keys
                        .iter()
                        .find(|candidate| (candidate.time - key.time).abs() <= KEY_TIME_EPSILON)
                    else {
                        continue;
                    };
                    clipboard.push(CurveClipboardKey {
                        track: key.track,
                        offset: key.time - anchor,
                        value: CurveKeyValue::Scalar(value.value),
                        interpolation: value.interpolation,
                    });
                }
            }
        }
        if clipboard.is_empty() {
            return false;
        }
        let count = clipboard.len();
        self.curve_clipboard = CurveClipboard { keys: clipboard };
        self.status = format!("Copied {count} key(s)");
        true
    }

    /// Paste copied keys at the playhead. Destination channels are resolved
    /// through the registry, so a paste can target another object, tag or
    /// material and only compatible channels are written. The whole paste,
    /// including overwrites, is one undo step.
    pub(super) fn paste_curve_keys(&mut self) -> bool {
        let Some(target) = self.current_anim_target() else {
            return false;
        };
        if self.curve_clipboard.keys.is_empty() {
            self.status = "Nothing copied to paste".into();
            return false;
        }
        let before_selection = self.selected_curve_keys();
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let mut pasted = Vec::new();
        for item in &self.curve_clipboard.keys {
            let channel = match item.track {
                CurveTrackId::Scalar(property) => AnimChannel::from_scalar_property(property),
                CurveTrackId::Rotation => Some(AnimChannel::Rotation),
                CurveTrackId::MorphWeight(_) => Some(AnimChannel::MorphWeight),
            };
            let Some(channel) = channel else {
                continue;
            };
            let compatible = match item.track {
                CurveTrackId::Scalar(_) | CurveTrackId::MorphWeight(_) => {
                    channel.kind() != ChannelKind::Quaternion
                }
                CurveTrackId::Rotation => channel.kind() == ChannelKind::Quaternion,
            };
            if !compatible || !self.channel_applies_to_target(channel, target) {
                continue;
            }
            if let CurveTrackId::MorphWeight(index) = item.track {
                if self.morph_weight_value(target, index).is_none() {
                    continue;
                }
            }
            let time = (self.playback.time + item.offset).max(0.0);
            if !time.is_finite() {
                continue;
            }
            match item.track {
                CurveTrackId::Scalar(property) => {
                    let CurveKeyValue::Scalar(value) = item.value else {
                        continue;
                    };
                    after.set_key(property, time, value, item.interpolation);
                    pasted.push(SelectedKey {
                        target,
                        track: item.track,
                        time,
                    });
                }
                CurveTrackId::Rotation => {
                    let CurveKeyValue::Rotation(value) = item.value else {
                        continue;
                    };
                    after.set_rotation_key(time, value, item.interpolation);
                    pasted.push(SelectedKey {
                        target,
                        track: item.track,
                        time,
                    });
                }
                CurveTrackId::MorphWeight(index) => {
                    let CurveKeyValue::Scalar(value) = item.value else {
                        continue;
                    };
                    after.set_morph_weight_key(index, time, value, item.interpolation);
                    pasted.push(SelectedKey {
                        target,
                        track: item.track,
                        time,
                    });
                }
            }
        }
        if pasted.is_empty() {
            self.status = "No copied channels apply to this target".into();
            return false;
        }
        let after_selection = pasted.clone();
        if !self.commit_pasted_target_clip(target, after, before_selection, after_selection) {
            return false;
        }
        let count = pasted.len();
        self.selected_keys = pasted;
        self.status = format!("Pasted {count} key(s)");
        true
    }

    #[cfg(test)]
    pub(super) fn move_curve_key(&mut self, property: AnimProperty, from: f32, to: f32) -> bool {
        let Some(target) = self.current_anim_target() else {
            return false;
        };
        self.move_target_curve_key(target, property, from, to)
    }

    #[cfg(test)]
    pub(super) fn delete_curve_key(&mut self, property: AnimProperty, time: f32) -> bool {
        let Some(target) = self.current_anim_target() else {
            return false;
        };
        self.delete_target_curve_key(target, property, time)
    }

    #[cfg(test)]
    pub(super) fn set_curve_key_interpolation(
        &mut self,
        property: AnimProperty,
        time: f32,
        interpolation: Interpolation,
    ) -> bool {
        let Some(target) = self.current_anim_target() else {
            return false;
        };
        self.set_target_curve_key_interpolation(target, property, time, interpolation)
    }

    pub(super) fn key_transforms_ready(&self) -> bool {
        !self.selection.is_empty() && !self.key_transform_edits().is_empty()
    }

    fn key_transform_channels(&mut self) {
        let edits = self.key_transform_edits();
        if edits.is_empty() {
            self.status = "No transform keys to change at this frame".into();
            return;
        }
        let count = edits.len();
        let time = self.playback.time;
        self.history
            .execute(&mut self.doc, Box::new(SetAnimationsCmd { edits }));
        self.status = if count == 1 {
            format!(
                "Keyed transforms at frame {}",
                (time * self.playback.fps).round()
            )
        } else {
            format!(
                "Keyed transforms on {count} objects at frame {}",
                (time * self.playback.fps).round()
            )
        };
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

impl LeopardApp {
    fn show_file_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        let mode = dialog.mode;
        let title = match mode {
            FileMode::Open => "Open Scene",
            FileMode::SaveAs => "Save Scene As",
            FileMode::Import => "Import Geometry",
            FileMode::Export => "Export Geometry",
        };
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Path");
                    ui.text_edit_singleline(&mut dialog.path);
                });
                ui.label(
                    egui::RichText::new("Supported: .lcscene, .obj, .stl, .gltf, .glb")
                        .small()
                        .weak(),
                );
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    if ui.button("OK").clicked() {
                        apply = true;
                    }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    apply = true;
                }
            });
        if cancel {
            open = false;
        }
        if apply {
            let path = dialog.path.clone();
            self.dialog = None;
            self.apply_file_dialog(ctx, mode, path);
        } else if !open {
            self.dialog = None;
            if matches!(mode, FileMode::SaveAs) {
                self.lifecycle.after_save_cancelled();
            }
        }
    }

    fn show_guard_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.lifecycle.pending_action() else {
            return;
        };
        self.lifecycle.refresh_dirty(&self.doc, true);
        if !self.lifecycle.is_dirty() {
            self.lifecycle.clear_guard();
            self.execute_action(ctx, action);
            return;
        }
        let document_name = self.document_name();
        let prompt = action.prompt();
        let mut choice = None;
        egui::Window::new("Unsaved Changes")
            .id(egui::Id::new("unsaved_changes_dialog"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_min_width(340.0);
                ui.label(format!("“{document_name}” has unsaved changes."));
                ui.label(
                    egui::RichText::new(format!("Save before {prompt}?"))
                        .small()
                        .weak(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        choice = Some(GuardChoice::Save);
                    }
                    if ui.button("Don't Save").clicked() {
                        choice = Some(GuardChoice::Discard);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(GuardChoice::Cancel);
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            choice = Some(GuardChoice::Cancel);
        }
        match choice {
            Some(GuardChoice::Save) => {
                self.lifecycle.choose(GuardChoice::Save);
                self.save_or_prompt(ctx);
            }
            Some(GuardChoice::Discard) => {
                if let GuardResult::Run(action) = self.lifecycle.choose(GuardChoice::Discard) {
                    self.lifecycle.discard_snapshot();
                    self.execute_action(ctx, action);
                }
            }
            Some(GuardChoice::Cancel) => {
                self.lifecycle.choose(GuardChoice::Cancel);
                self.status = "Cancelled".into();
            }
            None => {}
        }
    }

    fn show_recovery_dialog(&mut self, ctx: &egui::Context) {
        let Some(recovered) = self.lifecycle.recovery_prompt() else {
            return;
        };
        let origin = recovered
            .origin
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "an untitled scene".into());
        let age = recovered
            .saved_at
            .map(format_age)
            .unwrap_or_else(|| "an unknown time".into());
        let mut choice = None;
        egui::Window::new("Recover Unsaved Work")
            .id(egui::Id::new("recovery_dialog"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_min_width(400.0);
                ui.label(format!("LeopardCraft found unsaved work for {origin}."));
                ui.label(
                    egui::RichText::new(format!("Snapshot written {age}."))
                        .small()
                        .weak(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Recover").clicked() {
                        choice = Some(true);
                    }
                    if ui.button("Discard").clicked() {
                        choice = Some(false);
                    }
                });
            });
        match choice {
            Some(true) => {
                let Some(recovered) = self.lifecycle.take_recovery() else {
                    return;
                };
                let origin = recovered.origin.clone();
                self.load_scene(recovered.document, origin.clone());
                self.lifecycle.mark_recovered(origin);
                self.status = "Recovered unsaved scene from the previous session".into();
            }
            Some(false) => {
                self.lifecycle.discard_recovery();
                self.status = "Discarded recovered scene".into();
            }
            None => {}
        }
    }

    fn show_about(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let mut open = true;
        egui::Window::new("About LeopardCraft")
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.heading("LeopardCraft");
                ui.label("Clean-room 3D modelling, rendering and animation in pure Rust.");
                ui.label("egui + wgpu · deterministic CPU path tracer · offline-first");
                ui.separator();
                ui.label(
                    egui::RichText::new(
                        "No proprietary code, binaries or file internals were used. \
                         Geometry, shading, animation and file formats are implemented from \
                         public specifications and original architecture.",
                    )
                    .small(),
                );
                ui.label(egui::RichText::new("MIT OR Apache-2.0").small().weak());
            });
        self.show_about = open;
    }
}

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

impl LeopardApp {
    /// Keyboard commands shared by the real app frame and input-driven tests.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
            self.focus_search = true;
            self.browser_tab = BrowserTab::Objects;
        }
        // Check the more specific Shift+Cmd/Ctrl+Z first: egui ignores extra
        // Shift when matching, so otherwise a redo would be consumed as undo.
        let redo = ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers {
                    shift: true,
                    ..egui::Modifiers::COMMAND
                },
                egui::Key::Z,
            )
        });
        let undo = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        let typing = ctx.text_edit_focused();
        let native_copy = ctx.input(|i| i.events.contains(&egui::Event::Copy));
        let native_paste = ctx.input(|i| {
            i.events
                .iter()
                .any(|event| matches!(event, egui::Event::Paste(_)))
        });
        // egui-winit suppresses Ctrl/Cmd+V when the system clipboard has no
        // text, but still forwards the V key release. Fall back to the editor's
        // internal curve clipboard on that release.
        let paste_release = ctx.input(|i| {
            // Some backends report the modifier after releasing Ctrl, so a V
            // release in the same frame as a Ctrl release is still the chord.
            let ctrl_released = i.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::ControlLeft | egui::Key::ControlRight,
                        pressed: false,
                        ..
                    }
                )
            });
            i.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::V,
                        pressed: false,
                        modifiers,
                        ..
                    } if modifiers.command || ctrl_released
                )
            })
        });
        if !typing && self.show_curves {
            // Native backends translate Ctrl/Cmd+C and Ctrl/Cmd+V into clipboard
            // events instead of key presses, so handle both forms.
            if native_copy
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::C))
            {
                self.copy_selected_curve_keys();
            }
            if native_paste {
                self.paste_curve_keys();
                self.native_paste_pending = !paste_release;
            } else if paste_release {
                if self.native_paste_pending {
                    self.native_paste_pending = false;
                } else {
                    self.paste_curve_keys();
                }
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::V)) {
                self.paste_curve_keys();
                // Platforms that still deliver a raw V press will also deliver
                // the release; swallow it so the chord pastes only once.
                self.native_paste_pending = true;
            }
        } else if !self.show_curves {
            self.native_paste_pending = false;
        }
        if !typing && ctx.input(|i| i.key_pressed(egui::Key::Delete)) && !self.selection.is_empty()
        {
            self.delete_selected();
        }
        if !typing && ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.frame_selected();
        }
        if !typing
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::D))
            && !self.selection.is_empty()
        {
            self.duplicate_selected();
        }
    }

    fn ui_viewport(&mut self, ui: &mut egui::Ui) {
        let available = ui.available_size();
        if available.x < 32.0 || available.y < 32.0 {
            return;
        }
        let (rect, response) = ui.allocate_exact_size(available, Sense::click_and_drag());
        self.last_viewport_rect = rect;
        self.last_viewport_size = available;
        let ppp = ui.ctx().pixels_per_point();
        let pixel_w = (available.x * ppp).round().max(16.0) as u32;
        let pixel_h = (available.y * ppp).round().max(16.0) as u32;
        let evaluated = self.evaluate();
        let frame = self.build_frame(&evaluated);
        let texture = self.viewport.render(&frame, pixel_w, pixel_h);
        let painter = ui.painter_at(rect);
        if let Some(texture) = texture {
            painter.image(
                texture,
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            painter.rect_filled(rect, 0.0, Color32::from_rgb(120, 131, 141));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "wgpu viewport unavailable on this backend",
                egui::FontId::proportional(16.0),
                Color32::LIGHT_GRAY,
            );
        }

        let mut mode_hovered = false;
        let mesh_ready = self.selected_is_mesh();
        let weight_ready = self.weight_paint_ready();
        if self.weight_paint && !weight_ready {
            self.weight_paint = false;
        }
        let strip = Rect::from_min_size(
            rect.left_top() + Vec2::new(7.0, 8.0),
            Vec2::new(30.0, 5.0 * 27.0 + 7.0),
        );
        painter.rect_filled(
            strip,
            4.0,
            Color32::from_rgba_unmultiplied(240, 240, 240, 218),
        );
        painter.rect_stroke(
            strip,
            4.0,
            egui::Stroke::new(1.0, Color32::from_rgb(170, 170, 170)),
            egui::StrokeKind::Inside,
        );
        for (index, (icon, label)) in [
            (Icon::ObjectMode, "Object mode"),
            (Icon::PointMode, "Point mode"),
            (Icon::EdgeMode, "Edge mode"),
            (Icon::PolygonMode, "Polygon mode"),
            (Icon::WeightPaint, "Weight paint"),
        ]
        .into_iter()
        .enumerate()
        {
            let active = match index {
                0 => !self.mesh_edit && !self.weight_paint,
                4 => self.weight_paint,
                _ => {
                    self.mesh_edit
                        && matches!(
                            (index, self.mesh_select_mode),
                            (1, MeshSelectMode::Points)
                                | (2, MeshSelectMode::Edges)
                                | (3, MeshSelectMode::Polygons)
                        )
                }
            };
            let hit = Rect::from_min_size(
                strip.left_top() + Vec2::new(3.0, 4.0 + index as f32 * 27.0),
                Vec2::splat(24.0),
            );
            let interactive = match index {
                0 => true,
                4 => weight_ready,
                _ => mesh_ready,
            };
            let item = ui.interact(
                hit,
                ui.id().with(("viewport_mode", index)),
                if interactive {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );
            if active {
                painter.rect_filled(hit, 3.0, theme::PRESS);
                painter.rect_stroke(
                    hit,
                    3.0,
                    egui::Stroke::new(1.0, theme::SELECTION),
                    egui::StrokeKind::Inside,
                );
            } else if item.hovered() {
                painter.rect_filled(hit, 3.0, theme::HOVER);
            }
            icons::paint(
                &painter,
                hit.shrink(3.5),
                icon,
                if !interactive {
                    theme::DISABLED
                } else if active {
                    theme::SELECTION
                } else {
                    theme::TEXT
                },
            );
            if item.clicked() {
                if index == 0 {
                    self.set_weight_paint(false);
                    self.leave_mesh_edit();
                } else if index == 4 {
                    let enable = !self.weight_paint;
                    self.set_weight_paint(enable);
                } else {
                    self.set_weight_paint(false);
                    let mode = match index {
                        1 => MeshSelectMode::Points,
                        2 => MeshSelectMode::Edges,
                        _ => MeshSelectMode::Polygons,
                    };
                    self.set_mesh_mode(mode);
                }
            }
            mode_hovered |= item.hovered();
            if interactive {
                item.on_hover_text(label);
            } else {
                item.on_hover_text(if index == 4 {
                    "Bind a modifier-free mesh to joints to paint weights"
                } else {
                    "Select a mesh object to edit components"
                });
            }
        }
        if !mode_hovered {
            self.handle_viewport_input(ui, &response, &evaluated, rect, pixel_w, pixel_h);
        }
        self.draw_viewport_overlay(ui, rect, &evaluated, pixel_w, pixel_h);
    }

    fn build_frame(&self, evaluated: &EvaluatedScene) -> FrameData {
        #[cfg(test)]
        {
            let visible: Vec<NodeId> = evaluated.mesh_nodes().map(|node| node.id).collect();
            let selected = visible
                .iter()
                .copied()
                .filter(|id| self.selection.contains(*id))
                .collect();
            ui_test_support::record_viewport(visible, selected);
        }
        let aspect = (self.last_viewport_size.x / self.last_viewport_size.y.max(1.0)).max(0.01);
        let world = self.orbit.world();
        let camera = self.orbit.camera();
        let view_proj = camera.view_projection(world, aspect);
        let mut triangles = Vec::new();
        let mut lines = Vec::new();
        let light_dir = Vec3::new(0.42, 0.82, 0.38).normalize();
        let mut material_colors: HashMap<MaterialId, Vec3> = HashMap::new();

        if matches!(
            self.shading,
            ShadingMode::Shaded | ShadingMode::ShadedWireframe
        ) {
            for node in evaluated.mesh_nodes() {
                let Some(mesh) = node.mesh() else {
                    continue;
                };
                let normal_matrix = glam::Mat3::from_mat4(node.world).inverse().transpose();
                for tri in mesh.render_triangles(lc_geom::CREASE_ANGLE_DEG) {
                    let mat_id = node.material_for_slot(tri.material_slot).unwrap_or(0);
                    let base = *material_colors.entry(mat_id).or_insert_with(|| {
                        self.doc
                            .materials
                            .get(mat_id)
                            .map(|m| m.preview_color(&self.texture_cache))
                            .unwrap_or(Vec3::splat(0.65))
                    });
                    let color = base;
                    for v in &tri.v {
                        let p = node.world.transform_point3(v.position);
                        let n = (normal_matrix * v.normal).normalize_or(Vec3::Y);
                        triangles.push(Vertex::new(p, n, color));
                    }
                }
            }
        }

        if self.show_grid {
            let extent = 20i32;
            for i in -extent..=extent {
                let f = i as f32;
                let major = i % 5 == 0;
                let color = if i == 0 {
                    Vec3::new(0.25, 0.27, 0.30)
                } else if major {
                    Vec3::new(0.16, 0.18, 0.21)
                } else {
                    Vec3::new(0.09, 0.10, 0.12)
                };
                lines.push(Vertex::new(
                    Vec3::new(-extent as f32, 0.0, f),
                    Vec3::Y,
                    color,
                ));
                lines.push(Vertex::new(
                    Vec3::new(extent as f32, 0.0, f),
                    Vec3::Y,
                    color,
                ));
                lines.push(Vertex::new(
                    Vec3::new(f, 0.0, -extent as f32),
                    Vec3::Y,
                    color,
                ));
                lines.push(Vertex::new(
                    Vec3::new(f, 0.0, extent as f32),
                    Vec3::Y,
                    color,
                ));
            }
            // Coloured origin axes.
            lines.push(Vertex::new(
                Vec3::ZERO,
                Vec3::Y,
                Vec3::new(0.75, 0.22, 0.22),
            ));
            lines.push(Vertex::new(
                Vec3::X * 1.5,
                Vec3::Y,
                Vec3::new(0.75, 0.22, 0.22),
            ));
            lines.push(Vertex::new(
                Vec3::ZERO,
                Vec3::Y,
                Vec3::new(0.22, 0.65, 0.32),
            ));
            lines.push(Vertex::new(
                Vec3::Y * 1.5,
                Vec3::Y,
                Vec3::new(0.22, 0.65, 0.32),
            ));
            lines.push(Vertex::new(
                Vec3::ZERO,
                Vec3::Y,
                Vec3::new(0.27, 0.42, 0.85),
            ));
            lines.push(Vertex::new(
                Vec3::Z * 1.5,
                Vec3::Y,
                Vec3::new(0.27, 0.42, 0.85),
            ));
        }

        for node in &evaluated.nodes {
            if !node.rendered {
                continue;
            }
            let Some(spline) = node.spline() else {
                continue;
            };
            let selected = self.selection.contains(node.id);
            let color = if selected {
                Vec3::splat(0.97)
            } else {
                Vec3::new(0.80, 0.62, 0.20)
            };
            let sampled = spline.sample();
            for pair in sampled.windows(2) {
                lines.push(Vertex::new(
                    node.world.transform_point3(pair[0]),
                    Vec3::Y,
                    color,
                ));
                lines.push(Vertex::new(
                    node.world.transform_point3(pair[1]),
                    Vec3::Y,
                    color,
                ));
            }
            if spline.closed && sampled.len() > 2 {
                lines.push(Vertex::new(
                    node.world.transform_point3(*sampled.last().unwrap()),
                    Vec3::Y,
                    color,
                ));
                lines.push(Vertex::new(
                    node.world.transform_point3(sampled[0]),
                    Vec3::Y,
                    color,
                ));
            }
            for point in &spline.points {
                let p = node.world.transform_point3(point.position);
                let size = 0.07;
                for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                    lines.push(Vertex::new(p - axis * size, Vec3::Y, color));
                    lines.push(Vertex::new(p + axis * size, Vec3::Y, color));
                }
            }
        }

        let wireframe = matches!(
            self.shading,
            ShadingMode::Wireframe | ShadingMode::ShadedWireframe
        );
        let selected_overlay = !wireframe && !self.selection.is_empty();
        if wireframe || selected_overlay {
            for node in evaluated.mesh_nodes() {
                let Some(mesh) = node.mesh() else {
                    continue;
                };

                let selected = self.selection.contains(node.id);
                if selected_overlay && !selected {
                    continue;
                }
                let color = if selected {
                    Vec3::splat(0.98)
                } else {
                    Vec3::new(0.07, 0.08, 0.09)
                };
                for edge in mesh.edges() {
                    let a = mesh.positions.get(edge.a as usize).copied();
                    let b = mesh.positions.get(edge.b as usize).copied();
                    if let (Some(a), Some(b)) = (a, b) {
                        lines.push(Vertex::new(node.world.transform_point3(a), Vec3::Y, color));
                        lines.push(Vertex::new(node.world.transform_point3(b), Vec3::Y, color));
                    }
                }
            }
        }

        if self.show_helpers {
            for light in &evaluated.lights {
                let pos = light.world.w_axis.truncate();
                let color = light.light.color.normalize_or(Vec3::ONE);
                let size = 0.22;
                for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                    lines.push(Vertex::new(pos - axis * size, Vec3::Y, color));
                    lines.push(Vertex::new(pos + axis * size, Vec3::Y, color));
                }
            }
            for node in &evaluated.nodes {
                if let NodeKind::Camera { .. } = &self
                    .doc
                    .scene
                    .get(node.id)
                    .map(|n| n.kind.clone())
                    .unwrap_or(NodeKind::Null)
                {
                    let eye = node.world.w_axis.truncate();
                    let target = eye + lc_scene::Camera::forward(node.world) * 1.1;
                    let color = Vec3::new(0.95, 0.8, 0.28);
                    lines.push(Vertex::new(eye, Vec3::Y, color));
                    lines.push(Vertex::new(target, Vec3::Y, color));
                    let (right, up) =
                        lc_core::orthonormal_basis(lc_scene::Camera::forward(node.world));
                    let a = target + right * 0.35 + up * 0.22;
                    let b = target - right * 0.35 + up * 0.22;
                    let c = target - right * 0.35 - up * 0.22;
                    let d = target + right * 0.35 - up * 0.22;
                    for (p, q) in [
                        (a, b),
                        (b, c),
                        (c, d),
                        (d, a),
                        (eye, a),
                        (eye, b),
                        (eye, c),
                        (eye, d),
                    ] {
                        lines.push(Vertex::new(p, Vec3::Y, color));
                        lines.push(Vertex::new(q, Vec3::Y, color));
                    }
                }
            }
        }

        FrameData {
            triangles,
            lines,
            view_proj,
            eye: self.orbit.eye(),
            light_dir,
        }
    }

    fn handle_viewport_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        evaluated: &EvaluatedScene,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
    ) {
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.orbit.zoom(scroll);
                self.view_label = "Viewport".into();
            }
        }
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let modifiers = ui.input(|i| i.modifiers);
        if modifiers.alt {
            let dragged = [
                egui::PointerButton::Primary,
                egui::PointerButton::Middle,
                egui::PointerButton::Secondary,
            ]
            .into_iter()
            .find(|button| response.dragged_by(*button));
            if let Some(action) = dragged.and_then(|button| alt_navigation(modifiers, button)) {
                let delta = ui.input(|i| i.pointer.delta());
                match action {
                    ViewportNav::Orbit => self.orbit.orbit(delta),
                    ViewportNav::Pan => self.orbit.pan(delta),
                }
                self.view_label = "Viewport".into();
            }
        } else if response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
        {
            let delta = ui.input(|i| i.pointer.delta());
            if modifiers.shift || response.dragged_by(egui::PointerButton::Secondary) {
                self.orbit.pan(delta);
            } else {
                self.orbit.orbit(delta);
            }
            self.view_label = "Viewport".into();
        }

        if !modifiers.alt && self.weight_paint {
            let painting = response.dragged_by(egui::PointerButton::Primary)
                || response.clicked_by(egui::PointerButton::Primary);
            if response.drag_started_by(egui::PointerButton::Primary)
                || response.clicked_by(egui::PointerButton::Primary)
            {
                self.begin_weight_stroke();
            }
            if painting {
                if let Some(pointer) = pointer {
                    self.paint_weight_brush(evaluated, rect, pixel_w, pixel_h, pointer);
                }
            }
            if response.drag_stopped_by(egui::PointerButton::Primary)
                || response.clicked_by(egui::PointerButton::Primary)
            {
                self.commit_pending();
            }
            return;
        }

        let drag_started = !modifiers.alt && response.drag_started_by(egui::PointerButton::Primary);
        if drag_started {
            if let Some(pointer) = pointer {
                self.gizmo_drag = self.gizmo_hit(evaluated, rect, pixel_w, pixel_h, pointer);
                if self.gizmo_drag.is_none() && !self.mesh_edit {
                    let start = ui.input(|i| i.pointer.press_origin()).unwrap_or(pointer);
                    self.box_select = Some(BoxSelect {
                        start,
                        current: pointer,
                        base: self.selection.ids().to_vec(),
                        additive: modifiers.shift || modifiers.command,
                    });
                }
            }
        }
        if let Some(mut drag) = self.gizmo_drag.take() {
            if response.dragged_by(egui::PointerButton::Primary) {
                if let Some(pointer) = pointer {
                    let delta = pointer - drag.start_pointer;
                    if drag.screen_len > 1.0 {
                        let amount = delta.dot(drag.screen_dir) / drag.screen_len;
                        self.apply_gizmo_drag(&mut drag, amount);
                    }
                }
            }
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                self.commit_pending();
            } else {
                self.gizmo_drag = Some(drag);
            }
            return;
        }

        if let Some(mut marquee) = self.box_select.take() {
            if let Some(pointer) = pointer {
                marquee.current = pointer;
            }
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                self.finish_box_select(&marquee, evaluated, rect, pixel_w, pixel_h);
            } else {
                self.box_select = Some(marquee);
            }
            return;
        }

        if response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pointer) = pointer {
                if self.mesh_edit {
                    if let Some((id, mesh)) = self.selected_editable_mesh() {
                        if let Some(node) = evaluated.node(id) {
                            if let Some(hit) = self.pick_mesh_component(
                                &mesh, node.world, rect, pixel_w, pixel_h, pointer,
                            ) {
                                self.toggle_mesh_selection(hit, modifiers.shift);
                                self.status = format!(
                                    "{} of {} selected",
                                    self.mesh_selected.len(),
                                    self.mesh_select_mode.label().to_lowercase()
                                );
                            } else if !modifiers.shift {
                                self.clear_mesh_component_selection();
                            }
                        }
                    }
                    return;
                }
                if let Some(id) = self.pick(evaluated, rect, pixel_w, pixel_h, pointer) {
                    if modifiers.shift || modifiers.command {
                        self.selection.toggle(id);
                    } else {
                        self.selection.select_one(id);
                    }
                    self.sync_selection_material();
                    self.status = self.selection_status();
                } else if !(modifiers.shift || modifiers.command) {
                    self.selection.clear();
                }
            }
        }
    }

    fn select_all_objects(&mut self) {
        self.selection.set(self.doc.scene.depth_first());
        self.clear_mesh_component_selection();
        self.sync_selection_material();
        self.status = self.selection_status();
    }

    fn deselect_all_objects(&mut self) {
        self.selection.clear();
        self.clear_mesh_component_selection();
        self.status = "Selection cleared".into();
    }

    fn selection_status(&self) -> String {
        match self.selection.len() {
            0 => "Selection cleared".into(),
            1 => self
                .selection
                .primary()
                .and_then(|id| self.doc.scene.get(id))
                .map(|node| format!("Selected {}", node.name))
                .unwrap_or_else(|| "Selected object".into()),
            count => format!("Selected {count} objects"),
        }
    }

    fn finish_box_select(
        &mut self,
        marquee: &BoxSelect,
        evaluated: &EvaluatedScene,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
    ) {
        let area = Rect::from_two_pos(marquee.start, marquee.current);
        if area.width() < 4.0 && area.height() < 4.0 {
            return;
        }
        let mut hits = Vec::new();
        for node in &evaluated.nodes {
            if !node.visible {
                continue;
            }
            if let Some(bounds) = self.node_screen_rect(node, rect, pixel_w, pixel_h) {
                if area.intersects(bounds) {
                    hits.push(node.id);
                }
            }
        }
        if marquee.additive {
            self.selection.set(marquee.base.iter().copied().chain(hits));
        } else {
            self.selection.set(hits);
        }
        self.sync_selection_material();
        self.status = self.selection_status();
    }

    fn node_screen_rect(
        &self,
        node: &lc_scene::EvaluatedNode,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
    ) -> Option<Rect> {
        let points: Vec<Vec3> = if let Some(mesh) = node.mesh() {
            let bounds = mesh.bounds();
            if !bounds.is_valid() {
                return None;
            }
            let corners = [
                Vec3::new(bounds.min.x, bounds.min.y, bounds.min.z),
                Vec3::new(bounds.max.x, bounds.min.y, bounds.min.z),
                Vec3::new(bounds.min.x, bounds.max.y, bounds.min.z),
                Vec3::new(bounds.max.x, bounds.max.y, bounds.min.z),
                Vec3::new(bounds.min.x, bounds.min.y, bounds.max.z),
                Vec3::new(bounds.max.x, bounds.min.y, bounds.max.z),
                Vec3::new(bounds.min.x, bounds.max.y, bounds.max.z),
                Vec3::new(bounds.max.x, bounds.max.y, bounds.max.z),
            ];
            corners
                .into_iter()
                .map(|point| node.world.transform_point3(point))
                .collect()
        } else {
            vec![node.world.w_axis.truncate()]
        };
        let mut min = Pos2::new(f32::INFINITY, f32::INFINITY);
        let mut max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut any = false;
        for point in points {
            let Some(screen) = self.project(point, rect, pixel_w, pixel_h) else {
                continue;
            };
            any = true;
            min = Pos2::new(min.x.min(screen.x), min.y.min(screen.y));
            max = Pos2::new(max.x.max(screen.x), max.y.max(screen.y));
        }
        any.then(|| Rect::from_min_max(min, max).expand(3.0))
    }

    fn gizmo_hit(
        &self,
        evaluated: &EvaluatedScene,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
        pointer: Pos2,
    ) -> Option<GizmoDrag> {
        if self.mesh_edit || self.weight_paint {
            return None;
        }
        if self.gizmo == GizmoMode::Off {
            return None;
        }
        let roots = self.selection_transform_roots();
        if roots.is_empty() {
            return None;
        }
        let origin = self.selection_pivot(evaluated)?;
        let axis_node = self
            .selection
            .primary()
            .and_then(|id| evaluated.node(id))
            .or_else(|| roots.first().and_then(|id| evaluated.node(*id)))?;
        let scale = (self.orbit.distance * 0.16).clamp(0.3, 6.0);
        let axes = self.gizmo_axes(axis_node.world);
        let origin_screen = self.project(origin, rect, pixel_w, pixel_h)?;
        let mut best: Option<(usize, f32, Pos2, Vec2)> = None;
        for (i, axis) in axes.iter().enumerate() {
            let end = origin + *axis * scale;
            let Some(end_screen) = self.project(end, rect, pixel_w, pixel_h) else {
                continue;
            };
            let dir = end_screen - origin_screen;
            let len = dir.length();
            if len < 4.0 {
                continue;
            }
            let dir_n = dir / len;
            let rel = pointer - origin_screen;
            let t = (rel.dot(dir_n) / len).clamp(0.0, 1.0);
            let closest = origin_screen + dir * t;
            let dist = (pointer - closest).length();
            if dist < 14.0 && t > 0.15 && best.is_none_or(|(_, d, _, _)| dist < d) {
                best = Some((i, dist, end_screen, dir_n));
            }
        }
        best.map(|(axis, _, _, screen_dir)| GizmoDrag {
            nodes: roots
                .into_iter()
                .filter_map(|id| {
                    let node = self.doc.scene.get(id)?;
                    Some((id, node.transform))
                })
                .collect(),
            pivot: origin,
            mode: self.gizmo,
            axis,
            start_pointer: pointer,
            screen_dir,
            screen_len: {
                let end = origin + axes[axis] * scale;
                self.project(end, rect, pixel_w, pixel_h)
                    .map(|p| (p - origin_screen).length())
                    .unwrap_or(0.0)
            },
            world_dir: axes[axis],
        })
    }

    fn gizmo_axes(&self, world: Mat4) -> [Vec3; 3] {
        if self.gizmo_world {
            [Vec3::X, Vec3::Y, Vec3::Z]
        } else {
            [
                world.x_axis.truncate().normalize_or(Vec3::X),
                world.y_axis.truncate().normalize_or(Vec3::Y),
                world.z_axis.truncate().normalize_or(Vec3::Z),
            ]
        }
    }

    fn apply_gizmo_drag(&mut self, drag: &mut GizmoDrag, amount: f32) {
        if drag.mode == GizmoMode::Off {
            return;
        }
        let scale = (self.orbit.distance * 0.16).clamp(0.3, 6.0);
        let axis = drag.world_dir.normalize_or(Vec3::Y);
        let world_delta = match drag.mode {
            GizmoMode::Translate => Mat4::from_translation(axis * (amount * scale)),
            GizmoMode::Rotate => {
                let rotation = Mat4::from_quat(Quat::from_axis_angle(axis, amount * 1.8));
                Mat4::from_translation(drag.pivot) * rotation * Mat4::from_translation(-drag.pivot)
            }
            GizmoMode::Scale => {
                let factor = (1.0 + amount * 1.2).clamp(0.02, 100.0);
                let spread = factor - 1.0;
                let cols = [
                    glam::Vec4::new(
                        1.0 + spread * axis.x * axis.x,
                        spread * axis.x * axis.y,
                        spread * axis.x * axis.z,
                        0.0,
                    ),
                    glam::Vec4::new(
                        spread * axis.y * axis.x,
                        1.0 + spread * axis.y * axis.y,
                        spread * axis.y * axis.z,
                        0.0,
                    ),
                    glam::Vec4::new(
                        spread * axis.z * axis.x,
                        spread * axis.z * axis.y,
                        1.0 + spread * axis.z * axis.z,
                        0.0,
                    ),
                    glam::Vec4::new(0.0, 0.0, 0.0, 1.0),
                ];
                Mat4::from_translation(drag.pivot)
                    * Mat4::from_cols(cols[0], cols[1], cols[2], cols[3])
                    * Mat4::from_translation(-drag.pivot)
            }
            GizmoMode::Off => return,
        };
        let mut updates = Vec::new();
        for (id, start) in drag.nodes.clone() {
            let Some(node) = self.doc.scene.get(id) else {
                continue;
            };
            if node.locked {
                continue;
            }
            let parent_world = node
                .parent
                .map(|p| self.doc.scene.world_matrix_plain(p))
                .unwrap_or(Mat4::IDENTITY);
            let transform = match drag.mode {
                GizmoMode::Translate => {
                    let local_delta = parent_world
                        .inverse()
                        .transform_vector3(axis * (amount * scale));
                    Transform {
                        translation: start.translation + local_delta,
                        ..start
                    }
                }
                GizmoMode::Rotate | GizmoMode::Scale => {
                    let start_world = parent_world * start.matrix();
                    let new_local = parent_world.inverse() * world_delta * start_world;
                    let (scale, rotation, translation) = new_local.to_scale_rotation_translation();
                    Transform::new(translation, rotation, scale)
                }
                GizmoMode::Off => start,
            };
            updates.push((id, start, transform));
        }
        if updates.is_empty() {
            return;
        }
        let pending = self.pending_transform.get_or_insert_with(Vec::new);
        for (id, start, transform) in updates {
            if !pending.iter().any(|(pending_id, _)| *pending_id == id) {
                pending.push((id, start));
            }
            if let Some(node) = self.doc.scene.get_mut(id) {
                node.transform = transform;
            }
        }
    }

    fn pick(
        &self,
        evaluated: &EvaluatedScene,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
        pointer: Pos2,
    ) -> Option<NodeId> {
        let pixel = V2::new(
            (pointer.x - rect.min.x) / rect.width().max(1.0) * pixel_w as f32,
            (pointer.y - rect.min.y) / rect.height().max(1.0) * pixel_h as f32,
        );
        let world = self.orbit.world();
        let camera = self.orbit.camera();
        let mut rng = Pcg32::new(1);
        let ray = lc_render::RenderCamera::new(world, camera).ray(
            pixel,
            V2::new(pixel_w as f32, pixel_h as f32),
            &mut rng,
        );
        let mut best: Option<(f32, NodeId)> = None;
        for node in evaluated.mesh_nodes() {
            let Some(mesh) = node.mesh() else {
                continue;
            };
            let inv = node.world.inverse();
            let origin = inv.transform_point3(ray.origin);
            let dir = inv.transform_vector3(ray.dir);
            let local_ray = lc_core::Ray::new(origin, dir);
            for tri in mesh.render_triangles(lc_geom::CREASE_ANGLE_DEG) {
                if let Some((t, _, _)) = ray_triangle(
                    &local_ray,
                    tri.v[0].position,
                    tri.v[1].position,
                    tri.v[2].position,
                ) {
                    if t > 1e-5 && best.map(|(b, _)| t < b).unwrap_or(true) {
                        best = Some((t, node.id));
                    }
                }
            }
        }
        best.map(|(_, id)| id)
    }

    fn project(&self, point: Vec3, rect: Rect, pixel_w: u32, pixel_h: u32) -> Option<Pos2> {
        let camera = lc_render::RenderCamera::new(self.orbit.world(), self.orbit.camera());
        let p = camera.project(point, V2::new(pixel_w as f32, pixel_h as f32))?;
        Some(Pos2::new(
            rect.min.x + p.x / pixel_w as f32 * rect.width(),
            rect.min.y + p.y / pixel_h as f32 * rect.height(),
        ))
    }

    fn draw_weight_paint_overlay(
        &self,
        painter: &egui::Painter,
        ui: &egui::Ui,
        rect: Rect,
        evaluated: &EvaluatedScene,
        pixel_w: u32,
        pixel_h: u32,
    ) {
        let Some(id) = self.selection.primary() else {
            return;
        };
        let Some(node) = evaluated.node(id) else {
            return;
        };
        let Some(mesh) = node.mesh() else {
            return;
        };
        let Some(skin) = self.doc.scene.get(id).and_then(|node| node.skin.as_ref()) else {
            return;
        };
        let Some(joint) = self.active_weight_joint() else {
            return;
        };
        for (vertex, position) in mesh.positions.iter().enumerate() {
            let Some(screen) = self.project(
                node.world.transform_point3(*position),
                rect,
                pixel_w,
                pixel_h,
            ) else {
                continue;
            };
            let weight = skin.influence(vertex, joint).clamp(0.0, 1.0);
            let color = if weight > 0.001 {
                Color32::from_rgba_unmultiplied(245, 135, 35, (55.0 + 190.0 * weight) as u8)
            } else {
                Color32::from_rgba_unmultiplied(190, 195, 200, 80)
            };
            painter.circle_filled(screen, 2.4, color);
        }
        if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
            if rect.contains(pointer) {
                painter.circle_stroke(
                    pointer,
                    self.weight_brush.radius,
                    egui::Stroke::new(1.4, Color32::from_rgb(245, 160, 50)),
                );
            }
        }
    }

    fn draw_viewport_overlay(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        evaluated: &EvaluatedScene,
        pixel_w: u32,
        pixel_h: u32,
    ) {
        let painter = ui.painter_at(rect);
        if self.weight_paint {
            self.draw_weight_paint_overlay(&painter, ui, rect, evaluated, pixel_w, pixel_h);
            return;
        }
        if self.mesh_edit {
            self.draw_mesh_edit_overlay(ui, rect, evaluated, pixel_w, pixel_h);
            return;
        }
        for id in self.selection.ids() {
            let Some(node) = evaluated.node(*id) else {
                continue;
            };
            if let Some(origin) = self.project(node.world.w_axis.truncate(), rect, pixel_w, pixel_h)
            {
                painter.circle_filled(origin, 2.6, Color32::from_rgb(245, 248, 255));
                painter.circle_stroke(origin, 4.2, egui::Stroke::new(1.2, theme::SELECTION));
            }
        }
        if let Some(origin) = self.selection_pivot(evaluated) {
            let axis_node = self
                .selection
                .primary()
                .and_then(|id| evaluated.node(id))
                .or_else(|| {
                    self.selection
                        .ids()
                        .iter()
                        .find_map(|id| evaluated.node(*id))
                });
            if let (Some(node), Some(origin_screen)) =
                (axis_node, self.project(origin, rect, pixel_w, pixel_h))
            {
                let scale = (self.orbit.distance * 0.16).clamp(0.3, 6.0);
                let axes = self.gizmo_axes(node.world);
                let colors = [
                    Color32::from_rgb(230, 80, 80),
                    Color32::from_rgb(90, 210, 110),
                    Color32::from_rgb(80, 140, 245),
                ];
                if self.gizmo != GizmoMode::Off {
                    for (i, axis) in axes.iter().enumerate() {
                        if let Some(end) =
                            self.project(origin + *axis * scale, rect, pixel_w, pixel_h)
                        {
                            painter.line_segment(
                                [origin_screen, end],
                                egui::Stroke::new(
                                    if self
                                        .gizmo_drag
                                        .as_ref()
                                        .map(|d| d.axis == i)
                                        .unwrap_or(false)
                                    {
                                        4.0
                                    } else {
                                        2.2
                                    },
                                    colors[i],
                                ),
                            );
                            if self.gizmo == GizmoMode::Translate {
                                painter.circle_filled(end, 4.0, colors[i]);
                            }
                            painter.text(
                                end + Vec2::new(4.0, -4.0),
                                egui::Align2::LEFT_BOTTOM,
                                ["X", "Y", "Z"][i],
                                egui::FontId::proportional(11.0),
                                colors[i],
                            );
                        }
                    }
                    if self.gizmo == GizmoMode::Rotate {
                        painter.circle_stroke(
                            origin_screen,
                            38.0,
                            egui::Stroke::new(1.0, Color32::from_rgb(220, 180, 120)),
                        );
                    }
                }
            }
        }
        if let Some(marquee) = &self.box_select {
            let area = Rect::from_two_pos(marquee.start, marquee.current);
            painter.rect_filled(area, 0.0, Color32::from_rgba_unmultiplied(70, 120, 210, 28));
            painter.rect_stroke(
                area,
                0.0,
                egui::Stroke::new(1.0, theme::SELECTION),
                egui::StrokeKind::Inside,
            );
        }
    }
}

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
