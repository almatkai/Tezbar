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

import type { Stage } from './agent'

export type ChatRole = 'user' | 'assistant'

export interface ChatTurn {
  id: string
  role: ChatRole
  text: string
  /** Stages captured while this assistant turn ran, for replay in history. */
  stages?: Stage[]
  /** Optional error string if the assistant turn failed. */
  error?: string
  createdAt: number
}

export interface ChatSession {
  id: string
  title: string
  createdAt: number
  updatedAt: number
  turns: ChatTurn[]
}

/** Lightweight summary row used by list views (history dropdown). */
export interface ChatSessionSummary {
  id: string
  title: string
  createdAt: number
  updatedAt: number
  turnCount: number
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
  DELETE: 'chat:delete',
  CLEAR: 'chat:clear',
} as const
