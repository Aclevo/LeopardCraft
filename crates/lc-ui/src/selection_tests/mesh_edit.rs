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
    assert!(harness.app.status.contains("Select one or more UV points"));

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
fn scalpel_cuts_between_two_edges_of_one_polygon_and_undo_restores_the_mesh() {
    let (mut harness, _root) =
        InputHarness::new("input-scalpel", Surface::Shell, Vec2::new(1400.0, 1000.0));
    let cube = named(&harness.app, "Cube");
    let mut base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    lc_geom::box_projection(&mut base, 1.0);
    set_editable_geometry(&mut harness.app, cube, base.clone());
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;
    let depth = harness.app.history.depth();

    // Polygon mode is refused: no undo entry, and the status names the modes.
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, vec![0]);
    harness.app.mesh_selected = vec![0];
    assert!(!harness.app.mesh_scalpel_ready());
    let before = harness.app.doc.signature();
    harness.app.perform_mesh_action(MeshAction::Scalpel);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before);
    assert!(
        harness.app.status.contains("point or edge"),
        "got {:?}",
        harness.app.status
    );

    // Two edges of one polygon are enabled and cut that polygon in two.
    harness.app.mesh_select_mode = MeshSelectMode::Edges;
    let edges = base.edges();
    let polygon = base.polygons[0].clone();
    let polygon_edge = |edge: &lc_geom::MeshEdge| {
        let count = polygon.corners.len();
        (0..count).any(|index| {
            let a = polygon.corners[index].point;
            let b = polygon.corners[(index + 1) % count].point;
            (a == edge.a && b == edge.b) || (a == edge.b && b == edge.a)
        })
    };
    let face_edges: Vec<u32> = (0..edges.len() as u32)
        .filter(|index| polygon_edge(&edges[*index as usize]))
        .collect();
    assert!(
        face_edges.len() >= 2,
        "the first face has at least two edges"
    );
    let picks = vec![face_edges[0], face_edges[1]];
    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, picks.clone());
    harness.app.mesh_selected = picks;
    harness.open_menu("menu:Tools");
    assert!(
        harness.menu_item("Tools:Scalpel").1,
        "Scalpel must be enabled for two edges of one polygon"
    );
    harness.click_menu_item("Tools:Scalpel");
    assert_eq!(harness.app.history.depth(), depth + 1);
    let cut = selected_mesh(&harness.app, cube);
    assert_eq!(cut.polygons.len(), base.polygons.len() + 1);
    assert!(cut.validate().is_ok());
    assert_eq!(
        cut.positions.len(),
        base.positions.len() + 2,
        "each edge pick adds its own point"
    );
    assert!(
        cut.polygons
            .iter()
            .all(|polygon| polygon.corners.len() >= 3),
        "the cut must not leave a degenerate face"
    );
    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(selected_mesh(&harness.app, cube), base);

    // Two points that share no polygon are refused without an undo entry.
    harness.app.mesh_select_mode = MeshSelectMode::Points;
    // Body-diagonal corners of a box share no face.
    let corners = [0u32, 6];
    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, corners.to_vec());
    harness.app.mesh_selected = corners.to_vec();
    assert!(!harness.app.mesh_scalpel_ready());
    let before = harness.app.doc.signature();
    harness.app.perform_mesh_action(MeshAction::Scalpel);
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(harness.app.doc.signature(), before);
    assert!(
        harness.app.status.contains("one polygon"),
        "got {:?}",
        harness.app.status
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

#[test]
fn uv_editing_menu_and_seam_authoring_are_undoable() {
    let (mut harness, _root) =
        InputHarness::new("input-uv-seams", Surface::Shell, Vec2::new(1400.0, 1000.0));
    let cube = named(&harness.app, "Cube");
    let mut base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    lc_geom::box_projection(&mut base, 1.0);
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);

    harness.open_menu("menu:Tools");
    harness.open_submenu("Tools:UV Editing");
    for item in [
        "Tools:UV Editing:Toggle Seam",
        "Tools:UV Editing:Select All Seams",
        "Tools:UV Editing:Pack Charts",
        "Tools:UV Editing:Weld UVs",
        "Tools:UV Editing:Add UV Set",
    ] {
        assert!(harness.menu_item(item).1, "{item} is enabled for a UV mesh");
    }

    let (first, second) = {
        let mesh = selected_mesh(&harness.app, cube);
        let poly = &mesh.polygons[0];
        (poly.corners[0].uv, poly.corners[1].uv)
    };
    harness.app.uv_editor.set = 0;
    harness.app.uv_editor.selection = [first, second].into_iter().collect();
    harness.app.selected_uv = Some(first as usize);
    let depth = harness.app.history.depth();
    harness.open_menu("menu:Tools");
    harness.open_submenu("Tools:UV Editing");
    harness.click_menu_item("Tools:UV Editing:Toggle Seam");
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(selected_mesh(&harness.app, cube).seams.len(), 1);
    harness.undo_via_menu();
    assert!(selected_mesh(&harness.app, cube).seams.is_empty());

    // Selecting all seams needs no document edit; clearing them does.
    let mut mesh = selected_mesh(&harness.app, cube);
    assert!(mesh.set_seam(poly_edge(&mesh, 0, 0), poly_edge(&mesh, 0, 1), true));
    set_editable_geometry(&mut harness.app, cube, mesh);
    let depth = harness.app.history.depth();
    harness.app.uv_editor.selection.clear();
    harness.app.selected_uv = None;
    harness.app.select_all_uv_seams();
    assert_eq!(
        harness.app.history.depth(),
        depth,
        "selection is view state"
    );
    assert!(!harness.app.uv_editor.selection.is_empty());
    harness.app.select_uv_seam_loop();
    assert!(!harness.app.uv_editor.selection.is_empty());
    harness.app.clear_uv_seams();
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert!(selected_mesh(&harness.app, cube).seams.is_empty());
    harness.undo_via_menu();
    assert_eq!(selected_mesh(&harness.app, cube).seams.len(), 1);
}

fn poly_edge(mesh: &lc_geom::Mesh, face: usize, corner: usize) -> u32 {
    mesh.polygons[face].corners[corner].point
}

#[test]
fn uv_multi_pin_and_set_edits_are_one_undo_each_and_target_the_active_set() {
    let (mut harness, _root) = InputHarness::new(
        "input-uv-multiset",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let mut base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    lc_geom::box_projection(&mut base, 1.0);
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);

    harness.app.uv_editor.set = 0;
    harness.app.uv_editor.selection = [0u32, 1].into_iter().collect();
    harness.app.selected_uv = Some(0);
    assert!(harness.app.uv_pin_ready());
    let depth = harness.app.history.depth();
    harness.app.perform_mesh_action(MeshAction::TogglePinnedUv);
    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, cube);
    assert_eq!(
        mesh.pinned_uvs,
        vec![0, 1],
        "every selected point is pinned"
    );
    harness.app.perform_mesh_action(MeshAction::TogglePinnedUv);
    assert!(selected_mesh(&harness.app, cube).pinned_uvs.is_empty());
    assert_eq!(harness.app.history.depth(), depth + 2);

    let uv0_before = selected_mesh(&harness.app, cube).uv_value(0, 0).unwrap();
    harness.app.add_uv_set();
    assert_eq!(selected_mesh(&harness.app, cube).uv_set_count(), 2);
    assert_eq!(harness.app.uv_editor.set, 1, "the new set becomes active");
    assert_eq!(
        selected_mesh(&harness.app, cube).uv_value(0, 0),
        Some(uv0_before)
    );
    let depth = harness.app.history.depth();
    harness.app.uv_editor.selection = [0u32, 1].into_iter().collect();
    harness.app.selected_uv = Some(0);
    harness.app.uv_editor.numeric = V2::new(0.9, 0.1);
    harness.app.run_uv_slot_op(UvSlotOp::Numeric);
    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, cube);
    assert_eq!(mesh.uv_value(1, 0), Some(V2::new(0.9, 0.1)));
    assert_eq!(mesh.uv_value(1, 1), Some(V2::new(0.9, 0.1)));
    assert_eq!(mesh.uv_value(0, 0), Some(uv0_before), "UV0 is untouched");
    harness.undo_via_menu();
    assert_ne!(
        selected_mesh(&harness.app, cube).uv_value(1, 0),
        Some(V2::new(0.9, 0.1))
    );

    // Split needs a slot shared by several corners; swap in a two-face mesh
    // whose corners all use slot 0.
    let mut shared = lc_geom::Mesh::from_polygons(
        "shared",
        vec![
            glam::Vec3::new(0.0, 0.0, 0.0),
            glam::Vec3::new(0.0, 0.0, 1.0),
            glam::Vec3::new(1.0, 0.0, 1.0),
            glam::Vec3::new(1.0, 0.0, 0.0),
            glam::Vec3::new(2.0, 0.0, 1.0),
            glam::Vec3::new(2.0, 0.0, 0.0),
        ],
        vec![vec![0, 1, 2, 3], vec![3, 2, 4, 5]],
    );
    shared.uvs = vec![V2::ZERO];
    set_editable_geometry(&mut harness.app, cube, shared);
    harness.app.uv_editor.set = 0;
    harness.app.uv_editor.selection = [0u32].into_iter().collect();
    harness.app.selected_uv = Some(0);
    let before_slots = selected_mesh(&harness.app, cube).uvs.len();
    let depth = harness.app.history.depth();
    harness.app.run_uv_slot_op(UvSlotOp::Split);
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert!(selected_mesh(&harness.app, cube).uvs.len() > before_slots);
    harness.undo_via_menu();
    assert_eq!(selected_mesh(&harness.app, cube).uvs.len(), before_slots);
}

#[test]
fn uv_chart_transforms_and_packing_are_undoable_and_skip_pinned_charts() {
    let (mut harness, _root) =
        InputHarness::new("input-uv-charts", Surface::Shell, Vec2::new(1400.0, 1000.0));
    let cube = named(&harness.app, "Cube");
    set_editable_geometry(&mut harness.app, cube, lc_geom::box_mesh(1.0, 1.0, 1.0));
    select_object_rows(&mut harness, &[cube]);
    harness.app.apply_uv_op(UvOp::Unwrap);
    let charts = lc_geom::uv_charts(&selected_mesh(&harness.app, cube));
    assert!(charts.len() > 1, "an unwrapped cube has several charts");
    harness.app.uv_editor.set = 0;
    harness.app.uv_editor.selection = charts[0].slots.iter().copied().collect();
    harness.app.selected_uv = charts[0].slots.first().map(|slot| *slot as usize);
    harness.app.select_uv_island();
    let island = harness.app.uv_editor.selection.len();
    assert_eq!(
        island,
        charts[0].slots.len(),
        "island selects the whole chart"
    );

    let before_uvs = selected_mesh(&harness.app, cube).uvs.clone();
    harness.app.uv_editor.chart_move = V2::new(0.05, 0.02);
    harness.app.uv_editor.chart_rotate = 90.0;
    harness.app.uv_editor.chart_scale = 0.5;
    let depth = harness.app.history.depth();
    harness.app.transform_selected_uv_charts();
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_ne!(selected_mesh(&harness.app, cube).uvs, before_uvs);
    harness.undo_via_menu();
    assert_eq!(selected_mesh(&harness.app, cube).uvs, before_uvs);

    // A pinned chart is left exactly where it was, and the report says so.
    let pinned_slot = charts[1].slots[0];
    harness.app.uv_editor.selection = charts[1].slots.iter().copied().collect();
    harness.app.selected_uv = Some(pinned_slot as usize);
    harness.app.perform_mesh_action(MeshAction::TogglePinnedUv);
    let pinned_before = selected_mesh(&harness.app, cube).uv_value(0, pinned_slot);
    harness.app.uv_editor.selection.clear();
    harness.app.selected_uv = None;
    harness.app.uv_editor.pack_padding = 0.02;
    harness.app.uv_editor.pack_rotate = true;
    harness.app.uv_editor.pack_align = lc_geom::UvPackAlign::Center;
    let depth = harness.app.history.depth();
    harness.app.pack_selected_uv_charts();
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert!(harness.app.status.contains("left in place"));
    assert_eq!(
        selected_mesh(&harness.app, cube).uv_value(0, pinned_slot),
        pinned_before,
        "packing never moves a pinned chart"
    );
    harness.undo_via_menu();

    let signature = harness.app.doc.signature();
    let depth = harness.app.history.depth();
    harness.app.uv_editor.checker = false;
    harness.app.uv_editor.distortion = true;
    assert_eq!(harness.app.doc.signature(), signature);
    assert_eq!(
        harness.app.history.depth(),
        depth,
        "overlays are view state"
    );
    let stats = lc_geom::uv_distortion_stats(&selected_mesh(&harness.app, cube), 0).unwrap();
    assert!(stats.mean.is_finite() && stats.max >= 1.0);
}

#[test]
fn uv_operations_refuse_without_a_usable_selection() {
    let (mut harness, _root) = InputHarness::new(
        "input-uv-refusals",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let mut base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    lc_geom::box_projection(&mut base, 1.0);
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);
    harness.app.uv_editor.selection.clear();
    harness.app.selected_uv = None;
    let depth = harness.app.history.depth();
    harness.app.run_uv_slot_op(UvSlotOp::Weld);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(harness.app.status.contains("Select one or more UV points"));
    harness.app.toggle_selected_uv_seam();
    assert_eq!(harness.app.history.depth(), depth);
    assert!(harness.app.status.contains("adjacent"));

    harness.app.add_uv_set();
    assert_eq!(harness.app.uv_editor.set, 1);
    harness.app.uv_editor.selection = [0u32].into_iter().collect();
    let depth = harness.app.history.depth();
    harness.app.apply_uv_op(UvOp::Unwrap);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(harness.app.status.contains("switch the set to UV0"));
}

#[test]
fn crease_weight_action_stores_fractional_strength_and_undoes_once() {
    let (mut harness, _root) = InputHarness::new(
        "input-crease-weight",
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

    harness.app.mesh_crease_weight = 0.5;
    harness.app.perform_mesh_action(MeshAction::SetCreaseWeight);
    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, cube);
    assert!(mesh.creases.is_empty());
    assert_eq!(mesh.crease_weights.len(), 2);
    assert!((mesh.crease_weight(edges[0].a, edges[0].b) - 0.5).abs() < 1e-6);
    assert!((mesh.crease_weight(edges[1].a, edges[1].b) - 0.5).abs() < 1e-6);

    harness.undo_via_menu();
    assert!(selected_mesh(&harness.app, cube).crease_weights.is_empty());

    harness.app.mesh_selected = vec![0, 1];
    harness.app.mesh_components.insert(cube, vec![0, 1]);
    harness.app.mesh_crease_weight = 1.0;
    harness.app.perform_mesh_action(MeshAction::SetCreaseWeight);
    let mesh = selected_mesh(&harness.app, cube);
    assert_eq!(mesh.creases.len(), 2);
    assert!(mesh.crease_weights.is_empty());
}

#[test]
fn subdivide_selection_is_one_undo_and_keeps_the_unselected_region_watertight() {
    let (mut harness, _root) = InputHarness::new(
        "input-subdivide-selection",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let base = lc_geom::plane_mesh(3.0, 3.0, 3, 3);
    let before = base.clone();
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, vec![0]);
    harness.app.mesh_selected = vec![0];
    harness.app.mesh_subdivide_levels = 1;
    harness.app.mesh_subdivide_scheme = lc_geom::SubdivisionKind::CatmullClark;
    let depth = harness.app.history.depth();

    harness.app.perform_mesh_action(MeshAction::Subdivide);
    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, cube);
    assert_eq!(mesh.polygons.len(), 12);
    assert!(mesh.validate().is_ok());
    assert!(mesh.polygons.iter().any(|poly| poly.corners.len() == 5));
    let selected_points: std::collections::HashSet<u32> = before.polygons[0]
        .corners
        .iter()
        .map(|corner| corner.point)
        .collect();
    let unselected_points: std::collections::HashSet<u32> = before
        .polygons
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != 0)
        .flat_map(|(_, poly)| poly.corners.iter().map(|corner| corner.point))
        .filter(|point| !selected_points.contains(point))
        .collect();
    for point in unselected_points {
        assert_eq!(
            mesh.positions[point as usize],
            before.positions[point as usize]
        );
    }
    for edge in mesh.edges() {
        if edge.faces.len() == 1 {
            let a = mesh.positions[edge.a as usize];
            let b = mesh.positions[edge.b as usize];
            assert!(
                a.x.abs() > 1.5 - 1e-4
                    || a.z.abs() > 1.5 - 1e-4
                    || b.x.abs() > 1.5 - 1e-4
                    || b.z.abs() > 1.5 - 1e-4,
                "interior crack at {a:?}-{b:?}"
            );
        }
    }

    harness.undo_via_menu();
    assert_eq!(selected_mesh(&harness.app, cube), before);
}

#[test]
fn creased_edges_are_drawn_outside_edge_mode() {
    let (mut harness, _root) = InputHarness::new(
        "input-crease-overlay",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let mut base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    assert!(base.set_crease(0, 1, true));
    set_editable_geometry(&mut harness.app, cube, base.clone());
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Points;
    let evaluated = harness.app.evaluate();
    let world = evaluated.node(cube).expect("cube is evaluated").world;
    let a = world.transform_point3(base.positions[0]);
    let b = world.transform_point3(base.positions[1]);
    let frame = harness.app.build_frame(&evaluated);
    let drawn = frame.lines.as_chunks::<2>().0.iter().any(|pair| {
        let p0 = glam::Vec3::from_array(pair[0].position);
        let p1 = glam::Vec3::from_array(pair[1].position);
        let blue = pair[0].color[2] > 0.8 && pair[0].color[0] < 0.5;
        blue && ((p0.distance(a) < 1e-4 && p1.distance(b) < 1e-4)
            || (p0.distance(b) < 1e-4 && p1.distance(a) < 1e-4))
    });
    assert!(drawn, "the crease was not drawn while in point mode");
}

#[test]
fn clone_polygon_selection_is_one_undo_and_empty_clones_everything() {
    let (mut harness, _root) = InputHarness::new(
        "input-clone-polygons",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    set_editable_geometry(&mut harness.app, cube, base.clone());
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.mesh_components.clear();
    harness.app.mesh_components.insert(cube, vec![0]);
    harness.app.mesh_selected = vec![0];
    harness.app.mesh_clone = lc_geom::CloneOptions {
        copies: 2,
        direction: glam::Vec3::X * 2.0,
        ..Default::default()
    };
    let depth = harness.app.history.depth();

    harness.app.perform_mesh_action(MeshAction::Clone);
    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, cube);
    assert_eq!(mesh.polygons.len(), base.polygons.len() + 2);
    assert_eq!(mesh.positions.len(), base.positions.len() + 8);
    assert!(mesh.validate().is_ok());

    harness.undo_via_menu();
    assert_eq!(selected_mesh(&harness.app, cube), base);

    harness.app.mesh_selected.clear();
    harness.app.mesh_components.clear();
    harness.app.mesh_clone.copies = 1;
    harness.app.perform_mesh_action(MeshAction::Clone);
    assert_eq!(
        selected_mesh(&harness.app, cube).polygons.len(),
        base.polygons.len() * 2
    );
}

#[test]
fn fill_hole_click_tool_fills_the_clicked_border_and_is_one_undo() {
    let (mut harness, _root) = InputHarness::new(
        "input-fill-hole-click",
        Surface::Shell,
        Vec2::new(720.0, 540.0),
    );
    let cube = named(&harness.app, "Cube");
    let mut base = lc_geom::box_mesh(1.0, 1.0, 1.0);
    let removed = base.polygons.remove(0);
    let border: Vec<u32> = removed.corners.iter().map(|c| c.point).collect();
    set_editable_geometry(&mut harness.app, cube, base);
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.fill_hole_tool = true;
    let depth = harness.app.history.depth();
    harness.frame(Surface::Viewport, Vec::new(), egui::Modifiers::default());

    let evaluated = harness.app.evaluate();
    let node = evaluated.node(cube).expect("cube is evaluated");
    let mesh = selected_mesh(&harness.app, cube);
    let corner = mesh.positions[border[0] as usize];
    let rect = harness.app.last_viewport_rect;
    let (pixel_w, pixel_h) = harness.viewport_dims();
    let screen = harness
        .app
        .project(node.world.transform_point3(corner), rect, pixel_w, pixel_h)
        .expect("border point is on screen");
    harness.click(Surface::Viewport, screen, egui::Modifiers::default());

    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, cube);
    assert_eq!(mesh.polygons.len(), 6);
    assert!(mesh.boundary_loops().is_empty());
    assert!(
        harness.app.fill_hole_tool,
        "the tool stays active for another hole"
    );

    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(selected_mesh(&harness.app, cube).polygons.len(), 5);
    harness.app.redo();
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(selected_mesh(&harness.app, cube).polygons.len(), 6);
}

#[test]
fn scalpel_click_tool_chains_cuts_and_escape_resets_the_start() {
    let (mut harness, _root) = InputHarness::new(
        "input-scalpel-chain",
        Surface::Shell,
        Vec2::new(900.0, 700.0),
    );
    let mesh_id = named(&harness.app, "Cube");
    // A single hexagonal face keeps every click unambiguous: no back-face point
    // or edge shares a screen position, so the test drives real viewport clicks.
    let hexagon = lc_geom::Mesh::from_polygons(
        "hexagon",
        (0..6)
            .map(|i| {
                let angle = i as f32 * std::f32::consts::TAU / 6.0;
                glam::Vec3::new(angle.cos() * 1.5, 0.0, angle.sin() * 1.5)
            })
            .collect(),
        vec![vec![0, 1, 2, 3, 4, 5]],
    );
    let project = |harness: &InputHarness, point: glam::Vec3| -> egui::Pos2 {
        let evaluated = harness.app.evaluate();
        let node = evaluated.node(mesh_id).expect("the mesh is evaluated");
        let rect = harness.app.last_viewport_rect;
        let (w, h) = harness.viewport_dims();
        harness
            .app
            .project(node.world.transform_point3(point), rect, w, h)
            .expect("the point is on screen")
    };
    set_editable_geometry(&mut harness.app, mesh_id, hexagon.clone());
    select_object_rows(&mut harness, &[mesh_id]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.scalpel_tool = true;
    let depth = harness.app.history.depth();
    harness.frame(Surface::Viewport, Vec::new(), egui::Modifiers::default());

    // The first click only starts the cut.
    let start = project(&harness, hexagon.positions[0]);
    harness.click(Surface::Viewport, start, egui::Modifiers::default());
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(
        harness.app.scalpel_start,
        Some(lc_geom::ScalpelPick::Point(0))
    );

    // A second pick of the same polygon splits it and becomes the next start.
    let second = project(&harness, hexagon.positions[3]);
    harness.click(Surface::Viewport, second, egui::Modifiers::default());
    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, mesh_id);
    assert_eq!(mesh.polygons.len(), 2);
    assert_eq!(mesh.polygons[0].corners.len(), 4);
    assert_eq!(mesh.polygons[1].corners.len(), 4);
    assert!(mesh.validate().is_ok());
    assert_eq!(
        harness.app.scalpel_start,
        Some(lc_geom::ScalpelPick::Point(3)),
        "the cut's end point is the next cut's start"
    );
    assert!(
        harness.app.scalpel_tool,
        "a completed cut keeps the tool on"
    );

    // Chaining continues from the previous end point on the polygon holding it,
    // which leaves the hexagon as three faces.
    let third = project(&harness, hexagon.positions[5]);
    harness.click(Surface::Viewport, third, egui::Modifiers::default());
    assert_eq!(harness.app.history.depth(), depth + 2);
    let mesh = selected_mesh(&harness.app, mesh_id);
    assert_eq!(mesh.polygons.len(), 3);
    assert_eq!(mesh.polygons[1].corners.len(), 3);
    assert!(mesh.validate().is_ok());
    assert_eq!(
        harness.app.scalpel_start,
        Some(lc_geom::ScalpelPick::Point(5))
    );

    // A pick that shares no polygon with the chained start is refused and keeps
    // the start, so a cut never chains across a polygon boundary.
    let across = project(&harness, hexagon.positions[1]);
    harness.click(Surface::Viewport, across, egui::Modifiers::default());
    assert_eq!(harness.app.history.depth(), depth + 2);
    assert_eq!(selected_mesh(&harness.app, mesh_id).polygons.len(), 3);
    assert_eq!(
        harness.app.scalpel_start,
        Some(lc_geom::ScalpelPick::Point(5))
    );
    assert!(harness.app.status.contains("holds the cut start"));

    // ESC resets the pending cut instead of chaining further.
    harness.press_key(egui::Key::Escape, egui::Modifiers::default());
    assert_eq!(harness.app.scalpel_start, None);
    assert!(harness.app.scalpel_tool, "ESC resets the cut, not the tool");
    assert_eq!(harness.app.history.depth(), depth + 2);

    // Each completed cut is one undo step.
    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    harness.undo_via_menu();
    assert_eq!(selected_mesh(&harness.app, mesh_id).polygons.len(), 2);
}

#[test]
fn scalpel_click_tool_cuts_from_an_edge_to_a_point_and_chains_the_new_point() {
    let (mut harness, _root) = InputHarness::new(
        "input-scalpel-mixed-picks",
        Surface::Shell,
        Vec2::new(900.0, 700.0),
    );
    let mesh_id = named(&harness.app, "Cube");
    let quad = lc_geom::Mesh::from_polygons(
        "quad",
        vec![
            glam::Vec3::new(0.0, 0.0, 0.0),
            glam::Vec3::new(2.0, 0.0, 0.0),
            glam::Vec3::new(2.0, 0.0, 2.0),
            glam::Vec3::new(0.0, 0.0, 2.0),
        ],
        vec![vec![0, 1, 2, 3]],
    );
    let midpoint = (quad.positions[0] + quad.positions[1]) * 0.5;
    let project = |harness: &InputHarness, point: glam::Vec3| -> egui::Pos2 {
        let evaluated = harness.app.evaluate();
        let node = evaluated.node(mesh_id).expect("the mesh is evaluated");
        let rect = harness.app.last_viewport_rect;
        let (w, h) = harness.viewport_dims();
        harness
            .app
            .project(node.world.transform_point3(point), rect, w, h)
            .expect("the point is on screen")
    };
    set_editable_geometry(&mut harness.app, mesh_id, quad.clone());
    select_object_rows(&mut harness, &[mesh_id]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.scalpel_tool = true;
    let depth = harness.app.history.depth();
    harness.frame(Surface::Viewport, Vec::new(), egui::Modifiers::default());

    // A point click starts the cut and an edge click completes it: the
    // documented mixed point/edge pair inside one polygon.
    let corner = project(&harness, quad.positions[2]);
    harness.click(Surface::Viewport, corner, egui::Modifiers::default());
    assert_eq!(
        harness.app.scalpel_start,
        Some(lc_geom::ScalpelPick::Point(2))
    );
    let edge = project(&harness, midpoint);
    harness.click(Surface::Viewport, edge, egui::Modifiers::default());
    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, mesh_id);
    assert_eq!(mesh.polygons.len(), 2);
    assert_eq!(mesh.positions.len(), 5);
    assert!(
        mesh.positions[4].distance(midpoint) < 1e-3,
        "the edge pick inserts the point nearest the pick ray, at the midpoint"
    );
    assert!(mesh.validate().is_ok());
    assert_eq!(
        harness.app.scalpel_start,
        Some(lc_geom::ScalpelPick::Point(4)),
        "the inserted edge point becomes the next start"
    );

    harness.press_key(egui::Key::Escape, egui::Modifiers::default());
    assert_eq!(harness.app.scalpel_start, None);
}

#[test]
fn scalpel_pending_start_does_not_go_stale_across_undo_or_a_replaced_mesh() {
    let (mut harness, _root) = InputHarness::new(
        "input-scalpel-stale-start",
        Surface::Shell,
        Vec2::new(900.0, 700.0),
    );
    let mesh_id = named(&harness.app, "Cube");
    let hexagon = lc_geom::Mesh::from_polygons(
        "hexagon",
        (0..6)
            .map(|i| {
                let angle = i as f32 * std::f32::consts::TAU / 6.0;
                glam::Vec3::new(angle.cos() * 1.5, 0.0, angle.sin() * 1.5)
            })
            .collect(),
        vec![vec![0, 1, 2, 3, 4, 5]],
    );
    let project = |harness: &InputHarness, point: glam::Vec3| -> egui::Pos2 {
        let evaluated = harness.app.evaluate();
        let node = evaluated.node(mesh_id).expect("the mesh is evaluated");
        let rect = harness.app.last_viewport_rect;
        let (w, h) = harness.viewport_dims();
        harness
            .app
            .project(node.world.transform_point3(point), rect, w, h)
            .expect("the point is on screen")
    };
    set_editable_geometry(&mut harness.app, mesh_id, hexagon.clone());
    select_object_rows(&mut harness, &[mesh_id]);
    harness.app.mesh_edit = true;
    harness.app.mesh_select_mode = MeshSelectMode::Polygons;
    harness.app.scalpel_tool = true;
    let depth = harness.app.history.depth();
    harness.frame(Surface::Viewport, Vec::new(), egui::Modifiers::default());

    let start = project(&harness, hexagon.positions[0]);
    harness.click(Surface::Viewport, start, egui::Modifiers::default());
    let second = project(&harness, hexagon.positions[3]);
    harness.click(Surface::Viewport, second, egui::Modifiers::default());
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(selected_mesh(&harness.app, mesh_id).polygons.len(), 2);
    assert_eq!(
        harness.app.scalpel_start,
        Some(lc_geom::ScalpelPick::Point(3))
    );

    // Undo restores the pre-cut topology, so the chained start must go with it
    // instead of pointing at a point the restored mesh does not have.
    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    harness.undo_via_menu();
    assert_eq!(harness.app.history.depth(), depth);
    assert_eq!(selected_mesh(&harness.app, mesh_id).polygons.len(), 1);
    assert_eq!(harness.app.scalpel_start, None);
    assert!(
        harness.app.scalpel_tool,
        "undo does not switch the tool off"
    );

    // Redo re-applies the cut but leaves the cut UI reset.
    harness.app.redo();
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(selected_mesh(&harness.app, mesh_id).polygons.len(), 2);
    assert_eq!(harness.app.scalpel_start, None);

    // A start that a non-undo edit invalidated is dropped on the next click,
    // which then begins a fresh cut instead of refusing against the old start.
    harness.app.scalpel_start = Some(lc_geom::ScalpelPick::Point(99));
    harness.frame(Surface::Viewport, Vec::new(), egui::Modifiers::default());
    let start = project(&harness, hexagon.positions[0]);
    harness.click(Surface::Viewport, start, egui::Modifiers::default());
    assert_eq!(harness.app.history.depth(), depth + 1);
    assert_eq!(
        harness.app.scalpel_start,
        Some(lc_geom::ScalpelPick::Point(0))
    );
    assert!(harness.app.status.contains("previous start"));

    let chain = project(&harness, hexagon.positions[2]);
    harness.click(Surface::Viewport, chain, egui::Modifiers::default());
    assert_eq!(harness.app.history.depth(), depth + 2);
    let mesh = selected_mesh(&harness.app, mesh_id);
    assert_eq!(mesh.polygons.len(), 3);
    assert!(mesh.validate().is_ok());
    assert_eq!(
        harness.app.scalpel_start,
        Some(lc_geom::ScalpelPick::Point(2))
    );
}

#[test]
fn fill_hole_tool_refuses_a_hole_that_encloses_an_island() {
    let (mut harness, _root) = InputHarness::new(
        "input-fill-hole-island",
        Surface::Shell,
        Vec2::new(1400.0, 1000.0),
    );
    let cube = named(&harness.app, "Cube");
    let island = lc_geom::Mesh::from_polygons(
        "hole with island",
        vec![
            glam::Vec3::new(-2.0, 0.0, -2.0),
            glam::Vec3::new(2.0, 0.0, -2.0),
            glam::Vec3::new(2.0, 0.0, 2.0),
            glam::Vec3::new(-2.0, 0.0, 2.0),
            glam::Vec3::new(-1.0, 0.0, -1.0),
            glam::Vec3::new(1.0, 0.0, -1.0),
            glam::Vec3::new(1.0, 0.0, 1.0),
            glam::Vec3::new(-1.0, 0.0, 1.0),
            glam::Vec3::new(0.0, 0.0, 0.0),
            glam::Vec3::new(0.3, 0.0, 0.0),
            glam::Vec3::new(0.0, 0.3, 0.0),
        ],
        vec![
            vec![0, 1, 5, 4],
            vec![1, 2, 6, 5],
            vec![2, 3, 7, 6],
            vec![3, 0, 4, 7],
            vec![8, 9, 10],
        ],
    );
    set_editable_geometry(&mut harness.app, cube, island);
    select_object_rows(&mut harness, &[cube]);
    harness.app.mesh_edit = true;
    let depth = harness.app.history.depth();
    harness.app.fill_hole_at(4);
    assert_eq!(harness.app.history.depth(), depth);
    assert!(
        harness.app.status.contains("island"),
        "got {:?}",
        harness.app.status
    );
}

/// One quad in the XZ plane: every click point below is unambiguous on screen.
fn pick_tool_quad() -> Mesh {
    Mesh::from_polygons(
        "quad",
        vec![
            glam::Vec3::new(-1.0, 0.0, -1.0),
            glam::Vec3::new(1.0, 0.0, -1.0),
            glam::Vec3::new(1.0, 0.0, 1.0),
            glam::Vec3::new(-1.0, 0.0, 1.0),
        ],
        vec![vec![0, 1, 2, 3]],
    )
}

fn pick_tool_harness(label: &str, tool: MeshPickTool) -> (InputHarness, TestRoot, NodeId) {
    let (mut harness, root) = InputHarness::new(label, Surface::Shell, Vec2::new(900.0, 700.0));
    let id = named(&harness.app, "Cube");
    set_editable_geometry(&mut harness.app, id, pick_tool_quad());
    select_object_rows(&mut harness, &[id]);
    harness.app.mesh_edit = true;
    harness.app.pick_tool = Some(tool);
    harness.frame(Surface::Viewport, Vec::new(), egui::Modifiers::default());
    (harness, root, id)
}

fn pick_tool_project(harness: &InputHarness, id: NodeId, point: glam::Vec3) -> egui::Pos2 {
    let evaluated = harness.app.evaluate();
    let node = evaluated.node(id).expect("the mesh is evaluated");
    let rect = harness.app.last_viewport_rect;
    let (width, height) = harness.viewport_dims();
    harness
        .app
        .project(node.world.transform_point3(point), rect, width, height)
        .expect("the point is on screen")
}

#[test]
fn ring_cut_click_tool_cuts_the_plane_through_the_clicked_edge() {
    let (mut harness, _root, id) = pick_tool_harness("input-ring-cut-click", MeshPickTool::RingCut);
    let depth = harness.app.history.depth();
    let midpoint = glam::Vec3::new(0.0, 0.0, -1.0);
    let screen = pick_tool_project(&harness, id, midpoint);
    harness.click(Surface::Viewport, screen, egui::Modifiers::default());

    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, id);
    assert_eq!(mesh.polygons.len(), 2);
    assert_eq!(
        harness.app.mesh_axis, 0,
        "the plane follows the edge's axis"
    );
    assert!(mesh.validate().is_ok());
    assert_eq!(
        harness.app.pick_tool,
        Some(MeshPickTool::RingCut),
        "the tool stays active for another cut"
    );
}

#[test]
fn weld_click_tool_merges_two_points_at_the_midpoint() {
    let (mut harness, _root, id) = pick_tool_harness("input-weld-click", MeshPickTool::Weld);
    harness.app.mesh_weld_to = lc_geom::WeldTarget::Midpoint;
    let depth = harness.app.history.depth();
    let first = pick_tool_project(&harness, id, glam::Vec3::new(-1.0, 0.0, -1.0));
    harness.click(Surface::Viewport, first, egui::Modifiers::default());
    assert_eq!(
        harness.app.history.depth(),
        depth,
        "the first click only starts"
    );
    assert_eq!(harness.app.pick_start, Some(0));

    let second = pick_tool_project(&harness, id, glam::Vec3::new(1.0, 0.0, -1.0));
    harness.click(Surface::Viewport, second, egui::Modifiers::default());
    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, id);
    assert_eq!(mesh.positions.len(), 3);
    assert_eq!(mesh.polygons.len(), 1);
    assert_eq!(mesh.polygons[0].corners.len(), 3);
    assert!(
        mesh.positions
            .iter()
            .any(|position| (*position - glam::Vec3::new(0.0, 0.0, -1.0)).length() < 1e-5),
        "the merged point sits at the midpoint"
    );
    assert_eq!(harness.app.pick_start, None);
    assert!(mesh.validate().is_ok());
}

#[test]
fn weld_click_tool_can_keep_the_first_point() {
    let (mut harness, _root, id) = pick_tool_harness("input-weld-first-click", MeshPickTool::Weld);
    harness.app.mesh_weld_to = lc_geom::WeldTarget::FirstPoint;
    let first_click = glam::Vec3::new(1.0, 0.0, -1.0);
    let start = pick_tool_project(&harness, id, first_click);
    harness.click(Surface::Viewport, start, egui::Modifiers::default());
    let second = pick_tool_project(&harness, id, glam::Vec3::new(-1.0, 0.0, -1.0));
    harness.click(Surface::Viewport, second, egui::Modifiers::default());

    let mesh = selected_mesh(&harness.app, id);
    assert!(
        mesh.positions
            .iter()
            .any(|position| (*position - first_click).length() < 1e-5),
        "To: First Point keeps the first clicked position, got {:?}",
        mesh.positions
    );
    assert!(
        !mesh
            .positions
            .iter()
            .any(|position| (*position - glam::Vec3::new(0.0, 0.0, -1.0)).length() < 1e-5),
        "the midpoint is not used"
    );
    assert!(mesh.validate().is_ok());
}

#[test]
fn bridge_click_tool_bridges_two_clicked_borders() {
    let (mut harness, _root) = InputHarness::new(
        "input-bridge-click",
        Surface::Shell,
        Vec2::new(900.0, 700.0),
    );
    let id = named(&harness.app, "Cube");
    let borders = Mesh::from_polygons(
        "borders",
        vec![
            glam::Vec3::new(-1.0, 0.0, -2.0),
            glam::Vec3::new(0.0, 0.0, -2.0),
            glam::Vec3::new(0.0, 0.0, -1.0),
            glam::Vec3::new(-1.0, 0.0, -1.0),
            glam::Vec3::new(-1.0, 0.0, 1.0),
            glam::Vec3::new(0.0, 0.0, 1.0),
            glam::Vec3::new(0.0, 0.0, 2.0),
            glam::Vec3::new(-1.0, 0.0, 2.0),
        ],
        vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7]],
    );
    set_editable_geometry(&mut harness.app, id, borders);
    select_object_rows(&mut harness, &[id]);
    harness.app.mesh_edit = true;
    harness.app.pick_tool = Some(MeshPickTool::Bridge);
    harness.frame(Surface::Viewport, Vec::new(), egui::Modifiers::default());
    let depth = harness.app.history.depth();

    let first = pick_tool_project(&harness, id, glam::Vec3::new(-1.0, 0.0, -2.0));
    harness.click(Surface::Viewport, first, egui::Modifiers::default());
    assert_eq!(
        harness.app.history.depth(),
        depth,
        "the first click only starts"
    );
    assert_eq!(harness.app.pick_start, Some(0));

    let second = pick_tool_project(&harness, id, glam::Vec3::new(-1.0, 0.0, 1.0));
    harness.click(Surface::Viewport, second, egui::Modifiers::default());
    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, id);
    assert_eq!(mesh.polygons.len(), 6, "the two borders gained four quads");
    assert_eq!(harness.app.pick_start, None);
    assert!(mesh.validate().is_ok());
}

#[test]
fn insert_point_click_tool_splits_a_polygon_into_triangles() {
    let (mut harness, _root, id) =
        pick_tool_harness("input-insert-point-click", MeshPickTool::InsertPoint);
    let depth = harness.app.history.depth();
    let center = pick_tool_project(&harness, id, glam::Vec3::ZERO);
    harness.click(Surface::Viewport, center, egui::Modifiers::default());

    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, id);
    assert_eq!(mesh.polygons.len(), 4);
    assert!(mesh.polygons.iter().all(|poly| poly.corners.len() == 3));
    assert_eq!(mesh.positions.len(), 5);
    assert!(mesh.validate().is_ok());
}

#[test]
fn insert_point_click_tool_splits_a_clicked_edge() {
    let (mut harness, _root, id) =
        pick_tool_harness("input-insert-edge-click", MeshPickTool::InsertPoint);
    let depth = harness.app.history.depth();
    let midpoint = pick_tool_project(&harness, id, glam::Vec3::new(1.0, 0.0, 0.0));
    harness.click(Surface::Viewport, midpoint, egui::Modifiers::default());

    assert_eq!(harness.app.history.depth(), depth + 1);
    let mesh = selected_mesh(&harness.app, id);
    assert_eq!(
        mesh.polygons.len(),
        1,
        "an edge insert does not split the face"
    );
    assert_eq!(mesh.polygons[0].corners.len(), 5);
    assert_eq!(mesh.positions.len(), 5);
    assert!(mesh.validate().is_ok());
}

#[test]
fn pick_tool_menu_entries_are_exclusive_with_the_other_click_tools() {
    let (mut harness, _root, _id) = pick_tool_harness("input-pick-tool-menu", MeshPickTool::Weld);
    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    harness.open_menu("menu:Tools");
    assert!(harness.menu_item("Tools:Insert Point Tool").1);
    harness.click_menu_item("Tools:Insert Point Tool");
    assert_eq!(harness.app.pick_tool, Some(MeshPickTool::InsertPoint));

    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    harness.open_menu("menu:Tools");
    harness.click_menu_item("Tools:Scalpel Tool");
    assert!(harness.app.scalpel_tool);
    assert_eq!(harness.app.pick_tool, None, "the scalpel takes the click");

    harness.frame(Surface::Shell, Vec::new(), egui::Modifiers::default());
    harness.open_menu("menu:Tools");
    harness.click_menu_item("Tools:Bridge Tool");
    assert_eq!(harness.app.pick_tool, Some(MeshPickTool::Bridge));
    assert!(!harness.app.scalpel_tool);
    assert!(!harness.app.fill_hole_tool);
}
