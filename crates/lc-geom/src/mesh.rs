//! N-gon aware polygon mesh.
//!
//! Points, per-corner UVs and polygons are stored separately. A polygon can be
//! a triangle, quad or arbitrary n-gon; edges, wireframes and render buffers
//! are derived. This mirrors how box modelling tools treat topology: keep the
//! authored polygons, triangulate only for display/export/intersection.

use glam::{Mat4, Vec2, Vec3};
use lc_core::{Aabb, StableHasher};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

fn canonical_edge(a: u32, b: u32) -> [u32; 2] {
    if a < b {
        [a, b]
    } else {
        [b, a]
    }
}

/// One polygon corner: a point reference plus an independent UV reference.
/// UV seams therefore do not require duplicating points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Corner {
    pub point: u32,
    pub uv: u32,
}

impl Corner {
    pub const fn new(point: u32, uv: u32) -> Self {
        Self { point, uv }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Polygon {
    pub corners: Vec<Corner>,
    #[serde(default)]
    pub material_slot: u16,
}

impl Polygon {
    pub fn new(corners: Vec<Corner>) -> Self {
        Self {
            corners,
            material_slot: 0,
        }
    }

    pub fn points(&self) -> impl Iterator<Item = u32> + '_ {
        self.corners.iter().map(|c| c.point)
    }

    pub fn is_valid(&self) -> bool {
        self.corners.len() >= 3
    }
}

/// Editable polygon mesh.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Mesh {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub positions: Vec<Vec3>,
    /// Per-corner texture coordinates. May be empty when the mesh has no UVs.
    #[serde(default)]
    pub uvs: Vec<Vec2>,
    #[serde(default)]
    pub polygons: Vec<Polygon>,
    /// Authored sharp edges as canonical undirected point pairs. Creases are
    /// honoured by Catmull-Clark and Stam-Loop subdivision and ignored by
    /// linear subdivision.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub creases: Vec<[u32; 2]>,
    /// Authored UV seams as canonical undirected point pairs. Automatic unwrap
    /// cuts the mesh along these edges, so a seam is a chart border even where
    /// the neighbouring faces point the same way.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seams: Vec<[u32; 2]>,
    /// Pinned UV slots: indices into `uvs` whose coordinate unwrap, relax and
    /// normalize must leave exactly where the user put it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pinned_uvs: Vec<u32>,
}

/// Flat render triangle with corner attributes.
#[derive(Clone, Copy, Debug)]
pub struct RenderVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
    /// Source mesh point, used by exporters to copy skin weights and morph
    /// deltas onto the expanded render vertex.
    pub point: u32,
    /// Source polygon corner inside `RenderTriangle::polygon`.
    pub corner: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct RenderTriangle {
    pub v: [RenderVertex; 3],
    pub polygon: u32,
    pub material_slot: u16,
}

#[derive(Clone, Debug)]
pub struct MeshEdge {
    pub a: u32,
    pub b: u32,
    /// Polygons using this edge. One polygon means a mesh boundary.
    pub faces: Vec<u32>,
}

impl Mesh {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn from_polygons(
        name: impl Into<String>,
        positions: Vec<Vec3>,
        polygons: Vec<Vec<u32>>,
    ) -> Self {
        let mut mesh = Self::new(name);
        mesh.positions = positions;
        for poly in polygons {
            mesh.add_face(&poly);
        }
        mesh
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn polygon_count(&self) -> usize {
        self.polygons.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.polygons
            .iter()
            .map(|p| p.corners.len().saturating_sub(2))
            .sum()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty() || self.polygons.is_empty()
    }

    pub fn add_point(&mut self, p: Vec3) -> u32 {
        let idx = self.positions.len() as u32;
        self.positions.push(p);
        idx
    }

    pub fn add_uv(&mut self, uv: Vec2) -> u32 {
        let idx = self.uvs.len() as u32;
        self.uvs.push(uv);
        idx
    }

    /// Add a polygon referencing existing points, with UV index 0 (or 0 if
    /// the mesh has no UVs).
    pub fn add_face(&mut self, points: &[u32]) -> usize {
        let corners = points
            .iter()
            .map(|&p| Corner::new(p, 0))
            .collect::<Vec<_>>();
        self.polygons.push(Polygon::new(corners));
        self.polygons.len() - 1
    }

    /// Add a polygon with explicit per-corner UV coordinates.
    pub fn add_face_with_uvs(&mut self, points: &[u32], uvs: &[Vec2]) -> usize {
        let mut corners = Vec::with_capacity(points.len());
        for (i, &p) in points.iter().enumerate() {
            let uv = uvs.get(i).copied().unwrap_or(Vec2::ZERO);
            let uv_idx = self.add_uv(uv);
            corners.push(Corner::new(p, uv_idx));
        }
        self.polygons.push(Polygon::new(corners));
        self.polygons.len() - 1
    }

    pub fn uv_of(&self, corner: &Corner) -> Vec2 {
        self.uvs
            .get(corner.uv as usize)
            .copied()
            .unwrap_or(Vec2::ZERO)
    }

    pub fn polygon_points(&self, index: usize) -> Vec<Vec3> {
        match self.polygons.get(index) {
            Some(p) => p
                .corners
                .iter()
                .filter_map(|c| self.positions.get(c.point as usize).copied())
                .collect(),
            None => Vec::new(),
        }
    }

    pub fn polygon_center(&self, index: usize) -> Vec3 {
        let pts = self.polygon_points(index);
        if pts.is_empty() {
            Vec3::ZERO
        } else {
            pts.iter().copied().sum::<Vec3>() / pts.len() as f32
        }
    }

    /// Newell's method, stable for non-planar n-gons.
    pub fn face_normal(&self, index: usize) -> Vec3 {
        let Some(poly) = self.polygons.get(index) else {
            return Vec3::Y;
        };
        let n = poly.corners.len();
        if n < 3 {
            return Vec3::Y;
        }
        let mut normal = Vec3::ZERO;
        for i in 0..n {
            let Some(a) = self.positions.get(poly.corners[i].point as usize) else {
                continue;
            };
            let Some(b) = self.positions.get(poly.corners[(i + 1) % n].point as usize) else {
                continue;
            };
            normal.x += (a.y - b.y) * (a.z + b.z);
            normal.y += (a.z - b.z) * (a.x + b.x);
            normal.z += (a.x - b.x) * (a.y + b.y);
        }
        normal.normalize_or(Vec3::Y)
    }

    /// Smooth vertex normals, honoring a crease angle: a face only contributes
    /// to a corner normal when its normal is within `crease_deg` of the corner's
    /// own face. This gives cubes crisp edges and spheres smooth shading from
    /// one set of shared points.
    pub fn corner_normals(&self, crease_deg: f32) -> Vec<Vec<Vec3>> {
        let face_normals: Vec<Vec3> = (0..self.polygons.len())
            .map(|i| self.face_normal(i))
            .collect();
        let mut point_faces: Vec<Vec<u32>> = vec![Vec::new(); self.positions.len()];
        for (fi, poly) in self.polygons.iter().enumerate() {
            for c in &poly.corners {
                if let Some(faces) = point_faces.get_mut(c.point as usize) {
                    if !faces.contains(&(fi as u32)) {
                        faces.push(fi as u32);
                    }
                }
            }
        }
        let cos_limit = crease_deg.to_radians().cos();
        self.polygons
            .iter()
            .enumerate()
            .map(|(fi, poly)| {
                let own = face_normals[fi];
                poly.corners
                    .iter()
                    .map(|c| {
                        let mut sum = Vec3::ZERO;
                        if let Some(faces) = point_faces.get(c.point as usize) {
                            for &other in faces {
                                let n = face_normals[other as usize];
                                if n.dot(own) >= cos_limit {
                                    sum += n;
                                }
                            }
                        }
                        sum.normalize_or(own)
                    })
                    .collect()
            })
            .collect()
    }

    /// Triangulate one polygon, returning indices into its corner list.
    ///
    /// Concave n-gons are handled with ear clipping in the dominant projection
    /// plane; degenerate input falls back to a fan.
    pub fn polygon_triangles(&self, index: usize) -> Vec<[usize; 3]> {
        let Some(poly) = self.polygons.get(index) else {
            return Vec::new();
        };
        let n = poly.corners.len();
        if n < 3 {
            return Vec::new();
        }
        if n == 3 {
            return vec![[0, 1, 2]];
        }

        let normal = self.face_normal(index);
        let abs = normal.abs();
        let drop = if abs.x >= abs.y && abs.x >= abs.z {
            0
        } else if abs.y >= abs.z {
            1
        } else {
            2
        };
        let project = |p: Vec3| -> Vec2 {
            match drop {
                0 => Vec2::new(p.y, p.z),
                1 => Vec2::new(p.x, p.z),
                _ => Vec2::new(p.x, p.y),
            }
        };
        let pts: Vec<Vec2> = poly
            .corners
            .iter()
            .map(|c| project(self.positions[c.point as usize]))
            .collect();

        let mut order: Vec<usize> = (0..n).collect();
        let area2: f32 = (0..n)
            .map(|i| {
                let a = pts[i];
                let b = pts[(i + 1) % n];
                a.x * b.y - b.x * a.y
            })
            .sum();
        if area2 < 0.0 {
            order.reverse();
        }

        let cross2 = |a: Vec2, b: Vec2, c: Vec2| -> f32 {
            (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
        };
        let point_in_triangle = |p: Vec2, a: Vec2, b: Vec2, c: Vec2| -> bool {
            let d1 = cross2(a, b, p);
            let d2 = cross2(b, c, p);
            let d3 = cross2(c, a, p);
            (d1 >= -1e-9 && d2 >= -1e-9 && d3 >= -1e-9) || (d1 <= 1e-9 && d2 <= 1e-9 && d3 <= 1e-9)
        };

        let mut tris = Vec::with_capacity(n - 2);
        let mut guard = 0;
        while order.len() > 3 && guard < n * n {
            guard += 1;
            let len = order.len();
            let mut clipped = false;
            for i in 0..len {
                let i0 = order[(i + len - 1) % len];
                let i1 = order[i];
                let i2 = order[(i + 1) % len];
                let a = pts[i0];
                let b = pts[i1];
                let c = pts[i2];
                if cross2(a, b, c) <= 1e-9 {
                    continue; // reflex or collinear
                }
                let mut blocked = false;
                for &other in &order {
                    if other == i0 || other == i1 || other == i2 {
                        continue;
                    }
                    if point_in_triangle(pts[other], a, b, c) {
                        blocked = true;
                        break;
                    }
                }
                if blocked {
                    continue;
                }
                tris.push([i0, i1, i2]);
                order.remove(i);
                clipped = true;
                break;
            }
            if !clipped {
                break;
            }
        }
        if order.len() == 3 {
            tris.push([order[0], order[1], order[2]]);
        } else if order.len() > 3 {
            // Degenerate/self-intersecting polygon: fan the remainder.
            for i in 1..order.len() - 1 {
                tris.push([order[0], order[i], order[i + 1]]);
            }
        }

        // Restore authored winding for every triangle.
        for tri in &mut tris {
            let a = self.positions[poly.corners[tri[0]].point as usize];
            let b = self.positions[poly.corners[tri[1]].point as usize];
            let c = self.positions[poly.corners[tri[2]].point as usize];
            if (b - a).cross(c - a).dot(normal) < 0.0 {
                tri.swap(1, 2);
            }
        }
        tris
    }

    pub fn render_triangles(&self, crease_deg: f32) -> Vec<RenderTriangle> {
        let normals = self.corner_normals(crease_deg);
        let mut out = Vec::with_capacity(self.triangle_count());
        for (pi, poly) in self.polygons.iter().enumerate() {
            for tri in self.polygon_triangles(pi) {
                let mut v = [RenderVertex {
                    position: Vec3::ZERO,
                    normal: Vec3::Y,
                    uv: Vec2::ZERO,
                    point: 0,
                    corner: 0,
                }; 3];
                for k in 0..3 {
                    let c = &poly.corners[tri[k]];
                    v[k] = RenderVertex {
                        position: self.positions[c.point as usize],
                        normal: normals[pi][tri[k]],
                        uv: self.uv_of(c),
                        point: c.point,
                        corner: tri[k] as u32,
                    };
                }
                out.push(RenderTriangle {
                    v,
                    polygon: pi as u32,
                    material_slot: poly.material_slot,
                });
            }
        }
        out
    }

    /// Unique undirected edges with adjacent polygon lists, sorted for stable
    /// iteration and selection indices.
    pub fn edges(&self) -> Vec<MeshEdge> {
        let mut map: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        for (fi, poly) in self.polygons.iter().enumerate() {
            let n = poly.corners.len();
            for i in 0..n {
                let a = poly.corners[i].point;
                let b = poly.corners[(i + 1) % n].point;
                if a == b {
                    continue;
                }
                let key = if a < b { (a, b) } else { (b, a) };
                map.entry(key).or_default().push(fi as u32);
            }
        }
        let mut edges: Vec<MeshEdge> = map
            .into_iter()
            .map(|((a, b), faces)| MeshEdge { a, b, faces })
            .collect();
        edges.sort_by_key(|e| (e.a, e.b));
        edges
    }

    pub fn boundary_loops(&self) -> Vec<Vec<u32>> {
        // Directed boundary edges are those whose reverse does not occur.
        let mut directed: HashMap<(u32, u32), u32> = HashMap::new();
        for poly in &self.polygons {
            let n = poly.corners.len();
            for i in 0..n {
                let a = poly.corners[i].point;
                let b = poly.corners[(i + 1) % n].point;
                if a != b {
                    *directed.entry((a, b)).or_default() += 1;
                }
            }
        }
        let mut boundary: Vec<(u32, u32)> = directed
            .iter()
            .filter(|((a, b), _)| !directed.contains_key(&(*b, *a)))
            .map(|(&e, _)| e)
            .collect();
        boundary.sort();
        if boundary.is_empty() {
            return Vec::new();
        }
        let mut next: HashMap<u32, Vec<u32>> = HashMap::new();
        for (a, b) in &boundary {
            next.entry(*a).or_default().push(*b);
        }
        let mut loops = Vec::new();
        let mut used: HashMap<(u32, u32), bool> = boundary.iter().map(|e| (*e, false)).collect();
        let starts: Vec<u32> = boundary.iter().map(|e| e.0).collect();
        for start in starts {
            if next.get(&start).is_none_or(|v| v.is_empty()) {
                continue;
            }
            let mut loop_pts = Vec::new();
            let mut current = start;
            for _ in 0..boundary.len() + 1 {
                let Some(candidates) = next.get(&current) else {
                    break;
                };
                let Some(&nb) = candidates
                    .iter()
                    .find(|b| used.get(&(current, **b)).copied() == Some(false))
                else {
                    break;
                };
                used.insert((current, nb), true);
                loop_pts.push(current);
                current = nb;
                if current == start {
                    break;
                }
            }
            if loop_pts.len() >= 3 {
                loops.push(loop_pts);
            }
        }
        loops
    }

    pub fn bounds(&self) -> Aabb {
        Aabb::from_points(self.positions.iter().copied())
    }

    pub fn transform(&mut self, matrix: Mat4) {
        for p in &mut self.positions {
            *p = matrix.transform_point3(*p);
        }
    }

    /// Append another mesh, optionally transformed. UV and polygon indices are
    /// rebased; material slots are preserved.
    pub fn merge(&mut self, other: &Mesh, matrix: Mat4) {
        let point_offset = self.positions.len() as u32;
        let uv_offset = self.uvs.len() as u32;
        self.positions
            .extend(other.positions.iter().map(|p| matrix.transform_point3(*p)));
        self.uvs.extend_from_slice(&other.uvs);
        self.polygons.extend(other.polygons.iter().map(|p| {
            Polygon {
                corners: p
                    .corners
                    .iter()
                    .map(|c| Corner::new(c.point + point_offset, c.uv + uv_offset))
                    .collect(),
                material_slot: p.material_slot,
            }
        }));
        self.creases.extend(
            other
                .creases
                .iter()
                .map(|[a, b]| [a + point_offset, b + point_offset]),
        );
        self.seams.extend(
            other
                .seams
                .iter()
                .map(|[a, b]| [a + point_offset, b + point_offset]),
        );
        self.pinned_uvs
            .extend(other.pinned_uvs.iter().map(|slot| slot + uv_offset));
        self.normalize_creases();
        self.normalize_seams();
        self.normalize_pinned_uvs();
    }

    /// Merge points within `epsilon` using a spatial hash. UVs live on corners
    /// so they remain untouched. Degenerate polygons are removed.
    pub fn weld(&mut self, epsilon: f32) {
        let eps = epsilon.max(1e-7);
        let mut grid: HashMap<(i64, i64, i64), Vec<u32>> = HashMap::new();
        let mut remap: Vec<u32> = Vec::with_capacity(self.positions.len());
        let mut new_positions: Vec<Vec3> = Vec::new();
        for (i, p) in self.positions.iter().enumerate() {
            let cell = (
                (p.x / eps).floor() as i64,
                (p.y / eps).floor() as i64,
                (p.z / eps).floor() as i64,
            );
            let mut found = None;
            'search: for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        if let Some(bucket) = grid.get(&(cell.0 + dx, cell.1 + dy, cell.2 + dz)) {
                            for &j in bucket {
                                if new_positions[j as usize].distance(*p) <= eps {
                                    found = Some(j);
                                    break 'search;
                                }
                            }
                        }
                    }
                }
            }
            let id = match found {
                Some(j) => j,
                None => {
                    let id = new_positions.len() as u32;
                    new_positions.push(*p);
                    grid.entry(cell).or_default().push(id);
                    id
                }
            };
            remap.push(id);
            let _ = i;
        }
        for poly in &mut self.polygons {
            for c in &mut poly.corners {
                if let Some(&new) = remap.get(c.point as usize) {
                    c.point = new;
                }
            }
            dedup_corners(&mut poly.corners);
        }
        self.positions = new_positions;
        self.polygons.retain(|p| p.is_valid());
        self.remap_edge_marks(&remap);
        self.compact_points();
    }

    /// Remove points no polygon references.
    pub fn compact_points(&mut self) {
        self.compact_points_with_remap();
    }

    /// Remove points no polygon references and report where each old point
    /// went (`u32::MAX` marks a point that was dropped), so a caller can follow
    /// one specific point through the rebuild.
    pub fn compact_points_with_remap(&mut self) -> Vec<u32> {
        let mut used = vec![false; self.positions.len()];
        for p in &self.polygons {
            for c in &p.corners {
                if let Some(u) = used.get_mut(c.point as usize) {
                    *u = true;
                }
            }
        }
        let mut remap = vec![u32::MAX; self.positions.len()];
        let mut new_positions = Vec::new();
        for (i, used) in used.iter().enumerate() {
            if *used {
                remap[i] = new_positions.len() as u32;
                new_positions.push(self.positions[i]);
            }
        }
        if new_positions.len() == self.positions.len() {
            return (0..self.positions.len() as u32).collect();
        }
        for poly in &mut self.polygons {
            for c in &mut poly.corners {
                c.point = remap[c.point as usize];
            }
        }
        self.positions = new_positions;
        self.remap_edge_marks(&remap);
        remap
    }

    /// Merge the given points into one point at their centroid.
    ///
    /// This is the topology half of Cheetah3D's Collapse tool, shared with the
    /// editor's Weld command. The lowest selected index survives and every face
    /// that used one of the merged points uses it instead. Adjacent duplicate
    /// corners are folded first, so a quad that loses a corner becomes a
    /// triangle; a face that still repeats a point, or drops below three
    /// corners, is deleted rather than left pinched. Creases and seams follow
    /// the surviving indices (a mark with both endpoints merged disappears),
    /// pins on UV slots no corner uses any more are dropped, and unused points
    /// are compacted away. Returns the survivor's index in the rebuilt mesh, or
    /// `None` when fewer than two points were selected or no face survived to
    /// use the merged point.
    pub fn merge_points(&mut self, points: &[u32]) -> Option<u32> {
        let mut selected: Vec<u32> = points
            .iter()
            .copied()
            .filter(|p| (*p as usize) < self.positions.len())
            .collect();
        selected.sort_unstable();
        selected.dedup();
        if selected.len() < 2 {
            return None;
        }
        let centroid = selected
            .iter()
            .map(|p| self.positions[*p as usize])
            .sum::<Vec3>()
            / selected.len() as f32;
        let target = selected[0];
        self.positions[target as usize] = centroid;
        let merged: HashSet<u32> = selected.into_iter().collect();
        for poly in &mut self.polygons {
            for corner in &mut poly.corners {
                if merged.contains(&corner.point) {
                    corner.point = target;
                }
            }
        }
        let remap = self.finish_point_merge();
        remap
            .get(target as usize)
            .copied()
            .filter(|index| *index != u32::MAX)
    }

    /// Weld the selected points that sit within `epsilon` of each other.
    ///
    /// This is the topology half of Cheetah3D's Optimize tool, shared with the
    /// matching logic of `weld`. Only the given points are grouped, so unselected
    /// points stay exactly as authored. Points are visited in index order and each
    /// joins the first earlier survivor inside the tolerance, which keeps the
    /// result independent of the order they were selected in, and the survivor
    /// keeps its authored position. Returns the rebuilt indices of every selected
    /// point (merged points fold onto their survivor), or `None` when fewer than
    /// two points are given, the tolerance is not a positive finite number, or no
    /// two selected points are close enough to merge.
    pub fn weld_points(&mut self, points: &[u32], epsilon: f32) -> Option<Vec<u32>> {
        let (map, _) = weld_groups(&self.positions, points, epsilon)?;
        let mut selected: Vec<u32> = points
            .iter()
            .copied()
            .filter(|p| (*p as usize) < self.positions.len())
            .collect();
        selected.sort_unstable();
        selected.dedup();
        for poly in &mut self.polygons {
            for corner in &mut poly.corners {
                corner.point = map[corner.point as usize];
            }
        }
        let remap = self.finish_point_merge();
        let mut survivors: Vec<u32> = selected
            .iter()
            .filter_map(|p| remap.get(*p as usize).copied())
            .filter(|index| *index != u32::MAX)
            .collect();
        survivors.sort_unstable();
        survivors.dedup();
        Some(survivors)
    }

    /// Whether `weld_points` would merge at least one cluster.
    pub fn can_weld_points(&self, points: &[u32], epsilon: f32) -> bool {
        weld_groups(&self.positions, points, epsilon).is_some()
    }

    /// Slide the selected points a fraction of the way toward their neighbours.
    ///
    /// This is the geometry half of Cheetah3D's Point Slide tool: each selected
    /// point moves `fraction` of the distance toward the centroid of the points
    /// its edges directly connect it to, measured from the positions before the
    /// move so the result never depends on the order the points are visited in.
    /// A point with exactly two neighbours lands on the midpoint of the segment
    /// between them at `fraction = 1`. Only the selected positions change: no
    /// face, UV slot, material slot, crease, seam or pin is touched and no index
    /// moves. Returns whether any point actually moved.
    pub fn slide_points(&mut self, points: &[u32], fraction: f32) -> bool {
        if !fraction.is_finite() || !(0.0..=1.0).contains(&fraction) {
            return false;
        }
        let neighbours = neighbour_lists(self);
        let original = self.positions.clone();
        let mut moved = false;
        for point in selected_points(&original, points) {
            let Some(list) = neighbours.get(point as usize) else {
                continue;
            };
            if list.is_empty() {
                continue;
            }
            let centroid = list
                .iter()
                .map(|neighbour| original[*neighbour as usize])
                .sum::<Vec3>()
                / list.len() as f32;
            let target = original[point as usize].lerp(centroid, fraction);
            if target != original[point as usize] {
                self.positions[point as usize] = target;
                moved = true;
            }
        }
        moved
    }

    /// Whether `slide_points` would move at least one selected point.
    pub fn can_slide_points(&self, points: &[u32], fraction: f32) -> bool {
        if !fraction.is_finite() || fraction <= 0.0 || fraction > 1.0 {
            return false;
        }
        let neighbours = neighbour_lists(self);
        selected_points(&self.positions, points)
            .into_iter()
            .any(|point| {
                neighbours
                    .get(point as usize)
                    .is_some_and(|list| !list.is_empty())
            })
    }

    /// Shared rebuild after a point merge: fold adjacent duplicate corners,
    /// delete faces that repeat a point or fall below three corners, compact
    /// unused points (which remaps creases and seams), and drop pins on UV slots
    /// no corner uses any more. Returns the point remap from compaction.
    fn finish_point_merge(&mut self) -> Vec<u32> {
        for poly in &mut self.polygons {
            dedup_corners(&mut poly.corners);
        }
        self.polygons.retain(|poly| {
            if poly.corners.len() < 3 {
                return false;
            }
            let mut points: Vec<u32> = poly.corners.iter().map(|c| c.point).collect();
            points.sort_unstable();
            points.dedup();
            points.len() == poly.corners.len()
        });
        let remap = self.compact_points_with_remap();
        let mut used_uvs: Vec<u32> = self
            .polygons
            .iter()
            .flat_map(|poly| poly.corners.iter().map(|c| c.uv))
            .collect();
        used_uvs.sort_unstable();
        used_uvs.dedup();
        self.pinned_uvs
            .retain(|slot| used_uvs.binary_search(slot).is_ok());
        self.normalize_pinned_uvs();
        remap
    }

    pub fn remove_degenerate_polygons(&mut self) {
        self.polygons.retain(|p| {
            if p.corners.len() < 3 {
                return false;
            }
            let mut unique = Vec::with_capacity(p.corners.len());
            for c in &p.corners {
                if !unique.contains(&c.point) {
                    unique.push(c.point);
                }
            }
            unique.len() >= 3
        });
    }

    pub fn flip_normals(&mut self) {
        for poly in &mut self.polygons {
            poly.corners.reverse();
        }
    }

    /// Whether the edge between two points is marked as a crease.
    pub fn is_crease(&self, a: u32, b: u32) -> bool {
        edge_marked(&self.creases, a, b)
    }

    /// Add or remove a crease. Returns whether the mesh changed.
    pub fn set_crease(&mut self, a: u32, b: u32, creased: bool) -> bool {
        set_edge_marked(&mut self.creases, a, b, creased)
    }

    /// Toggle one edge's crease state and return the new state.
    pub fn toggle_crease(&mut self, a: u32, b: u32) -> bool {
        let creased = !self.is_crease(a, b);
        self.set_crease(a, b, creased);
        creased
    }

    /// Whether the edge between two points is marked as a UV seam.
    pub fn is_seam(&self, a: u32, b: u32) -> bool {
        edge_marked(&self.seams, a, b)
    }

    /// Add or remove a seam. Returns whether the mesh changed.
    pub fn set_seam(&mut self, a: u32, b: u32, seam: bool) -> bool {
        set_edge_marked(&mut self.seams, a, b, seam)
    }

    /// Toggle one edge's seam state and return the new state.
    pub fn toggle_seam(&mut self, a: u32, b: u32) -> bool {
        let seam = !self.is_seam(a, b);
        self.set_seam(a, b, seam);
        seam
    }

    /// Remove duplicate, degenerate and out-of-range crease entries and keep
    /// the list sorted so `is_crease` can use a binary search.
    pub fn normalize_creases(&mut self) {
        normalize_edge_marks(&mut self.creases, self.positions.len());
    }

    /// Same repair for seams, so `is_seam` can use a binary search.
    pub fn normalize_seams(&mut self) {
        normalize_edge_marks(&mut self.seams, self.positions.len());
    }

    /// Whether a UV slot is pinned.
    pub fn is_uv_pinned(&self, slot: u32) -> bool {
        self.pinned_uvs.binary_search(&slot).is_ok()
    }

    /// Add or remove a UV pin. Returns whether the mesh changed.
    pub fn set_uv_pinned(&mut self, slot: u32, pinned: bool) -> bool {
        let existing = self.is_uv_pinned(slot);
        if pinned == existing {
            return false;
        }
        if pinned {
            let index = self.pinned_uvs.partition_point(|s| *s < slot);
            self.pinned_uvs.insert(index, slot);
        } else if let Ok(index) = self.pinned_uvs.binary_search(&slot) {
            self.pinned_uvs.remove(index);
        }
        true
    }

    /// Toggle one UV slot's pin state and return the new state.
    pub fn toggle_uv_pinned(&mut self, slot: u32) -> bool {
        let pinned = !self.is_uv_pinned(slot);
        self.set_uv_pinned(slot, pinned);
        pinned
    }

    /// Drop duplicate and out-of-range UV pins and keep the list sorted so
    /// `is_uv_pinned` can use a binary search.
    pub fn normalize_pinned_uvs(&mut self) {
        let count = self.uvs.len() as u32;
        self.pinned_uvs.retain(|slot| *slot < count);
        self.pinned_uvs.sort_unstable();
        self.pinned_uvs.dedup();
    }

    /// Follow point renumbering through both edge-marker lists.
    fn remap_edge_marks(&mut self, remap: &[u32]) {
        remap_marks(&mut self.creases, remap);
        remap_marks(&mut self.seams, remap);
        self.normalize_creases();
        self.normalize_seams();
    }

    /// Structural validation used by tests and after imports.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        for (pi, poly) in self.polygons.iter().enumerate() {
            if poly.corners.len() < 3 {
                errors.push(format!("polygon {pi} has fewer than 3 corners"));
            }
            if poly.corners.len() > 1024 {
                errors.push(format!("polygon {pi} is unreasonably large"));
            }
            for c in &poly.corners {
                if c.point as usize >= self.positions.len() {
                    errors.push(format!("polygon {pi} references missing point {}", c.point));
                }
                if !self.uvs.is_empty() && c.uv as usize >= self.uvs.len() {
                    errors.push(format!("polygon {pi} references missing uv {}", c.uv));
                }
            }
        }
        for (i, p) in self.positions.iter().enumerate() {
            if !p.is_finite() {
                errors.push(format!("point {i} is not finite"));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Signed volume of the triangulated mesh (positive for outward winding).
    pub fn signed_volume(&self) -> f32 {
        let mut vol = 0.0;
        for tri in self.render_triangles(40.0) {
            let a = tri.v[0].position;
            let b = tri.v[1].position;
            let c = tri.v[2].position;
            vol += a.dot(b.cross(c));
        }
        vol / 6.0
    }

    pub fn signature(&self) -> u64 {
        let mut h = StableHasher::new();
        h.write_str(&self.name);
        h.write_u64(self.positions.len() as u64);
        for p in &self.positions {
            h.write_vec3(*p);
        }
        h.write_u64(self.uvs.len() as u64);
        for uv in &self.uvs {
            h.write_vec2(*uv);
        }
        h.write_u64(self.polygons.len() as u64);
        for poly in &self.polygons {
            h.write_u64(poly.corners.len() as u64);
            h.write_u32(poly.material_slot as u32);
            for c in &poly.corners {
                h.write_u32(c.point);
                h.write_u32(c.uv);
            }
        }
        h.write_u64(self.creases.len() as u64);
        for [a, b] in &self.creases {
            h.write_u32(*a);
            h.write_u32(*b);
        }
        h.write_u64(self.seams.len() as u64);
        for [a, b] in &self.seams {
            h.write_u32(*a);
            h.write_u32(*b);
        }
        h.write_u64(self.pinned_uvs.len() as u64);
        for slot in &self.pinned_uvs {
            h.write_u32(*slot);
        }
        h.finish()
    }

    /// Triangulated vertex soup useful for simple exporters.
    pub fn triangle_soup(&self) -> Vec<[Vec3; 3]> {
        self.render_triangles(40.0)
            .into_iter()
            .map(|t| [t.v[0].position, t.v[1].position, t.v[2].position])
            .collect()
    }
}

fn edge_marked(list: &[[u32; 2]], a: u32, b: u32) -> bool {
    list.binary_search(&canonical_edge(a, b)).is_ok()
}

fn set_edge_marked(list: &mut Vec<[u32; 2]>, a: u32, b: u32, marked: bool) -> bool {
    if a == b {
        return false;
    }
    let key = canonical_edge(a, b);
    let existing = list.binary_search(&key).is_ok();
    if marked == existing {
        return false;
    }
    if marked {
        let index = list.partition_point(|edge| *edge < key);
        list.insert(index, key);
    } else if let Ok(index) = list.binary_search(&key) {
        list.remove(index);
    }
    true
}

fn normalize_edge_marks(list: &mut Vec<[u32; 2]>, point_count: usize) {
    let n = point_count as u32;
    list.retain(|[a, b]| a != b && *a < n && *b < n);
    for edge in list.iter_mut() {
        *edge = canonical_edge(edge[0], edge[1]);
    }
    list.sort_unstable();
    list.dedup();
}

fn remap_marks(list: &mut Vec<[u32; 2]>, remap: &[u32]) {
    let mut next = Vec::with_capacity(list.len());
    for [a, b] in list.iter() {
        let mapped = remap
            .get(*a as usize)
            .zip(remap.get(*b as usize))
            .map(|(&a, &b)| canonical_edge(a, b));
        if let Some(edge) = mapped {
            if edge[0] != edge[1] {
                next.push(edge);
            }
        }
    }
    *list = next;
}

/// Sorted, de-duplicated, in-range selection of points.
fn selected_points(positions: &[Vec3], points: &[u32]) -> Vec<u32> {
    let mut selected: Vec<u32> = points
        .iter()
        .copied()
        .filter(|point| (*point as usize) < positions.len())
        .collect();
    selected.sort_unstable();
    selected.dedup();
    selected
}

/// Point indices connected by a polygon edge, sorted and de-duplicated.
fn neighbour_lists(mesh: &Mesh) -> Vec<Vec<u32>> {
    let mut neighbours: Vec<Vec<u32>> = vec![Vec::new(); mesh.positions.len()];
    for edge in mesh.edges() {
        if let Some(list) = neighbours.get_mut(edge.a as usize) {
            list.push(edge.b);
        }
        if let Some(list) = neighbours.get_mut(edge.b as usize) {
            list.push(edge.a);
        }
    }
    for list in &mut neighbours {
        list.sort_unstable();
        list.dedup();
    }
    neighbours
}

/// Group the selected points that lie within `epsilon` of each other.
///
/// Returns the point remap plus the surviving representatives, or `None` when
/// there is nothing to merge. Greedy first-fit in index order keeps the result
/// deterministic and independent of the order the points were picked in.
fn weld_groups(positions: &[Vec3], points: &[u32], epsilon: f32) -> Option<(Vec<u32>, Vec<u32>)> {
    if !epsilon.is_finite() || epsilon <= 0.0 {
        return None;
    }
    let mut selected: Vec<u32> = points
        .iter()
        .copied()
        .filter(|p| (*p as usize) < positions.len())
        .collect();
    selected.sort_unstable();
    selected.dedup();
    if selected.len() < 2 {
        return None;
    }
    let total = selected.len();
    let mut map: Vec<u32> = (0..positions.len() as u32).collect();
    let mut survivors: Vec<u32> = Vec::new();
    for point in selected {
        let position = positions[point as usize];
        match survivors
            .iter()
            .find(|survivor| positions[**survivor as usize].distance(position) <= epsilon)
        {
            Some(&survivor) => map[point as usize] = survivor,
            None => survivors.push(point),
        }
    }
    if survivors.len() == total {
        return None;
    }
    Some((map, survivors))
}

fn dedup_corners(corners: &mut Vec<Corner>) {
    corners.dedup_by_key(|c| c.point);
    while corners.len() > 1 && corners.first().map(|c| c.point) == corners.last().map(|c| c.point) {
        corners.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad_grid() -> Mesh {
        Mesh::from_polygons(
            "grid",
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(2.0, 0.0, 1.0),
            ],
            vec![vec![0, 3, 2, 1], vec![1, 2, 5, 4]],
        )
    }

    #[test]
    fn triangulates_ngons_and_quads() {
        let mesh = quad_grid();
        assert_eq!(mesh.triangle_count(), 4);
        assert_eq!(mesh.render_triangles(40.0).len(), 4);
        assert_eq!(mesh.polygon_triangles(0).len(), 2);
    }

    #[test]
    fn concave_ngon_ear_clips() {
        // Concave "arrow" polygon.
        let positions = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(1.0, 0.5, 0.0),
            Vec3::new(2.0, 1.5, 0.0),
            Vec3::new(0.0, 1.5, 0.0),
        ];
        let mesh = Mesh::from_polygons("arrow", positions, vec![vec![0, 4, 3, 2, 1]]);
        let tris = mesh.polygon_triangles(0);
        assert_eq!(tris.len(), 3);
        assert!(mesh.validate().is_ok());
    }

    #[test]
    fn creases_toggle_canonically_and_survive_compaction() {
        let mut mesh = Mesh::from_polygons(
            "quad",
            vec![
                Vec3::new(-1.0, 0.0, -1.0),
                Vec3::new(1.0, 0.0, -1.0),
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(-1.0, 0.0, 1.0),
            ],
            vec![vec![0, 1, 2, 3]],
        );
        assert!(mesh.set_crease(3, 1, true));
        assert!(mesh.is_crease(1, 3));
        assert_eq!(mesh.creases, vec![[1, 3]]);
        assert!(!mesh.set_crease(1, 3, true));
        assert!(!mesh.toggle_crease(1, 3));
        assert!(mesh.creases.is_empty());

        assert!(mesh.set_crease(0, 1, true));
        mesh.add_point(Vec3::new(5.0, 0.0, 5.0));
        assert!(mesh.set_crease(1, 4, true));
        mesh.compact_points();
        assert_eq!(mesh.positions.len(), 4);
        assert_eq!(mesh.creases, vec![[0, 1]]);
    }

    #[test]
    fn seams_mark_canonically_and_survive_compaction_and_merge() {
        let mut mesh = quad_grid();
        assert!(mesh.set_seam(1, 0, true));
        assert!(mesh.is_seam(0, 1));
        assert_eq!(mesh.seams, vec![[0, 1]]);
        assert!(!mesh.set_seam(0, 1, true));
        assert!(!mesh.toggle_seam(1, 0));
        assert!(mesh.seams.is_empty());

        assert!(mesh.set_seam(1, 2, true));
        assert!(mesh.set_crease(1, 2, true));
        assert!(mesh.is_crease(1, 2), "creases and seams stay independent");
        mesh.add_point(Vec3::new(9.0, 0.0, 9.0));
        assert!(mesh.set_seam(2, 6, true));
        mesh.compact_points();
        assert_eq!(mesh.positions.len(), 6);
        assert_eq!(mesh.seams, vec![[1, 2]]);
        assert_eq!(mesh.creases, vec![[1, 2]]);

        let mut other = crate::primitives::box_mesh(1.0, 1.0, 1.0);
        assert!(other.set_seam(0, 1, true));
        let offset = mesh.positions.len() as u32;
        mesh.merge(&other, Mat4::IDENTITY);
        assert_eq!(mesh.seams, vec![[1, 2], [offset, offset + 1]]);
    }

    #[test]
    fn uv_pins_toggle_canonically_and_survive_merge() {
        let mut mesh = quad_grid();
        mesh.uvs = vec![Vec2::ZERO, Vec2::new(0.5, 0.5)];
        assert!(mesh.set_uv_pinned(1, true));
        assert!(mesh.is_uv_pinned(1));
        assert!(!mesh.set_uv_pinned(1, true));
        assert!(!mesh.toggle_uv_pinned(1));
        assert!(mesh.pinned_uvs.is_empty());

        assert!(mesh.set_uv_pinned(0, true));
        let before = mesh.signature();
        assert!(mesh.set_uv_pinned(1, true));
        assert_ne!(before, mesh.signature(), "pins are part of the signature");
        assert_eq!(mesh.pinned_uvs, vec![0, 1]);

        mesh.pinned_uvs = vec![1, 1, 7];
        mesh.normalize_pinned_uvs();
        assert_eq!(mesh.pinned_uvs, vec![1], "out-of-range pins are dropped");

        let mut other = quad_grid();
        other.uvs = vec![Vec2::new(0.25, 0.25)];
        assert!(other.set_uv_pinned(0, true));
        let uv_offset = mesh.uvs.len() as u32;
        mesh.merge(&other, Mat4::IDENTITY);
        assert_eq!(mesh.pinned_uvs, vec![1, uv_offset]);
        assert_eq!(mesh.uvs.len() as u32, uv_offset + 1);
    }

    #[test]
    fn crease_angle_keeps_cube_flat() {
        let cube = crate::primitives::box_mesh(1.0, 1.0, 1.0);
        let normals = cube.corner_normals(30.0);
        // Every corner of a cube should have exactly its own face normal.
        for (pi, face) in normals.iter().enumerate() {
            let fnorm = cube.face_normal(pi);
            for n in face {
                assert!((*n - fnorm).length() < 1e-5);
            }
        }
    }

    #[test]
    fn boundary_loops_are_ordered() {
        let mesh = quad_grid();
        let loops = mesh.boundary_loops();
        assert_eq!(loops.len(), 1);
        assert_eq!(loops[0].len(), 6);
    }

    #[test]
    fn weld_and_compact() {
        let mut mesh = Mesh::from_polygons(
            "dup",
            vec![
                Vec3::ZERO,
                Vec3::X,
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(1e-5, 0.0, 0.0),
            ],
            vec![vec![0, 1, 2]],
        );
        mesh.weld(1e-3);
        assert_eq!(mesh.positions.len(), 3);
        assert!(mesh.validate().is_ok());
    }

    #[test]
    fn transform_and_bounds() {
        let mut mesh = quad_grid();
        mesh.transform(Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0)));
        let b = mesh.bounds();
        assert!((b.min - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-5);
        assert!(mesh.validate().is_ok());
    }
}
