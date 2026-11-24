# Freebuff / Codebuff local agents

This directory mirrors the six LeopardCraft agent lanes for the Freebuff and
Codebuff CLI. `lc-coordinator` splits a change into lane task packets and spawns
the six lane agents; the lane definitions carry the same ownership, invariants
and handoff rules as `docs/agents/` and `.claude/agents/`.

| Agent | Lane | Owns |
| --- | --- | --- |
| `lc-coordinator` | orchestration | no files; dispatches the six lanes |
| `lc-geometry` | geometry | `crates/lc-core`, `crates/lc-geom` |
| `lc-document` | document | `crates/lc-anim`, `crates/lc-scene` |
| `lc-rendering` | rendering | `crates/lc-shading`, `crates/lc-render` |
| `lc-interchange` | interchange | `crates/lc-io` |
| `lc-editor` | editor | `crates/lc-ui` |
| `lc-automation` | automation | `crates/lc-mcp`, `app`, `scripts/mcp-server.sh`, `.mcp.json` |

The authoritative rules remain [`AGENTS.md`](../AGENTS.md) and the
[orchestration playbook](../docs/agents/README.md). Each lane agent reads its
brief before editing.

## Requirements

- **Freebuff or Codebuff CLI.** The CLI loads `.agents` definitions from the
  project directory (and a parent directory), then exposes them as spawnable
  agents. Freebuff Desktop's `.agents` support is not documented; use the CLI.
- **Projects access** for full coding subagents. Freebuff offers them only in a
  Build session with Projects access, not in Plan mode or on sponsored runs.
- **Project trust.** Loading repository `.agents` code requires approval. On an
  interactive launch, accept the `Load and run these? [y/N]` prompt. For a
  non-interactive run, pass `--trust-agents` or set
  `CODEBUFF_TRUST_AGENT_DIRS=1` for that invocation. Trust is stored per
  absolute directory in the CLI config.
- **A usable model.** Local agents must name a model and do not inherit the
  session model. `.agents/types/model.ts` defaults to
  `deepseek/deepseek-v4-flash`, Freebuff's current free-tier default. Set
  `LEOPARDCRAFT_LANE_MODEL` before launching, or edit that constant, to the model
  your account and session can run.

## Using the lanes

Start the CLI in the repository and approve the `.agents` directory once:

```bash
freebuff
```

Then ask for the coordinator or a specific lane by name, for example:

```text
Use the lc-coordinator agent to implement the next UV seam task from TODO.md,
with the geometry and document lanes first and the editor lane after.
```

or dispatch one lane directly:

```text
Use the lc-geometry agent with this packet: objective ..., allowed paths ...,
acceptance `scripts/check-lane.sh geometry`, contract impact none, out of scope
editor UI changes.
```

The coordinator may spawn all six lanes in one call (the runtime limit is six
per `spawn_agents` call). Each returns the handoff format from
`docs/agents/README.md`; the coordinator checks scope and runs the full
`scripts/check.sh` gate after the integrator applies the batch.

## Checks and boundaries

Each lane agent runs `scripts/check-lane.sh <lane>` before reporting. The full
gate remains `scripts/check.sh`. The lane checker is host-agnostic and also works
outside Freebuff.

These definitions are executable repository code, like any project agent file:
read them before trusting the directory, and do not add secrets to them.

When a lane's scope, brief or check changes, update the matching
`.agents/lc-*.ts`, `.claude/agents/lc-*.md`, `docs/agents/<lane>.md`,
`AGENTS.md` and `scripts/check-lane.sh` together.
