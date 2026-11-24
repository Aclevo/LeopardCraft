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
    carry_extra_uv_sets(mesh, levels, &mut out, catmull_clark);
    out
}

/// Run the same subdivision on each additional UV set.
///
/// The UV0 rules are point-based and shared by the topology, so a probe mesh
/// whose UV0 is the extra set produces that set's coordinates for the new
/// points. Clearing the probe's extra sets keeps the call non-recursive.
fn carry_extra_uv_sets(
    source: &Mesh,
    levels: u32,
    out: &mut Mesh,
    subdivide: fn(&Mesh, u32) -> Mesh,
) {
    if source.extra_uvs.is_empty() || out.uvs.is_empty() {
        return;
    }
    let mut extras = Vec::with_capacity(source.extra_uvs.len());
    for set in &source.extra_uvs {
        let mut probe = source.clone();
        probe.uvs = set.clone();
        probe.extra_uvs.clear();
        let mut values = subdivide(&probe, levels).uvs;
        values.resize(out.uvs.len(), Vec2::ZERO);
        extras.push(values);
    }
    out.extra_uvs = extras;
    out.sync_uv_sets();
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

    // Boundary edges behave as full creases. Authored creases use the same
    // sharp vertex rule, blended toward the smooth rule by the weakest incident
    // crease weight: two incident creases follow the crease curve, three or
    // more pin the point, and a single crease falls back to the smooth rule.
    let sharp_neighbours = |p: usize| -> Vec<(u32, f32)> {
        let mut ns: Vec<(u32, f32)> = Vec::new();
        for &ei in &point_edges[p] {
            let (a, b, faces) = &edges[ei];
            let weight = if faces.len() < 2 {
                1.0
            } else {
                mesh.crease_weight(*a, *b)
            };
            if weight <= 0.0 {
                continue;
            }
            let other = if *a as usize == p {
                *b
            } else if *b as usize == p {
                *a
            } else {
                continue;
            };
            if let Some(existing) = ns.iter_mut().find(|(point, _)| *point == other) {
                existing.1 = existing.1.max(weight);
            } else {
                ns.push((other, weight));
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
        let f = faces.iter().map(|&fi| face_pos[fi]).sum::<Vec3>() / faces.len() as f32;
        let mut r = Vec3::ZERO;
        for &ei in &point_edges[p] {
            let (a, b, _) = edges[ei];
            r += (mesh.positions[a as usize] + mesh.positions[b as usize]) * 0.5;
        }
        r /= point_edges[p].len().max(1) as f32;
        let n = faces.len() as f32;
        let smooth = (f + r * 2.0 + pos * (n - 3.0)) / n;
        let sharp = sharp_neighbours(p);
        if sharp.len() >= 2 {
            let sharp_pos = if sharp.len() > 2 {
                pos
            } else {
                let sum = mesh.positions[sharp[0].0 as usize] + mesh.positions[sharp[1].0 as usize];
                pos * 0.75 + sum * 0.125
            };
            let strength = sharp
                .iter()
                .map(|(_, weight)| *weight)
                .fold(1.0_f32, f32::min);
            new_points.push(smooth.lerp(sharp_pos, strength));
        } else {
            new_points.push(smooth);
        }
    }
    for (i, p) in new_points.into_iter().enumerate() {
        positions[i] = p;
    }

    // Edge points.
    let mut edge_point_index = Vec::with_capacity(n_edges);
    for (a, b, faces) in &edges {
        let pa = mesh.positions[*a as usize];
        let pb = mesh.positions[*b as usize];
        let sharpness = if faces.len() < 2 {
            1.0
        } else {
            mesh.crease_weight(*a, *b)
        };
        let smooth = if faces.len() < 2 {
            (pa + pb) * 0.5
        } else {
            (pa + pb + face_pos[faces[0]] + face_pos[faces[1]]) * 0.25
        };
        let pos = smooth.lerp((pa + pb) * 0.5, sharpness);
        let uv_a = point_uv[*a as usize];
        let uv_b = point_uv[*b as usize];
        let smooth_uv = if faces.len() < 2 {
            (uv_a + uv_b) * 0.5
        } else {
            (uv_a + uv_b + face_uv[faces[0]] + face_uv[faces[1]]) * 0.25
        };
        let uv = smooth_uv.lerp((uv_a + uv_b) * 0.5, sharpness);
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

    // Split authored creases and seams onto both child edges, preserving each
    // fractional crease weight, so sharpness and UV chart borders survive
    // further subdivision levels.
    let mut creases = Vec::new();
    let mut crease_weights = Vec::new();
    let mut seams = Vec::new();
    for (ei, (a, b, _)) in edges.iter().enumerate() {
        let mid = edge_point_index[ei] as u32;
        let weight = mesh.crease_weight(*a, *b);
        push_crease(&mut creases, &mut crease_weights, *a, mid, weight);
        push_crease(&mut creases, &mut crease_weights, mid, *b, weight);
        if mesh.is_seam(*a, *b) {
            seams.push(pair(*a, mid));
            seams.push(pair(mid, *b));
        }
    }
    creases.sort_unstable();
    creases.dedup();
    crease_weights.sort_by_key(|(edge, _)| *edge);
    crease_weights.dedup_by(|(a, _), (b, _)| a == b);
    seams.sort_unstable();
    seams.dedup();

    // UV indices match point indices.
    let mut out = Mesh {
        name: mesh.name.clone(),
        positions,
        uvs,
        polygons: polys,
        creases,
        crease_weights: Box::new(crease_weights),
        seams,
        // Subdivision re-authors the UV set per point, so slot pins cannot
        // survive it and are cleared instead of pointing at unrelated UVs.
        pinned_uvs: Vec::new(),
        extra_uvs: Vec::new(),
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
    carry_extra_uv_sets(mesh, levels, &mut out, linear);
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
    let mut crease_weights = Vec::new();
    let mut seams = Vec::new();
    for [a, b] in &mesh.creases {
        if let Some(&mid) = edge_mid.get(&key(*a, *b)) {
            push_crease(&mut creases, &mut crease_weights, *a, mid, 1.0);
            push_crease(&mut creases, &mut crease_weights, mid, *b, 1.0);
        }
    }
    for ([a, b], weight) in mesh.crease_weights.iter() {
        if let Some(&mid) = edge_mid.get(&key(*a, *b)) {
            push_crease(&mut creases, &mut crease_weights, *a, mid, *weight);
            push_crease(&mut creases, &mut crease_weights, mid, *b, *weight);
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
    crease_weights.sort_by_key(|(edge, _)| *edge);
    crease_weights.dedup_by(|(a, _), (b, _)| a == b);
    seams.sort_unstable();
    seams.dedup();

    let mut out = Mesh {
        name: mesh.name.clone(),
        positions,
        uvs,
        polygons: polys,
        creases,
        crease_weights: Box::new(crease_weights),
        seams,
        // Subdivision re-authors the UV set per point, so slot pins cannot
        // survive it and are cleared instead of pointing at unrelated UVs.
        pinned_uvs: Vec::new(),
        extra_uvs: Vec::new(),
    };
    out.remove_degenerate_polygons();
    out.normalize_creases();
    out.normalize_seams();
    out
}

/// Stam & Loop's hybrid quad/triangle subdivision (sources 44 and 45).
///
/// One iteration is a single operator. Every n-gon is split into n quads around
/// its centroid — three quads for a triangle, four for a quad — and then the old
/// vertices, edge points and face points are positioned with the rule for the
/// region they sit in: Catmull-Clark's masks where every surrounding face is a
/// quad (or an n-gon), Loop's masks where every surrounding face is a triangle,
/// and the paper's mixed mask where the two regions meet. A mixed vertex takes
/// `alpha` for itself, `alpha/2` for each edge midpoint around it and `alpha/4`
/// for each face centroid, with `alpha` solved so the weights of the actual ring
/// sum to one; an edge with a quad on one side and a triangle on the other takes
/// the mean of the Catmull-Clark and Loop edge rules, because the paper's
/// ordinary-boundary mask survives only as figure text. Authored creases and
/// boundaries keep the sharp rules. The paper's optional vertex-correction pass
/// is not applied: it recovers the Catmull-Clark and Loop masks from a uniform
/// averaging pass, and this implementation applies those masks directly.
pub fn stam_loop(mesh: &Mesh, levels: u32) -> Mesh {
    let mut out = mesh.clone();
    for _ in 0..levels.min(6) {
        if out.polygons.is_empty() {
            break;
        }
        out = hybrid_once(&out);
    }
    carry_extra_uv_sets(mesh, levels, &mut out, stam_loop);
    out
}

/// An undirected edge of the cage with the opposite corner of every face
/// around it, which the Loop and mixed edge rules need.
type HybridEdge = (u32, u32, Vec<(usize, u32)>);

fn hybrid_once(mesh: &Mesh) -> Mesh {
    let npoints = mesh.positions.len();
    let face_count = mesh.polygons.len();
    let is_triangle: Vec<bool> = mesh
        .polygons
        .iter()
        .map(|poly| poly.corners.len() == 3)
        .collect();

    // Face centroids, which are also the face points of the split.
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

    // Unique undirected edges with the opposite corner of every adjacent face,
    // which the Loop edge rule needs.
    let mut edge_index: HashMap<(u32, u32), usize> = HashMap::new();
    let mut edges: Vec<HybridEdge> = Vec::new();
    for (fi, poly) in mesh.polygons.iter().enumerate() {
        let n = poly.corners.len();
        for i in 0..n {
            let a = poly.corners[i].point;
            let b = poly.corners[(i + 1) % n].point;
            if a == b {
                continue;
            }
            let key = key(a, b);
            match edge_index.get(&key) {
                Some(&index) => {
                    if !edges[index].2.iter().any(|(face, _)| *face == fi) {
                        edges[index].2.push((fi, poly.corners[(i + 2) % n].point));
                    }
                }
                None => {
                    edge_index.insert(key, edges.len());
                    edges.push((key.0, key.1, vec![(fi, poly.corners[(i + 2) % n].point)]));
                }
            }
        }
    }
    let n_edges = edges.len();

    let mut positions = Vec::with_capacity(npoints + n_edges + face_count);
    let mut uvs = Vec::with_capacity(npoints + n_edges + face_count);
    for p in &mesh.positions {
        positions.push(*p);
    }

    let mut point_faces: Vec<Vec<usize>> = vec![Vec::new(); npoints];
    let mut point_edges: Vec<Vec<usize>> = vec![Vec::new(); npoints];
    let mut point_neighbours: Vec<Vec<u32>> = vec![Vec::new(); npoints];
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
        if !point_neighbours[*a as usize].contains(b) {
            point_neighbours[*a as usize].push(*b);
        }
        if !point_neighbours[*b as usize].contains(a) {
            point_neighbours[*b as usize].push(*a);
        }
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

    // Boundary edges behave as full creases and authored creases blend the
    // smooth rule toward the sharp crease rule, as in the Catmull-Clark path.
    let sharp_neighbours = |p: usize| -> Vec<(u32, f32)> {
        let mut ns: Vec<(u32, f32)> = Vec::new();
        for &ei in &point_edges[p] {
            let (a, b, adjacent) = &edges[ei];
            let weight = if adjacent.len() < 2 {
                1.0
            } else {
                mesh.crease_weight(*a, *b)
            };
            if weight <= 0.0 {
                continue;
            }
            let other = if *a as usize == p {
                *b
            } else if *b as usize == p {
                *a
            } else {
                continue;
            };
            if let Some(existing) = ns.iter_mut().find(|(point, _)| *point == other) {
                existing.1 = existing.1.max(weight);
            } else {
                ns.push((other, weight));
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
        let triangles = faces.iter().filter(|fi| is_triangle[**fi]).count();
        let smooth = if triangles == 0 {
            // Catmull-Clark vertex rule.
            let f = faces.iter().map(|&fi| face_pos[fi]).sum::<Vec3>() / faces.len() as f32;
            let mut r = Vec3::ZERO;
            for &ei in &point_edges[p] {
                let (a, b, _) = edges[ei];
                r += (mesh.positions[a as usize] + mesh.positions[b as usize]) * 0.5;
            }
            r /= point_edges[p].len().max(1) as f32;
            let n = faces.len() as f32;
            (f + r * 2.0 + pos * (n - 3.0)) / n
        } else if triangles == faces.len() {
            // Loop vertex rule.
            let neighbours = &point_neighbours[p];
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
        } else {
            // Stam & Loop's mixed mask: `beta = alpha/2` on the edge midpoints
            // and `gamma = alpha/4` on the face centroids, with `alpha` fixed by
            // requiring the weights of this ring to sum to one.
            let n_e = point_edges[p].len() as f32;
            let n_f = faces.len() as f32;
            let alpha = 1.0 / (1.0 + n_e / 2.0 + n_f / 4.0);
            let edge_sum = point_edges[p]
                .iter()
                .map(|&ei| {
                    let (a, b, _) = edges[ei];
                    (mesh.positions[a as usize] + mesh.positions[b as usize]) * 0.5
                })
                .sum::<Vec3>();
            let face_sum = faces.iter().map(|&fi| face_pos[fi]).sum::<Vec3>();
            pos * alpha + edge_sum * (alpha / 2.0) + face_sum * (alpha / 4.0)
        };
        let sharp = sharp_neighbours(p);
        if sharp.len() >= 2 {
            let sharp_pos = if sharp.len() > 2 {
                pos
            } else {
                let sum = mesh.positions[sharp[0].0 as usize] + mesh.positions[sharp[1].0 as usize];
                pos * 0.75 + sum * 0.125
            };
            let strength = sharp
                .iter()
                .map(|(_, weight)| *weight)
                .fold(1.0_f32, f32::min);
            new_points.push(smooth.lerp(sharp_pos, strength));
        } else {
            new_points.push(smooth);
        }
    }
    for (i, p) in new_points.into_iter().enumerate() {
        positions[i] = p;
    }

    // Edge points: the Catmull-Clark rule on a quad edge, the Loop rule on a
    // triangle edge and their mean on an edge between the two regions.
    let mut edge_point_index = Vec::with_capacity(n_edges);
    for (a, b, adjacent) in &edges {
        let pa = mesh.positions[*a as usize];
        let pb = mesh.positions[*b as usize];
        let sharpness = if adjacent.len() < 2 {
            1.0
        } else {
            mesh.crease_weight(*a, *b)
        };
        let midpoint = (pa + pb) * 0.5;
        let midpoint_uv = (point_uv[*a as usize] + point_uv[*b as usize]) * 0.5;
        let (smooth, smooth_uv) = if adjacent.len() < 2 {
            (midpoint, midpoint_uv)
        } else {
            let first = adjacent[0].0;
            let second = adjacent[1].0;
            let cc = (pa + pb + face_pos[first] + face_pos[second]) * 0.25;
            let cc_uv =
                (point_uv[*a as usize] + point_uv[*b as usize] + face_uv[first] + face_uv[second])
                    * 0.25;
            let loop_rule = (pa + pb) * 0.375
                + (mesh.positions[adjacent[0].1 as usize] + mesh.positions[adjacent[1].1 as usize])
                    * 0.125;
            let loop_uv = midpoint_uv;
            match (is_triangle[first], is_triangle[second]) {
                (false, false) => (cc, cc_uv),
                (true, true) => (loop_rule, loop_uv),
                _ => ((cc + loop_rule) * 0.5, (cc_uv + loop_uv) * 0.5),
            }
        };
        let pos = smooth.lerp(midpoint, sharpness);
        let uv = smooth_uv.lerp(midpoint_uv, sharpness);
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

    // Faces: an n-gon becomes n quads around its centroid, so a triangle
    // becomes three quads and a quad four, and no cage is triangulated.
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

    // Split authored creases and seams onto both child edges, preserving each
    // fractional crease weight, so sharpness and UV chart borders survive
    // further subdivision levels.
    let mut creases = Vec::new();
    let mut crease_weights = Vec::new();
    let mut seams = Vec::new();
    for (ei, (a, b, _)) in edges.iter().enumerate() {
        let mid = edge_point_index[ei] as u32;
        let weight = mesh.crease_weight(*a, *b);
        push_crease(&mut creases, &mut crease_weights, *a, mid, weight);
        push_crease(&mut creases, &mut crease_weights, mid, *b, weight);
        if mesh.is_seam(*a, *b) {
            seams.push(pair(*a, mid));
            seams.push(pair(mid, *b));
        }
    }
    creases.sort_unstable();
    creases.dedup();
    crease_weights.sort_by_key(|(edge, _)| *edge);
    crease_weights.dedup_by(|(a, _), (b, _)| a == b);
    seams.sort_unstable();
    seams.dedup();

    let mut out = Mesh {
        name: mesh.name.clone(),
        positions,
        uvs,
        polygons: polys,
        creases,
        crease_weights: Box::new(crease_weights),
        seams,
        // Subdivision re-authors the UV set per point, so slot pins cannot
        // survive it and are cleared instead of pointing at unrelated UVs.
        pinned_uvs: Vec::new(),
        extra_uvs: Vec::new(),
    };
    out.remove_degenerate_polygons();
    out.normalize_creases();
    out.normalize_seams();
    out
}

/// Subdivision scheme used by the face-selective polygon tool.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SubdivisionKind {
    #[default]
    CatmullClark,
    StamLoop,
    Linear,
}

impl SubdivisionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::CatmullClark => "Catmull-Clark",
            Self::StamLoop => "Stam-Loop",
            Self::Linear => "Linear",
        }
    }
}

/// Subdivide only the selected polygons, leaving the rest of the mesh intact.
///
/// The selection is subdivided as a patch. Its boundary is pinned onto the
/// original boundary segments and every unselected polygon that shares one of
/// those segments gains the new collinear boundary points. The result is
/// watertight across the selection border, and unselected vertices keep their
/// exact positions. Creases, fractional crease weights, seams, UVs and material
/// slots survive; UV pins are cleared because subdivision re-authors the slots.
pub fn subdivide_faces(mesh: &Mesh, faces: &[usize], levels: u32, kind: SubdivisionKind) -> Mesh {
    let mut selected = vec![false; mesh.polygons.len()];
    for face in faces {
        if let Some(slot) = selected.get_mut(*face) {
            *slot = true;
        }
    }
    if levels == 0 || !selected.iter().any(|chosen| *chosen) {
        return mesh.clone();
    }

    let submesh = Mesh {
        name: mesh.name.clone(),
        positions: mesh.positions.clone(),
        uvs: mesh.uvs.clone(),
        extra_uvs: Vec::new(),
        polygons: mesh
            .polygons
            .iter()
            .zip(&selected)
            .filter(|(_, chosen)| **chosen)
            .map(|(poly, _)| poly.clone())
            .collect(),
        creases: mesh.creases.clone(),
        crease_weights: mesh.crease_weights.clone(),
        seams: mesh.seams.clone(),
        pinned_uvs: Vec::new(),
    };
    let mut patch = match kind {
        SubdivisionKind::CatmullClark => catmull_clark(&submesh, levels),
        SubdivisionKind::StamLoop => stam_loop(&submesh, levels),
        SubdivisionKind::Linear => linear(&submesh, levels),
    };

    // Original edges used by the selection. Edges on the selection border are
    // split by the patch, so their marks live in the patch already.
    let mut selected_edges: Vec<[u32; 2]> = Vec::new();
    let mut patch_edges: HashMap<(u32, u32), u32> = HashMap::new();
    for poly in &submesh.polygons {
        let n = poly.corners.len();
        for i in 0..n {
            let a = poly.corners[i].point;
            let b = poly.corners[(i + 1) % n].point;
            if a == b {
                continue;
            }
            selected_edges.push(pair(a, b));
        }
    }
    selected_edges.sort_unstable();
    selected_edges.dedup();
    let boundary_segments: Vec<[u32; 2]> = selected_edges
        .iter()
        .copied()
        .filter(|[a, b]| {
            submesh
                .polygons
                .iter()
                .filter(|poly| {
                    let n = poly.corners.len();
                    (0..n).any(|i| {
                        let x = poly.corners[i].point;
                        let y = poly.corners[(i + 1) % n].point;
                        pair(x, y) == [*a, *b]
                    })
                })
                .count()
                == 1
        })
        .collect();

    // A patch boundary vertex list: edges used by exactly one patch polygon.
    for poly in &patch.polygons {
        let n = poly.corners.len();
        for i in 0..n {
            let a = poly.corners[i].point;
            let b = poly.corners[(i + 1) % n].point;
            if a != b {
                *patch_edges.entry(key(a, b)).or_insert(0) += 1;
            }
        }
    }
    let boundary_points: Vec<u32> = patch_edges
        .iter()
        .filter(|(_, count)| **count == 1)
        .flat_map(|((a, b), _)| [*a, *b])
        .collect();

    // Pin every point on the patch boundary, plus new boundary points, so the
    // selection border stays on the original segments and the unselected
    // region keeps its exact shape.
    for point in &boundary_points {
        let index = *point as usize;
        if index < mesh.positions.len() {
            patch.positions[index] = mesh.positions[index];
        }
    }
    for (index, poly) in mesh.polygons.iter().enumerate() {
        if selected[index] {
            continue;
        }
        for corner in &poly.corners {
            let point = corner.point as usize;
            if point < patch.positions.len() {
                patch.positions[point] = mesh.positions[point];
            }
        }
    }
    for point in &boundary_points {
        let index = *point as usize;
        if index < mesh.positions.len() {
            continue;
        }
        let p = patch.positions[index];
        let mut closest = (f32::INFINITY, 0.0_f32, 0);
        for (segment, [a, b]) in boundary_segments.iter().enumerate() {
            let pa = mesh.positions[*a as usize];
            let pb = mesh.positions[*b as usize];
            let (t, distance) = segment_parameter(p, pa, pb);
            if distance < closest.0 {
                closest = (distance, t, segment);
            }
        }
        if closest.0.is_finite() {
            let [a, b] = boundary_segments[closest.2];
            patch.positions[index] =
                mesh.positions[a as usize].lerp(mesh.positions[b as usize], closest.1);
        }
    }

    // Re-add unselected polygons, threading every new boundary point that lies
    // on one of their edges into the corner list in order. Patch UVs are
    // authored per point, so unselected corners append their original UV value
    // to the same slots instead of reusing original slot indices.
    let mut polygons = patch.polygons.clone();
    let mut uvs = patch.uvs.clone();
    for (fi, poly) in mesh.polygons.iter().enumerate() {
        if selected[fi] {
            continue;
        }
        let mut corners = Vec::with_capacity(poly.corners.len());
        let n = poly.corners.len();
        for i in 0..n {
            let original_corner = poly.corners[i];
            let original_slot = uvs.len();
            uvs.push(mesh.uv_of(&original_corner));
            corners.push(Corner::new(original_corner.point, original_slot as u32));
            let a = poly.corners[i].point;
            let b = poly.corners[(i + 1) % n].point;
            if a == b {
                continue;
            }
            let pa = mesh.positions[a as usize];
            let pb = mesh.positions[b as usize];
            let mut inserts: Vec<(f32, u32)> = boundary_points
                .iter()
                .copied()
                .filter(|point| (*point as usize) >= mesh.positions.len())
                .filter_map(|point| {
                    let (t, distance) = segment_parameter(patch.positions[point as usize], pa, pb);
                    if t > 1e-5 && t < 1.0 - 1e-5 && distance <= 1e-4 {
                        Some((t, point))
                    } else {
                        None
                    }
                })
                .collect();
            inserts.sort_by(|(ta, pa), (tb, pb)| ta.total_cmp(tb).then_with(|| pa.cmp(pb)));
            inserts.dedup_by_key(|(_, point)| *point);
            for (_, point) in inserts {
                let slot = uvs.len();
                uvs.push(patch.uvs.get(point as usize).copied().unwrap_or(Vec2::ZERO));
                corners.push(Corner::new(point, slot as u32));
            }
        }
        polygons.push(Polygon {
            corners,
            material_slot: poly.material_slot,
        });
    }

    // Marks wholly inside the unselected region keep their original indices;
    // marks touching the selection were split by the patch.
    for [a, b] in &mesh.creases {
        if selected_edges.binary_search(&pair(*a, *b)).is_err() {
            patch.creases.push(pair(*a, *b));
        }
    }
    for ([a, b], weight) in mesh.crease_weights.iter() {
        if selected_edges.binary_search(&pair(*a, *b)).is_err() {
            patch.crease_weights.push((pair(*a, *b), *weight));
        }
    }
    for [a, b] in &mesh.seams {
        if selected_edges.binary_search(&pair(*a, *b)).is_err() {
            patch.seams.push(pair(*a, *b));
        }
    }
    patch.polygons = polygons;
    patch.uvs = uvs;
    patch.pinned_uvs.clear();
    patch.remove_degenerate_polygons();
    patch.normalize_creases();
    patch.normalize_seams();

    if !mesh.extra_uvs.is_empty() {
        let mut extras = Vec::with_capacity(mesh.extra_uvs.len());
        for set in &mesh.extra_uvs {
            let mut probe = mesh.clone();
            probe.uvs = set.clone();
            probe.extra_uvs.clear();
            let mut values = subdivide_faces(&probe, faces, levels, kind).uvs;
            values.resize(patch.uvs.len(), Vec2::ZERO);
            extras.push(values);
        }
        patch.extra_uvs = extras;
        patch.sync_uv_sets();
    }
    patch
}

/// Closest point parameter and distance from `p` to segment `a`-`b`.
fn segment_parameter(p: Vec3, a: Vec3, b: Vec3) -> (f32, f32) {
    let edge = b - a;
    let length_squared = edge.length_squared();
    if length_squared <= 1e-12 {
        return (0.0, p.distance(a));
    }
    let t = ((p - a).dot(edge) / length_squared).clamp(0.0, 1.0);
    (t, p.distance(a + edge * t))
}

fn push_crease(
    creases: &mut Vec<[u32; 2]>,
    weights: &mut Vec<([u32; 2], f32)>,
    a: u32,
    b: u32,
    weight: f32,
) {
    if weight <= 0.0 {
        return;
    }
    let edge = pair(a, b);
    if weight >= 1.0 {
        creases.push(edge);
    } else {
        weights.push((edge, weight));
    }
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

    fn top_weighted_cube(weight: f32) -> Mesh {
        let mut mesh = primitives::box_mesh(2.0, 2.0, 2.0);
        let top = mesh.polygons[4].clone();
        for i in 0..top.corners.len() {
            let a = top.corners[i].point;
            let b = top.corners[(i + 1) % top.corners.len()].point;
            if weight > 0.0 {
                assert!(mesh.set_crease_weight(a, b, weight));
            }
        }
        mesh
    }

    fn face_centre(mesh: &Mesh, face: usize) -> Vec3 {
        let points = mesh.polygon_points(face);
        points.iter().copied().sum::<Vec3>() / points.len() as f32
    }

    /// Independent Catmull-Clark smooth rule for one point, so the weighted
    /// test does not just repeat the implementation's arithmetic.
    fn cc_smooth_point(mesh: &Mesh, point: u32) -> Vec3 {
        let mut faces = Vec::new();
        for (fi, poly) in mesh.polygons.iter().enumerate() {
            if poly.corners.iter().any(|c| c.point == point) && !faces.contains(&fi) {
                faces.push(fi);
            }
        }
        let f = faces.iter().map(|fi| face_centre(mesh, *fi)).sum::<Vec3>() / faces.len() as f32;
        let incident = mesh
            .edges()
            .into_iter()
            .filter(|edge| edge.a == point || edge.b == point)
            .collect::<Vec<_>>();
        let r = incident
            .iter()
            .map(|edge| (mesh.positions[edge.a as usize] + mesh.positions[edge.b as usize]) * 0.5)
            .sum::<Vec3>()
            / incident.len() as f32;
        let pos = mesh.positions[point as usize];
        let n = faces.len() as f32;
        (f + r * 2.0 + pos * (n - 3.0)) / n
    }

    #[test]
    fn catmull_clark_crease_weights_blend_edge_and_vertex_rules_exactly() {
        let cube = primitives::box_mesh(2.0, 2.0, 2.0);
        let top = cube.polygons[4].clone();
        let a = top.corners[0].point;
        let b = top.corners[1].point;
        let edge_faces = cube
            .polygons
            .iter()
            .enumerate()
            .filter(|(_, poly)| {
                poly.corners.iter().any(|c| c.point == a)
                    && poly.corners.iter().any(|c| c.point == b)
            })
            .map(|(fi, _)| fi)
            .collect::<Vec<_>>();
        assert_eq!(edge_faces.len(), 2, "the test edge must be interior");
        let sharp_edge = (cube.positions[a as usize] + cube.positions[b as usize]) * 0.5;
        let smooth_edge = (cube.positions[a as usize]
            + cube.positions[b as usize]
            + face_centre(&cube, edge_faces[0])
            + face_centre(&cube, edge_faces[1]))
            * 0.25;

        let smooth_vertex = cc_smooth_point(&cube, a);
        let crease_neighbours = cube
            .edges()
            .into_iter()
            .filter(|edge| edge.a == a || edge.b == a)
            .filter(|edge| {
                top.corners.iter().any(|c| c.point == edge.a)
                    && top.corners.iter().any(|c| c.point == edge.b)
            })
            .map(|edge| if edge.a == a { edge.b } else { edge.a })
            .collect::<Vec<_>>();
        assert_eq!(crease_neighbours.len(), 2);
        let sharp_vertex = cube.positions[a as usize] * 0.75
            + (cube.positions[crease_neighbours[0] as usize]
                + cube.positions[crease_neighbours[1] as usize])
                * 0.125;

        for weight in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let weighted = top_weighted_cube(weight);
            let sub = catmull_clark(&weighted, 1);
            let edge_expected = smooth_edge.lerp(sharp_edge, weight);
            let vertex_expected = smooth_vertex.lerp(sharp_vertex, weight);
            assert!(
                sub.positions
                    .iter()
                    .any(|p| p.distance(edge_expected) < 1e-5),
                "weight {weight} did not produce the blended edge point at {edge_expected}"
            );
            assert!(
                sub.positions[a as usize].distance(vertex_expected) < 1e-5,
                "weight {weight} did not blend the crease corner: got {}, expected {}",
                sub.positions[a as usize],
                vertex_expected
            );
        }

        let half = catmull_clark(&top_weighted_cube(0.5), 1);
        assert!(
            half.crease_weights
                .iter()
                .any(|(_, weight)| (*weight - 0.5).abs() < 1e-6),
            "fractional child creases must keep their weight"
        );
    }

    #[test]
    fn stam_loop_blends_fractional_crease_corners_between_the_two_rules() {
        let zero = stam_loop(&top_weighted_cube(0.0), 1);
        let half = stam_loop(&top_weighted_cube(0.5), 1);
        let one = stam_loop(&top_weighted_cube(1.0), 1);
        let corner = primitives::box_mesh(2.0, 2.0, 2.0).polygons[4].corners[0].point as usize;
        let expected = zero.positions[corner].lerp(one.positions[corner], 0.5);
        assert!(half.positions[corner].distance(expected) < 1e-5);
        assert!(half.positions[corner].distance(zero.positions[corner]) > 1e-4);
        assert!(half.positions[corner].distance(one.positions[corner]) > 1e-4);
        assert!(half.crease_weights.iter().any(|(_, weight)| *weight == 0.5));
    }

    #[test]
    fn partial_subdivision_is_watertight_and_preserves_the_unselected_region() {
        let plane = primitives::plane_mesh(3.0, 3.0, 3, 3);
        let original_positions = plane.positions.clone();
        let selected_face = 0usize;
        let selected_points = plane.polygons[selected_face]
            .corners
            .iter()
            .map(|corner| corner.point)
            .collect::<Vec<_>>();
        for kind in [
            SubdivisionKind::CatmullClark,
            SubdivisionKind::StamLoop,
            SubdivisionKind::Linear,
        ] {
            let partial = subdivide_faces(&plane, &[selected_face], 1, kind);
            if let Err(errors) = partial.validate() {
                panic!("{kind:?} produced invalid geometry: {errors:?}");
            }
            let unselected_points: std::collections::HashSet<u32> = plane
                .polygons
                .iter()
                .enumerate()
                .filter(|(fi, _)| *fi != selected_face)
                .flat_map(|(_, poly)| poly.corners.iter().map(|corner| corner.point))
                .collect();
            for index in unselected_points {
                assert_eq!(
                    partial.positions[index as usize], original_positions[index as usize],
                    "{kind:?} moved unselected point {index}"
                );
            }
            // Every interior edge is shared by exactly two faces; only the
            // original outer border may stay open.
            for edge in partial.edges() {
                if edge.faces.len() == 1 {
                    let a = partial.positions[edge.a as usize];
                    let b = partial.positions[edge.b as usize];
                    let outer = a.x.abs() > 1.5 - 1e-4
                        || a.z.abs() > 1.5 - 1e-4
                        || b.x.abs() > 1.5 - 1e-4
                        || b.z.abs() > 1.5 - 1e-4;
                    assert!(outer, "{kind:?} left an interior crack at {:?}", (a, b));
                }
            }
            // The selection boundary gains one midpoint per selected edge, and
            // each is used by both the patch and its unselected neighbour.
            for i in 0..selected_points.len() {
                let a = selected_points[i];
                let b = selected_points[(i + 1) % selected_points.len()];
                let midpoint = plane.positions[a as usize].lerp(plane.positions[b as usize], 0.5);
                let end_a = plane.positions[a as usize];
                let end_b = plane.positions[b as usize];
                let uses = partial
                    .edges()
                    .into_iter()
                    .filter(|edge| {
                        let pa = partial.positions[edge.a as usize];
                        let pb = partial.positions[edge.b as usize];
                        let has_mid = pa.distance(midpoint) < 1e-5 || pb.distance(midpoint) < 1e-5;
                        let has_end = pa.distance(end_a) < 1e-5
                            || pa.distance(end_b) < 1e-5
                            || pb.distance(end_a) < 1e-5
                            || pb.distance(end_b) < 1e-5;
                        has_mid && has_end
                    })
                    .count();
                assert_eq!(
                    uses, 2,
                    "{kind:?} did not split the boundary edge into two shared segments"
                );
            }
            assert!(partial.polygons.iter().any(|poly| poly.corners.len() == 5));
            // No duplicate watertight-boundary vertex.
            for i in 0..partial.positions.len() {
                for j in (i + 1)..partial.positions.len() {
                    assert!(
                        partial.positions[i].distance(partial.positions[j]) > 1e-6,
                        "{kind:?} duplicated a boundary vertex"
                    );
                }
            }
        }
    }

    #[test]
    fn stam_loop_is_distinct_on_a_mixed_quad_triangle_cage() {
        let cage = mixed_quad_triangle_cage();
        let cc = catmull_clark(&cage, 2);
        let stam = stam_loop(&cage, 2);
        let linear = linear(&cage, 2);
        assert!(cc.polygons.iter().all(|p| p.corners.len() == 4));
        assert!(linear.polygons.iter().all(|p| p.corners.len() == 4));
        assert!(
            stam.polygons.iter().all(|p| p.corners.len() == 4),
            "Stam-Loop must quadrangulate a mixed cage instead of triangulating it"
        );
        assert!(stam.validate().is_ok());
        assert_ne!(cc.signature(), stam.signature());
        assert_ne!(linear.signature(), stam.signature());
        assert!((cc.signed_volume() - stam.signed_volume()).abs() > 1e-4);
        assert!((linear.signed_volume() - stam.signed_volume()).abs() > 1e-4);
    }

    #[test]
    fn stam_loop_reduces_to_catmull_clark_on_quad_and_ngon_cages() {
        let pentagon = Mesh::from_polygons(
            "pentagon",
            (0..5)
                .map(|i| {
                    let angle = std::f32::consts::TAU * i as f32 / 5.0;
                    Vec3::new(angle.cos(), 0.0, angle.sin())
                })
                .collect(),
            vec![vec![0, 1, 2, 3, 4]],
        );
        for cage in [
            primitives::box_mesh(2.0, 2.0, 2.0),
            primitives::plane_mesh(3.0, 3.0, 3, 3),
            pentagon,
        ] {
            for levels in [1u32, 2] {
                assert_eq!(
                    stam_loop(&cage, levels),
                    catmull_clark(&cage, levels),
                    "a cage of quads and n-gons must follow the Catmull-Clark masks at level {levels}"
                );
            }
        }
    }

    #[test]
    fn stam_loop_uses_loop_rules_on_a_triangle_only_cage() {
        let cube = primitives::box_mesh(2.0, 2.0, 2.0);
        let faces = (0..cube.polygons.len() as u32).collect::<Vec<_>>();
        let cage = crate::ops::triangulate_faces(&cube, &faces);
        assert!(cage.polygons.iter().all(|p| p.corners.len() == 3));

        let out = stam_loop(&cage, 1);
        assert_eq!(out.polygons.len(), cage.polygons.len() * 3);
        assert!(out.polygons.iter().all(|p| p.corners.len() == 4));

        // Old vertices keep Loop's vertex mask, including the valence-three
        // special case that a triangulated cube hits at every corner.
        for (index, position) in cage.positions.iter().enumerate() {
            let neighbours = cage
                .edges()
                .into_iter()
                .filter_map(|edge| {
                    if edge.a as usize == index {
                        Some(edge.b)
                    } else if edge.b as usize == index {
                        Some(edge.a)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert!(!neighbours.is_empty());
            let n = neighbours.len() as f32;
            let beta = if neighbours.len() == 3 {
                3.0 / 16.0
            } else {
                3.0 / (8.0 * n)
            };
            let sum = neighbours
                .iter()
                .map(|i| cage.positions[*i as usize])
                .sum::<Vec3>();
            let expected = *position * (1.0 - n * beta) + sum * beta;
            assert!(
                out.positions[index].distance(expected) < 1e-5,
                "vertex {index} did not use Loop's vertex mask"
            );
        }

        // An interior edge takes Loop's edge rule with both opposite corners.
        let edge = cage
            .edges()
            .into_iter()
            .find(|edge| edge.faces.len() == 2)
            .unwrap();
        let opposite = |face: u32| {
            let poly = &cage.polygons[face as usize];
            poly.corners
                .iter()
                .map(|c| c.point)
                .find(|p| *p != edge.a && *p != edge.b)
                .unwrap()
        };
        let expected = (cage.positions[edge.a as usize] + cage.positions[edge.b as usize]) * 0.375
            + (cage.positions[opposite(edge.faces[0]) as usize]
                + cage.positions[opposite(edge.faces[1]) as usize])
                * 0.125;
        assert!(
            out.positions.iter().any(|p| p.distance(expected) < 1e-5),
            "the triangle-only cage did not place Loop's edge point"
        );
    }

    #[test]
    fn stam_loop_positions_each_region_with_its_own_rule() {
        let cage = mixed_quad_triangle_cage();
        let hybrid = stam_loop(&cage, 1);
        let cc = catmull_clark(&cage, 1);
        assert!(hybrid.validate().is_ok());

        let ring = |point: usize| {
            cage.polygons
                .iter()
                .enumerate()
                .filter(|(_, poly)| poly.corners.iter().any(|c| c.point as usize == point))
                .map(|(fi, _)| fi)
                .collect::<Vec<_>>()
        };
        let quad_only = (0..cage.positions.len())
            .filter(|point| {
                let faces = ring(*point);
                !faces.is_empty() && faces.iter().all(|fi| cage.polygons[*fi].corners.len() != 3)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            quad_only.len(),
            4,
            "the far side of the cage stays quad-only"
        );
        for point in quad_only {
            assert!(
                hybrid.positions[point].distance(cc.positions[point]) < 1e-6,
                "quad-only vertex {point} left the Catmull-Clark mask"
            );
        }

        // The two corners of the triangulated face are mixed and take the
        // paper's mask, so they must differ from Catmull-Clark's positions.
        let mixed = (0..cage.positions.len())
            .filter(|point| {
                let faces = ring(*point);
                faces.iter().any(|fi| cage.polygons[*fi].corners.len() == 3)
                    && faces.iter().any(|fi| cage.polygons[*fi].corners.len() != 3)
            })
            .collect::<Vec<_>>();
        assert!(!mixed.is_empty());
        // A mixed vertex is placed by Stam & Loop's mask: alpha on the vertex
        // itself, alpha/2 on every edge midpoint around it and alpha/4 on every
        // face centroid, with alpha solved so the weights sum to one.
        let mut differs = 0;
        for point in mixed {
            let faces = ring(point);
            let midpoints = cage
                .edges()
                .into_iter()
                .filter_map(|edge| {
                    if edge.a as usize == point {
                        Some(edge.b)
                    } else if edge.b as usize == point {
                        Some(edge.a)
                    } else {
                        None
                    }
                })
                .map(|other| (cage.positions[point] + cage.positions[other as usize]) * 0.5)
                .collect::<Vec<_>>();
            let centroids = faces
                .iter()
                .map(|fi| {
                    let pts = cage.polygon_points(*fi);
                    pts.iter().copied().sum::<Vec3>() / pts.len() as f32
                })
                .collect::<Vec<_>>();
            let alpha = 1.0 / (1.0 + midpoints.len() as f32 / 2.0 + faces.len() as f32 / 4.0);
            let expected = cage.positions[point] * alpha
                + midpoints.iter().copied().sum::<Vec3>() * (alpha / 2.0)
                + centroids.iter().copied().sum::<Vec3>() * (alpha / 4.0);
            assert!(
                hybrid.positions[point].distance(expected) < 1e-6,
                "mixed vertex {point} did not take Stam & Loop's mask"
            );
            if hybrid.positions[point].distance(cc.positions[point]) > 1e-5 {
                differs += 1;
            }
        }
        assert!(
            differs > 0,
            "the mixed mask must move at least one vertex off Catmull-Clark"
        );

        // An edge with a quad on one side and a triangle on the other takes the
        // mean of the Catmull-Clark and Loop edge rules.
        let mixed_edge = cage
            .edges()
            .into_iter()
            .find(|edge| {
                edge.faces.len() == 2
                    && edge
                        .faces
                        .iter()
                        .any(|fi| cage.polygons[*fi as usize].corners.len() == 3)
                    && edge
                        .faces
                        .iter()
                        .any(|fi| cage.polygons[*fi as usize].corners.len() != 3)
            })
            .expect("the mixed cage must have a quad/triangle edge");
        let opposite = |face: u32| {
            cage.polygons[face as usize]
                .corners
                .iter()
                .map(|c| c.point)
                .find(|p| *p != mixed_edge.a && *p != mixed_edge.b)
                .unwrap()
        };
        let pa = cage.positions[mixed_edge.a as usize];
        let pb = cage.positions[mixed_edge.b as usize];
        let cc_edge = (pa
            + pb
            + face_centre(&cage, mixed_edge.faces[0] as usize)
            + face_centre(&cage, mixed_edge.faces[1] as usize))
            * 0.25;
        let loop_edge = (pa + pb) * 0.375
            + (cage.positions[opposite(mixed_edge.faces[0]) as usize]
                + cage.positions[opposite(mixed_edge.faces[1]) as usize])
                * 0.125;
        let expected = (cc_edge + loop_edge) * 0.5;
        assert!(
            hybrid.positions.iter().any(|p| p.distance(expected) < 1e-5),
            "a quad/triangle edge did not average the two edge rules"
        );

        // Every mask sums to one, so translating the cage translates every
        // point of the result by the same offset.
        let offset = Vec3::new(0.75, -1.25, 2.0);
        let mut moved = cage.clone();
        for p in &mut moved.positions {
            *p += offset;
        }
        let shifted = stam_loop(&moved, 1);
        assert_eq!(shifted.positions.len(), hybrid.positions.len());
        for (a, b) in shifted.positions.iter().zip(&hybrid.positions) {
            assert!(
                a.distance(*b + offset) < 1e-5,
                "the mixed Stam-Loop mask is not affine invariant"
            );
        }
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

    #[test]
    fn every_scheme_subdivides_additional_uv_sets_with_uv0() {
        for (name, subdivide) in [
            ("catmull-clark", catmull_clark as fn(&Mesh, u32) -> Mesh),
            ("linear", linear),
            ("stam-loop", stam_loop),
        ] {
            let mut cube = primitives::box_mesh(2.0, 2.0, 2.0);
            assert_eq!(cube.add_uv_set(), Some(1));
            let out = subdivide(&cube, 1);
            assert_eq!(out.uv_set_count(), 2, "{name} keeps the number of UV sets");
            assert_eq!(
                out.extra_uvs[0], out.uvs,
                "{name} subdivides a cloned set exactly like UV0"
            );
            assert!(out.validate().is_ok(), "{name} keeps sets parallel");
        }
    }

    #[test]
    fn partial_subdivision_stays_watertight_at_more_than_one_level() {
        let plane = primitives::plane_mesh(3.0, 3.0, 3, 3);
        let partial = subdivide_faces(&plane, &[0], 2, SubdivisionKind::CatmullClark);
        if let Err(errors) = partial.validate() {
            panic!("two-level partial subdivision is invalid: {errors:?}");
        }
        for edge in partial.edges() {
            if edge.faces.len() == 1 {
                let a = partial.positions[edge.a as usize];
                let b = partial.positions[edge.b as usize];
                let outer = a.x.abs() > 1.5 - 1e-4
                    || a.z.abs() > 1.5 - 1e-4
                    || b.x.abs() > 1.5 - 1e-4
                    || b.z.abs() > 1.5 - 1e-4;
                assert!(
                    outer,
                    "two-level partial subdivision cracked at {a:?}-{b:?}"
                );
            }
        }
    }
}
