/** Shared native agent events and renderer tool stages. */

export type StageStatus = 'running' | 'done' | 'failed'

export interface Stage {
  /** Monotonic index within the current agent run. */
  index: number
  /** Human-readable label (e.g. `bash: git status`). */
  label: string
  status: StageStatus
  /** Optional one-line detail (command stderr, error message, …). */
  detail?: string
  /** Bounded plain-text tool output, never image/base64 payloads. */
  output?: string
  outputTruncated?: boolean
  toolName?: string
  durationMs?: number
}

export type AgentTimelineItem = { type: 'text'; text: string } | { type: 'stage'; stage: Stage }

export interface AgentInputImage {
  type: 'image'
  /** Raw base64 bytes. Data-URL prefixes are accepted and stripped before the request. */
  data: string
  mimeType: 'image/png' | 'image/jpeg' | 'image/webp'
  width?: number
  height?: number
}

import type { AgentApprovalMode } from './llmConfig'
export type { AgentApprovalMode }

export interface AgentRunRequest {
  /** Client-generated correlation id; lets the UI subscribe before submitting. */
  runId?: string
  task: string
  sessionId?: string
  userTurnId?: string
  assistantTurnId?: string
  retry?: boolean
  priorTurnIds?: string[]
  priorTurns?: {
    id: string
    role: 'user' | 'assistant'
    text: string
    error?: string
    hasImages?: boolean
  }[]
  instructions?: string
  agentProfileId?: string
  images?: AgentInputImage[]
  /** Allows text-only runtimes to reject history whose images cannot be represented. */
  historyHasImages?: boolean
  /** Existing working directory for agent tools. */
  cwd?: string
  /** Permission mode for command execution. */
  approvalMode?: AgentApprovalMode
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
      /** False when this runtime only supports one-time approval. */
      canRemember?: boolean
      rememberKind?: 'tool' | 'command'
    }
  | { type: 'approval-expired'; runId: string; approvalId: string }
  | { type: 'done'; runId: string }
  | { type: 'error'; runId: string; message: string }
  /** A bounded, source-labelled native diagnostic. */
  | { type: 'log'; runId: string; source: 'stderr' | 'native'; line: string }

/** IPC channel names. Shared so main + preload + renderer cannot drift. */
export const AGENT_IPC = {
  RUN: 'agent:run',
  CANCEL: 'agent:cancel',
  EVENT: 'agent:event',
  APPROVE: 'agent:approve',
  CAPTURE_ACTIVE_SCREEN: 'agent:capture-active-screen',
} as const
