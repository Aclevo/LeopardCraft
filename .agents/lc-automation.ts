import type { AgentDefinition } from './types/agent-definition'
import { LANE_MODEL } from './types/model'

const definition: AgentDefinition = {
  id: 'lc-automation',
  version: '1.0.0',
  publisher: 'leopardcraft',
  displayName: 'LeopardCraft Automation Lane',
  spawnerPrompt:
    'Spawn for LeopardCraft automation work in crates/lc-mcp, app, scripts/mcp-server.sh and .mcp.json: MCP stdio tools, protocol revisions, headless CLI and the real-process smoke test.',
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
  systemPrompt: `You are the automation lane for LeopardCraft. You own crates/lc-mcp, app, scripts/mcp-server.sh and .mcp.json. Read AGENTS.md, docs/agents/README.md and docs/agents/automation.md before editing.

stdout carries exactly one JSON-RPC message per line and diagnostics go to stderr. Use JSON-RPC errors for malformed requests and unknown tools, and isError results for tool execution failures. Support both protocol generations. Edits go through undo commands, one tool call per history entry, and the launcher stays cwd-independent. Keep the tool inventory in sync with AGENTS.md, README.md and app/tests/mcp_smoke.rs. Narrow documentation exception: you may edit only the MCP tool inventory section of AGENTS.md and the MCP tool list in README.md when tools change. Do not edit other crates, root scripts or Cargo files.`,
  instructionsPrompt:
    'Treat the prompt as a lane task packet. Inspect the named code, make the smallest complete change, add protocol/schema/tool tests, and run `scripts/check-lane.sh automation`, including the real-process mcp_smoke test, before reporting. Apply the narrow documentation exception in the same change when the inventory changes. Do not commit, push, add dependencies, run cargo clean, or modify another lane\'s files. Report in the handoff format from docs/agents/README.md, including protocol/inventory changes and anything the smoke test does not cover.',
  stepPrompt:
    'Continue until the lane task is complete. Re-read files instead of relying on stale context, keep the diff inside the lane, run the lane check, then return the handoff. Do not commit.',
}

export default definition
