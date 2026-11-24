//! Editable mesh, component, UV and topology tool coverage.

use super::*;

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
fn edge_crease_toggle_is_one_undo_and_marks_the_selected_edges() {
    let (mut harness, _root) = InputHarness::new(
        "input-crease-toggle",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Edges;
    let edges = selected_mesh(&harness.app, cube).edges();
    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, vec![0, 1]);
    harness.app.mesh_selected = vec![0, 1];
    let depth = harness.app.history.depth();

    harness.app.perform_mesh_action(MeshAction::ToggleCrease);
    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, cube);
    assert!(mesh.is_crease(edges[0].a, edges[0].b));
    assert!(mesh.is_crease(edges[1].a, edges[1].b));
    assert_eq!(mesh.creases.len(), 2);

    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    assert!(selected_mesh(&harness.app, cube).creases.is_empty());

    harness.app.mesh_selected = vec![0, 1];
    harness.app.mesh_components.insert(cube, vec![0, 1]);
    harness.app.perform_mesh_action(MeshAction::ToggleCrease);
    assert_eq!(selected_mesh(&harness.app, cube).creases.len(), 2);
    harness.app.perform_mesh_action(MeshAction::ToggleCrease);
    assert!(
        !selected_mesh(&harness.app, cube).is_crease(edges[0].a, edges[0].b),
        "toggling the same edge twice must remove the crease"
    );
    assert_eq!(harness.app.history.depth(), depth + 2);
}

#[test]
fn edge_seam_toggle_is_one_undo_and_marks_the_selected_edges() {
    let (mut harness, _root) = InputHarness::new(
        "input-seam-toggle",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Edges;
    let edges = selected_mesh(&harness.app, cube).edges();
    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, vec![0, 1]);
    harness.app.mesh_selected = vec![0, 1];
    let depth = harness.app.history.depth();

    harness.app.perform_mesh_action(MeshAction::ToggleSeam);
    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, cube);
    assert!(mesh.is_seam(edges[0].a, edges[0].b));
    assert!(mesh.is_seam(edges[1].a, edges[1].b));
    assert_eq!(mesh.seams.len(), 2);
    assert!(
        mesh.creases.is_empty(),
        "marking seams must not mark creases"
    );

    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    assert!(selected_mesh(&harness.app, cube).seams.is_empty());

    harness.app.mesh_selected = vec![0, 1];
    harness.app.mesh_components.insert(cube, vec![0, 1]);
    harness.app.perform_mesh_action(MeshAction::ToggleSeam);
    assert_eq!(selected_mesh(&harness.app, cube).seams.len(), 2);
    harness.app.perform_mesh_action(MeshAction::ToggleSeam);
    assert!(
        selected_mesh(&harness.app, cube).seams.is_empty(),
        "toggling the same edges twice must remove the seams"
    );
    assert_eq!(harness.app.history.depth(), depth + 2);
}

#[test]
fn toggle_pinned_uv_is_one_undo_and_needs_a_selected_uv_point() {
    let (mut harness, _root) =
        InputHarness::new("input-uv-pin", Surface::Shell, Vec2::new(1400.0, 1000.0));
    let cube = named(&harness.app, "Cube");
    let mut base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    lc_geom::box_projection(&mut base, 1.0);
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);
    harness.app.selected_uv = None;
    assert!(
        !harness.app.uv_pin_ready(),
        "the pin command needs a selected UV point"
    );
    let depth = harness.app.history.depth();
    harness.app.perform_mesh_action(MeshAction::TogglePinnedUv);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(harness.app.status.contains("Select a UV point"));

    harness.app.selected_uv = Some(2);
    assert!(harness.app.uv_pin_ready());
    harness.app.perform_mesh_action(MeshAction::TogglePinnedUv);
    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, cube);
    assert!(mesh.is_uv_pinned(2));
    assert_eq!(mesh.pinned_uvs, vec![2]);

    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    assert!(selected_mesh(&harness.app, cube).pinned_uvs.is_empty());

    harness.app.selected_uv = Some(2);
    harness.app.perform_mesh_action(MeshAction::TogglePinnedUv);
    assert!(selected_mesh(&harness.app, cube).is_uv_pinned(2));
    harness.app.perform_mesh_action(MeshAction::TogglePinnedUv);
    assert!(
        selected_mesh(&harness.app, cube).pinned_uvs.is_empty(),
        "toggling the same UV point twice unpins it"
    );
    assert_eq!(harness.app.history.depth(), depth + 2);
}

#[test]
fn fill_hole_is_one_undo_and_gated_on_a_hole_border() {
    let (mut harness, _root) =
        InputHarness::new("input-fill-hole", Surface::Shell, Vec2::new(1400.0, 1000.0));
    let cube = named(&harness.app, "Cube");
    let mut base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    let removed = base.polygons.remove(0);
    let border: Vec<u32> = removed.corners.iter().map(|c| c.point).collect();
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Points;

    // A point away from the hole cannot select a border, so the tool refuses.
    let off_border = (0u32..8)
        .find(|p| !border.contains(p))
        .expect("a box has points off the removed face");
    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, vec![off_border]);
    harness.app.mesh_selected = vec![off_border];
    assert!(!harness.app.mesh_fill_hole_ready());
    assert_eq!(harness.app.selected_hole_count(), 0);
    let depth = harness.app.history.depth();
    harness.app.perform_mesh_action(MeshAction::FillHole);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(
        harness.app.status.contains("hole border"),
        "got {:?}",
        harness.app.status
    );

    // One border point is enough, as with the documented click-driven tool.
    harness.app.mesh_components.insert(cube, vec![border[0]]);
    harness.app.mesh_selected = vec![border[0]];
    assert!(harness.app.mesh_fill_hole_ready());
    assert_eq!(harness.app.selected_hole_count(), 1);
    harness.app.perform_mesh_action(MeshAction::FillHole);
    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, cube);
    assert_eq!(mesh.polygons.len(), 6);
    assert!(mesh.boundary_loops().is_empty(), "the hole is closed");

    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    let mesh = selected_mesh(&harness.app, cube);
    assert_eq!(mesh.polygons.len(), 5);
    assert_eq!(mesh.boundary_loops().len(), 1);
}

#[test]
fn split_is_polygon_mode_only_and_one_undo_per_copy() {
    let (mut harness, _root) =
        InputHarness::new("input-split", Surface::Shell, Vec2::new(1400.0, 1000.0));
    let cube = named(&harness.app, "Cube");
    let mut base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    lc_geom::box_projection(&mut base, 1.0);
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;

    // Source 39: the tool is "only available in polygon mode".
    harness.app.mesh_select_mode = MeshSelectMode::Points;
    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, vec![0, 1, 2, 3]);
    harness.app.mesh_selected = vec![0, 1, 2, 3];
    assert!(!harness.app.mesh_split_ready());
    harness.open_menu("menu:Tools");
    assert!(!harness.menu_item("Tools:Split").1);
    let depth = harness.app.history.depth();
    harness.app.perform_mesh_action(MeshAction::Split);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(
        harness.app.status.contains("polygon mode"),
        "got {:?}",
        harness.app.status
    );

    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.mesh_components.insert(cube, vec![0, 1]);
    harness.app.mesh_selected = vec![0, 1];
    assert!(harness.app.mesh_split_ready());
    assert_eq!(harness.app.selected_split_polygon_count(), 2);
    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    assert!(
        harness.menu_item("Tools:Split").1,
        "polygon mode enables the menu command"
    );
    harness.open_menu("menu:Tools"); // close the menu again

    // Source 39 copies "all tags and selections" to the new object.
    {
        let node = harness.app.doc.scene.get_mut(cube).unwrap();
        node.transform = lc_core::Transform::from_translation(Vec3::new(4.0, 0.5, 0.0));
        node.tags.push(lc_scene::Tag::RenderOverride {
            visible: true,
            casts_shadow: false,
        });
    }
    let nodes = harness.app.doc.scene.depth_first().len();
    harness.app.perform_mesh_action(MeshAction::Split);
    assert_eq!(
        harness.app.history.depth(),
        depth + 1,
        "one undo step adds the copy"
    );
    assert_eq!(harness.app.doc.scene.depth_first().len(), nodes + 1);
    assert!(
        harness.app.mesh_selected.is_empty(),
        "copied vertices get new indices"
    );

    let new_id = harness
        .app
        .selection
        .primary()
        .expect("the copy is selected");
    assert_ne!(new_id, cube);
    let copied = harness.app.doc.scene.get(new_id).unwrap();
    assert_eq!(copied.name, "Cube Split");
    assert_eq!(copied.transform.translation, Vec3::new(4.0, 0.5, 0.0));
    assert_eq!(copied.tags.len(), 1);
    let NodeKind::Mesh { geometry } = &copied.kind else {
        panic!("expected a mesh node");
    };
    let copied_mesh = geometry.mesh().expect("the copy is an editable mesh");
    assert_eq!(copied_mesh.polygons.len(), 2);
    assert!(copied_mesh.validate().is_ok());
    assert_eq!(
        selected_mesh(&harness.app, cube).polygons.len(),
        6,
        "the source keeps every polygon"
    );

    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.scene.depth_first().len(), nodes);
    assert!(harness.app.doc.scene.get(new_id).is_none());
    assert_eq!(selected_mesh(&harness.app, cube).polygons.len(), 6);
}

#[test]
fn triangulate_is_polygon_mode_gated_and_one_undo_per_action() {
    let (mut harness, _root) = InputHarness::new(
        "input-triangulate",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let mut base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    lc_geom::box_projection(&mut base, 1.0);
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;

    // Point mode is refused: no undo entry, and the command names the reason.
    harness.app.mesh_select_mode = MeshSelectMode::Points;
    harness.app.mesh_components.insert(cube, vec![0, 1, 2, 3]);
    harness.app.mesh_selected = vec![0, 1, 2, 3];
    assert!(!harness.app.mesh_triangulate_ready());
    let depth = harness.app.history.depth();
    harness.app.perform_mesh_action(MeshAction::Triangulate);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(
        harness.app.status.contains("polygon mode"),
        "got {:?}",
        harness.app.status
    );

    // Polygon mode without a selection is refused too: there is nothing to split.
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.mesh_components.insert(cube, Vec::new());
    harness.app.mesh_selected.clear();
    assert!(!harness.app.mesh_triangulate_ready());
    harness.app.perform_mesh_action(MeshAction::Triangulate);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(
        harness.app.status.contains("more than three corners"),
        "got {:?}",
        harness.app.status
    );

    // One selected quad: enabled, split into two triangles, one undo step.
    harness.app.mesh_components.insert(cube, vec![0]);
    harness.app.mesh_selected = vec![0];
    assert!(harness.app.mesh_triangulate_ready());
    harness.open_menu("menu:Tools");
    assert!(
        harness.menu_item("Tools:Triangulate").1,
        "polygon mode enables the menu command"
    );
    harness.open_menu("menu:Tools"); // close the menu again
    let positions = selected_mesh(&harness.app, cube).positions.clone();
    harness.app.perform_mesh_action(MeshAction::Triangulate);
    assert_eq!(
        harness.app.history.depth(),
        depth + 1,
        "one undo step for the whole operation"
    );
    {
        let mesh = selected_mesh(&harness.app, cube);
        assert_eq!(mesh.polygons.len(), 7, "one quad became two triangles");
        assert_eq!(
            mesh.positions, positions,
            "triangulating must not move points"
        );
        assert!(mesh.validate().is_ok());
        assert_eq!(
            mesh.polygons
                .iter()
                .filter(|p| p.corners.len() == 3)
                .count(),
            2
        );
    }

    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(selected_mesh(&harness.app, cube).polygons.len(), 6);
}

#[test]
fn shell_is_polygon_mode_gated_and_one_undo_per_action() {
    let (mut harness, _root) =
        InputHarness::new("input-shell", Surface::Shell, Vec2::new(1400.0, 1000.0));
    let cube = named(&harness.app, "Cube");
    let mut base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    lc_geom::box_projection(&mut base, 1.0);
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;
    harness.app.mesh_shell_thickness = 0.1;
    harness.app.mesh_shell_inside = true;
    harness.app.mesh_shell_angle = 30.0;
    harness.app.mesh_shell_quantize = 0.0;

    // Source 41: the tool is "only available in polygon mode".
    harness.app.mesh_select_mode = MeshSelectMode::Points;
    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, vec![0, 1, 2, 3]);
    harness.app.mesh_selected = vec![0, 1, 2, 3];
    assert!(!harness.app.mesh_shell_ready());
    harness.open_menu("menu:Tools");
    assert!(!harness.menu_item("Tools:Shell").1);
    let depth = harness.app.history.depth();
    harness.app.perform_mesh_action(MeshAction::Shell);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(
        harness.app.status.contains("polygon mode"),
        "got {:?}",
        harness.app.status
    );

    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    let faces: Vec<u32> = (0..6).collect();
    harness.app.mesh_components.insert(cube, faces.clone());
    harness.app.mesh_selected = faces;
    assert_eq!(harness.app.selected_shell_face_count(), 6);
    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    assert!(
        harness.menu_item("Tools:Shell").1,
        "polygon mode enables the menu command"
    );
    harness.open_menu("menu:Tools"); // close the menu again

    // A zero distance cannot build a wall.
    harness.app.mesh_shell_thickness = 0.0;
    assert!(!harness.app.mesh_shell_ready());
    harness.app.perform_mesh_action(MeshAction::Shell);
    assert!(
        harness.app.status.contains("thickness"),
        "got {:?}",
        harness.app.status
    );
    assert_eq!(harness.app.history.depth(), depth);

    // A signed distance would quietly flip the documented Inside side.
    harness.app.mesh_shell_thickness = -0.1;
    assert!(!harness.app.mesh_shell_ready());
    harness.app.perform_mesh_action(MeshAction::Shell);
    assert!(
        harness.app.status.contains("positive"),
        "got {:?}",
        harness.app.status
    );
    assert_eq!(harness.app.history.depth(), depth);

    harness.app.mesh_shell_thickness = 0.1;
    assert!(harness.app.mesh_shell_ready());
    harness.app.perform_mesh_action(MeshAction::Shell);
    assert_eq!(
        harness.app.history.depth(),
        depth + 1,
        "one undo step shells the selection"
    );
    assert!(harness.app.mesh_selected.is_empty(), "indices change");
    let mesh = selected_mesh(&harness.app, cube);
    assert_eq!(mesh.polygons.len(), 12, "outer and inner shell");
    assert_eq!(mesh.positions.len(), 16);
    assert!(
        mesh.boundary_loops().is_empty(),
        "hollowing a closed solid adds no border"
    );

    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    let mesh = selected_mesh(&harness.app, cube);
    assert_eq!(mesh.polygons.len(), 6);
    assert_eq!(mesh.positions.len(), 8);
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
fn flip_normal_reverses_the_selected_faces_and_one_undo_restores_them() {
    let (mut harness, _root) = InputHarness::new(
        "input-flip-normal",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let mut base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    lc_geom::box_projection(&mut base, 1.0);
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;

    // Point mode is refused: no undo entry, and the status names the reason.
    harness.app.mesh_select_mode = MeshSelectMode::Points;
    harness.app.mesh_components.insert(cube, vec![0, 1, 2, 3]);
    harness.app.mesh_selected = vec![0, 1, 2, 3];
    assert!(!harness.app.mesh_flip_normal_ready());
    let depth = harness.app.history.depth();
    harness.app.perform_mesh_action(MeshAction::FlipNormal);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(
        harness.app.status.contains("polygon mode"),
        "got {:?}",
        harness.app.status
    );

    // Polygon mode with no selected polygon is a no-op without an undo entry.
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.mesh_components.insert(cube, Vec::new());
    harness.app.mesh_selected.clear();
    assert!(!harness.app.mesh_flip_normal_ready());
    let before = harness.app.doc.signature();
    harness.app.perform_mesh_action(MeshAction::FlipNormal);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(
        harness.app.doc.signature(),
        before,
        "an empty selection must not touch the mesh"
    );
    assert!(
        harness.app.status.contains("Select at least one polygon"),
        "got {:?}",
        harness.app.status
    );

    // One selected face: the menu command is enabled and flips exactly that face.
    harness.app.mesh_components.insert(cube, vec![2]);
    harness.app.mesh_selected = vec![2];
    assert!(harness.app.mesh_flip_normal_ready());
    harness.open_menu("menu:Tools");
    assert!(
        harness.menu_item("Tools:Flip Normal").1,
        "polygon mode enables the menu command"
    );
    harness.open_menu("menu:Tools"); // close the menu again
    let (original, neighbour, normal) = {
        let mesh = selected_mesh(&harness.app, cube);
        (
            mesh.polygons[2].corners.clone(),
            mesh.polygons[1].corners.clone(),
            mesh.face_normal(2),
        )
    };
    harness.app.perform_mesh_action(MeshAction::FlipNormal);
    assert_eq!(
        harness.app.history.depth(),
        depth + 1,
        "one undo step for the whole flip"
    );
    let flipped: Vec<_> = original.iter().rev().cloned().collect();
    {
        let mesh = selected_mesh(&harness.app, cube);
        assert_eq!(
            mesh.polygons[2].corners, flipped,
            "the selected face reversed exactly"
        );
        assert_eq!(
            mesh.polygons[1].corners, neighbour,
            "faces outside the selection keep their winding"
        );
        assert!(
            (mesh.face_normal(2) + normal).length() < 1e-5,
            "the flipped face normal points the other way"
        );
        assert!(mesh.validate().is_ok());
    }
    assert_eq!(
        harness.app.mesh_selected,
        vec![2],
        "the face selection survives so the flip can be toggled back"
    );

    // The flipped winding is document state and survives a native round-trip.
    let text = lc_scene::to_json(&harness.app.doc).unwrap();
    let loaded = lc_scene::from_json(&text).unwrap();
    let NodeKind::Mesh { geometry } = &loaded.scene.get(cube).unwrap().kind else {
        panic!("the cube must load as a mesh");
    };
    let loaded_mesh = geometry.mesh().expect("an editable mesh");
    assert_eq!(
        loaded_mesh.polygons[2].corners, flipped,
        "saved winding must match the flipped mesh"
    );

    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(
        selected_mesh(&harness.app, cube).polygons[2].corners,
        original,
        "undo restores the exact prior winding"
    );
}

#[test]
fn collapse_welds_selected_points_and_one_undo_restores_the_mesh() {
    let (mut harness, _root) =
        InputHarness::new("input-collapse", Surface::Shell, Vec2::new(1400.0, 1000.0));
    let cube = named(&harness.app, "Cube");
    let mut base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    lc_geom::box_projection(&mut base, 1.0);
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;

    // Polygon mode is refused: no undo entry, and the status names the mode.
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.mesh_components.insert(cube, vec![0]);
    harness.app.mesh_selected = vec![0];
    assert!(!harness.app.mesh_collapse_ready());
    let depth = harness.app.history.depth();
    harness.app.perform_mesh_action(MeshAction::Collapse);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(
        harness.app.status.to_lowercase().contains("point"),
        "got {:?}",
        harness.app.status
    );

    // Point mode with no selected point is a no-op that pushes no undo step.
    harness.app.mesh_select_mode = MeshSelectMode::Points;
    harness.app.mesh_components.insert(cube, Vec::new());
    harness.app.mesh_selected.clear();
    assert!(!harness.app.mesh_collapse_ready());
    let before = harness.app.doc.signature();
    harness.app.perform_mesh_action(MeshAction::Collapse);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(
        harness.app.doc.signature(),
        before,
        "an empty selection must not touch the mesh"
    );
    assert!(
        harness.app.status.to_lowercase().contains("collapse"),
        "got {:?}",
        harness.app.status
    );

    // Three corners of one quad: enabled, one vertex at their centroid, one undo.
    let (before_positions, before_polygons, corners) = {
        let mesh = selected_mesh(&harness.app, cube);
        (
            mesh.positions.clone(),
            mesh.polygons.clone(),
            mesh.polygons[0].corners.clone(),
        )
    };
    let collapsed = vec![corners[0].point, corners[1].point, corners[2].point];
    let centroid = collapsed
        .iter()
        .map(|p| before_positions[*p as usize])
        .sum::<Vec3>()
        / collapsed.len() as f32;
    harness.app.mesh_components.insert(cube, collapsed.clone());
    harness.app.mesh_selected = collapsed.clone();
    assert!(harness.app.mesh_collapse_ready());
    harness.open_menu("menu:Tools");
    assert!(
        harness.menu_item("Tools:Collapse").1,
        "point mode enables the menu command"
    );
    harness.open_menu("menu:Tools"); // close the menu again
    harness.app.perform_mesh_action(MeshAction::Collapse);
    assert_eq!(
        harness.app.history.depth(),
        depth + 1,
        "one undo step for the whole collapse"
    );
    let survivor = {
        let mesh = selected_mesh(&harness.app, cube);
        assert!(mesh.validate().is_ok());
        assert_eq!(mesh.positions.len(), before_positions.len() - 2);
        assert_eq!(
            mesh.polygons.len(),
            before_polygons.len() - 1,
            "the face that collapsed to two corners is gone"
        );
        let survivor = mesh
            .positions
            .iter()
            .position(|p| (*p - centroid).length() < 1e-6)
            .expect("the merged vertex sits at the centroid") as u32;
        for poly in &mesh.polygons {
            let mut points: Vec<u32> = poly.corners.iter().map(|c| c.point).collect();
            points.sort_unstable();
            points.dedup();
            assert_eq!(points.len(), poly.corners.len(), "no face repeats a vertex");
            for corner in &poly.corners {
                assert!((corner.point as usize) < mesh.positions.len());
                assert!((corner.uv as usize) < mesh.uvs.len());
            }
        }
        survivor
    };
    assert_eq!(
        harness.app.mesh_selected,
        vec![survivor],
        "the selection follows the merged vertex"
    );
    assert_eq!(
        harness.app.mesh_components.get(&cube),
        Some(&vec![survivor])
    );
    let collapsed_mesh = selected_mesh(&harness.app, cube).clone();

    // The rebuilt topology is document state and survives a native round-trip.
    let text = lc_scene::to_json(&harness.app.doc).unwrap();
    let loaded = lc_scene::from_json(&text).unwrap();
    let NodeKind::Mesh { geometry } = &loaded.scene.get(cube).unwrap().kind else {
        panic!("the cube must load as a mesh");
    };
    let loaded_mesh = geometry.mesh().expect("an editable mesh");
    assert_eq!(loaded_mesh.positions, collapsed_mesh.positions);
    assert_eq!(loaded_mesh.polygons, collapsed_mesh.polygons);

    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    let mesh = selected_mesh(&harness.app, cube);
    assert_eq!(
        mesh.positions, before_positions,
        "undo restores the exact prior points"
    );
    assert_eq!(
        mesh.polygons, before_polygons,
        "undo restores the exact prior faces"
    );
}

#[test]
fn disconnect_detaches_selected_faces_and_one_undo_restores_the_mesh() {
    let (mut harness, _root) = InputHarness::new(
        "input-disconnect",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let mut base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    lc_geom::box_projection(&mut base, 1.0);
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;

    // Point mode is refused: no undo entry, and the status names the mode.
    harness.app.mesh_select_mode = MeshSelectMode::Points;
    harness.app.mesh_components.insert(cube, vec![0, 1, 2, 3]);
    harness.app.mesh_selected = vec![0, 1, 2, 3];
    assert!(!harness.app.mesh_disconnect_ready());
    let depth = harness.app.history.depth();
    harness.app.perform_mesh_action(MeshAction::Disconnect);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(
        harness.app.status.to_lowercase().contains("polygon"),
        "got {:?}",
        harness.app.status
    );

    // Polygon mode with nothing selected is a no-op that pushes no undo step.
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.mesh_components.insert(cube, Vec::new());
    harness.app.mesh_selected.clear();
    assert!(!harness.app.mesh_disconnect_ready());
    let before = harness.app.doc.signature();
    harness.app.perform_mesh_action(MeshAction::Disconnect);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(
        harness.app.doc.signature(),
        before,
        "an empty selection must not touch the mesh"
    );
    assert!(
        harness.app.status.to_lowercase().contains("disconnect"),
        "got {:?}",
        harness.app.status
    );

    // A whole-mesh selection is already one island, so it refuses without undo.
    let face_count = selected_mesh(&harness.app, cube).polygons.len() as u32;
    let all: Vec<u32> = (0..face_count).collect();
    harness.app.mesh_components.insert(cube, all.clone());
    harness.app.mesh_selected = all;
    assert!(!harness.app.mesh_disconnect_ready());
    let before = harness.app.doc.signature();
    harness.app.perform_mesh_action(MeshAction::Disconnect);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before);

    // One selected face: the menu command is enabled and detaches it in place.
    harness.app.mesh_components.insert(cube, vec![0]);
    harness.app.mesh_selected = vec![0];
    assert!(harness.app.mesh_disconnect_ready());
    harness.open_menu("menu:Tools");
    assert!(
        harness.menu_item("Tools:Disconnect").1,
        "polygon mode enables the menu command"
    );
    harness.open_menu("menu:Tools"); // close the menu again
    let (before_positions, before_polygons, face) = {
        let mesh = selected_mesh(&harness.app, cube);
        (
            mesh.positions.clone(),
            mesh.polygons.clone(),
            mesh.polygons[0].clone(),
        )
    };
    harness.app.perform_mesh_action(MeshAction::Disconnect);
    assert_eq!(
        harness.app.history.depth(),
        depth + 1,
        "one undo step for the whole disconnect"
    );
    let detached = {
        let mesh = selected_mesh(&harness.app, cube);
        assert!(mesh.validate().is_ok());
        assert_eq!(
            mesh.positions.len(),
            before_positions.len() + face.corners.len(),
            "one copy per point on the cut"
        );
        assert_eq!(mesh.polygons.len(), before_polygons.len());
        assert_eq!(
            mesh.polygons[1..],
            before_polygons[1..],
            "unselected faces are untouched"
        );
        for (corner, original) in mesh.polygons[0].corners.iter().zip(face.corners.iter()) {
            assert_ne!(corner.point, original.point, "cut points are duplicated");
            assert_eq!(
                mesh.positions[corner.point as usize], before_positions[original.point as usize],
                "duplication must not move a point"
            );
        }
        let mut used = vec![false; mesh.positions.len()];
        for poly in &mesh.polygons {
            for corner in &poly.corners {
                assert!((corner.point as usize) < mesh.positions.len());
                assert!((corner.uv as usize) < mesh.uvs.len());
                used[corner.point as usize] = true;
            }
        }
        assert!(
            used.iter().all(|used| *used),
            "no unused point is left behind"
        );
        mesh.clone()
    };
    assert_eq!(
        harness.app.mesh_selected,
        vec![0],
        "the face selection stays valid and names the detached island"
    );

    // The detached topology is document state and survives a native round-trip.
    let text = lc_scene::to_json(&harness.app.doc).unwrap();
    let loaded = lc_scene::from_json(&text).unwrap();
    let NodeKind::Mesh { geometry } = &loaded.scene.get(cube).unwrap().kind else {
        panic!("the cube must load as a mesh");
    };
    let loaded_mesh = geometry.mesh().expect("an editable mesh");
    assert_eq!(loaded_mesh.positions, detached.positions);
    assert_eq!(loaded_mesh.polygons, detached.polygons);

    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    let mesh = selected_mesh(&harness.app, cube);
    assert_eq!(
        mesh.positions, before_positions,
        "undo restores the exact prior points"
    );
    assert_eq!(
        mesh.polygons, before_polygons,
        "undo restores the exact prior faces"
    );
}

#[test]
fn cover_closes_the_selected_hole_and_one_undo_restores_the_mesh() {
    let (mut harness, _root) =
        InputHarness::new("input-cover", Surface::Shell, Vec2::new(1400.0, 1000.0));
    let cube = named(&harness.app, "Cube");
    let mut base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    lc_geom::box_projection(&mut base, 1.0);
    base.polygons.remove(0); // one open four-sided hole
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;

    // Point mode is refused: no undo entry, and the status names the mode.
    harness.app.mesh_select_mode = MeshSelectMode::Points;
    harness.app.mesh_components.insert(cube, vec![0, 1, 2, 3]);
    harness.app.mesh_selected = vec![0, 1, 2, 3];
    assert!(!harness.app.mesh_cover_ready());
    let depth = harness.app.history.depth();
    harness.app.perform_mesh_action(MeshAction::Cover);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(
        harness.app.status.to_lowercase().contains("polygon"),
        "got {:?}",
        harness.app.status
    );

    // Polygon mode with nothing selected is a no-op that pushes no undo step.
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.mesh_components.insert(cube, Vec::new());
    harness.app.mesh_selected.clear();
    assert!(!harness.app.mesh_cover_ready());
    let before = harness.app.doc.signature();
    harness.app.perform_mesh_action(MeshAction::Cover);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(
        harness.app.doc.signature(),
        before,
        "an empty selection must not touch the mesh"
    );
    assert!(
        harness.app.status.to_lowercase().contains("cover"),
        "got {:?}",
        harness.app.status
    );

    // One face does not ring the hole, so it cannot cover it.
    harness.app.mesh_components.insert(cube, vec![0]);
    harness.app.mesh_selected = vec![0];
    assert!(!harness.app.mesh_cover_ready());
    let before = harness.app.doc.signature();
    harness.app.perform_mesh_action(MeshAction::Cover);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before);

    // All five faces ring the hole: enabled, one new face, one undo step.
    let faces: Vec<u32> = (0..5).collect();
    harness.app.mesh_components.insert(cube, faces.clone());
    harness.app.mesh_selected = faces.clone();
    assert!(harness.app.mesh_cover_ready());
    harness.open_menu("menu:Tools");
    assert!(
        harness.menu_item("Tools:Cover").1,
        "polygon mode enables the menu command"
    );
    harness.open_menu("menu:Tools"); // close the menu again
    let (before_positions, before_polygons, before_uvs) = {
        let mesh = selected_mesh(&harness.app, cube);
        (
            mesh.positions.clone(),
            mesh.polygons.clone(),
            mesh.uvs.clone(),
        )
    };
    harness.app.perform_mesh_action(MeshAction::Cover);
    assert_eq!(
        harness.app.history.depth(),
        depth + 1,
        "one undo step for the whole cover"
    );
    let created;
    {
        let mesh = selected_mesh(&harness.app, cube);
        assert!(mesh.validate().is_ok());
        assert_eq!(mesh.polygons.len(), before_polygons.len() + 1);
        assert_eq!(
            mesh.positions, before_positions,
            "cover must not move a single point"
        );
        assert_eq!(
            mesh.polygons[..before_polygons.len()].to_vec(),
            before_polygons,
            "existing faces keep their corners, UV slots and slots"
        );
        assert_eq!(
            &mesh.uvs[..before_uvs.len()],
            &before_uvs[..],
            "existing UV slots are reused"
        );
        assert_eq!(mesh.uvs.len(), before_uvs.len() + 4);
        assert!(mesh.boundary_loops().is_empty(), "the hole is closed");
        assert!(
            mesh.edges().iter().all(|edge| edge.faces.len() == 2),
            "the filled mesh is watertight"
        );
        assert!(
            (mesh.signed_volume() - 1.0).abs() < 1e-4,
            "the solid is restored: {}",
            mesh.signed_volume()
        );
        for poly in &mesh.polygons {
            for corner in &poly.corners {
                assert!((corner.point as usize) < mesh.positions.len());
                assert!((corner.uv as usize) < mesh.uvs.len());
            }
        }
        created = before_polygons.len() as u32;
        assert_eq!(mesh.polygons[created as usize].corners.len(), 4);
    }
    assert_eq!(
        harness.app.mesh_selected,
        vec![created],
        "the selection names the new face"
    );
    assert_eq!(harness.app.mesh_components.get(&cube), Some(&vec![created]));
    let covered = selected_mesh(&harness.app, cube).clone();

    // The filled topology is document state and survives a native round-trip.
    let text = lc_scene::to_json(&harness.app.doc).unwrap();
    let loaded = lc_scene::from_json(&text).unwrap();
    let NodeKind::Mesh { geometry } = &loaded.scene.get(cube).unwrap().kind else {
        panic!("the cube must load as a mesh");
    };
    let loaded_mesh = geometry.mesh().expect("an editable mesh");
    assert_eq!(loaded_mesh.positions, covered.positions);
    assert_eq!(loaded_mesh.polygons, covered.polygons);
    assert_eq!(loaded_mesh.uvs, covered.uvs);

    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    let mesh = selected_mesh(&harness.app, cube);
    assert_eq!(
        mesh.positions, before_positions,
        "undo restores the exact prior points"
    );
    assert_eq!(
        mesh.polygons, before_polygons,
        "undo restores the exact prior faces"
    );
}

#[test]
fn create_polygon_appends_the_selected_face_and_one_undo_restores_the_mesh() {
    let (mut harness, _root) = InputHarness::new(
        "input-create-polygon",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let mut base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    lc_geom::box_projection(&mut base, 1.0);
    base.polygons.remove(0); // one open four-point hole
    let hole = base.boundary_loops().remove(0);
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;

    // Polygon mode is refused: no undo entry, and the status names the mode.
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.mesh_components.insert(cube, vec![0]);
    harness.app.mesh_selected = vec![0];
    assert!(!harness.app.mesh_create_polygon_ready());
    let depth = harness.app.history.depth();
    harness.app.perform_mesh_action(MeshAction::CreatePolygon);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(
        harness.app.status.to_lowercase().contains("point mode"),
        "got {:?}",
        harness.app.status
    );

    // Point mode with nothing selected is a no-op that pushes no undo step.
    harness.app.mesh_select_mode = MeshSelectMode::Points;
    harness.app.mesh_components.insert(cube, Vec::new());
    harness.app.mesh_selected.clear();
    assert!(!harness.app.mesh_create_polygon_ready());
    let before = harness.app.doc.signature();
    harness.app.perform_mesh_action(MeshAction::CreatePolygon);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(
        harness.app.doc.signature(),
        before,
        "an empty selection must not touch the mesh"
    );
    assert!(
        harness.app.status.to_lowercase().contains("create"),
        "got {:?}",
        harness.app.status
    );

    // Three of the four border points cannot close the hole.
    let partial: Vec<u32> = hole[..3].to_vec();
    harness.app.mesh_components.insert(cube, partial.clone());
    harness.app.mesh_selected = partial;
    assert!(!harness.app.mesh_create_polygon_ready());
    let before = harness.app.doc.signature();
    harness.app.perform_mesh_action(MeshAction::CreatePolygon);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before);

    // The whole border: enabled, one new face over the existing points.
    let mut selected = hole.clone();
    selected.reverse(); // the mesh adjacency decides the order, not the click order
    harness.app.mesh_components.insert(cube, selected.clone());
    harness.app.mesh_selected = selected;
    assert!(harness.app.mesh_create_polygon_ready());
    harness.open_menu("menu:Tools");
    assert!(
        harness.menu_item("Tools:Create Polygon").1,
        "point mode enables the menu command"
    );
    harness.open_menu("menu:Tools"); // close the menu again
    let (before_positions, before_polygons) = {
        let mesh = selected_mesh(&harness.app, cube);
        (mesh.positions.clone(), mesh.polygons.clone())
    };
    harness.app.perform_mesh_action(MeshAction::CreatePolygon);
    assert_eq!(
        harness.app.history.depth(),
        depth + 1,
        "one undo step for the whole action"
    );
    let created = before_polygons.len() as u32;
    {
        let mesh = selected_mesh(&harness.app, cube);
        assert!(mesh.validate().is_ok());
        assert_eq!(mesh.polygons.len(), before_polygons.len() + 1);
        assert_eq!(
            mesh.positions, before_positions,
            "no point is created or moved"
        );
        assert_eq!(
            mesh.polygons[..before_polygons.len()].to_vec(),
            before_polygons,
            "existing faces keep their corners, UV slots and slots"
        );
        let face = &mesh.polygons[created as usize];
        assert_eq!(face.corners.len(), hole.len());
        let mut face_points: Vec<u32> = face.corners.iter().map(|c| c.point).collect();
        face_points.sort_unstable();
        let mut expected = hole.clone();
        expected.sort_unstable();
        assert_eq!(face_points, expected, "the face reuses the border points");
        for corner in &mesh.polygons[created as usize].corners {
            assert!((corner.point as usize) < mesh.positions.len());
            assert!((corner.uv as usize) < mesh.uvs.len());
        }
        assert!(mesh.boundary_loops().is_empty(), "the hole is closed");
        assert!(
            (mesh.signed_volume() - 1.0).abs() < 1e-4,
            "the solid is restored: {}",
            mesh.signed_volume()
        );
    }
    assert_eq!(
        harness.app.mesh_selected,
        vec![created],
        "the selection names the new face"
    );
    assert_eq!(harness.app.mesh_components.get(&cube), Some(&vec![created]));
    let created_mesh = selected_mesh(&harness.app, cube).clone();

    // The new face is document state and survives a native round-trip.
    let text = lc_scene::to_json(&harness.app.doc).unwrap();
    let loaded = lc_scene::from_json(&text).unwrap();
    let NodeKind::Mesh { geometry } = &loaded.scene.get(cube).unwrap().kind else {
        panic!("the cube must load as a mesh");
    };
    let loaded_mesh = geometry.mesh().expect("an editable mesh");
    assert_eq!(loaded_mesh.positions, created_mesh.positions);
    assert_eq!(loaded_mesh.polygons, created_mesh.polygons);

    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    let mesh = selected_mesh(&harness.app, cube);
    assert_eq!(
        mesh.positions, before_positions,
        "undo restores the exact prior points"
    );
    assert_eq!(
        mesh.polygons, before_polygons,
        "undo restores the exact prior faces"
    );
}

#[test]
fn optimize_welds_nearby_selected_points_and_one_undo_restores_the_mesh() {
    let (mut harness, _root) =
        InputHarness::new("input-optimize", Surface::Shell, Vec2::new(1400.0, 1000.0));
    let cube = named(&harness.app, "Cube");
    // Two quads that meet along a seam of near-duplicate points.
    let mut mesh = lc_geom::Mesh::new("optimize strip");
    mesh.positions = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 1.0),
        Vec3::new(1.0004, 0.0, 0.0),
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::new(1.0004, 0.0, 1.0),
        Vec3::new(2.0, 0.0, 1.0),
    ];
    mesh.polygons.push(lc_geom::Polygon::new(vec![
        lc_geom::Corner::new(0, 0),
        lc_geom::Corner::new(2, 0),
        lc_geom::Corner::new(3, 0),
        lc_geom::Corner::new(1, 0),
    ]));
    mesh.polygons.push(lc_geom::Polygon::new(vec![
        lc_geom::Corner::new(4, 0),
        lc_geom::Corner::new(6, 0),
        lc_geom::Corner::new(7, 0),
        lc_geom::Corner::new(5, 0),
    ]));
    mesh.uvs = vec![glam::Vec2::ZERO; 8];
    let before_positions = mesh.positions.clone();
    let before_polygons = mesh.polygons.clone();
    set_editable_geometry(&mut harness.app, cube, mesh);
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;

    // Polygon mode is refused: no undo entry, and the status names the mode.
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.mesh_components.insert(cube, vec![0]);
    harness.app.mesh_selected = vec![0];
    assert!(!harness.app.mesh_optimize_ready());
    let depth = harness.app.history.depth();
    harness.app.perform_mesh_action(MeshAction::Optimize);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(
        harness.app.status.to_lowercase().contains("point mode"),
        "got {:?}",
        harness.app.status
    );

    // Point mode with nothing selected is a no-op with its own message.
    harness.app.mesh_select_mode = MeshSelectMode::Points;
    harness.app.mesh_components.insert(cube, Vec::new());
    harness.app.mesh_selected.clear();
    let before = harness.app.doc.signature();
    harness.app.perform_mesh_action(MeshAction::Optimize);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(
        harness.app.doc.signature(),
        before,
        "an empty selection must not touch the mesh"
    );
    assert!(
        harness.app.status.contains("at least two points"),
        "got {:?}",
        harness.app.status
    );

    // A zero tolerance is refused before any geometry work.
    harness.app.mesh_optimize_tolerance = 0.0;
    harness.app.mesh_components.insert(cube, vec![1, 4]);
    harness.app.mesh_selected = vec![1, 4];
    assert!(!harness.app.mesh_optimize_ready());
    harness.app.perform_mesh_action(MeshAction::Optimize);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(
        harness.app.status.contains("positive optimize tolerance"),
        "got {:?}",
        harness.app.status
    );

    // Two points further apart than the tolerance do not optimize.
    harness.app.mesh_optimize_tolerance = 0.001;
    harness.app.mesh_components.insert(cube, vec![0, 2]);
    harness.app.mesh_selected = vec![0, 2];
    assert!(!harness.app.mesh_optimize_ready());
    let before = harness.app.doc.signature();
    harness.app.perform_mesh_action(MeshAction::Optimize);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before);
    assert!(
        harness.app.status.contains("within the optimize tolerance"),
        "got {:?}",
        harness.app.status
    );

    // Both seam pairs: enabled, one welded strip, one undo step.
    let selected: Vec<u32> = vec![1, 3, 4, 6];
    harness.app.mesh_components.insert(cube, selected.clone());
    harness.app.mesh_selected = selected.clone();
    assert!(harness.app.mesh_optimize_ready());
    harness.open_menu("menu:Tools");
    assert!(
        harness.menu_item("Tools:Optimize").1,
        "point mode enables the menu command"
    );
    harness.open_menu("menu:Tools"); // close the menu again
    harness.app.perform_mesh_action(MeshAction::Optimize);
    assert_eq!(
        harness.app.history.depth(),
        depth + 1,
        "one undo step for the whole optimize"
    );
    {
        let mesh = selected_mesh(&harness.app, cube);
        assert!(mesh.validate().is_ok());
        assert_eq!(mesh.positions.len(), before_positions.len() - 2);
        assert_eq!(mesh.polygons.len(), before_polygons.len());
        assert_eq!(
            mesh.polygons[0], before_polygons[0],
            "a face away from the weld keeps its corner record"
        );
        assert_eq!(mesh.positions[5], before_positions[7]);
        let seam = mesh
            .edges()
            .into_iter()
            .find(|edge| edge.a == 1 && edge.b == 3)
            .expect("the welded seam is an edge");
        assert_eq!(seam.faces.len(), 2, "the weld joins both quads");
        for poly in &mesh.polygons {
            for corner in &poly.corners {
                assert!((corner.point as usize) < mesh.positions.len());
                assert!((corner.uv as usize) < mesh.uvs.len());
            }
        }
    }
    assert_eq!(
        harness.app.mesh_selected,
        vec![1, 3],
        "the selection follows the surviving points"
    );
    assert_eq!(harness.app.mesh_components.get(&cube), Some(&vec![1, 3]));
    let optimized = selected_mesh(&harness.app, cube).clone();

    // The welded topology is document state and survives a native round-trip.
    let text = lc_scene::to_json(&harness.app.doc).unwrap();
    let loaded = lc_scene::from_json(&text).unwrap();
    let NodeKind::Mesh { geometry } = &loaded.scene.get(cube).unwrap().kind else {
        panic!("the cube must load as a mesh");
    };
    let loaded_mesh = geometry.mesh().expect("an editable mesh");
    assert_eq!(loaded_mesh.positions, optimized.positions);
    assert_eq!(loaded_mesh.polygons, optimized.polygons);

    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    let mesh = selected_mesh(&harness.app, cube);
    assert_eq!(
        mesh.positions, before_positions,
        "undo restores the exact prior points"
    );
    assert_eq!(
        mesh.polygons, before_polygons,
        "undo restores the exact prior faces"
    );
}

#[test]
fn point_slide_moves_the_selection_and_one_undo_restores_the_mesh() {
    let (mut harness, _root) = InputHarness::new(
        "input-point-slide",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    // Two quads sharing an edge; point 0 has exactly two neighbours.
    let mut mesh = lc_geom::Mesh::new("slide strip");
    mesh.positions = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 1.0),
        Vec3::new(2.0, 0.0, 1.0),
    ];
    mesh.polygons.push(lc_geom::Polygon::new(vec![
        lc_geom::Corner::new(0, 0),
        lc_geom::Corner::new(3, 0),
        lc_geom::Corner::new(4, 0),
        lc_geom::Corner::new(1, 0),
    ]));
    mesh.polygons.push(lc_geom::Polygon::new(vec![
        lc_geom::Corner::new(1, 0),
        lc_geom::Corner::new(4, 0),
        lc_geom::Corner::new(5, 0),
        lc_geom::Corner::new(2, 0),
    ]));
    mesh.uvs = vec![glam::Vec2::ZERO; 6];
    set_editable_geometry(&mut harness.app, cube, mesh);
    // The editor's stored mesh is the byte-for-byte baseline.
    let before_positions = selected_mesh(&harness.app, cube).positions.clone();
    let before_polygons = selected_mesh(&harness.app, cube).polygons.clone();
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;

    // Polygon mode is refused before any position changes.
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.mesh_components.insert(cube, vec![0]);
    harness.app.mesh_selected = vec![0];
    assert!(!harness.app.mesh_point_slide_ready());
    let depth = harness.app.history.depth();
    harness.app.perform_mesh_action(MeshAction::PointSlide);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(
        harness.app.status.contains("point mode"),
        "got {:?}",
        harness.app.status
    );

    // Point mode with nothing selected is a no-op with its own message.
    harness.app.mesh_select_mode = MeshSelectMode::Points;
    harness.app.mesh_components.insert(cube, Vec::new());
    harness.app.mesh_selected.clear();
    let before = harness.app.doc.signature();
    harness.app.perform_mesh_action(MeshAction::PointSlide);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before);
    assert!(
        harness.app.status.contains("at least one point"),
        "got {:?}",
        harness.app.status
    );

    // A fraction outside 0..=1 is refused before any geometry work.
    harness.app.mesh_slide_fraction = 1.5;
    harness.app.mesh_components.insert(cube, vec![0]);
    harness.app.mesh_selected = vec![0];
    assert!(!harness.app.mesh_point_slide_ready());
    harness.app.perform_mesh_action(MeshAction::PointSlide);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before);
    assert!(
        harness.app.status.contains("slide fraction"),
        "got {:?}",
        harness.app.status
    );

    // A zero fraction cannot move anything and is its own no-op message.
    harness.app.mesh_slide_fraction = 0.0;
    harness.app.mesh_components.insert(cube, vec![0]);
    harness.app.mesh_selected = vec![0];
    assert!(!harness.app.mesh_point_slide_ready());
    harness.app.perform_mesh_action(MeshAction::PointSlide);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before);
    assert!(
        harness.app.status.contains("above 0"),
        "got {:?}",
        harness.app.status
    );

    // A selection that names no live point cannot slide either.
    harness.app.mesh_slide_fraction = 0.5;
    harness.app.mesh_components.insert(cube, vec![99]);
    harness.app.mesh_selected = vec![99];
    assert!(!harness.app.mesh_point_slide_ready());
    harness.app.perform_mesh_action(MeshAction::PointSlide);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before);
    assert!(
        harness.app.status.contains("neighbours"),
        "got {:?}",
        harness.app.status
    );

    // A valid point slides half way toward its neighbours in one undo step.
    harness.app.mesh_components.insert(cube, vec![0]);
    harness.app.mesh_selected = vec![0];
    assert!(harness.app.mesh_point_slide_ready());
    harness.open_menu("menu:Tools");
    assert!(
        harness.menu_item("Tools:Point Slide").1,
        "point mode enables the menu command"
    );
    harness.open_menu("menu:Tools"); // close the menu again
    harness.app.perform_mesh_action(MeshAction::PointSlide);
    assert_eq!(
        harness.app.history.depth(),
        depth + 1,
        "one undo step for the whole slide"
    );
    {
        let mesh = selected_mesh(&harness.app, cube);
        assert!(mesh.validate().is_ok());
        let expected = Vec3::new(0.25, 0.0, 0.25);
        assert!(
            (mesh.positions[0] - expected).length() < 1e-6,
            "got {:?}",
            mesh.positions[0]
        );
        assert_eq!(mesh.positions.len(), before_positions.len());
        for (position, before) in mesh.positions.iter().zip(&before_positions).skip(1) {
            assert_eq!(position, before);
        }
        assert_eq!(
            mesh.polygons, before_polygons,
            "sliding points keeps every face corner record"
        );
    }
    assert_eq!(
        harness.app.mesh_selected,
        vec![0],
        "the selection still names the slid point"
    );
    let slid = selected_mesh(&harness.app, cube).clone();

    // The slid positions are document state and survive a native round-trip.
    let text = lc_scene::to_json(&harness.app.doc).unwrap();
    let loaded = lc_scene::from_json(&text).unwrap();
    let NodeKind::Mesh { geometry } = &loaded.scene.get(cube).unwrap().kind else {
        panic!("the cube must load as a mesh");
    };
    let loaded_mesh = geometry.mesh().expect("an editable mesh");
    assert_eq!(loaded_mesh.positions, slid.positions);
    assert_eq!(loaded_mesh.polygons, slid.polygons);

    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    let mesh = selected_mesh(&harness.app, cube);
    assert_eq!(
        mesh.positions, before_positions,
        "undo restores the exact prior points"
    );
    assert_eq!(
        mesh.polygons, before_polygons,
        "undo restores the exact prior faces"
    );
}
