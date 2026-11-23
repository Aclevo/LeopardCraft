//! Catmull-Clark, Stam-Loop and linear subdivision surfaces.

use crate::{Corner, Mesh, Polygon};
use glam::{Vec2, Vec3};
use std::collections::HashMap;

/// Compute a Catmull-Clark subdivision level. Input n-gons become quads; UVs
/// are averaged per generated point (texture seams are not preserved exactly,
/// but mapping remains continuous within the averaged chart).
pub fn catmull_clark(mesh: &Mesh, levels: u32) -> Mesh {
    let mut out = mesh.clone();
    for _ in 0..levels.min(6) {
        if out.polygons.is_empty() {
            break;
        }
        out = subdivide_once(&out);
    }
    out
}

fn subdivide_once(mesh: &Mesh) -> Mesh {
    let npoints = mesh.positions.len();
    let face_count = mesh.polygons.len();
    let mut face_pos = Vec::with_capacity(face_count);
    let mut face_uv = Vec::with_capacity(face_count);
    for (fi, poly) in mesh.polygons.iter().enumerate() {
        let pts = mesh.polygon_points(fi);
        face_pos.push(if pts.is_empty() {
            Vec3::ZERO
        } else {
            pts.iter().copied().sum::<Vec3>() / pts.len() as f32
        });
        let uvs = poly.corners.iter().map(|c| mesh.uv_of(c)).sum::<Vec2>();
        face_uv.push(if poly.corners.is_empty() {
            Vec2::ZERO
        } else {
            uvs / poly.corners.len() as f32
        });
    }

    // Build unique undirected edges.
    let mut edge_index: HashMap<(u32, u32), usize> = HashMap::new();
    let mut edges: Vec<(u32, u32, Vec<usize>)> = Vec::new();
    for (fi, poly) in mesh.polygons.iter().enumerate() {
        let n = poly.corners.len();
        for i in 0..n {
            let a = poly.corners[i].point;
            let b = poly.corners[(i + 1) % n].point;
            if a == b {
                continue;
            }
            let key = if a < b { (a, b) } else { (b, a) };
            let idx = match edge_index.get(&key) {
                Some(&idx) => {
                    if !edges[idx].2.contains(&fi) {
                        edges[idx].2.push(fi);
                    }
                    idx
                }
                None => {
                    let idx = edges.len();
                    edge_index.insert(key, idx);
                    edges.push((key.0, key.1, vec![fi]));
                    idx
                }
            };
            let _ = idx;
        }
    }
    let n_edges = edges.len();

    // New point positions: originals first, then edge points, then face points.
    let mut positions = Vec::with_capacity(npoints + n_edges + face_count);
    let mut uvs = Vec::with_capacity(npoints + n_edges + face_count);
    for p in &mesh.positions {
        positions.push(*p);
    }

    // Accumulate per-point face/edge data for the smoothing rule.
    let mut point_faces: Vec<Vec<usize>> = vec![Vec::new(); npoints];
    let mut point_edges: Vec<Vec<usize>> = vec![Vec::new(); npoints];
    let mut point_uv_sum = vec![Vec2::ZERO; npoints];
    let mut point_uv_count = vec![0u32; npoints];
    for (fi, poly) in mesh.polygons.iter().enumerate() {
        for c in &poly.corners {
            if c.point as usize >= npoints {
                continue;
            }
            let pf = &mut point_faces[c.point as usize];
            if !pf.contains(&fi) {
                pf.push(fi);
            }
            point_uv_sum[c.point as usize] += mesh.uv_of(c);
            point_uv_count[c.point as usize] += 1;
        }
    }
    for (ei, (a, b, _)) in edges.iter().enumerate() {
        point_edges[*a as usize].push(ei);
        point_edges[*b as usize].push(ei);
    }

    let point_uv: Vec<Vec2> = (0..npoints)
        .map(|i| {
            if point_uv_count[i] > 0 {
                point_uv_sum[i] / point_uv_count[i] as f32
            } else {
                Vec2::ZERO
            }
        })
        .collect();
    uvs.extend_from_slice(&point_uv);

    // Boundary edges behave as creases. Authored creases use the same sharp
    // vertex rule: two incident creases follow the crease curve, three or more
    // pin the point, and a single crease falls back to the smooth rule.
    let sharp_neighbours = |p: usize| -> Vec<u32> {
        let mut ns = Vec::new();
        for &ei in &point_edges[p] {
            let (a, b, faces) = &edges[ei];
            if faces.len() > 1 && !mesh.is_crease(*a, *b) {
                continue;
            }
            let other = if *a as usize == p {
                *b
            } else if *b as usize == p {
                *a
            } else {
                continue;
            };
            if !ns.contains(&other) {
                ns.push(other);
            }
        }
        ns
    };

    let mut new_points = Vec::with_capacity(npoints);
    for p in 0..npoints {
        let pos = mesh.positions[p];
        let faces = &point_faces[p];
        if faces.is_empty() {
            new_points.push(pos);
            continue;
        }
        let sharp = sharp_neighbours(p);
        if sharp.len() >= 2 {
            if sharp.len() > 2 {
                new_points.push(pos);
            } else {
                let sum = mesh.positions[sharp[0] as usize] + mesh.positions[sharp[1] as usize];
                new_points.push(pos * 0.75 + sum * 0.125);
            }
            continue;
        }
        let f = faces.iter().map(|&fi| face_pos[fi]).sum::<Vec3>() / faces.len() as f32;
        let mut r = Vec3::ZERO;
        for &ei in &point_edges[p] {
            let (a, b, _) = edges[ei];
            r += (mesh.positions[a as usize] + mesh.positions[b as usize]) * 0.5;
        }
        r /= point_edges[p].len().max(1) as f32;
        let n = faces.len() as f32;
        new_points.push((f + r * 2.0 + pos * (n - 3.0)) / n);
    }
    for (i, p) in new_points.into_iter().enumerate() {
        positions[i] = p;
    }

    // Edge points.
    let mut edge_point_index = Vec::with_capacity(n_edges);
    for (a, b, faces) in &edges {
        let pa = mesh.positions[*a as usize];
        let pb = mesh.positions[*b as usize];
        let sharp = faces.len() < 2 || mesh.is_crease(*a, *b);
        let pos = if sharp {
            (pa + pb) * 0.5
        } else {
            (pa + pb + face_pos[faces[0]] + face_pos[faces[1]]) * 0.25
        };
        let uv_a = point_uv[*a as usize];
        let uv_b = point_uv[*b as usize];
        let uv = if sharp {
            (uv_a + uv_b) * 0.5
        } else {
            (uv_a + uv_b + face_uv[faces[0]] + face_uv[faces[1]]) * 0.25
        };
        let idx = positions.len();
        positions.push(pos);
        uvs.push(uv);
        edge_point_index.push(idx);
    }
    let face_base = positions.len();
    for fi in 0..face_count {
        positions.push(face_pos[fi]);
        uvs.push(face_uv[fi]);
    }

    // New faces.
    let mut polys = Vec::with_capacity(face_count * 4);
    for (fi, poly) in mesh.polygons.iter().enumerate() {
        let n = poly.corners.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let prev = poly.corners[(i + n - 1) % n].point;
            let cur = poly.corners[i].point;
            let next = poly.corners[(i + 1) % n].point;
            let e1 = edge_index[&key(prev, cur)];
            let e2 = edge_index[&key(cur, next)];
            let ids = [
                (cur as usize, cur as usize),
                (edge_point_index[e2], positions.len() - face_count + fi),
            ];
            let _ = ids;
            let corners = vec![
                Corner::new(cur, cur),
                Corner::new(edge_point_index[e2] as u32, edge_point_index[e2] as u32),
                Corner::new((face_base + fi) as u32, (face_base + fi) as u32),
                Corner::new(edge_point_index[e1] as u32, edge_point_index[e1] as u32),
            ];
            polys.push(Polygon {
                corners,
                material_slot: poly.material_slot,
            });
        }
    }

    // Split authored creases and seams onto both child edges so sharpness and
    // UV chart borders survive further subdivision levels.
    let mut creases = Vec::new();
    let mut seams = Vec::new();
    for (ei, (a, b, _)) in edges.iter().enumerate() {
        let mid = edge_point_index[ei] as u32;
        if mesh.is_crease(*a, *b) {
            creases.push(pair(*a, mid));
            creases.push(pair(mid, *b));
        }
        if mesh.is_seam(*a, *b) {
            seams.push(pair(*a, mid));
            seams.push(pair(mid, *b));
        }
    }
    creases.sort_unstable();
    creases.dedup();
    seams.sort_unstable();
    seams.dedup();

    // UV indices match point indices.
    let mut out = Mesh {
        name: mesh.name.clone(),
        positions,
        uvs,
        polygons: polys,
        creases,
        seams,
        // Subdivision re-authors the UV set per point, so slot pins cannot
        // survive it and are cleared instead of pointing at unrelated UVs.
        pinned_uvs: Vec::new(),
    };
    out.remove_degenerate_polygons();
    out.normalize_creases();
    out.normalize_seams();
    out
}

/// Linear (non-smoothing) subdivision. Each n-gon becomes n quads around its
/// centroid, with edge midpoints shared between neighbouring polygons so the
/// result stays watertight.
pub fn linear(mesh: &Mesh, levels: u32) -> Mesh {
    let mut out = mesh.clone();
    for _ in 0..levels.min(6) {
        if out.polygons.is_empty() {
            break;
        }
        out = linear_once(&out);
    }
    out
}

fn linear_once(mesh: &Mesh) -> Mesh {
    let mut positions = mesh.positions.clone();
    let mut uvs = mesh.uvs.clone();
    if uvs.len() < positions.len() {
        uvs.resize(positions.len(), Vec2::ZERO);
    }
    let mut edge_mid: HashMap<(u32, u32), u32> = HashMap::new();
    let mut centroids = Vec::with_capacity(mesh.polygons.len());
    for (fi, poly) in mesh.polygons.iter().enumerate() {
        let pts = mesh.polygon_points(fi);
        let center = if pts.is_empty() {
            Vec3::ZERO
        } else {
            pts.iter().copied().sum::<Vec3>() / pts.len() as f32
        };
        let uv = if poly.corners.is_empty() {
            Vec2::ZERO
        } else {
            poly.corners.iter().map(|c| mesh.uv_of(c)).sum::<Vec2>() / poly.corners.len() as f32
        };
        positions.push(center);
        uvs.push(uv);
        centroids.push((positions.len() - 1) as u32);
    }

    let mut polys = Vec::with_capacity(mesh.polygons.len() * 4);
    for (fi, poly) in mesh.polygons.iter().enumerate() {
        let n = poly.corners.len();
        if n < 3 {
            continue;
        }
        let mids: Vec<u32> = (0..n)
            .map(|i| {
                let a = poly.corners[i];
                let b = poly.corners[(i + 1) % n];
                let key = key(a.point, b.point);
                if let Some(&mid) = edge_mid.get(&key) {
                    return mid;
                }
                let pa = mesh
                    .positions
                    .get(a.point as usize)
                    .copied()
                    .unwrap_or(Vec3::ZERO);
                let pb = mesh
                    .positions
                    .get(b.point as usize)
                    .copied()
                    .unwrap_or(Vec3::ZERO);
                positions.push((pa + pb) * 0.5);
                uvs.push((mesh.uv_of(&a) + mesh.uv_of(&b)) * 0.5);
                let mid = (positions.len() - 1) as u32;
                edge_mid.insert(key, mid);
                mid
            })
            .collect();
        for i in 0..n {
            let prev = mids[(i + n - 1) % n];
            let next = mids[i];
            let corners = vec![
                Corner::new(poly.corners[i].point, poly.corners[i].uv),
                Corner::new(next, next),
                Corner::new(centroids[fi], centroids[fi]),
                Corner::new(prev, prev),
            ];
            polys.push(Polygon {
                corners,
                material_slot: poly.material_slot,
            });
        }
    }

    let mut creases = Vec::new();
    let mut seams = Vec::new();
    for [a, b] in &mesh.creases {
        if let Some(&mid) = edge_mid.get(&key(*a, *b)) {
            creases.push(pair(*a, mid));
            creases.push(pair(mid, *b));
        }
    }
    for [a, b] in &mesh.seams {
        if let Some(&mid) = edge_mid.get(&key(*a, *b)) {
            seams.push(pair(*a, mid));
            seams.push(pair(mid, *b));
        }
    }
    creases.sort_unstable();
    creases.dedup();
    seams.sort_unstable();
    seams.dedup();

    let mut out = Mesh {
        name: mesh.name.clone(),
        positions,
        uvs,
        polygons: polys,
        creases,
        seams,
        // Subdivision re-authors the UV set per point, so slot pins cannot
        // survive it and are cleared instead of pointing at unrelated UVs.
        pinned_uvs: Vec::new(),
    };
    out.remove_degenerate_polygons();
    out.normalize_creases();
    out.normalize_seams();
    out
}

/// Stam-Loop-style subdivision for mixed quad/triangle cages. The cage is
/// triangulated and each triangle is split into four using Loop edge and
/// vertex rules. Authored creases and mesh boundaries use the sharp rules and
/// are inherited by both child edges.
pub fn stam_loop(mesh: &Mesh, levels: u32) -> Mesh {
    let mut out = mesh.clone();
    for _ in 0..levels.min(6) {
        if out.polygons.is_empty() {
            break;
        }
        out = loop_once(&triangulated_for_loop(&out));
    }
    out
}

fn triangulated_for_loop(mesh: &Mesh) -> Mesh {
    let mut out = Mesh {
        name: mesh.name.clone(),
        positions: mesh.positions.clone(),
        uvs: mesh.uvs.clone(),
        polygons: Vec::with_capacity(mesh.polygons.len() * 2),
        creases: mesh.creases.clone(),
        seams: mesh.seams.clone(),
        pinned_uvs: mesh.pinned_uvs.clone(),
    };
    if out.uvs.len() < out.positions.len() {
        out.uvs.resize(out.positions.len(), Vec2::ZERO);
    }
    for (fi, poly) in mesh.polygons.iter().enumerate() {
        for tri in mesh.polygon_triangles(fi) {
            let corners = tri
                .iter()
                .map(|&corner| poly.corners[corner])
                .collect::<Vec<_>>();
            out.polygons.push(Polygon {
                corners,
                material_slot: poly.material_slot,
            });
        }
    }
    out.normalize_creases();
    out
}

/// An undirected edge with the (triangle, opposite point) pair of every
/// adjacent face, used by the Loop rules.
type LoopEdge = (u32, u32, Vec<(usize, u32)>);

fn loop_once(mesh: &Mesh) -> Mesh {
    let npoints = mesh.positions.len();
    let mut edge_index: HashMap<(u32, u32), usize> = HashMap::new();
    let mut edges: Vec<LoopEdge> = Vec::new();
    for (fi, poly) in mesh.polygons.iter().enumerate() {
        if poly.corners.len() != 3 {
            continue;
        }
        let p = [
            poly.corners[0].point,
            poly.corners[1].point,
            poly.corners[2].point,
        ];
        for i in 0..3 {
            let a = p[i];
            let b = p[(i + 1) % 3];
            let opposite = p[(i + 2) % 3];
            if a == b {
                continue;
            }
            let key = key(a, b);
            match edge_index.get(&key) {
                Some(&index) => edges[index].2.push((fi, opposite)),
                None => {
                    edge_index.insert(key, edges.len());
                    edges.push((key.0, key.1, vec![(fi, opposite)]));
                }
            }
        }
    }

    let mut point_neighbours: Vec<Vec<u32>> = vec![Vec::new(); npoints];
    let mut point_uv_sum = vec![Vec2::ZERO; npoints];
    let mut point_uv_count = vec![0u32; npoints];
    for (a, b, _) in &edges {
        if !point_neighbours[*a as usize].contains(b) {
            point_neighbours[*a as usize].push(*b);
        }
        if !point_neighbours[*b as usize].contains(a) {
            point_neighbours[*b as usize].push(*a);
        }
    }
    for poly in &mesh.polygons {
        for c in &poly.corners {
            if let Some(sum) = point_uv_sum.get_mut(c.point as usize) {
                *sum += mesh.uv_of(c);
                point_uv_count[c.point as usize] += 1;
            }
        }
    }

    let mut positions = Vec::with_capacity(npoints + edges.len());
    let mut uvs = Vec::with_capacity(npoints + edges.len());
    for p in 0..npoints {
        let pos = mesh.positions[p];
        let neighbours = &point_neighbours[p];
        let mut sharp = Vec::new();
        for &other in neighbours {
            let (a, b, adjacent) = &edges[edge_index[&key(p as u32, other)]];
            if adjacent.len() < 2 || mesh.is_crease(*a, *b) {
                sharp.push(other);
            }
        }
        let next = if sharp.len() >= 2 {
            if sharp.len() > 2 {
                pos
            } else {
                pos * 0.75
                    + (mesh.positions[sharp[0] as usize] + mesh.positions[sharp[1] as usize])
                        * 0.125
            }
        } else if neighbours.is_empty() {
            pos
        } else {
            let n = neighbours.len() as f32;
            let beta = if neighbours.len() == 3 {
                3.0 / 16.0
            } else {
                3.0 / (8.0 * n)
            };
            let sum = neighbours
                .iter()
                .map(|&i| mesh.positions[i as usize])
                .sum::<Vec3>();
            pos * (1.0 - n * beta) + sum * beta
        };
        positions.push(next);
        uvs.push(if point_uv_count[p] > 0 {
            point_uv_sum[p] / point_uv_count[p] as f32
        } else {
            Vec2::ZERO
        });
    }

    let mut edge_points = Vec::with_capacity(edges.len());
    for (a, b, opposite) in &edges {
        let pa = mesh.positions[*a as usize];
        let pb = mesh.positions[*b as usize];
        let sharp = opposite.len() < 2 || mesh.is_crease(*a, *b);
        let pos = if sharp {
            (pa + pb) * 0.5
        } else {
            let c = mesh.positions[opposite[0].1 as usize];
            let d = mesh.positions[opposite[1].1 as usize];
            (pa + pb) * 0.375 + (c + d) * 0.125
        };
        let uv = (mesh.uvs.get(*a as usize).copied().unwrap_or(Vec2::ZERO)
            + mesh.uvs.get(*b as usize).copied().unwrap_or(Vec2::ZERO))
            * 0.5;
        positions.push(pos);
        uvs.push(uv);
        edge_points.push((positions.len() - 1) as u32);
    }

    let mut polys = Vec::with_capacity(mesh.polygons.len() * 4);
    for poly in &mesh.polygons {
        if poly.corners.len() != 3 {
            continue;
        }
        let p = [
            poly.corners[0].point,
            poly.corners[1].point,
            poly.corners[2].point,
        ];
        let e01 = edge_points[edge_index[&key(p[0], p[1])]];
        let e12 = edge_points[edge_index[&key(p[1], p[2])]];
        let e20 = edge_points[edge_index[&key(p[2], p[0])]];
        for corners in [
            [p[0], e01, e20],
            [e01, p[1], e12],
            [e20, e12, p[2]],
            [e01, e12, e20],
        ] {
            polys.push(Polygon {
                corners: corners
                    .into_iter()
                    .map(|point| Corner::new(point, point))
                    .collect(),
                material_slot: poly.material_slot,
            });
        }
    }

    let mut creases = Vec::new();
    let mut seams = Vec::new();
    for (ei, (a, b, _)) in edges.iter().enumerate() {
        let mid = edge_points[ei];
        if mesh.is_crease(*a, *b) {
            creases.push(pair(*a, mid));
            creases.push(pair(mid, *b));
        }
        if mesh.is_seam(*a, *b) {
            seams.push(pair(*a, mid));
            seams.push(pair(mid, *b));
        }
    }
    creases.sort_unstable();
    creases.dedup();
    seams.sort_unstable();
    seams.dedup();

    let mut out = Mesh {
        name: mesh.name.clone(),
        positions,
        uvs,
        polygons: polys,
        creases,
        seams,
        // Subdivision re-authors the UV set per point, so slot pins cannot
        // survive it and are cleared instead of pointing at unrelated UVs.
        pinned_uvs: Vec::new(),
    };
    out.remove_degenerate_polygons();
    out.normalize_creases();
    out.normalize_seams();
    out
}

fn pair(a: u32, b: u32) -> [u32; 2] {
    if a < b {
        [a, b]
    } else {
        [b, a]
    }
}

fn key(a: u32, b: u32) -> (u32, u32) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives;
    use lc_core::Aabb;

    #[test]
    fn cube_level_one_has_quad_topology() {
        let cube = primitives::box_mesh(1.0, 1.0, 1.0);
        let sub = catmull_clark(&cube, 1);
        assert_eq!(sub.polygons.len(), 24);
        assert!(sub.polygons.iter().all(|p| p.corners.len() == 4));
        assert_eq!(sub.positions.len(), 26);
        assert_eq!(sub.edges().len(), 48);
        // V - E + F = 2 for a closed surface.
        assert_eq!(
            sub.positions.len() as i64 - sub.edges().len() as i64 + sub.polygons.len() as i64,
            2
        );
        assert!(sub.validate().is_ok());
    }

    #[test]
    fn linear_subdivision_keeps_the_surface_and_quad_topology() {
        let cube = primitives::box_mesh(2.0, 2.0, 2.0);
        let sub = linear(&cube, 1);
        assert_eq!(sub.polygons.len(), 24);
        assert!(sub.polygons.iter().all(|p| p.corners.len() == 4));
        assert_eq!(sub.positions.len(), 26);
        assert!(sub.validate().is_ok());
        let before: Aabb = cube.bounds();
        let after: Aabb = sub.bounds();
        assert!((before.size() - after.size()).length() < 1e-4);
        assert_eq!(sub.signed_volume() > 0.0, cube.signed_volume() > 0.0);
    }

    #[test]
    fn linear_subdivision_shares_edge_midpoints() {
        let plane = primitives::plane_mesh(2.0, 2.0, 2, 2);
        let sub = linear(&plane, 1);
        assert_eq!(sub.polygons.len(), 16);
        assert!(sub.validate().is_ok());
        assert_eq!(sub.positions.len(), 9 + 12 + 4);
    }

    fn mixed_quad_triangle_cage() -> Mesh {
        let mut mesh = primitives::box_mesh(2.0, 2.0, 2.0);
        let poly = mesh.polygons[0].clone();
        mesh.polygons[0] = Polygon::new(vec![poly.corners[0], poly.corners[1], poly.corners[2]]);
        mesh.polygons.push(Polygon::new(vec![
            poly.corners[0],
            poly.corners[2],
            poly.corners[3],
        ]));
        mesh.normalize_creases();
        mesh
    }

    fn top_creased_cube() -> Mesh {
        let mut mesh = primitives::box_mesh(2.0, 2.0, 2.0);
        let top = mesh.polygons[4].clone();
        for i in 0..top.corners.len() {
            let a = top.corners[i].point;
            let b = top.corners[(i + 1) % top.corners.len()].point;
            assert!(mesh.set_crease(a, b, true));
        }
        assert_eq!(mesh.creases.len(), 4);
        mesh
    }

    #[test]
    fn stam_loop_is_distinct_on_a_mixed_quad_triangle_cage() {
        let cage = mixed_quad_triangle_cage();
        let cc = catmull_clark(&cage, 2);
        let stam = stam_loop(&cage, 2);
        let linear = linear(&cage, 2);
        assert!(cc.polygons.iter().all(|p| p.corners.len() == 4));
        assert!(linear.polygons.iter().all(|p| p.corners.len() == 4));
        assert!(stam.polygons.iter().all(|p| p.corners.len() == 3));
        assert!(stam.validate().is_ok());
        assert_ne!(cc.signature(), stam.signature());
        assert_ne!(linear.signature(), stam.signature());
        assert!((cc.signed_volume() - stam.signed_volume()).abs() > 1e-4);
        assert!((linear.signed_volume() - stam.signed_volume()).abs() > 1e-4);
    }

    #[test]
    fn creases_stay_sharp_under_catmull_clark_and_stam_loop() {
        let smooth = primitives::box_mesh(2.0, 2.0, 2.0);
        let creased = top_creased_cube();
        for levels in [2u32, 4] {
            let cc_smooth = catmull_clark(&smooth, levels);
            let cc_sharp = catmull_clark(&creased, levels);
            assert!(
                cc_sharp.bounds().max.y > cc_smooth.bounds().max.y + 0.05,
                "Catmull-Clark lost its crease at level {levels}"
            );
            assert!((cc_sharp.bounds().max.y - 1.0).abs() < 1e-3);
            assert!(cc_sharp.creases.len() >= 4 * (1 << levels));

            let loop_smooth = stam_loop(&smooth, levels);
            let loop_sharp = stam_loop(&creased, levels);
            assert!(
                loop_sharp.bounds().max.y > loop_smooth.bounds().max.y + 0.05,
                "Stam-Loop lost its crease at level {levels}"
            );
            assert!((loop_sharp.bounds().max.y - 1.0).abs() < 1e-3);
            assert!(loop_sharp.creases.len() >= 4 * (1 << levels));
        }
    }

    #[test]
    fn linear_subdivision_ignores_creases() {
        let smooth = primitives::box_mesh(2.0, 2.0, 2.0);
        let creased = top_creased_cube();
        let smooth = linear(&smooth, 2);
        let creased = linear(&creased, 2);
        assert_eq!(smooth.positions, creased.positions);
        assert_eq!(smooth.polygons, creased.polygons);
        assert!(!creased.creases.is_empty());
    }

    #[test]
    fn subdivision_shrinks_toward_surface() {
        let cube = primitives::box_mesh(2.0, 2.0, 2.0);
        let sub = catmull_clark(&cube, 2);
        let b: Aabb = sub.bounds();
        assert!(b.size().x < 2.0);
        assert!(b.size().x > 1.0);
        assert!(sub.signed_volume() > 0.0);
        assert!(sub.positions.iter().all(|p| p.is_finite()));
    }
}
