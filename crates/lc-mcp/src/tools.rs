//! MCP tool definitions and their LeopardCraft operations.

use crate::server::McpServer;
use glam::{EulerRot, Quat, Vec3};
use lc_core::Transform;
use lc_scene::{
    AddNodeCmd, Command, DeleteNodeCmd, Document, GeometrySource, NodeId, NodeKind, NodeProps,
    Primitive, RenameNodeCmd, SetNodePropsCmd, SetTransformCmd,
};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub(crate) fn exists(name: &str) -> bool {
    matches!(
        name,
        "get_scene"
            | "list_objects"
            | "get_object"
            | "create_primitive"
            | "set_transform"
            | "rename_object"
            | "delete_object"
            | "set_object_properties"
            | "list_materials"
            | "undo"
            | "redo"
            | "save_scene"
            | "open_scene"
            | "new_scene"
            | "export_scene"
            | "render_image"
    )
}

pub(crate) fn list_result() -> Value {
    let mut result = json!({ "tools": definitions() });
    if let Some(object) = result.as_object_mut() {
        object.insert("ttlMs".into(), json!(0));
        object.insert("cacheScope".into(), json!("private"));
    }
    result
}

pub(crate) fn call(
    server: &mut McpServer,
    name: &str,
    args: &Map<String, Value>,
) -> Result<Value, String> {
    match name {
        "get_scene" => Ok(server.summary_payload()),
        "list_objects" => server.list_objects(),
        "get_object" => {
            let id = server.resolve_object(args, "object")?;
            server.object_payload(id)
        }
        "create_primitive" => server.create_primitive(args),
        "set_transform" => server.set_transform(args),
        "rename_object" => server.rename_object(args),
        "delete_object" => server.delete_object(args),
        "set_object_properties" => server.set_object_properties(args),
        "list_materials" => Ok(server.list_materials()),
        "undo" => server.undo(),
        "redo" => server.redo(),
        "save_scene" => server.save_scene(args),
        "open_scene" => server.open_scene(args),
        "new_scene" => server.new_scene(args),
        "export_scene" => server.export_scene(args),
        "render_image" => server.render_image(args),
        other => Err(format!("Unknown tool: {other}")),
    }
}

fn definitions() -> Vec<Value> {
    vec![
        tool(
            "get_scene",
            "Scene summary",
            "Summarize the current document: object counts, world bounds, active camera, \
             render settings, material count and undo state. Start here for an overview.",
            schema(json!({}), &[]),
            true,
            false,
        ),
        tool(
            "list_objects",
            "List objects",
            "List every object in depth-first hierarchy order with its id, name, type, \
             parent, children, visibility and material.",
            schema(json!({}), &[]),
            true,
            false,
        ),
        tool(
            "get_object",
            "Inspect object",
            "Return one object's transform, hierarchy, flags, material, tags, modifiers, \
             animation state and geometry statistics.",
            schema(json!({ "object": object_property() }), &["object"]),
            true,
            false,
        ),
        tool(
            "create_primitive",
            "Create primitive",
            "Create a box, sphere, cylinder, capsule, disc, cone, torus or plane mesh object. \
             Parameters default to the editor toolbar values. Undoable.",
            schema(
                json!({
                    "primitive": {
                        "type": "string",
                        "enum": ["box", "sphere", "cylinder", "capsule", "disc", "cone", "torus", "plane"],
                        "description": "Primitive shape to create."
                    },
                    "name": {
                        "type": "string",
                        "description": "Object name; defaults to the primitive label and is made unique if needed."
                    },
                    "parent": object_property(),
                    "translation": vec3_schema("Local translation in metres."),
                    "rotation_degrees": vec3_schema("Local XYZ Euler rotation in degrees."),
                    "scale": vec3_schema("Local scale; components must be non-zero."),
                    "width": { "type": "number", "description": "Box or plane width." },
                    "height": { "type": "number", "description": "Box, cylinder, capsule or cone height." },
                    "depth": { "type": "number", "description": "Box or plane depth." },
                    "radius": { "type": "number", "description": "Sphere, cylinder, capsule, disc or cone radius." },
                    "major_radius": { "type": "number", "description": "Torus ring radius." },
                    "minor_radius": { "type": "number", "description": "Torus tube radius." },
                    "segments": { "type": "integer", "minimum": 3, "maximum": 256, "description": "Radial segment count." },
                    "rings": { "type": "integer", "minimum": 2, "maximum": 128, "description": "Sphere, capsule or torus ring count." },
                    "segments_x": { "type": "integer", "minimum": 1, "maximum": 256, "description": "Plane segments along X." },
                    "segments_z": { "type": "integer", "minimum": 1, "maximum": 256, "description": "Plane segments along Z." }
                }),
                &["primitive"],
            ),
            false,
            false,
        ),
        tool(
            "set_transform",
            "Set transform",
            "Set an object's local translation, XYZ Euler rotation in degrees and/or scale. \
             Omitted components keep their current values. Undoable.",
            schema(
                json!({
                    "object": object_property(),
                    "translation": vec3_schema("Local translation in metres."),
                    "rotation_degrees": vec3_schema("Local XYZ Euler rotation in degrees."),
                    "scale": vec3_schema("Local scale; components must be non-zero.")
                }),
                &["object"],
            ),
            false,
            false,
        ),
        tool(
            "rename_object",
            "Rename object",
            "Rename an object. Names must be unique. Undoable.",
            schema(
                json!({
                    "object": object_property(),
                    "name": { "type": "string", "description": "New unique object name." }
                }),
                &["object", "name"],
            ),
            false,
            false,
        ),
        tool(
            "delete_object",
            "Delete object",
            "Delete an object together with its whole subtree. Undoable.",
            schema(json!({ "object": object_property() }), &["object"]),
            false,
            true,
        ),
        tool(
            "set_object_properties",
            "Edit object properties",
            "Change visibility, renderability, lock state or the primary material. Pass \
             material 'none' to clear the assignment. Undoable.",
            schema(
                json!({
                    "object": object_property(),
                    "visible": { "type": "boolean", "description": "Viewport visibility." },
                    "renderable": { "type": "boolean", "description": "Include the object in renders." },
                    "locked": { "type": "boolean", "description": "Prevent editing in the editor." },
                    "material": { "type": "string", "description": "Material id or exact name, or 'none'." }
                }),
                &["object"],
            ),
            false,
            false,
        ),
        tool(
            "list_materials",
            "List materials",
            "List the material library with ids, names and shadow/double-sided flags.",
            schema(json!({}), &[]),
            true,
            false,
        ),
        tool(
            "undo",
            "Undo",
            "Undo the most recent scene edit and report the command label.",
            schema(json!({}), &[]),
            false,
            false,
        ),
        tool(
            "redo",
            "Redo",
            "Redo the most recently undone scene edit and report the command label.",
            schema(json!({}), &[]),
            false,
            false,
        ),
        tool(
            "save_scene",
            "Save scene",
            "Save the document as .lcscene JSON and clear the dirty flag. Uses the loaded \
             scene path when 'path' is omitted; saves elsewhere are an ordinary Save As.",
            schema(
                json!({
                    "path": { "type": "string", "description": "Target .lcscene path." }
                }),
                &[],
            ),
            false,
            false,
        ),
        tool(
            "open_scene",
            "Open scene",
            "Load a .lcscene, OBJ, STL or glTF/GLB file, replacing the document and clearing \
             undo history.",
            schema(
                json!({
                    "path": { "type": "string", "description": "Scene or model file to load." }
                }),
                &["path"],
            ),
            false,
            true,
        ),
        tool(
            "new_scene",
            "New scene",
            "Replace the document with the built-in studio or an empty scene. Clears the \
             path and undo history.",
            schema(
                json!({
                    "template": {
                        "type": "string",
                        "enum": ["studio", "empty"],
                        "default": "studio",
                        "description": "Scene template."
                    }
                }),
                &[],
            ),
            false,
            true,
        ),
        tool(
            "export_scene",
            "Export scene",
            "Export the evaluated document to OBJ, STL, glTF/GLB or native .lcscene. Does not \
             change the document path or dirty flag.",
            schema(
                json!({
                    "path": { "type": "string", "description": "Target .obj, .stl, .glb, .gltf, .lcscene or .json path." }
                }),
                &["path"],
            ),
            false,
            false,
        ),
        tool(
            "render_image",
            "Render image",
            "Path-trace the current scene with the active camera to a .png or .exr file. Uses \
             document render settings unless width, height, samples, seed or time override them.",
            schema(
                json!({
                    "path": { "type": "string", "description": "Target .png or .exr path." },
                    "width": { "type": "integer", "minimum": 16, "maximum": 8192 },
                    "height": { "type": "integer", "minimum": 16, "maximum": 8192 },
                    "samples": { "type": "integer", "minimum": 1, "maximum": 4096 },
                    "seed": { "type": "integer", "minimum": 0 },
                    "time": { "type": "number", "minimum": 0, "description": "Animation time in seconds." }
                }),
                &["path"],
            ),
            false,
            false,
        ),
    ]
}

fn tool(
    name: &str,
    title: &str,
    description: &str,
    input_schema: Value,
    read_only: bool,
    destructive: bool,
) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": input_schema,
        "annotations": {
            "readOnlyHint": read_only,
            "destructiveHint": destructive,
            "idempotentHint": read_only,
            "openWorldHint": false
        }
    })
}

fn schema(properties: Value, required: &[&str]) -> Value {
    let mut schema = json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false
    });
    if !required.is_empty() {
        schema["required"] = json!(required);
    }
    schema
}

fn object_property() -> Value {
    json!({
        "type": "string",
        "description": "Object id or exact object name."
    })
}

fn vec3_schema(description: &str) -> Value {
    json!({
        "type": "array",
        "items": { "type": "number" },
        "minItems": 3,
        "maxItems": 3,
        "description": description
    })
}

impl McpServer {
    pub(crate) fn summary_payload(&self) -> Value {
        let evaluated = self.doc.evaluate(0.0);
        let mut nulls = 0usize;
        let mut meshes = 0usize;
        let mut splines = 0usize;
        let mut cameras = 0usize;
        let mut lights = 0usize;
        let mut joints = 0usize;
        for node in self.doc.scene.nodes.values() {
            match &node.kind {
                NodeKind::Null => nulls += 1,
                NodeKind::Mesh { .. } => meshes += 1,
                NodeKind::Spline { .. } => splines += 1,
                NodeKind::Camera { .. } => cameras += 1,
                NodeKind::Light { .. } => lights += 1,
                NodeKind::Joint { .. } => joints += 1,
            }
        }
        let active_camera = self
            .doc
            .active_camera_id()
            .and_then(|id| self.doc.node(id))
            .map(|node| json!({ "id": node.id, "name": node.name }));
        let bounds = if evaluated.bounds.is_valid() {
            json!({
                "min": vec3_array(evaluated.bounds.min),
                "max": vec3_array(evaluated.bounds.max)
            })
        } else {
            Value::Null
        };
        let render = &self.doc.settings.render;
        json!({
            "document": {
                "path": self.path.as_ref().map(|path| path.display().to_string()),
                "dirty": self.dirty,
                "format": self.doc.format,
                "version": self.doc.version
            },
            "objects": {
                "total": self.doc.scene.len(),
                "nulls": nulls,
                "meshes": meshes,
                "splines": splines,
                "cameras": cameras,
                "lights": lights,
                "joints": joints
            },
            "triangles": evaluated.triangle_count(),
            "bounds": bounds,
            "active_camera": active_camera,
            "materials": self.doc.materials.len(),
            "settings": {
                "units": self.doc.settings.units,
                "fps": self.doc.settings.fps,
                "frame_start": self.doc.settings.frame_start,
                "frame_end": self.doc.settings.frame_end,
                "background": vec3_array(self.doc.settings.background),
                "environment": self.doc.settings.environment.label(),
                "render": {
                    "width": render.width,
                    "height": render.height,
                    "samples": render.samples,
                    "max_bounces": render.max_bounces,
                    "exposure": render.exposure,
                    "transparent_background": render.transparent_background,
                    "seed": render.seed
                }
            },
            "undo": {
                "depth": self.history.depth(),
                "can_undo": self.history.can_undo(),
                "undo_label": self.history.undo_label(),
                "can_redo": self.history.can_redo(),
                "redo_label": self.history.redo_label()
            }
        })
    }

    fn list_objects(&self) -> Result<Value, String> {
        let objects: Vec<Value> = self
            .doc
            .scene
            .depth_first()
            .into_iter()
            .filter_map(|id| self.doc.node(id))
            .map(|node| {
                json!({
                    "id": node.id,
                    "name": node.name,
                    "type": node.kind.label(),
                    "kind": kind_name(&node.kind),
                    "parent": node.parent,
                    "children": node.children,
                    "visible": node.visible,
                    "renderable": node.renderable,
                    "locked": node.locked,
                    "material": material_json(&self.doc, node.material)
                })
            })
            .collect();
        Ok(json!({ "count": objects.len(), "objects": objects }))
    }

    fn object_payload(&self, id: NodeId) -> Result<Value, String> {
        let node = self
            .doc
            .node(id)
            .ok_or_else(|| format!("object {id} no longer exists"))?;
        let tags: Vec<&str> = node.tags.iter().map(|tag| tag.label()).collect();
        let modifiers = serde_json::to_value(&node.modifiers).unwrap_or_else(|_| json!([]));
        let skin = node
            .skin
            .as_ref()
            .map(|skin| {
                json!({
                    "joints": skin.joints.len(),
                    "weighted_vertices": skin.vertex_count()
                })
            })
            .unwrap_or(Value::Null);
        let morph_targets: Vec<&str> = node
            .morph
            .targets
            .iter()
            .map(|target| target.name.as_str())
            .collect();
        Ok(json!({
            "id": node.id,
            "name": node.name,
            "type": node.kind.label(),
            "kind": kind_name(&node.kind),
            "parent": node.parent,
            "children": node.children,
            "transform": transform_json(&node.transform),
            "visible": node.visible,
            "renderable": node.renderable,
            "locked": node.locked,
            "layer": node.layer,
            "material": material_json(&self.doc, node.material),
            "material_slots": node.material_slots,
            "tags": tags,
            "modifiers": modifiers,
            "animation": { "animated": !node.animation.is_empty() },
            "skin": skin,
            "morph_targets": morph_targets,
            "geometry": kind_details(&node.kind)
        }))
    }

    fn create_primitive(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let primitive_name = required_str(args, "primitive")?.trim().to_ascii_lowercase();
        let primitive = primitive_from_args(&primitive_name, args)?;
        let name = match optional_str(args, "name")? {
            Some(name) if name.trim().is_empty() => return Err("'name' must not be empty".into()),
            Some(name) => name.trim().to_string(),
            None => primitive.label().to_string(),
        };
        let parent = match optional_str(args, "parent")? {
            Some(raw) => Some(self.resolve_object_text(raw)?),
            None => None,
        };
        let transform = transform_from_args(args, Transform::IDENTITY)?;
        let mut node = self.doc.create_node(
            NodeKind::Mesh {
                geometry: GeometrySource::from_primitive(primitive),
            },
            &name,
        );
        node.transform = transform;
        let id = node.id;
        let command = Box::new(AddNodeCmd::new(&self.doc, node, parent));
        self.execute(command);
        self.object_payload(id)
    }

    fn set_transform(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = self.resolve_object(args, "object")?;
        let before = self
            .doc
            .node(id)
            .ok_or_else(|| format!("no object with id {id}"))?
            .transform;
        let after = transform_from_args(args, before)?;
        if after == before {
            return Err("transform is unchanged".into());
        }
        let command = Box::new(
            SetTransformCmd::new(&self.doc, id, after)
                .ok_or_else(|| format!("no object with id {id}"))?,
        );
        self.execute(command);
        self.object_payload(id)
    }

    fn rename_object(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = self.resolve_object(args, "object")?;
        let name = required_str(args, "name")?.trim().to_string();
        if name.is_empty() {
            return Err("'name' must not be empty".into());
        }
        let current = self
            .doc
            .node(id)
            .ok_or_else(|| format!("no object with id {id}"))?
            .name
            .clone();
        if current == name {
            return Err(format!("object is already named '{name}'"));
        }
        if self.doc.scene.find_by_name(&name).is_some() {
            return Err(format!("an object named '{name}' already exists"));
        }
        let command = Box::new(
            RenameNodeCmd::new(&self.doc, id, name)
                .ok_or_else(|| format!("no object with id {id}"))?,
        );
        self.execute(command);
        self.object_payload(id)
    }

    fn delete_object(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = self.resolve_object(args, "object")?;
        let name = self
            .doc
            .node(id)
            .ok_or_else(|| format!("no object with id {id}"))?
            .name
            .clone();
        let command = Box::new(DeleteNodeCmd::new(&self.doc, id));
        self.execute(command);
        Ok(json!({
            "deleted": { "id": id, "name": name },
            "remaining": self.doc.scene.len()
        }))
    }

    fn set_object_properties(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = self.resolve_object(args, "object")?;
        let node = self
            .doc
            .node(id)
            .ok_or_else(|| format!("no object with id {id}"))?;
        let mut properties = NodeProps::of(node);
        let mut changed = false;
        if let Some(visible) = optional_bool(args, "visible")? {
            if properties.visible != visible {
                properties.visible = visible;
                changed = true;
            }
        }
        if let Some(renderable) = optional_bool(args, "renderable")? {
            if properties.renderable != renderable {
                properties.renderable = renderable;
                changed = true;
            }
        }
        if let Some(locked) = optional_bool(args, "locked")? {
            if properties.locked != locked {
                properties.locked = locked;
                changed = true;
            }
        }
        if let Some(raw) = optional_str(args, "material")? {
            let material = if raw.trim().eq_ignore_ascii_case("none") {
                None
            } else {
                Some(self.resolve_material(raw.trim())?)
            };
            if properties.material != material {
                properties.material = material;
                changed = true;
            }
        }
        if !changed {
            return Err(
                "no property changes requested (visible, renderable, locked or material)".into(),
            );
        }
        let command = Box::new(
            SetNodePropsCmd::new(&self.doc, id, properties)
                .ok_or_else(|| format!("no object with id {id}"))?,
        );
        self.execute(command);
        self.object_payload(id)
    }

    fn list_materials(&self) -> Value {
        let materials: Vec<Value> = self
            .doc
            .materials
            .iter()
            .map(|material| {
                json!({
                    "id": material.id,
                    "name": material.name,
                    "double_sided": material.double_sided,
                    "casts_shadow": material.casts_shadow
                })
            })
            .collect();
        json!({ "materials": materials })
    }

    fn undo(&mut self) -> Result<Value, String> {
        let label = self
            .history
            .undo(&mut self.doc)
            .ok_or_else(|| "nothing to undo".to_string())?;
        self.dirty = true;
        Ok(json!({
            "undone": label,
            "can_undo": self.history.can_undo(),
            "undo_label": self.history.undo_label(),
            "can_redo": self.history.can_redo(),
            "redo_label": self.history.redo_label()
        }))
    }

    fn redo(&mut self) -> Result<Value, String> {
        let label = self
            .history
            .redo(&mut self.doc)
            .ok_or_else(|| "nothing to redo".to_string())?;
        self.dirty = true;
        Ok(json!({
            "redone": label,
            "can_undo": self.history.can_undo(),
            "undo_label": self.history.undo_label(),
            "can_redo": self.history.can_redo(),
            "redo_label": self.history.redo_label()
        }))
    }

    fn save_scene(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let path = match optional_str(args, "path")? {
            Some(raw) => PathBuf::from(raw),
            None => self
                .path
                .clone()
                .ok_or_else(|| "this scene has no path yet; pass 'path'".to_string())?,
        };
        let ext = extension(&path);
        if ext != "lcscene" && ext != "json" {
            return Err(format!(
                "save_scene writes .lcscene files; use export_scene for '.{ext}'"
            ));
        }
        lc_scene::save_native(&path, &self.doc)?;
        self.path = Some(path.clone());
        self.dirty = false;
        Ok(json!({
            "path": path.display().to_string(),
            "objects": self.doc.scene.len()
        }))
    }

    fn open_scene(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let path = PathBuf::from(required_str(args, "path")?);
        let (doc, report) = lc_io::import_with_report(&path)?;
        let _ = lc_scene::take_warnings();
        self.doc = doc;
        self.path = Some(path);
        self.history.clear();
        self.dirty = false;
        let mut payload = self.summary_payload();
        // A lossy glTF import must not look complete to an agent either.
        if let Some(summary) = report.summary() {
            payload["import_report"] = json!(summary);
        }
        Ok(payload)
    }

    fn new_scene(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let template = optional_str(args, "template")?
            .unwrap_or("studio")
            .trim()
            .to_ascii_lowercase();
        self.doc = match template.as_str() {
            "studio" | "demo" => Document::demo(),
            "empty" => Document::empty(),
            other => {
                return Err(format!(
                    "unknown template '{other}'; use 'studio' or 'empty'"
                ))
            }
        };
        self.path = None;
        self.history.clear();
        self.dirty = true;
        Ok(self.summary_payload())
    }

    fn export_scene(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let path = PathBuf::from(required_str(args, "path")?);
        let format = extension(&path);
        if !matches!(
            format.as_str(),
            "lcscene" | "json" | "obj" | "stl" | "glb" | "gltf"
        ) {
            return Err(format!(
                "unsupported export format '.{format}'; use .obj, .stl, .glb, .gltf, .lcscene or .json"
            ));
        }
        lc_io::export(&path, &self.doc)?;
        Ok(json!({
            "path": path.display().to_string(),
            "format": format,
            "objects": self.doc.scene.len()
        }))
    }

    fn render_image(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let path = PathBuf::from(required_str(args, "path")?);
        let format = extension(&path);
        if format != "png" && format != "exr" {
            return Err(format!(
                "render_image writes .png or .exr files, not '.{format}'"
            ));
        }
        let render = &self.doc.settings.render;
        let width = u32_arg(args, "width", render.width, 16, 8192)?;
        let height = u32_arg(args, "height", render.height, 16, 8192)?;
        let samples = u32_arg(args, "samples", render.samples, 1, 4096)?;
        let seed = u64_arg(args, "seed", render.seed)?;
        let time = f32_arg(args, "time", 0.0)?;
        if time < 0.0 {
            return Err("'time' must not be negative".into());
        }
        let base_dir = self
            .path
            .as_ref()
            .and_then(|path| path.parent())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let scene = lc_render::RenderScene::from_document(&self.doc, time, base_dir)?;
        let mut tracer = lc_render::PathTracer::new(width, height, seed);
        tracer.render_pass(&scene, samples);
        if format == "exr" {
            lc_render::save_linear_exr(&path, &tracer.hdr_pixels(), width, height)?;
        } else {
            lc_render::save_tracer_png(
                &path,
                &tracer,
                render.exposure,
                render.transparent_background,
            )?;
        }
        Ok(json!({
            "path": path.display().to_string(),
            "format": format,
            "width": width,
            "height": height,
            "samples": samples,
            "seed": seed,
            "time": time,
            "triangles": scene.triangle_count()
        }))
    }

    fn resolve_object(&self, args: &Map<String, Value>, key: &str) -> Result<NodeId, String> {
        let raw = args
            .get(key)
            .ok_or_else(|| format!("missing required argument '{key}'"))?;
        let text = match raw {
            Value::String(text) => text.clone(),
            Value::Number(number) => number.to_string(),
            _ => return Err(format!("'{key}' must be an object id or exact name")),
        };
        if text.trim().is_empty() {
            return Err(format!("'{key}' must not be empty"));
        }
        self.resolve_object_text(&text)
    }

    fn resolve_object_text(&self, text: &str) -> Result<NodeId, String> {
        let text = text.trim();
        if let Ok(id) = text.parse::<NodeId>() {
            if self.doc.node(id).is_some() {
                return Ok(id);
            }
        }
        if let Some(id) = self.doc.scene.find_by_name(text) {
            return Ok(id);
        }
        let lowered = text.to_ascii_lowercase();
        let matches: Vec<NodeId> = self
            .doc
            .scene
            .nodes
            .values()
            .filter(|node| node.name.to_ascii_lowercase() == lowered)
            .map(|node| node.id)
            .collect();
        match matches.as_slice() {
            [id] => Ok(*id),
            [] => Err(format!("no object with id or name '{text}'")),
            _ => Err(format!(
                "object name '{text}' is ambiguous; pass the numeric id"
            )),
        }
    }

    fn resolve_material(&self, text: &str) -> Result<u32, String> {
        if let Ok(id) = text.parse::<u32>() {
            if self.doc.materials.get(id).is_some() {
                return Ok(id);
            }
        }
        if let Some(material) = self
            .doc
            .materials
            .iter()
            .find(|material| material.name == text)
        {
            return Ok(material.id);
        }
        let lowered = text.to_ascii_lowercase();
        let matches: Vec<u32> = self
            .doc
            .materials
            .iter()
            .filter(|material| material.name.to_ascii_lowercase() == lowered)
            .map(|material| material.id)
            .collect();
        match matches.as_slice() {
            [id] => Ok(*id),
            [] => Err(format!("no material with id or name '{text}'")),
            _ => Err(format!(
                "material name '{text}' is ambiguous; pass the numeric id"
            )),
        }
    }

    fn execute(&mut self, command: Box<dyn Command>) -> String {
        self.dirty = true;
        self.history.execute(&mut self.doc, command)
    }
}

fn kind_name(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Null => "null",
        NodeKind::Mesh { .. } => "mesh",
        NodeKind::Spline { .. } => "spline",
        NodeKind::Camera { .. } => "camera",
        NodeKind::Light { .. } => "light",
        NodeKind::Joint { .. } => "joint",
    }
}

fn kind_details(kind: &NodeKind) -> Value {
    match kind {
        NodeKind::Null => Value::Null,
        NodeKind::Mesh { geometry } => {
            let built = geometry.mesh().is_none().then(|| geometry.build());
            let mesh = geometry
                .mesh()
                .or(built.as_ref())
                .expect("a mesh is either stored or built");
            let mut payload = json!({
                "source": if geometry.primitive().is_some() { "primitive" } else { "editable" },
                "vertices": mesh.positions.len(),
                "polygons": mesh.polygons.len(),
                "triangles": mesh.triangle_count(),
                "has_uvs": !mesh.uvs.is_empty(),
                "creases": mesh.creases.len()
            });
            if let Some(primitive) = geometry.primitive() {
                payload["primitive"] = serde_json::to_value(primitive).unwrap_or(Value::Null);
            }
            if let Some(origin) = geometry.origin_copied() {
                payload["origin"] = serde_json::to_value(origin).unwrap_or(Value::Null);
            }
            payload
        }
        NodeKind::Spline { spline } => json!({
            "control_points": spline.points.len(),
            "closed": spline.closed,
            "spline_kind": serde_json::to_value(spline.kind).unwrap_or(Value::Null)
        }),
        NodeKind::Camera { camera } => json!({
            "camera": serde_json::to_value(camera).unwrap_or(Value::Null)
        }),
        NodeKind::Light { light } => json!({
            "light": serde_json::to_value(light).unwrap_or(Value::Null)
        }),
        NodeKind::Joint { length, width } => json!({
            "joint": { "length": length, "width": width }
        }),
    }
}

fn material_json(doc: &Document, id: Option<u32>) -> Value {
    match id {
        Some(id) => json!({
            "id": id,
            "name": doc.materials.get(id).map(|material| material.name.clone())
        }),
        None => Value::Null,
    }
}

fn transform_json(transform: &Transform) -> Value {
    let euler = transform.euler_degrees();
    json!({
        "translation": vec3_array(transform.translation),
        "rotation_degrees": vec3_array(euler),
        "scale": vec3_array(transform.scale)
    })
}

fn vec3_array(vector: Vec3) -> Value {
    json!([vector.x, vector.y, vector.z])
}

fn primitive_from_args(name: &str, args: &Map<String, Value>) -> Result<Primitive, String> {
    match name {
        "box" => Ok(Primitive::Box {
            width: positive(args, "width", 1.0)?,
            height: positive(args, "height", 1.0)?,
            depth: positive(args, "depth", 1.0)?,
        }),
        "sphere" => Ok(Primitive::Sphere {
            radius: positive(args, "radius", 1.0)?,
            segments: u32_arg(args, "segments", 32, 3, 256)?,
            rings: u32_arg(args, "rings", 16, 2, 128)?,
        }),
        "cylinder" => Ok(Primitive::Cylinder {
            radius: positive(args, "radius", 0.7)?,
            height: positive(args, "height", 2.0)?,
            segments: u32_arg(args, "segments", 32, 3, 256)?,
        }),
        "capsule" => Ok(Primitive::Capsule {
            radius: positive(args, "radius", 0.5)?,
            height: positive(args, "height", 2.0)?,
            segments: u32_arg(args, "segments", 32, 3, 256)?,
            rings: u32_arg(args, "rings", 8, 2, 128)?,
        }),
        "disc" => Ok(Primitive::Disc {
            radius: positive(args, "radius", 1.0)?,
            segments: u32_arg(args, "segments", 32, 3, 256)?,
        }),
        "cone" => Ok(Primitive::Cone {
            radius: positive(args, "radius", 0.8)?,
            height: positive(args, "height", 2.0)?,
            segments: u32_arg(args, "segments", 32, 3, 256)?,
        }),
        "torus" => Ok(Primitive::Torus {
            major_radius: positive(args, "major_radius", 1.0)?,
            minor_radius: positive(args, "minor_radius", 0.28)?,
            segments: u32_arg(args, "segments", 40, 3, 256)?,
            rings: u32_arg(args, "rings", 16, 2, 128)?,
        }),
        "plane" => Ok(Primitive::Plane {
            width: positive(args, "width", 4.0)?,
            depth: positive(args, "depth", 4.0)?,
            segments_x: u32_arg(args, "segments_x", 1, 1, 256)?,
            segments_z: u32_arg(args, "segments_z", 1, 1, 256)?,
        }),
        other => Err(format!(
            "unknown primitive '{other}'; use box, sphere, cylinder, capsule, disc, cone, torus or plane"
        )),
    }
}

fn transform_from_args(
    args: &Map<String, Value>,
    fallback: Transform,
) -> Result<Transform, String> {
    let mut transform = fallback;
    if let Some(translation) = vec3_arg(args, "translation")? {
        transform.translation = translation;
    }
    if let Some(rotation) = vec3_arg(args, "rotation_degrees")? {
        transform.rotation = Quat::from_euler(
            EulerRot::XYZ,
            rotation.x.to_radians(),
            rotation.y.to_radians(),
            rotation.z.to_radians(),
        );
    }
    if let Some(scale) = vec3_arg(args, "scale")? {
        if scale.x == 0.0 || scale.y == 0.0 || scale.z == 0.0 {
            return Err("'scale' components must be non-zero".into());
        }
        transform.scale = scale;
    }
    Ok(transform)
}

fn required_str<'a>(args: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    match args.get(key) {
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(value),
        Some(Value::String(_)) => Err(format!("'{key}' must not be empty")),
        Some(_) => Err(format!("'{key}' must be a string")),
        None => Err(format!("missing required argument '{key}'")),
    }
}

fn optional_str<'a>(args: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(format!("'{key}' must be a string")),
    }
}

fn optional_bool(args: &Map<String, Value>, key: &str) -> Result<Option<bool>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(format!("'{key}' must be a boolean")),
    }
}

fn number_arg(args: &Map<String, Value>, key: &str) -> Result<Option<f64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_f64()
            .filter(|value| value.is_finite())
            .map(Some)
            .ok_or_else(|| format!("'{key}' must be a finite number")),
        Some(_) => Err(format!("'{key}' must be a number")),
    }
}

fn f32_arg(args: &Map<String, Value>, key: &str, fallback: f32) -> Result<f32, String> {
    match number_arg(args, key)? {
        None => Ok(fallback),
        Some(value) => {
            let value = value as f32;
            if value.is_finite() {
                Ok(value)
            } else {
                Err(format!("'{key}' is outside the supported number range"))
            }
        }
    }
}

fn u32_arg(
    args: &Map<String, Value>,
    key: &str,
    fallback: u32,
    min: u32,
    max: u32,
) -> Result<u32, String> {
    match number_arg(args, key)? {
        None => Ok(fallback),
        Some(value) => {
            if value.fract() != 0.0 || value < min as f64 || value > max as f64 {
                return Err(format!(
                    "'{key}' must be a whole number between {min} and {max}"
                ));
            }
            Ok(value as u32)
        }
    }
}

fn u64_arg(args: &Map<String, Value>, key: &str, fallback: u64) -> Result<u64, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(fallback),
        Some(Value::Number(number)) => number
            .as_u64()
            .ok_or_else(|| format!("'{key}' must be a non-negative integer")),
        Some(_) => Err(format!("'{key}' must be a non-negative integer")),
    }
}

fn vec3_arg(args: &Map<String, Value>, key: &str) -> Result<Option<Vec3>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(values)) => {
            if values.len() != 3 {
                return Err(format!("'{key}' must contain exactly three numbers"));
            }
            let mut components = [0.0f32; 3];
            for (index, value) in values.iter().enumerate() {
                components[index] = component(value, key)?;
            }
            Ok(Some(Vec3::from_array(components)))
        }
        Some(Value::Object(object)) => {
            let x = object
                .get("x")
                .ok_or_else(|| format!("'{key}' object needs x, y and z"))?;
            let y = object
                .get("y")
                .ok_or_else(|| format!("'{key}' object needs x, y and z"))?;
            let z = object
                .get("z")
                .ok_or_else(|| format!("'{key}' object needs x, y and z"))?;
            Ok(Some(Vec3::new(
                component(x, key)?,
                component(y, key)?,
                component(z, key)?,
            )))
        }
        Some(_) => Err(format!(
            "'{key}' must be [x, y, z] or an object with x, y and z"
        )),
    }
}

fn component(value: &Value, key: &str) -> Result<f32, String> {
    let number = value
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| format!("'{key}' components must be finite numbers"))?;
    let value = number as f32;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!(
            "'{key}' components are outside the supported number range"
        ))
    }
}

fn positive(args: &Map<String, Value>, key: &str, fallback: f32) -> Result<f32, String> {
    let value = f32_arg(args, key, fallback)?;
    if value <= 0.0 {
        return Err(format!("'{key}' must be greater than zero"));
    }
    Ok(value)
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Map<String, Value> {
        serde_json::from_str(text).expect("valid argument object")
    }

    #[test]
    fn out_of_range_numbers_do_not_become_infinite_transforms_or_primitives() {
        // `1e39` is finite in JSON/f64 but overflows f32. It must be rejected
        // instead of reaching a transform scale or a primitive dimension.
        match transform_from_args(&args(r#"{"scale":[1e39,1,1]}"#), Transform::IDENTITY) {
            Ok(transform) => panic!(
                "set_transform accepted a scale that became {:?}",
                transform.scale
            ),
            Err(error) => assert!(error.contains("scale"), "{error}"),
        }
        match primitive_from_args("box", &args(r#"{"width":1e39}"#)) {
            Ok(primitive) => panic!(
                "create_primitive accepted a width that builds {:?}",
                primitive.build().positions.first()
            ),
            Err(error) => assert!(error.contains("width"), "{error}"),
        }
    }
}
