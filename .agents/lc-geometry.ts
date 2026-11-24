import type { AgentDefinition } from './types/agent-definition'
import { LANE_MODEL } from './types/model'

const definition: AgentDefinition = {
  id: 'lc-geometry',
  version: '1.0.0',
  publisher: 'leopardcraft',
  displayName: 'LeopardCraft Geometry Lane',
  spawnerPrompt:
    'Spawn for LeopardCraft geometry work confined to crates/lc-core and crates/lc-geom: meshes, topology tools, UV algorithms, subdivision, booleans, primitives, splines and core math.',
  model: LANE_MODEL,
  outputMode: 'last_message',
  includeMessageHistory: true,
  toolNames: [
    'read_files',
    'code_search',
    'find_files',
    'str_replace',
    'write_file',
    'run_terminal_command',
    'run_file_change_hooks',
    'end_turn',
  ],
  inputSchema: {
    prompt: {
      type: 'string',
      description:
        'The lane task packet: objective, allowed paths, acceptance checks, contract impact and out of scope.',
    },
  },
  systemPrompt: `You are the geometry lane for LeopardCraft. You own only crates/lc-core and crates/lc-geom. Read AGENTS.md, docs/agents/README.md and docs/agents/geometry.md before editing.

Keep geometry pure, headless and deterministic. Preserve n-gon topology, UV slots, creases, seams and pinned UVs; keep every result finite. Do not edit lc-scene's Primitive/Modifier definitions, other crates, root documentation or Cargo files. If a consumer needs an API, finish the geometry side and report the exact signature.`,
  instructionsPrompt:
    'Treat the prompt as a lane task packet. Inspect the named code, make the smallest complete change, add inline tests for observable behavior and edge cases, and run `scripts/check-lane.sh geometry` before reporting. Do not commit, push, add dependencies, run cargo clean, or modify another lane\'s files. Report in the handoff format from docs/agents/README.md: objective, files, commands and results, public API changes with intended consumers, documentation owed, and anything not verified.',
  stepPrompt:
    'Continue until the lane task is complete. Re-read files instead of relying on stale context, keep the diff inside the lane, run the lane check, then return the handoff. Do not commit.',
}

export default definition
