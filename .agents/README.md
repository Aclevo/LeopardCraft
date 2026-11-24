# Codebuff local agents

This directory mirrors the six LeopardCraft agent lanes for the Codebuff CLI.
Freebuff loads the same definitions but refuses custom agent turns (see
[Requirements](#requirements)). `lc-coordinator` splits a change into lane task packets and spawns
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

- **Codebuff CLI, not Freebuff.** The Codebuff CLI loads `.agents` definitions
  from the project directory (and a parent directory) and admits their turns;
  custom agents are a Codebuff feature, paid in credits or an API key. The
  Freebuff build loads the same files but cannot run them: its parser accepts
  no prompt and no `--agent`, and free-mode admission only allows its own
  bundled agent ids, so an `lc-*` turn is refused with
  `free_mode_invalid_agent_model`. Freebuff Desktop loads nothing at all (its
  in-session `spawn_agents` advertised only `researcher-web`, observed
  2026-11-24). Details and sources:
  [`docs/agents/spawning.md`](../docs/agents/spawning.md).
- **Freebuff Projects access is not a route.** Freebuff's own coding subagents
  exist only in a Build session with Projects access (not in Plan mode or on
  sponsored runs), but they are Freebuff's agents, not these definitions.
- **Project trust.** Loading repository `.agents` code requires approval. On an
  interactive launch, accept the `Load and run these? [y/N]` prompt. For a
  non-interactive run, pass `--trust-agents` or set
  `CODEBUFF_TRUST_AGENT_DIRS=1` for that invocation. Trust is stored per
  absolute directory in the CLI config.
- **A usable model.** Local agents must name a model and do not inherit the
  session model. `.agents/types/model.ts` defaults to
  `deepseek/deepseek-v4-flash`, which is a valid Freebuff wire id (V4.1 Flash
  serves under it) but not a Freebuff entitlement the lane can carry: Freebuff
  free mode refuses custom agent ids outright. Set `LEOPARDCRAFT_LANE_MODEL`
  before launching, or edit that constant, to a model your Codebuff account can
  run.

## Using the lanes

Start the Codebuff CLI in the repository and approve the `.agents` directory
once:

```bash
codebuff
```

The Freebuff CLI cannot stand in for it: it loads the same definitions but
accepts no prompt and its free mode refuses custom agent turns (see
Requirements, and [`docs/agents/spawning.md`](../docs/agents/spawning.md)).

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

## Dispatching without an in-session spawn tool

`scripts/spawn-lane.sh [--print-packet] <lane> <packet|@file|->` is the
repository's single dispatch interface. Hand it a lane packet and it chooses the
path, so callers never branch on the host:

1. **`LEOPARDCRAFT_SPAWN_AGENTS`** — a command that runs the host's
   `spawn_agents` tool. It receives the agent id and the packet as arguments,
   with the packet also on stdin. Only a host that owns that tool can supply it,
   because a subprocess cannot call a tool that lives inside the agent process.
2. **Promptable CLI** — otherwise `LEOPARDCRAFT_LANE_CLI` (default `codebuff`)
   runs as `--trust-agents --cwd <repo root> "<packet>"`. The interactive
   `freebuff` client accepts no prompt argument, so it cannot be driven this
   way; if it is all you have, run
   `scripts/spawn-lane.sh --print-packet <lane> <packet>` and paste the prompt
   into that session. Claude Code is a third option once logged in:
   `claude -p --agent <agent> "<prompt>"` runs the matching
   `.claude/agents/` subagent, and the bridge snippet in
   [`docs/agents/spawning.md`](../docs/agents/spawning.md) exposes it through
   `LEOPARDCRAFT_SPAWN_AGENTS`.
3. **Neither** — the script exits 2 and names that one action.

## Checks and boundaries

Each lane agent runs `scripts/check-lane.sh <lane>` before reporting. The full
gate remains `scripts/check.sh`. The lane checker is host-agnostic and also works
outside Codebuff.

These definitions are executable repository code, like any project agent file:
read them before trusting the directory, and do not add secrets to them.

When a lane's scope, brief or check changes, update the matching
`.agents/lc-*.ts`, `.claude/agents/lc-*.md`, `docs/agents/<lane>.md`,
`AGENTS.md` and `scripts/check-lane.sh` together.
