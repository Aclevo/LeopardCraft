//! JSON-RPC dispatch and the stdio transport for the MCP server.

use crate::bridge::LiveClient;
use crate::tools;
use lc_scene::{Document, History};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// Classic MCP revision this server negotiates with `initialize`.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
/// Handshake-free MCP revision advertised through `server/discover`.
pub const LATEST_PROTOCOL_VERSION: &str = "2026-07-28";
pub const SERVER_NAME: &str = "leopardcraft";
pub const SCENE_URI: &str = "leopardcraft://scene";
pub const SUMMARY_URI: &str = "leopardcraft://scene/summary";
/// Default address for the opt-in Streamable HTTP transport.
pub const DEFAULT_HTTP_ADDR: &str = "127.0.0.1:8765";

const INSTRUCTIONS: &str = "LeopardCraft is an offline 3D modelling application. \
Call get_scene or list_objects before editing. Object arguments accept a numeric id or an exact name. \
Edits, undo and redo go through the document's undo commands; save_scene writes it to disk and \
render_image path-traces it with the document camera.";

const ATTACHED_INSTRUCTIONS: &str = "LeopardCraft is attached to the editor's live session. \
Tool calls, undo, redo and selection act on the document the user is editing; nothing is written to a \
separate copy. get_scene includes the editor's current object selection, and select_objects changes it.";

const DETACHED_INSTRUCTIONS: &str =
    "LeopardCraft owns an in-memory document that is not a running \
editor window. Start the server with --attach to operate on the editor's live session instead.";

/// A LeopardCraft document exposed as an MCP server.
pub struct McpServer {
    pub(crate) doc: Document,
    pub(crate) path: Option<PathBuf>,
    pub(crate) history: History,
    pub(crate) dirty: bool,
    subscriptions: BTreeSet<String>,
    pending: Vec<Value>,
    /// The editor session this server is attached to, if any. While attached,
    /// every tool call and resource belongs to that live document; the fields
    /// above stay as the fallback for when nothing is attached.
    live: Option<LiveClient>,
}

impl McpServer {
    pub fn new(doc: Document, path: Option<PathBuf>) -> Self {
        let dirty = path.is_none();
        Self {
            doc,
            path,
            history: History::new(256),
            dirty,
            subscriptions: BTreeSet::new(),
            pending: Vec::new(),
            live: None,
        }
    }

    /// Build a server around an existing document and history. The live bridge
    /// uses this so a tool call runs against the editor's own session, under
    /// the editor's undo history, without copying either.
    pub fn with_history(doc: Document, path: Option<PathBuf>, history: History) -> Self {
        let mut server = Self::new(doc, path);
        server.history = history;
        server
    }

    /// Give the document, path and history back, for example after the editor's
    /// UI thread ran one tool call on its live session.
    pub fn into_parts(self) -> (Document, Option<PathBuf>, History) {
        (self.doc, self.path, self.history)
    }

    /// Attach to a running editor. Tool calls and resources then describe its
    /// live document and selection instead of the private one.
    pub fn attach(&mut self, client: LiveClient) {
        self.live = Some(client);
    }

    pub fn detach(&mut self) -> bool {
        self.live.take().is_some()
    }

    pub fn is_attached(&self) -> bool {
        self.live.is_some()
    }

    pub fn live(&self) -> Option<&LiveClient> {
        self.live.as_ref()
    }

    /// Run one tool against this server's document and history. The live bridge
    /// host calls this on the editor's UI thread, one request at a time.
    pub fn call_tool_value(
        &mut self,
        name: &str,
        arguments: &Map<String, Value>,
    ) -> Result<Value, String> {
        if !tools::exists(name) {
            return Err(format!("Unknown tool: {name}"));
        }
        tools::call(self, name, arguments)
    }

    /// Set the attached editor's object selection.
    pub(crate) fn select_objects(&mut self, args: &Map<String, Value>) -> Result<Value, String> {
        let Some(client) = self.live.as_ref() else {
            return Err(
                "Object selection lives in the editor; start the server with --attach and enable \
                 Tools -> Live Agent Bridge, or use select_objects only while attached."
                    .into(),
            );
        };
        let objects = args
            .get("objects")
            .ok_or("Missing 'objects': an array of object ids or exact names")?;
        let objects = objects
            .as_array()
            .ok_or("'objects' must be an array of object ids or exact names")?;
        let mode = args
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("replace");
        if !matches!(mode, "replace" | "add" | "toggle") {
            return Err(format!(
                "Unknown selection mode '{mode}'; use replace, add or toggle"
            ));
        }
        client.select(objects, mode)
    }

    /// Take the notifications queued since the last call. Transports write them
    /// after the response that caused them.
    pub fn take_notifications(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.pending)
    }

    pub fn has_subscriptions(&self) -> bool {
        !self.subscriptions.is_empty()
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
        // Only subscribed clients pay for a signature comparison, and only the
        // request that changed the document queues an update.
        let revision = if self.subscriptions.is_empty() {
            None
        } else {
            Some(self.doc.signature())
        };

        let result = match method.as_str() {
            "initialize" => Ok(initialize_result(self.is_attached())),
            "ping" => Ok(json!({})),
            "server/discover" => Ok(discover_result(self.is_attached())),
            "tools/list" => Ok(tools::list_result()),
            "tools/call" => self.call_tool(&params),
            "resources/list" => Ok(resources_list_result()),
            "resources/templates/list" => Ok(cacheable(json!({ "resourceTemplates": [] }))),
            "resources/read" => self.read_resource(&params),
            "resources/subscribe" => self.subscribe(&params),
            "resources/unsubscribe" => self.unsubscribe(&params),
            "prompts/list" => Ok(prompts_list_result()),
            "prompts/get" => self.prompt(&params),
            other => Err((-32601, format!("Method not found: {other}"))),
        };
        if let Some(before) = revision {
            self.track_resource_updates(before);
        }

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

    /// Run the opt-in Streamable HTTP transport on a loopback address. The
    /// endpoint is printed to stdout, which is not the protocol channel here.
    pub fn serve_http(self, addr: &str) -> Result<(), String> {
        let transport = crate::http::HttpTransport::bind(addr, self)?;
        println!(
            "MCP Streamable HTTP endpoint: http://{}/mcp",
            transport.local_addr()?
        );
        std::io::stdout()
            .flush()
            .map_err(|error| format!("failed to report the endpoint: {error}"))?;
        transport.serve()
    }

    /// Transport loop over any buffered reader/writer pair, used by tests.
    pub fn serve<R: BufRead, W: Write>(&mut self, reader: R, mut writer: W) -> Result<(), String> {
        for line in reader.lines() {
            let line = line.map_err(|error| format!("failed to read stdin: {error}"))?;
            if let Some(response) = self.handle_line(&line) {
                writeln!(writer, "{response}")
                    .map_err(|error| format!("failed to write stdout: {error}"))?;
            }
            self.flush_notifications(&mut writer)?;
        }
        self.flush_notifications(&mut writer)
    }

    /// Write queued notifications as extra newline-delimited protocol lines.
    fn flush_notifications<W: Write>(&mut self, writer: &mut W) -> Result<(), String> {
        for notification in self.take_notifications() {
            writeln!(writer, "{}", serialize(notification))
                .map_err(|error| format!("failed to write stdout: {error}"))?;
        }
        writer
            .flush()
            .map_err(|error| format!("failed to flush stdout: {error}"))
    }

    /// Subscribe to a resource, refusing unknown URIs as invalid params.
    fn subscribe(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let uri = known_resource_uri(params)?;
        self.subscriptions.insert(uri);
        Ok(json!({}))
    }

    fn unsubscribe(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let uri = known_resource_uri(params)?;
        self.subscriptions.remove(&uri);
        Ok(json!({}))
    }

    /// Queue one `notifications/resources/updated` per subscription when a
    /// request changed the document.
    fn track_resource_updates(&mut self, before: u64) {
        let after = self.doc.signature();
        if after == before {
            return;
        }
        for uri in &self.subscriptions {
            self.pending.push(json!({
                "jsonrpc": "2.0",
                "method": "notifications/resources/updated",
                "params": { "uri": uri }
            }));
        }
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
        if self.live.is_some() {
            // Selection is editor state rather than a document edit, so it uses
            // the bridge's dedicated op instead of a tool round-trip.
            if name == "select_objects" {
                return Ok(match self.select_objects(arguments) {
                    Ok(payload) => tool_success(payload),
                    Err(message) => tool_error(message),
                });
            }
            let client = self.live.as_ref().expect("attached");
            return Ok(match client.call_tool(name, arguments) {
                Ok(payload) => tool_success(payload),
                Err(message) => tool_error(message),
            });
        }
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
            SCENE_URI => match self.live.as_ref() {
                Some(client) => client.scene_json().map_err(|error| (-32603, error))?,
                None => lc_scene::to_json(&self.doc).map_err(|error| (-32603, error))?,
            },
            SUMMARY_URI => match self.live.as_ref() {
                Some(client) => {
                    let snapshot = client.snapshot().map_err(|error| (-32603, error))?;
                    serde_json::to_string_pretty(&snapshot)
                        .map_err(|error| (-32603, error.to_string()))?
                }
                None => serde_json::to_string_pretty(&self.summary_payload())
                    .map_err(|error| (-32603, error.to_string()))?,
            },
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
        "resources": { "subscribe": true, "listChanged": false },
        "prompts": { "listChanged": false }
    })
}

/// Validate the `uri` of a subscription request against the resources this
/// server publishes.
fn known_resource_uri(params: &Value) -> Result<String, (i64, String)> {
    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .ok_or((-32602, "Missing resource uri".to_string()))?;
    match uri {
        SCENE_URI | SUMMARY_URI => Ok(uri.to_string()),
        other => Err((-32602, format!("Resource not found: {other}"))),
    }
}

fn instructions(attached: bool) -> String {
    let status = if attached {
        ATTACHED_INSTRUCTIONS
    } else {
        DETACHED_INSTRUCTIONS
    };
    format!("{INSTRUCTIONS} {status}")
}

fn initialize_result(attached: bool) -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": capabilities(),
        "serverInfo": server_info(),
        "instructions": instructions(attached)
    })
}

fn discover_result(attached: bool) -> Value {
    json!({
        "supportedVersions": [LATEST_PROTOCOL_VERSION, PROTOCOL_VERSION],
        "capabilities": capabilities(),
        "instructions": instructions(attached),
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

fn prompts_list_result() -> Value {
    cacheable(json!({
        "prompts": [
            {
                "name": "scene_overview",
                "title": "LeopardCraft scene overview",
                "description": "Summarise the open document: object counts, bounds, materials and render settings.",
                "arguments": []
            },
            {
                "name": "object_report",
                "title": "LeopardCraft object report",
                "description": "Describe one object's transform, modifiers, material and animation state.",
                "arguments": [{
                    "name": "object",
                    "description": "Object id or exact name.",
                    "required": true
                }]
            },
            {
                "name": "render_check",
                "title": "LeopardCraft render check",
                "description": "Review the render settings and prepare a deterministic render_image call.",
                "arguments": []
            }
        ]
    }))
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
        assert_eq!(
            response["result"]["capabilities"]["resources"]["subscribe"],
            true
        );
        assert_eq!(
            response["result"]["capabilities"]["prompts"]["listChanged"],
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
            "add_modifier",
            "update_modifier",
            "remove_modifier",
            "move_modifier",
            "get_material",
            "create_material",
            "update_material",
            "get_animation",
            "set_animation_key",
            "remove_animation_key",
            "create_take",
            "activate_take",
            "update_take",
            "delete_take",
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
    fn resource_subscriptions_track_document_changes() {
        let mut server = server();
        let response = request(
            &mut server,
            "resources/subscribe",
            json!({ "uri": SCENE_URI }),
        );
        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["resultType"], "complete");
        assert!(
            server.take_notifications().is_empty(),
            "subscribing is not a document change"
        );
        request(&mut server, "resources/list", json!({}));
        assert!(
            server.take_notifications().is_empty(),
            "reading the document queues nothing"
        );

        let response = request(
            &mut server,
            "tools/call",
            json!({ "name": "create_primitive", "arguments": { "primitive": "box" } }),
        );
        assert_eq!(response["result"]["isError"], false);
        let updates = server.take_notifications();
        assert_eq!(updates.len(), 1, "one update per subscription");
        assert_eq!(updates[0]["jsonrpc"], "2.0");
        assert_eq!(updates[0]["method"], "notifications/resources/updated");
        assert_eq!(updates[0]["params"]["uri"], SCENE_URI);

        // Undo changes the document again, so it notifies again.
        request(
            &mut server,
            "tools/call",
            json!({ "name": "undo", "arguments": {} }),
        );
        assert_eq!(server.take_notifications().len(), 1);

        // Unsubscribing stops the stream.
        let response = request(
            &mut server,
            "resources/unsubscribe",
            json!({ "uri": SCENE_URI }),
        );
        assert!(response.get("error").is_none(), "{response}");
        assert!(!server.has_subscriptions());
        request(
            &mut server,
            "tools/call",
            json!({ "name": "create_primitive", "arguments": { "primitive": "sphere" } }),
        );
        assert!(server.take_notifications().is_empty());

        // Unknown and missing resource URIs are invalid params, not silent no-ops.
        let response = request(
            &mut server,
            "resources/subscribe",
            json!({ "uri": "leopardcraft://missing" }),
        );
        assert_eq!(response["error"]["code"], -32602);
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Resource not found"));
        let response = request(&mut server, "resources/subscribe", json!({}));
        assert_eq!(response["error"]["code"], -32602);
        assert!(!server.has_subscriptions());
    }

    #[test]
    fn prompt_templates_fill_from_the_live_document() {
        let mut server = server();
        let response = request(&mut server, "prompts/list", json!({}));
        let prompts = response["result"]["prompts"].as_array().unwrap();
        let names: Vec<&str> = prompts
            .iter()
            .map(|prompt| prompt["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["scene_overview", "object_report", "render_check"]);
        let report = prompts
            .iter()
            .find(|prompt| prompt["name"] == "object_report")
            .unwrap();
        assert_eq!(report["arguments"][0]["name"], "object");
        assert_eq!(report["arguments"][0]["required"], true);

        request(
            &mut server,
            "tools/call",
            json!({ "name": "create_primitive", "arguments": { "primitive": "box", "name": "Crate" } }),
        );

        // The overview prompt embeds the document as it stands now.
        let response = request(
            &mut server,
            "prompts/get",
            json!({ "name": "scene_overview" }),
        );
        assert!(response.get("error").is_none(), "{response}");
        let text = response["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap();
        assert!(text.contains("\"meshes\": 1"), "{text}");

        // The object prompt uses the same id-or-name lookup as the tools.
        let response = request(
            &mut server,
            "prompts/get",
            json!({ "name": "object_report", "arguments": { "object": "Crate" } }),
        );
        let text = response["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap();
        assert!(text.contains("\"name\": \"Crate\""), "{text}");

        // Missing, unknown and unresolvable arguments are invalid params.
        let response = request(
            &mut server,
            "prompts/get",
            json!({ "name": "object_report" }),
        );
        assert_eq!(response["error"]["code"], -32602);
        let response = request(
            &mut server,
            "prompts/get",
            json!({ "name": "object_report", "arguments": { "object": "Nope" } }),
        );
        assert_eq!(response["error"]["code"], -32602);
        let response = request(
            &mut server,
            "prompts/get",
            json!({ "name": "no_such_prompt" }),
        );
        assert_eq!(response["error"]["code"], -32602);
    }

    #[test]
    fn stdio_serve_writes_subscription_notifications() {
        let mut server = server();
        let input = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"resources/subscribe\",",
            "\"params\":{\"uri\":\"leopardcraft://scene/summary\"}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",",
            "\"params\":{\"name\":\"create_primitive\",\"arguments\":{\"primitive\":\"box\"}}}\n"
        );
        let mut output = Vec::new();
        server
            .serve(std::io::Cursor::new(input.as_bytes()), &mut output)
            .expect("stdio serve loop");
        let text = String::from_utf8(output).expect("utf-8 output");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines.len(),
            3,
            "two responses and one notification: {lines:?}"
        );
        assert!(lines[0].contains("\"id\":1"), "{}", lines[0]);
        assert!(lines[1].contains("\"id\":2"), "{}", lines[1]);
        let notification: Value = serde_json::from_str(lines[2]).expect("notification json");
        assert_eq!(notification["method"], "notifications/resources/updated");
        assert_eq!(notification["params"]["uri"], SUMMARY_URI);
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

    #[test]
    fn modifier_tools_edit_the_stack_with_one_undo_each() {
        let mut server = server();
        let id = structured(&tool(
            &mut server,
            "create_primitive",
            json!({ "primitive": "box", "name": "Crate" }),
        ))["id"]
            .as_u64()
            .unwrap();
        let depth = structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"]
            .as_u64()
            .unwrap();

        let added = structured(&tool(
            &mut server,
            "add_modifier",
            json!({
                "object": id,
                "modifier": { "type": "subdivision", "levels": 2, "scheme": "catmull_clark" }
            }),
        ))
        .clone();
        assert_eq!(added["modifiers"][0]["type"], "subdivision");
        assert_eq!(
            structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"],
            json!(depth + 1)
        );

        let inserted = structured(&tool(
            &mut server,
            "add_modifier",
            json!({
                "object": "Crate",
                "index": 0,
                "modifier": { "type": "array", "count": 2, "offset": [1.0, 0.0, 0.0] }
            }),
        ))
        .clone();
        assert_eq!(inserted["modifiers"][0]["type"], "array");
        assert_eq!(inserted["modifiers"][1]["type"], "subdivision");

        let updated = structured(&tool(
            &mut server,
            "update_modifier",
            json!({
                "object": id,
                "index": 0,
                "modifier": { "type": "array", "count": 3, "offset": [2.0, 0.0, 0.0] }
            }),
        ))
        .clone();
        assert_eq!(updated["modifiers"][0]["count"], 3);

        let moved = structured(&tool(
            &mut server,
            "move_modifier",
            json!({ "object": id, "from": 0, "to": 1 }),
        ))
        .clone();
        assert_eq!(moved["modifiers"][0]["type"], "subdivision");
        assert_eq!(moved["modifiers"][1]["type"], "array");

        let removed = structured(&tool(
            &mut server,
            "remove_modifier",
            json!({ "object": id, "index": 1 }),
        ))
        .clone();
        assert_eq!(removed["modifiers"].as_array().unwrap().len(), 1);
        assert_eq!(removed["removed"]["type"], "array");

        tool(&mut server, "undo", json!({}));
        let object = structured(&tool(&mut server, "get_object", json!({ "object": id }))).clone();
        assert_eq!(object["modifiers"].as_array().unwrap().len(), 2);
        assert_eq!(object["modifiers"][1]["type"], "array");
        tool(&mut server, "undo", json!({}));
        tool(&mut server, "undo", json!({}));
        let object = structured(&tool(&mut server, "get_object", json!({ "object": id }))).clone();
        assert_eq!(object["modifiers"].as_array().unwrap().len(), 2);
        assert_eq!(object["modifiers"][0]["count"], 2);
        tool(&mut server, "undo", json!({}));
        let object = structured(&tool(&mut server, "get_object", json!({ "object": id }))).clone();
        assert_eq!(object["modifiers"].as_array().unwrap().len(), 1);
        tool(&mut server, "redo", json!({}));
        let object = structured(&tool(&mut server, "get_object", json!({ "object": id }))).clone();
        assert_eq!(object["modifiers"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn modifier_tools_reject_bad_targets_and_indices_without_undo() {
        let mut server = McpServer::new(Document::demo(), None);
        let camera = structured(&tool(&mut server, "list_objects", json!({})))["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|object| object["type"] == "Camera")
            .expect("demo scene has a camera")["id"]
            .as_u64()
            .unwrap();
        let box_id = structured(&tool(
            &mut server,
            "create_primitive",
            json!({ "primitive": "box", "name": "Box" }),
        ))["id"]
            .as_u64()
            .unwrap();
        let depth = structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"]
            .as_u64()
            .unwrap();

        let error = tool(
            &mut server,
            "add_modifier",
            json!({
                "object": camera,
                "modifier": { "type": "subdivision", "levels": 2 }
            }),
        );
        assert_eq!(error["isError"], json!(true));

        let error = tool(
            &mut server,
            "add_modifier",
            json!({
                "object": box_id,
                "index": 3,
                "modifier": { "type": "subdivision", "levels": 2 }
            }),
        );
        assert_eq!(error["isError"], json!(true));

        let error = tool(
            &mut server,
            "add_modifier",
            json!({
                "object": box_id,
                "modifier": { "type": "boolean", "operand": box_id, "operation": "union" }
            }),
        );
        assert_eq!(error["isError"], json!(true));

        let error = tool(
            &mut server,
            "add_modifier",
            json!({
                "object": box_id,
                "modifier": { "type": "wrap", "target": box_id, "angle_deg": 90.0 }
            }),
        );
        assert_eq!(error["isError"], json!(true));

        let error = tool(
            &mut server,
            "add_modifier",
            json!({
                "object": box_id,
                "modifier": { "type": "wrap", "target": camera, "angle_deg": 90.0 }
            }),
        );
        assert_eq!(error["isError"], json!(true));

        let error = tool(
            &mut server,
            "add_modifier",
            json!({
                "object": box_id,
                "modifier": { "type": "formula", "x": "x +", "y": "y", "z": "z" }
            }),
        );
        assert_eq!(error["isError"], json!(true));

        let added = structured(&tool(
            &mut server,
            "add_modifier",
            json!({
                "object": box_id,
                "modifier": { "type": "laplacian_smooth", "iterations": 2, "strength": 0.5 }
            }),
        ))
        .clone();
        assert_eq!(added["modifiers"][0]["type"], "laplacian_smooth");
        tool(&mut server, "undo", json!({}));

        let error = tool(
            &mut server,
            "update_modifier",
            json!({
                "object": box_id,
                "index": 0,
                "modifier": { "type": "subdivision", "levels": 1 }
            }),
        );
        assert_eq!(error["isError"], json!(true));

        let error = tool(
            &mut server,
            "remove_modifier",
            json!({ "object": box_id, "index": 0 }),
        );
        assert_eq!(error["isError"], json!(true));

        let error = tool(
            &mut server,
            "move_modifier",
            json!({ "object": box_id, "from": 0, "to": 0 }),
        );
        assert_eq!(error["isError"], json!(true));

        assert_eq!(
            structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"],
            json!(depth)
        );
    }
    #[test]
    fn modifier_stack_roundtrips_through_native_save_and_open() {
        let mut server = server();
        let id = structured(&tool(
            &mut server,
            "create_primitive",
            json!({ "primitive": "box", "name": "Crate" }),
        ))["id"]
            .as_u64()
            .unwrap();
        tool(
            &mut server,
            "add_modifier",
            json!({
                "object": id,
                "modifier": { "type": "subdivision", "levels": 2, "scheme": "stam_loop" }
            }),
        );
        tool(
            &mut server,
            "add_modifier",
            json!({
                "object": id,
                "index": 0,
                "modifier": { "type": "array", "count": 3, "offset": [1.0, 0.0, 0.0] }
            }),
        );
        let path = temp_dir().join("modifiers.lcscene");
        tool(
            &mut server,
            "save_scene",
            json!({ "path": path.to_string_lossy() }),
        );
        tool(&mut server, "new_scene", json!({ "template": "empty" }));
        tool(
            &mut server,
            "open_scene",
            json!({ "path": path.to_string_lossy() }),
        );
        let object = structured(&tool(&mut server, "get_object", json!({ "object": id }))).clone();
        assert_eq!(object["modifiers"].as_array().unwrap().len(), 2);
        assert_eq!(object["modifiers"][0]["type"], "array");
        assert_eq!(object["modifiers"][0]["count"], 3);
        assert_eq!(object["modifiers"][1]["scheme"], "stam_loop");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
    #[test]
    fn material_graph_tools_create_update_and_undo() {
        let mut server = McpServer::new(Document::demo(), None);
        let original = structured(&tool(
            &mut server,
            "get_material",
            json!({ "material": "Studio Grey" }),
        ))
        .clone();
        let mut graph = original["graph"].clone();
        let next = graph["next_id"].as_u64().unwrap();
        graph["nodes"].as_array_mut().unwrap().push(json!({
            "id": next,
            "kind": { "type": "value", "value": 0.25 },
            "position": [0.0, 180.0],
            "inputs": []
        }));
        graph["next_id"] = json!(next + 1);
        let depth = structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"]
            .as_u64()
            .unwrap();

        let updated = structured(&tool(
            &mut server,
            "update_material",
            json!({
                "material": "Studio Grey",
                "graph": graph,
                "double_sided": false
            }),
        ))
        .clone();
        assert_eq!(updated["double_sided"], false);
        assert_eq!(
            updated["graph"]["nodes"].as_array().unwrap().len(),
            original["graph"]["nodes"].as_array().unwrap().len() + 1
        );
        assert_eq!(
            structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"],
            json!(depth + 1)
        );

        tool(&mut server, "undo", json!({}));
        let undone = structured(&tool(
            &mut server,
            "get_material",
            json!({ "material": "Studio Grey" }),
        ))
        .clone();
        assert_eq!(undone["double_sided"], true);
        assert_eq!(
            undone["graph"]["nodes"].as_array().unwrap().len(),
            original["graph"]["nodes"].as_array().unwrap().len()
        );
        tool(&mut server, "redo", json!({}));
        assert_eq!(
            structured(&tool(
                &mut server,
                "get_material",
                json!({ "material": "Studio Grey" })
            ))["graph"]["nodes"]
                .as_array()
                .unwrap()
                .len(),
            original["graph"]["nodes"].as_array().unwrap().len() + 1
        );

        let created = structured(&tool(
            &mut server,
            "create_material",
            json!({
                "name": "Emissive",
                "base_color": [1.0, 0.2, 0.1],
                "roughness": 0.3,
                "metallic": 0.1,
                "casts_shadow": false
            }),
        ))
        .clone();
        assert_eq!(created["name"], "Emissive");
        assert_eq!(created["casts_shadow"], false);
        let error = tool(
            &mut server,
            "create_material",
            json!({ "name": "emissive" }),
        );
        assert_eq!(error["isError"], json!(true));

        let renamed = structured(&tool(
            &mut server,
            "update_material",
            json!({ "material": "Emissive", "name": "Glow" }),
        ))
        .clone();
        assert_eq!(renamed["name"], "Glow");
        tool(&mut server, "undo", json!({}));
        assert_eq!(
            structured(&tool(
                &mut server,
                "get_material",
                json!({ "material": "Emissive" })
            ))["name"],
            "Emissive"
        );
    }

    #[test]
    fn material_graph_tools_reject_malformed_graphs_without_undo() {
        let mut server = McpServer::new(Document::demo(), None);
        let original = structured(&tool(
            &mut server,
            "get_material",
            json!({ "material": "Studio Grey" }),
        ))
        .clone();
        let depth = structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"]
            .as_u64()
            .unwrap();
        let count = structured(&tool(&mut server, "list_materials", json!({})))["materials"]
            .as_array()
            .unwrap()
            .len();

        let mut stale = original["graph"].clone();
        stale["next_id"] = json!(0);
        let error = tool(
            &mut server,
            "update_material",
            json!({ "material": "Studio Grey", "graph": stale }),
        );
        assert_eq!(error["isError"], json!(true));

        let mut dangling = original["graph"].clone();
        dangling["links"][0]["from"] = json!(99);
        let error = tool(
            &mut server,
            "update_material",
            json!({ "material": "Studio Grey", "graph": dangling }),
        );
        assert_eq!(error["isError"], json!(true));

        let error = tool(
            &mut server,
            "create_material",
            json!({
                "name": "Broken",
                "graph": {
                    "nodes": [{ "id": 0, "kind": { "type": "material_output" }, "position": [0.0, 0.0], "inputs": [] }],
                    "links": [{ "from": 9, "to": 0, "to_input": 0 }],
                    "next_id": 1
                }
            }),
        );
        assert_eq!(error["isError"], json!(true));

        assert_eq!(
            structured(&tool(&mut server, "list_materials", json!({})))["materials"]
                .as_array()
                .unwrap()
                .len(),
            count
        );
        assert_eq!(
            structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"],
            json!(depth)
        );
    }

    #[test]
    fn material_graph_roundtrips_through_native_save_and_open() {
        let mut server = McpServer::new(Document::demo(), None);
        let original = structured(&tool(
            &mut server,
            "get_material",
            json!({ "material": "Studio Grey" }),
        ))
        .clone();
        let mut graph = original["graph"].clone();
        let next = graph["next_id"].as_u64().unwrap();
        graph["nodes"].as_array_mut().unwrap().push(json!({
            "id": next,
            "kind": { "type": "value", "value": 0.5 },
            "position": [0.0, 90.0],
            "inputs": []
        }));
        graph["next_id"] = json!(next + 1);
        tool(
            &mut server,
            "create_material",
            json!({ "name": "Roundtrip", "graph": graph }),
        );
        let path = temp_dir().join("materials.lcscene");
        tool(
            &mut server,
            "save_scene",
            json!({ "path": path.to_string_lossy() }),
        );
        tool(&mut server, "new_scene", json!({ "template": "empty" }));
        tool(
            &mut server,
            "open_scene",
            json!({ "path": path.to_string_lossy() }),
        );
        let loaded = structured(&tool(
            &mut server,
            "get_material",
            json!({ "material": "Roundtrip" }),
        ))
        .clone();
        assert_eq!(
            loaded["graph"]["nodes"].as_array().unwrap().len(),
            original["graph"]["nodes"].as_array().unwrap().len() + 1
        );
        assert_eq!(
            loaded["graph"]["links"].as_array().unwrap().len(),
            original["graph"]["links"].as_array().unwrap().len()
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
    #[test]
    fn animation_key_tools_edit_the_clip_and_undo() {
        let mut server = McpServer::new(Document::demo(), None);
        let depth = structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"]
            .as_u64()
            .unwrap();
        tool(
            &mut server,
            "set_animation_key",
            json!({
                "object": "Cube",
                "channel": "translation",
                "component": 0,
                "time": 0.0,
                "value": 0.0,
                "interpolation": "linear"
            }),
        );
        tool(
            &mut server,
            "set_animation_key",
            json!({
                "object": "Cube",
                "channel": "translation",
                "component": 0,
                "time": 1.0,
                "value": 2.0,
                "interpolation": "linear"
            }),
        );
        tool(
            &mut server,
            "set_animation_key",
            json!({
                "object": "Cube",
                "channel": "rotation",
                "time": 0.0,
                "rotation": [0.0, 0.0, 0.0, 1.0]
            }),
        );
        tool(
            &mut server,
            "set_animation_key",
            json!({
                "object": "Cube",
                "channel": "rotation",
                "time": 1.0,
                "rotation": [0.0, 0.70710677, 0.0, 0.70710677],
                "interpolation": "linear"
            }),
        );
        let clip = structured(&tool(
            &mut server,
            "get_animation",
            json!({ "object": "Cube" }),
        ))["clip"]
            .clone();
        assert_eq!(
            clip["tracks"][0]["curve"]["keys"].as_array().unwrap().len(),
            2
        );
        assert_eq!(clip["rotation"]["keys"].as_array().unwrap().len(), 2);
        assert_eq!(
            structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"],
            json!(depth + 4)
        );

        tool(
            &mut server,
            "remove_animation_key",
            json!({
                "object": "Cube",
                "channel": "translation",
                "component": 0,
                "time": 0.0
            }),
        );
        let clip = structured(&tool(
            &mut server,
            "get_animation",
            json!({ "object": "Cube" }),
        ))["clip"]
            .clone();
        assert_eq!(
            clip["tracks"][0]["curve"]["keys"].as_array().unwrap().len(),
            1
        );
        tool(&mut server, "undo", json!({}));
        let clip = structured(&tool(
            &mut server,
            "get_animation",
            json!({ "object": "Cube" }),
        ))["clip"]
            .clone();
        assert_eq!(
            clip["tracks"][0]["curve"]["keys"].as_array().unwrap().len(),
            2
        );
        tool(&mut server, "redo", json!({}));
        let clip = structured(&tool(
            &mut server,
            "get_animation",
            json!({ "object": "Cube" }),
        ))["clip"]
            .clone();
        assert_eq!(
            clip["tracks"][0]["curve"]["keys"].as_array().unwrap().len(),
            1
        );

        let after = structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"]
            .as_u64()
            .unwrap();
        let error = tool(
            &mut server,
            "set_animation_key",
            json!({
                "object": "Cube",
                "channel": "translation",
                "component": 0,
                "time": 1.0,
                "value": 2.0,
                "interpolation": "linear"
            }),
        );
        assert_eq!(error["isError"], json!(true));
        let error = tool(
            &mut server,
            "set_animation_key",
            json!({
                "object": "Cube",
                "channel": "fov",
                "time": 0.0,
                "value": 45.0
            }),
        );
        assert_eq!(error["isError"], json!(true));
        let error = tool(
            &mut server,
            "set_animation_key",
            json!({
                "object": "Cube",
                "channel": "translation",
                "component": 4,
                "time": 0.0,
                "value": 1.0
            }),
        );
        assert_eq!(error["isError"], json!(true));
        let error = tool(
            &mut server,
            "set_animation_key",
            json!({
                "object": "Cube",
                "channel": "translation",
                "time": -1.0,
                "value": 1.0
            }),
        );
        assert_eq!(error["isError"], json!(true));
        let error = tool(
            &mut server,
            "remove_animation_key",
            json!({
                "object": "Cube",
                "channel": "scale",
                "component": 0,
                "time": 0.0
            }),
        );
        assert_eq!(error["isError"], json!(true));
        assert_eq!(
            structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"],
            json!(after)
        );
    }

    #[test]
    fn animation_take_tools_create_activate_update_and_delete() {
        let mut server = McpServer::new(Document::demo(), None);
        let original_takes = structured(&tool(
            &mut server,
            "get_animation",
            json!({ "object": "Cube" }),
        ))["takes"]
            .as_array()
            .unwrap()
            .len();
        let depth = structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"]
            .as_u64()
            .unwrap();

        let created =
            structured(&tool(&mut server, "create_take", json!({ "name": "Walk" }))).clone();
        assert_eq!(
            created["takes"].as_array().unwrap().len(),
            original_takes + 1
        );
        assert_eq!(
            created["takes"].as_array().unwrap().last().unwrap()["name"],
            "Walk"
        );
        assert_eq!(
            created["takes"].as_array().unwrap().last().unwrap()["active"],
            true
        );
        assert_eq!(
            structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"],
            json!(depth + 1)
        );

        tool(
            &mut server,
            "set_animation_key",
            json!({
                "object": "Cube",
                "channel": "translation",
                "component": 1,
                "time": 0.5,
                "value": 3.0
            }),
        );
        tool(&mut server, "activate_take", json!({ "index": 0 }));
        assert_eq!(
            structured(&tool(
                &mut server,
                "get_animation",
                json!({ "object": "Cube" })
            ))["clip"]["tracks"]
                .as_array()
                .map(|tracks| tracks.len())
                .unwrap_or(0),
            0
        );
        tool(&mut server, "activate_take", json!({ "index": 1 }));
        let clip = structured(&tool(
            &mut server,
            "get_animation",
            json!({ "object": "Cube" }),
        ))["clip"]
            .clone();
        assert_eq!(
            clip["tracks"][0]["curve"]["keys"].as_array().unwrap().len(),
            1
        );

        let updated = structured(&tool(
            &mut server,
            "update_take",
            json!({ "index": 1, "name": "Sprint", "fps": 30.0 }),
        ))
        .clone();
        assert_eq!(updated["takes"][1]["name"], "Sprint");
        assert_eq!(updated["takes"][1]["fps"], 30.0);

        let deleted = structured(&tool(&mut server, "delete_take", json!({ "index": 1 }))).clone();
        assert_eq!(deleted["takes"].as_array().unwrap().len(), original_takes);
        tool(&mut server, "undo", json!({}));
        let restored = structured(&tool(
            &mut server,
            "get_animation",
            json!({ "object": "Cube" }),
        ))
        .clone();
        assert_eq!(
            restored["takes"].as_array().unwrap().len(),
            original_takes + 1
        );
        assert_eq!(restored["takes"][1]["name"], "Sprint");

        let after = structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"]
            .as_u64()
            .unwrap();
        let error = tool(&mut server, "activate_take", json!({ "index": 9 }));
        assert_eq!(error["isError"], json!(true));
        let error = tool(
            &mut server,
            "update_take",
            json!({ "index": 1, "fps": 1000.0 }),
        );
        assert_eq!(error["isError"], json!(true));
        let error = tool(
            &mut server,
            "update_take",
            json!({ "index": 1, "name": " " }),
        );
        assert_eq!(error["isError"], json!(true));
        let error = tool(&mut server, "delete_take", json!({ "index": 9 }));
        assert_eq!(error["isError"], json!(true));
        assert_eq!(
            structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"],
            json!(after)
        );
    }

    #[test]
    fn mesh_component_tools_transform_selection_and_undo() {
        let mut server = McpServer::new(Document::demo(), None);
        let original =
            structured(&tool(&mut server, "get_mesh", json!({ "object": "Cube" }))).clone();
        let original_positions = original["mesh"]["positions"].clone();
        let depth = structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"]
            .as_u64()
            .unwrap();

        let one = structured(&tool(
            &mut server,
            "edit_mesh",
            json!({
                "object": "Cube",
                "operation": "transform",
                "points": [0],
                "translation": [1.0, 0.0, 0.0]
            }),
        ))
        .clone();
        assert_eq!(one["editable"], json!(true));
        assert_ne!(one["mesh"]["positions"][0], original_positions[0]);
        assert_eq!(one["mesh"]["positions"][1], original_positions[1]);
        assert_eq!(
            structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"],
            json!(depth + 1)
        );

        tool(&mut server, "undo", json!({}));
        let undone =
            structured(&tool(&mut server, "get_mesh", json!({ "object": "Cube" }))).clone();
        assert_eq!(undone["editable"], json!(false));
        assert_eq!(undone["mesh"]["positions"], original_positions);
        tool(&mut server, "redo", json!({}));
        let redone =
            structured(&tool(&mut server, "get_mesh", json!({ "object": "Cube" }))).clone();
        assert_eq!(redone["editable"], json!(true));
        assert_ne!(redone["mesh"]["positions"][0], original_positions[0]);

        let face_points: Vec<u64> = redone["mesh"]["polygons"][0]["corners"]
            .as_array()
            .unwrap()
            .iter()
            .map(|corner| corner["point"].as_u64().unwrap())
            .collect();
        let face = structured(&tool(
            &mut server,
            "edit_mesh",
            json!({
                "object": "Cube",
                "operation": "transform",
                "faces": [0],
                "scale": [2.0, 2.0, 2.0]
            }),
        ))
        .clone();
        assert_eq!(face["operation"], "transform");
        assert!(face_points
            .iter()
            .all(|point| face["mesh"]["positions"][*point as usize]
                != redone["mesh"]["positions"][*point as usize]));

        let after = structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"]
            .as_u64()
            .unwrap();
        for error_args in [
            json!({ "object": "Cube", "operation": "transform", "points": [], "translation": [1.0, 0.0, 0.0] }),
            json!({ "object": "Cube", "operation": "transform", "points": [999], "translation": [1.0, 0.0, 0.0] }),
            json!({ "object": "Cube", "operation": "transform", "faces": [999], "translation": [1.0, 0.0, 0.0] }),
            json!({ "object": "Cube", "operation": "transform", "edges": [[0, 1]], "translation": [1.0, 0.0, 0.0] }),
            json!({ "object": "Cube", "operation": "transform", "points": [0], "scale": [0.0, 1.0, 1.0] }),
            json!({ "object": "Cube", "operation": "transform", "points": [0] }),
            json!({ "object": "Cube", "operation": "transform" }),
        ] {
            assert_eq!(
                tool(&mut server, "edit_mesh", error_args)["isError"],
                json!(true)
            );
        }
        assert_eq!(
            tool(
                &mut server,
                "edit_mesh",
                json!({ "object": "Cube", "operation": "extrude", "faces": [], "distance": 1.0 })
            )["isError"],
            json!(true)
        );
        assert_eq!(
            tool(&mut server, "get_mesh", json!({ "object": "Camera" }))["isError"],
            json!(true)
        );
        assert_eq!(
            tool(
                &mut server,
                "edit_mesh",
                json!({ "object": "Camera", "operation": "flip" })
            )["isError"],
            json!(true)
        );
        assert_eq!(
            structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"],
            json!(after)
        );
    }

    #[test]
    fn mesh_edit_tools_cover_face_and_point_operations_and_empty_meshes() {
        let mut server = McpServer::new(Document::demo(), None);
        let original =
            structured(&tool(&mut server, "get_mesh", json!({ "object": "Cube" }))).clone();

        let extruded = structured(&tool(
            &mut server,
            "edit_mesh",
            json!({ "object": "Cube", "operation": "extrude", "faces": [0], "distance": 0.5 }),
        ))
        .clone();
        assert!(extruded["faces"].as_u64().unwrap() > original["faces"].as_u64().unwrap());
        tool(&mut server, "undo", json!({}));
        let inset = structured(&tool(
            &mut server,
            "edit_mesh",
            json!({ "object": "Cube", "operation": "inset", "faces": [0], "amount": 0.15 }),
        ))
        .clone();
        assert!(inset["faces"].as_u64().unwrap() > original["faces"].as_u64().unwrap());
        let bevel = structured(&tool(
            &mut server,
            "edit_mesh",
            json!({ "object": "Cube", "operation": "bevel", "faces": [0], "amount": 0.1, "depth": 0.05 }),
        ))
        .clone();
        assert!(bevel["faces"].as_u64().unwrap() > original["faces"].as_u64().unwrap());
        let triangulated = structured(&tool(
            &mut server,
            "edit_mesh",
            json!({ "object": "Cube", "operation": "triangulate", "faces": [0], "mode": "quad_pair" }),
        ))
        .clone();
        assert!(triangulated["faces"].as_u64().unwrap() > original["faces"].as_u64().unwrap());
        let flipped = structured(&tool(
            &mut server,
            "edit_mesh",
            json!({ "object": "Cube", "operation": "flip", "faces": [0, 1] }),
        ))
        .clone();
        assert_eq!(flipped["faces"], triangulated["faces"]);
        tool(&mut server, "undo", json!({}));
        let slid = structured(&tool(
            &mut server,
            "edit_mesh",
            json!({ "object": "Cube", "operation": "slide", "points": [0], "fraction": 0.5 }),
        ))
        .clone();
        assert_eq!(slid["operation"], "slide");
        tool(&mut server, "undo", json!({}));
        let optimized = structured(&tool(
            &mut server,
            "edit_mesh",
            json!({ "object": "Cube", "operation": "optimize", "points": [0, 1], "tolerance": 100.0 }),
        ))
        .clone();
        assert_eq!(optimized["operation"], "optimize");
        tool(&mut server, "undo", json!({}));
        let collapsed = structured(&tool(
            &mut server,
            "edit_mesh",
            json!({ "object": "Cube", "operation": "collapse", "points": [0, 1] }),
        ))
        .clone();
        assert_eq!(collapsed["operation"], "collapse");
        tool(&mut server, "undo", json!({}));
        let disconnected = structured(&tool(
            &mut server,
            "edit_mesh",
            json!({ "object": "Cube", "operation": "disconnect", "faces": [0] }),
        ))
        .clone();
        assert_eq!(disconnected["operation"], "disconnect");
        tool(&mut server, "undo", json!({}));
        tool(&mut server, "undo", json!({}));
        tool(&mut server, "undo", json!({}));
        tool(&mut server, "undo", json!({}));
        let after = structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"]
            .as_u64()
            .unwrap();
        for error_args in [
            json!({ "object": "Cube", "operation": "extrude", "faces": [999], "distance": 1.0 }),
            json!({ "object": "Cube", "operation": "triangulate", "mode": "nope" }),
            json!({ "object": "Cube", "operation": "optimize", "points": [999], "tolerance": 0.1 }),
            json!({ "object": "Cube", "operation": "slide", "points": [999], "fraction": 0.5 }),
            json!({ "object": "Cube", "operation": "bridge", "loops": [[0, 1, 2]] }),
            json!({ "object": "Cube", "operation": "not_an_operation" }),
        ] {
            assert_eq!(
                tool(&mut server, "edit_mesh", error_args)["isError"],
                json!(true)
            );
        }
        assert_eq!(
            structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"],
            json!(after)
        );

        let empty = server.doc.create_node(
            lc_scene::NodeKind::Mesh {
                geometry: lc_scene::GeometrySource::editable(lc_geom::Mesh::default()),
            },
            "Empty",
        );
        let empty_id = empty.id;
        server
            .doc
            .scene
            .insert_node(empty, None, None)
            .expect("empty mesh node inserts");
        let empty_mesh = structured(&tool(
            &mut server,
            "get_mesh",
            json!({ "object": empty_id }),
        ))
        .clone();
        assert_eq!(empty_mesh["faces"], 0);
        assert_eq!(empty_mesh["uv_sets"], 0);
        let before_empty = structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"]
            .as_u64()
            .unwrap();
        for error_args in [
            json!({ "object": empty_id, "operation": "flip" }),
            json!({ "object": empty_id, "operation": "extrude", "distance": 1.0 }),
            json!({ "object": empty_id, "operation": "slide", "fraction": 1.0 }),
            json!({ "object": empty_id, "operation": "transform", "points": [0], "translation": [1.0, 0.0, 0.0] }),
        ] {
            assert_eq!(
                tool(&mut server, "edit_mesh", error_args)["isError"],
                json!(true)
            );
        }
        assert_eq!(
            structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"],
            json!(before_empty)
        );
    }

    #[test]
    fn uv_edit_tools_project_sets_seams_charts_pins_and_undo() {
        let mut server = McpServer::new(Document::demo(), None);

        let projected = structured(&tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "project", "mode": "planar", "axis": 2, "scale": 2.0 }),
        ))
        .clone();
        assert_eq!(projected["uv_sets"], 1);
        let slots = projected["uv_slots"].as_u64().unwrap();
        assert!(slots >= 4);

        let added = structured(&tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "add_set" }),
        ))
        .clone();
        assert_eq!(added["uv_sets"], 2);
        let set = structured(&tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "set_uv", "uv_set": 1, "slot": 0, "u": 0.25, "v": 0.75 }),
        ))
        .clone();
        assert_eq!(set["mesh"]["extra_uvs"][0][0], json!([0.25, 0.75]));

        let mut groups: std::collections::BTreeMap<u64, Vec<u64>> =
            std::collections::BTreeMap::new();
        for polygon in set["mesh"]["polygons"].as_array().unwrap() {
            for corner in polygon["corners"].as_array().unwrap() {
                groups
                    .entry(corner["point"].as_u64().unwrap())
                    .or_default()
                    .push(corner["uv"].as_u64().unwrap());
            }
        }
        let duplicate: Vec<u64> = groups
            .values()
            .find(|slots| slots.len() >= 2)
            .cloned()
            .expect("projected cube shares UV slots at a point");
        let welded = structured(&tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "weld", "slots": duplicate }),
        ))
        .clone();
        assert_eq!(welded["operation"], "weld");
        let survivor = *duplicate.iter().min().unwrap();
        let split = structured(&tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "split", "slots": [survivor] }),
        ))
        .clone();
        assert_eq!(split["operation"], "split");
        for (slot, u, v) in [(0, 0.0, 0.1), (1, 0.2, 0.3), (2, 0.9, 0.7)] {
            tool(
                &mut server,
                "edit_uv",
                json!({ "object": "Cube", "operation": "set_uv", "slot": slot, "u": u, "v": v }),
            );
        }
        for operation in [
            json!({ "object": "Cube", "operation": "align", "slots": [0, 1], "axis": 0 }),
            json!({ "object": "Cube", "operation": "distribute", "slots": [0, 1, 2], "axis": 0 }),
            json!({ "object": "Cube", "operation": "flip", "slots": [0, 1], "axis": 1 }),
        ] {
            let ran = tool(&mut server, "edit_uv", operation.clone());
            assert_eq!(ran["isError"], json!(false), "{operation}");
        }
        tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "set_uv", "slot": 0, "u": 0.0, "v": 0.0 }),
        );
        tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "set_uv", "slot": 1, "u": 1.0, "v": 0.0 }),
        );
        let rotated = tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "rotate", "slots": [0, 1], "quarter_turns": 1 }),
        );
        assert_eq!(rotated["isError"], json!(false));

        let seam = structured(&tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "set_seam", "a": 0, "b": 1 }),
        ))
        .clone();
        assert_eq!(seam["seams"], json!([[0, 1]]));
        let cleared = structured(&tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "clear_seams" }),
        ))
        .clone();
        assert_eq!(cleared["seams"], json!([]));
        tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "set_seam", "a": 0, "b": 1 }),
        );
        let unwrapped = structured(&tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "unwrap", "max_angle_deg": 66.0, "padding": 0.01 }),
        ))
        .clone();
        assert!(unwrapped["uv_charts"].as_u64().unwrap() >= 1);
        let relaxed = structured(&tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "relax", "iterations": 2, "strength": 0.5, "pin_boundary": true }),
        ))
        .clone();
        assert_eq!(relaxed["uv_sets"], 2);
        tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "set_uv", "slot": 0, "u": 5.0, "v": 5.0 }),
        );
        let normalized = structured(&tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "normalize" }),
        ))
        .clone();
        assert_eq!(normalized["operation"], "normalize");
        assert!(normalized["mesh"]["uvs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|uv| uv[0].as_f64().unwrap() >= 0.0
                && uv[0].as_f64().unwrap() <= 1.0
                && uv[1].as_f64().unwrap() >= 0.0
                && uv[1].as_f64().unwrap() <= 1.0));
        let packed = structured(&tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "pack", "padding": 0.02, "rotate": true, "align": "center" }),
        ))
        .clone();
        assert!(packed["uv_charts"].as_u64().unwrap() >= 1);
        let transformed = structured(&tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "transform_charts", "charts": [], "translation": [0.1, 0.1], "rotation_degrees": 15.0, "scale": 0.9 }),
        ))
        .clone();
        assert_eq!(transformed["operation"], "transform_charts");
        let pinned = structured(&tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "set_pinned", "slot": 0 }),
        ))
        .clone();
        assert_eq!(pinned["pinned_uvs"], json!([0]));
        let unpinned = structured(&tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "set_pinned", "slot": 0, "pinned": false }),
        ))
        .clone();
        assert_eq!(unpinned["pinned_uvs"], json!([]));
        let removed = structured(&tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "remove_set", "uv_set": 1 }),
        ))
        .clone();
        assert_eq!(removed["uv_sets"], 1);
        tool(&mut server, "undo", json!({}));
        assert_eq!(
            structured(&tool(&mut server, "get_mesh", json!({ "object": "Cube" })))["uv_sets"],
            2
        );

        let after = structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"]
            .as_u64()
            .unwrap();
        for error_args in [
            json!({ "object": "Cube", "operation": "project", "mode": "linear" }),
            json!({ "object": "Cube", "operation": "project", "mode": "planar", "uv_set": 9 }),
            json!({ "object": "Cube", "operation": "remove_set", "uv_set": 0 }),
            json!({ "object": "Cube", "operation": "set_uv", "slot": 999, "u": 0.0, "v": 0.0 }),
            json!({ "object": "Cube", "operation": "set_seam", "a": 0, "b": 999 }),
            json!({ "object": "Cube", "operation": "set_seam", "a": 0, "b": 6 }),
            json!({ "object": "Cube", "operation": "pack", "charts": [999] }),
            json!({ "object": "Cube", "operation": "weld", "slots": [] }),
        ] {
            assert_eq!(
                tool(&mut server, "edit_uv", error_args)["isError"],
                json!(true)
            );
        }
        assert_eq!(
            structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"],
            json!(after)
        );

        let bridge_mesh = lc_geom::Mesh::from_polygons(
            "bridge",
            vec![
                glam::Vec3::new(0.0, 0.0, 0.0),
                glam::Vec3::new(1.0, 0.0, 0.0),
                glam::Vec3::new(1.0, 0.0, 1.0),
                glam::Vec3::new(0.0, 0.0, 1.0),
                glam::Vec3::new(0.0, 1.0, 0.0),
                glam::Vec3::new(1.0, 1.0, 0.0),
                glam::Vec3::new(1.0, 1.0, 1.0),
                glam::Vec3::new(0.0, 1.0, 1.0),
            ],
            vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7]],
        );
        let bridge = server.doc.create_node(
            lc_scene::NodeKind::Mesh {
                geometry: lc_scene::GeometrySource::editable(bridge_mesh),
            },
            "Bridge",
        );
        let bridge_id = bridge.id;
        server
            .doc
            .scene
            .insert_node(bridge, None, None)
            .expect("bridge mesh node inserts");
        let bridged = structured(&tool(
            &mut server,
            "edit_mesh",
            json!({ "object": bridge_id, "operation": "bridge", "loops": [[0, 1, 2, 3], [4, 5, 6, 7]] }),
        ))
        .clone();
        assert!(bridged["faces"].as_u64().unwrap() > 2);

        let empty = server.doc.create_node(
            lc_scene::NodeKind::Mesh {
                geometry: lc_scene::GeometrySource::editable(lc_geom::Mesh::default()),
            },
            "EmptyUV",
        );
        let empty_id = empty.id;
        server
            .doc
            .scene
            .insert_node(empty, None, None)
            .expect("empty UV mesh node inserts");
        let before_empty = structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"]
            .as_u64()
            .unwrap();
        for error_args in [
            json!({ "object": empty_id, "operation": "add_set" }),
            json!({ "object": empty_id, "operation": "normalize" }),
            json!({ "object": empty_id, "operation": "set_uv", "slot": 0, "u": 0.0, "v": 0.0 }),
            json!({ "object": empty_id, "operation": "unwrap" }),
            json!({ "object": empty_id, "operation": "relax" }),
            json!({ "object": empty_id, "operation": "pack" }),
        ] {
            assert_eq!(
                tool(&mut server, "edit_uv", error_args)["isError"],
                json!(true)
            );
        }
        assert_eq!(
            structured(&tool(&mut server, "get_scene", json!({})))["undo"]["depth"],
            json!(before_empty)
        );
    }

    #[test]
    fn mesh_and_uv_edits_roundtrip_through_native_save_and_open() {
        let mut server = McpServer::new(Document::demo(), None);
        tool(
            &mut server,
            "edit_mesh",
            json!({ "object": "Cube", "operation": "extrude", "faces": [0], "distance": 0.4 }),
        );
        tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "project", "mode": "box", "scale": 1.0 }),
        );
        tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "add_set" }),
        );
        tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "set_uv", "uv_set": 1, "slot": 0, "u": 0.4, "v": 0.6 }),
        );
        tool(
            &mut server,
            "edit_uv",
            json!({ "object": "Cube", "operation": "set_seam", "a": 0, "b": 1 }),
        );
        let expected =
            structured(&tool(&mut server, "get_mesh", json!({ "object": "Cube" }))).clone();
        let path = temp_dir().join("mesh-uv.lcscene");
        tool(
            &mut server,
            "save_scene",
            json!({ "path": path.to_string_lossy() }),
        );
        tool(&mut server, "new_scene", json!({ "template": "empty" }));
        tool(
            &mut server,
            "open_scene",
            json!({ "path": path.to_string_lossy() }),
        );
        let loaded =
            structured(&tool(&mut server, "get_mesh", json!({ "object": "Cube" }))).clone();
        assert_eq!(loaded["faces"], expected["faces"]);
        assert_eq!(loaded["uv_sets"], expected["uv_sets"]);
        assert_eq!(loaded["uv_slots"], expected["uv_slots"]);
        assert_eq!(loaded["seams"], expected["seams"]);
        assert_eq!(loaded["mesh"]["positions"], expected["mesh"]["positions"]);
        assert_eq!(loaded["mesh"]["extra_uvs"], expected["mesh"]["extra_uvs"]);
        let uv = loaded["mesh"]["extra_uvs"][0][0].as_array().unwrap();
        assert!((uv[0].as_f64().unwrap() - 0.4).abs() < 1e-6);
        assert!((uv[1].as_f64().unwrap() - 0.6).abs() < 1e-6);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn animation_and_takes_roundtrip_through_native_save_and_open() {
        let mut server = McpServer::new(Document::demo(), None);
        tool(&mut server, "create_take", json!({ "name": "Walk" }));
        tool(
            &mut server,
            "set_animation_key",
            json!({
                "object": "Cube",
                "channel": "rotation",
                "time": 0.5,
                "rotation": [0.0, 0.0, 0.0, 1.0]
            }),
        );
        let path = temp_dir().join("animation.lcscene");
        tool(
            &mut server,
            "save_scene",
            json!({ "path": path.to_string_lossy() }),
        );
        tool(&mut server, "new_scene", json!({ "template": "empty" }));
        tool(
            &mut server,
            "open_scene",
            json!({ "path": path.to_string_lossy() }),
        );
        let loaded = structured(&tool(
            &mut server,
            "get_animation",
            json!({ "object": "Cube" }),
        ))
        .clone();
        assert_eq!(loaded["active_take"]["name"], "Walk");
        assert_eq!(loaded["takes"].as_array().unwrap().len(), 2);
        assert_eq!(
            loaded["clip"]["rotation"]["keys"].as_array().unwrap().len(),
            1
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
