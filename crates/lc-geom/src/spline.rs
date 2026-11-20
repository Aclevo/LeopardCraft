//! Splines used for path animation, sweeps and extrusions.

use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplineKind {
    Linear,
    Bezier,
    Smooth,
}

impl Default for SplineKind {
    fn default() -> Self {
        Self::Bezier
    }
}

/// Control point with relative Bezier handles.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SplinePoint {
    pub position: Vec3,
    pub handle_in: Vec3,
    pub handle_out: Vec3,
}

impl SplinePoint {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            handle_in: Vec3::ZERO,
            handle_out: Vec3::ZERO,
        }
    }

    pub fn with_handles(position: Vec3, handle_in: Vec3, handle_out: Vec3) -> Self {
        Self {
            position,
            handle_in,
            handle_out,
        }
    }

    pub fn in_position(&self) -> Vec3 {
        self.position + self.handle_in
    }

    pub fn out_position(&self) -> Vec3 {
        self.position + self.handle_out
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spline {
    pub points: Vec<SplinePoint>,
    #[serde(default)]
    pub kind: SplineKind,
    #[serde(default)]
    pub closed: bool,
    /// Samples per segment.
    #[serde(default = "default_resolution")]
    pub resolution: u32,
}

fn default_resolution() -> u32 {
    16
}

impl Default for Spline {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            kind: SplineKind::Bezier,
            closed: false,
            resolution: default_resolution(),
        }
    }
}

impl Spline {
    pub fn new(kind: SplineKind, points: Vec<Vec3>, closed: bool) -> Self {
        let mut s = Self {
            points: points.into_iter().map(SplinePoint::new).collect(),
            kind,
            closed,
            resolution: default_resolution(),
        };
        s.set_auto_handles();
        s
    }

    pub fn segment_count(&self) -> usize {
        match self.points.len() {
            0 | 1 => 0,
            n if self.closed => n,
            n => n - 1,
        }
    }

    /// Recompute smooth Bezier handles from neighbouring points (1/3 rule).
    pub fn set_auto_handles(&mut self) {
        let n = self.points.len();
        if n < 2 {
            for p in &mut self.points {
                p.handle_in = Vec3::ZERO;
                p.handle_out = Vec3::ZERO;
            }
            return;
        }
        for i in 0..n {
            let prev = if i == 0 {
                if self.closed {
                    self.points[n - 1].position
                } else {
                    self.points[0].position
                }
            } else {
                self.points[i - 1].position
            };
            let next = if i + 1 == n {
                if self.closed {
                    self.points[0].position
                } else {
                    self.points[n - 1].position
                }
            } else {
                self.points[i + 1].position
            };
            let dir = (next - prev).normalize_or(Vec3::Z);
            let len_out = (next - self.points[i].position).length() / 3.0;
            let len_in = (self.points[i].position - prev).length() / 3.0;
            self.points[i].handle_out = dir * len_out;
            self.points[i].handle_in = -dir * len_in;
        }
    }

    pub fn point_at_segment(&self, segment: usize, t: f32) -> Vec3 {
        let n = self.points.len();
        if n == 0 {
            return Vec3::ZERO;
        }
        if n == 1 {
            return self.points[0].position;
        }
        let t = t.clamp(0.0, 1.0);
        let i = segment % self.segment_count().max(1);
        let j = (i + 1) % n;
        let a = self.points[i];
        let b = self.points[j];
        match self.kind {
            SplineKind::Linear => a.position.lerp(b.position, t),
            SplineKind::Bezier => {
                let p0 = a.position;
                let p1 = a.out_position();
                let p2 = b.in_position();
                let p3 = b.position;
                cubic_bezier(p0, p1, p2, p3, t)
            }
            SplineKind::Smooth => {
                let prev = if i == 0 {
                    if self.closed {
                        self.points[n - 1].position
                    } else {
                        a.position * 2.0 - b.position
                    }
                } else {
                    self.points[i - 1].position
                };
                let next = if j + 1 >= n {
                    if self.closed {
                        self.points[0].position
                    } else {
                        b.position * 2.0 - a.position
                    }
                } else {
                    self.points[j + 1].position
                };
                catmull_rom(prev, a.position, b.position, next, t)
            }
        }
    }

    /// Sample the whole spline. Each segment contributes `resolution` steps,
    /// with duplicated joint points removed.
    pub fn sample(&self) -> Vec<Vec3> {
        let segs = self.segment_count();
        if segs == 0 {
            return self.points.iter().map(|p| p.position).collect();
        }
        let res = self.resolution.clamp(1, 256) as usize;
        let mut out = Vec::with_capacity(segs * res + 1);
        for seg in 0..segs {
            for k in 0..res {
                let t = k as f32 / res as f32;
                out.push(self.point_at_segment(seg, t));
            }
        }
        if !self.closed {
            out.push(self.point_at_segment(segs - 1, 1.0));
        }
        out
    }

    pub fn length(&self) -> f32 {
        let pts = self.sample();
        pts.windows(2).map(|w| w[0].distance(w[1])).sum()
    }

    pub fn signature(&self) -> u64 {
        use lc_core::StableHasher;
        let mut h = StableHasher::new();
        h.write_u64(self.points.len() as u64);
        for p in &self.points {
            h.write_vec3(p.position);
            h.write_vec3(p.handle_in);
            h.write_vec3(p.handle_out);
        }
        h.write_u8(self.closed as u8);
        h.finish()
    }
}

pub fn cubic_bezier(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let u = 1.0 - t;
    p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t)
}

pub fn catmull_rom(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let t2 = t * t;
    let t3 = t2 * t;
    (p1 * 2.0
        + (p2 - p0) * t
        + (p0 * 2.0 - p1 * 5.0 + p2 * 4.0 - p3) * t2
        + (p1 * 3.0 - p0 - p2 * 3.0 + p3) * t3)
        * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_endpoints() {
        let s = Spline::new(
            SplineKind::Bezier,
            vec![Vec3::ZERO, Vec3::X * 2.0, Vec3::new(2.0, 0.0, 2.0)],
            false,
        );
        let pts = s.sample();
        assert!(pts.first().unwrap().distance(Vec3::ZERO) < 1e-5);
        assert!(pts.last().unwrap().distance(Vec3::new(2.0, 0.0, 2.0)) < 1e-4);
        assert!(pts.len() > 20);
    }

    #[test]
    fn closed_linear_spline_loops() {
        let s = Spline::new(
            SplineKind::Linear,
            vec![Vec3::ZERO, Vec3::X, Vec3::new(1.0, 0.0, 1.0)],
            true,
        );
        assert_eq!(s.segment_count(), 3);
        let pts = s.sample();
        assert!(pts.len() > 30);
        for p in &pts {
            assert!(p.is_finite());
        }
    }

    #[test]
    fn smooth_sampling_stays_finite() {
        let mut s = Spline::new(
            SplineKind::Smooth,
            vec![Vec3::ZERO, Vec3::Y, Vec3::new(1.0, 2.0, 0.0)],
            false,
        );
        s.set_auto_handles();
        assert!(s.length() > 0.0);
        assert!(s.sample().iter().all(|p| p.is_finite()));
    }
}
