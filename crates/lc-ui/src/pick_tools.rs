//! Viewport click tools for the polygon commands whose input is a picked
//! element: Ring Cut, Weld, Bridge and Insert Point.
//!
//! Cheetah3D documents these as pick-driven tools (sources 46-49): Ring Cut
//! takes an edge click, Weld and Bridge take two point clicks, and Insert Point
//! takes a click inside a polygon or on one of its edges. Each handler reuses
//! the shipped command paths where they exist - the plane cut, the point merge,
//! `bridge_loops` - so the menu and click forms cannot drift apart.

use super::*;

impl LeopardApp {
    /// Turn a pick tool on or off. Pick tools are exclusive with each other and
    /// with the Scalpel and Fill Hole click tools, because two tools competing
    /// for the same click would be ambiguous.
    pub(super) fn activate_pick_tool(&mut self, tool: Option<MeshPickTool>) {
        self.pick_tool = tool;
        self.pick_start = None;
        if tool.is_some() {
            self.scalpel_tool = false;
            self.scalpel_start = None;
            self.fill_hole_tool = false;
        }
    }

    pub(super) fn toggle_pick_tool(&mut self, tool: MeshPickTool) {
        let next = (self.pick_tool != Some(tool)).then_some(tool);
        self.activate_pick_tool(next);
        self.status = match next {
            Some(tool) => format!("{} tool: {}", tool.label(), tool.hint()),
            None => "Pick tool off".into(),
        };
    }

    /// ESC drops a pending first click without leaving the tool.
    pub(super) fn pick_tool_cancel(&mut self) {
        self.pick_start = None;
        self.status = match self.pick_tool {
            Some(tool) => format!("{}: {}", tool.label(), tool.hint()),
            None => "Pick tool off".into(),
        };
    }

    /// The click tools act on one editable mesh, so a multi-object selection is
    /// refused instead of editing only the primary.
    fn pick_tool_mesh(&self) -> Option<(NodeId, Mesh)> {
        if !self.mesh_edit || self.selection.len() != 1 {
            return None;
        }
        self.selected_editable_mesh()
    }

    fn pick_tool_refusal(&mut self) {
        self.status = if !self.mesh_edit {
            "Enter component mode first".into()
        } else if self.selection.len() != 1 {
            "Click tools act on one editable mesh at a time".into()
        } else {
            "Select an editable mesh first".into()
        };
    }

    fn assign_mesh_components(&mut self, id: NodeId, components: Vec<u32>) {
        self.mesh_selected = components.clone();
        self.mesh_components.insert(id, components);
    }

    /// One Ring Cut click: the picked edge chooses the cut plane.
    ///
    /// The shipped Ring Cut is an axis-aligned plane slice, so the edge picks
    /// the world axis its direction runs along and the plane through the edge's
    /// midpoint at that offset, which puts the new loop across the edge ring.
    /// The manual's drag-to-slide ring insertion (source 46) is not reproduced;
    /// `TODO.md` §6b records that gap.
    pub(super) fn ring_cut_click_at(&mut self, a: u32, b: u32) {
        let Some((_id, mesh)) = self.pick_tool_mesh() else {
            self.pick_tool_refusal();
            return;
        };
        let (Some(start), Some(end)) = (
            mesh.positions.get(a as usize).copied(),
            mesh.positions.get(b as usize).copied(),
        ) else {
            self.status = "Ring Cut: click an edge of the mesh".into();
            return;
        };
        let direction = (end - start).normalize_or_zero();
        if direction == Vec3::ZERO {
            self.status = "Ring Cut: that edge has no direction".into();
            return;
        }
        let mut axis = 0;
        for candidate in 1..3 {
            if direction[candidate].abs() > direction[axis].abs() {
                axis = candidate;
            }
        }
        let offset = (start[axis] + end[axis]) * 0.5;
        self.mesh_axis = axis;
        self.mesh_offset = offset;
        if !self.mesh_ring_cut_ready() {
            self.status = "Ring Cut: the plane through that edge does not cross the mesh".into();
            return;
        }
        let depth = self.history.depth();
        self.perform_mesh_action(MeshAction::RingCut);
        if self.history.depth() > depth {
            self.status = format!(
                "Ring cut through the clicked edge ({}-plane at {offset:.3})",
                ["X", "Y", "Z"][axis]
            );
        }
    }

    /// One Weld click: the first point waits, the second merges both at the
    /// midpoint or the first point, as the `To` property selects (source 47).
    pub(super) fn weld_click_at(&mut self, point: u32) {
        let Some((id, mesh)) = self.pick_tool_mesh() else {
            self.pick_tool_refusal();
            return;
        };
        if mesh.positions.get(point as usize).is_none() {
            self.status = "Weld: click a point of the mesh".into();
            return;
        }
        // Another edit can replace the mesh without undo, so a start the current
        // mesh no longer has is dropped and this click begins again.
        let stale = self
            .pick_start
            .is_some_and(|start| (start as usize) >= mesh.positions.len());
        if stale {
            self.pick_start = None;
        }
        let Some(first) = self.pick_start else {
            self.pick_start = Some(point);
            self.status = if stale {
                "Weld: the previous point no longer exists; this click starts again".into()
            } else {
                "Weld: click the second point".into()
            };
            return;
        };
        if first == point {
            self.status = "Weld: click a different second point".into();
            return;
        }
        self.mesh_select_mode = MeshSelectMode::Points;
        self.assign_mesh_components(id, vec![first, point]);
        let depth = self.history.depth();
        self.perform_mesh_action(MeshAction::Weld);
        if self.history.depth() == depth {
            // The command refused and set its own status; keep the first pick.
            self.pick_start = Some(first);
            return;
        }
        self.clear_mesh_component_selection();
        self.pick_start = None;
        self.status = format!(
            "Welded the two clicked points (To: {})",
            self.mesh_weld_to.label()
        );
    }

    /// One Bridge click: the two picks must sit on different open borders, whose
    /// loops are selected and handed to the shipped Bridge command (source 48).
    pub(super) fn bridge_click_at(&mut self, point: u32) {
        let Some((id, mesh)) = self.pick_tool_mesh() else {
            self.pick_tool_refusal();
            return;
        };
        let loops = mesh.boundary_loops();
        let Some(loop_index) = loops.iter().position(|ring| ring.contains(&point)) else {
            self.status = "Bridge: click a point on an open border".into();
            return;
        };
        let stale = self
            .pick_start
            .is_some_and(|start| (start as usize) >= mesh.positions.len());
        if stale {
            self.pick_start = None;
        }
        let Some(first) = self.pick_start else {
            self.pick_start = Some(point);
            self.status = if stale {
                "Bridge: the previous point no longer exists; this click starts again".into()
            } else {
                "Bridge: click a point on the other border".into()
            };
            return;
        };
        let Some(first_index) = loops.iter().position(|ring| ring.contains(&first)) else {
            self.pick_start = Some(point);
            self.status =
                "Bridge: the first border point no longer exists; this click starts again".into();
            return;
        };
        if first_index == loop_index {
            self.status = "Bridge: pick the second point on a different border".into();
            return;
        }
        let (Some(first_loop), Some(second_loop)) = (loops.get(first_index), loops.get(loop_index))
        else {
            return;
        };
        if first_loop.len() != second_loop.len() {
            self.status = format!(
                "Bridge: the borders have {} and {} points and must match",
                first_loop.len(),
                second_loop.len()
            );
            return;
        }
        let edges = mesh.edges();
        let mut selected = Vec::new();
        for ring in [first_loop, second_loop] {
            for index in 0..ring.len() {
                let a = ring[index];
                let b = ring[(index + 1) % ring.len()];
                let key = (a.min(b), a.max(b));
                if let Ok(found) = edges.binary_search_by_key(&key, |edge| (edge.a, edge.b)) {
                    selected.push(found as u32);
                }
            }
        }
        selected.sort_unstable();
        selected.dedup();
        if selected.is_empty() {
            self.status = "Bridge: those borders have no selectable edges".into();
            return;
        }
        self.mesh_select_mode = MeshSelectMode::Edges;
        self.assign_mesh_components(id, selected);
        let depth = self.history.depth();
        self.perform_mesh_action(MeshAction::Bridge);
        if self.history.depth() == depth {
            return;
        }
        self.clear_mesh_component_selection();
        self.pick_start = None;
        self.status = "Bridged the two clicked borders".into();
    }

    /// One Insert Point click, inside a polygon or on one of its edges
    /// (source 49). The new point becomes the component selection.
    pub(super) fn insert_point_click_at(&mut self, face: u32, pick: lc_geom::InsertPointPick) {
        let Some((id, mesh)) = self.pick_tool_mesh() else {
            self.pick_tool_refusal();
            return;
        };
        let (updated, point) = match lc_geom::insert_point(&mesh, face, pick) {
            Ok(result) => result,
            Err(error) => {
                self.status = format!("Insert Point failed: {error}");
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
        self.mesh_select_mode = MeshSelectMode::Points;
        self.assign_mesh_components(id, vec![point]);
        self.status = "Inserted a point; it is selected".into();
    }

    /// Nearest mesh point to the pointer, in any component mode, so a point
    /// click does not depend on which mode the panel happens to be in.
    pub(super) fn pick_mesh_point_under(
        &self,
        mesh: &Mesh,
        world: Mat4,
        view: PickView,
        pointer: Pos2,
    ) -> Option<u32> {
        let mut best: Option<(f32, u32)> = None;
        for (index, position) in mesh.positions.iter().enumerate() {
            let Some(screen) = self.project(
                world.transform_point3(*position),
                view.rect,
                view.pixel_w,
                view.pixel_h,
            ) else {
                continue;
            };
            let distance = (screen - pointer).length();
            if distance < 10.0 && best.map(|(nearest, _)| distance < nearest).unwrap_or(true) {
                best = Some((distance, index as u32));
            }
        }
        best.map(|(_, index)| index)
    }

    /// Nearest edge to the pointer with the click's position along it.
    pub(super) fn pick_mesh_edge_under(
        &self,
        mesh: &Mesh,
        world: Mat4,
        view: PickView,
        pointer: Pos2,
    ) -> Option<(u32, u32, f32)> {
        self.pick_edges_under(mesh, mesh.edges(), world, view, pointer)
            .map(|(_, a, b, t)| (a, b, t))
    }

    /// Nearest edge of one polygon, so an Insert Point click on an edge cannot
    /// land on a back-facing edge that happens to share the screen position.
    pub(super) fn pick_face_edge_under(
        &self,
        mesh: &Mesh,
        face: u32,
        world: Mat4,
        view: PickView,
        pointer: Pos2,
    ) -> Option<(u32, u32, f32)> {
        let poly = mesh.polygons.get(face as usize)?;
        let mut edges = Vec::with_capacity(poly.corners.len());
        for index in 0..poly.corners.len() {
            let a = poly.corners[index].point;
            let b = poly.corners[(index + 1) % poly.corners.len()].point;
            if a != b {
                edges.push(lc_geom::MeshEdge {
                    a: a.min(b),
                    b: a.max(b),
                    faces: vec![face],
                });
            }
        }
        self.pick_edges_under(mesh, edges, world, view, pointer)
            .map(|(_, a, b, t)| (a, b, t))
    }

    fn pick_edges_under(
        &self,
        mesh: &Mesh,
        edges: Vec<lc_geom::MeshEdge>,
        world: Mat4,
        view: PickView,
        pointer: Pos2,
    ) -> Option<(f32, u32, u32, f32)> {
        let mut best: Option<(f32, u32, u32, f32)> = None;
        for edge in edges {
            let (Some(start), Some(end)) = (
                mesh.positions.get(edge.a as usize).copied(),
                mesh.positions.get(edge.b as usize).copied(),
            ) else {
                continue;
            };
            let (Some(start_screen), Some(end_screen)) = (
                self.project(
                    world.transform_point3(start),
                    view.rect,
                    view.pixel_w,
                    view.pixel_h,
                ),
                self.project(
                    world.transform_point3(end),
                    view.rect,
                    view.pixel_w,
                    view.pixel_h,
                ),
            ) else {
                continue;
            };
            let distance = point_segment_distance(pointer, start_screen, end_screen);
            if distance >= 8.0
                || best
                    .map(|(nearest, ..)| distance >= nearest)
                    .unwrap_or(false)
            {
                continue;
            }
            let along = end_screen - start_screen;
            let length_sq = along.length_sq();
            let t = if length_sq <= 1e-6 {
                0.5
            } else {
                ((pointer - start_screen).dot(along) / length_sq).clamp(0.0, 1.0)
            };
            best = Some((distance, edge.a, edge.b, t));
        }
        best
    }

    /// Where the pointer's ray meets one polygon's plane, in the mesh's local
    /// space. This is the position an Insert Point click adds inside the face.
    pub(super) fn pick_face_surface(
        &self,
        mesh: &Mesh,
        face: u32,
        world: Mat4,
        view: PickView,
        pointer: Pos2,
    ) -> Option<Vec3> {
        let poly = mesh.polygons.get(face as usize)?;
        let origin = mesh.positions.get(poly.corners.first()?.point as usize)?;
        let normal = mesh.face_normal(face as usize);
        let ray = self.pointer_ray(world, view.rect, view.pixel_w, view.pixel_h, pointer);
        let denominator = ray.dir.dot(normal);
        if denominator.abs() < 1e-6 {
            return None;
        }
        let distance = (*origin - ray.origin).dot(normal) / denominator;
        (distance > 1e-6).then(|| ray.origin + ray.dir * distance)
    }
}
