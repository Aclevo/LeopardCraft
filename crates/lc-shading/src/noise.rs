//! Deterministic procedural noise used by material nodes and the renderer.

use glam::Vec3;

#[inline]
fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

#[inline]
fn lattice(ix: i32, iy: i32, iz: i32, seed: u32) -> f32 {
    let h = hash_u32(
        (ix as u32).wrapping_mul(0x9e37_79b9)
            ^ (iy as u32).wrapping_mul(0x85eb_ca6b)
            ^ (iz as u32).wrapping_mul(0xc2b2_ae35)
            ^ seed,
    );
    (h as f32 / u32::MAX as f32) * 2.0 - 1.0
}

#[inline]
fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Smooth trilinear value noise in `[-1, 1]`.
pub fn value_noise(p: Vec3, seed: u32) -> f32 {
    let x0 = p.x.floor();
    let y0 = p.y.floor();
    let z0 = p.z.floor();
    let fx = smooth(p.x - x0);
    let fy = smooth(p.y - y0);
    let fz = smooth(p.z - z0);
    let (ix, iy, iz) = (x0 as i32, y0 as i32, z0 as i32);
    let c000 = lattice(ix, iy, iz, seed);
    let c100 = lattice(ix + 1, iy, iz, seed);
    let c010 = lattice(ix, iy + 1, iz, seed);
    let c110 = lattice(ix + 1, iy + 1, iz, seed);
    let c001 = lattice(ix, iy, iz + 1, seed);
    let c101 = lattice(ix + 1, iy, iz + 1, seed);
    let c011 = lattice(ix, iy + 1, iz + 1, seed);
    let c111 = lattice(ix + 1, iy + 1, iz + 1, seed);
    let x00 = c000 + (c100 - c000) * fx;
    let x10 = c010 + (c110 - c010) * fx;
    let x01 = c001 + (c101 - c001) * fx;
    let x11 = c011 + (c111 - c011) * fx;
    let y0v = x00 + (x10 - x00) * fy;
    let y1v = x01 + (x11 - x01) * fy;
    y0v + (y1v - y0v) * fz
}

/// Fractal Brownian motion. `roughness` controls amplitude falloff per octave.
pub fn fbm(p: Vec3, octaves: u32, roughness: f32, lacunarity: f32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut norm = 0.0;
    for i in 0..octaves.min(12) {
        sum += value_noise(p * frequency, seed.wrapping_add(i * 17)) * amplitude;
        norm += amplitude;
        amplitude *= roughness.clamp(0.0, 1.0);
        frequency *= lacunarity.max(1.0);
    }
    if norm > 0.0 {
        (sum / norm).clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

/// Worley/cellular noise: distance to the closest feature point in `[0, 1]`.
pub fn voronoi(p: Vec3, jitter: f32, seed: u32) -> f32 {
    let cell = p.floor();
    let local = p - cell;
    let (ix, iy, iz) = (cell.x as i32, cell.y as i32, cell.z as i32);
    let mut best = f32::INFINITY;
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cx = ix + dx;
                let cy = iy + dy;
                let cz = iz + dz;
                let h = hash_u32(
                    (cx as u32).wrapping_mul(73856093)
                        ^ (cy as u32).wrapping_mul(19349663)
                        ^ (cz as u32).wrapping_mul(83492791)
                        ^ seed,
                );
                let jx = ((h & 0xff) as f32 / 255.0 - 0.5) * jitter.clamp(0.0, 1.0);
                let jy = (((h >> 8) & 0xff) as f32 / 255.0 - 0.5) * jitter.clamp(0.0, 1.0);
                let jz = (((h >> 16) & 0xff) as f32 / 255.0 - 0.5) * jitter.clamp(0.0, 1.0);
                let point = Vec3::new(dx as f32, dy as f32, dz as f32)
                    + Vec3::new(jx, jy, jz)
                    + Vec3::splat(0.5);
                let d = (local - point).length();
                best = best.min(d);
            }
        }
    }
    best.clamp(0.0, 1.0)
}

/// Directional gradient used by bump mapping.
pub fn gradient(p: Vec3, eps: f32, seed: u32) -> Vec3 {
    let e = eps.max(1e-4);
    let dx = value_noise(p + Vec3::X * e, seed) - value_noise(p - Vec3::X * e, seed);
    let dy = value_noise(p + Vec3::Y * e, seed) - value_noise(p - Vec3::Y * e, seed);
    let dz = value_noise(p + Vec3::Z * e, seed) - value_noise(p - Vec3::Z * e, seed);
    Vec3::new(dx, dy, dz) / (2.0 * e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_deterministic_and_bounded() {
        let p = Vec3::new(1.2, -3.4, 0.7);
        for _ in 0..10 {
            let a = fbm(p, 5, 0.5, 2.0, 7);
            let b = fbm(p, 5, 0.5, 2.0, 7);
            assert_eq!(a, b);
            assert!((-1.0..=1.0).contains(&a));
        }
        let v = voronoi(p, 0.8, 3);
        assert!((0.0..=1.0).contains(&v));
    }

    #[test]
    fn different_seeds_differ() {
        let p = Vec3::new(0.3, 0.9, 1.7);
        assert_ne!(value_noise(p, 1), value_noise(p, 2));
    }
}
