/**
 * Observation model — ADAPTED from pi-coding-agent.
 *
 * pi's "context snapshot" is surfaced by two RPC commands:
 *
 *   { type: "get_state" }
 *     → { model, thinkingLevel, isStreaming, isCompacting,
 *         steeringMode, followUpMode, sessionFile, sessionId,
 *         sessionName?, autoCompactionEnabled, messageCount,
 *         pendingMessageCount }
 *
 *   { type: "get_session_stats" }
 *     → { sessionFile, sessionId, userMessages, assistantMessages,
 *         toolCalls, toolResults, totalMessages,
 *         tokens: { input, output, cacheRead, cacheWrite, total },
 *         cost, contextUsage?: { tokens, contextWindow, percent } }
 *
 * pi does NOT try to read the user's screen — it is a coding agent, so its
 * world is the working directory, the session log, and whatever tool
 * outputs (bash/read/grep/…) it accumulates into the conversation context.
 *
 * The observer here is a thin adapter: `snapshot()` asks the bridge for
 * `get_state` + `get_session_stats`, merges the useful fields, and returns
 * an `Observation` that the TezBar HUD (or a future planner) can read
 * without caring about the wire format.
 *
 * Callers pass in a `query(command)` function instead of depending on the
 * full bridge — keeps this file unit-testable with a fake transport.
 */

export interface Observation {
  /** Agent cwd — pi tools inherit this. */
  cwd: string
  /** Current pi session path or undefined if ephemeral (`--no-session`). */
  sessionFile: string | undefined
  model: string | undefined
  thinkingLevel: string | undefined
  isStreaming: boolean
  messageCount: number
  pendingMessageCount: number
  toolCalls: number
  contextUsage:
    | { tokens: number | null; contextWindow: number; percent: number | null }
    | undefined
}

type JsonValue = string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue }

export interface SnapshotQueryFn {
  (command: { type: string; [k: string]: unknown }): Promise<JsonValue>
}

interface StateResponse {
  model?: { id?: string; provider?: string } | null
  thinkingLevel?: string
  isStreaming?: boolean
  messageCount?: number
  pendingMessageCount?: number
  sessionFile?: string | null
}

interface StatsResponse {
  toolCalls?: number
  sessionFile?: string | null
  contextUsage?: {
    tokens: number | null
    contextWindow: number
    percent: number | null
  }
}

function asRecord(value: JsonValue): Record<string, JsonValue> | undefined {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return undefined
  return value as Record<string, JsonValue>
}

function modelLabel(state: StateResponse): string | undefined {
  const m = state.model
  if (!m) return undefined
  if (m.provider && m.id) return `${m.provider}/${m.id}`
  return m.id
}

export async function observe(
  cwd: string,
  query: SnapshotQueryFn,
): Promise<Observation> {
  // pi returns each as { data: … }. We only read fields we need; unknown
  // ones are ignored so a pi version bump cannot break the HUD.
  const [stateRaw, statsRaw] = await Promise.all([
    query({ type: 'get_state' }).catch(() => null),
    query({ type: 'get_session_stats' }).catch(() => null),
  ])

  const state = (asRecord(stateRaw as JsonValue)?.data as unknown as StateResponse) ?? {}
  const stats = (asRecord(statsRaw as JsonValue)?.data as unknown as StatsResponse) ?? {}

  return {
    cwd,
    sessionFile: state.sessionFile ?? stats.sessionFile ?? undefined,
    model: modelLabel(state),
    thinkingLevel: state.thinkingLevel,
    isStreaming: state.isStreaming === true,
    messageCount: state.messageCount ?? 0,
    pendingMessageCount: state.pendingMessageCount ?? 0,
    toolCalls: stats.toolCalls ?? 0,
    contextUsage: stats.contextUsage,
  }
}
