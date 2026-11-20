//! Image export: PNG (tone mapped) and linear EXR.

use crate::exr::write_exr;
use crate::PathTracer;
use glam::Vec3;
use std::path::Path;

pub fn save_rgba_png(
    path: impl AsRef<Path>,
    pixels: &[[u8; 4]],
    width: u32,
    height: u32,
) -> Result<(), String> {
    if pixels.len() != width as usize * height as usize {
        return Err("pixel buffer does not match image dimensions".into());
    }
    let mut raw = Vec::with_capacity(pixels.len() * 4);
    for p in pixels {
        raw.extend_from_slice(p);
    }
    image::save_buffer_with_format(
        path.as_ref(),
        &raw,
        width,
        height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|e| format!("failed to write {}: {e}", path.as_ref().display()))
}

pub fn save_tracer_png(
    path: impl AsRef<Path>,
    tracer: &PathTracer,
    exposure: f32,
    transparent_background: bool,
) -> Result<(), String> {
    let pixels = tracer.rgba8(exposure, transparent_background);
    save_rgba_png(path, &pixels, tracer.width, tracer.height)
}

pub fn save_linear_exr(
    path: impl AsRef<Path>,
    pixels: &[Vec3],
    width: u32,
    height: u32,
) -> Result<(), String> {
    write_exr(path, pixels, width, height)
}

/// Write a PNG from already-linear pixels, tone mapping with the standard
/// ACES curve and sRGB encoding.
pub fn save_linear_png(
    path: impl AsRef<Path>,
    pixels: &[Vec3],
    width: u32,
    height: u32,
    exposure: f32,
) -> Result<(), String> {
    let rgba: Vec<[u8; 4]> = pixels
        .iter()
        .map(|p| {
            let [r, g, b] = lc_core::to_srgb8(*p, exposure);
            [r, g, b, 255]
        })
        .collect();
    save_rgba_png(path, &rgba, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RenderScene;
    use lc_scene::Document;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "leopardcraft-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ))
    }

    #[test]
    fn writes_png_and_exr_files() {
        let doc = Document::demo();
        let scene = RenderScene::from_document(&doc, 0.0, ".").unwrap();
        let mut tracer = PathTracer::new(12, 8, 5);
        tracer.render_pass(&scene, 1);
        let png = temp_path("test.png");
        save_tracer_png(&png, &tracer, 0.0, false).unwrap();
        let bytes = std::fs::read(&png).unwrap();
        assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
        let brightest = decoded
            .pixels()
            .map(|p| p[0].max(p[1]).max(p[2]))
            .max()
            .unwrap_or(0);
        assert!(brightest > 8, "rendered PNG must not be uniformly black");
        let _ = std::fs::remove_file(&png);

        let exr = temp_path("test.exr");
        let pixels = tracer.hdr_pixels();
        save_linear_exr(&exr, &pixels, tracer.width, tracer.height).unwrap();
        let bytes = std::fs::read(&exr).unwrap();
        assert_eq!(&bytes[0..4], &20_000_630u32.to_le_bytes());
        let _ = std::fs::remove_file(&exr);
    }
}
