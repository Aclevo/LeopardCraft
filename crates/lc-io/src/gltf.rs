//! glTF 2.0 / GLB import and export.
//!
//! Export preserves the document hierarchy, local transforms and evaluated
//! (modifier-applied) meshes with PBR materials. Import supports indexed
//! triangle primitives, per-corner UVs, node transforms/hierarchies, materials
//! and external or embedded buffer data.

use crate::common::preview_surface;
use glam::{Mat4, Quat, Vec2, Vec3};
use lc_anim::{
    AnimChannel, AnimationClip, AnimationTrack, ChannelAddress, ChannelTarget, Curve,
    Interpolation, TangentType,
};
use lc_geom::Mesh;
use lc_scene::{
    Camera, Document, GeometrySource, Light, LightKind, MorphSet, MorphTarget, NodeId, NodeKind,
    ProjectionMode, Skin,
};
use lc_shading::{Material, MaterialId, ShaderNodeKind, ShaderValue};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

const GLB_MAGIC: u32 = 0x4654_6C67;
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;
const EXPORT_CREASE_DEG: f32 = 40.0;

/// Authored glTF data the importer recognised but chose not to carry across.
/// Skins, animation, morph targets, cameras and punctual lights now import; the
/// one remaining rewrite is camera aspect hints, which LeopardCraft derives
/// from the render output instead.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// glTF camera `aspectRatio` hints; LeopardCraft derives aspect from the
    /// render output, not from the camera.
    pub camera_aspect_ratios: usize,
}

impl ImportReport {
    pub fn is_empty(&self) -> bool {
        self.camera_aspect_ratios == 0
    }

    /// One actionable sentence naming what was rewritten and what the user can
    /// do about it, or `None` when the file imported exactly as authored.
    pub fn summary(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        Some(format!(
            "Ignored {}: LeopardCraft takes aspect from the render output, so set the output size instead.",
            counted(
                self.camera_aspect_ratios,
                "camera aspect ratio",
                "camera aspect ratios"
            )
        ))
    }
}

/// Per-node data resolved before the hierarchy walk, because a node's kind has
/// to be chosen when the node is created.
#[derive(Clone, Default)]
struct NodeExtras {
    camera: Option<Camera>,
    light: Option<Light>,
}

/// Map a glTF camera onto LeopardCraft's camera. A missing optional field falls
/// back to the LeopardCraft default rather than failing the import.
fn import_camera(
    camera: &Value,
    node_index: usize,
    report: &mut ImportReport,
) -> Result<Camera, String> {
    let number = |object: &Value, key: &str| object.get(key).and_then(Value::as_f64);
    if let Some(perspective) = camera.get("perspective") {
        let yfov = number(perspective, "yfov").ok_or_else(|| {
            format!(
                "glTF camera on node {node_index} has no perspective.yfov; export the camera again or remove it"
            )
        })? as f32;
        if !yfov.is_finite() || yfov <= 0.0 || yfov >= std::f32::consts::PI {
            return Err(format!(
                "glTF camera on node {node_index} has a vertical field of view of {yfov} rad, which must be between 0 and 180 degrees; fix the camera angle in the source file"
            ));
        }
        if perspective.get("aspectRatio").is_some() {
            report.camera_aspect_ratios += 1;
        }
        let near = number(perspective, "znear").unwrap_or(0.05) as f32;
        let far = number(perspective, "zfar").unwrap_or(1000.0) as f32;
        let mut imported = Camera::default();
        imported.fov_y_deg = yfov.to_degrees();
        imported.near = near.max(0.0001);
        imported.far = far.max(imported.near + 0.001);
        return Ok(imported);
    }
    if let Some(ortho) = camera.get("orthographic") {
        let ymag = number(ortho, "ymag").ok_or_else(|| {
            format!(
                "glTF camera on node {node_index} has no orthographic.ymag; export the camera again or remove it"
            )
        })? as f32;
        if !ymag.is_finite() || ymag <= 0.0 {
            return Err(format!(
                "glTF camera on node {node_index} has an orthographic half-height of {ymag}; it must be greater than zero"
            ));
        }
        let near = number(ortho, "znear").unwrap_or(0.05) as f32;
        let far = number(ortho, "zfar").unwrap_or(1000.0) as f32;
        let mut imported = Camera::default();
        imported.projection = ProjectionMode::Orthographic;
        imported.ortho_scale = ymag * 2.0;
        imported.near = near.max(0.0001);
        imported.far = far.max(imported.near + 0.001);
        return Ok(imported);
    }
    let declared = camera
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("no type");
    Err(format!(
        "glTF camera on node {node_index} declares {declared:?} but has no perspective or orthographic block; fix the camera in the source file or remove it"
    ))
}

/// glTF `range` is a distance cutoff; LeopardCraft's point and spot lights carry
/// a shadow/falloff radius instead, so the authored distance is kept there. glTF
/// omits `range` for an unlimited light, which maps to our 1 m default.
fn import_light_range(light: &Value) -> f32 {
    light
        .get("range")
        .and_then(Value::as_f64)
        .map(|range| range as f32)
        .filter(|range| range.is_finite() && *range > 0.0)
        .unwrap_or(1.0)
}

/// Map a `KHR_lights_punctual` light. glTF cone angles are radians and our spot
/// light stores degrees, so the import converts them.
fn import_light(light: &Value, node_index: usize) -> Result<Light, String> {
    let kind = match light.get("type").and_then(Value::as_str) {
        Some("directional") => LightKind::Directional,
        Some("point") => LightKind::Point {
            radius: import_light_range(light),
        },
        Some("spot") => {
            let spot = light.get("spot");
            let angle = |key: &str, fallback: f32| {
                spot.and_then(|spot| spot.get(key))
                    .and_then(Value::as_f64)
                    .map(|value| value as f32)
                    .unwrap_or(fallback)
            };
            let inner = angle("innerConeAngle", 0.0);
            let outer = angle("outerConeAngle", std::f32::consts::FRAC_PI_4);
            let limit = std::f32::consts::FRAC_PI_2;
            if !inner.is_finite()
                || !outer.is_finite()
                || inner < 0.0
                || outer <= 0.0
                || outer > limit + 1e-6
                || inner > outer + 1e-6
            {
                return Err(format!(
                    "glTF spot light on node {node_index} has cone angles of {inner} rad inner and {outer} rad outer; KHR_lights_punctual requires 0 <= inner <= outer <= 90 degrees (1.5708 rad), so fix the cone in the source file"
                ));
            }
            LightKind::Spot {
                radius: import_light_range(light),
                inner_deg: inner.to_degrees(),
                outer_deg: outer.to_degrees(),
            }
        }
        Some(other) => {
            return Err(format!(
                "glTF light type {other:?} is not part of KHR_lights_punctual (directional, point or spot); fix the light in the source file"
            ))
        }
        None => {
            return Err(
                "glTF light has no type; KHR_lights_punctual requires directional, point or spot"
                    .into(),
            )
        }
    };
    let color = light
        .get("color")
        .and_then(Value::as_array)
        .filter(|color| color.len() >= 3)
        .map(|color| {
            Vec3::new(
                color[0].as_f64().unwrap_or(1.0) as f32,
                color[1].as_f64().unwrap_or(1.0) as f32,
                color[2].as_f64().unwrap_or(1.0) as f32,
            )
        })
        .unwrap_or(Vec3::ONE);
    let intensity = light
        .get("intensity")
        .and_then(Value::as_f64)
        .unwrap_or(1.0) as f32;
    Ok(Light {
        kind,
        color: if color.is_finite() { color } else { Vec3::ONE },
        intensity: if intensity.is_finite() && intensity > 0.0 {
            intensity
        } else {
            1.0
        },
        cast_shadows: true,
    })
}

fn counted(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}

/// Animation channels an export cannot place. glTF animation targets a node
/// TRS path, so a node clip that addresses another object, a tag or a material
/// has no destination. Those channels are skipped and named instead of being
/// written onto the wrong node.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnimationExportReport {
    pub skipped_channels: Vec<String>,
}

impl AnimationExportReport {
    pub fn is_empty(&self) -> bool {
        self.skipped_channels.is_empty()
    }

    /// One sentence naming the channels and why they were omitted.
    pub fn summary(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        Some(format!(
            "Skipped {} animation channel(s) with no glTF target path: {}.",
            self.skipped_channels.len(),
            self.skipped_channels.join(", ")
        ))
    }
}

/// Whether a node's clip can export this address as node TRS animation.
fn exports_to_node(address: ChannelAddress, owner: ChannelTarget) -> bool {
    address.scope() == lc_anim::ChannelScope::Object && address.targets(owner)
}

/// Animation channels `doc` would lose on export.
pub fn animation_export_report(doc: &Document) -> AnimationExportReport {
    let mut report = AnimationExportReport::default();
    for id in doc.scene.depth_first() {
        let Some(node) = doc.scene.get(id) else {
            continue;
        };
        let owner = ChannelTarget::Object { id };
        for track in &node.animation.tracks {
            if !exports_to_node(track.address, owner) {
                report
                    .skipped_channels
                    .push(format!("{}: {}", node.name, track.address.label()));
            }
        }
        if let Some(rotation) = &node.animation.rotation {
            if !rotation.applies_to(owner) {
                report
                    .skipped_channels
                    .push(format!("{}: Rotation", node.name));
            }
        }
    }
    report
}

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
    crate::common::write_atomic(path.as_ref(), &out)
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
    crate::common::write_atomic(path.as_ref(), text.as_bytes())
}

pub fn import_glb(path: impl AsRef<Path>) -> Result<Document, String> {
    import_glb_with_report(path).map(|(doc, _)| doc)
}

pub fn import_gltf(path: impl AsRef<Path>) -> Result<Document, String> {
    import_gltf_with_report(path).map(|(doc, _)| doc)
}

/// Import a GLB and report the authored features the importer had to skip, so
/// callers can warn the user instead of presenting a lossy import as complete.
pub fn import_glb_with_report(path: impl AsRef<Path>) -> Result<(Document, ImportReport), String> {
    let path = path.as_ref();
    let bytes =
        std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let (gltf, bin) = parse_glb(&bytes)?;
    import_value_with_report(
        gltf,
        bin.as_deref(),
        path.parent().unwrap_or(Path::new(".")),
    )
}

pub fn import_gltf_with_report(path: impl AsRef<Path>) -> Result<(Document, ImportReport), String> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let gltf: Value = serde_json::from_str(&text).map_err(|e| format!("invalid glTF JSON: {e}"))?;
    let buffers = load_buffers(&gltf, None, path.parent().unwrap_or(Path::new(".")))?;
    import_value_with_buffers_and_report(gltf, buffers)
}

#[derive(Default)]
struct GltfBuild {
    binary: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl GltfBuild {
    fn align(&mut self) {
        while !self.binary.len().is_multiple_of(4) {
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
        self.add_f32_values(values, ty, components, Some(target), bounds)
    }

    fn add_animation_input(&mut self, times: &[f32]) -> usize {
        self.add_f32_values(times, "SCALAR", 1, None, true)
    }

    fn add_f32_values(
        &mut self,
        values: &[f32],
        ty: &str,
        components: usize,
        target: Option<u32>,
        bounds: bool,
    ) -> usize {
        self.align();
        let view_index = self.views.len();
        let byte_offset = self.binary.len();
        for value in values {
            self.binary.extend_from_slice(&value.to_le_bytes());
        }
        let mut view = json!({
            "buffer": 0,
            "byteOffset": byte_offset,
            "byteLength": values.len() * 4,
        });
        if let Some(target) = target {
            view["target"] = json!(target);
        }
        self.views.push(view);
        let accessor_index = self.accessors.len();
        let count = values.len().checked_div(components).unwrap_or(0);
        let mut accessor = json!({
            "bufferView": view_index,
            "byteOffset": 0,
            "componentType": 5126,
            "count": count,
            "type": ty,
        });
        if bounds && components > 0 && !values.is_empty() {
            let mut min = vec![f32::INFINITY; components];
            let mut max = vec![f32::NEG_INFINITY; components];
            for chunk in values.chunks(components) {
                for i in 0..components {
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

    fn add_u16_vec4(&mut self, values: &[[u16; 4]], target: u32) -> usize {
        self.align();
        let view_index = self.views.len();
        let byte_offset = self.binary.len();
        for value in values {
            for component in value {
                self.binary.extend_from_slice(&component.to_le_bytes());
            }
        }
        self.views.push(json!({
            "buffer": 0,
            "byteOffset": byte_offset,
            "byteLength": values.len() * 8,
            "target": target,
        }));
        let accessor_index = self.accessors.len();
        self.accessors.push(json!({
            "bufferView": view_index,
            "byteOffset": 0,
            "componentType": 5123,
            "count": values.len(),
            "type": "VEC4",
        }));
        accessor_index
    }

    fn add_mat4(&mut self, matrices: &[Mat4]) -> usize {
        self.align();
        let view_index = self.views.len();
        let byte_offset = self.binary.len();
        for matrix in matrices {
            for value in matrix.to_cols_array() {
                self.binary.extend_from_slice(&value.to_le_bytes());
            }
        }
        self.views.push(json!({
            "buffer": 0,
            "byteOffset": byte_offset,
            "byteLength": matrices.len() * 64,
        }));
        let accessor_index = self.accessors.len();
        self.accessors.push(json!({
            "bufferView": view_index,
            "byteOffset": 0,
            "componentType": 5126,
            "count": matrices.len(),
            "type": "MAT4",
        }));
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
    // Name animation channels the format cannot place instead of silently
    // writing them onto the wrong node.
    if let Some(summary) = animation_export_report(doc).summary() {
        eprintln!("warning: {summary}");
    }
    let evaluated = doc.evaluate(0.0);
    let order = doc.scene.depth_first();
    let mut index_of_node: HashMap<NodeId, usize> = HashMap::new();
    for (i, id) in order.iter().enumerate() {
        index_of_node.insert(*id, i);
    }
    // glTF stores the bind mesh, not the evaluated deformation. Record the
    // absolute bind world for every joint first so joint nodes can be exported
    // in the pose their inverse-bind matrices expect.
    let mut joint_bind_world: HashMap<NodeId, Mat4> = HashMap::new();
    for node in doc.scene.nodes.values() {
        let Some(skin) = &node.skin else {
            continue;
        };
        if skin.joints.is_empty()
            || skin.joints.len() > u16::MAX as usize
            || skin.inverse_bind.len() != skin.joints.len()
            || !skin
                .joints
                .iter()
                .all(|joint| doc.scene.nodes.contains_key(joint))
            || !skin.inverse_bind.iter().all(affine_bind_matrix)
            || !joints_share_common_root(doc, &skin.joints)
        {
            continue;
        }
        for (i, joint) in skin.joints.iter().enumerate() {
            if !doc.scene.nodes.contains_key(joint) {
                continue;
            }
            let Some(inverse) = skin.inverse_bind.get(i).copied() else {
                continue;
            };
            if !affine_bind_matrix(&inverse) || inverse.determinant().abs() <= 1e-8 {
                continue;
            }
            let world = inverse.inverse();
            if world.is_finite() {
                joint_bind_world.entry(*joint).or_insert(world);
            }
        }
    }
    let mut bind_doc = doc.clone();
    for node in bind_doc.scene.nodes.values_mut() {
        if let NodeKind::Mesh {
            geometry: GeometrySource::Editable { mesh, .. },
        } = &mut node.kind
        {
            if node.morph.has_base() && node.morph.base.len() == mesh.positions.len() {
                mesh.positions = node.morph.base.clone();
            }
        }
        node.skin = None;
        node.morph = MorphSet::default();
        node.animation.morph_weights.clear();
    }
    let bind = bind_doc.evaluate(0.0);
    let mut export_locals: HashMap<NodeId, lc_core::Transform> = HashMap::new();
    let mut export_worlds: HashMap<NodeId, Mat4> = HashMap::new();
    for id in &order {
        let Some(raw) = doc.scene.get(*id) else {
            continue;
        };
        let parent_world = raw
            .parent
            .and_then(|parent| export_worlds.get(&parent).copied())
            .unwrap_or(Mat4::IDENTITY);
        let evaluated_local = evaluated
            .node(*id)
            .map(|node| node.local)
            .unwrap_or(raw.transform);
        let local = if let Some(bind_world) = joint_bind_world.get(id).copied() {
            let from_bind = parent_world.inverse() * bind_world;
            if from_bind.is_finite() {
                let (scale, rotation, translation) = from_bind.to_scale_rotation_translation();
                lc_core::Transform {
                    translation,
                    rotation,
                    scale,
                }
            } else {
                evaluated_local
            }
        } else {
            evaluated_local
        };
        export_worlds.insert(*id, parent_world * local.matrix());
        export_locals.insert(*id, local);
    }
    let mut builder = GltfBuild::default();
    let mut nodes_json: Vec<Value> = Vec::with_capacity(order.len());
    let mut meshes_json: Vec<Value> = Vec::new();
    let mut skins_json: Vec<Value> = Vec::new();
    let mut morph_target_count: HashMap<NodeId, usize> = HashMap::new();
    let mut materials_json: Vec<Value> = Vec::new();
    let mut material_indices: BTreeMap<MaterialId, usize> = BTreeMap::new();

    for id in &order {
        let Some(raw) = doc.scene.get(*id) else {
            continue;
        };
        let Some(node) = bind.node(*id) else {
            continue;
        };
        let local = export_locals.get(id).copied().unwrap_or(raw.transform);
        let mut node_json = Map::new();
        node_json.insert("name".into(), Value::String(raw.name.clone()));
        node_json.insert(
            "translation".into(),
            json!([
                local.translation.x,
                local.translation.y,
                local.translation.z
            ]),
        );
        node_json.insert(
            "rotation".into(),
            json!([
                local.rotation.x,
                local.rotation.y,
                local.rotation.z,
                local.rotation.w
            ]),
        );
        node_json.insert(
            "scale".into(),
            json!([local.scale.x, local.scale.y, local.scale.z]),
        );
        let children: Vec<usize> = raw
            .children
            .iter()
            .filter_map(|child| index_of_node.get(child).copied())
            .collect();
        if !children.is_empty() {
            node_json.insert("children".into(), json!(children));
        }

        if let Some(mesh) = node.mesh() {
            if !mesh.is_empty() {
                let skin_export = raw.skin.as_ref().and_then(|skin| {
                    (skin.joints.len() <= u16::MAX as usize
                        && skin.inverse_bind.len() == skin.joints.len()
                        && skin.joints.iter().all(|joint| {
                            index_of_node.contains_key(joint)
                                && joint_bind_world.contains_key(joint)
                        })
                        && joints_share_common_root(doc, &skin.joints)
                        && skin.inverse_bind.iter().all(affine_bind_matrix))
                    .then(|| normalized_skin(skin, mesh.positions.len()))
                    .flatten()
                });
                if let (Some(_), Some(skin)) = (skin_export.as_ref(), raw.skin.as_ref()) {
                    let joints: Vec<usize> = skin
                        .joints
                        .iter()
                        .filter_map(|joint| index_of_node.get(joint).copied())
                        .collect();
                    // Stored inverse binds are relative to world space. glTF
                    // expects mesh-local bind space, so include the mesh node's
                    // bind world when the mesh is not at the origin.
                    let inverse_bind: Vec<Mat4> = skin
                        .inverse_bind
                        .iter()
                        .map(|inverse| *inverse * node.world)
                        .collect();
                    let inverse_bind_accessor = builder.add_mat4(&inverse_bind);
                    let skin_index = skins_json.len();
                    skins_json.push(json!({
                        "name": format!("{} Skin", raw.name),
                        "joints": joints,
                        "inverseBindMatrices": inverse_bind_accessor,
                    }));
                    node_json.insert("skin".into(), json!(skin_index));
                }

                let morph_ready = morph_export_ready(raw, mesh);
                let target_normals: Vec<Vec<Vec<Vec3>>> = if morph_ready {
                    raw.morph
                        .targets
                        .iter()
                        .map(|target| target_corner_normals(mesh, target))
                        .collect()
                } else {
                    Vec::new()
                };
                if morph_ready {
                    morph_target_count.insert(*id, raw.morph.targets.len());
                    let weights: Vec<f32> = raw
                        .morph
                        .targets
                        .iter()
                        .map(|target| {
                            if target.weight.is_finite() {
                                target.weight.clamp(0.0, 1.0)
                            } else {
                                0.0
                            }
                        })
                        .collect();
                    node_json.insert("weights".into(), json!(weights));
                }
                let mut groups: BTreeMap<MaterialId, Vec<lc_geom::RenderTriangle>> =
                    BTreeMap::new();
                for tri in mesh.render_triangles(EXPORT_CREASE_DEG) {
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
                    let mut extra_uvs: Vec<Vec<f32>> = (0..mesh.extra_uvs.len())
                        .map(|_| Vec::with_capacity(tris.len() * 6))
                        .collect();
                    let mut indices = Vec::with_capacity(tris.len() * 3);
                    let mut joints = Vec::new();
                    let mut weights = Vec::new();
                    let mut target_positions: Vec<Vec<f32>> = if morph_ready {
                        vec![Vec::with_capacity(tris.len() * 9); raw.morph.targets.len()]
                    } else {
                        Vec::new()
                    };
                    let mut target_normals_out: Vec<Vec<f32>> = if morph_ready {
                        vec![Vec::with_capacity(tris.len() * 9); raw.morph.targets.len()]
                    } else {
                        Vec::new()
                    };
                    for tri in &tris {
                        for v in &tri.v {
                            positions.extend_from_slice(&[
                                v.position.x,
                                v.position.y,
                                v.position.z,
                            ]);
                            normals.extend_from_slice(&[v.normal.x, v.normal.y, v.normal.z]);
                            uvs.extend_from_slice(&[v.uv.x, v.uv.y]);
                            let corner_slot = mesh
                                .polygons
                                .get(tri.polygon as usize)
                                .and_then(|poly| poly.corners.get(v.corner as usize))
                                .map(|corner| corner.uv);
                            for (set, values) in extra_uvs.iter_mut().enumerate() {
                                let uv = corner_slot
                                    .and_then(|slot| mesh.uv_value(set + 1, slot))
                                    .unwrap_or(Vec2::ZERO);
                                values.extend_from_slice(&[uv.x, uv.y]);
                            }
                            if let Some(skin) = &skin_export {
                                let point = v.point as usize;
                                joints.push(skin.joints[point]);
                                weights.extend_from_slice(&skin.weights[point]);
                            }
                            if morph_ready {
                                for (target_index, target) in raw.morph.targets.iter().enumerate() {
                                    let delta = target
                                        .deltas
                                        .get(v.point as usize)
                                        .copied()
                                        .unwrap_or(Vec3::ZERO);
                                    target_positions[target_index]
                                        .extend_from_slice(&[delta.x, delta.y, delta.z]);
                                    let normal = target_normals
                                        .get(target_index)
                                        .and_then(|normals| normals.get(tri.polygon as usize))
                                        .and_then(|corners| corners.get(v.corner as usize))
                                        .copied()
                                        .unwrap_or(v.normal);
                                    let normal_delta = normal - v.normal;
                                    target_normals_out[target_index].extend_from_slice(&[
                                        normal_delta.x,
                                        normal_delta.y,
                                        normal_delta.z,
                                    ]);
                                }
                            }
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
                    let mut attributes = json!({
                        "POSITION": pos_accessor,
                        "NORMAL": normal_accessor,
                        "TEXCOORD_0": uv_accessor,
                    });
                    for (set, values) in extra_uvs.iter().enumerate() {
                        let accessor = builder.add_f32(values, "VEC2", 2, 34962, false);
                        attributes[format!("TEXCOORD_{}", set + 1)] = json!(accessor);
                    }
                    if skin_export.is_some() && !joints.is_empty() {
                        let joint_accessor = builder.add_u16_vec4(&joints, 34962);
                        let weight_accessor = builder.add_f32(&weights, "VEC4", 4, 34962, false);
                        attributes["JOINTS_0"] = json!(joint_accessor);
                        attributes["WEIGHTS_0"] = json!(weight_accessor);
                    }
                    let mut targets = Vec::new();
                    if morph_ready {
                        for target_index in 0..raw.morph.targets.len() {
                            let position_accessor = builder.add_f32(
                                &target_positions[target_index],
                                "VEC3",
                                3,
                                34962,
                                true,
                            );
                            let normal_accessor = builder.add_f32(
                                &target_normals_out[target_index],
                                "VEC3",
                                3,
                                34962,
                                false,
                            );
                            targets.push(json!({
                                "POSITION": position_accessor,
                                "NORMAL": normal_accessor,
                            }));
                        }
                    }
                    let material_index =
                        material_json(doc, material_id, &mut materials_json, &mut material_indices);
                    let mut primitive = json!({
                        "attributes": attributes,
                        "indices": index_accessor,
                        "material": material_index,
                        "mode": 4,
                    });
                    if !targets.is_empty() {
                        primitive["targets"] = json!(targets);
                    }
                    primitives.push(primitive);
                }
                if primitives.is_empty() {
                    node_json.remove("skin");
                    node_json.remove("weights");
                    morph_target_count.remove(id);
                }
                if !primitives.is_empty() {
                    let mesh_index = meshes_json.len();
                    let mut mesh_json = json!({
                        "name": raw.name,
                        "primitives": primitives,
                    });
                    if morph_ready {
                        let names: Vec<String> = raw
                            .morph
                            .targets
                            .iter()
                            .enumerate()
                            .map(|(index, target)| {
                                if target.name.trim().is_empty() {
                                    format!("Morph {}", index + 1)
                                } else {
                                    target.name.clone()
                                }
                            })
                            .collect();
                        mesh_json["extras"] = json!({ "targetNames": names });
                    }
                    meshes_json.push(mesh_json);
                    node_json.insert("mesh".into(), json!(mesh_index));
                }
            }
        }
        nodes_json.push(Value::Object(node_json));
    }

    let mut animations_json: Vec<Value> = Vec::new();
    let times = take_times(doc);
    let take_name = doc
        .active_take()
        .map(|take| take.name.clone())
        .unwrap_or_else(|| "Take".into());
    let take_fps = doc.active_take_fps();
    if !times.is_empty() {
        let mut samplers_json: Vec<Value> = Vec::new();
        let mut channels_json: Vec<Value> = Vec::new();
        let mut input_accessor: Option<usize> = None;
        for id in &order {
            let Some(raw) = doc.scene.get(*id) else {
                continue;
            };
            let clip = &raw.animation;
            if clip.is_empty() {
                continue;
            }
            let Some(node_index) = index_of_node.get(id).copied() else {
                continue;
            };
            let base = export_locals.get(id).copied().unwrap_or(raw.transform);
            // Address the node explicitly, falling back to the clip's owner,
            // so tracks recorded against an explicit object target export too.
            let owner = ChannelTarget::Object { id: *id };
            let translation_curves = [0u8, 1, 2].map(|component| {
                clip.track_for(owner, AnimChannel::Translation, component)
                    .map(|track| &track.curve)
            });
            let scale_curves = [0u8, 1, 2].map(|component| {
                clip.track_for(owner, AnimChannel::Scale, component)
                    .map(|track| &track.curve)
            });
            let has_rotation = clip
                .rotation
                .as_ref()
                .is_some_and(|rotation| !rotation.is_empty() && rotation.applies_to(owner))
                || (0..3).any(|axis| clip.legacy_euler_curve(axis).is_some());
            let target_count = morph_target_count.get(id).copied().unwrap_or(0);
            let morph_curves: Vec<Option<&Curve>> = (0..target_count)
                .map(|target| clip.morph_weights.get(&(target as u32)))
                .collect();
            let has_morph = morph_curves.iter().any(Option::is_some);
            let has_translation = translation_curves.iter().any(Option::is_some);
            let has_scale = scale_curves.iter().any(Option::is_some);
            if !(has_translation || has_rotation || has_scale || has_morph) {
                continue;
            }
            if input_accessor.is_none() {
                input_accessor = Some(builder.add_animation_input(&times));
            }
            let input = input_accessor.unwrap();

            if has_translation {
                let present: Vec<&Curve> = translation_curves.iter().flatten().copied().collect();
                let interpolation = sampled_scalar_interpolation(&present, times.len());
                let values = sampled_vector(
                    translation_curves,
                    base.translation,
                    &times,
                    interpolation == "CUBICSPLINE",
                );
                let output = builder.add_f32_values(&values, "VEC3", 3, None, false);
                let sampler = samplers_json.len();
                samplers_json.push(json!({
                    "input": input,
                    "output": output,
                    "interpolation": interpolation,
                }));
                channels_json.push(json!({
                    "sampler": sampler,
                    "target": { "node": node_index, "path": "translation" },
                }));
            }
            if has_rotation {
                let interpolation = rotation_interpolation(clip);
                let mut values = Vec::with_capacity(times.len() * 4);
                for &time in &times {
                    let rotation = clip.apply_transform(base, time).rotation;
                    values.extend_from_slice(&[rotation.x, rotation.y, rotation.z, rotation.w]);
                }
                let output = builder.add_f32_values(&values, "VEC4", 4, None, false);
                let sampler = samplers_json.len();
                samplers_json.push(json!({
                    "input": input,
                    "output": output,
                    "interpolation": interpolation,
                }));
                channels_json.push(json!({
                    "sampler": sampler,
                    "target": { "node": node_index, "path": "rotation" },
                }));
            }
            if has_scale {
                let present: Vec<&Curve> = scale_curves.iter().flatten().copied().collect();
                let interpolation = sampled_scalar_interpolation(&present, times.len());
                let values = sampled_vector(
                    scale_curves,
                    base.scale,
                    &times,
                    interpolation == "CUBICSPLINE",
                );
                let output = builder.add_f32_values(&values, "VEC3", 3, None, false);
                let sampler = samplers_json.len();
                samplers_json.push(json!({
                    "input": input,
                    "output": output,
                    "interpolation": interpolation,
                }));
                channels_json.push(json!({
                    "sampler": sampler,
                    "target": { "node": node_index, "path": "scale" },
                }));
            }
            if has_morph {
                let present: Vec<&Curve> = morph_curves.iter().flatten().copied().collect();
                let interpolation = sampled_scalar_interpolation(&present, times.len());
                let cubic = interpolation == "CUBICSPLINE";
                let mut values =
                    Vec::with_capacity(times.len() * target_count * if cubic { 3 } else { 1 });
                for &time in &times {
                    if cubic {
                        for curve in &morph_curves {
                            values
                                .push(curve.map(|curve| curve_tangent(curve, time)).unwrap_or(0.0));
                        }
                    }
                    for (target, curve) in morph_curves.iter().enumerate() {
                        let fallback = raw
                            .morph
                            .targets
                            .get(target)
                            .map(|target| target.weight)
                            .unwrap_or(0.0);
                        values.push(
                            curve
                                .map(|curve| curve.sample(time))
                                .unwrap_or(fallback)
                                .clamp(0.0, 1.0),
                        );
                    }
                    if cubic {
                        for curve in &morph_curves {
                            values
                                .push(curve.map(|curve| curve_tangent(curve, time)).unwrap_or(0.0));
                        }
                    }
                }
                let output = builder.add_f32_values(&values, "SCALAR", 1, None, false);
                let sampler = samplers_json.len();
                samplers_json.push(json!({
                    "input": input,
                    "output": output,
                    "interpolation": interpolation,
                }));
                channels_json.push(json!({
                    "sampler": sampler,
                    "target": { "node": node_index, "path": "weights" },
                }));
            }
        }
        if !channels_json.is_empty() {
            animations_json.push(json!({
                "name": take_name.clone(),
                "extras": {
                    "leopardcraft": {
                        "take": take_name.clone(),
                        "fps": take_fps,
                    }
                },
                "samplers": samplers_json,
                "channels": channels_json,
            }));
        }
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
    if !skins_json.is_empty() {
        root["skins"] = json!(skins_json);
    }
    if !animations_json.is_empty() {
        root["animations"] = json!(animations_json);
        root["extras"] = json!({
            "leopardcraft": {
                "active_take": take_name,
                "fps": take_fps,
            }
        });
    }
    Ok((root, builder.binary))
}

struct SkinExport {
    joints: Vec<[u16; 4]>,
    weights: Vec<[f32; 4]>,
}

fn affine_bind_matrix(matrix: &Mat4) -> bool {
    if !matrix.is_finite() {
        return false;
    }
    let row = matrix.row(3);
    row.x.abs() <= 1e-6 && row.y.abs() <= 1e-6 && row.z.abs() <= 1e-6 && (row.w - 1.0).abs() <= 1e-6
}

/// glTF requires every joint of a skin to share a common ancestor. A rig whose
/// joints live in disconnected branches cannot be represented as one skin.
fn joints_share_common_root(doc: &Document, joints: &[NodeId]) -> bool {
    let mut common: Option<HashSet<NodeId>> = None;
    for joint in joints {
        let mut ancestors = HashSet::new();
        let mut current = Some(*joint);
        while let Some(id) = current {
            if !ancestors.insert(id) {
                break;
            }
            current = doc.scene.get(id).and_then(|node| node.parent);
        }
        common = Some(match common {
            Some(previous) => previous.intersection(&ancestors).copied().collect(),
            None => ancestors,
        });
        if common.as_ref().is_some_and(HashSet::is_empty) {
            return false;
        }
    }
    common.is_some()
}

/// Normalise a skin to glTF's four-influence, unit-weight representation.
/// Returns `None` when any vertex has no usable influence, so an incomplete
/// skin is skipped instead of exporting a corrupt one.
fn normalized_skin(skin: &Skin, point_count: usize) -> Option<SkinExport> {
    if skin.joints.is_empty()
        || skin.joints.len() > u16::MAX as usize
        || skin.inverse_bind.len() != skin.joints.len()
        || skin.joint_indices.len() != point_count
        || skin.weights.len() != point_count
    {
        return None;
    }
    let mut joints = Vec::with_capacity(point_count);
    let mut weights = Vec::with_capacity(point_count);
    for point in 0..point_count {
        let mut j = [0u16; 4];
        let mut w = [0.0f32; 4];
        let mut total = 0.0f32;
        for influence in 0..4 {
            let index = skin.joint_indices[point][influence];
            let weight = skin.weights[point][influence];
            if (index as usize) < skin.joints.len() && weight.is_finite() && weight > 0.0 {
                j[influence] = index as u16;
                w[influence] = weight;
                total += weight;
            }
        }
        if !total.is_finite() || total <= 1e-8 {
            return None;
        }
        for weight in &mut w {
            *weight /= total;
        }
        joints.push(j);
        weights.push(w);
    }
    Some(SkinExport { joints, weights })
}

fn morph_export_ready(node: &lc_scene::Node, mesh: &Mesh) -> bool {
    node.modifiers.is_empty()
        && !node.morph.targets.is_empty()
        && node.morph.has_base()
        && node.morph.base.len() == mesh.positions.len()
        && node.morph.base.iter().all(|position| position.is_finite())
        && node
            .morph
            .base
            .iter()
            .zip(&mesh.positions)
            .all(|(base, position)| (*base - *position).length() <= 1e-5)
        && node.morph.targets.iter().all(|target| {
            target.deltas.len() == mesh.positions.len()
                && target.deltas.iter().all(|delta| delta.is_finite())
                && target
                    .deltas
                    .iter()
                    .zip(&node.morph.base)
                    .all(|(delta, base)| (*base + *delta).is_finite())
        })
}

fn target_corner_normals(mesh: &Mesh, target: &MorphTarget) -> Vec<Vec<Vec3>> {
    let mut target_mesh = mesh.clone();
    target_mesh.positions = mesh
        .positions
        .iter()
        .zip(&target.deltas)
        .map(|(position, delta)| *position + *delta)
        .collect();
    target_mesh.corner_normals(EXPORT_CREASE_DEG)
}

fn scalar_interpolation(curves: &[&Curve]) -> &'static str {
    let (mut step, mut linear, mut cubic) = (true, true, true);
    let mut any = false;
    for curve in curves {
        for key in &curve.keys {
            any = true;
            step &= key.interpolation == Interpolation::Constant;
            linear &= key.interpolation == Interpolation::Linear;
            cubic &= matches!(
                key.interpolation,
                Interpolation::Smooth | Interpolation::Spline
            );
        }
    }
    if !any {
        "LINEAR"
    } else if step {
        "STEP"
    } else if linear {
        "LINEAR"
    } else if cubic {
        "CUBICSPLINE"
    } else {
        "LINEAR"
    }
}

/// CUBICSPLINE requires at least two keyframes. A single sampled time uses
/// LINEAR so an isolated key still exports a valid sampler.
fn sampled_scalar_interpolation(curves: &[&Curve], time_count: usize) -> &'static str {
    if time_count < 2 {
        "LINEAR"
    } else {
        scalar_interpolation(curves)
    }
}

/// Tangents for a sampled CUBICSPLINE channel. A central difference matches
/// the curve locally; the endpoints use the available one-sided span.
fn curve_tangent(curve: &Curve, time: f32) -> f32 {
    let h = 1e-3;
    let before_time = (time - h).max(0.0);
    let after_time = time + h;
    let span = after_time - before_time;
    if span <= 1e-9 {
        return 0.0;
    }
    (curve.sample(after_time) - curve.sample(before_time)) / span
}

fn take_times(doc: &Document) -> Vec<f32> {
    let mut times: Vec<f32> = doc
        .scene
        .nodes
        .values()
        .filter(|node| !node.animation.is_empty())
        .flat_map(|node| node.animation.key_times())
        .filter(|time| time.is_finite() && *time >= 0.0)
        .collect();
    if times.is_empty() {
        return times;
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-5);
    let start = times[0].max(0.0);
    let end = *times.last().unwrap_or(&start);
    let fps = doc.active_take_fps();
    let frames = (((end - start) * fps).ceil() as usize).min(100_000);
    for frame in 0..=frames {
        times.push(start + frame as f32 / fps);
    }
    times.push(end);
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    times
}

fn sampled_vector(curves: [Option<&Curve>; 3], base: Vec3, times: &[f32], cubic: bool) -> Vec<f32> {
    let mut values = Vec::with_capacity(times.len() * 3 * if cubic { 3 } else { 1 });
    let base = [base.x, base.y, base.z];
    for &time in times {
        let tangents = [
            curves[0]
                .map(|curve| curve_tangent(curve, time))
                .unwrap_or(0.0),
            curves[1]
                .map(|curve| curve_tangent(curve, time))
                .unwrap_or(0.0),
            curves[2]
                .map(|curve| curve_tangent(curve, time))
                .unwrap_or(0.0),
        ];
        if cubic {
            values.extend_from_slice(&tangents);
        }
        for (axis, curve) in curves.iter().enumerate() {
            values.push(curve.map(|curve| curve.sample(time)).unwrap_or(base[axis]));
        }
        if cubic {
            values.extend_from_slice(&tangents);
        }
    }
    values
}

fn rotation_interpolation(clip: &AnimationClip) -> &'static str {
    let mut any = false;
    let mut step = true;
    if let Some(rotation) = &clip.rotation {
        for key in &rotation.keys {
            any = true;
            step &= key.interpolation == Interpolation::Constant;
        }
    }
    for axis in 0..3 {
        if let Some(curve) = clip.legacy_euler_curve(axis) {
            for key in &curve.keys {
                any = true;
                step &= key.interpolation == Interpolation::Constant;
            }
        }
    }
    if any && step {
        "STEP"
    } else {
        "LINEAR"
    }
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
            surface.emission.x.clamp(0.0, 1.0),
            surface.emission.y.clamp(0.0, 1.0),
            surface.emission.z.clamp(0.0, 1.0),
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

fn import_value_with_report(
    gltf: Value,
    embedded: Option<&[u8]>,
    base_dir: &Path,
) -> Result<(Document, ImportReport), String> {
    let buffers = load_buffers(&gltf, embedded, base_dir)?;
    import_value_with_buffers_and_report(gltf, buffers)
}

#[derive(Clone)]
struct ImportedMesh {
    mesh: Mesh,
    slots: BTreeMap<u16, MaterialId>,
    /// Morph base and targets read from the glTF mesh, with mesh-level default
    /// weights. A node's own `weights` array overrides those per instance.
    morph: MorphSet,
    /// Per imported point, the four `skin.joints` indices from JOINTS_0. Empty
    /// when the primitive has no skinning attributes.
    joint_indices: Vec<[u32; 4]>,
    /// Per imported point, the four weights from WEIGHTS_0.
    weights: Vec<[f32; 4]>,
}

/// Convenience wrapper for tests that only assert on the document.
#[cfg(test)]
fn import_value_with_buffers(gltf: Value, buffers: Vec<Vec<u8>>) -> Result<Document, String> {
    import_value_with_buffers_and_report(gltf, buffers).map(|(doc, _)| doc)
}

fn import_value_with_buffers_and_report(
    gltf: Value,
    buffers: Vec<Vec<u8>>,
) -> Result<(Document, ImportReport), String> {
    let mut report = ImportReport::default();
    let mut doc = Document::empty();
    // LeopardCraft exports the active take's name and fps in `extras`. A file
    // without them keeps the default take built by `Document::empty`.
    let take_meta = gltf
        .get("extras")
        .and_then(|extras| extras.get("leopardcraft"))
        .or_else(|| {
            gltf.get("animations")
                .and_then(|animations| animations.as_array())
                .and_then(|animations| animations.first())
                .and_then(|animation| animation.get("extras"))
                .and_then(|extras| extras.get("leopardcraft"))
        });
    if let Some(meta) = take_meta {
        if let Some(name) = meta.get("take").and_then(|name| name.as_str()) {
            doc.rename_take(0, name);
        }
        if let Some(fps) = meta.get("fps").and_then(|fps| fps.as_f64()) {
            doc.set_take_fps(0, fps as f32);
        }
    }
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
            // Per-corner coordinates for TEXCOORD_1.., in polygon corner order.
            let mut extra_uv_values: Vec<Vec<Vec2>> = Vec::new();
            let mut extra_uv_set_count = 0usize;
            let mut slots: BTreeMap<u16, MaterialId> = BTreeMap::new();
            let mut morph = MorphSet::default();
            let mut skin_joint_indices: Vec<[u32; 4]> = Vec::new();
            let mut skin_weights: Vec<[f32; 4]> = Vec::new();
            // Every imported primitive of one glTF mesh shares the target list;
            // a primitive that disagrees cannot be merged into one morph set.
            let mut target_count: Option<usize> = None;
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
                let vertex_count = positions.len() / 3;
                let joints_attribute = attrs.get("JOINTS_0").and_then(|p| p.as_u64());
                let weights_attribute = attrs.get("WEIGHTS_0").and_then(|p| p.as_u64());
                if joints_attribute.is_some() != weights_attribute.is_some() {
                    return Err(format!(
                        "glTF mesh {mesh_name:?} declares JOINTS_0 and WEIGHTS_0 inconsistently; skinned primitives need both"
                    ));
                }
                let primitive_joints = joints_attribute
                    .map(|index| {
                        let (values, components) =
                            read_accessor_f32(&gltf, index as usize, &buffers)?;
                        if components != 4 || values.len() != vertex_count * 4 {
                            return Err(format!(
                                "glTF mesh {mesh_name:?}: JOINTS_0 must hold four indices per vertex"
                            ));
                        }
                        Ok(values)
                    })
                    .transpose()?;
                let primitive_weights = weights_attribute
                    .map(|index| {
                        let (values, components) =
                            read_accessor_f32(&gltf, index as usize, &buffers)?;
                        if components != 4 || values.len() != vertex_count * 4 {
                            return Err(format!(
                                "glTF mesh {mesh_name:?}: WEIGHTS_0 must hold four weights per vertex"
                            ));
                        }
                        Ok(values)
                    })
                    .transpose()?;
                let uvs = attrs
                    .get("TEXCOORD_0")
                    .and_then(|p| p.as_u64())
                    .map(|i| read_accessor_f32(&gltf, i as usize, &buffers))
                    .transpose()?
                    .map(|(v, _)| v)
                    .unwrap_or_default();
                let mut primitive_extra_uvs: Vec<Vec<f32>> = Vec::new();
                let mut set = 1usize;
                while let Some(index) = attrs
                    .get(format!("TEXCOORD_{set}"))
                    .and_then(|value| value.as_u64())
                {
                    let (values, _) = read_accessor_f32(&gltf, index as usize, &buffers)?;
                    primitive_extra_uvs.push(values);
                    set += 1;
                }
                extra_uv_set_count = extra_uv_set_count.max(primitive_extra_uvs.len());
                let primitive_targets = primitive
                    .get("targets")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                match target_count {
                    Some(expected) if expected != primitive_targets.len() => {
                        return Err(format!(
                            "glTF mesh {mesh_name:?} has primitives with {expected} and {} morph targets; merged primitives must declare the same targets",
                            primitive_targets.len()
                        ));
                    }
                    Some(_) => {}
                    None => {
                        target_count = Some(primitive_targets.len());
                        if !primitive_targets.is_empty() {
                            let names = mesh_value
                                .get("extras")
                                .and_then(|extras| extras.get("targetNames"))
                                .and_then(Value::as_array);
                            if let Some(names) = names {
                                if names.len() != primitive_targets.len() {
                                    return Err(format!(
                                        "glTF mesh {mesh_name:?} names {} morph targets but declares {}",
                                        names.len(),
                                        primitive_targets.len()
                                    ));
                                }
                            }
                            morph.targets = (0..primitive_targets.len())
                                .map(|index| {
                                    let name = names
                                        .and_then(|names| names.get(index))
                                        .and_then(Value::as_str)
                                        .unwrap_or("");
                                    MorphTarget::new(name, Vec::new())
                                })
                                .collect();
                        }
                    }
                }
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
                for chunk in positions.as_chunks::<3>().0 {
                    mesh.positions.push(Vec3::new(chunk[0], chunk[1], chunk[2]));
                }
                if let (Some(joints), Some(weights)) =
                    (primitive_joints.as_ref(), primitive_weights.as_ref())
                {
                    for vertex in 0..vertex_count {
                        let base = vertex * 4;
                        skin_joint_indices.push([
                            joints[base] as u32,
                            joints[base + 1] as u32,
                            joints[base + 2] as u32,
                            joints[base + 3] as u32,
                        ]);
                        skin_weights.push([
                            weights[base],
                            weights[base + 1],
                            weights[base + 2],
                            weights[base + 3],
                        ]);
                    }
                }
                if !primitive_targets.is_empty() {
                    let vertex_count = positions.len() / 3;
                    for (target_index, target) in primitive_targets.iter().enumerate() {
                        let deltas = match target.get("POSITION").and_then(Value::as_u64) {
                            Some(index) => {
                                let (values, _) =
                                    read_accessor_f32(&gltf, index as usize, &buffers)?;
                                if values.len() != vertex_count * 3 {
                                    return Err(format!(
                                        "glTF morph target {} of mesh {mesh_name:?} has {} positions for {vertex_count} vertices",
                                        target_index + 1,
                                        values.len() / 3
                                    ));
                                }
                                values
                                    .as_chunks::<3>()
                                    .0
                                    .iter()
                                    .map(|chunk| Vec3::new(chunk[0], chunk[1], chunk[2]))
                                    .collect::<Vec<Vec3>>()
                            }
                            // A target may carry only normal or tangent deltas.
                            None => vec![Vec3::ZERO; vertex_count],
                        };
                        if let Some(entry) = morph.targets.get_mut(target_index) {
                            entry.deltas.extend(deltas);
                        }
                    }
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
                        extra_uv_values.resize_with(extra_uv_set_count, Vec::new);
                        for (set_index, values) in extra_uv_values.iter_mut().enumerate() {
                            for index in tri {
                                let uv_index = *index as usize * 2;
                                let uv = primitive_extra_uvs
                                    .get(set_index)
                                    .filter(|values| uv_index + 1 < values.len())
                                    .map(|values| Vec2::new(values[uv_index], values[uv_index + 1]))
                                    .unwrap_or(Vec2::ZERO);
                                values.push(uv);
                            }
                        }
                    } else {
                        mesh.add_face(&points);
                    }
                    if let Some(poly) = mesh.polygons.last_mut() {
                        poly.material_slot = slot;
                    }
                }
            }
            if !extra_uv_values.is_empty() {
                mesh.extra_uvs = extra_uv_values;
                mesh.sync_uv_sets();
            }
            if target_count.unwrap_or(0) > 0 {
                if let Some(weights) = mesh_value.get("weights").and_then(Value::as_array) {
                    if weights.len() != morph.targets.len() {
                        return Err(format!(
                            "glTF mesh {mesh_name:?}: morph weights ({}) do not match morph targets ({})",
                            weights.len(),
                            morph.targets.len()
                        ));
                    }
                    for (target, weight) in morph.targets.iter_mut().zip(weights) {
                        target.weight = weight.as_f64().unwrap_or(0.0) as f32;
                    }
                }
                morph.base = mesh.positions.clone();
                morph.sanitize(Some(mesh.positions.len()));
            }
            if !mesh.positions.is_empty() {
                imported_meshes.push(ImportedMesh {
                    mesh,
                    slots,
                    morph,
                    joint_indices: skin_joint_indices,
                    weights: skin_weights,
                });
            }
        }
    }

    let nodes = gltf
        .get("nodes")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();
    // Resolve cameras and lights before the walk so node kinds are known when
    // nodes are created; unusable ones fail here with a message naming the node.
    let mut node_extras: Vec<NodeExtras> = vec![NodeExtras::default(); nodes.len()];
    {
        let cameras = gltf.get("cameras").and_then(Value::as_array);
        let lights = gltf
            .get("extensions")
            .and_then(|extensions| extensions.get("KHR_lights_punctual"))
            .and_then(|lights| lights.get("lights"))
            .and_then(Value::as_array);
        for (index, node) in nodes.iter().enumerate() {
            if let Some(camera_index) = node.get("camera").and_then(Value::as_u64) {
                let camera = cameras
                    .and_then(|cameras| cameras.get(camera_index as usize))
                    .ok_or_else(|| {
                        format!(
                            "glTF node {index} references camera {camera_index}, which the file does not define; fix or remove the camera"
                        )
                    })?;
                node_extras[index].camera = Some(import_camera(camera, index, &mut report)?);
            }
            if let Some(light_index) = node
                .get("extensions")
                .and_then(|extensions| extensions.get("KHR_lights_punctual"))
                .and_then(|light| light.get("light"))
                .and_then(Value::as_u64)
            {
                let light = lights
                    .and_then(|lights| lights.get(light_index as usize))
                    .ok_or_else(|| {
                        format!(
                            "glTF node {index} references light {light_index}, which the file does not define; fix or remove the light"
                        )
                    })?;
                node_extras[index].light = Some(import_light(light, index)?);
            }
        }
    }
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
            &nodes,
            &imported_meshes,
            &node_extras,
            root,
            None,
            &mut doc,
            &mut created,
            &mut visited,
        )?;
    }
    import_skins(
        &gltf,
        &nodes,
        &imported_meshes,
        &buffers,
        &created,
        &mut doc,
    )?;
    import_animations(&gltf, &buffers, &created, &mut doc)?;
    doc.settings.ensure_valid_layers();
    doc.sanitize();
    doc.validate()
        .map_err(|errors| format!("imported glTF is invalid:\n{}", errors.join("\n")))?;
    Ok((doc, report))
}

/// Build a `Skin` for every node that references a glTF skin. This runs after
/// the node walk so joint indices can be mapped to the created nodes in any
/// order the file lists them.
fn import_skins(
    gltf: &Value,
    nodes: &[Value],
    meshes: &[ImportedMesh],
    buffers: &[Vec<u8>],
    created: &BTreeMap<usize, NodeId>,
    doc: &mut Document,
) -> Result<(), String> {
    let skins = gltf
        .get("skins")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for (node_index, node_value) in nodes.iter().enumerate() {
        let Some(skin_index) = node_value.get("skin").and_then(Value::as_u64) else {
            continue;
        };
        // Nodes outside the imported scene were never created, so their skin
        // cannot be attached to anything.
        let Some(node_id) = created.get(&node_index).copied() else {
            continue;
        };
        let name = doc
            .scene
            .get(node_id)
            .map(|node| node.name.clone())
            .unwrap_or_default();
        let mesh_index = node_value
            .get("mesh")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("glTF node {name:?} has a skin but no mesh"))?
            as usize;
        let imported = meshes
            .get(mesh_index)
            .ok_or_else(|| format!("glTF node {name:?} references missing mesh {mesh_index}"))?;
        let definition = skins.get(skin_index as usize).ok_or_else(|| {
            format!(
                "glTF node {name:?} references skin {skin_index}, which the file does not define"
            )
        })?;
        let joint_nodes = definition
            .get("joints")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("glTF skin {skin_index} has no joints array"))?;
        if joint_nodes.is_empty() {
            return Err(format!("glTF skin {skin_index} has no joints"));
        }
        let mut joints = Vec::with_capacity(joint_nodes.len());
        for joint in joint_nodes {
            let index = joint.as_u64().ok_or_else(|| {
                format!("glTF skin {skin_index} has a joint index that is not an integer")
            })? as usize;
            let id = created.get(&index).ok_or_else(|| {
                format!(
                    "glTF node {name:?}: skin {skin_index} joint node {index} is not part of the imported hierarchy"
                )
            })?;
            joints.push(*id);
        }
        let inverse_bind = match definition
            .get("inverseBindMatrices")
            .and_then(Value::as_u64)
        {
            Some(accessor) => {
                let (values, components) = read_accessor_f32(gltf, accessor as usize, buffers)?;
                if components != 16 || values.len() != joints.len() * 16 {
                    return Err(format!(
                        "glTF skin {skin_index}: inverseBindMatrices must hold one MAT4 per joint"
                    ));
                }
                let matrices: Vec<Mat4> = values
                    .as_chunks::<16>()
                    .0
                    .iter()
                    .map(Mat4::from_cols_array)
                    .collect();
                if !matrices.iter().all(|matrix| matrix.is_finite()) {
                    return Err(format!(
                        "glTF skin {skin_index}: inverseBindMatrices contain non-finite values"
                    ));
                }
                matrices
            }
            // The glTF default when the accessor is absent is identity.
            None => vec![Mat4::IDENTITY; joints.len()],
        };
        let point_count = imported.mesh.positions.len();
        if imported.joint_indices.len() != point_count || imported.weights.len() != point_count {
            return Err(format!(
                "glTF node {name:?} is skinned but its mesh has no JOINTS_0/WEIGHTS_0 for every vertex"
            ));
        }
        let mut joint_indices = Vec::with_capacity(point_count);
        let mut weights = Vec::with_capacity(point_count);
        for point in 0..point_count {
            let mut point_joints = [0u32; 4];
            let mut point_weights = [0.0f32; 4];
            let mut total = 0.0f32;
            for influence in 0..4 {
                let index = imported.joint_indices[point][influence];
                let weight = imported.weights[point][influence];
                if !weight.is_finite() || weight <= 0.0 {
                    continue;
                }
                if index as usize >= joints.len() {
                    return Err(format!(
                        "glTF node {name:?}: JOINT index {index} is outside skin {skin_index}'s {} joints",
                        joints.len()
                    ));
                }
                point_joints[influence] = index;
                point_weights[influence] = weight;
                total += weight;
            }
            if total > 0.0 {
                for weight in &mut point_weights {
                    *weight /= total;
                }
            }
            joint_indices.push(point_joints);
            weights.push(point_weights);
        }
        if let Some(node) = doc.scene.get_mut(node_id) {
            node.skin = Some(Skin {
                joints,
                inverse_bind,
                joint_indices,
                weights,
            });
        }
    }
    Ok(())
}

/// Insert one imported key. CUBICSPLINE values are the authored glTF tangents
/// (value units per second), stored as hand-authored Spline slopes so a
/// re-export samples the same curve.
fn insert_imported_key(
    curve: &mut Curve,
    time: f32,
    value: f32,
    interpolation: Interpolation,
    cubic: bool,
    tangent_in: f32,
    tangent_out: f32,
) {
    let index = curve.insert(time, value, interpolation);
    if cubic {
        if let Some(key) = curve.keys.get_mut(index) {
            key.tangent_in = tangent_in;
            key.tangent_out = tangent_out;
            key.tangent_manual = true;
            key.tangent_in_type = TangentType::Spline;
            key.tangent_out_type = TangentType::Spline;
        }
    }
}

fn store_imported_curve(clip: &mut AnimationClip, address: ChannelAddress, curve: Curve) {
    if let Some(track) = clip.track_mut(address) {
        track.curve = curve;
    } else {
        clip.tracks.push(AnimationTrack { address, curve });
    }
}

/// Import glTF animation channels. The first animation fills the document's
/// default take; further animations become additional takes. Seconds stay
/// seconds, and each take's frame range is the clip duration at that take's
/// fps.
fn import_animations(
    gltf: &Value,
    buffers: &[Vec<u8>],
    created: &BTreeMap<usize, NodeId>,
    doc: &mut Document,
) -> Result<(), String> {
    let animations = gltf
        .get("animations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if animations.is_empty() {
        return Ok(());
    }
    for (animation_index, animation) in animations.iter().enumerate() {
        let name = animation
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or("Take");
        let fps = animation
            .get("extras")
            .and_then(|extras| extras.get("leopardcraft"))
            .and_then(|meta| meta.get("fps"))
            .and_then(Value::as_f64)
            .map(|fps| fps as f32)
            .filter(|fps| fps.is_finite() && *fps > 0.0)
            .unwrap_or_else(|| doc.active_take_fps());
        if animation_index == 0 {
            let take_name = doc.unique_take_name(name);
            doc.rename_take(0, &take_name);
            doc.set_take_fps(0, fps);
        } else {
            doc.create_take(name);
            let index = doc.active_take;
            doc.set_take_fps(index, fps);
        }
        let samplers = animation
            .get("samplers")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("glTF animation {animation_index} has no samplers"))?;
        let channels = animation
            .get("channels")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("glTF animation {animation_index} has no channels"))?;
        let mut duration = 0.0f32;
        for (channel_index, channel) in channels.iter().enumerate() {
            let sampler_index = channel
                .get("sampler")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    format!(
                        "glTF animation {animation_index} channel {channel_index} has no sampler"
                    )
                })? as usize;
            let sampler = samplers.get(sampler_index).ok_or_else(|| {
                format!(
                    "glTF animation {animation_index} references missing sampler {sampler_index}"
                )
            })?;
            let input = sampler
                .get("input")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    format!("glTF animation {animation_index} sampler {sampler_index} has no input")
                })? as usize;
            let output = sampler
                .get("output")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    format!(
                        "glTF animation {animation_index} sampler {sampler_index} has no output"
                    )
                })? as usize;
            let (times, time_components) = read_accessor_f32(gltf, input, buffers)?;
            if time_components != 1 || times.is_empty() {
                return Err(format!(
                    "glTF animation {animation_index} sampler {sampler_index} needs a non-empty SCALAR time accessor"
                ));
            }
            if !times.iter().all(|time| time.is_finite()) {
                return Err(format!(
                    "glTF animation {animation_index} has non-finite key times"
                ));
            }
            if times.windows(2).any(|pair| pair[1] <= pair[0]) {
                return Err(format!(
                    "glTF animation {animation_index} key times must be strictly increasing"
                ));
            }
            let interpolation = sampler
                .get("interpolation")
                .and_then(Value::as_str)
                .unwrap_or("LINEAR");
            let cubic = interpolation == "CUBICSPLINE";
            let interpolation_kind = match interpolation {
                "STEP" => Interpolation::Constant,
                "LINEAR" => Interpolation::Linear,
                "CUBICSPLINE" => Interpolation::Spline,
                other => {
                    return Err(format!(
                        "glTF animation {animation_index} uses unsupported interpolation {other:?}"
                    ))
                }
            };
            let (values, value_components) = read_accessor_f32(gltf, output, buffers)?;
            let target = channel.get("target").ok_or_else(|| {
                format!("glTF animation {animation_index} channel {channel_index} has no target")
            })?;
            let node_index = target.get("node").and_then(Value::as_u64).ok_or_else(|| {
                format!(
                    "glTF animation {animation_index} channel {channel_index} has no target node"
                )
            })? as usize;
            let node_id = created.get(&node_index).copied().ok_or_else(|| {
                format!(
                    "glTF animation {animation_index} targets node {node_index}, which the imported hierarchy does not contain"
                )
            })?;
            let path = target.get("path").and_then(Value::as_str).unwrap_or("");
            let keys = times.len();
            let multiplier = if cubic { 3 } else { 1 };
            duration = duration.max(times.last().copied().unwrap_or(0.0));
            match path {
                "translation" | "scale" => {
                    if value_components != 3 || values.len() != keys * 3 * multiplier {
                        return Err(format!(
                            "glTF animation {animation_index} {path} channel has the wrong output count"
                        ));
                    }
                    let channel_kind = if path == "translation" {
                        AnimChannel::Translation
                    } else {
                        AnimChannel::Scale
                    };
                    let clip = &mut doc
                        .scene
                        .get_mut(node_id)
                        .expect("created node must exist")
                        .animation;
                    for component in 0..3u8 {
                        let mut curve = Curve::new();
                        for (key, time) in times.iter().enumerate() {
                            let base = key * 3 * multiplier;
                            let (value, tangent_in, tangent_out) = if cubic {
                                (
                                    values[base + 3 + component as usize],
                                    values[base + component as usize],
                                    values[base + 6 + component as usize],
                                )
                            } else {
                                (values[base + component as usize], 0.0, 0.0)
                            };
                            insert_imported_key(
                                &mut curve,
                                *time,
                                value,
                                interpolation_kind,
                                cubic,
                                tangent_in,
                                tangent_out,
                            );
                        }
                        store_imported_curve(
                            clip,
                            ChannelAddress {
                                target: ChannelTarget::Object { id: node_id },
                                channel: channel_kind,
                                component,
                            },
                            curve,
                        );
                    }
                }
                "rotation" => {
                    if value_components != 4 || values.len() != keys * 4 * multiplier {
                        return Err(format!(
                            "glTF animation {animation_index} rotation channel has the wrong output count"
                        ));
                    }
                    // Quaternion keys carry no tangent handles; cubic rotation
                    // imports as Smooth, which re-derives Catmull-Rom slopes.
                    let rotation_interpolation = if cubic {
                        Interpolation::Smooth
                    } else {
                        interpolation_kind
                    };
                    let clip = &mut doc
                        .scene
                        .get_mut(node_id)
                        .expect("created node must exist")
                        .animation;
                    for (key, time) in times.iter().enumerate() {
                        let offset = key * 4 * multiplier + if cubic { 4 } else { 0 };
                        let rotation = Quat::from_xyzw(
                            values[offset],
                            values[offset + 1],
                            values[offset + 2],
                            values[offset + 3],
                        );
                        clip.set_rotation_key(
                            ChannelTarget::Object { id: node_id },
                            *time,
                            rotation,
                            rotation_interpolation,
                        );
                    }
                }
                "weights" => {
                    let target_count = doc
                        .scene
                        .get(node_id)
                        .map(|node| node.morph.targets.len())
                        .unwrap_or(0);
                    if target_count == 0 {
                        continue;
                    }
                    if value_components != 1 || values.len() != keys * target_count * multiplier {
                        return Err(format!(
                            "glTF animation {animation_index} weights channel has the wrong output count for {target_count} morph targets"
                        ));
                    }
                    let clip = &mut doc
                        .scene
                        .get_mut(node_id)
                        .expect("created node must exist")
                        .animation;
                    for target in 0..target_count {
                        let mut curve = Curve::new();
                        for (key, time) in times.iter().enumerate() {
                            let base = key * target_count * multiplier;
                            let (value, tangent_in, tangent_out) = if cubic {
                                (
                                    values[base + target_count + target],
                                    values[base + target],
                                    values[base + 2 * target_count + target],
                                )
                            } else {
                                (values[base + target], 0.0, 0.0)
                            };
                            insert_imported_key(
                                &mut curve,
                                *time,
                                value,
                                interpolation_kind,
                                cubic,
                                tangent_in,
                                tangent_out,
                            );
                        }
                        clip.morph_weights.insert(target as u32, curve);
                    }
                }
                other => {
                    return Err(format!(
                        "glTF animation {animation_index} targets unsupported path {other:?}"
                    ))
                }
            }
        }
        if animation_index == 0 {
            doc.settings.frame_start = 0.0;
            doc.settings.frame_end = (duration * fps).ceil().max(1.0);
        }
        doc.store_active_clips();
    }
    doc.activate_take(0);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn import_node(
    nodes: &[Value],
    meshes: &[ImportedMesh],
    extras: &[NodeExtras],
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
    let node_extras = extras.get(index).cloned().unwrap_or_default();
    let kind = if let Some(camera) = node_extras.camera {
        NodeKind::Camera { camera }
    } else if let Some(light) = node_extras.light {
        NodeKind::Light { light }
    } else if let Some(imported) = &mesh {
        NodeKind::Mesh {
            geometry: GeometrySource::editable(imported.mesh.clone()),
        }
    } else {
        NodeKind::Null
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
            node.morph = imported.morph.clone();
            if let Some(weights) = value.get("weights").and_then(|w| w.as_array()) {
                if weights.len() != node.morph.targets.len() {
                    return Err(format!(
                        "glTF node {name:?}: morph weights ({}) do not match morph targets ({})",
                        weights.len(),
                        node.morph.targets.len()
                    ));
                }
                for (target, weight) in node.morph.targets.iter_mut().zip(weights) {
                    target.weight = weight.as_f64().unwrap_or(0.0) as f32;
                }
                node.morph.sanitize(Some(imported.mesh.positions.len()));
            }
        }
    }
    created.insert(index, id);
    if let Some(children) = value.get("children").and_then(|c| c.as_array()) {
        for child in children.iter().filter_map(|c| c.as_u64()) {
            import_node(
                nodes,
                meshes,
                extras,
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
    let element_size = components * comp_size;
    let stride = view
        .get("byteStride")
        .and_then(|s| s.as_u64())
        .map(|s| s as usize)
        .unwrap_or(element_size);
    // The glTF spec requires a stride of at least one element. A smaller one
    // makes the per-component reads of the last element run past `needed`, so
    // reject it instead of panicking while slicing the buffer.
    if count > 0 && stride < element_size {
        return Err(format!(
            "glTF bufferView stride {stride} is smaller than its {element_size}-byte accessor element"
        ));
    }
    let view_offset = view.get("byteOffset").and_then(|o| o.as_u64()).unwrap_or(0) as usize;
    let accessor_offset = accessor
        .get("byteOffset")
        .and_then(|o| o.as_u64())
        .unwrap_or(0) as usize;
    let base = view_offset
        .checked_add(accessor_offset)
        .ok_or_else(|| "glTF accessor offset overflow".to_string())?;
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

#[cfg(test)]
mod animation_export_tests {
    use super::*;
    use lc_scene::{GeometrySource, NodeKind};

    fn quad_mesh() -> Mesh {
        let mut mesh = Mesh::new("MorphQuad");
        mesh.positions = vec![
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(-1.0, 1.0, 0.0),
        ];
        mesh.add_face(&[0, 1, 2, 3]);
        mesh
    }

    fn rigged_document() -> Document {
        let mut doc = Document::empty();
        doc.set_take_fps(0, 10.0);
        doc.rename_take(0, "Run");
        let root = doc.append_node(
            NodeKind::Joint {
                length: 0.5,
                width: 0.1,
            },
            "Root",
            None,
        );
        let child = doc.append_node(
            NodeKind::Joint {
                length: 0.5,
                width: 0.1,
            },
            "Child",
            Some(root),
        );
        doc.scene.get_mut(child).unwrap().transform = lc_core::Transform::from_translation(Vec3::Y);
        let mesh = quad_mesh();
        let base = mesh.positions.clone();
        let mesh_id = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::editable(mesh),
            },
            "MorphMesh",
            None,
        );
        let evaluated = doc.evaluate(0.0);
        let mut skin = Skin::new(
            vec![root, child],
            vec![
                evaluated.node(root).unwrap().world.inverse(),
                evaluated.node(child).unwrap().world.inverse(),
            ],
        );
        skin.joint_indices = vec![[0, 1, 0, 0], [1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 0, 0]];
        skin.weights = vec![
            [0.75, 0.25, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.5, 0.5, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        ];
        let mut smile = MorphTarget::new(
            "Smile",
            vec![Vec3::Z * 0.5, Vec3::Z * 0.25, Vec3::ZERO, Vec3::ZERO],
        );
        smile.weight = 0.25;
        let mut brow = MorphTarget::new(
            "Brow",
            vec![Vec3::X * 0.25, Vec3::ZERO, Vec3::ZERO, Vec3::X * 0.1],
        );
        brow.weight = 0.5;
        {
            let node = doc.scene.get_mut(mesh_id).unwrap();
            node.skin = Some(skin);
            node.morph = MorphSet {
                base,
                targets: vec![smile, brow],
            };
            node.animation
                .set_morph_weight_key(0, 0.0, 0.0, Interpolation::Linear);
            node.animation
                .set_morph_weight_key(0, 1.0, 1.0, Interpolation::Linear);
            node.animation
                .set_morph_weight_key(1, 0.0, 0.5, Interpolation::Linear);
            node.animation
                .set_morph_weight_key(1, 1.0, 0.5, Interpolation::Linear);
        }
        {
            let node = doc.scene.get_mut(root).unwrap();
            node.animation.set_key(
                ChannelAddress::owner_component(AnimChannel::Translation, 0),
                0.0,
                0.0,
                Interpolation::Constant,
            );
            node.animation.set_key(
                ChannelAddress::owner_component(AnimChannel::Translation, 0),
                1.0,
                1.5,
                Interpolation::Constant,
            );
        }
        {
            let node = doc.scene.get_mut(child).unwrap();
            node.animation.set_rotation_key(
                ChannelTarget::Owner,
                0.0,
                Quat::IDENTITY,
                Interpolation::Linear,
            );
            node.animation.set_rotation_key(
                ChannelTarget::Owner,
                1.0,
                Quat::from_rotation_z(0.5),
                Interpolation::Linear,
            );
            node.animation.set_key(
                ChannelAddress::owner_component(AnimChannel::Scale, 1),
                0.0,
                1.0,
                Interpolation::Linear,
            );
            node.animation.set_key(
                ChannelAddress::owner_component(AnimChannel::Scale, 1),
                1.0,
                1.5,
                Interpolation::Linear,
            );
        }
        doc
    }

    fn export_glb_value(doc: &Document) -> (Value, Vec<u8>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.glb");
        export_glb(&path, doc).unwrap();
        let bytes = std::fs::read(path).unwrap();
        let (gltf, bin) = parse_glb(&bytes).unwrap();
        (gltf, bin.unwrap_or_default())
    }

    fn node_index(gltf: &Value, name: &str) -> usize {
        gltf["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .position(|node| node["name"].as_str() == Some(name))
            .unwrap()
    }

    fn accessor(gltf: &Value, index: u64, buffers: &[Vec<u8>]) -> (Vec<f32>, usize) {
        read_accessor_f32(gltf, index as usize, buffers).unwrap()
    }

    fn accessor_index(value: &Value) -> usize {
        value.as_u64().expect("accessor index") as usize
    }

    fn accessor_count(gltf: &Value, index: usize) -> usize {
        gltf["accessors"][index]["count"]
            .as_u64()
            .expect("accessor count") as usize
    }

    fn accessor_components(accessor: &Value) -> usize {
        match accessor["type"].as_str().expect("accessor type") {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            "MAT4" => 16,
            other => panic!("unexpected accessor type {other}"),
        }
    }

    fn assert_close(label: &str, actual: f32, expected: f32) {
        let tolerance = 1e-5_f32.max(1e-5 * actual.abs().max(expected.abs()));
        assert!(
            (actual - expected).abs() <= tolerance,
            "{label}: {actual} != {expected}"
        );
    }

    fn assert_accessor_bounds(gltf: &Value, index: usize, buffers: &[Vec<u8>], label: &str) {
        let accessor_json = &gltf["accessors"][index];
        let components = accessor_components(accessor_json);
        let (values, read_components) = accessor(gltf, index as u64, buffers);
        assert_eq!(read_components, components, "{label}: component count");
        assert!(!values.is_empty(), "{label}: empty accessor");
        let min = accessor_json
            .get("min")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{label}: missing min"));
        let max = accessor_json
            .get("max")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{label}: missing max"));
        assert_eq!(min.len(), components, "{label}: min length");
        assert_eq!(max.len(), components, "{label}: max length");
        for component in 0..components {
            let mut actual_min = f32::INFINITY;
            let mut actual_max = f32::NEG_INFINITY;
            for chunk in values.chunks_exact(components) {
                actual_min = actual_min.min(chunk[component]);
                actual_max = actual_max.max(chunk[component]);
            }
            let declared_min = min[component].as_f64().expect("min value") as f32;
            let declared_max = max[component].as_f64().expect("max value") as f32;
            assert_close(
                &format!("{label}: min[{component}]"),
                declared_min,
                actual_min,
            );
            assert_close(
                &format!("{label}: max[{component}]"),
                declared_max,
                actual_max,
            );
        }
    }

    fn mesh_target_count(gltf: &Value, mesh_index: u64) -> usize {
        gltf["meshes"][mesh_index as usize]["primitives"][0]
            .get("targets")
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    }

    fn node_mesh_target_count(gltf: &Value, node_index: usize) -> Option<usize> {
        let mesh = gltf["nodes"][node_index].get("mesh")?.as_u64()?;
        Some(mesh_target_count(gltf, mesh))
    }

    fn assert_morph_conformance(gltf: &Value, buffers: &[Vec<u8>]) {
        let Some(meshes) = gltf.get("meshes").and_then(Value::as_array) else {
            return;
        };
        for (mesh_index, mesh) in meshes.iter().enumerate() {
            let primitives = mesh["primitives"].as_array().expect("primitives");
            assert!(!primitives.is_empty());
            let target_count = mesh_target_count(gltf, mesh_index as u64);
            let target_names = mesh
                .get("extras")
                .and_then(|extras| extras.get("targetNames"))
                .and_then(Value::as_array);
            if target_count > 0 {
                let names = target_names
                    .unwrap_or_else(|| panic!("mesh {mesh_index}: targets without targetNames"));
                assert_eq!(names.len(), target_count);
                assert!(names.iter().all(|name| name.is_string()));
            } else {
                assert!(
                    target_names.is_none(),
                    "mesh {mesh_index}: targetNames without targets"
                );
            }
            for (primitive_index, primitive) in primitives.iter().enumerate() {
                let attributes = primitive["attributes"].as_object().expect("attributes");
                let position = attributes
                    .get("POSITION")
                    .unwrap_or_else(|| panic!("mesh {mesh_index}: missing POSITION"));
                let position_index = accessor_index(position);
                let position_count = accessor_count(gltf, position_index);
                assert_eq!(gltf["accessors"][position_index]["type"], "VEC3");
                assert_eq!(gltf["accessors"][position_index]["componentType"], 5126);
                assert_accessor_bounds(
                    gltf,
                    position_index,
                    buffers,
                    &format!("mesh {mesh_index} primitive {primitive_index} POSITION"),
                );
                let targets = primitive
                    .get("targets")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                assert_eq!(targets, target_count);
                for (target_index, target) in primitive
                    .get("targets")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    let target_position = target
                        .get("POSITION")
                        .unwrap_or_else(|| panic!("mesh {mesh_index}: target POSITION"));
                    let target_position_index = accessor_index(target_position);
                    let target_accessor = &gltf["accessors"][target_position_index];
                    assert_eq!(target_accessor["type"], "VEC3");
                    assert_eq!(target_accessor["componentType"], 5126);
                    assert_eq!(accessor_count(gltf, target_position_index), position_count);
                    assert_accessor_bounds(
                        gltf,
                        target_position_index,
                        buffers,
                        &format!(
                            "mesh {mesh_index} primitive {primitive_index} target {target_index} POSITION"
                        ),
                    );
                    if let Some(normal) = target.get("NORMAL") {
                        let normal_index = accessor_index(normal);
                        assert_eq!(gltf["accessors"][normal_index]["type"], "VEC3");
                        assert_eq!(gltf["accessors"][normal_index]["componentType"], 5126);
                        assert_eq!(accessor_count(gltf, normal_index), position_count);
                    }
                }
            }
        }
        let nodes = gltf["nodes"].as_array().expect("nodes");
        for (node_index, node) in nodes.iter().enumerate() {
            let Some(mesh_index) = node.get("mesh").and_then(Value::as_u64) else {
                continue;
            };
            let target_count = mesh_target_count(gltf, mesh_index);
            if target_count > 0 {
                let weights = node
                    .get("weights")
                    .and_then(Value::as_array)
                    .unwrap_or_else(|| panic!("node {node_index}: missing morph weights"));
                assert_eq!(weights.len(), target_count);
                assert!(weights.iter().all(|weight| weight.as_f64().is_some()));
            } else {
                assert!(node.get("weights").is_none());
            }
        }
    }

    fn assert_skin_conformance(gltf: &Value, buffers: &[Vec<u8>]) {
        let nodes = gltf["nodes"].as_array().expect("nodes");
        let skins = gltf
            .get("skins")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for (skin_index, skin) in skins.iter().enumerate() {
            let joints = skin["joints"].as_array().expect("skin joints");
            assert!(!joints.is_empty());
            for joint in joints {
                assert!(
                    (joint.as_u64().expect("joint index") as usize) < nodes.len(),
                    "skin {skin_index}: joint out of range"
                );
            }
            if let Some(skeleton) = skin.get("skeleton") {
                assert!(
                    (skeleton.as_u64().expect("skeleton index") as usize) < nodes.len(),
                    "skin {skin_index}: skeleton out of range"
                );
            }
            if let Some(ibm) = skin.get("inverseBindMatrices") {
                let index = accessor_index(ibm);
                let accessor_json = &gltf["accessors"][index];
                assert_eq!(accessor_json["type"], "MAT4");
                assert_eq!(accessor_json["componentType"], 5126);
                assert!(accessor_count(gltf, index) >= joints.len());
                let (values, components) = accessor(gltf, index as u64, buffers);
                assert_eq!(components, 16);
                for matrix in 0..joints.len() {
                    let offset = matrix * 16;
                    assert_close(
                        &format!("skin {skin_index} inverse bind row"),
                        values[offset + 3],
                        0.0,
                    );
                    assert_close(
                        &format!("skin {skin_index} inverse bind row"),
                        values[offset + 7],
                        0.0,
                    );
                    assert_close(
                        &format!("skin {skin_index} inverse bind row"),
                        values[offset + 11],
                        0.0,
                    );
                    assert_close(
                        &format!("skin {skin_index} inverse bind row"),
                        values[offset + 15],
                        1.0,
                    );
                }
            }
        }
        for (node_index, node) in nodes.iter().enumerate() {
            let Some(skin_index) = node.get("skin").and_then(Value::as_u64) else {
                continue;
            };
            let skin = skins
                .get(skin_index as usize)
                .unwrap_or_else(|| panic!("node {node_index}: skin out of range"));
            let joints_count = skin["joints"].as_array().unwrap().len();
            let mesh_index = node["mesh"].as_u64().expect("skinned node mesh");
            let mesh = &gltf["meshes"][mesh_index as usize];
            for (primitive_index, primitive) in
                mesh["primitives"].as_array().unwrap().iter().enumerate()
            {
                let attributes = primitive["attributes"].as_object().unwrap();
                let position_count = accessor_count(
                    gltf,
                    accessor_index(attributes.get("POSITION").expect("POSITION")),
                );
                let mut set = 0usize;
                loop {
                    let joint = attributes.get(&format!("JOINTS_{set}"));
                    let weight = attributes.get(&format!("WEIGHTS_{set}"));
                    match (joint, weight) {
                        (None, None) => break,
                        (Some(joint), Some(weight)) => {
                            let joint_index = accessor_index(joint);
                            let weight_index = accessor_index(weight);
                            assert_eq!(gltf["accessors"][joint_index]["type"], "VEC4");
                            assert_eq!(gltf["accessors"][joint_index]["componentType"], 5123);
                            assert_eq!(gltf["accessors"][weight_index]["type"], "VEC4");
                            assert_eq!(gltf["accessors"][weight_index]["componentType"], 5126);
                            let (joint_values, joint_components) =
                                accessor(gltf, joint_index as u64, buffers);
                            let (weight_values, weight_components) =
                                accessor(gltf, weight_index as u64, buffers);
                            assert_eq!(joint_components, 4);
                            assert_eq!(weight_components, 4);
                            assert_eq!(joint_values.len(), position_count * 4);
                            assert_eq!(weight_values.len(), position_count * 4);
                            for joint in &joint_values {
                                assert_eq!(*joint, joint.floor());
                                assert!(*joint >= 0.0 && (*joint as usize) < joints_count);
                            }
                            for weights in weight_values.as_chunks::<4>().0 {
                                assert!(weights
                                    .iter()
                                    .all(|weight| (0.0..=1.0).contains(weight)));
                                assert!((weights.iter().sum::<f32>() - 1.0).abs() < 1e-5);
                            }
                            set += 1;
                            assert!(set <= 8, "too many joint sets");
                        }
                        _ => panic!(
                            "node {node_index} primitive {primitive_index}: incomplete joint set {set}"
                        ),
                    }
                }
                assert!(set > 0, "skinned primitive lacks JOINTS_0/WEIGHTS_0");
            }
        }
    }

    fn assert_animation_conformance(gltf: &Value, buffers: &[Vec<u8>]) {
        let animations = gltf["animations"].as_array().expect("animations");
        let nodes = gltf["nodes"].as_array().expect("nodes");
        for (animation_index, animation) in animations.iter().enumerate() {
            let samplers = animation["samplers"].as_array().expect("samplers");
            let channels = animation["channels"].as_array().expect("channels");
            assert!(!channels.is_empty());
            let mut input_counts = Vec::with_capacity(samplers.len());
            for (sampler_index, sampler) in samplers.iter().enumerate() {
                let input_index = accessor_index(sampler.get("input").expect("sampler input"));
                let input = &gltf["accessors"][input_index];
                assert_eq!(input["type"], "SCALAR");
                assert_eq!(input["componentType"], 5126);
                let (times, components) = accessor(gltf, input_index as u64, buffers);
                assert_eq!(components, 1);
                assert!(!times.is_empty());
                assert!(times[0] >= 0.0);
                assert!(times.windows(2).all(|pair| pair[0] < pair[1]));
                assert_accessor_bounds(
                    gltf,
                    input_index,
                    buffers,
                    &format!("animation {animation_index} sampler {sampler_index} input"),
                );
                let interpolation = sampler
                    .get("interpolation")
                    .and_then(Value::as_str)
                    .unwrap_or("LINEAR");
                assert!(matches!(interpolation, "LINEAR" | "STEP" | "CUBICSPLINE"));
                if interpolation == "CUBICSPLINE" {
                    assert!(times.len() >= 2, "CUBICSPLINE needs two keys");
                }
                input_counts.push(times.len());
            }
            for (channel_index, channel) in channels.iter().enumerate() {
                let sampler_index = channel["sampler"].as_u64().expect("channel sampler") as usize;
                let sampler = samplers
                    .get(sampler_index)
                    .unwrap_or_else(|| panic!("animation {animation_index}: sampler out of range"));
                let target = channel["target"].as_object().expect("channel target");
                let node_index = target
                    .get("node")
                    .and_then(Value::as_u64)
                    .expect("channel target node") as usize;
                assert!(node_index < nodes.len());
                let path = target["path"].as_str().expect("channel target path");
                let input_count = input_counts[sampler_index];
                let interpolation = sampler
                    .get("interpolation")
                    .and_then(Value::as_str)
                    .unwrap_or("LINEAR");
                let multiplier = if interpolation == "CUBICSPLINE" { 3 } else { 1 };
                let (expected_count, expected_type, components) = match path {
                    "translation" | "scale" => (input_count * multiplier, "VEC3", 3),
                    "rotation" => (input_count * multiplier, "VEC4", 4),
                    "weights" => {
                        let target_count =
                            node_mesh_target_count(gltf, node_index).unwrap_or_else(|| {
                                panic!("animation {animation_index}: weights node has no mesh")
                            });
                        assert!(target_count > 0);
                        (input_count * target_count * multiplier, "SCALAR", 1)
                    }
                    other => panic!("animation {animation_index}: bad path {other}"),
                };
                let output_index = accessor_index(sampler.get("output").expect("sampler output"));
                let output = &gltf["accessors"][output_index];
                assert_eq!(
                    output["type"], expected_type,
                    "animation {animation_index} channel {channel_index}"
                );
                assert_eq!(output["componentType"], 5126);
                assert_eq!(accessor_count(gltf, output_index), expected_count);
                let (values, output_components) = accessor(gltf, output_index as u64, buffers);
                assert_eq!(output_components, components);
                assert_eq!(values.len(), expected_count * components);
            }
        }
    }

    fn assert_gltf_conformance(gltf: &Value, buffers: &[Vec<u8>], animated: bool) {
        assert_eq!(gltf["asset"]["version"], "2.0");
        assert!(!gltf["nodes"].as_array().unwrap().is_empty());
        assert_morph_conformance(gltf, buffers);
        assert_skin_conformance(gltf, buffers);
        if animated {
            assert!(gltf.get("animations").is_some());
            assert_animation_conformance(gltf, buffers);
        } else {
            assert!(gltf.get("skins").is_none());
            assert!(gltf.get("animations").is_none());
            assert!(gltf["meshes"].as_array().unwrap().iter().all(|mesh| {
                mesh.get("extras").is_none()
                    && mesh["primitives"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|primitive| {
                            primitive.get("targets").is_none()
                                && !primitive["attributes"]
                                    .as_object()
                                    .unwrap()
                                    .contains_key("JOINTS_0")
                        })
            }));
        }
    }

    fn channel<'a>(gltf: &'a Value, node: usize, path: &str) -> &'a Value {
        let animation = &gltf["animations"][0];
        animation["channels"]
            .as_array()
            .unwrap()
            .iter()
            .find(|channel| {
                channel["target"]["node"].as_u64() == Some(node as u64)
                    && channel["target"]["path"].as_str() == Some(path)
            })
            .unwrap_or_else(|| panic!("missing {path} channel for node {node}"))
    }

    fn sampler_output<'a>(
        gltf: &'a Value,
        channel: &Value,
        buffers: &[Vec<u8>],
    ) -> (Vec<f32>, usize, &'a Value) {
        let animation = &gltf["animations"][0];
        let sampler_index = channel["sampler"].as_u64().unwrap() as usize;
        let sampler = &animation["samplers"][sampler_index];
        let (values, components) = accessor(gltf, sampler["output"].as_u64().unwrap(), buffers);
        (values, components, sampler)
    }

    #[test]
    fn exports_rigged_morphed_animated_glb() {
        let doc = rigged_document();
        let (gltf, bin) = export_glb_value(&doc);
        let buffers = vec![bin];
        let skins = gltf["skins"].as_array().unwrap();
        assert_eq!(skins.len(), 1);
        let skin = &skins[0];
        let skin_joints = skin["joints"].as_array().unwrap();
        assert_eq!(skin_joints.len(), 2);
        assert_eq!(
            skin_joints[0].as_u64().unwrap(),
            node_index(&gltf, "Root") as u64
        );
        assert_eq!(
            skin_joints[1].as_u64().unwrap(),
            node_index(&gltf, "Child") as u64
        );

        let (inverse_bind, components) = accessor(
            &gltf,
            skin["inverseBindMatrices"].as_u64().unwrap(),
            &buffers,
        );
        assert_eq!(components, 16);
        assert_eq!(inverse_bind.len(), 32);
        let inverse_bind_accessor =
            &gltf["accessors"][skin["inverseBindMatrices"].as_u64().unwrap() as usize];
        assert_eq!(inverse_bind_accessor["type"], "MAT4");
        assert_eq!(inverse_bind_accessor["count"], 2);
        let root_node = &gltf["nodes"][node_index(&gltf, "Root")];
        let child_node = &gltf["nodes"][node_index(&gltf, "Child")];
        assert_eq!(root_node["translation"], json!([0.0, 0.0, 0.0]));
        assert_eq!(child_node["translation"], json!([0.0, 1.0, 0.0]));
        let expected = doc
            .scene
            .get(doc.scene.find_by_name("MorphMesh").unwrap())
            .unwrap();
        for (joint, matrix) in expected
            .skin
            .as_ref()
            .unwrap()
            .inverse_bind
            .iter()
            .enumerate()
        {
            for (offset, expected) in matrix.to_cols_array().into_iter().enumerate() {
                assert!(
                    (inverse_bind[joint * 16 + offset] - expected).abs() < 1e-6,
                    "inverse bind mismatch for joint {joint}"
                );
            }
        }

        let mesh_node_index = node_index(&gltf, "MorphMesh");
        let mesh_node = &gltf["nodes"][mesh_node_index];
        assert_eq!(mesh_node["skin"].as_u64(), Some(0));
        assert_eq!(
            mesh_node["weights"].as_array().unwrap(),
            &vec![json!(0.25), json!(0.5)]
        );
        let mesh = &gltf["meshes"][mesh_node["mesh"].as_u64().unwrap() as usize];
        assert_eq!(
            mesh["extras"]["targetNames"].as_array().unwrap(),
            &vec![json!("Smile"), json!("Brow")]
        );
        let primitive = &mesh["primitives"][0];
        let attributes = primitive["attributes"].as_object().unwrap();
        assert!(attributes.contains_key("JOINTS_0"));
        assert!(attributes.contains_key("WEIGHTS_0"));
        assert_eq!(
            gltf["accessors"][attributes["JOINTS_0"].as_u64().unwrap() as usize]["componentType"],
            5123
        );
        assert_eq!(
            gltf["accessors"][attributes["JOINTS_0"].as_u64().unwrap() as usize]["type"],
            "VEC4"
        );
        assert_eq!(
            gltf["accessors"][attributes["WEIGHTS_0"].as_u64().unwrap() as usize]["type"],
            "VEC4"
        );
        let base_count = gltf["accessors"][attributes["POSITION"].as_u64().unwrap() as usize]
            ["count"]
            .as_u64()
            .unwrap() as usize;
        let (joints, joint_components) =
            accessor(&gltf, attributes["JOINTS_0"].as_u64().unwrap(), &buffers);
        assert_eq!(joint_components, 4);
        assert_eq!(joints.len(), base_count * 4);
        assert!(joints.iter().all(|joint| *joint < 2.0));
        let (weights, weight_components) =
            accessor(&gltf, attributes["WEIGHTS_0"].as_u64().unwrap(), &buffers);
        assert_eq!(weight_components, 4);
        assert_eq!(weights.len(), base_count * 4);
        assert!(joints
            .as_chunks::<4>()
            .0
            .iter()
            .zip(weights.as_chunks::<4>().0)
            .any(|(joints, weights)| joints == &[0.0, 1.0, 0.0, 0.0]
                && weights == &[0.75, 0.25, 0.0, 0.0]));
        for row in weights.as_chunks::<4>().0 {
            assert!(row.iter().all(|weight| (0.0..=1.0).contains(weight)));
            assert!((row.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        }

        let targets = primitive["targets"].as_array().unwrap();
        assert_eq!(targets.len(), 2);
        let (smile_positions, smile_components) =
            accessor(&gltf, targets[0]["POSITION"].as_u64().unwrap(), &buffers);
        assert_eq!(smile_components, 3);
        assert_eq!(smile_positions.len(), base_count * 3);
        assert!(smile_positions
            .iter()
            .skip(2)
            .step_by(3)
            .any(|z| (*z - 0.5).abs() < 1e-5));
        let smile_accessor = &gltf["accessors"][targets[0]["POSITION"].as_u64().unwrap() as usize];
        let min = smile_accessor["min"].as_array().unwrap();
        let max = smile_accessor["max"].as_array().unwrap();
        assert!((max[2].as_f64().unwrap() - 0.5).abs() < 1e-5);
        assert!((min[2].as_f64().unwrap() - 0.0).abs() < 1e-5);
        let (smile_normals, _) = accessor(&gltf, targets[0]["NORMAL"].as_u64().unwrap(), &buffers);
        assert!(smile_normals.iter().any(|normal| normal.abs() > 1e-4));
        let (brow_positions, _) =
            accessor(&gltf, targets[1]["POSITION"].as_u64().unwrap(), &buffers);
        assert!(brow_positions
            .iter()
            .step_by(3)
            .any(|x| (*x - 0.25).abs() < 1e-5));

        let animation = &gltf["animations"][0];
        assert_eq!(animation["name"], "Run");
        assert_eq!(animation["extras"]["leopardcraft"]["fps"], 10.0);
        assert_eq!(animation["extras"]["leopardcraft"]["take"], "Run");
        assert_eq!(gltf["extras"]["leopardcraft"]["active_take"], "Run");
        assert_eq!(gltf["extras"]["leopardcraft"]["fps"], 10.0);
        assert_eq!(animation["channels"].as_array().unwrap().len(), 4);
        assert_eq!(animation["samplers"].as_array().unwrap().len(), 4);
        let inputs: std::collections::BTreeSet<u64> = animation["samplers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|sampler| sampler["input"].as_u64().unwrap())
            .collect();
        assert_eq!(inputs.len(), 1, "channels must share one time base");
        let input = animation["samplers"][0]["input"].as_u64().unwrap();
        let input_accessor = &gltf["accessors"][input as usize];
        assert_eq!(input_accessor["min"][0].as_f64(), Some(0.0));
        assert_eq!(input_accessor["max"][0].as_f64(), Some(1.0));
        let (times, components) = accessor(&gltf, input, &buffers);
        assert_eq!(components, 1);
        assert!(times.first().is_some_and(|time| *time >= 0.0));
        assert!(times.windows(2).all(|pair| pair[0] < pair[1]));

        let root_index = node_index(&gltf, "Root");
        let translation = channel(&gltf, root_index, "translation");
        let (values, components, sampler) = sampler_output(&gltf, translation, &buffers);
        assert_eq!(components, 3);
        assert_eq!(sampler["interpolation"], "STEP");
        let last = values.len() - 3;
        assert!((values[last] - 1.5).abs() < 1e-5);
        let mid = (values.len() / 3) / 2 * 3;
        assert!(values[mid].abs() < 1e-5, "STEP should hold before the key");

        let child_index = node_index(&gltf, "Child");
        let rotation = channel(&gltf, child_index, "rotation");
        let (values, components, sampler) = sampler_output(&gltf, rotation, &buffers);
        assert_eq!(components, 4);
        assert_eq!(sampler["interpolation"], "LINEAR");
        let last = values.len() - 4;
        let expected = Quat::from_rotation_z(0.5);
        assert!((values[last] - expected.x).abs() < 1e-5);
        assert!((values[last + 1] - expected.y).abs() < 1e-5);
        assert!((values[last + 2] - expected.z).abs() < 1e-5);
        assert!((values[last + 3] - expected.w).abs() < 1e-5);

        let scale = channel(&gltf, child_index, "scale");
        let (values, components, _) = sampler_output(&gltf, scale, &buffers);
        assert_eq!(components, 3);
        let last = values.len() - 3;
        assert!((values[last + 1] - 1.5).abs() < 1e-5);

        let weights_channel = channel(&gltf, mesh_node_index, "weights");
        let (values, components, sampler) = sampler_output(&gltf, weights_channel, &buffers);
        assert_eq!(components, 1);
        assert_eq!(sampler["interpolation"], "LINEAR");
        assert_eq!(values.len(), times.len() * 2);
        assert!((values[0] - 0.0).abs() < 1e-5);
        assert!((values[1] - 0.5).abs() < 1e-5);
        let last = values.len() - 2;
        assert!((values[last] - 1.0).abs() < 1e-5);
        assert!((values[last + 1] - 0.5).abs() < 1e-5);

        let base_positions = accessor(&gltf, attributes["POSITION"].as_u64().unwrap(), &buffers).0;
        assert!(base_positions
            .as_chunks::<3>()
            .0
            .iter()
            .any(
                |position| (Vec3::from_slice(position) - Vec3::new(-1.0, 0.0, 0.0)).length() < 1e-5
            ));
        let position_accessor =
            &gltf["accessors"][attributes["POSITION"].as_u64().unwrap() as usize];
        assert!(position_accessor["min"].is_array());
        assert!(position_accessor["max"].is_array());
        assert_gltf_conformance(&gltf, &buffers, true);
    }

    #[test]
    fn exports_cubic_spline_translation_with_tangent_layout() {
        let mut doc = Document::empty();
        let id = doc.append_node(NodeKind::Null, "Animated Null", None);
        {
            let node = doc.scene.get_mut(id).unwrap();
            node.animation.set_key(
                ChannelAddress::owner_component(AnimChannel::Translation, 2),
                0.0,
                0.0,
                Interpolation::Smooth,
            );
            node.animation.set_key(
                ChannelAddress::owner_component(AnimChannel::Translation, 2),
                2.0,
                2.0,
                Interpolation::Smooth,
            );
        }
        let (gltf, bin) = export_glb_value(&doc);
        let buffers = vec![bin];
        let node = node_index(&gltf, "Animated Null");
        let channel = channel(&gltf, node, "translation");
        let (values, components, sampler) = sampler_output(&gltf, channel, &buffers);
        assert_eq!(components, 3);
        assert_eq!(sampler["interpolation"], "CUBICSPLINE");
        let input = sampler["input"].as_u64().unwrap();
        let (times, _) = accessor(&gltf, input, &buffers);
        let output = sampler["output"].as_u64().unwrap();
        assert_eq!(
            gltf["accessors"][output as usize]["count"]
                .as_u64()
                .unwrap() as usize,
            times.len() * 3
        );
        assert_eq!(values.len(), times.len() * 9);
        let last = values.len() - 9;
        assert!((values[last + 3 + 2] - 2.0).abs() < 1e-5);
        assert_gltf_conformance(&gltf, &buffers, true);
    }

    #[test]
    fn single_bezier_key_falls_back_to_a_valid_linear_sampler() {
        let mut doc = Document::empty();
        let id = doc.append_node(NodeKind::Null, "One Key", None);
        doc.scene.get_mut(id).unwrap().animation.set_key(
            ChannelAddress::owner_component(AnimChannel::Translation, 2),
            0.0,
            1.0,
            Interpolation::Smooth,
        );
        let (gltf, bin) = export_glb_value(&doc);
        let buffers = vec![bin];
        let channel = channel(&gltf, node_index(&gltf, "One Key"), "translation");
        let (values, _, sampler) = sampler_output(&gltf, channel, &buffers);
        assert_eq!(sampler["interpolation"], "LINEAR");
        assert_eq!(values.len(), 3);
        assert!((values[2] - 1.0).abs() < 1e-5);
        assert_gltf_conformance(&gltf, &buffers, true);
    }

    #[test]
    fn skins_and_animation_roundtrip_through_import() {
        let doc = rigged_document();
        let (gltf, bin) = export_glb_value(&doc);
        let imported = import_value_with_buffers(gltf, vec![bin]).unwrap();
        assert_eq!(imported.active_take().unwrap().name, "Run");
        assert!((imported.active_take_fps() - 10.0).abs() < 1e-5);
        let mesh_id = imported.scene.find_by_name("MorphMesh").unwrap();
        let node = imported.scene.get(mesh_id).unwrap();
        let skin = node.skin.as_ref().expect("skin was not imported");
        assert_eq!(skin.joints.len(), 2);
        assert_eq!(skin.inverse_bind.len(), 2);
        assert_eq!(skin.joint_indices.len(), skin.weights.len());
        for (joints, weights) in skin.joint_indices.iter().zip(&skin.weights) {
            let total: f32 = weights.iter().sum();
            assert!((total - 1.0).abs() < 1e-4, "weights {weights:?}");
            assert!(
                joints
                    .iter()
                    .all(|joint| (*joint as usize) < skin.joints.len()),
                "joint index out of range: {joints:?}"
            );
        }
        assert!(!node.animation.is_empty(), "animation was not imported");
        assert_eq!(node.morph.targets.len(), 2, "morph targets were lost");

        // The root translation is a Constant channel, so the sampled step keeps
        // its pre-key value until the second key.
        let root = imported.scene.find_by_name("Root").unwrap();
        let root_clip = &imported.scene.get(root).unwrap().animation;
        let address = ChannelAddress {
            target: ChannelTarget::Object { id: root },
            channel: AnimChannel::Translation,
            component: 0,
        };
        let curve = &root_clip.track(address).unwrap().curve;
        assert!((curve.sample(0.2) - 0.0).abs() < 1e-4);
        assert!((curve.sample(0.9) - 0.0).abs() < 1e-4);

        // Rotation arrives as a quaternion track: half a second into a one
        // second turn to 0.5 rad is about a quarter radian.
        let child = imported.scene.find_by_name("Child").unwrap();
        let child_clip = &imported.scene.get(child).unwrap().animation;
        let rotation = child_clip.sample_rotation(0.5).unwrap();
        assert!(
            rotation.dot(Quat::from_rotation_z(0.25)).abs() > 0.999,
            "rotation {rotation:?}"
        );

        // Morph-weight curves keep their target ordering.
        assert!((node.animation.sample_morph_weight(0, 0.5).unwrap() - 0.5).abs() < 0.02);

        // Seconds become a frame range at the imported take's fps.
        assert_eq!(imported.settings.frame_start, 0.0);
        assert!((imported.settings.frame_end - 10.0).abs() < 1e-4);
    }

    #[test]
    fn cubic_animation_import_keeps_hand_authored_tangents() {
        let mut doc = Document::empty();
        let id = doc.append_node(NodeKind::Null, "Smooth Null", None);
        {
            let clip = &mut doc.scene.get_mut(id).unwrap().animation;
            let address = ChannelAddress::owner_component(AnimChannel::Translation, 0);
            clip.set_key(address, 0.0, 0.0, Interpolation::Smooth);
            clip.set_key(address, 2.0, 2.0, Interpolation::Smooth);
        }
        let (gltf, bin) = export_glb_value(&doc);
        let imported = import_value_with_buffers(gltf, vec![bin]).unwrap();
        let id = imported.scene.find_by_name("Smooth Null").unwrap();
        let clip = &imported.scene.get(id).unwrap().animation;
        let address = ChannelAddress {
            target: ChannelTarget::Object { id },
            channel: AnimChannel::Translation,
            component: 0,
        };
        let curve = &clip.track(address).unwrap().curve;
        assert!(curve.keys.len() > 2);
        assert!(
            curve
                .keys
                .iter()
                .all(|key| key.interpolation == Interpolation::Spline),
            "CUBICSPLINE keys should import as Spline"
        );
        assert!(curve.keys[0].tangent_manual);
        assert!(curve.keys.iter().all(|key| key.tangent_in.is_finite()));
        assert!((curve.sample(1.0) - 1.0).abs() < 0.05);
    }

    #[test]
    fn skin_import_refuses_a_joint_index_outside_the_skin() {
        let doc = rigged_document();
        let (mut gltf, bin) = export_glb_value(&doc);
        let first_joint = gltf["skins"][0]["joints"][0].clone();
        gltf["skins"][0]["joints"] = json!([first_joint]);
        let inverse_bind = gltf["skins"][0]["inverseBindMatrices"].as_u64().unwrap();
        gltf["accessors"][inverse_bind as usize]["count"] = json!(1);
        let Err(error) = import_value_with_buffers_and_report(gltf, vec![bin]) else {
            panic!("a joint index outside the skin was accepted");
        };
        assert!(
            error.contains("JOINT index") && error.contains("outside skin"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn animation_import_refuses_a_missing_accessor() {
        let doc = rigged_document();
        let (mut gltf, bin) = export_glb_value(&doc);
        gltf["animations"][0]["samplers"][0]["input"] = json!(9999);
        let Err(error) = import_value_with_buffers_and_report(gltf, vec![bin]) else {
            panic!("an animation with a missing input accessor was accepted");
        };
        assert!(
            error.contains("missing accessor 9999"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn cameras_and_spot_lights_are_imported_while_skips_are_reported() {
        let doc = rigged_document();
        let (mut gltf, bin) = export_glb_value(&doc);
        // The exporter writes neither cameras nor lights, so add the nodes and
        // definitions a camera/light-bearing file carries (public glTF spec).
        gltf["cameras"] = json!([
            {"type": "perspective", "perspective": {"yfov": 0.7, "znear": 0.2, "zfar": 60.0, "aspectRatio": 1.5}},
            {"type": "orthographic", "orthographic": {"xmag": 2.0, "ymag": 3.0, "znear": 0.1, "zfar": 50.0}},
        ]);
        gltf["extensions"] = json!({"KHR_lights_punctual": {"lights": [
            {"type": "directional", "color": [1.0, 0.5, 0.25], "intensity": 4.5},
            {"type": "point", "color": [0.25, 0.5, 1.0], "intensity": 20.0, "range": 12.0},
            {"type": "spot", "color": [0.2, 1.0, 0.6], "intensity": 2.0, "range": 7.5, "spot": {"innerConeAngle": 0.3, "outerConeAngle": 0.5}},
        ]}});
        let added = [
            json!({"name": "Shot Camera", "camera": 0}),
            json!({"name": "Plan Camera", "camera": 1}),
            json!({"name": "Key Light", "extensions": {"KHR_lights_punctual": {"light": 0}}}),
            json!({"name": "Fill Light", "extensions": {"KHR_lights_punctual": {"light": 1}}}),
            json!({"name": "Spot Light", "extensions": {"KHR_lights_punctual": {"light": 2}}}),
        ];
        let nodes = gltf["nodes"].as_array_mut().expect("nodes");
        let first_index = nodes.len();
        let added_count = added.len();
        nodes.extend(added);
        let roots = gltf["scenes"][0]["nodes"]
            .as_array_mut()
            .expect("scene roots");
        for index in first_index..first_index + added_count {
            roots.push(json!(index));
        }

        let (imported, report) = import_value_with_buffers_and_report(gltf, vec![bin]).unwrap();
        assert_eq!(
            report.camera_aspect_ratios, 1,
            "camera aspect ratio not counted"
        );
        let summary = report.summary().expect("rewritten data without a message");
        for expected in ["1 camera aspect ratio", "set the output size instead"] {
            assert!(
                summary.contains(expected),
                "missing {expected:?}: {summary}"
            );
        }

        // Authored cameras are honoured, not reported as skipped.
        let camera_id = imported.scene.find_by_name("Shot Camera").unwrap();
        let NodeKind::Camera { camera } = &imported.scene.get(camera_id).unwrap().kind else {
            panic!("perspective camera imported as the wrong node kind");
        };
        assert!(
            (camera.fov_y_deg - 0.7_f32.to_degrees()).abs() < 1e-4,
            "fov {} != {} degrees",
            camera.fov_y_deg,
            0.7_f32.to_degrees()
        );
        assert!((camera.near - 0.2).abs() < 1e-5, "near clip lost");
        assert!((camera.far - 60.0).abs() < 1e-4, "far clip lost");
        let plan_id = imported.scene.find_by_name("Plan Camera").unwrap();
        let NodeKind::Camera { camera } = &imported.scene.get(plan_id).unwrap().kind else {
            panic!("orthographic camera imported as the wrong node kind");
        };
        assert_eq!(camera.projection, ProjectionMode::Orthographic);
        assert!(
            (camera.ortho_scale - 6.0).abs() < 1e-4,
            "ortho scale {} does not match ymag 3",
            camera.ortho_scale
        );

        // Authored lights are honoured too: type, colour and intensity.
        let key_id = imported.scene.find_by_name("Key Light").unwrap();
        let NodeKind::Light { light } = &imported.scene.get(key_id).unwrap().kind else {
            panic!("directional light imported as the wrong node kind");
        };
        assert_eq!(light.kind, LightKind::Directional);
        assert!((light.intensity - 4.5).abs() < 1e-5, "intensity lost");
        assert!(
            (light.color - Vec3::new(1.0, 0.5, 0.25)).length() < 1e-5,
            "colour {:?} lost",
            light.color
        );
        let fill_id = imported.scene.find_by_name("Fill Light").unwrap();
        let NodeKind::Light { light } = &imported.scene.get(fill_id).unwrap().kind else {
            panic!("point light imported as the wrong node kind");
        };
        let LightKind::Point { radius } = light.kind else {
            panic!("point light imported as {}", light.kind.label());
        };
        assert!(
            (radius - 12.0).abs() < 1e-5,
            "authored range {radius} is not the 12.0 the file declares"
        );
        assert!((light.intensity - 20.0).abs() < 1e-5);

        // A spot light keeps its authored cone and falloff distance.
        let spot_id = imported.scene.find_by_name("Spot Light").unwrap();
        let NodeKind::Light { light } = &imported.scene.get(spot_id).unwrap().kind else {
            panic!("spot light imported as the wrong node kind");
        };
        let LightKind::Spot {
            radius,
            inner_deg,
            outer_deg,
        } = light.kind
        else {
            panic!("spot light imported as {}", light.kind.label());
        };
        assert!(
            (radius - 7.5).abs() < 1e-5,
            "spot range {radius} is not the 7.5 the file declares"
        );
        assert!(
            (inner_deg - 0.3_f32.to_degrees()).abs() < 1e-4,
            "inner cone {inner_deg} != {} degrees",
            0.3_f32.to_degrees()
        );
        assert!(
            (outer_deg - 0.5_f32.to_degrees()).abs() < 1e-4,
            "outer cone {outer_deg} != {} degrees",
            0.5_f32.to_degrees()
        );
        assert!((light.intensity - 2.0).abs() < 1e-5, "spot intensity lost");
        assert!(
            (light.color - Vec3::new(0.2, 1.0, 0.6)).length() < 1e-5,
            "spot colour {:?} lost",
            light.color
        );

        // Geometry, morph targets, skinning and animation all arrive now.
        let id = imported.scene.find_by_name("MorphMesh").unwrap();
        let node = imported.scene.get(id).unwrap();
        assert_eq!(node.morph.targets.len(), 2, "morph targets were lost");
        let skin = node.skin.as_ref().expect("skin was not imported");
        assert_eq!(skin.joints.len(), 2);
        assert_eq!(skin.joint_indices.len(), skin.weights.len());
        assert!(!node.animation.is_empty(), "animation was not imported");
    }

    #[test]
    fn spot_light_import_refuses_an_inverted_cone() {
        let mut doc = Document::empty();
        doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::editable(lc_geom::box_mesh(1.0, 1.0, 1.0)),
            },
            "Box",
            None,
        );
        let (mut gltf, bin) = export_glb_value(&doc);
        gltf["extensions"] = json!({"KHR_lights_punctual": {"lights": [
            {"type": "spot", "spot": {"innerConeAngle": 0.9, "outerConeAngle": 0.4}},
        ]}});
        let nodes = gltf["nodes"].as_array_mut().expect("nodes");
        let index = nodes.len();
        nodes
            .push(json!({"name": "Bad Cone", "extensions": {"KHR_lights_punctual": {"light": 0}}}));
        gltf["scenes"][0]["nodes"]
            .as_array_mut()
            .expect("scene roots")
            .push(json!(index));
        let Err(error) = import_value_with_buffers_and_report(gltf, vec![bin]) else {
            panic!("an inverted spot cone was accepted");
        };
        assert!(
            error.contains("cone angles") && error.contains("90 degrees"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn camera_import_refuses_a_camera_without_projection_parameters() {
        let mut doc = Document::empty();
        doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::editable(lc_geom::box_mesh(1.0, 1.0, 1.0)),
            },
            "Box",
            None,
        );
        let (mut gltf, bin) = export_glb_value(&doc);
        gltf["cameras"] = json!([{"type": "perspective", "perspective": {"znear": 0.1}}]);
        let nodes = gltf["nodes"].as_array_mut().expect("nodes");
        let index = nodes.len();
        nodes.push(json!({"name": "Broken Camera", "camera": 0}));
        gltf["scenes"][0]["nodes"]
            .as_array_mut()
            .expect("scene roots")
            .push(json!(index));
        let Err(error) = import_value_with_buffers_and_report(gltf, vec![bin]) else {
            panic!("a camera without yfov was accepted");
        };
        assert!(
            error.contains("perspective.yfov"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn accessor_stride_smaller_than_its_element_is_rejected() {
        // A hand-edited file can declare a byteStride smaller than one VEC3
        // element. The last component read then runs past the buffer view and
        // used to panic instead of failing the import.
        let gltf = json!({
            "asset": {"version": "2.0"},
            "buffers": [{"byteLength": 12}],
            "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 12, "byteStride": 4}],
            "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}],
            "meshes": [{"name": "Tight", "primitives": [{"attributes": {"POSITION": 0}}]}],
            "nodes": [{"name": "Tight", "mesh": 0}],
            "scenes": [{"nodes": [0]}],
            "scene": 0
        });
        let Err(error) = import_value_with_buffers_and_report(gltf, vec![vec![0; 12]]) else {
            panic!("a short accessor stride was accepted");
        };
        assert!(error.contains("stride"), "unexpected error: {error}");
    }

    #[test]
    fn mesh_only_glb_imports_without_a_skip_report() {
        let mut doc = Document::empty();
        let mesh = lc_geom::box_mesh(1.0, 1.0, 1.0);
        let vertices = mesh.positions.len();
        doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::editable(mesh),
            },
            "Plain Box",
            None,
        );
        let (gltf, bin) = export_glb_value(&doc);
        let (imported, report) = import_value_with_buffers_and_report(gltf, vec![bin]).unwrap();
        assert!(
            report.is_empty(),
            "mesh-only file reported skips: {report:?}"
        );
        assert!(report.summary().is_none());
        let id = imported.scene.find_by_name("Plain Box").unwrap();
        let node = imported.scene.get(id).unwrap();
        let NodeKind::Mesh { geometry } = &node.kind else {
            panic!("plain mesh did not import as a mesh");
        };
        assert!(!geometry.mesh().unwrap().positions.is_empty());
        assert!(geometry.mesh().unwrap().positions.len() >= vertices);
    }

    #[test]
    fn imports_morph_targets_names_and_weights() {
        let mut doc = Document::empty();
        let mesh = lc_geom::box_mesh(1.0, 1.0, 1.0);
        let base = mesh.positions.clone();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::editable(mesh),
            },
            "Morph Import",
            None,
        );
        {
            let node = doc.scene.get_mut(id).unwrap();
            let mut lift = MorphTarget::new("Lift", vec![Vec3::Y; base.len()]);
            lift.weight = 0.25;
            let mut wide = MorphTarget::new("Wide", vec![Vec3::X * 0.5; base.len()]);
            wide.weight = 0.5;
            node.morph = MorphSet {
                base: base.clone(),
                targets: vec![lift, wide],
            };
        }
        let (gltf, bin) = export_glb_value(&doc);
        let imported = import_value_with_buffers(gltf, vec![bin]).unwrap();
        let id = imported.scene.find_by_name("Morph Import").unwrap();
        let node = imported.scene.get(id).unwrap();
        assert!(node.morph.has_base(), "imported node has no morph base");
        let names: Vec<&str> = node
            .morph
            .targets
            .iter()
            .map(|target| target.name.as_str())
            .collect();
        assert_eq!(names, ["Lift", "Wide"]);
        let weights: Vec<f32> = node
            .morph
            .targets
            .iter()
            .map(|target| target.weight)
            .collect();
        assert_eq!(weights, [0.25, 0.5]);
        let NodeKind::Mesh { geometry } = &node.kind else {
            panic!("imported morph source is not a mesh");
        };
        let mesh = geometry.mesh().expect("editable mesh");
        assert_eq!(node.morph.base.len(), mesh.positions.len());
        assert!(!node.morph.base.is_empty());
        for target in &node.morph.targets {
            assert_eq!(target.deltas.len(), mesh.positions.len());
        }
        assert!(
            node.morph.targets[0]
                .deltas
                .iter()
                .all(|delta| (*delta - Vec3::Y).length() < 1e-5),
            "lift deltas were not imported as offsets"
        );
        assert!(
            node.morph.targets[1]
                .deltas
                .iter()
                .all(|delta| (*delta - Vec3::X * 0.5).length() < 1e-5),
            "wide deltas were not imported as offsets"
        );
        let posed = imported.evaluate(0.0);
        let positions = &posed.node(id).unwrap().mesh().unwrap().positions;
        let expected = base[0] + Vec3::Y * 0.25 + Vec3::X * 0.5 * 0.5;
        assert!(
            (positions[0] - expected).length() < 1e-4,
            "posed {:?} expected {expected:?}",
            positions[0]
        );
    }

    #[test]
    fn morph_import_refuses_mismatched_weight_counts() {
        let mut doc = Document::empty();
        let mesh = lc_geom::box_mesh(1.0, 1.0, 1.0);
        let base = mesh.positions.clone();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::editable(mesh),
            },
            "Morph Weights",
            None,
        );
        {
            let node = doc.scene.get_mut(id).unwrap();
            let mut lift = MorphTarget::new("Lift", vec![Vec3::Y; base.len()]);
            lift.weight = 0.25;
            node.morph = MorphSet {
                base,
                targets: vec![lift],
            };
        }
        let (mut gltf, bin) = export_glb_value(&doc);
        let index = node_index(&gltf, "Morph Weights");
        gltf["nodes"][index]["weights"] = json!([0.25, 0.5]);
        let Err(error) = import_value_with_buffers(gltf, vec![bin]) else {
            panic!("mismatched morph weights were accepted");
        };
        assert!(
            error.contains("morph weights (2) do not match morph targets (1)"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn exports_cubic_spline_morph_weights_with_target_stride() {
        let mut doc = Document::empty();
        let mesh = lc_geom::box_mesh(1.0, 1.0, 1.0);
        let base = mesh.positions.clone();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::editable(mesh),
            },
            "Cubic Morph",
            None,
        );
        {
            let node = doc.scene.get_mut(id).unwrap();
            node.morph = MorphSet {
                base: base.clone(),
                targets: vec![
                    MorphTarget::new("Lift", vec![Vec3::Y; base.len()]),
                    MorphTarget::new("Wide", vec![Vec3::X * 0.5; base.len()]),
                ],
            };
            node.animation
                .set_morph_weight_key(0, 0.0, 0.0, Interpolation::Smooth);
            node.animation
                .set_morph_weight_key(0, 2.0, 1.0, Interpolation::Smooth);
            node.animation
                .set_morph_weight_key(1, 0.0, 1.0, Interpolation::Smooth);
            node.animation
                .set_morph_weight_key(1, 2.0, 0.0, Interpolation::Smooth);
        }
        let (gltf, bin) = export_glb_value(&doc);
        let buffers = vec![bin];
        let weights = channel(&gltf, node_index(&gltf, "Cubic Morph"), "weights");
        let (values, components, sampler) = sampler_output(&gltf, weights, &buffers);
        assert_eq!(components, 1);
        assert_eq!(sampler["interpolation"], "CUBICSPLINE");
        let input = sampler["input"].as_u64().unwrap();
        let (times, _) = accessor(&gltf, input, &buffers);
        assert!(times.len() >= 2);
        assert_eq!(
            gltf["accessors"][sampler["output"].as_u64().unwrap() as usize]["count"]
                .as_u64()
                .unwrap() as usize,
            times.len() * 2 * 3
        );
        assert_eq!(values.len(), times.len() * 2 * 3);
        assert_gltf_conformance(&gltf, &buffers, true);
    }

    #[test]
    fn morph_targets_are_skipped_when_a_modifier_stack_must_be_baked() {
        let mut doc = Document::empty();
        let mesh = lc_geom::box_mesh(1.0, 1.0, 1.0);
        let base = mesh.positions.clone();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::editable(mesh),
            },
            "Modified Morph",
            None,
        );
        {
            let node = doc.scene.get_mut(id).unwrap();
            node.morph = MorphSet {
                base: base.clone(),
                targets: vec![MorphTarget::new("Lift", vec![Vec3::Y; base.len()])],
            };
            node.modifiers.push(lc_scene::Modifier::Subdivision {
                levels: 1,
                scheme: lc_scene::SubdivisionScheme::CatmullClark,
            });
        }
        let (gltf, _) = export_glb_value(&doc);
        let mesh_node = &gltf["nodes"][node_index(&gltf, "Modified Morph")];
        assert!(mesh_node.get("weights").is_none());
        let mesh = &gltf["meshes"][mesh_node["mesh"].as_u64().unwrap() as usize];
        assert!(mesh.get("extras").is_none());
        assert!(mesh["primitives"][0].get("targets").is_none());
    }

    #[test]
    fn invalid_skin_and_stale_morph_are_skipped_without_panicking() {
        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::editable(lc_geom::box_mesh(1.0, 1.0, 1.0)),
            },
            "BrokenRig",
            None,
        );
        {
            let node = doc.scene.get_mut(id).unwrap();
            node.morph = MorphSet {
                base: vec![Vec3::ZERO],
                targets: vec![MorphTarget::new("Stale", vec![Vec3::ONE; 3])],
            };
            let mut skin = Skin::new(vec![999_999], vec![Mat4::IDENTITY]);
            skin.bind_all_to_first(1);
            node.skin = Some(skin);
        }
        let (gltf, _) = export_glb_value(&doc);
        assert!(gltf.get("skins").is_none());
        assert!(gltf.get("animations").is_none());
        let mesh_node = &gltf["nodes"][node_index(&gltf, "BrokenRig")];
        assert!(mesh_node.get("skin").is_none());
        assert!(mesh_node.get("weights").is_none());
        let mesh = &gltf["meshes"][mesh_node["mesh"].as_u64().unwrap() as usize];
        assert!(mesh.get("extras").is_none());
        let primitive = &mesh["primitives"][0];
        assert!(primitive.get("targets").is_none());
        assert!(!primitive["attributes"]
            .as_object()
            .unwrap()
            .contains_key("JOINTS_0"));
    }

    #[test]
    fn rig_free_export_keeps_the_static_mesh_shape() {
        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::editable(lc_geom::box_mesh(2.0, 1.0, 3.0)),
            },
            "Box",
            None,
        );
        doc.scene.get_mut(id).unwrap().transform =
            lc_core::Transform::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("static.gltf");
        export_gltf(&path, &doc).unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        let gltf: Value = serde_json::from_str(&text).unwrap();
        assert!(gltf.get("skins").is_none());
        assert!(gltf.get("animations").is_none());
        let mesh = &gltf["meshes"][0];
        assert!(mesh.get("extras").is_none());
        let primitive = &mesh["primitives"][0];
        assert!(primitive.get("targets").is_none());
        let attributes = primitive["attributes"].as_object().unwrap();
        assert_eq!(attributes.len(), 3);
        assert!(attributes.contains_key("POSITION"));
        assert!(attributes.contains_key("NORMAL"));
        assert!(attributes.contains_key("TEXCOORD_0"));
        assert!(!attributes.contains_key("JOINTS_0"));
        assert!(!attributes.contains_key("WEIGHTS_0"));
        let node = &gltf["nodes"][0];
        assert!(node.get("skin").is_none());
        assert!(node.get("weights").is_none());
        assert_eq!(node["translation"], json!([1.0, 2.0, 3.0]));
        let buffers = load_buffers(&gltf, None, dir.path()).unwrap();
        let (positions, _) = accessor(
            &gltf,
            primitive["attributes"]["POSITION"].as_u64().unwrap(),
            &buffers,
        );
        assert!(!positions.is_empty());
        assert!(positions.as_chunks::<3>().0.iter().any(|position| {
            (Vec3::from_slice(position) - Vec3::new(-1.0, -0.5, -1.5)).length() < 1e-5
        }));
        assert_gltf_conformance(&gltf, &buffers, false);
    }

    #[test]
    fn export_reports_and_skips_channels_without_a_gltf_path() {
        let mut doc = Document::empty();
        let id = doc.append_node(NodeKind::Null, "Node", None);
        let tag_address = ChannelAddress::new(
            ChannelTarget::Tag { node: id, index: 0 },
            AnimChannel::CopyLocationWeight,
            0,
        )
        .unwrap();
        doc.scene.get_mut(id).unwrap().animation.set_key(
            tag_address,
            0.0,
            1.0,
            Interpolation::Linear,
        );

        let report = animation_export_report(&doc);
        assert_eq!(report.skipped_channels.len(), 1);
        assert!(report.skipped_channels[0].contains("Node"));
        assert!(report
            .summary()
            .expect("skips are named")
            .contains("Copy Location Weight"));

        // The export still succeeds; a tag channel is not written as node TRS.
        let (gltf, _) = export_glb_value(&doc);
        assert!(gltf.get("animations").is_none());
    }

    #[test]
    fn glb_roundtrips_additional_uv_sets() {
        let mut doc = Document::empty();
        let mut mesh = lc_geom::box_mesh(1.0, 1.0, 1.0);
        lc_geom::box_projection(&mut mesh, 1.0);
        assert_eq!(mesh.add_uv_set(), Some(1));
        for slot in 0..mesh.uvs.len() as u32 {
            assert!(mesh.set_uv_value(1, slot, Vec2::new(0.125, 0.375)));
        }
        doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::editable(mesh),
            },
            "Two UV Sets",
            None,
        );
        let (gltf, bin) = export_glb_value(&doc);
        let primitive = &gltf["meshes"][0]["primitives"][0]["attributes"];
        assert!(primitive["TEXCOORD_0"].as_u64().is_some());
        assert!(
            primitive["TEXCOORD_1"].as_u64().is_some(),
            "a second UV set is written as TEXCOORD_1"
        );
        let (imported, _) = import_value_with_buffers_and_report(gltf, vec![bin]).unwrap();
        let id = imported.scene.find_by_name("Two UV Sets").unwrap();
        let NodeKind::Mesh { geometry } = &imported.scene.get(id).unwrap().kind else {
            panic!("expected a mesh");
        };
        let mesh = geometry.mesh().unwrap();
        assert_eq!(mesh.uv_set_count(), 2, "both sets survive the round trip");
        assert_eq!(mesh.extra_uvs[0].len(), mesh.uvs.len());
        assert!(mesh.extra_uvs[0]
            .iter()
            .all(|uv| (*uv - Vec2::new(0.125, 0.375)).length() < 1e-5));
        assert!(mesh.validate().is_ok());
    }

    #[test]
    fn explicitly_addressed_node_tracks_export_as_trs_channels() {
        let mut doc = Document::empty();
        let id = doc.append_node(NodeKind::Null, "Addressed", None);
        let address =
            ChannelAddress::new(ChannelTarget::Object { id }, AnimChannel::Translation, 0).unwrap();
        {
            let animation = &mut doc.scene.get_mut(id).unwrap().animation;
            animation.set_key(address, 0.0, 0.0, Interpolation::Linear);
            animation.set_key(address, 1.0, 1.0, Interpolation::Linear);
        }
        assert!(animation_export_report(&doc).is_empty());
        let (gltf, _) = export_glb_value(&doc);
        let channels = gltf["animations"][0]["channels"].as_array().unwrap();
        assert!(channels
            .iter()
            .any(|channel| channel["target"]["path"] == "translation"));
    }
}
