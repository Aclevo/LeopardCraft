//! Lane dispatch tests: the `spawn_agents` bridge is tried first, and the
//! promptable CLI is the fallback behind the same interface.
//!
//! `scripts/spawn-lane.sh` is the single entry point; these tests drive it with
//! fake bridge and CLI commands so no real agent host or account is needed.

#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the app crate lives inside the repository")
        .to_path_buf()
}

fn dispatcher() -> PathBuf {
    repo_root().join("scripts/spawn-lane.sh")
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "leopardcraft-lane-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&path).expect("create temp dir");
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

fn write_command(path: &Path, body: &str) {
    fs::write(path, body).expect("write fake command");
    let mut permissions = fs::metadata(path).expect("stat fake command").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("make fake command executable");
}

/// A bridge that records its agent id and packet, then succeeds or fails.
fn bridge(temp: &TempDir, status: u8) -> (PathBuf, PathBuf) {
    let record = temp.path().join("bridge.txt");
    let script = temp.path().join("spawn-bridge.sh");
    write_command(
        &script,
        &format!(
            "#!/usr/bin/env bash\nprintf '%s|%s\\n' \"$1\" \"$2\" >> '{record}'\ncat > /dev/null\nexit {status}\n",
            record = record.display()
        ),
    );
    (script, record)
}

/// A CLI that records its arguments separated by an ASCII unit separator (so a
/// multi-line prompt stays one argument) and succeeds.
fn cli(temp: &TempDir) -> (PathBuf, PathBuf) {
    let record = temp.path().join("cli.txt");
    let script = temp.path().join("fake-codebuff");
    write_command(
        &script,
        &format!(
            "#!/usr/bin/env bash\nfor argument in \"$@\"; do printf '%s\\036' \"$argument\"; done > '{record}'\nexit 0\n",
            record = record.display()
        ),
    );
    (script, record)
}

fn recorded_arguments(record: &Path) -> Vec<String> {
    let text = fs::read_to_string(record).expect("the CLI recorded its arguments");
    text.split('\u{1e}')
        .filter(|argument| !argument.is_empty())
        .map(str::to_string)
        .collect()
}

fn run(bridge: Option<&Path>, cli: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(dispatcher());
    command.args(args);
    command.env_remove("LEOPARDCRAFT_SPAWN_AGENTS");
    command.env("LEOPARDCRAFT_LANE_CLI", cli);
    if let Some(bridge) = bridge {
        command.env("LEOPARDCRAFT_SPAWN_AGENTS", bridge);
    }
    command.output().expect("run scripts/spawn-lane.sh")
}

#[test]
fn spawn_agents_bridge_runs_first_and_reports_its_path() {
    let temp = TempDir::new("bridge");
    let (bridge_script, bridge_record) = bridge(&temp, 0);
    let (cli_script, cli_record) = cli(&temp);

    let output = run(
        Some(&bridge_script),
        &cli_script,
        &["geometry", "Cut the seam along the picked edge"],
    );
    assert!(
        output.status.success(),
        "the bridge path must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("dispatch: spawn_agents (lc-geometry)"),
        "{stdout}"
    );
    let recorded = fs::read_to_string(&bridge_record).expect("the bridge recorded its call");
    assert!(
        recorded.contains("lc-geometry|Cut the seam along the picked edge"),
        "{recorded}"
    );
    assert!(
        !cli_record.exists(),
        "the CLI must not run when the spawn_agents bridge succeeds"
    );
}

#[test]
fn a_missing_bridge_falls_back_to_the_promptable_cli() {
    let temp = TempDir::new("fallback");
    let (cli_script, cli_record) = cli(&temp);

    let output = run(
        None,
        &cli_script,
        &["automation", "Wire the HTTP transport"],
    );
    assert!(
        output.status.success(),
        "the CLI fallback must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!(
            "dispatch: cli {} (lc-automation)",
            cli_script.display()
        )),
        "{stdout}"
    );
    let arguments = recorded_arguments(&cli_record);
    assert_eq!(
        arguments,
        [
            "--trust-agents",
            "--cwd",
            repo_root().to_str().expect("utf-8 repository path"),
            "Use the lc-automation agent with this LeopardCraft lane task packet. Read AGENTS.md,\ndocs/agents/README.md and docs/agents/automation.md first, stay inside the lane's\npaths (or request a handoff), run scripts/check-lane.sh automation before reporting,\nand answer in the handoff format. Do not commit, push or add dependencies.\n\nWire the HTTP transport",
        ],
        "{arguments:?}"
    );
}

#[test]
fn a_failing_bridge_selects_the_cli_and_says_why() {
    let temp = TempDir::new("bridge-fails");
    let (bridge_script, _bridge_record) = bridge(&temp, 3);
    let (cli_script, cli_record) = cli(&temp);

    let output = run(
        Some(&bridge_script),
        &cli_script,
        &["editor", "Add the Scalpel button"],
    );
    assert!(
        output.status.success(),
        "the fallback must still dispatch: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("spawn_agents bridge exited 3"),
        "the fallback must report why it was selected: {stderr}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("dispatch: cli"), "{stdout}");
    assert!(
        cli_record.exists(),
        "the CLI must run when the bridge fails"
    );
}

#[test]
fn with_no_bridge_and_no_cli_the_error_names_the_one_action() {
    let temp = TempDir::new("no-path");
    let missing_cli = temp.path().join("no-such-codebuff");

    let output = run(None, &missing_cli, &["document", "Land the command"]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "a missing path is a usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("LEOPARDCRAFT_SPAWN_AGENTS"),
        "the error must name the spawn bridge: {stderr}"
    );
    assert!(
        stderr.contains("codebuff"),
        "the error must name the CLI fallback: {stderr}"
    );
    assert!(
        stderr.contains("--print-packet"),
        "the error must offer the interactive escape hatch: {stderr}"
    );
    assert!(output.stdout.is_empty(), "nothing was dispatched");
}

#[test]
fn the_cli_failure_is_reported_instead_of_a_success_line() {
    let temp = TempDir::new("cli-fails");
    let failing_cli = temp.path().join("failing-codebuff");
    write_command(&failing_cli, "#!/usr/bin/env bash\nexit 4\n");

    let output = run(
        Some(Path::new("/nonexistent/bridge")),
        &failing_cli,
        &["rendering", "Tune the tracer"],
    );
    // The bridge path is unusable, so the CLI is used and its failure surfaces.
    assert_eq!(
        output.status.code(),
        Some(4),
        "the CLI status is propagated"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("was not dispatched"),
        "a CLI failure must not look like a dispatch: {stderr}"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("dispatch:"));
}

#[test]
fn packets_come_from_arguments_files_or_stdin_and_bad_input_is_refused() {
    let temp = TempDir::new("packets");
    let (cli_script, cli_record) = cli(&temp);

    // @file packets are read and printed without dispatching.
    let packet_file = temp.path().join("packet.md");
    fs::write(&packet_file, "File packet body").expect("write packet file");
    let argument = format!("@{}", packet_file.display());
    let output = run(
        None,
        &cli_script,
        &["--print-packet", "interchange", &argument],
    );
    assert!(output.status.success(), "print-packet succeeds");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("File packet body"), "{stdout}");
    assert!(!cli_record.exists(), "--print-packet must not dispatch");

    // A missing packet file is a usage error.
    let output = run(None, &cli_script, &["geometry", "@missing-packet.md"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("does not exist"));

    // An unknown lane is refused before any dispatch.
    let output = run(None, &cli_script, &["frontend", "Do it"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown lane"));

    // `-` reads the packet from stdin.
    let mut child = Command::new(dispatcher())
        .args(["--print-packet", "geometry", "-"])
        .env_remove("LEOPARDCRAFT_SPAWN_AGENTS")
        .env("LEOPARDCRAFT_LANE_CLI", &cli_script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the dispatcher");
    child
        .stdin
        .as_mut()
        .expect("piped stdin")
        .write_all(b"Packet from stdin\n")
        .expect("write the packet");
    let output = child.wait_with_output().expect("collect the dispatcher");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Packet from stdin"));
}
