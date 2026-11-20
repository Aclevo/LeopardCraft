//! glTF 2.0 / GLB import and export.
//!
//! Export preserves the document hierarchy, local transforms and evaluated
//! (modifier-applied) meshes with PBR materials. Import supports indexed
//! triangle primitives, per-corner UVs, node transforms/hierarchies, materials
//! and external or embedded buffer data.

use crate::common::preview_surface;
use glam::{Mat4, Quat, Vec2, Vec3};
use lc_geom::Mesh;
use lc_scene::{Document, GeometrySource, NodeId, NodeKind};
use lc_shading::{Material, MaterialId, ShaderNodeKind, ShaderValue};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

const GLB_MAGIC: u32 = 0x4654_6C67;
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;

pub fn export_glb(path: impl AsRef<Path>, doc: &Document) -> Result<(), String> {
    let (json_value, binary) = build(doc)?;
    let mut json_bytes = serde_json::to_vec(&json_value)
        .map_err(|e| format!("failed to serialise glTF JSON: {e}"))?;
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let mut bin = binary;
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let total_len = 12 + 8 + json_bytes.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(&GLB_MAGIC.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total_len as u32).to_le_bytes());
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&CHUNK_JSON.to_le_bytes());
    out.extend_from_slice(&json_bytes);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(&CHUNK_BIN.to_le_bytes());
    out.extend_from_slice(&bin);
    std::fs::write(path.as_ref(), out)
        .map_err(|e| format!("failed to write {}: {e}", path.as_ref().display()))
}

pub fn export_gltf(path: impl AsRef<Path>, doc: &Document) -> Result<(), String> {
    let (mut json_value, binary) = build(doc)?;
    let encoded = base64_encode(&binary);
    if let Some(buffers) = json_value.get_mut("buffers").and_then(|b| b.as_array_mut()) {
        if let Some(first) = buffers.first_mut() {
            first["uri"] = Value::String(format!("data:application/octet-stream;base64,{encoded}"));
        }
    }
    let text = serde_json::to_string_pretty(&json_value)
        .map_err(|e| format!("failed to serialise glTF JSON: {e}"))?;
    std::fs::write(path.as_ref(), text)
        .map_err(|e| format!("failed to write {}: {e}", path.as_ref().display()))
}

pub fn import_glb(path: impl AsRef<Path>) -> Result<Document, String> {
    let path = path.as_ref();
    let bytes =
        std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let (gltf, bin) = parse_glb(&bytes)?;
    import_value(
        gltf,
        bin.as_deref(),
        path.parent().unwrap_or(Path::new(".")),
    )
}

pub fn import_gltf(path: impl AsRef<Path>) -> Result<Document, String> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let gltf: Value = serde_json::from_str(&text).map_err(|e| format!("invalid glTF JSON: {e}"))?;
    let buffers = load_buffers(&gltf, None, path.parent().unwrap_or(Path::new(".")))?;
    import_value_with_buffers(gltf, buffers)
}

#[derive(Default)]
struct GltfBuild {
    binary: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl GltfBuild {
    fn align(&mut self) {
        while self.binary.len() % 4 != 0 {
            self.binary.push(0);
        }
    }

    fn add_f32(
        &mut self,
        values: &[f32],
        ty: &str,
        components: usize,
        target: u32,
        bounds: bool,
    ) -> usize {
        self.align();
        let view_index = self.views.len();
        let byte_offset = self.binary.len();
        for value in values {
            self.binary.extend_from_slice(&value.to_le_bytes());
        }
        self.views.push(json!({
            "buffer": 0,
            "byteOffset": byte_offset,
            "byteLength": values.len() * 4,
            "target": target,
        }));
        let accessor_index = self.accessors.len();
        let count = if components == 0 {
            0
        } else {
            values.len() / components
        };
        let mut accessor = json!({
            "bufferView": view_index,
            "byteOffset": 0,
            "componentType": 5126,
            "count": count,
            "type": ty,
        });
        if bounds && components == 3 && !values.is_empty() {
            let mut min = [f32::INFINITY; 3];
            let mut max = [f32::NEG_INFINITY; 3];
            for chunk in values.chunks_exact(3) {
                for i in 0..3 {
                    min[i] = min[i].min(chunk[i]);
                    max[i] = max[i].max(chunk[i]);
                }
            }
            accessor["min"] = json!(min);
            accessor["max"] = json!(max);
        }
        self.accessors.push(accessor);
        accessor_index
    }

    fn add_u32(&mut self, values: &[u32], target: u32) -> usize {
        self.align();
        let view_index = self.views.len();
        let byte_offset = self.binary.len();
        for value in values {
            self.binary.extend_from_slice(&value.to_le_bytes());
        }
        self.views.push(json!({
            "buffer": 0,
            "byteOffset": byte_offset,
            "byteLength": values.len() * 4,
            "target": target,
        }));
        let accessor_index = self.accessors.len();
        self.accessors.push(json!({
            "bufferView": view_index,
            "byteOffset": 0,
            "componentType": 5125,
            "count": values.len(),
            "type": "SCALAR",
        }));
        accessor_index
    }
}

fn build(doc: &Document) -> Result<(Value, Vec<u8>), String> {
    let evaluated = doc.evaluate(0.0);
    let order = doc.scene.depth_first();
    let mut index_of_node: HashMap<NodeId, usize> = HashMap::new();
    for (i, id) in order.iter().enumerate() {
        index_of_node.insert(*id, i);
    }
    let mut builder = GltfBuild::default();
    let mut nodes_json: Vec<Value> = Vec::with_capacity(order.len());
    let mut meshes_json: Vec<Value> = Vec::new();
    let mut mesh_index_by_node: HashMap<NodeId, usize> = HashMap::new();
    let mut materials_json: Vec<Value> = Vec::new();
    let mut material_indices: BTreeMap<MaterialId, usize> = BTreeMap::new();

    for id in &order {
        let Some(node) = evaluated.node(*id) else {
            continue;
        };
        let mut node_json = Map::new();
        node_json.insert("name".into(), Value::String(node.name.clone()));
        node_json.insert(
            "translation".into(),
            json!([
                node.local.translation.x,
                node.local.translation.y,
                node.local.translation.z
            ]),
        );
        node_json.insert(
            "rotation".into(),
            json!([
                node.local.rotation.x,
                node.local.rotation.y,
                node.local.rotation.z,
                node.local.rotation.w
            ]),
        );
        node_json.insert(
            "scale".into(),
            json!([node.local.scale.x, node.local.scale.y, node.local.scale.z]),
        );
        let children: Vec<usize> = doc
            .scene
            .get(*id)
            .map(|n| {
                n.children
                    .iter()
                    .filter_map(|c| index_of_node.get(c).copied())
                    .collect()
            })
            .unwrap_or_default();
        if !children.is_empty() {
            node_json.insert("children".into(), json!(children));
        }

        if let Some(mesh) = node.mesh() {
            if !mesh.is_empty() {
                let mut groups: BTreeMap<MaterialId, Vec<lc_geom::RenderTriangle>> =
                    BTreeMap::new();
                for tri in mesh.render_triangles(40.0) {
                    let material = node.material_for_slot(tri.material_slot).unwrap_or(0);
                    groups.entry(material).or_default().push(tri);
                }
                let mut primitives = Vec::new();
                for (material_id, tris) in groups {
                    if tris.is_empty() {
                        continue;
                    }
                    let mut positions = Vec::with_capacity(tris.len() * 9);
                    let mut normals = Vec::with_capacity(tris.len() * 9);
                    let mut uvs = Vec::with_capacity(tris.len() * 6);
                    let mut indices = Vec::with_capacity(tris.len() * 3);
                    for tri in &tris {
                        for v in &tri.v {
                            positions.extend_from_slice(&[
                                v.position.x,
                                v.position.y,
                                v.position.z,
                            ]);
                            normals.extend_from_slice(&[v.normal.x, v.normal.y, v.normal.z]);
                            uvs.extend_from_slice(&[v.uv.x, v.uv.y]);
                            indices.push(indices.len() as u32);
                        }
                    }
                    if positions.is_empty() {
                        continue;
                    }
                    let pos_accessor = builder.add_f32(&positions, "VEC3", 3, 34962, true);
                    let normal_accessor = builder.add_f32(&normals, "VEC3", 3, 34962, false);
                    let uv_accessor = builder.add_f32(&uvs, "VEC2", 2, 34962, false);
                    let index_accessor = builder.add_u32(&indices, 34963);
                    let material_index =
                        material_json(doc, material_id, &mut materials_json, &mut material_indices);
                    primitives.push(json!({
                        "attributes": {
                            "POSITION": pos_accessor,
                            "NORMAL": normal_accessor,
                            "TEXCOORD_0": uv_accessor,
                        },
                        "indices": index_accessor,
                        "material": material_index,
                        "mode": 4,
                    }));
                }
                if !primitives.is_empty() {
                    let mesh_index = meshes_json.len();
                    meshes_json.push(json!({
                        "name": node.name,
                        "primitives": primitives,
                    }));
                    mesh_index_by_node.insert(*id, mesh_index);
                    node_json.insert("mesh".into(), json!(mesh_index));
                }
            }
        }
        nodes_json.push(Value::Object(node_json));
    }

    let root_indices: Vec<usize> = doc
        .scene
        .roots
        .iter()
        .filter_map(|id| index_of_node.get(id).copied())
        .collect();
    if builder.binary.is_empty() {
        builder.binary.extend_from_slice(&[0, 0, 0, 0]);
    }
    let mut root = json!({
        "asset": {
            "version": "2.0",
            "generator": "LeopardCraft",
        },
        "scene": 0,
        "scenes": [{
            "name": "Scene",
            "nodes": root_indices,
        }],
        "nodes": nodes_json,
        "accessors": builder.accessors,
        "bufferViews": builder.views,
        "buffers": [{ "byteLength": builder.binary.len() }],
    });
    if !meshes_json.is_empty() {
        root["meshes"] = json!(meshes_json);
    }
    if !materials_json.is_empty() {
        root["materials"] = json!(materials_json);
    }
    Ok((root, builder.binary))
}

fn material_json(
    doc: &Document,
    id: MaterialId,
    out: &mut Vec<Value>,
    indices: &mut BTreeMap<MaterialId, usize>,
) -> usize {
    if let Some(index) = indices.get(&id) {
        return *index;
    }
    let index = out.len();
    indices.insert(id, index);
    let material = doc.materials.get(id);
    let surface = material.map(preview_surface).unwrap_or_default();
    let base = surface.base_color.clamp(Vec3::ZERO, Vec3::splat(1.0));
    let mut value = json!({
        "name": material.map(|m| m.name.clone()).unwrap_or_else(|| "Material".into()),
        "doubleSided": material.map(|m| m.double_sided).unwrap_or(true),
        "pbrMetallicRoughness": {
            "baseColorFactor": [base.x, base.y, base.z, surface.alpha.clamp(0.0, 1.0)],
            "metallicFactor": surface.metallic.clamp(0.0, 1.0),
            "roughnessFactor": surface.roughness.clamp(0.04, 1.0),
        },
        "emissiveFactor": [
            surface.emission.x.max(0.0).min(1.0),
            surface.emission.y.max(0.0).min(1.0),
            surface.emission.z.max(0.0).min(1.0),
        ],
    });
    if surface.alpha < 0.999 {
        value["alphaMode"] = Value::String("BLEND".into());
    }
    out.push(value);
    index
}

fn parse_glb(bytes: &[u8]) -> Result<(Value, Option<Vec<u8>>), String> {
    if bytes.len() < 12 {
        return Err("GLB is too short".into());
    }
    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let total = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    if magic != GLB_MAGIC || version != 2 {
        return Err("not a glTF 2.0 binary file".into());
    }
    if total > bytes.len() {
        return Err("GLB header length exceeds file size".into());
    }
    let mut offset = 12usize;
    let mut json: Option<Value> = None;
    let mut bin: Option<Vec<u8>> = None;
    while offset + 8 <= total {
        let length = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let kind = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
        offset += 8;
        if offset + length > total {
            return Err("GLB chunk exceeds file size".into());
        }
        let data = &bytes[offset..offset + length];
        match kind {
            CHUNK_JSON => {
                let text = std::str::from_utf8(data)
                    .map_err(|_| "GLB JSON chunk is not valid UTF-8")?
                    .trim_end_matches(['\0', ' ']);
                json =
                    Some(serde_json::from_str(text).map_err(|e| format!("invalid GLB JSON: {e}"))?);
            }
            CHUNK_BIN => bin = Some(data.to_vec()),
            _ => {}
        }
        offset += length;
    }
    let json = json.ok_or_else(|| "GLB has no JSON chunk".to_string())?;
    Ok((json, bin))
}

fn load_buffers(
    gltf: &Value,
    embedded: Option<&[u8]>,
    base_dir: &Path,
) -> Result<Vec<Vec<u8>>, String> {
    let mut out = Vec::new();
    let empty = Vec::new();
    let buffers = gltf
        .get("buffers")
        .and_then(|b| b.as_array())
        .unwrap_or(&empty);
    for (i, buffer) in buffers.iter().enumerate() {
        if i == 0 {
            if let Some(bytes) = embedded {
                out.push(bytes.to_vec());
                continue;
            }
        }
        let uri = buffer.get("uri").and_then(|u| u.as_str()).unwrap_or("");
        if uri.is_empty() {
            return Err(format!("buffer {i} has no data"));
        }
        if let Some(data) = uri.strip_prefix("data:") {
            let payload = data
                .split_once(',')
                .map(|(_, p)| p)
                .ok_or_else(|| "malformed data URI".to_string())?;
            out.push(base64_decode(payload)?);
        } else {
            let path = if Path::new(uri).is_absolute() {
                PathBuf::from(uri)
            } else {
                base_dir.join(uri)
            };
            out.push(
                std::fs::read(&path)
                    .map_err(|e| format!("failed to read buffer {}: {e}", path.display()))?,
            );
        }
    }
    Ok(out)
}

fn import_value(gltf: Value, embedded: Option<&[u8]>, base_dir: &Path) -> Result<Document, String> {
    let buffers = load_buffers(&gltf, embedded, base_dir)?;
    import_value_with_buffers(gltf, buffers)
}

#[derive(Clone)]
struct ImportedMesh {
    mesh: Mesh,
    slots: BTreeMap<u16, MaterialId>,
}

fn import_value_with_buffers(gltf: Value, buffers: Vec<Vec<u8>>) -> Result<Document, String> {
    let mut doc = Document::empty();
    let mut material_map: BTreeMap<usize, MaterialId> = BTreeMap::new();
    if let Some(materials) = gltf.get("materials").and_then(|m| m.as_array()) {
        for (i, value) in materials.iter().enumerate() {
            let name = value
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("Material");
            let pbr = value.get("pbrMetallicRoughness");
            let base = pbr
                .and_then(|p| p.get("baseColorFactor"))
                .and_then(|c| c.as_array())
                .map(|c| {
                    Vec3::new(
                        c.first().and_then(|v| v.as_f64()).unwrap_or(0.8) as f32,
                        c.get(1).and_then(|v| v.as_f64()).unwrap_or(0.8) as f32,
                        c.get(2).and_then(|v| v.as_f64()).unwrap_or(0.8) as f32,
                    )
                })
                .unwrap_or(Vec3::splat(0.8));
            let metallic = pbr
                .and_then(|p| p.get("metallicFactor"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32;
            let roughness = pbr
                .and_then(|p| p.get("roughnessFactor"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5) as f32;
            let mut material = Material::pbr(
                0,
                name,
                base,
                roughness.clamp(0.04, 1.0),
                metallic.clamp(0.0, 1.0),
            );
            if let Some(emissive) = value.get("emissiveFactor").and_then(|v| v.as_array()) {
                let e = Vec3::new(
                    emissive.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    emissive.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    emissive.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                );
                if e.max_element() > 0.0 {
                    if let Some(p) = material
                        .graph
                        .nodes
                        .iter_mut()
                        .find(|n| matches!(n.kind, ShaderNodeKind::Principled))
                    {
                        if let Some(input) = p.inputs.get_mut(5) {
                            *input = ShaderValue::Color(e);
                        }
                        if let Some(input) = p.inputs.get_mut(6) {
                            *input = ShaderValue::Float(1.0);
                        }
                    }
                }
            }
            material.double_sided = value
                .get("doubleSided")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            material_map.insert(i, doc.materials.add(material));
        }
    }

    let mut imported_meshes: Vec<ImportedMesh> = Vec::new();
    if let Some(meshes) = gltf.get("meshes").and_then(|m| m.as_array()) {
        for mesh_value in meshes {
            let mesh_name = mesh_value
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("Mesh");
            let mut mesh = Mesh::new(mesh_name);
            let mut slots: BTreeMap<u16, MaterialId> = BTreeMap::new();
            let mut next_slot = 0u16;
            let primitives = mesh_value
                .get("primitives")
                .and_then(|p| p.as_array())
                .cloned()
                .unwrap_or_default();
            for primitive in primitives {
                if primitive.get("mode").and_then(|m| m.as_u64()).unwrap_or(4) != 4 {
                    continue;
                }
                let attrs = primitive.get("attributes").cloned().unwrap_or_default();
                let Some(pos_index) = attrs.get("POSITION").and_then(|p| p.as_u64()) else {
                    continue;
                };
                let (positions, _) = read_accessor_f32(&gltf, pos_index as usize, &buffers)?;
                if positions.len() < 3 {
                    continue;
                }
                let uvs = attrs
                    .get("TEXCOORD_0")
                    .and_then(|p| p.as_u64())
                    .map(|i| read_accessor_f32(&gltf, i as usize, &buffers))
                    .transpose()?
                    .map(|(v, _)| v)
                    .unwrap_or_default();
                let indices = if let Some(index) = primitive.get("indices").and_then(|i| i.as_u64())
                {
                    read_accessor_u32(&gltf, index as usize, &buffers)?
                } else {
                    (0..(positions.len() / 3) as u32).collect()
                };
                let material = primitive
                    .get("material")
                    .and_then(|m| m.as_u64())
                    .and_then(|i| material_map.get(&(i as usize)).copied())
                    .unwrap_or(0);
                let slot = slots
                    .iter()
                    .find(|(_, m)| **m == material)
                    .map(|(s, _)| *s)
                    .unwrap_or_else(|| {
                        let s = next_slot;
                        next_slot = next_slot.saturating_add(1);
                        slots.insert(s, material);
                        s
                    });
                let point_base = mesh.positions.len() as u32;
                for chunk in positions.chunks_exact(3) {
                    mesh.positions.push(Vec3::new(chunk[0], chunk[1], chunk[2]));
                }
                let has_uv = uvs.len() >= (positions.len() / 3) * 2;
                for tri in indices.chunks(3) {
                    if tri.len() < 3 {
                        continue;
                    }
                    let mut points = [0u32; 3];
                    let mut valid = true;
                    for (k, index) in tri.iter().enumerate() {
                        if *index >= (positions.len() / 3) as u32 {
                            valid = false;
                            break;
                        }
                        points[k] = point_base + *index;
                    }
                    if !valid {
                        continue;
                    }
                    if has_uv {
                        let mut corner_uvs = [Vec2::ZERO; 3];
                        for (k, index) in tri.iter().enumerate() {
                            let uv_index = *index as usize * 2;
                            if uv_index + 1 < uvs.len() {
                                corner_uvs[k] = Vec2::new(uvs[uv_index], uvs[uv_index + 1]);
                            }
                        }
                        mesh.add_face_with_uvs(&points, &corner_uvs);
                    } else {
                        mesh.add_face(&points);
                    }
                    if let Some(poly) = mesh.polygons.last_mut() {
                        poly.material_slot = slot;
                    }
                }
            }
            if !mesh.positions.is_empty() {
                imported_meshes.push(ImportedMesh { mesh, slots });
            }
        }
    }

    let nodes = gltf
        .get("nodes")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();
    let mut created: BTreeMap<usize, NodeId> = BTreeMap::new();
    let mut visited = HashSet::new();
    let mut scene_roots: Vec<usize> = Vec::new();
    let scene_index = gltf.get("scene").and_then(|s| s.as_u64()).unwrap_or(0) as usize;
    if let Some(scene) = gltf
        .get("scenes")
        .and_then(|s| s.as_array())
        .and_then(|s| s.get(scene_index))
    {
        if let Some(roots) = scene.get("nodes").and_then(|n| n.as_array()) {
            scene_roots = roots
                .iter()
                .filter_map(|v| v.as_u64().map(|i| i as usize))
                .collect();
        }
    }
    if scene_roots.is_empty() {
        let mut has_parent = HashSet::new();
        for node in &nodes {
            if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
                for child in children.iter().filter_map(|c| c.as_u64()) {
                    has_parent.insert(child as usize);
                }
            }
        }
        scene_roots = (0..nodes.len())
            .filter(|i| !has_parent.contains(i))
            .collect();
    }
    for root in scene_roots {
        import_node(
            &gltf,
            &nodes,
            &imported_meshes,
            root,
            None,
            &mut doc,
            &mut created,
            &mut visited,
        )?;
    }
    doc.settings.ensure_valid_layers();
    doc.sanitize();
    doc.validate()
        .map_err(|errors| format!("imported glTF is invalid:\n{}", errors.join("\n")))?;
    Ok(doc)
}

fn import_node(
    gltf: &Value,
    nodes: &[Value],
    meshes: &[ImportedMesh],
    index: usize,
    parent: Option<NodeId>,
    doc: &mut Document,
    created: &mut BTreeMap<usize, NodeId>,
    visited: &mut HashSet<usize>,
) -> Result<NodeId, String> {
    if let Some(id) = created.get(&index) {
        return Ok(*id);
    }
    if !visited.insert(index) {
        return Err("glTF node hierarchy contains a cycle".into());
    }
    let Some(value) = nodes.get(index) else {
        return Err(format!("glTF references missing node {index}"));
    };
    let name = value
        .get("name")
        .and_then(|n| n.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("Node {index}"));
    let mesh = value
        .get("mesh")
        .and_then(|m| m.as_u64())
        .and_then(|m| meshes.get(m as usize))
        .cloned();
    let kind = match &mesh {
        Some(imported) => NodeKind::Mesh {
            geometry: GeometrySource::editable(imported.mesh.clone()),
        },
        None => NodeKind::Null,
    };
    let node = doc.create_node(kind, &name);
    let id = node.id;
    doc.scene
        .insert_node(node, parent, None)
        .map_err(|e| format!("failed to import glTF node: {e}"))?;
    if let Some(node) = doc.scene.get_mut(id) {
        node.transform = node_transform(value);
        if let Some(imported) = &mesh {
            node.material_slots = imported.slots.clone();
            node.material = imported.slots.values().next().copied();
        }
    }
    created.insert(index, id);
    if let Some(children) = value.get("children").and_then(|c| c.as_array()) {
        for child in children.iter().filter_map(|c| c.as_u64()) {
            import_node(
                gltf,
                nodes,
                meshes,
                child as usize,
                Some(id),
                doc,
                created,
                visited,
            )?;
        }
    }
    Ok(id)
}

fn node_transform(value: &Value) -> lc_core::Transform {
    if let Some(matrix) = value.get("matrix").and_then(|m| m.as_array()) {
        if matrix.len() == 16 {
            let mut cols = [0.0f32; 16];
            for (i, v) in matrix.iter().take(16).enumerate() {
                cols[i] = v.as_f64().unwrap_or(0.0) as f32;
            }
            let mat = Mat4::from_cols_array(&cols);
            let (scale, rotation, translation) = mat.to_scale_rotation_translation();
            if translation.is_finite() && scale.is_finite() && rotation.is_finite() {
                return lc_core::Transform {
                    translation,
                    rotation,
                    scale,
                };
            }
        }
    }
    let translation = json_vec3(value.get("translation")).unwrap_or(Vec3::ZERO);
    let scale = json_vec3(value.get("scale")).unwrap_or(Vec3::ONE);
    let rotation = value
        .get("rotation")
        .and_then(|r| r.as_array())
        .filter(|r| r.len() >= 4)
        .map(|r| {
            Quat::from_xyzw(
                r[0].as_f64().unwrap_or(0.0) as f32,
                r[1].as_f64().unwrap_or(0.0) as f32,
                r[2].as_f64().unwrap_or(0.0) as f32,
                r[3].as_f64().unwrap_or(1.0) as f32,
            )
            .normalize()
        })
        .unwrap_or(Quat::IDENTITY);
    lc_core::Transform {
        translation: if translation.is_finite() {
            translation
        } else {
            Vec3::ZERO
        },
        rotation,
        scale: if scale.is_finite() { scale } else { Vec3::ONE },
    }
}

fn json_vec3(value: Option<&Value>) -> Option<Vec3> {
    let values = value?.as_array()?;
    if values.len() < 3 {
        return None;
    }
    Some(Vec3::new(
        values[0].as_f64()? as f32,
        values[1].as_f64()? as f32,
        values[2].as_f64()? as f32,
    ))
}

fn read_accessor_f32(
    gltf: &Value,
    index: usize,
    buffers: &[Vec<u8>],
) -> Result<(Vec<f32>, usize), String> {
    let accessors = gltf
        .get("accessors")
        .and_then(|a| a.as_array())
        .ok_or_else(|| "glTF has no accessors".to_string())?;
    let views = gltf
        .get("bufferViews")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "glTF has no bufferViews".to_string())?;
    let accessor = accessors
        .get(index)
        .ok_or_else(|| format!("missing accessor {index}"))?;
    if accessor.get("sparse").is_some() {
        return Err("sparse glTF accessors are not supported".into());
    }
    let view_index = accessor
        .get("bufferView")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "accessor has no bufferView".to_string())? as usize;
    let view = views
        .get(view_index)
        .ok_or_else(|| format!("missing bufferView {view_index}"))?;
    let buffer_index = view.get("buffer").and_then(|b| b.as_u64()).unwrap_or(0) as usize;
    let buffer = buffers
        .get(buffer_index)
        .ok_or_else(|| format!("missing buffer {buffer_index}"))?;
    let component_type = accessor
        .get("componentType")
        .and_then(|c| c.as_u64())
        .unwrap_or(5126);
    let components = match accessor
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("SCALAR")
    {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        "MAT4" => 16,
        _ => return Err("unsupported glTF accessor type".into()),
    };
    let count = accessor.get("count").and_then(|c| c.as_u64()).unwrap_or(0) as usize;
    let comp_size = match component_type {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        _ => return Err(format!("unsupported glTF component type {component_type}")),
    };
    let stride = view
        .get("byteStride")
        .and_then(|s| s.as_u64())
        .map(|s| s as usize)
        .unwrap_or(components * comp_size);
    let view_offset = view.get("byteOffset").and_then(|o| o.as_u64()).unwrap_or(0) as usize;
    let accessor_offset = accessor
        .get("byteOffset")
        .and_then(|o| o.as_u64())
        .unwrap_or(0) as usize;
    let base = view_offset + accessor_offset;
    let needed = count
        .checked_mul(stride)
        .and_then(|n| n.checked_add(base))
        .ok_or_else(|| "glTF accessor size overflow".to_string())?;
    if needed > buffer.len() {
        return Err("glTF accessor reads past the end of its buffer".into());
    }
    let mut out = Vec::with_capacity(count * components);
    for i in 0..count {
        for c in 0..components {
            let offset = base + i * stride + c * comp_size;
            let value = match component_type {
                5126 => f32::from_le_bytes(buffer[offset..offset + 4].try_into().unwrap()),
                5125 => u32::from_le_bytes(buffer[offset..offset + 4].try_into().unwrap()) as f32,
                5123 => u16::from_le_bytes(buffer[offset..offset + 2].try_into().unwrap()) as f32,
                5121 => buffer[offset] as f32,
                5122 => i16::from_le_bytes(buffer[offset..offset + 2].try_into().unwrap()) as f32,
                5120 => buffer[offset] as i8 as f32,
                _ => 0.0,
            };
            out.push(value);
        }
    }
    Ok((out, components))
}

fn read_accessor_u32(gltf: &Value, index: usize, buffers: &[Vec<u8>]) -> Result<Vec<u32>, String> {
    let (values, _) = read_accessor_f32(gltf, index, buffers)?;
    Ok(values.into_iter().map(|v| v.max(0.0) as u32).collect())
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    let mut values = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0u32;
    for byte in text.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            break;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("invalid base64 data".into()),
        } as u32;
        acc = (acc << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            values.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(values)
}

#[allow(dead_code)]
fn _kind(_: NodeKind) {}
