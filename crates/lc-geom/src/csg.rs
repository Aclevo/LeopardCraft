//! Constructive solid geometry on triangle meshes using a BSP tree.
//!
//! The algorithm is the classic public BSP polygon-clipping construction,
//! implemented from scratch. Inputs are triangulated, results are welded n-gons
//! carrying the first input's structure. UVs are not preserved through CSG.

use crate::{Corner, Mesh, Polygon};
use glam::Vec3;
use std::collections::HashMap;

const EPS: f32 = 1e-4;

#[derive(Clone, Copy, Debug)]
struct Plane {
    normal: Vec3,
    w: f32,
}

impl Plane {
    fn from_points(a: Vec3, b: Vec3, c: Vec3) -> Option<Self> {
        let n = (b - a).cross(c - a);
        if n.length_squared() < 1e-14 {
            return None;
        }
        let normal = n.normalize();
        Some(Self {
            normal,
            w: normal.dot(a),
        })
    }

    fn flip(&mut self) {
        self.normal = -self.normal;
        self.w = -self.w;
    }
}

#[derive(Clone, Debug)]
struct CsgPolygon {
    verts: Vec<Vec3>,
    plane: Plane,
}

impl CsgPolygon {
    fn new(verts: Vec<Vec3>) -> Option<Self> {
        let n = verts.len();
        if n < 3 {
            return None;
        }
        let mut plane = None;
        for i in 2..n {
            if let Some(p) = Plane::from_points(verts[0], verts[1], verts[i]) {
                plane = Some(p);
                break;
            }
        }
        if let Some(p) = Plane::from_points(verts[0], verts[1], verts[2]) {
            plane = Some(p);
        }
        Some(Self {
            verts,
            plane: plane?,
        })
    }

    fn flip(&mut self) {
        self.verts.reverse();
        self.plane.flip();
    }
}

fn split_polygon(
    poly: &CsgPolygon,
    plane: &Plane,
    coplanar_front: &mut Vec<CsgPolygon>,
    coplanar_back: &mut Vec<CsgPolygon>,
    front: &mut Vec<CsgPolygon>,
    back: &mut Vec<CsgPolygon>,
) {
    // 0 = coplanar, 1 = front, 2 = back
    let mut types = Vec::with_capacity(poly.verts.len());
    let mut distances = Vec::with_capacity(poly.verts.len());
    let mut poly_type = 0u8;
    for v in &poly.verts {
        let d = plane.normal.dot(*v) - plane.w;
        distances.push(d);
        let t = if d < -EPS {
            2
        } else if d > EPS {
            1
        } else {
            0
        };
        types.push(t);
        poly_type |= t;
    }
    match poly_type {
        0 => {
            if poly.plane.normal.dot(plane.normal) > 0.0 {
                coplanar_front.push(poly.clone());
            } else {
                coplanar_back.push(poly.clone());
            }
        }
        1 => front.push(poly.clone()),
        2 => back.push(poly.clone()),
        _ => {
            let n = poly.verts.len();
            let mut f = Vec::new();
            let mut b = Vec::new();
            for i in 0..n {
                let j = (i + 1) % n;
                let ti = types[i] as i32;
                let tj = types[j] as i32;
                if ti != 2 {
                    f.push(poly.verts[i]);
                }
                if ti != 1 {
                    b.push(poly.verts[i]);
                }
                if (ti | tj) == 3 {
                    let denom = distances[i] - distances[j];
                    if denom.abs() > 1e-9 {
                        let t = distances[i] / denom;
                        let v = poly.verts[i].lerp(poly.verts[j], t);
                        f.push(v);
                        b.push(v);
                    }
                }
            }
            if let Some(p) = CsgPolygon::new(f) {
                front.push(p);
            }
            if let Some(p) = CsgPolygon::new(b) {
                back.push(p);
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
struct BspNode {
    plane: Option<Plane>,
    front: Option<Box<BspNode>>,
    back: Option<Box<BspNode>>,
    polygons: Vec<CsgPolygon>,
}

impl BspNode {
    fn from_mesh(mesh: &Mesh) -> Self {
        let mut node = Self::default();
        node.build(mesh_to_polygons(mesh), 0);
        node
    }

    fn build(&mut self, polys: Vec<CsgPolygon>, depth: usize) {
        if polys.is_empty() || depth > 128 {
            return;
        }
        if self.plane.is_none() {
            self.plane = Some(polys[0].plane);
        }
        let plane = self.plane.unwrap();
        let mut f = Vec::new();
        let mut b = Vec::new();
        let mut cf = Vec::new();
        let mut cb = Vec::new();
        for p in polys {
            split_polygon(&p, &plane, &mut cf, &mut cb, &mut f, &mut b);
        }
        self.polygons.extend(cf);
        self.polygons.extend(cb);
        if !f.is_empty() {
            self.front
                .get_or_insert_with(|| Box::new(BspNode::default()))
                .build(f, depth + 1);
        }
        if !b.is_empty() {
            self.back
                .get_or_insert_with(|| Box::new(BspNode::default()))
                .build(b, depth + 1);
        }
    }

    fn clip_polygons(&self, polys: Vec<CsgPolygon>) -> Vec<CsgPolygon> {
        let Some(plane) = self.plane else {
            return polys;
        };
        let mut f = Vec::new();
        let mut b = Vec::new();
        for p in polys {
            let mut cf = Vec::new();
            let mut cb = Vec::new();
            split_polygon(&p, &plane, &mut cf, &mut cb, &mut f, &mut b);
            f.extend(cf);
            b.extend(cb);
        }
        let mut out = match &self.front {
            Some(front) => front.clip_polygons(f),
            None => f,
        };
        if let Some(back) = &self.back {
            out.extend(back.clip_polygons(b));
        }
        out
    }

    fn clip_to(&mut self, other: &BspNode) {
        self.polygons = other.clip_polygons(std::mem::take(&mut self.polygons));
        if let Some(front) = &mut self.front {
            front.clip_to(other);
        }
        if let Some(back) = &mut self.back {
            back.clip_to(other);
        }
    }

    fn invert(&mut self) {
        for p in &mut self.polygons {
            p.flip();
        }
        if let Some(plane) = &mut self.plane {
            plane.flip();
        }
        if let Some(front) = &mut self.front {
            front.invert();
        }
        if let Some(back) = &mut self.back {
            back.invert();
        }
        std::mem::swap(&mut self.front, &mut self.back);
    }

    fn all_polygons(&self, out: &mut Vec<CsgPolygon>) {
        out.extend(self.polygons.iter().cloned());
        if let Some(front) = &self.front {
            front.all_polygons(out);
        }
        if let Some(back) = &self.back {
            back.all_polygons(out);
        }
    }
}

fn mesh_to_polygons(mesh: &Mesh) -> Vec<CsgPolygon> {
    let mut out = Vec::new();
    for tri in mesh.render_triangles(40.0) {
        let verts = vec![tri.v[0].position, tri.v[1].position, tri.v[2].position];
        if let Some(p) = CsgPolygon::new(verts) {
            out.push(p);
        }
    }
    out
}

fn polygons_to_mesh(polys: Vec<CsgPolygon>, name: &str) -> Mesh {
    let mut mesh = Mesh::new(name);
    let mut map: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut add = |p: Vec3, mesh: &mut Mesh| -> u32 {
        let key = (
            (p.x / 1e-5).round() as i64,
            (p.y / 1e-5).round() as i64,
            (p.z / 1e-5).round() as i64,
        );
        *map.entry(key).or_insert_with(|| mesh.add_point(p))
    };
    for poly in polys {
        let mut ids = Vec::with_capacity(poly.verts.len());
        for v in &poly.verts {
            ids.push(add(*v, &mut mesh));
        }
        ids.dedup();
        if ids.len() < 3 {
            continue;
        }
        let corners = ids
            .iter()
            .map(|&p| {
                let uv = mesh.add_uv(glam::Vec2::ZERO);
                Corner::new(p, uv)
            })
            .collect();
        mesh.polygons.push(Polygon {
            corners,
            material_slot: 0,
        });
    }
    mesh.remove_degenerate_polygons();
    mesh
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanOp {
    Union,
    Subtract,
    Intersect,
}

pub fn boolean(a: &Mesh, b: &Mesh, op: BooleanOp) -> Mesh {
    let mut a_tree = BspNode::from_mesh(a);
    let mut b_tree = BspNode::from_mesh(b);
    match op {
        BooleanOp::Union => {
            a_tree.clip_to(&b_tree);
            b_tree.clip_to(&a_tree);
            b_tree.invert();
            b_tree.clip_to(&a_tree);
            b_tree.invert();
            let mut polys = Vec::new();
            b_tree.all_polygons(&mut polys);
            a_tree.build(polys, 0);
        }
        BooleanOp::Subtract => {
            a_tree.invert();
            a_tree.clip_to(&b_tree);
            b_tree.clip_to(&a_tree);
            b_tree.invert();
            b_tree.clip_to(&a_tree);
            b_tree.invert();
            let mut polys = Vec::new();
            b_tree.all_polygons(&mut polys);
            a_tree.build(polys, 0);
            a_tree.invert();
        }
        BooleanOp::Intersect => {
            a_tree.invert();
            b_tree.clip_to(&a_tree);
            b_tree.invert();
            a_tree.clip_to(&b_tree);
            b_tree.clip_to(&a_tree);
            let mut polys = Vec::new();
            b_tree.all_polygons(&mut polys);
            a_tree.build(polys, 0);
            a_tree.invert();
        }
    }
    let mut polys = Vec::new();
    a_tree.all_polygons(&mut polys);
    polygons_to_mesh(polys, "Boolean")
}

pub fn union(a: &Mesh, b: &Mesh) -> Mesh {
    boolean(a, b, BooleanOp::Union)
}

pub fn subtract(a: &Mesh, b: &Mesh) -> Mesh {
    boolean(a, b, BooleanOp::Subtract)
}

pub fn intersect(a: &Mesh, b: &Mesh) -> Mesh {
    boolean(a, b, BooleanOp::Intersect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives;

    fn cube_at(x: f32) -> Mesh {
        let mut m = primitives::box_mesh(2.0, 2.0, 2.0);
        m.transform(glam::Mat4::from_translation(Vec3::new(x, 0.0, 0.0)));
        m
    }

    #[test]
    fn boolean_union_has_expected_volume() {
        let a = cube_at(0.0);
        let b = cube_at(1.0);
        let out = union(&a, &b);
        assert!(out.validate().is_ok());
        let vol = out.signed_volume();
        assert!(
            (10.0..14.5).contains(&vol),
            "unexpected union volume {vol}, tris {}",
            out.triangle_count()
        );
    }

    #[test]
    fn boolean_subtract_and_intersect() {
        let a = cube_at(0.0);
        let b = cube_at(1.0);
        let sub = subtract(&a, &b);
        assert!(sub.validate().is_ok());
        let vol = sub.signed_volume();
        assert!(
            (2.0..6.0).contains(&vol),
            "unexpected subtract volume {vol}"
        );

        let inter = intersect(&a, &b);
        assert!(inter.validate().is_ok());
        let vol = inter.signed_volume();
        assert!(
            (2.0..6.0).contains(&vol),
            "unexpected intersect volume {vol}"
        );
    }

    #[test]
    fn disjoint_union_keeps_both() {
        let a = cube_at(0.0);
        let b = cube_at(10.0);
        let out = union(&a, &b);
        assert!(out.validate().is_ok());
        let vol = out.signed_volume();
        assert!(
            (14.0..18.0).contains(&vol),
            "unexpected disjoint volume {vol}"
        );
    }
}
