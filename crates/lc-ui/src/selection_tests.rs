//! Focused coverage for the shared multi-object selection: additive clicks,
//! marquee selection, shared-pivot transforms, batch flags, group/reparent and
//! multi delete/duplicate with a single undo step each.

use super::*;

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
        curve_key_drag: None,
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

#[test]
fn additive_click_keeps_multiple_objects_and_last_click_is_primary() {
    let (mut app, _root) = test_app("additive");
    let cube = named(&app, "Cube");
    let ground = named(&app, "Ground");
    let panel = named(&app, "Checker Panel");

    app.selection.select_one(cube);
    app.selection.toggle(ground);
    assert_eq!(app.selection.ids(), &[cube, ground]);
    assert_eq!(app.selection.primary(), Some(ground));

    app.selection.toggle(cube);
    assert_eq!(app.selection.ids(), &[ground]);
    assert_eq!(app.selection.primary(), Some(ground));

    app.selection.toggle(panel);
    assert_eq!(app.selection.ids(), &[ground, panel]);
    assert!(app.selection_status().contains("2 objects"));

    app.selection.select_one(cube);
    assert_eq!(app.selection.len(), 1);
    assert_eq!(app.selection_status(), "Selected Cube");
}

#[test]
fn marquee_selects_objects_by_projected_bounds_and_is_additive() {
    let (mut app, _root) = test_app("marquee");
    let cube = named(&app, "Cube");
    let ground = named(&app, "Ground");
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(240.0));
    let evaluated = app.evaluate();
    let cube_rect = app
        .node_screen_rect(evaluated.node(cube).unwrap(), rect, 240, 240)
        .expect("cube projects into the viewport");
    let marquee = BoxSelect {
        start: cube_rect.min,
        current: cube_rect.max,
        base: Vec::new(),
        additive: false,
    };
    app.finish_box_select(&marquee, &evaluated, rect, 240, 240);
    assert!(
        app.selection.contains(cube),
        "marquee should contain the cube, got {:?}",
        app.selection.ids()
    );

    let additive = BoxSelect {
        start: cube_rect.min,
        current: cube_rect.max,
        base: vec![ground],
        additive: true,
    };
    app.finish_box_select(&additive, &evaluated, rect, 240, 240);
    assert!(app.selection.contains(ground));
    assert!(app.selection.contains(cube));
}

#[test]
fn shared_pivot_translate_moves_every_root_in_one_undo_step() {
    let (mut app, _root) = test_app("shared-translate");
    let cube = named(&app, "Cube");
    let ground = named(&app, "Ground");
    let panel = named(&app, "Checker Panel");
    app.selection.set([cube, ground, panel]);
    let before: Vec<(NodeId, Transform)> = app
        .selection_transform_roots()
        .into_iter()
        .map(|id| (id, app.doc.scene.get(id).unwrap().transform))
        .collect();
    let evaluated = app.evaluate();
    let pivot = app.selection_pivot(&evaluated).expect("shared pivot");
    let mut drag = GizmoDrag {
        nodes: before.clone(),
        pivot,
        mode: GizmoMode::Translate,
        axis: 0,
        start_pointer: Pos2::ZERO,
        screen_dir: Vec2::X,
        screen_len: 100.0,
        world_dir: Vec3::X,
    };
    app.apply_gizmo_drag(&mut drag, 0.5);
    app.commit_pending();

    let expected = 0.5 * (app.orbit.distance * 0.16).clamp(0.3, 6.0);
    for (id, start) in &before {
        let after = app.doc.scene.get(*id).unwrap().transform;
        assert!((after.translation.x - start.translation.x - expected).abs() < 1e-4);
        assert!((after.translation.y - start.translation.y).abs() < 1e-5);
        assert!((after.translation.z - start.translation.z).abs() < 1e-5);
    }
    assert_eq!(
        app.history.undo_label().as_deref(),
        Some("Transform 3 Objects")
    );

    app.undo();
    for (id, start) in &before {
        assert_eq!(app.doc.scene.get(*id).unwrap().transform, *start);
    }
}

#[test]
fn shared_pivot_rotate_and_scale_use_the_same_pivot_and_undo() {
    let (mut app, _root) = test_app("shared-rotate-scale");
    let cube = named(&app, "Cube");
    let panel = named(&app, "Checker Panel");
    app.selection.set([cube, panel]);
    let roots = app.selection_transform_roots();
    let before: Vec<(NodeId, Transform)> = roots
        .iter()
        .map(|id| (*id, app.doc.scene.get(*id).unwrap().transform))
        .collect();
    let pivot = app.selection_pivot(&app.evaluate()).expect("shared pivot");
    let starts = before.clone();
    let mut drag = GizmoDrag {
        nodes: starts.clone(),
        pivot,
        mode: GizmoMode::Rotate,
        axis: 1,
        start_pointer: Pos2::ZERO,
        screen_dir: Vec2::Y,
        screen_len: 100.0,
        world_dir: Vec3::Y,
    };
    app.apply_gizmo_drag(&mut drag, 0.5);
    app.commit_pending();
    let angle = 0.5 * 1.8;
    let rotation = Quat::from_axis_angle(Vec3::Y, angle);
    for (id, start) in &starts {
        let expected = pivot + rotation * (start.translation - pivot);
        let actual = app.doc.scene.get(*id).unwrap().transform.translation;
        assert!((actual - expected).length() < 1e-4);
    }
    app.undo();
    for (id, start) in &before {
        assert_eq!(app.doc.scene.get(*id).unwrap().transform, *start);
    }

    let pivot = app.selection_pivot(&app.evaluate()).expect("shared pivot");
    let mut drag = GizmoDrag {
        nodes: before.clone(),
        pivot,
        mode: GizmoMode::Scale,
        axis: 0,
        start_pointer: Pos2::ZERO,
        screen_dir: Vec2::X,
        screen_len: 100.0,
        world_dir: Vec3::X,
    };
    app.apply_gizmo_drag(&mut drag, 0.5);
    app.commit_pending();
    let factor = 1.0 + 0.5 * 1.2;
    for (id, start) in &before {
        let offset = start.translation - pivot;
        let expected = pivot + Vec3::new(offset.x * factor, offset.y, offset.z);
        let actual = app.doc.scene.get(*id).unwrap().transform.translation;
        assert!((actual - expected).length() < 1e-4);
    }
    app.undo();
    for (id, start) in &before {
        assert_eq!(app.doc.scene.get(*id).unwrap().transform, *start);
    }
}

#[test]
fn multi_delete_and_duplicate_each_undo_as_one_step() {
    let (mut app, _root) = test_app("delete-duplicate");
    let cube = named(&app, "Cube");
    let ground = named(&app, "Ground");
    let panel = named(&app, "Checker Panel");
    let original_signature = app.doc.signature();
    let original_count = app.doc.scene.len();

    app.selection.set([cube, ground, panel]);
    app.delete_selected();
    assert_eq!(app.doc.scene.len(), original_count - 3);
    assert!(app.selection.is_empty());
    app.undo();
    assert_eq!(app.doc.signature(), original_signature);

    app.selection.set([cube, panel]);
    app.duplicate_selected();
    assert_eq!(app.doc.scene.len(), original_count + 2);
    assert_eq!(app.selection.len(), 2);
    let copies = app.selection.ids().to_vec();
    assert!(copies.iter().all(|id| *id != cube && *id != panel));
    let originals = [cube, panel];
    for (copy, original) in copies.iter().zip(originals) {
        let copy = app.doc.scene.get(*copy).expect("copy exists");
        let original = app.doc.scene.get(original);
        assert_eq!(copy.parent, original.and_then(|node| node.parent));
    }
    app.undo();
    assert_eq!(app.doc.signature(), original_signature);
    assert!(
        app.selection.is_empty(),
        "undone copies leave the selection"
    );
}

#[test]
fn batch_visibility_and_lock_share_one_undo_step_each() {
    let (mut app, _root) = test_app("batch-flags");
    let ids = [
        named(&app, "Cube"),
        named(&app, "Ground"),
        named(&app, "Checker Panel"),
    ];
    app.selection.set(ids);
    let before = app.history.depth();

    assert!(app.selection_flag_ready(NodeFlag::Visible, false));
    app.set_selection_flag(NodeFlag::Visible, false);
    assert!(ids
        .iter()
        .all(|id| !app.doc.scene.get(*id).unwrap().visible));
    assert_eq!(app.history.depth(), before + 1);

    assert!(!app.selection_flag_ready(NodeFlag::Visible, false));
    let signature = app.doc.signature();
    app.set_selection_flag(NodeFlag::Visible, false);
    assert_eq!(app.history.depth(), before + 1);
    assert_eq!(app.doc.signature(), signature);
    assert!(app.selection_flag_ready(NodeFlag::Visible, true));

    app.set_selection_flag(NodeFlag::Locked, true);
    assert!(ids.iter().all(|id| app.doc.scene.get(*id).unwrap().locked));
    assert_eq!(app.history.depth(), before + 2);

    app.undo();
    assert!(ids.iter().all(|id| !app.doc.scene.get(*id).unwrap().locked));
    app.undo();
    assert!(ids.iter().all(|id| app.doc.scene.get(*id).unwrap().visible));
}

#[test]
fn empty_flags_and_key_transforms_are_refused_by_the_predicates_that_disable_them() {
    let (mut app, _root) = test_app("empty-flag-key-refusal");
    app.selection.clear();
    let signature = app.doc.signature();
    let depth = app.history.depth();
    assert!(!app.selection_flag_ready(NodeFlag::Visible, false));
    assert!(!app.key_transforms_ready());

    app.set_selection_flag(NodeFlag::Visible, false);
    app.key_transform_channels();
    assert_eq!(app.doc.signature(), signature);
    assert_eq!(app.history.depth(), depth);
}

#[test]
fn group_preserves_world_transforms_and_undo_restores_the_scene() {
    let (mut app, _root) = test_app("group");
    let cube = named(&app, "Cube");
    let panel = named(&app, "Checker Panel");
    let before_signature = app.doc.signature();
    let cube_world = app.doc.scene.world_matrix_plain(cube);
    let panel_world = app.doc.scene.world_matrix_plain(panel);

    app.selection.set([cube, panel]);
    app.group_selection();

    let group = named(&app, "Group");
    assert_eq!(app.selection.primary(), Some(group));
    assert_eq!(app.doc.scene.get(cube).unwrap().parent, Some(group));
    assert_eq!(app.doc.scene.get(panel).unwrap().parent, Some(group));
    assert_mat4_close(app.doc.scene.world_matrix_plain(cube), cube_world);
    assert_mat4_close(app.doc.scene.world_matrix_plain(panel), panel_world);

    app.undo();
    assert_eq!(app.doc.signature(), before_signature);
    assert!(app.doc.scene.find_by_name("Group").is_none());
    assert!(app.selection.is_empty());
}

#[test]
fn batch_reparent_preserves_world_transforms_and_undo_restores_parents() {
    let (mut app, _root) = test_app("reparent");
    let cube = named(&app, "Cube");
    let panel = named(&app, "Checker Panel");
    let studio = named(&app, "Studio");
    let before_signature = app.doc.signature();
    let cube_world = app.doc.scene.world_matrix_plain(cube);
    let panel_world = app.doc.scene.world_matrix_plain(panel);
    let old_parents = (
        app.doc.scene.get(cube).unwrap().parent,
        app.doc.scene.get(panel).unwrap().parent,
    );

    app.selection.set([cube, panel]);
    assert!(app.reparent_selection(Some(studio)));
    assert_eq!(app.doc.scene.get(cube).unwrap().parent, Some(studio));
    assert_eq!(app.doc.scene.get(panel).unwrap().parent, Some(studio));
    assert_mat4_close(app.doc.scene.world_matrix_plain(cube), cube_world);
    assert_mat4_close(app.doc.scene.world_matrix_plain(panel), panel_world);

    app.undo();
    assert_eq!(app.doc.signature(), before_signature);
    assert_eq!(app.doc.scene.get(cube).unwrap().parent, old_parents.0);
    assert_eq!(app.doc.scene.get(panel).unwrap().parent, old_parents.1);

    app.selection.select_one(cube);
    assert!(!app.reparent_selection(Some(cube)));
    assert_eq!(app.doc.signature(), before_signature);
}

#[test]
fn workspace_state_restores_every_selected_object() {
    let (mut app, _root) = test_app("workspace");
    let cube = named(&app, "Cube");
    let panel = named(&app, "Checker Panel");
    app.selection.set([cube, panel]);
    let state = app.workspace_state();
    assert_eq!(state.selected_names.len(), 2);
    assert_eq!(state.selected_name.as_deref(), Some("Checker Panel"));

    app.selection.clear();
    app.apply_workspace_state(state);
    assert_eq!(app.selection.ids(), &[cube, panel]);
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

#[test]
fn viewport_pointer_input_replaces_toggles_and_marquee_selects() {
    let (mut harness, _root) =
        InputHarness::new("input-viewport", Surface::Viewport, Vec2::new(720.0, 540.0));
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    let sphere = named(&harness.app, "Glass Sphere");
    let key_light = named(&harness.app, "Key Light");
    harness.app.gizmo = GizmoMode::Off;

    let cube_point = harness.viewport_point(cube);
    let panel_point = harness.viewport_point(panel);
    let sphere_point = harness.viewport_point(sphere);

    harness.click(Surface::Viewport, panel_point, egui::Modifiers::default());
    assert_eq!(
        harness.app.selection.ids(),
        &[panel],
        "plain viewport click must replace the selection"
    );

    harness.click(Surface::Viewport, cube_point, egui::Modifiers::default());
    harness.click(
        Surface::Viewport,
        panel_point,
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(harness.app.selection.ids(), &[cube, panel]);
    assert_eq!(harness.app.selection.primary(), Some(panel));

    harness.click(Surface::Viewport, sphere_point, egui::Modifiers::COMMAND);
    assert_eq!(harness.app.selection.ids(), &[cube, panel, sphere]);

    harness.click(
        Surface::Viewport,
        panel_point,
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(harness.app.selection.ids(), &[cube, sphere]);
    harness.click(
        Surface::Viewport,
        panel_point,
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(harness.app.selection.ids(), &[cube, sphere, panel]);

    harness.click(Surface::Viewport, cube_point, egui::Modifiers::default());
    let target_rect = harness.node_rect(key_light);
    let cube_rect = harness.node_rect(cube);
    let start = harness.empty_point_near(target_rect);
    let end = target_rect.center();
    harness.drag(Surface::Viewport, start, end, egui::Modifiers::default());
    assert!(
        harness.app.selection.contains(key_light),
        "plain marquee must select the key light, got {:?}",
        harness.app.selection.ids()
    );
    assert!(
        !harness.app.selection.contains(cube),
        "plain marquee must replace the previous cube selection"
    );
    assert!(
        !cube_rect.intersects(Rect::from_two_pos(start, end)),
        "marquee setup must not overlap the previous selection"
    );

    harness.click(Surface::Viewport, cube_point, egui::Modifiers::default());
    let start = harness.empty_point_near(target_rect);
    harness.drag(
        Surface::Viewport,
        start,
        target_rect.center(),
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert!(
        harness.app.selection.contains(cube) && harness.app.selection.contains(key_light),
        "additive marquee must retain the cube and add the key light, got {:?}",
        harness.app.selection.ids()
    );
}

#[test]
fn object_browser_pointer_input_highlights_rows_and_batches_flags_and_actions() {
    let (mut harness, _root) =
        InputHarness::new("input-objects", Surface::Objects, Vec2::new(420.0, 560.0));
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    let sphere = named(&harness.app, "Glass Sphere");

    harness.click(
        Surface::Objects,
        object_row_click(harness.object_row(panel)),
        egui::Modifiers::default(),
    );
    assert_eq!(harness.app.selection.ids(), &[panel]);
    assert_eq!(harness.object_highlight(panel), Some(true));
    assert_eq!(harness.object_highlight(cube), Some(false));

    harness.click(
        Surface::Objects,
        object_row_click(harness.object_row(cube)),
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(harness.app.selection.ids(), &[panel, cube]);
    harness.frame(Surface::Objects, Vec::new(), egui::Modifiers::default());
    assert_eq!(harness.object_highlight(panel), Some(true));
    assert_eq!(harness.object_highlight(cube), Some(true));
    assert_eq!(harness.object_highlight(sphere), Some(false));

    harness.click(
        Surface::Objects,
        object_row_click(harness.object_row(sphere)),
        egui::Modifiers::COMMAND,
    );
    assert_eq!(harness.app.selection.ids(), &[panel, cube, sphere]);
    assert_eq!(harness.object_highlight(sphere), Some(true));

    harness.click(
        Surface::Objects,
        object_row_click(harness.object_row(cube)),
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(harness.app.selection.ids(), &[panel, sphere]);
    harness.frame(Surface::Objects, Vec::new(), egui::Modifiers::default());
    assert_eq!(harness.object_highlight(cube), Some(false));
    assert_eq!(harness.object_highlight(panel), Some(true));
    assert_eq!(harness.object_highlight(sphere), Some(true));

    harness.click(
        Surface::Objects,
        object_row_click(harness.object_row(panel)),
        egui::Modifiers::default(),
    );
    assert_eq!(harness.app.selection.ids(), &[panel]);
    assert_eq!(harness.object_highlight(panel), Some(true));
    assert_eq!(harness.object_highlight(cube), Some(false));

    harness.click(
        Surface::Objects,
        object_row_click(harness.object_row(cube)),
        egui::Modifiers::default(),
    );
    harness.click(
        Surface::Objects,
        object_row_click(harness.object_row(panel)),
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(harness.app.selection.ids(), &[cube, panel]);

    let depth = harness.app.history.depth();
    harness.click(
        Surface::Objects,
        harness.object_icon(cube, 2).center(),
        egui::Modifiers::default(),
    );
    assert!(harness.app.doc.scene.get(cube).unwrap().locked);
    assert!(harness.app.doc.scene.get(panel).unwrap().locked);
    assert_eq!(harness.app.history.depth(), depth + 1);
    harness.press_key(egui::Key::Z, egui::Modifiers::COMMAND);
    assert!(!harness.app.doc.scene.get(cube).unwrap().locked);
    assert!(!harness.app.doc.scene.get(panel).unwrap().locked);
    assert_eq!(harness.app.history.depth(), depth);

    harness.frame(Surface::Objects, Vec::new(), egui::Modifiers::default());
    harness.click(
        Surface::Objects,
        harness.object_icon(cube, 0).center(),
        egui::Modifiers::default(),
    );
    assert!(!harness.app.doc.scene.get(cube).unwrap().visible);
    assert!(!harness.app.doc.scene.get(panel).unwrap().visible);
    assert_eq!(harness.app.history.depth(), depth + 1);
    harness.press_key(egui::Key::Z, egui::Modifiers::COMMAND);
    assert!(harness.app.doc.scene.get(cube).unwrap().visible);
    assert!(harness.app.doc.scene.get(panel).unwrap().visible);
    assert_eq!(harness.app.history.depth(), depth);

    let original_count = harness.app.doc.scene.len();
    harness.press_key(egui::Key::Delete, egui::Modifiers::default());
    assert_eq!(harness.app.doc.scene.len(), original_count - 2);
    assert!(harness.app.selection.is_empty());
    assert_eq!(harness.app.history.depth(), depth + 1);
    harness.press_key(egui::Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(harness.app.doc.scene.len(), original_count);
    assert_eq!(harness.app.history.depth(), depth);

    harness.frame(Surface::Objects, Vec::new(), egui::Modifiers::default());
    harness.click(
        Surface::Objects,
        object_row_click(harness.object_row(cube)),
        egui::Modifiers::default(),
    );
    harness.click(
        Surface::Objects,
        object_row_click(harness.object_row(panel)),
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(harness.app.selection.ids(), &[cube, panel]);
    harness.press_key(egui::Key::D, egui::Modifiers::COMMAND);
    assert_eq!(harness.app.doc.scene.len(), original_count + 2);
    assert_eq!(harness.app.selection.len(), 2);
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(
        harness.app.history.undo_label().as_deref(),
        Some("Duplicate 2 Objects")
    );
    harness.press_key(egui::Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(harness.app.doc.scene.len(), original_count);
    assert_eq!(harness.app.history.depth(), depth);
}

#[test]
fn viewport_gizmo_input_transforms_a_multi_selection_as_one_command() {
    let (mut harness, _root) =
        InputHarness::new("input-gizmo", Surface::Viewport, Vec2::new(720.0, 540.0));
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    harness.app.gizmo = GizmoMode::Off;
    let cube_point = harness.viewport_point(cube);
    let panel_point = harness.viewport_point(panel);
    harness.click(Surface::Viewport, cube_point, egui::Modifiers::default());
    harness.click(
        Surface::Viewport,
        panel_point,
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(harness.app.selection.ids(), &[cube, panel]);

    for mode in [GizmoMode::Translate, GizmoMode::Rotate, GizmoMode::Scale] {
        harness.app.gizmo = mode;
        let (start, end) = harness
            .gizmo_handle()
            .unwrap_or_else(|| panic!("no usable gizmo handle for {mode:?}"));
        let before: Vec<(NodeId, Vec3)> = harness
            .app
            .selection_roots()
            .into_iter()
            .map(|id| (id, world_position(&harness.app, id)))
            .collect();
        assert_eq!(before.len(), 2);
        let depth = harness.app.history.depth();

        harness.press(Surface::Viewport, start, egui::Modifiers::default());
        for step in 1..=8 {
            let t = step as f32 / 8.0;
            harness.move_to(
                Surface::Viewport,
                start + (end - start) * t,
                egui::Modifiers::default(),
            );
        }
        let drag = harness
            .app
            .gizmo_drag
            .clone()
            .unwrap_or_else(|| panic!("gizmo drag did not start for {mode:?}"));
        let amount = (end - drag.start_pointer).dot(drag.screen_dir) / drag.screen_len;
        assert!(
            amount.abs() > 0.01,
            "gizmo drag amount was too small: {amount}"
        );
        harness.release(Surface::Viewport, end, egui::Modifiers::default());
        harness.frame(Surface::Viewport, Vec::new(), egui::Modifiers::default());

        assert_eq!(harness.app.history.depth(), depth + 1);
        assert_eq!(
            harness.app.history.undo_label().as_deref(),
            Some("Transform 2 Objects")
        );
        let axis = drag.world_dir.normalize_or(Vec3::Y);
        let scale = (harness.app.orbit.distance * 0.16).clamp(0.3, 6.0);
        for (id, before_world) in &before {
            let expected = match mode {
                GizmoMode::Translate => *before_world + axis * (amount * scale),
                GizmoMode::Rotate => {
                    let rotation = Quat::from_axis_angle(axis, amount * 1.8);
                    drag.pivot + rotation * (*before_world - drag.pivot)
                }
                GizmoMode::Scale => {
                    let factor = (1.0 + amount * 1.2).clamp(0.02, 100.0);
                    let offset = *before_world - drag.pivot;
                    drag.pivot + offset + axis * (axis.dot(offset) * (factor - 1.0))
                }
                GizmoMode::Off => unreachable!(),
            };
            assert_world_close(world_position(&harness.app, *id), expected);
        }

        harness.press_key(egui::Key::Z, egui::Modifiers::COMMAND);
        assert_eq!(harness.app.history.depth(), depth);
        assert_eq!(harness.app.selection.ids(), &[cube, panel]);
        for (id, before_world) in &before {
            assert_world_close(world_position(&harness.app, *id), *before_world);
        }
    }
}

#[test]
fn context_menu_duplicate_and_delete_dispatch_real_items_for_the_whole_selection() {
    let (mut harness, _root) = InputHarness::new(
        "input-context-edit",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    let original = harness.app.doc.signature();
    let original_count = harness.app.doc.scene.len();

    select_object_rows(&mut harness, &[cube, panel]);
    let depth = harness.app.history.depth();
    harness.open_context_menu(cube);
    assert_eq!(
        harness.app.selection.ids(),
        &[cube, panel],
        "opening a context menu on a selected row must not collapse the selection"
    );
    harness.click_menu_item("ctx:Duplicate");

    assert_eq!(harness.app.doc.scene.len(), original_count + 2);
    assert_eq!(harness.app.selection.len(), 2);
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(
        harness.app.history.undo_label().as_deref(),
        Some("Duplicate 2 Objects")
    );
    assert_selection_reflected(&harness, harness.app.selection.ids());

    harness.undo_via_menu();
    assert_eq!(harness.app.doc.signature(), original);
    assert_eq!(harness.app.history.depth(), depth);

    select_object_rows(&mut harness, &[cube, panel]);
    harness.open_context_menu(cube);
    harness.click_menu_item("ctx:Delete");
    assert_eq!(harness.app.doc.scene.len(), original_count - 2);
    assert!(harness.app.selection.is_empty());
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(
        harness.app.history.undo_label().as_deref(),
        Some("Delete 2 Objects")
    );
    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    let visible = harness.viewport_hits().0;
    assert!(!visible.contains(&cube) && !visible.contains(&panel));

    harness.undo_via_menu();
    assert_eq!(harness.app.doc.signature(), original);
    assert_eq!(harness.app.doc.scene.len(), original_count);
}

#[test]
fn context_menu_group_unparent_reparent_frame_and_select_use_real_items() {
    let (mut harness, _root) = InputHarness::new(
        "input-context-structure",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    let studio = named(&harness.app, "Studio");
    let original = harness.app.doc.signature();

    select_object_rows(&mut harness, &[cube, panel]);
    let depth = harness.app.history.depth();
    harness.open_context_menu(cube);
    harness.click_menu_item("ctx:Group Selection");
    let group = named(&harness.app, "Group");
    assert_eq!(harness.app.selection.ids(), &[group]);
    assert_eq!(harness.app.doc.scene.get(cube).unwrap().parent, Some(group));
    assert_eq!(
        harness.app.doc.scene.get(panel).unwrap().parent,
        Some(group)
    );
    assert_eq!(harness.app.history.depth(), depth + 1);
    harness.undo_via_menu();
    assert_eq!(harness.app.doc.signature(), original);

    select_object_rows(&mut harness, &[cube, panel]);
    harness.open_context_menu(cube);
    harness.click_menu_item("ctx:Unparent Selection");
    assert_eq!(harness.app.doc.scene.get(cube).unwrap().parent, None);
    assert_eq!(harness.app.doc.scene.get(panel).unwrap().parent, None);
    assert_eq!(harness.app.history.depth(), depth + 1);
    harness.undo_via_menu();
    assert_eq!(harness.app.doc.signature(), original);

    select_object_rows(&mut harness, &[cube, panel]);
    harness.open_context_menu(cube);
    harness.open_submenu("ctx:Reparent Selection to...");
    harness.click_menu_item("ctx:Reparent:Studio");
    assert_eq!(
        harness.app.doc.scene.get(cube).unwrap().parent,
        Some(studio)
    );
    assert_eq!(
        harness.app.doc.scene.get(panel).unwrap().parent,
        Some(studio)
    );
    assert_eq!(harness.app.history.depth(), depth + 1);
    harness.undo_via_menu();
    assert_eq!(harness.app.doc.signature(), original);

    select_object_rows(&mut harness, &[cube, panel]);
    harness.open_context_menu(cube);
    harness.open_submenu("ctx:Reparent Selection to...");
    harness.click_menu_item("ctx:Reparent:Scene Root");
    assert_eq!(harness.app.doc.scene.get(cube).unwrap().parent, None);
    assert_eq!(harness.app.doc.scene.get(panel).unwrap().parent, None);
    assert_eq!(harness.app.history.depth(), depth + 1);
    harness.undo_via_menu();
    assert_eq!(harness.app.doc.signature(), original);

    select_object_rows(&mut harness, &[cube, panel]);
    let cube_center = world_position(&harness.app, cube);
    let distance_before = harness.app.orbit.distance;
    harness.open_context_menu(cube);
    harness.click_menu_item("ctx:Frame Selected");
    assert_eq!(
        harness.app.selection.ids(),
        &[cube, panel],
        "Frame Selected must not collapse a multi-selection to the context row"
    );
    assert!(
        (harness.app.orbit.target - cube_center).length() > 0.05,
        "frame target should cover both selected objects, not just the context row"
    );
    assert!(harness.app.orbit.distance > 0.0 && distance_before > 0.0);
    assert_eq!(harness.app.history.depth(), depth);

    select_object_rows(&mut harness, &[cube, panel]);
    harness.open_context_menu(panel);
    harness.click_menu_item("ctx:Select");
    assert_eq!(harness.app.selection.ids(), &[panel]);
    assert_eq!(harness.app.history.depth(), depth);
}

#[test]
fn object_menu_flags_convert_subdivision_boolean_and_uv_apply_to_the_whole_selection() {
    let (mut harness, _root) = InputHarness::new(
        "input-object-menu",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    let ground = named(&harness.app, "Ground");
    let original = harness.app.doc.signature();

    select_object_rows(&mut harness, &[cube, panel]);
    let mut depth = harness.app.history.depth();
    harness.open_menu("menu:Objects");
    harness.open_submenu("Objects:Selected Flags");
    harness.click_menu_item("Objects:Flags:Hide Selected");
    assert!(!harness.app.doc.scene.get(cube).unwrap().visible);
    assert!(!harness.app.doc.scene.get(panel).unwrap().visible);
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(harness.object_flags(cube), Some((false, true, false)));
    assert_eq!(harness.object_flags(panel), Some((false, true, false)));
    let visible = harness.viewport_hits().0;
    assert!(!visible.contains(&cube) && !visible.contains(&panel));
    harness.open_menu("menu:Objects");
    harness.open_submenu("Objects:Selected Flags");
    harness.click_menu_item("Objects:Flags:Show Selected");
    assert!(harness.app.doc.scene.get(cube).unwrap().visible);
    assert!(harness.app.doc.scene.get(panel).unwrap().visible);
    assert_eq!(harness.app.history.depth(), depth + 2);
    harness.undo_via_menu();
    assert!(!harness.app.doc.scene.get(cube).unwrap().visible);
    assert!(!harness.app.doc.scene.get(panel).unwrap().visible);
    harness.undo_via_menu();
    assert!(harness.app.doc.scene.get(cube).unwrap().visible);
    assert!(harness.app.doc.scene.get(panel).unwrap().visible);
    assert_eq!(harness.app.history.depth(), depth);

    harness.open_menu("menu:Objects");
    harness.open_submenu("Objects:Selected Flags");
    harness.click_menu_item("Objects:Flags:Lock Selected");
    assert!(harness.app.doc.scene.get(cube).unwrap().locked);
    assert!(harness.app.doc.scene.get(panel).unwrap().locked);
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(harness.object_flags(cube), Some((true, true, true)));
    assert_eq!(harness.object_flags(panel), Some((true, true, true)));
    harness.open_menu("menu:Objects");
    harness.open_submenu("Objects:Selected Flags");
    harness.click_menu_item("Objects:Flags:Unlock Selected");
    assert!(!harness.app.doc.scene.get(cube).unwrap().locked);
    assert!(!harness.app.doc.scene.get(panel).unwrap().locked);
    assert_eq!(harness.app.history.depth(), depth + 2);
    harness.undo_via_menu();
    assert!(harness.app.doc.scene.get(cube).unwrap().locked);
    assert!(harness.app.doc.scene.get(panel).unwrap().locked);
    harness.undo_via_menu();
    assert!(!harness.app.doc.scene.get(cube).unwrap().locked);
    assert!(!harness.app.doc.scene.get(panel).unwrap().locked);
    assert_eq!(harness.app.history.depth(), depth);

    harness.open_menu("menu:Objects");
    harness.open_submenu("Objects:Selected Flags");
    harness.click_menu_item("Objects:Flags:Disable Render on Selected");
    assert!(!harness.app.doc.scene.get(cube).unwrap().renderable);
    assert!(!harness.app.doc.scene.get(panel).unwrap().renderable);
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(harness.object_flags(cube), Some((true, false, false)));
    assert_eq!(harness.object_flags(panel), Some((true, false, false)));
    harness.open_menu("menu:Objects");
    harness.open_submenu("Objects:Selected Flags");
    harness.click_menu_item("Objects:Flags:Enable Render on Selected");
    assert!(harness.app.doc.scene.get(cube).unwrap().renderable);
    assert!(harness.app.doc.scene.get(panel).unwrap().renderable);
    assert_eq!(harness.app.history.depth(), depth + 2);
    harness.undo_via_menu();
    assert!(!harness.app.doc.scene.get(cube).unwrap().renderable);
    assert!(!harness.app.doc.scene.get(panel).unwrap().renderable);
    harness.undo_via_menu();
    assert!(harness.app.doc.scene.get(cube).unwrap().renderable);
    assert!(harness.app.doc.scene.get(panel).unwrap().renderable);
    assert_eq!(harness.app.history.depth(), depth);

    harness.open_menu("menu:Selection");
    harness.click_menu_item("Selection:Convert to Editable Mesh");
    assert!(editable_mesh(&harness.app, cube));
    assert!(editable_mesh(&harness.app, panel));
    assert_selection_reflected(&harness, &[cube, panel]);
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(
        harness.app.history.undo_label().as_deref(),
        Some("Edit Geometry on 2 Objects")
    );
    harness.undo_via_menu();
    assert!(!editable_mesh(&harness.app, cube));
    assert!(!editable_mesh(&harness.app, panel));
    assert_eq!(harness.app.doc.signature(), original);

    let cube_subdivisions = subdivision_count(&harness.app, cube);
    let panel_subdivisions = subdivision_count(&harness.app, panel);
    harness.open_menu("menu:Objects");
    harness.click_menu_item("Objects:Add Subdivision");
    assert_eq!(subdivision_count(&harness.app, cube), cube_subdivisions + 1);
    assert_eq!(
        subdivision_count(&harness.app, panel),
        panel_subdivisions + 1
    );
    assert_selection_reflected(&harness, &[cube, panel]);
    assert_eq!(harness.app.history.depth(), depth + 1);
    harness.undo_via_menu();
    assert_eq!(subdivision_count(&harness.app, cube), cube_subdivisions);
    assert_eq!(subdivision_count(&harness.app, panel), panel_subdivisions);

    harness.open_menu("menu:Objects");
    harness.click_menu_item("Objects:Add Boolean Subtract");
    let cube_operands = boolean_operands(&harness.app, cube);
    let panel_operands = boolean_operands(&harness.app, panel);
    assert_eq!(cube_operands.len(), 1);
    assert_eq!(panel_operands, cube_operands);
    assert_eq!(cube_operands[0], ground);
    assert_selection_reflected(&harness, &[cube, panel]);
    assert_eq!(harness.app.history.depth(), depth + 1);
    harness.undo_via_menu();
    assert!(boolean_operands(&harness.app, cube).is_empty());
    assert!(boolean_operands(&harness.app, panel).is_empty());

    harness.open_menu("menu:Objects");
    harness.click_menu_item("Objects:Convert to Editable Mesh");
    assert!(editable_mesh(&harness.app, cube));
    assert!(editable_mesh(&harness.app, panel));
    depth = harness.app.history.depth();
    for key in [
        "Tools:UV Projection:Planar",
        "Tools:UV Projection:Box",
        "Tools:UV Projection:Cylindrical",
        "Tools:UV Projection:Spherical",
        "Tools:UV Projection:Automatic Unwrap",
        "Tools:UV Projection:Relax",
        "Tools:UV Projection:Normalize",
    ] {
        let before_uv = harness.app.doc.signature();
        harness.open_menu("menu:Tools");
        harness.open_submenu("Tools:UV Projection");
        harness.click_menu_item(key);
        assert_eq!(harness.app.history.depth(), depth + 1, "{key}");
        assert_eq!(
            harness.app.history.undo_label().as_deref(),
            Some("Edit Geometry on 2 Objects"),
            "{key}"
        );
        harness.undo_via_menu();
        assert_eq!(harness.app.doc.signature(), before_uv, "{key}");
        assert_eq!(harness.app.history.depth(), depth, "{key}");
        assert_selection_reflected(&harness, &[cube, panel]);
    }
    assert!(editable_mesh(&harness.app, cube));
    assert!(editable_mesh(&harness.app, panel));
}

#[test]
fn edit_selection_objects_tools_and_animation_menus_use_the_shared_selection() {
    let (mut harness, _root) =
        InputHarness::new("input-menus", Surface::Shell, Vec2::new(1400.0, 1000.0));
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    let original = harness.app.doc.signature();
    let original_count = harness.app.doc.scene.len();

    select_object_rows(&mut harness, &[cube, panel]);
    let depth = harness.app.history.depth();
    harness.open_menu("menu:Edit");
    harness.click_menu_item("Edit:Duplicate");
    assert_eq!(harness.app.doc.scene.len(), original_count + 2);
    assert_eq!(harness.app.selection.len(), 2);
    assert_eq!(harness.app.history.depth(), depth + 1);
    harness.undo_via_menu();
    assert_eq!(harness.app.doc.signature(), original);

    select_object_rows(&mut harness, &[cube, panel]);
    harness.open_menu("menu:Edit");
    harness.click_menu_item("Edit:Delete");
    assert_eq!(harness.app.doc.scene.len(), original_count - 2);
    assert!(harness.app.selection.is_empty());
    assert_eq!(harness.app.history.depth(), depth + 1);
    harness.undo_via_menu();
    assert_eq!(harness.app.doc.signature(), original);

    harness.open_menu("menu:Selection");
    harness.click_menu_item("Selection:Select All Objects");
    assert_eq!(harness.app.selection.len(), harness.app.doc.scene.len());
    assert_eq!(harness.app.history.depth(), depth);
    harness.open_menu("menu:Selection");
    harness.click_menu_item("Selection:Deselect All Objects");
    assert!(harness.app.selection.is_empty());
    assert_eq!(harness.app.history.depth(), depth);

    select_object_rows(&mut harness, &[cube, panel]);
    harness.open_menu("menu:Objects");
    harness.click_menu_item("Objects:Group Selection");
    assert_eq!(
        named(&harness.app, "Group"),
        harness.app.selection.primary().unwrap()
    );
    assert_eq!(harness.app.history.depth(), depth + 1);
    harness.undo_via_menu();
    assert_eq!(harness.app.doc.signature(), original);

    for (key, mode) in [
        ("Tools:Move", GizmoMode::Translate),
        ("Tools:Rotate", GizmoMode::Rotate),
        ("Tools:Scale", GizmoMode::Scale),
        ("Tools:No Gizmo", GizmoMode::Off),
    ] {
        select_object_rows(&mut harness, &[cube, panel]);
        harness.open_menu("menu:Tools");
        harness.click_menu_item(key);
        assert_eq!(harness.app.gizmo, mode);
        assert_eq!(harness.app.selection.ids(), &[cube, panel]);
        assert_eq!(harness.app.history.depth(), depth);
    }

    select_object_rows(&mut harness, &[cube, panel]);
    let cube_center = world_position(&harness.app, cube);
    harness.open_menu("menu:Tools");
    harness.click_menu_item("Tools:Frame Selected");
    assert_eq!(harness.app.selection.ids(), &[cube, panel]);
    assert!((harness.app.orbit.target - cube_center).length() > 0.05);
    assert_eq!(harness.app.history.depth(), depth);

    select_object_rows(&mut harness, &[cube, panel]);
    let before_animations: Vec<_> = [cube, panel]
        .iter()
        .map(|id| harness.app.doc.scene.get(*id).unwrap().animation.clone())
        .collect();
    harness.open_menu("menu:Animation");
    harness.click_menu_item("Animation:Key Transforms");
    for (index, id) in [cube, panel].iter().enumerate() {
        assert_ne!(
            harness.app.doc.scene.get(*id).unwrap().animation,
            before_animations[index],
            "Key Transforms should key every selected object"
        );
    }
    assert_selection_reflected(&harness, &[cube, panel]);
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(
        harness.app.history.undo_label().as_deref(),
        Some("Edit Animation on 2 Objects")
    );
    assert!(!harness.app.key_transforms_ready());
    let keyed_depth = harness.app.history.depth();
    harness.app.key_transform_channels();
    assert_eq!(harness.app.history.depth(), keyed_depth);
    harness.undo_via_menu();
    for (index, id) in [cube, panel].iter().enumerate() {
        assert_eq!(
            harness.app.doc.scene.get(*id).unwrap().animation,
            before_animations[index]
        );
    }
}

#[test]
fn toolbar_gizmo_frame_and_subdivision_icons_preserve_and_apply_to_the_selection() {
    let (mut harness, _root) =
        InputHarness::new("input-toolbar", Surface::Shell, Vec2::new(1400.0, 1000.0));
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    select_object_rows(&mut harness, &[cube, panel]);
    let depth = harness.app.history.depth();

    for (key, mode) in [
        ("toolbar:Move", GizmoMode::Translate),
        ("toolbar:Rotate", GizmoMode::Rotate),
        ("toolbar:Scale", GizmoMode::Scale),
    ] {
        let (rect, enabled) = harness.menu_item(key);
        assert!(enabled, "toolbar action {key:?} should be enabled");
        harness.click(Surface::Shell, rect.center(), egui::Modifiers::default());
        assert_eq!(harness.app.gizmo, mode);
        assert_eq!(harness.app.selection.ids(), &[cube, panel]);
        assert_eq!(harness.app.history.depth(), depth);
    }

    let cube_center = world_position(&harness.app, cube);
    let (rect, enabled) = harness.menu_item("toolbar:Frame Selected");
    assert!(enabled);
    harness.click(Surface::Shell, rect.center(), egui::Modifiers::default());
    assert_eq!(harness.app.selection.ids(), &[cube, panel]);
    assert!((harness.app.orbit.target - cube_center).length() > 0.05);
    assert_eq!(harness.app.history.depth(), depth);

    let cube_subdivisions = subdivision_count(&harness.app, cube);
    let panel_subdivisions = subdivision_count(&harness.app, panel);
    let (rect, enabled) = harness.menu_item("toolbar:Add Subdivision");
    assert!(enabled);
    harness.click(Surface::Shell, rect.center(), egui::Modifiers::default());
    assert_eq!(subdivision_count(&harness.app, cube), cube_subdivisions + 1);
    assert_eq!(
        subdivision_count(&harness.app, panel),
        panel_subdivisions + 1
    );
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(
        harness.app.history.undo_label().as_deref(),
        Some("Edit Geometry on 2 Objects")
    );
    harness.undo_via_menu();
    assert_eq!(subdivision_count(&harness.app, cube), cube_subdivisions);
    assert_eq!(subdivision_count(&harness.app, panel), panel_subdivisions);
}

#[test]
fn single_selection_menu_actions_keep_single_object_undo() {
    let (mut harness, _root) = InputHarness::new(
        "input-single-menu",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let original = harness.app.doc.signature();
    let original_count = harness.app.doc.scene.len();

    select_object_rows(&mut harness, &[cube]);
    let depth = harness.app.history.depth();
    harness.open_context_menu(cube);
    harness.click_menu_item("ctx:Duplicate");
    assert_eq!(harness.app.doc.scene.len(), original_count + 1);
    assert_eq!(harness.app.selection.len(), 1);
    assert_selection_reflected(&harness, harness.app.selection.ids());
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(
        harness.app.history.undo_label().as_deref(),
        Some("Duplicate")
    );
    harness.undo_via_menu();
    assert_eq!(harness.app.doc.signature(), original);

    select_object_rows(&mut harness, &[cube]);
    harness.open_context_menu(cube);
    harness.click_menu_item("ctx:Delete");
    assert_eq!(harness.app.doc.scene.len(), original_count - 1);
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(
        harness.app.history.undo_label().as_deref(),
        Some("Delete Cube")
    );
    harness.undo_via_menu();
    assert_eq!(harness.app.doc.signature(), original);

    select_object_rows(&mut harness, &[cube]);
    harness.open_menu("menu:Objects");
    harness.open_submenu("Objects:Selected Flags");
    harness.click_menu_item("Objects:Flags:Hide Selected");
    assert!(!harness.app.doc.scene.get(cube).unwrap().visible);
    assert_eq!(harness.app.history.depth(), depth + 1);
    harness.undo_via_menu();
    assert!(harness.app.doc.scene.get(cube).unwrap().visible);

    select_object_rows(&mut harness, &[cube]);
    harness.open_menu("menu:Objects");
    harness.open_submenu("Objects:Selected Flags");
    harness.click_menu_item("Objects:Flags:Lock Selected");
    assert!(harness.app.doc.scene.get(cube).unwrap().locked);
    assert_eq!(harness.app.history.depth(), depth + 1);
    harness.undo_via_menu();
    assert!(!harness.app.doc.scene.get(cube).unwrap().locked);

    let subdivisions = subdivision_count(&harness.app, cube);
    select_object_rows(&mut harness, &[cube]);
    harness.open_menu("menu:Objects");
    harness.click_menu_item("Objects:Add Subdivision");
    assert_eq!(subdivision_count(&harness.app, cube), subdivisions + 1);
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(
        harness.app.history.undo_label().as_deref(),
        Some("Edit Geometry")
    );
    harness.undo_via_menu();
    assert_eq!(subdivision_count(&harness.app, cube), subdivisions);

    select_object_rows(&mut harness, &[cube]);
    harness.open_menu("menu:Selection");
    harness.click_menu_item("Selection:Convert to Editable Mesh");
    assert!(editable_mesh(&harness.app, cube));
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(
        harness.app.history.undo_label().as_deref(),
        Some("Edit Geometry")
    );
    harness.undo_via_menu();
    assert!(!editable_mesh(&harness.app, cube));

    let before_animation = harness.app.doc.scene.get(cube).unwrap().animation.clone();
    select_object_rows(&mut harness, &[cube]);
    harness.open_menu("menu:Animation");
    harness.click_menu_item("Animation:Key Transforms");
    assert_ne!(
        harness.app.doc.scene.get(cube).unwrap().animation,
        before_animation
    );
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(
        harness.app.history.undo_label().as_deref(),
        Some("Edit Animation")
    );
    harness.undo_via_menu();
    assert_eq!(
        harness.app.doc.scene.get(cube).unwrap().animation,
        before_animation
    );
    assert_eq!(harness.app.doc.signature(), original);
}

#[test]
fn mesh_only_menu_and_toolbar_actions_are_disabled_for_non_mesh_selections() {
    let (mut harness, _root) = InputHarness::new(
        "input-menu-disabled",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let key_light = named(&harness.app, "Key Light");
    let camera = named(&harness.app, "Camera");
    select_object_rows(&mut harness, &[key_light, camera]);

    harness.open_menu("menu:Objects");
    assert!(!harness.menu_item("Objects:Add Subdivision").1);
    assert!(!harness.menu_item("Objects:Add Boolean Subtract").1);
    harness.open_menu("menu:Selection");
    assert!(!harness.menu_item("Selection:Convert to Editable Mesh").1);
    assert!(!harness.menu_item("toolbar:Add Subdivision").1);
    harness.open_menu("menu:Tools");
    harness.open_submenu("Tools:UV Projection");
    assert!(!harness.menu_item("Tools:UV Projection:Box").1);
    assert_eq!(harness.app.selection.ids(), &[key_light, camera]);
    assert_eq!(harness.app.history.depth(), 0);

    harness.open_menu("menu:Selection");
    harness.click_menu_item("Selection:Deselect All Objects");
    assert!(harness.app.selection.is_empty());
    harness.open_menu("menu:Objects");
    harness.open_submenu("Objects:Selected Flags");
    assert!(!harness.menu_item("Objects:Flags:Hide Selected").1);
    harness.open_menu("menu:Animation");
    assert!(!harness.menu_item("Animation:Key Transforms").1);
    assert_eq!(harness.app.history.depth(), 0);

    harness.click(
        Surface::Shell,
        object_row_click(harness.object_row(cube)),
        egui::Modifiers::default(),
    );
    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    assert_eq!(harness.app.selection.ids(), &[cube]);
    harness.open_menu("menu:Objects");
    assert!(harness.menu_item("Objects:Add Subdivision").1);
    assert!(harness.menu_item("toolbar:Add Subdivision").1);
    harness.open_menu("menu:Tools");
    harness.open_submenu("Tools:UV Projection");
    assert!(!harness.menu_item("Tools:UV Projection:Box").1);
}

#[test]
fn component_and_topology_keys_follow_empty_single_multi_and_mixed_selections() {
    let (mut harness, _root) = InputHarness::new(
        "input-component-enabled",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    let key_light = named(&harness.app, "Key Light");
    let mode_keys = [
        "Selection:Point Mode",
        "Selection:Edge Mode",
        "Selection:Polygon Mode",
    ];
    let topology_keys = [
        "Tools:Extrude",
        "Tools:Inset",
        "Tools:Bevel",
        "Tools:Ring Cut",
        "Tools:Weld Points",
        "Tools:Bridge Loops",
    ];

    harness.app.selection.clear();
    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    harness.open_menu("menu:Selection");
    assert!(harness.menu_item("Selection:Object Mode").1);
    for key in mode_keys {
        assert!(
            !harness.menu_item(key).1,
            "{key} should be disabled when empty"
        );
    }
    assert!(!harness.menu_item("Selection:Select All Components").1);
    assert!(!harness.menu_item("Selection:Deselect All Components").1);
    assert!(!harness.menu_item("toolbar:Point Mode").1);
    harness.open_menu("menu:Tools");
    for key in topology_keys {
        assert!(
            !harness.menu_item(key).1,
            "{key} should be disabled when empty"
        );
    }

    select_object_rows(&mut harness, &[cube, key_light]);
    harness.open_menu("menu:Selection");
    for key in mode_keys {
        assert!(
            !harness.menu_item(key).1,
            "{key} should be disabled for a mixed selection"
        );
    }
    assert!(!harness.menu_item("Selection:Select All Components").1);
    assert!(!harness.menu_item("toolbar:Polygon Mode").1);
    harness.open_menu("menu:Tools");
    for key in topology_keys {
        assert!(
            !harness.menu_item(key).1,
            "{key} should be disabled for a mixed selection"
        );
    }
    let depth = harness.app.history.depth();
    harness.app.set_mesh_mode(MeshSelectMode::Points);
    assert!(!harness.app.mesh_edit);
    assert!(harness.app.status.contains("mesh-only"));
    harness.app.perform_mesh_action(MeshAction::SelectAll);
    assert!(harness.app.mesh_components.is_empty());
    harness.app.perform_mesh_action(MeshAction::Extrude);
    assert!(harness.app.status.contains("editable mesh"));
    assert_eq!(harness.app.history.depth(), depth);

    select_object_rows(&mut harness, &[cube]);
    harness.open_menu("menu:Selection");
    for key in mode_keys {
        assert!(
            harness.menu_item(key).1,
            "{key} should be enabled for one mesh"
        );
    }
    assert!(!harness.menu_item("Selection:Select All Components").1);
    assert!(harness.menu_item("toolbar:Point Mode").1);
    harness.open_menu("menu:Tools");
    for key in topology_keys {
        assert!(
            !harness.menu_item(key).1,
            "{key} should be disabled outside component mode"
        );
    }

    select_object_rows(&mut harness, &[cube, panel]);
    harness.open_menu("menu:Selection");
    for key in mode_keys {
        assert!(
            harness.menu_item(key).1,
            "{key} should be enabled for two meshes"
        );
    }
    assert!(!harness.menu_item("Selection:Select All Components").1);
    assert!(harness.menu_item("toolbar:Edge Mode").1);
    harness.open_menu("menu:Tools");
    for key in topology_keys {
        assert!(
            !harness.menu_item(key).1,
            "{key} should be disabled outside component mode"
        );
    }
}

#[test]
fn component_modes_and_select_all_cover_every_selected_mesh_with_one_conversion_undo() {
    let (mut harness, _root) = InputHarness::new(
        "input-component-multi",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    select_object_rows(&mut harness, &[cube, panel]);
    let depth = harness.app.history.depth();

    harness.open_menu("menu:Selection");
    harness.click_menu_item("Selection:Point Mode");
    assert!(harness.app.mesh_edit);
    assert_eq!(harness.app.mesh_select_mode, MeshSelectMode::Points);
    assert!(editable_mesh(&harness.app, cube));
    assert!(editable_mesh(&harness.app, panel));
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(
        harness.app.history.undo_label().as_deref(),
        Some("Edit Geometry on 2 Objects")
    );

    harness.open_menu("menu:Selection");
    harness.click_menu_item("Selection:Select All Components");
    for id in [cube, panel] {
        let point_count = selected_mesh(&harness.app, id).positions.len() as u32;
        assert_eq!(
            harness.app.mesh_components.get(&id).map(Vec::len),
            Some(point_count as usize),
            "select-all should cover {id:?}"
        );
    }
    assert_eq!(
        harness.app.mesh_selected.len(),
        selected_mesh(&harness.app, panel).positions.len()
    );
    assert_eq!(harness.app.history.depth(), depth + 1);

    harness.open_menu("menu:Selection");
    harness.click_menu_item("Selection:Deselect All Components");
    assert!(harness.app.mesh_components.values().all(Vec::is_empty));
    assert!(harness.app.mesh_selected.is_empty());
    assert_eq!(harness.app.history.depth(), depth + 1);

    for (key, mode) in [
        ("Selection:Edge Mode", MeshSelectMode::Edges),
        ("Selection:Polygon Mode", MeshSelectMode::Polygons),
    ] {
        harness.open_menu("menu:Selection");
        harness.click_menu_item(key);
        assert_eq!(harness.app.mesh_select_mode, mode);
        assert_eq!(harness.app.history.depth(), depth + 1);
    }

    for (key, mode) in [
        ("toolbar:Point Mode", MeshSelectMode::Points),
        ("toolbar:Edge Mode", MeshSelectMode::Edges),
        ("toolbar:Polygon Mode", MeshSelectMode::Polygons),
    ] {
        let (rect, enabled) = harness.menu_item(key);
        assert!(enabled, "{key} should be enabled for two editable meshes");
        harness.click(Surface::Shell, rect.center(), egui::Modifiers::default());
        assert_eq!(harness.app.mesh_select_mode, mode);
        assert_eq!(harness.app.history.depth(), depth + 1);
    }
    let (rect, enabled) = harness.menu_item("toolbar:Object Mode");
    assert!(enabled);
    harness.click(Surface::Shell, rect.center(), egui::Modifiers::default());
    assert!(!harness.app.mesh_edit);
    assert!(harness.app.mesh_components.is_empty());

    harness.open_menu("menu:Selection");
    harness.click_menu_item("Selection:Point Mode");
    harness.open_menu("menu:Selection");
    harness.click_menu_item("Selection:Object Mode");
    assert!(!harness.app.mesh_edit);
    assert_eq!(harness.app.history.depth(), depth + 1);

    harness.undo_via_menu();
    assert!(!editable_mesh(&harness.app, cube));
    assert!(!editable_mesh(&harness.app, panel));
    assert_eq!(harness.app.history.depth(), depth);
}

#[test]
fn component_select_all_and_none_keep_single_mesh_behavior() {
    let (mut harness, _root) = InputHarness::new(
        "input-component-single",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    select_object_rows(&mut harness, &[cube]);
    let depth = harness.app.history.depth();

    harness.open_menu("menu:Selection");
    harness.click_menu_item("Selection:Polygon Mode");
    assert!(harness.app.mesh_edit);
    assert!(editable_mesh(&harness.app, cube));
    assert_eq!(harness.app.history.depth(), depth + 1);

    harness.open_menu("menu:Selection");
    harness.click_menu_item("Selection:Select All Components");
    let polygons = selected_mesh(&harness.app, cube).polygons.len();
    assert_eq!(harness.app.mesh_selected.len(), polygons);
    assert_eq!(
        harness.app.mesh_components.get(&cube).map(Vec::len),
        Some(polygons)
    );
    assert_eq!(harness.app.history.depth(), depth + 1);

    harness.open_menu("menu:Selection");
    harness.click_menu_item("Selection:Deselect All Components");
    assert!(harness.app.mesh_selected.is_empty());
    assert_eq!(harness.app.history.depth(), depth + 1);

    harness.undo_via_menu();
    assert!(!editable_mesh(&harness.app, cube));
    assert_eq!(harness.app.history.depth(), depth);
}

#[test]
fn mesh_topology_tools_apply_to_every_selected_mesh_with_one_undo() {
    let (mut harness, _root) = InputHarness::new(
        "input-topology-multi",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    select_object_rows(&mut harness, &[cube, panel]);
    let depth = harness.app.history.depth();

    for (key, mode, bridge) in [
        ("Tools:Extrude", MeshSelectMode::Polygons, false),
        ("Tools:Inset", MeshSelectMode::Polygons, false),
        ("Tools:Bevel", MeshSelectMode::Polygons, false),
        ("Tools:Ring Cut", MeshSelectMode::Polygons, false),
        ("Tools:Weld Points", MeshSelectMode::Points, false),
        ("Tools:Bridge Loops", MeshSelectMode::Edges, true),
    ] {
        let base = if bridge {
            bridge_fixture_mesh()
        } else {
            lc_geom::box_mesh(1.0, 1.0, 1.0)
        };
        set_editable_geometry(&mut harness.app, cube, base.clone());
        set_editable_geometry(&mut harness.app, panel, base.clone());
        harness.app.mesh_edit = true;
        harness.app.mesh_select_mode = mode;
        let components: Vec<u32> = match key {
            "Tools:Weld Points" => vec![0, 1],
            "Tools:Ring Cut" => Vec::new(),
            "Tools:Bridge Loops" => (0..base.edges().len() as u32).collect(),
            _ => (0..base.polygons.len() as u32).collect(),
        };
        set_selected_components(&mut harness.app, &[cube, panel], &components);
        let before_signature = harness.app.doc.signature();
        let before_cube = selected_mesh(&harness.app, cube);
        let before_panel = selected_mesh(&harness.app, panel);

        harness.open_menu("menu:Tools");
        harness.click_menu_item(key);
        assert_eq!(harness.app.history.depth(), depth + 1, "{key}");
        assert_eq!(
            harness.app.history.undo_label().as_deref(),
            Some("Edit Geometry on 2 Objects"),
            "{key}"
        );
        assert_ne!(selected_mesh(&harness.app, cube), before_cube, "{key} cube");
        assert_ne!(
            selected_mesh(&harness.app, panel),
            before_panel,
            "{key} panel"
        );
        harness.undo_via_menu();
        assert_eq!(harness.app.doc.signature(), before_signature, "{key}");
        assert_eq!(harness.app.history.depth(), depth, "{key}");
    }
}

#[test]
fn mesh_topology_tools_keep_single_mesh_undo_and_untouched_objects() {
    let (mut harness, _root) = InputHarness::new(
        "input-topology-single",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    select_object_rows(&mut harness, &[cube]);
    let depth = harness.app.history.depth();
    let untouched = harness.app.doc.scene.signature();

    for (key, mode, bridge) in [
        ("Tools:Extrude", MeshSelectMode::Polygons, false),
        ("Tools:Inset", MeshSelectMode::Polygons, false),
        ("Tools:Bevel", MeshSelectMode::Polygons, false),
        ("Tools:Ring Cut", MeshSelectMode::Polygons, false),
        ("Tools:Weld Points", MeshSelectMode::Points, false),
        ("Tools:Bridge Loops", MeshSelectMode::Edges, true),
    ] {
        let base = if bridge {
            bridge_fixture_mesh()
        } else {
            lc_geom::box_mesh(1.0, 1.0, 1.0)
        };
        set_editable_geometry(&mut harness.app, cube, base.clone());
        harness.app.mesh_edit = true;
        harness.app.mesh_select_mode = mode;
        let components: Vec<u32> = match key {
            "Tools:Weld Points" => vec![0, 1],
            "Tools:Ring Cut" => Vec::new(),
            "Tools:Bridge Loops" => (0..base.edges().len() as u32).collect(),
            _ => (0..base.polygons.len() as u32).collect(),
        };
        harness.app.mesh_components.clear();
        set_selected_components(&mut harness.app, &[cube], &components);
        let before_signature = harness.app.doc.signature();
        let before_cube = selected_mesh(&harness.app, cube);

        harness.open_menu("menu:Tools");
        harness.click_menu_item(key);
        assert_eq!(harness.app.history.depth(), depth + 1, "{key}");
        assert_eq!(
            harness.app.history.undo_label().as_deref(),
            Some("Edit Geometry"),
            "{key}"
        );
        assert_ne!(selected_mesh(&harness.app, cube), before_cube, "{key}");
        assert!(!editable_mesh(&harness.app, panel));
        harness.undo_via_menu();
        assert_eq!(harness.app.doc.signature(), before_signature, "{key}");
        assert_eq!(harness.app.history.depth(), depth, "{key}");
    }
    let _ = untouched;
}

#[test]
fn mesh_topology_tools_refuse_a_component_selection_missing_from_one_mesh() {
    let (mut harness, _root) = InputHarness::new(
        "input-topology-partial",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    set_editable_geometry(&mut harness.app, cube, lc_geom::box_mesh(1.0, 1.0, 1.0));
    set_editable_geometry(&mut harness.app, panel, lc_geom::box_mesh(1.0, 1.0, 1.0));
    select_object_rows(&mut harness, &[cube, panel]);
    harness.app.mesh_edit = true;
    let depth = harness.app.history.depth();

    for (key, mode, action, components) in [
        (
            "Tools:Extrude",
            MeshSelectMode::Polygons,
            MeshAction::Extrude,
            (0..6u32).collect::<Vec<_>>(),
        ),
        (
            "Tools:Inset",
            MeshSelectMode::Polygons,
            MeshAction::Inset,
            (0..6u32).collect::<Vec<_>>(),
        ),
        (
            "Tools:Bevel",
            MeshSelectMode::Polygons,
            MeshAction::Bevel,
            (0..6u32).collect::<Vec<_>>(),
        ),
        (
            "Tools:Weld Points",
            MeshSelectMode::Points,
            MeshAction::Weld,
            vec![0, 1],
        ),
        (
            "Tools:Bridge Loops",
            MeshSelectMode::Edges,
            MeshAction::Bridge,
            (0..12u32).collect::<Vec<_>>(),
        ),
    ] {
        harness.app.mesh_select_mode = mode;
        harness.app.mesh_components.clear();
        harness.app.mesh_selected = components.clone();
        harness.app.mesh_components.insert(cube, components.clone());
        let before_signature = harness.app.doc.signature();

        harness.open_menu("menu:Tools");
        assert!(
            !harness.menu_item(key).1,
            "{key} must be disabled when one selected mesh has no selected components"
        );
        harness.open_menu("menu:Tools");
        harness.app.perform_mesh_action(action);
        assert!(
            harness.app.status.contains("every selected mesh"),
            "{key} should refuse with a status, got {:?}",
            harness.app.status
        );
        assert_eq!(harness.app.history.depth(), depth, "{key}");
        assert_eq!(harness.app.doc.signature(), before_signature, "{key}");
    }
}

#[test]
fn bridge_loops_requires_exactly_two_valid_loops_on_every_selected_mesh() {
    let (mut harness, _root) = InputHarness::new(
        "input-bridge-loops",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    let bridgeable = bridge_fixture_mesh();
    set_editable_geometry(&mut harness.app, cube, bridgeable.clone());
    set_editable_geometry(&mut harness.app, panel, bridgeable.clone());
    let depth = harness.app.history.depth();

    let all_edges: Vec<u32> = (0..bridgeable.edges().len() as u32).collect();
    let one_loop = all_edges[..all_edges.len() / 2].to_vec();

    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Edges;
    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, one_loop.clone());
    harness.app.mesh_selected = one_loop.clone();
    harness.open_menu("menu:Tools");
    assert!(
        !harness.menu_item("Tools:Bridge Loops").1,
        "Bridge Loops must be disabled for a single closed loop"
    );
    harness.open_menu("menu:Tools");
    let before_signature = harness.app.doc.signature();
    harness.app.perform_mesh_action(MeshAction::Bridge);
    assert!(
        harness.app.status.contains("closed edge loops"),
        "single-loop refusal should explain the loop requirement, got {:?}",
        harness.app.status
    );
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before_signature);

    select_object_rows(&mut harness, &[cube, panel]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Edges;
    for (cube_edges, panel_edges, enabled) in [
        (one_loop.clone(), all_edges.clone(), false),
        (all_edges.clone(), one_loop.clone(), false),
        (all_edges.clone(), all_edges.clone(), true),
    ] {
        harness.app.mesh_components.clear();
        harness.app.mesh_components.insert(cube, cube_edges);
        harness.app.mesh_components.insert(panel, panel_edges);
        harness.app.mesh_selected = all_edges.clone();
        harness.open_menu("menu:Tools");
        let bridge_enabled = harness.menu_item("Tools:Bridge Loops").1;
        assert_eq!(
            bridge_enabled, enabled,
            "Bridge Loops enablement must follow loop validity on every selected mesh"
        );
        if bridge_enabled {
            harness.click_menu_item("Tools:Bridge Loops");
            assert_eq!(harness.app.history.depth(), depth + 1);
            assert_ne!(selected_mesh(&harness.app, cube), bridgeable);
            assert_ne!(selected_mesh(&harness.app, panel), bridgeable);
            harness.undo_via_menu();
            assert_eq!(harness.app.history.depth(), depth);
            assert_eq!(selected_mesh(&harness.app, cube), bridgeable);
            assert_eq!(selected_mesh(&harness.app, panel), bridgeable);
        } else {
            let before_signature = harness.app.doc.signature();
            harness.open_menu("menu:Tools");
            harness.app.perform_mesh_action(MeshAction::Bridge);
            assert!(
                harness.app.status.contains("closed edge loops"),
                "partial loop coverage should refuse with a status, got {:?}",
                harness.app.status
            );
            assert_eq!(harness.app.history.depth(), depth);
            assert_eq!(harness.app.doc.signature(), before_signature);
        }
    }
}

#[test]
fn convert_enablement_refuses_already_editable_meshes_like_the_handler() {
    let (mut harness, _root) = InputHarness::new(
        "input-convert-parity",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    let base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    set_editable_geometry(&mut harness.app, cube, base.clone());
    set_editable_geometry(&mut harness.app, panel, base);
    select_object_rows(&mut harness, &[cube, panel]);
    let depth = harness.app.history.depth();
    let before_signature = harness.app.doc.signature();

    harness.open_menu("menu:Selection");
    assert!(!harness.menu_item("Selection:Convert to Editable Mesh").1);
    harness.open_menu("menu:Objects");
    assert!(!harness.menu_item("Objects:Convert to Editable Mesh").1);

    harness.app.make_selected_editable();
    assert!(
        harness.app.status.contains("already editable"),
        "got {:?}",
        harness.app.status
    );
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before_signature);
}

#[test]
fn subdivision_boolean_and_enter_refuse_exactly_when_shared_predicates_say_no() {
    let (mut harness, _root) = InputHarness::new(
        "input-object-tool-parity",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let key_light = named(&harness.app, "Key Light");
    select_object_rows(&mut harness, &[key_light]);
    let depth = harness.app.history.depth();
    let before_signature = harness.app.doc.signature();

    harness.open_menu("menu:Objects");
    assert!(!harness.menu_item("Objects:Add Subdivision").1);
    assert!(!harness.menu_item("Objects:Add Boolean Subtract").1);
    assert!(!harness.menu_item("toolbar:Add Subdivision").1);
    harness.open_menu("menu:Objects");

    harness.app.add_subdivision();
    assert!(
        harness.app.status.contains("Select a mesh object"),
        "got {:?}",
        harness.app.status
    );
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before_signature);

    harness.app.perform_mesh_action(MeshAction::Enter);
    assert!(!harness.app.mesh_edit);
    assert!(
        harness.app.status.contains("mesh-only"),
        "got {:?}",
        harness.app.status
    );
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before_signature);

    let mesh_ids: Vec<NodeId> = harness
        .app
        .doc
        .scene
        .depth_first()
        .into_iter()
        .filter(|id| {
            harness
                .app
                .doc
                .scene
                .get(*id)
                .map(|node| node.is_mesh())
                .unwrap_or(false)
        })
        .collect();
    harness.app.selection.set(mesh_ids);
    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    harness.open_menu("menu:Objects");
    assert!(harness.menu_item("Objects:Add Subdivision").1);
    assert!(!harness.menu_item("Objects:Add Boolean Subtract").1);

    harness.app.add_boolean();
    assert!(
        harness.app.status.contains("unselected mesh"),
        "got {:?}",
        harness.app.status
    );
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before_signature);
}

#[test]
fn ring_cut_enablement_tracks_the_cut_plane_and_never_partially_cuts() {
    let (mut harness, _root) = InputHarness::new(
        "input-ring-cut-parity",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    let base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    set_editable_geometry(&mut harness.app, cube, base.clone());
    set_editable_geometry(&mut harness.app, panel, base.clone());
    select_object_rows(&mut harness, &[cube, panel]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.mesh_axis = 1;
    harness.app.mesh_offset = 0.0;
    let depth = harness.app.history.depth();
    let before_signature = harness.app.doc.signature();

    harness.open_menu("menu:Tools");
    assert!(harness.menu_item("Tools:Ring Cut").1);
    harness.click_menu_item("Tools:Ring Cut");
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_ne!(selected_mesh(&harness.app, cube), base);
    assert_ne!(selected_mesh(&harness.app, panel), base);
    harness.undo_via_menu();
    assert_eq!(harness.app.doc.signature(), before_signature);
    assert_eq!(harness.app.history.depth(), depth);

    harness.app.mesh_offset = 100.0;
    let before_signature = harness.app.doc.signature();
    harness.open_menu("menu:Tools");
    assert!(!harness.menu_item("Tools:Ring Cut").1);
    harness.open_menu("menu:Tools");
    harness.app.perform_mesh_action(MeshAction::RingCut);
    assert!(
        harness.app.status.contains("cut plane"),
        "got {:?}",
        harness.app.status
    );
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before_signature);

    let mut shifted = base.clone();
    shifted.transform(Mat4::from_translation(Vec3::Y * 10.0));
    set_editable_geometry(&mut harness.app, panel, shifted);
    harness.app.mesh_offset = 0.0;
    let before_signature = harness.app.doc.signature();
    let before_cube = selected_mesh(&harness.app, cube);
    let before_panel = selected_mesh(&harness.app, panel);
    harness.open_menu("menu:Tools");
    assert!(!harness.menu_item("Tools:Ring Cut").1);
    harness.app.perform_mesh_action(MeshAction::RingCut);
    assert!(
        harness.app.status.contains("cut plane"),
        "got {:?}",
        harness.app.status
    );
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before_signature);
    assert_eq!(selected_mesh(&harness.app, cube), before_cube);
    assert_eq!(selected_mesh(&harness.app, panel), before_panel);
}

#[test]
fn weld_enablement_requires_distinct_points_on_every_mesh() {
    let (mut harness, _root) = InputHarness::new(
        "input-weld-parity",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    let base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    set_editable_geometry(&mut harness.app, cube, base.clone());
    set_editable_geometry(&mut harness.app, panel, base);
    select_object_rows(&mut harness, &[cube, panel]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Points;
    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, vec![0, 1]);
    harness.app.mesh_components.insert(panel, vec![0, 0]);
    harness.app.mesh_selected = vec![0, 0];
    let depth = harness.app.history.depth();
    let before_signature = harness.app.doc.signature();

    assert!(harness.app.mesh_topology_ready(MeshSelectMode::Points, 2));
    assert!(!harness.app.mesh_weld_ready());
    harness.open_menu("menu:Tools");
    assert!(!harness.menu_item("Tools:Weld Points").1);
    harness.app.perform_mesh_action(MeshAction::Weld);
    assert!(
        harness.app.status.contains("distinct points"),
        "got {:?}",
        harness.app.status
    );
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before_signature);
}

#[test]
fn component_transform_covers_every_selected_mesh_or_refuses_the_whole_selection() {
    let (mut harness, _root) = InputHarness::new(
        "input-transform-parity",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    let base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    set_editable_geometry(&mut harness.app, cube, base.clone());
    set_editable_geometry(&mut harness.app, panel, base.clone());
    select_object_rows(&mut harness, &[cube, panel]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Points;
    let points: Vec<u32> = (0..base.positions.len() as u32).collect();
    set_selected_components(&mut harness.app, &[cube, panel], &points);
    let depth = harness.app.history.depth();
    let before_signature = harness.app.doc.signature();
    let before_cube = selected_mesh(&harness.app, cube);
    let before_panel = selected_mesh(&harness.app, panel);

    assert!(harness.app.mesh_transform_ready());
    harness.app.perform_mesh_action(MeshAction::Transform);
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(
        harness.app.history.undo_label().as_deref(),
        Some("Edit Geometry on 2 Objects")
    );
    assert_ne!(selected_mesh(&harness.app, cube), before_cube);
    assert_ne!(selected_mesh(&harness.app, panel), before_panel);
    harness.undo_via_menu();
    assert_eq!(harness.app.doc.signature(), before_signature);
    assert_eq!(harness.app.history.depth(), depth);

    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, points.clone());
    harness.app.mesh_components.insert(panel, Vec::new());
    harness.app.mesh_selected = points;
    assert!(!harness.app.mesh_transform_ready());
    let before_signature = harness.app.doc.signature();
    let before_cube = selected_mesh(&harness.app, cube);
    let before_panel = selected_mesh(&harness.app, panel);
    harness.app.perform_mesh_action(MeshAction::Transform);
    assert!(
        harness.app.status.contains("every selected mesh"),
        "got {:?}",
        harness.app.status
    );
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before_signature);
    assert_eq!(selected_mesh(&harness.app, cube), before_cube);
    assert_eq!(selected_mesh(&harness.app, panel), before_panel);

    let points: Vec<u32> = (0..before_cube.positions.len() as u32).collect();
    harness.app.mesh_components.insert(panel, points.clone());
    harness.app.mesh_selected = points;
    harness.app.mesh_value = 0.0;
    assert!(!harness.app.mesh_transform_ready());
    let before_signature = harness.app.doc.signature();
    harness.app.perform_mesh_action(MeshAction::Transform);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before_signature);
}

#[test]
fn material_assignment_covers_every_selected_mesh_or_refuses_the_whole_selection() {
    let (mut harness, _root) = InputHarness::new(
        "input-material-parity",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    let base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    set_editable_geometry(&mut harness.app, cube, base.clone());
    set_editable_geometry(&mut harness.app, panel, base);
    select_object_rows(&mut harness, &[cube, panel]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.selected_material = Some(1);
    set_selected_components(&mut harness.app, &[cube, panel], &[0, 1]);
    let depth = harness.app.history.depth();
    let before_signature = harness.app.doc.signature();

    assert!(harness.app.mesh_assign_material_ready());
    harness.app.perform_mesh_action(MeshAction::AssignMaterial);
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(
        harness.app.history.undo_label().as_deref(),
        Some("Assign material on 2 Objects")
    );
    for id in [cube, panel] {
        assert_eq!(polygon_material(&harness.app, id, 0), Some(1), "{id:?}");
        assert_eq!(polygon_material(&harness.app, id, 1), Some(1), "{id:?}");
    }
    harness.undo_via_menu();
    assert_eq!(harness.app.doc.signature(), before_signature);
    assert_eq!(harness.app.history.depth(), depth);

    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, vec![0]);
    harness.app.mesh_components.insert(panel, Vec::new());
    harness.app.mesh_selected = vec![0];
    assert!(!harness.app.mesh_assign_material_ready());
    let before_signature = harness.app.doc.signature();
    let before_cube = polygon_material(&harness.app, cube, 0);
    let before_panel = polygon_material(&harness.app, panel, 0);
    harness.app.perform_mesh_action(MeshAction::AssignMaterial);
    assert!(
        harness.app.status.contains("every selected mesh"),
        "got {:?}",
        harness.app.status
    );
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before_signature);
    assert_eq!(polygon_material(&harness.app, cube, 0), before_cube);
    assert_eq!(polygon_material(&harness.app, panel, 0), before_panel);

    harness.app.mesh_components.insert(panel, vec![0]);
    harness.app.selected_material = None;
    assert!(!harness.app.mesh_assign_material_ready());
    harness.app.perform_mesh_action(MeshAction::AssignMaterial);
    assert!(
        harness.app.status.contains("Select a material"),
        "got {:?}",
        harness.app.status
    );
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before_signature);
}

#[test]
fn uv_projection_handler_refuses_the_mixed_selection_its_menu_disables() {
    let (mut harness, _root) =
        InputHarness::new("input-uv-parity", Surface::Shell, Vec2::new(1400.0, 1000.0));
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    set_editable_geometry(&mut harness.app, cube, lc_geom::box_mesh(1.0, 1.0, 1.0));
    select_object_rows(&mut harness, &[cube, panel]);
    assert!(
        harness
            .app
            .doc
            .scene
            .get(panel)
            .map(|node| node.is_mesh())
            .unwrap_or(false),
        "fixture needs a selected primitive mesh"
    );
    assert!(!editable_mesh(&harness.app, panel));
    assert!(!harness.app.selection_uv_ready());
    let depth = harness.app.history.depth();
    let before_signature = harness.app.doc.signature();

    harness.open_menu("menu:Tools");
    harness.open_submenu("Tools:UV Projection");
    for key in [
        "Tools:UV Projection:Planar",
        "Tools:UV Projection:Box",
        "Tools:UV Projection:Cylindrical",
        "Tools:UV Projection:Spherical",
        "Tools:UV Projection:Automatic Unwrap",
        "Tools:UV Projection:Relax",
        "Tools:UV Projection:Normalize",
    ] {
        assert!(!harness.menu_item(key).1, "{key}");
    }

    harness.app.apply_uv_op(UvOp::Box);
    assert!(
        harness.app.status.contains("Convert all selected meshes"),
        "got {:?}",
        harness.app.status
    );
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before_signature);
}

#[test]
fn inspector_tool_buttons_follow_the_shared_multi_mesh_predicates() {
    let (mut harness, _root) = InputHarness::new(
        "input-panel-parity",
        Surface::Shell,
        Vec2::new(1600.0, 1200.0),
    );
    let cube = named(&harness.app, "Cube");
    let panel = named(&harness.app, "Checker Panel");
    let base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    set_editable_geometry(&mut harness.app, cube, base.clone());
    set_editable_geometry(&mut harness.app, panel, base);
    select_object_rows(&mut harness, &[cube, panel]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, vec![0]);
    harness.app.mesh_selected = vec![0];
    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());

    for key in [
        "panel:Extrude",
        "panel:Inset",
        "panel:Bevel",
        "panel:Apply Transform",
        "panel:Assign Material",
    ] {
        assert!(
            !harness.menu_item(key).1,
            "{key} must follow every selected mesh"
        );
    }
    assert!(harness.menu_item("panel:Ring Cut").1);

    harness.app.mesh_components.insert(panel, vec![0]);
    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    for key in [
        "panel:Extrude",
        "panel:Inset",
        "panel:Bevel",
        "panel:Apply Transform",
        "panel:Assign Material",
    ] {
        assert!(harness.menu_item(key).1, "{key}");
    }

    harness.app.mesh_select_mode = MeshSelectMode::Points;
    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, vec![0, 1]);
    harness.app.mesh_components.insert(panel, vec![0, 1]);
    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    assert!(harness.menu_item("panel:Weld Points").1);
    assert!(!harness.menu_item("panel:Extrude").1);
    assert!(!harness.menu_item("panel:Bridge Loops").1);

    let bridgeable = bridge_fixture_mesh();
    set_editable_geometry(&mut harness.app, cube, bridgeable.clone());
    set_editable_geometry(&mut harness.app, panel, bridgeable.clone());
    harness.app.mesh_select_mode = MeshSelectMode::Edges;
    let all_edges: Vec<u32> = (0..bridgeable.edges().len() as u32).collect();
    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, all_edges.clone());
    harness.app.mesh_components.insert(panel, all_edges.clone());
    harness.app.mesh_selected = all_edges;
    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    assert!(harness.menu_item("panel:Bridge Loops").1);
    assert!(!harness.menu_item("panel:Weld Points").1);

    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.mesh_offset = 100.0;
    harness.app.mesh_selected = vec![0];
    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    assert!(!harness.menu_item("panel:Ring Cut").1);

    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, vec![0]);
    harness.app.mesh_components.insert(panel, Vec::new());
    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    assert!(!harness.menu_item("panel:Apply Transform").1);
    assert!(!harness.menu_item("panel:Assign Material").1);
}

#[test]
fn direct_uv_point_editing_refuses_non_editable_meshes_and_invalid_points() {
    let (mut app, _root) = test_app("uv-point-parity");
    let cube = named(&app, "Cube");
    let panel = named(&app, "Checker Panel");

    app.selection.select_one(cube);
    app.make_selected_editable();
    let mesh = selected_mesh(&app, cube);
    assert!(!mesh.uvs.is_empty());
    let original_uv = mesh.uvs[0];
    let before = app.doc.signature();
    let depth = app.history.depth();

    assert!(app.uv_point_edit_ready());
    assert!(app.set_uv_point(cube, 0, V2::new(0.25, 0.75)));
    assert_eq!(app.selected_uv, Some(0));
    assert_eq!(selected_mesh(&app, cube).uvs[0], V2::new(0.25, 0.75));
    assert_eq!(
        app.history.depth(),
        depth,
        "a UV drag stays pending until the pointer is released"
    );
    app.commit_pending();
    assert_eq!(app.history.depth(), depth + 1);
    app.undo();
    assert_eq!(app.doc.signature(), before);
    assert_eq!(selected_mesh(&app, cube).uvs[0], original_uv);

    let before = app.doc.signature();
    let depth = app.history.depth();
    assert!(!app.set_uv_point(cube, 999, V2::ZERO));
    assert_eq!(app.doc.signature(), before);
    assert_eq!(app.history.depth(), depth);

    app.selection.select_one(panel);
    let before = app.doc.signature();
    assert!(!app.uv_point_edit_ready());
    assert!(!app.set_uv_point(panel, 0, V2::ZERO));
    assert_eq!(app.doc.signature(), before);
    assert_eq!(app.history.depth(), depth);
}

#[test]
fn modifier_add_and_remove_validate_through_the_same_predicates() {
    let (mut app, _root) = test_app("modifier-parity");
    let cube = named(&app, "Cube");
    let panel = named(&app, "Checker Panel");
    let light = named(&app, "Key Light");
    let subdivision = Modifier::Subdivision { levels: 1 };

    app.selection.select_one(cube);
    let initial = app.doc.scene.get(cube).unwrap().modifiers.clone();
    let depth = app.history.depth();
    assert!(app.modifier_ready(&subdivision));
    assert!(app.add_modifier_to_primary(subdivision.clone()));
    assert_eq!(
        app.doc.scene.get(cube).unwrap().modifiers,
        [initial.clone(), vec![subdivision.clone()]].concat()
    );
    assert_eq!(app.history.depth(), depth + 1);
    assert!(app.remove_modifier_from_primary(initial.len()));
    assert_eq!(app.doc.scene.get(cube).unwrap().modifiers, initial);
    assert_eq!(app.history.depth(), depth + 2);
    app.undo();
    assert_eq!(
        app.doc.scene.get(cube).unwrap().modifiers,
        [initial.clone(), vec![subdivision.clone()]].concat()
    );
    app.undo();
    assert_eq!(app.doc.scene.get(cube).unwrap().modifiers, initial);

    let before = app.doc.signature();
    let depth = app.history.depth();
    assert!(!app.remove_modifier_from_primary(9));
    assert_eq!(app.doc.signature(), before);
    assert_eq!(app.history.depth(), depth);

    app.selection.select_one(light);
    let before = app.doc.signature();
    let depth = app.history.depth();
    assert!(!app.modifier_ready(&subdivision));
    assert!(!app.add_modifier_to_primary(subdivision.clone()));
    assert_eq!(app.doc.signature(), before);
    assert_eq!(app.history.depth(), depth);

    app.add_spline();
    let spline = app.selection.primary().unwrap();
    let sweep = Modifier::defaults()
        .into_iter()
        .find(|modifier| matches!(modifier, Modifier::Sweep { .. }))
        .unwrap();
    assert!(app.modifier_ready(&sweep));
    assert!(!app.modifier_ready(&subdivision));
    assert!(app.add_modifier_to_primary(sweep));
    assert_eq!(app.doc.scene.get(spline).unwrap().modifiers.len(), 1);

    app.selection.select_one(cube);
    let boolean = Modifier::Boolean {
        operand: panel,
        operation: lc_scene::CsgOperation::Subtract,
    };
    assert!(app.modifier_ready(&boolean));
    assert!(app.add_modifier_to_primary(boolean));
    assert!(matches!(
        app.doc.scene.get(cube).unwrap().modifiers.last(),
        Some(Modifier::Boolean { operand, .. }) if *operand == panel
    ));
    let self_boolean = Modifier::Boolean {
        operand: cube,
        operation: lc_scene::CsgOperation::Subtract,
    };
    assert!(!app.modifier_ready(&self_boolean));
    let before = app.doc.signature();
    let depth = app.history.depth();
    assert!(!app.add_modifier_to_primary(self_boolean));
    assert_eq!(app.doc.signature(), before);
    assert_eq!(app.history.depth(), depth);

    let cube_modifiers = app.doc.scene.get(cube).unwrap().modifiers.len();
    let panel_modifiers = app.doc.scene.get(panel).unwrap().modifiers.len();
    app.selection.set([panel, cube]);
    let depth = app.history.depth();
    assert!(app.modifier_ready(&subdivision));
    assert!(app.add_modifier_to_primary(subdivision));
    assert_eq!(
        app.doc.scene.get(cube).unwrap().modifiers.len(),
        cube_modifiers + 1
    );
    assert_eq!(
        app.doc.scene.get(panel).unwrap().modifiers.len(),
        panel_modifiers,
        "inspector modifiers are explicitly single-object"
    );
    assert_eq!(app.history.depth(), depth + 1);
}

#[test]
fn skin_binding_and_unbinding_share_their_predicate_and_refuse_noop_rebinds() {
    let (mut app, _root) = test_app("skin-parity");
    let cube = named(&app, "Cube");
    let panel = named(&app, "Checker Panel");
    let light = named(&app, "Key Light");

    app.selection.select_one(cube);
    let before = app.doc.signature();
    let depth = app.history.depth();
    assert!(!skin_ready(&app));
    assert!(!app.bind_skin_to_all_joints());
    assert_eq!(app.doc.signature(), before);
    assert_eq!(app.history.depth(), depth);

    app.add_joint();
    app.selection.select_one(cube);
    assert!(skin_ready(&app));
    let depth = app.history.depth();
    assert!(app.bind_skin_to_all_joints());
    assert!(app.doc.scene.get(cube).unwrap().skin.is_some());
    assert_eq!(app.history.depth(), depth + 1);

    assert!(!skin_ready(&app), "an identical rebind has no effect");
    let before = app.doc.signature();
    let depth = app.history.depth();
    assert!(!app.bind_skin_to_all_joints());
    assert_eq!(app.doc.signature(), before);
    assert_eq!(app.history.depth(), depth);

    assert!(skin_unbind_ready(app.doc.scene.get(cube).unwrap()));
    assert!(app.unbind_skin_from_primary());
    assert!(app.doc.scene.get(cube).unwrap().skin.is_none());
    assert_eq!(app.history.depth(), depth + 1);
    app.undo();
    assert!(app.doc.scene.get(cube).unwrap().skin.is_some());
    app.undo();
    assert!(app.doc.scene.get(cube).unwrap().skin.is_none());

    let before = app.doc.signature();
    let depth = app.history.depth();
    assert!(!app.unbind_skin_from_primary());
    assert_eq!(app.doc.signature(), before);
    assert_eq!(app.history.depth(), depth);

    app.selection.select_one(light);
    let before = app.doc.signature();
    let depth = app.history.depth();
    assert!(!skin_ready(&app));
    assert!(!app.bind_skin_to_all_joints());
    assert_eq!(app.doc.signature(), before);
    assert_eq!(app.history.depth(), depth);

    app.selection.set([panel, cube]);
    assert!(skin_ready(&app));
    let depth = app.history.depth();
    assert!(app.bind_skin_to_all_joints());
    assert!(app.doc.scene.get(cube).unwrap().skin.is_some());
    assert!(
        app.doc.scene.get(panel).unwrap().skin.is_none(),
        "inspector skin binding is explicitly primary-object only"
    );
    assert_eq!(app.history.depth(), depth + 1);
}

#[test]
fn curve_key_selection_move_and_delete_share_single_undo_steps() {
    let (mut app, _root) = test_app("curve-key-edit");
    let cube = named(&app, "Cube");
    app.selection.select_one(cube);
    app.doc.scene.get_mut(cube).unwrap().animation = AnimationClip::default();
    {
        let animation = &mut app.doc.scene.get_mut(cube).unwrap().animation;
        animation.set_key(AnimProperty::TranslationX, 0.0, 0.0, Interpolation::Linear);
        animation.set_key(AnimProperty::TranslationX, 2.0, 4.0, Interpolation::Linear);
    }
    app.history.clear();

    assert!(app.select_curve_key(AnimProperty::TranslationX, 0.0));
    assert!(app.selected_curve_key().is_some());
    assert!(
        !app.select_curve_key(AnimProperty::TranslationX, 1.0),
        "there is no key at that time"
    );
    assert!(!app.select_curve_key(AnimProperty::ScaleY, 0.0));

    assert!(app.move_curve_key(AnimProperty::TranslationX, 0.0, 1.0));
    let key_times = |app: &LeopardApp| -> Vec<f32> {
        app.doc
            .scene
            .get(cube)
            .unwrap()
            .animation
            .track(AnimProperty::TranslationX)
            .unwrap()
            .curve
            .keys
            .iter()
            .map(|key| key.time)
            .collect()
    };
    assert_eq!(key_times(&app).len(), 2);
    assert!((key_times(&app)[0] - 1.0).abs() < 1e-4);
    assert_eq!(app.history.depth(), 1);

    assert!(
        !app.move_curve_key(AnimProperty::TranslationX, 1.0, 2.0),
        "moving onto another key would merge two keys"
    );
    assert!(!app.move_curve_key(AnimProperty::TranslationX, 1.0, 1.0));
    assert!(!app.move_curve_key(AnimProperty::TranslationX, 1.0, f32::NAN));
    assert_eq!(app.history.depth(), 1);

    app.history.undo(&mut app.doc);
    assert!((key_times(&app)[0] - 0.0).abs() < 1e-4);
    app.history.redo(&mut app.doc);
    assert!((key_times(&app)[0] - 1.0).abs() < 1e-4);

    assert!(app.delete_curve_key(AnimProperty::TranslationX, 1.0));
    assert_eq!(key_times(&app), vec![2.0]);
    app.history.undo(&mut app.doc);
    assert_eq!(key_times(&app), vec![1.0, 2.0]);
}

#[test]
fn deleting_a_tracks_last_key_drops_the_track_and_undo_restores_it() {
    let (mut app, _root) = test_app("curve-key-delete-track");
    let cube = named(&app, "Cube");
    app.selection.select_one(cube);
    app.doc.scene.get_mut(cube).unwrap().animation = AnimationClip::default();
    app.doc.scene.get_mut(cube).unwrap().animation.set_key(
        AnimProperty::TranslationY,
        0.5,
        1.0,
        Interpolation::Linear,
    );
    app.history.clear();

    assert!(app.delete_curve_key(AnimProperty::TranslationY, 0.5));
    assert!(app.doc.scene.get(cube).unwrap().animation.is_empty());
    assert_eq!(app.history.depth(), 1);

    app.history.undo(&mut app.doc);
    assert!(app
        .doc
        .scene
        .get(cube)
        .unwrap()
        .animation
        .track(AnimProperty::TranslationY)
        .is_some());
}

#[test]
fn per_key_interpolation_changes_only_the_targeted_key() {
    let (mut app, _root) = test_app("curve-key-interpolation");
    let cube = named(&app, "Cube");
    app.selection.select_one(cube);
    {
        let animation = &mut app.doc.scene.get_mut(cube).unwrap().animation;
        *animation = AnimationClip::default();
        animation.set_key(AnimProperty::RotationY, 0.0, 0.0, Interpolation::Linear);
        animation.set_key(AnimProperty::RotationY, 1.0, 90.0, Interpolation::Linear);
    }
    app.history.clear();

    assert!(app.set_curve_key_interpolation(AnimProperty::RotationY, 0.0, Interpolation::Step));
    let track = app
        .doc
        .scene
        .get(cube)
        .unwrap()
        .animation
        .track(AnimProperty::RotationY)
        .unwrap();
    assert_eq!(track.curve.keys[0].interpolation, Interpolation::Step);
    assert_eq!(track.curve.keys[1].interpolation, Interpolation::Linear);
    assert_eq!(app.history.depth(), 1);

    let before = app.doc.signature();
    assert!(!app.set_curve_key_interpolation(AnimProperty::RotationY, 0.0, Interpolation::Step));
    assert!(!app.set_curve_key_interpolation(AnimProperty::RotationY, 4.0, Interpolation::Step));
    assert_eq!(app.doc.signature(), before);
    assert_eq!(app.history.depth(), 1);
}

#[test]
fn recorded_rotation_round_trips_through_registry_and_key_editing() {
    let (mut app, _root) = test_app("rotation-registry");
    let cube = named(&app, "Cube");
    app.selection.select_one(cube);
    app.doc.scene.get_mut(cube).unwrap().animation = AnimationClip::default();
    let rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    app.doc.scene.get_mut(cube).unwrap().transform.rotation = rotation;
    app.playback.time = 0.5;
    app.history.clear();

    app.key_transform_channels();
    let clip = &app.doc.scene.get(cube).unwrap().animation;
    assert!(
        clip.track(AnimProperty::RotationX).is_none(),
        "rotation must be recorded as one quaternion channel, not Euler curves"
    );
    let track = clip.rotation.as_ref().expect("quaternion rotation track");
    assert_eq!(track.keys.len(), 1);
    assert!((track.keys[0].time - 0.5).abs() < 1e-4);
    assert!(track.keys[0].value.dot(rotation).abs() > 0.9999);

    let depth = app.history.depth();
    assert!(app.select_rotation_key(0.5));
    assert!(app.move_selected_curve_key(1.0));
    let moved = &app
        .doc
        .scene
        .get(cube)
        .unwrap()
        .animation
        .rotation
        .as_ref()
        .unwrap()
        .keys[0];
    assert!((moved.time - 1.0).abs() < 1e-4);
    assert_eq!(app.history.depth(), depth + 1);

    app.history.undo(&mut app.doc);
    let clip = &app.doc.scene.get(cube).unwrap().animation;
    let restored = clip.rotation.as_ref().expect("rotation restored by undo");
    assert!((restored.keys[0].time - 0.5).abs() < 1e-4);
    let sampled = clip.sample_rotation(0.5).expect("recorded rotation sample");
    assert!(
        sampled.dot(rotation).abs() > 0.9999,
        "the recorded key must reproduce the authored rotation"
    );

    assert!(app.select_rotation_key(0.5));
    assert!(app.set_selected_curve_key_interpolation(Interpolation::Step));
    assert_eq!(
        app.doc
            .scene
            .get(cube)
            .unwrap()
            .animation
            .rotation
            .as_ref()
            .unwrap()
            .keys[0]
            .interpolation,
        Interpolation::Step
    );

    assert!(app.select_rotation_key(0.5));
    assert!(app.delete_selected_curve_key());
    assert!(app
        .doc
        .scene
        .get(cube)
        .unwrap()
        .animation
        .rotation
        .is_none());
    app.history.undo(&mut app.doc);
    assert!(app
        .doc
        .scene
        .get(cube)
        .unwrap()
        .animation
        .rotation
        .is_some());
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

#[test]
fn recorded_tag_channel_round_trips_through_registry_and_key_editing() {
    let (mut app, _root) = test_app("tag-registry");
    let cube = named(&app, "Cube");
    let source = named(&app, "Sphere");
    app.selection.select_one(cube);
    let tag_index = {
        let node = app.doc.scene.get_mut(cube).unwrap();
        node.tags
            .retain(|tag| !matches!(tag, Tag::CopyLocation { .. }));
        node.tags.push(Tag::CopyLocation {
            source,
            offset: Vec3::ZERO,
            weight: 0.0,
        });
        node.tag_animations.clear();
        node.tags.len() - 1
    };
    let target = AnimTarget::Tag(cube, tag_index);
    assert!(app.set_anim_target(target));
    let rows = app.curve_rows(target);
    assert_eq!(rows.len(), 1, "tag scope declares one registry channel");
    assert_eq!(rows[0].channel, AnimChannel::CopyLocationWeight);
    assert_eq!(rows[0].channel.component_count(), 1);
    assert_eq!(rows[0].property, Some(AnimProperty::CopyLocationWeight));

    app.history.clear();
    app.playback.time = 0.0;
    assert!(app.record_channel_key(target, AnimChannel::CopyLocationWeight));
    app.playback.time = 1.0;
    if let Some(Tag::CopyLocation { weight, .. }) =
        app.doc.scene.get_mut(cube).unwrap().tags.get_mut(tag_index)
    {
        *weight = 1.0;
    }
    assert!(app.record_channel_key(target, AnimChannel::CopyLocationWeight));
    let clip = app
        .doc
        .scene
        .get(cube)
        .unwrap()
        .tag_animations
        .get(&(tag_index as u32))
        .cloned()
        .expect("recording wrote the per-tag clip");
    assert_eq!(
        clip.track(AnimProperty::CopyLocationWeight)
            .unwrap()
            .curve
            .keys
            .len(),
        2
    );
    assert!((clip.sample_or(AnimProperty::CopyLocationWeight, 1.0, -1.0) - 1.0).abs() < 1e-4);

    let world_at_one = app.doc.evaluate(1.0);
    let cube_pos = world_at_one.node(cube).unwrap().world.w_axis.truncate();
    let source_pos = world_at_one.node(source).unwrap().world.w_axis.truncate();
    assert!(
        (cube_pos - source_pos).length() < 0.05,
        "evaluation must sample the recorded per-tag weight"
    );

    let depth = app.history.depth();
    assert!(app.select_target_track_key(
        target,
        CurveTrackId::Scalar(AnimProperty::CopyLocationWeight),
        1.0
    ));
    assert!(app.move_selected_curve_key(2.0));
    assert_eq!(app.history.depth(), depth + 1);
    let clip = app
        .doc
        .scene
        .get(cube)
        .unwrap()
        .tag_animations
        .get(&(tag_index as u32))
        .unwrap()
        .clone();
    assert!((clip.sample_or(AnimProperty::CopyLocationWeight, 2.0, -1.0) - 1.0).abs() < 1e-4);
    app.history.undo(&mut app.doc);
    let restored = app
        .doc
        .scene
        .get(cube)
        .unwrap()
        .tag_animations
        .get(&(tag_index as u32))
        .unwrap()
        .clone();
    assert!((restored.sample_or(AnimProperty::CopyLocationWeight, 1.0, -1.0) - 1.0).abs() < 1e-4);

    let depth = app.history.depth();
    assert!(app.select_target_track_key(
        target,
        CurveTrackId::Scalar(AnimProperty::CopyLocationWeight),
        0.0
    ));
    assert!(app.set_selected_curve_key_interpolation(Interpolation::Step));
    assert_eq!(app.history.depth(), depth + 1);
    let stepped = app
        .doc
        .scene
        .get(cube)
        .unwrap()
        .tag_animations
        .get(&(tag_index as u32))
        .unwrap()
        .clone();
    assert!(
        stepped
            .sample_or(AnimProperty::CopyLocationWeight, 0.5, -1.0)
            .abs()
            < 1e-4
    );
    app.history.undo(&mut app.doc);

    let depth = app.history.depth();
    assert!(app.select_target_track_key(
        target,
        CurveTrackId::Scalar(AnimProperty::CopyLocationWeight),
        0.0
    ));
    assert!(app.delete_selected_curve_key());
    assert_eq!(app.history.depth(), depth + 1);
    let one_key = app
        .doc
        .scene
        .get(cube)
        .unwrap()
        .tag_animations
        .get(&(tag_index as u32))
        .unwrap()
        .clone();
    assert_eq!(
        one_key
            .track(AnimProperty::CopyLocationWeight)
            .unwrap()
            .curve
            .keys
            .len(),
        1
    );
    assert!((one_key.sample_or(AnimProperty::CopyLocationWeight, 0.5, -1.0) - 1.0).abs() < 1e-4);
    app.history.undo(&mut app.doc);
    assert_eq!(
        app.doc
            .scene
            .get(cube)
            .unwrap()
            .tag_animations
            .get(&(tag_index as u32))
            .unwrap()
            .track(AnimProperty::CopyLocationWeight)
            .unwrap()
            .curve
            .keys
            .len(),
        2
    );
}

#[test]
fn recorded_material_channel_round_trips_through_registry_and_key_editing() {
    let (mut app, _root) = test_app("material-registry");
    let material_id = app.selected_material.expect("demo material selection");
    app.selection.select_one(named(&app, "Cube"));
    {
        let material = app.doc.materials.get_mut(material_id).unwrap();
        set_principled_float(material, 1, 0.9);
        set_principled_float(material, 2, 0.2);
        material.animation = AnimationClip::default();
    }
    let target = AnimTarget::Material(material_id);
    assert!(app.set_anim_target(target));
    let rows = app.curve_rows(target);
    assert_eq!(
        rows.len(),
        2,
        "material scope declares roughness and metallic"
    );
    assert!(rows
        .iter()
        .any(|row| row.channel == AnimChannel::MaterialRoughness));
    assert!(rows
        .iter()
        .any(|row| row.channel == AnimChannel::MaterialMetallic));
    assert!(app
        .curve_rows(AnimTarget::Object(named(&app, "Cube")))
        .iter()
        .any(|row| row.channel == AnimChannel::Translation && row.property.is_some()));
    assert!(
        !app.curve_rows(AnimTarget::Object(named(&app, "Cube")))
            .iter()
            .any(|row| row.channel == AnimChannel::Fov),
        "camera-only registry channels do not appear for a mesh"
    );

    app.history.clear();
    app.playback.time = 0.0;
    assert!(app.record_channel_key(target, AnimChannel::MaterialRoughness));
    app.playback.time = 1.0;
    set_principled_float(app.doc.materials.get_mut(material_id).unwrap(), 2, 0.8);
    let base = app
        .doc
        .materials
        .get(material_id)
        .unwrap()
        .base_roughness(&app.texture_cache);
    assert!((base - 0.8).abs() < 1e-4, "base {base}");
    assert!(app.record_channel_key(target, AnimChannel::MaterialRoughness));
    let clip = app
        .doc
        .materials
        .get(material_id)
        .unwrap()
        .animation
        .clone();
    assert_eq!(
        clip.track(AnimProperty::MaterialRoughness)
            .unwrap()
            .curve
            .keys
            .len(),
        2
    );
    let sampled = clip.sample_or(AnimProperty::MaterialRoughness, 1.0, -1.0);
    let keys = &clip
        .track(AnimProperty::MaterialRoughness)
        .unwrap()
        .curve
        .keys;
    assert!(
        (sampled - 0.8).abs() < 1e-4,
        "keys {keys:?} sampled {sampled}"
    );

    let depth = app.history.depth();
    assert!(app.select_target_track_key(
        target,
        CurveTrackId::Scalar(AnimProperty::MaterialRoughness),
        1.0
    ));
    assert!(app.move_selected_curve_key(2.0));
    assert_eq!(app.history.depth(), depth + 1);
    let moved = app
        .doc
        .materials
        .get(material_id)
        .unwrap()
        .animation
        .clone();
    assert!((moved.sample_or(AnimProperty::MaterialRoughness, 2.0, -1.0) - 0.8).abs() < 1e-4);
    app.history.undo(&mut app.doc);

    let depth = app.history.depth();
    assert!(app.select_target_track_key(
        target,
        CurveTrackId::Scalar(AnimProperty::MaterialRoughness),
        0.0
    ));
    assert!(app.set_selected_curve_key_interpolation(Interpolation::Step));
    assert_eq!(app.history.depth(), depth + 1);
    let ctx = lc_shading::ShadingContext::new(&app.texture_cache);
    let stepped = app
        .doc
        .materials
        .get(material_id)
        .unwrap()
        .evaluate_at(&ctx, 0.75)
        .roughness;
    assert!(
        (stepped - 0.2).abs() < 1e-4,
        "step interpolation holds the first key"
    );
    app.history.undo(&mut app.doc);

    let depth = app.history.depth();
    assert!(app.select_target_track_key(
        target,
        CurveTrackId::Scalar(AnimProperty::MaterialRoughness),
        0.0
    ));
    assert!(app.delete_selected_curve_key());
    assert_eq!(app.history.depth(), depth + 1);
    let ctx = lc_shading::ShadingContext::new(&app.texture_cache);
    let deleted = app
        .doc
        .materials
        .get(material_id)
        .unwrap()
        .evaluate_at(&ctx, 0.75)
        .roughness;
    assert!((deleted - 0.8).abs() < 1e-4);
    app.history.undo(&mut app.doc);
    assert_eq!(
        app.doc
            .materials
            .get(material_id)
            .unwrap()
            .animation
            .track(AnimProperty::MaterialRoughness)
            .unwrap()
            .curve
            .keys
            .len(),
        2
    );
}

#[test]
fn fcurve_box_selection_and_copy_paste_cover_object_scalar_and_rotation_keys() {
    let (mut app, _root) = test_app("curve-box-object");
    let cube = named(&app, "Cube");
    let sphere = named(&app, "Sphere");
    app.selection.select_one(cube);
    let rotation = Quat::from_rotation_y(0.5);
    {
        let node = app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        node.animation
            .set_key(AnimProperty::TranslationX, 0.0, 1.0, Interpolation::Linear);
        node.animation
            .set_key(AnimProperty::TranslationX, 1.0, 2.0, Interpolation::Linear);
        node.animation
            .set_key(AnimProperty::TranslationX, 2.5, 50.0, Interpolation::Linear);
        node.animation
            .set_rotation_key(1.0, rotation, Interpolation::Linear);
        node.animation
            .set_rotation_key(2.5, Quat::from_rotation_y(1.0), Interpolation::Linear);
    }
    let target = AnimTarget::Object(cube);
    assert!(app.set_anim_target(target));
    let clip = app.target_clip(target).unwrap();

    assert!(app.select_keys_in_box(target, &clip, (0.0, 1.0), (-10.0, 10.0), false));
    let selected = app.selected_curve_keys();
    assert_eq!(selected.len(), 3, "two scalar keys and one rotation key");
    assert!(selected.iter().any(|key| {
        key.track == CurveTrackId::Scalar(AnimProperty::TranslationX)
            && (key.time - 1.0).abs() < 1e-4
    }));
    assert!(selected
        .iter()
        .any(|key| key.track == CurveTrackId::Rotation && (key.time - 1.0).abs() < 1e-4));
    assert!(!selected.iter().any(|key| (key.time - 2.5).abs() < 1e-4));

    assert!(app.select_target_track_key(
        target,
        CurveTrackId::Scalar(AnimProperty::TranslationX),
        2.5
    ));
    assert!(app.select_keys_in_box(target, &clip, (0.0, 1.0), (-10.0, 10.0), true));
    assert_eq!(app.selected_curve_keys().len(), 4, "additive box selection");

    assert!(app.select_keys_in_box(target, &clip, (0.0, 1.0), (-10.0, 10.0), false));
    assert!(app.copy_selected_curve_keys());
    assert_eq!(app.curve_clipboard.keys.len(), 3);
    app.playback.time = 5.0;
    app.history.clear();
    assert!(app.paste_curve_keys());
    assert_eq!(app.history.depth(), 1, "paste is one undo step");
    let pasted = app.target_clip(target).unwrap();
    assert!((pasted.sample_or(AnimProperty::TranslationX, 5.0, -1.0) - 1.0).abs() < 1e-4);
    assert!((pasted.sample_or(AnimProperty::TranslationX, 6.0, -1.0) - 2.0).abs() < 1e-4);
    assert!(pasted.sample_rotation(6.0).is_some());
    assert!(pasted.sample(AnimProperty::TranslationX, 6.0).is_some());
    app.history.undo(&mut app.doc);
    let restored = app.target_clip(target).unwrap();
    assert!(restored
        .track(AnimProperty::TranslationX)
        .unwrap()
        .curve
        .keys
        .iter()
        .all(|key| (key.time - 5.0).abs() > KEY_TIME_EPSILON));
    assert!(restored
        .rotation
        .as_ref()
        .unwrap()
        .keys
        .iter()
        .all(|key| (key.time - 6.0).abs() > KEY_TIME_EPSILON));

    assert!(app.select_keys_in_box(target, &restored, (0.0, 1.0), (-10.0, 10.0), false));
    assert!(app.copy_selected_curve_keys());
    let other = AnimTarget::Object(sphere);
    app.selection.select_one(sphere);
    assert!(app.set_anim_target(other));
    {
        let node = app.doc.scene.get_mut(sphere).unwrap();
        node.animation = AnimationClip::default();
        node.animation
            .set_key(AnimProperty::TranslationX, 3.0, 99.0, Interpolation::Linear);
    }
    app.playback.time = 3.0;
    let depth = app.history.depth();
    assert!(app.paste_curve_keys());
    assert_eq!(app.history.depth(), depth + 1);
    let other_clip = app.target_clip(other).unwrap();
    let overwritten = other_clip.sample_or(AnimProperty::TranslationX, 3.0, -1.0);
    assert!(
        (overwritten - 1.0).abs() < 1e-4,
        "paste overwrites an existing key: sampled {overwritten}, keys {:?}",
        other_clip
            .track(AnimProperty::TranslationX)
            .unwrap()
            .curve
            .keys
    );
    assert!(other_clip.sample_rotation(4.0).is_some());
    assert!((other_clip.sample_or(AnimProperty::TranslationX, 4.0, -1.0) - 2.0).abs() < 1e-4);
    app.history.undo(&mut app.doc);
    let other_clip = app.target_clip(other).unwrap();
    assert!(
        (other_clip.sample_or(AnimProperty::TranslationX, 3.0, -1.0) - 99.0).abs() < 1e-4,
        "one undo restores the overwritten key"
    );
    assert!(other_clip
        .track(AnimProperty::TranslationX)
        .unwrap()
        .curve
        .keys
        .iter()
        .all(|key| (key.time - 4.0).abs() > KEY_TIME_EPSILON));
    assert!(other_clip
        .rotation
        .as_ref()
        .map(|track| {
            track
                .keys
                .iter()
                .all(|key| (key.time - 4.0).abs() > KEY_TIME_EPSILON)
        })
        .unwrap_or(true));
}

#[test]
fn fcurve_box_selection_and_copy_paste_cover_tag_and_material_scopes() {
    let (mut app, _root) = test_app("curve-box-scopes");
    let cube = named(&app, "Cube");
    let sphere = named(&app, "Sphere");
    for node_id in [cube, sphere] {
        let node = app.doc.scene.get_mut(node_id).unwrap();
        node.tags
            .retain(|tag| !matches!(tag, Tag::CopyLocation { .. }));
        node.tags.push(Tag::CopyLocation {
            source: if node_id == cube { sphere } else { cube },
            offset: Vec3::ZERO,
            weight: 0.0,
        });
        let mut clip = AnimationClip::default();
        clip.set_key(
            AnimProperty::CopyLocationWeight,
            0.0,
            0.0,
            Interpolation::Linear,
        );
        clip.set_key(
            AnimProperty::CopyLocationWeight,
            1.0,
            1.0,
            Interpolation::Linear,
        );
        clip.set_key(
            AnimProperty::CopyLocationWeight,
            2.5,
            0.9,
            Interpolation::Linear,
        );
        node.tag_animations.insert(0, clip);
    }
    let tag_target = AnimTarget::Tag(cube, 0);
    assert!(app.set_anim_target(tag_target));
    let tag_clip = app.target_clip(tag_target).unwrap();
    assert!(app.select_keys_in_box(tag_target, &tag_clip, (0.0, 1.0), (0.0, 1.0), false));
    assert_eq!(app.selected_curve_keys().len(), 2);
    assert!(app.copy_selected_curve_keys());

    let material_target = AnimTarget::Material(0);
    assert!(app.set_anim_target(material_target));
    app.playback.time = 4.0;
    let depth = app.history.depth();
    assert!(
        !app.paste_curve_keys(),
        "tag channels are not pasted into a material scope"
    );
    assert_eq!(app.history.depth(), depth);

    let other_tag = AnimTarget::Tag(sphere, 0);
    app.selection.select_one(sphere);
    assert!(app.set_anim_target(other_tag));
    assert!(app.paste_curve_keys());
    assert_eq!(app.history.depth(), depth + 1);
    let pasted = app.target_clip(other_tag).unwrap();
    assert!((pasted.sample_or(AnimProperty::CopyLocationWeight, 4.0, -1.0) - 0.0).abs() < 1e-4);
    assert!((pasted.sample_or(AnimProperty::CopyLocationWeight, 5.0, -1.0) - 1.0).abs() < 1e-4);
    app.history.undo(&mut app.doc);
    assert!(app
        .target_clip(other_tag)
        .unwrap()
        .track(AnimProperty::CopyLocationWeight)
        .unwrap()
        .curve
        .keys
        .iter()
        .all(|key| (key.time - 4.0).abs() > KEY_TIME_EPSILON));

    for material_id in [0, 1] {
        let material = app.doc.materials.get_mut(material_id).unwrap();
        let mut clip = AnimationClip::default();
        clip.set_key(
            AnimProperty::MaterialRoughness,
            0.0,
            0.2,
            Interpolation::Linear,
        );
        clip.set_key(
            AnimProperty::MaterialRoughness,
            1.0,
            0.8,
            Interpolation::Linear,
        );
        clip.set_key(
            AnimProperty::MaterialRoughness,
            2.5,
            0.4,
            Interpolation::Linear,
        );
        clip.set_key(
            AnimProperty::MaterialMetallic,
            0.0,
            0.1,
            Interpolation::Linear,
        );
        clip.set_key(
            AnimProperty::MaterialMetallic,
            1.0,
            0.9,
            Interpolation::Linear,
        );
        material.animation = clip;
    }
    assert!(app.set_anim_target(material_target));
    let material_clip = app.target_clip(material_target).unwrap();
    assert!(app.select_keys_in_box(
        material_target,
        &material_clip,
        (0.0, 1.0),
        (0.0, 1.0),
        false
    ));
    assert_eq!(app.selected_curve_keys().len(), 4);
    assert!(app.copy_selected_curve_keys());
    let other_material = AnimTarget::Material(1);
    assert!(app.set_anim_target(other_material));
    app.playback.time = 3.0;
    let depth = app.history.depth();
    assert!(app.paste_curve_keys());
    assert_eq!(app.history.depth(), depth + 1);
    let pasted = app.target_clip(other_material).unwrap();
    assert!((pasted.sample_or(AnimProperty::MaterialRoughness, 3.0, -1.0) - 0.2).abs() < 1e-4);
    assert!((pasted.sample_or(AnimProperty::MaterialRoughness, 4.0, -1.0) - 0.8).abs() < 1e-4);
    assert!((pasted.sample_or(AnimProperty::MaterialMetallic, 4.0, -1.0) - 0.9).abs() < 1e-4);
    app.history.undo(&mut app.doc);
    let restored = app.target_clip(other_material).unwrap();
    assert!(restored
        .track(AnimProperty::MaterialRoughness)
        .unwrap()
        .curve
        .keys
        .iter()
        .all(|key| (key.time - 3.0).abs() > KEY_TIME_EPSILON));
    assert!(restored
        .track(AnimProperty::MaterialMetallic)
        .unwrap()
        .curve
        .keys
        .iter()
        .all(|key| (key.time - 4.0).abs() > KEY_TIME_EPSILON));
}

#[test]
fn fcurve_paste_undo_redo_reactivates_targets_for_all_scopes() {
    let (mut harness, _root) = InputHarness::new(
        "curve-paste-undo-scopes",
        Surface::Shortcuts,
        Vec2::new(800.0, 600.0),
    );
    harness.app.show_curves = true;
    let cube = named(&harness.app, "Cube");
    let sphere = named(&harness.app, "Sphere");

    {
        let node = harness.app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        node.animation
            .set_key(AnimProperty::TranslationX, 0.0, 4.0, Interpolation::Linear);
        node.animation
            .set_key(AnimProperty::TranslationX, 1.0, 5.0, Interpolation::Linear);
    }
    {
        let node = harness.app.doc.scene.get_mut(sphere).unwrap();
        node.animation = AnimationClip::default();
        node.animation
            .set_key(AnimProperty::TranslationX, 2.0, 99.0, Interpolation::Linear);
    }
    let source_object = AnimTarget::Object(cube);
    assert!(activate_curve_target(&mut harness.app, source_object));
    let source_clip = harness.app.target_clip(source_object).unwrap();
    assert!(harness.app.select_keys_in_box(
        source_object,
        &source_clip,
        (0.0, 1.0),
        (0.0, 10.0),
        false
    ));
    assert!(harness.app.copy_selected_curve_keys());

    let destination_object = AnimTarget::Object(sphere);
    assert!(activate_curve_target(&mut harness.app, destination_object));
    let destination_clip = harness.app.target_clip(destination_object).unwrap();
    assert!(harness.app.select_keys_in_box(
        destination_object,
        &destination_clip,
        (2.0, 2.0),
        (0.0, 100.0),
        false
    ));
    let object_prior_selection = harness.app.selected_curve_keys();
    assert_eq!(object_prior_selection.len(), 1);
    harness.app.playback.time = 4.0;
    harness.app.history.clear();

    assert!(harness.app.paste_curve_keys());
    assert_eq!(harness.app.history.depth(), 1);
    assert_curve_times(&harness.app, destination_object, &[2.0, 4.0, 5.0]);
    let object_pasted_selection = harness.app.selected_curve_keys();
    assert_eq!(object_pasted_selection.len(), 2);

    assert!(activate_curve_target(&mut harness.app, source_object));
    assert_eq!(
        harness.app.current_anim_target(),
        Some(source_object),
        "switch away from the pasted object"
    );
    harness.press_key(egui::Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(harness.app.history.depth(), 0);
    assert_curve_times(&harness.app, destination_object, &[2.0]);
    assert_eq!(
        harness.app.current_anim_target(),
        Some(destination_object),
        "undo re-activates the pasted object target"
    );
    assert_eq!(harness.app.selected_curve_keys(), object_prior_selection);
    assert_eq!(harness.app.selection.primary(), Some(sphere));

    assert!(activate_curve_target(&mut harness.app, source_object));
    harness.press_key(
        egui::Key::Z,
        egui::Modifiers {
            shift: true,
            ..egui::Modifiers::COMMAND
        },
    );
    assert_eq!(harness.app.history.depth(), 1);
    assert_curve_times(&harness.app, destination_object, &[2.0, 4.0, 5.0]);
    assert_eq!(
        harness.app.current_anim_target(),
        Some(destination_object),
        "redo re-activates the pasted object target"
    );
    assert_eq!(harness.app.selected_curve_keys(), object_pasted_selection);
    assert_eq!(harness.app.selection.primary(), Some(sphere));

    for node_id in [cube, sphere] {
        let node = harness.app.doc.scene.get_mut(node_id).unwrap();
        node.tags
            .retain(|tag| !matches!(tag, Tag::CopyLocation { .. }));
        node.tags.push(Tag::CopyLocation {
            source: if node_id == cube { sphere } else { cube },
            offset: Vec3::ZERO,
            weight: 0.0,
        });
        let mut clip = AnimationClip::default();
        if node_id == cube {
            clip.set_key(
                AnimProperty::CopyLocationWeight,
                0.0,
                0.0,
                Interpolation::Linear,
            );
            clip.set_key(
                AnimProperty::CopyLocationWeight,
                1.0,
                1.0,
                Interpolation::Linear,
            );
        } else {
            clip.set_key(
                AnimProperty::CopyLocationWeight,
                2.0,
                0.5,
                Interpolation::Linear,
            );
        }
        node.tag_animations.insert(0, clip);
    }

    let source_tag = AnimTarget::Tag(cube, 0);
    assert!(activate_curve_target(&mut harness.app, source_tag));
    let source_tag_clip = harness.app.target_clip(source_tag).unwrap();
    assert!(harness.app.select_keys_in_box(
        source_tag,
        &source_tag_clip,
        (0.0, 1.0),
        (0.0, 1.0),
        false
    ));
    assert_eq!(harness.app.selected_curve_keys().len(), 2);
    assert!(harness.app.copy_selected_curve_keys());

    let destination_tag = AnimTarget::Tag(sphere, 0);
    assert!(activate_curve_target(&mut harness.app, destination_tag));
    let destination_clip = harness.app.target_clip(destination_tag).unwrap();
    assert!(harness.app.select_keys_in_box(
        destination_tag,
        &destination_clip,
        (2.0, 2.0),
        (0.0, 1.0),
        false
    ));
    let tag_prior_selection = harness.app.selected_curve_keys();
    assert_eq!(tag_prior_selection.len(), 1);
    harness.app.playback.time = 4.0;
    harness.app.history.clear();

    assert!(harness.app.paste_curve_keys());
    assert_eq!(harness.app.history.depth(), 1);
    assert_property_curve_times(
        &harness.app,
        destination_tag,
        AnimProperty::CopyLocationWeight,
        &[2.0, 4.0, 5.0],
    );
    let tag_pasted_selection = harness.app.selected_curve_keys();
    assert_eq!(tag_pasted_selection.len(), 2);

    let away = AnimTarget::Object(cube);
    assert!(activate_curve_target(&mut harness.app, away));
    assert_eq!(harness.app.current_anim_target(), Some(away));
    harness.press_key(egui::Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(harness.app.history.depth(), 0);
    assert_property_curve_times(
        &harness.app,
        destination_tag,
        AnimProperty::CopyLocationWeight,
        &[2.0],
    );
    assert_eq!(
        harness.app.current_anim_target(),
        Some(destination_tag),
        "undo re-activates the pasted tag target"
    );
    assert_eq!(harness.app.selected_curve_keys(), tag_prior_selection);
    assert_eq!(harness.app.selection.primary(), Some(sphere));

    assert!(activate_curve_target(&mut harness.app, away));
    harness.press_key(
        egui::Key::Z,
        egui::Modifiers {
            shift: true,
            ..egui::Modifiers::COMMAND
        },
    );
    assert_eq!(harness.app.history.depth(), 1);
    assert_property_curve_times(
        &harness.app,
        destination_tag,
        AnimProperty::CopyLocationWeight,
        &[2.0, 4.0, 5.0],
    );
    assert_eq!(
        harness.app.current_anim_target(),
        Some(destination_tag),
        "redo re-activates the pasted tag target"
    );
    assert_eq!(harness.app.selected_curve_keys(), tag_pasted_selection);
    assert_eq!(harness.app.selection.primary(), Some(sphere));

    for material_id in [0, 1] {
        let material = harness.app.doc.materials.get_mut(material_id).unwrap();
        let mut clip = AnimationClip::default();
        if material_id == 0 {
            clip.set_key(
                AnimProperty::MaterialRoughness,
                0.0,
                0.2,
                Interpolation::Linear,
            );
            clip.set_key(
                AnimProperty::MaterialRoughness,
                1.0,
                0.8,
                Interpolation::Linear,
            );
        } else {
            clip.set_key(
                AnimProperty::MaterialRoughness,
                2.0,
                0.5,
                Interpolation::Linear,
            );
        }
        material.animation = clip;
    }

    let source_material = AnimTarget::Material(0);
    assert!(activate_curve_target(&mut harness.app, source_material));
    let source_material_clip = harness.app.target_clip(source_material).unwrap();
    assert!(harness.app.select_keys_in_box(
        source_material,
        &source_material_clip,
        (0.0, 1.0),
        (0.0, 1.0),
        false
    ));
    assert!(harness.app.copy_selected_curve_keys());

    let destination_material = AnimTarget::Material(1);
    assert!(activate_curve_target(
        &mut harness.app,
        destination_material
    ));
    let destination_clip = harness.app.target_clip(destination_material).unwrap();
    assert!(harness.app.select_keys_in_box(
        destination_material,
        &destination_clip,
        (2.0, 2.0),
        (0.0, 1.0),
        false
    ));
    let material_prior_selection = harness.app.selected_curve_keys();
    assert_eq!(material_prior_selection.len(), 1);
    harness.app.playback.time = 4.0;
    harness.app.history.clear();

    assert!(harness.app.paste_curve_keys());
    assert_eq!(harness.app.history.depth(), 1);
    assert_property_curve_times(
        &harness.app,
        destination_material,
        AnimProperty::MaterialRoughness,
        &[2.0, 4.0, 5.0],
    );
    let material_pasted_selection = harness.app.selected_curve_keys();
    assert_eq!(material_pasted_selection.len(), 2);

    let away = AnimTarget::Object(sphere);
    assert!(activate_curve_target(&mut harness.app, away));
    assert_eq!(harness.app.current_anim_target(), Some(away));
    harness.press_key(egui::Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(harness.app.history.depth(), 0);
    assert_property_curve_times(
        &harness.app,
        destination_material,
        AnimProperty::MaterialRoughness,
        &[2.0],
    );
    assert_eq!(
        harness.app.current_anim_target(),
        Some(destination_material),
        "undo re-activates the pasted material target"
    );
    assert_eq!(harness.app.selected_curve_keys(), material_prior_selection);

    assert!(activate_curve_target(&mut harness.app, away));
    harness.press_key(
        egui::Key::Z,
        egui::Modifiers {
            shift: true,
            ..egui::Modifiers::COMMAND
        },
    );
    assert_eq!(harness.app.history.depth(), 1);
    assert_property_curve_times(
        &harness.app,
        destination_material,
        AnimProperty::MaterialRoughness,
        &[2.0, 4.0, 5.0],
    );
    assert_eq!(
        harness.app.current_anim_target(),
        Some(destination_material),
        "redo re-activates the pasted material target"
    );
    assert_eq!(harness.app.selected_curve_keys(), material_pasted_selection);
}

#[test]
fn fcurve_paste_undo_with_missing_target_restores_edit_and_preserves_history() {
    let (mut harness, _root) = InputHarness::new(
        "curve-paste-missing-target",
        Surface::Shortcuts,
        Vec2::new(800.0, 600.0),
    );
    harness.app.show_curves = true;
    let cube = named(&harness.app, "Cube");
    let sphere = named(&harness.app, "Sphere");
    for (node_id, source) in [(cube, sphere), (sphere, cube)] {
        let node = harness.app.doc.scene.get_mut(node_id).unwrap();
        node.tags.clear();
        node.tags.push(Tag::CopyLocation {
            source,
            offset: Vec3::ZERO,
            weight: 0.0,
        });
    }
    let mut cube_clip = AnimationClip::default();
    for time in [0.0, 1.0] {
        cube_clip.set_key(
            AnimProperty::CopyLocationWeight,
            time,
            0.5,
            Interpolation::Linear,
        );
    }
    harness
        .app
        .doc
        .scene
        .get_mut(cube)
        .unwrap()
        .tag_animations
        .insert(0, cube_clip);
    let mut sphere_clip = AnimationClip::default();
    sphere_clip.set_key(
        AnimProperty::CopyLocationWeight,
        2.0,
        0.5,
        Interpolation::Linear,
    );
    harness
        .app
        .doc
        .scene
        .get_mut(sphere)
        .unwrap()
        .tag_animations
        .insert(0, sphere_clip);

    let source_tag = AnimTarget::Tag(cube, 0);
    assert!(activate_curve_target(&mut harness.app, source_tag));
    let source_clip = harness.app.target_clip(source_tag).unwrap();
    assert!(harness.app.select_keys_in_box(
        source_tag,
        &source_clip,
        (0.0, 1.0),
        (0.0, 1.0),
        false
    ));
    assert!(harness.app.copy_selected_curve_keys());

    let destination_tag = AnimTarget::Tag(sphere, 0);
    assert!(activate_curve_target(&mut harness.app, destination_tag));
    harness.app.playback.time = 4.0;
    harness.app.history.clear();
    assert!(harness.app.paste_curve_keys());
    assert_eq!(harness.app.history.depth(), 1);

    // Switch away, then make the recorded tag target disappear without a
    // history command. Undo must still run and redo must remain usable.
    let away = AnimTarget::Object(cube);
    assert!(activate_curve_target(&mut harness.app, away));
    harness.app.doc.scene.get_mut(sphere).unwrap().tags.clear();
    assert!(!harness.app.target_is_valid(destination_tag));

    harness.press_key(egui::Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(harness.app.history.depth(), 0);
    assert!(harness.app.history.can_redo());
    assert_eq!(harness.app.current_anim_target(), Some(away));
    let restored = harness
        .app
        .doc
        .scene
        .get(sphere)
        .unwrap()
        .tag_animations
        .get(&0)
        .unwrap();
    let restored_times: Vec<f32> = restored
        .track(AnimProperty::CopyLocationWeight)
        .unwrap()
        .curve
        .keys
        .iter()
        .map(|key| key.time)
        .collect();
    assert_eq!(restored_times, vec![2.0]);

    harness.press_key(
        egui::Key::Z,
        egui::Modifiers {
            shift: true,
            ..egui::Modifiers::COMMAND
        },
    );
    assert_eq!(harness.app.history.depth(), 1);
    assert!(harness.app.history.can_undo());
    assert!(!harness.app.history.can_redo());
    assert_eq!(harness.app.current_anim_target(), Some(away));
    let reapplied = harness
        .app
        .doc
        .scene
        .get(sphere)
        .unwrap()
        .tag_animations
        .get(&0)
        .unwrap();
    let reapplied_times: Vec<f32> = reapplied
        .track(AnimProperty::CopyLocationWeight)
        .unwrap()
        .curve
        .keys
        .iter()
        .map(|key| key.time)
        .collect();
    assert_eq!(reapplied_times, vec![2.0, 4.0, 5.0]);
}

#[test]
fn fcurve_paste_undo_with_deleted_object_target_keeps_history_sound() {
    let (mut harness, _root) = InputHarness::new(
        "curve-paste-deleted-object",
        Surface::Shortcuts,
        Vec2::new(800.0, 600.0),
    );
    harness.app.show_curves = true;
    let cube = named(&harness.app, "Cube");
    let sphere = named(&harness.app, "Sphere");
    {
        let node = harness.app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        node.animation
            .set_key(AnimProperty::TranslationX, 0.0, 1.0, Interpolation::Linear);
    }
    let source = AnimTarget::Object(cube);
    assert!(activate_curve_target(&mut harness.app, source));
    let clip = harness.app.target_clip(source).unwrap();
    assert!(harness
        .app
        .select_keys_in_box(source, &clip, (0.0, 0.0), (0.0, 10.0), false));
    assert!(harness.app.copy_selected_curve_keys());

    let destination = AnimTarget::Object(sphere);
    assert!(activate_curve_target(&mut harness.app, destination));
    harness.app.playback.time = 3.0;
    harness.app.history.clear();
    assert!(harness.app.paste_curve_keys());
    assert_eq!(harness.app.history.depth(), 1);

    assert!(activate_curve_target(&mut harness.app, source));
    harness.app.doc.scene.remove_subtree(sphere);
    assert!(!harness.app.target_is_valid(destination));

    harness.press_key(egui::Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(harness.app.history.depth(), 0);
    assert!(harness.app.history.can_redo());
    assert_eq!(harness.app.current_anim_target(), Some(source));

    harness.press_key(
        egui::Key::Z,
        egui::Modifiers {
            shift: true,
            ..egui::Modifiers::COMMAND
        },
    );
    assert_eq!(harness.app.history.depth(), 1);
    assert!(harness.app.history.can_undo());
    assert!(!harness.app.history.can_redo());
    assert_eq!(harness.app.current_anim_target(), Some(source));
}

#[test]
fn fcurve_multi_key_drag_preserves_spacing_as_one_undo_step() {
    let (mut app, _root) = test_app("curve-multi-drag");
    let cube = named(&app, "Cube");
    app.selection.select_one(cube);
    {
        let node = app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        for (time, value) in [(0.0, 1.0), (1.0, 2.0), (2.0, 3.0)] {
            node.animation.set_key(
                AnimProperty::TranslationX,
                time,
                value,
                Interpolation::Linear,
            );
        }
    }
    let target = AnimTarget::Object(cube);
    assert!(app.set_anim_target(target));
    let clip = app.target_clip(target).unwrap();
    assert!(app.select_keys_in_box(target, &clip, (0.0, 2.0), (0.0, 4.0), false));
    assert_eq!(app.selected_curve_keys().len(), 3);
    app.history.clear();
    assert!(app.move_selected_curve_key(5.0));
    assert_eq!(app.history.depth(), 1, "multi-key drag is one undo step");
    let moved = app.target_clip(target).unwrap();
    let keys = &moved.track(AnimProperty::TranslationX).unwrap().curve.keys;
    assert_eq!(keys.len(), 3);
    assert!((keys[0].time - 3.0).abs() < 1e-4);
    assert!((keys[1].time - 4.0).abs() < 1e-4);
    assert!((keys[2].time - 5.0).abs() < 1e-4);
    assert!((keys[0].value - 1.0).abs() < 1e-4);
    app.history.undo(&mut app.doc);
    let restored = app.target_clip(target).unwrap();
    let keys = &restored
        .track(AnimProperty::TranslationX)
        .unwrap()
        .curve
        .keys;
    assert!((keys[0].time - 0.0).abs() < 1e-4);
    assert!((keys[1].time - 1.0).abs() < 1e-4);
    assert!((keys[2].time - 2.0).abs() < 1e-4);
}

#[test]
fn fcurve_multi_key_drag_clamps_at_zero_without_collapsing_spacing() {
    let (mut app, _root) = test_app("curve-multi-clamp");
    let cube = named(&app, "Cube");
    app.selection.select_one(cube);
    {
        let node = app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        for time in [1.0, 2.0, 3.0] {
            node.animation.set_key(
                AnimProperty::TranslationX,
                time,
                time,
                Interpolation::Linear,
            );
        }
    }
    let target = AnimTarget::Object(cube);
    assert!(app.set_anim_target(target));
    let clip = app.target_clip(target).unwrap();
    assert!(app.select_keys_in_box(target, &clip, (1.0, 3.0), (1.0, 3.0), false));
    assert!(app.move_selected_curve_key(-1.0));
    let moved = app.target_clip(target).unwrap();
    let keys = &moved.track(AnimProperty::TranslationX).unwrap().curve.keys;
    assert_eq!(keys.len(), 3);
    assert!((keys[0].time - 0.0).abs() < 1e-4);
    assert!((keys[1].time - 1.0).abs() < 1e-4);
    assert!((keys[2].time - 2.0).abs() < 1e-4);
}

#[test]
fn fcurve_paste_uses_playhead_not_source_anchor_for_relative_offsets() {
    let (mut app, _root) = test_app("curve-paste-offset");
    let cube = named(&app, "Cube");
    app.selection.select_one(cube);
    {
        let node = app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        for (time, value) in [(5.0, 1.0), (6.0, 2.0)] {
            node.animation.set_key(
                AnimProperty::TranslationX,
                time,
                value,
                Interpolation::Linear,
            );
        }
    }
    let target = AnimTarget::Object(cube);
    assert!(app.set_anim_target(target));
    let clip = app.target_clip(target).unwrap();
    assert!(app.select_keys_in_box(target, &clip, (5.0, 6.0), (1.0, 2.0), false));
    assert!(app.copy_selected_curve_keys());
    app.playback.time = 1.0;
    assert!(app.paste_curve_keys());
    let pasted = app.target_clip(target).unwrap();
    let keys = &pasted.track(AnimProperty::TranslationX).unwrap().curve.keys;
    assert!(keys.iter().any(|key| (key.time - 1.0).abs() < 1e-4));
    assert!(keys.iter().any(|key| (key.time - 2.0).abs() < 1e-4));
}

#[test]
fn fcurve_copy_paste_round_trips_selected_keys_as_one_undo() {
    let (mut app, _root) = test_app("curve-copy-round-trip");
    let cube = named(&app, "Cube");
    let sphere = named(&app, "Sphere");
    app.selection.select_one(cube);
    {
        let node = app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        node.animation
            .set_key(AnimProperty::TranslationX, 0.0, 1.0, Interpolation::Linear);
        node.animation
            .set_key(AnimProperty::TranslationX, 1.0, 2.0, Interpolation::Linear);
    }
    let source = AnimTarget::Object(cube);
    assert!(app.set_anim_target(source));
    assert!(app.select_target_track_key_additive(
        source,
        CurveTrackId::Scalar(AnimProperty::TranslationX),
        0.0,
        false
    ));
    assert!(app.select_target_track_key_additive(
        source,
        CurveTrackId::Scalar(AnimProperty::TranslationX),
        1.0,
        true
    ));
    assert!(app.copy_selected_curve_keys());
    app.selection.select_one(sphere);
    let destination = AnimTarget::Object(sphere);
    assert!(app.set_anim_target(destination));
    app.playback.time = 3.0;
    app.history.clear();
    assert!(app.paste_curve_keys());
    assert_eq!(app.history.depth(), 1, "paste is one undo step");
    let pasted = app.target_clip(destination).unwrap();
    let keys = &pasted.track(AnimProperty::TranslationX).unwrap().curve.keys;
    assert!(keys.iter().any(|key| (key.time - 3.0).abs() < 1e-4));
    assert!(keys.iter().any(|key| (key.time - 4.0).abs() < 1e-4));
    assert!((pasted.sample_or(AnimProperty::TranslationX, 3.0, -1.0) - 1.0).abs() < 1e-4);
    assert!((pasted.sample_or(AnimProperty::TranslationX, 4.0, -1.0) - 2.0).abs() < 1e-4);
    app.history.undo(&mut app.doc);
    let restored = app.target_clip(destination).unwrap();
    assert!(!restored
        .track(AnimProperty::TranslationX)
        .is_some_and(|track| track
            .curve
            .keys
            .iter()
            .any(|key| (key.time - 3.0).abs() < 1e-4)));
}

#[test]
fn fcurve_keyboard_copy_paste_uses_the_playhead() {
    let (mut harness, _root) = InputHarness::new(
        "curve-keyboard-copy",
        Surface::Shortcuts,
        Vec2::new(800.0, 600.0),
    );
    harness.app.show_curves = true;
    let cube = named(&harness.app, "Cube");
    harness.app.selection.select_one(cube);
    {
        let node = harness.app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        node.animation
            .set_key(AnimProperty::TranslationX, 0.0, 4.0, Interpolation::Linear);
    }
    let target = AnimTarget::Object(cube);
    assert!(harness.app.set_anim_target(target));
    let clip = harness.app.target_clip(target).unwrap();
    assert!(harness
        .app
        .select_keys_in_box(target, &clip, (0.0, 0.0), (0.0, 10.0), false));
    harness.press_key(egui::Key::C, egui::Modifiers::COMMAND);
    assert_eq!(harness.app.curve_clipboard.keys.len(), 1);
    harness.app.playback.time = 2.0;
    harness.press_key(egui::Key::V, egui::Modifiers::COMMAND);
    let pasted = harness.app.target_clip(target).unwrap();
    assert!((pasted.sample_or(AnimProperty::TranslationX, 2.0, -1.0) - 4.0).abs() < 1e-4);
    assert_eq!(harness.app.history.depth(), 1);
}

#[test]
fn fcurve_copy_paste_handle_native_clipboard_events() {
    let (mut harness, _root) = InputHarness::new(
        "curve-native-clipboard",
        Surface::Shortcuts,
        Vec2::new(800.0, 600.0),
    );
    harness.app.show_curves = true;
    let cube = named(&harness.app, "Cube");
    harness.app.selection.select_one(cube);
    {
        let node = harness.app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        node.animation
            .set_key(AnimProperty::TranslationX, 0.0, 4.0, Interpolation::Linear);
    }
    let target = AnimTarget::Object(cube);
    assert!(harness.app.set_anim_target(target));
    let clip = harness.app.target_clip(target).unwrap();
    assert!(harness
        .app
        .select_keys_in_box(target, &clip, (0.0, 0.0), (0.0, 10.0), false));
    harness.frame(
        Surface::Shortcuts,
        vec![egui::Event::Copy],
        egui::Modifiers::COMMAND,
    );
    assert_eq!(harness.app.curve_clipboard.keys.len(), 1);
    harness.app.playback.time = 2.0;
    harness.frame(
        Surface::Shortcuts,
        vec![egui::Event::Paste(String::new())],
        egui::Modifiers::COMMAND,
    );
    let pasted = harness.app.target_clip(target).unwrap();
    assert!((pasted.sample_or(AnimProperty::TranslationX, 2.0, -1.0) - 4.0).abs() < 1e-4);
    // The V release after a real clipboard paste must not paste a second time.
    let depth = harness.app.history.depth();
    harness.frame(
        Surface::Shortcuts,
        vec![egui::Event::Key {
            key: egui::Key::V,
            physical_key: Some(egui::Key::V),
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }],
        egui::Modifiers::COMMAND,
    );
    assert_eq!(harness.app.history.depth(), depth);
}

#[test]
fn fcurve_paste_works_when_native_clipboard_is_empty() {
    let (mut harness, _root) = InputHarness::new(
        "curve-empty-clipboard",
        Surface::Shortcuts,
        Vec2::new(800.0, 600.0),
    );
    harness.app.show_curves = true;
    let cube = named(&harness.app, "Cube");
    harness.app.selection.select_one(cube);
    {
        let node = harness.app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        node.animation
            .set_key(AnimProperty::TranslationX, 0.0, 4.0, Interpolation::Linear);
    }
    let target = AnimTarget::Object(cube);
    assert!(harness.app.set_anim_target(target));
    let clip = harness.app.target_clip(target).unwrap();
    assert!(harness
        .app
        .select_keys_in_box(target, &clip, (0.0, 0.0), (0.0, 10.0), false));
    assert!(harness.app.copy_selected_curve_keys());
    harness.app.playback.time = 2.0;
    let depth = harness.app.history.depth();
    // An empty system clipboard produces no Event::Paste; only Ctrl/Cmd+V's
    // key release reaches egui-winit.
    harness.frame(
        Surface::Shortcuts,
        vec![egui::Event::Key {
            key: egui::Key::V,
            physical_key: Some(egui::Key::V),
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }],
        egui::Modifiers::COMMAND,
    );
    assert_eq!(harness.app.history.depth(), depth + 1);
    let pasted = harness.app.target_clip(target).unwrap();
    assert!((pasted.sample_or(AnimProperty::TranslationX, 2.0, -1.0) - 4.0).abs() < 1e-4);
}

#[test]
fn fcurve_empty_clipboard_paste_accepts_ctrl_release_before_v() {
    let (mut harness, _root) = InputHarness::new(
        "curve-empty-clipboard-release-order",
        Surface::Shortcuts,
        Vec2::new(800.0, 600.0),
    );
    harness.app.show_curves = true;
    let cube = named(&harness.app, "Cube");
    harness.app.selection.select_one(cube);
    {
        let node = harness.app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        node.animation
            .set_key(AnimProperty::TranslationX, 0.0, 4.0, Interpolation::Linear);
    }
    let target = AnimTarget::Object(cube);
    assert!(harness.app.set_anim_target(target));
    let clip = harness.app.target_clip(target).unwrap();
    assert!(harness
        .app
        .select_keys_in_box(target, &clip, (0.0, 0.0), (0.0, 10.0), false));
    assert!(harness.app.copy_selected_curve_keys());
    harness.app.playback.time = 2.0;
    let depth = harness.app.history.depth();
    let release = |key| egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    };
    harness.frame(
        Surface::Shortcuts,
        vec![release(egui::Key::ControlLeft), release(egui::Key::V)],
        egui::Modifiers::default(),
    );
    assert_eq!(harness.app.history.depth(), depth + 1);
    let pasted = harness.app.target_clip(target).unwrap();
    assert!((pasted.sample_or(AnimProperty::TranslationX, 2.0, -1.0) - 4.0).abs() < 1e-4);
}

#[test]
fn fcurve_internal_paste_undo_redo_restores_selection_and_keys() {
    let (mut harness, _root, target, prior_selection) =
        curve_paste_fixture("curve-paste-undo-internal");
    assert!(harness.app.copy_selected_curve_keys());
    harness.app.playback.time = 3.0;
    harness.app.history.clear();

    assert!(harness.app.paste_curve_keys());
    assert_eq!(harness.app.history.depth(), 1, "paste is one undo entry");
    assert_curve_times(&harness.app, target, &[0.0, 1.0, 3.0, 4.0]);
    let pasted_selection = harness.app.selected_curve_keys();
    assert_eq!(pasted_selection.len(), 2);

    harness.press_key(egui::Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(harness.app.history.depth(), 0);
    assert_curve_times(&harness.app, target, &[0.0, 1.0]);
    assert_eq!(harness.app.selected_curve_keys(), prior_selection);
    assert_eq!(
        harness.app.selection.primary(),
        Some(named(&harness.app, "Cube"))
    );

    harness.press_key(
        egui::Key::Z,
        egui::Modifiers {
            shift: true,
            ..egui::Modifiers::COMMAND
        },
    );
    assert_eq!(harness.app.history.depth(), 1);
    assert_curve_times(&harness.app, target, &[0.0, 1.0, 3.0, 4.0]);
    assert_eq!(harness.app.selected_curve_keys(), pasted_selection);
}

#[test]
fn fcurve_native_paste_undo_redo_uses_one_history_entry() {
    let (mut harness, _root, target, prior_selection) =
        curve_paste_fixture("curve-paste-undo-native");
    harness.frame(
        Surface::Shortcuts,
        vec![egui::Event::Copy],
        egui::Modifiers::COMMAND,
    );
    harness.app.playback.time = 3.0;
    harness.app.history.clear();

    harness.frame(
        Surface::Shortcuts,
        vec![egui::Event::Paste("external clipboard text".into())],
        egui::Modifiers::COMMAND,
    );
    assert_eq!(harness.app.history.depth(), 1, "paste is one undo entry");
    assert_curve_times(&harness.app, target, &[0.0, 1.0, 3.0, 4.0]);
    let pasted_selection = harness.app.selected_curve_keys();

    // The release of a native clipboard chord must not paste a second time.
    harness.frame(
        Surface::Shortcuts,
        vec![egui::Event::Key {
            key: egui::Key::V,
            physical_key: Some(egui::Key::V),
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }],
        egui::Modifiers::COMMAND,
    );
    assert_eq!(harness.app.history.depth(), 1);

    harness.press_key(egui::Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(harness.app.history.depth(), 0);
    assert_curve_times(&harness.app, target, &[0.0, 1.0]);
    assert_eq!(harness.app.selected_curve_keys(), prior_selection);

    harness.press_key(
        egui::Key::Z,
        egui::Modifiers {
            shift: true,
            ..egui::Modifiers::COMMAND
        },
    );
    assert_eq!(harness.app.history.depth(), 1);
    assert_curve_times(&harness.app, target, &[0.0, 1.0, 3.0, 4.0]);
    assert_eq!(harness.app.selected_curve_keys(), pasted_selection);
}

#[test]
fn fcurve_empty_clipboard_paste_undo_redo_restores_selection_and_keys() {
    let (mut harness, _root, target, prior_selection) =
        curve_paste_fixture("curve-paste-undo-empty-clipboard");
    assert!(harness.app.copy_selected_curve_keys());
    harness.app.playback.time = 3.0;
    harness.app.history.clear();

    // An empty system clipboard produces no Event::Paste; only the V release
    // reaches the editor, which must still create one undoable paste.
    harness.frame(
        Surface::Shortcuts,
        vec![egui::Event::Key {
            key: egui::Key::V,
            physical_key: Some(egui::Key::V),
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }],
        egui::Modifiers::COMMAND,
    );
    assert_eq!(harness.app.history.depth(), 1, "paste is one undo entry");
    assert_curve_times(&harness.app, target, &[0.0, 1.0, 3.0, 4.0]);
    let pasted_selection = harness.app.selected_curve_keys();

    harness.press_key(egui::Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(harness.app.history.depth(), 0);
    assert_curve_times(&harness.app, target, &[0.0, 1.0]);
    assert_eq!(harness.app.selected_curve_keys(), prior_selection);

    harness.press_key(
        egui::Key::Z,
        egui::Modifiers {
            shift: true,
            ..egui::Modifiers::COMMAND
        },
    );
    assert_eq!(harness.app.history.depth(), 1);
    assert_curve_times(&harness.app, target, &[0.0, 1.0, 3.0, 4.0]);
    assert_eq!(harness.app.selected_curve_keys(), pasted_selection);
}

#[test]
fn fcurve_empty_clipboard_paste_noop_leaves_history_and_keys_unchanged() {
    let (mut harness, _root, target, prior_selection) =
        curve_paste_fixture("curve-paste-empty-noop");
    let signature = harness.app.doc.signature();
    harness.app.history.clear();

    assert!(!harness.app.paste_curve_keys());
    harness.frame(
        Surface::Shortcuts,
        vec![egui::Event::Paste(String::new())],
        egui::Modifiers::COMMAND,
    );
    // The native paste above leaves a pending release; swallow it, then send a
    // second release so the empty-clipboard fallback also gets exercised.
    for _ in 0..2 {
        harness.frame(
            Surface::Shortcuts,
            vec![egui::Event::Key {
                key: egui::Key::V,
                physical_key: Some(egui::Key::V),
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
            egui::Modifiers::COMMAND,
        );
    }

    assert_eq!(harness.app.history.depth(), 0);
    assert!(!harness.app.history.can_undo());
    assert!(!harness.app.history.can_redo());
    assert_eq!(harness.app.doc.signature(), signature);
    assert_curve_times(&harness.app, target, &[0.0, 1.0]);
    assert_eq!(harness.app.selected_curve_keys(), prior_selection);
}

#[test]
fn fcurve_keyboard_shortcuts_survive_non_text_widget_focus() {
    let (mut harness, _root) = InputHarness::new(
        "curve-keyboard-focus",
        Surface::Shortcuts,
        Vec2::new(800.0, 600.0),
    );
    harness.app.show_curves = true;
    let cube = named(&harness.app, "Cube");
    harness.app.selection.select_one(cube);
    {
        let node = harness.app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        node.animation
            .set_key(AnimProperty::TranslationX, 0.0, 4.0, Interpolation::Linear);
    }
    let target = AnimTarget::Object(cube);
    assert!(harness.app.set_anim_target(target));
    let clip = harness.app.target_clip(target).unwrap();
    assert!(harness
        .app
        .select_keys_in_box(target, &clip, (0.0, 0.0), (0.0, 10.0), false));
    harness
        .ctx
        .memory_mut(|memory| memory.request_focus(egui::Id::new("a_clicked_button")));
    assert!(!harness.ctx.text_edit_focused());
    harness.press_key(egui::Key::C, egui::Modifiers::COMMAND);
    assert_eq!(harness.app.curve_clipboard.keys.len(), 1);
    harness.app.playback.time = 2.0;
    harness.press_key(egui::Key::V, egui::Modifiers::COMMAND);
    let pasted = harness.app.target_clip(target).unwrap();
    assert!((pasted.sample_or(AnimProperty::TranslationX, 2.0, -1.0) - 4.0).abs() < 1e-4);
}
