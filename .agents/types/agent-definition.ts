/**
 * Minimal local mirror of the Codebuff/Freebuff `AgentDefinition` shape.
 *
 * The runtime loads `.agents/*.ts` with Bun and only needs the object returned
 * as the default export; this file exists for editor type safety. See
 * https://www.codebuff.com/docs/agents/agent-reference for the full upstream
 * type (reasoning, provider routing, MCP servers, handleSteps and so on).
 */

export type ModelName = string

export interface AgentDefinition {
  /** Lowercase letters, numbers and hyphens only. */
  id: string
  version?: string
  publisher?: string
  displayName: string
  /** OpenRouter-style model id. Local agents do not inherit the session model. */
  model: ModelName
  outputMode?: 'last_message' | 'all_messages' | 'structured_output'
  includeMessageHistory?: boolean
  toolNames?: string[]
  /** Local agent ids or fully qualified `publisher/agent@version` ids. */
  spawnableAgents?: string[]
  inputSchema?: {
    prompt?: { type: 'string'; description?: string }
    params?: Record<string, unknown>
  }
  systemPrompt?: string
  instructionsPrompt?: string
  stepPrompt?: string
}
