/**
 * Model used by every LeopardCraft lane agent.
 *
 * Local `.agents` definitions must name a model and do not inherit the
 * session's model. Set LEOPARDCRAFT_LANE_MODEL before launching the CLI to
 * match the model your Freebuff account and session can run; otherwise this
 * defaults to Freebuff's current free-tier default.
 */
const env = (
  globalThis as {
    process?: { env?: Record<string, string | undefined> }
  }
).process?.env

export const LANE_MODEL =
  env?.LEOPARDCRAFT_LANE_MODEL || 'deepseek/deepseek-v4-flash'
