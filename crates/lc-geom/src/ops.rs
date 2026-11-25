//! Direct-modelling operations: extrude, inset/bevel, ring cut, bridge,
//! mirror/array and simple deformers, plus lathe/sweep creators.

use crate::{Corner, Mesh, Polygon, Spline};
use glam::{Mat4, Quat, Vec2, Vec3};
use std::collections::{BTreeMap, HashMap, HashSet};

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

/// Whether every corner of `face` references a point that exists. Operations
/// that build derived point sets ignore a face they cannot read instead of
/// indexing past the position list with the face's corner points.
fn readable_face(mesh: &Mesh, face: usize) -> bool {
    mesh.polygons.get(face).is_some_and(|poly| {
        poly.corners
            .iter()
            .all(|corner| (corner.point as usize) < mesh.positions.len())
    })
}

/// Repair non-finite point positions exactly as document loading does, so the
/// Mesh-returning operations below cannot multiply NaN or infinity into new
/// geometry. Returns `None` when every position is already finite.
fn repaired_positions(mesh: &Mesh) -> Option<Mesh> {
    if mesh.positions.iter().all(|position| position.is_finite()) {
        return None;
    }
    let mut repaired = mesh.clone();
    for position in &mut repaired.positions {
        if !position.is_finite() {
            *position = Vec3::ZERO;
        }
    }
    Some(repaired)
}

/// Copy the selected polygons into a standalone mesh.
///
/// Cheetah3D's Split tool "creates a completely new polygon object with the
/// content of the current polygon selection" (source 39) and leaves the source
/// mesh untouched; its documented workflow deletes the copied faces from the
/// original afterwards. Only the points, UV slots, creases, seams and pins the
/// copied faces actually use travel with them.
pub fn extract_faces(mesh: &Mesh, faces: &[u32]) -> Result<Mesh, String> {
    let mut selected: Vec<usize> = Vec::with_capacity(faces.len());
    for &face in faces {
        let index = face as usize;
        if index >= mesh.polygons.len() {
            return Err(format!("polygon {face} does not exist"));
        }
        if !selected.contains(&index) {
            selected.push(index);
        }
    }
    if selected.is_empty() {
        return Err("select at least one polygon to split".into());
    }

    let mut out = Mesh {
        name: mesh.name.clone(),
        ..Mesh::default()
    };
    let has_uvs = !mesh.uvs.is_empty();
    let mut point_map: HashMap<u32, u32> = HashMap::new();
    let mut uv_map: HashMap<u32, u32> = HashMap::new();
    for &index in &selected {
        let poly = &mesh.polygons[index];
        let mut corners = Vec::with_capacity(poly.corners.len());
        for corner in &poly.corners {
            let point = match point_map.get(&corner.point) {
                Some(&mapped) => mapped,
                None => {
                    let Some(position) = mesh.positions.get(corner.point as usize) else {
                        return Err(format!(
                            "polygon {index} references missing point {}",
                            corner.point
                        ));
                    };
                    let mapped = out.positions.len() as u32;
                    out.positions.push(*position);
                    point_map.insert(corner.point, mapped);
                    mapped
                }
            };
            let uv = if has_uvs {
                match uv_map.get(&corner.uv) {
                    Some(&mapped) => mapped,
                    None => {
                        let mapped = out.uvs.len() as u32;
                        out.uvs.push(mesh.uv_of(corner));
                        uv_map.insert(corner.uv, mapped);
                        mapped
                    }
                }
            } else {
                0
            };
            corners.push(Corner::new(point, uv));
        }
        out.polygons.push(Polygon {
            corners,
            material_slot: poly.material_slot,
        });
    }

    // Every additional UV set follows the same slot mapping as UV0.
    for set in 1..=mesh.extra_uvs.len() {
        let mut values = vec![Vec2::ZERO; out.uvs.len()];
        for (old, new) in &uv_map {
            if let Some(uv) = mesh.uv_value(set, *old) {
                values[*new as usize] = uv;
            }
        }
        out.extra_uvs.push(values);
    }

    // Marks only follow when both endpoints (or the UV slot) are copied; a
    // mark that reached outside the selection has nothing to belong to.
    let carry = |list: &[[u32; 2]], map: &HashMap<u32, u32>| -> Vec<[u32; 2]> {
        list.iter()
            .filter_map(|edge| {
                let a = *map.get(&edge[0])?;
                let b = *map.get(&edge[1])?;
                Some(if a < b { [a, b] } else { [b, a] })
            })
            .collect()
    };
    out.creases = carry(&mesh.creases, &point_map);
    out.crease_weights = Box::new(
        mesh.crease_weights
            .iter()
            .filter_map(|(edge, weight)| {
                let a = *point_map.get(&edge[0])?;
                let b = *point_map.get(&edge[1])?;
                Some((if a < b { [a, b] } else { [b, a] }, *weight))
            })
            .collect::<Vec<_>>(),
    );
    out.seams = carry(&mesh.seams, &point_map);
    out.pinned_uvs = mesh
        .pinned_uvs
        .iter()
        .filter_map(|slot| uv_map.get(slot).copied())
        .collect();
    out.normalize_creases();
    out.normalize_seams();
    out.normalize_pinned_uvs();
    Ok(out)
}

/// Region a Shell gesture would cover: the selected polygons plus every
/// neighbour reachable across an edge whose faces deviate by at most `angle`
/// degrees, matching the documented "Angle" property (source 41). Angles
/// outside 0-180 behave as the nearest limit.
pub fn shell_region(mesh: &Mesh, faces: &[u32], angle: f32) -> Vec<u32> {
    let limit = angle.clamp(0.0, 180.0).to_radians().cos() - 1e-6;
    let mut region: Vec<u32> = faces
        .iter()
        .copied()
        .filter(|f| (*f as usize) < mesh.polygons.len())
        .collect();
    region.sort_unstable();
    region.dedup();
    if region.is_empty() {
        return region;
    }

    let mut adjacency: HashMap<u32, Vec<u32>> = HashMap::new();
    for edge in mesh.edges() {
        if edge.faces.len() == 2 {
            adjacency
                .entry(edge.faces[0])
                .or_default()
                .push(edge.faces[1]);
            adjacency
                .entry(edge.faces[1])
                .or_default()
                .push(edge.faces[0]);
        }
    }

    let mut in_region: HashSet<u32> = region.iter().copied().collect();
    let mut cursor = 0;
    while cursor < region.len() {
        let face = region[cursor];
        cursor += 1;
        let normal = mesh.face_normal(face as usize);
        let neighbours: &[u32] = adjacency.get(&face).map(Vec::as_slice).unwrap_or(&[]);
        for &other in neighbours {
            if in_region.contains(&other) {
                continue;
            }
            if normal.dot(mesh.face_normal(other as usize)) >= limit {
                in_region.insert(other);
                region.push(other);
            }
        }
    }
    region.sort_unstable();
    region
}

/// Build a wall of `thickness` behind or in front of a polygon region.
///
/// `inside` follows the documented "Inside" property (source 41): the copy is
/// moved against the normal direction when set, along it otherwise. The copy is
/// always wound the opposite way to the original so the pair and the rim quads
/// form one closed shell; `quantize` snaps the distance to a step, as the
/// documented "Quantize" property describes.
pub fn shell_faces(
    mesh: &Mesh,
    faces: &[u32],
    thickness: f32,
    inside: bool,
    angle: f32,
    quantize: f32,
) -> Result<Mesh, String> {
    if !thickness.is_finite() || !angle.is_finite() || !quantize.is_finite() {
        return Err("shell settings must be finite".into());
    }
    // Thickness is a magnitude; the documented Inside property chooses the
    // side, so a signed distance is rejected rather than cancelling it out.
    if thickness < 0.0 {
        return Err("shell thickness must not be negative; use Inside for the other side".into());
    }
    let step = quantize.max(0.0);
    let mut distance = thickness;
    if step > 1e-6 {
        distance = (distance / step).round() * step;
    }
    if distance.abs() < 1e-6 {
        return Err("shell thickness must not be zero".into());
    }

    let region = shell_region(mesh, faces, angle);
    if region.is_empty() {
        return Err("select at least one polygon to shell".into());
    }
    let sign = if inside { -1.0 } else { 1.0 };

    let mut point_normals = vec![Vec3::ZERO; mesh.positions.len()];
    for &face in &region {
        let normal = mesh.face_normal(face as usize);
        for corner in &mesh.polygons[face as usize].corners {
            if let Some(acc) = point_normals.get_mut(corner.point as usize) {
                *acc += normal;
            }
        }
    }

    let mut out = mesh.clone();
    let mut shell_point: Vec<Option<u32>> = vec![None; mesh.positions.len()];
    for &face in &region {
        for corner in &mesh.polygons[face as usize].corners {
            let index = corner.point as usize;
            // Validate the caller's corner reference before touching the
            // per-point map, which is sized from the same position list.
            let Some(position) = mesh.positions.get(index) else {
                return Err(format!(
                    "polygon {face} references missing point {}",
                    corner.point
                ));
            };
            let Some(slot) = shell_point.get_mut(index) else {
                return Err(format!(
                    "polygon {face} references missing point {}",
                    corner.point
                ));
            };
            if slot.is_some() {
                continue;
            }
            let direction = point_normals
                .get(index)
                .copied()
                .unwrap_or(Vec3::ZERO)
                .try_normalize()
                .unwrap_or(Vec3::Y);
            let shelled = out.positions.len() as u32;
            out.positions.push(*position + direction * distance * sign);
            *slot = Some(shelled);
        }
    }

    for &face in &region {
        let poly = &mesh.polygons[face as usize];
        let mut corners = Vec::with_capacity(poly.corners.len());
        for corner in poly.corners.iter().rev() {
            let mapped = shell_point
                .get(corner.point as usize)
                .copied()
                .flatten()
                .ok_or_else(|| {
                    format!("polygon {face} references missing point {}", corner.point)
                })?;
            corners.push(Corner::new(mapped, corner.uv));
        }
        out.polygons.push(Polygon {
            corners,
            material_slot: poly.material_slot,
        });
    }

    // Every border of the region gets a rim quad joining the two surfaces.
    let in_region: HashSet<u32> = region.iter().copied().collect();
    for edge in mesh.edges() {
        let touching: Vec<u32> = edge
            .faces
            .iter()
            .copied()
            .filter(|f| in_region.contains(f))
            .collect();
        if touching.len() != 1 {
            continue;
        }
        let face = touching[0] as usize;
        let poly = &mesh.polygons[face];
        let count = poly.corners.len();
        let mut rim = None;
        for i in 0..count {
            let start = poly.corners[i];
            let end = poly.corners[(i + 1) % count];
            let matches = (start.point == edge.a && end.point == edge.b)
                || (start.point == edge.b && end.point == edge.a);
            if !matches {
                continue;
            }
            let (Some(start_shell), Some(end_shell)) = (
                shell_point.get(start.point as usize).copied().flatten(),
                shell_point.get(end.point as usize).copied().flatten(),
            ) else {
                break;
            };
            rim = Some(vec![
                Corner::new(end.point, end.uv),
                Corner::new(start.point, start.uv),
                Corner::new(start_shell, start.uv),
                Corner::new(end_shell, end.uv),
            ]);
            break;
        }
        if let Some(corners) = rim {
            out.polygons.push(Polygon {
                corners,
                material_slot: poly.material_slot,
            });
        }
    }

    if let Err(errors) = out.validate() {
        return Err(format!(
            "shell produced invalid geometry: {}",
            errors.join("; ")
        ));
    }
    Ok(out)
}

/// Extrude selected polygons.
///
/// With `individual = false`, a connected selection is extruded as a region:
/// interior walls are skipped and shared points stay welded. With
/// `individual = true`, each face is lifted along its own normal.
pub fn extrude_faces(mesh: &Mesh, faces: &[u32], distance: f32, individual: bool) -> Mesh {
    // A mesh can reach memory without the load-time repair (for example a
    // primitive built from a dimension that deserialized to infinity), so
    // repair its positions before deriving new points from them.
    let repaired = repaired_positions(mesh);
    let mesh = repaired.as_ref().unwrap_or(mesh);
    let selected: HashSet<usize> = faces
        .iter()
        .map(|&f| f as usize)
        .filter(|&f| readable_face(mesh, f))
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
        // Ordered so the new point indices (and the document signature they
        // feed) do not depend on HashMap iteration order.
        let mut offsets: BTreeMap<u32, Vec3> = BTreeMap::new();
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
    // Same entry repair as extrude: this helper returns a Mesh and has no error
    // channel, but it must not convert non-finite input points into NaN output.
    let repaired = repaired_positions(mesh);
    let mesh = repaired.as_ref().unwrap_or(mesh);
    let selected: HashSet<usize> = faces
        .iter()
        .map(|&f| f as usize)
        .filter(|&f| readable_face(mesh, f))
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

/// Where one Insert Point click lands on a polygon.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InsertPointPick {
    /// A local-space position inside the polygon.
    Inside(Vec3),
    /// A position on one of the polygon's edges, `t` from 0 at `a` to 1 at `b`.
    Edge { a: u32, b: u32, t: f32 },
}

/// Insert one point into a polygon, the way Cheetah3D's Insert Point tool does
/// (source 49).
///
/// An `Inside` pick adds a point at the given local-space position and replaces
/// the polygon with one triangle per corner, all sharing the new point, so the
/// surface is unchanged. Every original corner keeps its UV slot, the face keeps
/// its material slot, and the new point's UV is the average of the face's corner
/// UVs: the manual does not document a UV rule, so that is the clean-room choice.
/// A position whose fan would contain a zero-area triangle (a corner, or a
/// position off the face's plane) is refused instead of producing degenerate
/// polygons.
///
/// An `Edge` pick inserts the point on that edge at `t`, splits the edge in
/// every polygon that uses it so the mesh stays watertight, and moves a seam or
/// crease weight onto both halves, exactly like a Scalpel edge pick. `t` must be
/// strictly between 0 and 1 because a corner already exists there.
///
/// Returns the new point's index in the rebuilt mesh.
pub fn insert_point(mesh: &Mesh, face: u32, pick: InsertPointPick) -> Result<(Mesh, u32), String> {
    let Some(poly) = mesh.polygons.get(face as usize) else {
        return Err(format!("face {face} does not exist"));
    };
    let count = poly.corners.len();
    if count < 3 {
        return Err(format!("face {face} has fewer than three corners"));
    }
    let mut out = mesh.clone();
    match pick {
        InsertPointPick::Inside(position) => {
            if !position.is_finite() {
                return Err("the inserted point must be a finite position".into());
            }
            let uv = poly
                .corners
                .iter()
                .map(|corner| out.uv_of(corner))
                .sum::<Vec2>()
                / count as f32;
            let point = out.add_point(position);
            let slot = out.add_uv(uv);
            let mut triangles = Vec::with_capacity(count);
            for index in 0..count {
                let a = poly.corners[index];
                let b = poly.corners[(index + 1) % count];
                let (Some(pa), Some(pb)) = (
                    mesh.positions.get(a.point as usize),
                    mesh.positions.get(b.point as usize),
                ) else {
                    return Err(format!("face {face} references a missing point"));
                };
                if (pa - position).cross(pb - position).length() <= 1e-9 {
                    return Err("the inserted point would produce a degenerate triangle".into());
                }
                triangles.push(Polygon {
                    corners: vec![a, b, Corner::new(point, slot)],
                    material_slot: poly.material_slot,
                });
            }
            out.polygons
                .splice(face as usize..=face as usize, triangles);
            Ok((out, point))
        }
        InsertPointPick::Edge { a, b, t } => {
            if a == b {
                return Err("an inserted edge point needs two different points".into());
            }
            if !t.is_finite() || t <= 0.0 || t >= 1.0 {
                return Err("an inserted edge point must sit between the edge ends".into());
            }
            let found = (0..count).find_map(|index| {
                let start = poly.corners[index].point;
                let end = poly.corners[(index + 1) % count].point;
                if start == a && end == b {
                    Some((index, t))
                } else if start == b && end == a {
                    Some((index, 1.0 - t))
                } else {
                    None
                }
            });
            let Some((index, t)) = found else {
                return Err(format!("points {a} and {b} are not an edge of face {face}"));
            };
            let start = poly.corners[index];
            let end = poly.corners[(index + 1) % count];
            let (Some(start_position), Some(end_position)) = (
                out.positions.get(start.point as usize).copied(),
                out.positions.get(end.point as usize).copied(),
            ) else {
                return Err(format!("face {face} references a missing point"));
            };
            let position = start_position.lerp(end_position, t);
            let uv = out.uv_of(&start).lerp(out.uv_of(&end), t);
            let point = out.add_point(position);
            let slot = out.add_uv(uv);
            out.polygons[face as usize]
                .corners
                .insert(index + 1, Corner::new(point, slot));
            scalpel_split_shared_edge(&mut out, start.point, end.point, point, t);
            scalpel_split_marks(&mut out, start.point, end.point, point);
            Ok((out, point))
        }
    }
}

/// Where one end of a Scalpel cut lands on the polygon being cut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScalpelPick {
    /// A point the polygon already uses.
    Point(u32),
    /// A position on one of the polygon's edges, `t` from 0 at `a` to 1 at `b`.
    Edge { a: u32, b: u32, t: f32 },
}

/// A pick resolved against the polygon's corner ring.
#[derive(Clone, Copy, Debug)]
enum ScalpelSide {
    Corner(usize),
    Edge(usize, f32),
}

fn resolve_scalpel_pick(
    poly: &Polygon,
    face: u32,
    pick: ScalpelPick,
) -> Result<ScalpelSide, String> {
    let count = poly.corners.len();
    match pick {
        ScalpelPick::Point(point) => poly
            .corners
            .iter()
            .position(|corner| corner.point == point)
            .map(ScalpelSide::Corner)
            .ok_or_else(|| format!("point {point} is not a corner of face {face}")),
        ScalpelPick::Edge { a, b, t } => {
            if a == b {
                return Err("a scalpel edge needs two different points".into());
            }
            if !t.is_finite() || !(0.0..=1.0).contains(&t) {
                return Err("a scalpel edge position must be between 0 and 1".into());
            }
            let found = (0..count).find_map(|index| {
                let start = poly.corners[index].point;
                let end = poly.corners[(index + 1) % count].point;
                if start == a && end == b {
                    Some((index, t))
                } else if start == b && end == a {
                    Some((index, 1.0 - t))
                } else {
                    None
                }
            });
            let Some((index, t)) = found else {
                return Err(format!("points {a} and {b} are not an edge of face {face}"));
            };
            if t <= 0.0 {
                Ok(ScalpelSide::Corner(index))
            } else if t >= 1.0 {
                Ok(ScalpelSide::Corner((index + 1) % count))
            } else {
                Ok(ScalpelSide::Edge(index, t))
            }
        }
    }
}

/// Position of a resolved pick along the corner ring, used to order the two
/// ends of the cut so the ring walk is independent of click order.
fn scalpel_position(side: &ScalpelSide) -> f32 {
    match side {
        ScalpelSide::Corner(index) => *index as f32,
        ScalpelSide::Edge(index, t) => *index as f32 + *t,
    }
}

/// The corner record for one cut end, creating the point and UV slot when the
/// pick sits inside an edge.
fn scalpel_corner(
    out: &mut Mesh,
    original: &Polygon,
    face: u32,
    side: ScalpelSide,
) -> Result<Corner, String> {
    let (index, t) = match side {
        ScalpelSide::Corner(index) => return Ok(original.corners[index]),
        ScalpelSide::Edge(index, t) => (index, t),
    };
    let count = original.corners.len();
    let start = original.corners[index];
    let end = original.corners[(index + 1) % count];
    let (Some(start_position), Some(end_position)) = (
        out.positions.get(start.point as usize).copied(),
        out.positions.get(end.point as usize).copied(),
    ) else {
        return Err(format!("face {face} references a missing point"));
    };
    let position = start_position.lerp(end_position, t);
    let uv = out.uv_of(&start).lerp(out.uv_of(&end), t);
    let point = out.add_point(position);
    let slot = out.add_uv(uv);
    scalpel_split_shared_edge(out, start.point, end.point, point, t);
    scalpel_split_marks(out, start.point, end.point, point);
    Ok(Corner::new(point, slot))
}

/// Insert a new point into every other polygon that uses the split edge, so the
/// shared edge carries the same point on both sides and the mesh stays
/// watertight.
fn scalpel_split_shared_edge(out: &mut Mesh, a: u32, b: u32, point: u32, t: f32) {
    let mut inserts = Vec::new();
    for (polygon, poly) in out.polygons.iter().enumerate() {
        let count = poly.corners.len();
        for index in 0..count {
            let start = poly.corners[index];
            let end = poly.corners[(index + 1) % count];
            if start.point == b && end.point == a {
                inserts.push((polygon, index, start, end));
            }
        }
    }
    // Later corners first, so an earlier insert cannot shift a pending index.
    inserts.sort_by(|left, right| left.0.cmp(&right.0).then(right.1.cmp(&left.1)));
    for (polygon, index, start, end) in inserts {
        // `t` runs from a to b, so the neighbour meets the point at 1 - t.
        let uv = out.uv_of(&start).lerp(out.uv_of(&end), 1.0 - t);
        let slot = out.add_uv(uv);
        out.polygons[polygon]
            .corners
            .insert(index + 1, Corner::new(point, slot));
    }
}

/// Move an edge's seam and crease weight onto the two halves the new point
/// creates, so a cut through a marked edge keeps the mark on both sides.
fn scalpel_split_marks(out: &mut Mesh, a: u32, b: u32, point: u32) {
    let crease = out.crease_weight(a, b);
    if crease > 0.0 {
        out.set_crease_weight(a, b, 0.0);
        out.set_crease_weight(a, point, crease);
        out.set_crease_weight(point, b, crease);
    }
    if out.is_seam(a, b) {
        out.set_seam(a, b, false);
        out.set_seam(a, point, true);
        out.set_seam(point, b, true);
    }
}

/// Cut one polygon between two picks, replacing it with the two faces the cut
/// leaves behind.
///
/// Cheetah3D's Scalpel documents clicking an edge or point and finalizing on a
/// second edge or point of the same polygon (source 43), so one cut stays inside
/// one polygon and ends on its boundary. A pick inside an edge inserts a point
/// there and splits the edge for the neighbouring face too, so the mesh stays
/// watertight, and a seam or crease weight on that edge follows both halves.
/// Corner UVs and the material slot are kept and the new cut edge carries no
/// seam or crease. One call makes one cut between two boundary picks; the
/// editor's click tool chains calls by feeding a cut's end point back in as the
/// next start (source 43).
pub fn scalpel_face(
    mesh: &Mesh,
    face: u32,
    from: ScalpelPick,
    to: ScalpelPick,
) -> Result<Mesh, String> {
    scalpel_face_ends(mesh, face, from, to).map(|(mesh, _, _)| mesh)
}

/// Cut one polygon like [`scalpel_face`] and also report the points the cut
/// ends on, `from` first and `to` second. The click tool needs those indices to
/// chain a cut's end point into the next cut without guessing it from
/// positions.
pub fn scalpel_face_ends(
    mesh: &Mesh,
    face: u32,
    from: ScalpelPick,
    to: ScalpelPick,
) -> Result<(Mesh, u32, u32), String> {
    let Some(poly) = mesh.polygons.get(face as usize) else {
        return Err(format!("face {face} does not exist"));
    };
    let count = poly.corners.len();
    if count < 3 {
        return Err(format!("face {face} has fewer than three corners"));
    }
    let from = resolve_scalpel_pick(poly, face, from)?;
    let to = resolve_scalpel_pick(poly, face, to)?;
    let from_position = scalpel_position(&from);
    let to_position = scalpel_position(&to);
    if (from_position - to_position).abs() <= 1e-6 {
        return Err("the scalpel needs two different positions on the polygon".into());
    }
    let (first, second) = if from_position < to_position {
        (from, to)
    } else {
        (to, from)
    };

    let mut out = mesh.clone();
    let first_corner = scalpel_corner(&mut out, poly, face, first)?;
    let second_corner = scalpel_corner(&mut out, poly, face, second)?;

    // Rebuild the corner ring with each cut end in its place, then split the
    // ring between the two ends into the faces on either side of the cut.
    let mut ring: Vec<Corner> = Vec::with_capacity(count + 2);
    let mut endpoint = [0usize; 2];
    for (index, corner) in poly.corners.iter().enumerate() {
        ring.push(*corner);
        for (which, side) in [first, second].into_iter().enumerate() {
            let corner = if which == 0 {
                first_corner
            } else {
                second_corner
            };
            match side {
                ScalpelSide::Corner(base) if base == index => endpoint[which] = ring.len() - 1,
                ScalpelSide::Edge(base, _) if base == index => {
                    ring.push(corner);
                    endpoint[which] = ring.len() - 1;
                }
                _ => {}
            }
        }
    }
    let low = endpoint[0].min(endpoint[1]);
    let high = endpoint[0].max(endpoint[1]);
    let first_face: Vec<Corner> = ring[low..=high].to_vec();
    let mut second_face: Vec<Corner> = ring[high..].to_vec();
    second_face.extend_from_slice(&ring[..=low]);
    if first_face.len() < 3 || second_face.len() < 3 {
        return Err("the cut would leave a polygon with fewer than three corners".into());
    }
    let material_slot = poly.material_slot;
    out.polygons[face as usize] = Polygon {
        corners: first_face,
        material_slot,
    };
    out.polygons.push(Polygon {
        corners: second_face,
        material_slot,
    });
    let (from_end, to_end) = if from_position < to_position {
        (first_corner.point, second_corner.point)
    } else {
        (second_corner.point, first_corner.point)
    };
    Ok((out, from_end, to_end))
}

/// Bridge two boundary loops with quads. Loops must have the same length.
pub fn bridge_loops(mesh: &Mesh, loop_a: &[u32], loop_b: &[u32]) -> Result<Mesh, String> {
    if loop_a.len() != loop_b.len() || loop_a.len() < 3 {
        return Err("bridge requires two loops with the same length (>= 3)".into());
    }
    if loop_a
        .iter()
        .chain(loop_b.iter())
        .any(|point| (*point as usize) >= mesh.positions.len())
    {
        return Err("bridge loops reference missing points".into());
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
/// How a selected n-gon is split into triangles. Cheetah3D documents a split
/// option but its manual page is not among the indexed sources, so the two
/// choices are clean-room: `ShortestDiagonal` ear-clips the polygon choosing
/// the shortest valid diagonal first, and `QuadPair` splits a quad along its
/// shorter diagonal (other n-gons still ear-clip).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TriangulateMode {
    #[default]
    ShortestDiagonal,
    QuadPair,
}

impl TriangulateMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::ShortestDiagonal => "Shortest Diagonal",
            Self::QuadPair => "Quad Pair",
        }
    }
}

/// Split the selected polygons into triangles. Unselected polygons keep their
/// n-gons, so the operation only rewrites the faces the user selected, and
/// every corner keeps its point, UV slot and material slot. Creases, seams and
/// pinned UV slots are point/corner indices, so they survive unchanged; the new
/// diagonals carry no crease or seam. Degenerate triangles are skipped instead
/// of producing invalid geometry.
pub fn triangulate_faces(mesh: &Mesh, faces: &[u32]) -> Mesh {
    triangulate_faces_with(mesh, faces, TriangulateMode::ShortestDiagonal)
}

pub fn triangulate_faces_with(mesh: &Mesh, faces: &[u32], mode: TriangulateMode) -> Mesh {
    let selected: HashSet<u32> = faces
        .iter()
        .copied()
        .filter(|face| (*face as usize) < mesh.polygons.len())
        .collect();
    let mut out = mesh.clone();
    out.polygons.clear();
    for (index, polygon) in mesh.polygons.iter().enumerate() {
        if !selected.contains(&(index as u32)) || polygon.corners.len() < 3 {
            out.polygons.push(polygon.clone());
            continue;
        }
        for triangle in triangulate_corners(polygon, &mesh.positions, mode) {
            out.polygons.push(Polygon {
                corners: triangle,
                material_slot: polygon.material_slot,
            });
        }
    }
    out
}

fn polygon_newell_normal(polygon: &Polygon, positions: &[Vec3]) -> Vec3 {
    let n = polygon.corners.len();
    let mut normal = Vec3::ZERO;
    for i in 0..n {
        let a = positions[polygon.corners[i].point as usize];
        let b = positions[polygon.corners[(i + 1) % n].point as usize];
        normal.x += (a.y - b.y) * (a.z + b.z);
        normal.y += (a.z - b.z) * (a.x + b.x);
        normal.z += (a.x - b.x) * (a.y + b.y);
    }
    normal.normalize_or(Vec3::Y)
}

/// Ear-clip one polygon, preserving corner order (and therefore winding). A
/// quad in `QuadPair` mode takes its shorter diagonal directly; every other
/// case removes the valid ear with the shortest diagonal.
fn triangulate_corners(
    polygon: &Polygon,
    positions: &[Vec3],
    mode: TriangulateMode,
) -> Vec<Vec<Corner>> {
    let count = polygon.corners.len();
    if count == 3 {
        return vec![polygon.corners.clone()];
    }
    let point = |index: usize| positions[polygon.corners[index].point as usize];
    let diagonal = |a: usize, b: usize| point(a).distance(point(b));
    if count == 4 && mode == TriangulateMode::QuadPair {
        return if diagonal(0, 2) <= diagonal(1, 3) {
            vec![
                vec![polygon.corners[0], polygon.corners[1], polygon.corners[2]],
                vec![polygon.corners[0], polygon.corners[2], polygon.corners[3]],
            ]
        } else {
            vec![
                vec![polygon.corners[1], polygon.corners[2], polygon.corners[3]],
                vec![polygon.corners[1], polygon.corners[3], polygon.corners[0]],
            ]
        };
    }

    let origin = point(0);
    let normal = polygon_newell_normal(polygon, positions);
    let u = (point(1) - origin).normalize_or(Vec3::X);
    let v = normal.cross(u).normalize_or(Vec3::Z);
    let projected: Vec<Vec2> = (0..count)
        .map(|index| {
            let offset = point(index) - origin;
            Vec2::new(offset.dot(u), offset.dot(v))
        })
        .collect();
    let signed_area: f32 = (0..count)
        .map(|i| {
            let a = projected[i];
            let b = projected[(i + 1) % count];
            a.x * b.y - b.x * a.y
        })
        .sum();
    let sign = if signed_area >= 0.0 { 1.0 } else { -1.0 };
    let cross = |a: Vec2, b: Vec2, c: Vec2| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);

    let mut remaining: Vec<usize> = (0..count).collect();
    let mut triangles = Vec::with_capacity(count - 2);
    while remaining.len() > 3 {
        let mut best: Option<(usize, f32)> = None;
        for (slot, &current) in remaining.iter().enumerate() {
            let len = remaining.len();
            let prev = remaining[(slot + len - 1) % len];
            let next = remaining[(slot + 1) % len];
            if cross(projected[prev], projected[current], projected[next]) * sign <= 1e-9 {
                continue; // reflex or collinear
            }
            let blocked = remaining.iter().any(|&other| {
                other != prev
                    && other != current
                    && other != next
                    && point_in_triangle(
                        projected[other],
                        projected[prev],
                        projected[current],
                        projected[next],
                    )
            });
            if blocked {
                continue;
            }
            let length = diagonal(prev, next);
            if best.is_none_or(|(_, shortest)| length < shortest) {
                best = Some((slot, length));
            }
        }
        let Some((slot, _)) = best else {
            break;
        };
        let len = remaining.len();
        let current = remaining[slot];
        let prev = remaining[(slot + len - 1) % len];
        let next = remaining[(slot + 1) % len];
        triangles.push(vec![
            polygon.corners[prev],
            polygon.corners[current],
            polygon.corners[next],
        ]);
        remaining.remove(slot);
    }
    // Degenerate/self-intersecting polygons leave a fan for the remainder.
    for index in 1..remaining.len().saturating_sub(1) {
        triangles.push(vec![
            polygon.corners[remaining[0]],
            polygon.corners[remaining[index]],
            polygon.corners[remaining[index + 1]],
        ]);
    }
    triangles
}

fn point_in_triangle(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let cross = |p: Vec2, a: Vec2, b: Vec2| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    let d1 = cross(p, a, b);
    let d2 = cross(p, b, c);
    let d3 = cross(p, c, a);
    (d1 >= -1e-9 && d2 >= -1e-9 && d3 >= -1e-9) || (d1 <= 1e-9 && d2 <= 1e-9 && d3 <= 1e-9)
}

/// Reverse the winding of the selected polygons, so their normals point the
/// other way.
///
/// Cheetah3D lists Flip Normal in its polygon-tool set (source 1) but the manual
/// page for it is not among the pages indexed here, so acting on the current
/// polygon selection is a LeopardCraft clean-room choice. Only selected polygons
/// change; every corner record keeps its point and UV slot, and creases, seams
/// and pinned UV slots are point pairs or slot indices, so they are untouched.
/// Reversing the corner order is what rendering, shading and glTF export read as
/// the face normal, and it is the same reversal `Mesh::flip_normals` applies to
/// a whole mesh after mirroring. A repeated face index flips once and
/// out-of-range indices are ignored, so a duplicated component selection cannot
/// silently flip a face twice.
pub fn flip_faces(mesh: &Mesh, faces: &[u32]) -> Mesh {
    let selected: HashSet<u32> = faces
        .iter()
        .copied()
        .filter(|face| (*face as usize) < mesh.polygons.len())
        .collect();
    let mut out = mesh.clone();
    for face in selected {
        out.polygons[face as usize].corners.reverse();
    }
    out
}

/// Weld the selected points into one point at their centroid.
///
/// Cheetah3D lists Collapse in its polygon-tool set (source 1); its manual page
/// is not among the pages indexed here, so the exact selection rule is a
/// LeopardCraft clean-room choice. This is the point-mode reading of the tool:
/// every selected point becomes the lowest-indexed one, faces keep the vertices
/// they do not collapse, a face that loses an edge becomes a smaller polygon and
/// a face that repeats the merged point is removed. The returned index is the
/// survivor in the rebuilt mesh (`None` when nothing merged or no face survived),
/// so the caller can keep its component selection valid.
pub fn collapse_points(mesh: &Mesh, points: &[u32]) -> (Mesh, Option<u32>) {
    let mut out = mesh.clone();
    let survivor = out.merge_points(points);
    (out, survivor)
}

/// Which points each side of a face selection uses, as `(selected, other)`.
fn point_sides(mesh: &Mesh, selected: &HashSet<usize>) -> (Vec<bool>, Vec<bool>) {
    let mut in_selected = vec![false; mesh.positions.len()];
    let mut in_other = vec![false; mesh.positions.len()];
    for (index, poly) in mesh.polygons.iter().enumerate() {
        let side = if selected.contains(&index) {
            &mut in_selected
        } else {
            &mut in_other
        };
        for corner in &poly.corners {
            if let Some(flag) = side.get_mut(corner.point as usize) {
                *flag = true;
            }
        }
    }
    (in_selected, in_other)
}

fn selected_face_set(mesh: &Mesh, faces: &[u32]) -> HashSet<usize> {
    faces
        .iter()
        .map(|face| *face as usize)
        .filter(|face| *face < mesh.polygons.len())
        .collect()
}

/// Whether `disconnect_faces` would detach anything: at least one selected face
/// and a point that face shares with a face outside the selection.
///
/// A whole-mesh selection, a selection of already separate islands, or an empty
/// selection cannot be disconnected, so the editor disables the command instead
/// of reporting a no-op.
pub fn can_disconnect(mesh: &Mesh, faces: &[u32]) -> bool {
    let selected = selected_face_set(mesh, faces);
    if selected.is_empty() {
        return false;
    }
    let (in_selected, in_other) = point_sides(mesh, &selected);
    in_selected
        .iter()
        .zip(in_other.iter())
        .any(|(a, b)| *a && *b)
}

/// Detach the selected faces from the rest of the mesh without moving anything.
///
/// Cheetah3D lists Disconnect in its polygon-tool set (source 1); the manual
/// page is not among the pages indexed here, so working on the current polygon
/// selection is a LeopardCraft clean-room choice. Points shared by a selected
/// and an unselected face are duplicated: the selected faces switch to the copy
/// and the unselected faces keep the original, so the region becomes its own
/// topological island while every position and winding order stays exactly as
/// authored. Points used only by the selection stay shared between those faces,
/// and the unselected polygons keep their corner records untouched.
///
/// Duplicating a point does not touch UV slots, so a corner keeps its UV value,
/// its pin and its material slot; creases and seams follow the faces on each
/// side, keeping the original mark where an unselected face still uses the edge
/// and adding the same mark on the duplicated indices for the selected side.
/// Every point stays referenced by some face (the originals by the unselected
/// side, the copies by the selection), so nothing is compacted away and polygon
/// indices never change.
pub fn disconnect_faces(mesh: &Mesh, faces: &[u32]) -> Mesh {
    let selected = selected_face_set(mesh, faces);
    if selected.is_empty() {
        return mesh.clone();
    }
    let (in_selected, in_other) = point_sides(mesh, &selected);
    let mut map: Vec<u32> = (0..mesh.positions.len() as u32).collect();
    let mut out = mesh.clone();
    for (index, (a, b)) in in_selected.iter().zip(in_other.iter()).enumerate() {
        if *a && *b {
            map[index] = out.positions.len() as u32;
            out.positions.push(mesh.positions[index]);
        }
    }
    if out.positions.len() == mesh.positions.len() {
        return mesh.clone();
    }
    for index in &selected {
        for corner in &mut out.polygons[*index].corners {
            corner.point = map[corner.point as usize];
        }
    }

    // Edges each side uses, in original point indices, so a mark can follow its
    // faces across the cut.
    let mut selected_edges: HashSet<[u32; 2]> = HashSet::new();
    let mut other_edges: HashSet<[u32; 2]> = HashSet::new();
    for (index, poly) in mesh.polygons.iter().enumerate() {
        let side = if selected.contains(&index) {
            &mut selected_edges
        } else {
            &mut other_edges
        };
        let n = poly.corners.len();
        for i in 0..n {
            let a = poly.corners[i].point;
            let b = poly.corners[(i + 1) % n].point;
            side.insert(if a < b { [a, b] } else { [b, a] });
        }
    }
    let follow_marks = |list: &[[u32; 2]]| -> Vec<[u32; 2]> {
        let mut next: Vec<[u32; 2]> = Vec::with_capacity(list.len());
        for [a, b] in list {
            let key = if a < b { [*a, *b] } else { [*b, *a] };
            let (ma, mb) = (map[*a as usize], map[*b as usize]);
            let remapped = ma != *a || mb != *b;
            if !remapped || other_edges.contains(&key) {
                next.push(key);
            }
            if remapped && ma != mb && selected_edges.contains(&key) {
                next.push(if ma < mb { [ma, mb] } else { [mb, ma] });
            }
        }
        next.sort_unstable();
        next.dedup();
        next
    };
    out.creases = follow_marks(&mesh.creases);
    out.crease_weights = Box::new({
        let mut next: Vec<([u32; 2], f32)> = Vec::with_capacity(mesh.crease_weights.len());
        for ([a, b], weight) in mesh.crease_weights.iter() {
            let key = if a < b { [*a, *b] } else { [*b, *a] };
            let (ma, mb) = (map[*a as usize], map[*b as usize]);
            let remapped = ma != *a || mb != *b;
            if !remapped || other_edges.contains(&key) {
                next.push((key, *weight));
            }
            if remapped && ma != mb && selected_edges.contains(&key) {
                next.push((if ma < mb { [ma, mb] } else { [mb, ma] }, *weight));
            }
        }
        next.sort_by(|(a, wa), (b, wb)| a.cmp(b).then_with(|| wa.total_cmp(wb)));
        next.dedup_by(|(a, _), (b, _)| a == b);
        next
    });
    out.seams = follow_marks(&mesh.seams);
    out
}

/// Boundary loops that contain at least one of `points`, in mesh edge order.
///
/// Cheetah3D's Fill Hole tool needs only one point on the hole border and finds
/// the rest of it automatically (source 38), so callers hand over the points
/// their component selection covers and get back the loops those points sit on.
pub fn open_holes(mesh: &Mesh, points: &[u32]) -> Vec<Vec<u32>> {
    if points.is_empty() {
        return Vec::new();
    }
    let selected: HashSet<u32> = points.iter().copied().collect();
    mesh.boundary_loops()
        .into_iter()
        .filter(|looped| looped.iter().any(|p| selected.contains(p)))
        .collect()
}

/// Fill each boundary loop with one n-gon.
///
/// The loop is reversed before it becomes a face so every shared border edge is
/// traversed opposite to its existing neighbour, which keeps the new polygon's
/// winding and normal consistent with the surface around it. UVs are a
/// clean-room planar projection of the loop into its own 0..1 box and the
/// material slot is inherited from the faces on the border; the Fill Hole
/// manual documents no properties at all (source 38), so neither of those is a
/// parity claim.
pub fn fill_holes(mesh: &Mesh, loops: &[Vec<u32>]) -> Result<Mesh, String> {
    if loops.is_empty() {
        return Err("select a point on a hole border".into());
    }
    let components = point_components(mesh);
    let mut edge_faces: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    for edge in mesh.edges() {
        edge_faces.insert((edge.a, edge.b), edge.faces.clone());
    }
    let mut out = mesh.clone();
    for looped in loops {
        if looped.len() < 3 {
            return Err("a hole border needs at least three points".into());
        }
        // A single n-gon cannot represent a hole with an island inside it: it
        // would pass straight through the island and hide it. The manual says
        // nothing about this case, so refuse it rather than invent a covering
        // surface; filling the island's own border or removing the island
        // first is unambiguous.
        let outer_component = components
            .get(looped[0] as usize)
            .copied()
            .unwrap_or(u32::MAX);
        for (point, position) in mesh.positions.iter().enumerate() {
            let component = components.get(point).copied().unwrap_or(u32::MAX);
            if component == u32::MAX || component == outer_component {
                continue;
            }
            if loop_encloses_point(mesh, looped, *position) {
                return Err("that hole encloses an island; fill or remove the island first".into());
            }
        }
        let mut slot = u16::MAX;
        for i in 0..looped.len() {
            let a = looped[i];
            let b = looped[(i + 1) % looped.len()];
            let key = if a < b { (a, b) } else { (b, a) };
            let Some(faces) = edge_faces.get(&key) else {
                return Err("that border is not a mesh edge".into());
            };
            if faces.len() != 1 {
                return Err("the selected border is not an open hole".into());
            }
            slot = slot.min(mesh.polygons[faces[0] as usize].material_slot);
        }
        let mut points: Vec<u32> = looped.clone();
        points.reverse();
        let Some(normal) = loop_normal(mesh, &points) else {
            return Err("the hole border references a missing point".into());
        };
        if points
            .iter()
            .filter_map(|point| mesh.positions.get(*point as usize))
            .any(|position| !position.is_finite())
        {
            return Err("the hole border references a non-finite point".into());
        }
        let (tangent, bitangent) = lc_core::orthonormal_basis(normal);
        // Centre, projection and bounds are f64: opposite-signed f32
        // coordinates can overflow their own sum or span past f32::MAX, which
        // would turn this planar UV projection into NaN.
        let count = points.len() as f64;
        let mut center = [0.0f64; 3];
        for p in points
            .iter()
            .filter_map(|p| mesh.positions.get(*p as usize))
        {
            center[0] += p.x as f64;
            center[1] += p.y as f64;
            center[2] += p.z as f64;
        }
        let center = [center[0] / count, center[1] / count, center[2] / count];
        let project = |p: Vec3| -> [f64; 2] {
            let offset = [
                p.x as f64 - center[0],
                p.y as f64 - center[1],
                p.z as f64 - center[2],
            ];
            [
                offset[0] * tangent.x as f64
                    + offset[1] * tangent.y as f64
                    + offset[2] * tangent.z as f64,
                offset[0] * bitangent.x as f64
                    + offset[1] * bitangent.y as f64
                    + offset[2] * bitangent.z as f64,
            ]
        };
        let projected: Vec<[f64; 2]> = points
            .iter()
            .map(|p| project(mesh.positions[*p as usize]))
            .collect();
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for uv in &projected {
            min[0] = min[0].min(uv[0]);
            min[1] = min[1].min(uv[1]);
            max[0] = max[0].max(uv[0]);
            max[1] = max[1].max(uv[1]);
        }
        let size = [(max[0] - min[0]).max(1e-5), (max[1] - min[1]).max(1e-5)];
        let uvs: Vec<Vec2> = projected
            .iter()
            .map(|uv| {
                Vec2::new(
                    ((uv[0] - min[0]) / size[0]) as f32,
                    ((uv[1] - min[1]) / size[1]) as f32,
                )
            })
            .collect();
        push_polygon(&mut out, &points, &uvs, slot);
    }
    out.remove_degenerate_polygons();
    Ok(out)
}

/// Closed open-mesh boundary loops that the selected faces border on every side.
///
/// Cover reads the selection as the ring of polygons around a hole: a loop only
/// qualifies when every one of its edges is an open boundary edge belonging to a
/// selected face. A loop the selection only partly borders, a boundary that is
/// already closed into the mesh, and an open chain that `boundary_loops` could
/// not close are all left out, so the fill never depends on guessing.
pub fn coverable_holes(mesh: &Mesh, faces: &[u32]) -> Vec<Vec<u32>> {
    let selected = selected_face_set(mesh, faces);
    if selected.is_empty() {
        return Vec::new();
    }
    let mut edge_face: HashMap<(u32, u32), u32> = HashMap::new();
    for edge in mesh.edges() {
        if edge.faces.len() == 1 {
            edge_face.insert((edge.a, edge.b), edge.faces[0]);
        }
    }
    mesh.boundary_loops()
        .into_iter()
        .filter(|looped| {
            let n = looped.len();
            n >= 3
                && (0..n).all(|i| {
                    let a = looped[i];
                    let b = looped[(i + 1) % n];
                    let key = if a < b { (a, b) } else { (b, a) };
                    edge_face
                        .get(&key)
                        .is_some_and(|face| selected.contains(&(*face as usize)))
                })
        })
        .collect()
}

/// Close the holes the selected faces border with one polygon each.
///
/// Cheetah3D lists Cover in its polygon-tool set (source 1); its manual page is
/// not among the pages indexed here, so reading the selection as the ring of
/// faces around a hole is a LeopardCraft clean-room choice. The fill reuses the
/// existing boundary points in their existing order and goes through the same
/// constructor as Fill Hole: the polygon is reversed so it winds with its
/// neighbours, inherits the lowest bordering material slot and gets a planar UV
/// projection into its own 0..1 box, so the result is watertight and shares
/// indices with the geometry around it. No existing point or polygon changes.
/// Returns the new face indices so the caller can keep a valid selection.
pub fn cover_faces(mesh: &Mesh, faces: &[u32]) -> Result<(Mesh, Vec<u32>), String> {
    let loops = coverable_holes(mesh, faces);
    if loops.is_empty() {
        return Err("select polygons that border an open hole on every side".into());
    }
    let start = mesh.polygons.len() as u32;
    let out = fill_holes(mesh, &loops)?;
    let created = (start..out.polygons.len() as u32).collect();
    Ok((out, created))
}

/// Whether a polygon already uses exactly this set of points.
fn face_exists(mesh: &Mesh, points: &[u32]) -> bool {
    let mut wanted: Vec<u32> = points.to_vec();
    wanted.sort_unstable();
    mesh.polygons.iter().any(|poly| {
        let mut corners: Vec<u32> = poly.corners.iter().map(|corner| corner.point).collect();
        corners.sort_unstable();
        corners == wanted
    })
}

/// Newell area of an ordered point loop, zero when a point is missing.
fn loop_area(mesh: &Mesh, points: &[u32]) -> f32 {
    let n = points.len();
    let mut normal = Vec3::ZERO;
    for i in 0..n {
        let (Some(a), Some(b)) = (
            mesh.positions.get(points[i] as usize),
            mesh.positions.get(points[(i + 1) % n] as usize),
        ) else {
            return 0.0;
        };
        normal.x += (a.y - b.y) * (a.z + b.z);
        normal.y += (a.z - b.z) * (a.x + b.x);
        normal.z += (a.x - b.x) * (a.y + b.y);
    }
    normal.length() * 0.5
}

/// The UV a point already carries on the first corner that uses it.
fn point_uv(mesh: &Mesh, point: u32) -> (Vec2, bool) {
    for poly in &mesh.polygons {
        for corner in &poly.corners {
            if corner.point == point {
                return (mesh.uv_of(corner), mesh.is_uv_pinned(corner.uv));
            }
        }
    }
    (Vec2::ZERO, false)
}

/// Weld the selected point clusters that sit within `tolerance` of each other.
///
/// Cheetah3D lists Optimize in its polygon-tool set (source 1); its manual page
/// is not among the pages indexed here, so welding only the selected clusters is
/// a LeopardCraft clean-room reading of the tool. This wraps `Mesh::weld_points`:
/// each cluster keeps its lowest-indexed point at its authored position, faces
/// keep the vertices they do not merge, a face that loses an edge becomes a
/// smaller polygon and a face that repeats a merged point is removed. Creases,
/// seams and pins follow the surviving indices. Returns the rebuilt indices of
/// the selected points for the component selection, or `None` when nothing
/// merges.
pub fn optimize_points(mesh: &Mesh, points: &[u32], tolerance: f32) -> Option<(Mesh, Vec<u32>)> {
    let mut out = mesh.clone();
    let survivors = out.weld_points(points, tolerance)?;
    Some((out, survivors))
}

/// Whether `optimize_points` would merge anything.
pub fn can_optimize(mesh: &Mesh, points: &[u32], tolerance: f32) -> bool {
    mesh.can_weld_points(points, tolerance)
}

/// Move, rotate and scale a set of points around their combined centroid.
///
/// Cheetah3D's component transform works on the point, edge or polygon
/// selection with its viewport axis gizmo. MCP callers have no persistent
/// selection, so this helper takes explicit point indices and one world-space
/// translation, an XYZ Euler rotation in degrees and a per-axis scale. Points
/// are transformed in that order (`scale`, `rotate`, `translate`); UVs,
/// topology, creases, seams and pins are untouched. Returns `None` when no
/// selected index is valid or the result would not change the mesh.
pub fn transform_points(
    mesh: &Mesh,
    points: &[u32],
    translation: Vec3,
    rotation_degrees: Vec3,
    scale: Vec3,
) -> Option<Mesh> {
    if !translation.is_finite() || !rotation_degrees.is_finite() || !scale.is_finite() {
        return None;
    }
    let mut selected: Vec<u32> = points
        .iter()
        .copied()
        .filter(|point| (*point as usize) < mesh.positions.len())
        .collect();
    selected.sort_unstable();
    selected.dedup();
    if selected.is_empty() {
        return None;
    }
    let centre = selected
        .iter()
        .map(|point| mesh.positions[*point as usize])
        .sum::<Vec3>()
        / selected.len() as f32;
    let rotation = Quat::from_euler(
        glam::EulerRot::XYZ,
        rotation_degrees.x.to_radians(),
        rotation_degrees.y.to_radians(),
        rotation_degrees.z.to_radians(),
    );
    let mut out = mesh.clone();
    let mut changed = false;
    for point in selected {
        let position = out.positions[point as usize];
        let local = (position - centre) * scale;
        let moved = centre + rotation * local + translation;
        if moved != position {
            changed = true;
        }
        out.positions[point as usize] = moved;
    }
    changed.then_some(out)
}

/// Slide the selected points a fraction of the way toward their neighbours.
///
/// Cheetah3D lists Point Slide in its polygon-tool set (source 1); its manual page
/// is not among the pages indexed here, so this reading is a LeopardCraft
/// clean-room choice. Each selected point moves `fraction` toward the centroid of
/// the points its edges connect it to, measured from the positions before the
/// move, so a point with exactly two neighbours lands on the midpoint of its edge
/// at a fraction of 1. This wraps `Mesh::slide_points`: only selected positions change and no
/// topology, UV, material, crease, seam or pin data is touched, so the component
/// selection stays valid. Returns `None` when nothing moves.
pub fn slide_points(mesh: &Mesh, points: &[u32], fraction: f32) -> Option<Mesh> {
    let mut out = mesh.clone();
    out.slide_points(points, fraction).then_some(out)
}

/// Whether `slide_points` would move at least one selected point.
pub fn can_slide_points(mesh: &Mesh, points: &[u32], fraction: f32) -> bool {
    mesh.can_slide_points(points, fraction)
}

/// Open boundary loops the selection covers completely, in mesh boundary order.
///
/// Create Polygon reads the selection as the points around a hole: a loop only
/// qualifies when every one of its points is selected, it is not already a face
/// and its Newell area is non-zero. A partial run, a collinear run and a face
/// that already exists are therefore left out, while the surviving loops are
/// returned in the mesh's own adjacency walk, so the result never depends on
/// the order the points were clicked.
pub fn faceable_loops(mesh: &Mesh, points: &[u32]) -> Vec<Vec<u32>> {
    let mut selected: Vec<u32> = points
        .iter()
        .copied()
        .filter(|point| (*point as usize) < mesh.positions.len())
        .collect();
    selected.sort_unstable();
    selected.dedup();
    if selected.len() < 3 {
        return Vec::new();
    }
    let selected: HashSet<u32> = selected.into_iter().collect();
    mesh.boundary_loops()
        .into_iter()
        .filter(|looped| {
            looped.len() >= 3
                && looped.iter().all(|point| selected.contains(point))
                && !face_exists(mesh, looped)
                && loop_area(mesh, looped) > 1e-8
        })
        .collect()
}

/// Build new polygons from the selected points without touching the rest.
///
/// Cheetah3D lists Create Polygon in its polygon-tool set (source 1); its manual
/// page is not among the pages indexed here, so reading the selection as the
/// points around a hole is a LeopardCraft clean-room choice. Each qualifying
/// boundary loop becomes one polygon over the existing point indices, ordered by
/// the mesh's boundary walk and reversed so it winds with its neighbours; no
/// point is created, moved or renumbered and no existing polygon changes. New
/// corners get their UV from the point's existing corner (and inherit its pin),
/// the polygon takes the lowest material slot bordering the loop, and its Newell
/// area is non-zero because degenerate loops are filtered out. Returns the new
/// face indices so the caller can keep a valid selection.
pub fn create_polygon_faces(mesh: &Mesh, points: &[u32]) -> Result<(Mesh, Vec<u32>), String> {
    let loops = faceable_loops(mesh, points);
    if loops.is_empty() {
        return Err("select at least three boundary points that close an open region".into());
    }
    let mut slot_by_edge: HashMap<(u32, u32), u16> = HashMap::new();
    for edge in mesh.edges() {
        if edge.faces.len() == 1 {
            slot_by_edge.insert(
                (edge.a, edge.b),
                mesh.polygons[edge.faces[0] as usize].material_slot,
            );
        }
    }
    let mut out = mesh.clone();
    let mut created = Vec::with_capacity(loops.len());
    for looped in &loops {
        let n = looped.len();
        let mut slot = u16::MAX;
        for i in 0..n {
            let a = looped[i];
            let b = looped[(i + 1) % n];
            let key = if a < b { (a, b) } else { (b, a) };
            if let Some(edge_slot) = slot_by_edge.get(&key) {
                slot = slot.min(*edge_slot);
            }
        }
        if slot == u16::MAX {
            slot = 0;
        }
        let mut ordered = looped.clone();
        ordered.reverse();
        let inherited: Vec<(Vec2, bool)> = ordered.iter().map(|p| point_uv(mesh, *p)).collect();
        let uvs: Vec<Vec2> = inherited.iter().map(|(uv, _)| *uv).collect();
        let index = out.add_face_with_uvs(&ordered, &uvs);
        out.polygons[index].material_slot = slot;
        let first_uv = out.uvs.len() as u32 - ordered.len() as u32;
        for (i, (_, pinned)) in inherited.iter().enumerate() {
            if *pinned {
                out.set_uv_pinned(first_uv + i as u32, true);
            }
        }
        created.push(index as u32);
    }
    Ok((out, created))
}

/// Newell normal of an ordered point loop, or `None` when a point is missing.
/// Connected components of the polygon mesh, labelled per point. Points that
/// no polygon uses, and points whose index is out of range, keep `u32::MAX`.
fn point_components(mesh: &Mesh) -> Vec<u32> {
    let mut adjacency = vec![Vec::new(); mesh.positions.len()];
    for polygon in &mesh.polygons {
        let n = polygon.corners.len();
        for i in 0..n {
            let a = polygon.corners[i].point as usize;
            let b = polygon.corners[(i + 1) % n].point as usize;
            if a >= adjacency.len() || b >= adjacency.len() || a == b {
                continue;
            }
            adjacency[a].push(b as u32);
            adjacency[b].push(a as u32);
        }
    }
    let mut components = vec![u32::MAX; mesh.positions.len()];
    let mut next = 0;
    for start in 0..adjacency.len() {
        if components[start] != u32::MAX || adjacency[start].is_empty() {
            continue;
        }
        components[start] = next;
        let mut stack = vec![start];
        while let Some(point) = stack.pop() {
            for &neighbour in &adjacency[point] {
                let neighbour = neighbour as usize;
                if components[neighbour] == u32::MAX {
                    components[neighbour] = next;
                    stack.push(neighbour);
                }
            }
        }
        next += 1;
    }
    components
}

/// Whether `point` projects inside the closed boundary in its own best-fit
/// plane. Used only to detect a separate island enclosed by a hole border;
/// degenerate borders (no usable plane) never report containment.
fn loop_encloses_point(mesh: &Mesh, looped: &[u32], point: Vec3) -> bool {
    if looped.len() < 3 {
        return false;
    }
    let Some(normal) = loop_normal(mesh, looped) else {
        return false;
    };
    let (tangent, bitangent) = lc_core::orthonormal_basis(normal);
    let Some(origin) = looped
        .first()
        .and_then(|p| mesh.positions.get(*p as usize))
        .copied()
    else {
        return false;
    };
    let project = |p: Vec3| -> [f64; 2] {
        let offset = [
            p.x as f64 - origin.x as f64,
            p.y as f64 - origin.y as f64,
            p.z as f64 - origin.z as f64,
        ];
        [
            offset[0] * tangent.x as f64
                + offset[1] * tangent.y as f64
                + offset[2] * tangent.z as f64,
            offset[0] * bitangent.x as f64
                + offset[1] * bitangent.y as f64
                + offset[2] * bitangent.z as f64,
        ]
    };
    let border: Vec<[f64; 2]> = looped
        .iter()
        .filter_map(|p| mesh.positions.get(*p as usize).map(|p| project(*p)))
        .collect();
    if border.len() != looped.len() {
        return false;
    }
    let p = project(point);
    let mut inside = false;
    for i in 0..border.len() {
        let a = border[i];
        let b = border[(i + 1) % border.len()];
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}

fn loop_normal(mesh: &Mesh, points: &[u32]) -> Option<Vec3> {
    let n = points.len();
    if n < 3 {
        return None;
    }
    let mut normal = Vec3::ZERO;
    for i in 0..n {
        let a = mesh.positions.get(points[i] as usize)?;
        let b = mesh.positions.get(points[(i + 1) % n] as usize)?;
        normal.x += (a.y - b.y) * (a.z + b.z);
        normal.y += (a.z - b.z) * (a.x + b.x);
        normal.z += (a.x - b.x) * (a.y + b.y);
    }
    Some(normal.normalize_or(Vec3::Y))
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

/// Parameters for Cheetah3D's Clone tool. Values mirror the manual's tool
/// properties: `copies`, approximate `holes` ratio, object-space `direction`,
/// interpolated `scale` and `rotation_deg` around that direction, plus the
/// random position/scale/rotation variations and `keep_ratio`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloneOptions {
    pub copies: u32,
    pub holes: f32,
    pub direction: Vec3,
    pub scale: f32,
    pub rotation_deg: f32,
    pub position_variation: f32,
    pub scale_variation: f32,
    pub rotation_variation_deg: f32,
    pub keep_ratio: bool,
}

impl Default for CloneOptions {
    fn default() -> Self {
        Self {
            copies: 1,
            holes: 0.0,
            direction: Vec3::X,
            scale: 1.0,
            rotation_deg: 0.0,
            position_variation: 0.0,
            scale_variation: 0.0,
            rotation_variation_deg: 0.0,
            keep_ratio: true,
        }
    }
}

fn clone_offset(options: &CloneOptions, t: f32) -> Vec3 {
    options.direction * t
}

/// Apply the Clone tool to a polygon selection. Empty `faces` clones every
/// polygon, matching the manual. Each copy is transformed around the selection
/// centroid, and a copy may be skipped according to the approximate `holes`
/// ratio using a fixed deterministic stream rather than wall-clock randomness.
pub fn clone_faces(mesh: &Mesh, faces: &[u32], options: &CloneOptions) -> Mesh {
    let mut out = mesh.clone();
    let copies = options.copies.min(256);
    if copies == 0 {
        return out;
    }
    let selected: Vec<usize> = if faces.is_empty() {
        (0..mesh.polygons.len()).collect()
    } else {
        let mut set = vec![false; mesh.polygons.len()];
        for face in faces {
            if let Some(slot) = set.get_mut(*face as usize) {
                *slot = true;
            }
        }
        set.iter()
            .enumerate()
            .filter_map(|(index, chosen)| chosen.then_some(index))
            .collect()
    };
    if selected.is_empty() {
        return out;
    }
    let mut used: Vec<u32> = selected
        .iter()
        .flat_map(|face| mesh.polygons[*face].corners.iter().map(|c| c.point))
        .collect();
    used.sort_unstable();
    used.dedup();
    if used.is_empty() {
        return out;
    }
    let pivot = used
        .iter()
        .map(|point| mesh.positions[*point as usize])
        .sum::<Vec3>()
        / used.len() as f32;
    let direction = options.direction.normalize_or(Vec3::Y);
    let mut rng = lc_core::Pcg32::new(0xC10E_5EED);

    for copy in 1..=copies {
        if rng.next_f32() < options.holes.clamp(0.0, 1.0) {
            continue;
        }
        let t = copy as f32 / copies as f32;
        let base_scale = 1.0 + (options.scale - 1.0) * t;
        let mut scale = Vec3::splat(base_scale);
        if options.keep_ratio {
            if options.scale_variation != 0.0 {
                let factor = 1.0
                    + rng.range(
                        -options.scale_variation.abs(),
                        options.scale_variation.abs(),
                    );
                scale *= factor;
            }
        } else {
            for axis in 0..3 {
                let factor = 1.0
                    + rng.range(
                        -options.scale_variation.abs(),
                        options.scale_variation.abs(),
                    );
                scale[axis] *= factor;
            }
        }
        let mut rotation = Quat::from_axis_angle(direction, options.rotation_deg.to_radians() * t);
        if options.rotation_variation_deg != 0.0 {
            let axis = Vec3::new(
                rng.range(-1.0, 1.0),
                rng.range(-1.0, 1.0),
                rng.range(-1.0, 1.0),
            )
            .normalize_or(Vec3::Y);
            rotation *= Quat::from_axis_angle(
                axis,
                rng.range(
                    -options.rotation_variation_deg.abs(),
                    options.rotation_variation_deg.abs(),
                )
                .to_radians(),
            );
        }
        let variation = Vec3::new(
            rng.range(-1.0, 1.0),
            rng.range(-1.0, 1.0),
            rng.range(-1.0, 1.0),
        ) * options.position_variation;
        let offset = clone_offset(options, t) + variation;
        let transform = Mat4::from_translation(offset + pivot)
            * Mat4::from_quat(rotation)
            * Mat4::from_scale(scale)
            * Mat4::from_translation(-pivot);

        let mut point_map: HashMap<u32, u32> = HashMap::with_capacity(used.len());
        for point in &used {
            let position = transform.transform_point3(mesh.positions[*point as usize]);
            point_map.insert(*point, out.add_point(position));
        }
        for face in &selected {
            let poly = &mesh.polygons[*face];
            out.polygons.push(Polygon {
                corners: poly
                    .corners
                    .iter()
                    .filter_map(|corner| {
                        point_map
                            .get(&corner.point)
                            .map(|point| Corner::new(*point, corner.uv))
                    })
                    .collect(),
                material_slot: poly.material_slot,
            });
        }
        let map_edge = |edge: &[u32; 2]| -> Option<[u32; 2]> {
            let a = *point_map.get(&edge[0])?;
            let b = *point_map.get(&edge[1])?;
            Some(if a < b { [a, b] } else { [b, a] })
        };
        for edge in &mesh.creases {
            if let Some(edge) = map_edge(edge) {
                out.creases.push(edge);
            }
        }
        for (edge, weight) in mesh.crease_weights.iter() {
            if let Some(edge) = map_edge(edge) {
                out.crease_weights.push((edge, *weight));
            }
        }
        for edge in &mesh.seams {
            if let Some(edge) = map_edge(edge) {
                out.seams.push(edge);
            }
        }
    }
    out.remove_degenerate_polygons();
    out.normalize_creases();
    out.normalize_seams();
    out
}

/// Clone selected points as loose points. This is the point-mode behavior of
/// the same tool; faces are not invented for a point-only selection.
pub fn clone_points(mesh: &Mesh, points: &[u32], options: &CloneOptions) -> Mesh {
    let mut out = mesh.clone();
    let copies = options.copies.min(256);
    if copies == 0 {
        return out;
    }
    let selected: Vec<u32> = if points.is_empty() {
        (0..mesh.positions.len() as u32).collect()
    } else {
        let mut chosen: Vec<u32> = points
            .iter()
            .copied()
            .filter(|point| (*point as usize) < mesh.positions.len())
            .collect();
        chosen.sort_unstable();
        chosen.dedup();
        chosen
    };
    if selected.is_empty() {
        return out;
    }
    let pivot = selected
        .iter()
        .map(|point| mesh.positions[*point as usize])
        .sum::<Vec3>()
        / selected.len() as f32;
    let direction = options.direction.normalize_or(Vec3::Y);
    let mut rng = lc_core::Pcg32::new(0xC10E_5EED);
    for copy in 1..=copies {
        if rng.next_f32() < options.holes.clamp(0.0, 1.0) {
            continue;
        }
        let t = copy as f32 / copies as f32;
        let scale = 1.0 + (options.scale - 1.0) * t;
        let rotation = Quat::from_axis_angle(direction, options.rotation_deg.to_radians() * t);
        let variation = Vec3::new(
            rng.range(-1.0, 1.0),
            rng.range(-1.0, 1.0),
            rng.range(-1.0, 1.0),
        ) * options.position_variation;
        let transform = Mat4::from_translation(clone_offset(options, t) + variation + pivot)
            * Mat4::from_quat(rotation)
            * Mat4::from_scale(Vec3::splat(scale))
            * Mat4::from_translation(-pivot);
        for point in &selected {
            out.add_point(transform.transform_point3(mesh.positions[*point as usize]));
        }
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

/// Twist around the chosen axis by up to `angle_deg`, distributed linearly
/// along that axis. The two perpendicular coordinates are rotated.
pub fn twist(mesh: &Mesh, axis: usize, angle_deg: f32, center: Vec3) -> Mesh {
    let mut out = mesh.clone();
    let axis = axis.min(2);
    let bounds = mesh.bounds();
    if !bounds.is_valid() {
        return out;
    }
    let total = angle_deg.to_radians();
    if total.abs() < 1e-6 {
        return out;
    }
    let extent = bounds.size()[axis].max(1e-6);
    let (u, v) = match axis {
        0 => (1, 2),
        1 => (2, 0),
        _ => (0, 1),
    };
    for p in &mut out.positions {
        let t = (p[axis] - bounds.min[axis]) / extent - 0.5;
        let a = t * total;
        let du = p[u] - center[u];
        let dv = p[v] - center[v];
        p[u] = center[u] + du * a.cos() - dv * a.sin();
        p[v] = center[v] + du * a.sin() + dv * a.cos();
    }
    out
}

/// Shear along `axis`, moving points in a direction perpendicular to it in
/// proportion to their position along the axis. `direction_deg` rotates the
/// shear direction in the perpendicular plane.
pub fn shear(mesh: &Mesh, axis: usize, distance: f32, direction_deg: f32, center: Vec3) -> Mesh {
    let mut out = mesh.clone();
    let axis = axis.min(2);
    let bounds = mesh.bounds();
    if !bounds.is_valid() || distance.abs() < 1e-6 {
        return out;
    }
    let extent = bounds.size()[axis].max(1e-6);
    let dir = direction_deg.to_radians();
    let (u, v) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    for p in &mut out.positions {
        let t = (p[axis] - bounds.min[axis]) / extent - 0.5;
        p[u] += distance * t * dir.cos();
        p[v] += distance * t * dir.sin();
    }
    let _ = center;
    out
}

/// Bulge the selected cross-section axes outward around the mesh's mid-plane.
/// Positive strength expands the middle; negative strength pinches it.
pub fn bulge(mesh: &Mesh, strength: f32, x_axis: bool, z_axis: bool) -> Mesh {
    let mut out = mesh.clone();
    let bounds = mesh.bounds();
    if !bounds.is_valid() {
        return out;
    }
    let center = bounds.center();
    let half = (bounds.size().y * 0.5).max(1e-6);
    let scale = (1.0 + strength).max(0.01);
    for p in &mut out.positions {
        let t = (1.0 - (p.y - center.y).abs() / half).clamp(0.0, 1.0);
        // Strongest at the mid-plane, but never exactly zero at the ends so a
        // low-poly box still responds instead of looking unchanged.
        let factor = 1.0 + (scale - 1.0) * (0.25 + 0.75 * t);
        if x_axis {
            p.x = center.x + (p.x - center.x) * factor;
        }
        if z_axis {
            p.z = center.z + (p.z - center.z) * factor;
        }
    }
    out
}

/// Move vertices toward a sphere of `radius` around the bounds centre by
/// `strength` (0 = unchanged, 1 = fully spherical).
pub fn spherify(mesh: &Mesh, radius: f32, strength: f32) -> Mesh {
    let mut out = mesh.clone();
    let bounds = mesh.bounds();
    if !bounds.is_valid() {
        return out;
    }
    let center = bounds.center();
    let radius = radius.max(1e-6);
    let strength = strength.clamp(0.0, 1.0);
    for p in &mut out.positions {
        let offset = *p - center;
        let len = offset.length();
        if len > 1e-6 {
            let target = center + offset / len * radius;
            *p = p.lerp(target, strength);
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

// ---------------------------------------------------------------------------
// Displacement, formula, magnet, smoothing, reduction and wrap modifiers.
// ---------------------------------------------------------------------------

/// Deterministic lattice hash in -1..=1 for the displacement height field.
fn lattice_noise(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let mut hash = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ (z as u32).wrapping_mul(0xcb1a_b31f)
        ^ seed.wrapping_mul(0x9e37_79b9);
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(0x5bd1_e995);
    hash ^= hash >> 15;
    hash as f32 / u32::MAX as f32 * 2.0 - 1.0
}

/// Smooth trilinear value noise in -1..=1. Every input and seed combination is
/// deterministic, so a displaced mesh is identical on every evaluation.
fn value_noise(point: Vec3, seed: u32) -> f32 {
    if !point.is_finite() {
        return 0.0;
    }
    let base = point.floor();
    let f = point - base;
    let t = f * f * (Vec3::splat(3.0) - 2.0 * f);
    let (x, y, z) = (base.x as i32, base.y as i32, base.z as i32);
    let corner = |dx: i32, dy: i32, dz: i32| lattice_noise(x + dx, y + dy, z + dz, seed);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(corner(0, 0, 0), corner(1, 0, 0), t.x);
    let x10 = lerp(corner(0, 1, 0), corner(1, 1, 0), t.x);
    let x01 = lerp(corner(0, 0, 1), corner(1, 0, 1), t.x);
    let x11 = lerp(corner(0, 1, 1), corner(1, 1, 1), t.x);
    lerp(lerp(x00, x10, t.y), lerp(x01, x11, t.y), t.z).clamp(-1.0, 1.0)
}

/// Area-weighted point normals: every face adds its normal to each of its
/// corners, so a point moves along the surface it belongs to. Points no face
/// uses keep a zero normal and therefore stay put.
fn point_normals(mesh: &Mesh) -> Vec<Vec3> {
    let mut normals = vec![Vec3::ZERO; mesh.positions.len()];
    for face in 0..mesh.polygons.len() {
        let normal = mesh.face_normal(face);
        for corner in &mesh.polygons[face].corners {
            if let Some(target) = normals.get_mut(corner.point as usize) {
                *target += normal;
            }
        }
    }
    for normal in &mut normals {
        *normal = normal.normalize_or_zero();
    }
    normals
}

/// Offset every point along its normal by a deterministic value-noise field.
///
/// Cheetah3D's Displacement modifier documents a texture input; LeopardCraft has
/// no texture binding a modifier can read, so this is the clean-room reading:
/// the height field is smooth value noise in -1..=1 sampled at `frequency` and
/// scaled by `amplitude`, with `seed` selecting a different field. The same mesh
/// and parameters always give the same geometry, and a zero amplitude returns
/// the mesh unchanged.
pub fn displace(mesh: &Mesh, amplitude: f32, frequency: f32, seed: u32) -> Mesh {
    let mut out = mesh.clone();
    if !amplitude.is_finite() || amplitude == 0.0 {
        return out;
    }
    let frequency = if frequency.is_finite() && frequency != 0.0 {
        frequency
    } else {
        1.0
    };
    let normals = point_normals(mesh);
    for (point, normal) in out.positions.iter_mut().zip(normals) {
        let height = value_noise(*point * frequency, seed);
        *point += normal * (amplitude * height);
    }
    out
}

/// One parsed Formula expression.
#[derive(Clone, Debug, PartialEq)]
enum FormulaExpr {
    Number(f32),
    Variable(usize),
    Neg(Box<FormulaExpr>),
    Add(Box<FormulaExpr>, Box<FormulaExpr>),
    Subtract(Box<FormulaExpr>, Box<FormulaExpr>),
    Multiply(Box<FormulaExpr>, Box<FormulaExpr>),
    Divide(Box<FormulaExpr>, Box<FormulaExpr>),
    Power(Box<FormulaExpr>, Box<FormulaExpr>),
    Call(FormulaFunction, Vec<FormulaExpr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormulaFunction {
    Sin,
    Cos,
    Tan,
    Abs,
    Sqrt,
    Exp,
    Ln,
    Floor,
    Ceil,
    Round,
    Sign,
    Min,
    Max,
}

impl FormulaFunction {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "sin" => Self::Sin,
            "cos" => Self::Cos,
            "tan" => Self::Tan,
            "abs" => Self::Abs,
            "sqrt" => Self::Sqrt,
            "exp" => Self::Exp,
            "ln" => Self::Ln,
            "floor" => Self::Floor,
            "ceil" => Self::Ceil,
            "round" => Self::Round,
            "sign" => Self::Sign,
            "min" => Self::Min,
            "max" => Self::Max,
            _ => return None,
        })
    }

    fn arity(self) -> usize {
        match self {
            Self::Min | Self::Max => 2,
            _ => 1,
        }
    }

    fn evaluate(self, args: &[f32]) -> f32 {
        match (self, args) {
            (Self::Min, [a, b]) => a.min(*b),
            (Self::Max, [a, b]) => a.max(*b),
            (Self::Sin, [a]) => a.sin(),
            (Self::Cos, [a]) => a.cos(),
            (Self::Tan, [a]) => a.tan(),
            (Self::Abs, [a]) => a.abs(),
            (Self::Sqrt, [a]) => a.sqrt(),
            (Self::Exp, [a]) => a.exp(),
            (Self::Ln, [a]) => a.ln(),
            (Self::Floor, [a]) => a.floor(),
            (Self::Ceil, [a]) => a.ceil(),
            (Self::Round, [a]) => a.round(),
            (Self::Sign, [a]) => a.signum(),
            _ => f32::NAN,
        }
    }
}

impl FormulaExpr {
    fn evaluate(&self, point: Vec3) -> f32 {
        match self {
            Self::Number(value) => *value,
            Self::Variable(0) => point.x,
            Self::Variable(1) => point.y,
            Self::Variable(2) => point.z,
            Self::Variable(_) => f32::NAN,
            Self::Neg(inner) => -inner.evaluate(point),
            Self::Add(a, b) => a.evaluate(point) + b.evaluate(point),
            Self::Subtract(a, b) => a.evaluate(point) - b.evaluate(point),
            Self::Multiply(a, b) => a.evaluate(point) * b.evaluate(point),
            Self::Divide(a, b) => a.evaluate(point) / b.evaluate(point),
            Self::Power(a, b) => a.evaluate(point).powf(b.evaluate(point)),
            Self::Call(function, args) => {
                let values: Vec<f32> = args.iter().map(|arg| arg.evaluate(point)).collect();
                function.evaluate(&values)
            }
        }
    }
}

/// Recursive-descent reader for Formula expressions: `+ - * / ^`, parentheses,
/// the variables `x`, `y`, `z`, the constants `pi` and `e`, and a fixed set of
/// one- and two-argument functions.
struct FormulaParser<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> FormulaParser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            input: source.as_bytes(),
            position: 0,
        }
    }

    fn skip_space(&mut self) {
        while let Some(byte) = self.input.get(self.position) {
            if !byte.is_ascii_whitespace() {
                break;
            }
            self.position += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_space();
        self.input.get(self.position).copied()
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn parse(mut self) -> Result<FormulaExpr, String> {
        let expr = self.expression()?;
        match self.peek() {
            None => Ok(expr),
            Some(byte) => Err(format!("unexpected '{}'", byte as char)),
        }
    }

    fn expression(&mut self) -> Result<FormulaExpr, String> {
        let mut left = self.term()?;
        loop {
            match self.peek() {
                Some(b'+') => {
                    self.position += 1;
                    left = FormulaExpr::Add(Box::new(left), Box::new(self.term()?));
                }
                Some(b'-') => {
                    self.position += 1;
                    left = FormulaExpr::Subtract(Box::new(left), Box::new(self.term()?));
                }
                _ => return Ok(left),
            }
        }
    }

    fn term(&mut self) -> Result<FormulaExpr, String> {
        let mut left = self.unary()?;
        loop {
            match self.peek() {
                Some(b'*') => {
                    self.position += 1;
                    left = FormulaExpr::Multiply(Box::new(left), Box::new(self.unary()?));
                }
                Some(b'/') => {
                    self.position += 1;
                    left = FormulaExpr::Divide(Box::new(left), Box::new(self.unary()?));
                }
                _ => return Ok(left),
            }
        }
    }

    fn unary(&mut self) -> Result<FormulaExpr, String> {
        if self.eat(b'-') {
            return Ok(FormulaExpr::Neg(Box::new(self.unary()?)));
        }
        if self.eat(b'+') {
            return self.unary();
        }
        self.power()
    }

    fn power(&mut self) -> Result<FormulaExpr, String> {
        let base = self.primary()?;
        if self.eat(b'^') {
            let exponent = self.unary()?;
            return Ok(FormulaExpr::Power(Box::new(base), Box::new(exponent)));
        }
        Ok(base)
    }

    fn primary(&mut self) -> Result<FormulaExpr, String> {
        match self.peek() {
            None => Err("unexpected end of expression".into()),
            Some(b'(') => {
                self.position += 1;
                let inner = self.expression()?;
                if !self.eat(b')') {
                    return Err("missing ')'".into());
                }
                Ok(inner)
            }
            Some(byte) if byte.is_ascii_digit() || byte == b'.' => self.number(),
            Some(byte) if byte.is_ascii_alphabetic() || byte == b'_' => self.identifier(),
            Some(byte) => Err(format!("unexpected '{}'", byte as char)),
        }
    }

    fn number(&mut self) -> Result<FormulaExpr, String> {
        let start = self.position;
        while let Some(byte) = self.input.get(self.position) {
            if byte.is_ascii_digit() || *byte == b'.' {
                self.position += 1;
            } else {
                break;
            }
        }
        let text = std::str::from_utf8(&self.input[start..self.position]).unwrap_or("");
        let value: f32 = text
            .parse()
            .map_err(|_| format!("'{text}' is not a number"))?;
        if !value.is_finite() {
            return Err(format!("'{text}' is not a finite number"));
        }
        Ok(FormulaExpr::Number(value))
    }

    fn identifier(&mut self) -> Result<FormulaExpr, String> {
        let start = self.position;
        while let Some(byte) = self.input.get(self.position) {
            if byte.is_ascii_alphanumeric() || *byte == b'_' {
                self.position += 1;
            } else {
                break;
            }
        }
        let name = std::str::from_utf8(&self.input[start..self.position]).unwrap_or("");
        match name {
            "x" => return Ok(FormulaExpr::Variable(0)),
            "y" => return Ok(FormulaExpr::Variable(1)),
            "z" => return Ok(FormulaExpr::Variable(2)),
            "pi" => return Ok(FormulaExpr::Number(std::f32::consts::PI)),
            "e" => return Ok(FormulaExpr::Number(std::f32::consts::E)),
            _ => {}
        }
        let Some(function) = FormulaFunction::parse(name) else {
            return Err(format!("unknown name '{name}'"));
        };
        if !self.eat(b'(') {
            return Err(format!("'{name}' needs parentheses"));
        }
        let mut args = Vec::new();
        if !self.eat(b')') {
            loop {
                args.push(self.expression()?);
                if self.eat(b',') {
                    continue;
                }
                if self.eat(b')') {
                    break;
                }
                return Err(format!("missing ')' after '{name}('"));
            }
        }
        if args.len() != function.arity() {
            return Err(format!(
                "'{name}' takes {} argument(s), not {}",
                function.arity(),
                args.len()
            ));
        }
        Ok(FormulaExpr::Call(function, args))
    }
}

/// Whether a Formula axis expression parses. An empty expression means "leave
/// this axis alone" and is valid.
pub fn formula_expression_is_valid(expression: &str) -> bool {
    expression.trim().is_empty() || FormulaParser::new(expression).parse().is_ok()
}

/// Apply up to three Formula expressions, one per axis, to every point.
///
/// Cheetah3D's Formula modifier evaluates a user formula per vertex; its manual
/// page is not among the indexed sources, so this is the clean-room reading:
/// each axis takes an expression in `x`, `y` and `z` (the point's rest position)
/// that produces the new coordinate. An empty expression keeps that coordinate,
/// and a non-finite result (a division by zero, an out-of-domain `sqrt`) also
/// keeps it, so a formula can never introduce NaN into geometry. A malformed
/// expression fails with a message naming the axis and the problem.
pub fn formula(mesh: &Mesh, x: &str, y: &str, z: &str) -> Result<Mesh, String> {
    let parse_axis = |axis: &str, expression: &str| -> Result<Option<FormulaExpr>, String> {
        if expression.trim().is_empty() {
            return Ok(None);
        }
        FormulaParser::new(expression)
            .parse()
            .map(Some)
            .map_err(|error| format!("formula {axis} expression '{expression}': {error}"))
    };
    let expressions = [
        parse_axis("X", x)?,
        parse_axis("Y", y)?,
        parse_axis("Z", z)?,
    ];
    let mut out = mesh.clone();
    for point in &mut out.positions {
        let rest = *point;
        let mut next = rest;
        for (axis, expression) in expressions.iter().enumerate() {
            let Some(expression) = expression else {
                continue;
            };
            let value = expression.evaluate(rest);
            if value.is_finite() {
                next[axis] = value;
            }
        }
        *point = next;
    }
    Ok(out)
}

/// Pull points toward `center`, or push them away when `strength` is negative.
///
/// Cheetah3D's Magnet is a deformer with a strength; this clean-room reading
/// treats it as an attraction field: a point inside `radius` moves toward the
/// bounds centre by `strength` times a smooth falloff that is 1 at the centre
/// and 0 at the radius, so nothing outside the radius moves and the border
/// displacement is zero. Strength is clamped to -1..=1 and a non-positive
/// radius does nothing.
pub fn magnet(mesh: &Mesh, strength: f32, radius: f32, center: Vec3) -> Mesh {
    let mut out = mesh.clone();
    if !strength.is_finite() || strength == 0.0 || !radius.is_finite() || radius <= 0.0 {
        return out;
    }
    let strength = strength.clamp(-1.0, 1.0);
    for point in &mut out.positions {
        let offset = *point - center;
        let distance = offset.length();
        if !distance.is_finite() || distance > radius || distance <= 1e-6 {
            continue;
        }
        let t = 1.0 - distance / radius;
        let falloff = t * t * (3.0 - 2.0 * t);
        *point = point.lerp(center, strength * falloff);
    }
    out
}

/// Umbrella-smooth every point toward the average of its edge neighbours.
///
/// Cheetah3D's Laplacian Smooth exposes iterations and a strength; this is the
/// textbook uniform Laplacian, run `iterations` times (capped at 64 so one
/// evaluation stays responsive) with `strength` as the step size. Border points
/// of an open mesh are held when `preserve_boundary` is set, so a patch keeps
/// its outline and only its interior relaxes.
pub fn laplacian_smooth(
    mesh: &Mesh,
    iterations: u32,
    strength: f32,
    preserve_boundary: bool,
) -> Mesh {
    let mut out = mesh.clone();
    let strength = if strength.is_finite() {
        strength.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if iterations == 0 || strength == 0.0 {
        return out;
    }
    let mut neighbours: Vec<Vec<u32>> = vec![Vec::new(); mesh.positions.len()];
    let mut edge_faces: HashMap<(u32, u32), u32> = HashMap::new();
    for poly in &mesh.polygons {
        let count = poly.corners.len();
        for index in 0..count {
            let a = poly.corners[index].point;
            let b = poly.corners[(index + 1) % count].point;
            if a == b {
                continue;
            }
            if (b as usize) < neighbours.len() {
                if let Some(list) = neighbours.get_mut(a as usize) {
                    list.push(b);
                }
            }
            *edge_faces.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    for list in &mut neighbours {
        list.sort_unstable();
        list.dedup();
    }
    let mut boundary = vec![false; mesh.positions.len()];
    for ((a, b), faces) in &edge_faces {
        if *faces == 1 {
            for point in [*a as usize, *b as usize] {
                if let Some(flag) = boundary.get_mut(point) {
                    *flag = true;
                }
            }
        }
    }
    for _ in 0..iterations.min(64) {
        let previous = out.positions.clone();
        for (index, point) in out.positions.iter_mut().enumerate() {
            if preserve_boundary && boundary[index] {
                continue;
            }
            let list = &neighbours[index];
            if list.is_empty() {
                continue;
            }
            let average = list
                .iter()
                .map(|neighbour| previous[*neighbour as usize])
                .sum::<Vec3>()
                / list.len() as f32;
            *point = previous[index].lerp(average, strength);
        }
    }
    out
}

/// Reduce the polygon count toward `ratio` of the authored faces by collapsing
/// the shortest edge of a triangle at its midpoint.
///
/// Cheetah3D's Polygon Reduction takes a target percentage; its manual page is
/// not among the indexed sources, so this is the clean-room reading: a
/// deterministic greedy collapse. Only edges that belong to a triangle are
/// candidates, because collapsing one of those always removes at least one
/// face, which is what makes the loop terminate instead of just turning quads
/// into triangles. `collapse_points` keeps the midpoint and carries creases,
/// seams and pinned UVs across the merge, so the reduced mesh still validates.
/// The mesh is returned unchanged when the ratio is not in 0..1, when it has no
/// triangles, or when the target cannot be reached.
pub fn reduce_polygons(mesh: &Mesh, ratio: f32) -> Mesh {
    let mut out = mesh.clone();
    if !ratio.is_finite() || ratio <= 0.0 || ratio >= 1.0 || mesh.polygons.is_empty() {
        return out;
    }
    let target = ((mesh.polygons.len() as f32 * ratio).round() as usize).max(1);
    let mut attempts = 0usize;
    let limit = mesh.polygons.len() * 2 + 16;
    while out.polygons.len() > target && attempts < limit {
        attempts += 1;
        let mut triangles: HashSet<(u32, u32)> = HashSet::new();
        for poly in &out.polygons {
            let count = poly.corners.len();
            if count != 3 {
                continue;
            }
            for index in 0..count {
                let a = poly.corners[index].point;
                let b = poly.corners[(index + 1) % count].point;
                if a != b
                    && (a as usize) < out.positions.len()
                    && (b as usize) < out.positions.len()
                {
                    triangles.insert((a.min(b), a.max(b)));
                }
            }
        }
        let mut candidates: Vec<(f32, u32, u32)> = triangles
            .iter()
            .map(|(a, b)| {
                let length = (out.positions[*a as usize] - out.positions[*b as usize]).length();
                (if length.is_finite() { length } else { f32::MAX }, *a, *b)
            })
            .collect();
        candidates.sort_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then(left.1.cmp(&right.1))
                .then(left.2.cmp(&right.2))
        });
        let Some(&(_, a, b)) = candidates.first() else {
            break;
        };
        out = collapse_points(&out, &[a, b]).0;
    }
    out
}

/// Rotate every point around the target object's `axis` so the source's extent
/// along that axis becomes a rotation angle.
///
/// Cheetah3D's Wrap modifier takes a target object; its manual page is not
/// among the indexed sources, so this is the clean-room reading: the source's
/// position along `axis` maps to an angle of up to `angle_deg / 2` at each end
/// around the target's centre line, which curls the source around the target
/// while keeping each point's distance from that line. A zero angle, an empty
/// target and a degenerate extent all return the mesh unchanged.
pub fn wrap_mesh(mesh: &Mesh, target: &Mesh, axis: usize, angle_deg: f32) -> Mesh {
    let mut out = mesh.clone();
    let source_bounds = mesh.bounds();
    let target_bounds = target.bounds();
    if !angle_deg.is_finite() || angle_deg == 0.0 {
        return out;
    }
    if !source_bounds.is_valid() || !target_bounds.is_valid() {
        return out;
    }
    let axis = axis.min(2);
    let (u, v) = match axis {
        0 => (1, 2),
        1 => (2, 0),
        _ => (0, 1),
    };
    let center = target_bounds.center();
    let extent = source_bounds.size()[axis].max(1e-6);
    let total = angle_deg.to_radians();
    for point in &mut out.positions {
        let t = (point[axis] - source_bounds.min[axis]) / extent - 0.5;
        let angle = t * total;
        let du = point[u] - center[u];
        let dv = point[v] - center[v];
        point[u] = center[u] + du * angle.cos() - dv * angle.sin();
        point[v] = center[v] + du * angle.sin() + dv * angle.cos();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{primitives, uv};

    /// A flat 3×3 point grid with four quads and a raised centre and corner, so
    /// the smoothing and reduction tests have an open border and a bump.
    fn bumpy_quad_grid() -> Mesh {
        let mut mesh = Mesh::new("grid");
        for z in 0..3 {
            for x in 0..3 {
                mesh.add_point(Vec3::new(x as f32, 0.0, z as f32));
            }
        }
        for z in 0..2 {
            for x in 0..2 {
                let start = (z * 3 + x) as u32;
                mesh.add_face(&[start, start + 1, start + 4, start + 3]);
            }
        }
        mesh.positions[4].y = 1.0;
        mesh.positions[0].y = 1.0;
        mesh
    }

    #[test]
    fn displacement_moves_points_along_normals_deterministically() {
        let source = primitives::sphere_mesh(1.0, 16, 8);
        let displaced = displace(&source, 0.2, 2.0, 7);
        assert_eq!(displaced.positions.len(), source.positions.len());
        assert_eq!(displaced.polygons, source.polygons);
        assert!(displaced.positions.iter().all(|point| point.is_finite()));
        assert!(displaced.validate().is_ok());
        assert_ne!(displaced.positions, source.positions);
        for (before, after) in source.positions.iter().zip(&displaced.positions) {
            assert!((*after - *before).length() <= 0.2 + 1e-4);
        }
        assert_eq!(
            displaced.positions,
            displace(&source, 0.2, 2.0, 7).positions
        );
        assert_ne!(
            displaced.positions,
            displace(&source, 0.2, 2.0, 8).positions
        );
        assert_eq!(displace(&source, 0.0, 2.0, 7).positions, source.positions);
    }

    #[test]
    fn formula_evaluates_per_axis_and_rejects_malformed_input() {
        let mut mesh = Mesh::new("formula");
        mesh.add_point(Vec3::new(1.0, 2.0, 3.0));
        mesh.add_point(Vec3::new(-2.0, 0.5, 0.0));

        let moved = formula(&mesh, "x + 1", "y * 2", "sqrt(16)").unwrap();
        assert_eq!(moved.positions[0], Vec3::new(2.0, 4.0, 4.0));
        assert_eq!(moved.positions[1], Vec3::new(-1.0, 1.0, 4.0));

        let precedence = formula(&mesh, "", "", "x + 2 * 3 ^ 2").unwrap();
        assert_eq!(precedence.positions[0], Vec3::new(1.0, 2.0, 19.0));

        // An empty expression and a non-finite result both keep the coordinate.
        let guarded = formula(&mesh, "", "", "1 / (x - x)").unwrap();
        assert_eq!(guarded.positions[0], mesh.positions[0]);

        let error = formula(&mesh, "x +", "", "").unwrap_err();
        assert!(
            error.contains("formula X") && error.contains("unexpected end"),
            "{error}"
        );
        assert!(formula(&mesh, "", "nope(1)", "").is_err());
        assert!(formula(&mesh, "", "min(x)", "").is_err());
        assert!(formula_expression_is_valid("min(x, 2) + max(y, 0)"));
        assert!(!formula_expression_is_valid("min(x)"));
        assert!(formula_expression_is_valid(""));
    }

    #[test]
    fn magnet_pulls_nearer_points_further_and_respects_the_radius() {
        let mesh = Mesh::from_polygons(
            "magnet",
            vec![
                Vec3::new(0.25, 0.0, 0.0),
                Vec3::new(0.75, 0.0, 0.0),
                Vec3::new(3.0, 0.0, 0.0),
            ],
            vec![vec![0, 1, 2]],
        );
        let pulled = magnet(&mesh, 1.0, 1.0, Vec3::ZERO);
        let near = pulled.positions[0].length();
        let far = pulled.positions[1].length();
        assert!(near < far && far < 0.75, "near {near} far {far}");
        assert_eq!(pulled.positions[2], Vec3::new(3.0, 0.0, 0.0));

        let pushed = magnet(&mesh, -1.0, 1.0, Vec3::ZERO);
        assert!(pushed.positions[0].length() > 0.25);
        assert_eq!(
            magnet(&mesh, 1.0, 0.0, Vec3::ZERO).positions,
            mesh.positions
        );
    }

    #[test]
    fn laplacian_smooth_relaxes_the_interior_and_can_hold_the_border() {
        let grid = bumpy_quad_grid();
        let smoothed = laplacian_smooth(&grid, 1, 1.0, true);
        assert!(smoothed.positions[4].y < 1.0 - 1e-3);
        assert_eq!(smoothed.positions[0].y, 1.0, "the corner is a border point");
        assert!(smoothed.positions.iter().all(|point| point.is_finite()));
        assert_eq!(smoothed.polygons, grid.polygons);

        let loose = laplacian_smooth(&grid, 1, 1.0, false);
        assert!(loose.positions[0].y.abs() < 1e-6);
        assert_eq!(
            smoothed.positions[4],
            laplacian_smooth(&grid, 1, 1.0, true).positions[4]
        );
        assert_eq!(
            laplacian_smooth(&grid, 0, 1.0, true).positions,
            grid.positions
        );
    }

    #[test]
    fn polygon_reduction_halves_a_triangulated_grid() {
        let grid = bumpy_quad_grid();
        let triangles = triangulate_faces(&grid, &[0, 1, 2, 3]);
        assert_eq!(triangles.polygons.len(), 8);
        let reduced = reduce_polygons(&triangles, 0.5);
        assert!(reduced.polygons.len() < triangles.polygons.len());
        assert!(reduced.polygons.len() <= 4, "{}", reduced.polygons.len());
        assert!(reduced.validate().is_ok());
        assert_eq!(
            reduced.positions.len(),
            reduce_polygons(&triangles, 0.5).positions.len()
        );
        // A quad-only cage has no triangle edge to collapse, so it is unchanged.
        assert_eq!(reduce_polygons(&grid, 0.5).polygons, grid.polygons);
        assert_eq!(
            reduce_polygons(&triangles, 1.0).polygons,
            triangles.polygons
        );
    }

    #[test]
    fn wrap_bends_the_source_around_the_target_axis() {
        let source = Mesh::from_polygons(
            "source",
            vec![
                Vec3::new(-1.0, 3.0, 0.0),
                Vec3::new(0.0, 3.0, 0.0),
                Vec3::new(1.0, 3.0, 0.0),
            ],
            vec![vec![0, 1, 2]],
        );
        let target = Mesh::from_polygons(
            "target",
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            vec![vec![0, 1, 2]],
        );
        let center = target.bounds().center();
        let wrapped = wrap_mesh(&source, &target, 0, 90.0);
        assert!(wrapped.positions.iter().all(|point| point.is_finite()));
        assert_ne!(wrapped.positions, source.positions);
        // The source's middle stays put and its ends swing around the target; a
        // point keeps its distance from the target's centre line.
        assert_eq!(wrapped.positions[1], source.positions[1]);
        for (before, after) in source.positions.iter().zip(&wrapped.positions) {
            let radius_before = (*before - center).length();
            let radius_after = (*after - center).length();
            assert!((radius_before - radius_after).abs() < 1e-4);
        }
        assert!((wrapped.positions[0] - source.positions[0]).length() > 1e-3);
        assert_eq!(
            wrap_mesh(&source, &target, 0, 0.0).positions,
            source.positions
        );
    }

    fn flat_quad() -> Mesh {
        Mesh::from_polygons(
            "quad",
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(0.0, 0.0, 1.0),
            ],
            vec![vec![0, 1, 2, 3]],
        )
    }

    fn two_quad_strip() -> Mesh {
        Mesh::from_polygons(
            "strip",
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(2.0, 0.0, 1.0),
            ],
            vec![vec![0, 1, 2, 3], vec![1, 4, 5, 2]],
        )
    }

    #[test]
    fn scalpel_cuts_one_polygon_between_two_edges() {
        let quad = flat_quad();
        let cut = scalpel_face(
            &quad,
            0,
            ScalpelPick::Edge { a: 0, b: 1, t: 0.5 },
            ScalpelPick::Edge { a: 3, b: 2, t: 0.5 },
        )
        .unwrap();
        assert_eq!(cut.polygons.len(), 2);
        assert_eq!(cut.positions.len(), 6);
        assert!(cut.validate().is_ok());
        assert_eq!(cut.polygons[0].corners.len(), 4);
        assert_eq!(cut.polygons[1].corners.len(), 4);
        // The two ends of the cut sit on the midpoints and both faces share them.
        assert_eq!(cut.positions[4], Vec3::new(0.5, 0.0, 0.0));
        assert_eq!(cut.positions[5], Vec3::new(0.5, 0.0, 1.0));
        for point in [4u32, 5] {
            for poly in &cut.polygons {
                assert!(poly.corners.iter().any(|corner| corner.point == point));
            }
        }
    }

    #[test]
    fn scalpel_face_ends_reports_the_points_a_chained_cut_continues_from() {
        let quad = flat_quad();
        // The edge pick inserts point 4 and the corner pick keeps point 2, so a
        // chained cut needs exactly those indices, in from/to order.
        let (cut, from, to) = scalpel_face_ends(
            &quad,
            0,
            ScalpelPick::Edge { a: 0, b: 1, t: 0.5 },
            ScalpelPick::Point(2),
        )
        .unwrap();
        assert_eq!((from, to), (4, 2));
        assert_eq!(cut.positions[from as usize], Vec3::new(0.5, 0.0, 0.0));
        // The reported ends are corners of both new faces, which is what lets
        // the next cut start on either half.
        for point in [from, to] {
            for poly in &cut.polygons {
                assert!(poly.corners.iter().any(|corner| corner.point == point));
            }
        }
        // Reversed picks report the ends in their own order.
        let (_, from, to) = scalpel_face_ends(
            &quad,
            0,
            ScalpelPick::Point(2),
            ScalpelPick::Edge { a: 0, b: 1, t: 0.5 },
        )
        .unwrap();
        assert_eq!((from, to), (2, 4));
    }

    #[test]
    fn scalpel_corner_to_edge_makes_a_triangle_and_a_quad() {
        let quad = flat_quad();
        let cut = scalpel_face(
            &quad,
            0,
            ScalpelPick::Point(0),
            ScalpelPick::Edge { a: 2, b: 3, t: 0.5 },
        )
        .unwrap();
        assert!(cut.validate().is_ok());
        assert_eq!(cut.polygons.len(), 2);
        assert_eq!(cut.polygons[0].corners.len(), 4);
        assert_eq!(cut.polygons[1].corners.len(), 3);
        // Both faces keep the authored material slot.
        assert_eq!(
            cut.polygons[0].material_slot,
            quad.polygons[0].material_slot
        );
        assert_eq!(
            cut.polygons[1].material_slot,
            quad.polygons[0].material_slot
        );
    }

    #[test]
    fn scalpel_splits_shared_edges_and_keeps_seams_and_creases() {
        let strip = two_quad_strip();
        let mut marked = strip.clone();
        assert!(marked.set_seam(1, 2, true));
        assert!(marked.set_crease_weight(1, 2, 0.5));
        let cut = scalpel_face(
            &marked,
            0,
            ScalpelPick::Edge { a: 1, b: 2, t: 0.5 },
            ScalpelPick::Edge { a: 3, b: 0, t: 0.5 },
        )
        .unwrap();
        assert!(cut.validate().is_ok());
        let inserted = cut
            .positions
            .iter()
            .position(|point| (*point - Vec3::new(1.0, 0.0, 0.5)).length() < 1e-6)
            .expect("the cut adds the midpoint of the shared edge") as u32;
        // The neighbour keeps its corner count plus the split point, so the
        // shared edge is split the same way on both sides.
        assert_eq!(cut.polygons[1].corners.len(), 5);
        for (a, b) in [(1, inserted), (inserted, 2)] {
            let edge = cut
                .edges()
                .into_iter()
                .find(|edge| (edge.a == a && edge.b == b) || (edge.a == b && edge.b == a))
                .expect("the split edge exists");
            assert_eq!(edge.faces.len(), 2, "the split edge stays interior");
        }
        let cut_edge = cut
            .edges()
            .into_iter()
            .find(|edge| edge.a >= 6 && edge.b >= 6)
            .expect("the cut edge exists");
        assert_eq!(cut_edge.faces.len(), 2);
        // Seam and crease weight follow both halves of the split edge.
        assert!(!cut.is_seam(1, 2));
        assert!(cut.is_seam(1, inserted) && cut.is_seam(inserted, 2));
        assert_eq!(cut.crease_weight(1, inserted), 0.5);
        assert_eq!(cut.crease_weight(inserted, 2), 0.5);
    }

    #[test]
    fn scalpel_rejects_picks_outside_the_polygon_and_degenerate_cuts() {
        let quad = flat_quad();
        let cases = [
            (ScalpelPick::Point(9), ScalpelPick::Point(0)),
            (
                ScalpelPick::Edge { a: 0, b: 2, t: 0.5 },
                ScalpelPick::Edge { a: 1, b: 3, t: 0.5 },
            ),
            (
                ScalpelPick::Edge { a: 0, b: 1, t: 1.5 },
                ScalpelPick::Edge { a: 2, b: 3, t: 0.5 },
            ),
            (ScalpelPick::Point(1), ScalpelPick::Point(1)),
            (ScalpelPick::Point(0), ScalpelPick::Point(1)),
        ];
        for (from, to) in cases {
            assert!(
                scalpel_face(&quad, 0, from, to).is_err(),
                "{from:?} to {to:?} should be refused"
            );
        }
        assert!(scalpel_face(&quad, 4, ScalpelPick::Point(0), ScalpelPick::Point(2)).is_err());
    }

    #[test]
    fn triangulate_splits_only_the_selected_polygons() {
        let mut mesh = primitives::box_mesh(1.0, 1.0, 1.0);
        uv::box_projection(&mut mesh, 1.0);
        let quad = mesh.polygons[0].clone();
        mesh.polygons[0].material_slot = 2;
        mesh.polygons[1].material_slot = 3;
        assert!(mesh.set_crease(quad.corners[0].point, quad.corners[1].point, true));
        assert!(mesh.set_seam(quad.corners[0].point, quad.corners[1].point, true));
        let positions = mesh.positions.clone();
        let uvs = mesh.uvs.clone();

        // An out-of-range index is ignored instead of panicking.
        let out = triangulate_faces(&mesh, &[0, 99]);
        assert!(out.validate().is_ok());
        assert_eq!(out.polygons.len(), 7, "one quad becomes two triangles");
        assert_eq!(
            out.positions, positions,
            "triangulating must not move points"
        );
        assert_eq!(out.uvs, uvs, "corner UV slots are reused, not rebuilt");
        assert_eq!(out.creases.len(), 1, "the crease did not survive");
        assert_eq!(out.seams.len(), 1, "the seam did not survive");
        assert_eq!(
            out.polygons[2].corners.len(),
            4,
            "an unselected polygon keeps its corners"
        );
        assert_eq!(out.polygons[2].material_slot, 3, "slots follow the polygon");
        let triangles: Vec<&Polygon> = out
            .polygons
            .iter()
            .filter(|polygon| polygon.material_slot == 2)
            .collect();
        assert_eq!(triangles.len(), 2);
        for triangle in &triangles {
            assert_eq!(triangle.corners.len(), 3);
        }
        let covered: Vec<u32> = triangles
            .iter()
            .flat_map(|triangle| triangle.corners.iter().map(|corner| corner.point))
            .collect();
        for corner in &quad.corners {
            assert!(
                covered.contains(&corner.point),
                "triangulation dropped point {}",
                corner.point
            );
        }
        // The two triangles share exactly one edge: the new diagonal.
        let diagonal = (triangles[0].corners[0].point, triangles[0].corners[2].point);
        assert!(
            triangles[1]
                .corners
                .iter()
                .filter(|corner| { corner.point == diagonal.0 || corner.point == diagonal.1 })
                .count()
                == 2,
            "the split triangles do not share the diagonal"
        );
        assert!(
            out.creases
                .iter()
                .all(|edge| !(edge.contains(&diagonal.0) && edge.contains(&diagonal.1))),
            "the new diagonal must not inherit a crease"
        );

        // Selecting nothing leaves every n-gon alone.
        let untouched = triangulate_faces(&mesh, &[]);
        assert_eq!(untouched.polygons.len(), mesh.polygons.len());
        assert_eq!(untouched.polygons[0].corners.len(), 4);
    }

    #[test]
    fn triangulation_modes_choose_different_diagonals() {
        // A skewed quad whose 1-3 diagonal is shorter than 0-2. Shortest
        // Diagonal must pick 1-3 instead of the old fixed fan from corner 0.
        let mut mesh = Mesh::new("Skew");
        mesh.positions = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.1, 3.0, 0.0),
            Vec3::new(0.0, 3.0, 0.0),
        ];
        mesh.add_face(&[0, 1, 2, 3]);
        let shortest = triangulate_faces_with(&mesh, &[0], TriangulateMode::ShortestDiagonal);
        assert_eq!(shortest.polygons.len(), 2);
        let points: Vec<Vec<u32>> = shortest
            .polygons
            .iter()
            .map(|polygon| polygon.corners.iter().map(|corner| corner.point).collect())
            .collect();
        assert!(points.iter().all(|triangle| triangle.len() == 3));
        assert!(
            points
                .iter()
                .any(|triangle| triangle.contains(&1) && triangle.contains(&3)),
            "ShortestDiagonal should split along the 1-3 diagonal: {points:?}"
        );
        assert!(
            !points
                .iter()
                .any(|triangle| triangle.contains(&0) && triangle.contains(&2)),
            "the longer 0-2 diagonal must not be used: {points:?}"
        );
        let paired = triangulate_faces_with(&mesh, &[0], TriangulateMode::QuadPair);
        assert_eq!(paired.polygons.len(), 2);
        assert_eq!(paired.polygons[0].corners[0].point, 1);
        assert_eq!(paired.polygons[0].corners[2].point, 3);
        assert!(shortest.validate().is_ok());
        assert!(paired.validate().is_ok());
    }

    #[test]
    fn collapse_points_welds_the_selection_into_one_vertex() {
        let mut mesh = primitives::box_mesh(1.0, 1.0, 1.0);
        uv::box_projection(&mut mesh, 1.0);
        let quad = mesh.polygons[0].corners.clone();
        let collapsed = [quad[0].point, quad[1].point, quad[2].point];
        let centroid = collapsed
            .iter()
            .map(|p| mesh.positions[*p as usize])
            .sum::<Vec3>()
            / collapsed.len() as f32;
        let untouched: Vec<(Vec3, u32)> = mesh
            .positions
            .iter()
            .enumerate()
            .filter(|(i, _)| !collapsed.contains(&(*i as u32)))
            .map(|(_, p)| (*p, 0))
            .collect();
        // A crease and a seam between two surviving points, a pin on a corner
        // of a face that survives, and a pin on a corner the collapse deletes.
        let kept_corner = quad[3];
        let kept_neighbour = mesh.polygons[1].corners[2];
        let kept_uv = kept_neighbour.uv;
        assert!(mesh.set_crease(kept_corner.point, kept_neighbour.point, true));
        assert!(mesh.set_seam(kept_corner.point, kept_neighbour.point, true));
        assert!(mesh.set_uv_pinned(kept_uv, true));
        assert!(mesh.set_uv_pinned(quad[0].uv, true));

        let (out, survivor) =
            collapse_points(&mesh, &[quad[0].point, quad[1].point, quad[2].point]);
        let survivor = survivor.expect("the merged point survives in the remaining faces");
        assert!(out.validate().is_ok());
        assert_eq!(
            out.positions.len(),
            mesh.positions.len() - 2,
            "three points become one"
        );
        assert_eq!(
            out.positions[survivor as usize], centroid,
            "the surviving point sits at the centroid"
        );
        for (position, _) in &untouched {
            assert!(
                out.positions
                    .iter()
                    .any(|p| (*p - *position).length() < 1e-6),
                "unselected point {position:?} moved or vanished"
            );
        }
        assert_eq!(
            out.polygons.len(),
            mesh.polygons.len() - 1,
            "the face that collapsed to two corners is removed"
        );
        for (index, poly) in out.polygons.iter().enumerate() {
            let mut points: Vec<u32> = poly.corners.iter().map(|c| c.point).collect();
            points.sort_unstable();
            points.dedup();
            assert_eq!(
                points.len(),
                poly.corners.len(),
                "polygon {index} repeats a point"
            );
            assert!(poly.corners.len() >= 3);
            for corner in &poly.corners {
                assert!((corner.point as usize) < out.positions.len());
                assert!((corner.uv as usize) < out.uvs.len());
            }
        }
        assert_eq!(
            out.creases.len(),
            1,
            "the crease follows the surviving points"
        );
        assert_eq!(out.seams.len(), 1, "the seam follows the surviving points");
        assert!(
            out.is_uv_pinned(kept_uv),
            "a pin on a surviving corner is kept"
        );
        assert!(
            !out.is_uv_pinned(quad[0].uv),
            "a pin on a dropped corner is cleaned up"
        );

        // Opposite corners of one quad leave a repeated point, not a pinched face.
        let pinched = primitives::box_mesh(1.0, 1.0, 1.0);
        let quad = pinched.polygons[0].corners.clone();
        let (out, _) = collapse_points(&pinched, &[quad[0].point, quad[2].point]);
        assert!(out.validate().is_ok());
        assert_eq!(out.polygons.len(), pinched.polygons.len() - 1);
        for poly in &out.polygons {
            let mut points: Vec<u32> = poly.corners.iter().map(|c| c.point).collect();
            points.sort_unstable();
            points.dedup();
            assert_eq!(points.len(), poly.corners.len());
        }

        // Nothing selected, or a single point, is a no-op with no survivor.
        assert!(collapse_points(&mesh, &[]).1.is_none());
        assert_eq!(collapse_points(&mesh, &[]).0.signature(), mesh.signature());
        assert!(collapse_points(&mesh, &[quad[0].point]).1.is_none());
        assert_eq!(
            collapse_points(&mesh, &[99, 98]).0.signature(),
            mesh.signature()
        );
    }

    #[test]
    fn disconnect_faces_detaches_the_selection_without_moving_it() {
        let mut mesh = primitives::box_mesh(1.0, 1.0, 1.0);
        uv::box_projection(&mut mesh, 1.0);
        let face = mesh.polygons[0].clone();
        let original_positions = mesh.positions.clone();
        let original_uvs = mesh.uvs.clone();
        let others: Vec<Polygon> = mesh.polygons[1..].to_vec();
        // A crease and a seam across a boundary edge of the selection, a crease
        // on an untouched face, and a pin on a corner of the detached face.
        let (a, b) = (face.corners[0].point, face.corners[1].point);
        let far = mesh.polygons[4].clone();
        assert!(mesh.set_crease(a, b, true));
        assert!(mesh.set_seam(a, b, true));
        assert!(mesh.set_crease(far.corners[0].point, far.corners[1].point, true));
        let pinned_uv = face.corners[0].uv;
        assert!(mesh.set_uv_pinned(pinned_uv, true));
        assert!(can_disconnect(&mesh, &[0]));

        let out = disconnect_faces(&mesh, &[0, 0, 99]);
        assert!(out.validate().is_ok());
        assert_eq!(
            out.positions.len(),
            mesh.positions.len() + face.corners.len(),
            "one copy per point on the cut"
        );
        assert_eq!(
            out.polygons.len(),
            mesh.polygons.len(),
            "no face is added or lost"
        );
        assert_eq!(
            out.polygons[1..].to_vec(),
            others,
            "unselected faces are untouched"
        );
        assert_eq!(out.uvs, original_uvs, "UV slots are shared, not rebuilt");
        assert!(
            out.is_uv_pinned(pinned_uv),
            "the pin still points at a live slot"
        );
        assert_eq!(
            out.polygons[0].material_slot, face.material_slot,
            "the material slot follows the face"
        );

        // The detached face keeps its positions and winding, but on new points.
        let detached = &out.polygons[0];
        assert_eq!(detached.corners.len(), face.corners.len());
        for (corner, original) in detached.corners.iter().zip(face.corners.iter()) {
            assert_ne!(
                corner.point, original.point,
                "a cut point must be duplicated, not shared"
            );
            assert_eq!(
                out.positions[corner.point as usize], original_positions[original.point as usize],
                "duplication must not move a point"
            );
            assert_eq!(corner.uv, original.uv, "the corner keeps its UV slot");
        }
        // No unused or out-of-range point after the rebuild.
        let mut used = vec![false; out.positions.len()];
        for poly in &out.polygons {
            for corner in &poly.corners {
                assert!((corner.point as usize) < out.positions.len());
                assert!((corner.uv as usize) < out.uvs.len());
                used[corner.point as usize] = true;
            }
        }
        assert!(
            used.iter().all(|used| *used),
            "the rebuild left an unused point"
        );

        // Creases and seams exist on both sides of the cut.
        let mapped = |point: u32| {
            detached
                .corners
                .iter()
                .find(|c| {
                    out.positions[c.point as usize].distance(original_positions[point as usize])
                        < 1e-6
                })
                .map(|c| c.point)
                .unwrap()
        };
        let (a2, b2) = (mapped(a), mapped(b));
        assert!(out.is_crease(a, b), "the unselected side keeps the crease");
        assert!(
            out.is_crease(a2, b2),
            "the detached side gets its own crease"
        );
        assert!(out.is_seam(a, b) && out.is_seam(a2, b2));
        assert!(
            out.is_crease(far.corners[0].point, far.corners[1].point),
            "creases inside unselected faces are untouched"
        );

        // Whole mesh, empty and already-separate selections cannot disconnect.
        let all: Vec<u32> = (0..mesh.polygons.len() as u32).collect();
        assert!(!can_disconnect(&mesh, &all));
        assert_eq!(disconnect_faces(&mesh, &all).signature(), mesh.signature());
        assert!(!can_disconnect(&mesh, &[]));
        assert_eq!(disconnect_faces(&mesh, &[]).signature(), mesh.signature());
        assert_eq!(disconnect_faces(&mesh, &[99]).signature(), mesh.signature());
    }

    #[test]
    fn cover_faces_closes_the_hole_the_selection_borders() {
        // A box with one face deleted is a single four-sided hole.
        let mut mesh = primitives::box_mesh(1.0, 1.0, 1.0);
        uv::box_projection(&mut mesh, 1.0);
        mesh.polygons.remove(0);
        for polygon in &mut mesh.polygons {
            polygon.material_slot = 3;
        }
        let border_points = {
            let holes = mesh.boundary_loops();
            assert_eq!(holes.len(), 1, "the box has one hole");
            holes[0].clone()
        };
        let pinned = mesh.polygons[0].corners[0].uv;
        let (crease_a, crease_b) = (
            mesh.polygons[0].corners[0].point,
            mesh.polygons[0].corners[1].point,
        );
        let (seam_a, seam_b) = (
            mesh.polygons[1].corners[0].point,
            mesh.polygons[1].corners[1].point,
        );
        assert!(mesh.set_crease(crease_a, crease_b, true));
        assert!(mesh.set_seam(seam_a, seam_b, true));
        assert!(mesh.set_uv_pinned(pinned, true));
        let positions = mesh.positions.clone();
        let uvs = mesh.uvs.clone();
        let polygons = mesh.polygons.clone();
        let all: Vec<u32> = (0..polygons.len() as u32).collect();

        // A selection that only partly borders the hole cannot cover it.
        assert!(coverable_holes(&mesh, &[0]).is_empty());
        assert!(cover_faces(&mesh, &[0]).is_err());

        let holes = coverable_holes(&mesh, &all);
        assert_eq!(holes.len(), 1);
        assert_eq!(holes[0].len(), 4, "the hole has four border points");
        let (out, created) = cover_faces(&mesh, &all).expect("the hole is coverable");
        assert_eq!(created, vec![polygons.len() as u32]);
        assert!(out.validate().is_ok());
        assert_eq!(out.positions, positions, "cover must not move a point");
        assert_eq!(
            out.polygons[..polygons.len()].to_vec(),
            polygons,
            "existing faces keep their corners, UV slots and slots"
        );
        assert_eq!(
            &out.uvs[..uvs.len()],
            &uvs[..],
            "existing UV slots are reused"
        );
        assert_eq!(
            out.uvs.len(),
            uvs.len() + 4,
            "the new face gets its own UVs"
        );
        assert!(out.uvs[uvs.len()..].iter().all(|uv| uv.is_finite()));
        assert!(out.is_crease(crease_a, crease_b));
        assert!(out.is_seam(seam_a, seam_b));
        assert!(out.is_uv_pinned(pinned));

        // The new face reuses the existing border points, closes the mesh and
        // inherits the bordering material slot.
        let face = &out.polygons[created[0] as usize];
        assert_eq!(face.corners.len(), 4);
        assert_eq!(face.material_slot, 3);
        let mut face_points: Vec<u32> = face.corners.iter().map(|c| c.point).collect();
        face_points.sort_unstable();
        let mut expected = border_points.clone();
        expected.sort_unstable();
        assert_eq!(
            face_points, expected,
            "the fill reuses the existing border points"
        );
        assert!(face_points.iter().all(|p| (*p as usize) < positions.len()));
        for corner in &face.corners {
            assert!((corner.uv as usize) < out.uvs.len());
        }
        assert!(out.boundary_loops().is_empty(), "the hole is watertight");
        assert!(
            out.edges().iter().all(|edge| edge.faces.len() == 2),
            "every edge is shared by two faces"
        );
        assert!(
            (out.signed_volume() - 1.0).abs() < 1e-4,
            "filling the hole restores the unit box outward: {}",
            out.signed_volume()
        );

        // Nothing to cover: empty selection or an already closed mesh.
        assert!(coverable_holes(&mesh, &[]).is_empty());
        assert!(cover_faces(&mesh, &[]).is_err());
        let closed = primitives::box_mesh(1.0, 1.0, 1.0);
        let closed_faces: Vec<u32> = (0..closed.polygons.len() as u32).collect();
        assert!(coverable_holes(&closed, &closed_faces).is_empty());
        assert!(cover_faces(&closed, &closed_faces).is_err());
    }

    #[test]
    fn create_polygon_faces_closes_the_selected_boundary_loops() {
        // A box with one face deleted: one four-point hole to close.
        let mut mesh = primitives::box_mesh(1.0, 1.0, 1.0);
        uv::box_projection(&mut mesh, 1.0);
        mesh.polygons.remove(0);
        for polygon in &mut mesh.polygons {
            polygon.material_slot = 3;
        }
        let loops = mesh.boundary_loops();
        assert_eq!(loops.len(), 1);
        let hole = loops[0].clone();
        let positions = mesh.positions.clone();
        let polygons = mesh.polygons.clone();
        let uvs = mesh.uvs.clone();
        // The point's existing UV slot is pinned, so its new corner should be too.
        let source_point = hole[0];
        let source = mesh
            .polygons
            .iter()
            .flat_map(|poly| poly.corners.iter())
            .find(|corner| corner.point == source_point)
            .cloned()
            .expect("the hole point is used by the surviving faces");
        let source_uv = mesh.uv_of(&source);
        assert!(mesh.set_uv_pinned(source.uv, true));

        // Click order must not matter: hand the points over reversed.
        let mut selected = hole.clone();
        selected.reverse();
        assert!(faceable_loops(&mesh, &selected[..2]).is_empty());
        assert!(create_polygon_faces(&mesh, &selected[..2]).is_err());
        assert!(faceable_loops(&mesh, &hole[..3]).is_empty());

        let (out, created) = create_polygon_faces(&mesh, &selected).expect("the hole is faceable");
        assert_eq!(created, vec![polygons.len() as u32]);
        assert!(out.validate().is_ok());
        assert_eq!(out.positions, positions, "no point is created or moved");
        assert_eq!(
            out.polygons[..polygons.len()].to_vec(),
            polygons,
            "existing faces keep their corners, UV slots and slots"
        );
        assert_eq!(
            &out.uvs[..uvs.len()],
            &uvs[..],
            "existing UV slots are reused"
        );
        assert_eq!(out.uvs.len(), uvs.len() + hole.len());
        let face = &out.polygons[created[0] as usize];
        assert_eq!(face.material_slot, 3, "the bordering slot is inherited");
        assert_eq!(face.corners.len(), hole.len());
        let mut face_points: Vec<u32> = face.corners.iter().map(|c| c.point).collect();
        face_points.sort_unstable();
        let mut expected = hole.clone();
        expected.sort_unstable();
        assert_eq!(
            face_points, expected,
            "the new face reuses the existing point indices"
        );
        let hole_edges: HashSet<[u32; 2]> = (0..hole.len())
            .map(|i| {
                let (a, b) = (hole[i], hole[(i + 1) % hole.len()]);
                if a < b {
                    [a, b]
                } else {
                    [b, a]
                }
            })
            .collect();
        for (i, corner) in face.corners.iter().enumerate() {
            let next = face.corners[(i + 1) % face.corners.len()].point;
            let edge = if corner.point < next {
                [corner.point, next]
            } else {
                [next, corner.point]
            };
            assert!(
                hole_edges.contains(&edge),
                "the face follows the hole's adjacency, not the click order"
            );
        }
        let new_corner = face
            .corners
            .iter()
            .find(|corner| corner.point == source_point)
            .expect("the point is on the new face");
        assert_eq!(
            out.uv_of(new_corner),
            source_uv,
            "the new corner inherits its UV"
        );
        assert!(
            out.is_uv_pinned(new_corner.uv),
            "the pin is inherited with the UV"
        );
        assert!(out.boundary_loops().is_empty(), "the hole is closed");
        assert!(
            (out.signed_volume() - 1.0).abs() < 1e-4,
            "the fill restores the solid outward: {}",
            out.signed_volume()
        );

        // The hole is gone, so the same selection no longer creates anything.
        assert!(create_polygon_faces(&out, &selected).is_err());

        // Two separate holes create two faces; a partly selected one is skipped.
        let mut two = primitives::box_mesh(1.0, 1.0, 1.0);
        uv::box_projection(&mut two, 1.0);
        let mut removed = None;
        'pair: for a in 0..two.polygons.len() {
            for b in (a + 1)..two.polygons.len() {
                let mut trial = two.clone();
                trial.polygons.remove(b);
                trial.polygons.remove(a);
                if trial.boundary_loops().len() == 2 {
                    removed = Some((a, b));
                    break 'pair;
                }
            }
        }
        let (a, b) = removed.expect("some two faces leave two separate holes");
        two.polygons.remove(b);
        two.polygons.remove(a);
        let holes = two.boundary_loops();
        assert_eq!(holes.len(), 2);
        let mut both: Vec<u32> = holes.iter().flatten().copied().collect();
        both.sort_unstable();
        both.dedup();
        let (filled, created) = create_polygon_faces(&two, &both).expect("both holes are faceable");
        assert_eq!(created.len(), 2, "each closed run becomes one face");
        assert!(filled.boundary_loops().is_empty());
        assert!((filled.signed_volume() - 1.0).abs() < 1e-4);

        let mut partial = holes[0].clone();
        partial.extend(holes[1][..3].iter().copied());
        let (half, created) =
            create_polygon_faces(&two, &partial).expect("the complete run counts");
        assert_eq!(created.len(), 1, "the partly selected run is skipped");
        assert_eq!(half.polygons.len(), two.polygons.len() + 1);
        assert_eq!(half.boundary_loops().len(), 1);

        // Nothing to create: empty selection or a mesh with no open region.
        assert!(faceable_loops(&mesh, &[]).is_empty());
        assert!(create_polygon_faces(&mesh, &[]).is_err());
        let closed = primitives::box_mesh(1.0, 1.0, 1.0);
        let all: Vec<u32> = (0..closed.positions.len() as u32).collect();
        assert!(faceable_loops(&closed, &all).is_empty());
        assert!(create_polygon_faces(&closed, &all).is_err());
    }

    #[test]
    fn optimize_points_welds_only_nearby_selected_points() {
        // Two quads that meet along a seam of near-duplicate points.
        let mut mesh = Mesh::new("optimize strip");
        mesh.positions = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(1.0004, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(1.0004, 0.0, 1.0),
            Vec3::new(2.0, 0.0, 1.0),
        ];
        mesh.uvs = (0..12).map(|i| Vec2::new(i as f32 * 0.1, 0.0)).collect();
        mesh.polygons.push(Polygon::new(vec![
            Corner::new(0, 0),
            Corner::new(2, 2),
            Corner::new(3, 3),
            Corner::new(1, 1),
        ]));
        mesh.polygons.push(Polygon::new(vec![
            Corner::new(4, 4),
            Corner::new(6, 6),
            Corner::new(7, 7),
            Corner::new(5, 5),
        ]));
        // A quad that collapses to a line once both pairs merge.
        mesh.polygons.push(Polygon::new(vec![
            Corner::new(1, 8),
            Corner::new(4, 9),
            Corner::new(6, 10),
            Corner::new(3, 11),
        ]));
        assert!(mesh.set_crease(0, 2, true));
        assert!(mesh.set_seam(1, 4, true));
        // One pin on a surviving face, one on the face the merge deletes.
        assert!(mesh.set_uv_pinned(0, true));
        assert!(mesh.set_uv_pinned(8, true));
        let positions = mesh.positions.clone();
        let first = mesh.polygons[0].clone();

        // Nothing to merge: too few points, no tolerance, or points too far apart.
        assert!(optimize_points(&mesh, &[], 0.001).is_none());
        assert!(optimize_points(&mesh, &[1], 0.001).is_none());
        assert!(optimize_points(&mesh, &[1, 4], 0.0).is_none());
        assert!(optimize_points(&mesh, &[1, 4], f32::NAN).is_none());
        assert!(optimize_points(&mesh, &[0, 2], 0.001).is_none());
        assert!(!can_optimize(&mesh, &[1, 4], 0.0001));
        assert!(can_optimize(&mesh, &[1, 4], 0.001));

        let (out, survivors) =
            optimize_points(&mesh, &[1, 3, 4, 6], 0.001).expect("the seam merges");
        assert!(out.validate().is_ok());
        assert_eq!(
            out.positions.len(),
            positions.len() - 2,
            "two duplicates fold away"
        );
        assert_eq!(
            out.polygons.len(),
            2,
            "the quad that collapses to a line is removed"
        );
        assert_eq!(survivors, vec![1, 3], "the selection follows the survivors");
        assert_eq!(out.positions[0], positions[0]);
        assert_eq!(
            out.positions[1], positions[1],
            "a survivor keeps its authored position"
        );
        assert_eq!(out.positions[2], positions[2]);
        assert_eq!(out.positions[3], positions[3]);
        assert_eq!(
            out.positions[4], positions[5],
            "an unselected point is untouched"
        );
        assert_eq!(out.positions[5], positions[7]);
        assert_eq!(
            out.polygons[0], first,
            "an untouched face keeps its corner record"
        );
        let seam = out
            .edges()
            .into_iter()
            .find(|edge| edge.a == 1 && edge.b == 3)
            .expect("the welded seam is an edge");
        assert_eq!(
            seam.faces.len(),
            2,
            "the weld joins both quads along the seam"
        );
        assert_eq!(
            out.boundary_loops().len(),
            1,
            "the flat strip keeps its single outer border"
        );
        for poly in &out.polygons {
            for corner in &poly.corners {
                assert!((corner.point as usize) < out.positions.len());
                assert!((corner.uv as usize) < out.uvs.len());
            }
        }
        assert!(out.is_crease(0, 2), "a crease away from the weld survives");
        assert!(
            !out.is_seam(1, 1),
            "a seam whose endpoints merged disappears"
        );
        assert!(out.is_uv_pinned(0), "a pin on a surviving face is kept");
        assert!(
            !out.is_uv_pinned(8),
            "a pin on a corner the merge deletes is cleaned up"
        );

        // Out-of-range and duplicate indices do not change the count of survivors.
        let (dup, survivors) = optimize_points(&mesh, &[4, 1, 4, 99], 0.001).unwrap();
        assert_eq!(
            dup.signature(),
            optimize_points(&mesh, &[1, 4], 0.001)
                .unwrap()
                .0
                .signature()
        );
        assert_eq!(survivors, vec![1]);
    }

    #[test]
    fn slide_points_moves_only_the_selection_toward_its_neighbours() {
        // Two quads sharing an edge: point 0 has exactly two neighbours and
        // point 1 has three, so both the midpoint and the generalized rule show.
        let mut mesh = Mesh::new("slide strip");
        mesh.positions = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(2.0, 0.0, 1.0),
        ];
        mesh.polygons.push(Polygon::new(vec![
            Corner::new(0, 0),
            Corner::new(3, 0),
            Corner::new(4, 0),
            Corner::new(1, 0),
        ]));
        mesh.polygons.push(Polygon::new(vec![
            Corner::new(1, 0),
            Corner::new(4, 0),
            Corner::new(5, 0),
            Corner::new(2, 0),
        ]));
        let positions = mesh.positions.clone();
        let polygons = mesh.polygons.clone();

        // Two neighbours: a full slide is exactly the segment midpoint, and a
        // half slide stops half way there.
        let out = slide_points(&mesh, &[0], 1.0).expect("point 0 can slide");
        assert_eq!(
            out.positions[0],
            Vec3::new(0.5, 0.0, 0.5),
            "a two-neighbour point lands on the segment midpoint"
        );
        let out = slide_points(&mesh, &[0], 0.5).expect("point 0 can slide");
        assert_eq!(
            out.positions[0],
            Vec3::new(0.25, 0.0, 0.25),
            "a half slide stops half way to the midpoint"
        );
        assert_eq!(out.positions.len(), positions.len());
        for (index, (actual, expected)) in out.positions.iter().zip(&positions).enumerate().skip(1)
        {
            assert_eq!(actual, expected, "point {index} moved");
        }
        assert_eq!(out.polygons, polygons, "faces keep their corner records");
        assert!(out.validate().is_ok());

        // Three neighbours use their centroid, measured before the move.
        let out = slide_points(&mesh, &[1], 0.5).expect("point 1 can slide");
        assert_eq!(out.positions[0], positions[0], "a neighbour is not moved");
        assert!((out.positions[1] - Vec3::new(1.0, 0.0, 1.0 / 6.0)).length() < 1e-6);

        // Fraction 0 moves nothing and is not offered as an operation.
        assert!(slide_points(&mesh, &[0], 0.0).is_none());
        assert!(!can_slide_points(&mesh, &[0], 0.0));
        assert!(can_slide_points(&mesh, &[0], 0.5));

        // Nothing to slide: empty, invalid fractions, or a point with no edge.
        assert!(slide_points(&mesh, &[], 0.5).is_none());
        assert!(slide_points(&mesh, &[99], 0.5).is_none());
        assert!(slide_points(&mesh, &[0], -0.5).is_none());
        assert!(slide_points(&mesh, &[0], 1.5).is_none());
        assert!(slide_points(&mesh, &[0], f32::NAN).is_none());
        let mut isolated = mesh.clone();
        isolated.positions.push(Vec3::new(4.0, 0.0, 4.0));
        assert!(!can_slide_points(&isolated, &[6], 0.5));
        assert!(slide_points(&isolated, &[6], 0.5).is_none());

        // A duplicate selection slides once, not twice.
        let once = slide_points(&mesh, &[0], 0.25).unwrap();
        let twice = slide_points(&mesh, &[0, 0], 0.25).unwrap();
        assert_eq!(twice.positions, once.positions, "a duplicate slides once");
    }

    #[test]
    fn flip_faces_reverses_only_the_selected_winding() {
        let mut mesh = primitives::box_mesh(1.0, 1.0, 1.0);
        uv::box_projection(&mut mesh, 1.0);
        mesh.polygons[0].material_slot = 2;
        let original = mesh.polygons[0].corners.clone();
        let neighbour = mesh.polygons[1].corners.clone();
        let normal = mesh.face_normal(0);
        let positions = mesh.positions.clone();
        let uvs = mesh.uvs.clone();

        // A duplicated index flips the face once; an out-of-range index is ignored.
        let out = flip_faces(&mesh, &[0, 0, 99]);
        assert!(out.validate().is_ok());
        let reversed: Vec<_> = original.iter().rev().cloned().collect();
        assert_eq!(
            out.polygons[0].corners, reversed,
            "winding must reverse exactly"
        );
        assert_eq!(
            out.polygons[0].material_slot, 2,
            "the slot follows the face"
        );
        assert_eq!(
            out.polygons[1].corners, neighbour,
            "unselected polygons keep their winding"
        );
        assert!(
            (out.face_normal(0) + normal).length() < 1e-5,
            "the face normal must point the other way"
        );
        assert_eq!(out.positions, positions, "flipping must not move points");
        assert_eq!(out.uvs, uvs, "corner UV slots are reused, not rebuilt");

        // An empty selection leaves the mesh exactly as it was.
        let untouched = flip_faces(&mesh, &[]);
        assert_eq!(untouched.signature(), mesh.signature());
        assert_eq!(untouched.polygons, mesh.polygons);

        // Flipping every face of a closed mesh negates its signed volume.
        let all: Vec<u32> = (0..mesh.polygons.len() as u32).collect();
        let inverted = flip_faces(&mesh, &all);
        assert!(
            (inverted.signed_volume() + mesh.signed_volume()).abs() < 1e-4,
            "inside-out volume {} versus {}",
            inverted.signed_volume(),
            mesh.signed_volume()
        );
    }

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
    fn region_extrude_adds_offset_points_in_source_order() {
        // Region extrusion used to collect the new points from a HashMap, so
        // point order (and with it the document signature and every exported
        // index) changed from one run to the next.
        for _ in 0..8 {
            let mesh = Mesh::from_polygons(
                "strip",
                vec![
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 1.0),
                    Vec3::new(0.0, 0.0, 1.0),
                    Vec3::new(2.0, 0.0, 0.0),
                    Vec3::new(2.0, 0.0, 1.0),
                ],
                vec![vec![0, 3, 2, 1], vec![1, 2, 5, 4]],
            );
            let out = extrude_faces(&mesh, &[0, 1], 0.5, false);
            let base = mesh.positions.len();
            for (k, source) in (0..mesh.positions.len() as u32).enumerate() {
                let expected = mesh.positions[source as usize] + Vec3::Y * 0.5;
                assert_eq!(
                    out.positions[base + k],
                    expected,
                    "offset point {k} must follow source point {source}"
                );
            }
        }
    }

    #[test]
    fn extrude_and_inset_ignore_faces_with_missing_points() {
        let mut mesh = Mesh::new("broken extrude");
        mesh.positions = vec![Vec3::ZERO, Vec3::X, Vec3::Z];
        mesh.polygons.push(Polygon::new(vec![
            Corner::new(0, 0),
            Corner::new(1, 0),
            Corner::new(9, 0),
        ]));
        // The face cannot be read, so it stays untouched instead of indexing
        // past the position list while building offset points.
        let extruded = extrude_faces(&mesh, &[0], 0.5, false);
        assert_eq!(extruded.polygons.len(), 1);
        let (inset, inner) = inset_faces(&mesh, &[0], 0.5);
        assert!(inner.is_empty());
        assert_eq!(inset.polygons.len(), 1);
    }

    #[test]
    fn bridge_loops_reject_missing_points() {
        let mesh = primitives::plane_mesh(1.0, 1.0, 1, 1);
        let Err(error) = bridge_loops(&mesh, &[0, 1, 2], &[0, 1, 9]) else {
            panic!("bridge accepted a loop that references a missing point");
        };
        assert!(
            error.contains("missing points"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn extrude_and_inset_repair_non_finite_input_positions() {
        // A mesh that reached memory without the load-time repair (for example
        // a primitive built from an out-of-range dimension) must not multiply
        // its NaN/infinite points into the new geometry.
        let mesh = Mesh::from_polygons(
            "non-finite input",
            vec![Vec3::new(f32::NAN, 0.0, 0.0), Vec3::X, Vec3::Z],
            vec![vec![0, 1, 2]],
        );
        let extruded = extrude_faces(&mesh, &[0], 0.5, false);
        assert!(
            extruded.positions.iter().all(|p| p.is_finite()),
            "extrude emitted non-finite positions: {:?}",
            extruded.positions
        );
        assert!(extruded.validate().is_ok());
        let (inset, _) = inset_faces(&mesh, &[0], 0.5);
        assert!(
            inset.positions.iter().all(|p| p.is_finite()),
            "inset emitted non-finite positions: {:?}",
            inset.positions
        );
        assert!(inset.validate().is_ok());
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
    fn insert_point_inside_a_polygon_splits_it_into_triangles() {
        let mut quad = Mesh::from_polygons(
            "quad",
            vec![
                Vec3::new(-1.0, -1.0, 0.0),
                Vec3::new(1.0, -1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(-1.0, 1.0, 0.0),
            ],
            vec![vec![0, 1, 2, 3]],
        );
        quad.polygons[0].material_slot = 2;
        let (out, point) =
            insert_point(&quad, 0, InsertPointPick::Inside(Vec3::new(0.25, 0.0, 0.0)))
                .expect("a point inside the quad splits it");
        assert_eq!(out.polygons.len(), 4);
        assert_eq!(out.positions.len(), 5);
        assert_eq!(out.positions[point as usize], Vec3::new(0.25, 0.0, 0.0));
        for poly in &out.polygons {
            assert_eq!(poly.corners.len(), 3);
            assert_eq!(poly.material_slot, 2);
            assert!(
                poly.corners.iter().any(|corner| corner.point == point),
                "every triangle shares the inserted point"
            );
        }
        assert!(out.validate().is_ok());
        let loops = out.boundary_loops();
        assert_eq!(loops.len(), 1);
        assert_eq!(loops[0].len(), 4, "the quad's silhouette is unchanged");
    }

    #[test]
    fn insert_point_on_an_edge_splits_it_in_both_polygons_and_keeps_marks() {
        let mut pair = Mesh::from_polygons(
            "pair",
            vec![
                Vec3::new(-1.0, -1.0, 0.0),
                Vec3::new(0.0, -1.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(-1.0, 1.0, 0.0),
                Vec3::new(1.0, -1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
            ],
            vec![vec![0, 1, 2, 3], vec![1, 4, 5, 2]],
        );
        pair.set_seam(1, 2, true);
        pair.set_crease_weight(1, 2, 0.5);
        let (out, point) = insert_point(&pair, 0, InsertPointPick::Edge { a: 1, b: 2, t: 0.5 })
            .expect("a mid-edge point splits the shared edge");
        assert_eq!(out.positions.len(), 7);
        assert_eq!(out.positions[point as usize], Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(out.polygons[0].corners.len(), 5);
        assert_eq!(out.polygons[1].corners.len(), 5);
        assert!(
            out.polygons
                .iter()
                .all(|poly| poly.corners.iter().any(|corner| corner.point == point)),
            "both polygons share the new point, so the edge stays watertight"
        );
        assert!(out.is_seam(1, point) && out.is_seam(point, 2));
        assert!(!out.is_seam(1, 2));
        assert!((out.crease_weight(1, point) - 0.5).abs() < 1e-6);
        assert!((out.crease_weight(point, 2) - 0.5).abs() < 1e-6);
        assert!(out.validate().is_ok());
    }

    #[test]
    fn insert_point_refuses_degenerate_positions_and_edges() {
        let quad = Mesh::from_polygons(
            "quad",
            vec![
                Vec3::new(-1.0, -1.0, 0.0),
                Vec3::new(1.0, -1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(-1.0, 1.0, 0.0),
            ],
            vec![vec![0, 1, 2, 3]],
        );
        assert!(insert_point(
            &quad,
            0,
            InsertPointPick::Inside(Vec3::new(-1.0, -1.0, 0.0))
        )
        .is_err());
        assert!(insert_point(&quad, 0, InsertPointPick::Inside(Vec3::splat(f32::NAN))).is_err());
        assert!(insert_point(&quad, 7, InsertPointPick::Inside(Vec3::ZERO)).is_err());
        assert!(insert_point(&quad, 0, InsertPointPick::Edge { a: 0, b: 1, t: 0.0 }).is_err());
        assert!(insert_point(&quad, 0, InsertPointPick::Edge { a: 0, b: 1, t: 1.0 }).is_err());
        assert!(insert_point(&quad, 0, InsertPointPick::Edge { a: 0, b: 2, t: 0.5 }).is_err());
        assert_eq!(
            quad.positions.len(),
            4,
            "a refused pick leaves the mesh alone"
        );
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

    fn quad_patch() -> Mesh {
        let mut mesh = Mesh {
            positions: vec![
                Vec3::new(-0.5, 0.0, -0.5),
                Vec3::new(0.5, 0.0, -0.5),
                Vec3::new(0.5, 0.0, 0.5),
                Vec3::new(-0.5, 0.0, 0.5),
            ],
            ..Mesh::default()
        };
        mesh.add_face(&[0, 1, 2, 3]);
        mesh
    }

    #[test]
    fn shell_faces_builds_a_closed_wall_from_a_plane() {
        let patch = quad_patch();
        let wall = shell_faces(&patch, &[0], 0.25, true, 30.0, 0.0).unwrap();
        assert_eq!(wall.polygons.len(), 6, "face, copy and four rims");
        assert_eq!(wall.positions.len(), 8);
        assert!(
            wall.boundary_loops().is_empty(),
            "the rim quads close the wall"
        );
        assert!(wall.validate().is_ok());
        assert!(
            (wall.signed_volume() - 0.25).abs() < 1e-5,
            "a 1x1 patch at 0.25 thickness has 0.25 volume, got {}",
            wall.signed_volume()
        );

        // Inside follows the documented -normal direction, outside +normal.
        let height = |mesh: &Mesh| mesh.positions.iter().map(|p| p.y).sum::<f32>();
        assert!(height(&wall) > 0.0, "the copy sits against the normal");
        let outside = shell_faces(&patch, &[0], 0.25, false, 30.0, 0.0).unwrap();
        assert!(height(&outside) < 0.0, "outside moves along the normal");
        assert_eq!(outside.polygons.len(), 6);
        assert!(outside.boundary_loops().is_empty());
    }

    #[test]
    fn shelling_a_face_that_references_a_missing_point_is_an_error() {
        let mut mesh = Mesh::new("broken shell");
        mesh.positions = vec![Vec3::ZERO, Vec3::X, Vec3::Z];
        mesh.polygons.push(Polygon::new(vec![
            Corner::new(0, 0),
            Corner::new(1, 0),
            Corner::new(9, 0),
        ]));
        let Err(error) = shell_faces(&mesh, &[0], 0.1, false, 45.0, 0.0) else {
            panic!("shell accepted a face that references a missing point");
        };
        assert!(
            error.contains("polygon 0 references missing point 9"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn shell_region_follows_the_documented_angle_and_quantize() {
        // An upright quad and a floor quad meeting at 90 degrees.
        let mut mesh = Mesh {
            positions: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(1.0, 0.0, -1.0),
                Vec3::new(0.0, 0.0, -1.0),
            ],
            ..Mesh::default()
        };
        mesh.add_face(&[0, 1, 2, 3]);
        mesh.add_face(&[1, 0, 5, 4]);
        assert_eq!(
            shell_region(&mesh, &[0], 30.0),
            vec![0],
            "bigger angles are needed to cross a fold"
        );
        assert_eq!(shell_region(&mesh, &[0], 120.0), vec![0, 1]);

        // Quantize snaps the distance to the step, so 0.27 becomes 0.3 and
        // 0.05 collapses to zero, which the tool rejects.
        let patch = quad_patch();
        let wall = shell_faces(&patch, &[0], 0.27, true, 30.0, 0.1).unwrap();
        assert!((wall.positions[4].y - 0.3).abs() < 1e-5);
        assert!(
            shell_faces(&patch, &[0], 0.04, true, 30.0, 0.1).is_err(),
            "0.04 snaps to zero at a 0.1 step"
        );
        assert!(shell_faces(&patch, &[0], 0.0, true, 30.0, 0.0).is_err());
        assert!(
            shell_faces(&patch, &[0], -0.2, true, 30.0, 0.0).is_err(),
            "thickness is a magnitude; Inside chooses the side"
        );
        assert!(shell_faces(&patch, &[], 0.1, true, 30.0, 0.0).is_err());
        assert_eq!(
            shell_region(&mesh, &[0], 999.0),
            shell_region(&mesh, &[0], 180.0),
            "angles outside 0-180 behave as the nearest limit"
        );
    }

    #[test]
    fn shelling_every_polygon_hollows_a_solid() {
        let solid = crate::primitives::box_mesh(1.0, 1.0, 1.0);
        let faces: Vec<u32> = (0..solid.polygons.len() as u32).collect();
        let thickness = 0.1f32;
        let hollow = shell_faces(&solid, &faces, thickness, true, 30.0, 0.0).unwrap();
        assert_eq!(hollow.polygons.len(), 12, "outer and inner shell");
        assert_eq!(hollow.positions.len(), 16);
        assert!(
            hollow.boundary_loops().is_empty(),
            "a hollow solid needs no rim"
        );
        assert!(hollow.validate().is_ok());
        let inner_side = 1.0 - 2.0 * thickness / 3.0f32.sqrt();
        let expected = 1.0 - inner_side.powi(3);
        assert!(
            (hollow.signed_volume() - expected).abs() < 1e-4,
            "{expected} wall volume, got {}",
            hollow.signed_volume()
        );
        assert_eq!(
            shell_region(&solid, &[0], 30.0),
            vec![0],
            "a box fold is steeper than 30 degrees"
        );
    }

    #[test]
    fn extract_faces_copies_the_selection_and_keeps_marks() {
        let mut mesh = crate::primitives::box_mesh(1.0, 1.0, 1.0);
        crate::box_projection(&mut mesh, 1.0);
        let face: Vec<u32> = mesh.polygons[0].corners.iter().map(|c| c.point).collect();
        let neighbour: Vec<u32> = mesh.polygons[1].corners.iter().map(|c| c.point).collect();
        mesh.toggle_crease(face[0], face[1]);
        mesh.toggle_seam(face[1], face[2]);
        mesh.toggle_crease(neighbour[0], neighbour[1]);
        let pinned = mesh.polygons[0].corners[0].uv;
        mesh.set_uv_pinned(pinned, true);
        let pinned_uv = mesh.uvs[pinned as usize];
        let source = mesh.clone();

        let part = extract_faces(&mesh, &[0, 0]).expect("one face is a valid split");
        assert_eq!(mesh, source, "splitting copies, it never edits the source");
        assert_eq!(part.positions.len(), 4);
        assert_eq!(part.polygons.len(), 1);
        assert_eq!(
            part.polygons[0].points().collect::<Vec<_>>(),
            vec![0, 1, 2, 3],
            "copied corners are renumbered in first-use order"
        );
        let expected: Vec<Vec3> = face.iter().map(|p| mesh.positions[*p as usize]).collect();
        assert_eq!(part.positions, expected, "corners keep their positions");
        assert_eq!(
            part.polygons[0].material_slot,
            mesh.polygons[0].material_slot
        );
        for (from, to) in mesh.polygons[0]
            .corners
            .iter()
            .zip(&part.polygons[0].corners)
        {
            assert_eq!(part.uv_of(to), mesh.uv_of(from));
        }
        assert_eq!(
            part.creases.len(),
            1,
            "only the crease inside the selection follows"
        );
        assert_eq!(part.seams.len(), 1);
        assert_eq!(part.pinned_uvs.len(), 1);
        assert_eq!(part.uvs[part.pinned_uvs[0] as usize], pinned_uv);
        assert!(part.validate().is_ok(), "{:?}", part.validate().err());

        assert!(extract_faces(&mesh, &[]).is_err(), "a split needs polygons");
        assert!(
            extract_faces(&mesh, &[99]).is_err(),
            "missing polygons are rejected"
        );
    }

    #[test]
    fn extract_faces_shares_points_between_copied_faces() {
        let mut mesh = Mesh {
            positions: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(2.0, 0.0, 1.0),
            ],
            ..Mesh::default()
        };
        mesh.add_face(&[0, 3, 4, 1]);
        mesh.add_face(&[1, 4, 5, 2]);
        let part = extract_faces(&mesh, &[1, 0]).expect("two quads are a valid split");
        assert_eq!(part.polygons.len(), 2);
        assert_eq!(
            part.positions.len(),
            6,
            "points shared by the copied faces stay shared"
        );
        assert_eq!(
            part.positions[..4],
            [
                mesh.positions[1],
                mesh.positions[4],
                mesh.positions[5],
                mesh.positions[2]
            ],
            "copied faces keep the order of the selection"
        );
        assert!(part.validate().is_ok());
    }

    #[test]
    fn fill_holes_closes_a_missing_face_with_consistent_winding() {
        let solid = primitives::box_mesh(1.0, 1.0, 1.0);
        let mut open = solid.clone();
        let removed = open.polygons.remove(0);
        assert_eq!(open.boundary_loops().len(), 1);

        let border: Vec<u32> = removed.corners.iter().map(|c| c.point).collect();
        let holes = open_holes(&open, &border[..1]);
        assert_eq!(
            holes.len(),
            1,
            "one point on the border finds the whole hole"
        );
        let filled = fill_holes(&open, &holes).unwrap();
        assert_eq!(filled.polygons.len(), solid.polygons.len());
        assert!(
            filled.boundary_loops().is_empty(),
            "filling must close the border"
        );
        assert!(filled.validate().is_ok());
        assert!(
            (filled.signed_volume() - solid.signed_volume()).abs() < 1e-4,
            "the fill must wind with the rest of the solid, not against it"
        );
    }

    #[test]
    fn fill_holes_keeps_uvs_finite_for_extreme_coordinates() {
        let mesh = Mesh::from_polygons(
            "huge hole",
            vec![
                Vec3::new(-2.0e38, 0.0, -2.0e38),
                Vec3::new(2.0e38, 0.0, -2.0e38),
                Vec3::new(2.0e38, 0.0, 2.0e38),
                Vec3::new(-2.0e38, 0.0, 2.0e38),
            ],
            vec![vec![0, 1, 2, 3]],
        );
        // This border order sums two same-signed coordinates first, so the
        // f32 centre becomes infinite and the projected UVs become NaN.
        let filled = fill_holes(&mesh, &[vec![1, 2, 3, 0]]).unwrap();
        let face = filled.polygons.last().expect("filled face");
        for corner in &face.corners {
            let uv = filled.uv_of(corner);
            assert!(uv.is_finite(), "fill hole produced non-finite UVs: {uv:?}");
        }
    }

    #[test]
    fn fill_holes_needs_a_selected_border_point() {
        let solid = primitives::box_mesh(1.0, 1.0, 1.0);
        assert!(
            open_holes(&solid, &[0]).is_empty(),
            "a closed mesh has no hole border"
        );
        assert!(fill_holes(&solid, &[]).is_err());

        let mut open = solid.clone();
        open.polygons.remove(0);
        let border: HashSet<u32> = open.boundary_loops()[0].iter().copied().collect();
        let off_border = (0..open.positions.len() as u32)
            .find(|p| !border.contains(p))
            .expect("a box has points off the removed face");
        assert!(
            open_holes(&open, &[off_border]).is_empty(),
            "interior points do not select a hole"
        );
        assert_eq!(
            open_holes(&open, &[*border.iter().next().unwrap()]).len(),
            1
        );
        assert!(
            fill_holes(&solid, &[vec![0, 1, 2]]).is_err(),
            "a border between two faces is not a hole"
        );
    }

    #[test]
    fn fill_holes_rejects_non_finite_border_points() {
        let mesh = Mesh::from_polygons(
            "non-finite hole",
            vec![
                Vec3::new(f32::NAN, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ],
            vec![vec![0, 1, 2]],
        );
        let Err(error) = fill_holes(&mesh, &[vec![0, 1, 2]]) else {
            panic!("fill hole accepted a non-finite border point");
        };
        assert!(error.contains("non-finite"), "unexpected error: {error}");
    }

    #[test]
    fn fill_holes_fills_every_selected_border_and_keeps_mesh_marks() {
        let mut open = primitives::box_mesh(2.0, 2.0, 2.0);
        let first = open.polygons.remove(0);
        let second = open.polygons.remove(0);
        crate::uv::box_projection(&mut open, 1.0);
        assert!(open.set_crease(0, 1, true));
        assert!(open.set_seam(0, 1, true));
        assert!(open.set_uv_pinned(0, true));

        let points: Vec<u32> = first
            .corners
            .iter()
            .chain(second.corners.iter())
            .map(|c| c.point)
            .collect();
        let holes = open_holes(&open, &points);
        assert_eq!(holes.len(), 2, "both borders touch the selection");
        let filled = fill_holes(&open, &holes).unwrap();
        assert!(filled.boundary_loops().is_empty());
        assert!(filled.is_crease(0, 1));
        assert!(filled.is_seam(0, 1));
        assert!(filled.is_uv_pinned(0));
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
    fn deformers_move_geometry_predictably() {
        let cube = primitives::box_mesh(2.0, 4.0, 2.0);
        let bounds = cube.bounds();
        let center = bounds.center();

        let twisted = twist(&cube, 1, 90.0, center);
        assert!(twisted.validate().is_ok());
        assert!((twisted.bounds().size().y - 4.0).abs() < 1e-4);
        assert!(twisted.positions.iter().all(|p| p.is_finite()));

        let sheared = shear(&cube, 1, 2.0, 0.0, center);
        assert!(sheared.validate().is_ok());
        assert!(sheared.bounds().max.x > bounds.max.x + 0.5);
        assert!(sheared.bounds().min.x < bounds.min.x - 0.5);
        assert!(
            (sheared.bounds().center().x - bounds.center().x).abs() < 1e-4,
            "shear keeps the midpoint centred"
        );

        let bulged = bulge(&cube, 0.5, true, true);
        assert!(bulged.validate().is_ok());
        assert!(bulged.bounds().size().x > bounds.size().x);
        assert!((bulged.bounds().size().y - 4.0).abs() < 1e-4);

        let spherical = spherify(&cube, 1.0, 1.0);
        assert!(spherical.validate().is_ok());
        let c = spherical.bounds().center();
        for p in &spherical.positions {
            assert!((p.distance(c) - 1.0).abs() < 1e-3, "{p:?}");
        }
    }

    #[test]
    fn deformer_extremes_stay_finite() {
        let plane = primitives::plane_mesh(1.0, 1.0, 1, 1);
        for out in [
            twist(&plane, 2, 720.0, Vec3::ZERO),
            shear(&plane, 0, -5.0, 45.0, Vec3::ZERO),
            bulge(&plane, -4.0, true, false),
            spherify(&plane, 0.0, 1.0),
        ] {
            assert!(out.positions.iter().all(|p| p.is_finite()));
            assert!(out.validate().is_ok());
        }
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

#[cfg(test)]
mod clone_tests {
    use super::*;

    fn quad() -> Mesh {
        Mesh::from_polygons(
            "quad",
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(2.0, 0.0, 2.0),
                Vec3::new(0.0, 0.0, 2.0),
            ],
            vec![vec![0, 1, 2, 3]],
        )
    }

    #[test]
    fn clone_faces_offsets_scales_and_rotates_each_copy() {
        let mesh = quad();
        let options = CloneOptions {
            copies: 2,
            direction: Vec3::X * 2.0,
            scale: 0.5,
            ..Default::default()
        };
        let out = clone_faces(&mesh, &[0], &options);
        assert_eq!(out.polygons.len(), 3);
        assert_eq!(out.positions.len(), 12);
        assert!(out.validate().is_ok());
        let centroid = |start: usize| {
            (start..start + 4)
                .map(|index| out.positions[index])
                .sum::<Vec3>()
                / 4.0
        };
        assert!(
            (centroid(4).x - 2.0).abs() < 1e-5,
            "copy1 centroid {}",
            centroid(4)
        );
        assert!((centroid(8).x - 1.0 - 2.0).abs() < 1e-5);
        // The second copy ends at the requested 0.5 scale.
        let width = out.positions[8].distance(out.positions[9]);
        assert!((width - 1.0).abs() < 1e-5);
    }

    #[test]
    fn clone_empty_selection_copies_every_polygon_and_holes_skip_copies() {
        let cube = crate::primitives::box_mesh(1.0, 1.0, 1.0);
        let out = clone_faces(
            &cube,
            &[],
            &CloneOptions {
                copies: 1,
                direction: Vec3::Y,
                ..Default::default()
            },
        );
        assert_eq!(out.polygons.len(), cube.polygons.len() * 2);
        let holed = clone_faces(
            &cube,
            &[],
            &CloneOptions {
                copies: 3,
                holes: 1.0,
                direction: Vec3::Y,
                ..Default::default()
            },
        );
        assert_eq!(holed.polygons.len(), cube.polygons.len());
    }

    #[test]
    fn clone_copies_creases_and_is_deterministic_with_variation() {
        let mut mesh = quad();
        assert!(mesh.set_crease(0, 1, true));
        let options = CloneOptions {
            copies: 3,
            direction: Vec3::X,
            scale_variation: 0.25,
            position_variation: 0.1,
            rotation_variation_deg: 15.0,
            ..Default::default()
        };
        let first = clone_faces(&mesh, &[0], &options);
        let second = clone_faces(&mesh, &[0], &options);
        assert_eq!(first, second);
        assert_eq!(first.creases.len(), 4);
        assert_eq!(first.polygons.len(), 4);
        assert!(first.validate().is_ok());
    }

    #[test]
    fn clone_points_duplicates_only_the_selected_points() {
        let mesh = quad();
        let out = clone_points(
            &mesh,
            &[0, 2],
            &CloneOptions {
                copies: 2,
                direction: Vec3::Y,
                ..Default::default()
            },
        );
        assert_eq!(out.positions.len(), mesh.positions.len() + 4);
        assert_eq!(out.polygons.len(), mesh.polygons.len());
        assert_eq!(out.positions[4].y, 0.5);
        assert_eq!(out.positions[6].y, 1.0);
    }
}

#[test]
fn clone_keep_ratio_makes_scale_variation_uniform() {
    let mesh = Mesh::from_polygons(
        "rect",
        vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(4.0, 0.0, 2.0),
            Vec3::new(0.0, 0.0, 2.0),
        ],
        vec![vec![0, 1, 2, 3]],
    );
    let mut options = CloneOptions {
        copies: 1,
        scale_variation: 0.5,
        keep_ratio: true,
        ..Default::default()
    };
    let uniform = clone_faces(&mesh, &[0], &options);
    let width = uniform.positions[4].distance(uniform.positions[5]);
    let depth = uniform.positions[4].distance(uniform.positions[7]);
    assert!((width / depth - 2.0).abs() < 1e-5);

    options.keep_ratio = false;
    let independent = clone_faces(&mesh, &[0], &options);
    let width = independent.positions[4].distance(independent.positions[5]);
    let depth = independent.positions[4].distance(independent.positions[7]);
    assert!(
        (width / depth - 2.0).abs() > 1e-4,
        "independent scale variation kept the original ratio"
    );
}

#[test]
fn fill_holes_refuses_a_hole_that_encloses_an_island() {
    let mesh = Mesh::from_polygons(
        "hole with island",
        vec![
            Vec3::new(-2.0, 0.0, -2.0),
            Vec3::new(2.0, 0.0, -2.0),
            Vec3::new(2.0, 0.0, 2.0),
            Vec3::new(-2.0, 0.0, 2.0),
            Vec3::new(-1.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(-1.0, 0.0, 1.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.3, 0.0, 0.0),
            Vec3::new(0.0, 0.3, 0.0),
        ],
        vec![
            vec![0, 1, 5, 4],
            vec![1, 2, 6, 5],
            vec![2, 3, 7, 6],
            vec![3, 0, 4, 7],
            vec![8, 9, 10],
        ],
    );
    let holes = open_holes(&mesh, &[4]);
    assert_eq!(holes.len(), 1);
    let Err(error) = fill_holes(&mesh, &holes) else {
        panic!("filled a hole through the island");
    };
    assert!(error.contains("island"), "unexpected error: {error}");
}
