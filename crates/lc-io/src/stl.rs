//! STL import/export (binary and ASCII).

use crate::common::add_mesh_node;
use glam::Vec3;
use lc_geom::Mesh;
use lc_scene::Document;
use std::fmt::Write as _;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StlFormat {
    Binary,
    Ascii,
}

pub fn export(path: impl AsRef<Path>, doc: &Document, format: StlFormat) -> Result<(), String> {
    let path = path.as_ref();
    let evaluated = doc.evaluate(0.0);
    let mut triangles: Vec<[Vec3; 3]> = Vec::new();
    for node in evaluated.mesh_nodes() {
        let Some(mesh) = node.mesh() else {
            continue;
        };
        for tri in mesh.render_triangles(40.0) {
            triangles.push(tri.v.map(|v| node.world.transform_point3(v.position)));
        }
    }
    match format {
        StlFormat::Binary => {
            let mut bytes = Vec::with_capacity(84 + triangles.len() * 50);
            let mut header = [0u8; 80];
            let label = b"LeopardCraft binary STL";
            header[..label.len()].copy_from_slice(label);
            bytes.extend_from_slice(&header);
            let count = u32::try_from(triangles.len())
                .map_err(|_| "too many triangles for binary STL".to_string())?;
            bytes.extend_from_slice(&count.to_le_bytes());
            for tri in &triangles {
                let normal = (tri[1] - tri[0])
                    .cross(tri[2] - tri[0])
                    .normalize_or(Vec3::Y);
                for v in [normal, tri[0], tri[1], tri[2]] {
                    bytes.extend_from_slice(&v.x.to_le_bytes());
                    bytes.extend_from_slice(&v.y.to_le_bytes());
                    bytes.extend_from_slice(&v.z.to_le_bytes());
                }
                bytes.extend_from_slice(&0u16.to_le_bytes());
            }
            std::fs::write(path, bytes)
                .map_err(|e| format!("failed to write {}: {e}", path.display()))
        }
        StlFormat::Ascii => {
            let mut out = String::from("solid leopardcraft\n");
            for tri in &triangles {
                let normal = (tri[1] - tri[0])
                    .cross(tri[2] - tri[0])
                    .normalize_or(Vec3::Y);
                writeln!(out, "  facet normal {} {} {}", normal.x, normal.y, normal.z).ok();
                writeln!(out, "    outer loop").ok();
                for v in tri {
                    writeln!(out, "      vertex {} {} {}", v.x, v.y, v.z).ok();
                }
                writeln!(out, "    endloop").ok();
                writeln!(out, "  endfacet").ok();
            }
            writeln!(out, "endsolid leopardcraft").ok();
            std::fs::write(path, out)
                .map_err(|e| format!("failed to write {}: {e}", path.display()))
        }
    }
}

pub fn import(path: impl AsRef<Path>) -> Result<Document, String> {
    let path = path.as_ref();
    let bytes =
        std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let triangles = parse(&bytes)?;
    if triangles.is_empty() {
        return Err("STL contains no triangles".into());
    }
    let mut mesh = Mesh::new("Imported STL");
    for tri in triangles {
        let base = mesh.positions.len() as u32;
        mesh.positions.extend_from_slice(&tri);
        mesh.add_face(&[base, base + 1, base + 2]);
    }
    mesh.weld(1e-6);
    mesh.remove_degenerate_polygons();
    let mut doc = Document::empty();
    add_mesh_node(&mut doc, "Imported STL", mesh, None)?;
    doc.settings.ensure_valid_layers();
    doc.sanitize();
    Ok(doc)
}

fn parse(bytes: &[u8]) -> Result<Vec<[Vec3; 3]>, String> {
    let looks_ascii = bytes
        .iter()
        .take(128)
        .copied()
        .filter(|b| !b.is_ascii_whitespace())
        .take(5)
        .eq(b"solid".iter().copied());
    if looks_ascii {
        let text = std::str::from_utf8(bytes).map_err(|_| "ASCII STL is not valid UTF-8")?;
        return parse_ascii(text);
    }
    if bytes.len() >= 84 {
        let count = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
        if count <= 10_000_000 && bytes.len() >= 84 + count * 50 {
            let mut triangles = Vec::with_capacity(count);
            for i in 0..count {
                let offset = 84 + i * 50;
                let mut values = [0.0f32; 9];
                for (j, value) in values.iter_mut().enumerate() {
                    let start = offset + 12 + j * 4;
                    *value = f32::from_le_bytes([
                        bytes[start],
                        bytes[start + 1],
                        bytes[start + 2],
                        bytes[start + 3],
                    ]);
                }
                let tri = [
                    Vec3::new(values[0], values[1], values[2]),
                    Vec3::new(values[3], values[4], values[5]),
                    Vec3::new(values[6], values[7], values[8]),
                ];
                if tri.iter().all(|v| v.is_finite()) {
                    triangles.push(tri);
                }
            }
            return Ok(triangles);
        }
    }
    Err("unrecognised STL file".into())
}

fn parse_ascii(text: &str) -> Result<Vec<[Vec3; 3]>, String> {
    let mut triangles = Vec::new();
    let mut vertices: Vec<Vec3> = Vec::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if parts
            .next()
            .map(|p| p.eq_ignore_ascii_case("vertex"))
            .unwrap_or(false)
        {
            let values: Vec<f32> = parts.filter_map(|v| v.parse().ok()).collect();
            if values.len() == 3 && values.iter().all(|v| v.is_finite()) {
                vertices.push(Vec3::new(values[0], values[1], values[2]));
                if vertices.len() == 3 {
                    triangles.push([vertices[0], vertices[1], vertices[2]]);
                    vertices.clear();
                }
            }
        }
    }
    Ok(triangles)
}
