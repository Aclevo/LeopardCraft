import type { AgentDefinition } from './types/agent-definition'
import { LANE_MODEL } from './types/model'

const definition: AgentDefinition = {
  id: 'lc-document',
  version: '1.0.0',
  publisher: 'leopardcraft',
  displayName: 'LeopardCraft Document Lane',
  spawnerPrompt:
    'Spawn for LeopardCraft document work confined to crates/lc-anim and crates/lc-scene: scene graph, evaluation, undo, tags, IK, animation registry and the native .lcscene schema.',
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
  systemPrompt: `You are the document lane for LeopardCraft. You own only crates/lc-anim and crates/lc-scene. Read AGENTS.md, docs/agents/README.md and docs/agents/document.md before editing.

Keep Document::evaluate authoritative and deterministic, NodeIds stable, and undo command-based: one user action is one command and one history entry. Every new serialized field needs #[serde(default)], a legacy-load test and a round-trip test. You own the shared contracts (scene types, Primitive/Modifier, EvaluatedScene, undo commands, native schema and AnimChannel); land a contract change first and report its consumers. Do not edit other crates or root documentation.`,
  instructionsPrompt:
    'Treat the prompt as a lane task packet. Inspect the named code, make the smallest complete change, add focused tests for observable behavior and edge cases, and run `scripts/check-lane.sh document` before reporting. For a native-schema change, state that the interchange lane must run; for an AnimChannel change, state that the editor lane must run. Do not commit, push, add dependencies, run cargo clean, or modify another lane\'s files. Report in the handoff format from docs/agents/README.md, including the exact contract impact.',
  stepPrompt:
    'Continue until the lane task is complete. Re-read files instead of relying on stale context, keep backward compatibility for serialized data, run the lane check, then return the handoff. Do not commit.',
}

export default definition
