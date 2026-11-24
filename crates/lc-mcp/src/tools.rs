//! MCP tool definitions and their LeopardCraft operations.

use crate::server::McpServer;
use glam::{EulerRot, Quat, Vec2, Vec3};
use lc_anim::{AnimChannel, ChannelTarget, Interpolation};
use lc_core::Transform;
use lc_geom::{
    align_uv_slots, automatic_unwrap, bevel_faces, box_projection_in_set, bridge_loops,
    collapse_points, cover_faces, create_polygon_faces, cylindrical_projection_in_set,
    disconnect_faces, distribute_uv_slots, extrude_faces, fill_holes, flip_faces, flip_uv_slots,
    inset_faces, normalize_uvs, open_holes, optimize_points, pack_uv_charts,
    planar_projection_in_set, relax_uvs, rotate_uv_slots, slide_points,
    spherical_projection_in_set, split_uv_slots, transform_points, transform_uv_charts,
    triangulate_faces_with, uv_charts, weld_uv_slots, Mesh, TriangulateMode, UvAxis, UvPackAlign,
    UvPackOptions,
};
use lc_scene::{
    AddMaterialCmd, AddNodeCmd, Command, DeleteNodeCmd, Document, GeometrySource, Modifier, NodeId,
    NodeKind, NodeProps, Primitive, RenameNodeCmd, SetAnimationCmd, SetMaterialCmd,
    SetNodePropsCmd, SetShapeCmd, SetTakesCmd, SetTransformCmd, Shape,
};
use lc_shading::{Material, ShaderGraph};
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
            | "add_modifier"
            | "update_modifier"
            | "remove_modifier"
            | "move_modifier"
            | "get_material"
            | "create_material"
            | "update_material"
            | "get_animation"
            | "set_animation_key"
            | "remove_animation_key"
            | "create_take"
            | "activate_take"
            | "update_take"
            | "delete_take"
            | "get_mesh"
            | "edit_mesh"
            | "edit_uv"
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
        "add_modifier" => server.add_modifier(args),
        "update_modifier" => server.update_modifier(args),
        "remove_modifier" => server.remove_modifier(args),
        "move_modifier" => server.move_modifier(args),
        "get_material" => server.get_material(args),
        "create_material" => server.create_material(args),
        "update_material" => server.update_material(args),
        "get_animation" => server.get_animation(args),
        "set_animation_key" => server.set_animation_key(args),
        "remove_animation_key" => server.remove_animation_key(args),
        "create_take" => server.create_take(args),
        "activate_take" => server.activate_take(args),
        "update_take" => server.update_take(args),
        "delete_take" => server.delete_take(args),
        "get_mesh" => server.get_mesh(args),
        "edit_mesh" => server.edit_mesh(args),
        "edit_uv" => server.edit_uv(args),
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
            "add_modifier",
            "Add modifier",
            "Append a modifier to a mesh or spline object's non-destructive stack, or insert \
             it at 'index'. The modifier object uses the same snake_case 'type' and fields \
             shown by get_object. Undoable.",
            schema(
                json!({
                    "object": object_property(),
                    "modifier": modifier_property(),
                    "index": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Insertion position; defaults to the end of the stack."
                    }
                }),
                &["object", "modifier"],
            ),
            false,
            false,
        ),
        tool(
            "update_modifier",
            "Update modifier",
            "Replace one entry in an object's modifier stack by index. The modifier object uses \
             the same snake_case 'type' and fields shown by get_object. Undoable.",
            schema(
                json!({
                    "object": object_property(),
                    "index": { "type": "integer", "minimum": 0, "description": "Stack position." },
                    "modifier": modifier_property()
                }),
                &["object", "index", "modifier"],
            ),
            false,
            false,
        ),
        tool(
            "remove_modifier",
            "Remove modifier",
            "Remove one entry from an object's modifier stack by index and return the removed \
             modifier. Undoable.",
            schema(
                json!({
                    "object": object_property(),
                    "index": { "type": "integer", "minimum": 0, "description": "Stack position." }
                }),
                &["object", "index"],
            ),
            false,
            true,
        ),
        tool(
            "move_modifier",
            "Move modifier",
            "Move one modifier to another position in the stack. Both indices are checked \
             before anything changes. Undoable.",
            schema(
                json!({
                    "object": object_property(),
                    "from": { "type": "integer", "minimum": 0, "description": "Current stack position." },
                    "to": { "type": "integer", "minimum": 0, "description": "New stack position." }
                }),
                &["object", "from", "to"],
            ),
            false,
            false,
        ),
        tool(
            "get_material",
            "Inspect material",
            "Return one material's name, flags, animation clip and complete shader graph, \
             including node ids, kinds, positions, inputs and links. Use this as the template \
             for update_material.",
            schema(json!({ "material": material_property() }), &["material"]),
            true,
            false,
        ),
        tool(
            "create_material",
            "Create material",
            "Create a material in the library with a Principled graph, or replace that graph \
             with a validated 'graph' object. Names must be unique. Undoable.",
            schema(
                json!({
                    "name": { "type": "string", "description": "Unique material name." },
                    "base_color": vec3_schema("Principled base colour; defaults to 0.8 grey."),
                    "roughness": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "metallic": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "double_sided": { "type": "boolean", "default": true },
                    "casts_shadow": { "type": "boolean", "default": true },
                    "graph": shader_graph_property()
                }),
                &["name"],
            ),
            false,
            false,
        ),
        tool(
            "update_material",
            "Update material",
            "Change a material's name, double-sided/shadow flags and/or complete shader graph. \
             The graph must satisfy the same node/link invariants as get_material. Undoable.",
            schema(
                json!({
                    "material": material_property(),
                    "name": { "type": "string", "description": "New unique material name." },
                    "double_sided": { "type": "boolean" },
                    "casts_shadow": { "type": "boolean" },
                    "graph": shader_graph_property()
                }),
                &["material"],
            ),
            false,
            false,
        ),
        tool(
            "get_animation",
            "Inspect animation",
            "Return the active take and an object's complete animation clip, plus the take \
             list. Useful before keying or inspecting existing channels.",
            schema(json!({ "object": object_property() }), &["object"]),
            true,
            false,
        ),
        tool(
            "set_animation_key",
            "Set animation key",
            "Insert or replace one key on an object's active-take clip. Scalar channels use \
             'component' and 'value'; rotation uses a four-number xyzw 'rotation'; morph \
             weight uses 'morph_target'. Undoable.",
            schema(
                json!({
                    "object": object_property(),
                    "channel": animation_channel_property(),
                    "component": { "type": "integer", "minimum": 0, "maximum": 3 },
                    "time": { "type": "number", "minimum": 0.0, "description": "Seconds." },
                    "value": { "type": "number" },
                    "rotation": {
                        "type": "array",
                        "items": { "type": "number" },
                        "minItems": 4,
                        "maxItems": 4,
                        "description": "Quaternion xyzw; normalized before storing."
                    },
                    "morph_target": { "type": "integer", "minimum": 0 },
                    "interpolation": {
                        "type": "string",
                        "enum": ["constant", "linear", "smooth", "spline"],
                        "default": "smooth"
                    }
                }),
                &["object", "channel", "time"],
            ),
            false,
            false,
        ),
        tool(
            "remove_animation_key",
            "Remove animation key",
            "Remove the nearest key at 'time' from one object channel in the active take. \
             The addressing fields match set_animation_key. Undoable.",
            schema(
                json!({
                    "object": object_property(),
                    "channel": animation_channel_property(),
                    "component": { "type": "integer", "minimum": 0, "maximum": 3 },
                    "time": { "type": "number", "minimum": 0.0 },
                    "morph_target": { "type": "integer", "minimum": 0 }
                }),
                &["object", "channel", "time"],
            ),
            false,
            true,
        ),
        tool(
            "create_take",
            "Create take",
            "Create a new empty take from an optional name base, make it active and clear the \
             live clips. Undoable.",
            schema(
                json!({
                    "name": { "type": "string", "description": "Base name; made unique." }
                }),
                &[],
            ),
            false,
            false,
        ),
        tool(
            "activate_take",
            "Activate take",
            "Activate a take by index, storing the current live clips first. Undoable.",
            schema(
                json!({
                    "index": { "type": "integer", "minimum": 0, "description": "Zero-based take index." }
                }),
                &["index"],
            ),
            false,
            false,
        ),
        tool(
            "update_take",
            "Update take",
            "Rename a take and/or set its fps in one undo step. Omitted fields keep their \
             values; duplicates, empty names and non-finite fps are refused.",
            schema(
                json!({
                    "index": { "type": "integer", "minimum": 0, "description": "Defaults to the active take." },
                    "name": { "type": "string" },
                    "fps": { "type": "number", "minimum": 1.0, "maximum": 240.0 }
                }),
                &[],
            ),
            false,
            false,
        ),
        tool(
            "delete_take",
            "Delete take",
            "Delete a take by index. Deleting the active take activates a neighbour; deleting \
             the last take creates a fresh empty take. Undoable.",
            schema(
                json!({
                    "index": { "type": "integer", "minimum": 0, "description": "Defaults to the active take." }
                }),
                &[],
            ),
            false,
            true,
        ),
        tool(
            "get_mesh",
            "Inspect mesh",
            "Return an object's editable mesh and topology summary: positions, polygons \
             (with point/UV-slot corners), UV sets, UV seams, pins and chart count. Primitive \
             meshes report their generated topology and are converted by the first edit_mesh \
             or edit_uv call.",
            schema(json!({ "object": object_property() }), &["object"]),
            true,
            false,
        ),
        tool(
            "edit_mesh",
            "Edit mesh components",
            "Apply one direct-modelling operation to explicit component indices. Selection is \
             per call: omitted 'faces' or 'points' selects all components of that kind, while \
             an empty array is refused. Supported operations: transform, extrude, inset, bevel, \
             flip, triangulate, optimize, slide, fill_holes, create_polygon, disconnect, cover, \
             collapse and bridge. Converts a primitive to an editable mesh in the same undo \
             entry. Undoable.",
            schema(
                json!({
                    "object": object_property(),
                    "operation": {
                        "type": "string",
                        "enum": [
                            "transform", "extrude", "inset", "bevel", "flip", "triangulate",
                            "optimize", "slide", "fill_holes", "create_polygon", "disconnect",
                            "cover", "collapse", "bridge"
                        ]
                    },
                    "faces": indices_schema("Polygon indices; omitted means all faces."),
                    "points": indices_schema("Point indices; omitted means all points."),
                    "loops": {
                        "type": "array",
                        "description": "Exactly two point-index loops for bridge.",
                        "items": {
                            "type": "array",
                            "items": { "type": "integer", "minimum": 0 },
                            "minItems": 3
                        },
                        "minItems": 2,
                        "maxItems": 2
                    },
                    "translation": vec3_schema("World-space component translation."),
                    "rotation_degrees": vec3_schema("XYZ Euler rotation around the selection centroid."),
                    "scale": vec3_schema("Per-axis scale around the selection centroid; components must be non-zero."),
                    "distance": { "type": "number", "description": "Extrude distance." },
                    "individual": { "type": "boolean", "default": false },
                    "amount": { "type": "number", "description": "Inset width or bevel amount." },
                    "depth": { "type": "number", "description": "Bevel depth; defaults to 0." },
                    "mode": { "type": "string", "enum": ["shortest_diagonal", "quad_pair"], "default": "shortest_diagonal" },
                    "tolerance": { "type": "number", "minimum": 0.0, "description": "Optimize weld tolerance." },
                    "fraction": { "type": "number", "description": "Point-slide fraction." }
                }),
                &["object", "operation"],
            ),
            false,
            false,
        ),
        tool(
            "edit_uv",
            "Edit UVs",
            "Apply one UV operation to an object. Selection is per call: omitted or empty \
             'charts' selects every chart, while the slot operations need an explicit \
             non-empty 'slots' list. Indices are validated before the edit. Supported \
             operations: project, \
             unwrap, relax, normalize, add_set, remove_set, set_uv, set_pinned, set_seam, \
             clear_seams, pack, transform_charts, weld, split, align, distribute, flip and \
             rotate. Converts a primitive in the same undo entry. Undoable.",
            schema(
                json!({
                    "object": object_property(),
                    "operation": {
                        "type": "string",
                        "enum": [
                            "project", "unwrap", "relax", "normalize", "add_set", "remove_set",
                            "set_uv", "set_pinned", "set_seam", "clear_seams", "pack",
                            "transform_charts", "weld", "split", "align", "distribute", "flip",
                            "rotate"
                        ]
                    },
                    "uv_set": { "type": "integer", "minimum": 0, "default": 0 },
                    "mode": { "type": "string", "enum": ["planar", "box", "cylindrical", "spherical"] },
                    "axis": { "type": "integer", "minimum": 0, "maximum": 2, "description": "Planar projection axis or UV U=0/V=1/both=2 axis." },
                    "scale": { "type": "number", "default": 1.0 },
                    "height_scale": { "type": "number", "default": 1.0 },
                    "max_angle_deg": { "type": "number", "minimum": 0.0, "maximum": 180.0, "default": 66.0 },
                    "padding": { "type": "number", "minimum": 0.0 },
                    "iterations": { "type": "integer", "minimum": 1, "maximum": 1000, "default": 10 },
                    "strength": { "type": "number", "minimum": 0.0, "maximum": 1.0, "default": 1.0 },
                    "pin_boundary": { "type": "boolean", "default": true },
                    "slot": { "type": "integer", "minimum": 0 },
                    "u": { "type": "number" },
                    "v": { "type": "number" },
                    "pinned": { "type": "boolean", "default": true },
                    "a": { "type": "integer", "minimum": 0 },
                    "b": { "type": "integer", "minimum": 0 },
                    "seam": { "type": "boolean", "default": true },
                    "charts": indices_schema("UV chart indices; omitted/empty means all for pack."),
                    "slots": indices_schema("UV slot indices."),
                    "translation": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2 },
                    "rotation_degrees": { "type": "number", "default": 0.0 },
                    "rotate": { "type": "boolean", "default": false },
                    "align": { "type": "string", "enum": ["bottom_left", "top_left", "center"], "default": "bottom_left" },
                    "quarter_turns": { "type": "integer", "description": "Clockwise 90-degree steps for rotate." }
                }),
                &["object", "operation"],
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

fn indices_schema(description: &str) -> Value {
    json!({
        "type": "array",
        "description": description,
        "items": { "type": "integer", "minimum": 0 }
    })
}

fn optional_u32s(args: &Map<String, Value>, key: &str) -> Result<Option<Vec<u32>>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(values)) => {
            let mut out = Vec::with_capacity(values.len());
            for value in values {
                let index = value
                    .as_u64()
                    .ok_or_else(|| format!("'{key}' must contain non-negative whole numbers"))?;
                out.push(
                    u32::try_from(index)
                        .map_err(|_| format!("'{key}' index {index} is too large"))?,
                );
            }
            out.sort_unstable();
            out.dedup();
            Ok(Some(out))
        }
        Some(_) => Err(format!("'{key}' must be an array of indices")),
    }
}

/// Read an optional index array and range-check it. An explicit empty array is
/// refused so "nothing selected" is never a silent no-op; `None` means the
/// argument was absent.
fn index_selection(
    args: &Map<String, Value>,
    key: &str,
    what: &str,
    len: usize,
) -> Result<Option<Vec<u32>>, String> {
    let Some(selected) = optional_u32s(args, key)? else {
        return Ok(None);
    };
    if selected.is_empty() {
        return Err(format!("'{key}' must not be empty"));
    }
    if let Some(bad) = selected.iter().find(|index| **index as usize >= len) {
        return Err(format!(
            "{what} {bad} is outside the {len} {what}(s) on the mesh"
        ));
    }
    Ok(Some(selected))
}

/// Face indices, defaulting to every face.
fn face_selection(mesh: &Mesh, args: &Map<String, Value>) -> Result<Vec<u32>, String> {
    if mesh.polygons.is_empty() {
        return Err("the mesh has no faces".into());
    }
    Ok(index_selection(args, "faces", "face", mesh.polygons.len())?
        .unwrap_or_else(|| (0..mesh.polygons.len() as u32).collect()))
}

/// Point indices, defaulting to every point.
fn point_selection(mesh: &Mesh, args: &Map<String, Value>) -> Result<Vec<u32>, String> {
    if mesh.positions.is_empty() {
        return Err("the mesh has no points".into());
    }
    Ok(
        index_selection(args, "points", "point", mesh.positions.len())?
            .unwrap_or_else(|| (0..mesh.positions.len() as u32).collect()),
    )
}

/// UV slot indices. Unlike faces and points there is no "every slot" default,
/// because welding or rotating every slot at once would rewrite the whole layout.
fn slot_selection(mesh: &Mesh, args: &Map<String, Value>) -> Result<Vec<u32>, String> {
    index_selection(args, "slots", "UV slot", mesh.uvs.len())?
        .ok_or_else(|| "missing required argument 'slots'".to_string())
}

/// Points behind a transform selection: an explicit point list, or the points of
/// the selected faces. An edge selection is just its two points, so the tool
/// keeps one selection vocabulary instead of a third, edge-specific mode.
fn transform_selection(mesh: &Mesh, args: &Map<String, Value>) -> Result<Vec<u32>, String> {
    let mut points =
        if let Some(points) = index_selection(args, "points", "point", mesh.positions.len())? {
            points
        } else if let Some(faces) = index_selection(args, "faces", "face", mesh.polygons.len())? {
            faces
                .iter()
                .flat_map(|face| mesh.polygons[*face as usize].corners.iter())
                .map(|corner| corner.point)
                .collect()
        } else {
            return Err("transform needs a non-empty 'points' or 'faces' selection".into());
        };
    points.sort_unstable();
    points.dedup();
    Ok(points)
}

fn edge_exists(mesh: &Mesh, a: u32, b: u32) -> bool {
    mesh.edges()
        .iter()
        .any(|edge| (edge.a == a && edge.b == b) || (edge.a == b && edge.b == a))
}

fn point_loops(args: &Map<String, Value>) -> Result<[Vec<u32>; 2], String> {
    let loops = args
        .get("loops")
        .and_then(Value::as_array)
        .filter(|loops| loops.len() == 2)
        .ok_or_else(|| "bridge needs exactly two 'loops'".to_string())?;
    let mut parsed: [Vec<u32>; 2] = [Vec::new(), Vec::new()];
    for (index, loop_value) in loops.iter().enumerate() {
        let loop_array = loop_value
            .as_array()
            .filter(|points| points.len() >= 3)
            .ok_or_else(|| "each bridge loop needs at least three points".to_string())?;
        for point in loop_array {
            let value = point
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| "bridge loop points must be non-negative integers".to_string())?;
            parsed[index].push(value);
        }
    }
    Ok(parsed)
}

fn triangulate_mode(args: &Map<String, Value>) -> Result<TriangulateMode, String> {
    match optional_str(args, "mode")?.unwrap_or("shortest_diagonal") {
        "shortest_diagonal" => Ok(TriangulateMode::ShortestDiagonal),
        "quad_pair" => Ok(TriangulateMode::QuadPair),
        other => Err(format!("unknown triangulate mode '{other}'")),
    }
}

fn pack_align(args: &Map<String, Value>) -> Result<UvPackAlign, String> {
    match optional_str(args, "align")?.unwrap_or("bottom_left") {
        "bottom_left" => Ok(UvPackAlign::BottomLeft),
        "top_left" => Ok(UvPackAlign::TopLeft),
        "center" => Ok(UvPackAlign::Center),
        other => Err(format!("unknown pack alignment '{other}'")),
    }
}

fn uv_axis(args: &Map<String, Value>) -> Result<UvAxis, String> {
    match optional_index(args, "axis")?.unwrap_or(2) {
        0 => Ok(UvAxis::U),
        1 => Ok(UvAxis::V),
        2 => Ok(UvAxis::Both),
        other => Err(format!("axis {other} must be 0 (U), 1 (V) or 2 (both)")),
    }
}

/// UV chart indices for `pack` and `transform_charts`, defaulting to every
/// chart. Charts are derived from the mesh, so the "every chart" default is
/// expanded once here; the geometry helpers otherwise disagree about what an
/// empty chart list means.
fn chart_selection(mesh: &Mesh, args: &Map<String, Value>) -> Result<Vec<usize>, String> {
    let count = uv_charts(mesh).len();
    let selected = optional_u32s(args, "charts")?.unwrap_or_default();
    if selected.is_empty() {
        return Ok((0..count).collect());
    }
    if let Some(bad) = selected.iter().find(|chart| **chart as usize >= count) {
        return Err(format!(
            "chart {bad} is outside the {count} chart(s) on the mesh"
        ));
    }
    Ok(selected.into_iter().map(|chart| chart as usize).collect())
}

/// Refuse an operation on a UV set the mesh does not have. Projection and
/// packing skip a missing set silently, which would read as a successful no-op.
fn require_uv_set(mesh: &Mesh, set: usize) -> Result<(), String> {
    if mesh.uv_set(set).is_some() {
        return Ok(());
    }
    Err(format!("UV set {set} does not exist"))
}

fn required_quarter_turns(args: &Map<String, Value>) -> Result<i32, String> {
    let turns = required_index(args, "quarter_turns")?;
    i32::try_from(turns).map_err(|_| "'quarter_turns' is too large".to_string())
}

fn vec2_arg(args: &Map<String, Value>, key: &str) -> Result<Option<Vec2>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(values)) if values.len() == 2 => Ok(Some(Vec2::new(
            component(&values[0], key)?,
            component(&values[1], key)?,
        ))),
        Some(_) => Err(format!("'{key}' must be [u, v]")),
    }
}

fn object_property() -> Value {
    json!({
        "type": "string",
        "description": "Object id or exact object name."
    })
}

fn animation_channel_property() -> Value {
    json!({
        "type": "string",
        "enum": [
            "translation",
            "rotation",
            "scale",
            "fov",
            "light_intensity",
            "light_color",
            "morph_weight"
        ],
        "description": "Animatable object channel. Material and tag channels use their own owners."
    })
}

fn material_property() -> Value {
    json!({
        "type": "string",
        "description": "Material id or exact name."
    })
}

fn shader_graph_property() -> Value {
    json!({
        "type": "object",
        "description": "Complete ShaderGraph: nodes with id/kind/position/inputs, links with from/to/to_input, and next_id.",
        "required": ["nodes", "links", "next_id"],
        "properties": {
            "nodes": { "type": "array", "items": { "type": "object" } },
            "links": { "type": "array", "items": { "type": "object" } },
            "next_id": { "type": "integer", "minimum": 0 }
        }
    })
}

fn modifier_property() -> Value {
    json!({
        "type": "object",
        "description": "Modifier object with a snake_case type and that type's fields, matching get_object's modifiers array.",
        "required": ["type"],
        "properties": {
            "type": {
                "type": "string",
                "enum": [
                    "subdivision",
                    "extrude_faces",
                    "bevel_faces",
                    "array",
                    "symmetry",
                    "boolean",
                    "bend",
                    "taper",
                    "twist",
                    "shear",
                    "bulge",
                    "spherify",
                    "lathe",
                    "sweep",
                    "ring_cut",
                    "displacement",
                    "formula",
                    "magnet",
                    "laplacian_smooth",
                    "polygon_reduction",
                    "wrap"
                ]
            }
        }
    })
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into())
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
    /// Fill a prompt template from the live document. The text is generated per
    /// call, so a prompt describes the document as it stands rather than a
    /// placeholder, and object arguments use the same id-or-name lookup as the
    /// tools.
    pub(crate) fn prompt(&self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or((-32602, "Missing prompt name".to_string()))?;
        let empty = Map::new();
        let arguments = match params.get("arguments") {
            None | Some(Value::Null) => &empty,
            Some(Value::Object(map)) => map,
            Some(_) => return Err((-32602, "Prompt arguments must be an object".to_string())),
        };
        let (description, text) = match name {
            "scene_overview" => (
                "Summarise the open LeopardCraft scene.",
                format!(
                    "Here is the current LeopardCraft document summary. Describe what the scene \
                     contains and what the next modelling, animation or render step should be.\n\n{}",
                    pretty(&self.summary_payload())
                ),
            ),
            "object_report" => {
                let id = self
                    .resolve_object(arguments, "object")
                    .map_err(|error| (-32602, error))?;
                let payload = self.object_payload(id).map_err(|error| (-32602, error))?;
                (
                    "Describe one LeopardCraft object.",
                    format!(
                        "Here is one object from the open LeopardCraft document. Explain its \
                         transform, parenting, modifiers, material and animation state, and call \
                         out anything that looks wrong.\n\n{}",
                        pretty(&payload)
                    ),
                )
            }
            "render_check" => {
                let settings = self
                    .summary_payload()
                    .get("settings")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                (
                    "Review the render settings and prepare a render.",
                    format!(
                        "Here are the LeopardCraft document and render settings. Check that the \
                         resolution, sample count, exposure and seed are suitable for the preview, \
                         then call render_image with the same values; a fixed seed renders the \
                         same image every time.\n\n{}",
                        pretty(&settings)
                    ),
                )
            }
            other => return Err((-32602, format!("Unknown prompt: {other}"))),
        };
        Ok(json!({
            "description": description,
            "messages": [{
                "role": "user",
                "content": { "type": "text", "text": text }
            }]
        }))
    }

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

    fn add_modifier(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = self.resolve_object(args, "object")?;
        let modifier = modifier_from_args(args)?;
        self.check_modifier_target(id, &modifier)?;
        let node = self
            .doc
            .node(id)
            .ok_or_else(|| format!("no object with id {id}"))?;
        let before = Shape::of(node);
        let index = match optional_index(args, "index")? {
            Some(index) if index > before.modifiers.len() => {
                return Err(format!(
                    "index {index} is past the end of the {} modifier(s) on {}",
                    before.modifiers.len(),
                    node.name
                ));
            }
            Some(index) => index,
            None => before.modifiers.len(),
        };
        let mut after = before.clone();
        after.modifiers.insert(index, modifier.clone());
        self.execute(Box::new(SetShapeCmd { id, before, after }));
        let mut payload = self.object_payload(id)?;
        payload["index"] = json!(index);
        Ok(payload)
    }

    fn update_modifier(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = self.resolve_object(args, "object")?;
        let index = required_index(args, "index")?;
        let modifier = modifier_from_args(args)?;
        self.check_modifier_target(id, &modifier)?;
        let node = self
            .doc
            .node(id)
            .ok_or_else(|| format!("no object with id {id}"))?;
        let before = Shape::of(node);
        let Some(current) = before.modifiers.get(index) else {
            return Err(format!(
                "index {index} is outside the {} modifier(s) on {}",
                before.modifiers.len(),
                node.name
            ));
        };
        if current == &modifier {
            return Err(format!("modifier at index {index} is unchanged"));
        }
        let mut after = before.clone();
        after.modifiers[index] = modifier;
        self.execute(Box::new(SetShapeCmd { id, before, after }));
        let mut payload = self.object_payload(id)?;
        payload["index"] = json!(index);
        Ok(payload)
    }

    fn remove_modifier(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = self.resolve_object(args, "object")?;
        let index = required_index(args, "index")?;
        let node = self
            .doc
            .node(id)
            .ok_or_else(|| format!("no object with id {id}"))?;
        let before = Shape::of(node);
        if index >= before.modifiers.len() {
            return Err(format!(
                "index {index} is outside the {} modifier(s) on {}",
                before.modifiers.len(),
                node.name
            ));
        }
        let mut after = before.clone();
        let removed = after.modifiers.remove(index);
        self.execute(Box::new(SetShapeCmd { id, before, after }));
        let mut payload = self.object_payload(id)?;
        payload["removed"] = serde_json::to_value(&removed).unwrap_or(Value::Null);
        payload["index"] = json!(index);
        Ok(payload)
    }

    fn move_modifier(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = self.resolve_object(args, "object")?;
        let from = required_index(args, "from")?;
        let to = required_index(args, "to")?;
        let node = self
            .doc
            .node(id)
            .ok_or_else(|| format!("no object with id {id}"))?;
        let before = Shape::of(node);
        if from >= before.modifiers.len() || to >= before.modifiers.len() {
            return Err(format!(
                "indices {from} and {to} must both be inside the {} modifier(s) on {}",
                before.modifiers.len(),
                node.name
            ));
        }
        if from == to {
            return Err(format!("modifier is already at index {from}"));
        }
        let mut after = before.clone();
        let modifier = after.modifiers.remove(from);
        after.modifiers.insert(to, modifier);
        self.execute(Box::new(SetShapeCmd { id, before, after }));
        let mut payload = self.object_payload(id)?;
        payload["from"] = json!(from);
        payload["to"] = json!(to);
        Ok(payload)
    }

    /// Reject modifiers that this object cannot carry before anything is
    /// pushed onto the undo stack, mirroring the editor's add-button gating.
    fn check_modifier_target(&self, id: NodeId, modifier: &Modifier) -> Result<(), String> {
        let node = self
            .doc
            .node(id)
            .ok_or_else(|| format!("no object with id {id}"))?;
        let applies = match modifier {
            Modifier::Sweep { .. } => matches!(node.kind, NodeKind::Spline { .. }),
            Modifier::Lathe { .. } => true,
            _ => node.is_mesh(),
        };
        if !applies {
            return Err(format!(
                "{} cannot be added to {}",
                modifier.label(),
                node.name
            ));
        }
        if !modifier.parameters_are_finite() {
            return Err(format!(
                "{} parameters must all be finite numbers",
                modifier.label()
            ));
        }
        if let Modifier::Boolean { operand, .. } = modifier {
            if *operand == id {
                return Err("a Boolean modifier cannot use its own object as the operand".into());
            }
            let operand_node = self
                .doc
                .node(*operand)
                .ok_or_else(|| format!("Boolean operand {operand} no longer exists"))?;
            if !operand_node.is_mesh() {
                return Err(format!(
                    "Boolean operand {} is not a mesh object",
                    operand_node.name
                ));
            }
        }
        if let Modifier::Wrap { target, .. } = modifier {
            if *target == id {
                return Err("a Wrap modifier cannot use its own object as the target".into());
            }
            let target_node = self
                .doc
                .node(*target)
                .ok_or_else(|| format!("Wrap target {target} no longer exists"))?;
            if !target_node.is_mesh() {
                return Err(format!(
                    "Wrap target {} is not a mesh object",
                    target_node.name
                ));
            }
        }
        if let Modifier::Formula { x, y, z } = modifier {
            // Parsing on an empty mesh surfaces the axis and the syntax problem
            // without touching the document.
            lc_geom::formula(&Mesh::default(), x, y, z)?;
        }
        Ok(())
    }

    fn get_mesh(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = self.resolve_object(args, "object")?;
        self.mesh_payload(id)
    }

    fn mesh_payload(&self, id: NodeId) -> Result<Value, String> {
        let node = self
            .doc
            .node(id)
            .ok_or_else(|| format!("no object with id {id}"))?;
        let NodeKind::Mesh { geometry } = &node.kind else {
            return Err(format!("{} is not a mesh object", node.name));
        };
        let mesh = geometry.build();
        let charts = uv_charts(&mesh).len();
        Ok(json!({
            "object": { "id": node.id, "name": node.name },
            "editable": geometry.mesh().is_some(),
            "vertices": mesh.positions.len(),
            "edges": mesh.edges().len(),
            "faces": mesh.polygons.len(),
            "uv_sets": mesh.uv_set_count(),
            "uv_slots": mesh.uvs.len(),
            "uv_charts": charts,
            "seams": mesh.seams,
            "pinned_uvs": mesh.pinned_uvs,
            "mesh": serde_json::to_value(&mesh).unwrap_or(Value::Null)
        }))
    }

    /// Resolve a mesh object argument to its id, the pre-edit shape for undo and
    /// a clone of the mesh its geometry builds.
    fn mesh_edit_target(&self, args: &Map<String, Value>) -> Result<(NodeId, Shape, Mesh), String> {
        let id = self.resolve_object(args, "object")?;
        let node = self
            .doc
            .node(id)
            .ok_or_else(|| format!("no object with id {id}"))?;
        let NodeKind::Mesh { geometry } = &node.kind else {
            return Err(format!("{} is not a mesh object", node.name));
        };
        Ok((id, Shape::of(node), geometry.build()))
    }

    /// The refreshed mesh summary plus the operation that produced it.
    fn mesh_edit_payload(&self, id: NodeId, operation: String) -> Result<Value, String> {
        let mut payload = self.mesh_payload(id)?;
        payload["operation"] = json!(operation);
        Ok(payload)
    }

    fn edit_mesh(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let operation = required_str(args, "operation")?.to_ascii_lowercase();
        let (id, before, mesh) = self.mesh_edit_target(args)?;
        let edited = match operation.as_str() {
            "transform" => {
                let points = transform_selection(&mesh, args)?;
                let translation = vec3_arg(args, "translation")?.unwrap_or(Vec3::ZERO);
                let rotation = vec3_arg(args, "rotation_degrees")?.unwrap_or(Vec3::ZERO);
                let scale = vec3_arg(args, "scale")?.unwrap_or(Vec3::ONE);
                if scale.x == 0.0 || scale.y == 0.0 || scale.z == 0.0 {
                    return Err("'scale' components must be non-zero".into());
                }
                transform_points(&mesh, &points, translation, rotation, scale)
                    .ok_or_else(|| "that transform does not move any selected component".to_string())?
            }
            "extrude" => {
                let faces = face_selection(&mesh, args)?;
                let distance = required_number(args, "distance")?;
                let individual = optional_bool(args, "individual")?.unwrap_or(false);
                extrude_faces(&mesh, &faces, distance, individual)
            }
            "inset" => {
                let faces = face_selection(&mesh, args)?;
                inset_faces(&mesh, &faces, required_number(args, "amount")?).0
            }
            "bevel" => {
                let faces = face_selection(&mesh, args)?;
                let amount = required_number(args, "amount")?;
                let depth = f32_arg(args, "depth", 0.0)?;
                bevel_faces(&mesh, &faces, amount, depth)
            }
            "flip" => flip_faces(&mesh, &face_selection(&mesh, args)?),
            "triangulate" => {
                let faces = face_selection(&mesh, args)?;
                triangulate_faces_with(&mesh, &faces, triangulate_mode(args)?)
            }
            "optimize" => {
                let points = point_selection(&mesh, args)?;
                let tolerance = f32_arg(args, "tolerance", 0.0)?;
                if tolerance < 0.0 {
                    return Err("'tolerance' must not be negative".into());
                }
                optimize_points(&mesh, &points, tolerance)
                    .map(|(mesh, _)| mesh)
                    .ok_or_else(|| "no selected points are within the weld tolerance".to_string())?
            }
            "slide" => {
                let points = point_selection(&mesh, args)?;
                slide_points(&mesh, &points, required_number(args, "fraction")?)
                    .ok_or_else(|| "that slide does not move any selected point".to_string())?
            }
            "fill_holes" => {
                let points = point_selection(&mesh, args)?;
                let loops = open_holes(&mesh, &points);
                if loops.is_empty() {
                    return Err("the point selection does not enclose a boundary hole".into());
                }
                fill_holes(&mesh, &loops)?
            }
            "create_polygon" => {
                let points = point_selection(&mesh, args)?;
                create_polygon_faces(&mesh, &points)?.0
            }
            "disconnect" => disconnect_faces(&mesh, &face_selection(&mesh, args)?),
            "cover" => {
                let faces = face_selection(&mesh, args)?;
                cover_faces(&mesh, &faces)?.0
            }
            "collapse" => {
                let points = point_selection(&mesh, args)?;
                collapse_points(&mesh, &points).0
            }
            "bridge" => {
                let loops = point_loops(args)?;
                bridge_loops(&mesh, &loops[0], &loops[1])?
            }
            other => {
                return Err(format!(
                    "unknown mesh operation '{other}'; use transform, extrude, inset, bevel, flip, triangulate, optimize, slide, fill_holes, create_polygon, disconnect, cover, collapse or bridge"
                ))
            }
        };
        self.commit_mesh_edit(id, before, edited)?;
        self.mesh_edit_payload(id, operation)
    }

    fn edit_uv(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let operation = required_str(args, "operation")?.to_ascii_lowercase();
        let (id, before, mut mesh) = self.mesh_edit_target(args)?;
        let set = optional_index(args, "uv_set")?.unwrap_or(0);
        match operation.as_str() {
            "project" => {
                require_uv_set(&mesh, set)?;
                let mode = required_str(args, "mode")?.to_ascii_lowercase();
                let scale = f32_arg(args, "scale", 1.0)?;
                let height_scale = f32_arg(args, "height_scale", 1.0)?;
                let axis = optional_index(args, "axis")?.unwrap_or(2);
                match mode.as_str() {
                    "planar" => {
                        if axis > 2 {
                            return Err("'axis' must be 0, 1 or 2".into());
                        }
                        planar_projection_in_set(&mut mesh, set, axis, scale);
                    }
                    "box" => box_projection_in_set(&mut mesh, set, scale),
                    "cylindrical" => {
                        cylindrical_projection_in_set(&mut mesh, set, scale, height_scale)
                    }
                    "spherical" => spherical_projection_in_set(&mut mesh, set, scale),
                    other => return Err(format!("unknown projection mode '{other}'")),
                }
            }
            "unwrap" => {
                let max_angle = f32_arg(args, "max_angle_deg", 66.0)?.clamp(0.0, 180.0);
                let padding = f32_arg(args, "padding", 0.0)?.max(0.0);
                automatic_unwrap(&mut mesh, max_angle, padding);
            }
            "relax" => {
                let iterations = u32_arg(args, "iterations", 10, 1, 1000)?;
                let strength = f32_arg(args, "strength", 1.0)?.clamp(0.0, 1.0);
                let pin_boundary = optional_bool(args, "pin_boundary")?.unwrap_or(true);
                relax_uvs(&mut mesh, iterations, strength, pin_boundary);
            }
            "normalize" => normalize_uvs(&mut mesh),
            "add_set" => {
                mesh.add_uv_set().ok_or_else(|| {
                    "a mesh needs a UV0 layout before another set can be added".to_string()
                })?;
            }
            "remove_set" => {
                if !mesh.remove_uv_set(set) {
                    return Err(format!(
                        "UV set {set} cannot be removed (0 is UV0 and must exist)"
                    ));
                }
            }
            "set_uv" => {
                let slot = required_index(args, "slot")? as u32;
                let u = required_number(args, "u")?;
                let v = required_number(args, "v")?;
                if mesh.uv_value(set, slot).is_none() {
                    return Err(format!("UV slot {slot} does not exist in set {set}"));
                }
                mesh.set_uv_value(set, slot, Vec2::new(u, v));
            }
            "set_pinned" => {
                let slot = required_index(args, "slot")? as u32;
                let pinned = optional_bool(args, "pinned")?.unwrap_or(true);
                if mesh.uv_value(0, slot).is_none() {
                    return Err(format!("UV slot {slot} does not exist"));
                }
                mesh.set_uv_pinned(slot, pinned);
            }
            "set_seam" => {
                let a = required_index(args, "a")? as u32;
                let b = required_index(args, "b")? as u32;
                let seam = optional_bool(args, "seam")?.unwrap_or(true);
                if !edge_exists(&mesh, a, b) {
                    return Err(format!("points {a} and {b} do not share a mesh edge"));
                }
                mesh.set_seam(a, b, seam);
            }
            "clear_seams" => mesh.seams.clear(),
            "pack" => {
                require_uv_set(&mesh, set)?;
                let charts = chart_selection(&mesh, args)?;
                let options = UvPackOptions {
                    padding: f32_arg(args, "padding", 0.0)?.max(0.0),
                    rotate: optional_bool(args, "rotate")?.unwrap_or(false),
                    align: pack_align(args)?,
                };
                let report = pack_uv_charts(&mut mesh, set, &charts, options);
                if report.packed == 0 {
                    return Err(format!(
                        "no charts were packed ({} pinned); select unpinned charts or unpin slots",
                        report.skipped_pinned
                    ));
                }
            }
            "transform_charts" => {
                require_uv_set(&mesh, set)?;
                let charts = chart_selection(&mesh, args)?;
                let translation = vec2_arg(args, "translation")?.unwrap_or(Vec2::ZERO);
                let rotation = f32_arg(args, "rotation_degrees", 0.0)?;
                let scale = f32_arg(args, "scale", 1.0)?;
                if scale <= 0.0 {
                    return Err("'scale' must be greater than zero".into());
                }
                if transform_uv_charts(&mut mesh, set, &charts, translation, rotation, scale) == 0 {
                    return Err("no charts were transformed".into());
                }
            }
            "weld" => {
                let slots = slot_selection(&mesh, args)?;
                if weld_uv_slots(&mut mesh, &slots) == 0 {
                    return Err("the selected UV slots cannot be welded".into());
                }
            }
            "split" => {
                let slots = slot_selection(&mesh, args)?;
                if split_uv_slots(&mut mesh, &slots) == 0 {
                    return Err("the selected UV slots are used by one corner and cannot split".into());
                }
            }
            "align" | "distribute" | "flip" | "rotate" => {
                require_uv_set(&mesh, set)?;
                let slots = slot_selection(&mesh, args)?;
                let changed = match operation.as_str() {
                    "align" => align_uv_slots(&mut mesh, set, &slots, uv_axis(args)?),
                    "distribute" => distribute_uv_slots(&mut mesh, set, &slots, uv_axis(args)?),
                    "flip" => flip_uv_slots(&mut mesh, set, &slots, uv_axis(args)?),
                    _ => rotate_uv_slots(&mut mesh, set, &slots, required_quarter_turns(args)?),
                };
                if !changed {
                    return Err("the selected UV slots do not satisfy that operation".into());
                }
            }
            other => {
                return Err(format!(
                    "unknown UV operation '{other}'; use project, unwrap, relax, normalize, add_set, remove_set, set_uv, set_pinned, set_seam, clear_seams, pack, transform_charts, weld, split, align, distribute, flip or rotate"
                ))
            }
        }
        self.commit_mesh_edit(id, before, mesh)?;
        self.mesh_edit_payload(id, operation)
    }

    fn commit_mesh_edit(&mut self, id: NodeId, before: Shape, mesh: Mesh) -> Result<(), String> {
        let NodeKind::Mesh { geometry } = &before.kind else {
            return Err("mesh edit lost the mesh object".into());
        };
        if geometry.build() == mesh {
            return Err("the operation did not change the mesh".into());
        }
        let mut after = before.clone();
        after.kind = NodeKind::Mesh {
            geometry: geometry.with_mesh(mesh),
        };
        self.execute(Box::new(SetShapeCmd { id, before, after }));
        Ok(())
    }

    fn get_material(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = self.resolve_material(required_str(args, "material")?)?;
        self.material_payload(id)
    }

    fn material_payload(&self, id: u32) -> Result<Value, String> {
        let material = self
            .doc
            .materials
            .get(id)
            .ok_or_else(|| format!("material {id} no longer exists"))?;
        Ok(serde_json::to_value(material).unwrap_or(Value::Null))
    }

    fn create_material(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let name = required_str(args, "name")?.trim().to_string();
        if self.material_name_taken(&name, None) {
            return Err(format!("a material named '{name}' already exists"));
        }
        let base_color = vec3_arg(args, "base_color")?
            .unwrap_or(Vec3::splat(0.8))
            .max(Vec3::ZERO);
        let roughness = f32_arg(args, "roughness", 0.5)?.clamp(0.02, 1.0);
        let metallic = f32_arg(args, "metallic", 0.0)?.clamp(0.0, 1.0);
        let mut material = Material::pbr(0, name, base_color, roughness, metallic);
        if let Some(graph) = graph_from_args(args)? {
            material.graph = graph;
        }
        material.double_sided = optional_bool(args, "double_sided")?.unwrap_or(true);
        material.casts_shadow = optional_bool(args, "casts_shadow")?.unwrap_or(true);
        let id = self.doc.materials.next_id.max(
            self.doc
                .materials
                .iter()
                .map(|m| m.id + 1)
                .max()
                .unwrap_or(0),
        );
        material.id = id;
        self.execute(Box::new(AddMaterialCmd {
            material: material.clone(),
        }));
        Ok(serde_json::to_value(&material).unwrap_or(Value::Null))
    }

    fn update_material(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = self.resolve_material(required_str(args, "material")?)?;
        let before = self
            .doc
            .materials
            .get(id)
            .cloned()
            .ok_or_else(|| format!("material {id} no longer exists"))?;
        let mut after = before.clone();
        let mut changed = false;
        if let Some(raw) = optional_str(args, "name")? {
            let name = raw.trim();
            if name.is_empty() {
                return Err("'name' must not be empty".into());
            }
            if self.material_name_taken(name, Some(id)) {
                return Err(format!("a material named '{name}' already exists"));
            }
            if after.name != name {
                after.name = name.to_string();
                changed = true;
            }
        }
        if let Some(double_sided) = optional_bool(args, "double_sided")? {
            if after.double_sided != double_sided {
                after.double_sided = double_sided;
                changed = true;
            }
        }
        if let Some(casts_shadow) = optional_bool(args, "casts_shadow")? {
            if after.casts_shadow != casts_shadow {
                after.casts_shadow = casts_shadow;
                changed = true;
            }
        }
        if let Some(graph) = graph_from_args(args)? {
            if after.graph != graph {
                after.graph = graph;
                changed = true;
            }
        }
        if !changed {
            return Err(
                "no material changes requested (name, double_sided, casts_shadow or graph)".into(),
            );
        }
        self.execute(Box::new(SetMaterialCmd { before, after }));
        self.material_payload(id)
    }

    fn material_name_taken(&self, name: &str, except: Option<u32>) -> bool {
        let lowered = name.to_ascii_lowercase();
        self.doc.materials.iter().any(|material| {
            Some(material.id) != except && material.name.to_ascii_lowercase() == lowered
        })
    }

    fn get_animation(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = self.resolve_object(args, "object")?;
        self.animation_payload(id)
    }

    fn animation_payload(&self, id: NodeId) -> Result<Value, String> {
        let node = self
            .doc
            .node(id)
            .ok_or_else(|| format!("no object with id {id}"))?;
        let take = self.doc.takes.get(self.doc.active_take);
        let takes: Vec<Value> = self
            .doc
            .takes
            .iter()
            .enumerate()
            .map(|(index, take)| {
                json!({
                    "index": index,
                    "name": take.name,
                    "fps": take.fps,
                    "active": index == self.doc.active_take
                })
            })
            .collect();
        Ok(json!({
            "object": { "id": node.id, "name": node.name },
            "active_take": {
                "index": self.doc.active_take,
                "name": take.map(|take| take.name.clone()),
                "fps": take.map(|take| take.fps)
            },
            "takes": takes,
            "clip": serde_json::to_value(&node.animation).unwrap_or(Value::Null),
            "empty": node.animation.is_empty()
        }))
    }

    fn set_animation_key(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = self.resolve_object(args, "object")?;
        let channel = animation_channel_from_args(args)?;
        let time = required_time(args, "time")?;
        let interpolation = interpolation_from_args(args)?;
        let node = self
            .doc
            .node(id)
            .ok_or_else(|| format!("no object with id {id}"))?;
        self.check_animation_channel(node, channel)?;
        let before = node.animation.clone();
        let mut after = before.clone();
        match channel {
            AnimChannel::Rotation => {
                let rotation = rotation_from_args(args)?;
                after.set_rotation_key(ChannelTarget::Owner, time, rotation, interpolation);
            }
            AnimChannel::MorphWeight => {
                let target = required_index(args, "morph_target")?;
                if target >= node.morph.targets.len() {
                    return Err(format!(
                        "morph target {target} is outside the {} target(s) on {}",
                        node.morph.targets.len(),
                        node.name
                    ));
                }
                let value = required_number(args, "value")?;
                after.set_morph_weight_key(target as u32, time, value, interpolation);
            }
            _ => {
                let component = optional_index(args, "component")?.unwrap_or(0);
                let Some(address) = lc_anim::ChannelAddress::owner(channel, component as u8) else {
                    return Err(format!("{} has no component {component}", channel.label()));
                };
                let value = required_number(args, "value")?;
                after.set_key(address, time, value, interpolation);
            }
        }
        if after == before {
            return Err("that key already has the requested value and interpolation".into());
        }
        self.execute(Box::new(SetAnimationCmd { id, before, after }));
        self.animation_payload(id)
    }

    fn remove_animation_key(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = self.resolve_object(args, "object")?;
        let channel = animation_channel_from_args(args)?;
        let time = required_time(args, "time")?;
        let node = self
            .doc
            .node(id)
            .ok_or_else(|| format!("no object with id {id}"))?;
        self.check_animation_channel(node, channel)?;
        let before = node.animation.clone();
        let mut after = before.clone();
        match channel {
            AnimChannel::Rotation => after.remove_rotation_key(time),
            AnimChannel::MorphWeight => {
                let target = required_index(args, "morph_target")?;
                if target >= node.morph.targets.len() {
                    return Err(format!(
                        "morph target {target} is outside the {} target(s) on {}",
                        node.morph.targets.len(),
                        node.name
                    ));
                }
                after.remove_morph_weight_key(target as u32, time);
            }
            _ => {
                let component = optional_index(args, "component")?.unwrap_or(0);
                let Some(address) = lc_anim::ChannelAddress::owner(channel, component as u8) else {
                    return Err(format!("{} has no component {component}", channel.label()));
                };
                after.remove_key(address, time);
            }
        }
        if after == before {
            return Err(format!(
                "no {} key near {time} on {}",
                channel.label(),
                node.name
            ));
        }
        self.execute(Box::new(SetAnimationCmd { id, before, after }));
        self.animation_payload(id)
    }

    /// Reject channels whose data lives on a material or tag, or whose owner
    /// kind does not match the object.
    fn check_animation_channel(
        &self,
        node: &lc_scene::Node,
        channel: AnimChannel,
    ) -> Result<(), String> {
        match channel {
            AnimChannel::MaterialRoughness | AnimChannel::MaterialMetallic => {
                Err("material channels belong to a material, not an object".into())
            }
            AnimChannel::CopyLocationWeight => {
                Err("tag channels belong to a tag clip, not an object".into())
            }
            AnimChannel::Fov if !matches!(node.kind, NodeKind::Camera { .. }) => {
                Err(format!("FOV keys need a camera; {} is not one", node.name))
            }
            AnimChannel::LightIntensity | AnimChannel::LightColor
                if !matches!(node.kind, NodeKind::Light { .. }) =>
            {
                Err(format!(
                    "light channels need a light; {} is not one",
                    node.name
                ))
            }
            AnimChannel::MorphWeight if node.morph.targets.is_empty() => {
                Err(format!("{} has no morph targets", node.name))
            }
            _ => Ok(()),
        }
    }

    fn create_take(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let base = optional_str(args, "name")?.unwrap_or("Take");
        let before = self.doc.take_state();
        let index = self.doc.create_take(base);
        let after = self.doc.take_state();
        self.execute(Box::new(SetTakesCmd {
            label: "New Take".into(),
            before,
            after,
        }));
        Ok(json!({
            "created": index,
            "takes": self.takes_payload()
        }))
    }

    fn activate_take(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let index = required_index(args, "index")?;
        if index >= self.doc.takes.len() {
            return Err(format!("take {index} does not exist"));
        }
        if index == self.doc.active_take {
            return Err(format!("take {index} is already active"));
        }
        let before = self.doc.take_state();
        if !self.doc.activate_take(index) {
            return Err(format!("take {index} could not be activated"));
        }
        let after = self.doc.take_state();
        self.execute(Box::new(SetTakesCmd {
            label: "Activate Take".into(),
            before,
            after,
        }));
        Ok(json!({
            "active": index,
            "takes": self.takes_payload()
        }))
    }

    fn update_take(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let index = optional_index(args, "index")?.unwrap_or(self.doc.active_take);
        if index >= self.doc.takes.len() {
            return Err(format!("take {index} does not exist"));
        }
        let name = match optional_str(args, "name")? {
            Some(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    return Err("'name' must not be empty".into());
                }
                Some(trimmed)
            }
            None => None,
        };
        if let Some(name) = name {
            if self
                .doc
                .takes
                .iter()
                .enumerate()
                .any(|(other, take)| other != index && take.name == name)
            {
                return Err(format!("a take named '{name}' already exists"));
            }
        }
        let fps = match number_arg(args, "fps")? {
            Some(fps) => {
                if !(1.0..=240.0).contains(&fps) {
                    return Err("'fps' must be between 1 and 240".into());
                }
                Some(fps as f32)
            }
            None => None,
        };
        let before = self.doc.take_state();
        let mut changed = false;
        if let Some(name) = name {
            if self.doc.takes[index].name != name && self.doc.rename_take(index, name) {
                changed = true;
            }
        }
        if let Some(fps) = fps {
            if self.doc.set_take_fps(index, fps) {
                changed = true;
            }
        }
        if !changed {
            return Err("no take changes requested or values already match".into());
        }
        let after = self.doc.take_state();
        self.execute(Box::new(SetTakesCmd {
            label: "Edit Take".into(),
            before,
            after,
        }));
        Ok(json!({
            "updated": index,
            "takes": self.takes_payload()
        }))
    }

    fn delete_take(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let index = optional_index(args, "index")?.unwrap_or(self.doc.active_take);
        if index >= self.doc.takes.len() {
            return Err(format!("take {index} does not exist"));
        }
        let before = self.doc.take_state();
        if !self.doc.delete_take(index) {
            return Err(format!("take {index} could not be deleted"));
        }
        let after = self.doc.take_state();
        self.execute(Box::new(SetTakesCmd {
            label: "Delete Take".into(),
            before,
            after,
        }));
        Ok(json!({
            "deleted": index,
            "takes": self.takes_payload()
        }))
    }

    fn takes_payload(&self) -> Value {
        json!(self
            .doc
            .takes
            .iter()
            .enumerate()
            .map(|(index, take)| json!({
                "index": index,
                "name": take.name,
                "fps": take.fps,
                "active": index == self.doc.active_take
            }))
            .collect::<Vec<_>>())
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

fn optional_index(args: &Map<String, Value>, key: &str) -> Result<Option<usize>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => {
            let value = number
                .as_u64()
                .ok_or_else(|| format!("'{key}' must be a non-negative integer"))?;
            usize::try_from(value)
                .map(Some)
                .map_err(|_| format!("'{key}' is too large"))
        }
        Some(_) => Err(format!("'{key}' must be a non-negative integer")),
    }
}

fn required_index(args: &Map<String, Value>, key: &str) -> Result<usize, String> {
    optional_index(args, key)?.ok_or_else(|| format!("missing required argument '{key}'"))
}

fn animation_channel_from_args(args: &Map<String, Value>) -> Result<AnimChannel, String> {
    let raw = required_str(args, "channel")?;
    serde_json::from_value::<AnimChannel>(Value::String(raw.to_string()))
        .map_err(|_| format!("unknown animation channel '{raw}'"))
}

fn required_number(args: &Map<String, Value>, key: &str) -> Result<f32, String> {
    number_arg(args, key)?.map_or_else(
        || Err(format!("missing required argument '{key}'")),
        |value| {
            let value = value as f32;
            if value.is_finite() {
                Ok(value)
            } else {
                Err(format!("'{key}' is outside the supported number range"))
            }
        },
    )
}

fn required_time(args: &Map<String, Value>, key: &str) -> Result<f32, String> {
    let time = required_number(args, key)?;
    if time < 0.0 {
        return Err(format!("'{key}' must not be negative"));
    }
    Ok(time)
}

fn interpolation_from_args(args: &Map<String, Value>) -> Result<Interpolation, String> {
    match optional_str(args, "interpolation")? {
        None => Ok(Interpolation::Smooth),
        Some(raw) => serde_json::from_value::<Interpolation>(Value::String(raw.to_string()))
            .map_err(|_| format!("unknown interpolation '{raw}'")),
    }
}

fn rotation_from_args(args: &Map<String, Value>) -> Result<Quat, String> {
    let value = args
        .get("rotation")
        .ok_or_else(|| "missing required argument 'rotation'".to_string())?;
    let array = value
        .as_array()
        .ok_or_else(|| "'rotation' must be an array of four numbers".to_string())?;
    if array.len() != 4 {
        return Err("'rotation' must contain four numbers".into());
    }
    let mut values = [0.0f32; 4];
    for (index, item) in array.iter().enumerate() {
        let number = item
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("rotation[{index}] must be a finite number"))?
            as f32;
        if !number.is_finite() {
            return Err(format!(
                "rotation[{index}] is outside the supported number range"
            ));
        }
        values[index] = number;
    }
    Ok(Quat::from_xyzw(values[0], values[1], values[2], values[3]))
}

fn graph_from_args(args: &Map<String, Value>) -> Result<Option<ShaderGraph>, String> {
    match args.get("graph") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(_)) => {
            let graph = serde_json::from_value::<ShaderGraph>(args["graph"].clone())
                .map_err(|error| format!("invalid shader graph: {error}"))?;
            graph
                .validate()
                .map_err(|error| format!("invalid shader graph: {error}"))?;
            Ok(Some(graph))
        }
        Some(_) => Err("'graph' must be an object".into()),
    }
}

fn modifier_from_args(args: &Map<String, Value>) -> Result<Modifier, String> {
    let value = args
        .get("modifier")
        .ok_or_else(|| "missing required argument 'modifier'".to_string())?;
    if !value.is_object() {
        return Err("'modifier' must be an object".into());
    }
    serde_json::from_value::<Modifier>(value.clone())
        .map_err(|error| format!("invalid modifier: {error}"))
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
