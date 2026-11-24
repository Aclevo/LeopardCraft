# Automation lane

You own `crates/lc-mcp`, `app`, `scripts/mcp-server.sh` and `.mcp.json`: the
Model Context Protocol server, the headless CLI and the real-process smoke
test. Read `AGENTS.md`, `docs/agents/README.md` and this brief before changing
anything.

## Mission

Expose LeopardCraft to agents and scripts without a GUI, over a clean stdio
protocol and a deterministic headless CLI. The MCP server owns its own
`Document` and `History`; it is not attached to a running editor.

## Entry points

- `crates/lc-mcp/src/server.rs` — JSON-RPC loop, handshake/discovery, protocol
  errors, resources and subscriptions, prompt dispatch, stdio framing.
- `crates/lc-mcp/src/http.rs` — opt-in Streamable HTTP transport on a loopback
  endpoint, with the Server-Sent Events stream for resource updates and the
  `Mcp-Session-Id` session rules (400 without the header, 404 for an unknown or
  terminated id, `DELETE /mcp` to end a session, oldest session evicted at 64).
- `crates/lc-mcp/src/tools.rs` — tool schemas, argument parsing and undoable
  edits.
- `app/src/main.rs` — CLI parsing and GUI/headless/MCP dispatch.
- `app/tests/mcp_smoke.rs` — real binary handshake, tool inventory/schemas,
  representative calls and `.mcp.json` launch.
- `scripts/mcp-server.sh` — cwd-independent launcher; `.mcp.json` — project
  client entry.

## Invariants

- stdout carries exactly one JSON-RPC message per line and nothing else.
  Diagnostics go to stderr. Any `println!` in server code corrupts the stream.
- Unknown tools and malformed requests use JSON-RPC errors; tool execution
  failures use `isError: true` results. Never mix the two.
- Support both protocol generations: the `2025-06-18` `initialize` handshake and
  the handshake-free `2026-07-28` revision, including `server/discover`.
- Object arguments accept a numeric id or exact name. Edits go through the
  editor's undo commands, one tool call per history entry. The server tracks the
  document path and dirty state across saves.
- The advertised tool inventory must exactly match `AGENTS.md` and the README
  tool list; `mcp_smoke` enforces the `AGENTS.md` side. Adding or removing a tool
  updates the schemas, the smoke test, the inventory and the README together.
- Keep the launcher cwd-independent and Bash-only. It builds a missing or stale
  binary, honors `LEOPARDCRAFT_MCP_BIN`, and does not write into the repository.
- The engine remains offline; no telemetry, network or wall-clock dependence in
  tool behavior. The Streamable HTTP transport is opt-in, binds loopback only,
  tracks `Mcp-Session-Id` sessions, and is never required by the stdio path.

## Interfaces and boundaries

You consume all lower crates through their public APIs and command types. New
scene capabilities require the document lane; new render/export options require
rendering or interchange. Do not add a tool that mutates `Document` directly
because the command does not exist yet — request the command first.

Do not edit `lc-scene`, `lc-anim`, `lc-geom`, `lc-shading`, `lc-render`,
`lc-io`, `lc-ui`, root scripts other than the MCP launcher, or `Cargo.toml`.

**Narrow documentation exception:** when adding or removing an MCP tool, you may
edit only the `## MCP tool inventory` section of `AGENTS.md` and the MCP tool
list in `README.md`, and must keep `mcp_smoke` green. Every other root-document
edit is an integrator handoff.

## Tests and check

Inline tests in `lc-mcp/src/server.rs` cover protocol/error behavior. The
real-process smoke test in `app/tests/mcp_smoke.rs` must pass for any protocol,
tool, launcher or `.mcp.json` change; it launches the built binary from outside
the repository. Add deterministic schema and tool-call tests for new tools.

```bash
scripts/check-lane.sh automation
```

The lane check formats and lints `lc-mcp` and `leopardcraft`, runs `lc-mcp`
tests and then the real MCP smoke test. That smoke test compares the inventory
with `AGENTS.md`, so the narrow documentation exception must be applied in the
same change. The integrator runs the full gate afterwards.

## TODO mapping

Own `TODO.md` section 7 (MCP automation) and the MCP/CLI limitations in
`README.md`. Attaching to a live editor remains open, but its contract is
decided and recorded in `TODO.md` §7: a loopback HTTP/JSON endpoint with a
per-session token published in a `0600` registry, an Origin allow-list, one
request at a time on the editor's UI thread, and attach augmenting rather than
replacing the editor's document. Coordinate contract needs with the owning lane
before starting.

## Handoff

Report files, commands and results, protocol/tool inventory changes, docs
edited under the exception, and anything not exercised by the smoke test (for
example a real MCP host's approval flow).
