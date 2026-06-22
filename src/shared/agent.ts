/**
 * Shared agent types exchanged between the main process (which speaks to
 * the pi coding agent over JSONL RPC) and the renderer HUD.
 *
 * The Stage shape is the one the acceptance spec pins down:
 *   { index, label, status: "running" | "done" | "failed" }
 *
 * Stages are derived by `src/main/agent/loop.ts` from pi's
 * `tool_execution_start` / `tool_execution_end` events (one stage per tool
 * call), plus a final "answered" stage when the agent emits its last
 * assistant message.
 */

export type StageStatus = 'running' | 'done' | 'failed'

export interface Stage {
  /** Monotonic index within the current agent run. */
  index: number
  /** Human-readable label (e.g. `bash: git status`). */
  label: string
  status: StageStatus
  /** Optional one-line detail (command stderr, error message, …). */
  detail?: string
}

export interface AgentInputImage {
  type: 'image'
  /** Raw base64 bytes. Data-URL prefixes are accepted and stripped before RPC. */
  data: string
  mimeType: 'image/png' | 'image/jpeg' | 'image/webp'
  width?: number
  height?: number
}

export interface AgentRunRequest {
  task: string
  images?: AgentInputImage[]
}

export type AgentApprovalDecision = 'deny' | 'once' | 'always'

export interface AgentApprovalResponse {
  runId: string
  approvalId: string
  decision: AgentApprovalDecision
}

export type AgentRunEvent =
  | { type: 'start'; runId: string; task: string }
  | { type: 'stage'; runId: string; stage: Stage }
  | { type: 'message'; runId: string; delta: string }
  | { type: 'answer'; runId: string; text: string }
  | {
      type: 'approval'
      runId: string
      approvalId: string
      title: string
      command: string
      suggestedRule?: string
    }
  | { type: 'done'; runId: string }
  | { type: 'error'; runId: string; message: string }
  /** One line from the pi subprocess stderr (config, auth, model errors, …). */
  | { type: 'log'; runId: string; source: 'stderr'; line: string }

/** IPC channel names. Shared so main + preload + renderer cannot drift. */
export const AGENT_IPC = {
  RUN: 'agent:run',
  CANCEL: 'agent:cancel',
  EVENT: 'agent:event',
  APPROVE: 'agent:approve',
  CAPTURE_ACTIVE_SCREEN: 'agent:capture-active-screen',
} as const
