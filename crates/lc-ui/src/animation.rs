//! Timeline targets, F-curve keys, key selection, clipboard and retiming.

use super::*;

impl LeopardApp {
    /// Keys every selected object whose transform is not already keyed at the
    /// current frame.
    pub(super) fn key_transform_edits(&self) -> Vec<(NodeId, AnimationClip, AnimationClip)> {
        let time = self.playback.time;
        let mut edits = Vec::new();
        for id in self.selection.ids() {
            let Some(node) = self.doc.scene.get(*id) else {
                continue;
            };
            let before = node.animation.clone();
            let mut after = before.clone();
            after.record_transform(&node.transform, time, Interpolation::Smooth);
            if after != before {
                edits.push((*id, before, after));
            }
        }
        edits
    }

    /// Whether a target still names a live entity that owns animatable data.
    pub(super) fn target_is_valid(&self, target: AnimTarget) -> bool {
        match target {
            AnimTarget::Object(id) => self.doc.scene.get(id).is_some(),
            AnimTarget::Tag(node, index) => self
                .doc
                .scene
                .get(node)
                .and_then(|n| n.tags.get(index))
                .is_some_and(|tag| Self::tag_animation_channel(tag).is_some()),
            AnimTarget::Material(id) => self.doc.materials.get(id).is_some(),
        }
    }

    /// Map a scene tag to the registry channel that drives it.
    pub(super) fn tag_animation_channel(tag: &Tag) -> Option<AnimChannel> {
        match tag {
            Tag::CopyLocation { .. } => Some(AnimChannel::CopyLocationWeight),
            _ => None,
        }
    }

    /// The explicit F-curve target while it still follows the selection,
    /// otherwise the primary object. Tag targets follow their owning object;
    /// material targets are independent because materials have their own
    /// selection.
    pub(super) fn current_anim_target(&self) -> Option<AnimTarget> {
        if let Some(target) = self.anim_target {
            let follows_selection = match target {
                AnimTarget::Object(id) => self.selection.primary() == Some(id),
                AnimTarget::Tag(node, _) => self.selection.primary() == Some(node),
                AnimTarget::Material(_) => true,
            };
            if follows_selection && self.target_is_valid(target) {
                return Some(target);
            }
        }
        self.selection.primary().map(AnimTarget::Object)
    }

    /// Choose what the F-curve editor edits. Changing target drops key and row
    /// selections that belonged to the previous entity.
    pub(super) fn set_anim_target(&mut self, target: AnimTarget) -> bool {
        if !self.target_is_valid(target) {
            return false;
        }
        if self.anim_target != Some(target) {
            self.selected_keys.clear();
            self.curve_key_drag = None;
            self.curve_tangent_drag = None;
            self.curve_box_select = None;
            self.selected_curve_channel = None;
        }
        self.anim_target = Some(target);
        true
    }

    /// Current clip for a target. Tag targets without keys still yield an
    /// empty clip so the editor can show their registry rows and record into
    /// them; object and material targets require a live entity.
    pub(super) fn target_clip(&self, target: AnimTarget) -> Option<AnimationClip> {
        match target {
            AnimTarget::Object(id) => Some(self.doc.scene.get(id)?.animation.clone()),
            AnimTarget::Tag(node, index) => {
                let owner = self.doc.scene.get(node)?;
                let tag = owner.tags.get(index)?;
                Self::tag_animation_channel(tag)?;
                Some(
                    owner
                        .tag_animations
                        .get(&(index as u32))
                        .cloned()
                        .unwrap_or_default(),
                )
            }
            AnimTarget::Material(id) => Some(self.doc.materials.get(id)?.animation.clone()),
        }
    }

    pub(super) fn target_label(&self, target: AnimTarget) -> String {
        match target {
            AnimTarget::Object(id) => format!(
                "Object: {}",
                self.doc
                    .scene
                    .get(id)
                    .map(|n| n.name.as_str())
                    .unwrap_or("Object")
            ),
            AnimTarget::Tag(node, index) => {
                let tag_name = self
                    .doc
                    .scene
                    .get(node)
                    .and_then(|n| n.tags.get(index))
                    .map(|tag| tag.label())
                    .unwrap_or("Tag");
                format!("Tag: {tag_name} #{}", index + 1)
            }
            AnimTarget::Material(id) => format!(
                "Material: {}",
                self.doc
                    .materials
                    .get(id)
                    .map(|m| m.name.as_str())
                    .unwrap_or("Material")
            ),
        }
    }

    pub(super) fn channel_applies_to_target(
        &self,
        channel: AnimChannel,
        target: AnimTarget,
    ) -> bool {
        let scope_matches = match target {
            AnimTarget::Object(_) => channel.scope() == lc_anim::ChannelScope::Object,
            AnimTarget::Tag(..) => channel.scope() == lc_anim::ChannelScope::Tag,
            AnimTarget::Material(_) => channel.scope() == lc_anim::ChannelScope::Material,
        };
        if !scope_matches {
            return false;
        }
        let AnimTarget::Object(id) = target else {
            return true;
        };
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        match channel.applies_to() {
            ChannelAppliesTo::AnyObject => true,
            ChannelAppliesTo::Camera => matches!(node.kind, NodeKind::Camera { .. }),
            ChannelAppliesTo::Light => matches!(node.kind, NodeKind::Light { .. }),
            ChannelAppliesTo::Morph => {
                matches!(node.kind, NodeKind::Mesh { .. }) && !node.morph.targets.is_empty()
            }
            ChannelAppliesTo::Tag | ChannelAppliesTo::Material => false,
        }
    }

    /// Every registry channel offered for a target, expanded into one row per
    /// scalar component. Quaternion channels stay a single row, and a mesh with
    /// morph targets gets one dynamic row per target weight.
    pub(super) fn curve_rows(&self, target: AnimTarget) -> Vec<CurveRow> {
        let mut rows = Vec::new();
        for channel in AnimChannel::ALL {
            if channel == AnimChannel::MorphWeight
                || !self.channel_applies_to_target(channel, target)
            {
                continue;
            }
            match channel.kind() {
                ChannelKind::Quaternion => rows.push(CurveRow {
                    channel,
                    property: None,
                    morph_target: None,
                }),
                _ => {
                    for property in channel.scalar_properties() {
                        rows.push(CurveRow {
                            channel,
                            property: Some(*property),
                            morph_target: None,
                        });
                    }
                }
            }
        }
        if let AnimTarget::Object(id) = target {
            if let Some(node) = self.doc.scene.get(id) {
                if matches!(node.kind, NodeKind::Mesh { .. }) {
                    for index in 0..node.morph.targets.len() {
                        rows.push(CurveRow {
                            channel: AnimChannel::MorphWeight,
                            property: None,
                            morph_target: Some(index as u32),
                        });
                    }
                }
            }
        }
        rows
    }

    pub(super) fn curve_row_track(row: CurveRow) -> CurveTrackId {
        if let Some(index) = row.morph_target {
            CurveTrackId::MorphWeight(index)
        } else if let Some(property) = row.property {
            CurveTrackId::Scalar(property)
        } else {
            CurveTrackId::Rotation
        }
    }

    pub(super) fn curve_row_label(&self, target: AnimTarget, row: CurveRow) -> String {
        if let Some(index) = row.morph_target {
            let name = match target {
                AnimTarget::Object(id) => self
                    .doc
                    .scene
                    .get(id)
                    .and_then(|node| node.morph.targets.get(index as usize))
                    .map(|target| target.name.as_str())
                    .unwrap_or("Morph"),
                _ => "Morph",
            };
            return format!("{name} Weight");
        }
        match row.property {
            Some(property) => {
                AnimChannel::scalar_label(property).unwrap_or_else(|| property.label().to_string())
            }
            None => row.channel.label().to_string(),
        }
    }

    /// Live values for a registry channel, in registry component order.
    pub(super) fn channel_values(
        &self,
        target: AnimTarget,
        channel: AnimChannel,
    ) -> Option<Vec<f32>> {
        match (target, channel) {
            (AnimTarget::Object(id), AnimChannel::Translation) => {
                let node = self.doc.scene.get(id)?;
                Some(node.transform.translation.to_array().to_vec())
            }
            (AnimTarget::Object(id), AnimChannel::Scale) => {
                let node = self.doc.scene.get(id)?;
                Some(node.transform.scale.to_array().to_vec())
            }
            (AnimTarget::Object(id), AnimChannel::Fov) => match &self.doc.scene.get(id)?.kind {
                NodeKind::Camera { camera } => Some(vec![camera.fov_y_deg]),
                _ => None,
            },
            (AnimTarget::Object(id), AnimChannel::LightIntensity) => {
                match &self.doc.scene.get(id)?.kind {
                    NodeKind::Light { light } => Some(vec![light.intensity]),
                    _ => None,
                }
            }
            (AnimTarget::Object(id), AnimChannel::LightColor) => {
                match &self.doc.scene.get(id)?.kind {
                    NodeKind::Light { light } => Some(light.color.to_array().to_vec()),
                    _ => None,
                }
            }
            (AnimTarget::Tag(node, index), AnimChannel::CopyLocationWeight) => {
                match self.doc.scene.get(node)?.tags.get(index)? {
                    Tag::CopyLocation { weight, .. } => Some(vec![*weight]),
                    _ => None,
                }
            }
            (AnimTarget::Material(id), AnimChannel::MaterialRoughness) => Some(vec![self
                .doc
                .materials
                .get(id)?
                .base_roughness(&self.texture_cache)]),
            (AnimTarget::Material(id), AnimChannel::MaterialMetallic) => Some(vec![self
                .doc
                .materials
                .get(id)?
                .base_metallic(&self.texture_cache)]),
            _ => None,
        }
    }

    pub(super) fn morph_weight_value(&self, target: AnimTarget, index: u32) -> Option<f32> {
        let AnimTarget::Object(id) = target else {
            return None;
        };
        self.doc
            .scene
            .get(id)?
            .morph
            .targets
            .get(index as usize)
            .map(|target| target.weight)
    }

    /// Record one registry row at the playhead. Morph-weight rows need their
    /// target index in addition to the channel, so the F-curve editor uses this
    /// entry point for both dynamic and ordinary rows.
    pub(super) fn record_curve_row_key(&mut self, target: AnimTarget, row: CurveRow) -> bool {
        if row.channel == AnimChannel::MorphWeight {
            let Some(index) = row.morph_target else {
                return false;
            };
            if !self.target_is_valid(target) || !self.channel_applies_to_target(row.channel, target)
            {
                return false;
            }
            let Some(value) = self.morph_weight_value(target, index) else {
                return false;
            };
            let time = self.playback.time;
            let Some(mut after) = self.target_clip(target) else {
                return false;
            };
            after.set_morph_weight_key(index, time, value, Interpolation::Smooth);
            if !self.commit_target_clip(target, after) {
                return false;
            }
            self.anim_target = Some(target);
            self.selected_curve_channel = Some((target, row));
            self.status = format!(
                "Keyed {} at frame {}",
                self.curve_row_label(target, row),
                (time * self.playback.fps).round()
            );
            return true;
        }
        if self.record_channel_key(target, row.channel) {
            self.selected_curve_channel = Some((target, row));
            true
        } else {
            false
        }
    }

    /// Record one registry channel at the playhead as one undoable command.
    pub(super) fn record_channel_key(&mut self, target: AnimTarget, channel: AnimChannel) -> bool {
        if !self.target_is_valid(target) || !self.channel_applies_to_target(channel, target) {
            return false;
        }
        let time = self.playback.time;
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        if channel.kind() == ChannelKind::Quaternion {
            let AnimTarget::Object(id) = target else {
                return false;
            };
            let Some(node) = self.doc.scene.get(id) else {
                return false;
            };
            after.set_rotation_key(time, node.transform.rotation, Interpolation::Smooth);
        } else {
            let Some(values) = self.channel_values(target, channel) else {
                return false;
            };
            if !after.record_channel(channel, &values, time, Interpolation::Smooth) {
                return false;
            }
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.anim_target = Some(target);
        self.selected_curve_channel = Some((
            target,
            CurveRow {
                channel,
                property: channel.scalar_properties().first().copied(),
                morph_target: None,
            },
        ));
        self.status = format!(
            "Keyed {} at frame {}",
            channel.label(),
            (time * self.playback.fps).round()
        );
        true
    }

    /// Build the single command that writes a clip to the entity that owns it.
    pub(super) fn target_clip_command(
        &self,
        target: AnimTarget,
        after: AnimationClip,
    ) -> Option<Box<dyn Command>> {
        match target {
            AnimTarget::Object(id) => Some(Box::new(SetAnimationCmd::new(&self.doc, id, after)?)),
            AnimTarget::Tag(node, index) => Some(Box::new(SetTagAnimationCmd::new(
                &self.doc, node, index, after,
            )?)),
            AnimTarget::Material(id) => {
                let before = self.doc.materials.get(id).cloned()?;
                let mut updated = before.clone();
                updated.animation = after;
                if updated == before {
                    return None;
                }
                Some(Box::new(SetMaterialCmd {
                    before,
                    after: updated,
                }))
            }
        }
    }

    /// Commit a clip to the entity that owns it as exactly one undo step.
    pub(super) fn commit_target_clip(&mut self, target: AnimTarget, after: AnimationClip) -> bool {
        let Some(cmd) = self.target_clip_command(target, after) else {
            return false;
        };
        self.history.execute(&mut self.doc, cmd);
        true
    }

    /// Commit a paste and carry its before/after key selection on the same
    /// history entry. Undo and redo can then restore the editor selection
    /// without a second history stack.
    pub(super) fn commit_pasted_target_clip(
        &mut self,
        target: AnimTarget,
        after: AnimationClip,
        before_selection: Vec<SelectedKey>,
        after_selection: Vec<SelectedKey>,
    ) -> bool {
        let Some(inner) = self.target_clip_command(target, after) else {
            return false;
        };
        self.history.execute(
            &mut self.doc,
            Box::new(PasteCurveKeysCmd {
                inner,
                target,
                before_selection,
                after_selection,
            }),
        );
        true
    }

    /// Index of the key at `time` on `property`, using the editor's key
    /// tolerance so selection survives frame snapping.
    pub(super) fn curve_key_index(
        clip: &AnimationClip,
        property: AnimProperty,
        time: f32,
    ) -> Option<usize> {
        let track = clip.track(property)?;
        track
            .curve
            .keys
            .iter()
            .position(|key| (key.time - time).abs() <= KEY_TIME_EPSILON)
    }

    pub(super) fn morph_curve_key_index(
        clip: &AnimationClip,
        target: u32,
        time: f32,
    ) -> Option<usize> {
        clip.morph_weights
            .get(&target)?
            .keys
            .iter()
            .position(|key| (key.time - time).abs() <= KEY_TIME_EPSILON)
    }

    /// Select the F-curve key at `time` on the primary object.
    #[cfg(test)]
    pub(super) fn select_curve_key(&mut self, property: AnimProperty, time: f32) -> bool {
        self.select_track_key(CurveTrackId::Scalar(property), time)
    }

    /// Select one quaternion rotation key on the primary object.
    #[cfg(test)]
    pub(super) fn select_rotation_key(&mut self, time: f32) -> bool {
        self.select_track_key(CurveTrackId::Rotation, time)
    }

    #[cfg(test)]
    pub(super) fn select_track_key(&mut self, track: CurveTrackId, time: f32) -> bool {
        let Some(target) = self.current_anim_target() else {
            return false;
        };
        self.select_target_track_key(target, track, time)
    }

    /// Select a key on an explicit object, tag or material target.
    #[cfg(test)]
    pub(super) fn select_target_track_key(
        &mut self,
        target: AnimTarget,
        track: CurveTrackId,
        time: f32,
    ) -> bool {
        self.select_target_track_key_additive(target, track, time, false)
    }

    /// Add or replace one F-curve key in the current selection.
    pub(super) fn select_target_track_key_additive(
        &mut self,
        target: AnimTarget,
        track: CurveTrackId,
        time: f32,
        additive: bool,
    ) -> bool {
        let Some(clip) = self.target_clip(target) else {
            return false;
        };
        let key = SelectedKey {
            target,
            track,
            time,
        };
        if !Self::curve_key_exists(&clip, &key) {
            return false;
        }
        if !additive {
            self.selected_keys.clear();
        }
        if !self
            .selected_keys
            .iter()
            .any(|existing| Self::same_curve_key(*existing, key))
        {
            self.selected_keys.push(key);
        }
        true
    }

    /// Toggle a key in the selection for shift/command clicks, matching the
    /// object selection semantics.
    pub(super) fn toggle_target_track_key(
        &mut self,
        target: AnimTarget,
        track: CurveTrackId,
        time: f32,
    ) -> bool {
        let Some(clip) = self.target_clip(target) else {
            return false;
        };
        let key = SelectedKey {
            target,
            track,
            time,
        };
        if !Self::curve_key_exists(&clip, &key) {
            return false;
        }
        if let Some(index) = self
            .selected_keys
            .iter()
            .position(|existing| Self::same_curve_key(*existing, key))
        {
            self.selected_keys.remove(index);
        } else {
            self.selected_keys.push(key);
        }
        true
    }

    pub(super) fn same_curve_key(a: SelectedKey, b: SelectedKey) -> bool {
        a.target == b.target && a.track == b.track && (a.time - b.time).abs() <= KEY_TIME_EPSILON
    }

    pub(super) fn curve_key_exists(clip: &AnimationClip, key: &SelectedKey) -> bool {
        match key.track {
            CurveTrackId::Scalar(property) => {
                Self::curve_key_index(clip, property, key.time).is_some()
            }
            CurveTrackId::Rotation => clip.rotation_key_index(key.time).is_some(),
            CurveTrackId::MorphWeight(target) => {
                Self::morph_curve_key_index(clip, target, key.time).is_some()
            }
        }
    }

    /// All selected keys that still exist on the current target.
    pub(super) fn selected_curve_keys(&self) -> Vec<SelectedKey> {
        let Some(target) = self.current_anim_target() else {
            return Vec::new();
        };
        let Some(clip) = self.target_clip(target) else {
            return Vec::new();
        };
        let mut keys = Vec::new();
        for key in &self.selected_keys {
            if key.target != target || !Self::curve_key_exists(&clip, key) {
                continue;
            }
            if !keys
                .iter()
                .any(|existing| Self::same_curve_key(*existing, *key))
            {
                keys.push(*key);
            }
        }
        keys
    }

    /// The last selected key, used by single-key controls and tests.
    #[cfg(test)]
    pub(super) fn selected_curve_key(&self) -> Option<SelectedKey> {
        self.selected_curve_keys().last().copied()
    }

    pub(super) fn is_curve_key_selected(&self, key: &SelectedKey) -> bool {
        self.selected_keys
            .iter()
            .any(|existing| Self::same_curve_key(*existing, *key))
    }

    pub(super) fn clear_curve_key_selection(&mut self) {
        self.selected_keys.clear();
    }

    /// Select every scalar or quaternion key whose frame and displayed value
    /// fall inside the marquee. Additive selection keeps what was already
    /// selected, matching shift/command selection elsewhere in the editor.
    pub(super) fn select_keys_in_box(
        &mut self,
        target: AnimTarget,
        clip: &AnimationClip,
        time_range: (f32, f32),
        value_range: (f32, f32),
        additive: bool,
    ) -> bool {
        let (time_min, time_max) = time_range;
        let (value_min, value_max) = value_range;
        if !time_min.is_finite()
            || !time_max.is_finite()
            || !value_min.is_finite()
            || !value_max.is_finite()
        {
            return false;
        }
        let mut hits = Vec::new();
        for track in &clip.tracks {
            for key in &track.curve.keys {
                if key.time >= time_min
                    && key.time <= time_max
                    && key.value >= value_min
                    && key.value <= value_max
                {
                    hits.push(SelectedKey {
                        target,
                        track: CurveTrackId::Scalar(track.property),
                        time: key.time,
                    });
                }
            }
        }
        if let Some(track) = &clip.rotation {
            for key in &track.keys {
                if key.time >= time_min
                    && key.time <= time_max
                    && key.value.w >= value_min
                    && key.value.w <= value_max
                {
                    hits.push(SelectedKey {
                        target,
                        track: CurveTrackId::Rotation,
                        time: key.time,
                    });
                }
            }
        }
        for (target_index, curve) in &clip.morph_weights {
            for key in &curve.keys {
                if key.time >= time_min
                    && key.time <= time_max
                    && key.value >= value_min
                    && key.value <= value_max
                {
                    hits.push(SelectedKey {
                        target,
                        track: CurveTrackId::MorphWeight(*target_index),
                        time: key.time,
                    });
                }
            }
        }
        if !additive {
            self.selected_keys.clear();
        }
        for hit in hits {
            if !self
                .selected_keys
                .iter()
                .any(|existing| Self::same_curve_key(*existing, hit))
            {
                self.selected_keys.push(hit);
            }
        }
        !self.selected_keys.is_empty()
    }

    /// Move one scalar key on any object, tag or material target in time as a
    /// single undo step.
    #[cfg(test)]
    pub(super) fn move_target_curve_key(
        &mut self,
        target: AnimTarget,
        property: AnimProperty,
        from: f32,
        to: f32,
    ) -> bool {
        if !to.is_finite() {
            return false;
        }
        let to = to.max(0.0);
        let Some(before) = self.target_clip(target) else {
            return false;
        };
        let mut after = before.clone();
        let Some(track) = after.track_mut(property) else {
            return false;
        };
        let Some(index) = track
            .curve
            .keys
            .iter()
            .position(|key| (key.time - from).abs() <= KEY_TIME_EPSILON)
        else {
            return false;
        };
        if (to - from).abs() <= 1e-4 {
            return false;
        }
        if track
            .curve
            .keys
            .iter()
            .enumerate()
            .any(|(i, key)| i != index && (key.time - to).abs() <= KEY_TIME_EPSILON)
        {
            self.status = "Another key already sits at that frame".into();
            return false;
        }
        track.curve.keys[index].time = to;
        track.curve.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        track.curve.update_tangents();
        if after == before {
            return false;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.selected_keys = vec![SelectedKey {
            target,
            track: CurveTrackId::Scalar(property),
            time: to,
        }];
        self.status = format!("Moved key to frame {}", (to * self.playback.fps).round());
        true
    }

    /// Delete one scalar key on any target as a single undo step, dropping the
    /// track when its last key goes away.
    pub(super) fn delete_target_curve_key(
        &mut self,
        target: AnimTarget,
        property: AnimProperty,
        time: f32,
    ) -> bool {
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let Some(track) = after.track_mut(property) else {
            return false;
        };
        let Some(index) = track
            .curve
            .keys
            .iter()
            .position(|key| (key.time - time).abs() <= KEY_TIME_EPSILON)
        else {
            return false;
        };
        track.curve.keys.remove(index);
        after.tracks.retain(|track| !track.curve.keys.is_empty());
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.selected_keys.clear();
        self.status = "Deleted key".into();
        true
    }

    /// Change interpolation for exactly one scalar key on any target.
    #[cfg(test)]
    pub(super) fn set_target_curve_key_interpolation(
        &mut self,
        target: AnimTarget,
        property: AnimProperty,
        time: f32,
        interpolation: Interpolation,
    ) -> bool {
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let Some(track) = after.track_mut(property) else {
            return false;
        };
        let Some(index) = track
            .curve
            .keys
            .iter()
            .position(|key| (key.time - time).abs() <= KEY_TIME_EPSILON)
        else {
            return false;
        };
        if track.curve.keys[index].interpolation == interpolation {
            // Reselecting Smooth can still be a real edit when a legacy key
            // retains manual slopes; let the curve normalize it.
            if interpolation != Interpolation::Smooth {
                return false;
            }
        }
        if !track.curve.set_interpolation(time, interpolation) {
            return false;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.selected_keys = vec![SelectedKey {
            target,
            track: CurveTrackId::Scalar(property),
            time,
        }];
        self.status = format!("Set key interpolation to {}", interpolation.label());
        true
    }

    /// Apply an interpolation change from one channel row. When keys in that
    /// row are selected, only those keys change; otherwise the entire row
    /// changes, matching the old per-row control's intent while making the edit
    /// undoable and applying the same tangent rules as the selection commands.
    pub(super) fn set_curve_row_interpolation(
        &mut self,
        target: AnimTarget,
        row: CurveRow,
        interpolation: Interpolation,
    ) -> bool {
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let track_id = Self::curve_row_track(row);
        let selected_times: Vec<f32> = self
            .selected_curve_keys()
            .into_iter()
            .filter(|key| key.target == target && key.track == track_id)
            .map(|key| key.time)
            .collect();
        let mut changed = 0;
        if let Some(property) = row.property {
            if let Some(track) = after.track_mut(property) {
                let times: Vec<f32> = track
                    .curve
                    .keys
                    .iter()
                    .filter(|key| {
                        selected_times.is_empty()
                            || selected_times
                                .iter()
                                .any(|time| (key.time - time).abs() <= KEY_TIME_EPSILON)
                    })
                    .map(|key| key.time)
                    .collect();
                for time in times {
                    if track.curve.set_interpolation(time, interpolation) {
                        changed += 1;
                    }
                }
            }
        } else if let Some(index) = row.morph_target {
            if let Some(curve) = after.morph_weights.get_mut(&index) {
                let times: Vec<f32> = curve
                    .keys
                    .iter()
                    .filter(|key| {
                        selected_times.is_empty()
                            || selected_times
                                .iter()
                                .any(|time| (key.time - time).abs() <= KEY_TIME_EPSILON)
                    })
                    .map(|key| key.time)
                    .collect();
                for time in times {
                    if curve.set_interpolation(time, interpolation) {
                        changed += 1;
                    }
                }
            }
        } else if let Some(track) = after.rotation.as_mut() {
            let times: Vec<f32> = track
                .keys
                .iter()
                .filter(|key| {
                    selected_times.is_empty()
                        || selected_times
                            .iter()
                            .any(|time| (key.time - time).abs() <= KEY_TIME_EPSILON)
                })
                .map(|key| key.time)
                .collect();
            for time in times {
                if track.set_interpolation(time, interpolation) {
                    changed += 1;
                }
            }
        }
        if changed == 0 {
            return false;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.status = format!("Set key interpolation to {}", interpolation.label());
        true
    }

    pub(super) fn delete_target_morph_weight_curve_key(
        &mut self,
        target: AnimTarget,
        target_index: u32,
        time: f32,
    ) -> bool {
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let Some(index) = Self::morph_curve_key_index(&after, target_index, time) else {
            return false;
        };
        let Some(curve) = after.morph_weights.get_mut(&target_index) else {
            return false;
        };
        curve.keys.remove(index);
        after
            .morph_weights
            .retain(|_, curve| !curve.keys.is_empty());
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.selected_keys.clear();
        self.status = "Deleted key".into();
        true
    }

    pub(super) fn delete_target_rotation_curve_key(
        &mut self,
        target: AnimTarget,
        time: f32,
    ) -> bool {
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let Some(track) = after.rotation.as_mut() else {
            return false;
        };
        if track.key_index(time).is_none() {
            return false;
        }
        track.remove_nearest(time);
        if track.is_empty() {
            after.rotation = None;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.selected_keys.clear();
        self.status = "Deleted key".into();
        true
    }

    /// Retime the selected F-curve keys around their earliest selected time.
    ///
    /// The pivot is the earliest selected key, so the selection's leading edge
    /// stays fixed and later keys move toward or away from it. Times are not
    /// frame-snapped. Positive factors are required; a collision with an
    /// unselected key or an implied negative time rejects the whole edit without
    /// changing the document. Auto tangents are re-derived and manual tangents
    /// are rescaled per incident segment by the anim crate. One target commit
    /// makes the retime one undo step.
    pub(super) fn retime_selected_curve_keys(&mut self, factor: f32) -> bool {
        if !factor.is_finite() || factor <= 0.0 || (factor - 1.0).abs() <= 1e-6 {
            self.status = "Enter a positive retime factor other than 1".into();
            return false;
        }
        let keys = self.selected_curve_keys();
        let Some(first) = keys.first() else {
            self.status = "Select F-curve keys to retime".into();
            return false;
        };
        let target = first.target;
        if keys.iter().any(|key| key.target != target) {
            return false;
        }
        let pivot = keys
            .iter()
            .map(|key| key.time)
            .fold(f32::INFINITY, f32::min);
        let Some(before) = self.target_clip(target) else {
            return false;
        };
        let mut after = before.clone();
        let mut scalar_tracks: Vec<(AnimProperty, Vec<f32>)> = Vec::new();
        let mut morph_tracks: Vec<(u32, Vec<f32>)> = Vec::new();
        let mut rotation_times = Vec::new();
        for key in &keys {
            match key.track {
                CurveTrackId::Scalar(property) => {
                    if let Some((_, times)) = scalar_tracks
                        .iter_mut()
                        .find(|(candidate, _)| *candidate == property)
                    {
                        times.push(key.time);
                    } else {
                        scalar_tracks.push((property, vec![key.time]));
                    }
                }
                CurveTrackId::Rotation => rotation_times.push(key.time),
                CurveTrackId::MorphWeight(target) => {
                    if let Some((_, times)) = morph_tracks
                        .iter_mut()
                        .find(|(candidate, _)| *candidate == target)
                    {
                        times.push(key.time);
                    } else {
                        morph_tracks.push((target, vec![key.time]));
                    }
                }
            }
        }
        for (property, times) in &scalar_tracks {
            let Some(track) = after.track_mut(*property) else {
                return false;
            };
            if !track.curve.retime_selected_keys(pivot, factor, times) {
                self.status = "Retime would collide with another key or leave the timeline".into();
                return false;
            }
        }
        if !rotation_times.is_empty() {
            let Some(track) = after.rotation.as_mut() else {
                return false;
            };
            if !track.retime_selected_keys(pivot, factor, &rotation_times) {
                self.status = "Retime would collide with another key or leave the timeline".into();
                return false;
            }
        }
        for (target_index, times) in &morph_tracks {
            let Some(curve) = after.morph_weights.get_mut(target_index) else {
                return false;
            };
            if !curve.retime_selected_keys(pivot, factor, times) {
                self.status = "Retime would collide with another key or leave the timeline".into();
                return false;
            }
        }
        if after == before {
            self.status = "Those keys already have the requested timing".into();
            return false;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.selected_keys = keys
            .into_iter()
            .map(|mut key| {
                key.time = pivot + (key.time - pivot) * factor;
                key
            })
            .collect();
        self.status = format!("Retimed {} key(s) by ×{factor}", self.selected_keys.len());
        true
    }

    /// Retime every key in one object's take around zero seconds. The take
    /// manager uses this path; it is one `SetAnimationCmd` undo step and covers
    /// scalar, morph-weight and quaternion rotation channels. Tag and material
    /// clips are not part of a per-object take; retime their selected keys from
    /// the F-curve editor instead.
    pub(super) fn retime_animation_take(&mut self, id: NodeId, factor: f32) -> bool {
        if !factor.is_finite() || factor <= 0.0 || (factor - 1.0).abs() <= 1e-6 {
            self.status = "Enter a positive retime factor other than 1".into();
            return false;
        }
        let Some(node) = self.doc.scene.get(id) else {
            return false;
        };
        let name = node.name.clone();
        let before = node.animation.clone();
        if before.is_empty() {
            self.status = "This take has no keys to retime".into();
            return false;
        }
        let mut after = before.clone();
        if !after.retime_all(0.0, factor) {
            self.status = "Retime would collide with another key or leave the timeline".into();
            return false;
        }
        if after == before {
            self.status = "That factor does not change any key times in this take".into();
            return false;
        }
        let key_count = after
            .tracks
            .iter()
            .map(|track| track.curve.keys.len())
            .sum::<usize>()
            + after.rotation.as_ref().map_or(0, |track| track.keys.len())
            + after
                .morph_weights
                .values()
                .map(|curve| curve.keys.len())
                .sum::<usize>();
        let Some(command) = SetAnimationCmd::new(&self.doc, id, after) else {
            return false;
        };
        self.history.execute(&mut self.doc, Box::new(command));
        for key in &mut self.selected_keys {
            if key.target == AnimTarget::Object(id) {
                key.time *= factor;
            }
        }
        self.status = format!("Retimed take {name} by ×{factor} ({key_count} keys)");
        true
    }

    /// Move every selected key by one frame delta as a single undo step.
    pub(super) fn move_selected_curve_keys_by(&mut self, delta: f32) -> bool {
        if !delta.is_finite() || delta.abs() <= 1e-4 {
            return false;
        }
        let keys = self.selected_curve_keys();
        if keys.is_empty() {
            return false;
        }
        let target = keys[0].target;
        if keys.iter().any(|key| key.target != target) {
            return false;
        }
        let Some(before) = self.target_clip(target) else {
            return false;
        };
        // Clamp the whole selection at frame zero so spacing is preserved even
        // when the drag would push the earliest key before the timeline start.
        let earliest = keys
            .iter()
            .map(|key| key.time)
            .fold(f32::INFINITY, f32::min);
        let delta = delta.max(-earliest);
        if delta.abs() <= 1e-4 {
            return false;
        }
        let mut after = before.clone();
        let mut moved = Vec::with_capacity(keys.len());
        for key in &keys {
            moved.push((*key, key.time + delta));
        }
        for (key, new_time) in &moved {
            let selected_times: Vec<f32> = keys
                .iter()
                .filter(|other| other.track == key.track)
                .map(|other| other.time)
                .collect();
            let collides = match key.track {
                CurveTrackId::Scalar(property) => after.track(property).is_some_and(|track| {
                    track.curve.keys.iter().any(|candidate| {
                        (candidate.time - *new_time).abs() <= KEY_TIME_EPSILON
                            && !selected_times
                                .iter()
                                .any(|time| (candidate.time - time).abs() <= KEY_TIME_EPSILON)
                    })
                }),
                CurveTrackId::Rotation => after.rotation.as_ref().is_some_and(|track| {
                    track.keys.iter().any(|candidate| {
                        (candidate.time - *new_time).abs() <= KEY_TIME_EPSILON
                            && !selected_times
                                .iter()
                                .any(|time| (candidate.time - time).abs() <= KEY_TIME_EPSILON)
                    })
                }),
                CurveTrackId::MorphWeight(target) => {
                    after.morph_weights.get(&target).is_some_and(|curve| {
                        curve.keys.iter().any(|candidate| {
                            (candidate.time - *new_time).abs() <= KEY_TIME_EPSILON
                                && !selected_times
                                    .iter()
                                    .any(|time| (candidate.time - time).abs() <= KEY_TIME_EPSILON)
                        })
                    })
                }
            };
            if collides {
                self.status = "Another key already sits at that frame".into();
                return false;
            }
        }
        for (index, (key, new_time)) in moved.iter().enumerate() {
            for (other, other_time) in moved.iter().skip(index + 1) {
                if key.track == other.track && (*new_time - *other_time).abs() <= KEY_TIME_EPSILON {
                    self.status = "Two selected keys would land on the same frame".into();
                    return false;
                }
            }
        }
        for (key, new_time) in &moved {
            match key.track {
                CurveTrackId::Scalar(property) => {
                    let Some(track) = after.track_mut(property) else {
                        return false;
                    };
                    let Some(index) = track.curve.keys.iter().position(|candidate| {
                        (candidate.time - key.time).abs() <= KEY_TIME_EPSILON
                    }) else {
                        return false;
                    };
                    track.curve.keys[index].time = *new_time;
                }
                CurveTrackId::Rotation => {
                    let Some(track) = after.rotation.as_mut() else {
                        return false;
                    };
                    let Some(index) = track.key_index(key.time) else {
                        return false;
                    };
                    track.keys[index].time = *new_time;
                }
                CurveTrackId::MorphWeight(target) => {
                    let Some(curve) = after.morph_weights.get_mut(&target) else {
                        return false;
                    };
                    let Some(index) = curve.keys.iter().position(|candidate| {
                        (candidate.time - key.time).abs() <= KEY_TIME_EPSILON
                    }) else {
                        return false;
                    };
                    curve.keys[index].time = *new_time;
                }
            }
        }
        for track in &mut after.tracks {
            track.curve.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            track.curve.update_tangents();
        }
        if let Some(track) = after.rotation.as_mut() {
            track.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        }
        for curve in after.morph_weights.values_mut() {
            curve.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            curve.update_tangents();
        }
        if after == before {
            return false;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.selected_keys = moved
            .into_iter()
            .map(|(mut key, time)| {
                key.time = time;
                key
            })
            .collect();
        self.status = format!(
            "Moved {} key(s) by {} frame(s)",
            self.selected_keys.len(),
            (delta * self.playback.fps).round()
        );
        true
    }

    /// Move the selected anchor key to `to`, carrying the whole multi-key
    /// selection with it.
    #[cfg(test)]
    pub(super) fn move_selected_curve_key(&mut self, to: f32) -> bool {
        let Some(key) = self.selected_curve_key() else {
            return false;
        };
        self.move_selected_curve_keys_by(to - key.time)
    }

    /// Set one hand-authored Bezier tangent slope for a scalar or morph key.
    /// One drag becomes one undo step; the editor keeps the other side of the
    /// key exactly as it was.
    pub(super) fn set_target_curve_tangent(
        &mut self,
        target: AnimTarget,
        track: CurveTrackId,
        time: f32,
        incoming: bool,
        slope: f32,
    ) -> bool {
        if !slope.is_finite() {
            return false;
        }
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let changed = match track {
            CurveTrackId::Scalar(property) => {
                let Some(track) = after.track_mut(property) else {
                    return false;
                };
                let Some(index) = track
                    .curve
                    .keys
                    .iter()
                    .position(|key| (key.time - time).abs() <= KEY_TIME_EPSILON)
                else {
                    return false;
                };
                let key = &mut track.curve.keys[index];
                let before = (key.tangent_in, key.tangent_out, key.tangent_manual);
                if incoming {
                    key.tangent_in = slope;
                    key.tangent_in_type = TangentType::Spline;
                } else {
                    key.tangent_out = slope;
                    key.tangent_out_type = TangentType::Spline;
                }
                key.tangent_manual = true;
                before != (key.tangent_in, key.tangent_out, key.tangent_manual)
            }
            CurveTrackId::MorphWeight(target_index) => {
                let Some(curve) = after.morph_weights.get_mut(&target_index) else {
                    return false;
                };
                let Some(index) = curve
                    .keys
                    .iter()
                    .position(|key| (key.time - time).abs() <= KEY_TIME_EPSILON)
                else {
                    return false;
                };
                let key = &mut curve.keys[index];
                let before = (key.tangent_in, key.tangent_out, key.tangent_manual);
                if incoming {
                    key.tangent_in = slope;
                    key.tangent_in_type = TangentType::Spline;
                } else {
                    key.tangent_out = slope;
                    key.tangent_out_type = TangentType::Spline;
                }
                key.tangent_manual = true;
                before != (key.tangent_in, key.tangent_out, key.tangent_manual)
            }
            CurveTrackId::Rotation => return false,
        };
        if !changed {
            return false;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.status = "Adjusted tangent".into();
        true
    }

    /// Delete every selected key as a single undo step. A track disappears
    /// when its last key is removed, and a quaternion track is dropped with it.
    pub(super) fn delete_selected_curve_key(&mut self) -> bool {
        let keys = self.selected_curve_keys();
        if keys.is_empty() {
            return false;
        }
        let target = keys[0].target;
        if keys.iter().any(|key| key.target != target) {
            return false;
        }
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let count = keys.len();
        for key in &keys {
            match key.track {
                CurveTrackId::Scalar(property) => {
                    if let Some(track) = after.track_mut(property) {
                        track.curve.remove_nearest(key.time);
                    }
                }
                CurveTrackId::Rotation => {
                    if let Some(track) = after.rotation.as_mut() {
                        track.remove_nearest(key.time);
                        if track.is_empty() {
                            after.rotation = None;
                        }
                    }
                }
                CurveTrackId::MorphWeight(target) => {
                    if let Some(curve) = after.morph_weights.get_mut(&target) {
                        curve.remove_nearest(key.time);
                    }
                }
            }
        }
        after.tracks.retain(|track| !track.curve.keys.is_empty());
        after
            .morph_weights
            .retain(|_, curve| !curve.keys.is_empty());
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.selected_keys.clear();
        self.status = if count == 1 {
            "Deleted key".into()
        } else {
            format!("Deleted {count} keys")
        };
        true
    }

    /// Set interpolation for every selected key as one undo step. The anim
    /// crate applies the tangent-ownership rule: Smooth replaces hand-authored
    /// slopes with automatic ones, while Spline preserves them. Scalar, morph
    /// and quaternion keys all pass through this same selection.
    pub(super) fn set_selected_curve_key_interpolation(
        &mut self,
        interpolation: Interpolation,
    ) -> bool {
        let keys = self.selected_curve_keys();
        if keys.is_empty() {
            return false;
        }
        let target = keys[0].target;
        if keys.iter().any(|key| key.target != target) {
            return false;
        }
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let mut changed = 0;
        for key in &keys {
            match key.track {
                CurveTrackId::Scalar(property) => {
                    if let Some(track) = after.track_mut(property) {
                        if track.curve.set_interpolation(key.time, interpolation) {
                            changed += 1;
                        }
                    }
                }
                CurveTrackId::Rotation => {
                    if let Some(track) = after.rotation.as_mut() {
                        if track.set_interpolation(key.time, interpolation) {
                            changed += 1;
                        }
                    }
                }
                CurveTrackId::MorphWeight(target) => {
                    if let Some(curve) = after.morph_weights.get_mut(&target) {
                        if curve.set_interpolation(key.time, interpolation) {
                            changed += 1;
                        }
                    }
                }
            }
        }
        if changed == 0 {
            return false;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.status = format!("Set key interpolation to {}", interpolation.label());
        true
    }

    /// Set the same tangent derivation rule on the selected keys. `Tangent >
    /// Type` passes both sides; the left/right menu variants set only one. The
    /// whole selection is committed once, so types and overwritten slopes undo
    /// together.
    pub(super) fn set_selected_curve_tangent_type(
        &mut self,
        tangent_type: TangentType,
        incoming: bool,
        outgoing: bool,
    ) -> bool {
        let keys = self.selected_curve_keys();
        if keys.is_empty() {
            self.status = "Select F-curve keys to change their tangent type".into();
            return false;
        }
        let target = keys[0].target;
        if keys.iter().any(|key| key.target != target) {
            return false;
        }
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let mut changed = 0;
        for key in &keys {
            match key.track {
                CurveTrackId::Scalar(property) => {
                    if let Some(track) = after.track_mut(property) {
                        if track
                            .curve
                            .set_tangent_type(key.time, tangent_type, incoming, outgoing)
                        {
                            changed += 1;
                        }
                    }
                }
                CurveTrackId::Rotation => {
                    if let Some(track) = after.rotation.as_mut() {
                        if track.set_tangent_type(key.time, tangent_type, incoming, outgoing) {
                            changed += 1;
                        }
                    }
                }
                CurveTrackId::MorphWeight(target) => {
                    if let Some(curve) = after.morph_weights.get_mut(&target) {
                        if curve.set_tangent_type(key.time, tangent_type, incoming, outgoing) {
                            changed += 1;
                        }
                    }
                }
            }
        }
        if changed == 0 {
            self.status = "Those keys already use that tangent type".into();
            return false;
        }
        if !self.commit_target_clip(target, after) {
            return false;
        }
        self.status = format!("Set tangent type to {}", tangent_type.label());
        true
    }

    /// The common interpolation of the current key selection, or `None` for a
    /// mixed selection. The toolbar uses this to show `Mixed` rather than
    /// implying that the first key speaks for the rest.
    pub(super) fn selected_curve_interpolation(&self) -> Option<Interpolation> {
        let keys = self.selected_curve_keys();
        let clip = self.target_clip(self.current_anim_target()?)?;
        let mut common = None;
        for key in keys {
            let interpolation = match key.track {
                CurveTrackId::Scalar(property) => {
                    clip.track(property)?
                        .curve
                        .keys
                        .iter()
                        .find(|candidate| (candidate.time - key.time).abs() <= KEY_TIME_EPSILON)?
                        .interpolation
                }
                CurveTrackId::Rotation => {
                    clip.rotation
                        .as_ref()?
                        .keys
                        .iter()
                        .find(|candidate| (candidate.time - key.time).abs() <= KEY_TIME_EPSILON)?
                        .interpolation
                }
                CurveTrackId::MorphWeight(target) => {
                    clip.morph_weights
                        .get(&target)?
                        .keys
                        .iter()
                        .find(|candidate| (candidate.time - key.time).abs() <= KEY_TIME_EPSILON)?
                        .interpolation
                }
            };
            match common {
                Some(existing) if existing != interpolation => return None,
                None => common = Some(interpolation),
                _ => {}
            }
        }
        common
    }

    /// The common tangent type across every selected side, or `None` when the
    /// selection is mixed (including a key whose two sides differ).
    pub(super) fn selected_curve_tangent_type(&self) -> Option<TangentType> {
        let keys = self.selected_curve_keys();
        let clip = self.target_clip(self.current_anim_target()?)?;
        let mut common = None;
        for key in keys {
            let (incoming, outgoing) = match key.track {
                CurveTrackId::Scalar(property) => {
                    let key =
                        clip.track(property)?.curve.keys.iter().find(|candidate| {
                            (candidate.time - key.time).abs() <= KEY_TIME_EPSILON
                        })?;
                    (key.tangent_in_type, key.tangent_out_type)
                }
                CurveTrackId::Rotation => {
                    let key =
                        clip.rotation.as_ref()?.keys.iter().find(|candidate| {
                            (candidate.time - key.time).abs() <= KEY_TIME_EPSILON
                        })?;
                    (key.tangent_in_type, key.tangent_out_type)
                }
                CurveTrackId::MorphWeight(target) => {
                    let key = clip
                        .morph_weights
                        .get(&target)?
                        .keys
                        .iter()
                        .find(|candidate| (candidate.time - key.time).abs() <= KEY_TIME_EPSILON)?;
                    (key.tangent_in_type, key.tangent_out_type)
                }
            };
            if incoming != outgoing {
                return None;
            }
            match common {
                Some(existing) if existing != incoming => return None,
                None => common = Some(incoming),
                _ => {}
            }
        }
        common
    }

    /// Copy the current key selection, preserving each key's offset from the
    /// earliest selected key.
    pub(super) fn copy_selected_curve_keys(&mut self) -> bool {
        let keys = self.selected_curve_keys();
        let Some(first) = keys.first() else {
            self.status = "Select F-curve keys to copy".into();
            return false;
        };
        let target = first.target;
        let Some(clip) = self.target_clip(target) else {
            return false;
        };
        let anchor = keys
            .iter()
            .map(|key| key.time)
            .fold(f32::INFINITY, f32::min);
        let mut clipboard = Vec::with_capacity(keys.len());
        for key in &keys {
            match key.track {
                CurveTrackId::Scalar(property) => {
                    let Some(track) = clip.track(property) else {
                        continue;
                    };
                    let Some(value) =
                        track.curve.keys.iter().find(|candidate| {
                            (candidate.time - key.time).abs() <= KEY_TIME_EPSILON
                        })
                    else {
                        continue;
                    };
                    clipboard.push(CurveClipboardKey {
                        track: key.track,
                        offset: key.time - anchor,
                        value: CurveKeyValue::Scalar(value.value),
                        interpolation: value.interpolation,
                    });
                }
                CurveTrackId::Rotation => {
                    let Some(track) = clip.rotation.as_ref() else {
                        continue;
                    };
                    let Some(value) = track
                        .keys
                        .iter()
                        .find(|candidate| (candidate.time - key.time).abs() <= KEY_TIME_EPSILON)
                    else {
                        continue;
                    };
                    clipboard.push(CurveClipboardKey {
                        track: key.track,
                        offset: key.time - anchor,
                        value: CurveKeyValue::Rotation(value.value),
                        interpolation: value.interpolation,
                    });
                }
                CurveTrackId::MorphWeight(target) => {
                    let Some(curve) = clip.morph_weights.get(&target) else {
                        continue;
                    };
                    let Some(value) = curve
                        .keys
                        .iter()
                        .find(|candidate| (candidate.time - key.time).abs() <= KEY_TIME_EPSILON)
                    else {
                        continue;
                    };
                    clipboard.push(CurveClipboardKey {
                        track: key.track,
                        offset: key.time - anchor,
                        value: CurveKeyValue::Scalar(value.value),
                        interpolation: value.interpolation,
                    });
                }
            }
        }
        if clipboard.is_empty() {
            return false;
        }
        let count = clipboard.len();
        self.curve_clipboard = CurveClipboard { keys: clipboard };
        self.status = format!("Copied {count} key(s)");
        true
    }

    /// Paste copied keys at the playhead. Destination channels are resolved
    /// through the registry, so a paste can target another object, tag or
    /// material and only compatible channels are written. The whole paste,
    /// including overwrites, is one undo step.
    pub(super) fn paste_curve_keys(&mut self) -> bool {
        let Some(target) = self.current_anim_target() else {
            return false;
        };
        if self.curve_clipboard.keys.is_empty() {
            self.status = "Nothing copied to paste".into();
            return false;
        }
        let before_selection = self.selected_curve_keys();
        let Some(mut after) = self.target_clip(target) else {
            return false;
        };
        let mut pasted = Vec::new();
        for item in &self.curve_clipboard.keys {
            let channel = match item.track {
                CurveTrackId::Scalar(property) => AnimChannel::from_scalar_property(property),
                CurveTrackId::Rotation => Some(AnimChannel::Rotation),
                CurveTrackId::MorphWeight(_) => Some(AnimChannel::MorphWeight),
            };
            let Some(channel) = channel else {
                continue;
            };
            let compatible = match item.track {
                CurveTrackId::Scalar(_) | CurveTrackId::MorphWeight(_) => {
                    channel.kind() != ChannelKind::Quaternion
                }
                CurveTrackId::Rotation => channel.kind() == ChannelKind::Quaternion,
            };
            if !compatible || !self.channel_applies_to_target(channel, target) {
                continue;
            }
            if let CurveTrackId::MorphWeight(index) = item.track {
                if self.morph_weight_value(target, index).is_none() {
                    continue;
                }
            }
            let time = (self.playback.time + item.offset).max(0.0);
            if !time.is_finite() {
                continue;
            }
            match item.track {
                CurveTrackId::Scalar(property) => {
                    let CurveKeyValue::Scalar(value) = item.value else {
                        continue;
                    };
                    after.set_key(property, time, value, item.interpolation);
                    pasted.push(SelectedKey {
                        target,
                        track: item.track,
                        time,
                    });
                }
                CurveTrackId::Rotation => {
                    let CurveKeyValue::Rotation(value) = item.value else {
                        continue;
                    };
                    after.set_rotation_key(time, value, item.interpolation);
                    pasted.push(SelectedKey {
                        target,
                        track: item.track,
                        time,
                    });
                }
                CurveTrackId::MorphWeight(index) => {
                    let CurveKeyValue::Scalar(value) = item.value else {
                        continue;
                    };
                    after.set_morph_weight_key(index, time, value, item.interpolation);
                    pasted.push(SelectedKey {
                        target,
                        track: item.track,
                        time,
                    });
                }
            }
        }
        if pasted.is_empty() {
            self.status = "No copied channels apply to this target".into();
            return false;
        }
        let after_selection = pasted.clone();
        if !self.commit_pasted_target_clip(target, after, before_selection, after_selection) {
            return false;
        }
        let count = pasted.len();
        self.selected_keys = pasted;
        self.status = format!("Pasted {count} key(s)");
        true
    }

    #[cfg(test)]
    pub(super) fn move_curve_key(&mut self, property: AnimProperty, from: f32, to: f32) -> bool {
        let Some(target) = self.current_anim_target() else {
            return false;
        };
        self.move_target_curve_key(target, property, from, to)
    }

    #[cfg(test)]
    pub(super) fn delete_curve_key(&mut self, property: AnimProperty, time: f32) -> bool {
        let Some(target) = self.current_anim_target() else {
            return false;
        };
        self.delete_target_curve_key(target, property, time)
    }

    #[cfg(test)]
    pub(super) fn set_curve_key_interpolation(
        &mut self,
        property: AnimProperty,
        time: f32,
        interpolation: Interpolation,
    ) -> bool {
        let Some(target) = self.current_anim_target() else {
            return false;
        };
        self.set_target_curve_key_interpolation(target, property, time, interpolation)
    }

    pub(super) fn key_transforms_ready(&self) -> bool {
        !self.selection.is_empty() && !self.key_transform_edits().is_empty()
    }

    pub(super) fn key_transform_channels(&mut self) {
        let edits = self.key_transform_edits();
        if edits.is_empty() {
            self.status = "No transform keys to change at this frame".into();
            return;
        }
        let count = edits.len();
        let time = self.playback.time;
        self.history
            .execute(&mut self.doc, Box::new(SetAnimationsCmd { edits }));
        self.status = if count == 1 {
            format!(
                "Keyed transforms at frame {}",
                (time * self.playback.fps).round()
            )
        } else {
            format!(
                "Keyed transforms on {count} objects at frame {}",
                (time * self.playback.fps).round()
            )
        };
    }
}
