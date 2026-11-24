//! Shared harness for the editor workflow test modules: app construction,
//! input simulation, mesh fixtures and curve helpers.

use super::*;

#[path = "selection_tests/animation.rs"]
mod animation;
#[path = "selection_tests/deform.rs"]
mod deform;
#[path = "selection_tests/documents.rs"]
mod documents;
#[path = "selection_tests/mesh_edit.rs"]
mod mesh_edit;
#[path = "selection_tests/scene_edit.rs"]
mod scene_edit;
#[path = "selection_tests/viewport_ui.rs"]
mod viewport_ui;

static NEXT_ROOT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct TestRoot {
    path: PathBuf,
}

impl TestRoot {
    fn new(label: &str) -> Self {
        let sequence = NEXT_ROOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "leopardcraft-selection-{}-{sequence}-{label}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn test_app(label: &str) -> (LeopardApp, TestRoot) {
    let root = TestRoot::new(label);
    let doc = Document::demo();
    let fps = doc.settings.fps.max(1.0);
    let mut selection = Selection::default();
    if let Some(id) = doc.scene.find_by_name("Cube") {
        selection.select_one(id);
    }
    let mut lifecycle = DocumentLifecycle::new(
        RecoveryStore::at(root.path.join("recovery")),
        Duration::ZERO,
    );
    assert!(matches!(lifecycle.startup(&doc), StartupOutcome::Clean));
    let app = LeopardApp {
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
        viewport: ViewportRenderer::new(None),
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
        object_row_rects: Vec::new(),
        object_row_highlights: Vec::new(),
        object_row_icons: Vec::new(),
        object_row_flags: Vec::new(),
    };
    (app, root)
}

fn named(app: &LeopardApp, name: &str) -> NodeId {
    app.doc
        .scene
        .find_by_name(name)
        .unwrap_or_else(|| panic!("demo scene is missing {name}"))
}

fn activate_curve_target(app: &mut LeopardApp, target: AnimTarget) -> bool {
    match target {
        AnimTarget::Object(id) => app.selection.select_one(id),
        AnimTarget::Tag(node, _) => app.selection.select_one(node),
        AnimTarget::Material(_) => {}
    }
    app.set_anim_target(target)
}

fn curve_paste_fixture(label: &str) -> (InputHarness, TestRoot, AnimTarget, Vec<SelectedKey>) {
    let (mut harness, root) = InputHarness::new(label, Surface::Shortcuts, Vec2::new(800.0, 600.0));
    harness.app.show_curves = true;
    let cube = named(&harness.app, "Cube");
    harness.app.selection.select_one(cube);
    {
        let node = harness.app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        for (time, value) in [(0.0, 4.0), (1.0, 5.0)] {
            node.animation.set_key(
                AnimProperty::TranslationX,
                time,
                value,
                Interpolation::Linear,
            );
        }
    }
    let target = AnimTarget::Object(cube);
    assert!(harness.app.set_anim_target(target));
    let clip = harness.app.target_clip(target).unwrap();
    assert!(harness
        .app
        .select_keys_in_box(target, &clip, (0.0, 1.0), (4.0, 5.0), false));
    let selected = harness.app.selected_curve_keys();
    assert_eq!(selected.len(), 2);
    (harness, root, target, selected)
}

fn curve_property_times(app: &LeopardApp, target: AnimTarget, property: AnimProperty) -> Vec<f32> {
    let clip = app.target_clip(target).unwrap();
    let mut times: Vec<f32> = clip
        .track(property)
        .map(|track| track.curve.keys.iter().map(|key| key.time).collect())
        .unwrap_or_default();
    times.sort_by(|a, b| a.total_cmp(b));
    times
}

fn assert_property_curve_times(
    app: &LeopardApp,
    target: AnimTarget,
    property: AnimProperty,
    expected: &[f32],
) {
    let actual = curve_property_times(app, target, property);
    assert_eq!(actual.len(), expected.len(), "times: {actual:?}");
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "expected key times {expected:?}, got {actual:?}"
        );
    }
}

fn assert_curve_times(app: &LeopardApp, target: AnimTarget, expected: &[f32]) {
    assert_property_curve_times(app, target, AnimProperty::TranslationX, expected);
}

fn assert_mat4_close(actual: Mat4, expected: Mat4) {
    for (actual, expected) in actual
        .to_cols_array()
        .into_iter()
        .zip(expected.to_cols_array())
    {
        assert!((actual - expected).abs() < 1e-4, "{actual} != {expected}");
    }
}

// ---------------------------------------------------------------------------
// Input-driven coverage
//
// These tests do not call the selection/transform helpers directly. They feed
// raw egui pointer, modifier and key events through the same `ui_viewport`,
// `ui_objects` and `handle_shortcuts` paths used by the running app, so a
// disconnected click handler or a stale single-selection consumer fails here.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Surface {
    Viewport,
    Objects,
    Shortcuts,
    Shell,
}

struct InputHarness {
    app: LeopardApp,
    ctx: egui::Context,
    screen: Rect,
    time: f64,
}

impl InputHarness {
    fn new(label: &str, surface: Surface, size: Vec2) -> (Self, TestRoot) {
        let (app, root) = test_app(label);
        let mut harness = Self {
            app,
            ctx: egui::Context::default(),
            screen: Rect::from_min_size(Pos2::ZERO, size),
            time: 0.0,
        };
        harness.frame(surface, Vec::new(), egui::Modifiers::default());
        (harness, root)
    }

    fn frame(&mut self, surface: Surface, events: Vec<egui::Event>, modifiers: egui::Modifiers) {
        let mut all_events = Vec::with_capacity(events.len() + 1);
        all_events.push(egui::Event::ModifiersChanged(modifiers));
        all_events.extend(events);
        let raw = egui::RawInput {
            screen_rect: Some(self.screen),
            time: Some(self.time),
            events: all_events,
            focused: true,
            ..Default::default()
        };
        self.time += 1.0 / 60.0;
        let ctx = self.ctx.clone();
        let app = &mut self.app;
        let mut output = ctx.run_ui(raw, |ui| match surface {
            Surface::Viewport => app.ui_viewport(ui),
            Surface::Objects => app.ui_objects(ui),
            // Keep the viewport widget registered while testing shortcuts;
            // egui hit-tests a press against the previous frame's widgets.
            Surface::Shortcuts => {
                app.handle_shortcuts(ui.ctx());
                app.ui_viewport(ui);
            }
            Surface::Shell => {
                app.handle_shortcuts(ui.ctx());
                app.shell(ui);
            }
        });
        // No renderer consumes the font atlas delta in this headless harness.
        output.textures_delta.clear();
    }

    fn move_to(&mut self, surface: Surface, pos: Pos2, modifiers: egui::Modifiers) {
        self.frame(surface, vec![egui::Event::PointerMoved(pos)], modifiers);
    }

    fn press_button(
        &mut self,
        surface: Surface,
        pos: Pos2,
        button: egui::PointerButton,
        modifiers: egui::Modifiers,
    ) {
        self.frame(
            surface,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers,
                },
            ],
            modifiers,
        );
    }

    fn release_button(
        &mut self,
        surface: Surface,
        pos: Pos2,
        button: egui::PointerButton,
        modifiers: egui::Modifiers,
    ) {
        self.frame(
            surface,
            vec![egui::Event::PointerButton {
                pos,
                button,
                pressed: false,
                modifiers,
            }],
            modifiers,
        );
    }

    fn press(&mut self, surface: Surface, pos: Pos2, modifiers: egui::Modifiers) {
        self.press_button(surface, pos, egui::PointerButton::Primary, modifiers);
    }

    fn release(&mut self, surface: Surface, pos: Pos2, modifiers: egui::Modifiers) {
        self.release_button(surface, pos, egui::PointerButton::Primary, modifiers);
    }

    fn click(&mut self, surface: Surface, pos: Pos2, modifiers: egui::Modifiers) {
        self.press(surface, pos, modifiers);
        self.release(surface, pos, modifiers);
        self.frame(surface, Vec::new(), egui::Modifiers::default());
    }

    fn right_click(&mut self, surface: Surface, pos: Pos2) {
        self.press_button(
            surface,
            pos,
            egui::PointerButton::Secondary,
            egui::Modifiers::default(),
        );
        self.release_button(
            surface,
            pos,
            egui::PointerButton::Secondary,
            egui::Modifiers::default(),
        );
        self.frame(surface, Vec::new(), egui::Modifiers::default());
    }

    fn drag(&mut self, surface: Surface, start: Pos2, end: Pos2, modifiers: egui::Modifiers) {
        self.press(surface, start, modifiers);
        for step in 1..=6 {
            let t = step as f32 / 6.0;
            self.move_to(surface, start + (end - start) * t, modifiers);
        }
        self.release(surface, end, modifiers);
        self.frame(surface, Vec::new(), egui::Modifiers::default());
    }

    fn press_key(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        let event = |pressed| egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers,
        };
        self.frame(Surface::Shortcuts, vec![event(true)], modifiers);
        self.frame(Surface::Shortcuts, vec![event(false)], modifiers);
        self.frame(Surface::Shortcuts, Vec::new(), egui::Modifiers::default());
    }

    fn viewport_dims(&self) -> (u32, u32) {
        let size = self.app.last_viewport_size;
        let ppp = self.ctx.pixels_per_point();
        (
            (size.x * ppp).round().max(16.0) as u32,
            (size.y * ppp).round().max(16.0) as u32,
        )
    }

    fn node_rect(&self, id: NodeId) -> Rect {
        let evaluated = self.app.evaluate();
        let node = evaluated
            .node(id)
            .unwrap_or_else(|| panic!("missing evaluated node {id:?}"));
        let (pixel_w, pixel_h) = self.viewport_dims();
        self.app
            .node_screen_rect(node, self.app.last_viewport_rect, pixel_w, pixel_h)
            .unwrap_or_else(|| panic!("node {id:?} does not project into the viewport"))
    }

    fn viewport_point(&self, id: NodeId) -> Pos2 {
        let evaluated = self.app.evaluate();
        let node = evaluated
            .node(id)
            .unwrap_or_else(|| panic!("missing evaluated node {id:?}"));
        let (pixel_w, pixel_h) = self.viewport_dims();
        let rect = self.app.last_viewport_rect;
        let bounds = self
            .app
            .node_screen_rect(node, rect, pixel_w, pixel_h)
            .unwrap_or_else(|| panic!("node {id:?} does not project into the viewport"));
        for (fx, fy) in [
            (0.5, 0.5),
            (0.4, 0.5),
            (0.6, 0.5),
            (0.5, 0.4),
            (0.5, 0.6),
            (0.3, 0.3),
            (0.7, 0.7),
            (0.3, 0.7),
            (0.7, 0.3),
        ] {
            let point = Pos2::new(
                bounds.left() + bounds.width() * fx,
                bounds.top() + bounds.height() * fy,
            );
            if self.app.pick(&evaluated, rect, pixel_w, pixel_h, point) == Some(id) {
                return point;
            }
        }
        panic!("no pickable screen point found for {id:?}");
    }

    fn modes_strip(&self) -> Rect {
        Rect::from_min_size(
            self.app.last_viewport_rect.left_top() + Vec2::new(4.0, 4.0),
            Vec2::new(40.0, 132.0),
        )
    }

    fn is_empty_viewport_point(&self, point: Pos2, evaluated: &EvaluatedScene) -> bool {
        let (pixel_w, pixel_h) = self.viewport_dims();
        self.app
            .pick(
                evaluated,
                self.app.last_viewport_rect,
                pixel_w,
                pixel_h,
                point,
            )
            .is_none()
            && self
                .app
                .gizmo_hit(
                    evaluated,
                    self.app.last_viewport_rect,
                    pixel_w,
                    pixel_h,
                    point,
                )
                .is_none()
    }

    fn empty_point(&self) -> Pos2 {
        let rect = self.app.last_viewport_rect;
        let strip = self.modes_strip();
        let evaluated = self.app.evaluate();
        for iy in 0..=10 {
            for ix in 0..=10 {
                let point = Pos2::new(
                    rect.left() + rect.width() * (0.08 + 0.084 * ix as f32),
                    rect.top() + rect.height() * (0.08 + 0.084 * iy as f32),
                );
                if !strip.contains(point) && self.is_empty_viewport_point(point, &evaluated) {
                    return point;
                }
            }
        }
        panic!("no empty viewport point found");
    }

    fn empty_point_near(&self, target: Rect) -> Pos2 {
        let rect = self.app.last_viewport_rect;
        let strip = self.modes_strip();
        let evaluated = self.app.evaluate();
        let candidates = [
            target.left_top() - Vec2::splat(12.0),
            target.right_top() + Vec2::new(12.0, -12.0),
            target.left_bottom() + Vec2::new(-12.0, 12.0),
            target.right_bottom() + Vec2::splat(12.0),
        ];
        for point in candidates {
            if rect.contains(point)
                && !strip.contains(point)
                && self.is_empty_viewport_point(point, &evaluated)
            {
                return point;
            }
        }
        self.empty_point()
    }

    fn gizmo_handle(&self) -> Option<(Pos2, Pos2)> {
        let rect = self.app.last_viewport_rect;
        let (pixel_w, pixel_h) = self.viewport_dims();
        let evaluated = self.app.evaluate();
        let pivot = self.app.selection_pivot(&evaluated)?;
        let primary = self.app.selection.primary()?;
        let node = evaluated.node(primary)?;
        let origin = self.app.project(pivot, rect, pixel_w, pixel_h)?;
        let scale = (self.app.orbit.distance * 0.16).clamp(0.3, 6.0);
        for axis in self.app.gizmo_axes(node.world) {
            let Some(axis_end) = self
                .app
                .project(pivot + axis * scale, rect, pixel_w, pixel_h)
            else {
                continue;
            };
            let direction = axis_end - origin;
            let length = direction.length();
            if length < 36.0 {
                continue;
            }
            let direction = direction / length;
            for fraction in [0.5, 0.6, 0.4] {
                let start = origin + direction * (length * fraction);
                if !rect.contains(start)
                    || self
                        .app
                        .gizmo_hit(&evaluated, rect, pixel_w, pixel_h, start)
                        .is_none()
                {
                    continue;
                }
                // Keep the first drag frame on the axis segment; a long
                // synthetic jump can overshoot the handle before egui reports
                // `drag_started`, which would miss the gizmo entirely.
                let travel = (length * 0.35).clamp(12.0, 60.0);
                for sign in [1.0, -1.0] {
                    let end = start + direction * (travel * sign);
                    if rect.contains(end) {
                        return Some((start, end));
                    }
                }
            }
        }
        None
    }

    fn object_row(&self, id: NodeId) -> Rect {
        self.app
            .object_row_rects
            .iter()
            .find(|(row, _)| *row == id)
            .map(|(_, rect)| *rect)
            .unwrap_or_else(|| panic!("object browser has no row for {id:?}"))
    }

    fn object_highlight(&self, id: NodeId) -> Option<bool> {
        self.app
            .object_row_highlights
            .iter()
            .find(|(row, _)| *row == id)
            .map(|(_, selected)| *selected)
    }

    fn object_icon(&self, id: NodeId, slot: usize) -> Rect {
        self.app
            .object_row_icons
            .iter()
            .find(|(row, ..)| *row == id)
            .map(|(_, eye, render, lock)| [*eye, *render, *lock][slot])
            .unwrap_or_else(|| panic!("object browser has no icons for {id:?}"))
    }

    fn object_flags(&self, id: NodeId) -> Option<(bool, bool, bool)> {
        self.app
            .object_row_flags
            .iter()
            .find(|(row, ..)| *row == id)
            .map(|(_, visible, renderable, locked)| (*visible, *renderable, *locked))
    }

    fn viewport_hits(&self) -> (Vec<NodeId>, Vec<NodeId>) {
        ui_test_support::viewport_hits()
    }

    fn menu_item(&self, key: &str) -> (Rect, bool) {
        ui_test_support::menu_hit(key)
            .unwrap_or_else(|| panic!("menu item {key:?} was not rendered in the last shell frame"))
    }

    fn open_menu(&mut self, top_key: &str) -> Rect {
        let (rect, enabled) = self.menu_item(top_key);
        assert!(enabled, "top-level menu {top_key:?} should be enabled");
        self.click(Surface::Shell, rect.center(), egui::Modifiers::default());
        self.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
        rect
    }

    fn click_menu_item(&mut self, key: &str) {
        let (rect, enabled) = self.menu_item(key);
        assert!(enabled, "menu item {key:?} should be enabled");
        self.click(Surface::Shell, rect.center(), egui::Modifiers::default());
        self.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    }

    fn open_submenu(&mut self, key: &str) {
        self.click_menu_item(key);
        self.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    }

    fn open_context_menu(&mut self, id: NodeId) {
        let row = self.object_row(id);
        self.right_click(Surface::Shell, object_row_click(row));
        self.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    }

    fn undo_via_menu(&mut self) {
        self.open_menu("menu:Edit");
        self.click_menu_item("Edit:Undo");
    }
}

fn object_row_click(rect: Rect) -> Pos2 {
    Pos2::new(rect.left() + 80.0, rect.center().y)
}

fn world_position(app: &LeopardApp, id: NodeId) -> Vec3 {
    app.doc.scene.world_matrix_plain(id).w_axis.truncate()
}

fn assert_world_close(actual: Vec3, expected: Vec3) {
    assert!(
        (actual - expected).length() < 2e-4,
        "{actual:?} != {expected:?}"
    );
}

fn select_object_rows(harness: &mut InputHarness, ids: &[NodeId]) {
    for (index, id) in ids.iter().enumerate() {
        let rect = harness.object_row(*id);
        let modifiers = if index == 0 {
            egui::Modifiers::default()
        } else {
            egui::Modifiers {
                shift: true,
                ..Default::default()
            }
        };
        harness.click(Surface::Shell, object_row_click(rect), modifiers);
    }
    assert_eq!(harness.app.selection.ids(), ids);
    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
}

fn assert_selection_reflected(harness: &InputHarness, ids: &[NodeId]) {
    for id in ids {
        assert_eq!(
            harness.object_highlight(*id),
            Some(true),
            "browser row {id:?} should stay highlighted"
        );
    }
    let (visible, selected) = harness.viewport_hits();
    for id in ids {
        assert!(
            visible.contains(id),
            "viewport should render selected object {id:?}"
        );
        assert!(
            selected.contains(id),
            "viewport selection overlay should include {id:?}"
        );
    }
}

fn editable_mesh(app: &LeopardApp, id: NodeId) -> bool {
    matches!(
        app.doc.scene.get(id).map(|node| &node.kind),
        Some(NodeKind::Mesh {
            geometry: GeometrySource::Editable { .. }
        })
    )
}

fn set_editable_geometry(app: &mut LeopardApp, id: NodeId, mesh: Mesh) {
    let node = app
        .doc
        .scene
        .get_mut(id)
        .unwrap_or_else(|| panic!("demo scene is missing {id:?}"));
    node.kind = NodeKind::Mesh {
        geometry: GeometrySource::editable(mesh),
    };
}

fn set_selected_components(app: &mut LeopardApp, ids: &[NodeId], components: &[u32]) {
    for id in ids {
        app.mesh_components.insert(*id, components.to_vec());
    }
    if let Some(primary) = app.selection.primary() {
        app.mesh_selected = components.to_vec();
        app.mesh_components.insert(primary, components.to_vec());
    }
}

fn skin_ready(app: &LeopardApp) -> bool {
    let Some(target) = app.skin_target() else {
        return false;
    };
    let Some(node) = app.selection.primary().and_then(|id| app.doc.scene.get(id)) else {
        return false;
    };
    skin_binding_ready(node, &target)
}

fn selected_mesh(app: &LeopardApp, id: NodeId) -> Mesh {
    match app.doc.scene.get(id).map(|node| &node.kind) {
        Some(NodeKind::Mesh {
            geometry: GeometrySource::Editable { mesh, .. },
        }) => mesh.clone(),
        _ => panic!("{id:?} is not an editable mesh"),
    }
}

fn polygon_material(app: &LeopardApp, id: NodeId, index: usize) -> Option<MaterialId> {
    let node = app.doc.scene.get(id)?;
    let NodeKind::Mesh {
        geometry: GeometrySource::Editable { mesh, .. },
    } = &node.kind
    else {
        return None;
    };
    let poly = mesh.polygons.get(index)?;
    node.material_slots.get(&poly.material_slot).copied()
}

/// Two disconnected quads give Bridge two clean boundary loops per object.
fn bridge_fixture_mesh() -> Mesh {
    let mut mesh = lc_geom::plane_mesh(1.0, 1.0, 1, 1);
    mesh.merge(
        &lc_geom::plane_mesh(1.0, 1.0, 1, 1),
        Mat4::from_translation(Vec3::Y * 1.5),
    );
    mesh
}

fn subdivision_count(app: &LeopardApp, id: NodeId) -> usize {
    app.doc
        .scene
        .get(id)
        .map(|node| {
            node.modifiers
                .iter()
                .filter(|modifier| matches!(modifier, Modifier::Subdivision { .. }))
                .count()
        })
        .unwrap_or(0)
}

fn boolean_operands(app: &LeopardApp, id: NodeId) -> Vec<NodeId> {
    app.doc
        .scene
        .get(id)
        .map(|node| {
            node.modifiers
                .iter()
                .filter_map(|modifier| match modifier {
                    Modifier::Boolean { operand, .. } => Some(*operand),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn set_principled_float(material: &mut Material, index: usize, value: f32) {
    let node = material
        .graph
        .nodes
        .iter_mut()
        .find(|node| matches!(node.kind, ShaderNodeKind::Principled))
        .expect("demo material has a Principled node");
    node.inputs[index] = ShaderValue::Float(value);
}
