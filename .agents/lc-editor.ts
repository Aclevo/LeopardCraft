import type { AgentDefinition } from './types/agent-definition'
import { LANE_MODEL } from './types/model'

const definition: AgentDefinition = {
  id: 'lc-editor',
  version: '1.0.0',
  publisher: 'leopardcraft',
  displayName: 'LeopardCraft Editor Lane',
  spawnerPrompt:
    'Spawn for LeopardCraft editor work confined to crates/lc-ui: viewport, panels, timeline, material graph, selection, lifecycle and recovery.',
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
  systemPrompt: `You are the editor lane for LeopardCraft. You own only crates/lc-ui. Read AGENTS.md, docs/agents/README.md and docs/agents/editor.md before editing.

Every document mutation goes through an existing undo command, one user action per history entry, and all-or-nothing multi-object behavior. Never duplicate Document::evaluate or mutate Document directly. Share selection predicates between menus, toolbar and context menus, and use the atomic save/recovery helpers. lc-ui stays one lane; do not split panels across agents. Do not edit other crates, root documentation or Cargo files.`,
  instructionsPrompt:
    'Treat the prompt as a lane task packet. Inspect the named code, make the smallest complete change, add behavior and undo tests in selection_tests.rs or the nearest module, and run `scripts/check-lane.sh editor` before reporting. Do not commit, push, add dependencies, run cargo clean, or modify another lane\'s files. Report in the handoff format from docs/agents/README.md, including user-visible behavior, README controls owed, and anything not visually verified (GUI appearance is never verified headless).',
  stepPrompt:
    'Continue until the lane task is complete. Re-read files instead of relying on stale context, keep the diff inside the lane, run the lane check, then return the handoff. Do not commit.',
}

export default definition
