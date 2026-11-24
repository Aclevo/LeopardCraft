//! Live-agent bridge host: the editor half of [`lc_mcp::bridge`].
//!
//! The bridge endpoint never touches the document. It hands requests to this
//! module, which the UI thread calls once per frame. Every request is answered
//! before the next frame, so tool calls serialise on the editor's thread, and a
//! tool that edits the document runs under the editor's own undo history,
//! exactly like a panel button. Selection changes are editor state, so they are
//! not undo entries.

use super::selection::Selection;
use lc_mcp::bridge::selection_payload;
use lc_mcp::McpServer;
use lc_scene::{Document, History};
use serde_json::{json, Map, Value};
use std::path::PathBuf;

/// The editor state a live request may read or change.
pub(super) struct LiveSession<'a> {
    pub doc: &'a mut Document,
    pub history: &'a mut History,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub selection: &'a mut Selection,
}

/// Answer one bridge request. Errors leave no partial edit: every mutation runs
/// through a command or is validated before anything changes.
pub(super) fn handle(session: &mut LiveSession<'_>, request: &Value) -> Result<Value, String> {
    let op = request
        .get("op")
        .and_then(Value::as_str)
        .ok_or("bridge request has no 'op'")?;
    let arguments = request
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    match op {
        "snapshot" => Ok(snapshot(session)),
        "scene" => lc_scene::to_json(session.doc).map(Value::String),
        "select" => select(session, &arguments),
        "tool" => tool(session, &arguments),
        other => Err(format!("unknown bridge op '{other}'")),
    }
}

fn selection_snapshot(session: &LiveSession<'_>) -> Value {
    let ids = session.selection.ids();
    let names = ids
        .iter()
        .map(|id| {
            session
                .doc
                .scene
                .get(*id)
                .map(|node| node.name.clone())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    selection_payload(ids, &names)
}

fn snapshot(session: &LiveSession<'_>) -> Value {
    let objects = session
        .doc
        .scene
        .nodes
        .iter()
        .map(|(id, node)| {
            json!({
                "id": id,
                "name": node.name,
                "kind": format!("{:?}", node.kind),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "revision": session.doc.signature(),
        "dirty": session.dirty,
        "path": session.path.as_ref().map(|path| path.display().to_string()),
        "objects": objects,
        "selection": selection_snapshot(session),
        "undo_depth": session.history.depth(),
        "can_undo": session.history.can_undo(),
        "can_redo": session.history.can_redo(),
    })
}

fn select(session: &mut LiveSession<'_>, arguments: &Value) -> Result<Value, String> {
    let objects = arguments
        .get("objects")
        .and_then(Value::as_array)
        .ok_or("'objects' must be an array of object ids or exact names")?;
    let mode = arguments
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("replace");
    if !matches!(mode, "replace" | "add" | "toggle") {
        return Err(format!(
            "unknown selection mode '{mode}'; use replace, add or toggle"
        ));
    }
    // Resolve everything first, so an unknown name leaves the selection as it
    // was rather than half-applied.
    let mut ids = Vec::with_capacity(objects.len());
    for object in objects {
        let id = match object {
            Value::Number(number) => number
                .as_u64()
                .ok_or("object ids must be positive integers")?,
            Value::String(name) => session
                .doc
                .scene
                .find_by_name(name)
                .ok_or_else(|| format!("no object named '{name}'"))?,
            _ => return Err("objects must be numeric ids or exact names".into()),
        };
        if session.doc.scene.get(id).is_none() {
            return Err(format!("no object with id {id}"));
        }
        ids.push(id);
    }
    match mode {
        "replace" => session.selection.set(ids),
        "add" => {
            for id in ids {
                session.selection.add(id);
            }
        }
        _ => {
            for id in ids {
                session.selection.toggle(id);
            }
        }
    }
    Ok(selection_snapshot(session))
}

fn tool(session: &mut LiveSession<'_>, arguments: &Value) -> Result<Value, String> {
    let name = arguments
        .get("name")
        .and_then(Value::as_str)
        .ok_or("tool request has no 'name'")?;
    let tool_arguments = match arguments.get("arguments") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(map)) => map.clone(),
        Some(_) => return Err("tool arguments must be an object".into()),
    };
    // Run the tool against the editor's own document and history, then hand
    // both back. The editor stays the only document owner, and the tool's
    // command lands in the editor's undo stack as one entry.
    let doc = std::mem::replace(session.doc, Document::demo());
    let history = std::mem::replace(session.history, History::new(256));
    let mut server = McpServer::with_history(doc, session.path.clone(), history);
    let result = server.call_tool_value(name, &tool_arguments);
    let (doc, path, history) = server.into_parts();
    *session.doc = doc;
    *session.history = history;
    session.path = path;
    session.selection.retain_existing(&session.doc.scene);
    let mut payload = result?;
    // `get_scene` is the agent's overview of the live session, so it carries the
    // editor's selection there is no separate tool for.
    if name == "get_scene" {
        if let Some(map) = payload.as_object_mut() {
            map.insert("selection".into(), selection_snapshot(session));
        }
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{Duration, Instant};

    #[test]
    fn editor_host_services_bridge_requests_over_the_loopback_endpoint() {
        let registry = std::env::temp_dir().join(format!(
            "leopardcraft-live-host-{}.json",
            std::process::id()
        ));
        let server = lc_mcp::bridge::BridgeServer::bind("test", "secret".into(), registry).unwrap();
        let client = lc_mcp::LiveClient::new(lc_mcp::EditorEntry {
            name: "test".into(),
            port: server.port(),
            pid: std::process::id(),
            token: "secret".into(),
        });
        let mut doc = Document::demo();
        let mut history = History::new(256);
        let mut selection = Selection::default();
        let selected = {
            let mut session = LiveSession {
                doc: &mut doc,
                history: &mut history,
                path: None,
                dirty: false,
                selection: &mut selection,
            };
            let caller = std::thread::spawn(move || {
                client.command(
                    "tool",
                    json!({
                        "name": "create_primitive",
                        "arguments": { "primitive": "sphere", "name": "Live Sphere" }
                    }),
                )?;
                client.select(&[json!("Live Sphere")], "replace")
            });
            let deadline = Instant::now() + Duration::from_secs(10);
            while !caller.is_finished() {
                for job in server.drain() {
                    let answer = handle(&mut session, &job.request);
                    job.reply(answer);
                }
                assert!(Instant::now() < deadline, "the bridge request timed out");
                std::thread::sleep(Duration::from_millis(5));
            }
            caller
                .join()
                .unwrap()
                .expect("the editor answered both requests")
        };
        assert_eq!(selected["names"], json!(["Live Sphere"]));
        let sphere = doc.scene.find_by_name("Live Sphere").expect("created live");
        assert_eq!(selection.ids(), &[sphere]);
        assert_eq!(history.depth(), 1);
    }

    #[test]
    fn tool_requests_edit_the_live_document_under_its_own_history() {
        let mut doc = Document::demo();
        let mut history = History::new(256);
        let mut selection = Selection::default();
        let before = doc.scene.len();
        let mut session = LiveSession {
            doc: &mut doc,
            history: &mut history,
            path: None,
            dirty: false,
            selection: &mut selection,
        };

        let created = handle(
            &mut session,
            &json!({
                "op": "tool",
                "arguments": {
                    "name": "create_primitive",
                    "arguments": { "primitive": "box", "name": "Agent Box" }
                }
            }),
        )
        .expect("create_primitive runs on the live document");
        assert!(created.get("id").is_some(), "tool result: {created}");
        assert_eq!(session.doc.scene.len(), before + 1);
        assert!(session.doc.scene.find_by_name("Agent Box").is_some());
        assert_eq!(
            session.history.depth(),
            1,
            "a live tool call is exactly one undo entry"
        );

        let scene = handle(&mut session, &json!({ "op": "scene" })).unwrap();
        let text = scene.as_str().expect("scene JSON is text");
        assert!(text.contains("Agent Box"));

        let snapshot = handle(&mut session, &json!({ "op": "snapshot" })).unwrap();
        assert_eq!(snapshot["objects"].as_array().unwrap().len(), before + 1);
        assert_eq!(snapshot["undo_depth"], 1);
        assert_eq!(snapshot["can_undo"], true);

        // Undo goes through the same history, so the agent sees the editor's
        // real stack rather than a parallel one.
        handle(
            &mut session,
            &json!({ "op": "tool", "arguments": { "name": "undo", "arguments": {} } }),
        )
        .unwrap();
        assert!(session.doc.scene.find_by_name("Agent Box").is_none());
        assert_eq!(session.history.depth(), 0);
    }

    #[test]
    fn get_scene_reports_the_editor_selection() {
        let mut doc = Document::demo();
        let mut history = History::new(256);
        let mut selection = Selection::default();
        let cube = doc.scene.find_by_name("Cube").unwrap();
        let mut session = LiveSession {
            doc: &mut doc,
            history: &mut history,
            path: None,
            dirty: false,
            selection: &mut selection,
        };

        let answer = handle(
            &mut session,
            &json!({
                "op": "select",
                "arguments": { "objects": ["Cube"], "mode": "replace" }
            }),
        )
        .unwrap();
        assert_eq!(answer["names"], json!(["Cube"]));
        assert_eq!(session.selection.ids(), &[cube]);

        let overview = handle(
            &mut session,
            &json!({
                "op": "tool",
                "arguments": { "name": "get_scene", "arguments": {} }
            }),
        )
        .unwrap();
        assert_eq!(overview["selection"]["objects"], json!([cube]));
        assert_eq!(overview["selection"]["names"], json!(["Cube"]));
    }

    #[test]
    fn an_unknown_selection_name_leaves_the_selection_untouched() {
        let mut doc = Document::demo();
        let mut history = History::new(256);
        let mut selection = Selection::default();
        let cube = doc.scene.find_by_name("Cube").unwrap();
        selection.select_one(cube);
        let mut session = LiveSession {
            doc: &mut doc,
            history: &mut history,
            path: None,
            dirty: false,
            selection: &mut selection,
        };
        let error = handle(
            &mut session,
            &json!({
                "op": "select",
                "arguments": { "objects": ["Cube", "No Such Object"], "mode": "replace" }
            }),
        )
        .unwrap_err();
        assert!(error.contains("No Such Object"), "{error}");
        assert_eq!(session.selection.ids(), &[cube]);
    }
}
