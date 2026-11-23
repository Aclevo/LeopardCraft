//! UV projection, chart packing and relaxation.

use crate::Mesh;
use glam::{Vec2, Vec3};
use std::collections::{HashMap, HashSet, VecDeque};

/// Replace the whole UV set with fresh per-corner slots. Projections re-author
/// every coordinate, so they also drop the pins that referred to the old slots.
fn assign_corner_uvs(mesh: &mut Mesh, uvs: Vec<Vec2>) {
    let mut k = 0u32;
    for poly in &mut mesh.polygons {
        for corner in &mut poly.corners {
            corner.uv = k;
            k += 1;
        }
    }
    mesh.uvs = uvs;
    mesh.pinned_uvs.clear();
}

/// Rebuild the per-corner UV slots from `corner_values` (polygon then corner
/// order) while keeping pinned slots intact: every corner whose current slot is
/// pinned keeps one shared slot per pin and the pinned coordinate, so a pin
/// survives an operation that rewrites the rest of the UV set.
fn assign_corner_uvs_keeping_pins(mesh: &mut Mesh, corner_values: Vec<Vec2>) {
    let pinned: HashMap<u32, Vec2> = mesh
        .pinned_uvs
        .iter()
        .filter_map(|slot| mesh.uvs.get(*slot as usize).map(|uv| (*slot, *uv)))
        .collect();
    let mut slot_remap: HashMap<u32, u32> = HashMap::new();
    let mut uvs: Vec<Vec2> = Vec::with_capacity(corner_values.len());
    let mut slots: Vec<u32> = Vec::with_capacity(corner_values.len());
    let mut index = 0usize;
    for poly in &mesh.polygons {
        for corner in &poly.corners {
            let value = corner_values.get(index).copied().unwrap_or(Vec2::ZERO);
            index += 1;
            let slot = match pinned.get(&corner.uv) {
                Some(&pinned_uv) => *slot_remap.entry(corner.uv).or_insert_with(|| {
                    uvs.push(pinned_uv);
                    (uvs.len() - 1) as u32
                }),
                None => {
                    uvs.push(value);
                    (uvs.len() - 1) as u32
                }
            };
            slots.push(slot);
        }
    }
    let mut next = 0usize;
    for poly in &mut mesh.polygons {
        for corner in &mut poly.corners {
            corner.uv = slots[next];
            next += 1;
        }
    }
    mesh.uvs = uvs;
    let mut pinned_slots: Vec<u32> = slot_remap.values().copied().collect();
    pinned_slots.sort_unstable();
    mesh.pinned_uvs = pinned_slots;
}

/// Fit values into 0..1 preserving aspect ratio. Bounds and the scale are
/// measured in f64 because opposite-signed f32 coordinates can span more than
/// f32::MAX; the f32 subtraction then overflows to infinity and the infinite
/// offsets multiplied by the resulting zero scale turn into NaN.
fn normalize_into_unit(values: &mut [Vec2]) {
    if values.is_empty() {
        return;
    }
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for uv in values.iter() {
        min[0] = min[0].min(uv.x as f64);
        min[1] = min[1].min(uv.y as f64);
        max[0] = max[0].max(uv.x as f64);
        max[1] = max[1].max(uv.y as f64);
    }
    let extent = (max[0] - min[0]).max(max[1] - min[1]).max(1e-6);
    let scale = 1.0 / extent;
    for uv in values.iter_mut() {
        uv.x = ((uv.x as f64 - min[0]) * scale) as f32;
        uv.y = ((uv.y as f64 - min[1]) * scale) as f32;
    }
}

/// Pinned UV slots with their authored coordinates, in slot order.
fn pin_anchors(mesh: &Mesh) -> Vec<(u32, Vec2)> {
    mesh.pinned_uvs
        .iter()
        .filter_map(|slot| mesh.uvs.get(*slot as usize).map(|uv| (*slot, *uv)))
        .collect()
}

/// Best-fit uniform scale plus translation per chart, so a chart's pinned
/// points approach their authored UVs before the exact pin values are written
/// back. Charts without a pin are untouched. This is a clean-room anchor fit,
/// not an iterative constraint solver: it has no rotational or area freedom.
fn align_charts_to_pins(
    mesh: &Mesh,
    corner_start: &[usize],
    corner_uv: &mut [Vec2],
    corner_chart: &[usize],
    chart_count: usize,
) {
    let pins: HashMap<u32, Vec2> = pin_anchors(mesh).into_iter().collect();
    if pins.is_empty() {
        return;
    }
    let mut anchors: Vec<Vec<(Vec2, Vec2)>> = vec![Vec::new(); chart_count];
    for (pi, poly) in mesh.polygons.iter().enumerate() {
        for (k, corner) in poly.corners.iter().enumerate() {
            let Some(&target) = pins.get(&corner.uv) else {
                continue;
            };
            let at = corner_start[pi] + k;
            if corner_chart[at] < chart_count {
                anchors[corner_chart[at]].push((corner_uv[at], target));
            }
        }
    }
    for (chart, pairs) in anchors.iter().enumerate() {
        if pairs.is_empty() {
            continue;
        }
        let n = pairs.len() as f32;
        let from = pairs.iter().map(|(p, _)| *p).sum::<Vec2>() / n;
        let to = pairs.iter().map(|(_, q)| *q).sum::<Vec2>() / n;
        let (mut num, mut den) = (0.0f32, 0.0f32);
        for (p, q) in pairs {
            num += (*p - from).dot(*q - to);
            den += (*p - from).length_squared();
        }
        let fit = if den > 1e-12 {
            (num / den).max(1e-4)
        } else {
            1.0
        };
        for (pi, poly) in mesh.polygons.iter().enumerate() {
            for k in 0..poly.corners.len() {
                let at = corner_start[pi] + k;
                if corner_chart[at] == chart {
                    corner_uv[at] = to + (corner_uv[at] - from) * fit;
                }
            }
        }
    }
}

fn corner_positions(mesh: &Mesh) -> Vec<Vec3> {
    let mut out = Vec::with_capacity(mesh.uvs.len());
    for poly in &mesh.polygons {
        for c in &poly.corners {
            out.push(mesh.positions[c.point as usize]);
        }
    }
    out
}

/// Project along an axis with 1 unit = `scale` UV units.
pub fn planar_projection(mesh: &mut Mesh, axis: usize, scale: f32) {
    let s = if scale.abs() < 1e-6 { 1.0 } else { scale };
    let pts = corner_positions(mesh);
    let uvs = pts
        .iter()
        .map(|p| match axis {
            0 => Vec2::new(p.z * s, p.y * s),
            1 => Vec2::new(p.x * s, p.z * s),
            _ => Vec2::new(p.x * s, p.y * s),
        })
        .collect();
    assign_corner_uvs(mesh, uvs);
}

/// Per-face box projection: each face uses the plane most aligned with its normal.
pub fn box_projection(mesh: &mut Mesh, scale: f32) {
    let s = if scale.abs() < 1e-6 { 1.0 } else { scale };
    let mut uvs = Vec::new();
    for (pi, poly) in mesh.polygons.iter().enumerate() {
        let n = mesh.face_normal(pi);
        for c in &poly.corners {
            let p = mesh.positions[c.point as usize];
            let uv = if n.y.abs() >= n.x.abs() && n.y.abs() >= n.z.abs() {
                Vec2::new(p.x, p.z)
            } else if n.x.abs() >= n.z.abs() {
                Vec2::new(p.z, p.y)
            } else {
                Vec2::new(p.x, p.y)
            };
            uvs.push(uv * s);
        }
    }
    assign_corner_uvs(mesh, uvs);
}

pub fn cylindrical_projection(mesh: &mut Mesh, scale: f32, height_scale: f32) {
    let s = if scale.abs() < 1e-6 { 1.0 } else { scale };
    let pts = corner_positions(mesh);
    let uvs = pts
        .iter()
        .map(|p| {
            let u = p.z.atan2(p.x) / std::f32::consts::TAU + 0.5;
            Vec2::new(u * s, p.y * height_scale)
        })
        .collect();
    assign_corner_uvs(mesh, uvs);
}

pub fn spherical_projection(mesh: &mut Mesh, scale: f32) {
    let pts = corner_positions(mesh);
    let uvs = pts
        .iter()
        .map(|p| {
            let v = p.normalize_or(Vec3::Y);
            let u = v.z.atan2(v.x) / std::f32::consts::TAU + 0.5;
            let vv = v.y.acos() / std::f32::consts::PI;
            Vec2::new(u, vv) * scale
        })
        .collect();
    assign_corner_uvs(mesh, uvs);
}

/// Split the mesh into charts of connected faces whose normals stay within
/// `max_angle_deg`, project each chart, and shelf-pack the charts into 0..1.
/// This is a practical automatic unwrap: low distortion per chart and no
/// overlap between charts.
///
/// Authored seams (`Mesh::seams`) are hard chart borders: an unwrap never
/// crosses a seam, even where the two faces point the same way, so marking a
/// cut is how a closed or smoothly curved surface is flattened without
/// overlapping islands.
pub fn automatic_unwrap(mesh: &mut Mesh, max_angle_deg: f32, padding: f32) {
    let face_count = mesh.polygons.len();
    if face_count == 0 {
        return;
    }
    let normals: Vec<Vec3> = (0..face_count).map(|i| mesh.face_normal(i)).collect();
    let edges = mesh.edges();
    let mut edge_faces: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    for e in &edges {
        edge_faces.insert((e.a, e.b), e.faces.clone());
    }
    let seams: HashSet<(u32, u32)> = mesh
        .seams
        .iter()
        .map(|[a, b]| if a < b { (*a, *b) } else { (*b, *a) })
        .collect();
    let cos_limit = max_angle_deg.to_radians().cos();
    let mut chart_of = vec![usize::MAX; face_count];
    let mut charts: Vec<Vec<usize>> = Vec::new();
    for seed in 0..face_count {
        if chart_of[seed] != usize::MAX {
            continue;
        }
        let chart_id = charts.len();
        let mut chart = Vec::new();
        let mut queue = VecDeque::from([seed]);
        chart_of[seed] = chart_id;
        while let Some(f) = queue.pop_front() {
            chart.push(f);
            let poly = &mesh.polygons[f];
            let n = poly.corners.len();
            for i in 0..n {
                let a = poly.corners[i].point;
                let b = poly.corners[(i + 1) % n].point;
                let key = if a < b { (a, b) } else { (b, a) };
                if seams.contains(&key) {
                    continue;
                }
                if let Some(faces) = edge_faces.get(&key) {
                    for &other in faces {
                        let other = other as usize;
                        if chart_of[other] == usize::MAX
                            && normals[other].dot(normals[f]) >= cos_limit
                        {
                            chart_of[other] = chart_id;
                            queue.push_back(other);
                        }
                    }
                }
            }
        }
        charts.push(chart);
    }

    // Raw 2D coordinates per chart, projected onto its average normal. The
    // dot products, bounds and packing are f64 so coordinates whose span
    // exceeds f32::MAX still pack into 0..1 instead of overflowing to NaN.
    struct ChartData {
        faces: Vec<usize>,
        uvs: Vec<[f64; 2]>,
        min: [f64; 2],
        max: [f64; 2],
    }
    let dot = |p: Vec3, q: Vec3| -> f64 {
        p.x as f64 * q.x as f64 + p.y as f64 * q.y as f64 + p.z as f64 * q.z as f64
    };
    let mut data: Vec<ChartData> = Vec::new();
    for faces in charts {
        let mut avg = Vec3::ZERO;
        for &f in &faces {
            avg += normals[f];
        }
        let avg = avg.normalize_or(Vec3::Y);
        let (t, b) = lc_core::orthonormal_basis(avg);
        let mut uvs = Vec::new();
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for &f in &faces {
            for c in &mesh.polygons[f].corners {
                let p = mesh.positions[c.point as usize];
                let uv = [dot(p, t), dot(p, b)];
                min[0] = min[0].min(uv[0]);
                min[1] = min[1].min(uv[1]);
                max[0] = max[0].max(uv[0]);
                max[1] = max[1].max(uv[1]);
                uvs.push(uv);
            }
        }
        data.push(ChartData {
            faces,
            uvs,
            min,
            max,
        });
    }

    // Shelf packing: sort by height, fill rows in a virtual square.
    data.sort_by(|a, b| {
        let ha = a.max[1] - a.min[1];
        let hb = b.max[1] - b.min[1];
        hb.partial_cmp(&ha).unwrap_or(std::cmp::Ordering::Equal)
    });
    let pad = if padding.is_finite() {
        padding.max(0.0)
    } else {
        0.0
    } as f64;
    let mut cursor = [pad, pad];
    let mut row_height = 0.0f64;
    let mut placements: Vec<([f64; 2], [f64; 2])> = Vec::with_capacity(data.len());
    let shelf_width = 0.96f64;
    for c in &data {
        let size = [
            (c.max[0] - c.min[0]).max(1e-5),
            (c.max[1] - c.min[1]).max(1e-5),
        ];
        if cursor[0] + size[0] > shelf_width {
            cursor[0] = pad;
            cursor[1] += row_height + pad;
            row_height = 0.0;
        }
        placements.push((cursor, size));
        cursor[0] += size[0] + pad;
        row_height = row_height.max(size[1]);
    }
    let total_height = (cursor[1] + row_height + pad).max(1e-5);
    let scale = (1.0 / shelf_width).min(1.0 / total_height);

    // Place the packed coordinates per corner, indexed by polygon then corner,
    // so every face keeps its own chart projection and the authored corner
    // order. Pinned corners are anchored to their pinned coordinate before the
    // slots are rebuilt.
    let mut corner_start = Vec::with_capacity(face_count);
    let mut total_corners = 0usize;
    for poly in &mesh.polygons {
        corner_start.push(total_corners);
        total_corners += poly.corners.len();
    }
    let mut corner_uv = vec![Vec2::ZERO; total_corners];
    let mut corner_chart = vec![usize::MAX; total_corners];
    for (ci, chart) in data.iter().enumerate() {
        let (offset, _size) = placements[ci];
        let mut chart_cursor = 0usize;
        for &f in &chart.faces {
            let corners = mesh.polygons[f].corners.len();
            for k in 0..corners {
                let raw = chart.uvs[chart_cursor + k];
                let at = corner_start[f] + k;
                corner_uv[at] = Vec2::new(
                    ((offset[0] + (raw[0] - chart.min[0])) * scale) as f32,
                    ((offset[1] + (raw[1] - chart.min[1])) * scale) as f32,
                );
                corner_chart[at] = ci;
            }
            chart_cursor += corners;
        }
    }
    align_charts_to_pins(
        mesh,
        &corner_start,
        &mut corner_uv,
        &corner_chart,
        data.len(),
    );
    assign_corner_uvs_keeping_pins(mesh, corner_uv);
}

/// Laplacian relaxation of existing UVs, per chart.
///
/// A chart is a group of faces that seams do not divide, so an authored seam
/// (source 37) is never averaged across: each side of the cut keeps its own UV
/// layout instead of collapsing onto one shared point value. Corners on a chart
/// border, meaning a seam edge or a mesh boundary, are optionally held so a
/// chart cannot shrink away from its cut, and pinned UV slots are always hard
/// constraints.
pub fn relax_uvs(mesh: &mut Mesh, iterations: u32, strength: f32, pin_boundary: bool) {
    let face_count = mesh.polygons.len();
    if face_count == 0 {
        return;
    }
    let mut corner_start = Vec::with_capacity(face_count);
    let mut corner_count = 0usize;
    for poly in &mesh.polygons {
        corner_start.push(corner_count);
        corner_count += poly.corners.len();
    }
    if corner_count == 0 {
        return;
    }

    let seams: HashSet<(u32, u32)> = mesh
        .seams
        .iter()
        .map(|[a, b]| if a < b { (*a, *b) } else { (*b, *a) })
        .collect();
    let edges = mesh.edges();
    let mut edge_faces: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    for edge in &edges {
        edge_faces.insert((edge.a, edge.b), edge.faces.clone());
    }

    // Charts: connected faces across edges that are not seams.
    let mut chart_of = vec![usize::MAX; face_count];
    for seed in 0..face_count {
        if chart_of[seed] != usize::MAX {
            continue;
        }
        let chart = seed;
        chart_of[seed] = chart;
        let mut queue = VecDeque::from([seed]);
        while let Some(face) = queue.pop_front() {
            let poly = &mesh.polygons[face];
            let n = poly.corners.len();
            for i in 0..n {
                let a = poly.corners[i].point;
                let b = poly.corners[(i + 1) % n].point;
                let key = if a < b { (a, b) } else { (b, a) };
                if seams.contains(&key) {
                    continue;
                }
                if let Some(faces) = edge_faces.get(&key) {
                    for &other in faces {
                        let other = other as usize;
                        if chart_of[other] == usize::MAX {
                            chart_of[other] = chart;
                            queue.push_back(other);
                        }
                    }
                }
            }
        }
    }

    // Corner adjacency: neighbouring corners of the same face plus the matching
    // corner across each interior edge. Nothing crosses a seam, so a seam point
    // has one relaxable value per chart side.
    let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); corner_count];
    for (pi, poly) in mesh.polygons.iter().enumerate() {
        let n = poly.corners.len();
        let base = corner_start[pi];
        for k in 0..n {
            let here = base + k;
            neighbours[here].push(base + (k + n - 1) % n);
            neighbours[here].push(base + (k + 1) % n);
        }
    }
    let corner_at = |face: usize, point: u32| -> Option<usize> {
        mesh.polygons[face]
            .corners
            .iter()
            .position(|corner| corner.point == point)
            .map(|k| corner_start[face] + k)
    };
    for edge in &edges {
        if edge.faces.len() != 2 || seams.contains(&(edge.a, edge.b)) {
            continue;
        }
        let (first, second) = (edge.faces[0] as usize, edge.faces[1] as usize);
        if chart_of[first] != chart_of[second] {
            continue;
        }
        for point in [edge.a, edge.b] {
            if let (Some(a), Some(b)) = (corner_at(first, point), corner_at(second, point)) {
                neighbours[a].push(b);
                neighbours[b].push(a);
            }
        }
    }

    // Border corners: endpoints of a seam edge or of a mesh boundary edge.
    let mut border = vec![false; mesh.positions.len()];
    for edge in &edges {
        if edge.faces.len() == 1 || seams.contains(&(edge.a, edge.b)) {
            for point in [edge.a, edge.b] {
                if let Some(flag) = border.get_mut(point as usize) {
                    *flag = true;
                }
            }
        }
    }

    let pins: HashMap<u32, Vec2> = pin_anchors(mesh).into_iter().collect();
    let mut current = Vec::with_capacity(corner_count);
    let mut pinned_corner = Vec::with_capacity(corner_count);
    let mut corner_point = Vec::with_capacity(corner_count);
    for poly in &mesh.polygons {
        for corner in &poly.corners {
            current.push(mesh.uv_of(corner));
            pinned_corner.push(pins.contains_key(&corner.uv));
            corner_point.push(corner.point);
        }
    }

    let s = strength.clamp(0.0, 1.0);
    for _ in 0..iterations {
        let mut next = current.clone();
        for corner in 0..corner_count {
            if pinned_corner[corner]
                || (pin_boundary && border[corner_point[corner] as usize])
                || neighbours[corner].is_empty()
            {
                continue;
            }
            let list = &neighbours[corner];
            let avg = list.iter().map(|&other| current[other]).sum::<Vec2>() / list.len() as f32;
            next[corner] = current[corner].lerp(avg, s);
        }
        current = next;
    }

    if pins.is_empty() {
        // Normalize the relaxed layout into 0..1 while preserving aspect.
        normalize_into_unit(&mut current);
    }
    // Pins are hard constraints: their slots keep their authored coordinate and
    // the layout is not re-fitted into 0..1, because that would move them.
    // Relaxation stays a clean-room per-chart Laplacian average, not a
    // parameterisation solve.
    assign_corner_uvs_keeping_pins(mesh, current);
}

/// Fit every UV into `0..1`, preserving aspect ratio.
pub fn normalize_uvs(mesh: &mut Mesh) {
    if mesh.uvs.is_empty() {
        return;
    }
    // Pins are hard constraints, so normalization leaves pinned slots exactly
    // where the user put them. Cheetah3D documents this only for Unwrap UV.
    let pinned = pin_anchors(mesh);
    normalize_into_unit(&mut mesh.uvs);
    for (slot, uv) in pinned {
        if let Some(target) = mesh.uvs.get_mut(slot as usize) {
            *target = uv;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives;

    #[test]
    fn projections_cover_unit_range() {
        for projection in 0..4 {
            let mut mesh = primitives::box_mesh(2.0, 2.0, 2.0);
            match projection {
                0 => planar_projection(&mut mesh, 1, 0.25),
                1 => box_projection(&mut mesh, 0.25),
                2 => cylindrical_projection(&mut mesh, 0.25, 0.25),
                _ => spherical_projection(&mut mesh, 0.25),
            }
            assert_eq!(
                mesh.uvs.len(),
                mesh.polygons.iter().map(|p| p.corners.len()).sum::<usize>()
            );
            assert!(mesh.uvs.iter().all(|uv| uv.is_finite()));
        }
    }

    #[test]
    fn automatic_unwrap_packs_within_unit_square() {
        let mut mesh = primitives::box_mesh(1.0, 1.0, 1.0);
        automatic_unwrap(&mut mesh, 45.0, 0.01);
        assert!(!mesh.uvs.is_empty());
        for uv in &mesh.uvs {
            assert!(
                uv.x >= -1e-4 && uv.x <= 1.0 + 1e-4,
                "uv out of range: {uv:?}"
            );
            assert!(
                uv.y >= -1e-4 && uv.y <= 1.0 + 1e-4,
                "uv out of range: {uv:?}"
            );
        }
    }

    fn face_point_uv(mesh: &Mesh, face: usize, point: u32) -> Vec2 {
        mesh.polygons[face]
            .corners
            .iter()
            .find(|c| c.point == point)
            .map(|c| mesh.uv_of(c))
            .expect("face does not reference the point")
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
            vec![vec![0, 3, 2, 1], vec![1, 2, 5, 4]],
        )
    }

    #[test]
    fn seams_cut_the_unwrap_into_separate_charts() {
        let mut smooth = two_quad_strip();
        automatic_unwrap(&mut smooth, 45.0, 0.01);
        let shared = face_point_uv(&smooth, 0, 1);
        assert!(
            (shared - face_point_uv(&smooth, 1, 1)).length() < 1e-5,
            "without a seam both quads share one chart, so the point has one UV"
        );

        let mut seamed = two_quad_strip();
        assert!(seamed.set_seam(1, 2, true));
        automatic_unwrap(&mut seamed, 45.0, 0.01);
        assert!(
            (face_point_uv(&seamed, 0, 1) - face_point_uv(&seamed, 1, 1)).length() > 1e-3,
            "a seam must separate the two charts"
        );
        assert!((face_point_uv(&seamed, 0, 2) - face_point_uv(&seamed, 1, 2)).length() > 1e-3);
        for uv in &seamed.uvs {
            assert!(uv.is_finite());
            assert!(
                uv.x >= -1e-4 && uv.x <= 1.0 + 1e-4 && uv.y >= -1e-4 && uv.y <= 1.0 + 1e-4,
                "uv out of range: {uv:?}"
            );
        }

        // Unwrapping is deterministic: the same cage and seams give the same UVs.
        let mut again = two_quad_strip();
        assert!(again.set_seam(1, 2, true));
        automatic_unwrap(&mut again, 45.0, 0.01);
        assert_eq!(seamed.uvs, again.uvs);
    }

    #[test]
    fn unwrap_accepts_unsorted_duplicate_seam_entries() {
        let mut mesh = two_quad_strip();
        mesh.seams = vec![[2, 1], [1, 2]];
        automatic_unwrap(&mut mesh, 45.0, 0.01);
        assert!((face_point_uv(&mesh, 0, 1) - face_point_uv(&mesh, 1, 1)).length() > 1e-3);
    }

    fn corner_value(mesh: &Mesh, poly: usize, corner: usize) -> Vec2 {
        mesh.uv_of(&mesh.polygons[poly].corners[corner])
    }

    #[test]
    fn unwrap_keeps_pinned_uvs_and_moves_the_rest() {
        let mut mesh = two_quad_strip();
        automatic_unwrap(&mut mesh, 45.0, 0.01);
        let slot_a = mesh.polygons[0].corners[0].uv;
        let slot_b = mesh.polygons[0].corners[2].uv;
        // Move the points somewhere specific first, as a user would before
        // pinning them.
        mesh.uvs[slot_a as usize] = Vec2::new(0.15, 0.85);
        mesh.uvs[slot_b as usize] = Vec2::new(0.45, 0.85);
        let anchor_a = mesh.uvs[slot_a as usize];
        let anchor_b = mesh.uvs[slot_b as usize];
        assert!(mesh.set_uv_pinned(slot_a, true));
        assert!(mesh.set_uv_pinned(slot_b, true));
        let unpinned_before = corner_value(&mesh, 1, 3);
        assert_ne!(unpinned_before, Vec2::ZERO);

        automatic_unwrap(&mut mesh, 45.0, 0.01);

        assert_eq!(
            corner_value(&mesh, 0, 0),
            anchor_a,
            "a pinned UV must not move through an unwrap"
        );
        assert_eq!(
            corner_value(&mesh, 0, 2),
            anchor_b,
            "a pinned UV must not move through an unwrap"
        );
        assert_eq!(mesh.pinned_uvs.len(), 2);
        assert!(mesh.is_uv_pinned(mesh.polygons[0].corners[0].uv));
        assert!(mesh.is_uv_pinned(mesh.polygons[0].corners[2].uv));
        assert_ne!(
            corner_value(&mesh, 1, 3),
            unpinned_before,
            "unpinned UVs are re-solved by the unwrap"
        );
        assert!(mesh.uvs.iter().all(|uv| uv.is_finite()));
    }

    #[test]
    fn relax_keeps_pinned_uvs_and_moves_the_rest() {
        let mut mesh = two_quad_strip();
        cylindrical_projection(&mut mesh, 0.4, 0.4);
        let pinned_slot = mesh.polygons[1].corners[1].uv;
        let anchor = mesh.uvs[pinned_slot as usize];
        assert!(mesh.set_uv_pinned(pinned_slot, true));
        let loose_before = corner_value(&mesh, 0, 0);

        relax_uvs(&mut mesh, 4, 0.5, false);

        assert_eq!(
            corner_value(&mesh, 1, 1),
            anchor,
            "a pinned UV must not move through a relax"
        );
        assert_eq!(mesh.pinned_uvs.len(), 1);
        assert!(mesh.is_uv_pinned(mesh.polygons[1].corners[1].uv));
        assert_ne!(
            corner_value(&mesh, 0, 0),
            loose_before,
            "unpinned UVs relax"
        );
        assert!(mesh.uvs.iter().all(|uv| uv.is_finite()));
    }

    #[test]
    fn relax_keeps_seams_as_chart_borders() {
        let mut mesh = two_quad_strip();
        planar_projection(&mut mesh, 1, 1.0);
        let shared = mesh
            .edges()
            .into_iter()
            .find(|edge| edge.faces.len() == 2)
            .expect("the strip shares one edge");
        assert!(mesh.set_seam(shared.a, shared.b, true));
        // Park the second chart in its own part of UV space, as an unwrap would.
        for k in 0..mesh.polygons[1].corners.len() {
            let slot = mesh.polygons[1].corners[k].uv as usize;
            mesh.uvs[slot] += Vec2::new(0.6, 0.0);
        }
        let gap = |mesh: &Mesh| {
            (face_point_uv(mesh, 0, shared.a).x - face_point_uv(mesh, 1, shared.a).x).abs()
        };
        assert!(gap(&mesh) > 0.5, "the charts start apart");

        // Free borders: each side relaxes inside its own chart, so the cut never
        // collapses onto one shared point value.
        relax_uvs(&mut mesh, 8, 0.5, false);
        assert!(
            gap(&mesh) > 0.2,
            "a seam must stay a chart border, gap {}",
            gap(&mesh)
        );
        assert!(mesh.uvs.iter().all(|uv| uv.is_finite()));

        // Held borders: a pin freezes the layout, and seam endpoints are held as
        // chart borders instead of being averaged with the other side.
        let anchor_slot = mesh.polygons[0].corners[0].uv;
        let anchor = mesh.uvs[anchor_slot as usize];
        assert!(mesh.set_uv_pinned(anchor_slot, true));
        let cut_before = face_point_uv(&mesh, 0, shared.a);
        relax_uvs(&mut mesh, 4, 0.5, true);
        assert_eq!(face_point_uv(&mesh, 0, 0), anchor, "a pin stays exact");
        assert_eq!(
            face_point_uv(&mesh, 0, shared.a),
            cut_before,
            "a seam endpoint is a chart border and is held"
        );
        assert!(gap(&mesh) > 0.2);
    }

    #[test]
    fn projections_clear_pins_and_normalize_keeps_them() {
        let mut mesh = two_quad_strip();
        planar_projection(&mut mesh, 1, 1.0);
        assert!(mesh.set_uv_pinned(0, true));
        box_projection(&mut mesh, 1.0);
        assert!(
            mesh.pinned_uvs.is_empty(),
            "a projection re-authors every UV, so old pins cannot survive"
        );

        assert!(mesh.set_uv_pinned(1, true));
        let anchor = mesh.uvs[1];
        normalize_uvs(&mut mesh);
        assert_eq!(mesh.uvs[1], anchor, "normalize keeps pinned slots in place");
    }

    fn huge_quad() -> Mesh {
        Mesh::from_polygons(
            "huge",
            vec![
                Vec3::new(-2.0e38, 0.0, -2.0e38),
                Vec3::new(2.0e38, 0.0, -2.0e38),
                Vec3::new(2.0e38, 0.0, 2.0e38),
                Vec3::new(-2.0e38, 0.0, 2.0e38),
            ],
            vec![vec![0, 1, 2, 3]],
        )
    }

    #[test]
    fn normalize_maps_extreme_coordinates_without_overflowing() {
        let mut mesh = huge_quad();
        // The projected UV span is 4e38, more than f32::MAX. Subtracting the
        // bounds in f32 overflows to infinity and the scaled values become NaN.
        planar_projection(&mut mesh, 1, 1.0);
        normalize_uvs(&mut mesh);
        assert!(
            mesh.uvs.iter().all(|uv| uv.is_finite()),
            "normalize produced non-finite UVs: {:?}",
            mesh.uvs
        );
        for uv in &mesh.uvs {
            assert!(
                uv.x >= -1e-4 && uv.x <= 1.0 + 1e-4 && uv.y >= -1e-4 && uv.y <= 1.0 + 1e-4,
                "uv out of range: {uv:?}"
            );
        }
    }

    #[test]
    fn unwrap_keeps_extreme_coordinates_finite() {
        let mut mesh = huge_quad();
        automatic_unwrap(&mut mesh, 45.0, 0.01);
        assert!(
            mesh.uvs.iter().all(|uv| uv.is_finite()),
            "unwrap produced non-finite UVs: {:?}",
            mesh.uvs
        );
        for uv in &mesh.uvs {
            assert!(
                uv.x >= -1e-4 && uv.x <= 1.0 + 1e-4 && uv.y >= -1e-4 && uv.y <= 1.0 + 1e-4,
                "uv out of range: {uv:?}"
            );
        }
    }

    #[test]
    fn relax_normalizes_extreme_coordinates_without_overflowing() {
        let mut mesh = huge_quad();
        planar_projection(&mut mesh, 1, 1.0);
        relax_uvs(&mut mesh, 1, 0.0, false);
        assert!(
            mesh.uvs.iter().all(|uv| uv.is_finite()),
            "relax produced non-finite UVs: {:?}",
            mesh.uvs
        );
    }

    #[test]
    fn relaxation_is_finite() {
        let mut mesh = primitives::sphere_mesh(1.0, 16, 8);
        cylindrical_projection(&mut mesh, 0.3, 0.3);
        relax_uvs(&mut mesh, 4, 0.5, true);
        assert!(mesh.uvs.iter().all(|uv| uv.is_finite()));
    }
}
