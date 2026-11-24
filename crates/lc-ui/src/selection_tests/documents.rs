//! Import and playback frame-range coverage.

use super::*;

#[test]
fn import_maps_frame_range_to_playback_seconds() {
    let (mut app, root) = test_app("import-frame-range");
    let mut doc = Document::empty();
    doc.settings.fps = 24.0;
    doc.settings.frame_start = 12.0;
    doc.settings.frame_end = 96.0;
    doc.append_node(NodeKind::Null, "Imported", None);
    let path = root.path.join("take.lcscene");
    std::fs::write(&path, lc_scene::to_json(&doc).unwrap()).unwrap();

    app.import_file(path);

    // Documents store frames; the playback clock runs in seconds.
    assert!((app.playback.fps - 24.0).abs() < 1e-6);
    assert!(
        (app.playback.start - 0.5).abs() < 1e-6,
        "start {}",
        app.playback.start
    );
    assert!(
        (app.playback.end - 4.0).abs() < 1e-6,
        "end {}",
        app.playback.end
    );
    assert!((app.playback.time - 0.5).abs() < 1e-6);
}

/// A minimal glTF with a camera `aspectRatio`, which is the one piece of
/// authored data the importer deliberately rewrites. It gives the pre-import
/// dialog a real report to show without depending on skin or animation support.
fn write_gltf_with_camera_aspect(root: &std::path::Path) -> PathBuf {
    let mut positions = Vec::new();
    for point in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for value in point {
            positions.extend_from_slice(&value.to_le_bytes());
        }
    }
    let bin = root.join("tri.bin");
    std::fs::write(&bin, &positions).unwrap();
    let json = r#"{
        "asset": {"version": "2.0"},
        "buffers": [{"byteLength": 36, "uri": "tri.bin"}],
        "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 36}],
        "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}],
        "meshes": [{"name": "Tri", "primitives": [{"attributes": {"POSITION": 0}}]}],
        "cameras": [{"type": "perspective", "perspective": {"yfov": 0.7, "znear": 0.1, "aspectRatio": 1.5}}],
        "nodes": [{"name": "Tri", "mesh": 0}, {"name": "Shot", "camera": 0}],
        "scenes": [{"nodes": [0, 1]}],
        "scene": 0
    }"#;
    let path = root.join("aspect.gltf");
    std::fs::write(&path, json).unwrap();
    path
}

#[test]
fn gltf_open_stages_skipped_data_until_confirmed() {
    let (mut app, root) = test_app("pre-import-open");
    let path = write_gltf_with_camera_aspect(&root.path);
    let before = app.doc.signature();
    let depth = app.history.depth();

    assert!(
        !app.open_file(path.clone()),
        "an open with skipped data waits for confirmation"
    );
    assert!(app.pending_import.is_some(), "the import is staged");
    assert_eq!(app.doc.signature(), before, "the scene is untouched");
    assert_eq!(app.history.depth(), depth, "no undo entry before consent");
    assert!(app.status.contains("import report"));

    app.cancel_pending_import();
    assert!(app.pending_import.is_none());
    assert_eq!(app.doc.signature(), before);
    assert!(app.status.contains("left unchanged"));

    assert!(!app.open_file(path));
    app.apply_pending_import();
    assert!(app.pending_import.is_none());
    assert_ne!(
        app.doc.signature(),
        before,
        "the skipped animation is not part of the imported scene"
    );
}

#[test]
fn gltf_import_stages_a_report_and_one_undo_restores_the_scene() {
    let (mut app, root) = test_app("pre-import-report");
    let path = write_gltf_with_camera_aspect(&root.path);
    let before = app.doc.signature();
    let depth = app.history.depth();

    app.import_file(path);
    assert!(app.pending_import.is_some());
    assert_eq!(app.doc.signature(), before, "import is staged, not applied");
    assert_eq!(app.history.depth(), depth);

    app.apply_pending_import();
    assert_eq!(
        app.history.depth(),
        depth + 1,
        "one undo step for the import"
    );
    assert!(app.status.starts_with("Imported"), "{}", app.status);
    app.history.undo(&mut app.doc);
    assert_eq!(app.doc.signature(), before, "undo restores the prior scene");
}

#[test]
fn playback_range_clamps_finite_and_loop_is_view_state() {
    let (mut app, _root) = test_app("playback-range");
    app.playback.start = 1.0;
    app.playback.end = 5.0;
    app.playback.fps = 24.0;
    app.playback.time = 3.0;
    let signature = app.doc.signature();
    let depth = app.history.depth();

    assert!(app.set_playback_start(-2.0));
    assert_eq!(app.playback.start, 0.0);
    assert!(app.set_playback_end(2.0));
    assert_eq!(app.playback.end, 2.0);
    assert!(app.set_playback_start(9.0));
    assert_eq!(app.playback.start, 2.0, "start clamps to the end");
    assert!(!app.set_playback_start(f32::NAN));
    assert_eq!(app.playback.start, 2.0, "a non-finite start is refused");
    assert!(!app.set_playback_end(f32::INFINITY));
    assert_eq!(app.playback.end, 2.0, "a non-finite end is refused");

    app.playback.time = 99.0;
    assert!(app.set_playback_start(1.0));
    assert!((app.playback.time - 2.0).abs() < 1e-6);
    assert!(app.set_playback_end(0.0));
    assert_eq!(app.playback.end, 1.0, "end clamps to the start");
    assert!(!app.set_playback_end(0.0));

    assert!(app.set_playback_fps(30.0));
    assert_eq!(app.playback.fps, 30.0);
    assert!(!app.set_playback_fps(f32::NAN));

    assert!(app.set_playback_looping(false));
    assert!(!app.playback.looping);
    assert!(!app.set_playback_looping(false));
    assert!(app.set_playback_looping(true));
    assert!(app.playback.looping);

    assert_eq!(app.history.depth(), depth, "playback is view state");
    assert_eq!(
        app.doc.signature(),
        signature,
        "playback must not dirty the scene"
    );
}
