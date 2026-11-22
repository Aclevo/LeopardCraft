//! Direct-modelling operations: extrude, inset/bevel, ring cut, bridge,
//! mirror/array and simple deformers, plus lathe/sweep creators.

use crate::{Corner, Mesh, Polygon, Spline};
use glam::{Mat4, Quat, Vec2, Vec3};
use std::collections::{HashMap, HashSet};

fn push_polygon(mesh: &mut Mesh, points: &[u32], uvs: &[Vec2], material_slot: u16) -> usize {
    let mut corners = Vec::with_capacity(points.len());
    for (i, &p) in points.iter().enumerate() {
        let uv = mesh.add_uv(uvs.get(i).copied().unwrap_or(Vec2::ZERO));
        corners.push(Corner::new(p, uv));
    }
    mesh.polygons.push(Polygon {
        corners,
        material_slot,
    });
    mesh.polygons.len() - 1
}

/// Extrude selected polygons.
///
/// With `individual = false`, a connected selection is extruded as a region:
/// interior walls are skipped and shared points stay welded. With
/// `individual = true`, each face is lifted along its own normal.
pub fn extrude_faces(mesh: &Mesh, faces: &[u32], distance: f32, individual: bool) -> Mesh {
    let selected: HashSet<usize> = faces
        .iter()
        .filter_map(|&f| (f as usize).lt(&mesh.polygons.len()).then_some(f as usize))
        .collect();
    if selected.is_empty() {
        return mesh.clone();
    }
    let mut out = mesh.clone();
    out.polygons.clear();
    let mut new_polys: Vec<Polygon> = Vec::with_capacity(mesh.polygons.len());
    // Extruding a face on an open mesh must leave a back cap behind; extruding
    // a face of a closed solid must not (the original would become internal).
    let boundary_edges: HashSet<(u32, u32)> = mesh
        .edges()
        .into_iter()
        .filter(|e| e.faces.len() == 1)
        .map(|e| (e.a, e.b))
        .collect();
    let mut touches_open_boundary = false;
    for &fi in &selected {
        let poly = &mesh.polygons[fi];
        let n = poly.corners.len();
        for i in 0..n {
            let a = poly.corners[i].point;
            let b = poly.corners[(i + 1) % n].point;
            let key = if a < b { (a, b) } else { (b, a) };
            if boundary_edges.contains(&key) {
                touches_open_boundary = true;
            }
        }
    }

    if individual {
        for (fi, poly) in mesh.polygons.iter().enumerate() {
            if !selected.contains(&fi) {
                new_polys.push(poly.clone());
                continue;
            }
            let normal = mesh.face_normal(fi);
            let offset = normal * distance;
            let mut top = Vec::with_capacity(poly.corners.len());
            for c in &poly.corners {
                let p = mesh.positions[c.point as usize] + offset;
                let np = out.add_point(p);
                let uv = out.add_uv(mesh.uv_of(c));
                top.push(Corner::new(np, uv));
            }
            let n = poly.corners.len();
            for i in 0..n {
                let a = poly.corners[i];
                let b = poly.corners[(i + 1) % n];
                let ta = top[i];
                let tb = top[(i + 1) % n];
                let sides = [a.point, b.point, tb.point, ta.point];
                let uvs = [
                    mesh.uv_of(&a),
                    mesh.uv_of(&b),
                    mesh.uv_of(&b),
                    mesh.uv_of(&a),
                ];
                push_polygon(&mut out, &sides, &uvs, poly.material_slot);
            }
            new_polys.push(Polygon {
                corners: top,
                material_slot: poly.material_slot,
            });
            if touches_open_boundary {
                let mut back = poly.clone();
                back.corners.reverse();
                new_polys.push(back);
            }
        }
    } else {
        // Offset points using averaged normals of the selected region.
        let mut offsets: HashMap<u32, Vec3> = HashMap::new();
        for &fi in &selected {
            let n = mesh.face_normal(fi);
            for c in &mesh.polygons[fi].corners {
                *offsets.entry(c.point).or_insert(Vec3::ZERO) += n;
            }
        }
        let mut top_points: HashMap<u32, u32> = HashMap::new();
        for (p, normal) in &offsets {
            let idx = out
                .add_point(mesh.positions[*p as usize] + normal.normalize_or(Vec3::Y) * distance);
            top_points.insert(*p, idx);
        }

        // Directed boundary of the selected region.
        let mut directed: HashSet<(u32, u32)> = HashSet::new();
        for &fi in &selected {
            let poly = &mesh.polygons[fi];
            let n = poly.corners.len();
            for i in 0..n {
                let a = poly.corners[i].point;
                let b = poly.corners[(i + 1) % n].point;
                if a != b {
                    directed.insert((a, b));
                }
            }
        }

        for (fi, poly) in mesh.polygons.iter().enumerate() {
            if !selected.contains(&fi) {
                new_polys.push(poly.clone());
                continue;
            }
            let n = poly.corners.len();
            let mut top = Vec::with_capacity(n);
            for c in &poly.corners {
                let np = top_points[&c.point];
                let uv = out.add_uv(mesh.uv_of(c));
                top.push(Corner::new(np, uv));
            }
            // Side walls only on region boundary edges.
            for i in 0..n {
                let a = poly.corners[i];
                let b = poly.corners[(i + 1) % n];
                if directed.contains(&(b.point, a.point)) || a.point == b.point {
                    continue;
                }
                let ta = top[i];
                let tb = top[(i + 1) % n];
                let uvs = [
                    mesh.uv_of(&a),
                    mesh.uv_of(&b),
                    mesh.uv_of(&b),
                    mesh.uv_of(&a),
                ];
                push_polygon(
                    &mut out,
                    &[a.point, b.point, tb.point, ta.point],
                    &uvs,
                    poly.material_slot,
                );
            }
            new_polys.push(Polygon {
                corners: top,
                material_slot: poly.material_slot,
            });
        }
        if touches_open_boundary {
            for &fi in &selected {
                let mut back = mesh.polygons[fi].clone();
                back.corners.reverse();
                new_polys.push(back);
            }
        }
    }
    // Side walls were appended to `out.polygons` while building; keep them.
    new_polys.append(&mut out.polygons);
    out.polygons = new_polys;
    out.remove_degenerate_polygons();
    out
}

/// Inset each selected face. Returns the new mesh and the indices of the
/// inset (inner) faces.
pub fn inset_faces(mesh: &Mesh, faces: &[u32], amount: f32) -> (Mesh, Vec<u32>) {
    let selected: HashSet<usize> = faces
        .iter()
        .filter_map(|&f| (f as usize).lt(&mesh.polygons.len()).then_some(f as usize))
        .collect();
    if selected.is_empty() {
        return (mesh.clone(), Vec::new());
    }
    let mut out = mesh.clone();
    out.polygons.clear();
    let mut new_polys: Vec<Polygon> = Vec::new();
    let mut inner_faces = Vec::new();
    for (fi, poly) in mesh.polygons.iter().enumerate() {
        if !selected.contains(&fi) || poly.corners.len() < 3 {
            new_polys.push(poly.clone());
            continue;
        }
        let n = poly.corners.len();
        let center = mesh.polygon_center(fi);
        let mut avg_edge = 0.0;
        for i in 0..n {
            let a = mesh.positions[poly.corners[i].point as usize];
            let b = mesh.positions[poly.corners[(i + 1) % n].point as usize];
            avg_edge += a.distance(b);
        }
        avg_edge /= n as f32;
        let fraction = if avg_edge > 1e-6 {
            (amount.abs() / avg_edge).clamp(0.0, 0.92)
        } else {
            0.0
        };
        let mut inner_corners = Vec::with_capacity(n);
        let mut inner_points = Vec::with_capacity(n);
        let avg_uv = poly.corners.iter().map(|c| mesh.uv_of(c)).sum::<Vec2>() / n as f32;
        for c in &poly.corners {
            let p = mesh.positions[c.point as usize];
            let np = out.add_point(p.lerp(center, fraction));
            let uv = out.add_uv(mesh.uv_of(c).lerp(avg_uv, fraction));
            inner_corners.push(Corner::new(np, uv));
            inner_points.push(np);
        }
        for i in 0..n {
            let a = poly.corners[i];
            let b = poly.corners[(i + 1) % n];
            let ia = inner_corners[i];
            let ib = inner_corners[(i + 1) % n];
            let uvs = [
                mesh.uv_of(&a),
                mesh.uv_of(&b),
                mesh.uv_of(&ib),
                mesh.uv_of(&ia),
            ];
            push_polygon(
                &mut out,
                &[a.point, b.point, ib.point, ia.point],
                &uvs,
                poly.material_slot,
            );
        }
        let idx = new_polys.len() as u32;
        new_polys.push(Polygon {
            corners: inner_corners,
            material_slot: poly.material_slot,
        });
        inner_faces.push(idx);
    }
    new_polys.append(&mut out.polygons);
    out.polygons = new_polys;
    (out, inner_faces)
}

/// Face bevel: inset the selected faces and raise the inset region, producing
/// chamfered borders. Works on n-gons and is stable for open meshes.
pub fn bevel_faces(mesh: &Mesh, faces: &[u32], amount: f32, depth: f32) -> Mesh {
    let (inset, inner) = inset_faces(mesh, faces, amount);
    if inner.is_empty() || depth.abs() < 1e-9 {
        return inset;
    }
    extrude_faces(&inset, &inner, depth, true)
}

/// Slice every polygon crossing an axis-aligned plane, keeping both sides.
/// Coincident cut points are shared through an edge cache.
pub fn ring_cut(mesh: &Mesh, axis: usize, offset: f32) -> Mesh {
    let mut out = mesh.clone();
    out.polygons.clear();
    let normal = match axis {
        0 => Vec3::X,
        1 => Vec3::Y,
        _ => Vec3::Z,
    };
    let mut edge_cache: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
    let mut new_polys: Vec<Polygon> = Vec::new();
    for poly in &mesh.polygons {
        let n = poly.corners.len();
        if n < 3 {
            continue;
        }
        let dists: Vec<f32> = poly
            .corners
            .iter()
            .map(|c| mesh.positions[c.point as usize].dot(normal) - offset)
            .collect();
        let all_front = dists.iter().all(|d| *d >= -1e-6);
        let all_back = dists.iter().all(|d| *d <= 1e-6);
        if all_front || all_back {
            new_polys.push(poly.clone());
            continue;
        }
        for keep_front in [true, false] {
            let mut corners: Vec<(u32, u32)> = Vec::new(); // (point, uv)
            for i in 0..n {
                let j = (i + 1) % n;
                let di = dists[i];
                let dj = dists[j];
                let inside = if keep_front { di >= 0.0 } else { di <= 0.0 };
                if inside {
                    corners.push((poly.corners[i].point, poly.corners[i].uv));
                }
                let crosses = (di > 1e-6 && dj < -1e-6) || (di < -1e-6 && dj > 1e-6);
                if crosses {
                    let a = poly.corners[i];
                    let b = poly.corners[j];
                    let key = if a.point < b.point {
                        (a.point, b.point)
                    } else {
                        (b.point, a.point)
                    };
                    let entry = edge_cache.entry(key).or_insert_with(|| {
                        let t = di / (di - dj);
                        let pa = mesh.positions[a.point as usize];
                        let pb = mesh.positions[b.point as usize];
                        let p = pa.lerp(pb, t);
                        let uv = mesh.uv_of(&a).lerp(mesh.uv_of(&b), t);
                        (out.add_point(p), out.add_uv(uv))
                    });
                    corners.push(*entry);
                }
            }
            // Remove consecutive duplicates.
            corners.dedup_by_key(|c| c.0);
            while corners.len() > 1 && corners.first().map(|c| c.0) == corners.last().map(|c| c.0) {
                corners.pop();
            }
            if corners.len() >= 3 {
                let mut new_corners = Vec::with_capacity(corners.len());
                for (p, uv) in corners {
                    new_corners.push(Corner::new(p, uv));
                }
                new_polys.push(Polygon {
                    corners: new_corners,
                    material_slot: poly.material_slot,
                });
            }
        }
    }
    out.polygons = new_polys;
    out.remove_degenerate_polygons();
    out
}

/// Bridge two boundary loops with quads. Loops must have the same length.
pub fn bridge_loops(mesh: &Mesh, loop_a: &[u32], loop_b: &[u32]) -> Result<Mesh, String> {
    if loop_a.len() != loop_b.len() || loop_a.len() < 3 {
        return Err("bridge requires two loops with the same length (>= 3)".into());
    }
    let mut out = mesh.clone();
    let n = loop_a.len();
    // Rotate loop_b so its closest point pairs with loop_a[0].
    let a0 = mesh.positions[loop_a[0] as usize];
    let start = (0..n)
        .min_by(|&i, &j| {
            let di = mesh.positions[loop_b[i] as usize].distance_squared(a0);
            let dj = mesh.positions[loop_b[j] as usize].distance_squared(a0);
            di.partial_cmp(&dj).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(0);
    let combined_center = loop_a
        .iter()
        .chain(loop_b.iter())
        .map(|&p| mesh.positions[p as usize])
        .sum::<Vec3>()
        / (n * 2) as f32;
    for i in 0..n {
        let a = loop_a[i];
        let an = loop_a[(i + 1) % n];
        let b = loop_b[(start + i) % n];
        let bn = loop_b[(start + i + 1) % n];
        let mut quad = [a, an, bn, b];
        let p0 = mesh.positions[quad[0] as usize];
        let p1 = mesh.positions[quad[1] as usize];
        let p2 = mesh.positions[quad[2] as usize];
        let p3 = mesh.positions[quad[3] as usize];
        let nrm = (p1 - p0).cross(p2 - p0);
        let center = (p0 + p1 + p2 + p3) * 0.25;
        if nrm.dot(center - combined_center) < 0.0 {
            quad.reverse();
        }
        let uvs = [
            Vec2::new(i as f32 / n as f32, 0.0),
            Vec2::new((i + 1) as f32 / n as f32, 0.0),
            Vec2::new((i + 1) as f32 / n as f32, 1.0),
            Vec2::new(i as f32 / n as f32, 1.0),
        ];
        push_polygon(&mut out, &quad, &uvs, 0);
    }
    Ok(out)
}
/// Duplicate a mesh along a translation, adding rotation per copy around Y.
pub fn array_mesh(mesh: &Mesh, count: u32, offset: Vec3, rotate_y_deg: f32) -> Mesh {
    let mut out = mesh.clone();
    let count = count.clamp(1, 256);
    let rot = Mat4::from_rotation_y(rotate_y_deg.to_radians());
    let base = mesh.clone();
    let mut accumulated_rot = Mat4::IDENTITY;
    for i in 1..count {
        accumulated_rot *= rot;
        let m = Mat4::from_translation(offset * i as f32) * accumulated_rot;
        out.merge(&base, m);
    }
    out
}

/// Mirror a mesh across an axis and weld the seam.
pub fn symmetry_mesh(mesh: &Mesh, axis: usize, threshold: f32) -> Mesh {
    let mut out = mesh.clone();
    let mirror = match axis {
        0 => Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)),
        1 => Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0)),
        _ => Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0)),
    };
    let mut mirrored = mesh.clone();
    mirrored.transform(mirror);
    mirrored.flip_normals();
    out.merge(&mirrored, Mat4::IDENTITY);
    out.weld(threshold.max(1e-6));
    out
}

/// Bend around Z, curving the Y axis. `angle_deg` is the total bend.
pub fn bend(mesh: &Mesh, angle_deg: f32, center: Vec3) -> Mesh {
    let mut out = mesh.clone();
    let bounds = mesh.bounds();
    if !bounds.is_valid() {
        return out;
    }
    let height = bounds.size().y.max(1e-6);
    let angle = angle_deg.to_radians();
    if angle.abs() < 1e-6 {
        return out;
    }
    let radius = height / angle;
    for p in &mut out.positions {
        let s = (p.y - bounds.min.y) / height;
        let a = s * angle;
        let px = p.x - center.x;
        let cx = center.x - radius;
        let new_x = cx + (radius + px) * a.cos();
        let new_y = bounds.min.y + (radius + px) * a.sin();
        *p = Vec3::new(new_x, new_y, p.z);
    }
    out
}

/// Taper perpendicular to the chosen axis. `amount` scales the end by 1+amount.
pub fn taper(mesh: &Mesh, axis: usize, amount: f32, center: Vec3) -> Mesh {
    let mut out = mesh.clone();
    let bounds = mesh.bounds();
    if !bounds.is_valid() {
        return out;
    }
    let size = bounds.size();
    let extent = size[axis].max(1e-6);
    for p in &mut out.positions {
        let t = ((p[axis] - bounds.min[axis]) / extent).clamp(0.0, 1.0);
        let factor = 1.0 + amount * t;
        for k in 0..3 {
            if k != axis {
                p[k] = center[k] + (p[k] - center[k]) * factor;
            }
        }
    }
    out
}

/// Revolve a profile (x = radius, y = height) around Y.
pub fn lathe(profile: &[Vec2], segments: u32, angle_deg: f32, caps: bool) -> Mesh {
    let mut profile = profile.to_vec();
    if profile.len() < 2 {
        return Mesh::new("Lathe");
    }
    // Order bottom-to-top so surface winding is outward.
    if profile.last().unwrap().y < profile.first().unwrap().y {
        profile.reverse();
    }
    let segments = segments.clamp(3, 512);
    let full = (angle_deg.abs() - 360.0).abs() < 1e-3;
    let sweep = angle_deg.to_radians();
    let cols = segments + 1;
    let mut m = Mesh::new("Lathe");
    let mut grid: Vec<Vec<u32>> = Vec::new();
    for (pi, p) in profile.iter().enumerate() {
        let mut row = Vec::new();
        for s in 0..cols {
            let t = s as f32 / segments as f32;
            let a = sweep * t;
            let pos = Vec3::new(p.x * a.cos(), p.y, -p.x * a.sin());
            row.push(m.add_point(pos));
            let _ = pi;
        }
        grid.push(row);
    }
    for i in 0..profile.len() - 1 {
        for s in 0..segments as usize {
            let a = grid[i][s];
            let b = grid[i + 1][s];
            let c = grid[i + 1][s + 1];
            let d = grid[i][s + 1];
            let quad = [a, d, c, b];
            let uvs = [
                Vec2::new(s as f32 / segments as f32, i as f32 / profile.len() as f32),
                Vec2::new(
                    (s + 1) as f32 / segments as f32,
                    i as f32 / profile.len() as f32,
                ),
                Vec2::new(
                    (s + 1) as f32 / segments as f32,
                    (i + 1) as f32 / profile.len() as f32,
                ),
                Vec2::new(
                    s as f32 / segments as f32,
                    (i + 1) as f32 / profile.len() as f32,
                ),
            ];
            push_polygon(&mut m, &quad, &uvs, 0);
        }
    }
    if caps {
        for idx in [0usize, profile.len() - 1] {
            let r = profile[idx].x;
            if r.abs() < 1e-5 {
                continue;
            }
            let ring: Vec<u32> = (0..segments as usize).map(|s| grid[idx][s]).collect();
            let mut cap = ring;
            // Top caps face +Y, bottom caps face -Y.
            let up = idx == profile.len() - 1;
            if !up {
                cap.reverse();
            }
            let uvs: Vec<Vec2> = (0..cap.len())
                .map(|s| {
                    let a = std::f32::consts::TAU * s as f32 / cap.len() as f32;
                    Vec2::new(0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin())
                })
                .collect();
            push_polygon(&mut m, &cap, &uvs, 0);
        }
    }
    if full {
        m.weld(1e-5);
    }
    m
}

/// Sweep a 2D profile along a sampled path using parallel transport frames.
pub fn sweep(profile: &[Vec2], path: &[Vec3], closed_path: bool, caps: bool) -> Mesh {
    let mut m = Mesh::new("Sweep");
    if profile.len() < 3 || path.len() < 2 {
        return m;
    }
    let mut tangents = Vec::with_capacity(path.len());
    for i in 0..path.len() {
        let prev = if i == 0 {
            if closed_path {
                path[path.len() - 1]
            } else {
                path[0]
            }
        } else {
            path[i - 1]
        };
        let next = if i + 1 == path.len() {
            if closed_path {
                path[0]
            } else {
                path[path.len() - 1]
            }
        } else {
            path[i + 1]
        };
        tangents.push((next - prev).normalize_or(Vec3::Z));
    }
    let mut normal = lc_core::orthonormal_basis(tangents[0]).0;
    let mut frames = Vec::with_capacity(path.len());
    for (i, tangent) in tangents.iter().enumerate() {
        if i > 0 {
            let q = Quat::from_rotation_arc(tangents[i - 1], *tangent);
            normal = q * normal;
            normal = (normal - *tangent * normal.dot(*tangent))
                .normalize_or(lc_core::orthonormal_basis(*tangent).0);
        }
        let binormal = tangent.cross(normal).normalize_or(Vec3::Y);
        frames.push((normal, binormal));
    }
    let mut rings: Vec<Vec<u32>> = Vec::new();
    for (i, p) in path.iter().enumerate() {
        let (n, b) = frames[i];
        let mut ring = Vec::new();
        for v in profile {
            ring.push(m.add_point(*p + n * v.x + b * v.y));
        }
        rings.push(ring);
    }
    let ring_count = if closed_path {
        path.len()
    } else {
        path.len() - 1
    };
    for i in 0..ring_count {
        let i0 = i;
        let i1 = (i + 1) % path.len();
        for j in 0..profile.len() {
            let j0 = j;
            let j1 = (j + 1) % profile.len();
            let a = rings[i0][j0];
            let b = rings[i0][j1];
            let c = rings[i1][j1];
            let d = rings[i1][j0];
            let quad = [a, b, c, d];
            let uvs = [
                Vec2::new(
                    j as f32 / profile.len() as f32,
                    i as f32 / ring_count.max(1) as f32,
                ),
                Vec2::new(
                    (j + 1) as f32 / profile.len() as f32,
                    i as f32 / ring_count.max(1) as f32,
                ),
                Vec2::new(
                    (j + 1) as f32 / profile.len() as f32,
                    (i + 1) as f32 / ring_count.max(1) as f32,
                ),
                Vec2::new(
                    j as f32 / profile.len() as f32,
                    (i + 1) as f32 / ring_count.max(1) as f32,
                ),
            ];
            push_polygon(&mut m, &quad, &uvs, 0);
        }
    }
    if caps && !closed_path {
        for (ring_idx, last) in [(0usize, false), (path.len() - 1, true)] {
            let ring: Vec<u32> = rings[ring_idx].clone();
            let mut cap = ring;
            if !last {
                cap.reverse();
            }
            let uvs: Vec<Vec2> = (0..cap.len())
                .map(|j| {
                    let a = std::f32::consts::TAU * j as f32 / cap.len() as f32;
                    Vec2::new(0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin())
                })
                .collect();
            push_polygon(&mut m, &cap, &uvs, 0);
        }
    }
    m
}

/// Extrude a closed 2D profile along +Z.
pub fn extrude_profile(profile: &[Vec2], depth: f32) -> Mesh {
    let mut m = Mesh::new("Extrude");
    if profile.len() < 3 {
        return m;
    }
    let mut pts = profile.to_vec();
    let area2: f32 = (0..pts.len())
        .map(|i| {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            a.x * b.y - b.x * a.y
        })
        .sum();
    if area2 < 0.0 {
        pts.reverse();
    }
    let ids: Vec<u32> = pts
        .iter()
        .map(|p| m.add_point(Vec3::new(p.x, p.y, 0.0)))
        .collect();
    m.add_face(&ids);
    // Front face winding may be -Z depending on projection; pick +Z depth.
    extrude_faces(&m, &[0], depth.abs().max(1e-5), false)
}

/// Sample a spline and sweep a profile along it.
pub fn sweep_spline(profile: &[Vec2], spline: &Spline, caps: bool) -> Mesh {
    sweep(profile, &spline.sample(), spline.closed, caps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{primitives, uv};

    #[test]
    fn extrude_region_closes_a_plane() {
        let plane = primitives::plane_mesh(2.0, 2.0, 1, 1);
        let out = extrude_faces(&plane, &[0], 1.0, false);
        assert_eq!(out.polygons.len(), 6);
        assert!(out.validate().is_ok());
        let vol = out.signed_volume();
        assert!((vol - 4.0).abs() < 0.1, "volume {vol}");
    }

    #[test]
    fn extrude_individual_and_inset_are_valid() {
        let cube = primitives::box_mesh(1.0, 1.0, 1.0);
        let out = extrude_faces(&cube, &[0, 1], 0.3, true);
        assert!(out.validate().is_ok());
        let (inset, inner) = inset_faces(&cube, &[0, 1], 0.1);
        assert_eq!(inner.len(), 2);
        assert!(inset.validate().is_ok());
        let bevel = bevel_faces(&cube, &[0], 0.1, 0.05);
        assert!(bevel.validate().is_ok());
    }

    #[test]
    fn ring_cut_splits_cube_quads() {
        let cube = primitives::box_mesh(2.0, 2.0, 2.0);
        let cut = ring_cut(&cube, 1, 0.0);
        assert!(cut.polygons.len() > cube.polygons.len());
        assert!(cut.validate().is_ok());
        assert!(cut.signed_volume() > 0.0);
    }

    #[test]
    fn bridge_two_planes() {
        let a = primitives::plane_mesh(1.0, 1.0, 1, 1);
        let mut b = primitives::plane_mesh(1.0, 1.0, 1, 1);
        b.transform(Mat4::from_translation(Vec3::new(0.0, 2.0, 0.0)));
        let mut combined = a.clone();
        combined.merge(&b, Mat4::IDENTITY);
        let loops = combined.boundary_loops();
        assert_eq!(loops.len(), 2);
        let out = bridge_loops(&combined, &loops[0], &loops[1]).unwrap();
        assert!(out.validate().is_ok());
        assert_eq!(out.polygons.len(), 6);
    }

    #[test]
    fn deformers_stay_finite() {
        let cube = primitives::box_mesh(1.0, 2.0, 1.0);
        for out in [
            bend(&cube, 45.0, Vec3::ZERO),
            taper(&cube, 1, 0.5, Vec3::ZERO),
            array_mesh(&cube, 3, Vec3::new(1.5, 0.0, 0.0), 15.0),
            symmetry_mesh(&cube, 0, 1e-4),
        ] {
            assert!(out.validate().is_ok());
            assert!(out.positions.iter().all(|p| p.is_finite()));
        }
    }

    #[test]
    fn lathe_and_sweep_build_geometry() {
        let profile = vec![
            Vec2::new(0.0, -1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ];
        let solid = lathe(&profile, 24, 360.0, true);
        assert!(solid.validate().is_ok());
        assert!(solid.signed_volume() > 0.0);
        let path = Spline::new(
            crate::SplineKind::Bezier,
            vec![
                Vec3::ZERO,
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 2.0, 0.0),
            ],
            false,
        );
        let tube = sweep_spline(&primitives::disc_profile(0.2, 12), &path, true);
        assert!(tube.validate().is_ok());
        let ext = extrude_profile(&profile, 1.0);
        assert!(ext.validate().is_ok());
        assert!(ext.signed_volume() > 0.0);
    }

    #[test]
    fn extrude_preserves_uvs() {
        let mut plane = primitives::plane_mesh(1.0, 1.0, 1, 1);
        uv::planar_projection(&mut plane, 1, 1.0);
        let out = extrude_faces(&plane, &[0], 0.5, false);
        assert!(!out.uvs.is_empty());
        assert!(out.validate().is_ok());
    }
}
