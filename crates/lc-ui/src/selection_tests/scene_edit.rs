//! Selection, pivot transforms, batch flags, grouping, reparenting and deletion.

use super::*;

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
