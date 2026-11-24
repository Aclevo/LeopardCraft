//! Live-editor bridge: a loopback HTTP/JSON endpoint inside the editor, its
//! per-session token published in a user-only registry, and the matching client
//! for the MCP server.
//!
//! The editor is the only document owner. The endpoint never touches the
//! document itself: it validates the request (loopback `Origin` allow-list and
//! the session token), then hands it to the editor's UI thread and waits for the
//! answer. The UI thread services one request at a time, so a tool call that is
//! slow or waiting on a modal dialog answers `503 editor busy` instead of racing
//! the document.
//!
//! Trust material is generated from the operating system's entropy source
//! (`/dev/urandom`). Nothing here runs unless the user enables the bridge in the
//! editor, and the engine's evaluation, geometry and rendering paths stay
//! offline and deterministic.

use serde_json::{json, Map, Value};
use std::fs;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// The single endpoint that serves every bridge request.
pub const COMMAND_PATH: &str = "/command";
/// Environment override for the registry file, mostly for tests and packaging.
pub const REGISTRY_ENV: &str = "LEOPARDCRAFT_BRIDGE_REGISTRY";
/// Environment override that pins one editor endpoint, e.g.
/// `http://127.0.0.1:47011`, so several open editors stay addressable.
pub const ENDPOINT_ENV: &str = "LEOPARDCRAFT_EDITOR_ENDPOINT";
/// Token that belongs to [`ENDPOINT_ENV`] when it is set.
pub const TOKEN_ENV: &str = "LEOPARDCRAFT_EDITOR_TOKEN";

/// How long the endpoint waits for the editor's UI thread before it reports the
/// editor as busy.
const REPLY_TIMEOUT: Duration = Duration::from_secs(3);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;
const TOKEN_BYTES: usize = 32;

/// One running editor, as published in the registry file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorEntry {
    /// Human-readable document or window name, for picking between editors.
    pub name: String,
    pub port: u16,
    pub pid: u32,
    pub token: String,
}

impl EditorEntry {
    pub fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn to_value(&self) -> Value {
        json!({
            "name": self.name,
            "port": self.port,
            "pid": self.pid,
            "token": self.token,
        })
    }

    fn from_value(value: &Value) -> Option<Self> {
        let port = value.get("port")?.as_u64()?;
        if port == 0 || port > u16::MAX as u64 {
            return None;
        }
        Some(Self {
            name: value.get("name")?.as_str()?.to_string(),
            port: port as u16,
            pid: value.get("pid")?.as_u64()?.try_into().ok()?,
            token: value.get("token")?.as_str()?.to_string(),
        })
    }
}

/// Path of the registry that lists every running editor. `XDG_RUNTIME_DIR` is
/// the natural home for per-session state; without it the path falls back to the
/// per-user state directory and then to the temporary directory.
pub fn registry_path() -> PathBuf {
    if let Some(path) = std::env::var_os(REGISTRY_ENV) {
        return PathBuf::from(path);
    }
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime)
            .join("leopardcraft")
            .join("editors.json");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local/state/leopardcraft")
            .join("editors.json");
    }
    std::env::temp_dir()
        .join("leopardcraft")
        .join("editors.json")
}

/// Every entry currently published, including ones whose process is gone.
pub fn read_entries_at(path: &Path) -> Vec<EditorEntry> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    value
        .get("editors")
        .and_then(Value::as_array)
        .map(|entries| entries.iter().filter_map(EditorEntry::from_value).collect())
        .unwrap_or_default()
}

/// Replace the registry file. The directory is `0700` and the file `0600`, so
/// another local user cannot read a session token, and the write is atomic.
pub fn write_entries_at(path: &Path, entries: &[EditorEntry]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }
    let body = json!({ "editors": entries.iter().map(EditorEntry::to_value).collect::<Vec<_>>() });
    let text = serde_json::to_string_pretty(&body).map_err(|error| error.to_string())?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temporary, text).map_err(|error| format!("cannot write registry: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("cannot restrict registry permissions: {error}"))?;
    }
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("cannot publish registry: {error}")
    })
}

/// Add or replace this editor's entry. Entries whose process no longer exists
/// are dropped at the same time, so a crashed editor does not accumulate.
pub fn register_at(path: &Path, entry: &EditorEntry) -> Result<(), String> {
    let mut entries = read_entries_at(path);
    entries.retain(|existing| existing.port != entry.port && pid_alive(existing.pid));
    entries.push(entry.clone());
    write_entries_at(path, &entries)
}

/// Remove this editor's entry when the bridge shuts down.
pub fn unregister_at(path: &Path, port: u16, pid: u32) {
    let mut entries = read_entries_at(path);
    let before = entries.len();
    entries.retain(|entry| entry.port != port || entry.pid != pid);
    if entries.len() != before {
        let _ = write_entries_at(path, &entries);
    }
}

/// `true` when a process with that id is still running. Linux exposes this
/// through `/proc`; other platforms keep the entry, and the endpoint check in
/// [`LiveClient::discover`] rejects dead ones.
fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        true
    }
}

/// Editor entries that look live and answer a health check.
pub fn discover_entries_at(path: &Path) -> Vec<EditorEntry> {
    read_entries_at(path)
        .into_iter()
        .filter(|entry| pid_alive(entry.pid))
        .collect()
}

/// Find the editor to attach to. `LEOPARDCRAFT_EDITOR_ENDPOINT` pins one
/// instance and wins over the registry; a registry with several live editors is
/// an error rather than a guess, so the caller pins one explicitly.
pub fn discover() -> Result<EditorEntry, String> {
    if let Some(endpoint) = std::env::var_os(ENDPOINT_ENV) {
        let endpoint = endpoint.to_string_lossy().to_string();
        let port = endpoint
            .rsplit(':')
            .next()
            .and_then(|port| port.parse::<u16>().ok())
            .ok_or_else(|| format!("{ENDPOINT_ENV} must end in a port: {endpoint}"))?;
        let token = std::env::var(TOKEN_ENV).map_err(|_| {
            format!("{ENDPOINT_ENV} is set but {TOKEN_ENV} is missing; copy both from the editor")
        })?;
        return Ok(EditorEntry {
            name: "environment".into(),
            port,
            pid: 0,
            token,
        });
    }
    let entries = discover_entries_at(&registry_path());
    match entries.len() {
        0 => Err(
            "no running LeopardCraft editor was found; enable Tools -> Live Agent Bridge in the \
             editor, or set LEOPARDCRAFT_EDITOR_ENDPOINT and LEOPARDCRAFT_EDITOR_TOKEN"
                .into(),
        ),
        1 => Ok(entries.into_iter().next().expect("one entry")),
        _ => Err(format!(
            "{} editors are running ({}); pin one with {ENDPOINT_ENV} and {TOKEN_ENV}",
            entries.len(),
            entries
                .iter()
                .map(|entry| format!("{} on port {}", entry.name, entry.port))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// A fresh session token: 32 bytes from the operating system's entropy source,
/// hex-encoded. There is deliberately no fallback to time or a fixed seed.
pub fn new_token() -> Result<String, String> {
    let mut bytes = [0u8; TOKEN_BYTES];
    let mut source = fs::File::open("/dev/urandom")
        .map_err(|error| format!("cannot read /dev/urandom for a session token: {error}"))?;
    source
        .read_exact(&mut bytes)
        .map_err(|error| format!("cannot read a session token: {error}"))?;
    let mut token = String::with_capacity(TOKEN_BYTES * 2);
    for byte in bytes {
        token.push_str(&format!("{byte:02x}"));
    }
    Ok(token)
}

/// Compare two tokens without an early exit, so a wrong token cannot be
/// recovered byte by byte from response timing.
fn tokens_match(expected: &str, supplied: &str) -> bool {
    let expected = expected.as_bytes();
    let supplied = supplied.as_bytes();
    if expected.len() != supplied.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in expected.iter().zip(supplied) {
        difference |= a ^ b;
    }
    difference == 0
}

/// A web page cannot reach the endpoint through DNS rebinding: only absent
/// `Origin` headers (native clients) and loopback origins are accepted.
pub fn origin_allowed(origin: &str) -> bool {
    let Some(rest) = origin.strip_prefix("http://") else {
        return false;
    };
    let host = if let Some(rest) = rest.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default()
    } else {
        rest.split(':').next().unwrap_or_default()
    };
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// One request handed to the editor's UI thread. The endpoint thread blocks on
/// the reply channel, so it never accepts a second request in parallel.
pub struct BridgeJob {
    pub request: Value,
    reply: Sender<Result<Value, String>>,
}

impl BridgeJob {
    /// Answer the waiting endpoint. Errors are returned to the agent as a tool
    /// failure, never as a partial edit.
    pub fn reply(self, answer: Result<Value, String>) {
        let _ = self.reply.send(answer);
    }
}

/// A bound loopback endpoint plus its registry entry. Dropping it removes the
/// registry entry and stops the accept thread.
pub struct BridgeServer {
    token: String,
    entry: EditorEntry,
    jobs: Receiver<BridgeJob>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    registry: PathBuf,
}

impl BridgeServer {
    /// Bind `127.0.0.1:0`, register the session and start accepting. The caller
    /// must keep the returned value alive for as long as the bridge is enabled.
    pub fn start(name: &str) -> Result<Self, String> {
        let token = new_token()?;
        let registry = registry_path();
        let server = Self::bind(name, token, registry.clone())?;
        register_at(&registry, server.entry())?;
        Ok(server)
    }

    /// Bind without publishing, which is what tests use.
    pub fn bind(name: &str, token: String, registry: PathBuf) -> Result<Self, String> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .map_err(|error| format!("cannot bind the live bridge: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("cannot configure the live bridge: {error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| format!("cannot read the live bridge address: {error}"))?
            .port();
        let entry = EditorEntry {
            name: name.to_string(),
            port,
            pid: std::process::id(),
            token: token.clone(),
        };
        let (jobs_tx, jobs) = channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread = {
            let token = token.clone();
            let shutdown = shutdown.clone();
            let listener = listener
                .try_clone()
                .map_err(|error| format!("cannot share the live bridge listener: {error}"))?;
            let jobs = jobs_tx.clone();
            thread::Builder::new()
                .name("leopardcraft-live-bridge".into())
                .spawn(move || accept_loop(listener, token, jobs, shutdown))
                .map_err(|error| format!("cannot start the live bridge: {error}"))?
        };
        Ok(Self {
            token,
            entry,
            jobs,
            shutdown,
            thread: Some(thread),
            registry,
        })
    }

    pub fn entry(&self) -> &EditorEntry {
        &self.entry
    }

    pub fn port(&self) -> u16 {
        self.entry.port
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// Requests waiting for the UI thread. The editor drains them once per
    /// frame and answers each one exactly once.
    pub fn drain(&self) -> Vec<BridgeJob> {
        let mut jobs = Vec::new();
        while let Ok(job) = self.jobs.try_recv() {
            jobs.push(job);
        }
        jobs
    }
}

impl Drop for BridgeServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        unregister_at(&self.registry, self.entry.port, self.entry.pid);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn accept_loop(
    listener: TcpListener,
    token: String,
    jobs: Sender<BridgeJob>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => serve_connection(stream, &token, &jobs),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Result<Request, String> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let header_end = loop {
        if let Some(index) = find_header_end(&buffer) {
            break index;
        }
        if buffer.len() > MAX_HEADER_BYTES {
            return Err("request headers too large".into());
        }
        let read = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("connection closed before the headers".into());
        }
        buffer.extend_from_slice(&chunk[..read]);
    };
    let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let mut headers = Vec::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    let content_length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return Err("request body too large".into());
    }
    let mut body = buffer[(header_end + 4).min(buffer.len())..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(content_length);
    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn serve_connection(mut stream: TcpStream, token: &str, jobs: &Sender<BridgeJob>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let request = match read_request(&mut stream) {
        Ok(request) => request,
        Err(error) => {
            let _ = respond(&mut stream, 400, &json!({ "ok": false, "error": error }));
            return;
        }
    };
    if request.method != "POST" || request.path != COMMAND_PATH {
        let _ = respond(
            &mut stream,
            404,
            &json!({ "ok": false, "error": format!("unknown endpoint {}", request.path) }),
        );
        return;
    }
    if let Some((_, origin)) = request.headers.iter().find(|(name, _)| name == "origin") {
        if !origin_allowed(origin) {
            let _ = respond(
                &mut stream,
                403,
                &json!({ "ok": false, "error": "origin is not a loopback origin" }),
            );
            return;
        }
    }
    let supplied = request
        .headers
        .iter()
        .find(|(name, _)| name == "authorization")
        .and_then(|(_, value)| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if !tokens_match(token, supplied) {
        let _ = respond(
            &mut stream,
            401,
            &json!({ "ok": false, "error": "missing or wrong session token" }),
        );
        return;
    }
    let payload: Value = match serde_json::from_slice(&request.body) {
        Ok(payload) => payload,
        Err(error) => {
            let _ = respond(
                &mut stream,
                400,
                &json!({ "ok": false, "error": format!("invalid JSON: {error}") }),
            );
            return;
        }
    };
    let (reply_tx, reply_rx) = channel();
    if jobs
        .send(BridgeJob {
            request: payload,
            reply: reply_tx,
        })
        .is_err()
    {
        let _ = respond(
            &mut stream,
            503,
            &json!({ "ok": false, "error": "editor is shutting down" }),
        );
        return;
    }
    match reply_rx.recv_timeout(REPLY_TIMEOUT) {
        Ok(Ok(result)) => {
            let _ = respond(&mut stream, 200, &json!({ "ok": true, "result": result }));
        }
        Ok(Err(error)) => {
            let _ = respond(&mut stream, 200, &json!({ "ok": false, "error": error }));
        }
        Err(_) => {
            let _ = respond(
                &mut stream,
                503,
                &json!({ "ok": false, "error": "editor is busy; retry the call" }),
            );
        }
    }
}

fn respond(stream: &mut TcpStream, status: u16, body: &Value) -> std::io::Result<()> {
    let text = body.to_string();
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Service Unavailable",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
    stream.write_all(response.as_bytes())
}

/// The MCP server's half of the bridge: one attached editor.
pub struct LiveClient {
    entry: EditorEntry,
}

impl LiveClient {
    pub fn new(entry: EditorEntry) -> Self {
        Self { entry }
    }

    /// Discover the attached editor from the registry or the environment.
    pub fn discover() -> Result<Self, String> {
        Ok(Self::new(discover()?))
    }

    pub fn entry(&self) -> &EditorEntry {
        &self.entry
    }

    /// Send one request to the editor and wait for its answer.
    pub fn command(&self, op: &str, arguments: Value) -> Result<Value, String> {
        let body = json!({ "op": op, "arguments": arguments }).to_string();
        let request = format!(
            "POST {COMMAND_PATH} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nOrigin: http://127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}",
            port = self.entry.port,
            token = self.entry.token,
            length = body.len(),
        );
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, self.entry.port));
        let mut stream = TcpStream::connect_timeout(&address, CLIENT_TIMEOUT).map_err(|error| {
            format!(
                "cannot reach the editor on port {}: {error}",
                self.entry.port
            )
        })?;
        let _ = stream.set_read_timeout(Some(CLIENT_TIMEOUT));
        let _ = stream.set_write_timeout(Some(CLIENT_TIMEOUT));
        stream
            .write_all(request.as_bytes())
            .map_err(|error| format!("cannot send the bridge request: {error}"))?;
        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .map_err(|error| format!("cannot read the bridge answer: {error}"))?;
        let text = String::from_utf8_lossy(&response).to_string();
        let (head, body) = text
            .split_once("\r\n\r\n")
            .ok_or_else(|| "the editor sent a malformed bridge answer".to_string())?;
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|status| status.parse::<u16>().ok())
            .ok_or_else(|| "the editor sent no HTTP status".to_string())?;
        let payload: Value = serde_json::from_str(body)
            .map_err(|error| format!("the editor sent invalid JSON: {error}"))?;
        let detail = payload
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("the editor refused the request");
        if status == 200 {
            if payload.get("ok").and_then(Value::as_bool) == Some(true) {
                return Ok(payload.get("result").cloned().unwrap_or(Value::Null));
            }
            return Err(detail.to_string());
        }
        Err(format!("the editor answered HTTP {status}: {detail}"))
    }

    /// The editor's live document and selection.
    pub fn snapshot(&self) -> Result<Value, String> {
        self.command("snapshot", json!({}))
    }

    /// The editor's live document as native JSON text, for the scene resource.
    pub fn scene_json(&self) -> Result<String, String> {
        let value = self.command("scene", json!({}))?;
        value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "the editor returned no scene JSON".to_string())
    }

    /// Run one MCP tool against the editor's live document.
    pub fn call_tool(&self, name: &str, arguments: &Map<String, Value>) -> Result<Value, String> {
        self.command(
            "tool",
            json!({ "name": name, "arguments": Value::Object(arguments.clone()) }),
        )
    }

    /// Set the editor's object selection: `replace`, `add` or `toggle`.
    pub fn select(&self, objects: &[Value], mode: &str) -> Result<Value, String> {
        self.command("select", json!({ "objects": objects, "mode": mode }))
    }
}

/// Shared helper: the editor's selection payload.
pub fn selection_payload(ids: &[u64], names: &[String]) -> Value {
    json!({
        "objects": ids,
        "names": names,
        "primary": ids.last().copied(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn unique_registry(label: &str) -> PathBuf {
        std::env::temp_dir()
            .join("leopardcraft-bridge-tests")
            .join(format!("{label}-{}.json", std::process::id()))
    }

    fn test_entry(port: u16, token: &str) -> EditorEntry {
        EditorEntry {
            name: "test".into(),
            port,
            pid: std::process::id(),
            token: token.into(),
        }
    }

    /// Act as the editor's UI thread: answer one job and return its request.
    fn answer_one(server: &BridgeServer, result: Value) -> Value {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let jobs = server.drain();
            if let Some(job) = jobs.into_iter().next() {
                let request = job.request.clone();
                job.reply(Ok(result));
                return request;
            }
            assert!(
                Instant::now() < deadline,
                "the endpoint never delivered a job"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn origin_allow_list_accepts_loopback_origins_only() {
        for allowed in [
            "http://127.0.0.1",
            "http://127.0.0.1:47011",
            "http://localhost:8080",
            "http://[::1]:9999",
        ] {
            assert!(origin_allowed(allowed), "{allowed} should be allowed");
        }
        for rejected in [
            "https://127.0.0.1",
            "http://evil.example",
            "http://127.0.0.1.evil.example",
            "null",
        ] {
            assert!(!origin_allowed(rejected), "{rejected} should be rejected");
        }
    }

    #[test]
    fn registry_file_is_private_and_prunes_replaced_entries() {
        let path = unique_registry("registry");
        let _ = fs::remove_file(&path);
        let old = test_entry(40001, "old");
        let new = test_entry(40002, "new");
        register_at(&path, &old).unwrap();
        register_at(&path, &new).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["editors"].as_array().unwrap().len(), 2);
        assert!(text.contains("40001") && text.contains("40002"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "the token file must stay user-only");
        }
        unregister_at(&path, old.port, old.pid);
        let remaining = read_entries_at(&path);
        assert_eq!(remaining, vec![new.clone()]);
        assert_eq!(remaining[0].token, "new");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn bridge_requires_the_session_token_and_reports_the_editor_busy() {
        let server = BridgeServer::bind("test", "secret".into(), unique_registry("token")).unwrap();
        let port = server.port();
        let wrong = LiveClient::new(test_entry(port, "wrong"));
        let error = wrong.command("snapshot", json!({})).unwrap_err();
        assert!(error.contains("401"), "unexpected answer: {error}");

        let right = LiveClient::new(test_entry(port, "secret"));
        let request = std::thread::spawn(move || right.command("snapshot", json!({})));
        let delivered = answer_one(&server, json!({ "revision": 7 }));
        assert_eq!(delivered["op"], "snapshot");
        assert_eq!(request.join().unwrap().unwrap()["revision"], 7);
    }

    /// A fake editor: owns a document and answers bridge jobs the way the
    /// editor's UI thread does, through the shared tool implementation.
    fn spawn_fake_editor(server: BridgeServer, stop: Arc<AtomicBool>) -> JoinHandle<()> {
        thread::spawn(move || {
            let mut doc = lc_scene::Document::demo();
            let mut history = lc_scene::History::new(256);
            while !stop.load(Ordering::Relaxed) {
                for job in server.drain() {
                    let request = job.request.clone();
                    let op = request["op"].as_str().unwrap_or_default();
                    let answer = match op {
                        "snapshot" => Ok(json!({
                            "revision": doc.signature(),
                            "undo_depth": history.depth(),
                        })),
                        "scene" => lc_scene::to_json(&doc).map(Value::String),
                        "select" => Ok(json!({
                            "objects": request["arguments"]["objects"].clone(),
                        })),
                        "tool" => {
                            let name = request["arguments"]["name"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string();
                            let arguments = request["arguments"]["arguments"]
                                .as_object()
                                .cloned()
                                .unwrap_or_default();
                            let current = std::mem::replace(&mut doc, lc_scene::Document::demo());
                            let stack =
                                std::mem::replace(&mut history, lc_scene::History::new(256));
                            let mut session =
                                crate::server::McpServer::with_history(current, None, stack);
                            let result = session.call_tool_value(&name, &arguments);
                            let (next, _, stack) = session.into_parts();
                            doc = next;
                            history = stack;
                            result
                        }
                        other => Err(format!("unknown op {other}")),
                    };
                    job.reply(answer);
                }
                thread::sleep(Duration::from_millis(5));
            }
        })
    }

    fn object_names(response: &Value) -> Vec<String> {
        response["result"]["structuredContent"]["objects"]
            .as_array()
            .map(|objects| {
                objects
                    .iter()
                    .filter_map(|object| object["name"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn call(server: &mut crate::server::McpServer, id: u64, tool: &str, arguments: Value) -> Value {
        server
            .handle(json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": { "name": tool, "arguments": arguments }
            }))
            .expect("tools/call always answers")
    }

    #[test]
    fn attach_routes_tools_resources_and_undo_to_the_live_editor() {
        let server =
            BridgeServer::bind("test", "secret".into(), unique_registry("attach")).unwrap();
        let port = server.port();
        let stop = Arc::new(AtomicBool::new(false));
        let editor = spawn_fake_editor(server, stop.clone());
        let client = LiveClient::new(test_entry(port, "secret"));
        let mut mcp = crate::server::McpServer::new(lc_scene::Document::demo(), None);

        // Attached: every tool call runs on the editor's live document.
        mcp.attach(client);
        let created = call(
            &mut mcp,
            1,
            "create_primitive",
            json!({ "primitive": "box", "name": "Agent Box" }),
        );
        assert_eq!(created["result"]["isError"], false, "{created}");
        let live = call(&mut mcp, 2, "list_objects", json!({}));
        assert!(object_names(&live).contains(&"Agent Box".to_string()));

        let scene = mcp
            .handle(json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "resources/read",
                "params": { "uri": crate::server::SCENE_URI }
            }))
            .unwrap();
        assert!(scene["result"]["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Agent Box"));

        let selected = call(
            &mut mcp,
            4,
            "select_objects",
            json!({ "objects": ["Cube"], "mode": "replace" }),
        );
        assert_eq!(
            selected["result"]["structuredContent"]["objects"],
            json!(["Cube"]),
            "select_objects response: {selected}"
        );

        // Undo goes through the editor's own stack, which is what removes the
        // object the tool call created there.
        call(&mut mcp, 5, "undo", json!({}));
        let after_undo = call(&mut mcp, 6, "list_objects", json!({}));
        assert!(!object_names(&after_undo).contains(&"Agent Box".to_string()));

        // Detaching falls back to the private document, which never saw the
        // live edit.
        assert!(mcp.detach());
        let local = call(&mut mcp, 7, "list_objects", json!({}));
        assert!(!object_names(&local).contains(&"Agent Box".to_string()));
        let detached_selection = call(
            &mut mcp,
            8,
            "select_objects",
            json!({ "objects": ["Cube"] }),
        );
        assert_eq!(detached_selection["result"]["isError"], true);

        stop.store(true, Ordering::Relaxed);
        let _ = editor.join();
    }

    #[test]
    fn bridge_answers_busy_when_the_editor_does_not_drain() {
        let server = BridgeServer::bind("test", "secret".into(), unique_registry("busy")).unwrap();
        let port = server.port();
        let client = LiveClient::new(test_entry(port, "secret"));
        let error = client.command("snapshot", json!({})).unwrap_err();
        assert!(error.contains("busy"), "unexpected answer: {error}");
    }
}
