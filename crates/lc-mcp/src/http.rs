//! MCP Streamable HTTP transport: one loopback POST/GET endpoint.
//!
//! Clients POST JSON-RPC messages to `/mcp` and receive the response as JSON;
//! notifications are answered with `202 Accepted` and no body. A GET on the same
//! endpoint opens a Server-Sent Events stream that carries the server's
//! `notifications/resources/updated` messages, which is how resource
//! subscriptions reach an HTTP client. The listener only accepts loopback
//! addresses, so enabling this transport never opens the project to the network.
//!
//! Sessions follow the Streamable HTTP transport rules: the response to
//! `initialize` carries an `Mcp-Session-Id`, every later request has to repeat
//! it, an unknown id is answered with `404` so the client initializes again, and
//! `DELETE /mcp` ends a session. Reconnecting with a live id resumes the
//! document's resource subscriptions because those live in the server, not in
//! the connection.

use crate::server::McpServer;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

/// The single endpoint that serves every MCP HTTP request.
pub const HTTP_ENDPOINT: &str = "/mcp";

/// Header the server assigns in the `initialize` response and requires on every
/// later request, so a client can tell a resumed session from a new one.
pub const SESSION_HEADER: &str = "mcp-session-id";

/// Live sessions kept at once. A session is a few bytes plus its event streams,
/// so the oldest is evicted when a client keeps initializing without
/// terminating; the evicted client re-initializes exactly as it would after any
/// other session loss.
const MAX_SESSIONS: usize = 64;

const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(10);

static NEXT_STREAM: AtomicU64 = AtomicU64::new(1);
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

/// A bound Streamable HTTP endpoint. Each accepted connection is served on its
/// own thread so a streaming GET never blocks a POST.
pub struct HttpTransport {
    listener: TcpListener,
    state: Arc<State>,
}

struct State {
    server: Mutex<McpServer>,
    /// Open event streams: stream id, the session that opened it, and its sender.
    streams: Mutex<Vec<(u64, String, Sender<Value>)>>,
    /// Live session ids in creation order, so the oldest can be evicted.
    sessions: Mutex<Vec<String>>,
}

impl State {
    fn server(&self) -> MutexGuard<'_, McpServer> {
        lock(&self.server)
    }

    fn streams(&self) -> MutexGuard<'_, Vec<(u64, String, Sender<Value>)>> {
        lock(&self.streams)
    }

    fn sessions(&self) -> MutexGuard<'_, Vec<String>> {
        lock(&self.sessions)
    }

    fn drop_stream(&self, id: u64) {
        self.streams().retain(|(stream, _, _)| *stream != id);
    }

    /// Assign an id for a new session, evicting the oldest session and its
    /// streams when the cap is reached.
    fn open_session(&self) -> String {
        let mut sessions = self.sessions();
        while sessions.len() >= MAX_SESSIONS {
            let expired = sessions.remove(0);
            self.streams().retain(|(_, session, _)| session != &expired);
        }
        let id = format!("lc-{:016x}", NEXT_SESSION.fetch_add(1, Ordering::Relaxed));
        sessions.push(id.clone());
        id
    }

    fn session_is_live(&self, id: &str) -> bool {
        self.sessions().iter().any(|session| session == id)
    }

    /// Terminate a session: its id stops working and its event streams are
    /// dropped, which ends those connections.
    fn close_session(&self, id: &str) -> bool {
        let mut sessions = self.sessions();
        let live = sessions.iter().any(|session| session == id);
        sessions.retain(|session| session != id);
        drop(sessions);
        if live {
            self.streams().retain(|(_, session, _)| session != id);
        }
        live
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl HttpTransport {
    /// Bind a loopback address, refusing anything else.
    pub fn bind(addr: &str, server: McpServer) -> Result<Self, String> {
        let socket = addr
            .to_socket_addrs()
            .map_err(|error| format!("invalid MCP HTTP address '{addr}': {error}"))?
            .next()
            .ok_or_else(|| format!("invalid MCP HTTP address '{addr}'"))?;
        if !socket.ip().is_loopback() {
            return Err(format!(
                "the MCP HTTP transport only binds loopback addresses, not {socket}"
            ));
        }
        let listener = TcpListener::bind(socket)
            .map_err(|error| format!("failed to bind {socket}: {error}"))?;
        Ok(Self {
            listener,
            state: Arc::new(State {
                server: Mutex::new(server),
                streams: Mutex::new(Vec::new()),
                sessions: Mutex::new(Vec::new()),
            }),
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, String> {
        self.listener
            .local_addr()
            .map_err(|error| format!("failed to read the HTTP address: {error}"))
    }

    /// Accept connections until the listener fails.
    pub fn serve(self) -> Result<(), String> {
        let HttpTransport { listener, state } = self;
        for incoming in listener.incoming() {
            match incoming {
                Ok(stream) => {
                    let state = state.clone();
                    std::thread::spawn(move || handle_connection(stream, state));
                }
                Err(error) => return Err(format!("MCP HTTP accept failed: {error}")),
            }
        }
        Ok(())
    }
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

enum ReadError {
    Closed,
    Status(u16, &'static str),
}

fn read_request(reader: &mut BufReader<TcpStream>) -> Result<Request, ReadError> {
    fn read_line(reader: &mut BufReader<TcpStream>, line: &mut String) -> Result<usize, ReadError> {
        reader.read_line(line).map_err(|_| ReadError::Closed)
    }

    let mut line = String::new();
    if read_line(reader, &mut line)? == 0 {
        return Err(ReadError::Closed);
    }
    let mut budget = line.len();
    if budget > MAX_HEADER_BYTES {
        return Err(ReadError::Status(431, "Request Header Fields Too Large"));
    }
    let mut parts = line.split_whitespace();
    let method = parts
        .next()
        .ok_or(ReadError::Status(400, "Bad Request"))?
        .to_ascii_uppercase();
    let target = parts.next().ok_or(ReadError::Status(400, "Bad Request"))?;
    let version = parts.next().unwrap_or_default();
    if !version.starts_with("HTTP/1.") {
        return Err(ReadError::Status(505, "HTTP Version Not Supported"));
    }
    let path = target.split(['?', '#']).next().unwrap_or("").to_string();

    let mut headers: Vec<(String, String)> = Vec::new();
    loop {
        line.clear();
        let bytes = read_line(reader, &mut line)?;
        if bytes == 0 {
            return Err(ReadError::Closed);
        }
        budget += bytes;
        if budget > MAX_HEADER_BYTES {
            return Err(ReadError::Status(431, "Request Header Fields Too Large"));
        }
        if line.trim().is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(ReadError::Status(400, "Bad Request"));
        };
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }

    let mut request = Request {
        method,
        path,
        headers,
        body: Vec::new(),
    };
    if let Some(length) = request.header("content-length") {
        let length: usize = length
            .parse()
            .map_err(|_| ReadError::Status(400, "Bad Request"))?;
        if length > MAX_BODY_BYTES {
            return Err(ReadError::Status(413, "Payload Too Large"));
        }
        request.body = vec![0; length];
        reader
            .read_exact(&mut request.body)
            .map_err(|_| ReadError::Closed)?;
    } else if request.header("transfer-encoding").is_some() {
        return Err(ReadError::Status(411, "Length Required"));
    }
    Ok(request)
}

/// Write one complete response and close the connection.
fn write_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    extra: &str,
    body: &[u8],
) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\n{extra}Connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn handle_connection(stream: TcpStream, state: Arc<State>) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let mut writer = match stream.try_clone() {
        Ok(clone) => clone,
        Err(_) => return,
    };
    let mut reader = BufReader::new(stream);
    let request = match read_request(&mut reader) {
        Ok(request) => request,
        Err(ReadError::Closed) => return,
        Err(ReadError::Status(status, reason)) => {
            write_response(
                &mut writer,
                status,
                reason,
                "text/plain; charset=utf-8",
                "",
                reason.as_bytes(),
            );
            return;
        }
    };
    if request.path != HTTP_ENDPOINT {
        write_response(
            &mut writer,
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            "",
            b"unknown MCP endpoint",
        );
        return;
    }
    match request.method.as_str() {
        "POST" => post_message(&request, &state, &mut writer),
        "GET" => stream_events(&request, &state, writer),
        "DELETE" => delete_session(&request, &state, &mut writer),
        _ => write_response(
            &mut writer,
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            "Allow: DELETE, GET, POST\r\n",
            b"the MCP endpoint accepts POST, GET and DELETE",
        ),
    }
}

/// Resolve the request's `Mcp-Session-Id`. `None` means a response has already
/// been written and the caller must stop: a request without the header skipped
/// `initialize`, and an unknown id is a session this server no longer has, which
/// the client answers by initializing again.
fn require_session(request: &Request, state: &State, writer: &mut TcpStream) -> Option<String> {
    let id = request
        .header(SESSION_HEADER)
        .map(str::trim)
        .filter(|id| !id.is_empty());
    let Some(id) = id else {
        write_response(
            writer,
            400,
            "Bad Request",
            "text/plain; charset=utf-8",
            "",
            b"missing the Mcp-Session-Id header; send initialize first",
        );
        return None;
    };
    if state.session_is_live(id) {
        Some(id.to_string())
    } else {
        write_response(
            writer,
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            "",
            b"unknown MCP session; send initialize again",
        );
        None
    }
}

/// `DELETE /mcp` terminates the session named by the header. Its event streams
/// end with it, and later requests with that id are answered with `404`.
fn delete_session(request: &Request, state: &State, writer: &mut TcpStream) {
    let Some(id) = require_session(request, state, writer) else {
        return;
    };
    state.close_session(&id);
    write_response(
        writer,
        200,
        "OK",
        "text/plain; charset=utf-8",
        "",
        b"session terminated",
    );
}

fn post_message(request: &Request, state: &State, writer: &mut TcpStream) {
    let content_type = request.header("content-type").unwrap_or("");
    if !content_type
        .to_ascii_lowercase()
        .starts_with("application/json")
    {
        write_response(
            writer,
            415,
            "Unsupported Media Type",
            "text/plain; charset=utf-8",
            "",
            b"POST bodies must be application/json",
        );
        return;
    }
    let message: Value = match serde_json::from_slice(&request.body) {
        Ok(message) => message,
        Err(error) => {
            let failure = serde_json::json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": { "code": -32700, "message": format!("Parse error: {error}") }
            });
            let body = serde_json::to_string(&failure).unwrap_or_default();
            write_response(
                writer,
                400,
                "Bad Request",
                "application/json",
                "",
                body.as_bytes(),
            );
            return;
        }
    };
    let initialize = message.get("method").and_then(Value::as_str) == Some("initialize");
    let session = if initialize {
        // The initialize response is where a session starts; any id the client
        // sent before it is meaningless.
        state.open_session()
    } else {
        match require_session(request, state, writer) {
            Some(session) => session,
            None => return,
        }
    };
    let session_header = format!("{SESSION_HEADER}: {session}\r\n");
    match state.server().handle(message) {
        Some(response) => {
            let body = serde_json::to_string(&response).unwrap_or_else(|_| "{}".into());
            write_response(
                writer,
                200,
                "OK",
                "application/json",
                &session_header,
                body.as_bytes(),
            );
        }
        None => write_response(
            writer,
            202,
            "Accepted",
            "text/plain; charset=utf-8",
            &session_header,
            b"",
        ),
    }
    // Deliver resource-update notifications to every open event stream.
    broadcast(state);
}

fn stream_events(request: &Request, state: &State, mut writer: TcpStream) {
    let wants_events = request
        .header("accept")
        .map(|value| value.to_ascii_lowercase().contains("text/event-stream"))
        .unwrap_or(false);
    if !wants_events {
        write_response(
            &mut writer,
            406,
            "Not Acceptable",
            "text/plain; charset=utf-8",
            "",
            b"GET requires Accept: text/event-stream",
        );
        return;
    }
    let Some(session) = require_session(request, state, &mut writer) else {
        return;
    };
    let (sender, receiver) = channel();
    let id = NEXT_STREAM.fetch_add(1, Ordering::Relaxed);
    state.streams().push((id, session, sender));
    let opened = write!(
        writer,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\
         Connection: close\r\n\r\n: connected\n"
    )
    .and_then(|_| writer.flush())
    .is_ok();
    if !opened {
        state.drop_stream(id);
        return;
    }
    // The stream ends when the client goes away or the sender is dropped.
    drain_events(&mut writer, &receiver);
    state.drop_stream(id);
}

fn drain_events(writer: &mut TcpStream, receiver: &Receiver<Value>) {
    for notification in receiver {
        let data = serde_json::to_string(&notification).unwrap_or_else(|_| "{}".into());
        let sent = write!(writer, "event: message\ndata: {data}\n\n")
            .and_then(|_| writer.flush())
            .is_ok();
        if !sent {
            return;
        }
    }
}

fn broadcast(state: &State) {
    let pending = state.server().take_notifications();
    if pending.is_empty() {
        return;
    }
    state.streams().retain(|(_, _, sender)| {
        pending
            .iter()
            .all(|notification| sender.send(notification.clone()).is_ok())
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use lc_scene::Document;

    fn spawn() -> SocketAddr {
        let transport = HttpTransport::bind("127.0.0.1:0", McpServer::new(Document::empty(), None))
            .expect("loopback bind succeeds");
        let addr = transport.local_addr().expect("bound address");
        std::thread::spawn(move || {
            let _ = transport.serve();
        });
        addr
    }

    fn send(addr: SocketAddr, request: &str) -> String {
        let mut stream = TcpStream::connect(addr).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("read timeout");
        stream.write_all(request.as_bytes()).expect("write request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        response
    }

    const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
    const LIST_OBJECTS: &str = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_objects","arguments":{}}}"#;

    fn session_header(session: Option<&str>) -> String {
        session
            .map(|id| format!("{SESSION_HEADER}: {id}\r\n"))
            .unwrap_or_default()
    }

    fn post(addr: SocketAddr, session: Option<&str>, body: &str) -> String {
        let header = session_header(session);
        send(
            addr,
            &format!(
                "POST /mcp HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n\
                 {header}Content-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        )
    }

    fn delete(addr: SocketAddr, session: Option<&str>) -> String {
        let header = session_header(session);
        send(
            addr,
            &format!("DELETE /mcp HTTP/1.1\r\nHost: localhost\r\n{header}\r\n"),
        )
    }

    fn session_of(response: &str) -> String {
        response
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with(SESSION_HEADER))
            .and_then(|line| line.split_once(':'))
            .map(|(_, value)| value.trim().to_string())
            .expect("the initialize response carries a session id")
    }

    fn status(response: &str) -> &str {
        response.lines().next().unwrap_or_default()
    }

    fn json_body(response: &str) -> Value {
        let body = response.split_once("\r\n\r\n").expect("response headers").1;
        serde_json::from_str(body).expect("json response body")
    }

    #[test]
    fn http_transport_serves_requests_and_refuses_bad_ones() {
        let addr = spawn();
        let response = post(addr, None, INITIALIZE);
        assert!(status(&response).contains("200 OK"), "{response}");
        let session = session_of(&response);
        assert!(
            response.contains(&format!("{SESSION_HEADER}: {session}")),
            "{response}"
        );
        assert!(
            response
                .to_ascii_lowercase()
                .contains("content-type: application/json"),
            "{response}"
        );
        let value = json_body(&response);
        assert_eq!(value["result"]["serverInfo"]["name"], "leopardcraft");
        assert_eq!(
            value["result"]["capabilities"]["resources"]["subscribe"],
            true
        );

        // A notification has no id, so it is accepted without a response body.
        let response = post(
            addr,
            Some(&session),
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        );
        assert!(status(&response).contains("202 Accepted"), "{response}");

        let response = post(addr, Some(&session), LIST_OBJECTS);
        assert!(status(&response).contains("200 OK"), "{response}");
        assert_eq!(json_body(&response)["result"]["isError"], false);

        // Unknown paths, methods and media types never reach the document.
        let response = send(addr, "GET /other HTTP/1.1\r\nHost: localhost\r\n\r\n");
        assert!(status(&response).contains("404"), "{response}");
        let response = send(addr, "PUT /mcp HTTP/1.1\r\nHost: localhost\r\n\r\n");
        assert!(status(&response).contains("405"), "{response}");
        assert!(response.contains("Allow: DELETE, GET, POST"), "{response}");
        // DELETE is a session operation, so a request without one is refused.
        let response = delete(addr, None);
        assert!(status(&response).contains("400"), "{response}");
        let response = send(
            addr,
            "POST /mcp HTTP/1.1\r\nHost: localhost\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\n{}",
        );
        assert!(status(&response).contains("415"), "{response}");
        let response = send(addr, "GET /mcp HTTP/1.1\r\nHost: localhost\r\n\r\n");
        assert!(status(&response).contains("406"), "{response}");

        // Broken JSON is a JSON-RPC parse error carried by a 400 response.
        let response = post(addr, Some(&session), "{");
        assert!(status(&response).contains("400"), "{response}");
        assert_eq!(json_body(&response)["error"]["code"], -32700);
    }

    #[test]
    fn http_stream_delivers_resource_updates_after_edits() {
        let addr = spawn();
        let session = session_of(&post(addr, None, INITIALIZE));
        let response = post(
            addr,
            Some(&session),
            r#"{"jsonrpc":"2.0","id":1,"method":"resources/subscribe","params":{"uri":"leopardcraft://scene/summary"}}"#,
        );
        assert!(status(&response).contains("200 OK"), "{response}");

        let stream = TcpStream::connect(addr).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("read timeout");
        let mut writer = stream.try_clone().expect("clone the socket");
        writer
            .write_all(
                format!(
                    "GET /mcp HTTP/1.1\r\nHost: localhost\r\nAccept: text/event-stream\r\n\
                     {SESSION_HEADER}: {session}\r\n\r\n"
                )
                .as_bytes(),
            )
            .expect("write event request");
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        loop {
            line.clear();
            assert!(reader.read_line(&mut line).expect("headers") > 0);
            if line.trim().is_empty() {
                break;
            }
        }
        line.clear();
        reader.read_line(&mut line).expect("connected comment");
        assert_eq!(line.trim(), ": connected");

        let response = post(
            addr,
            Some(&session),
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"create_primitive","arguments":{"primitive":"box"}}}"#,
        );
        assert!(status(&response).contains("200 OK"), "{response}");

        line.clear();
        reader.read_line(&mut line).expect("event frame");
        assert_eq!(line.trim(), "event: message");
        line.clear();
        reader.read_line(&mut line).expect("data frame");
        let data = line.trim().strip_prefix("data: ").expect("data prefix");
        let value: Value = serde_json::from_str(data).expect("notification json");
        assert_eq!(value["method"], "notifications/resources/updated");
        assert_eq!(value["params"]["uri"], "leopardcraft://scene/summary");
    }

    #[test]
    fn http_sessions_require_the_header_and_can_be_terminated() {
        let addr = spawn();
        let session = session_of(&post(addr, None, INITIALIZE));

        // A client that skips initialize, or reuses an id the server dropped, is
        // told what to do instead of touching the document.
        let response = post(addr, None, LIST_OBJECTS);
        assert!(status(&response).contains("400"), "{response}");
        assert!(response.contains("initialize"), "{response}");
        let response = post(addr, Some("lc-00000000000000ff"), LIST_OBJECTS);
        assert!(status(&response).contains("404"), "{response}");
        assert!(response.contains("initialize"), "{response}");
        let response = send(
            addr,
            "GET /mcp HTTP/1.1\r\nHost: localhost\r\nAccept: text/event-stream\r\n\r\n",
        );
        assert!(status(&response).contains("400"), "{response}");

        // A live id works, and every response repeats it.
        let response = post(addr, Some(&session), LIST_OBJECTS);
        assert!(status(&response).contains("200 OK"), "{response}");
        assert!(
            response.contains(&format!("{SESSION_HEADER}: {session}")),
            "{response}"
        );

        // DELETE ends the session; the id is then unknown, twice over.
        let response = delete(addr, Some(&session));
        assert!(status(&response).contains("200 OK"), "{response}");
        let response = post(addr, Some(&session), LIST_OBJECTS);
        assert!(status(&response).contains("404"), "{response}");
        let response = delete(addr, Some(&session));
        assert!(status(&response).contains("404"), "{response}");

        // Initializing again issues a fresh id that works.
        let next = session_of(&post(addr, None, INITIALIZE));
        assert_ne!(next, session);
        let response = post(addr, Some(&next), LIST_OBJECTS);
        assert!(status(&response).contains("200 OK"), "{response}");
    }

    #[test]
    fn http_sessions_evict_the_oldest_at_the_cap() {
        let addr = spawn();
        let first = session_of(&post(addr, None, INITIALIZE));
        let mut last = first.clone();
        for _ in 0..MAX_SESSIONS {
            last = session_of(&post(addr, None, INITIALIZE));
            assert_ne!(last, first, "session ids must be unique");
        }
        // The cap is full, so the first session was evicted and its id is gone.
        let response = post(addr, Some(&first), LIST_OBJECTS);
        assert!(
            status(&response).contains("404"),
            "the oldest session must be evicted: {response}"
        );
        let response = post(addr, Some(&last), LIST_OBJECTS);
        assert!(status(&response).contains("200 OK"), "{response}");
    }

    #[test]
    fn http_transport_refuses_non_loopback_addresses() {
        let error = match HttpTransport::bind("0.0.0.0:0", McpServer::new(Document::empty(), None))
        {
            Ok(_) => panic!("a non-loopback bind must be refused"),
            Err(error) => error,
        };
        assert!(error.contains("loopback"), "{error}");
    }
}
