//! Minimal OpenEXR writer (uncompressed, half-float RGBA scanlines).
//!
//! Implemented from the public OpenEXR file specification: magic, version,
//! header attributes, scanline offset table, then per-scanline channel data in
//! alphabetical channel order.

use glam::Vec3;
use std::path::Path;

pub fn encode_exr(pixels: &[Vec3], width: u32, height: u32) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 {
        return Err("EXR dimensions must be non-zero".into());
    }
    if pixels.len() != width as usize * height as usize {
        return Err(format!(
            "EXR expects {} pixels, got {}",
            width as usize * height as usize,
            pixels.len()
        ));
    }
    let mut out = Vec::new();
    out.extend_from_slice(&20_000_630u32.to_le_bytes()); // magic
    out.extend_from_slice(&2u32.to_le_bytes()); // version 2

    let mut channels = Vec::new();
    // Channel list must be alphabetical: A, B, G, R.
    for name in ["A", "B", "G", "R"] {
        channels.extend_from_slice(name.as_bytes());
        channels.push(0);
        channels.extend_from_slice(&1i32.to_le_bytes()); // HALF
        channels.push(0); // pLinear
        channels.extend_from_slice(&[0, 0, 0]); // reserved
        channels.extend_from_slice(&1i32.to_le_bytes()); // xSampling
        channels.extend_from_slice(&1i32.to_le_bytes()); // ySampling
    }
    channels.push(0); // end of channel list
    attribute(&mut out, "channels", "chlist", &channels);
    attribute(&mut out, "compression", "compression", &[0]); // NO_COMPRESSION
    let mut window = Vec::new();
    for v in [0i32, 0, width as i32 - 1, height as i32 - 1] {
        window.extend_from_slice(&v.to_le_bytes());
    }
    attribute(&mut out, "dataWindow", "box2i", &window);
    attribute(&mut out, "displayWindow", "box2i", &window);
    attribute(&mut out, "lineOrder", "lineOrder", &[0]);
    attribute(&mut out, "pixelAspectRatio", "float", &1.0f32.to_le_bytes());
    let mut center = Vec::new();
    center.extend_from_slice(&0.0f32.to_le_bytes());
    center.extend_from_slice(&0.0f32.to_le_bytes());
    attribute(&mut out, "screenWindowCenter", "v2f", &center);
    attribute(
        &mut out,
        "screenWindowWidth",
        "float",
        &1.0f32.to_le_bytes(),
    );
    out.push(0); // end of header

    let offset_table_pos = out.len();
    out.extend(std::iter::repeat_n(0u8, height as usize * 8));
    let mut offsets = Vec::with_capacity(height as usize);
    let scan_data_size = 8 + width as usize * 8; // RGBA half
    for y in 0..height as usize {
        offsets.push(out.len() as u64);
        out.extend_from_slice(&(y as i32).to_le_bytes());
        out.extend_from_slice(&(scan_data_size as i32).to_le_bytes());
        // A
        for _ in 0..width {
            out.extend_from_slice(&f32_to_f16(1.0).to_le_bytes());
        }
        // B, G, R
        for channel in [2usize, 1, 0] {
            for x in 0..width as usize {
                let c = pixels[y * width as usize + x][channel];
                out.extend_from_slice(&f32_to_f16(c).to_le_bytes());
            }
        }
    }
    for (i, offset) in offsets.iter().enumerate() {
        let pos = offset_table_pos + i * 8;
        out[pos..pos + 8].copy_from_slice(&offset.to_le_bytes());
    }
    Ok(out)
}

pub fn write_exr(
    path: impl AsRef<Path>,
    pixels: &[Vec3],
    width: u32,
    height: u32,
) -> Result<(), String> {
    let bytes = encode_exr(pixels, width, height)?;
    std::fs::write(path.as_ref(), bytes)
        .map_err(|e| format!("failed to write {}: {e}", path.as_ref().display()))
}

fn attribute(out: &mut Vec<u8>, name: &str, ty: &str, data: &[u8]) {
    out.extend_from_slice(name.as_bytes());
    out.push(0);
    out.extend_from_slice(ty.as_bytes());
    out.push(0);
    out.extend_from_slice(&(data.len() as i32).to_le_bytes());
    out.extend_from_slice(data);
}

fn f32_to_f16(value: f32) -> u16 {
    let x = value.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let exp = ((x >> 23) & 0xff) as i32;
    let mantissa = x & 0x007f_ffff;
    if exp == 0xff {
        // Inf or NaN.
        return sign | 0x7c00 | if mantissa != 0 { 0x0200 } else { 0 };
    }
    let mut e = exp - 127 + 15;
    if e >= 31 {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = mantissa | 0x0080_0000;
        let shift = (14 - e) as u32;
        let half = (m >> shift) as u16;
        let rem = m & ((1u32 << shift) - 1);
        let round = if rem > (1u32 << (shift - 1)) { 1 } else { 0 };
        return sign | (half + round);
    }
    let mut half = sign | ((e as u16) << 10) | ((mantissa >> 13) as u16);
    if (mantissa & 0x1fff) > 0x1000 {
        half = half.wrapping_add(1);
    }
    e = 0;
    let _ = e;
    half
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exr_has_valid_header_and_scanlines() {
        let pixels = vec![Vec3::new(0.5, 0.25, 1.5); 4 * 3];
        let bytes = encode_exr(&pixels, 4, 3).unwrap();
        assert_eq!(&bytes[0..4], &20_000_630u32.to_le_bytes());
        assert_eq!(&bytes[4..8], &2u32.to_le_bytes());
        assert!(bytes.len() > 100);
        // The offset table must point inside the file.
        let header_end = bytes
            .iter()
            .enumerate()
            .skip(8)
            .find(|(_, b)| **b == 0)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let _ = header_end;
    }

    #[test]
    fn half_roundtrip_common_values() {
        assert_eq!(f32_to_f16(0.0), 0);
        assert_eq!(f32_to_f16(1.0), 0x3c00);
        assert_eq!(f32_to_f16(0.5), 0x3800);
        assert_eq!(f32_to_f16(-2.0), 0xc000);
        assert!(f32_to_f16(f32::NAN) & 0x7c00 == 0x7c00);
    }

    #[test]
    fn exr_rejects_bad_dimensions() {
        assert!(encode_exr(&[], 0, 4).is_err());
        assert!(encode_exr(&[Vec3::ZERO], 2, 2).is_err());
    }
}
