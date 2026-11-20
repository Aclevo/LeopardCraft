//! Texture loading and sampling. Textures are decoded once, kept in linear
//! light, and never fetched from the network.

use glam::{Vec2, Vec3};
use lc_core::srgb_to_linear;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct Texture2D {
    pub width: u32,
    pub height: u32,
    /// Linear RGB values.
    pub pixels: Vec<Vec3>,
    pub hdr: bool,
}

impl Texture2D {
    pub fn from_pixels(width: u32, height: u32, pixels: Vec<Vec3>) -> Self {
        Self {
            width,
            height,
            pixels,
            hdr: true,
        }
    }

    pub fn sample(&self, uv: Vec2) -> Vec3 {
        if self.width == 0 || self.height == 0 || self.pixels.is_empty() {
            return Vec3::splat(0.8);
        }
        // Repeat wrapping with bilinear filtering.
        let x = uv.x.rem_euclid(1.0) * self.width as f32 - 0.5;
        let y = uv.y.rem_euclid(1.0) * self.height as f32 - 0.5;
        let x0 = x.floor() as i64;
        let y0 = y.floor() as i64;
        let fx = x - x.floor();
        let fy = y - y.floor();
        let fetch = |ix: i64, iy: i64| -> Vec3 {
            let wx = ix.rem_euclid(self.width as i64) as usize;
            let wy = iy.rem_euclid(self.height as i64) as usize;
            self.pixels
                .get(wy * self.width as usize + wx)
                .copied()
                .unwrap_or(Vec3::splat(0.8))
        };
        let c00 = fetch(x0, y0);
        let c10 = fetch(x0 + 1, y0);
        let c01 = fetch(x0, y0 + 1);
        let c11 = fetch(x0 + 1, y0 + 1);
        c00.lerp(c10, fx).lerp(c01.lerp(c11, fx), fy)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let image = image::open(path)
            .map_err(|e| format!("failed to load texture {}: {e}", path.display()))?;
        let hdr = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("hdr") || e.eq_ignore_ascii_case("exr"))
            .unwrap_or(false);
        let rgba = image.to_rgba32f();
        let (w, h) = (rgba.width(), rgba.height());
        if w == 0 || h == 0 {
            return Err("texture has zero size".into());
        }
        let mut pixels = Vec::with_capacity((w * h) as usize);
        for p in rgba.pixels() {
            let c = Vec3::new(p[0], p[1], p[2]);
            pixels.push(if hdr {
                c
            } else {
                Vec3::new(
                    srgb_to_linear(c.x),
                    srgb_to_linear(c.y),
                    srgb_to_linear(c.z),
                )
            });
        }
        Ok(Self {
            width: w,
            height: h,
            pixels,
            hdr,
        })
    }
}

/// Lazily populated texture cache with a base directory for relative paths.
#[derive(Clone, Debug, Default)]
pub struct TextureCache {
    pub base_dir: PathBuf,
    textures: HashMap<String, Texture2D>,
    failed: HashSet<String>,
}

impl TextureCache {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            textures: HashMap::new(),
            failed: HashSet::new(),
        }
    }

    pub fn set_base_dir(&mut self, dir: impl Into<PathBuf>) {
        self.base_dir = dir.into();
    }

    pub fn ensure(&mut self, path: &str) -> bool {
        if path.trim().is_empty() {
            return false;
        }
        if self.textures.contains_key(path) {
            return true;
        }
        if self.failed.contains(path) {
            return false;
        }
        let p = PathBuf::from(path);
        let resolved = if p.is_absolute() {
            p
        } else {
            self.base_dir.join(p)
        };
        match Texture2D::load(&resolved) {
            Ok(tex) => {
                self.textures.insert(path.to_string(), tex);
                true
            }
            Err(_) => {
                self.failed.insert(path.to_string());
                false
            }
        }
    }

    pub fn get(&self, path: &str) -> Option<&Texture2D> {
        self.textures.get(path)
    }

    pub fn insert(&mut self, path: impl Into<String>, texture: Texture2D) {
        self.textures.insert(path.into(), texture);
    }

    pub fn loaded_count(&self) -> usize {
        self.textures.len()
    }

    pub fn missing(&self) -> impl Iterator<Item = &String> {
        self.failed.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bilinear_sampling_is_smooth() {
        let tex = Texture2D::from_pixels(2, 2, vec![Vec3::ZERO, Vec3::X, Vec3::Z, Vec3::ONE]);
        let left = tex.sample(Vec2::new(0.0, 0.0));
        let mid = tex.sample(Vec2::new(0.5, 0.5));
        assert!(mid.length() > 0.0);
        assert!(left.is_finite());
    }

    #[test]
    fn repeat_wrapping_wraps() {
        let tex = Texture2D::from_pixels(2, 1, vec![Vec3::ZERO, Vec3::X]);
        let a = tex.sample(Vec2::new(0.25, 0.5));
        let b = tex.sample(Vec2::new(1.25, 0.5));
        assert!((a - b).length() < 1e-5);
    }
}
