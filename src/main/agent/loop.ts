/**
 * Agent loop — MIRRORS pi-coding-agent's turn structure.
 *
 * pi does NOT expose a plan → reflect → repeat meta-loop. Instead, the LLM
 * itself plans inline (via thinking blocks and tool-use), and pi's runtime
 * drives the tool calls. One pi "run" looks like:
 *
 *     agent_start                         ← pi begins processing the prompt
 *       turn_start
 *         message_start (assistant)
 *           message_update (text_delta | thinking_delta | toolcall_delta)
 *         message_end
 *         tool_execution_start            ← one per tool call
 *           tool_execution_update         ← streamed partial output
 *         tool_execution_end              ← per-call result
 *       turn_end                          ← includes tool results
 *       (repeat turn_start/… until no more tool calls)
 *     agent_end                           ← pi stops emitting messages
 *
 * The TezBar HUD only needs "stage-level" granularity:
 *
 *   • each pi `tool_execution_start` becomes a new Stage{status:"running"}
 *   • the matching `tool_execution_end` flips it to "done" or "failed"
 *     depending on `isError`
 *   • we keep a final "answered" stage that flips to done when pi emits
 *     its last assistant text (no further tool_execution_start events)
 *
 * The driver is a PURE mapper from pi events → stage updates + message
 * deltas. It does not own the subprocess — bridge.ts does — so it stays
 * testable with a hand-rolled event iterator.
 *
 * This file is the READ-ME for the loop structure and the actual mapper.
 */

import type { Stage, StageStatus } from '../../shared/agent'
import { labelForToolCall } from './tools'

/** Pi's RPC events are JSON objects keyed by `type`. We accept anything
 *  shaped like `{ type: string, ... }` for forward-compat with future pi
 *  releases, and narrow per-case inside `createLoopDriver`. */
export type PiEvent = { type: string } & Record<string, unknown>

export interface LoopCallbacks {
  onStage: (stage: Stage) => void
  onMessageDelta: (delta: string) => void
  onAnswer: (text: string) => void
  onDone: () => void
  onError: (message: string) => void
}

interface StageTracker {
  stages: Map<string /* toolCallId */, Stage>
  nextIndex: number
  /** Accumulated text for the most recent assistant message — flushed on
   *  message_update text_end or message_end. */
  currentText: string
  /** Set to true on agent_end so we can coalesce a final "answered" stage. */
  ended: boolean
}

function errorDetail(result: unknown): string | undefined {
  // pi returns a `result.content` array; the first text block usually
  // carries the human error summary. Everything else is noise for the HUD.
  if (!result || typeof result !== 'object') return undefined
  const content = (result as { content?: unknown }).content
  if (!Array.isArray(content)) return undefined
  for (const item of content) {
    if (item && typeof item === 'object' && (item as { type?: unknown }).type === 'text') {
      const text = (item as { text?: unknown }).text
      if (typeof text === 'string' && text.trim()) {
        // Collapse multi-line errors to one line for the HUD.
        return text.replace(/\s+/g, ' ').trim().slice(0, 160)
      }
    }
  }
  return undefined
}

export function createLoopDriver(callbacks: LoopCallbacks): (event: PiEvent) => void {
  const tracker: StageTracker = {
    stages: new Map(),
    nextIndex: 0,
    currentText: '',
    ended: false,
  }

  const emitStage = (stage: Stage): void => {
    tracker.stages.set(`stage:${stage.index}`, stage)
    callbacks.onStage(stage)
  }

  const updateStageStatus = (
    toolCallId: string,
    status: StageStatus,
    detail?: string,
  ): void => {
    const existing = tracker.stages.get(toolCallId)
    if (!existing) return
    const next: Stage = detail ? { ...existing, status, detail } : { ...existing, status }
    tracker.stages.set(toolCallId, next)
    callbacks.onStage(next)
  }

  const asString = (v: unknown, fallback = ''): string =>
    typeof v === 'string' ? v : fallback
  const asBool = (v: unknown): boolean => v === true
  const asNumber = (v: unknown, fallback = 0): number =>
    typeof v === 'number' && Number.isFinite(v) ? v : fallback
  const asRecord = (v: unknown): Record<string, unknown> | undefined =>
    v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : undefined

  return function handle(event: PiEvent): void {
    switch (event.type) {
      case 'agent_start': {
        tracker.stages.clear()
        tracker.nextIndex = 0
        tracker.currentText = ''
        tracker.ended = false
        return
      }

      case 'message_update': {
        const ev = asRecord(event['assistantMessageEvent'])
        if (!ev) return
        const subType = asString(ev['type'])
        const delta = ev['delta']
        const content = ev['content']
        if (subType === 'text_delta' && typeof delta === 'string') {
          tracker.currentText += delta
          callbacks.onMessageDelta(delta)
        } else if (subType === 'text_end' && typeof content === 'string') {
          tracker.currentText = content
        }
        return
      }

      case 'tool_execution_start': {
        const toolCallId = asString(event['toolCallId'])
        if (!toolCallId) return
        const index = tracker.nextIndex++
        const stage: Stage = {
          index,
          label: labelForToolCall(asString(event['toolName'], 'tool'), event['args']),
          status: 'running',
        }
        tracker.stages.set(toolCallId, stage)
        emitStage(stage)
        return
      }

      case 'tool_execution_end': {
        const toolCallId = asString(event['toolCallId'])
        if (!toolCallId) return
        const isError = asBool(event['isError'])
        updateStageStatus(
          toolCallId,
          isError ? 'failed' : 'done',
          isError ? errorDetail(event['result']) : undefined,
        )
        return
      }

      case 'auto_retry_start': {
        const attempt = asNumber(event['attempt'])
        const maxAttempts = asNumber(event['maxAttempts'])
        const stage: Stage = {
          index: tracker.nextIndex++,
          label: `retry (${attempt}/${maxAttempts})`,
          status: 'running',
          detail: asString(event['errorMessage']).slice(0, 160) || undefined,
        }
        tracker.stages.set(`retry:${attempt}`, stage)
        emitStage(stage)
        return
      }

      case 'auto_retry_end': {
        const retryKey = Array.from(tracker.stages.keys())
          .reverse()
          .find((k) => k.startsWith('retry:'))
        if (retryKey) {
          updateStageStatus(
            retryKey,
            asBool(event['success']) ? 'done' : 'failed',
            asString(event['finalError']) || undefined,
          )
        }
        return
      }

      case 'agent_end': {
        tracker.ended = true
        if (tracker.currentText.trim()) {
          callbacks.onAnswer(tracker.currentText.trim())
        }
        callbacks.onDone()
        return
      }

      default:
        return
    }
  }
}
