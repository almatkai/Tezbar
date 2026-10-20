/** Pure Pi RPC → Tezbar event mapping.
 *
 * Pi 1.x ends low-level runs with agent_end, but can still retry or run queued
 * work afterwards. Only agent_settled is terminal. Older event streams without
 * the willRetry field retain their agent_end completion behavior.
 * message_end is authoritative; deltas are only a live preview.
 */
import type { Stage } from '../../shared/agent'
import { labelForToolCall } from './tools'

export type PiEvent = { type: string } & Record<string, unknown>
export interface LoopCallbacks {
  onStage: (stage: Stage) => void
  onMessageDelta: (delta: string) => void
  onAnswer: (text: string) => void
  onDone: () => void
  onError: (message: string) => void
}

const MAX_TOOL_OUTPUT_CHARS = 8_000
function record(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined
}
function text(value: unknown): string {
  return typeof value === 'string' ? value : ''
}
function contentText(value: unknown): string {
  if (!Array.isArray(value)) return ''
  return value
    .flatMap((item) => {
      const block = record(item)
      return block?.type === 'text' && typeof block.text === 'string' ? [block.text] : []
    })
    .join('\n\n')
}
function toolOutput(value: unknown): Pick<Stage, 'output' | 'outputTruncated'> {
  const output = contentText(record(value)?.content).replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '')
  return output
    ? {
        output: output.slice(0, MAX_TOOL_OUTPUT_CHARS),
        outputTruncated: output.length > MAX_TOOL_OUTPUT_CHARS,
      }
    : {}
}

export function createLoopDriver(callbacks: LoopCallbacks): (event: PiEvent) => void {
  const stages = new Map<string, Stage>()
  const blocks = new Map<number, string>()
  const previousText: string[] = []
  let nextIndex = 0
  let currentText = ''
  let currentFailed = false
  let needsSeparator = false
  let modernLifecycle = false
  let finished = false
  let needsFinalAnswer = false
  let lastError: string | undefined

  const update = (key: string, patch: Partial<Stage>): void => {
    const stage = stages.get(key)
    if (!stage) return
    const next = { ...stage, ...patch }
    stages.set(key, next)
    callbacks.onStage(next)
  }
  const startStage = (key: string, label: string, extra: Partial<Stage> = {}): void => {
    const stage: Stage = { index: nextIndex++, label, status: 'running', ...extra }
    stages.set(key, stage)
    callbacks.onStage(stage)
  }
  const finishThinking = (): void => {
    if (stages.get('thinking')?.status === 'running') update('thinking', { status: 'done' })
  }
  const snapshot = (value: unknown): void => {
    const message = record(value)
    if (message?.role !== 'assistant') return
    const stopReason = text(message.stopReason)
    if (stopReason === 'toolUse') needsFinalAnswer = true
    else if (
      contentText(message.content).trim() &&
      stopReason !== 'error' &&
      stopReason !== 'aborted'
    )
      needsFinalAnswer = false
    if (stopReason === 'error' || stopReason === 'aborted' || message.errorMessage) {
      lastError =
        text(message.errorMessage) ||
        (stopReason === 'aborted'
          ? 'Agent run aborted'
          : 'The model provider encountered an error.')
      currentFailed = true
    } else if (stopReason && stopReason !== 'pending') {
      lastError = undefined
      currentFailed = false
    }
    if (Array.isArray(message.content)) {
      currentText = contentText(message.content)
      blocks.clear()
      message.content.forEach((value, index) => {
        const block = record(value)
        if (block?.type === 'text') blocks.set(index, text(block.text))
      })
    }
  }
  const finish = (): void => {
    if (finished) return
    finished = true
    finishThinking()
    const answer = [...previousText, currentFailed ? '' : currentText.trim()]
      .filter(Boolean)
      .join('\n\n')
    for (const [key, stage] of stages) {
      if (stage.status === 'running')
        update(key, {
          status: 'failed',
          detail: lastError || 'Run ended before this step finished.',
        })
    }
    if (answer) callbacks.onAnswer(answer)
    if (lastError) callbacks.onError(lastError)
    else if (!answer || needsFinalAnswer) callbacks.onError('Agent finished without a response.')
    else callbacks.onDone()
  }

  return (event): void => {
    if (finished) return
    switch (event.type) {
      case 'agent_start':
        // A new low-level run can be an automatic retry. Do not erase progress.
        return
      case 'turn_start':
        if (currentText.trim() && !currentFailed) previousText.push(currentText.trim())
        currentText = ''
        blocks.clear()
        currentFailed = false
        needsSeparator = previousText.length > 0
        return
      case 'message_start':
        return
      case 'message_end':
      case 'turn_end':
        snapshot(event.message)
        return
      case 'message_update': {
        const updateEvent = record(event.assistantMessageEvent)
        if (!updateEvent) return
        if (updateEvent.type === 'thinking_delta') {
          if (stages.get('thinking')?.status !== 'running') startStage('thinking', 'Thinking…')
        } else if (updateEvent.type === 'thinking_end') {
          finishThinking()
        } else if (updateEvent.type === 'text_delta' || updateEvent.type === 'text_end') {
          finishThinking()
          const index = typeof updateEvent.contentIndex === 'number' ? updateEvent.contentIndex : 0
          if (updateEvent.type === 'text_delta') {
            const delta = text(updateEvent.delta)
            if (delta.trim()) needsFinalAnswer = false
            if (needsSeparator && delta) {
              callbacks.onMessageDelta('\n\n')
              needsSeparator = false
            }
            blocks.set(index, (blocks.get(index) ?? '') + delta)
            callbacks.onMessageDelta(delta)
          } else {
            blocks.set(index, text(updateEvent.content))
            if (text(updateEvent.content).trim()) needsFinalAnswer = false
          }
          currentText = [...blocks.entries()]
            .sort(([a], [b]) => a - b)
            .map(([, value]) => value)
            .join('\n\n')
        }
        return
      }
      case 'tool_execution_start': {
        needsFinalAnswer = true
        finishThinking()
        const id = text(event.toolCallId)
        if (id)
          startStage(id, labelForToolCall(text(event.toolName) || 'tool', event.args), {
            toolName: text(event.toolName),
          })
        return
      }
      case 'tool_execution_update':
        update(text(event.toolCallId), toolOutput(event.partialResult))
        return
      case 'tool_execution_end': {
        const output = toolOutput(event.result)
        const failed = event.isError === true
        update(text(event.toolCallId), {
          ...output,
          status: failed ? 'failed' : 'done',
          detail: failed
            ? (output.output || 'Tool failed').replace(/\s+/g, ' ').slice(0, 160)
            : undefined,
          durationMs:
            typeof event.durationMs === 'number' && event.durationMs >= 0
              ? event.durationMs
              : undefined,
        })
        return
      }
      case 'auto_retry_start':
        finishThinking()
        if (stages.get('retry')?.status === 'running')
          update('retry', { status: 'failed', detail: 'Attempt failed; trying again.' })
        startStage('retry', `Retrying (${event.attempt ?? 1}/${event.maxAttempts ?? 3})`, {
          detail: text(event.errorMessage).slice(0, 160) || undefined,
        })
        return
      case 'auto_retry_end':
        if (event.success === true) lastError = undefined
        else
          lastError =
            text(event.finalError) || lastError || 'The provider could not complete the request.'
        update('retry', {
          status: event.success === true ? 'done' : 'failed',
          detail: event.success === true ? undefined : lastError,
        })
        return
      case 'agent_end': {
        if (typeof event.willRetry === 'boolean') modernLifecycle = true
        const messages = Array.isArray(event.messages) ? event.messages : []
        const assistantMessages = messages.filter(
          (message) => record(message)?.role === 'assistant'
        )
        const last = assistantMessages.at(-1)
        if (last) snapshot(last)
        if (event.willRetry === true || modernLifecycle) return
        finish()
        return
      }
      case 'agent_settled':
        if (event.aborted === true) lastError = 'Agent run aborted'
        finish()
        return
    }
  }
}
