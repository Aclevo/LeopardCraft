//! Deterministic checks that real scene warnings reach the editor's visible
//! state. These tests build the actual `LeopardApp` with a headless viewport
//! (no window or GPU) and call the same open, save and autosave methods the
//! egui shell calls. Assertions read `status` and `console`, which are the
//! fields rendered by the status bar and Console panel.

use super::*;
use std::process::Command;

static NEXT_ROOT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct TestRoot {
    path: PathBuf,
}

impl TestRoot {
    fn new(label: &str) -> Self {
        let sequence = NEXT_ROOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "leopardcraft-warning-surface-{}-{sequence}-{label}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn dir(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn test_app(recovery_dir: &Path, autosave_interval: Duration) -> LeopardApp {
    let doc = Document::demo();
    let fps = doc.settings.fps.max(1.0);
    let mut selection = Selection::default();
    if let Some(id) = doc.scene.find_by_name("Cube") {
        selection.select_one(id);
    }
    let mut lifecycle = DocumentLifecycle::new(
        RecoveryStore::at(recovery_dir.to_path_buf()),
        autosave_interval,
    );
    assert!(matches!(lifecycle.startup(&doc), StartupOutcome::Clean));

    LeopardApp {
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
    }
}

fn seed_scene(path: &Path, doc: &Document) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    lc_io::save_native(path, doc).unwrap();
}

fn rename_root(doc: &mut Document, name: &str) {
    let id = *doc.scene.roots.first().expect("demo scene has roots");
    doc.scene.get_mut(id).expect("root node exists").name = name.into();
}

fn block_staging(target: &Path, nonce: u64) -> PathBuf {
    let parent = target.parent().expect("target has a parent");
    let name = target
        .file_name()
        .expect("target has a file name")
        .to_string_lossy();
    let stale = parent.join(format!(".{name}.tmp-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&stale).unwrap();
    std::fs::write(stale.join("blocker"), b"keep").unwrap();
    stale
}

fn assert_console_warning(app: &LeopardApp, needle: &str, path: &Path) {
    let path = path.display().to_string();
    let line = app
        .console
        .iter()
        .rev()
        .find(|line| line.contains("warning: ") && line.contains(needle) && line.contains(&path))
        .unwrap_or_else(|| {
            panic!(
                "no Console line contains {needle:?} for {path}; console={:?}",
                app.console
            )
        });
    assert!(line.contains(&path));
}

fn assert_logged_status(app: &LeopardApp, prefix: &str, needle: &str) {
    let line = app
        .console
        .iter()
        .rev()
        .find(|line| line.contains(prefix) && line.contains(needle))
        .unwrap_or_else(|| {
            panic!(
                "no logged status line contains {prefix:?} and {needle:?}; status={:?}; console={:?}",
                app.status, app.console
            )
        });
    assert!(line.contains(prefix));
}

fn exercise_scene_warning_surfaces() {
    let root = TestRoot::new("open-save");
    let startup_dir = root.dir("startup");
    let open_dir = root.dir("elsewhere");
    std::fs::create_dir_all(&startup_dir).unwrap();
    std::fs::create_dir_all(&open_dir).unwrap();
    let startup_path = startup_dir.join("startup.lcscene");
    let open_path = open_dir.join("scene.lcscene");
    seed_scene(&startup_path, &Document::demo());
    let mut elsewhere = Document::demo();
    rename_root(&mut elsewhere, "Elsewhere Scene");
    let elsewhere_signature = elsewhere.signature();
    seed_scene(&open_path, &elsewhere);
    assert_ne!(
        startup_path.parent(),
        open_path.parent(),
        "the open target must not be in the startup directory"
    );

    let mut app = test_app(&root.dir("recovery-main"), Duration::ZERO);
    // `launch_gui` establishes the startup scene this way before the first frame.
    app.load_scene(Document::demo(), Some(startup_path.clone()));
    assert_eq!(app.lifecycle.path(), Some(startup_path.as_path()));
    assert!(app.status.starts_with("Ready"));
    let _ = lc_scene::take_warnings();

    let stale = block_staging(&open_path, 9_000_001);
    assert!(app.open_file(open_path.clone()));
    assert_eq!(app.lifecycle.path(), Some(open_path.as_path()));
    assert_eq!(app.doc.signature(), elsewhere_signature);
    assert!(
        app.status.starts_with("Opened "),
        "open status was replaced: {:?}",
        app.status
    );
    assert!(
        app.status.contains("failed to reconcile"),
        "open warning missing from status: {:?}",
        app.status
    );
    assert_console_warning(&app, "failed to reconcile", &open_path);
    assert!(stale.is_dir(), "the blocked staging artifact was removed");
    app.log_status();
    assert_logged_status(&app, "Opened ", "failed to reconcile");

    assert!(app
        .save_file(open_path.clone())
        .expect("save should succeed despite the warning")
        .is_none());
    assert!(
        app.status.starts_with("Saved "),
        "save status was replaced: {:?}",
        app.status
    );
    assert!(
        app.status.contains("failed to reconcile"),
        "save warning missing from status: {:?}",
        app.status
    );
    assert_console_warning(&app, "failed to reconcile", &open_path);
    assert!(stale.is_dir(), "the blocked staging artifact was removed");
    app.log_status();
    assert_logged_status(&app, "Saved ", "failed to reconcile");
    assert!(lc_io::load_native(&open_path).is_ok());
}

fn exercise_autosave_warning_surface() {
    let root = TestRoot::new("autosave");
    let recovery_dir = root.dir("recovery");
    std::fs::create_dir_all(&recovery_dir).unwrap();
    let snapshot_path = recovery_dir.join("recovery.lcscene");
    let mut app = test_app(&recovery_dir, Duration::ZERO);
    rename_root(&mut app.doc, "Autosave dirty");
    let _ = lc_scene::take_warnings();

    let stale = block_staging(&snapshot_path, 9_000_002);
    app.autosave_tick(&eframe::egui::Context::default());
    assert!(
        app.status.starts_with("Ready"),
        "autosave replaced the status: {:?}",
        app.status
    );
    assert!(
        app.status.contains("failed to reconcile"),
        "autosave warning missing from status: {:?}",
        app.status
    );
    assert_console_warning(&app, "failed to reconcile", &snapshot_path);
    assert!(stale.is_dir(), "the blocked staging artifact was removed");
    assert!(lc_io::load_native(&snapshot_path).is_ok());
    app.log_status();
    assert_logged_status(&app, "Ready", "failed to reconcile");
}

#[cfg(unix)]
struct NoReadDir {
    path: PathBuf,
}

#[cfg(unix)]
impl NoReadDir {
    fn new(path: &Path) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o300);
        std::fs::set_permissions(path, permissions).unwrap();
        Self {
            path: path.to_path_buf(),
        }
    }
}

#[cfg(unix)]
impl Drop for NoReadDir {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&self.path) {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o700);
            let _ = std::fs::set_permissions(&self.path, permissions);
        }
    }
}

/// Remove directory read permission so `read_dir` (staging scan) and the
/// directory `File::open` used for the post-rename fsync both fail. Creating
/// and renaming the file still work because the directory keeps write+execute.
#[cfg(unix)]
fn exercise_directory_flush_surface() -> bool {
    let root = TestRoot::new("directory-flush");
    let scene_dir = root.dir("unreadable");
    std::fs::create_dir_all(&scene_dir).unwrap();
    let path = scene_dir.join("scene.lcscene");
    seed_scene(&path, &Document::demo());
    let mut app = test_app(&root.dir("recovery"), Duration::ZERO);
    app.load_scene(Document::demo(), Some(path.clone()));
    let _ = lc_scene::take_warnings();

    let guard = NoReadDir::new(&scene_dir);
    if std::fs::File::open(&scene_dir).is_ok() {
        drop(guard);
        eprintln!(
            "skipping directory-flush surface check: this user bypasses directory permissions"
        );
        return false;
    }

    let result = app.save_file(path.clone());
    drop(guard);
    let action = result.expect("save should commit despite the directory-flush warning");
    assert!(action.is_none());
    assert!(
        app.status.starts_with("Saved "),
        "flush save status was replaced: {:?}",
        app.status
    );
    assert!(
        app.status.contains("scene warnings"),
        "flush warnings are not reflected in status: {:?}",
        app.status
    );
    let path_text = path.display().to_string();
    assert!(
        app.console.iter().any(|line| {
            line.contains("warning: saved")
                && line.contains("could not flush its directory entry")
                && line.contains(&path_text)
        }),
        "directory-flush warning missing from Console: {:?}",
        app.console
    );
    assert!(
        app.console.iter().any(|line| {
            line.contains("warning: failed to reconcile") && line.contains(&path_text)
        }),
        "staging-scan warning missing from Console: {:?}",
        app.console
    );
    app.log_status();
    assert_logged_status(&app, "Saved ", "scene warnings");
    assert!(lc_io::load_native(&path).is_ok());
    true
}

#[test]
fn open_save_and_autosave_warnings_reach_editor_surfaces() {
    exercise_scene_warning_surfaces();
    exercise_autosave_warning_surface();
    #[cfg(unix)]
    assert!(
        exercise_directory_flush_surface(),
        "the directory-flush surface check could not induce a permission failure"
    );
}

#[test]
#[ignore = "child process helper for stderr capture"]
fn warning_stderr_child() {
    exercise_scene_warning_surfaces();
    exercise_autosave_warning_surface();
    #[cfg(unix)]
    {
        if exercise_directory_flush_surface() {
            println!("DIRECTORY_FLUSH_EXERCISED=1");
        }
    }
}

#[test]
fn warnings_still_reach_stderr() {
    let executable = std::env::current_exe().expect("the test executable exists");
    let output = Command::new(executable)
        .args([
            "--ignored",
            "--exact",
            "app::warning_surface_tests::warning_stderr_child",
            "--nocapture",
        ])
        .output()
        .expect("run the warning stderr child");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "stderr child failed: {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    let flush_ran = stdout.contains("DIRECTORY_FLUSH_EXERCISED=1");
    #[cfg(unix)]
    assert!(
        flush_ran,
        "the directory-flush child did not exercise the flush warning\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let expected = if flush_ran { 4 } else { 3 };
    let recovery_warnings = stderr.matches("warning: failed to reconcile").count();
    assert!(
        recovery_warnings >= expected,
        "expected at least {expected} recovery warnings on stderr, got {recovery_warnings}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("failed to remove stale scene staging file"),
        "staging-cleanup warning text missing from stderr\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    if flush_ran {
        assert!(
            stderr.contains("could not flush its directory entry"),
            "directory-flush warning missing from stderr\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }
}
