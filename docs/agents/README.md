# Multi-agent orchestration

LeopardCraft is cut into six non-overlapping write scopes ("lanes") so several
coding agents can work on different parts of the software at the same time. The
canonical project instructions remain [`AGENTS.md`](../../AGENTS.md); every lane
agent reads that file first, then this playbook, then its own brief.

The repository does not run agents itself. The orchestrator (a main Claude Code
session, another coding host, or a human) dispatches task packets, collects
handoffs, applies shared-document edits and runs the full integration gate.
[`scripts/spawn-lane.sh`](../../scripts/spawn-lane.sh) is the single dispatch
interface for that: it hands one lane packet to the host's `spawn_agents` bridge
when one is exported, otherwise to the promptable Codebuff CLI, and it prints
the composed prompt for an interactive session. Host mechanics — why an
in-session `spawn_agents` can refuse every lane id, and the exact command for
Codebuff, `claude` and a manual session — are in [`spawning.md`](spawning.md).

## Lane model

Dependencies point from left to right:

```text
lc-core -> lc-geom / lc-anim / lc-shading -> lc-scene -> lc-render / lc-io
         -> lc-ui / lc-mcp -> app
```

| Lane | Owns | Depends on | Brief | Check |
| --- | --- | --- | --- | --- |
| geometry | `crates/lc-core`, `crates/lc-geom` | — | [`geometry.md`](geometry.md) | `scripts/check-lane.sh geometry` |
| document | `crates/lc-anim`, `crates/lc-scene` | geometry | [`document.md`](document.md) | `scripts/check-lane.sh document` |
| rendering | `crates/lc-shading`, `crates/lc-render` | document | [`rendering.md`](rendering.md) | `scripts/check-lane.sh rendering` |
| interchange | `crates/lc-io` | geometry, document | [`interchange.md`](interchange.md) | `scripts/check-lane.sh interchange` |
| editor | `crates/lc-ui` | all of the above | [`editor.md`](editor.md) | `scripts/check-lane.sh editor` |
| automation | `crates/lc-mcp`, `app`, `scripts/mcp-server.sh`, `.mcp.json` | all of the above | [`automation.md`](automation.md) | `scripts/check-lane.sh automation` |

`lc-core` is small but every lane links it, so geometry treats changes there as
contract-sensitive and reports them explicitly. `lc-ui` deliberately stays one
lane: `app.rs` is the editor's shared state hub, so splitting the UI across
parallel agents would create constant conflicts.

The lane briefs are host-agnostic. Two thin adapters expose the same lanes:

- `.claude/agents/lc-*.md` — Claude Code project subagents.
- `.agents/lc-*.ts` plus `.agents/lc-coordinator.ts` — Codebuff CLI local
  agents (Freebuff loads the same files but refuses their turns; see
  [`spawning.md`](spawning.md)).
  [`../../.agents/README.md`](../../.agents/README.md) covers the trust gate,
  Projects access and the shared model constant.

Both adapters are integrator-owned and change with the lane table and
`scripts/check-lane.sh`.

## Batch goals

Multi-lane batches are recorded under [`goals/`](goals/) with a stable id, lane
packets, acceptance checks and a definition of done. The goal file is the
startable unit for a new session: hand it to `lc-coordinator` in Codebuff, or
start a Freebuff Mission with the goal text (a Mission runs Freebuff's own
agents, not the lane definitions). [`goals/ROADMAP.md`](goals/ROADMAP.md) indexes the
remaining backlog as G30–G39; G30, G31 and G32 are complete, and `G33.md` is the next
startable mission. Update a goal's status as lanes land, and remove its pointers
from `AGENTS.md` and `TODO.md` when it closes.

## Shared files

Root documents, build metadata and orchestration tooling are owned by the
integrator (the main/orchestrating session), not by a lane:

- `AGENTS.md`, `CLAUDE.md`, `README.md`, `TODO.md`
- `Cargo.toml`, `Cargo.lock`
- `scripts/check.sh`, `scripts/check-lane.sh`, `scripts/spawn-lane.sh`
- `.github/`, `packaging/`, `.gitignore`
- `docs/cheetah3d-parity.md`, `docs/agents/README.md`, `.claude/agents/`

Each lane owns its own `docs/agents/<lane>.md`; keep it true when the lane's
interfaces change.

The one narrow exception: the automation lane may edit only the
`## MCP tool inventory` section of `AGENTS.md` and the MCP tool list in
`README.md` when it adds or removes a tool, because `app/tests/mcp_smoke.rs`
compares the inventory to `tools/list`. Every other root-document edit is a
handoff item for the integrator.

## Contract changes

These files and APIs are consumed across lanes. Changes to them are "contract
changes", and the owning lane lands them before dependent work starts:

| Contract | Owner | Consumers | Extra checks |
| --- | --- | --- | --- |
| `lc-scene/src/scene.rs` node, tag, skin and morph types | document | rendering, interchange, editor, automation | full gate after merge |
| `lc-scene/src/geometry.rs` `Primitive`/`Modifier` | document | geometry, rendering, editor | geometry + document lane checks |
| `lc-scene/src/eval.rs` `EvaluatedScene` | document | rendering, editor, automation | rendering + editor lane checks |
| `lc-scene/src/undo.rs` command vocabulary | document | editor, automation | editor + automation lane checks |
| `lc-scene/src/format.rs` serialized schema | document | interchange, editor | interchange lane check and a legacy-load test |
| `lc-anim/src/lib.rs` `AnimChannel` registry and `ChannelAddress` | document | editor, interchange | editor lane check |
| MCP tool inventory and schemas | automation | root docs, rendered/exported output | `scripts/check-lane.sh automation` |
| `crates/lc-core` public math/types | geometry | every crate | full gate after merge |

Rules for a contract change:

1. The owning lane implements it with a backward-compatible default where the
   data is serialized, plus a round-trip or legacy test.
2. The handoff names the changed API, its consumers, and the migration needed.
3. The integrator runs the extra checks above before dispatching dependent
   lanes. Do not let two lanes guess at the same contract.
4. Consumers adapt to the new API. They never copy evaluation, serialization or
   undo logic into their own crate to avoid waiting.

Additive APIs come first: add the new function/type/field beside the old one,
land it, then switch consumers and remove the old path in a separate task. A
lane must not edit another lane's crate, even for a one-line fix; it requests
the change instead.

## Dispatching work

Every task packet should contain:

```text
lane:           geometry | document | rendering | interchange | editor | automation
objective:      one user-visible or contract-level outcome
allowed paths:  the lane's owned paths, and nothing else
read first:     AGENTS.md, docs/agents/README.md, docs/agents/<lane>.md
todo item:      TODO.md section and checkbox, if any
acceptance:     exact tests/commands that must pass, plus edge cases
contract impact: none, or the exact shared API/schema change
out of scope:   adjacent features and cleanup that must not be touched
```

Dispatch rules:

- One lane per worktree/branch when the host supports isolation. Otherwise run
  only lanes whose paths are disjoint, and never let two agents edit the same
  shared file in one round.
- Run a lane check when the task is handed off, not after every edit. Full gate
  runs once per integration batch.
- A feature slice that spans lanes is one sequence of lane tasks with a single
  owner for the user-visible outcome. The prerequisite lane lands its API first;
  the owning lane then wires the slice and reports documentation debt.
- Lanes may run concurrently when they consume only already-landed contracts.
  `document` is the usual serialization point: wait for its schema changes
  before starting interchange, editor or automation work that needs them.

Two worked examples:

- **UV seam authoring** (geometry + document + editor): geometry exposes seam
  chart borders in `lc-geom/src/uv.rs`; document persists seam state and adds
  the undo command; editor adds the toggle and viewport colour. The geometry
  task lands first, then document, then editor. The editor task owns the final
  user-visible acceptance test; the geometry task owns the algorithm tests.
- **glTF animation import** (interchange + document): document exposes any
  missing skin/animation types with serde defaults and a legacy-load test;
  interchange reads them and reports skipped data. If no contract change is
  needed, interchange runs alone and adds parse/round-trip tests.
- **Multiple UV sets** (geometry + document + editor + interchange): geometry
  adds the parallel-set storage helpers and slot-preserving operations;
  document gives the field a serde default, repairs damaged lengths and tests
  native round-trip; editor adds the set selector and routes UV edits to the
  active set; interchange finally maps the sets to glTF `TEXCOORD_n` and names
  the OBJ loss. Material binding to a non-zero set stays out of this sequence
  because it touches the rendering lane.

## Working agreement

- Never commit, push, tag, deploy or publish unless the current request asks.
  The integrator owns that decision.
- Do not add dependencies, run `cargo clean`, reformat another lane's files, or
  revert unrelated work. Reuse workspace dependencies.
- Keep the engine offline and deterministic. No network, wall-clock time or
  unseeded randomness in evaluation, geometry or rendering. Lane checks must not
  need a display.
- Preserve the lane boundary even when a fix looks trivial. Report the exact
  file, signature and reason instead of editing across scopes.
- Follow the root testing rules: observable behavior, edge cases, undo tests for
  document mutations, legacy tests for serialized defaults, and the real MCP
  smoke test for protocol changes.
- Owed documentation (README, TODO, parity matrix) is reported at handoff; the
  integrator applies it after the lane passes, except the automation inventory
  exception above.

## Verification and integration

A lane is done only when:

- `scripts/check-lane.sh <lane>` passes;
- the change stays inside the lane's paths (or the requested contract change was
  approved first);
- the handoff lists files, commands, results and contract impact;
- GUI work says plainly what was not visually verified.

Merge order follows the dependency graph: geometry → document → rendering and
interchange → editor and automation. After merging a batch, the integrator runs
`scripts/check.sh` (fmt, clippy with `-D warnings`, the workspace tests, the MCP
smoke test and `git diff --check`). A lane check is an inner loop and is not a
merge proof: it does not compile dependents.

Rebase or re-apply a lane's work before its check; never report a pre-merge pass
as covering a later edit. If a lane check fails for a reason outside the lane,
stop and report it rather than editing the offending crate.

## Handoff format

```text
objective:      what was asked and what landed
files:          every changed path
commands:       exact commands run, with pass/fail
contract:       shared API/schema changes and consumers affected
docs owed:      README/TODO/parity updates the integrator must apply
limitations:    what was not run or not verified (for example GUI appearance)
follow-ups:     small discovered gaps, not a redesign proposal
```

## Maintaining the setup

The lane table here, the summary in `AGENTS.md`, the case mapping in
`scripts/check-lane.sh`, the six wrappers in `.claude/agents/` and the six
definitions plus coordinator in `.agents/` describe the same six lanes. Change
them together in one integrator edit. If a crate is added
or split, give it an owning lane, a brief, a check mapping and a subagent
wrapper, or fold it into an existing lane explicitly.

Lane checks intentionally format and test only their own packages, so they do
not reformat another lane's in-progress work. All lanes share one Cargo build
directory, so concurrent checks queue on Cargo's lock; do not run the full gate
inside every lane.
