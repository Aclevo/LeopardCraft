//! Parametric primitive generators. All meshes are outward wound and carry UVs.

use crate::{Corner, Mesh, Polygon};
use glam::{Vec2, Vec3};
use std::f32::consts::{PI, TAU};

fn push_face(mesh: &mut Mesh, points: &[u32], uvs: &[Vec2], material_slot: u16) {
    debug_assert_eq!(points.len(), uvs.len());
    let mut corners = Vec::with_capacity(points.len());
    for (i, &p) in points.iter().enumerate() {
        let uv = mesh.add_uv(uvs[i]);
        corners.push(Corner::new(p, uv));
    }
    mesh.polygons.push(Polygon {
        corners,
        material_slot,
    });
}

/// Axis-aligned box centred at the origin.
pub fn box_mesh(width: f32, height: f32, depth: f32) -> Mesh {
    let hx = (width * 0.5).abs().max(1e-4);
    let hy = (height * 0.5).abs().max(1e-4);
    let hz = (depth * 0.5).abs().max(1e-4);
    let mut m = Mesh::new("Box");
    let p = [
        Vec3::new(-hx, -hy, -hz),
        Vec3::new(hx, -hy, -hz),
        Vec3::new(hx, hy, -hz),
        Vec3::new(-hx, hy, -hz),
        Vec3::new(-hx, -hy, hz),
        Vec3::new(hx, -hy, hz),
        Vec3::new(hx, hy, hz),
        Vec3::new(-hx, hy, hz),
    ];
    for v in p {
        m.add_point(v);
    }
    let uv = [
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 1.0),
    ];
    // Faces are ordered CCW when viewed from outside.
    for face in [
        [0u32, 3, 2, 1], // -Z
        [4, 5, 6, 7],    // +Z
        [1, 2, 6, 5],    // +X
        [0, 4, 7, 3],    // -X
        [3, 7, 6, 2],    // +Y
        [0, 1, 5, 4],    // -Y
    ] {
        push_face(&mut m, &face, &uv, 0);
    }
    m
}

/// UV sphere with duplicated seam and pole vertices so UVs stay continuous.
pub fn sphere_mesh(radius: f32, segments: u32, rings: u32) -> Mesh {
    let r = radius.abs().max(1e-4);
    let segments = segments.clamp(3, 512);
    let rings = rings.clamp(2, 512);
    let mut m = Mesh::new("Sphere");
    let mut ring_points: Vec<Vec<u32>> = Vec::new();
    for ring in 0..=rings {
        let v = ring as f32 / rings as f32;
        let phi = PI * v;
        let y = phi.cos() * r;
        let ring_r = phi.sin() * r;
        let mut row = Vec::new();
        for seg in 0..=segments {
            let u = seg as f32 / segments as f32;
            let theta = TAU * u;
            let p = Vec3::new(ring_r * theta.cos(), y, ring_r * theta.sin());
            row.push(m.add_point(p));
        }
        ring_points.push(row);
    }
    let uv = |u: f32, v: f32| Vec2::new(u, 1.0 - v);

    for seg in 0..segments {
        let s = seg as usize;
        let sn = (seg + 1) as usize;
        // top cap
        let top = [ring_points[0][s], ring_points[1][sn], ring_points[1][s]];
        push_face(
            &mut m,
            &top,
            &[
                uv(s as f32 / segments as f32, 0.0),
                uv(sn as f32 / segments as f32, 1.0 / rings as f32),
                uv(s as f32 / segments as f32, 1.0 / rings as f32),
            ],
            0,
        );
        for ring in 1..rings {
            let r0 = ring as usize;
            let r1 = ring as usize + 1;
            let quad = [
                ring_points[r0][s],
                ring_points[r0][sn],
                ring_points[r1][sn],
                ring_points[r1][s],
            ];
            push_face(
                &mut m,
                &quad,
                &[
                    uv(s as f32 / segments as f32, ring as f32 / rings as f32),
                    uv(sn as f32 / segments as f32, ring as f32 / rings as f32),
                    uv(
                        sn as f32 / segments as f32,
                        (ring + 1) as f32 / rings as f32,
                    ),
                    uv(s as f32 / segments as f32, (ring + 1) as f32 / rings as f32),
                ],
                0,
            );
        }
        // bottom cap
        let bottom = [
            ring_points[rings as usize][s],
            ring_points[rings as usize - 1][s],
            ring_points[rings as usize - 1][sn],
        ];
        push_face(
            &mut m,
            &bottom,
            &[
                uv(s as f32 / segments as f32, 1.0),
                uv(
                    s as f32 / segments as f32,
                    (rings - 1) as f32 / rings as f32,
                ),
                uv(
                    sn as f32 / segments as f32,
                    (rings - 1) as f32 / rings as f32,
                ),
            ],
            0,
        );
    }
    m
}

/// Cylinder along Y with n-gon caps.
pub fn cylinder_mesh(radius: f32, height: f32, segments: u32) -> Mesh {
    let r = radius.abs().max(1e-4);
    let h = (height * 0.5).abs().max(1e-4);
    let segments = segments.clamp(3, 512);
    let mut m = Mesh::new("Cylinder");
    let mut top = Vec::new();
    let mut bottom = Vec::new();
    for seg in 0..=segments {
        let u = seg as f32 / segments as f32;
        let theta = TAU * u;
        top.push(m.add_point(Vec3::new(r * theta.cos(), h, r * theta.sin())));
        bottom.push(m.add_point(Vec3::new(r * theta.cos(), -h, r * theta.sin())));
    }
    for seg in 0..segments {
        let s = seg as usize;
        let sn = s + 1;
        let quad = [top[s], top[sn], bottom[sn], bottom[s]];
        push_face(
            &mut m,
            &quad,
            &[
                Vec2::new(seg as f32 / segments as f32, 1.0),
                Vec2::new((seg + 1) as f32 / segments as f32, 1.0),
                Vec2::new((seg + 1) as f32 / segments as f32, 0.0),
                Vec2::new(seg as f32 / segments as f32, 0.0),
            ],
            0,
        );
    }
    // Top cap faces +Y, so its ring is reversed; bottom uses the forward ring.
    let mut cap_top = top[..segments as usize].to_vec();
    cap_top.reverse();
    let cap_uvs: Vec<Vec2> = (0..segments)
        .map(|i| {
            let a = TAU * i as f32 / segments as f32;
            Vec2::new(0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin())
        })
        .collect();
    push_face(&mut m, &cap_top, &cap_uvs, 0);
    push_face(&mut m, &bottom[..segments as usize], &cap_uvs, 0);
    m
}

/// Cone along Y with an n-gon base.
pub fn cone_mesh(radius: f32, height: f32, segments: u32) -> Mesh {
    let r = radius.abs().max(1e-4);
    let h = (height * 0.5).abs().max(1e-4);
    let segments = segments.clamp(3, 512);
    let mut m = Mesh::new("Cone");
    let apex = m.add_point(Vec3::new(0.0, h, 0.0));
    let mut base = Vec::new();
    for seg in 0..=segments {
        let u = seg as f32 / segments as f32;
        let theta = TAU * u;
        base.push(m.add_point(Vec3::new(r * theta.cos(), -h, r * theta.sin())));
    }
    for seg in 0..segments {
        let s = seg as usize;
        let sn = s + 1;
        push_face(
            &mut m,
            &[apex, base[sn], base[s]],
            &[
                Vec2::new(0.5, 1.0),
                Vec2::new((seg + 1) as f32 / segments as f32, 0.0),
                Vec2::new(seg as f32 / segments as f32, 0.0),
            ],
            0,
        );
    }
    let cap_uvs: Vec<Vec2> = (0..segments)
        .map(|i| {
            let a = TAU * i as f32 / segments as f32;
            Vec2::new(0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin())
        })
        .collect();
    push_face(&mut m, &base[..segments as usize], &cap_uvs, 0);
    m
}

/// Torus in the XZ plane, tube around the major circle.
pub fn torus_mesh(major_radius: f32, minor_radius: f32, segments: u32, rings: u32) -> Mesh {
    let major = major_radius.abs().max(1e-4);
    let minor = minor_radius.abs().max(1e-4);
    let segments = segments.clamp(3, 512);
    let rings = rings.clamp(3, 512);
    let mut m = Mesh::new("Torus");
    let mut grid: Vec<Vec<u32>> = Vec::new();
    for i in 0..=segments {
        let u = TAU * i as f32 / segments as f32;
        let mut row = Vec::new();
        for j in 0..=rings {
            let v = TAU * j as f32 / rings as f32;
            let rr = major + minor * v.cos();
            let p = Vec3::new(rr * u.cos(), minor * v.sin(), rr * u.sin());
            row.push(m.add_point(p));
        }
        grid.push(row);
    }
    for i in 0..segments {
        for j in 0..rings {
            let (a, b) = (i as usize, j as usize);
            let quad = [
                grid[a][b],
                grid[a][b + 1],
                grid[a + 1][b + 1],
                grid[a + 1][b],
            ];
            push_face(
                &mut m,
                &quad,
                &[
                    Vec2::new(i as f32 / segments as f32, j as f32 / rings as f32),
                    Vec2::new(i as f32 / segments as f32, (j + 1) as f32 / rings as f32),
                    Vec2::new(
                        (i + 1) as f32 / segments as f32,
                        (j + 1) as f32 / rings as f32,
                    ),
                    Vec2::new((i + 1) as f32 / segments as f32, j as f32 / rings as f32),
                ],
                0,
            );
        }
    }
    m
}

/// Ground plane in XZ facing +Y.
pub fn plane_mesh(width: f32, depth: f32, segments_x: u32, segments_z: u32) -> Mesh {
    let w = width.abs().max(1e-4);
    let d = depth.abs().max(1e-4);
    let sx = segments_x.clamp(1, 256);
    let sz = segments_z.clamp(1, 256);
    let mut m = Mesh::new("Plane");
    let mut grid = Vec::new();
    for z in 0..=sz {
        let mut row = Vec::new();
        for x in 0..=sx {
            let u = x as f32 / sx as f32;
            let v = z as f32 / sz as f32;
            let p = Vec3::new((u - 0.5) * w, 0.0, (v - 0.5) * d);
            row.push(m.add_point(p));
        }
        grid.push(row);
    }
    for z in 0..sz as usize {
        for x in 0..sx as usize {
            let a = grid[z][x];
            let b = grid[z][x + 1];
            let c = grid[z + 1][x + 1];
            let d0 = grid[z + 1][x];
            // CCW from +Y.
            push_face(
                &mut m,
                &[a, d0, c, b],
                &[
                    Vec2::new(x as f32 / sx as f32, z as f32 / sz as f32),
                    Vec2::new(x as f32 / sx as f32, (z + 1) as f32 / sz as f32),
                    Vec2::new((x + 1) as f32 / sx as f32, (z + 1) as f32 / sz as f32),
                    Vec2::new((x + 1) as f32 / sx as f32, z as f32 / sz as f32),
                ],
                0,
            );
        }
    }
    m
}

/// A simple 2D profile disc used by lathe/sweep examples.
pub fn disc_profile(radius: f32, segments: u32) -> Vec<Vec2> {
    let segments = segments.clamp(3, 512);
    (0..segments)
        .map(|i| {
            let a = TAU * i as f32 / segments as f32;
            Vec2::new(radius * a.cos(), radius * a.sin())
        })
        .collect()
}

/// A closed smooth-ish profile for lathe tests/demo.
pub fn capsule_profile(radius: f32, height: f32, steps: u32) -> Vec<Vec2> {
    let mut pts = Vec::new();
    let steps = steps.max(4) / 2;
    for i in 0..=steps {
        let a = PI * i as f32 / steps as f32;
        pts.push(Vec2::new(radius * a.sin(), height * 0.5 + radius * a.cos()));
    }
    for i in 0..=steps {
        let a = PI * i as f32 / steps as f32;
        pts.push(Vec2::new(
            radius * a.sin(),
            -height * 0.5 - radius * a.cos(),
        ));
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_are_valid_and_outward() {
        let meshes = [
            box_mesh(2.0, 2.0, 2.0),
            sphere_mesh(1.0, 24, 16),
            cylinder_mesh(1.0, 2.0, 24),
            cone_mesh(1.0, 2.0, 24),
            torus_mesh(1.0, 0.35, 32, 16),
            plane_mesh(4.0, 4.0, 4, 4),
        ];
        for m in &meshes {
            assert!(m.validate().is_ok(), "{:?}", m.validate().err());
            assert!(m.triangle_count() > 0);
        }
        assert!(meshes[0].signed_volume() > 0.0);
        assert!(meshes[1].signed_volume() > 0.0);
        assert!(meshes[2].signed_volume() > 0.0);
        assert!(meshes[3].signed_volume() > 0.0);
        assert!(meshes[4].signed_volume() > 0.0);
    }

    #[test]
    fn sphere_radius_is_exact() {
        let sphere = sphere_mesh(2.5, 16, 12);
        for p in &sphere.positions {
            assert!((p.length() - 2.5).abs() < 1e-4);
        }
    }

    #[test]
    fn cylinder_caps_are_ngons() {
        let cyl = cylinder_mesh(1.0, 2.0, 8);
        let ngons = cyl.polygons.iter().filter(|p| p.corners.len() == 8).count();
        assert_eq!(ngons, 2);
    }
}
