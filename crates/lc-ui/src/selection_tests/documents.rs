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
