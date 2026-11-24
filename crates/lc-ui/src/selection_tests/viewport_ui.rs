//! Input-driven viewport, object browser, menu and toolbar coverage.

use super::*;

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
