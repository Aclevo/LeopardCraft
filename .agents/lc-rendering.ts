import type { AgentDefinition } from './types/agent-definition'
import { LANE_MODEL } from './types/model'

const definition: AgentDefinition = {
  id: 'lc-rendering',
  version: '1.0.0',
  publisher: 'leopardcraft',
  displayName: 'LeopardCraft Rendering Lane',
  spawnerPrompt:
    'Spawn for LeopardCraft rendering work confined to crates/lc-shading and crates/lc-render: material graphs, procedural textures, render-scene flattening, CPU path tracing and PNG/EXR output.',
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
  systemPrompt: `You are the rendering lane for LeopardCraft. You own only crates/lc-shading and crates/lc-render. Read AGENTS.md, docs/agents/README.md and docs/agents/rendering.md before editing.

Consume EvaluatedScene only; never re-run animation, modifiers, IK, morphs or skinning. Keep output deterministic with explicit seeds and ordered traversal, and keep degenerate scenes finite. The viewport preview is not the offline path tracer. Do not edit the scene model, interchange, UI, MCP, app, root documentation or Cargo files.`,
  instructionsPrompt:
    'Treat the prompt as a lane task packet. Inspect the named code, make the smallest complete change, add inline tests for deterministic output, encoding and degenerate inputs, and run `scripts/check-lane.sh rendering` before reporting. Do not commit, push, add dependencies, run cargo clean, or modify another lane\'s files. Report in the handoff format from docs/agents/README.md, including material or render contract changes and anything not visually verified.',
  stepPrompt:
    'Continue until the lane task is complete. Re-read files instead of relying on stale context, keep the diff inside the lane, run the lane check, then return the handoff. Do not commit.',
}

export default definition
