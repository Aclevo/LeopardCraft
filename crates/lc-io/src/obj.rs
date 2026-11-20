//! Wavefront OBJ + MTL import and export.
//!
//! Deliberately covers the widely implemented subset: vertices, per-corner
//! texture coordinates, normals, polygons/objects, material libraries and the
//! standard diffuse/specular/roughness/opacity/emission entries.

use crate::common::{
    add_mesh_node, add_texture_to_base_color, preview_surface, safe_material_name,
    safe_object_name, set_principled_input,
};
use glam::{Mat3, Vec2, Vec3};
use lc_geom::{Corner, Mesh, Polygon};
use lc_scene::Document;
use lc_shading::{Material, MaterialId, ShaderValue};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
struct Mtl {
    name: String,
    kd: Vec3,
    ks: Vec3,
    ns: f32,
    alpha: f32,
    ke: Vec3,
    map_kd: Option<String>,
}

impl Mtl {
    fn new(name: String) -> Self {
        Self {
            name,
            kd: Vec3::splat(0.8),
            ks: Vec3::ZERO,
            ns: 250.0,
            alpha: 1.0,
            ke: Vec3::ZERO,
            map_kd: None,
        }
    }
}

#[derive(Clone, Debug)]
struct Face {
    points: Vec<u32>,
    uvs: Vec<u32>,
    material: Option<String>,
}

#[derive(Clone, Debug)]
struct Group {
    name: String,
    faces: Vec<Face>,
}

pub fn export(path: impl AsRef<Path>, doc: &Document) -> Result<(), String> {
    let path = path.as_ref();
    let mtl_path = path.with_extension("mtl");
    let mtl_name = mtl_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "materials.mtl".into());

    let evaluated = doc.evaluate(0.0);
    let mut material_names: HashMap<MaterialId, String> = HashMap::new();
    let mut used_names = Vec::new();
    let mut mtl = String::new();
    writeln!(mtl, "# LeopardCraft material library").ok();
    for material in doc.materials.iter() {
        let name = safe_material_name(&material.name, &mut used_names);
        material_names.insert(material.id, name.clone());
        let surface = preview_surface(material);
        writeln!(mtl, "\nnewmtl {name}").ok();
        writeln!(mtl, "Ka 0 0 0").ok();
        writeln!(
            mtl,
            "Kd {:.6} {:.6} {:.6}",
            surface.base_color.x, surface.base_color.y, surface.base_color.z
        )
        .ok();
        let spec = Vec3::splat(0.04).lerp(surface.base_color, surface.metallic);
        writeln!(mtl, "Ks {:.6} {:.6} {:.6}", spec.x, spec.y, spec.z).ok();
        writeln!(mtl, "Ns {:.3}", (1.0 - surface.roughness) * 1000.0).ok();
        writeln!(mtl, "d {:.6}", surface.alpha.clamp(0.0, 1.0)).ok();
        writeln!(mtl, "Tr {:.6}", 1.0 - surface.alpha.clamp(0.0, 1.0)).ok();
        writeln!(
            mtl,
            "Ke {:.6} {:.6} {:.6}",
            surface.emission.x, surface.emission.y, surface.emission.z
        )
        .ok();
        if let Some(tex) = material.graph.texture_paths().into_iter().next() {
            writeln!(mtl, "map_Kd {}", tex).ok();
        }
    }
    std::fs::write(&mtl_path, mtl)
        .map_err(|e| format!("failed to write {}: {e}", mtl_path.display()))?;

    let mut out = String::new();
    writeln!(out, "# LeopardCraft OBJ export").ok();
    writeln!(out, "mtllib {mtl_name}").ok();
    let mut v_offset = 1usize;
    let mut vt_offset = 1usize;
    let mut vn_offset = 1usize;
    let fallback_name = material_names
        .values()
        .next()
        .cloned()
        .unwrap_or_else(|| "default".into());

    for node in evaluated.mesh_nodes() {
        let Some(mesh) = node.mesh() else {
            continue;
        };
        if mesh.is_empty() {
            continue;
        }
        let name = safe_object_name(&node.name);
        writeln!(out, "\no {name}").ok();
        let world = node.world;
        let normal_matrix = Mat3::from_mat4(world).inverse().transpose();
        for p in &mesh.positions {
            let p = world.transform_point3(*p);
            writeln!(out, "v {:.6} {:.6} {:.6}", p.x, p.y, p.z).ok();
        }
        for uv in &mesh.uvs {
            writeln!(out, "vt {:.6} {:.6}", uv.x, uv.y).ok();
        }
        let normals = mesh.corner_normals(40.0);
        let mut normal_count = 0usize;
        for (pi, poly) in mesh.polygons.iter().enumerate() {
            for (ci, _) in poly.corners.iter().enumerate() {
                let n = normals
                    .get(pi)
                    .and_then(|face| face.get(ci))
                    .copied()
                    .unwrap_or(Vec3::Y);
                let n = (normal_matrix * n).normalize_or(Vec3::Y);
                writeln!(out, "vn {:.6} {:.6} {:.6}", n.x, n.y, n.z).ok();
                normal_count += 1;
            }
        }

        let mut current_material: Option<String> = None;
        let mut corner_base = 0usize;
        for poly in &mesh.polygons {
            let mat_id = node.material_for_slot(poly.material_slot).unwrap_or(0);
            let name = material_names
                .get(&mat_id)
                .cloned()
                .unwrap_or_else(|| fallback_name.clone());
            if current_material.as_deref() != Some(name.as_str()) {
                writeln!(out, "usemtl {name}").ok();
                current_material = Some(name);
            }
            let has_uv = !mesh.uvs.is_empty();
            let mut face = String::new();
            for (ci, corner) in poly.corners.iter().enumerate() {
                let v = v_offset + corner.point as usize;
                let vn = vn_offset + corner_base + ci;
                if has_uv {
                    let vt = vt_offset + (corner.uv as usize).min(mesh.uvs.len() - 1);
                    write!(face, " {v}/{vt}/{vn}").ok();
                } else {
                    write!(face, " {v}//{vn}").ok();
                }
            }
            writeln!(out, "f{}", face).ok();
            corner_base += poly.corners.len();
        }
        v_offset += mesh.positions.len();
        vt_offset += mesh.uvs.len();
        vn_offset += normal_count;
    }
    std::fs::write(path, out).map_err(|e| format!("failed to write {}: {e}", path.display()))
}

pub fn import(path: impl AsRef<Path>) -> Result<Document, String> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let base = path.parent().unwrap_or(Path::new("."));
    let mut mtls = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.to_ascii_lowercase().starts_with("mtllib ") {
            let name = line[7..].trim();
            if !name.is_empty() {
                let mtl_path = resolve_path(base, name);
                if let Ok(mtl_text) = std::fs::read_to_string(&mtl_path) {
                    parse_mtl_text(&mtl_text, mtl_path.parent().unwrap_or(base), &mut mtls);
                }
            }
        }
    }

    let mut positions: Vec<Vec3> = Vec::new();
    let mut uvs: Vec<Vec2> = Vec::new();
    let mut groups: Vec<Group> = Vec::new();
    let mut group_index: HashMap<String, usize> = HashMap::new();
    let mut current_group = 0usize;
    groups.push(Group {
        name: "Imported OBJ".into(),
        faces: Vec::new(),
    });
    group_index.insert("Imported OBJ".into(), 0);
    let mut current_material: Option<String> = None;

    let ensure_group =
        |name: &str, groups: &mut Vec<Group>, group_index: &mut HashMap<String, usize>| -> usize {
            if let Some(&i) = group_index.get(name) {
                return i;
            }
            let i = groups.len();
            groups.push(Group {
                name: name.to_string(),
                faces: Vec::new(),
            });
            group_index.insert(name.to_string(), i);
            i
        };

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(tag) = parts.next() else {
            continue;
        };
        match tag {
            "v" => {
                let values: Vec<f32> = parts.filter_map(|v| v.parse().ok()).collect();
                if values.len() >= 3 && values.iter().all(|v| v.is_finite()) {
                    positions.push(Vec3::new(values[0], values[1], values[2]));
                }
            }
            "vt" => {
                let values: Vec<f32> = parts.filter_map(|v| v.parse().ok()).collect();
                if values.len() >= 2 && values.iter().all(|v| v.is_finite()) {
                    uvs.push(Vec2::new(values[0], values[1]));
                }
            }
            "o" | "g" => {
                let name = parts.collect::<Vec<_>>().join(" ");
                if !name.is_empty() {
                    current_group = ensure_group(&name, &mut groups, &mut group_index);
                }
            }
            "usemtl" => {
                let name = parts.collect::<Vec<_>>().join(" ");
                current_material = if name.is_empty() { None } else { Some(name) };
            }
            "f" => {
                let mut points = Vec::new();
                let mut face_uvs = Vec::new();
                for token in parts {
                    let pieces: Vec<&str> = token.split('/').collect();
                    let Some(v) = pieces
                        .first()
                        .and_then(|s| resolve_index(s, positions.len()))
                    else {
                        continue;
                    };
                    points.push(v);
                    let uv = pieces
                        .get(1)
                        .and_then(|s| resolve_index(s, uvs.len()))
                        .unwrap_or(0);
                    face_uvs.push(uv);
                }
                if points.len() >= 3 {
                    groups[current_group].faces.push(Face {
                        points,
                        uvs: face_uvs,
                        material: current_material.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    let mut doc = Document::empty();
    let mut material_ids: HashMap<String, MaterialId> = HashMap::new();
    for mtl in &mtls {
        if material_ids.contains_key(&mtl.name) {
            continue;
        }
        let id = doc.materials.add(material_from_mtl(mtl));
        material_ids.insert(mtl.name.clone(), id);
    }

    for group in groups {
        if group.faces.is_empty() {
            continue;
        }
        let mut mesh = Mesh::new(group.name.clone());
        mesh.positions = positions.clone();
        mesh.uvs = uvs.clone();
        let mut slots: HashMap<String, u16> = HashMap::new();
        let mut slot_ids: Vec<MaterialId> = Vec::new();
        for face in &group.faces {
            let material_name = face.material.clone().unwrap_or_default();
            let slot = if let Some(s) = slots.get(&material_name) {
                *s
            } else {
                if slot_ids.len() >= u16::MAX as usize {
                    return Err("too many materials in OBJ".into());
                }
                let slot = slot_ids.len() as u16;
                let id = material_ids
                    .get(&material_name)
                    .copied()
                    .or_else(|| {
                        doc.materials
                            .iter()
                            .find(|m| m.name == material_name)
                            .map(|m| m.id)
                    })
                    .unwrap_or(0);
                slots.insert(material_name, slot);
                slot_ids.push(id);
                slot
            };
            let corners: Vec<Corner> = face
                .points
                .iter()
                .enumerate()
                .map(|(i, p)| Corner::new(*p, face.uvs.get(i).copied().unwrap_or(0)))
                .collect();
            mesh.polygons.push(Polygon {
                corners,
                material_slot: slot,
            });
        }
        if mesh.uvs.is_empty() {
            for poly in &mut mesh.polygons {
                for c in &mut poly.corners {
                    c.uv = 0;
                }
            }
        }
        let id = add_mesh_node(&mut doc, &group.name, mesh, None)?;
        if let Some(node) = doc.scene.get_mut(id) {
            for (slot, mat) in slot_ids.iter().enumerate() {
                node.material_slots.insert(slot as u16, *mat);
            }
            node.material = slot_ids.first().copied().or(Some(0));
        }
    }
    doc.settings.ensure_valid_layers();
    doc.sanitize();
    Ok(doc)
}

fn material_from_mtl(mtl: &Mtl) -> Material {
    let roughness = (1.0 - (mtl.ns / 1000.0)).clamp(0.04, 1.0);
    let mut material = Material::pbr(0, mtl.name.clone(), mtl.kd.max(Vec3::ZERO), roughness, 0.0);
    let metallic = if mtl.ks.max_element() > 0.5 { 0.3 } else { 0.0 };
    set_principled_input(&mut material, 1, ShaderValue::Float(metallic));
    set_principled_input(
        &mut material,
        7,
        ShaderValue::Float(mtl.alpha.clamp(0.0, 1.0)),
    );
    if mtl.ke.max_element() > 0.0 {
        set_principled_input(&mut material, 5, ShaderValue::Color(mtl.ke));
        set_principled_input(&mut material, 6, ShaderValue::Float(1.0));
    }
    if let Some(path) = &mtl.map_kd {
        add_texture_to_base_color(&mut material, path);
    }
    material
}

fn parse_mtl_text(text: &str, base: &Path, out: &mut Vec<Mtl>) {
    let mut current: Option<Mtl> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(tag) = parts.next() else {
            continue;
        };
        match tag.to_ascii_lowercase().as_str() {
            "newmtl" => {
                if let Some(m) = current.take() {
                    out.push(m);
                }
                let name = parts.collect::<Vec<_>>().join(" ");
                current = Some(Mtl::new(if name.is_empty() {
                    "Material".into()
                } else {
                    name
                }));
            }
            "kd" | "ks" | "ke" => {
                let Some(m) = current.as_mut() else {
                    continue;
                };
                let values: Vec<f32> = parts.filter_map(|v| v.parse().ok()).collect();
                let color = match values.as_slice() {
                    [r, g, b, ..] => Vec3::new(*r, *g, *b),
                    [v] => Vec3::splat(*v),
                    _ => continue,
                };
                match tag.to_ascii_lowercase().as_str() {
                    "kd" => m.kd = color,
                    "ks" => m.ks = color,
                    "ke" => m.ke = color,
                    _ => {}
                }
            }
            "ns" => {
                if let (Some(m), Some(v)) = (current.as_mut(), parts.next()) {
                    if let Ok(v) = v.parse::<f32>() {
                        m.ns = v.clamp(0.0, 1000.0);
                    }
                }
            }
            "d" => {
                if let (Some(m), Some(v)) = (current.as_mut(), parts.next()) {
                    if let Ok(v) = v.parse::<f32>() {
                        m.alpha = v.clamp(0.0, 1.0);
                    }
                }
            }
            "tr" => {
                if let (Some(m), Some(v)) = (current.as_mut(), parts.next()) {
                    if let Ok(v) = v.parse::<f32>() {
                        m.alpha = (1.0 - v).clamp(0.0, 1.0);
                    }
                }
            }
            "map_kd" => {
                let Some(m) = current.as_mut() else {
                    continue;
                };
                let name = parts.collect::<Vec<_>>().join(" ");
                if !name.is_empty() {
                    let resolved = resolve_path(base, &name);
                    m.map_kd = Some(resolved.to_string_lossy().to_string());
                }
            }
            _ => {}
        }
    }
    if let Some(m) = current {
        out.push(m);
    }
}

fn resolve_index(token: &str, len: usize) -> Option<u32> {
    if token.is_empty() || len == 0 {
        return None;
    }
    let value: i64 = token.parse().ok()?;
    if value > 0 {
        let idx = (value - 1) as usize;
        if idx < len {
            Some(idx as u32)
        } else {
            None
        }
    } else if value < 0 {
        let idx = len as i64 + value;
        if idx >= 0 && (idx as usize) < len {
            Some(idx as u32)
        } else {
            None
        }
    } else {
        None
    }
}

fn resolve_path(base: &Path, name: &str) -> PathBuf {
    let p = Path::new(name);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}
