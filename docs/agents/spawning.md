# Spawning lane sub-agents

`spawn_agents` is a tool owned by the **agent host**, not by this repository.
The repository only supplies agent definitions (`.agents/lc-*.ts` for the
Codebuff CLI, `.claude/agents/lc-*.md` for Claude Code) and the
dispatch script [`scripts/spawn-lane.sh`](../../scripts/spawn-lane.sh). A host
that knows nothing about those directories cannot spawn them, however correct
the definitions are — and one host that *does* read them still refuses their
turns. This page records what fails, why, and the commands that actually
dispatch lane work.

The *content* of the work — lane boundaries, task packets, contract rules,
merge order, handoff format — stays in [`README.md`](README.md); this page is
only about the mechanism.

## Symptom and diagnosis

Observed 2026-11-24 in a Freebuff Desktop session:

```text
spawn_agents(agent_type: "lc-geometry") -> Agent "lc-geometry" is not available to spawn.
spawn_agents(agent_type: "basher")      -> Agent "basher" is not available to spawn.
```

The second line is the useful one. A typo in a lane id would fail only that
lane; refusing a standard built-in agent too means the host publishes an
allow-list, and that list contained exactly one entry: `researcher-web`.

So the failure is **not** in `.agents/`, `.claude/agents/`,
`docs/agents/` or `scripts/spawn-lane.sh`. It is host capability:

| Path | State in that session | Consequence |
| --- | --- | --- |
| In-session `spawn_agents` | advertises only `researcher-web` | lane agents cannot be spawned |
| `LEOPARDCRAFT_SPAWN_AGENTS` | unset | no bridge for `scripts/spawn-lane.sh` |
| `freebuff` on `PATH` | the desktop bundle (`/tmp/.mount_*/freebuff`) | its CLI accepts no prompt, so it cannot be the `LEOPARDCRAFT_LANE_CLI` fallback |
| `codebuff` | not installed | CLI fallback unavailable |
| `claude` | 2.1.327 installed, **not logged in** | `.claude/agents/` route needs `claude /login` |

## Two products, two builds

The upstream monorepo builds both clients from one `cli/` package. Freebuff is
the `FREEBUFF_MODE=true` variant, published as its own npm package
(`freebuff`), and the two ship with separate accounts and billing. They are not
interchangeable, and **Freebuff hours cannot fund a lane agent**:

- **The Freebuff CLI loads `.agents`, but cannot dispatch.** Installing it with
  `npm install -g freebuff` gives a client whose parser
  (`cli/src/cli-args.ts`) has no prompt argument and no `--agent`; it accepts
  `login`/`doctor`, `--continue`, `--cwd`, `--trust-agents` and `--version`
  only. `parseArgs` hardcodes `initialPrompt` to `null` when `isFreebuff`, so
  there is nothing to pass a packet to.
- **Free-mode admission allowlists agent ids, not just models.** The chat
  admission gate pairs agent and model: `isFreeModeAllowedAgentModel` in
  `common/src/constants/free-agents.ts` returns `false` unless the id appears
  in `FREE_MODE_AGENT_MODELS`, a map of bundled Freebuff ids (`base2-free*`,
  `base3-free*`, `file-picker`, `researcher-web`, `code-reviewer-*`, the
  Desktop thread roots). Unknown ids are refused with
  `free_mode_invalid_agent_model`; the map's own comment says it exists to stop
  "users trying to use arbitrary agents for free". A repository agent such as
  `lc-geometry` is in no catalog, so it loads and is then refused.
- **The model string is not the blocker.** `deepseek/deepseek-v4-flash` — the
  default in [`.agents/types/model.ts`](../../.agents/types/model.ts) — is
  Freebuff's Flash wire id (`common/src/constants/freebuff-model-ids.ts`;
  V4.1 Flash serves under it), and local loading only requires a definition to
  carry a non-empty `id` and `model` (`sdk/src/agents/load-agents.ts`). The
  refusal is keyed on the agent id and its pairing with a model, so changing
  `LEOPARDCRAFT_LANE_MODEL` does not open the door.
- **Custom agents are a Codebuff feature.** The Freebuff README hands custom
  agents to Codebuff ("To create custom agents or embed them in another
  application, see the Codebuff documentation and `@codebuff/sdk`"). Codebuff
  is the paid product: credits or an API key, its own model catalog.

One adjacent option the Freebuff README does document: Freebuff Desktop can run
locally installed Claude Code and Codex agents on your own provider account,
separate from Freebuff's catalog. That is a route for `.claude/agents/` lanes,
not for `.agents/` ones, and it needs that account signed in.

## Using Freebuff's free models for lane work

The gate blocks lane *agents*, not lane *work*. Two legitimate ways to keep
Freebuff's free catalog:

- **One scoped packet per session.** Compose it with
  `scripts/spawn-lane.sh --print-packet <lane> <packet>` and paste it into a
  Freebuff CLI session or Desktop tab. Desktop isolates concurrent agents in
  separate workspaces, so one tab per lane runs disjoint scopes in parallel and
  the integrator merges. Hours are metered per session, so parallel lanes spend
  in parallel, and every merge stays manual.
- **One integrator, sequential lanes.** A single Freebuff session does each
  lane in turn, running `scripts/check-lane.sh <lane>` after each and
  `scripts/check.sh` once at the end. Slower, but every rule in
  [`README.md`](README.md) still holds and no second agent is needed.

Either way the lane boundary is carried by the packet (allowed paths,
acceptance, out of scope), not by an agent id, and Freebuff's own allowlisted
helpers still do the in-session file exploration, research and review.

Freebuff does ship one first-party subagent that edits:
`base3-fast-worker-deepseek-flash`, the parallel worker of the retired
`base3-fast` fan-out. It is pinned to the fast DeepSeek model (a mismatch fails
the session gate with `session_model_mismatch`), inherits its root's prompt and
history, and since 2026-10-30 is spawned only by released builds that still
carry the fan-out. Its id is fixed and its definition is compiled into the
product, so it cannot be repointed at an `lc-*` lane: it is history, not an
extension point.

**Not an acceptable workaround:** shipping a lane under an allowlisted bundled
id (`file-picker`, a Desktop thread root, `researcher-web`, ...) so it slips
past `FREE_MODE_AGENT_MODELS`. Local definitions override bundled agents with
the same id, so it would technically load, but that gate exists to stop
arbitrary agents running on free capacity; using it that way is circumvention,
not configuration. If you need real `lc-*` subagents, change host (Codebuff or
Claude Code), never the agent id.

## Choosing a dispatch path

Pick from the top; each step is only a fallback for the one above.

### 1. Codebuff CLI — the only host that loads `.agents` and admits their turns

```bash
npm install -g codebuff            # requires node/npm; paid credits or API key
cd /path/to/LeopardCraft
codebuff                           # approve the .agents trust prompt once
```

Interactively, ask for a lane or the coordinator by name. Non-interactively, the
CLI takes an initial prompt and one agent id:

```bash
CODEBUFF_TRUST_AGENT_DIRS=1 codebuff --trust-agents --cwd . \
  "Use the lc-coordinator agent with this packet: objective ..., lanes ...,
   acceptance scripts/check-lane.sh <lane>, contract impact none."

codebuff --agent lc-geometry --cwd . "<packet>"
```

Inside that session `spawn_agents` exposes `lc-geometry`, `lc-document`,
`lc-rendering`, `lc-interchange`, `lc-editor`, `lc-automation` and
`lc-coordinator`. Local definitions must name a model and do not inherit the
session model; set `LEOPARDCRAFT_LANE_MODEL` to a model your Codebuff account
can run. The repository default is Freebuff's wire id, which is not a Freebuff
entitlement you can carry across.

Trust is per absolute directory, recorded in
`<configDir>/trusted-agent-dirs.json`; non-interactive runs skip untrusted
directories unless `--trust-agents` or `CODEBUFF_TRUST_AGENT_DIRS=1` is set.

### 2. `scripts/spawn-lane.sh` — one interface, no host branching

```bash
scripts/spawn-lane.sh geometry "Cut the seam along the picked edge"
scripts/spawn-lane.sh geometry @packets/seam.md
scripts/spawn-lane.sh --print-packet document - <<'EOF'
objective: land the seam undo command
...
EOF
```

Resolution order:

1. `LEOPARDCRAFT_SPAWN_AGENTS` — a command that runs the host's `spawn_agents`.
   It is invoked as `<command> lc-<lane> <packet>` with the packet also on
   stdin. Only a host that owns an in-process `spawn_agents` can supply this,
   because a subprocess cannot call a tool that lives inside the agent process.
2. `LEOPARDCRAFT_LANE_CLI` (default `codebuff`) invoked as
   `--trust-agents --cwd <repo root> "<prompt>"`. This is why the Codebuff CLI
   works out of the box and why neither the `freebuff` CLI nor the desktop
   client can be this fallback: both reject the prompt argument.
3. Neither: exit 2, with the single action that unblocks dispatch.

A failing bridge or CLI is reported, never swallowed: the bridge's non-zero
status prints a warning and falls back, a CLI failure propagates its status, and
the success line is always `dispatch: spawn_agents (<agent>)` or
`dispatch: cli <bin> (<agent>)`. `--print-packet` composes the prompt without
dispatching.

### 3. Claude Code project subagents

`.claude/agents/lc-*.md` are ordinary project subagents (`name`,
`description`, `tools`, `model: inherit`). With the CLI logged in:

```bash
claude                                   # then ask for the lc-geometry subagent
claude -p --agent lc-geometry "<packet>"  # non-interactive
```

`--agent` selects the definition for the session, so this runs one lane per
invocation. Wrap it as a bridge when you need `scripts/spawn-lane.sh` to use it:

```bash
#!/usr/bin/env bash
# LeopardCraft lane bridge for Claude Code; export it as
# LEOPARDCRAFT_SPAWN_AGENTS=/absolute/path/to/lane-claude-bridge.sh
set -euo pipefail
agent="$1"; packet="$2"
cd "${LEOPARDCRAFT_ROOT:-$(dirname "$0")/..}"
exec claude -p --agent "$agent" "$packet"
```

Not verified on this machine: the 2.1.327 binary answered
`Not logged in · Please run /login`, and `claude agents --json` returned `[]`
while unauthenticated. Confirm with an interactive `claude agents` before
relying on this route.

### 4. No spawn host at all (this Desktop session)

Two honest options, both single-session:

- Compose the packet and hand it to a human-launched CLI or Claude session:
  `scripts/spawn-lane.sh --print-packet <lane> <packet>`.
- Act as integrator yourself: run the lanes **sequentially**, one lane's paths at
  a time, running `scripts/check-lane.sh <lane>` after each and the full
  `scripts/check.sh` once at the end. This is slower but keeps every rule in
  [`README.md`](README.md) intact — no two writers in one scope, contracts land
  before consumers.

## Orchestrating a batch

The rules live in [`README.md`](README.md#dispatching-work); the short version
that matters when you are the one calling `spawn_agents`:

- **Packets, not topics.** Every packet names lane, objective, allowed paths,
  read-first files, TODO item, acceptance commands, contract impact and out of
  scope. One user-visible or contract-level outcome each.
- **Six per call, disjoint scopes.** `spawn_agents` accepts at most six agents
  per call, which happens to be exactly the lane count. Never put two lanes on
  the same shared file in one round.
- **Contracts first.** `document` (and `geometry` for `lc-core`) lands shared
  API/schema changes before rendering, interchange, editor or automation
  consume them. Sequence rounds; do not let two lanes guess the same contract.
- **The orchestrator does not edit code.** Dispatch, read handoffs, apply owed
  documentation, run the full gate. Anything else is a lane's job.
- **A lane check is not a merge proof.** `scripts/check-lane.sh <lane>` is the
  inner loop and does not compile dependents; `scripts/check.sh` runs once per
  integration batch.

A coordinator prompt that works on any of the hosts above:

```text
Use the lc-coordinator agent to implement <outcome> from TODO.md section <n>.
Dispatch geometry and document first, then interchange and editor once their
contracts land. Each lane: allowed paths only, scripts/check-lane.sh <lane>
before reporting, handoff format from docs/agents/README.md. Do not commit,
push or add dependencies. Return the integration summary.
```

## Verifying that a spawn happened

- The tool call succeeds and returns an agent id; a refusal reads
  `Agent "<id>" is not available to spawn` and means the host's allow-list, not
  the packet, is the problem.
- `free_mode_invalid_agent_model` is a different refusal: the host accepted the
  agent id but the server's free-mode allowlist did not. That is a Freebuff
  session trying to run a custom agent; move the work to Codebuff or Claude
  Code rather than editing the definition.
- `scripts/spawn-lane.sh` prints one `dispatch:` line on success and nothing on
  stdout when it only composed a prompt.
- The report is in the handoff format and names the lane check it ran. A report
  without a lane check is unfinished work, not a completed handoff.
- If the host exposes only built-in agents (`researcher-web` here), a research
  sub-agent is still worth spawning for web research, but it must not be handed
  lane code: it has no lane brief and no write scope.

## Sources

- `CodebuffAI/freebuff` — `cli/src/cli-args.ts` (Freebuff argument surface),
  `cli/src/utils/constants.ts` (`IS_FREEBUFF`), `cli/src/utils/agent-dir-trust.ts`
  (trust gate), `common/src/constants/free-agents.ts`
  (`FREE_MODE_AGENT_MODELS`, `isFreeModeAllowedAgentModel`),
  `common/src/constants/freebuff-model-ids.ts` (wire ids),
  `sdk/src/agents/load-agents.ts` (local loading), `docs/agents-and-tools.md`
  (trust gate and tool surface), `README.md` (products, models, custom agents).
- `CodebuffAI/codebuff` — the same files without `IS_FREEBUFF`, plus
  `freebuff/SPEC.md` (the build-time split).
