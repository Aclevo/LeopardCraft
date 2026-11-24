//! UV projection, chart packing and relaxation.

use crate::Mesh;
use glam::{Vec2, Vec3};
use std::collections::{HashMap, HashSet, VecDeque};

/// Normal-angle threshold used to split Relax charts, matching the editor's
/// default Unwrap UV angle so both tools agree on where a hard border is.
pub const RELAX_CHART_ANGLE_DEG: f32 = 45.0;

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
/// Write per-corner projection values into one UV set.
///
/// Set 0 rebuilds the slot topology (the historical projection behaviour); an
/// additional set already shares UV0's slots, so its coordinates are written
/// through each corner's slot instead.
fn assign_corner_uvs_in_set(mesh: &mut Mesh, set: usize, uvs: Vec<Vec2>) {
    if set == 0 {
        assign_corner_uvs(mesh, uvs);
        return;
    }
    let mut values = Vec::with_capacity(uvs.len());
    let mut index = 0usize;
    for poly in &mesh.polygons {
        for corner in &poly.corners {
            values.push((corner.uv, uvs.get(index).copied().unwrap_or(Vec2::ZERO)));
            index += 1;
        }
    }
    let Some(target) = mesh.uv_set_mut(set) else {
        return;
    };
    for (slot, uv) in values {
        if let Some(value) = target.get_mut(slot as usize) {
            *value = uv;
        }
    }
}

pub fn planar_projection(mesh: &mut Mesh, axis: usize, scale: f32) {
    planar_projection_in_set(mesh, 0, axis, scale);
}

/// Planar projection into any UV set. UV0 authors the slot layout; additional
/// sets keep the shared slots and only rewrite their coordinates.
pub fn planar_projection_in_set(mesh: &mut Mesh, set: usize, axis: usize, scale: f32) {
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
    assign_corner_uvs_in_set(mesh, set, uvs);
}

/// Per-face box projection: each face uses the plane most aligned with its normal.
pub fn box_projection(mesh: &mut Mesh, scale: f32) {
    box_projection_in_set(mesh, 0, scale);
}

pub fn box_projection_in_set(mesh: &mut Mesh, set: usize, scale: f32) {
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
    assign_corner_uvs_in_set(mesh, set, uvs);
}

pub fn cylindrical_projection(mesh: &mut Mesh, scale: f32, height_scale: f32) {
    cylindrical_projection_in_set(mesh, 0, scale, height_scale);
}

pub fn cylindrical_projection_in_set(mesh: &mut Mesh, set: usize, scale: f32, height_scale: f32) {
    let s = if scale.abs() < 1e-6 { 1.0 } else { scale };
    let pts = corner_positions(mesh);
    let uvs = pts
        .iter()
        .map(|p| {
            let u = p.z.atan2(p.x) / std::f32::consts::TAU + 0.5;
            Vec2::new(u * s, p.y * height_scale)
        })
        .collect();
    assign_corner_uvs_in_set(mesh, set, uvs);
}

pub fn spherical_projection(mesh: &mut Mesh, scale: f32) {
    spherical_projection_in_set(mesh, 0, scale);
}

pub fn spherical_projection_in_set(mesh: &mut Mesh, set: usize, scale: f32) {
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
    assign_corner_uvs_in_set(mesh, set, uvs);
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
/// A chart is a group of connected faces whose normals stay within
/// [`RELAX_CHART_ANGLE_DEG`], cut by authored seams and by the same normal-angle
/// borders `automatic_unwrap` uses. Neither a seam nor a sharp normal break is
/// averaged across, so each side keeps its own UV layout instead of collapsing
/// onto one shared point value. Corners on a chart border, meaning a seam edge,
/// a normal-angle break or a mesh boundary, are optionally held so a chart
/// cannot shrink away from its cut, and pinned UV slots are always hard
/// constraints. Without pins the smoothed charts are re-packed into 0..1 so two
/// charts cannot drift into each other's bounds.
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
    let normals: Vec<Vec3> = (0..face_count).map(|face| mesh.face_normal(face)).collect();
    let cos_limit = RELAX_CHART_ANGLE_DEG.to_radians().cos();

    // Charts: connected faces across edges that are neither seams nor
    // normal-angle breaks.
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
                        if chart_of[other] == usize::MAX
                            && normals[other].dot(normals[face]) >= cos_limit
                        {
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

    // Border corners: endpoints of a seam edge, a normal-angle break or a mesh
    // boundary edge, matching the charts above.
    let mut border = vec![false; mesh.positions.len()];
    for edge in &edges {
        let normal_break = edge.faces.len() == 2 && {
            let (first, second) = (edge.faces[0] as usize, edge.faces[1] as usize);
            normals[first].dot(normals[second]) < cos_limit
        };
        if edge.faces.len() == 1 || seams.contains(&(edge.a, edge.b)) || normal_break {
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
        // Pack the smoothed charts back apart, then fit the layout into 0..1
        // while preserving aspect. Pins are excluded because packing would
        // move their authored coordinates.
        repack_relaxed_charts(&corner_start, &chart_of, &mut current);
        normalize_into_unit(&mut current);
    }
    // Pins are hard constraints: their slots keep their authored coordinate and
    // the layout is not re-fitted into 0..1, because that would move them.
    // Relaxation stays a clean-room per-chart Laplacian average, not a
    // parameterisation solve.
    assign_corner_uvs_keeping_pins(mesh, current);
}

/// Shelf-pack relaxed chart UVs into a virtual square so charts that drifted
/// into each other's bounds are separated again. Each chart keeps its relaxed
/// shape; only a per-chart translation and one global uniform scale are applied,
/// so the result is deterministic and stays finite for extreme coordinates.
fn repack_relaxed_charts(corner_start: &[usize], chart_of: &[usize], values: &mut [Vec2]) {
    if values.is_empty() {
        return;
    }
    let mut charts: Vec<usize> = chart_of
        .iter()
        .copied()
        .filter(|chart| *chart != usize::MAX)
        .collect();
    charts.sort_unstable();
    charts.dedup();
    if charts.is_empty() {
        return;
    }
    let mut bounds = Vec::with_capacity(charts.len());
    for chart in &charts {
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for (face, &face_chart) in chart_of.iter().enumerate() {
            if face_chart != *chart {
                continue;
            }
            let start = corner_start[face];
            let end = corner_start
                .get(face + 1)
                .copied()
                .unwrap_or(values.len())
                .min(values.len());
            for value in &values[start..end] {
                min[0] = min[0].min(value.x as f64);
                min[1] = min[1].min(value.y as f64);
                max[0] = max[0].max(value.x as f64);
                max[1] = max[1].max(value.y as f64);
            }
        }
        if !min[0].is_finite() || !min[1].is_finite() || !max[0].is_finite() || !max[1].is_finite()
        {
            min = [0.0, 0.0];
            max = [1e-5, 1e-5];
        }
        bounds.push((min, max));
    }

    const PADDING: f64 = 0.01;
    const SHELF_WIDTH: f64 = 0.96;
    let mut order: Vec<usize> = (0..charts.len()).collect();
    order.sort_by(|&a, &b| {
        let height_a = bounds[a].1[1] - bounds[a].0[1];
        let height_b = bounds[b].1[1] - bounds[b].0[1];
        height_b
            .partial_cmp(&height_a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| charts[a].cmp(&charts[b]))
    });
    let mut cursor = [PADDING, PADDING];
    let mut row_height = 0.0f64;
    let mut placements = vec![[0.0f64; 2]; charts.len()];
    for &slot in &order {
        let (min, max) = bounds[slot];
        let size = [(max[0] - min[0]).max(1e-5), (max[1] - min[1]).max(1e-5)];
        if cursor[0] + size[0] > SHELF_WIDTH {
            cursor[0] = PADDING;
            cursor[1] += row_height + PADDING;
            row_height = 0.0;
        }
        placements[slot] = cursor;
        cursor[0] += size[0] + PADDING;
        row_height = row_height.max(size[1]);
    }
    let total_height = (cursor[1] + row_height + PADDING).max(1e-5);
    let scale = (1.0 / SHELF_WIDTH).min(1.0 / total_height);
    for (face, &chart) in chart_of.iter().enumerate() {
        let Some(slot) = charts.iter().position(|candidate| *candidate == chart) else {
            continue;
        };
        let (min, _) = bounds[slot];
        let placement = placements[slot];
        let start = corner_start[face];
        let end = corner_start
            .get(face + 1)
            .copied()
            .unwrap_or(values.len())
            .min(values.len());
        for value in &mut values[start..end] {
            value.x = ((placement[0] + (value.x as f64 - min[0])) * scale) as f32;
            value.y = ((placement[1] + (value.y as f64 - min[1])) * scale) as f32;
        }
    }
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

// ---------------------------------------------------------------------------
// Chart authoring, packing and UV slot operations.
// ---------------------------------------------------------------------------

/// Axis or axes a UV operation acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UvAxis {
    U,
    V,
    Both,
}

/// Where a packed block sits inside 0..1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum UvPackAlign {
    #[default]
    BottomLeft,
    TopLeft,
    Center,
}

/// Packing controls exposed by the UV chart tools.
#[derive(Clone, Copy, Debug)]
pub struct UvPackOptions {
    /// Gap between charts, in normalized UV units.
    pub padding: f32,
    /// Rotate a chart by 90° when that gives it a shorter wide box.
    pub rotate: bool,
    pub align: UvPackAlign,
}

impl Default for UvPackOptions {
    fn default() -> Self {
        Self {
            padding: 0.01,
            rotate: false,
            align: UvPackAlign::BottomLeft,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UvPackReport {
    pub packed: usize,
    /// Charts left untouched because they contain pinned UV slots.
    pub skipped_pinned: usize,
}

/// One unwrap chart: the faces it contains and the UV slots those faces use.
#[derive(Clone, Debug, PartialEq)]
pub struct UvChart {
    pub faces: Vec<u32>,
    pub slots: Vec<u32>,
}

/// Chart index per polygon, cut by authored seams and the same normal-angle
/// threshold the unwrap and relax tools use. Chart ids are dense and ordered
/// by their lowest face index, so the result is deterministic.
pub fn uv_chart_indices(mesh: &Mesh) -> Vec<usize> {
    let face_count = mesh.polygons.len();
    if face_count == 0 {
        return Vec::new();
    }
    let normals: Vec<Vec3> = (0..face_count).map(|i| mesh.face_normal(i)).collect();
    let mut edge_faces: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    for edge in mesh.edges() {
        edge_faces.insert((edge.a, edge.b), edge.faces);
    }
    let seams: HashSet<(u32, u32)> = mesh
        .seams
        .iter()
        .map(|[a, b]| if a < b { (*a, *b) } else { (*b, *a) })
        .collect();
    let cos_limit = RELAX_CHART_ANGLE_DEG.to_radians().cos();
    let mut chart_of = vec![usize::MAX; face_count];
    let mut next_chart = 0usize;
    for seed in 0..face_count {
        if chart_of[seed] != usize::MAX {
            continue;
        }
        let chart = next_chart;
        next_chart += 1;
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
                        if chart_of[other] == usize::MAX
                            && normals[other].dot(normals[face]) >= cos_limit
                        {
                            chart_of[other] = chart;
                            queue.push_back(other);
                        }
                    }
                }
            }
        }
    }
    chart_of
}

/// Every chart with its faces and unique UV slots, in chart order.
pub fn uv_charts(mesh: &Mesh) -> Vec<UvChart> {
    let chart_of = uv_chart_indices(mesh);
    let count = chart_of.iter().copied().filter(|c| *c != usize::MAX).max();
    let Some(last) = count else {
        return Vec::new();
    };
    let mut charts: Vec<UvChart> = (0..=last)
        .map(|_| UvChart {
            faces: Vec::new(),
            slots: Vec::new(),
        })
        .collect();
    for (face, &chart) in chart_of.iter().enumerate() {
        if chart == usize::MAX {
            continue;
        }
        charts[chart].faces.push(face as u32);
        for corner in &mesh.polygons[face].corners {
            charts[chart].slots.push(corner.uv);
        }
    }
    for chart in &mut charts {
        chart.slots.sort_unstable();
        chart.slots.dedup();
    }
    charts.retain(|chart| !chart.faces.is_empty());
    charts
}

/// Charts that contain any of the given UV slots.
pub fn charts_for_uv_slots(mesh: &Mesh, slots: &[u32]) -> Vec<usize> {
    let selected: HashSet<u32> = slots.iter().copied().collect();
    if selected.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for (chart, entry) in uv_charts(mesh).into_iter().enumerate() {
        if entry.slots.iter().any(|slot| selected.contains(slot)) {
            hits.push(chart);
        }
    }
    hits
}

/// All UV slots of the islands that contain `slots`, sorted and deduplicated.
/// This is the editor's island selection: a chart is one UV island.
pub fn uv_island_slots(mesh: &Mesh, slots: &[u32]) -> Vec<u32> {
    let charts = uv_charts(mesh);
    let mut out = Vec::new();
    for chart in charts_for_uv_slots(mesh, slots) {
        out.extend_from_slice(&charts[chart].slots);
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn valid_slots(mesh: &Mesh, set: usize, slots: &[u32]) -> Vec<u32> {
    let Some(values) = mesh.uv_set(set) else {
        return Vec::new();
    };
    let mut valid: Vec<u32> = slots
        .iter()
        .copied()
        .filter(|slot| (*slot as usize) < values.len())
        .collect();
    valid.sort_unstable();
    valid.dedup();
    valid
}

fn chart_slot_set(charts: &[UvChart], selected: &[usize]) -> Vec<u32> {
    let mut slots = Vec::new();
    for &chart in selected {
        if let Some(entry) = charts.get(chart) {
            slots.extend_from_slice(&entry.slots);
        }
    }
    slots.sort_unstable();
    slots.dedup();
    slots
}

fn centroid(values: &[Vec2], slots: &[u32]) -> Vec2 {
    let mut sum = Vec2::ZERO;
    let mut count = 0usize;
    for &slot in slots {
        if let Some(value) = values.get(slot as usize) {
            sum += *value;
            count += 1;
        }
    }
    if count == 0 {
        Vec2::ZERO
    } else {
        sum / count as f32
    }
}

/// Move, rotate and scale whole charts around their combined centre. Charts
/// are addressable by `uv_charts`/`charts_for_uv_slots`; only the selected
/// set's coordinates change, while pin flags stay on their slots. Returns the
/// number of UV slots touched.
pub fn transform_uv_charts(
    mesh: &mut Mesh,
    set: usize,
    charts: &[usize],
    translation: Vec2,
    rotation_deg: f32,
    scale: f32,
) -> usize {
    let entries = uv_charts(mesh);
    let mut slots = chart_slot_set(&entries, charts);
    slots.retain(|slot| valid_slots(mesh, set, &[*slot]).len() == 1);
    if slots.is_empty() {
        return 0;
    }
    let factor = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let angle = if rotation_deg.is_finite() {
        rotation_deg.to_radians()
    } else {
        0.0
    };
    let (sin, cos) = angle.sin_cos();
    let Some(values) = mesh.uv_set_mut(set) else {
        return 0;
    };
    let centre = centroid(values, &slots);
    let translation = if translation.is_finite() {
        translation
    } else {
        Vec2::ZERO
    };
    let mut touched = 0usize;
    for slot in slots {
        let Some(value) = values.get_mut(slot as usize) else {
            continue;
        };
        let local = (*value - centre) * factor;
        *value = centre
            + Vec2::new(local.x * cos - local.y * sin, local.x * sin + local.y * cos)
            + translation;
        touched += 1;
    }
    touched
}

/// Shelf-pack selected charts into 0..1 with explicit padding, optional 90°
/// chart rotation and block alignment. Charts containing pinned slots are left
/// exactly as authored so a pin never moves; the report counts them.
pub fn pack_uv_charts(
    mesh: &mut Mesh,
    set: usize,
    charts: &[usize],
    options: UvPackOptions,
) -> UvPackReport {
    let entries = uv_charts(mesh);
    let selected: Vec<usize> = if charts.is_empty() {
        (0..entries.len()).collect()
    } else {
        let mut selected: Vec<usize> = charts
            .iter()
            .copied()
            .filter(|chart| *chart < entries.len())
            .collect();
        selected.sort_unstable();
        selected.dedup();
        selected
    };
    if selected.is_empty() {
        return UvPackReport {
            packed: 0,
            skipped_pinned: 0,
        };
    }
    struct Entry {
        chart: usize,
        min: Vec2,
        max: Vec2,
    }
    let mut items = Vec::new();
    let mut skipped_pinned = 0usize;
    for &chart in &selected {
        let Some(entry) = entries.get(chart) else {
            continue;
        };
        if entry.slots.iter().any(|slot| mesh.is_uv_pinned(*slot)) {
            skipped_pinned += 1;
            continue;
        }
        let Some(values) = mesh.uv_set(set) else {
            continue;
        };
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for &slot in &entry.slots {
            if let Some(value) = values.get(slot as usize) {
                min = min.min(*value);
                max = max.max(*value);
            }
        }
        if !min.is_finite() || !max.is_finite() {
            continue;
        }
        if options.rotate && (max.y - min.y) > (max.x - min.x) {
            let centre = (min + max) * 0.5;
            let mut rotated_min = Vec2::splat(f32::INFINITY);
            let mut rotated_max = Vec2::splat(f32::NEG_INFINITY);
            let values = mesh.uv_set_mut(set).expect("set existed above");
            for &slot in &entry.slots {
                let Some(value) = values.get_mut(slot as usize) else {
                    continue;
                };
                let local = *value - centre;
                *value = centre + Vec2::new(-local.y, local.x);
                rotated_min = rotated_min.min(*value);
                rotated_max = rotated_max.max(*value);
            }
            if rotated_min.is_finite() && rotated_max.is_finite() {
                min = rotated_min;
                max = rotated_max;
            }
        }
        items.push(Entry { chart, min, max });
    }
    if items.is_empty() {
        return UvPackReport {
            packed: 0,
            skipped_pinned,
        };
    }
    let extent = items
        .iter()
        .map(|item| (item.max - item.min).max_element())
        .fold(0.0f32, f32::max)
        .max(1e-5);
    let padding = if options.padding.is_finite() {
        options.padding.clamp(0.0, 0.2)
    } else {
        0.0
    };
    let shelf_width = (0.96 - padding).max(0.2);
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| {
        let height_a = (items[a].max.y - items[a].min.y) / extent;
        let height_b = (items[b].max.y - items[b].min.y) / extent;
        height_b
            .partial_cmp(&height_a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| items[a].chart.cmp(&items[b].chart))
    });
    let mut cursor = Vec2::splat(padding);
    let mut row_height = 0.0f32;
    let mut placements = vec![Vec2::ZERO; items.len()];
    let mut block_width = 0.0f32;
    for &slot in &order {
        let size = (items[slot].max - items[slot].min) / extent;
        let width = size.x.max(1e-5);
        let height = size.y.max(1e-5);
        if cursor.x + width > shelf_width {
            cursor.x = padding;
            cursor.y += row_height + padding;
            row_height = 0.0;
        }
        placements[slot] = cursor;
        cursor.x += width + padding;
        block_width = block_width.max(cursor.x);
        row_height = row_height.max(height);
    }
    let block_height = cursor.y + row_height + padding;
    let fit = 1.0 / block_width.max(block_height).max(1e-5);
    let align = match options.align {
        UvPackAlign::BottomLeft => Vec2::ZERO,
        UvPackAlign::TopLeft => Vec2::new(0.0, 1.0 - block_height * fit),
        UvPackAlign::Center => Vec2::new(
            (1.0 - block_width * fit) * 0.5,
            (1.0 - block_height * fit) * 0.5,
        ),
    };
    for (slot, item) in items.iter().enumerate() {
        let placement = placements[slot];
        let Some(values) = mesh.uv_set_mut(set) else {
            break;
        };
        for &uv_slot in &entries[item.chart].slots {
            let Some(value) = values.get_mut(uv_slot as usize) else {
                continue;
            };
            let local = (*value - item.min) / extent;
            *value = align + (placement + local) * fit;
        }
    }
    UvPackReport {
        packed: items.len(),
        skipped_pinned,
    }
}

/// Append one UV slot to every set. Values beyond the set count become zero.
fn push_uv_slot(mesh: &mut Mesh, values: &[Vec2]) -> u32 {
    let slot = mesh.uvs.len() as u32;
    mesh.uvs.push(values.first().copied().unwrap_or(Vec2::ZERO));
    for (index, set) in mesh.extra_uvs.iter_mut().enumerate() {
        set.push(values.get(index + 1).copied().unwrap_or(Vec2::ZERO));
    }
    slot
}

/// Drop UV slots no polygon corner uses, across every set, and remap pins.
/// Returns how many slots were removed.
fn remove_unused_uv_slots(mesh: &mut Mesh) -> usize {
    let mut used = vec![false; mesh.uvs.len()];
    for poly in &mesh.polygons {
        for corner in &poly.corners {
            if let Some(flag) = used.get_mut(corner.uv as usize) {
                *flag = true;
            }
        }
    }
    let mut remap = vec![u32::MAX; used.len()];
    let mut new_uvs = Vec::new();
    let mut new_extras: Vec<Vec<Vec2>> = vec![Vec::new(); mesh.extra_uvs.len()];
    for (slot, keep) in used.iter().enumerate() {
        if !keep {
            continue;
        }
        remap[slot] = new_uvs.len() as u32;
        new_uvs.push(mesh.uvs[slot]);
        for (set, values) in new_extras.iter_mut().enumerate() {
            values.push(
                mesh.extra_uvs
                    .get(set)
                    .and_then(|values| values.get(slot))
                    .copied()
                    .unwrap_or(Vec2::ZERO),
            );
        }
    }
    let removed = used.len() - new_uvs.len();
    if removed == 0 {
        return 0;
    }
    for poly in &mut mesh.polygons {
        for corner in &mut poly.corners {
            if let Some(mapped) = remap.get(corner.uv as usize) {
                if *mapped != u32::MAX {
                    corner.uv = *mapped;
                }
            }
        }
    }
    mesh.uvs = new_uvs;
    mesh.extra_uvs = new_extras;
    for pin in &mut mesh.pinned_uvs {
        if let Some(mapped) = remap.get(*pin as usize) {
            *pin = *mapped;
        }
    }
    mesh.normalize_pinned_uvs();
    removed
}

/// Weld selected UV slots that belong to the same mesh point. The lowest slot
/// index survives, every corner using a merged slot follows it, unused slots
/// are removed from every set and pins follow the surviving index. Returns the
/// number of slots removed.
pub fn weld_uv_slots(mesh: &mut Mesh, slots: &[u32]) -> usize {
    let selected: Vec<u32> = {
        let mut selected = valid_slots(mesh, 0, slots);
        selected.sort_unstable();
        selected
    };
    if selected.len() < 2 {
        return 0;
    }
    let mut point_of: HashMap<u32, u32> = HashMap::new();
    for poly in &mesh.polygons {
        for corner in &poly.corners {
            point_of.entry(corner.uv).or_insert(corner.point);
        }
    }
    let mut groups: std::collections::BTreeMap<u32, Vec<u32>> = std::collections::BTreeMap::new();
    for slot in &selected {
        if let Some(point) = point_of.get(slot) {
            groups.entry(*point).or_default().push(*slot);
        }
    }
    let mut survivor = HashMap::new();
    for group in groups.values() {
        if group.len() < 2 {
            continue;
        }
        let first = group[0];
        for slot in &group[1..] {
            survivor.insert(*slot, first);
        }
    }
    if survivor.is_empty() {
        return 0;
    }
    for poly in &mut mesh.polygons {
        for corner in &mut poly.corners {
            if let Some(target) = survivor.get(&corner.uv) {
                corner.uv = *target;
            }
        }
    }
    remove_unused_uv_slots(mesh)
}

/// Split selected UV slots so each polygon corner owns its own slot. A slot
/// used by one corner is left alone. The new slot copies every set's
/// coordinate and inherits a pin. Returns the number of slots created.
pub fn split_uv_slots(mesh: &mut Mesh, slots: &[u32]) -> usize {
    let selected = valid_slots(mesh, 0, slots);
    if selected.is_empty() {
        return 0;
    }
    let mut usage: HashMap<u32, usize> = HashMap::new();
    for poly in &mesh.polygons {
        for corner in &poly.corners {
            *usage.entry(corner.uv).or_insert(0) += 1;
        }
    }
    let mut targets = Vec::new();
    for (pi, poly) in mesh.polygons.iter().enumerate() {
        for (ci, corner) in poly.corners.iter().enumerate() {
            if selected.contains(&corner.uv) && usage.get(&corner.uv).copied().unwrap_or(0) > 1 {
                targets.push((pi, ci, corner.uv));
            }
        }
    }
    let mut created = 0usize;
    let sets = mesh.uv_set_count();
    for (pi, ci, slot) in targets {
        let values: Vec<Vec2> = (0..sets)
            .map(|set| mesh.uv_value(set, slot).unwrap_or(Vec2::ZERO))
            .collect();
        let pinned = mesh.is_uv_pinned(slot);
        let next = push_uv_slot(mesh, &values);
        mesh.polygons[pi].corners[ci].uv = next;
        if pinned {
            mesh.set_uv_pinned(next, true);
        }
        created += 1;
    }
    mesh.normalize_pinned_uvs();
    created
}

/// Align selected slots on one axis (or both) to their average coordinate.
pub fn align_uv_slots(mesh: &mut Mesh, set: usize, slots: &[u32], axis: UvAxis) -> bool {
    let selected = valid_slots(mesh, set, slots);
    if selected.len() < 2 {
        return false;
    }
    let Some(values) = mesh.uv_set_mut(set) else {
        return false;
    };
    let centre = centroid(values, &selected);
    for slot in &selected {
        if let Some(value) = values.get_mut(*slot as usize) {
            if matches!(axis, UvAxis::U | UvAxis::Both) {
                value.x = centre.x;
            }
            if matches!(axis, UvAxis::V | UvAxis::Both) {
                value.y = centre.y;
            }
        }
    }
    true
}

/// Space selected slots evenly along one axis between the current extremes.
/// Ordering is deterministic (coordinate, then slot index).
pub fn distribute_uv_slots(mesh: &mut Mesh, set: usize, slots: &[u32], axis: UvAxis) -> bool {
    let selected = valid_slots(mesh, set, slots);
    if selected.len() < 3 || axis == UvAxis::Both {
        return false;
    }
    let Some(values) = mesh.uv_set_mut(set) else {
        return false;
    };
    let key = |value: Vec2| if axis == UvAxis::U { value.x } else { value.y };
    let mut ordered = selected.clone();
    ordered.sort_by(|a, b| {
        let va = values.get(*a as usize).copied().unwrap_or(Vec2::ZERO);
        let vb = values.get(*b as usize).copied().unwrap_or(Vec2::ZERO);
        key(va)
            .partial_cmp(&key(vb))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cmp(b))
    });
    let first = values
        .get(ordered[0] as usize)
        .copied()
        .unwrap_or(Vec2::ZERO);
    let last = values
        .get(*ordered.last().unwrap() as usize)
        .copied()
        .unwrap_or(Vec2::ZERO);
    let start = key(first);
    let end = key(last);
    let steps = (ordered.len() - 1) as f32;
    for (index, slot) in ordered.iter().enumerate() {
        let Some(value) = values.get_mut(*slot as usize) else {
            continue;
        };
        let coordinate = start + (end - start) * index as f32 / steps;
        if axis == UvAxis::U {
            value.x = coordinate;
        } else {
            value.y = coordinate;
        }
    }
    true
}

/// Mirror selected slots across their centre on one axis.
pub fn flip_uv_slots(mesh: &mut Mesh, set: usize, slots: &[u32], axis: UvAxis) -> bool {
    let selected = valid_slots(mesh, set, slots);
    if selected.len() < 2 || axis == UvAxis::Both {
        return false;
    }
    let Some(values) = mesh.uv_set_mut(set) else {
        return false;
    };
    let centre = centroid(values, &selected);
    for slot in &selected {
        if let Some(value) = values.get_mut(*slot as usize) {
            if axis == UvAxis::U {
                value.x = centre.x * 2.0 - value.x;
            } else {
                value.y = centre.y * 2.0 - value.y;
            }
        }
    }
    true
}

/// Rotate selected slots around their centre in 90° steps.
pub fn rotate_uv_slots(mesh: &mut Mesh, set: usize, slots: &[u32], quarter_turns: i32) -> bool {
    let selected = valid_slots(mesh, set, slots);
    if selected.is_empty() {
        return false;
    }
    let turns = quarter_turns.rem_euclid(4);
    if turns == 0 {
        return false;
    }
    let Some(values) = mesh.uv_set_mut(set) else {
        return false;
    };
    let centre = centroid(values, &selected);
    for slot in &selected {
        if let Some(value) = values.get_mut(*slot as usize) {
            let mut local = *value - centre;
            for _ in 0..turns {
                local = Vec2::new(-local.y, local.x);
            }
            *value = centre + local;
        }
    }
    true
}

/// Area ratio between one face's UVs and its 3D surface area, normalized so
/// an even layout has ratio 1. Degenerate faces report 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UvFaceDistortion {
    pub polygon: u32,
    pub uv_area: f32,
    pub surface_area: f32,
    /// `max(ratio, 1 / ratio)`; 1 is undistorted, 2 means 2× stretch.
    pub stretch: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UvDistortionStats {
    pub faces: usize,
    pub mean: f32,
    pub max: f32,
    /// Faces stretched or squashed by more than 2×.
    pub distorted: usize,
}

fn polygon_world_area(mesh: &Mesh, face: usize) -> f32 {
    let points = mesh.polygon_points(face);
    if points.len() < 3 {
        return 0.0;
    }
    let mut normal = Vec3::ZERO;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        normal += a.cross(b);
    }
    normal.length() * 0.5
}

fn polygon_uv_area(mesh: &Mesh, face: usize, set: usize) -> f32 {
    let mut area = 0.0f32;
    let Some(poly) = mesh.polygons.get(face) else {
        return 0.0;
    };
    for i in 0..poly.corners.len() {
        let a = mesh.uv_of_set(&poly.corners[i], set);
        let b = mesh.uv_of_set(&poly.corners[(i + 1) % poly.corners.len()], set);
        area += a.x * b.y - b.x * a.y;
    }
    area.abs() * 0.5
}

/// Per-face UV stretch for one set, normalized against the mesh's overall UV
/// density so uniform texel density reports 1 everywhere.
pub fn uv_face_distortion(mesh: &Mesh, set: usize) -> Vec<UvFaceDistortion> {
    let mut uv_total = 0.0f32;
    let mut world_total = 0.0f32;
    for face in 0..mesh.polygons.len() {
        uv_total += polygon_uv_area(mesh, face, set);
        world_total += polygon_world_area(mesh, face);
    }
    let mut out = Vec::with_capacity(mesh.polygons.len());
    for face in 0..mesh.polygons.len() {
        let uv_area = polygon_uv_area(mesh, face, set);
        let surface_area = polygon_world_area(mesh, face);
        let density = if uv_total > 1e-12 && world_total > 1e-12 {
            (uv_area / uv_total) / (surface_area / world_total)
        } else {
            1.0
        };
        let stretch = if density.is_finite() && density > 1e-9 {
            density.max(1.0 / density)
        } else {
            1.0
        };
        out.push(UvFaceDistortion {
            polygon: face as u32,
            uv_area,
            surface_area,
            stretch,
        });
    }
    out
}

pub fn uv_distortion_stats(mesh: &Mesh, set: usize) -> Option<UvDistortionStats> {
    let faces = uv_face_distortion(mesh, set);
    if faces.is_empty() {
        return None;
    }
    let mean = faces.iter().map(|face| face.stretch).sum::<f32>() / faces.len() as f32;
    let max = faces.iter().map(|face| face.stretch).fold(0.0f32, f32::max);
    let distorted = faces.iter().filter(|face| face.stretch > 2.0).count();
    Some(UvDistortionStats {
        faces: faces.len(),
        mean,
        max,
        distorted,
    })
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

    #[test]
    fn automatic_unwrap_packs_disconnected_charts_without_overlap() {
        // Four disconnected quads form four charts. The shelf packer must keep
        // their UV bounds inside 0..1 and pairwise disjoint, not only inside
        // the unit square.
        let mut positions = Vec::new();
        let mut polygons = Vec::new();
        for i in 0..4u32 {
            let x = i as f32 * 2.0;
            let base = positions.len() as u32;
            positions.extend([
                Vec3::new(x, 0.0, 0.0),
                Vec3::new(x, 0.0, 1.0),
                Vec3::new(x + 1.0, 0.0, 1.0),
                Vec3::new(x + 1.0, 0.0, 0.0),
            ]);
            polygons.push(vec![base, base + 1, base + 2, base + 3]);
        }
        let mut mesh = Mesh::from_polygons("charts", positions, polygons);
        automatic_unwrap(&mut mesh, 45.0, 0.01);

        let bounds: Vec<(Vec2, Vec2)> = (0..mesh.polygons.len())
            .map(|face| {
                let mut min = Vec2::splat(f32::INFINITY);
                let mut max = Vec2::splat(f32::NEG_INFINITY);
                for corner in &mesh.polygons[face].corners {
                    let uv = mesh.uv_of(corner);
                    min = min.min(uv);
                    max = max.max(uv);
                }
                (min, max)
            })
            .collect();
        const EPS: f32 = 1e-5;
        for (chart, (min, max)) in bounds.iter().enumerate() {
            assert!(
                min.x >= -EPS && min.y >= -EPS && max.x <= 1.0 + EPS && max.y <= 1.0 + EPS,
                "chart {chart} escaped the unit square: {min:?}..{max:?}"
            );
            assert!(
                max.x - min.x > EPS && max.y - min.y > EPS,
                "chart {chart} collapsed: {min:?}..{max:?}"
            );
        }
        for a in 0..bounds.len() {
            for b in (a + 1)..bounds.len() {
                let (amin, amax) = bounds[a];
                let (bmin, bmax) = bounds[b];
                let overlap_x = amin.x < bmax.x - EPS && bmin.x < amax.x - EPS;
                let overlap_y = amin.y < bmax.y - EPS && bmin.y < amax.y - EPS;
                assert!(
                    !(overlap_x && overlap_y),
                    "charts {a} and {b} overlap: {amin:?}..{amax:?} vs {bmin:?}..{bmax:?}"
                );
            }
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

    fn folded_quad_strip() -> Mesh {
        // Two quads sharing the edge between points 1 and 2; the second quad is
        // turned 90 degrees, so the shared edge is a normal-angle chart border.
        Mesh::from_polygons(
            "fold",
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(1.0, 1.0, 1.0),
            ],
            vec![vec![0, 3, 2, 1], vec![1, 2, 5, 4]],
        )
    }

    #[test]
    fn relax_splits_charts_on_the_unwraps_normal_angle_borders() {
        let mut mesh = folded_quad_strip();
        planar_projection(&mut mesh, 1, 1.0);
        let shared = mesh
            .edges()
            .into_iter()
            .find(|edge| edge.faces.len() == 2)
            .expect("the fold shares one edge");
        assert!(
            mesh.face_normal(0).dot(mesh.face_normal(1)) < RELAX_CHART_ANGLE_DEG.to_radians().cos(),
            "the fixture must fold beyond the chart angle"
        );
        for k in 0..mesh.polygons[1].corners.len() {
            let slot = mesh.polygons[1].corners[k].uv as usize;
            mesh.uvs[slot] += Vec2::new(0.6, 0.0);
        }
        let gap = |mesh: &Mesh| {
            (face_point_uv(mesh, 0, shared.a).x - face_point_uv(mesh, 1, shared.a).x).abs()
        };
        assert!(gap(&mesh) > 0.5, "the charts start apart");

        relax_uvs(&mut mesh, 8, 0.5, false);
        assert!(
            gap(&mesh) > 0.1,
            "a normal-angle border must not be averaged across, gap {}",
            gap(&mesh)
        );
        assert!(mesh.uvs.iter().all(|uv| uv.is_finite()));
        assert!(mesh.uvs.iter().all(|uv| {
            uv.x >= -1e-4 && uv.x <= 1.0 + 1e-4 && uv.y >= -1e-4 && uv.y <= 1.0 + 1e-4
        }));
    }

    #[test]
    fn relax_repacks_disconnected_charts_without_overlap() {
        let mut positions = Vec::new();
        let mut polygons = Vec::new();
        for i in 0..4u32 {
            let x = i as f32 * 3.0;
            let base = positions.len() as u32;
            positions.extend([
                Vec3::new(x, 0.0, 0.0),
                Vec3::new(x, 0.0, 1.0),
                Vec3::new(x + 1.0, 0.0, 1.0),
                Vec3::new(x + 1.0, 0.0, 0.0),
            ]);
            polygons.push(vec![base, base + 1, base + 2, base + 3]);
        }
        let mut mesh = Mesh::from_polygons("relax-charts", positions, polygons);
        planar_projection(&mut mesh, 1, 1.0);
        relax_uvs(&mut mesh, 1, 0.25, false);

        let bounds: Vec<(Vec2, Vec2)> = (0..mesh.polygons.len())
            .map(|face| {
                let mut min = Vec2::splat(f32::INFINITY);
                let mut max = Vec2::splat(f32::NEG_INFINITY);
                for corner in &mesh.polygons[face].corners {
                    let uv = mesh.uv_of(corner);
                    min = min.min(uv);
                    max = max.max(uv);
                }
                (min, max)
            })
            .collect();
        const EPS: f32 = 1e-5;
        for (chart, (min, max)) in bounds.iter().enumerate() {
            assert!(
                min.x >= -EPS && min.y >= -EPS && max.x <= 1.0 + EPS && max.y <= 1.0 + EPS,
                "chart {chart} escaped the unit square: {min:?}..{max:?}"
            );
            assert!(
                max.x - min.x > EPS && max.y - min.y > EPS,
                "chart {chart} collapsed: {min:?}..{max:?}"
            );
        }
        for a in 0..bounds.len() {
            for b in (a + 1)..bounds.len() {
                let (amin, amax) = bounds[a];
                let (bmin, bmax) = bounds[b];
                let overlap_x = amin.x < bmax.x - EPS && bmin.x < amax.x - EPS;
                let overlap_y = amin.y < bmax.y - EPS && bmin.y < amax.y - EPS;
                assert!(
                    !(overlap_x && overlap_y),
                    "charts {a} and {b} overlap after relax: {amin:?}..{amax:?} vs {bmin:?}..{bmax:?}"
                );
            }
        }
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

    fn disconnected_quads(count: usize) -> Mesh {
        let mut positions = Vec::new();
        let mut polygons = Vec::new();
        for i in 0..count {
            let x = i as f32 * 2.0;
            let base = positions.len() as u32;
            positions.extend([
                Vec3::new(x, 0.0, 0.0),
                Vec3::new(x, 0.0, 1.0),
                Vec3::new(x + 1.0, 0.0, 1.0),
                Vec3::new(x + 1.0, 0.0, 0.0),
            ]);
            polygons.push(vec![base, base + 1, base + 2, base + 3]);
        }
        Mesh::from_polygons("quads", positions, polygons)
    }

    #[test]
    fn seams_split_charts_and_chart_transforms_stay_inside_their_chart() {
        let mut mesh = disconnected_quads(2);
        planar_projection(&mut mesh, 1, 1.0);
        assert_eq!(
            uv_charts(&mesh).len(),
            2,
            "disconnected quads are separate charts"
        );
        let charts = uv_charts(&mesh);
        let first = charts[0].slots.clone();
        let second = charts[1].slots.clone();
        let other_before: Vec<Vec2> = second.iter().map(|slot| mesh.uvs[*slot as usize]).collect();
        let moved = transform_uv_charts(&mut mesh, 0, &[0], Vec2::new(0.1, 0.02), 90.0, 0.5);
        assert_eq!(moved, first.len());
        for (slot, before) in second.iter().zip(&other_before) {
            assert_eq!(
                mesh.uvs[*slot as usize], *before,
                "transforming chart 0 must not touch chart 1"
            );
        }
        let once = mesh.uvs.clone();
        transform_uv_charts(&mut mesh, 0, &[0], Vec2::new(0.1, 0.02), 90.0, 0.5);
        let twice = mesh.uvs.clone();
        let mut reference = disconnected_quads(2);
        planar_projection(&mut reference, 1, 1.0);
        transform_uv_charts(&mut reference, 0, &[0], Vec2::new(0.1, 0.02), 90.0, 0.5);
        transform_uv_charts(&mut reference, 0, &[0], Vec2::new(0.1, 0.02), 90.0, 0.5);
        assert_eq!(twice, reference.uvs, "chart transforms are deterministic");
        assert_ne!(once, twice);
        assert!(mesh.uvs.iter().all(|uv| uv.is_finite()));
    }

    #[test]
    fn authored_seam_splits_a_connected_surface_into_charts() {
        let positions = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 1.0),
            Vec3::new(2.0, 0.0, 0.0),
        ];
        let mut mesh =
            Mesh::from_polygons("strip", positions, vec![vec![0, 1, 2, 3], vec![3, 2, 4, 5]]);
        assert_eq!(uv_chart_indices(&mesh), vec![0, 0]);
        mesh.seams.push([2, 3]);
        assert_eq!(uv_chart_indices(&mesh), vec![0, 1]);
        planar_projection(&mut mesh, 1, 1.0);
        let first = uv_charts(&mesh)[0].slots[0];
        let island = uv_island_slots(&mesh, &[first]);
        assert_eq!(island, uv_charts(&mesh)[0].slots);
        assert!(!island.contains(&uv_charts(&mesh)[1].slots[0]));
    }

    #[test]
    fn packing_controls_are_deterministic_and_leave_pinned_charts_fixed() {
        let mut mesh = disconnected_quads(4);
        planar_projection(&mut mesh, 1, 0.1);
        let charts = uv_charts(&mesh);
        assert_eq!(charts.len(), 4);
        let pinned_slot = charts[3].slots[0];
        mesh.set_uv_pinned(pinned_slot, true);
        let pinned_before = mesh.uvs[pinned_slot as usize];
        let options = UvPackOptions {
            padding: 0.02,
            rotate: true,
            align: UvPackAlign::Center,
        };
        let report = pack_uv_charts(&mut mesh, 0, &[], options);
        assert_eq!(report.packed, 3);
        assert_eq!(report.skipped_pinned, 1);
        assert_eq!(
            mesh.uvs[pinned_slot as usize], pinned_before,
            "a pinned chart keeps its authored coordinates"
        );
        let packed: Vec<usize> = (0..3).collect();
        for (face, chart) in charts.iter().enumerate().take(3) {
            let _ = face;
            for &slot in &chart.slots {
                let uv = mesh.uvs[slot as usize];
                assert!(
                    uv.x >= -1e-4 && uv.x <= 1.0 + 1e-4 && uv.y >= -1e-4 && uv.y <= 1.0 + 1e-4,
                    "packed chart {chart:?} escaped 0..1: {uv:?}"
                );
            }
        }
        let bounds = |mesh: &Mesh, chart: &UvChart| {
            let mut min = Vec2::splat(f32::INFINITY);
            let mut max = Vec2::splat(f32::NEG_INFINITY);
            for &slot in &chart.slots {
                min = min.min(mesh.uvs[slot as usize]);
                max = max.max(mesh.uvs[slot as usize]);
            }
            (min, max)
        };
        for a in packed.iter() {
            for b in packed.iter().filter(|b| *b > a) {
                let (amin, amax) = bounds(&mesh, &charts[*a]);
                let (bmin, bmax) = bounds(&mesh, &charts[*b]);
                let overlap_x = amin.x < bmax.x - 1e-5 && bmin.x < amax.x - 1e-5;
                let overlap_y = amin.y < bmax.y - 1e-5 && bmin.y < amax.y - 1e-5;
                assert!(
                    !(overlap_x && overlap_y),
                    "packed charts {a} and {b} overlap"
                );
            }
        }
        let mut again = disconnected_quads(4);
        planar_projection(&mut again, 1, 0.1);
        again.set_uv_pinned(pinned_slot, true);
        pack_uv_charts(&mut again, 0, &[], options);
        assert_eq!(mesh.uvs, again.uvs, "packing is deterministic");
    }

    #[test]
    fn split_weld_align_distribute_flip_and_rotate_edit_slots_only() {
        let mut mesh = Mesh::new("slots");
        mesh.add_uv(Vec2::ZERO);
        mesh.add_face(&[0, 1, 2, 3]);
        mesh.add_face(&[3, 2, 4, 5]);
        assert_eq!(mesh.uvs.len(), 1);
        assert_eq!(split_uv_slots(&mut mesh, &[0]), 8);
        assert_eq!(mesh.uvs.len(), 9);
        assert!(mesh
            .polygons
            .iter()
            .all(|poly| poly.corners.iter().all(|corner| corner.uv != 0)));
        let all: Vec<u32> = (0..mesh.uvs.len() as u32).collect();
        assert_eq!(weld_uv_slots(&mut mesh, &all), 3);
        assert_eq!(mesh.uvs.len(), 6, "unused and merged slots are removed");
        assert!(mesh.polygons[0].corners[3].uv == mesh.polygons[1].corners[0].uv);

        let mut mesh = disconnected_quads(2);
        planar_projection(&mut mesh, 1, 1.0);
        let slots: Vec<u32> = (0..4).collect();
        mesh.uv_set_mut(0).unwrap()[0] = Vec2::new(0.0, 0.1);
        mesh.uv_set_mut(0).unwrap()[1] = Vec2::new(0.2, 0.3);
        mesh.uv_set_mut(0).unwrap()[2] = Vec2::new(0.4, 0.5);
        mesh.uv_set_mut(0).unwrap()[3] = Vec2::new(1.0, 0.7);
        assert!(align_uv_slots(&mut mesh, 0, &slots, UvAxis::U));
        for slot in &slots {
            assert!((mesh.uvs[*slot as usize].x - 0.4).abs() < 1e-5);
        }
        assert!(distribute_uv_slots(&mut mesh, 0, &slots, UvAxis::V));
        let mut ys: Vec<f32> = slots
            .iter()
            .map(|slot| mesh.uvs[*slot as usize].y)
            .collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!((ys[0] - 0.1).abs() < 1e-4);
        assert!((ys[3] - 0.7).abs() < 1e-4);
        assert!((ys[1] - ys[0] - (ys[2] - ys[1])).abs() < 1e-4);
        let before_flip = mesh.uvs.clone();
        assert!(flip_uv_slots(&mut mesh, 0, &slots, UvAxis::V));
        assert_ne!(mesh.uvs, before_flip);
        assert!(flip_uv_slots(&mut mesh, 0, &slots, UvAxis::V));
        for (a, b) in mesh.uvs.iter().zip(&before_flip) {
            assert!((a.x - b.x).abs() < 1e-5 && (a.y - b.y).abs() < 1e-5);
        }
        let before_rotate = mesh.uvs.clone();
        assert!(rotate_uv_slots(&mut mesh, 0, &slots, 1));
        assert!(rotate_uv_slots(&mut mesh, 0, &slots, 3));
        for (a, b) in mesh.uvs.iter().zip(&before_rotate) {
            assert!((a.x - b.x).abs() < 1e-1 && (a.y - b.y).abs() < 1e-1);
        }
    }

    #[test]
    fn distortion_readout_flags_stretched_faces_and_even_quads_report_one() {
        let mut mesh = disconnected_quads(4);
        planar_projection(&mut mesh, 1, 1.0);
        let even = uv_face_distortion(&mesh, 0);
        assert_eq!(even.len(), 4);
        for face in &even {
            assert!(
                (face.stretch - 1.0).abs() < 1e-4,
                "even quads are undistorted: {face:?}"
            );
        }
        // Stretch the first face's UVs tenfold: the normalized readout must
        // flag it while the untouched faces stay near 1.
        let first: Vec<u32> = uv_charts(&mesh)[0].slots.clone();
        for slot in first {
            mesh.uvs[slot as usize].x *= 10.0;
        }
        let stats = uv_distortion_stats(&mesh, 0).expect("stats");
        assert_eq!(stats.faces, 4);
        assert!(stats.max > 2.0, "stretched face is visible: {stats:?}");
        assert!(stats.distorted >= 1);
        assert!(stats.mean.is_finite());
    }
}
