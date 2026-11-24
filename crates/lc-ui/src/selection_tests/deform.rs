//! Modifier, skin binding, weight painting and morph coverage.

use super::*;

#[test]
fn modifier_add_and_remove_validate_through_the_same_predicates() {
    let (mut app, _root) = test_app("modifier-parity");
    let cube = named(&app, "Cube");
    let panel = named(&app, "Checker Panel");
    let light = named(&app, "Key Light");
    let subdivision = Modifier::Subdivision {
        levels: 1,
        scheme: lc_scene::SubdivisionScheme::CatmullClark,
    };

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
    assert!(!app.bind_skin_to_joints());
    assert_eq!(app.doc.signature(), before);
    assert_eq!(app.history.depth(), depth);

    app.add_joint();
    app.selection.select_one(cube);
    assert!(skin_ready(&app));
    let depth = app.history.depth();
    assert!(app.bind_skin_to_joints());
    assert!(app.doc.scene.get(cube).unwrap().skin.is_some());
    assert_eq!(app.history.depth(), depth + 1);

    assert!(
        skin_ready(&app),
        "binding prerequisites stay available for a no-op rebind check"
    );
    let before = app.doc.signature();
    let depth = app.history.depth();
    assert!(!app.bind_skin_to_joints());
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
    assert!(!app.bind_skin_to_joints());
    assert_eq!(app.doc.signature(), before);
    assert_eq!(app.history.depth(), depth);

    app.selection.set([panel, cube]);
    assert!(skin_ready(&app));
    let depth = app.history.depth();
    assert!(app.bind_skin_to_joints());
    assert!(app.doc.scene.get(cube).unwrap().skin.is_some());
    assert!(
        app.doc.scene.get(panel).unwrap().skin.is_none(),
        "inspector skin binding is explicitly primary-object only"
    );
    assert_eq!(app.history.depth(), depth + 1);
}

#[test]
fn weight_brush_falloff_is_deterministic_and_bounded() {
    assert_eq!(WeightFalloff::Constant.factor(0.0, 40.0), 1.0);
    assert_eq!(WeightFalloff::Constant.factor(40.0, 40.0), 1.0);
    assert!((WeightFalloff::Linear.factor(20.0, 40.0) - 0.5).abs() < 1e-6);
    assert_eq!(WeightFalloff::Linear.factor(0.0, 40.0), 1.0);
    assert_eq!(WeightFalloff::Linear.factor(40.0, 40.0), 0.0);
    assert_eq!(WeightFalloff::Smooth.factor(0.0, 40.0), 1.0);
    assert_eq!(WeightFalloff::Smooth.factor(40.0, 40.0), 0.0);
    assert!(WeightFalloff::Smooth.factor(10.0, 40.0) > 0.5);
    assert!(WeightFalloff::Smooth.factor(30.0, 40.0) < 0.5);
}

#[test]
fn weight_paint_stroke_is_one_undo_step() {
    let (mut app, _root) = test_app("weight-paint-undo");
    let id = app.doc.append_node(
        NodeKind::Mesh {
            geometry: GeometrySource::editable(lc_geom::box_mesh(1.0, 1.0, 1.0)),
        },
        "Weight Mesh",
        None,
    );
    app.selection.select_one(id);
    app.add_joint();
    app.add_joint();
    app.selection.select_one(id);
    app.bind_auto_weights = false;
    assert!(app.bind_skin_to_joints());
    assert!(app.weight_paint_ready());
    app.set_weight_paint(true);
    assert!(app.weight_paint);
    app.weight_joint = Some(1);

    let before = app.doc.signature();
    let depth = app.history.depth();
    assert!(app.begin_weight_stroke());
    let changed = app
        .doc
        .scene
        .get_mut(id)
        .unwrap()
        .skin
        .as_mut()
        .unwrap()
        .set_influence(0, 1, 0.5);
    assert!(changed);
    app.commit_pending();
    assert_eq!(app.history.depth(), depth + 1);
    assert_ne!(app.doc.signature(), before);
    app.undo();
    assert_eq!(app.doc.signature(), before);
}

#[test]
fn weight_brush_paints_the_vertices_under_the_cursor() {
    let (mut app, _root) = test_app("weight-paint-brush");
    let id = app.doc.append_node(
        NodeKind::Mesh {
            geometry: GeometrySource::editable(lc_geom::box_mesh(1.0, 1.0, 1.0)),
        },
        "Brush Mesh",
        None,
    );
    app.selection.select_one(id);
    app.add_joint();
    app.add_joint();
    app.selection.select_one(id);
    app.bind_auto_weights = false;
    assert!(app.bind_skin_to_joints());
    app.set_weight_paint(true);
    app.weight_joint = Some(1);
    app.weight_brush.radius = 8.0;
    app.weight_brush.strength = 1.0;
    app.weight_brush.mode = WeightBrushMode::Add;
    app.weight_brush.falloff = WeightFalloff::Constant;

    let evaluated = app.evaluate();
    let node = evaluated.node(id).unwrap();
    let mesh = node.mesh().unwrap();
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(200.0));
    let pointer = mesh
        .positions
        .iter()
        .find_map(|position| app.project(node.world.transform_point3(*position), rect, 200, 200))
        .expect("the test mesh must be visible from the default camera");
    let before = app.doc.signature();
    assert!(app.begin_weight_stroke());
    assert!(app.paint_weight_brush(&evaluated, rect, 200, 200, pointer));
    app.commit_pending();
    assert_ne!(app.doc.signature(), before);
    let skin = app.doc.scene.get(id).unwrap().skin.as_ref().unwrap();
    assert!(skin.influence(0, 1) > 0.0);
}

#[test]
fn selected_joint_binding_restricts_the_skin_and_leaves_uncovered_vertices_at_rest() {
    let (mut app, _root) = test_app("bind-selected-joints");
    let mesh = lc_geom::box_mesh(2.0, 1.0, 1.0);
    let vertex_count = mesh.positions.len();
    let id = app.doc.append_node(
        NodeKind::Mesh {
            geometry: GeometrySource::editable(mesh),
        },
        "Selected Joint Mesh",
        None,
    );
    let joint = app.doc.append_node(
        NodeKind::Joint {
            length: 0.8,
            width: 0.08,
        },
        "Chosen Joint",
        None,
    );
    let unused = app.doc.append_node(
        NodeKind::Joint {
            length: 0.8,
            width: 0.08,
        },
        "Unchosen Joint",
        None,
    );
    app.selection.select_one(id);
    app.bind_auto_weights = false;
    app.set_all_bind_joints(false);
    app.set_bind_joint(joint, true);
    assert_eq!(app.bind_joint_ids(), vec![joint]);
    assert!(app.bind_skin_to_joints());
    let skin = app.doc.scene.get(id).unwrap().skin.as_ref().unwrap();
    assert_eq!(skin.joints, vec![joint]);
    assert!(!skin.joints.contains(&unused));
    assert_eq!(skin.vertex_count(), vertex_count);
    assert!(skin.weights.iter().all(|weights| weights == &[0.0; 4]));
    assert!(
        skin.joint_indices.iter().all(|indices| indices == &[0; 4]),
        "unweighted vertices must not be assigned a fallback joint"
    );

    let rest = app
        .evaluate()
        .node(id)
        .and_then(|node| node.mesh())
        .map(|mesh| mesh.positions.clone())
        .unwrap();
    app.doc.scene.get_mut(joint).unwrap().transform =
        Transform::from_translation(Vec3::new(0.0, 2.0, 0.0));
    let posed = app
        .evaluate()
        .node(id)
        .and_then(|node| node.mesh())
        .map(|mesh| mesh.positions.clone())
        .unwrap();
    assert!(
        rest.iter()
            .zip(&posed)
            .all(|(a, b)| (*a - *b).length() < 1e-5),
        "unweighted vertices must stay at their rest positions"
    );
}

#[test]
fn proximity_bind_produces_normalized_weights_that_deform_when_posed() {
    let (mut app, _root) = test_app("proximity-bind");
    let mesh = lc_geom::box_mesh(2.0, 1.0, 1.0);
    let base_positions = mesh.positions.clone();
    let id = app.doc.append_node(
        NodeKind::Mesh {
            geometry: GeometrySource::editable(mesh),
        },
        "Proximity Mesh",
        None,
    );
    app.doc.scene.get_mut(id).unwrap().transform =
        Transform::from_translation(Vec3::new(3.0, 0.0, 0.0));
    let left = app.doc.append_node(
        NodeKind::Joint {
            length: 0.8,
            width: 0.08,
        },
        "Left Joint",
        None,
    );
    let right = app.doc.append_node(
        NodeKind::Joint {
            length: 0.8,
            width: 0.08,
        },
        "Right Joint",
        None,
    );
    let unused = app.doc.append_node(
        NodeKind::Joint {
            length: 0.8,
            width: 0.08,
        },
        "Unused Auto Joint",
        None,
    );
    app.doc.scene.get_mut(left).unwrap().transform =
        Transform::from_translation(Vec3::new(2.0, 0.0, 0.0));
    app.doc.scene.get_mut(right).unwrap().transform =
        Transform::from_translation(Vec3::new(4.0, 0.0, 0.0));
    app.selection.select_one(id);
    app.bind_joints = Some([left, right].into_iter().collect());
    app.bind_auto_weights = true;
    app.bind_dropoff = 2.5;
    app.bind_strength = 0.8;
    app.bind_max_influences = 2;
    assert!(app.bind_skin_to_joints());
    let skin = app.doc.scene.get(id).unwrap().skin.as_ref().unwrap();
    assert_eq!(skin.joints, vec![left, right]);
    assert!(!skin.joints.contains(&unused));
    for weights in skin.weights.iter().take(skin.vertex_count()) {
        let total: f32 = weights.iter().sum();
        assert!((total - 1.0).abs() < 1e-5);
    }
    assert!(skin.influence(0, 0) > skin.influence(0, 1));
    assert!(skin.influence(1, 1) > skin.influence(1, 0));

    let rest = app
        .evaluate()
        .node(id)
        .and_then(|node| node.mesh())
        .map(|mesh| mesh.positions.clone())
        .unwrap();
    assert!(
        rest.iter()
            .zip(&base_positions)
            .all(|(a, b)| (*a - *b).length() < 1e-5),
        "binding a transformed mesh must preserve its rest pose"
    );
    app.doc.scene.get_mut(right).unwrap().transform =
        Transform::from_translation(Vec3::new(4.0, 1.0, 0.0));
    let posed = app
        .evaluate()
        .node(id)
        .and_then(|node| node.mesh())
        .map(|mesh| mesh.positions.clone())
        .unwrap();
    let max_delta = rest
        .iter()
        .zip(&posed)
        .map(|(a, b)| (*a - *b).length())
        .fold(0.0f32, f32::max);
    assert!(max_delta > 0.05, "auto weights must deform when posed");
}

#[test]
fn auto_weight_panel_controls_change_the_bound_weights() {
    let (mut app, _root) = test_app("auto-weight-controls");
    let id = app.doc.append_node(
        NodeKind::Mesh {
            geometry: GeometrySource::editable(lc_geom::box_mesh(2.0, 1.0, 1.0)),
        },
        "Auto Controls Mesh",
        None,
    );
    let left = app.doc.append_node(
        NodeKind::Joint {
            length: 0.8,
            width: 0.08,
        },
        "Controls Left",
        None,
    );
    let right = app.doc.append_node(
        NodeKind::Joint {
            length: 0.8,
            width: 0.08,
        },
        "Controls Right",
        None,
    );
    app.doc.scene.get_mut(left).unwrap().transform =
        Transform::from_translation(Vec3::new(-1.0, 0.0, 0.0));
    app.doc.scene.get_mut(right).unwrap().transform =
        Transform::from_translation(Vec3::new(1.0, 0.0, 0.0));
    app.selection.select_one(id);
    app.bind_joints = Some([left, right].into_iter().collect());
    app.bind_auto_weights = true;
    app.bind_dropoff = 2.0;
    app.bind_strength = 0.8;
    app.bind_max_influences = 2;
    assert!(app.bind_skin_to_joints());
    let two_way = app.doc.scene.get(id).unwrap().skin.clone().unwrap().weights;
    assert_eq!(two_way[0].iter().filter(|weight| **weight > 0.0).count(), 2);

    assert!(app.unbind_skin_from_primary());
    app.bind_dropoff = 0.5;
    app.bind_strength = 0.1;
    assert!(app.bind_skin_to_joints());
    let softer = app.doc.scene.get(id).unwrap().skin.clone().unwrap().weights;
    assert_eq!(softer[0].iter().filter(|weight| **weight > 0.0).count(), 2);
    assert_ne!(
        two_way[0], softer[0],
        "panel dropoff and strength settings must reach the bind"
    );

    assert!(app.unbind_skin_from_primary());
    app.bind_max_influences = 1;
    assert!(app.bind_skin_to_joints());
    let one_way = app.doc.scene.get(id).unwrap().skin.clone().unwrap().weights;
    assert_eq!(one_way[0].iter().filter(|weight| **weight > 0.0).count(), 1);
    assert_ne!(
        softer[0], one_way[0],
        "panel max-influence setting must reach the bind"
    );
}

#[test]
fn rebinding_preserves_covered_weights_and_keeps_the_brush_usable() {
    let (mut app, _root) = test_app("preserve-rebind");
    let mesh = lc_geom::box_mesh(3.0, 1.0, 1.0);
    let id = app.doc.append_node(
        NodeKind::Mesh {
            geometry: GeometrySource::editable(mesh),
        },
        "Rebind Mesh",
        None,
    );
    let left = app.doc.append_node(
        NodeKind::Joint {
            length: 0.8,
            width: 0.08,
        },
        "Rebind Left",
        None,
    );
    let middle = app.doc.append_node(
        NodeKind::Joint {
            length: 0.8,
            width: 0.08,
        },
        "Rebind Middle",
        None,
    );
    let right = app.doc.append_node(
        NodeKind::Joint {
            length: 0.8,
            width: 0.08,
        },
        "Rebind Right",
        None,
    );
    app.doc.scene.get_mut(left).unwrap().transform =
        Transform::from_translation(Vec3::new(-1.0, 0.0, 0.0));
    app.doc.scene.get_mut(right).unwrap().transform =
        Transform::from_translation(Vec3::new(1.0, 0.0, 0.0));
    app.selection.select_one(id);
    app.bind_joints = Some([left, middle, right].into_iter().collect());
    app.bind_auto_weights = true;
    app.bind_max_influences = 3;
    assert!(app.bind_skin_to_joints());
    let initial_right_index = app
        .doc
        .scene
        .get(id)
        .and_then(|node| node.skin.as_ref())
        .and_then(|skin| skin.joints.iter().position(|joint| *joint == right))
        .unwrap();
    let initial_left_index = app
        .doc
        .scene
        .get(id)
        .and_then(|node| node.skin.as_ref())
        .and_then(|skin| skin.joints.iter().position(|joint| *joint == left))
        .unwrap();
    let initial_middle_index = app
        .doc
        .scene
        .get(id)
        .and_then(|node| node.skin.as_ref())
        .and_then(|skin| skin.joints.iter().position(|joint| *joint == middle))
        .unwrap();
    app.doc
        .scene
        .get_mut(id)
        .unwrap()
        .skin
        .as_mut()
        .unwrap()
        .set_exclusive_influence(0, initial_right_index, 1.0);
    app.doc
        .scene
        .get_mut(id)
        .unwrap()
        .skin
        .as_mut()
        .unwrap()
        .set_exclusive_influence(1, initial_left_index, 1.0);
    app.doc
        .scene
        .get_mut(id)
        .unwrap()
        .skin
        .as_mut()
        .unwrap()
        .set_exclusive_influence(2, 0, 0.0);
    app.weight_joint = Some(initial_middle_index);

    // This is a real rebind: the middle joint is removed. The painted vertices
    // must keep their right/left weights; only the cleared vertex is auto-filled.
    app.bind_joints = Some([left, right].into_iter().collect());
    assert!(app.bind_skin_to_joints());
    let (left_index, right_index) = app
        .doc
        .scene
        .get(id)
        .and_then(|node| node.skin.as_ref())
        .map(|skin| {
            (
                skin.joints.iter().position(|joint| *joint == left).unwrap(),
                skin.joints
                    .iter()
                    .position(|joint| *joint == right)
                    .unwrap(),
            )
        })
        .unwrap();
    {
        let skin = app.doc.scene.get(id).unwrap().skin.as_ref().unwrap();
        assert_eq!(skin.joints, vec![left, right]);
        assert!((skin.influence(0, right_index) - 1.0).abs() < 1e-5);
        assert_eq!(skin.influence(0, left_index), 0.0);
        assert!((skin.influence(1, left_index) - 1.0).abs() < 1e-5);
        assert_eq!(skin.influence(1, right_index), 0.0);
        let filled: f32 = skin.weights[2].iter().sum();
        assert!((filled - 1.0).abs() < 1e-5);
        assert!(skin.weights[2].iter().any(|weight| *weight > 0.0));
        for weights in skin.weights.iter().take(skin.vertex_count()) {
            let total: f32 = weights.iter().sum();
            if total > 0.0 {
                assert!((total - 1.0).abs() < 1e-5);
            }
            assert!(weights.iter().filter(|weight| **weight > 0.0).count() <= 2);
        }
    }
    assert_eq!(app.active_weight_joint(), Some(0));

    // Dropping the painted joint leaves covered vertices intact and clears the
    // vertex that had no remaining influence; the brush still works afterwards.
    app.bind_joints = Some([right].into_iter().collect());
    app.bind_auto_weights = false;
    assert!(app.bind_skin_to_joints());
    assert_eq!(app.active_weight_joint(), Some(0));
    let skin = app.doc.scene.get(id).unwrap().skin.as_ref().unwrap();
    assert_eq!(skin.joints, vec![right]);
    assert!((skin.influence(0, 0) - 1.0).abs() < 1e-5);
    assert!(skin.weights[1].iter().all(|weight| *weight == 0.0));

    // The heat-map overlay must read the rebound skin and show both the painted
    // vertex (orange) and the cleared vertex (grey).
    let evaluated = app.evaluate();
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(200.0));
    let ctx = egui::Context::default();
    ctx.set_fonts(egui::FontDefinitions::empty());
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        app.draw_weight_paint_overlay(ui.painter(), ui, rect, &evaluated, 200, 200);
    });
    let fills: Vec<egui::Color32> = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Circle(circle) => Some(circle.fill),
            _ => None,
        })
        .collect();
    output.drop_without_applying_deltas();
    let zero_color = egui::Color32::from_rgba_unmultiplied(190, 195, 200, 80);
    let painted_color = egui::Color32::from_rgba_unmultiplied(245, 135, 35, 245);
    assert!(
        fills.contains(&painted_color),
        "the heat map must paint the active joint's nonzero weight"
    );
    assert!(
        fills.contains(&zero_color),
        "the heat map must show zero-weight vertices in grey"
    );

    app.set_weight_paint(true);
    assert!(app.weight_paint_ready());
    app.weight_joint = Some(0);
    let before = app.doc.signature();
    let depth = app.history.depth();
    assert!(app.begin_weight_stroke());
    assert!(app
        .doc
        .scene
        .get_mut(id)
        .unwrap()
        .skin
        .as_mut()
        .unwrap()
        .set_influence(1, 0, 0.5));
    let painted = app.doc.scene.get(id).unwrap().skin.as_ref().unwrap();
    assert!((painted.weights[1].iter().sum::<f32>() - 1.0).abs() < 1e-5);
    assert_eq!(
        painted.weights[1]
            .iter()
            .filter(|weight| **weight > 0.0)
            .count(),
        1
    );
    app.commit_pending();
    assert_eq!(app.history.depth(), depth + 1);
    assert_ne!(app.doc.signature(), before);
    app.undo();
    assert_eq!(app.doc.signature(), before);
}

#[test]
fn morph_targets_capture_weight_and_undo() {
    let (mut app, _root) = test_app("morph-targets");
    let id = app.selection.primary().expect("selected cube");
    app.make_selected_editable();
    // Morphs are recorded against the editable mesh, before modifiers rebuild
    // its topology, so exercise the workflow on the raw cube surface.
    app.doc.scene.get_mut(id).unwrap().modifiers.clear();
    assert!(app.record_morph_base());
    assert_eq!(app.history.undo_label().as_deref(), Some("Edit Morph"));

    let (base, moved) = {
        let node = app.doc.scene.get(id).unwrap();
        let NodeKind::Mesh {
            geometry: GeometrySource::Editable { mesh, .. },
        } = &node.kind
        else {
            panic!("expected an editable mesh");
        };
        (mesh.positions[0], mesh.positions[0] + Vec3::Y * 2.0)
    };
    if let NodeKind::Mesh {
        geometry: GeometrySource::Editable { mesh, .. },
    } = &mut app.doc.scene.get_mut(id).unwrap().kind
    {
        mesh.positions[0] = moved;
    }
    assert!(app.capture_morph_target());
    let target = &app.doc.scene.get(id).unwrap().morph.targets[0];
    assert_eq!(target.name, "Morph 1");
    assert!((target.weight - 0.0).abs() < 1e-6);
    let delta = target.deltas[0];
    assert!((delta - Vec3::Y * 2.0).length() < 1e-5);

    let original = app.doc.scene.get(id).unwrap().clone();
    let mut edited = original.clone();
    edited.morph.targets[0].weight = 0.5;
    app.apply_node_edits(id, &original, &edited);
    app.commit_pending();
    assert_eq!(app.history.undo_label().as_deref(), Some("Edit Morph"));
    let posed = app.evaluate();
    let position = posed.node(id).unwrap().mesh().unwrap().positions[0];
    assert!(
        (position - (base + delta * 0.5)).length() < 1e-4,
        "position {position:?} base {base:?} delta {delta:?} weight {}",
        app.doc.scene.get(id).unwrap().morph.targets[0].weight
    );

    app.undo();
    assert!((app.doc.scene.get(id).unwrap().morph.targets[0].weight - 0.0).abs() < 1e-6);
    app.undo();
    assert!(app.doc.scene.get(id).unwrap().morph.targets.is_empty());
    app.undo();
    assert!(!app.doc.scene.get(id).unwrap().morph.has_base());
    app.undo();
    assert!(matches!(
        app.doc.scene.get(id).unwrap().kind,
        NodeKind::Mesh {
            geometry: GeometrySource::Primitive { .. }
        }
    ));
}
