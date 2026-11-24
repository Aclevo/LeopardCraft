/**
 * Model used by every LeopardCraft lane agent.
 *
 * Local `.agents` definitions must name a model and do not inherit the
 * session's model. Set LEOPARDCRAFT_LANE_MODEL before launching the Codebuff
 * CLI to a model your Codebuff account can run; the default below is Freebuff's
 * Flash wire id, which is not an entitlement these agents can carry.
 */
const env = (
  globalThis as {
    process?: { env?: Record<string, string | undefined> }
  }
).process?.env

export const LANE_MODEL =
  env?.LEOPARDCRAFT_LANE_MODEL || 'deepseek/deepseek-v4-flash'
