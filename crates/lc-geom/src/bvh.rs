//! Deterministic bounding volume hierarchy over ray-hittable objects.

use glam::Vec3;
use lc_core::{Aabb, Ray};
use std::cmp::Ordering;

pub trait BvhObject {
    fn bounds(&self) -> Aabb;
    fn centroid(&self) -> Vec3;
    /// Returns `(t, u, v)` for the closest hit along the ray.
    fn ray_hit(&self, ray: &Ray) -> Option<(f32, f32, f32)>;
}

#[derive(Clone, Debug, Copy)]
struct BvhNode {
    bounds: Aabb,
    /// Index of the first primitive in `order` (leaf) or left child (inner).
    start: u32,
    /// Primitive count for leaves; 0 for inner nodes.
    count: u32,
    /// Right child index for inner nodes.
    right: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Bvh {
    nodes: Vec<BvhNode>,
    order: Vec<u32>,
}

#[derive(Clone, Copy, Debug)]
pub struct BvhHit {
    pub prim: u32,
    pub t: f32,
    pub u: f32,
    pub v: f32,
}

impl Bvh {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn primitive_count(&self) -> usize {
        self.order.len()
    }

    pub fn build<T: BvhObject>(objects: &[T]) -> Self {
        if objects.is_empty() {
            return Self::default();
        }
        let mut order: Vec<u32> = (0..objects.len() as u32).collect();
        let centroids: Vec<Vec3> = objects.iter().map(|o| o.centroid()).collect();
        let bounds: Vec<Aabb> = objects.iter().map(|o| o.bounds()).collect();
        let mut bvh = Self {
            nodes: Vec::with_capacity(objects.len() * 2),
            order: Vec::with_capacity(objects.len()),
        };
        bvh.build_recursive(&mut order, &centroids, &bounds, 0, objects.len(), 0);
        bvh.order = order;
        bvh
    }

    fn build_recursive(
        &mut self,
        order: &mut [u32],
        centroids: &[Vec3],
        bounds: &[Aabb],
        start: usize,
        end: usize,
        depth: usize,
    ) -> u32 {
        let mut node_bounds = Aabb::EMPTY;
        for &i in &order[start..end] {
            node_bounds = node_bounds.union(&bounds[i as usize]);
        }
        let node_index = self.nodes.len() as u32;
        self.nodes.push(BvhNode {
            bounds: node_bounds,
            start: start as u32,
            count: (end - start) as u32,
            right: 0,
        });
        if end - start <= 4 || depth > 64 {
            return node_index;
        }

        // Split at the median of the largest centroid extent.
        let mut cmin = Vec3::splat(f32::INFINITY);
        let mut cmax = Vec3::splat(f32::NEG_INFINITY);
        for &i in &order[start..end] {
            let c = centroids[i as usize];
            cmin = cmin.min(c);
            cmax = cmax.max(c);
        }
        let extent = cmax - cmin;
        let axis = if extent.x >= extent.y && extent.x >= extent.z {
            0
        } else if extent.y >= extent.z {
            1
        } else {
            2
        };
        if extent[axis] <= 1e-12 {
            return node_index; // coincident centroids: leave as leaf
        }
        let mid = (start + end) / 2;
        order[start..end].select_nth_unstable_by(mid - start, |&a, &b| {
            centroids[a as usize][axis]
                .partial_cmp(&centroids[b as usize][axis])
                .unwrap_or(Ordering::Equal)
        });
        let left = self.build_recursive(order, centroids, bounds, start, mid, depth + 1);
        let right = self.build_recursive(order, centroids, bounds, mid, end, depth + 1);
        self.nodes[node_index as usize] = BvhNode {
            bounds: node_bounds,
            start: left,
            count: 0,
            right,
        };
        node_index
    }

    pub fn nearest<T: BvhObject>(&self, objects: &[T], ray: &Ray, max_t: f32) -> Option<BvhHit> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut best_t = max_t;
        let mut best: Option<BvhHit> = None;
        let mut stack = Vec::with_capacity(64);
        stack.push(0u32);
        while let Some(index) = stack.pop() {
            let node = self.nodes[index as usize];
            if !ray_hits_aabb(ray, &node.bounds, best_t) {
                continue;
            }
            if node.count > 0 {
                for &pi in &self.order[node.start as usize..(node.start + node.count) as usize] {
                    if let Some((t, u, v)) = objects[pi as usize].ray_hit(ray) {
                        if t < best_t {
                            best_t = t;
                            best = Some(BvhHit { prim: pi, t, u, v });
                        }
                    }
                }
            } else {
                stack.push(node.right);
                stack.push(node.start);
            }
        }
        best
    }

    pub fn any_hit<T: BvhObject>(&self, objects: &[T], ray: &Ray, max_t: f32) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        let mut stack = Vec::with_capacity(64);
        stack.push(0u32);
        while let Some(index) = stack.pop() {
            let node = self.nodes[index as usize];
            if !ray_hits_aabb(ray, &node.bounds, max_t) {
                continue;
            }
            if node.count > 0 {
                for &pi in &self.order[node.start as usize..(node.start + node.count) as usize] {
                    if let Some((t, _, _)) = objects[pi as usize].ray_hit(ray) {
                        if t < max_t {
                            return true;
                        }
                    }
                }
            } else {
                stack.push(node.right);
                stack.push(node.start);
            }
        }
        false
    }

    pub fn bounds(&self) -> Aabb {
        self.nodes.first().map(|n| n.bounds).unwrap_or(Aabb::EMPTY)
    }
}

fn ray_hits_aabb(ray: &Ray, bounds: &Aabb, max_t: f32) -> bool {
    match bounds.intersect_ray(ray) {
        Some((near, _)) => near <= max_t,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lc_core::ray_triangle;

    struct Tri {
        a: Vec3,
        b: Vec3,
        c: Vec3,
    }
    impl BvhObject for Tri {
        fn bounds(&self) -> Aabb {
            Aabb::from_points([self.a, self.b, self.c])
        }
        fn centroid(&self) -> Vec3 {
            (self.a + self.b + self.c) / 3.0
        }
        fn ray_hit(&self, ray: &Ray) -> Option<(f32, f32, f32)> {
            ray_triangle(ray, self.a, self.b, self.c)
        }
    }

    #[test]
    fn bvh_matches_brute_force() {
        let mut tris = Vec::new();
        for z in 0..10 {
            for x in 0..10 {
                let o = Vec3::new(x as f32, 0.0, -(z as f32));
                tris.push(Tri {
                    a: o,
                    b: o + Vec3::X,
                    c: o + Vec3::new(0.0, 0.1, 1.0),
                });
            }
        }
        let bvh = Bvh::build(&tris);
        let mut rng = lc_core::Pcg32::new(42);
        for _ in 0..100 {
            let ray = Ray::new(
                Vec3::new(rng.range(-2.0, 12.0), 3.0, rng.range(-14.0, 2.0)),
                Vec3::new(rng.range(-0.2, 0.2), -1.0, rng.range(-0.2, 0.2)),
            );
            let hit = bvh.nearest(&tris, &ray, f32::INFINITY);
            let brute = tris
                .iter()
                .filter_map(|t| t.ray_hit(&ray))
                .map(|h| h.0)
                .fold(f32::INFINITY, f32::min);
            match hit {
                Some(h) => assert!((h.t - brute).abs() < 1e-4),
                None => assert!(!brute.is_finite()),
            }
        }
    }

    #[test]
    fn any_hit_respects_max_distance() {
        let tri = Tri {
            a: Vec3::ZERO,
            b: Vec3::X,
            c: Vec3::Z,
        };
        let tris = [tri];
        let bvh = Bvh::build(&tris);
        let ray = Ray::new(Vec3::new(0.1, 1.0, 0.1), Vec3::new(0.0, -1.0, 0.0));
        assert!(bvh.any_hit(&tris, &ray, 2.0));
        assert!(!bvh.any_hit(&tris, &ray, 0.5));
    }
}
