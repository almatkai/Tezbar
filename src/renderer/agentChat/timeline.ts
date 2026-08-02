import type { AgentTimelineItem, Stage } from '../../shared/agent'

/**
 * Timeline mutations for the live agent stream. The renderer receives text
 * deltas and stage (tool-call) updates over IPC and keeps them in a single
 * ordered list so the user sees tool calls exactly where they happened
 * relative to the assistant's prose.
 */

export function appendTimelineText(items: AgentTimelineItem[], delta: string): AgentTimelineItem[] {
  if (!delta) return items
  const next = items.slice()
  const last = next[next.length - 1]
  if (last?.type === 'text') {
    next[next.length - 1] = { type: 'text', text: last.text + delta }
  } else {
    next.push({ type: 'text', text: delta })
  }
  return next
}

/**
 * Inserts a stage at the current end of the timeline. Updates to a stage
 * that already exists (status flips, detail lines) are applied in place so
 * the original ordering — which mirrors the real execution order — is kept:
 * a tool call that ran *before* a paragraph never jumps back above it.
 */
export function upsertTimelineStage(
  items: AgentTimelineItem[],
  stage: Stage
): AgentTimelineItem[] {
  const next = items.slice()
  const index = next.findIndex((item) => item.type === 'stage' && item.stage.index === stage.index)
  if (index >= 0) {
    next[index] = { type: 'stage', stage }
  } else {
    next.push({ type: 'stage', stage })
  }
  return next
}
