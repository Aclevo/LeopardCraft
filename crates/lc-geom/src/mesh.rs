//! N-gon aware polygon mesh.
//!
//! Points, per-corner UVs and polygons are stored separately. A polygon can be
//! a triangle, quad or arbitrary n-gon; edges, wireframes and render buffers
//! are derived. This mirrors how box modelling tools treat topology: keep the
//! authored polygons, triangulate only for display/export/intersection.

use glam::{Mat4, Vec2, Vec3};
use lc_core::{Aabb, StableHasher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
}

/// Flat render triangle with corner attributes.
#[derive(Clone, Copy, Debug)]
pub struct RenderVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
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
                }; 3];
                for k in 0..3 {
                    let c = &poly.corners[tri[k]];
                    v[k] = RenderVertex {
                        position: self.positions[c.point as usize],
                        normal: normals[pi][tri[k]],
                        uv: self.uv_of(c),
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
            if next.get(&start).map_or(true, |v| v.is_empty()) {
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
        self.compact_points();
    }

    /// Remove points no polygon references.
    pub fn compact_points(&mut self) {
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
            return;
        }
        for poly in &mut self.polygons {
            for c in &mut poly.corners {
                c.point = remap[c.point as usize];
            }
        }
        self.positions = new_positions;
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
