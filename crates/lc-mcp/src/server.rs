//! JSON-RPC dispatch and the stdio transport for the MCP server.

use crate::tools;
use lc_scene::{Document, History};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// Classic MCP revision this server negotiates with `initialize`.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
/// Handshake-free MCP revision advertised through `server/discover`.
pub const LATEST_PROTOCOL_VERSION: &str = "2026-07-28";
pub const SERVER_NAME: &str = "leopardcraft";
pub const SCENE_URI: &str = "leopardcraft://scene";
pub const SUMMARY_URI: &str = "leopardcraft://scene/summary";

const INSTRUCTIONS: &str = "LeopardCraft is an offline 3D modelling application. \
Call get_scene or list_objects before editing. Object arguments accept a numeric id or an exact name. \
Edits, undo and redo operate on the server's in-memory document; save_scene writes it to disk and \
render_image path-traces it with the document camera. A running editor window is not attached.";

/// A LeopardCraft document exposed as an MCP server.
pub struct McpServer {
    pub(crate) doc: Document,
    pub(crate) path: Option<PathBuf>,
    pub(crate) history: History,
    pub(crate) dirty: bool,
}

impl McpServer {
    pub fn new(doc: Document, path: Option<PathBuf>) -> Self {
        let dirty = path.is_none();
        Self {
            doc,
            path,
            history: History::new(256),
            dirty,
        }
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Handle one newline-delimited JSON-RPC message. Notifications return
    /// `None`; every request returns one serialized response line.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        if line.trim().is_empty() {
            return None;
        }
        let message: Value = match serde_json::from_str(line) {
            Ok(message) => message,
            Err(error) => {
                return Some(serialize(error_response(
                    Value::Null,
                    -32700,
                    format!("Parse error: {error}"),
                )))
            }
        };
        self.handle(message).map(serialize)
    }

    /// Handle one decoded JSON-RPC message.
    pub fn handle(&mut self, message: Value) -> Option<Value> {
        let Some(object) = message.as_object() else {
            return Some(error_response(Value::Null, -32600, "Invalid Request"));
        };
        let id = object.get("id").cloned();
        let method = object
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let Some(method) = method else {
            return id.map(|id| error_response(id, -32600, "Invalid Request"));
        };
        // A message without an id is a notification. The server has no
        // asynchronous work to cancel, so every notification is accepted and
        // answered with silence.
        let id = id?;
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(error_response(
                id,
                -32600,
                "Invalid Request: expected jsonrpc 2.0",
            ));
        }
        let params = object.get("params").cloned().unwrap_or(Value::Null);

        let result = match method.as_str() {
            "initialize" => Ok(initialize_result()),
            "ping" => Ok(json!({})),
            "server/discover" => Ok(discover_result()),
            "tools/list" => Ok(tools::list_result()),
            "tools/call" => self.call_tool(&params),
            "resources/list" => Ok(resources_list_result()),
            "resources/templates/list" => Ok(cacheable(json!({ "resourceTemplates": [] }))),
            "resources/read" => self.read_resource(&params),
            "prompts/list" => Ok(cacheable(json!({ "prompts": [] }))),
            other => Err((-32601, format!("Method not found: {other}"))),
        };

        Some(match result {
            Ok(result) => success_response(id, result),
            Err((code, message)) => error_response(id, code, message),
        })
    }

    /// Run the blocking stdio transport until the client closes stdin.
    pub fn serve_stdio(mut self) -> Result<(), String> {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        self.serve(stdin.lock(), stdout.lock())
    }

    /// Transport loop over any buffered reader/writer pair, used by tests.
    pub fn serve<R: BufRead, W: Write>(&mut self, reader: R, mut writer: W) -> Result<(), String> {
        for line in reader.lines() {
            let line = line.map_err(|error| format!("failed to read stdin: {error}"))?;
            if let Some(response) = self.handle_line(&line) {
                writeln!(writer, "{response}")
                    .map_err(|error| format!("failed to write stdout: {error}"))?;
                writer
                    .flush()
                    .map_err(|error| format!("failed to flush stdout: {error}"))?;
            }
        }
        Ok(())
    }

    fn call_tool(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or((-32602, "Missing tool name".to_string()))?;
        if !tools::exists(name) {
            return Err((-32602, format!("Unknown tool: {name}")));
        }
        let arguments = match params.get("arguments") {
            None | Some(Value::Null) => json!({}),
            Some(Value::Object(map)) => Value::Object(map.clone()),
            Some(_) => return Err((-32602, "Tool arguments must be an object".to_string())),
        };
        let arguments = arguments
            .as_object()
            .expect("arguments were normalized to an object");
        match tools::call(self, name, arguments) {
            Ok(payload) => Ok(tool_success(payload)),
            Err(message) => Ok(tool_error(message)),
        }
    }

    fn read_resource(&self, params: &Value) -> Result<Value, (i64, String)> {
        let uri = params
            .get("uri")
            .and_then(Value::as_str)
            .ok_or((-32602, "Missing resource uri".to_string()))?;
        let text = match uri {
            SCENE_URI => lc_scene::to_json(&self.doc).map_err(|error| (-32603, error))?,
            SUMMARY_URI => serde_json::to_string_pretty(&self.summary_payload())
                .map_err(|error| (-32603, error.to_string()))?,
            other => return Err((-32602, format!("Resource not found: {other}"))),
        };
        Ok(cacheable(json!({
            "contents": [{
                "uri": uri,
                "mimeType": "application/json",
                "text": text
            }]
        })))
    }
}

fn serialize(value: Value) -> String {
    serde_json::to_string(&value).unwrap_or_else(|_| {
        r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Serialization failed"}}"#
            .to_string()
    })
}

fn server_info() -> Value {
    json!({
        "name": SERVER_NAME,
        "title": "LeopardCraft",
        "version": env!("CARGO_PKG_VERSION")
    })
}

fn server_meta() -> Value {
    json!({
        "io.modelcontextprotocol/serverInfo": server_info()
    })
}

fn capabilities() -> Value {
    json!({
        "tools": { "listChanged": false },
        "resources": { "subscribe": false, "listChanged": false }
    })
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": capabilities(),
        "serverInfo": server_info(),
        "instructions": INSTRUCTIONS
    })
}

fn discover_result() -> Value {
    json!({
        "supportedVersions": [LATEST_PROTOCOL_VERSION, PROTOCOL_VERSION],
        "capabilities": capabilities(),
        "instructions": INSTRUCTIONS,
        "ttlMs": 0,
        "cacheScope": "private"
    })
}

/// Add the 2026 cache/result fields. Older clients ignore unknown fields.
fn cacheable(mut result: Value) -> Value {
    if let Some(object) = result.as_object_mut() {
        object.insert("ttlMs".into(), json!(0));
        object.insert("cacheScope".into(), json!("private"));
    }
    result
}

fn success_response(id: Value, mut result: Value) -> Value {
    if let Some(object) = result.as_object_mut() {
        object.insert("resultType".into(), json!("complete"));
        object.insert("_meta".into(), server_meta());
    }
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message.into() }
    })
}

fn tool_success(payload: Value) -> Value {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into());
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": payload,
        "isError": false
    })
}

fn tool_error(message: String) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true
    })
}

fn resources_list_result() -> Value {
    cacheable(json!({
        "resources": [
            {
                "uri": SCENE_URI,
                "name": "scene",
                "title": "LeopardCraft scene document",
                "description": "The complete current document as .lcscene JSON.",
                "mimeType": "application/json"
            },
            {
                "uri": SUMMARY_URI,
                "name": "scene-summary",
                "title": "LeopardCraft scene summary",
                "description": "Object counts, render settings and undo state for the current document.",
                "mimeType": "application/json"
            }
        ]
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn server() -> McpServer {
        McpServer::new(Document::empty(), None)
    }

    fn request(server: &mut McpServer, method: &str, params: Value) -> Value {
        server
            .handle(json!({
                "jsonrpc": "2.0",
                "id": 7,
                "method": method,
                "params": params
            }))
            .expect("requests always receive a response")
    }

    fn tool(server: &mut McpServer, name: &str, arguments: Value) -> Value {
        let response = request(
            server,
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        assert!(
            response.get("error").is_none(),
            "tool call failed: {response}"
        );
        response["result"].clone()
    }

    fn structured(result: &Value) -> &Value {
        assert_eq!(result["isError"], json!(false), "tool error: {result}");
        &result["structuredContent"]
    }

    #[test]
    fn initialize_negotiates_and_advertises_capabilities() {
        let mut server = server();
        let response = request(
            &mut server,
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1.0" }
            }),
        );
        assert_eq!(response["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(response["result"]["serverInfo"]["name"], SERVER_NAME);
        assert_eq!(
            response["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
        assert_eq!(response["result"]["resultType"], "complete");
        assert!(response["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("LeopardCraft"));
    }

    #[test]
    fn discover_offers_both_protocol_revisions() {
        let mut server = server();
        let response = request(&mut server, "server/discover", json!({}));
        let versions = response["result"]["supportedVersions"].as_array().unwrap();
        assert!(versions.contains(&json!(PROTOCOL_VERSION)));
        assert!(versions.contains(&json!(LATEST_PROTOCOL_VERSION)));
        assert_eq!(
            response["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
            SERVER_NAME
        );
    }

    #[test]
    fn handshake_free_tools_list_works() {
        // 2026-07-28 clients may omit initialize entirely and carry protocol
        // metadata in per-request _meta.
        let mut server = server();
        let response = server
            .handle(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/list",
                "params": {
                    "_meta": {
                        "io.modelcontextprotocol/protocolVersion": LATEST_PROTOCOL_VERSION,
                        "io.modelcontextprotocol/clientInfo": {
                            "name": "test",
                            "version": "1.0"
                        },
                        "io.modelcontextprotocol/clientCapabilities": {}
                    }
                }
            }))
            .unwrap();
        assert!(response["result"]["tools"].as_array().unwrap().len() >= 15);
    }

    #[test]
    fn tools_list_is_deterministic_and_well_formed() {
        let mut server = server();
        let first = request(&mut server, "tools/list", json!({}));
        let second = request(&mut server, "tools/list", json!({}));
        let tools = first["result"]["tools"].as_array().unwrap();
        assert_eq!(tools, second["result"]["tools"].as_array().unwrap());
        assert!(tools.len() >= 15);
        for tool in tools {
            let name = tool["name"].as_str().unwrap();
            assert!(tools::exists(name), "missing dispatch for {name}");
            assert_eq!(tool["inputSchema"]["type"], "object", "{name}");
            assert!(tool["description"].as_str().unwrap().len() > 20);
        }
        let names: Vec<&str> = tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        for expected in [
            "get_scene",
            "list_objects",
            "get_object",
            "create_primitive",
            "set_transform",
            "rename_object",
            "delete_object",
            "set_object_properties",
            "list_materials",
            "undo",
            "redo",
            "save_scene",
            "open_scene",
            "new_scene",
            "export_scene",
            "render_image",
        ] {
            assert!(names.contains(&expected), "missing tool {expected}");
        }
    }

    #[test]
    fn create_transform_and_undo_redo() {
        let mut server = server();
        let created = tool(
            &mut server,
            "create_primitive",
            json!({
                "primitive": "box",
                "name": "Crate",
                "translation": [1.0, 2.0, 3.0],
                "rotation_degrees": [0.0, 90.0, 0.0],
                "scale": [2.0, 1.0, 1.0]
            }),
        );
        let object = structured(&created);
        let id = object["id"].as_u64().unwrap();
        assert_eq!(object["name"], "Crate");
        assert_eq!(object["type"], "Box");
        assert_eq!(object["transform"]["translation"], json!([1.0, 2.0, 3.0]));

        let list = tool(&mut server, "list_objects", json!({}));
        let list = structured(&list);
        assert_eq!(list["count"], json!(1));
        assert_eq!(list["objects"][0]["id"], json!(id));

        let transformed = tool(
            &mut server,
            "set_transform",
            json!({ "object": id.to_string(), "translation": [0.0, 1.0, 0.0], "scale": [1.0, 1.0, 1.0] }),
        );
        let transformed = structured(&transformed);
        assert_eq!(
            transformed["transform"]["translation"],
            json!([0.0, 1.0, 0.0])
        );

        let undo = tool(&mut server, "undo", json!({}));
        assert_eq!(structured(&undo)["undone"], "Transform");
        let undo = tool(&mut server, "undo", json!({}));
        assert_eq!(structured(&undo)["undone"], "Add Crate");
        let list = tool(&mut server, "list_objects", json!({}));
        assert_eq!(structured(&list)["count"], json!(0));

        let redo = tool(&mut server, "redo", json!({}));
        assert_eq!(structured(&redo)["redone"], "Add Crate");
        let list = tool(&mut server, "list_objects", json!({}));
        assert_eq!(structured(&list)["count"], json!(1));
    }

    #[test]
    fn rename_properties_parenting_and_delete() {
        let mut server = server();
        let parent = structured(&tool(
            &mut server,
            "create_primitive",
            json!({ "primitive": "box", "name": "Parent" }),
        ))["id"]
            .as_u64()
            .unwrap();
        let child = structured(&tool(
            &mut server,
            "create_primitive",
            json!({ "primitive": "sphere", "name": "Child", "parent": "Parent" }),
        ))["id"]
            .as_u64()
            .unwrap();

        let list = tool(&mut server, "list_objects", json!({}));
        let list = structured(&list);
        assert_eq!(list["objects"][1]["parent"], json!(parent));

        let renamed = tool(
            &mut server,
            "rename_object",
            json!({ "object": child.to_string(), "name": "Ball" }),
        );
        assert_eq!(structured(&renamed)["name"], "Ball");

        let materials = structured(&tool(&mut server, "list_materials", json!({}))).clone();
        let material_name = materials["materials"][0]["name"]
            .as_str()
            .unwrap()
            .to_string();
        let edited = tool(
            &mut server,
            "set_object_properties",
            json!({ "object": "Ball", "visible": false, "material": material_name }),
        );
        let edited = structured(&edited);
        assert_eq!(edited["visible"], json!(false));
        assert_eq!(edited["material"]["name"], material_name);

        let error = tool(
            &mut server,
            "set_object_properties",
            json!({ "object": "Ball" }),
        );
        assert_eq!(error["isError"], json!(true));

        let deleted = tool(
            &mut server,
            "delete_object",
            json!({ "object": parent.to_string() }),
        );
        assert_eq!(structured(&deleted)["deleted"]["name"], "Parent");
        let list = tool(&mut server, "list_objects", json!({}));
        assert_eq!(structured(&list)["count"], json!(0));

        tool(&mut server, "undo", json!({}));
        let list = tool(&mut server, "list_objects", json!({}));
        assert_eq!(structured(&list)["count"], json!(2));
    }

    #[test]
    fn scene_summary_reports_counts_and_settings() {
        let mut server = McpServer::new(Document::demo(), None);
        let summary = structured(&tool(&mut server, "get_scene", json!({}))).clone();
        assert!(summary["objects"]["total"].as_u64().unwrap() >= 9);
        assert!(summary["objects"]["meshes"].as_u64().unwrap() >= 5);
        assert!(summary["triangles"].as_u64().unwrap() > 0);
        assert!(summary["active_camera"]["name"].is_string());
        assert_eq!(summary["document"]["dirty"], json!(true));
        assert!(summary["materials"].as_u64().unwrap() >= 6);
    }

    #[test]
    fn resources_expose_the_document_and_summary() {
        let mut server = McpServer::new(Document::demo(), None);
        let list = request(&mut server, "resources/list", json!({}));
        let uris: Vec<&str> = list["result"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|resource| resource["uri"].as_str().unwrap())
            .collect();
        assert!(uris.contains(&SCENE_URI));
        assert!(uris.contains(&SUMMARY_URI));

        let read = request(&mut server, "resources/read", json!({ "uri": SCENE_URI }));
        let text = read["result"]["contents"][0]["text"].as_str().unwrap();
        let restored = lc_scene::from_json(text).unwrap();
        assert_eq!(restored.signature(), server.document().signature());

        let missing = request(
            &mut server,
            "resources/read",
            json!({ "uri": "leopardcraft://missing" }),
        );
        assert_eq!(missing["error"]["code"], json!(-32602));
    }

    #[test]
    fn save_and_open_roundtrip_preserves_the_document() {
        let dir = temp_dir();
        let path = dir.join("scene.lcscene");
        let mut server = server();
        tool(
            &mut server,
            "create_primitive",
            json!({ "primitive": "torus", "name": "Ring", "translation": [0.0, 1.0, 0.0] }),
        );
        let signature = server.document().signature();
        let saved = tool(
            &mut server,
            "save_scene",
            json!({ "path": path.display().to_string() }),
        );
        assert_eq!(structured(&saved)["path"], path.display().to_string());
        assert!(path.exists());
        assert!(!server.is_dirty());

        let reset = tool(&mut server, "new_scene", json!({ "template": "empty" }));
        assert_eq!(structured(&reset)["objects"]["total"], json!(0));
        assert_eq!(server.path(), None);

        let opened = tool(
            &mut server,
            "open_scene",
            json!({ "path": path.display().to_string() }),
        );
        let opened = structured(&opened);
        assert_eq!(opened["objects"]["total"], json!(1));
        assert_eq!(server.document().signature(), signature);
        assert_eq!(server.path(), Some(path.as_path()));
        assert!(!server.is_dirty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn export_writes_interchange_files() {
        let dir = temp_dir();
        let path = dir.join("model.glb");
        let mut server = server();
        tool(
            &mut server,
            "create_primitive",
            json!({ "primitive": "cylinder" }),
        );
        let exported = tool(
            &mut server,
            "export_scene",
            json!({ "path": path.display().to_string() }),
        );
        assert_eq!(structured(&exported)["format"], "glb");
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"glTF");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn render_image_writes_a_png() {
        let dir = temp_dir();
        let path = dir.join("preview.png");
        let mut server = McpServer::new(Document::demo(), None);
        let rendered = tool(
            &mut server,
            "render_image",
            json!({
                "path": path.display().to_string(),
                "width": 16,
                "height": 16,
                "samples": 1,
                "seed": 3
            }),
        );
        let rendered = structured(&rendered);
        assert_eq!(rendered["width"], json!(16));
        assert_eq!(rendered["format"], "png");
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..8], b"\x89PNG\r\n\x1a\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unknown_objects_and_tools_are_reported_cleanly() {
        let mut server = server();
        let missing_object = tool(
            &mut server,
            "get_object",
            json!({ "object": "does-not-exist" }),
        );
        assert_eq!(missing_object["isError"], json!(true));
        assert!(missing_object["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("does-not-exist"));

        let unknown_tool = request(
            &mut server,
            "tools/call",
            json!({ "name": "not_a_tool", "arguments": {} }),
        );
        assert_eq!(unknown_tool["error"]["code"], json!(-32602));

        let missing_argument = tool(&mut server, "create_primitive", json!({}));
        assert_eq!(missing_argument["isError"], json!(true));
        assert!(missing_argument["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("primitive"));

        let invalid = tool(
            &mut server,
            "create_primitive",
            json!({ "primitive": "dodecahedron" }),
        );
        assert_eq!(invalid["isError"], json!(true));
    }

    #[test]
    fn protocol_errors_and_notifications_are_handled() {
        let mut server = server();
        let method = request(&mut server, "nope/nothing", json!({}));
        assert_eq!(method["error"]["code"], json!(-32601));

        let parsed: Value =
            serde_json::from_str(&server.handle_line("{not json").unwrap()).unwrap();
        assert_eq!(parsed["error"]["code"], json!(-32700));
        assert_eq!(parsed["id"], Value::Null);

        assert!(server
            .handle(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .is_none());
        assert!(server
            .handle(json!({
                "jsonrpc": "2.0",
                "method": "notifications/cancelled",
                "params": { "requestId": 1, "reason": "test" }
            }))
            .is_none());

        let bad_version = server
            .handle(json!({ "jsonrpc": "1.0", "id": 2, "method": "ping" }))
            .unwrap();
        assert_eq!(bad_version["error"]["code"], json!(-32600));
    }

    #[test]
    fn serve_loop_answers_line_delimited_requests() {
        let mut server = McpServer::new(Document::empty(), None);
        let input = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n",
            "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}\n"
        );
        let mut output = Vec::new();
        server
            .serve(std::io::Cursor::new(input.as_bytes()), &mut output)
            .unwrap();
        let lines: Vec<&str> = std::str::from_utf8(&output)
            .unwrap()
            .lines()
            .filter(|line| !line.is_empty())
            .collect();
        assert_eq!(lines.len(), 2);
        let first: Value = serde_json::from_str(lines[0]).unwrap();
        let second: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(first["id"], json!(1));
        assert_eq!(second["id"], json!(2));
        assert!(!second["result"]["tools"].as_array().unwrap().is_empty());
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "leopardcraft-mcp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
