//! Catmull-Clark subdivision surfaces.

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

    let boundary_neighbours = |p: usize| -> Vec<u32> {
        let mut ns = Vec::new();
        for &ei in &point_edges[p] {
            let (a, b, faces) = &edges[ei];
            if faces.len() == 1 {
                if *a as usize == p {
                    ns.push(*b);
                } else if *b as usize == p {
                    ns.push(*a);
                }
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
        let bns = boundary_neighbours(p);
        if !bns.is_empty() {
            let mut sum = Vec3::ZERO;
            for &b in bns.iter().take(2) {
                sum += mesh.positions[b as usize];
            }
            new_points.push(pos * 0.75 + sum * 0.125);
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
    for (_ei, (a, b, faces)) in edges.iter().enumerate() {
        let pa = mesh.positions[*a as usize];
        let pb = mesh.positions[*b as usize];
        let pos = if faces.len() < 2 {
            (pa + pb) * 0.5
        } else {
            (pa + pb + face_pos[faces[0]] + face_pos[faces[1]]) * 0.25
        };
        let uv_a = point_uv[*a as usize];
        let uv_b = point_uv[*b as usize];
        let uv = if faces.len() < 2 {
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

    // UV indices match point indices.
    let mut out = Mesh {
        name: mesh.name.clone(),
        positions,
        uvs,
        polygons: polys,
    };
    out.remove_degenerate_polygons();
    out
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
