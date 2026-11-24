# AGENTS.md

LeopardCraft is a clean-room, offline-first 3D modelling, rendering and
animation application written in Rust. The workspace contains a native
eframe/wgpu editor and headless engine, render and file-format libraries. There
is no telemetry, account or network dependency.

This is the instruction file for AI coding agents. Read it before changing code.
`README.md` is the user-facing manual, `docs/cheetah3d-parity.md` records the
researched Cheetah3D feature matrix, and `TODO.md` tracks feature work.

## Ground rules

- Do not commit, push, tag, deploy or publish unless the current request asks.
- Preserve existing uncommitted work. Never revert or reformat unrelated files.
- Make the smallest complete change that satisfies the request. Avoid unrelated
  refactors, extra features, new dependencies and broad speculative test sweeps.
- Keep the engine offline and deterministic: no network, telemetry, wall-clock
  time or unseeded randomness in evaluation, geometry or rendering paths.
- Add or update tests for changed logic. Update user-facing documentation when
  behavior, controls, formats or commands change.
- Follow existing patterns in the nearest crate instead of introducing a new
  framework or abstraction. Ask only when a decision cannot be derived from the
  code, docs or current request.

## Commands

Run from the repository root.

```bash
# Editor (needs a display and GPU)
cargo run -p leopardcraft

# All tests
cargo test --workspace

# Focused test by crate and name substring
cargo test -p lc-geom subdiv
cargo test -p lc-mcp save_and_open

# Deterministic headless smoke test: evaluation, native round-trip,
# OBJ/GLB/STL round-trips and PNG/EXR output
cargo run -p leopardcraft -- --headless selftest

# Offline render or interchange export
cargo run -p leopardcraft -- --headless render \
  --scene scene.lcscene --out render.png --width 640 --height 360 --samples 16
cargo run -p leopardcraft -- --headless export \
  --scene scene.lcscene --out model.glb

# Full gate: fmt, clippy -D warnings, all tests including the MCP smoke test,
# and git diff --check
scripts/check.sh

# Fast per-lane check for one agent lane (see "Multi-agent lanes")
scripts/check-lane.sh geometry

# MCP smoke test alone: real binary, initialize + tools/list + tool calls
cargo test -p leopardcraft --test mcp_smoke

# MCP stdio server for agent-driven scene inspection/editing
# (`mcp-server.sh` also works from any directory when called by absolute path)
scripts/mcp-server.sh --scene scene.lcscene
cargo run -p leopardcraft -- --mcp --scene scene.lcscene
```

The `scripts/check.sh` clippy step is stricter than CI (`-D warnings`). CI also
runs the headless selftest and, through `cargo test --workspace`, the MCP smoke
test. On Linux the GUI build needs the X11, Wayland,
xkbcommon and Vulkan development packages listed in
`.github/workflows/ci.yml`; headless tests do not need a display.

## Workspace map

| Path | Responsibility |
| --- | --- |
| `app/src/main.rs` | Binary entry point, argument parsing, GUI/headless/MCP dispatch |
| `crates/lc-core` | `Transform`, AABB, rays, PCG32, colour, tone mapping, stable hashing |
| `crates/lc-geom` | N-gon `Mesh`, primitives, splines, UVs, BVH, topology ops, subdivision, CSG |
| `crates/lc-shading` | Material library, node shader graph, procedural noise, texture cache |
| `crates/lc-anim` | F-curves, clips, per-property/channel registry, playback, indexed skinning |
| `crates/lc-scene` | Document/scene graph, tags, modifiers, IK, evaluation, undo, native format |
| `crates/lc-render` | Render-scene flattening, CPU path tracer, PNG/EXR output |
| `crates/lc-io` | Native `.lcscene`, OBJ/MTL, STL and glTF/GLB import/export |
| `crates/lc-mcp` | Model Context Protocol stdio server and its scene tools |
| `crates/lc-ui` | eframe editor: viewport, panels, timeline, material graph, lifecycle |
| `app/tests/mcp_smoke.rs` | Real-process MCP handshake, config launch and tool-inventory checks |
| `scripts` | Full-gate and per-lane checkers and the cwd-independent MCP launcher |
| `packaging` | Distribution packaging inputs |
| `docs/cheetah3d-parity.md` | Researched Cheetah3D parity matrix, sources and gaps |
| `docs/agents` | Multi-agent lane briefs and the orchestration playbook |
| `.agents` | Freebuff/Codebuff CLI lane agents and coordinator |
| `TODO.md` | Planned and completed feature work by workstream |

## Architecture

The core data path is:

```text
Document (scene + materials + settings)
   -> Document::evaluate(time)              crates/lc-scene/src/eval.rs
   -> EvaluatedScene (flat, render-ready)
   -> RenderScene -> PathTracer             crates/lc-render
   -> PNG / EXR

EvaluatedScene -> wgpu viewport overlay      crates/lc-ui
```

- `Document` owns a `Scene` (`BTreeMap<NodeId, Node>` plus ordered roots), a
  `MaterialLibrary` and `DocumentSettings`. Node IDs are stable `u64`s and must
  not be renumbered during ordinary edits.
- `NodeKind` is `Null`, `Mesh`, `Spline`, `Camera`, `Light` or `Joint`. Mesh
  geometry is either a live `Primitive` or an editable `Mesh`; modifiers are
  non-destructive and applied during evaluation.
- `Document::evaluate` is the authoritative engine path: animation and joint
  limits, IK and tag constraints, visibility chains, then geometry, modifier
  stacks, morph targets and skinning. It is deterministic and does not touch
  the GPU or clock. Do not duplicate evaluation logic in the renderer or UI.
- Undo is command-based (`crates/lc-scene/src/undo.rs`). One user action should
  be one `Command`/history entry. The editor and MCP server each own a
  `Document` plus a `History`; both use the same commands.
- The native `.lcscene` format is validated and sanitized on load in
  `crates/lc-scene/src/format.rs`; writes are atomic. `lc-io` dispatches
  interchange formats. `Document::signature()` is the semantic dirty-check
  signature and deliberately ignores the ID allocator.
- UI state lives in `LeopardApp` (`crates/lc-ui/src/app.rs`), with panels in
  `panels.rs`/`panels_more.rs`, toolbar/menu code in `shell.rs`, the wgpu
  viewport in `viewport.rs`, and selection/action tests in `selection_tests.rs`.
- The MCP server (`crates/lc-mcp/src/server.rs` and `tools.rs`) speaks
  newline-delimited JSON-RPC over stdio. It supports the `2025-06-18`
  `initialize` handshake and the `2026-07-28` handshake-free revision. It owns
  a separate document; it is not attached to a running editor window.

## Where to change what

| Task | Start here | Also update |
| --- | --- | --- |
| New primitive | `lc-geom/src/primitives.rs`, `Primitive` in `lc-scene/src/geometry.rs` | `build`/`label`, toolbar defaults in `lc-ui/src/shell.rs`, primitive validity and native round-trip tests |
| New modifier | `Modifier` variants/defaults/apply in `lc-scene/src/geometry.rs`, geometry in `lc-geom/src/ops.rs` | modifier stack UI in `lc-ui/src/panels.rs`, serde/default tests |
| New animatable property | `AnimProperty`/`AnimChannel` in `lc-anim/src/lib.rs` | sampling in `lc-scene/src/eval.rs`, F-curve registry UI, recording and undo tests |
| New tag or constraint | `Tag` in `lc-scene/src/scene.rs` | evaluation in `lc-scene/src/eval.rs`, tags panel, native round-trip/default test |
| New file format field | owning type plus `lc-scene/src/format.rs` | `lc-io` if interchange, README, legacy-default and round-trip tests |
| UI workflow or panel | `lc-ui/src/app.rs` plus `panels.rs`/`panels_more.rs` | `selection.rs` predicates, `selection_tests.rs`, README controls |
| MCP tool | `lc-mcp/src/tools.rs` | protocol/error tests in `lc-mcp/src/server.rs`, real-process checks in `app/tests/mcp_smoke.rs`, the MCP tool inventory below, README tool list |
| Rendering feature | `lc-render/src/scene.rs`, `tracer.rs`, `output.rs` | selftest/render tests, README limitations |

## Multi-agent lanes

The repository is split into six non-overlapping write scopes so several agents
can work on different parts of the software in parallel. `docs/agents/README.md`
is the orchestration playbook: lane dependencies, shared contracts, task packets,
handoffs, merge order and integration checks. Each lane has a brief with its
entry points, invariants and tests. `.claude/agents/lc-*.md` exposes the same
lanes as Claude Code project subagents; `.agents/lc-*.ts` and
`.agents/lc-coordinator.ts` expose them to the Freebuff/Codebuff CLI.

| Lane | Write scope | Brief | Check |
| --- | --- | --- | --- |
| geometry | `crates/lc-core`, `crates/lc-geom` | `docs/agents/geometry.md` | `scripts/check-lane.sh geometry` |
| document | `crates/lc-anim`, `crates/lc-scene` | `docs/agents/document.md` | `scripts/check-lane.sh document` |
| rendering | `crates/lc-shading`, `crates/lc-render` | `docs/agents/rendering.md` | `scripts/check-lane.sh rendering` |
| interchange | `crates/lc-io` | `docs/agents/interchange.md` | `scripts/check-lane.sh interchange` |
| editor | `crates/lc-ui` | `docs/agents/editor.md` | `scripts/check-lane.sh editor` |
| automation | `crates/lc-mcp`, `app`, `scripts/mcp-server.sh`, `.mcp.json` | `docs/agents/automation.md` | `scripts/check-lane.sh automation` |

The main/orchestrating session is the integrator. It owns `AGENTS.md`,
`CLAUDE.md`, `README.md`, `TODO.md`, `Cargo.toml`/`Cargo.lock`,
`scripts/check.sh`, `scripts/check-lane.sh`, `.github/`, `packaging/`,
`docs/cheetah3d-parity.md`, `docs/agents/README.md`, `.claude/agents/` and
`.agents/`. Each lane owns its brief and its write scope.

- A lane edits only its own paths and brief. A cross-lane need is a handoff with
  the exact requested API, never an edit in another crate.
- Shared contracts are `scene.rs`, `geometry.rs`, `eval.rs`, `undo.rs`,
  `format.rs`, the `lc-anim` channel registry, `EvaluatedScene` and the MCP tool
  inventory. The owning lane lands a contract change first, names its consumers,
  and dependent lanes wait.
- Additive APIs and backward-compatible defaults come first. Never copy
  evaluation, serialization or undo logic into another crate to avoid waiting.
- A lane check is the fast inner loop; the integrator runs `scripts/check.sh`
  after merging and before calling a batch done. A contract change needs the
  full gate before dependent lanes start.
- `.claude/agents/` and `.agents/` are thin host adapters over the same six
  lanes. Change them with the lane table and `scripts/check-lane.sh`.
- The automation lane may edit only the `## MCP tool inventory` section of this
  file and the MCP tool list in `README.md` when tools change; every other root
  document edit goes through the integrator.

## Cheetah3D parity work

- Treat `docs/cheetah3d-parity.md` as the researched source of truth. Update the
  relevant matrix row and sources when a parity claim or behavior changes.
- This is a clean-room project: use public documentation and original code. Do
  not copy proprietary code, inspect proprietary binaries, or reverse-engineer
  file internals; distinguish documented Cheetah3D behavior from LeopardCraft's
  own approximations in docs and commit-ready notes.
- Prefer a vertical slice: data model, evaluation, serialization and undo, UI,
  tests, then README/TODO/parity-doc updates. Keep each slice independently
  testable and undoable.
- Record clean-room choices and remaining gaps honestly. Do not claim exact
  parity where the public manual or other sources are silent.
- Check `TODO.md` for the current open items before starting; mark completed
  items and add newly discovered gaps in the same pass.

## Conventions and pitfalls

- **Determinism:** iterate ordered containers or sort before hashing/rendering;
  use `Pcg32` with explicit seeds. Do not depend on `HashMap` iteration order,
  system time or thread scheduling in outputs. `StableHasher` is for signatures,
  not general hashing.
- **Angles and units:** scene units are metres; UI/API angles are degrees,
  math is radians. Rotations are stored as normalised quaternions.
- **Finite data:** reject or sanitize NaN/infinite values at load boundaries and
  keep degenerate geometry/IK paths finite instead of panicking.
- **Backward compatibility:** new serialized fields need `#[serde(default)]`
  and a legacy-loading test. Old `.lcscene` files must keep loading.
- **Undo boundaries:** use existing commands rather than mutating `Document`
  directly from UI/MCP handlers. Multi-object actions apply all-or-nothing.
- **File safety:** use the existing atomic save helpers; never write directly
  over a user target or leave temporary files after a failure.
- **Creases** are sorted canonical point pairs; normalize them after topology
  changes. Subdivision schemes and legacy defaults are documented in `TODO.md`.
- **MCP stdout:** stdout must contain only one JSON-RPC message per line. Log to
  stderr. Tool execution failures use `isError: true` results; unknown tools and
  malformed requests use JSON-RPC errors.
- **No new dependencies** without a concrete need. Reuse workspace dependencies
  declared in the root `Cargo.toml`.
- **Clippy** runs with `-D warnings` in the gate. Fix the cause; do not add
  blanket `allow` attributes to silence lints.
- **Docs:** update README for user-visible behavior, `TODO.md` checkboxes for
  completed work, and the parity matrix only when making researched Cheetah3D
  parity claims.

## Testing and finishing

- Most tests are inline `#[cfg(test)] mod tests` beside the code; UI behavior
  tests live in `lc-ui/src/selection_tests.rs`. Follow the nearest pattern.
- Test observable behavior and edge cases, not implementation details. A serialized
  format change needs round-trip and legacy tests. A UI action needs an undo test
  when it mutates the document.
- The ignored `warning_stderr_child` test is an intentional child-process helper;
  leave it ignored.
- Run focused tests while iterating, then `scripts/check.sh` before finishing.
  A lane uses `scripts/check-lane.sh <lane>` as its fast inner loop; the
  orchestrating/integrating session runs the full gate after merging. Re-run
  the affected check after any repair; do not report an earlier pass as
  covering a later edit. MCP changes need the real-process smoke test
  (`app/tests/mcp_smoke.rs`) in addition to unit tests.
- GUI checks require launching the editor; headless tests cannot prove visual
  appearance. Say what was not visually verified instead of claiming it works.
- Report the files changed, the commands actually run and their results, and any
  remaining limitations. Do not commit unless asked.

## MCP tool inventory

The server advertises exactly these tools. The real-process smoke test compares
this section with `tools/list`:

`get_scene`, `list_objects`, `get_object`, `create_primitive`, `set_transform`,
`rename_object`, `delete_object`, `set_object_properties`, `list_materials`,
`undo`, `redo`, `save_scene`, `open_scene`, `new_scene`, `export_scene`,
`render_image`.

See `README.md` for their parameters and grouping.

## MCP quick start

The project-scoped `.mcp.json` launches `scripts/mcp-server.sh` through
`${CLAUDE_PROJECT_DIR:-.}`. Claude Code 2.1.327 runs project-scoped stdio
servers with the repository root as their working directory and sets
`CLAUDE_PROJECT_DIR` in the server environment; it finds this config from the
root and from a subdirectory. The relative command relies on that project-root
working directory. The wrapper itself resolves its own location, pins the
workspace `Cargo.toml` and `leopardcraft` package, changes to the repository
root and runs the built binary. Other MCP hosts that do not launch from the
project root should use the absolute path to `scripts/mcp-server.sh` (or the
built binary).

Claude Code asks for approval the first time it sees a project-scoped server;
until then `claude mcp list` reports `⏸ Pending approval`, not a failed server.

The launcher builds the binary when it is missing or older than the source
(so it needs `cargo` on `PATH` on first use); `LEOPARDCRAFT_MCP_BIN` overrides
the binary for tests and packaging. The launcher is Bash, so Windows hosts need
the same Git Bash environment as `scripts/check.sh`.

Resources: `leopardcraft://scene` (full native JSON) and
`leopardcraft://scene/summary`. The complete tool list is above.

To smoke-test the protocol without an MCP host:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | scripts/mcp-server.sh
```

Expect exactly two response lines and empty stderr apart from build messages.
Any accidental `println!` in server code corrupts the stream; write diagnostics
to stderr. `cargo test -p leopardcraft --test mcp_smoke` runs the complete
handshake and tool-call check, including a launch through `.mcp.json` from a
temporary directory outside the repository.
