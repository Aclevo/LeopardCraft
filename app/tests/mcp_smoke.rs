//! End-to-end MCP smoke tests.
//!
//! These tests spawn the real LeopardCraft binary with a working directory
//! outside the repository, complete an `initialize` + `tools/list` handshake,
//! exercise representative tools, and compare the advertised inventory with
//! the list documented in `AGENTS.md`. The second test reads the committed
//! `.mcp.json`, expands `${CLAUDE_PROJECT_DIR:-.}` to the repository path, and
//! launches that command from a temporary directory outside the repository.
//! Claude Code itself starts project-scoped servers at the project root; a real
//! Claude Code health check is a manual verification, not part of this test.

use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

struct McpClient {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
}

impl McpClient {
    fn start(mut command: Command, cwd: &Path) -> Self {
        command
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut child = command
            .spawn()
            .unwrap_or_else(|error| panic!("failed to launch the MCP server: {error}"));
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if sender.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            child,
            stdin: Some(stdin),
            lines,
        }
    }

    fn request(&mut self, request: Value) -> Value {
        self.write(&request);
        let method = request["method"].as_str().unwrap_or("request");
        let line = self
            .lines
            .recv_timeout(Duration::from_secs(20))
            .unwrap_or_else(|_| panic!("timed out waiting for a reply to {method}"));
        serde_json::from_str(&line)
            .unwrap_or_else(|error| panic!("invalid JSON reply to {method}: {error}: {line}"))
    }

    fn notify(&mut self, notification: Value) {
        self.write(&notification);
    }

    fn write(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin is still open");
        let line = serde_json::to_string(message).expect("JSON-RPC message is serializable");
        writeln!(stdin, "{line}").expect("write MCP request");
        stdin.flush().expect("flush MCP request");
    }

    fn finish(&mut self) -> ExitStatus {
        self.stdin.take();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self.child.try_wait().expect("wait for the MCP server") {
                return status;
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                panic!("MCP server did not exit after stdin closed");
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn assert_no_extra_lines(&mut self) {
        if let Ok(line) = self.lines.recv_timeout(Duration::from_millis(250)) {
            panic!("MCP server emitted an unexpected stdout line: {line}");
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn mcp_handshake_and_tools_from_outside_repo() {
    let root = repo_root();
    let temp = TempDir::new("direct");
    assert_ne!(temp.path(), root.as_path());

    let mut command = Command::new(env!("CARGO_BIN_EXE_leopardcraft"));
    command.arg("--mcp");
    let mut client = McpClient::start(command, temp.path());
    exercise_server(&mut client, &root, temp.path());
}

#[cfg(unix)]
#[test]
fn project_mcp_config_launches_from_outside_repo() {
    let root = repo_root();
    let temp = TempDir::new("config");
    assert_ne!(temp.path(), root.as_path());

    let (command, args) = load_launch_command(&root);
    assert!(
        Path::new(&command).is_absolute(),
        "expanded .mcp.json command is not absolute: {command}"
    );
    // The host is assumed to know the project directory (Claude Code runs the
    // server from it); the point of this test is a launch from an unrelated cwd.
    let mut launch = Command::new(&command);
    launch.args(&args).env("CLAUDE_PROJECT_DIR", &root);

    let mut client = McpClient::start(launch, temp.path());
    exercise_server(&mut client, &root, temp.path());
}

#[test]
fn http_transport_serves_the_protocol_from_the_real_binary() {
    let temp = TempDir::new("http");
    let child = Command::new(env!("CARGO_BIN_EXE_leopardcraft"))
        .arg("--mcp-http")
        .arg("--addr")
        .arg("127.0.0.1:0")
        .current_dir(temp.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("launch the MCP HTTP server");
    let mut guard = ChildGuard(child);
    let stdout = guard.0.stdout.take().expect("piped stdout");
    let (sender, lines) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(line) => {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    let endpoint_line = lines
        .recv_timeout(Duration::from_secs(20))
        .expect("the HTTP server reports its endpoint");
    let endpoint = endpoint_line
        .trim()
        .strip_prefix("MCP Streamable HTTP endpoint: ")
        .unwrap_or_else(|| panic!("unexpected startup output: {endpoint_line}"));
    assert!(endpoint.starts_with("http://127.0.0.1:"), "{endpoint}");
    assert!(endpoint.ends_with("/mcp"), "{endpoint}");
    let addr = endpoint
        .trim_start_matches("http://")
        .trim_end_matches("/mcp")
        .to_string();

    let response = http_post(
        &addr,
        None,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    let body = response.split_once("\r\n\r\n").expect("response headers").1;
    let initialized: Value = serde_json::from_str(body).expect("json response body");
    assert_eq!(initialized["result"]["serverInfo"]["name"], "leopardcraft");
    assert_eq!(
        initialized["result"]["capabilities"]["resources"]["subscribe"],
        true
    );
    // The handshake starts a session, and later requests must repeat its id.
    let session = response
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("mcp-session-id"))
        .and_then(|line| line.split_once(':'))
        .map(|(_, value)| value.trim().to_string())
        .expect("the initialize response carries a session id");

    let response = http_post(
        &addr,
        None,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_objects","arguments":{}}}"#,
    );
    assert!(
        response.starts_with("HTTP/1.1 400"),
        "a request without the session header must be refused: {response}"
    );

    let response = http_post(
        &addr,
        Some(&session),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_objects","arguments":{}}}"#,
    );
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    let body = response.split_once("\r\n\r\n").expect("response headers").1;
    let listed: Value = serde_json::from_str(body).expect("json response body");
    assert_eq!(listed["result"]["isError"], false);

    // The transport refuses to expose the document beyond loopback.
    let refused = Command::new(env!("CARGO_BIN_EXE_leopardcraft"))
        .args(["--mcp-http", "--addr", "0.0.0.0:0"])
        .current_dir(temp.path())
        .output()
        .expect("run the refused MCP HTTP server");
    assert!(!refused.status.success(), "non-loopback bind must fail");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("loopback"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    guard.0.kill().expect("stop the HTTP server");
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn http_post(addr: &str, session: Option<&str>, body: &str) -> String {
    let mut stream = TcpStream::connect(addr).expect("connect to the MCP HTTP endpoint");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("read timeout");
    let header = session
        .map(|id| format!("Mcp-Session-Id: {id}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n\
         {header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("write HTTP request");
    stream.flush().expect("flush HTTP request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read HTTP response");
    response
}

fn exercise_server(client: &mut McpClient, root: &Path, temp: &Path) {
    let initialized = client.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "leopardcraft-smoke", "version": "1.0" }
        }
    }));
    assert_eq!(initialized["jsonrpc"], "2.0");
    assert_eq!(initialized["id"], 1);
    assert_eq!(initialized["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(initialized["result"]["serverInfo"]["name"], "leopardcraft");
    assert_eq!(
        initialized["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(
        initialized["result"]["capabilities"]["tools"]["listChanged"],
        false
    );
    assert_eq!(initialized["result"]["resultType"], "complete");

    client.notify(json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }));

    let listed = client.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }));
    assert_eq!(listed["id"], 2);
    assert_eq!(listed["result"]["resultType"], "complete");
    let tools = listed["result"]["tools"]
        .as_array()
        .expect("tools/list returns an array")
        .clone();
    assert_eq!(tools.len(), 34, "unexpected advertised tool count");
    assert_documented_tools(root, &tools);
    assert_tool_schemas(&tools);

    let created = client.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "create_primitive",
            "arguments": {
                "primitive": "box",
                "name": "Smoke Box",
                "translation": [1.0, 2.0, 3.0]
            }
        }
    }));
    assert_eq!(created["result"]["isError"], false);
    let object = &created["result"]["structuredContent"];
    assert_eq!(object["name"], "Smoke Box");
    assert_eq!(object["type"], "Box");
    assert_eq!(object["transform"]["translation"], json!([1.0, 2.0, 3.0]));

    let scene = client.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "get_scene", "arguments": {} }
    }));
    assert_eq!(scene["result"]["isError"], false);
    let summary = &scene["result"]["structuredContent"];
    assert!(summary["objects"]["total"].as_u64().unwrap() >= 1);
    assert!(summary["triangles"].as_u64().unwrap() > 0);
    assert_eq!(summary["document"]["path"], Value::Null);

    let scene_path = temp.join("smoke.lcscene");
    let saved = client.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "save_scene",
            "arguments": { "path": scene_path.display().to_string() }
        }
    }));
    assert_eq!(saved["result"]["isError"], false);
    assert!(scene_path.exists(), "save_scene did not write its target");
    let saved_json: Value =
        serde_json::from_str(&fs::read_to_string(&scene_path).expect("read saved scene"))
            .expect("saved scene is JSON");
    assert_eq!(saved_json["format"], "leopardcraft-scene");

    let status = client.finish();
    client.assert_no_extra_lines();
    assert!(
        status.success(),
        "MCP server exited unsuccessfully: {status:?}"
    );
}

fn assert_documented_tools(root: &Path, tools: &[Value]) {
    let mut advertised: Vec<String> = tools
        .iter()
        .map(|tool| {
            tool["name"]
                .as_str()
                .expect("every tool has a name")
                .to_string()
        })
        .collect();
    advertised.sort();

    let agents = fs::read_to_string(root.join("AGENTS.md")).expect("AGENTS.md exists");
    let section = agents
        .split("\n## MCP tool inventory\n")
        .nth(1)
        .expect("AGENTS.md has an 'MCP tool inventory' section");
    let section = section.split("\n## ").next().expect("inventory section");
    let mut documented = backticked_identifiers(section);
    documented.sort();
    assert_eq!(
        advertised, documented,
        "tools/list does not match the AGENTS.md inventory"
    );
}

fn assert_tool_schemas(tools: &[Value]) {
    for tool in tools {
        let name = tool["name"].as_str().expect("tool name");
        assert!(
            tool["description"].as_str().unwrap_or_default().len() > 20,
            "{name} needs a useful description"
        );
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object", "{name} input schema");
        assert!(schema["properties"].is_object(), "{name} properties");
        assert_eq!(
            tool["annotations"]["openWorldHint"], false,
            "{name} must be local-only"
        );
    }

    let create = tools
        .iter()
        .find(|tool| tool["name"] == "create_primitive")
        .expect("create_primitive is advertised");
    assert_eq!(
        create["inputSchema"]["properties"]["primitive"]["enum"]
            .as_array()
            .expect("primitive enum")
            .len(),
        8
    );
    assert!(create["inputSchema"]["required"]
        .as_array()
        .expect("required arguments")
        .contains(&json!("primitive")));

    let render = tools
        .iter()
        .find(|tool| tool["name"] == "render_image")
        .expect("render_image is advertised");
    assert!(render["inputSchema"]["required"]
        .as_array()
        .expect("required arguments")
        .contains(&json!("path")));
}

fn backticked_identifiers(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut inside = false;
    for part in text.split('`') {
        if inside {
            let candidate = part.trim();
            if !candidate.is_empty()
                && candidate
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                names.push(candidate.to_string());
            }
        }
        inside = !inside;
    }
    names.sort();
    names.dedup();
    names
}

fn load_launch_command(root: &Path) -> (String, Vec<String>) {
    let text = fs::read_to_string(root.join(".mcp.json")).expect(".mcp.json exists");
    let config: Value = serde_json::from_str(&text).expect(".mcp.json is valid JSON");
    let server = &config["mcpServers"]["leopardcraft"];
    let command = server["command"]
        .as_str()
        .expect(".mcp.json leopardcraft.command");
    let args: Vec<String> = server["args"]
        .as_array()
        .map(|args| {
            args.iter()
                .map(|arg| arg.as_str().expect("string arguments").to_string())
                .collect()
        })
        .unwrap_or_default();

    let expand = |value: &str| {
        value
            .replace("${CLAUDE_PROJECT_DIR:-.}", &root.display().to_string())
            .replace("${CLAUDE_PROJECT_DIR}", &root.display().to_string())
    };
    let command = expand(command);
    assert!(
        !command.contains("${"),
        ".mcp.json has an unexpanded command variable: {command}"
    );
    (command, args.into_iter().map(|arg| expand(&arg)).collect())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("app crate lives inside the workspace")
        .to_path_buf()
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "leopardcraft-mcp-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&path).expect("create temporary directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
