/**
 * Chat sessions — the persistent layer behind the AI-mode HUD in the
 * command bar. One session groups turns (user + assistant) that the user
 * experienced as a single conversation. A new submission within
 * `CHAT_CONTINUATION_WINDOW_MS` of the previous turn in the same session
 * reuses it; otherwise the renderer starts a fresh session.
 *
 * Sessions are owned by the main process (sqlite-backed in
 * `src/main/chat/sessionStore.ts`) and surfaced to the renderer via the
 * `chat:*` IPC channels below.
 */

import type { AgentInputImage, AgentTimelineItem, Stage } from './agent'

export type ChatRole = 'user' | 'assistant'

export interface ChatAttachment {
  kind: 'image'
  name: string
  mimeType: AgentInputImage['mimeType']
  /** Present only in the live renderer session; history stores metadata. */
  data?: string
  width?: number
  height?: number
}

export interface ChatResponseMeta {
  provider: string
  providerTitle: string
  model: string
  /** Estimated output token count when provider usage is not available. */
  tokenCount?: number
}

export interface ChatTurn {
  id: string
  role: ChatRole
  text: string
  /** Provider/model metadata for assistant turns. */
  responseMeta?: ChatResponseMeta
  /** Stages captured while this assistant turn ran, for replay in history. */
  stages?: Stage[]
  /** Text and tool stages in the order the user experienced them. */
  timeline?: AgentTimelineItem[]
  /** Optional error string if the assistant turn failed. */
  error?: string
  attachments?: ChatAttachment[]
  createdAt: number
}

export interface ChatSession {
  id: string
  title: string
  createdAt: number
  updatedAt: number
  /** Directory where Pi starts for every agent turn in this conversation. */
  workingDirectory?: string
  turns: ChatTurn[]
}

/** Lightweight summary row used by list views (history dropdown). */
export interface ChatSessionSummary {
  id: string
  title: string
  createdAt: number
  updatedAt: number
  turnCount: number
  workingDirectory?: string
  /** Preview of the most recent user prompt, for the history dropdown. */
  preview: string
}

/**
 * How long after the *previous* turn the next user submission still counts
 * as a continuation. 30 seconds per product requirements — a user who
 * pauses longer than that probably moved on to a new task.
 */
export const CHAT_CONTINUATION_WINDOW_MS = 30_000

/** Hard cap on how many turns we pack into the context prompt. Older
 *  turns are still kept in storage; we just stop sending them to the
 *  chat provider so the prompt does not balloon beyond what the model
 *  can handle. */
export const CHAT_CONTEXT_MAX_TURNS = 16

export const CHAT_IPC = {
  RUN: 'chat:run',
  LIST: 'chat:list',
  GET: 'chat:get',
  APPEND: 'chat:append',
  UPDATE_TITLE: 'chat:update-title',
  DELETE_TURN: 'chat:delete-turn',
  DELETE: 'chat:delete',
  CLEAR: 'chat:clear',
} as const
