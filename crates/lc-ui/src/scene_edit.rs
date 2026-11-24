//! Object creation, selection, grouping, reparenting and deletion workflows.

use super::*;

impl LeopardApp {
    pub(super) fn add_node(&mut self, node: Node, parent: Option<NodeId>) {
        let id = node.id;
        let cmd = AddNodeCmd::new(&self.doc, node, parent);
        self.history.execute(&mut self.doc, Box::new(cmd));
        self.selection.select_one(id);
        self.mesh_edit = false;
        self.clear_mesh_component_selection();
        self.browser_tab = BrowserTab::Objects;
        self.reveal_selected_in_browser = true;
        let cleared_filter = !self.hierarchy_filter.is_empty();
        if cleared_filter {
            self.hierarchy_filter.clear();
        }
        let name = self
            .doc
            .scene
            .get(id)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        self.status = if cleared_filter {
            format!("Added {name} · search cleared")
        } else {
            format!("Added {name}")
        };
    }

    pub(super) fn add_primitive(&mut self, primitive: Primitive) {
        let node = self.doc.create_node(
            NodeKind::Mesh {
                geometry: GeometrySource::from_primitive(primitive),
            },
            primitive.label(),
        );
        self.add_node(node, None);
        if let Some(id) = self.selection.primary() {
            if let Some(n) = self.doc.scene.get_mut(id) {
                n.material = Some(0);
            }
        }
        self.sync_selection_material();
    }

    pub(super) fn add_light(&mut self, kind: LightKind) {
        let mut node = self.doc.create_node(
            NodeKind::Light {
                light: Light {
                    kind,
                    color: Vec3::new(1.0, 0.92, 0.82),
                    intensity: 8.0,
                    cast_shadows: true,
                },
            },
            "Light",
        );
        node.transform = Transform::from_translation(Vec3::new(2.5, 3.2, 2.0));
        self.add_node(node, None);
    }

    pub(super) fn add_camera(&mut self) {
        let eye = Vec3::new(5.2, 3.4, 6.4);
        let mut node = self.doc.create_node(
            NodeKind::Camera {
                camera: Camera::default(),
            },
            "Camera",
        );
        node.transform = Transform::new(
            eye,
            look_at_quat(eye, Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
            Vec3::ONE,
        );
        self.add_node(node, None);
    }

    pub(super) fn add_null(&mut self) {
        let node = self.doc.create_node(NodeKind::Null, "Null");
        self.add_node(node, None);
    }

    /// Bake the selected tip's solved chain over the playback range as one undo
    /// step, keying every frame like Cheetah3D's IK bake command.
    pub(super) fn bake_ik_chain(&mut self, tip: NodeId) -> bool {
        let Some(chain) = self
            .doc
            .scene
            .get(tip)
            .and_then(|node| node.ik.as_ref())
            .cloned()
        else {
            self.status = "This joint has no IK chain to bake".into();
            return false;
        };
        if !chain.enabled {
            self.status = "Enable the IK chain before baking".into();
            return false;
        }
        let joints: BTreeSet<NodeId> = ik_chain_joints(&self.doc.scene, tip, &chain)
            .into_iter()
            .collect();
        if joints.is_empty() {
            self.status = "The IK chain has no movable joints to bake".into();
            return false;
        }
        let start = self.playback.start;
        let end = self.playback.end.max(start);
        let edits: Vec<_> = bake_ik_range_edits(&self.doc, start, end, self.playback.fps)
            .into_iter()
            .filter(|(id, _, _)| joints.contains(id))
            .collect();
        if edits.is_empty() {
            self.status =
                "The IK bake range is empty, too long, or already has this pose keyed".into();
            return false;
        }
        let count = edits.len();
        self.history
            .execute(&mut self.doc, Box::new(SetAnimationsCmd { edits }));
        self.status = format!(
            "Baked IK on {count} joint{} over frames {}–{}",
            if count == 1 { "" } else { "s" },
            (start * self.playback.fps).round(),
            (end * self.playback.fps).round()
        );
        true
    }

    pub(super) fn add_joint(&mut self) {
        let node = self.doc.create_node(
            NodeKind::Joint {
                length: 0.8,
                width: 0.08,
            },
            "Joint",
        );
        self.add_node(node, None);
    }

    pub(super) fn add_spline(&mut self) {
        let node = self.doc.create_node(
            NodeKind::Spline {
                spline: lc_geom::Spline::new(
                    lc_geom::SplineKind::Bezier,
                    vec![
                        Vec3::new(-2.0, 0.4, 0.0),
                        Vec3::new(-1.0, 1.5, 0.0),
                        Vec3::new(0.0, 0.5, 0.0),
                        Vec3::new(1.0, 1.5, 0.0),
                        Vec3::new(2.0, 0.4, 0.0),
                    ],
                    false,
                ),
            },
            "Spline",
        );
        self.add_node(node, None);
    }

    pub(super) fn delete_selected(&mut self) {
        let roots = self.selection_roots();
        if roots.is_empty() {
            return;
        }
        let count = roots.len();
        let single_name = (count == 1)
            .then(|| self.doc.scene.get(roots[0]).map(|node| node.name.clone()))
            .flatten();
        let before = self.doc.clone();
        for id in roots {
            self.doc.scene.remove_subtree(id);
        }
        let after = self.doc.clone();
        self.history.execute(
            &mut self.doc,
            Box::new(lc_scene::ReplaceDocumentCmd {
                label: match single_name {
                    Some(name) => format!("Delete {name}"),
                    None => format!("Delete {count} Objects"),
                },
                before,
                after,
            }),
        );
        self.selection.clear();
        self.mesh_edit = false;
        self.clear_mesh_component_selection();
        self.status = if count == 1 {
            "Deleted object".into()
        } else {
            format!("Deleted {count} objects")
        };
    }

    pub(super) fn duplicate_selected(&mut self) {
        let roots = self.selection_roots();
        if roots.is_empty() {
            return;
        }
        let count = roots.len();
        let before = self.doc.clone();
        let mut copies = Vec::new();
        for id in roots {
            if let Some(new_id) = self.doc.scene.duplicate_subtree(id) {
                copies.push(new_id);
            }
        }
        if copies.is_empty() {
            return;
        }
        let after = self.doc.clone();
        self.history.execute(
            &mut self.doc,
            Box::new(lc_scene::ReplaceDocumentCmd {
                label: if count == 1 {
                    "Duplicate".into()
                } else {
                    format!("Duplicate {count} Objects")
                },
                before,
                after,
            }),
        );
        self.selection.set(copies);
        self.status = if count == 1 {
            "Duplicated subtree".into()
        } else {
            format!("Duplicated {count} objects")
        };
    }

    /// Selected nodes without any selected ancestor. Moving a parent already
    /// moves its children, and deleting or duplicating a parent covers them.
    pub(super) fn selection_roots(&self) -> Vec<NodeId> {
        let selected: HashSet<NodeId> = self.selection.ids().iter().copied().collect();
        if selected.is_empty() {
            return Vec::new();
        }
        self.doc
            .scene
            .depth_first()
            .into_iter()
            .filter(|id| selected.contains(id))
            .filter(|id| {
                !self
                    .doc
                    .scene
                    .parent_chain(*id)
                    .iter()
                    .any(|parent| selected.contains(parent))
            })
            .collect()
    }

    /// Roots that accept a gizmo transform. Locked objects stay selected but
    /// are not edited by the shared pivot.
    pub(super) fn selection_transform_roots(&self) -> Vec<NodeId> {
        self.selection_roots()
            .into_iter()
            .filter(|id| {
                self.doc
                    .scene
                    .get(*id)
                    .map(|node| !node.locked)
                    .unwrap_or(false)
            })
            .collect()
    }

    pub(super) fn selection_pivot(&self, evaluated: &EvaluatedScene) -> Option<Vec3> {
        let roots = self.selection_transform_roots();
        let mut sum = Vec3::ZERO;
        let mut count = 0usize;
        for id in roots {
            if let Some(node) = evaluated.node(id) {
                sum += node.world.w_axis.truncate();
                count += 1;
            }
        }
        (count > 0).then_some(sum / count as f32)
    }

    pub(super) fn selection_flag_ready(&self, flag: NodeFlag, value: bool) -> bool {
        self.selection.ids().iter().any(|id| {
            self.doc
                .scene
                .get(*id)
                .map(|node| flag.get(&NodeProps::of(node)) != value)
                .unwrap_or(false)
        })
    }

    pub(super) fn set_selection_flag(&mut self, flag: NodeFlag, value: bool) {
        if !self.selection_flag_ready(flag, value) {
            self.status = "Selected objects already have that setting".into();
            return;
        }
        let ids: Vec<NodeId> = self.selection.ids().to_vec();
        self.set_nodes_flag(&ids, flag, value);
    }

    pub(super) fn set_nodes_flag(&mut self, ids: &[NodeId], flag: NodeFlag, value: bool) {
        let mut edits = Vec::new();
        for id in ids {
            let Some(node) = self.doc.scene.get(*id) else {
                continue;
            };
            let before = NodeProps::of(node);
            if flag.get(&before) == value {
                continue;
            }
            let mut after = before.clone();
            flag.set(&mut after, value);
            edits.push((*id, before, after));
        }
        if edits.is_empty() {
            return;
        }
        let count = edits.len();
        self.history
            .execute(&mut self.doc, Box::new(SetNodesPropsCmd { edits }));
        let label = match flag {
            NodeFlag::Visible => "visibility",
            NodeFlag::Renderable => "render flag",
            NodeFlag::Locked => "lock",
        };
        self.status = if count == 1 {
            format!("Changed {label}")
        } else {
            format!("Changed {label} on {count} objects")
        };
    }

    /// Group the selected roots under a new Null at their shared pivot while
    /// preserving every object's world transform.
    pub(super) fn group_selection(&mut self) {
        let roots = self.selection_roots();
        if roots.is_empty() {
            return;
        }
        let pivot = self.selection_pivot(&self.evaluate()).unwrap_or(Vec3::ZERO);
        let parents: HashSet<Option<NodeId>> = roots
            .iter()
            .map(|id| self.doc.scene.get(*id).and_then(|node| node.parent))
            .collect();
        let group_parent = if parents.len() == 1 {
            parents.into_iter().next().flatten()
        } else {
            None
        };
        let parent_world = group_parent
            .map(|parent| self.doc.scene.world_matrix_plain(parent))
            .unwrap_or(Mat4::IDENTITY);
        let before = self.doc.clone();
        let mut group = self.doc.create_node(NodeKind::Null, "Group");
        let (scale, rotation, translation) = (parent_world.inverse()
            * Mat4::from_translation(pivot))
        .to_scale_rotation_translation();
        group.transform = Transform::new(translation, rotation, scale);
        let group_id = group.id;
        if let Err(error) = self.doc.scene.insert_node(group, group_parent, None) {
            self.doc = before;
            self.status = format!("Group failed: {error}");
            return;
        }
        let group_inverse = self.doc.scene.world_matrix_plain(group_id).inverse();
        for id in &roots {
            let old_world = self.doc.scene.world_matrix_plain(*id);
            if let Err(error) = self.doc.scene.reparent(*id, Some(group_id), None) {
                self.doc = before;
                self.status = format!("Group failed: {error}");
                return;
            }
            let (scale, rotation, translation) =
                (group_inverse * old_world).to_scale_rotation_translation();
            if let Some(node) = self.doc.scene.get_mut(*id) {
                node.transform = Transform::new(translation, rotation, scale);
            }
        }
        let after = self.doc.clone();
        self.history.execute(
            &mut self.doc,
            Box::new(ReplaceDocumentCmd {
                label: format!("Group {} Objects", roots.len()),
                before,
                after,
            }),
        );
        self.selection.select_one(group_id);
        self.mesh_edit = false;
        self.clear_mesh_component_selection();
        self.status = format!("Grouped {} objects", roots.len());
    }

    /// Reparent selected roots under `new_parent` (or the scene root) while
    /// preserving their world transforms.
    pub(super) fn reparent_selection(&mut self, new_parent: Option<NodeId>) -> bool {
        let roots = self.selection_roots();
        if roots.is_empty() {
            return false;
        }
        if let Some(parent) = new_parent {
            if roots
                .iter()
                .any(|id| *id == parent || self.doc.scene.is_ancestor(*id, parent))
            {
                self.status = "Cannot parent an object to itself or its descendant".into();
                return false;
            }
        }
        let before = self.doc.clone();
        for id in &roots {
            let old_world = self.doc.scene.world_matrix_plain(*id);
            if let Err(error) = self.doc.scene.reparent(*id, new_parent, None) {
                self.doc = before;
                self.status = format!("Reparent failed: {error}");
                return false;
            }
            let parent_world = new_parent
                .map(|parent| self.doc.scene.world_matrix_plain(parent))
                .unwrap_or(Mat4::IDENTITY);
            let (scale, rotation, translation) =
                (parent_world.inverse() * old_world).to_scale_rotation_translation();
            if let Some(node) = self.doc.scene.get_mut(*id) {
                node.transform = Transform::new(translation, rotation, scale);
            }
        }
        let after = self.doc.clone();
        self.history.execute(
            &mut self.doc,
            Box::new(ReplaceDocumentCmd {
                label: if roots.len() == 1 {
                    "Reparent".into()
                } else {
                    format!("Reparent {} Objects", roots.len())
                },
                before,
                after,
            }),
        );
        self.status = if roots.len() == 1 {
            "Reparented object".into()
        } else {
            format!("Reparented {} objects", roots.len())
        };
        true
    }
}
