//! The LeopardCraft editor: Cheetah-style fixed workspace, viewport,
//! inspectors, material graph, UV view, timeline and background renderer.

use crate::icons::{self, Icon};
use crate::theme;
use crate::viewport::{FrameData, Vertex, ViewportRenderer};
use eframe::egui;
use egui::{Color32, Pos2, Rect, Sense, Vec2};
use glam::{Mat4, Quat, Vec2 as V2, Vec3};
use lc_anim::{AnimProperty, Interpolation, Playback};
use lc_core::{look_at_quat, ray_triangle, Pcg32, Transform};
use lc_geom::Mesh;
use lc_render::{PathTracer, RenderScene};
use lc_scene::{
    AddNodeCmd, Camera, DeleteNodeCmd, Document, Environment, EvaluatedScene, GeometrySource,
    History, Light, LightKind, Modifier, Node, NodeId, NodeKind, NodeProps, Primitive,
    ProjectionMode, RenameNodeCmd, ReparentCmd, ReplaceDocumentCmd, SetAnimationCmd,
    SetMaterialCmd, SetNodePropsCmd, SetShapeCmd, SetTransformCmd, Shape, Skin, Tag,
};
use lc_shading::{Material, MaterialId, ShaderNodeKind, ShaderValue, TextureCache};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[path = "panels.rs"]
mod panels;
#[path = "panels_more.rs"]
mod panels_more;
#[path = "shell.rs"]
mod shell;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorView {
    Scene3d,
    Uv,
    Nodes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InspectorTab {
    Object,
    Material,
    Render,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowserTab {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum ShadingMode {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum GizmoMode {
    Off,
    Translate,
    Rotate,
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MeshSelectMode {
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
    Bridge,
    AssignMaterial,
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
    selected_name: Option<String>,
}

impl Default for InspectorTab {
    fn default() -> Self {
        Self::Object
    }
}

impl Default for BrowserTab {
    fn default() -> Self {
        Self::Objects
    }
}

impl Default for EditorView {
    fn default() -> Self {
        Self::Scene3d
    }
}

impl Default for ShadingMode {
    fn default() -> Self {
        Self::Shaded
    }
}

impl Default for GizmoMode {
    fn default() -> Self {
        Self::Translate
    }
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

#[derive(Clone, Copy, Debug)]
struct GizmoDrag {
    node: NodeId,
    mode: GizmoMode,
    axis: usize,
    start_pointer: Pos2,
    screen_dir: Vec2,
    screen_len: f32,
    world_dir: Vec3,
    start_transform: Transform,
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
    selected: Option<NodeId>,
    selected_material: Option<MaterialId>,
    selected_shader_node: Option<u32>,
    selected_uv: Option<usize>,
    playback: Playback,
    orbit: OrbitCamera,
    shading: ShadingMode,
    gizmo: GizmoMode,
    gizmo_world: bool,
    show_grid: bool,
    show_helpers: bool,
    viewport: ViewportRenderer,
    texture_cache: TextureCache,
    file_path: Option<PathBuf>,
    dialog: Option<FileDialog>,
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
    pending_transform: Option<(NodeId, Transform)>,
    pending_shape: Option<(NodeId, Shape)>,
    pending_material: Option<(MaterialId, Material)>,
    gizmo_drag: Option<GizmoDrag>,
    mesh_edit: bool,
    mesh_select_mode: MeshSelectMode,
    mesh_selected: Vec<u32>,
    mesh_transform: MeshTransformMode,
    mesh_space: MeshTransformSpace,
    mesh_axis: usize,
    mesh_value: f32,
    mesh_depth: f32,
    mesh_offset: f32,
    mesh_extrude_individual: bool,
    last_viewport_rect: Rect,
    last_viewport_size: Vec2,
}

impl LeopardApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::theme::install(&cc.egui_ctx);
        let doc = Document::demo();
        let fps = doc.settings.fps.max(1.0);
        let mut app = Self {
            playback: Playback {
                start: doc.settings.frame_start / fps,
                end: doc.settings.frame_end / fps,
                fps,
                looping: true,
                ..Default::default()
            },
            selected: doc.scene.find_by_name("Cube"),
            selected_material: Some(0),
            selected_shader_node: None,
            selected_uv: None,
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
            file_path: None,
            dialog: None,
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
            mesh_edit: false,
            mesh_select_mode: MeshSelectMode::Polygons,
            mesh_selected: Vec::new(),
            mesh_transform: MeshTransformMode::Move,
            mesh_space: MeshTransformSpace::World,
            mesh_axis: 1,
            mesh_value: 0.25,
            mesh_depth: 0.05,
            mesh_offset: 0.0,
            mesh_extrude_individual: false,
            last_viewport_rect: Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)),
            last_viewport_size: Vec2::splat(100.0),
        };
        if let Some(storage) = cc.storage {
            if let Some(state) = eframe::get_value::<WorkspaceState>(storage, WORKSPACE_STORAGE_KEY)
            {
                app.apply_workspace_state(state);
            }
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
                .selected
                .and_then(|id| self.doc.scene.get(id))
                .map(|node| node.name.clone()),
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
        self.selected = state
            .selected_name
            .as_deref()
            .and_then(|name| self.doc.scene.find_by_name(name));
        self.sync_selection_material();
    }

    pub fn load_scene(&mut self, doc: Document, path: Option<PathBuf>) {
        if let Some(active) = self.active_render.take() {
            active.cancel.store(true, Ordering::Relaxed);
        }
        self.playback.fps = doc.settings.fps.max(1.0);
        self.playback.start = doc.settings.frame_start / self.playback.fps;
        self.playback.end = doc.settings.frame_end / self.playback.fps;
        self.playback.time = self.playback.start;
        self.selected = doc.scene.find_by_name("Cube").or_else(|| {
            doc.scene
                .depth_first()
                .into_iter()
                .find(|id| doc.scene.get(*id).map(|n| n.is_mesh()).unwrap_or(false))
        });
        self.doc = doc;
        self.history.clear();
        self.file_path = path;
        self.pending_transform = None;
        self.pending_shape = None;
        self.pending_material = None;
        self.mesh_edit = false;
        self.mesh_selected.clear();
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
        if let Some(id) = self.selected {
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
        let Some(evaluated) = self
            .selected
            .and_then(|id| self.evaluate().node(id).cloned())
        else {
            self.orbit.frame(self.scene_bounds());
            return;
        };
        if let Some(mesh) = evaluated.mesh() {
            self.orbit.frame(mesh.bounds().transformed(evaluated.world));
        } else {
            self.orbit.target = evaluated.world.w_axis.truncate();
        }
    }

    fn add_node(&mut self, node: Node, parent: Option<NodeId>) {
        let id = node.id;
        let cmd = AddNodeCmd::new(&self.doc, node, parent);
        self.history.execute(&mut self.doc, Box::new(cmd));
        self.selected = Some(id);
        self.mesh_edit = false;
        self.mesh_selected.clear();
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
        if let Some(id) = self.selected {
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
        let Some(id) = self.selected else {
            return;
        };
        let cmd = DeleteNodeCmd::new(&self.doc, id);
        self.history.execute(&mut self.doc, Box::new(cmd));
        self.selected = None;
        self.mesh_edit = false;
        self.mesh_selected.clear();
        self.status = "Deleted object".into();
    }

    fn duplicate_selected(&mut self) {
        let Some(id) = self.selected else {
            return;
        };
        let before = self.doc.clone();
        if let Some(new_id) = self.doc.scene.duplicate_subtree(id) {
            let after = self.doc.clone();
            self.history.execute(
                &mut self.doc,
                Box::new(lc_scene::ReplaceDocumentCmd {
                    label: "Duplicate".into(),
                    before,
                    after,
                }),
            );
            self.selected = Some(new_id);
            self.status = "Duplicated subtree".into();
        }
    }

    fn undo(&mut self) {
        if let Some(label) = self.history.undo(&mut self.doc) {
            self.status = format!("Undo: {label}");
            self.invalidate_pending();
        }
    }

    fn redo(&mut self) {
        if let Some(label) = self.history.redo(&mut self.doc) {
            self.status = format!("Redo: {label}");
            self.invalidate_pending();
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
        self.status = "New scene from studio template".into();
    }

    fn open_file(&mut self, path: PathBuf) {
        match lc_io::import(&path) {
            Ok(doc) => {
                let name = path.display().to_string();
                self.load_scene(doc, Some(path));
                self.status = format!("Opened {name}");
            }
            Err(e) => self.status = format!("Open failed: {e}"),
        }
    }

    fn import_file(&mut self, path: PathBuf) {
        match lc_io::import(&path) {
            Ok(mut doc) => {
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
                self.selected = self.doc.scene.roots.first().copied();
                self.playback.start = self.doc.settings.frame_start;
                self.playback.end = self.doc.settings.frame_end;
                self.playback.fps = self.doc.settings.fps;
                self.status = format!("Imported {}", path.display());
            }
            Err(e) => self.status = format!("Import failed: {e}"),
        }
    }

    fn save_file(&mut self, path: PathBuf) {
        match lc_io::export(&path, &self.doc) {
            Ok(()) => {
                if path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("lcscene"))
                    .unwrap_or(false)
                {
                    self.file_path = Some(path.clone());
                }
                self.status = format!("Saved {}", path.display());
            }
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }

    fn apply_file_dialog(&mut self, mode: FileMode, text: String) {
        let path = if text.trim().is_empty() {
            self.status = "No file path entered".into();
            return;
        } else {
            PathBuf::from(text.trim())
        };
        match mode {
            FileMode::Open => self.open_file(path),
            FileMode::Import => self.import_file(path),
            FileMode::SaveAs | FileMode::Export => self.save_file(path),
        }
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
            None => return,
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
            .file_path
            .as_ref()
            .and_then(|p| p.parent())
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
                    .flat_map(|p| p)
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
        if let Some((id, before)) = self.pending_transform.take() {
            if let Some(after) = self.doc.scene.get(id).map(|n| n.transform) {
                if after != before {
                    self.history.execute(
                        &mut self.doc,
                        Box::new(SetTransformCmd { id, before, after }),
                    );
                }
            }
        }
        if let Some((id, before)) = self.pending_shape.take() {
            if let Some(node) = self.doc.scene.get(id) {
                let after = Shape::of(node);
                if after.kind != before.kind
                    || after.modifiers != before.modifiers
                    || after.skin != before.skin
                {
                    self.history
                        .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
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
        {
            if self.pending_shape.is_none() {
                self.pending_shape = Some((id, before_shape.clone()));
            }
            if let Some(node) = self.doc.scene.get_mut(id) {
                node.kind = edited.kind.clone();
                node.modifiers = edited.modifiers.clone();
                node.skin = edited.skin.clone();
            }
        }
        if edited.transform != original.transform {
            if self.pending_transform.is_none() {
                self.pending_transform = Some((id, current.transform));
            }
            if let Some(node) = self.doc.scene.get_mut(id) {
                node.transform = edited.transform;
            }
        }
        self.sync_selection_material();
    }

    fn set_shape_now(&mut self, id: NodeId, kind: NodeKind) {
        let Some(node) = self.doc.scene.get(id) else {
            return;
        };
        let before = Shape::of(node);
        let mut after = before.clone();
        after.kind = kind;
        if before.kind != after.kind {
            self.history
                .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
        }
    }

    pub(super) fn selected_is_mesh(&self) -> bool {
        self.selected
            .and_then(|id| self.doc.scene.get(id))
            .map(|node| node.is_mesh())
            .unwrap_or(false)
    }

    fn selected_editable_mesh(&self) -> Option<(NodeId, Mesh)> {
        let id = self.selected?;
        let node = self.doc.scene.get(id)?;
        match &node.kind {
            NodeKind::Mesh {
                geometry: GeometrySource::Editable { mesh, .. },
            } => Some((id, mesh.clone())),
            _ => None,
        }
    }

    fn make_selected_editable(&mut self) {
        let Some(id) = self.selected else {
            return;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return;
        };
        let mut kind = node.kind.clone();
        if let NodeKind::Mesh { geometry } = &mut kind {
            if geometry.make_editable() {
                self.set_shape_now(id, kind);
                self.status = "Converted primitive to editable mesh".into();
            }
        }
    }

    fn apply_uv_op(&mut self, op: UvOp) {
        let Some((id, mut mesh)) = self.selected_editable_mesh() else {
            self.status = "Select an editable mesh first".into();
            return;
        };
        let before = self.doc.scene.get(id).map(Shape::of);
        match op {
            UvOp::Planar => lc_geom::planar_projection(&mut mesh, 1, 1.0),
            UvOp::Box => lc_geom::box_projection(&mut mesh, 1.0),
            UvOp::Cylindrical => lc_geom::cylindrical_projection(&mut mesh, 1.0, 1.0),
            UvOp::Spherical => lc_geom::spherical_projection(&mut mesh, 1.0),
            UvOp::Unwrap => lc_geom::automatic_unwrap(&mut mesh, 45.0, 0.01),
            UvOp::Relax => lc_geom::relax_uvs(&mut mesh, 8, 0.5, true),
            UvOp::Normalize => lc_geom::normalize_uvs(&mut mesh),
        }
        if let Some(before) = before {
            let mut after = before.clone();
            after.kind = match after.kind {
                NodeKind::Mesh { geometry } => NodeKind::Mesh {
                    geometry: geometry.with_mesh(mesh),
                },
                other => other,
            };
            self.history
                .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
            self.status = "Updated UVs".into();
        }
    }

    fn perform_mesh_action(&mut self, action: MeshAction) {
        match action {
            MeshAction::Enter => {
                if self.selected_editable_mesh().is_none() {
                    self.make_selected_editable();
                }
                if self.selected_editable_mesh().is_some() {
                    self.mesh_edit = true;
                    self.mesh_selected.clear();
                    self.status = "Mesh edit mode — click components in the viewport".into();
                } else if self.selected.is_some() {
                    self.status = "Select a mesh object first".into();
                }
            }
            MeshAction::Exit => {
                self.mesh_edit = false;
                self.mesh_selected.clear();
                self.status = "Object mode".into();
            }
            MeshAction::SelectAll => {
                if let Some((_, mesh)) = self.selected_editable_mesh() {
                    self.mesh_selected = match self.mesh_select_mode {
                        MeshSelectMode::Points => (0..mesh.positions.len() as u32).collect(),
                        MeshSelectMode::Edges => (0..mesh.edges().len() as u32).collect(),
                        MeshSelectMode::Polygons => (0..mesh.polygons.len() as u32).collect(),
                    };
                }
            }
            MeshAction::SelectNone => self.mesh_selected.clear(),
            MeshAction::Transform => self.mesh_transform_selection(),
            MeshAction::Extrude => {
                if !self.require_mesh_selection(MeshSelectMode::Polygons, "extrude") {
                    return;
                }
                let faces = self.mesh_selected.clone();
                let distance = self.mesh_value;
                let individual = self.mesh_extrude_individual;
                self.apply_mesh_operation("Extruded faces", move |mesh| {
                    Ok(lc_geom::extrude_faces(mesh, &faces, distance, individual))
                });
            }
            MeshAction::Inset => {
                if !self.require_mesh_selection(MeshSelectMode::Polygons, "inset") {
                    return;
                }
                let faces = self.mesh_selected.clone();
                let amount = self.mesh_value.abs().max(1e-4);
                self.apply_mesh_operation("Inset faces", move |mesh| {
                    Ok(lc_geom::bevel_faces(mesh, &faces, amount, 0.0))
                });
            }
            MeshAction::Bevel => {
                if !self.require_mesh_selection(MeshSelectMode::Polygons, "bevel") {
                    return;
                }
                let faces = self.mesh_selected.clone();
                let amount = self.mesh_value.abs().max(1e-4);
                let depth = self.mesh_depth;
                self.apply_mesh_operation("Bevelled faces", move |mesh| {
                    Ok(lc_geom::bevel_faces(mesh, &faces, amount, depth))
                });
            }
            MeshAction::RingCut => {
                let axis = self.mesh_axis;
                let offset = self.mesh_offset;
                self.apply_mesh_operation("Ring cut", move |mesh| {
                    Ok(lc_geom::ring_cut(mesh, axis, offset))
                });
            }
            MeshAction::Weld => {
                if !self.require_mesh_selection(MeshSelectMode::Points, "weld") {
                    return;
                }
                let points = self.mesh_selected.clone();
                self.apply_mesh_operation("Welded points", move |mesh| {
                    let welded = weld_selected_points(mesh, &points);
                    if welded == *mesh {
                        return Err("select at least two distinct points to weld".into());
                    }
                    Ok(welded)
                });
            }
            MeshAction::Bridge => {
                if !self.require_mesh_selection(MeshSelectMode::Edges, "bridge") {
                    return;
                }
                let edges = self.mesh_selected.clone();
                self.apply_mesh_operation("Bridged edge loops", move |mesh| {
                    let loops = selected_edge_loops(mesh, &edges)?;
                    if loops.len() != 2 {
                        return Err(format!(
                            "select two closed edge loops (found {})",
                            loops.len()
                        ));
                    }
                    lc_geom::bridge_loops(mesh, &loops[0], &loops[1])
                });
            }
            MeshAction::AssignMaterial => self.assign_material_to_selected_faces(),
        }
    }

    fn require_mesh_selection(&mut self, mode: MeshSelectMode, action: &str) -> bool {
        if self.selected_editable_mesh().is_none() {
            self.status = "Select an editable mesh first".into();
            return false;
        }
        if self.mesh_select_mode != mode {
            self.status = format!("Switch to {} selection to {action}", mode.label());
            return false;
        }
        if self.mesh_selected.is_empty() {
            self.status = format!("Select {} first", mode.label().to_lowercase());
            return false;
        }
        true
    }

    fn apply_mesh_operation(
        &mut self,
        message: &str,
        operation: impl FnOnce(&Mesh) -> Result<Mesh, String>,
    ) {
        let Some((id, mesh)) = self.selected_editable_mesh() else {
            self.status = "Select an editable mesh first".into();
            return;
        };
        let Some(before) = self.doc.scene.get(id).map(Shape::of) else {
            return;
        };
        match operation(&mesh) {
            Ok(updated) if updated == mesh => {
                self.status = format!("{message}: no change");
            }
            Ok(updated) => {
                self.commit_mesh_edit(id, before, updated, true, message.to_string());
            }
            Err(error) => self.status = format!("{message} failed: {error}"),
        }
    }

    fn commit_mesh_edit(
        &mut self,
        id: NodeId,
        before: Shape,
        mesh: Mesh,
        clear_selection: bool,
        message: String,
    ) {
        if let Err(errors) = mesh.validate() {
            self.status = format!("Mesh edit rejected: {}", errors.join("; "));
            return;
        }
        let mut after = before.clone();
        after.kind = match after.kind {
            NodeKind::Mesh { geometry } => NodeKind::Mesh {
                geometry: geometry.with_mesh(mesh),
            },
            other => other,
        };
        if before.kind != after.kind {
            self.history
                .execute(&mut self.doc, Box::new(SetShapeCmd { id, before, after }));
        }
        if clear_selection {
            self.mesh_selected.clear();
        }
        self.status = message;
    }

    fn mesh_transform_selection(&mut self) {
        let Some((id, mut mesh)) = self.selected_editable_mesh() else {
            self.status = "Select an editable mesh first".into();
            return;
        };
        let affected = mesh_affected_points(&mesh, self.mesh_select_mode, &self.mesh_selected);
        if affected.is_empty() {
            self.status = "Select mesh components to transform".into();
            return;
        }
        let affected: HashSet<u32> = affected.into_iter().collect();
        let center = affected
            .iter()
            .filter_map(|p| mesh.positions.get(*p as usize).copied())
            .sum::<Vec3>()
            / affected.len().max(1) as f32;
        let node_world = self.doc.scene.world_matrix_plain(id);
        let rotation = node_world.to_scale_rotation_translation().1;
        let local_axis = match self.mesh_space {
            MeshTransformSpace::Local => axis_vector(self.mesh_axis),
            MeshTransformSpace::World => {
                (rotation.inverse() * axis_vector(self.mesh_axis)).normalize_or(Vec3::Y)
            }
            MeshTransformSpace::Normal => {
                let mut normal = Vec3::ZERO;
                for (fi, poly) in mesh.polygons.iter().enumerate() {
                    if poly.corners.iter().any(|c| affected.contains(&c.point)) {
                        normal += mesh.face_normal(fi);
                    }
                }
                normal.normalize_or(Vec3::Y)
            }
        };
        let value = self.mesh_value;
        match self.mesh_transform {
            MeshTransformMode::Move => {
                for &p in &affected {
                    if let Some(position) = mesh.positions.get_mut(p as usize) {
                        *position += local_axis * value;
                    }
                }
            }
            MeshTransformMode::Rotate => {
                let rotation = Quat::from_axis_angle(local_axis, value.to_radians());
                for &p in &affected {
                    if let Some(position) = mesh.positions.get_mut(p as usize) {
                        *position = center + rotation * (*position - center);
                    }
                }
            }
            MeshTransformMode::Scale => {
                for &p in &affected {
                    if let Some(position) = mesh.positions.get_mut(p as usize) {
                        let offset = *position - center;
                        *position = center + offset + local_axis * offset.dot(local_axis) * value;
                    }
                }
            }
        }
        let Some(before) = self.doc.scene.get(id).map(Shape::of) else {
            return;
        };
        self.commit_mesh_edit(
            id,
            before,
            mesh,
            false,
            format!(
                "Transformed selected {}",
                self.mesh_select_mode.label().to_lowercase()
            ),
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
    }

    fn assign_material_to_selected_faces(&mut self) {
        if self.mesh_select_mode != MeshSelectMode::Polygons || self.mesh_selected.is_empty() {
            self.status = "Select polygons before assigning a material".into();
            return;
        }
        let Some(material_id) = self
            .selected_material
            .filter(|material| self.doc.materials.get(*material).is_some())
        else {
            self.status = "Select a material in the Materials tab first".into();
            return;
        };
        let Some((id, mut mesh)) = self.selected_editable_mesh() else {
            self.status = "Select an editable mesh first".into();
            return;
        };
        let before = self.doc.clone();
        let Some(node) = self.doc.scene.get_mut(id) else {
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
        for index in &self.mesh_selected {
            if let Some(poly) = mesh.polygons.get_mut(*index as usize) {
                poly.material_slot = slot;
            }
        }
        let origin = match &node.kind {
            NodeKind::Mesh { geometry } => geometry.origin_copied(),
            _ => None,
        };
        node.kind = NodeKind::Mesh {
            geometry: GeometrySource::Editable { mesh, origin },
        };
        let after = self.doc.clone();
        self.history.execute(
            &mut self.doc,
            Box::new(ReplaceDocumentCmd {
                label: "Assign polygon material".into(),
                before,
                after,
            }),
        );
        self.status = format!("Assigned material {material_id} to selected polygons");
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
                    painter.line_segment(
                        [a, b],
                        egui::Stroke::new(
                            if chosen { 3.0 } else { 1.0 },
                            if chosen {
                                selected_color
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

    fn key_transform_channels(&mut self) {
        let Some(id) = self.selected else {
            return;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return;
        };
        let mut clip = node.animation.clone();
        let t = node.transform;
        let e = t.euler_degrees();
        let time = self.playback.time;
        clip.set_key(
            AnimProperty::TranslationX,
            time,
            t.translation.x,
            Interpolation::Bezier,
        );
        clip.set_key(
            AnimProperty::TranslationY,
            time,
            t.translation.y,
            Interpolation::Bezier,
        );
        clip.set_key(
            AnimProperty::TranslationZ,
            time,
            t.translation.z,
            Interpolation::Bezier,
        );
        clip.set_key(AnimProperty::RotationX, time, e.x, Interpolation::Bezier);
        clip.set_key(AnimProperty::RotationY, time, e.y, Interpolation::Bezier);
        clip.set_key(AnimProperty::RotationZ, time, e.z, Interpolation::Bezier);
        clip.set_key(AnimProperty::ScaleX, time, t.scale.x, Interpolation::Bezier);
        clip.set_key(AnimProperty::ScaleY, time, t.scale.y, Interpolation::Bezier);
        clip.set_key(AnimProperty::ScaleZ, time, t.scale.z, Interpolation::Bezier);
        if let Some(cmd) = SetAnimationCmd::new(&self.doc, id, clip) {
            self.history.execute(&mut self.doc, Box::new(cmd));
            self.status = format!(
                "Keyed transforms at frame {}",
                (time * self.playback.fps).round()
            );
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
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        if self.playback.playing {
            self.playback.advance(dt);
            ctx.request_repaint();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
            self.focus_search = true;
            self.browser_tab = BrowserTab::Objects;
        }
        let undo = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
        let redo = ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers {
                    shift: true,
                    ..egui::Modifiers::COMMAND
                },
                egui::Key::Z,
            )
        });
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        let typing = ctx.memory(|m| m.focused().is_some());
        if !typing && ctx.input(|i| i.key_pressed(egui::Key::Delete)) && self.selected.is_some() {
            self.delete_selected();
        }
        if !typing && ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.frame_selected();
        }

        self.poll_render(&ctx);
        self.log_status();
        self.shell(root);

        self.show_file_dialog(&ctx);
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
            self.apply_file_dialog(mode, path);
        } else if !open {
            self.dialog = None;
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

// ---------------------------------------------------------------------------
// Viewport
// ---------------------------------------------------------------------------

impl LeopardApp {
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
        let strip = Rect::from_min_size(
            rect.left_top() + Vec2::new(7.0, 8.0),
            Vec2::new(30.0, 4.0 * 27.0 + 7.0),
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
        ]
        .into_iter()
        .enumerate()
        {
            let active = if index == 0 {
                !self.mesh_edit
            } else {
                self.mesh_edit
                    && matches!(
                        (index, self.mesh_select_mode),
                        (1, MeshSelectMode::Points)
                            | (2, MeshSelectMode::Edges)
                            | (3, MeshSelectMode::Polygons)
                    )
            };
            let hit = Rect::from_min_size(
                strip.left_top() + Vec2::new(3.0, 4.0 + index as f32 * 27.0),
                Vec2::splat(24.0),
            );
            let interactive = index == 0 || mesh_ready;
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
                    self.leave_mesh_edit();
                } else {
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
                item.on_hover_text("Select a mesh object to edit components");
            }
        }
        if !mode_hovered {
            self.handle_viewport_input(ui, &response, &evaluated, rect, pixel_w, pixel_h);
        }
        self.draw_viewport_overlay(ui, rect, &evaluated, pixel_w, pixel_h);
    }

    fn build_frame(&self, evaluated: &EvaluatedScene) -> FrameData {
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
            let selected = Some(node.id) == self.selected;
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
        let selected_overlay = !wireframe && self.selected.is_some();
        if wireframe || selected_overlay {
            for node in evaluated.mesh_nodes() {
                let Some(mesh) = node.mesh() else {
                    continue;
                };

                let selected = Some(node.id) == self.selected;
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

        let drag_started = !modifiers.alt && response.drag_started_by(egui::PointerButton::Primary);
        if drag_started {
            if let Some(pointer) = pointer {
                self.gizmo_drag = self.gizmo_hit(evaluated, rect, pixel_w, pixel_h, pointer);
            }
        }
        if let Some(mut drag) = self.gizmo_drag {
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
                self.gizmo_drag = None;
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
                                self.mesh_selected.clear();
                            }
                        }
                    }
                    return;
                }
                if let Some(id) = self.pick(evaluated, rect, pixel_w, pixel_h, pointer) {
                    self.selected = Some(id);
                    self.sync_selection_material();
                    self.status = format!(
                        "Selected {}",
                        self.doc
                            .scene
                            .get(id)
                            .map(|n| n.name.clone())
                            .unwrap_or_default()
                    );
                } else {
                    self.selected = None;
                }
            }
        }
    }

    fn gizmo_hit(
        &self,
        evaluated: &EvaluatedScene,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
        pointer: Pos2,
    ) -> Option<GizmoDrag> {
        if self.mesh_edit {
            return None;
        }
        if self.gizmo == GizmoMode::Off {
            return None;
        }
        let id = self.selected?;
        let node = evaluated.node(id)?;
        let origin = node.world.w_axis.truncate();
        let scale = (self.orbit.distance * 0.16).clamp(0.3, 6.0);
        let axes = self.gizmo_axes(node.world);
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
            if dist < 14.0 && t > 0.15 {
                if best.map(|(_, d, _, _)| dist < d).unwrap_or(true) {
                    best = Some((i, dist, end_screen, dir_n));
                }
            }
        }
        best.map(|(axis, _, _, screen_dir)| GizmoDrag {
            node: id,
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
            world_dir: if self.gizmo_world {
                axes[axis]
            } else {
                (node.local.rotation * axes[axis]).normalize_or(axes[axis])
            },
            start_transform: node.local,
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
        let Some(node) = self.doc.scene.get(drag.node) else {
            return;
        };
        let parent_world = node
            .parent
            .map(|p| self.doc.scene.world_matrix_plain(p))
            .unwrap_or(Mat4::IDENTITY);
        let scale = (self.orbit.distance * 0.16).clamp(0.3, 6.0);
        let mut transform = drag.start_transform;
        match drag.mode {
            GizmoMode::Translate => {
                let world_delta = drag.world_dir * (amount * scale);
                let local_delta = parent_world.inverse().transform_vector3(world_delta);
                transform.translation = drag.start_transform.translation + local_delta;
            }
            GizmoMode::Scale => {
                let factor = (1.0 + amount * 1.2).clamp(0.02, 100.0);
                let base = drag.world_dir;
                let local_axis =
                    (drag.start_transform.rotation.inverse() * base).normalize_or(Vec3::Y);
                let abs = local_axis.abs();
                if abs.x >= abs.y && abs.x >= abs.z {
                    transform.scale.x = drag.start_transform.scale.x * factor;
                } else if abs.y >= abs.z {
                    transform.scale.y = drag.start_transform.scale.y * factor;
                } else {
                    transform.scale.z = drag.start_transform.scale.z * factor;
                }
            }
            GizmoMode::Rotate => {
                let angle = amount * 1.8;
                let local_axis = (drag.start_transform.rotation.inverse() * drag.world_dir)
                    .normalize_or(Vec3::Y);
                transform.rotation = (Quat::from_axis_angle(local_axis, angle)
                    * drag.start_transform.rotation)
                    .normalize();
            }
            GizmoMode::Off => {}
        }
        if let Some(node) = self.doc.scene.get_mut(drag.node) {
            if self.pending_transform.is_none() {
                self.pending_transform = Some((drag.node, drag.start_transform));
            }
            node.transform = transform;
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

    fn draw_viewport_overlay(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        evaluated: &EvaluatedScene,
        pixel_w: u32,
        pixel_h: u32,
    ) {
        let painter = ui.painter_at(rect);
        if self.mesh_edit {
            self.draw_mesh_edit_overlay(ui, rect, evaluated, pixel_w, pixel_h);
            return;
        }
        if let Some(id) = self.selected {
            if let Some(node) = evaluated.node(id) {
                let origin = node.world.w_axis.truncate();
                if let Some(origin_screen) = self.project(origin, rect, pixel_w, pixel_h) {
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
                                        if self.gizmo_drag.map(|d| d.axis == i).unwrap_or(false) {
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

fn weld_selected_points(mesh: &Mesh, selected: &[u32]) -> Mesh {
    let mut unique: Vec<u32> = selected
        .iter()
        .copied()
        .filter(|p| (*p as usize) < mesh.positions.len())
        .collect();
    unique.sort_unstable();
    unique.dedup();
    if unique.len() < 2 {
        return mesh.clone();
    }
    let center = unique
        .iter()
        .map(|p| mesh.positions[*p as usize])
        .sum::<Vec3>()
        / unique.len() as f32;
    let target = unique[0];
    let remap: HashMap<u32, u32> = unique.iter().map(|p| (*p, target)).collect();
    let mut out = mesh.clone();
    out.positions[target as usize] = center;
    for poly in &mut out.polygons {
        for corner in &mut poly.corners {
            if let Some(mapped) = remap.get(&corner.point) {
                corner.point = *mapped;
            }
        }
    }
    out.remove_degenerate_polygons();
    out.compact_points();
    out
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
