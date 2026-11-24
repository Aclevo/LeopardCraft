//! Timeline, F-curve key editing, clipboard and retiming coverage.

use super::*;

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
fn fcurve_manual_tangents_survive_edits_and_undo_redo() {
    let (mut app, _root) = test_app("fcurve-tangents");
    let cube = named(&app, "Cube");
    app.selection.select_one(cube);
    let target = AnimTarget::Object(cube);
    {
        let node = app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        node.animation
            .set_key(AnimProperty::TranslationX, 0.0, 0.0, Interpolation::Smooth);
        node.animation
            .set_key(AnimProperty::TranslationX, 1.0, 1.0, Interpolation::Smooth);
    }
    assert!(app.set_anim_target(target));
    assert!(app.select_curve_key(AnimProperty::TranslationX, 0.0));
    assert!(app.set_target_curve_tangent(
        target,
        CurveTrackId::Scalar(AnimProperty::TranslationX),
        0.0,
        false,
        2.0
    ));
    let clip = app.target_clip(target).unwrap();
    let key = clip.track(AnimProperty::TranslationX).unwrap().curve.keys[0];
    assert!(key.tangent_manual);
    assert!((key.tangent_out - 2.0).abs() < 1e-5);
    let sampled = clip.sample(AnimProperty::TranslationX, 0.5).unwrap();
    assert!(
        (sampled - 0.625).abs() < 1e-4,
        "manual tangent sampled {sampled}"
    );

    app.undo();
    let restored = app.target_clip(target).unwrap();
    let key = restored
        .track(AnimProperty::TranslationX)
        .unwrap()
        .curve
        .keys[0];
    assert!(!key.tangent_manual);
    assert!((restored.sample(AnimProperty::TranslationX, 0.5).unwrap() - 0.5).abs() < 1e-4);
    app.redo();
    let restored = app.target_clip(target).unwrap();
    let key = restored
        .track(AnimProperty::TranslationX)
        .unwrap()
        .curve
        .keys[0];
    assert!(key.tangent_manual);
    assert!((key.tangent_out - 2.0).abs() < 1e-5);

    // A later key insertion must not overwrite the hand-authored slope.
    app.doc.scene.get_mut(cube).unwrap().animation.set_key(
        AnimProperty::TranslationX,
        2.0,
        0.0,
        Interpolation::Smooth,
    );
    let key = app
        .target_clip(target)
        .unwrap()
        .track(AnimProperty::TranslationX)
        .unwrap()
        .curve
        .keys[0];
    assert!(key.tangent_manual);
    assert!((key.tangent_out - 2.0).abs() < 1e-5);
}

#[test]
fn fcurve_retime_selected_channels_is_one_undo_and_rescales_manual_tangents() {
    let (mut app, _root) = test_app("fcurve-retime-selected");
    let cube = named(&app, "Cube");
    app.selection.select_one(cube);
    {
        let node = app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        for (time, value) in [(1.0, 0.0), (3.0, 1.0)] {
            node.animation.set_key(
                AnimProperty::TranslationX,
                time,
                value,
                Interpolation::Smooth,
            );
            node.animation
                .set_morph_weight_key(0, time, value, Interpolation::Smooth);
        }
        node.animation
            .set_rotation_key(1.0, Quat::IDENTITY, Interpolation::Linear);
        node.animation
            .set_rotation_key(3.0, Quat::from_rotation_y(1.0), Interpolation::Linear);
        let curve = &mut node
            .animation
            .track_mut(AnimProperty::TranslationX)
            .unwrap()
            .curve;
        assert!(curve.set_manual_tangents(1.0, 0.0, 2.0));
        assert!(curve.set_manual_tangents(3.0, 0.0, 0.0));
    }
    let target = AnimTarget::Object(cube);
    assert!(app.set_anim_target(target));
    let old_mid = app
        .target_clip(target)
        .unwrap()
        .sample(AnimProperty::TranslationX, 2.0)
        .unwrap();
    for track in [
        CurveTrackId::Scalar(AnimProperty::TranslationX),
        CurveTrackId::MorphWeight(0),
        CurveTrackId::Rotation,
    ] {
        for time in [1.0, 3.0] {
            assert!(app.select_target_track_key_additive(target, track, time, true));
        }
    }
    assert_eq!(app.selected_curve_keys().len(), 6);
    app.history.clear();
    assert!(app.retime_selected_curve_keys(2.0));
    assert_eq!(app.history.depth(), 1, "retime is one undo step");
    let clip = app.target_clip(target).unwrap();
    assert_property_curve_times(&app, target, AnimProperty::TranslationX, &[1.0, 5.0]);
    assert_eq!(
        clip.morph_weights.get(&0).unwrap().keys[1].time,
        5.0,
        "morph keys move in the same pass"
    );
    assert_eq!(
        clip.rotation.as_ref().unwrap().keys[1].time,
        5.0,
        "rotation keys move in the same pass"
    );
    let key = clip.track(AnimProperty::TranslationX).unwrap().curve.keys[0];
    assert!(key.tangent_manual);
    assert!((key.tangent_out - 1.0).abs() < 1e-5);
    assert!((clip.sample(AnimProperty::TranslationX, 3.0).unwrap() - old_mid).abs() < 1e-4);

    app.undo();
    let restored = app.target_clip(target).unwrap();
    assert_property_curve_times(&app, target, AnimProperty::TranslationX, &[1.0, 3.0]);
    let key = restored
        .track(AnimProperty::TranslationX)
        .unwrap()
        .curve
        .keys[0];
    assert!((key.tangent_out - 2.0).abs() < 1e-5);
    assert!(key.tangent_manual);
    assert!((restored.sample(AnimProperty::TranslationX, 2.0).unwrap() - old_mid).abs() < 1e-4);

    app.redo();
    let redone = app.target_clip(target).unwrap();
    assert_property_curve_times(&app, target, AnimProperty::TranslationX, &[1.0, 5.0]);
    assert!(
        (redone.track(AnimProperty::TranslationX).unwrap().curve.keys[0].tangent_out - 1.0).abs()
            < 1e-5
    );
}

#[test]
fn fcurve_retime_keeps_tracks_that_are_anchored_at_the_pivot_consistent() {
    let (mut app, _root) = test_app("fcurve-retime-anchored");
    let cube = named(&app, "Cube");
    app.selection.select_one(cube);
    {
        let node = app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        node.animation
            .set_key(AnimProperty::TranslationX, 1.0, 0.0, Interpolation::Linear);
        node.animation
            .set_rotation_key(2.0, Quat::from_rotation_y(0.5), Interpolation::Linear);
    }
    let target = AnimTarget::Object(cube);
    assert!(app.set_anim_target(target));
    assert!(app.select_target_track_key_additive(
        target,
        CurveTrackId::Scalar(AnimProperty::TranslationX),
        1.0,
        true
    ));
    assert!(app.select_target_track_key_additive(target, CurveTrackId::Rotation, 2.0, true));
    app.history.clear();

    assert!(app.retime_selected_curve_keys(2.0));
    assert_eq!(app.history.depth(), 1);
    let clip = app.target_clip(target).unwrap();
    assert_eq!(
        clip.track(AnimProperty::TranslationX).unwrap().curve.keys[0].time,
        1.0,
        "the scalar track is anchored at the pivot"
    );
    assert_eq!(
        clip.rotation.as_ref().unwrap().keys[0].time,
        3.0,
        "the other selected track still moves"
    );
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

    assert!(app.set_curve_key_interpolation(AnimProperty::RotationY, 0.0, Interpolation::Constant));
    let track = app
        .doc
        .scene
        .get(cube)
        .unwrap()
        .animation
        .track(AnimProperty::RotationY)
        .unwrap();
    assert_eq!(track.curve.keys[0].interpolation, Interpolation::Constant);
    assert_eq!(track.curve.keys[1].interpolation, Interpolation::Linear);
    assert_eq!(app.history.depth(), 1);

    let before = app.doc.signature();
    assert!(!app.set_curve_key_interpolation(
        AnimProperty::RotationY,
        0.0,
        Interpolation::Constant
    ));
    assert!(!app.set_curve_key_interpolation(
        AnimProperty::RotationY,
        4.0,
        Interpolation::Constant
    ));
    assert_eq!(app.doc.signature(), before);
    assert_eq!(app.history.depth(), 1);
}

#[test]
fn fcurve_interpolation_and_tangent_type_cover_mixed_selection_with_undo() {
    let (mut app, _root) = test_app("curve-interpolation-tangent-mixed");
    let cube = named(&app, "Cube");
    app.selection.select_one(cube);
    {
        let animation = &mut app.doc.scene.get_mut(cube).unwrap().animation;
        *animation = AnimationClip::default();
        for (time, value) in [(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)] {
            animation.set_key(
                AnimProperty::TranslationX,
                time,
                value,
                Interpolation::Spline,
            );
            animation.set_morph_weight_key(0, time, value, Interpolation::Spline);
            animation.set_rotation_key(time, Quat::from_rotation_y(value), Interpolation::Spline);
        }
        let curve = &mut animation
            .track_mut(AnimProperty::TranslationX)
            .unwrap()
            .curve;
        assert!(curve.set_manual_tangents(0.0, 0.0, 2.0));
    }
    let target = AnimTarget::Object(cube);
    assert!(app.set_anim_target(target));
    for track in [
        CurveTrackId::Scalar(AnimProperty::TranslationX),
        CurveTrackId::MorphWeight(0),
        CurveTrackId::Rotation,
    ] {
        assert!(app.select_target_track_key_additive(target, track, 0.0, true));
    }
    assert_eq!(app.selected_curve_keys().len(), 3);
    assert_eq!(
        app.selected_curve_interpolation(),
        Some(Interpolation::Spline)
    );
    assert_eq!(app.selected_curve_tangent_type(), Some(TangentType::Spline));

    app.history.clear();
    assert!(app.set_selected_curve_key_interpolation(Interpolation::Smooth));
    assert_eq!(
        app.history.depth(),
        1,
        "interpolation edit is one undo step"
    );
    let clip = app.target_clip(target).unwrap();
    assert_eq!(
        clip.track(AnimProperty::TranslationX).unwrap().curve.keys[0].interpolation,
        Interpolation::Smooth
    );
    assert_eq!(
        clip.morph_weights.get(&0).unwrap().keys[0].interpolation,
        Interpolation::Smooth
    );
    assert_eq!(
        clip.rotation.as_ref().unwrap().keys[0].interpolation,
        Interpolation::Smooth
    );
    assert!(
        !clip.track(AnimProperty::TranslationX).unwrap().curve.keys[0].tangent_manual,
        "Smooth redefines the hand-authored tangent"
    );

    app.undo();
    let restored = app.target_clip(target).unwrap();
    assert_eq!(
        restored
            .track(AnimProperty::TranslationX)
            .unwrap()
            .curve
            .keys[0]
            .interpolation,
        Interpolation::Spline
    );
    assert!(
        restored
            .track(AnimProperty::TranslationX)
            .unwrap()
            .curve
            .keys[0]
            .tangent_manual
    );
    assert!(
        (restored
            .track(AnimProperty::TranslationX)
            .unwrap()
            .curve
            .keys[0]
            .tangent_out
            - 2.0)
            .abs()
            < 1e-5,
        "undo restores the previous hand-authored slope"
    );
    app.redo();
    assert_eq!(
        app.target_clip(target)
            .unwrap()
            .rotation
            .as_ref()
            .unwrap()
            .keys[0]
            .interpolation,
        Interpolation::Smooth
    );

    app.undo();
    app.history.clear();
    assert!(app.set_selected_curve_tangent_type(TangentType::Linear, true, true));
    assert_eq!(app.history.depth(), 1, "tangent type edit is one undo step");
    let changed = app.target_clip(target).unwrap();
    let scalar = changed
        .track(AnimProperty::TranslationX)
        .unwrap()
        .curve
        .keys[0];
    assert_eq!(scalar.tangent_in_type, TangentType::Linear);
    assert_eq!(scalar.tangent_out_type, TangentType::Linear);
    assert!(!scalar.tangent_manual);
    assert_eq!(
        changed.morph_weights.get(&0).unwrap().keys[0].tangent_in_type,
        TangentType::Linear
    );
    assert_eq!(
        changed.rotation.as_ref().unwrap().keys[0].tangent_in_type,
        TangentType::Linear,
        "rotation keys receive the same tangent-type edit"
    );

    app.undo();
    let restored = app.target_clip(target).unwrap();
    let scalar = restored
        .track(AnimProperty::TranslationX)
        .unwrap()
        .curve
        .keys[0];
    assert!(scalar.tangent_manual);
    assert!((scalar.tangent_out - 2.0).abs() < 1e-5);
    assert_eq!(scalar.tangent_out_type, TangentType::Spline);
    app.redo();
    assert_eq!(
        app.target_clip(target)
            .unwrap()
            .track(AnimProperty::TranslationX)
            .unwrap()
            .curve
            .keys[0]
            .tangent_out_type,
        TangentType::Linear
    );
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
    assert!(app.set_selected_curve_key_interpolation(Interpolation::Constant));
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
        Interpolation::Constant
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
    assert!(app.set_selected_curve_key_interpolation(Interpolation::Constant));
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
    assert!(app.set_selected_curve_tangent_type(TangentType::Linear, true, true));
    assert_eq!(app.history.depth(), depth + 1);
    let typed = app
        .doc
        .scene
        .get(cube)
        .unwrap()
        .tag_animations
        .get(&(tag_index as u32))
        .unwrap()
        .clone();
    assert_eq!(
        typed
            .track(AnimProperty::CopyLocationWeight)
            .unwrap()
            .curve
            .keys[0]
            .tangent_out_type,
        TangentType::Linear
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
    assert!(app.set_selected_curve_key_interpolation(Interpolation::Constant));
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
    assert!(app.set_selected_curve_tangent_type(TangentType::Linear, true, true));
    assert_eq!(app.history.depth(), depth + 1);
    let typed = app
        .doc
        .materials
        .get(material_id)
        .unwrap()
        .animation
        .clone();
    assert_eq!(
        typed
            .track(AnimProperty::MaterialRoughness)
            .unwrap()
            .curve
            .keys[0]
            .tangent_out_type,
        TangentType::Linear
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
fn baking_an_ik_take_is_one_undo_step_and_writes_every_frame() {
    let (mut app, _root) = test_app("bake-ik-take");
    let shoulder = app.doc.append_node(
        NodeKind::Joint {
            length: 1.0,
            width: 0.08,
        },
        "IK Shoulder",
        None,
    );
    let elbow = app.doc.append_node(
        NodeKind::Joint {
            length: 1.0,
            width: 0.08,
        },
        "IK Elbow",
        Some(shoulder),
    );
    let wrist = app.doc.append_node(
        NodeKind::Joint {
            length: 1.0,
            width: 0.08,
        },
        "IK Wrist",
        Some(elbow),
    );
    app.doc.scene.get_mut(elbow).unwrap().transform = Transform::from_translation(Vec3::X);
    app.doc.scene.get_mut(wrist).unwrap().transform = Transform::from_translation(Vec3::X);
    let target = app.doc.append_node(NodeKind::Null, "IK Goal", None);
    app.doc.scene.get_mut(target).unwrap().transform =
        Transform::from_translation(Vec3::new(1.0, 1.2, 0.0));
    app.doc.scene.get_mut(wrist).unwrap().ik = Some(lc_scene::IkChain::new(target));
    app.playback.start = 0.0;
    app.playback.end = 1.0;
    app.playback.fps = 1.0;
    app.playback.time = 0.0;
    let depth = app.history.depth();
    assert!(app.bake_ik_chain(wrist));
    assert_eq!(app.history.depth(), depth + 1);
    for joint in [shoulder, elbow] {
        let track = app
            .doc
            .scene
            .get(joint)
            .unwrap()
            .animation
            .rotation
            .as_ref()
            .expect("bake must create rotation F-curves");
        assert_eq!(track.keys.len(), 2, "both take ends should be keyed");
    }
    assert!(app.history.undo(&mut app.doc).is_some());
    assert_eq!(app.history.depth(), depth);
    assert!(app
        .doc
        .scene
        .get(shoulder)
        .unwrap()
        .animation
        .rotation
        .is_none());
}

#[test]
fn morph_weight_fcurve_row_keys_and_undo_redo() {
    let (mut app, _root) = test_app("morph-fcurve");
    let id = app.selection.primary().expect("selected cube");
    app.make_selected_editable();
    app.doc.scene.get_mut(id).unwrap().modifiers.clear();
    assert!(app.record_morph_base());
    let (base, delta) = {
        let node = app.doc.scene.get_mut(id).unwrap();
        let base = node.morph.base.clone();
        let delta = Vec3::Y;
        node.morph
            .targets
            .push(MorphTarget::new("Lift", vec![delta; base.len()]));
        (base, delta)
    };
    let target = AnimTarget::Object(id);
    assert!(app.set_anim_target(target));
    let rows = app.curve_rows(target);
    let row = *rows
        .iter()
        .find(|row| row.channel == AnimChannel::MorphWeight && row.morph_target == Some(0))
        .expect("morph target contributes an F-curve row");
    assert_eq!(row.property, None);
    assert_eq!(app.curve_row_label(target, row), "Lift Weight");

    app.history.clear();
    app.playback.time = 0.0;
    app.doc.scene.get_mut(id).unwrap().morph.targets[0].weight = 0.0;
    assert!(app.record_curve_row_key(target, row));
    app.playback.time = 1.0;
    app.doc.scene.get_mut(id).unwrap().morph.targets[0].weight = 1.0;
    assert!(app.record_curve_row_key(target, row));
    let clip = app.doc.scene.get(id).unwrap().animation.clone();
    let curve = clip.morph_weights.get(&0).expect("morph weight curve");
    assert_eq!(curve.keys.len(), 2, "both authored keys must be present");
    assert!((clip.sample_morph_weight(0, 0.5).unwrap() - 0.5).abs() < 1e-5);

    app.playback.time = 0.5;
    let posed = app.evaluate().node(id).unwrap().mesh().unwrap().positions[0];
    assert!(
        (posed - (base[0] + delta * 0.5)).length() < 1e-4,
        "evaluated morph weight did not interpolate"
    );

    app.undo();
    assert_eq!(
        app.doc
            .scene
            .get(id)
            .unwrap()
            .animation
            .morph_weights
            .get(&0)
            .unwrap()
            .keys
            .len(),
        1,
        "undo must remove the second curve key"
    );
    app.redo();
    assert_eq!(
        app.doc
            .scene
            .get(id)
            .unwrap()
            .animation
            .morph_weights
            .get(&0)
            .unwrap()
            .keys
            .len(),
        2,
        "redo must restore the second curve key"
    );
    app.playback.time = 0.5;
    let posed = app.evaluate().node(id).unwrap().mesh().unwrap().positions[0];
    assert!(
        (posed - (base[0] + delta * 0.5)).length() < 1e-4,
        "redo must restore the evaluated morph deformation"
    );
    assert!(app.select_target_track_key(target, CurveTrackId::MorphWeight(0), 0.0));
    assert!(app.set_selected_curve_key_interpolation(Interpolation::Linear));
    assert_eq!(
        app.doc
            .scene
            .get(id)
            .unwrap()
            .animation
            .morph_weights
            .get(&0)
            .unwrap()
            .keys[0]
            .interpolation,
        Interpolation::Linear,
        "morph F-curve keys use the shared interpolation path"
    );
    assert!(app.select_target_track_key(target, CurveTrackId::MorphWeight(0), 1.0));
    assert!(app.set_target_curve_tangent(target, CurveTrackId::MorphWeight(0), 1.0, false, -2.0));
    let key = app
        .target_clip(target)
        .unwrap()
        .morph_weights
        .get(&0)
        .unwrap()
        .keys[1];
    assert!(key.tangent_manual);
    assert!((key.tangent_out + 2.0).abs() < 1e-5);
}

#[test]
fn fcurve_retime_covers_tag_and_material_targets() {
    let (mut app, _root) = test_app("fcurve-retime-scopes");
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
    let tag_target = AnimTarget::Tag(cube, tag_index);
    {
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
        app.doc
            .scene
            .get_mut(cube)
            .unwrap()
            .tag_animations
            .insert(tag_index as u32, clip);
    }
    assert!(app.set_anim_target(tag_target));
    for time in [0.0, 1.0] {
        assert!(app.select_target_track_key_additive(
            tag_target,
            CurveTrackId::Scalar(AnimProperty::CopyLocationWeight),
            time,
            true
        ));
    }
    app.history.clear();
    assert!(app.retime_selected_curve_keys(2.0));
    assert_eq!(app.history.depth(), 1);
    let tag_clip = app.target_clip(tag_target).unwrap();
    assert_eq!(
        tag_clip
            .track(AnimProperty::CopyLocationWeight)
            .unwrap()
            .curve
            .keys
            .iter()
            .map(|key| key.time)
            .collect::<Vec<_>>(),
        vec![0.0, 2.0]
    );
    app.undo();
    let tag_clip = app.target_clip(tag_target).unwrap();
    assert_eq!(
        tag_clip
            .track(AnimProperty::CopyLocationWeight)
            .unwrap()
            .curve
            .keys[1]
            .time,
        1.0
    );

    let material_id = app.selected_material.expect("demo material selection");
    {
        let material = app.doc.materials.get_mut(material_id).unwrap();
        material.animation = AnimationClip::default();
        material.animation.set_key(
            AnimProperty::MaterialRoughness,
            0.0,
            0.2,
            Interpolation::Linear,
        );
        material.animation.set_key(
            AnimProperty::MaterialRoughness,
            1.0,
            0.8,
            Interpolation::Linear,
        );
    }
    let material_target = AnimTarget::Material(material_id);
    assert!(app.set_anim_target(material_target));
    for time in [0.0, 1.0] {
        assert!(app.select_target_track_key_additive(
            material_target,
            CurveTrackId::Scalar(AnimProperty::MaterialRoughness),
            time,
            true
        ));
    }
    app.history.clear();
    assert!(app.retime_selected_curve_keys(2.0));
    assert_eq!(app.history.depth(), 1);
    let material_clip = app.target_clip(material_target).unwrap();
    assert_eq!(
        material_clip
            .track(AnimProperty::MaterialRoughness)
            .unwrap()
            .curve
            .keys[1]
            .time,
        2.0
    );
    app.undo();
    assert_eq!(
        app.target_clip(material_target)
            .unwrap()
            .track(AnimProperty::MaterialRoughness)
            .unwrap()
            .curve
            .keys[1]
            .time,
        1.0
    );
}

#[test]
fn take_retime_scales_every_channel_in_one_undo_step() {
    let (mut app, _root) = test_app("take-retime");
    let cube = named(&app, "Cube");
    app.selection.select_one(cube);
    {
        let node = app.doc.scene.get_mut(cube).unwrap();
        node.animation = AnimationClip::default();
        node.animation
            .set_key(AnimProperty::TranslationX, 1.0, 0.0, Interpolation::Smooth);
        node.animation
            .set_key(AnimProperty::TranslationX, 3.0, 1.0, Interpolation::Smooth);
        node.animation
            .set_morph_weight_key(0, 1.0, 0.0, Interpolation::Linear);
        node.animation
            .set_morph_weight_key(0, 3.0, 1.0, Interpolation::Linear);
        node.animation
            .set_rotation_key(1.0, Quat::IDENTITY, Interpolation::Linear);
        node.animation
            .set_rotation_key(3.0, Quat::from_rotation_y(1.0), Interpolation::Linear);
    }
    app.history.clear();
    assert!(app.retime_animation_take(cube, 2.0));
    assert_eq!(app.history.depth(), 1, "take retime is one undo step");
    let clip = app.doc.scene.get(cube).unwrap().animation.clone();
    assert_property_curve_times(
        &app,
        AnimTarget::Object(cube),
        AnimProperty::TranslationX,
        &[2.0, 6.0],
    );
    assert_eq!(clip.morph_weights.get(&0).unwrap().keys[1].time, 6.0);
    assert_eq!(clip.rotation.as_ref().unwrap().keys[1].time, 6.0);
    assert!(app.status.contains("Retimed take"));

    app.undo();
    assert_property_curve_times(
        &app,
        AnimTarget::Object(cube),
        AnimProperty::TranslationX,
        &[1.0, 3.0],
    );
    app.redo();
    assert_property_curve_times(
        &app,
        AnimTarget::Object(cube),
        AnimProperty::TranslationX,
        &[2.0, 6.0],
    );
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
