//! Editable-mesh, component selection, UV and mesh tool workflows.

use super::*;

impl LeopardApp {
    pub(super) fn selected_is_mesh(&self) -> bool {
        self.selection
            .primary()
            .and_then(|id| self.doc.scene.get(id))
            .map(|node| node.is_mesh())
            .unwrap_or(false)
    }

    /// Component mode needs a mesh-only object selection: every selected
    /// object must be a mesh, and primitives are eligible because entering
    /// component mode can convert them in one batch.
    pub(super) fn mesh_mode_ready(&self) -> bool {
        !self.selection.is_empty()
            && self.selection.ids().iter().all(|id| {
                self.doc
                    .scene
                    .get(*id)
                    .map(|node| node.is_mesh())
                    .unwrap_or(false)
            })
    }

    pub(super) fn selected_meshes_editable(&self) -> bool {
        !self.selection.is_empty()
            && self.selection.ids().iter().all(|id| {
                matches!(
                    self.doc.scene.get(*id).map(|node| &node.kind),
                    Some(NodeKind::Mesh {
                        geometry: GeometrySource::Editable { .. }
                    })
                )
            })
    }

    pub(super) fn mesh_component_selection_ready(&self) -> bool {
        self.mesh_edit && self.selected_meshes_editable()
    }

    /// True when every selected mesh is editable, in the required component
    /// mode, and has at least `minimum` selected components. This is the single
    /// acceptance rule for every component tool: menu enablement and the
    /// handlers both go through it.
    pub(super) fn mesh_topology_ready(&self, mode: MeshSelectMode, minimum: usize) -> bool {
        self.mesh_component_selection_ready()
            && self.mesh_select_mode == mode
            && self
                .selection
                .ids()
                .iter()
                .all(|id| self.mesh_components_for(*id).len() >= minimum)
    }

    pub(super) fn editable_mesh(&self, id: NodeId) -> Option<&Mesh> {
        match self.doc.scene.get(id).map(|node| &node.kind) {
            Some(NodeKind::Mesh {
                geometry: GeometrySource::Editable { mesh, .. },
            }) => Some(mesh),
            _ => None,
        }
    }

    /// Weld needs two distinct, in-range points on every selected mesh.
    pub(super) fn mesh_weld_ready(&self) -> bool {
        self.mesh_component_selection_ready()
            && self.mesh_select_mode == MeshSelectMode::Points
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        selected_valid_points(mesh, self.mesh_components_for(*id)).len() >= 2
                    })
                    .unwrap_or(false)
            })
    }

    /// Optimize welds the selected points that sit within the tolerance, so it
    /// needs point mode, a positive tolerance and at least one cluster on every
    /// selected mesh.
    pub(super) fn mesh_optimize_ready(&self) -> bool {
        self.mesh_component_selection_ready()
            && self.mesh_select_mode == MeshSelectMode::Points
            && self.mesh_optimize_tolerance.is_finite()
            && self.mesh_optimize_tolerance > 0.0
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        lc_geom::can_optimize(
                            mesh,
                            self.mesh_components_for(*id),
                            self.mesh_optimize_tolerance,
                        )
                    })
                    .unwrap_or(false)
            })
    }

    /// Point Slide moves the selected points toward their neighbours, so it
    /// needs point mode, a fraction above zero and at least one selected point
    /// that an edge connects to something.
    pub(super) fn mesh_point_slide_ready(&self) -> bool {
        self.mesh_component_selection_ready()
            && self.mesh_select_mode == MeshSelectMode::Points
            && self.mesh_slide_fraction.is_finite()
            && self.mesh_slide_fraction > 0.0
            && self.mesh_slide_fraction <= 1.0
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        lc_geom::can_slide_points(
                            mesh,
                            self.mesh_components_for(*id),
                            self.mesh_slide_fraction,
                        )
                    })
                    .unwrap_or(false)
            })
    }

    /// Collapse welds the selected points into one at their centroid, so it
    /// needs point mode and two distinct selected points on every selected mesh
    /// — the same rule as Weld, which shares its geometry call.
    pub(super) fn mesh_collapse_ready(&self) -> bool {
        self.mesh_weld_ready()
    }

    /// Ring Cut is a through-plane cut: it must cross geometry on every
    /// selected mesh, otherwise the handler would report "no change".
    pub(super) fn mesh_ring_cut_ready(&self) -> bool {
        self.mesh_component_selection_ready()
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| ring_cut_would_split(mesh, self.mesh_axis, self.mesh_offset))
                    .unwrap_or(false)
            })
    }

    /// Component transforms need at least one affected point that the current
    /// mode, axis and value would actually move on every selected mesh.
    pub(super) fn mesh_transform_ready(&self) -> bool {
        self.mesh_component_selection_ready()
            && self.selection.ids().iter().all(|id| {
                let Some(mesh) = self.editable_mesh(*id) else {
                    return false;
                };
                let affected: HashSet<u32> = mesh_affected_points(
                    mesh,
                    self.mesh_select_mode,
                    self.mesh_components_for(*id),
                )
                .into_iter()
                .collect();
                if affected.is_empty() {
                    return false;
                }
                let rotation = self
                    .doc
                    .scene
                    .world_matrix_plain(*id)
                    .to_scale_rotation_translation()
                    .1;
                let local_axis =
                    mesh_transform_axis(mesh, &affected, self.mesh_space, self.mesh_axis, rotation);
                mesh_transform_would_change(
                    mesh,
                    &affected,
                    self.mesh_transform,
                    local_axis,
                    self.mesh_value,
                )
            })
    }

    /// Material assignment needs a material and selected polygons on every
    /// selected mesh so one undo cannot leave part of the selection untouched.
    pub(super) fn mesh_assign_material_ready(&self) -> bool {
        self.mesh_component_selection_ready()
            && self.mesh_select_mode == MeshSelectMode::Polygons
            && self.selected_material_id().is_some()
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        self.mesh_components_for(*id)
                            .iter()
                            .any(|index| mesh.polygons.get(*index as usize).is_some())
                    })
                    .unwrap_or(false)
            })
    }

    pub(super) fn selected_material_id(&self) -> Option<MaterialId> {
        self.selected_material
            .filter(|id| self.doc.materials.get(*id).is_some())
    }

    /// Bridge Loops needs every selected mesh to carry exactly two clean
    /// closed edge loops of equal length. This one check backs both the menu
    /// button and the handler.
    pub(super) fn mesh_bridge_ready(&self) -> bool {
        self.mesh_bridge_failure().is_none()
    }

    pub(super) fn mesh_bridge_failure(&self) -> Option<String> {
        if !self.mesh_topology_ready(MeshSelectMode::Edges, 1) {
            return Some(self.mesh_tool_refusal(MeshSelectMode::Edges, 1, "bridge"));
        }
        for id in self.selection.ids() {
            if let Err(error) = self.check_bridge_loops(*id) {
                let target = if self.selection.len() == 1 {
                    String::new()
                } else {
                    let name = self
                        .doc
                        .scene
                        .get(*id)
                        .map(|node| node.name.clone())
                        .unwrap_or_default();
                    format!(" on \"{name}\"")
                };
                return Some(format!(
                    "Bridge Loops needs exactly two closed edge loops of equal length{target}: {error}"
                ));
            }
        }
        None
    }

    pub(super) fn check_bridge_loops(&self, id: NodeId) -> Result<(), String> {
        let mesh = self.editable_mesh(id).ok_or("not an editable mesh")?;
        selected_bridge_loops(mesh, self.mesh_components_for(id)).map(|_| ())
    }

    /// Fill Hole accepts a selection in point, edge or polygon mode and only
    /// needs part of one open border, matching the documented tool (source 38).
    pub(super) fn mesh_fill_hole_ready(&self) -> bool {
        if !self.mesh_component_selection_ready() {
            return false;
        }
        let mode = self.mesh_select_mode;
        self.selection.ids().iter().all(|id| {
            self.editable_mesh(*id)
                .map(|mesh| {
                    let components = self.mesh_components_for(*id);
                    if components.is_empty() {
                        return false;
                    }
                    let points = mesh_affected_points(mesh, mode, components);
                    !lc_geom::open_holes(mesh, &points).is_empty()
                })
                .unwrap_or(false)
        })
    }

    /// Open hole borders under the primary mesh's component selection, used by
    /// the Fill Hole count label.
    pub(super) fn selected_hole_count(&self) -> usize {
        let Some((_, mesh)) = self.selected_editable_mesh() else {
            return 0;
        };
        let points = mesh_affected_points(&mesh, self.mesh_select_mode, &self.mesh_selected);
        lc_geom::open_holes(&mesh, &points).len()
    }

    /// Split copies the selected polygons into a new object, so like the
    /// documented tool (source 39) it is only available in polygon mode on raw
    /// polygon objects.
    pub(super) fn mesh_split_ready(&self) -> bool {
        self.mesh_topology_ready(MeshSelectMode::Polygons, 1)
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        !selected_valid_polygons(mesh, self.mesh_components_for(*id)).is_empty()
                    })
                    .unwrap_or(false)
            })
    }

    /// Scalpel cuts one polygon between two picks: two edges or two points that
    /// share a polygon. It only enables when that cut would really split the
    /// mesh, so the button never reports a no-op.
    pub(super) fn mesh_scalpel_ready(&self) -> bool {
        if !self.mesh_component_selection_ready() {
            return false;
        }
        let mode = self.mesh_select_mode;
        if !matches!(mode, MeshSelectMode::Points | MeshSelectMode::Edges) {
            return false;
        }
        self.selection.ids().iter().all(|id| {
            self.editable_mesh(*id)
                .map(|mesh| {
                    scalpel_cut(mesh, mode, self.mesh_components_for(*id))
                        .and_then(|(face, from, to)| lc_geom::scalpel_face(mesh, face, from, to))
                        .is_ok()
                })
                .unwrap_or(false)
        })
    }

    /// Clone works in point and polygon mode; an empty selection means every
    /// point or polygon, as the manual documents.
    pub(super) fn mesh_clone_ready(&self) -> bool {
        self.mesh_edit
            && self.selected_meshes_editable()
            && matches!(
                self.mesh_select_mode,
                MeshSelectMode::Points | MeshSelectMode::Polygons
            )
    }

    /// Partial subdivision needs polygon mode and at least one usable selected
    /// polygon; it shares Split's selection predicate.
    pub(super) fn mesh_subdivide_ready(&self) -> bool {
        self.mesh_split_ready()
    }

    /// Triangulate needs polygon mode and at least one selected polygon that
    /// actually has more than three corners, so it never reports a no-op.
    pub(super) fn mesh_triangulate_ready(&self) -> bool {
        self.mesh_topology_ready(MeshSelectMode::Polygons, 1)
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        selected_valid_polygons(mesh, self.mesh_components_for(*id))
                            .iter()
                            .any(|face| {
                                mesh.polygons
                                    .get(*face as usize)
                                    .is_some_and(|polygon| polygon.corners.len() > 3)
                            })
                    })
                    .unwrap_or(false)
            })
    }

    /// Flip Normal reverses the winding of the selected polygons, so like Split
    /// and Shell it needs polygon mode on raw polygon objects plus at least one
    /// usable selected polygon.
    pub(super) fn mesh_flip_normal_ready(&self) -> bool {
        self.mesh_topology_ready(MeshSelectMode::Polygons, 1)
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        !selected_valid_polygons(mesh, self.mesh_components_for(*id)).is_empty()
                    })
                    .unwrap_or(false)
            })
    }

    /// Disconnect needs polygon mode and a selected face that still shares a
    /// point with the rest of the mesh; a whole-mesh selection is already one
    /// island, so the command would be a no-op.
    pub(super) fn mesh_disconnect_ready(&self) -> bool {
        self.mesh_topology_ready(MeshSelectMode::Polygons, 1)
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| lc_geom::can_disconnect(mesh, self.mesh_components_for(*id)))
                    .unwrap_or(false)
            })
    }

    /// Cover fills the holes the selected faces border on every side, so it
    /// needs polygon mode and at least one closed open boundary loop the
    /// selection rings completely.
    pub(super) fn mesh_cover_ready(&self) -> bool {
        self.mesh_topology_ready(MeshSelectMode::Polygons, 1)
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        !lc_geom::coverable_holes(mesh, self.mesh_components_for(*id)).is_empty()
                    })
                    .unwrap_or(false)
            })
    }

    /// Create Polygon turns the selected boundary points into a new face, so it
    /// needs point mode and at least one open boundary loop fully selected.
    pub(super) fn mesh_create_polygon_ready(&self) -> bool {
        self.mesh_component_selection_ready()
            && self.mesh_select_mode == MeshSelectMode::Points
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        !lc_geom::faceable_loops(mesh, self.mesh_components_for(*id)).is_empty()
                    })
                    .unwrap_or(false)
            })
    }

    /// Shell needs polygon mode on raw polygon objects plus a usable distance,
    /// following the documented modes and properties (source 41).
    pub(super) fn mesh_shell_ready(&self) -> bool {
        self.mesh_shell_thickness.is_finite()
            && self.mesh_shell_thickness > 1e-6
            && self.mesh_topology_ready(MeshSelectMode::Polygons, 1)
            && self.selection.ids().iter().all(|id| {
                self.editable_mesh(*id)
                    .map(|mesh| {
                        !selected_valid_polygons(mesh, self.mesh_components_for(*id)).is_empty()
                    })
                    .unwrap_or(false)
            })
    }

    /// Polygons the documented Angle property groups into the shell region
    /// under the primary mesh's selection.
    pub(super) fn selected_shell_face_count(&self) -> usize {
        match self.selected_editable_mesh() {
            Some((_, mesh)) if self.mesh_select_mode == MeshSelectMode::Polygons => {
                let faces = selected_valid_polygons(&mesh, &self.mesh_selected);
                lc_geom::shell_region(&mesh, &faces, self.mesh_shell_angle).len()
            }
            _ => 0,
        }
    }

    /// Polygons the Split button would copy out of the primary mesh.
    pub(super) fn selected_split_polygon_count(&self) -> usize {
        match self.selected_editable_mesh() {
            Some((_, mesh)) if self.mesh_select_mode == MeshSelectMode::Polygons => {
                selected_valid_polygons(&mesh, &self.mesh_selected).len()
            }
            _ => 0,
        }
    }

    pub(super) fn mesh_components_for(&self, id: NodeId) -> &[u32] {
        self.mesh_components
            .get(&id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(super) fn clear_mesh_component_selection(&mut self) {
        self.mesh_selected.clear();
        self.mesh_components.clear();
    }

    /// Direct UV editing manipulates the primary mesh only and needs at least
    /// one UV point to pick and drag.
    pub(super) fn uv_point_edit_ready(&self) -> bool {
        self.selected_editable_mesh()
            .map(|(_, mesh)| mesh.uv_set_count() > 0)
            .unwrap_or(false)
    }

    /// Menu/toolbar predicate for the UV authoring commands.
    pub(super) fn uv_edit_ready(&self) -> bool {
        self.uv_point_edit_ready()
    }

    /// The active UV set, clamped to the mesh's current set count.
    pub(super) fn uv_active_set(&self, mesh: &Mesh) -> usize {
        self.uv_editor
            .set
            .min(mesh.uv_set_count().saturating_sub(1))
    }

    /// Selected UV slots, including the last-clicked primary point.
    pub(super) fn uv_selected_slots(&self) -> Vec<u32> {
        let mut slots: Vec<u32> = self.uv_editor.selection.iter().copied().collect();
        if let Some(index) = self.selected_uv {
            let index = index as u32;
            if !slots.contains(&index) {
                slots.push(index);
            }
        }
        slots.sort_unstable();
        slots.dedup();
        slots
    }

    pub(super) fn selected_uv_slots_for(&self, mesh: &Mesh) -> Vec<u32> {
        let set = self.uv_active_set(mesh);
        let Some(values) = mesh.uv_set(set) else {
            return Vec::new();
        };
        let mut slots = self.uv_selected_slots();
        slots.retain(|slot| (*slot as usize) < values.len());
        slots
    }

    /// Drop stale selection and clamp the active set after a slot edit.
    pub(super) fn prune_uv_selection(&mut self, mesh: &Mesh) {
        let len = mesh.uvs.len() as u32;
        self.uv_editor.selection.retain(|slot| *slot < len);
        if self.selected_uv.is_some_and(|index| index as u32 >= len) {
            self.selected_uv = None;
        }
        let sets = mesh.uv_set_count();
        if self.uv_editor.set >= sets {
            self.uv_editor.set = sets.saturating_sub(1);
        }
    }

    pub(super) fn set_uv_point(&mut self, id: NodeId, index: usize, uv: V2) -> bool {
        let Some((primary, mesh)) = self.selected_editable_mesh() else {
            self.status = "UV editing requires an editable mesh".into();
            return false;
        };
        if !self.uv_point_edit_ready() || primary != id {
            self.status = "Select the editable mesh that owns the UV point".into();
            return false;
        }
        let set = self.uv_active_set(&mesh);
        let Some(before) = self.doc.scene.get(id).map(Shape::of) else {
            return false;
        };
        let Some(node) = self.doc.scene.get_mut(id) else {
            return false;
        };
        let NodeKind::Mesh { geometry } = &mut node.kind else {
            return false;
        };
        let GeometrySource::Editable { mesh, .. } = geometry else {
            return false;
        };
        let Some(point) = mesh
            .uv_set_mut(set)
            .and_then(|values| values.get_mut(index))
            .map(|value| *value)
        else {
            self.status = "That UV point no longer exists".into();
            return false;
        };
        if point != uv {
            mesh.set_uv_value(set, index as u32, uv);
            if self.pending_shape.is_none() {
                self.pending_shape = Some((id, before));
            }
        }
        self.uv_editor.selection.insert(index as u32);
        self.selected_uv = Some(index);
        true
    }

    /// Move every selected UV point by one drag delta in the active set.
    pub(super) fn nudge_selected_uv(&mut self, delta: V2) -> bool {
        let Some((id, mesh)) = self.selected_editable_mesh() else {
            return false;
        };
        let set = self.uv_active_set(&mesh);
        let slots = self.selected_uv_slots_for(&mesh);
        let mut moved = false;
        for slot in slots {
            let Some(current) = mesh.uv_value(set, slot) else {
                continue;
            };
            if self.set_uv_point(id, slot as usize, current + delta) {
                moved = true;
            }
        }
        moved
    }

    /// The pin command covers every selected UV point on the primary mesh.
    pub(super) fn uv_pin_ready(&self) -> bool {
        self.selected_editable_mesh()
            .is_some_and(|(_, mesh)| !self.selected_uv_slots_for(&mesh).is_empty())
    }

    pub(super) fn toggle_selected_uv_pins(&mut self) {
        let Some((_, mesh)) = self.selected_editable_mesh() else {
            self.status = "Pin UVs on an editable mesh with a UV set".into();
            return;
        };
        let slots = self.selected_uv_slots_for(&mesh);
        if slots.is_empty() {
            self.status = "Select one or more UV points in the UV view first".into();
            return;
        }
        let pin = !slots.iter().all(|slot| mesh.is_uv_pinned(*slot));
        let count = slots.len();
        let message = if pin {
            format!("Pinned {count} UV point(s)")
        } else {
            format!("Unpinned {count} UV point(s)")
        };
        self.commit_uv_edit(message, |mesh| {
            for slot in &slots {
                mesh.set_uv_pinned(*slot, pin);
            }
            Some(String::new())
        });
    }

    /// Apply one document mutation to the primary mesh's UVs as one undo step.
    /// The closure returns a status message, or `None` for a refused edit.
    pub(super) fn commit_uv_edit(
        &mut self,
        message: String,
        edit: impl FnOnce(&mut Mesh) -> Option<String>,
    ) -> bool {
        let Some((id, mesh)) = self.selected_editable_mesh() else {
            self.status = "UV editing requires an editable mesh".into();
            return false;
        };
        let Some(before) = self.doc.scene.get(id).map(Shape::of) else {
            return false;
        };
        let mut updated = mesh.clone();
        let Some(edited) = edit(&mut updated) else {
            self.status = "UVs unchanged".into();
            return false;
        };
        let status = if edited.is_empty() { message } else { edited };
        if updated == mesh {
            self.status = status;
            return false;
        }
        let NodeKind::Mesh { geometry } = &before.kind else {
            return false;
        };
        let mut after = before.clone();
        after.kind = NodeKind::Mesh {
            geometry: geometry.with_mesh(updated.clone()),
        };
        self.history.execute(
            &mut self.doc,
            Box::new(SetShapesCmd {
                edits: vec![(id, before, after)],
            }),
        );
        self.prune_uv_selection(&updated);
        self.status = status;
        true
    }

    /// Toggle seams on the mesh edges between adjacent selected UV points.
    pub(super) fn toggle_selected_uv_seam(&mut self) {
        let Some((_, mesh)) = self.selected_editable_mesh() else {
            self.status = "UV seams need an editable mesh".into();
            return;
        };
        let slots: HashSet<u32> = self.selected_uv_slots_for(&mesh).into_iter().collect();
        if slots.len() < 2 {
            self.status = "Select two adjacent UV points across a mesh edge first".into();
            return;
        }
        let mut edges = Vec::new();
        for poly in &mesh.polygons {
            let n = poly.corners.len();
            for i in 0..n {
                let a = poly.corners[i];
                let b = poly.corners[(i + 1) % n];
                if slots.contains(&a.uv) && slots.contains(&b.uv) {
                    edges.push(if a.point < b.point {
                        [a.point, b.point]
                    } else {
                        [b.point, a.point]
                    });
                }
            }
        }
        edges.sort_unstable();
        edges.dedup();
        if edges.is_empty() {
            self.status = "Selected UV points are not adjacent on a mesh edge".into();
            return;
        }
        let count = edges.len();
        self.commit_uv_edit(format!("Toggled {count} UV seam edge(s)"), |mesh| {
            for [a, b] in &edges {
                mesh.toggle_seam(*a, *b);
            }
            Some(String::new())
        });
    }

    pub(super) fn select_all_uv_seams(&mut self) {
        let Some((_, mesh)) = self.selected_editable_mesh() else {
            self.status = "UV seams need an editable mesh".into();
            return;
        };
        if mesh.seams.is_empty() {
            self.status = "The mesh has no UV seams".into();
            return;
        }
        let points: HashSet<u32> = mesh.seams.iter().flatten().copied().collect();
        let mut slots = Vec::new();
        for poly in &mesh.polygons {
            for corner in &poly.corners {
                if points.contains(&corner.point) {
                    slots.push(corner.uv);
                }
            }
        }
        slots.sort_unstable();
        slots.dedup();
        let count = slots.len();
        self.uv_editor.selection = slots.into_iter().collect();
        self.selected_uv = self.uv_editor.selection.iter().next().map(|s| *s as usize);
        self.status = format!("Selected {count} UV points on seams");
    }

    pub(super) fn clear_uv_seams(&mut self) {
        let Some((_, mesh)) = self.selected_editable_mesh() else {
            self.status = "UV seams need an editable mesh".into();
            return;
        };
        if mesh.seams.is_empty() {
            self.status = "The mesh has no UV seams".into();
            return;
        }
        self.commit_uv_edit("Cleared UV seams".into(), |mesh| {
            mesh.seams.clear();
            Some(String::new())
        });
    }

    /// Select every UV point on the seam loop connected to the selection. With
    /// no seam selected, this selects every seam point.
    pub(super) fn select_uv_seam_loop(&mut self) {
        let Some((_, mesh)) = self.selected_editable_mesh() else {
            self.status = "UV seams need an editable mesh".into();
            return;
        };
        if mesh.seams.is_empty() {
            self.status = "The mesh has no UV seams".into();
            return;
        }
        let mut graph: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for [a, b] in &mesh.seams {
            graph.entry(*a).or_default().push(*b);
            graph.entry(*b).or_default().push(*a);
        }
        let slots = self.selected_uv_slots_for(&mesh);
        let mut selected_points = HashSet::new();
        for poly in &mesh.polygons {
            for corner in &poly.corners {
                if slots.contains(&corner.uv) {
                    selected_points.insert(corner.point);
                }
            }
        }
        let seeds: Vec<u32> = graph
            .keys()
            .copied()
            .filter(|point| selected_points.is_empty() || selected_points.contains(point))
            .collect();
        let mut loop_points = HashSet::new();
        let mut queue: Vec<u32> = seeds;
        while let Some(point) = queue.pop() {
            if !loop_points.insert(point) {
                continue;
            }
            if let Some(neighbours) = graph.get(&point) {
                queue.extend(neighbours.iter().copied());
            }
        }
        if loop_points.is_empty() {
            self.status = "Select a UV point on a seam first".into();
            return;
        }
        let mut loop_slots = Vec::new();
        for poly in &mesh.polygons {
            for corner in &poly.corners {
                if loop_points.contains(&corner.point) {
                    loop_slots.push(corner.uv);
                }
            }
        }
        loop_slots.sort_unstable();
        loop_slots.dedup();
        let count = loop_slots.len();
        self.uv_editor.selection = loop_slots.into_iter().collect();
        self.selected_uv = self.uv_editor.selection.iter().next().map(|s| *s as usize);
        self.status = format!("Selected seam loop with {count} UV points");
    }

    pub(super) fn select_uv_island(&mut self) {
        let Some((_, mesh)) = self.selected_editable_mesh() else {
            self.status = "Select an editable mesh with a UV set".into();
            return;
        };
        let slots = self.selected_uv_slots_for(&mesh);
        if slots.is_empty() {
            self.status = "Select a UV point before selecting its island".into();
            return;
        }
        let island = lc_geom::uv_island_slots(&mesh, &slots);
        let count = island.len();
        self.uv_editor.selection = island.into_iter().collect();
        self.selected_uv = self.uv_editor.selection.iter().next().map(|s| *s as usize);
        self.status = format!("Selected {count} UV points in the island");
    }

    pub(super) fn select_all_uv_points(&mut self) {
        let Some((_, mesh)) = self.selected_editable_mesh() else {
            self.status = "Select an editable mesh with a UV set".into();
            return;
        };
        let set = self.uv_active_set(&mesh);
        let count = mesh.uv_set(set).map(|values| values.len()).unwrap_or(0);
        self.uv_editor.selection = (0..count as u32).collect();
        self.selected_uv = (0..count).next();
        self.status = format!("Selected {count} UV points");
    }

    pub(super) fn clear_uv_selection(&mut self) {
        self.uv_editor.selection.clear();
        self.selected_uv = None;
        self.status = "Cleared UV selection".into();
    }

    /// Charts touched by the current UV selection, with the active set.
    fn selected_uv_charts(&self) -> Option<(usize, Vec<usize>, usize)> {
        let (_, mesh) = self.selected_editable_mesh()?;
        let set = self.uv_active_set(&mesh);
        let slots = self.selected_uv_slots_for(&mesh);
        let charts = lc_geom::charts_for_uv_slots(&mesh, &slots);
        Some((set, charts, mesh.uv_set_count()))
    }

    pub(super) fn transform_selected_uv_charts(&mut self) {
        let Some((set, charts, _)) = self.selected_uv_charts() else {
            self.status = "Select a UV point or island first".into();
            return;
        };
        if charts.is_empty() {
            self.status = "Select a UV point or island first".into();
            return;
        }
        let move_by = self.uv_editor.chart_move;
        let rotate = self.uv_editor.chart_rotate;
        let scale = self.uv_editor.chart_scale;
        if move_by == V2::ZERO && rotate == 0.0 && (scale - 1.0).abs() < 1e-6 {
            self.status = "Enter a chart move, rotation or scale first".into();
            return;
        }
        let count = charts.len();
        self.commit_uv_edit(format!("Transformed {count} UV chart(s)"), |mesh| {
            lc_geom::transform_uv_charts(mesh, set, &charts, move_by, rotate, scale);
            Some(String::new())
        });
    }

    /// Pack the charts touched by the selection, or every chart when nothing
    /// is selected. Pinned charts stay at their authored coordinates.
    pub(super) fn pack_selected_uv_charts(&mut self) {
        let Some((set, charts, _)) = self.selected_uv_charts() else {
            self.status = "Select an editable mesh with a UV set".into();
            return;
        };
        let options = lc_geom::UvPackOptions {
            padding: self.uv_editor.pack_padding,
            rotate: self.uv_editor.pack_rotate,
            align: self.uv_editor.pack_align,
        };
        self.commit_uv_edit("Packed UV charts".into(), |mesh| {
            let report = lc_geom::pack_uv_charts(mesh, set, &charts, options);
            if report.packed == 0 {
                return None;
            }
            Some(if report.skipped_pinned > 0 {
                format!(
                    "Packed {} UV chart(s); {} pinned chart(s) left in place",
                    report.packed, report.skipped_pinned
                )
            } else {
                format!("Packed {} UV chart(s)", report.packed)
            })
        });
    }

    /// Run a slot-level UV operation on the active set. Weld/Split change the
    /// shared slot topology and therefore every set.
    pub(super) fn run_uv_slot_op(&mut self, op: UvSlotOp) {
        let Some((_, mesh)) = self.selected_editable_mesh() else {
            self.status = "Select an editable mesh with a UV set".into();
            return;
        };
        let set = self.uv_active_set(&mesh);
        let slots = self.selected_uv_slots_for(&mesh);
        if slots.is_empty() {
            self.status = "Select one or more UV points first".into();
            return;
        }
        if slots.len() < 2
            && !matches!(op, UvSlotOp::Numeric | UvSlotOp::Split | UvSlotOp::Rotate90)
        {
            self.status = "That UV operation needs at least two selected points".into();
            return;
        }
        let numeric = self.uv_editor.numeric;
        self.commit_uv_edit("Updated UVs".into(), |mesh| match op {
            UvSlotOp::Weld => {
                let removed = lc_geom::weld_uv_slots(mesh, &slots);
                (removed > 0).then(|| format!("Welded UV points; {removed} slot(s) removed"))
            }
            UvSlotOp::Split => {
                let created = lc_geom::split_uv_slots(mesh, &slots);
                (created > 0).then(|| format!("Split UV points into {created} new slot(s)"))
            }
            UvSlotOp::AlignU => lc_geom::align_uv_slots(mesh, set, &slots, lc_geom::UvAxis::U)
                .then(|| "Aligned UVs on U".into()),
            UvSlotOp::AlignV => lc_geom::align_uv_slots(mesh, set, &slots, lc_geom::UvAxis::V)
                .then(|| "Aligned UVs on V".into()),
            UvSlotOp::DistributeU => {
                lc_geom::distribute_uv_slots(mesh, set, &slots, lc_geom::UvAxis::U)
                    .then(|| "Distributed UVs on U".into())
            }
            UvSlotOp::DistributeV => {
                lc_geom::distribute_uv_slots(mesh, set, &slots, lc_geom::UvAxis::V)
                    .then(|| "Distributed UVs on V".into())
            }
            UvSlotOp::FlipU => lc_geom::flip_uv_slots(mesh, set, &slots, lc_geom::UvAxis::U)
                .then(|| "Flipped UVs on U".into()),
            UvSlotOp::FlipV => lc_geom::flip_uv_slots(mesh, set, &slots, lc_geom::UvAxis::V)
                .then(|| "Flipped UVs on V".into()),
            UvSlotOp::Rotate90 => {
                lc_geom::rotate_uv_slots(mesh, set, &slots, 1).then(|| "Rotated UVs 90°".into())
            }
            UvSlotOp::Numeric => {
                let mut changed = false;
                for slot in &slots {
                    changed |= mesh.set_uv_value(set, *slot, numeric);
                }
                changed.then(|| {
                    format!(
                        "Set {} UV point(s) to {:.4}, {:.4}",
                        slots.len(),
                        numeric.x,
                        numeric.y
                    )
                })
            }
        });
    }

    pub(super) fn add_uv_set(&mut self) {
        let Some((_, mesh)) = self.selected_editable_mesh() else {
            self.status = "Select an editable mesh with a UV set".into();
            return;
        };
        if mesh.uvs.is_empty() {
            self.status = "Apply a projection before adding a UV set".into();
            return;
        }
        let next = mesh.uv_set_count();
        if self.commit_uv_edit("Added a UV set".into(), |mesh| {
            mesh.add_uv_set()
                .map(|index| format!("Added UV set {index}"))
        }) {
            self.uv_editor.set = next;
            self.status = format!("Added UV set {next}");
        }
    }

    pub(super) fn remove_uv_set(&mut self) {
        let Some((_, mesh)) = self.selected_editable_mesh() else {
            self.status = "Select an editable mesh with a UV set".into();
            return;
        };
        let set = self.uv_active_set(&mesh);
        if set == 0 {
            self.status = "UV0 cannot be removed".into();
            return;
        }
        self.commit_uv_edit(format!("Removed UV set {set}"), |mesh| {
            mesh.remove_uv_set(set).then_some(String::new())
        });
    }

    pub(super) fn selected_editable_mesh(&self) -> Option<(NodeId, Mesh)> {
        let id = self.selection.primary()?;
        let node = self.doc.scene.get(id)?;
        match &node.kind {
            NodeKind::Mesh {
                geometry: GeometrySource::Editable { mesh, .. },
            } => Some((id, mesh.clone())),
            _ => None,
        }
    }

    pub(super) fn selection_has_mesh(&self) -> bool {
        self.selection.ids().iter().any(|id| {
            self.doc
                .scene
                .get(*id)
                .map(|node| node.is_mesh())
                .unwrap_or(false)
        })
    }

    /// Any selected mesh that still needs converting to an editable mesh.
    pub(super) fn selection_convert_ready(&self) -> bool {
        self.selection.ids().iter().any(|id| {
            matches!(
                self.doc.scene.get(*id).map(|node| &node.kind),
                Some(NodeKind::Mesh { geometry }) if !matches!(geometry, GeometrySource::Editable { .. })
            )
        })
    }

    pub(super) fn boolean_operand(&self) -> Option<NodeId> {
        let selected: HashSet<NodeId> = self.selection.ids().iter().copied().collect();
        self.doc.scene.depth_first().into_iter().find(|id| {
            !selected.contains(id)
                && self
                    .doc
                    .scene
                    .get(*id)
                    .map(|node| node.is_mesh())
                    .unwrap_or(false)
        })
    }

    pub(super) fn boolean_ready(&self) -> bool {
        self.selection_has_mesh() && self.boolean_operand().is_some()
    }

    /// UV projection can only touch editable meshes. Require every selected
    /// mesh to be editable so a multi-selection action never silently skips a
    /// selected primitive; non-mesh objects are ignored.
    pub(super) fn selection_uv_ready(&self) -> bool {
        let mut has_mesh = false;
        for id in self.selection.ids() {
            let Some(node) = self.doc.scene.get(*id) else {
                continue;
            };
            if !node.is_mesh() {
                continue;
            }
            has_mesh = true;
            if !matches!(
                node.kind,
                NodeKind::Mesh {
                    geometry: GeometrySource::Editable { .. }
                }
            ) {
                return false;
            }
        }
        has_mesh
    }

    pub(super) fn make_selected_editable(&mut self) {
        if !self.selection_convert_ready() {
            self.status = if self.selection_has_mesh() {
                "Selected meshes are already editable".into()
            } else {
                "Select a mesh object first".into()
            };
            return;
        }
        let mut edits = Vec::new();
        for id in self.selection.ids() {
            let Some(node) = self.doc.scene.get(*id) else {
                continue;
            };
            let before = Shape::of(node);
            let mut after = before.clone();
            if let NodeKind::Mesh { geometry } = &mut after.kind {
                geometry.make_editable();
            }
            if before.kind != after.kind {
                edits.push((*id, before, after));
            }
        }
        let count = edits.len();
        self.history
            .execute(&mut self.doc, Box::new(SetShapesCmd { edits }));
        self.status = if count == 1 {
            "Converted primitive to editable mesh".into()
        } else {
            format!("Converted {count} primitives to editable meshes")
        };
    }

    pub(super) fn apply_uv_op(&mut self, op: UvOp) {
        if !self.selection_uv_ready() {
            self.status = if self.selection_has_mesh() {
                "Convert all selected meshes to editable first".into()
            } else {
                "Select an editable mesh first".into()
            };
            return;
        }
        let uv0_only = matches!(op, UvOp::Unwrap | UvOp::Relax | UvOp::Normalize);
        if uv0_only {
            for id in self.selection.ids() {
                let Some(NodeKind::Mesh { geometry }) = self.doc.scene.get(*id).map(|n| &n.kind)
                else {
                    continue;
                };
                let GeometrySource::Editable { mesh, .. } = geometry else {
                    continue;
                };
                if self.uv_active_set(mesh) != 0 {
                    self.status =
                        "Unwrap, Relax and Normalize author UV0; switch the set to UV0 first"
                            .into();
                    return;
                }
            }
        }
        let mut edits = Vec::new();
        for id in self.selection.ids() {
            let Some(node) = self.doc.scene.get(*id) else {
                continue;
            };
            let before = Shape::of(node);
            let NodeKind::Mesh { geometry } = &before.kind else {
                continue;
            };
            let GeometrySource::Editable { mesh, .. } = geometry else {
                continue;
            };
            let mut mesh = mesh.clone();
            let set = self.uv_active_set(&mesh);
            match op {
                UvOp::Planar => lc_geom::planar_projection_in_set(&mut mesh, set, 1, 1.0),
                UvOp::Box => lc_geom::box_projection_in_set(&mut mesh, set, 1.0),
                UvOp::Cylindrical => {
                    lc_geom::cylindrical_projection_in_set(&mut mesh, set, 1.0, 1.0)
                }
                UvOp::Spherical => lc_geom::spherical_projection_in_set(&mut mesh, set, 1.0),
                UvOp::Unwrap => lc_geom::automatic_unwrap(&mut mesh, 45.0, 0.01),
                UvOp::Relax => lc_geom::relax_uvs(&mut mesh, 8, 0.5, true),
                UvOp::Normalize => lc_geom::normalize_uvs(&mut mesh),
            }
            let mut after = before.clone();
            after.kind = NodeKind::Mesh {
                geometry: geometry.with_mesh(mesh),
            };
            edits.push((*id, before, after));
        }
        if edits.is_empty() {
            self.status = "Select an editable mesh first".into();
            return;
        }
        let count = edits.len();
        self.history
            .execute(&mut self.doc, Box::new(SetShapesCmd { edits }));
        self.status = if count == 1 {
            "Updated UVs".into()
        } else {
            format!("Updated UVs on {count} objects")
        };
    }

    pub(super) fn perform_mesh_action(&mut self, action: MeshAction) {
        match action {
            MeshAction::Enter => {
                if !self.mesh_mode_ready() {
                    self.status = if self.selection.is_empty() {
                        "Select a mesh object to edit components".into()
                    } else {
                        "Component editing needs a mesh-only selection".into()
                    };
                    return;
                }
                if !self.selected_meshes_editable() {
                    self.make_selected_editable();
                }
                if self.selected_meshes_editable() {
                    self.mesh_edit = true;
                    self.clear_mesh_component_selection();
                    self.status = "Mesh edit mode — click components in the viewport".into();
                } else if !self.selection.is_empty() {
                    self.status = "Select a mesh object first".into();
                }
            }
            MeshAction::Exit => {
                self.mesh_edit = false;
                self.clear_mesh_component_selection();
                self.status = "Object mode".into();
            }
            MeshAction::SelectAll => {
                if !self.mesh_component_selection_ready() {
                    self.status = "Enter component mode with editable meshes first".into();
                    return;
                }
                let ids = self.selection.ids().to_vec();
                let mut primary = None;
                for id in ids {
                    let Some(node) = self.doc.scene.get(id) else {
                        continue;
                    };
                    let NodeKind::Mesh {
                        geometry: GeometrySource::Editable { mesh, .. },
                    } = &node.kind
                    else {
                        continue;
                    };
                    let components: Vec<u32> = match self.mesh_select_mode {
                        MeshSelectMode::Points => (0..mesh.positions.len() as u32).collect(),
                        MeshSelectMode::Edges => (0..mesh.edges().len() as u32).collect(),
                        MeshSelectMode::Polygons => (0..mesh.polygons.len() as u32).collect(),
                    };
                    if self.selection.primary() == Some(id) {
                        primary = Some(components.clone());
                    }
                    self.mesh_components.insert(id, components);
                }
                self.mesh_selected = primary.unwrap_or_default();
            }
            MeshAction::SelectNone => {
                if !self.mesh_component_selection_ready() {
                    self.status = "Enter component mode with editable meshes first".into();
                    return;
                }
                self.clear_mesh_component_selection();
            }
            MeshAction::Transform => self.mesh_transform_selection(),
            MeshAction::Extrude => {
                if !self.require_mesh_selection(MeshSelectMode::Polygons, 1, "extrude") {
                    return;
                }
                let distance = self.mesh_value;
                let individual = self.mesh_extrude_individual;
                self.apply_mesh_operation("Extruded faces", move |_, mesh, faces| {
                    Ok(lc_geom::extrude_faces(mesh, faces, distance, individual))
                });
            }
            MeshAction::Inset => {
                if !self.require_mesh_selection(MeshSelectMode::Polygons, 1, "inset") {
                    return;
                }
                let amount = self.mesh_value.abs().max(1e-4);
                self.apply_mesh_operation("Inset faces", move |_, mesh, faces| {
                    Ok(lc_geom::bevel_faces(mesh, faces, amount, 0.0))
                });
            }
            MeshAction::Bevel => {
                if !self.require_mesh_selection(MeshSelectMode::Polygons, 1, "bevel") {
                    return;
                }
                let amount = self.mesh_value.abs().max(1e-4);
                let depth = self.mesh_depth;
                self.apply_mesh_operation("Bevelled faces", move |_, mesh, faces| {
                    Ok(lc_geom::bevel_faces(mesh, faces, amount, depth))
                });
            }
            MeshAction::RingCut => {
                if !self.mesh_ring_cut_ready() {
                    self.status = if self.mesh_component_selection_ready() {
                        "Ring cut needs the cut plane to cross every selected mesh".into()
                    } else {
                        "Enter component mode with editable meshes first".into()
                    };
                    return;
                }
                let axis = self.mesh_axis;
                let offset = self.mesh_offset;
                self.apply_mesh_operation("Ring cut", move |_, mesh, _| {
                    Ok(lc_geom::ring_cut(mesh, axis, offset))
                });
            }
            MeshAction::Weld => {
                if !self.mesh_weld_ready() {
                    self.status = if !self.mesh_topology_ready(MeshSelectMode::Points, 2) {
                        self.mesh_tool_refusal(MeshSelectMode::Points, 2, "weld")
                    } else if self.selection.len() == 1 {
                        "Select at least two distinct points to weld".into()
                    } else {
                        "Select at least two distinct points on every selected mesh to weld".into()
                    };
                    return;
                }
                self.apply_mesh_operation("Welded points", |_, mesh, points| {
                    Ok(weld_selected_points(mesh, points))
                });
            }
            MeshAction::Collapse => {
                if !self.mesh_collapse_ready() {
                    self.status = if !self.mesh_topology_ready(MeshSelectMode::Points, 2) {
                        self.mesh_tool_refusal(MeshSelectMode::Points, 2, "collapse")
                    } else {
                        "Select at least two distinct points to collapse".into()
                    };
                    return;
                }
                // The merged point can land on a new index after compaction, so
                // the collapse reports its survivor and the selection follows it.
                let survivors = std::cell::RefCell::new(Vec::new());
                let depth = self.history.depth();
                self.apply_mesh_operation_selection(
                    "Collapsed points",
                    true,
                    |id, mesh, points| {
                        let (out, survivor) = lc_geom::collapse_points(mesh, points);
                        if let Some(survivor) = survivor {
                            survivors.borrow_mut().push((id, survivor));
                        }
                        Ok(out)
                    },
                );
                if self.history.depth() == depth {
                    return;
                }
                let survivors = survivors.into_inner();
                for (id, survivor) in &survivors {
                    self.mesh_components.insert(*id, vec![*survivor]);
                }
                self.mesh_selected = self
                    .selection
                    .primary()
                    .and_then(|primary| {
                        survivors
                            .iter()
                            .find(|(id, _)| *id == primary)
                            .map(|(_, survivor)| vec![*survivor])
                    })
                    .unwrap_or_default();
            }
            MeshAction::Optimize => {
                if !self.mesh_optimize_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Points {
                        "Optimize is only available in point mode".into()
                    } else if !self.mesh_optimize_tolerance.is_finite()
                        || self.mesh_optimize_tolerance <= 0.0
                    {
                        "Set a positive optimize tolerance first".into()
                    } else if self
                        .selection
                        .ids()
                        .iter()
                        .any(|id| self.mesh_components_for(*id).len() < 2)
                    {
                        "Select at least two points to optimize".into()
                    } else {
                        "No selected points sit within the optimize tolerance".into()
                    };
                    return;
                }
                let tolerance = self.mesh_optimize_tolerance;
                let survivors = std::cell::RefCell::new(Vec::new());
                let depth = self.history.depth();
                self.apply_mesh_operation_selection(
                    "Optimized points",
                    true,
                    |id, mesh, components| {
                        let points = selected_valid_points(mesh, components);
                        if points.len() < 2 {
                            return Err("select at least two points to optimize".into());
                        }
                        let Some((out, kept)) = lc_geom::optimize_points(mesh, &points, tolerance)
                        else {
                            return Err(
                                "no selected points sit within the optimize tolerance".into()
                            );
                        };
                        survivors.borrow_mut().push((id, kept));
                        Ok(out)
                    },
                );
                if self.history.depth() == depth {
                    return;
                }
                let survivors = survivors.into_inner();
                for (id, kept) in &survivors {
                    self.mesh_components.insert(*id, kept.clone());
                }
                self.mesh_selected = self
                    .selection
                    .primary()
                    .and_then(|primary| {
                        survivors
                            .iter()
                            .find(|(id, _)| *id == primary)
                            .map(|(_, kept)| kept.clone())
                    })
                    .unwrap_or_default();
            }
            MeshAction::PointSlide => {
                if !self.mesh_point_slide_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Points {
                        "Point Slide is only available in point mode".into()
                    } else if !self.mesh_slide_fraction.is_finite()
                        || self.mesh_slide_fraction < 0.0
                        || self.mesh_slide_fraction > 1.0
                    {
                        "Set a slide fraction between 0 and 1 first".into()
                    } else if self.mesh_slide_fraction == 0.0 {
                        "Set a slide fraction above 0 to slide points".into()
                    } else if self
                        .selection
                        .ids()
                        .iter()
                        .all(|id| self.mesh_components_for(*id).is_empty())
                    {
                        "Select at least one point to slide".into()
                    } else {
                        "No selected point has neighbours to slide toward".into()
                    };
                    return;
                }
                let fraction = self.mesh_slide_fraction;
                self.apply_mesh_operation_selection("Slid points", false, |_, mesh, components| {
                    let points = selected_valid_points(mesh, components);
                    if points.is_empty() {
                        return Err("select at least one point to slide".into());
                    }
                    lc_geom::slide_points(mesh, &points, fraction)
                        .ok_or_else(|| "no selected point has neighbours to slide toward".into())
                });
            }
            MeshAction::Bridge => {
                if let Some(status) = self.mesh_bridge_failure() {
                    self.status = status;
                    return;
                }
                self.apply_mesh_operation("Bridged edge loops", |_, mesh, edges| {
                    let (loop_a, loop_b) = selected_bridge_loops(mesh, edges)?;
                    lc_geom::bridge_loops(mesh, &loop_a, &loop_b)
                });
            }
            MeshAction::FillHole => {
                if !self.mesh_fill_hole_ready() {
                    self.status = if self.mesh_component_selection_ready() {
                        "Select a point on a hole border first".into()
                    } else {
                        "Enter component mode with editable meshes first".into()
                    };
                    return;
                }
                let mode = self.mesh_select_mode;
                self.apply_mesh_operation("Filled hole", move |_, mesh, components| {
                    let points = mesh_affected_points(mesh, mode, components);
                    let holes = lc_geom::open_holes(mesh, &points);
                    if holes.is_empty() {
                        return Err("select a point on a hole border".into());
                    }
                    lc_geom::fill_holes(mesh, &holes)
                });
            }
            MeshAction::Split => self.split_selected_polygons(),
            MeshAction::Shell => {
                if !self.mesh_shell_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Polygons {
                        "Shell is only available in polygon mode".into()
                    } else if self.mesh_shell_thickness <= 1e-6 {
                        "Set a positive shell thickness first; Inside picks the side".into()
                    } else {
                        "Select at least one polygon to shell".into()
                    };
                    return;
                }
                let thickness = self.mesh_shell_thickness;
                let inside = self.mesh_shell_inside;
                let angle = self.mesh_shell_angle;
                let quantize = self.mesh_shell_quantize;
                self.apply_mesh_operation("Shelled polygons", move |_, mesh, components| {
                    let faces = selected_valid_polygons(mesh, components);
                    lc_geom::shell_faces(mesh, &faces, thickness, inside, angle, quantize)
                });
            }
            MeshAction::ToggleCrease => {
                if !self.require_mesh_selection(MeshSelectMode::Edges, 1, "toggle creases") {
                    return;
                }
                self.apply_mesh_operation_selection(
                    "Toggled edge creases",
                    false,
                    |_, mesh, selected| {
                        let edges = mesh.edges();
                        let mut updated = mesh.clone();
                        for &index in selected {
                            if let Some(edge) = edges.get(index as usize) {
                                updated.toggle_crease(edge.a, edge.b);
                            }
                        }
                        Ok(updated)
                    },
                );
            }
            MeshAction::SetCreaseWeight => {
                if !self.require_mesh_selection(MeshSelectMode::Edges, 1, "set crease weights") {
                    return;
                }
                let weight = self.mesh_crease_weight;
                self.apply_mesh_operation_selection(
                    "Set edge crease weights",
                    false,
                    move |_, mesh, selected| {
                        let edges = mesh.edges();
                        let mut updated = mesh.clone();
                        for &index in selected {
                            if let Some(edge) = edges.get(index as usize) {
                                updated.set_crease_weight(edge.a, edge.b, weight);
                            }
                        }
                        Ok(updated)
                    },
                );
            }
            MeshAction::Subdivide => {
                if !self.mesh_subdivide_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Polygons {
                        "Subdivide is only available in polygon mode".into()
                    } else {
                        "Select at least one polygon to subdivide".into()
                    };
                    return;
                }
                let levels = self.mesh_subdivide_levels.clamp(1, 6);
                let scheme = self.mesh_subdivide_scheme;
                self.apply_mesh_operation(
                    "Subdivided selected polygons",
                    move |_, mesh, components| {
                        let faces = selected_valid_polygons(mesh, components);
                        if faces.is_empty() {
                            return Err("select at least one polygon to subdivide".into());
                        }
                        let faces = faces.iter().map(|face| *face as usize).collect::<Vec<_>>();
                        Ok(lc_geom::subdivide_faces(mesh, &faces, levels, scheme))
                    },
                );
            }
            MeshAction::Clone => {
                if !self.mesh_clone_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else {
                        "Clone is only available in point or polygon mode".into()
                    };
                    return;
                }
                let options = self.mesh_clone;
                let mode = self.mesh_select_mode;
                self.apply_mesh_operation_selection(
                    "Cloned selection",
                    false,
                    move |_, mesh, selected| match mode {
                        MeshSelectMode::Points => {
                            Ok(lc_geom::clone_points(mesh, selected, &options))
                        }
                        MeshSelectMode::Polygons => {
                            Ok(lc_geom::clone_faces(mesh, selected, &options))
                        }
                        MeshSelectMode::Edges => Err("clone needs point or polygon mode".into()),
                    },
                );
            }
            MeshAction::ToggleSeam => {
                if !self.require_mesh_selection(MeshSelectMode::Edges, 1, "toggle UV seams") {
                    return;
                }
                self.apply_mesh_operation_selection(
                    "Toggled UV seams",
                    false,
                    |_, mesh, selected| {
                        let edges = mesh.edges();
                        let mut updated = mesh.clone();
                        for &index in selected {
                            if let Some(edge) = edges.get(index as usize) {
                                updated.toggle_seam(edge.a, edge.b);
                            }
                        }
                        Ok(updated)
                    },
                );
            }
            MeshAction::Triangulate => {
                if !self.mesh_triangulate_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Polygons {
                        "Triangulate is only available in polygon mode".into()
                    } else {
                        "Select at least one polygon with more than three corners to triangulate"
                            .into()
                    };
                    return;
                }
                let mode = self.triangulate_mode;
                self.apply_mesh_operation("Triangulated polygons", move |_, mesh, components| {
                    let faces = selected_valid_polygons(mesh, components);
                    if faces.is_empty() {
                        return Err(
                            "select at least one polygon with more than three corners to triangulate"
                                .into(),
                        );
                    }
                    Ok(lc_geom::triangulate_faces_with(mesh, &faces, mode))
                });
            }
            MeshAction::Scalpel => {
                if !self.mesh_scalpel_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if !matches!(
                        self.mesh_select_mode,
                        MeshSelectMode::Points | MeshSelectMode::Edges
                    ) {
                        "Scalpel works in point or edge mode".into()
                    } else {
                        "Select two edges or two points of one polygon to cut".into()
                    };
                    return;
                }
                let mode = self.mesh_select_mode;
                self.apply_mesh_operation("Cut polygon", move |_, mesh, components| {
                    let (face, from, to) = scalpel_cut(mesh, mode, components)?;
                    lc_geom::scalpel_face(mesh, face, from, to)
                });
            }
            MeshAction::FlipNormal => {
                if !self.mesh_flip_normal_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Polygons {
                        "Flip Normal is only available in polygon mode".into()
                    } else {
                        "Select at least one polygon to flip".into()
                    };
                    return;
                }
                // Face indices do not change, so the selection stays and the
                // same command flips the faces back.
                self.apply_mesh_operation_selection(
                    "Flipped polygon normals",
                    false,
                    |_, mesh, components| {
                        let faces = selected_valid_polygons(mesh, components);
                        if faces.is_empty() {
                            return Err("select at least one polygon to flip".into());
                        }
                        Ok(lc_geom::flip_faces(mesh, &faces))
                    },
                );
            }
            MeshAction::Disconnect => {
                if !self.mesh_disconnect_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Polygons {
                        "Disconnect is only available in polygon mode".into()
                    } else {
                        "Select polygons that border the rest of the mesh to disconnect".into()
                    };
                    return;
                }
                // Face indices do not change, so the selection stays exactly as
                // it was and now names the detached island.
                self.apply_mesh_operation_selection(
                    "Disconnected polygons",
                    false,
                    |_, mesh, components| {
                        let faces = selected_valid_polygons(mesh, components);
                        if faces.is_empty() {
                            return Err("select at least one polygon to disconnect".into());
                        }
                        Ok(lc_geom::disconnect_faces(mesh, &faces))
                    },
                );
            }
            MeshAction::Cover => {
                if !self.mesh_cover_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Polygons {
                        "Cover is only available in polygon mode".into()
                    } else {
                        "Select the polygons that border an open hole on every side to cover it"
                            .into()
                    };
                    return;
                }
                // The fill appends faces, so the handler selects them once the
                // command has actually been pushed.
                let created = std::cell::RefCell::new(Vec::new());
                let depth = self.history.depth();
                self.apply_mesh_operation_selection(
                    "Covered holes",
                    true,
                    |id, mesh, components| {
                        let faces = selected_valid_polygons(mesh, components);
                        if faces.is_empty() {
                            return Err("select at least one polygon to cover".into());
                        }
                        let (out, new_faces) = lc_geom::cover_faces(mesh, &faces)?;
                        created.borrow_mut().push((id, new_faces));
                        Ok(out)
                    },
                );
                if self.history.depth() == depth {
                    return;
                }
                let created = created.into_inner();
                for (id, faces) in &created {
                    if !faces.is_empty() {
                        self.mesh_components.insert(*id, faces.clone());
                    }
                }
                self.mesh_selected = self
                    .selection
                    .primary()
                    .and_then(|primary| {
                        created
                            .iter()
                            .find(|(id, _)| *id == primary)
                            .map(|(_, faces)| faces.clone())
                    })
                    .unwrap_or_default();
            }
            MeshAction::CreatePolygon => {
                if !self.mesh_create_polygon_ready() {
                    self.status = if !self.mesh_component_selection_ready() {
                        "Enter component mode with editable meshes first".into()
                    } else if self.mesh_select_mode != MeshSelectMode::Points {
                        "Create Polygon is only available in point mode".into()
                    } else {
                        "Select the boundary points that close an open region to create a polygon"
                            .into()
                    };
                    return;
                }
                // New faces are appended, so the handler selects them once the
                // command has actually been pushed.
                let created = std::cell::RefCell::new(Vec::new());
                let depth = self.history.depth();
                self.apply_mesh_operation_selection(
                    "Created polygons",
                    true,
                    |id, mesh, components| {
                        let points = selected_valid_points(mesh, components);
                        if points.len() < 3 {
                            return Err(
                                "select at least three boundary points that close an open region"
                                    .into(),
                            );
                        }
                        let (out, new_faces) = lc_geom::create_polygon_faces(mesh, &points)?;
                        created.borrow_mut().push((id, new_faces));
                        Ok(out)
                    },
                );
                if self.history.depth() == depth {
                    return;
                }
                let created = created.into_inner();
                for (id, faces) in &created {
                    if !faces.is_empty() {
                        self.mesh_components.insert(*id, faces.clone());
                    }
                }
                self.mesh_selected = self
                    .selection
                    .primary()
                    .and_then(|primary| {
                        created
                            .iter()
                            .find(|(id, _)| *id == primary)
                            .map(|(_, faces)| faces.clone())
                    })
                    .unwrap_or_default();
            }
            MeshAction::TogglePinnedUv => self.toggle_selected_uv_pins(),
            MeshAction::AssignMaterial => self.assign_material_to_selected_faces(),
        }
    }

    pub(super) fn require_mesh_selection(
        &mut self,
        mode: MeshSelectMode,
        minimum: usize,
        action: &str,
    ) -> bool {
        if self.mesh_topology_ready(mode, minimum) {
            return true;
        }
        self.status = self.mesh_tool_refusal(mode, minimum, action);
        false
    }

    /// Explains why the shared component-tool rule rejected the selection.
    pub(super) fn mesh_tool_refusal(
        &self,
        mode: MeshSelectMode,
        minimum: usize,
        action: &str,
    ) -> String {
        if !self.selected_meshes_editable() {
            return if self.selection.is_empty() {
                "Select an editable mesh first".into()
            } else {
                "Select only editable mesh objects first".into()
            };
        }
        if !self.mesh_edit {
            return "Enter component mode first".into();
        }
        if self.mesh_select_mode != mode {
            return format!("Switch to {} selection to {action}", mode.label());
        }
        let label = mode.label().to_lowercase();
        if minimum <= 1 {
            if self.selection.len() == 1 {
                format!("Select {label} first")
            } else {
                format!("Select {label} on every selected mesh first")
            }
        } else if self.selection.len() == 1 {
            format!("Select at least {minimum} {label} to {action}")
        } else {
            format!("Select at least {minimum} {label} on every selected mesh to {action}")
        }
    }

    pub(super) fn apply_mesh_operation(
        &mut self,
        message: &str,
        operation: impl Fn(NodeId, &Mesh, &[u32]) -> Result<Mesh, String>,
    ) {
        self.apply_mesh_operation_selection(message, true, operation);
    }

    pub(super) fn apply_mesh_operation_selection(
        &mut self,
        message: &str,
        clear_selection: bool,
        operation: impl Fn(NodeId, &Mesh, &[u32]) -> Result<Mesh, String>,
    ) {
        if !self.mesh_edit {
            self.status = "Enter component mode first".into();
            return;
        }
        if !self.selected_meshes_editable() {
            self.status = if self.selection.is_empty() {
                "Select an editable mesh first".into()
            } else {
                "Select only editable mesh objects first".into()
            };
            return;
        }
        let mut edits = Vec::new();
        for id in self.selection.ids().to_vec() {
            let Some(node) = self.doc.scene.get(id) else {
                return;
            };
            let before = Shape::of(node);
            let NodeKind::Mesh {
                geometry: GeometrySource::Editable { mesh, .. },
            } = &node.kind
            else {
                return;
            };
            let mesh = mesh.clone();
            let components = self.mesh_components_for(id).to_vec();
            let updated = match operation(id, &mesh, &components) {
                Ok(updated) if updated == mesh => {
                    self.status = format!("{message}: no change");
                    return;
                }
                Ok(updated) => updated,
                Err(error) => {
                    self.status = format!("{message} failed: {error}");
                    return;
                }
            };
            if let Err(errors) = updated.validate() {
                self.status = format!("Mesh edit rejected: {}", errors.join("; "));
                return;
            }
            let mut after = before.clone();
            after.kind = match after.kind {
                NodeKind::Mesh { geometry } => NodeKind::Mesh {
                    geometry: geometry.with_mesh(updated),
                },
                other => other,
            };
            edits.push((id, before, after));
        }
        if edits.is_empty() {
            self.status = "Select an editable mesh first".into();
            return;
        }
        self.history
            .execute(&mut self.doc, Box::new(SetShapesCmd { edits }));
        if clear_selection {
            self.clear_mesh_component_selection();
        }
        self.status = message.to_string();
    }

    /// Copy the selected polygons into a new object and leave the source mesh
    /// untouched, exactly the documented Split behaviour (sources 39 and 40).
    pub(super) fn split_selected_polygons(&mut self) {
        if !self.mesh_split_ready() {
            self.status = if !self.mesh_component_selection_ready() {
                "Enter component mode with editable meshes first".into()
            } else if self.mesh_select_mode != MeshSelectMode::Polygons {
                "Split is only available in polygon mode".into()
            } else {
                "Select at least one polygon to split".into()
            };
            return;
        }
        let mut nodes: Vec<(Node, Option<NodeId>)> = Vec::new();
        let mut polygons = 0usize;
        for id in self.selection.ids().to_vec() {
            let Some(source) = self.doc.scene.get(id).cloned() else {
                continue;
            };
            let NodeKind::Mesh {
                geometry: GeometrySource::Editable { mesh, .. },
            } = &source.kind
            else {
                continue;
            };
            let faces = selected_valid_polygons(mesh, self.mesh_components_for(id));
            let part = match lc_geom::extract_faces(mesh, &faces) {
                Ok(part) => part,
                Err(error) => {
                    self.status = format!("Split failed: {error}");
                    return;
                }
            };
            polygons += part.polygons.len();
            // Source 39 copies "all tags and selections" to the new object. The
            // modifier stack, skin, morph targets and object animation stay
            // behind: the manual is silent and the new object holds raw geometry.
            let mut node = self.doc.create_node(
                NodeKind::Mesh {
                    geometry: GeometrySource::editable(part),
                },
                &format!("{} Split", source.name),
            );
            node.transform = source.transform;
            node.visible = source.visible;
            node.renderable = source.renderable;
            node.locked = source.locked;
            node.layer = source.layer;
            node.material = source.material;
            node.material_slots = source.material_slots.clone();
            node.tags = source.tags.clone();
            node.tag_animations = source.tag_animations.clone();
            nodes.push((node, source.parent));
        }
        if nodes.is_empty() {
            self.status = "Select an editable mesh first".into();
            return;
        }
        let ids: Vec<NodeId> = nodes.iter().map(|(node, _)| node.id).collect();
        let name = nodes[0].0.name.clone();
        let objects = nodes.len();
        self.history.execute(
            &mut self.doc,
            Box::new(lc_scene::AddNodesCmd {
                label: if objects == 1 {
                    "Split".into()
                } else {
                    format!("Split {objects} Objects")
                },
                nodes,
            }),
        );
        self.selection.set(ids);
        self.clear_mesh_component_selection();
        self.status = if objects == 1 {
            format!(
                "Split {polygons} polygon{} into \"{name}\"",
                if polygons == 1 { "" } else { "s" }
            )
        } else {
            format!("Split {polygons} polygons into {objects} new objects")
        };
    }

    pub(super) fn mesh_transform_selection(&mut self) {
        if !self.mesh_transform_ready() {
            self.status = if self.mesh_component_selection_ready() {
                "Select components to transform on every selected mesh".into()
            } else {
                "Enter component mode with editable meshes first".into()
            };
            return;
        }
        let mode = self.mesh_select_mode;
        let space = self.mesh_space;
        let axis = self.mesh_axis;
        let transform = self.mesh_transform;
        let value = self.mesh_value;
        let rotations: HashMap<NodeId, Quat> = self
            .selection
            .ids()
            .iter()
            .map(|id| {
                (
                    *id,
                    self.doc
                        .scene
                        .world_matrix_plain(*id)
                        .to_scale_rotation_translation()
                        .1,
                )
            })
            .collect();
        self.apply_mesh_operation_selection(
            &format!("Transformed selected {}", mode.label().to_lowercase()),
            false,
            move |id, mesh, components| {
                let affected: HashSet<u32> = mesh_affected_points(mesh, mode, components)
                    .into_iter()
                    .collect();
                if affected.is_empty() {
                    return Err("select mesh components to transform".into());
                }
                let center = affected
                    .iter()
                    .filter_map(|p| mesh.positions.get(*p as usize).copied())
                    .sum::<Vec3>()
                    / affected.len() as f32;
                let local_axis = mesh_transform_axis(
                    mesh,
                    &affected,
                    space,
                    axis,
                    rotations.get(&id).copied().unwrap_or(Quat::IDENTITY),
                );
                let mut out = mesh.clone();
                match transform {
                    MeshTransformMode::Move => {
                        for &p in &affected {
                            if let Some(position) = out.positions.get_mut(p as usize) {
                                *position += local_axis * value;
                            }
                        }
                    }
                    MeshTransformMode::Rotate => {
                        let rotation = Quat::from_axis_angle(local_axis, value.to_radians());
                        for &p in &affected {
                            if let Some(position) = out.positions.get_mut(p as usize) {
                                *position = center + rotation * (*position - center);
                            }
                        }
                    }
                    MeshTransformMode::Scale => {
                        for &p in &affected {
                            if let Some(position) = out.positions.get_mut(p as usize) {
                                let offset = *position - center;
                                *position =
                                    center + offset + local_axis * offset.dot(local_axis) * value;
                            }
                        }
                    }
                }
                Ok(out)
            },
        );
    }

    pub(super) fn toggle_mesh_selection(&mut self, index: u32, additive: bool) {
        if additive {
            if let Some(position) = self.mesh_selected.iter().position(|i| *i == index) {
                self.mesh_selected.remove(position);
            } else {
                self.mesh_selected.push(index);
            }
        } else {
            self.mesh_selected = vec![index];
        }
        if let Some(id) = self.selection.primary() {
            self.mesh_components.insert(id, self.mesh_selected.clone());
        }
    }

    pub(super) fn assign_material_to_selected_faces(&mut self) {
        if !self.mesh_assign_material_ready() {
            self.status = if !self.mesh_component_selection_ready() {
                "Enter component mode with editable meshes first".into()
            } else if self.mesh_select_mode != MeshSelectMode::Polygons {
                "Switch to Polygon selection to assign a material".into()
            } else if self.selected_material_id().is_none() {
                "Select a material in the Materials tab first".into()
            } else if self.selection.len() == 1 {
                "Select polygons before assigning a material".into()
            } else {
                "Select polygons on every selected mesh before assigning a material".into()
            };
            return;
        }
        let Some(material_id) = self.selected_material_id() else {
            return;
        };
        let before = self.doc.clone();
        for id in self.selection.ids().to_vec() {
            let components = self.mesh_components_for(id).to_vec();
            let Some(node) = self.doc.scene.get_mut(id) else {
                self.doc = before;
                self.status = "Assign material failed: a mesh left the scene".into();
                return;
            };
            let NodeKind::Mesh {
                geometry: GeometrySource::Editable { mesh, .. },
            } = &mut node.kind
            else {
                self.doc = before;
                self.status = "Assign material failed: not an editable mesh".into();
                return;
            };
            let slot = node
                .material_slots
                .iter()
                .find(|(_, material)| **material == material_id)
                .map(|(slot, _)| *slot)
                .unwrap_or_else(|| {
                    node.material_slots
                        .keys()
                        .next_back()
                        .map(|slot| slot.saturating_add(1))
                        .unwrap_or(0)
                });
            node.material_slots.insert(slot, material_id);
            for index in components {
                if let Some(poly) = mesh.polygons.get_mut(index as usize) {
                    poly.material_slot = slot;
                }
            }
        }
        let after = self.doc.clone();
        let count = self.selection.len();
        self.history.execute(
            &mut self.doc,
            Box::new(ReplaceDocumentCmd {
                label: if count == 1 {
                    "Assign polygon material".into()
                } else {
                    format!("Assign material on {count} Objects")
                },
                before,
                after,
            }),
        );
        self.status = if count == 1 {
            format!("Assigned material {material_id} to selected polygons")
        } else {
            format!("Assigned material {material_id} on {count} objects")
        };
    }

    /// Nearest point that lies on an open boundary, used by the click-driven
    /// Fill Hole tool. Picking ignores the current component mode because the
    /// manual's tool is available in all three modes.
    pub(super) fn pick_boundary_point(
        &self,
        mesh: &Mesh,
        world: Mat4,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
        pointer: Pos2,
    ) -> Option<u32> {
        let boundary: HashSet<u32> = mesh.boundary_loops().into_iter().flatten().collect();
        let mut best: Option<(f32, u32)> = None;
        for (i, p) in mesh.positions.iter().enumerate() {
            if !boundary.contains(&(i as u32)) {
                continue;
            }
            let Some(screen) = self.project(world.transform_point3(*p), rect, pixel_w, pixel_h)
            else {
                continue;
            };
            let distance = (screen - pointer).length();
            if distance < 10.0 && best.map(|(d, _)| distance < d).unwrap_or(true) {
                best = Some((distance, i as u32));
            }
        }
        best.map(|(_, i)| i)
    }

    /// Fill the hole whose border contains `point`, without changing the
    /// component selection first. The result is one undo step and the stale
    /// component selection is cleared, matching the selection-driven command.
    pub(super) fn fill_hole_at(&mut self, point: u32) {
        if !self.mesh_edit {
            self.status = "Enter component mode first".into();
            return;
        }
        let Some((id, mesh)) = self.selected_editable_mesh() else {
            self.status = "Select an editable mesh first".into();
            return;
        };
        let holes = lc_geom::open_holes(&mesh, &[point]);
        if holes.is_empty() {
            self.status = "Click a point on an open hole border".into();
            return;
        }
        let updated = match lc_geom::fill_holes(&mesh, &holes) {
            Ok(updated) if updated == mesh => {
                self.status = "Filled hole: no change".into();
                return;
            }
            Ok(updated) => updated,
            Err(error) => {
                self.status = format!("Filled hole failed: {error}");
                return;
            }
        };
        if let Err(errors) = updated.validate() {
            self.status = format!("Mesh edit rejected: {}", errors.join("; "));
            return;
        }
        let Some(node) = self.doc.scene.get(id) else {
            return;
        };
        let before = Shape::of(node);
        let mut after = before.clone();
        after.kind = match after.kind {
            NodeKind::Mesh { geometry } => NodeKind::Mesh {
                geometry: geometry.with_mesh(updated),
            },
            other => other,
        };
        self.history.execute(
            &mut self.doc,
            Box::new(SetShapesCmd {
                edits: vec![(id, before, after)],
            }),
        );
        self.clear_mesh_component_selection();
        self.status = "Filled hole".into();
    }

    /// The pick ray in the mesh's local space, shared by the Scalpel's edge
    /// parameter and face hit tests.
    fn pointer_ray(
        &self,
        world: Mat4,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
        pointer: Pos2,
    ) -> lc_core::Ray {
        let pixel = glam::Vec2::new(
            (pointer.x - rect.min.x) / rect.width().max(1.0) * pixel_w as f32,
            (pointer.y - rect.min.y) / rect.height().max(1.0) * pixel_h as f32,
        );
        let mut rng = Pcg32::new(1);
        let ray = lc_render::RenderCamera::new(self.orbit.world(), self.orbit.camera()).ray(
            pixel,
            glam::Vec2::new(pixel_w as f32, pixel_h as f32),
            &mut rng,
        );
        let inv = world.inverse();
        lc_core::Ray::new(
            inv.transform_point3(ray.origin),
            inv.transform_vector3(ray.dir),
        )
    }

    /// Pick the Scalpel's cut end under the pointer: the nearest point within
    /// the point tolerance, otherwise the nearest edge segment and the point on
    /// it closest to the pick ray. Points win, so a click on a corner is a point
    /// pick.
    pub(super) fn pick_scalpel_pick(
        &self,
        mesh: &Mesh,
        world: Mat4,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
        pointer: Pos2,
    ) -> Option<lc_geom::ScalpelPick> {
        let mut best_point: Option<(f32, u32)> = None;
        for (i, p) in mesh.positions.iter().enumerate() {
            let Some(screen) = self.project(world.transform_point3(*p), rect, pixel_w, pixel_h)
            else {
                continue;
            };
            let distance = (screen - pointer).length();
            if distance < 10.0 && best_point.map(|(d, _)| distance < d).unwrap_or(true) {
                best_point = Some((distance, i as u32));
            }
        }
        if let Some((_, point)) = best_point {
            return Some(lc_geom::ScalpelPick::Point(point));
        }
        let ray = self.pointer_ray(world, rect, pixel_w, pixel_h, pointer);
        let mut best_edge: Option<(f32, u32, u32, f32)> = None;
        for edge in mesh.edges() {
            if edge.a == edge.b {
                continue;
            }
            let (Some(from), Some(to), Some(start), Some(end)) = (
                mesh.positions.get(edge.a as usize),
                mesh.positions.get(edge.b as usize),
                mesh.positions
                    .get(edge.a as usize)
                    .and_then(|p| self.project(world.transform_point3(*p), rect, pixel_w, pixel_h)),
                mesh.positions
                    .get(edge.b as usize)
                    .and_then(|p| self.project(world.transform_point3(*p), rect, pixel_w, pixel_h)),
            ) else {
                continue;
            };
            let segment = end - start;
            let length_sq = segment.length_sq();
            if length_sq <= 1e-6 {
                continue;
            }
            let screen_t = ((pointer - start).dot(segment) / length_sq).clamp(0.0, 1.0);
            let distance = (start + segment * screen_t - pointer).length();
            if distance < 8.0 && best_edge.map(|(d, ..)| distance < d).unwrap_or(true) {
                // The picked position is the point on the 3D edge nearest the
                // pick ray, so clicking a projected midpoint inserts the true
                // midpoint instead of a screen-space approximation.
                let t = closest_segment_parameter(&ray, *from, *to).unwrap_or(screen_t);
                best_edge = Some((distance, edge.a, edge.b, t));
            }
        }
        best_edge.map(|(_, a, b, t)| lc_geom::ScalpelPick::Edge { a, b, t })
    }

    /// The polygon under the pointer, used to choose which face a chained
    /// Scalpel cut splits when the two picks alone belong to more than one.
    pub(super) fn pick_mesh_face(
        &self,
        mesh: &Mesh,
        world: Mat4,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
        pointer: Pos2,
    ) -> Option<u32> {
        let local = self.pointer_ray(world, rect, pixel_w, pixel_h, pointer);
        let mut best: Option<(f32, u32)> = None;
        for (face, poly) in mesh.polygons.iter().enumerate() {
            for tri in mesh.polygon_triangles(face) {
                let points = [tri[0], tri[1], tri[2]].map(|corner| poly.corners[corner].point);
                let (Some(a), Some(b), Some(c)) = (
                    mesh.positions.get(points[0] as usize),
                    mesh.positions.get(points[1] as usize),
                    mesh.positions.get(points[2] as usize),
                ) else {
                    continue;
                };
                if let Some((t, _, _)) = ray_triangle(&local, *a, *b, *c) {
                    if t > 1e-5 && best.map(|(nearest, _)| t < nearest).unwrap_or(true) {
                        best = Some((t, face as u32));
                    }
                }
            }
        }
        best.map(|(_, face)| face)
    }

    /// Apply one Scalpel click: the first pick starts a cut, the second splits
    /// the polygon that holds both picks, and the cut's end point becomes the
    /// next start until ESC (source 43). Each completed cut is one undo step.
    pub(super) fn scalpel_click_at(
        &mut self,
        pick: lc_geom::ScalpelPick,
        under_pointer: Option<u32>,
    ) {
        if !self.mesh_edit {
            self.status = "Enter component mode first".into();
            return;
        }
        let Some((id, mesh)) = self.selected_editable_mesh() else {
            self.status = "Select an editable mesh first".into();
            return;
        };
        if !scalpel_pick_on_any_face(&mesh, pick) {
            self.status = "Click a point or edge of a polygon".into();
            return;
        }
        // Another edit can replace the mesh without going through undo, so drop
        // a start the current mesh no longer has and let this click begin a new
        // cut instead of refusing every click against the stale start.
        let stale = self
            .scalpel_start
            .is_some_and(|start| !scalpel_pick_on_any_face(&mesh, start));
        if stale {
            self.scalpel_start = None;
        }
        let Some(start) = self.scalpel_start else {
            self.scalpel_start = Some(pick);
            self.status = if stale {
                "Scalpel: the previous start no longer exists; this click starts a new cut".into()
            } else {
                "Scalpel start set: click a second point or edge of the same polygon".into()
            };
            return;
        };
        let Some(face) = scalpel_chain_face(&mesh, under_pointer, start, pick) else {
            self.status =
                "Scalpel: the second pick must be on the polygon that holds the cut start".into();
            return;
        };
        let (updated, _, end) = match lc_geom::scalpel_face_ends(&mesh, face, start, pick) {
            Ok(result) => result,
            Err(error) => {
                self.status = format!("Scalpel failed: {error}");
                return;
            }
        };
        if let Err(errors) = updated.validate() {
            self.status = format!("Mesh edit rejected: {}", errors.join("; "));
            return;
        }
        let Some(node) = self.doc.scene.get(id) else {
            return;
        };
        let before = Shape::of(node);
        let mut after = before.clone();
        after.kind = match after.kind {
            NodeKind::Mesh { geometry } => NodeKind::Mesh {
                geometry: geometry.with_mesh(updated),
            },
            other => other,
        };
        self.history.execute(
            &mut self.doc,
            Box::new(SetShapesCmd {
                edits: vec![(id, before, after)],
            }),
        );
        self.clear_mesh_component_selection();
        self.scalpel_start = Some(lc_geom::ScalpelPick::Point(end));
        self.status = "Cut polygon: the end point is the next start (Esc resets)".into();
    }

    /// ESC resets the chained cut without leaving the tool.
    pub(super) fn scalpel_cancel(&mut self) {
        self.scalpel_start = None;
        self.status = "Scalpel reset: click a point or edge to start a cut".into();
    }

    pub(super) fn pick_mesh_component(
        &self,
        mesh: &Mesh,
        world: Mat4,
        rect: Rect,
        pixel_w: u32,
        pixel_h: u32,
        pointer: Pos2,
    ) -> Option<u32> {
        match self.mesh_select_mode {
            MeshSelectMode::Points => {
                let mut best: Option<(f32, u32)> = None;
                for (i, p) in mesh.positions.iter().enumerate() {
                    let Some(screen) =
                        self.project(world.transform_point3(*p), rect, pixel_w, pixel_h)
                    else {
                        continue;
                    };
                    let distance = (screen - pointer).length();
                    if distance < 10.0 && best.map(|(d, _)| distance < d).unwrap_or(true) {
                        best = Some((distance, i as u32));
                    }
                }
                best.map(|(_, i)| i)
            }
            MeshSelectMode::Edges => {
                let mut best: Option<(f32, u32)> = None;
                for (i, edge) in mesh.edges().iter().enumerate() {
                    let Some(a) = mesh.positions.get(edge.a as usize) else {
                        continue;
                    };
                    let Some(b) = mesh.positions.get(edge.b as usize) else {
                        continue;
                    };
                    let Some(a) = self.project(world.transform_point3(*a), rect, pixel_w, pixel_h)
                    else {
                        continue;
                    };
                    let Some(b) = self.project(world.transform_point3(*b), rect, pixel_w, pixel_h)
                    else {
                        continue;
                    };
                    let distance = point_segment_distance(pointer, a, b);
                    if distance < 8.0 && best.map(|(d, _)| distance < d).unwrap_or(true) {
                        best = Some((distance, i as u32));
                    }
                }
                best.map(|(_, i)| i)
            }
            MeshSelectMode::Polygons => {
                let mut best: Option<(f32, u32)> = None;
                for (i, poly) in mesh.polygons.iter().enumerate() {
                    let points: Vec<Pos2> = poly
                        .corners
                        .iter()
                        .filter_map(|c| mesh.positions.get(c.point as usize).copied())
                        .filter_map(|p| {
                            self.project(world.transform_point3(p), rect, pixel_w, pixel_h)
                        })
                        .collect();
                    if points.len() < 3 || !point_in_polygon(pointer, &points) {
                        continue;
                    }
                    let center = points.iter().fold(Vec2::ZERO, |acc, p| acc + p.to_vec2())
                        / points.len() as f32;
                    let distance = (center - pointer.to_vec2()).length();
                    if best.map(|(d, _)| distance < d).unwrap_or(true) {
                        best = Some((distance, i as u32));
                    }
                }
                best.map(|(_, i)| i)
            }
        }
    }

    pub(super) fn draw_mesh_edit_overlay(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        evaluated: &EvaluatedScene,
        pixel_w: u32,
        pixel_h: u32,
    ) {
        let Some((id, mesh)) = self.selected_editable_mesh() else {
            return;
        };
        let Some(node) = evaluated.node(id) else {
            return;
        };
        let painter = ui.painter_at(rect);
        let selected: HashSet<u32> = self.mesh_selected.iter().copied().collect();
        let selected_color = Color32::from_rgb(255, 164, 64);
        let points = |points: &mut Vec<Pos2>, indices: &[u32]| {
            points.clear();
            for index in indices {
                if let Some(p) = mesh.positions.get(*index as usize) {
                    if let Some(screen) =
                        self.project(node.world.transform_point3(*p), rect, pixel_w, pixel_h)
                    {
                        points.push(screen);
                    }
                }
            }
        };
        match self.mesh_select_mode {
            MeshSelectMode::Points => {
                for (i, p) in mesh.positions.iter().enumerate() {
                    if let Some(screen) =
                        self.project(node.world.transform_point3(*p), rect, pixel_w, pixel_h)
                    {
                        let chosen = selected.contains(&(i as u32));
                        painter.circle_filled(
                            screen,
                            if chosen { 4.2 } else { 2.3 },
                            if chosen {
                                selected_color
                            } else {
                                Color32::from_rgb(130, 205, 250)
                            },
                        );
                    }
                }
            }
            MeshSelectMode::Edges => {
                for (i, edge) in mesh.edges().iter().enumerate() {
                    let Some(a) = mesh.positions.get(edge.a as usize) else {
                        continue;
                    };
                    let Some(b) = mesh.positions.get(edge.b as usize) else {
                        continue;
                    };
                    let (Some(a), Some(b)) = (
                        self.project(node.world.transform_point3(*a), rect, pixel_w, pixel_h),
                        self.project(node.world.transform_point3(*b), rect, pixel_w, pixel_h),
                    ) else {
                        continue;
                    };
                    let chosen = selected.contains(&(i as u32));
                    let seam = mesh.is_seam(edge.a, edge.b);
                    let creased = mesh.is_crease(edge.a, edge.b);
                    painter.line_segment(
                        [a, b],
                        egui::Stroke::new(
                            if chosen {
                                3.0
                            } else if seam {
                                2.5
                            } else if creased {
                                2.0
                            } else {
                                1.0
                            },
                            if chosen {
                                selected_color
                            } else if seam {
                                // Seam edges are green, as in Cheetah3D's UV tutorial.
                                Color32::from_rgb(90, 200, 110)
                            } else if creased {
                                Color32::from_rgb(90, 165, 255)
                            } else {
                                Color32::from_rgb(120, 135, 155)
                            },
                        ),
                    );
                }
            }
            MeshSelectMode::Polygons => {
                for (i, poly) in mesh.polygons.iter().enumerate() {
                    let poly_points: Vec<Pos2> = poly
                        .corners
                        .iter()
                        .filter_map(|c| mesh.positions.get(c.point as usize).copied())
                        .filter_map(|p| {
                            self.project(node.world.transform_point3(p), rect, pixel_w, pixel_h)
                        })
                        .collect();
                    if poly_points.len() < 3 {
                        continue;
                    }
                    let chosen = selected.contains(&(i as u32));
                    if chosen {
                        painter.add(egui::Shape::convex_polygon(
                            poly_points.clone(),
                            Color32::from_rgba_unmultiplied(255, 150, 50, 55),
                            egui::Stroke::new(2.0, selected_color),
                        ));
                    } else {
                        for k in 0..poly_points.len() {
                            painter.line_segment(
                                [poly_points[k], poly_points[(k + 1) % poly_points.len()]],
                                egui::Stroke::new(0.8, Color32::from_rgb(90, 105, 125)),
                            );
                        }
                    }
                }
            }
        }
        let mut handles = Vec::new();
        points(
            &mut handles,
            &mesh_affected_points(&mesh, self.mesh_select_mode, &self.mesh_selected),
        );
        for p in handles {
            painter.circle_stroke(p, 3.5, egui::Stroke::new(1.0, selected_color));
        }
    }
}
