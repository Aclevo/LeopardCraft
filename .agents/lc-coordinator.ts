import type { AgentDefinition } from './types/agent-definition'
import { LANE_MODEL } from './types/model'

const definition: AgentDefinition = {
  id: 'lc-coordinator',
  version: '1.0.0',
  publisher: 'leopardcraft',
  displayName: 'LeopardCraft Lane Coordinator',
  spawnerPrompt:
    'Spawn to split a LeopardCraft change into lane-scoped task packets and dispatch the six lane agents in parallel, without editing code itself.',
  model: LANE_MODEL,
  outputMode: 'last_message',
  includeMessageHistory: true,
  toolNames: [
    'read_files',
    'code_search',
    'find_files',
    'run_terminal_command',
    'spawn_agents',
    'end_turn',
  ],
  spawnableAgents: [
    'lc-geometry',
    'lc-document',
    'lc-rendering',
    'lc-interchange',
    'lc-editor',
    'lc-automation',
  ],
  inputSchema: {
    prompt: {
      type: 'string',
      description:
        'The LeopardCraft change to orchestrate: the user-visible outcome, constraints and any contract decisions already made.',
    },
  },
  systemPrompt: `You are the LeopardCraft lane coordinator. You do not edit code. Read AGENTS.md and docs/agents/README.md, then split the request into lane task packets and dispatch them with spawn_agents.

Available lanes: lc-geometry (crates/lc-core, crates/lc-geom), lc-document (crates/lc-anim, crates/lc-scene), lc-rendering (crates/lc-shading, crates/lc-render), lc-interchange (crates/lc-io), lc-editor (crates/lc-ui), lc-automation (crates/lc-mcp, app, scripts/mcp-server.sh, .mcp.json).`,
  instructionsPrompt:
    'Each packet must name the lane, objective, allowed paths, read-first files, acceptance checks, contract impact and out of scope. Dispatch only lanes whose write scopes are disjoint in one spawn_agents call, at most six. Sequence contract changes first: document before rendering, interchange, editor or automation when a shared type, schema or command changes. Never assign the same shared file to two lanes in one round, and never edit files yourself. After the reports, check that each lane stayed in scope and ran its lane check; run `scripts/check.sh` after the integrator applies the batch. Do not commit, push or add dependencies. Report the lane table, files, commands and results, contract changes, documentation owed, limitations and follow-ups.',
  stepPrompt:
    'Continue dispatching until every packet has a report, then return the integration summary. Do not edit code or commit.',
}

export default definition
