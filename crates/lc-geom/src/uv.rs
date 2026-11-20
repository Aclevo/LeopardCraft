//! UV projection, chart packing and relaxation.

use crate::Mesh;
use glam::{Vec2, Vec3};
use std::collections::{HashMap, HashSet, VecDeque};

fn assign_corner_uvs(mesh: &mut Mesh, uvs: Vec<Vec2>) {
    let mut k = 0u32;
    for poly in &mut mesh.polygons {
        for corner in &mut poly.corners {
            corner.uv = k;
            k += 1;
        }
    }
    mesh.uvs = uvs;
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

    // Raw 2D coordinates per chart, projected onto its average normal.
    struct ChartData {
        faces: Vec<usize>,
        uvs: Vec<Vec2>,
        bounds_min: Vec2,
        bounds_max: Vec2,
    }
    let mut data: Vec<ChartData> = Vec::new();
    for faces in charts {
        let mut avg = Vec3::ZERO;
        for &f in &faces {
            avg += normals[f];
        }
        let avg = avg.normalize_or(Vec3::Y);
        let (t, b) = lc_core::orthonormal_basis(avg);
        let mut uvs = Vec::new();
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for &f in &faces {
            for c in &mesh.polygons[f].corners {
                let p = mesh.positions[c.point as usize];
                let uv = Vec2::new(p.dot(t), p.dot(b));
                min = min.min(uv);
                max = max.max(uv);
                uvs.push(uv);
            }
        }
        data.push(ChartData {
            faces,
            uvs,
            bounds_min: min,
            bounds_max: max,
        });
    }

    // Shelf packing: sort by height, fill rows in a virtual square.
    data.sort_by(|a, b| {
        let ha = a.bounds_max.y - a.bounds_min.y;
        let hb = b.bounds_max.y - b.bounds_min.y;
        hb.partial_cmp(&ha).unwrap_or(std::cmp::Ordering::Equal)
    });
    let pad = padding.max(0.0);
    let mut cursor = Vec2::new(pad, pad);
    let mut row_height = 0.0f32;
    let mut placements: Vec<(Vec2, Vec2)> = Vec::with_capacity(data.len());
    let shelf_width = 0.96f32;
    for c in &data {
        let size = (c.bounds_max - c.bounds_min).max(Vec2::splat(1e-5));
        if cursor.x + size.x > shelf_width {
            cursor.x = pad;
            cursor.y += row_height + pad;
            row_height = 0.0;
        }
        placements.push((cursor, size));
        cursor.x += size.x + pad;
        row_height = row_height.max(size.y);
    }
    let total_height = (cursor.y + row_height + pad).max(1e-5);
    let scale = (1.0 / shelf_width).min(1.0 / total_height);

    // Assign corner UVs in the new order. Every corner gets a fresh UV slot so
    // unwrapping works on meshes that previously shared slots.
    let mut per_corner: Vec<Vec2> = Vec::new();
    for (ci, chart) in data.iter().enumerate() {
        let (offset, _size) = placements[ci];
        let sizef = (chart.bounds_max - chart.bounds_min).max(Vec2::splat(1e-5));
        let mut chart_cursor = 0usize;
        for &f in &chart.faces {
            let corners = mesh.polygons[f].corners.len();
            for k in 0..corners {
                let raw = chart.uvs[chart_cursor + k];
                let norm = Vec2::new(
                    (raw.x - chart.bounds_min.x) / sizef.x,
                    (raw.y - chart.bounds_min.y) / sizef.y,
                );
                per_corner.push((offset + norm * sizef) * scale);
            }
            chart_cursor += corners;
        }
    }
    assign_corner_uvs(mesh, per_corner);
}

/// Laplacian relaxation of existing UVs. Boundary points are optionally
/// pinned so charts do not shrink away from seams.
pub fn relax_uvs(mesh: &mut Mesh, iterations: u32, strength: f32, pin_boundary: bool) {
    let mut adjacency: Vec<HashSet<u32>> = vec![HashSet::new(); mesh.positions.len()];
    let mut boundary = vec![false; mesh.positions.len()];
    for e in mesh.edges() {
        adjacency[e.a as usize].insert(e.b);
        adjacency[e.b as usize].insert(e.a);
        if e.faces.len() == 1 {
            boundary[e.a as usize] = true;
            boundary[e.b as usize] = true;
        }
    }
    // Average authored UVs per point.
    let mut point_uv_sum = vec![Vec2::ZERO; mesh.positions.len()];
    let mut point_uv_count = vec![0u32; mesh.positions.len()];
    for poly in &mesh.polygons {
        for c in &poly.corners {
            if let Some(sum) = point_uv_sum.get_mut(c.point as usize) {
                *sum += mesh.uv_of(c);
                point_uv_count[c.point as usize] += 1;
            }
        }
    }
    let point_uv: Vec<Vec2> = (0..mesh.positions.len())
        .map(|i| {
            if point_uv_count[i] == 0 {
                Vec2::ZERO
            } else {
                point_uv_sum[i] / point_uv_count[i] as f32
            }
        })
        .collect();
    let mut current = point_uv;
    let s = strength.clamp(0.0, 1.0);
    for _ in 0..iterations {
        let mut next = current.clone();
        for p in 0..mesh.positions.len() {
            if adjacency[p].is_empty() || (pin_boundary && boundary[p]) {
                continue;
            }
            let avg = adjacency[p]
                .iter()
                .map(|&n| current[n as usize])
                .sum::<Vec2>()
                / adjacency[p].len() as f32;
            next[p] = current[p].lerp(avg, s);
        }
        current = next;
    }
    // Normalize point UVs into 0..1 while preserving aspect.
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for &uv in &current {
        min = min.min(uv);
        max = max.max(uv);
    }
    let extent = (max - min).max(Vec2::splat(1e-6));
    let scale = 1.0 / extent.max_element();
    let mut final_uvs = Vec::with_capacity(mesh.uvs.len());
    for poly in &mesh.polygons {
        for c in &poly.corners {
            final_uvs.push((current[c.point as usize] - min) * scale);
        }
    }
    assign_corner_uvs(mesh, final_uvs);
}

/// Fit every UV into `0..1`, preserving aspect ratio.
pub fn normalize_uvs(mesh: &mut Mesh) {
    if mesh.uvs.is_empty() {
        return;
    }
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for uv in &mesh.uvs {
        min = min.min(*uv);
        max = max.max(*uv);
    }
    let extent = (max - min).max(Vec2::splat(1e-6));
    let scale = 1.0 / extent.max_element();
    for uv in &mut mesh.uvs {
        *uv = (*uv - min) * scale;
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

    #[test]
    fn relaxation_is_finite() {
        let mut mesh = primitives::sphere_mesh(1.0, 16, 8);
        cylindrical_projection(&mut mesh, 0.3, 0.3);
        relax_uvs(&mut mesh, 4, 0.5, true);
        assert!(mesh.uvs.iter().all(|uv| uv.is_finite()));
    }
}
