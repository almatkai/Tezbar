import { Chat } from '@ai-sdk/react'
import { createChat } from '@shadcn/helpers/ai-sdk'
import { describe, expect, it, vi } from 'vitest'
import type { AgentRunEvent, AgentRunRequest } from '../../shared/agent'
import {
  agentMessageError,
  agentMessageStages,
  agentMessageText,
  createNativeAgentTransport,
  finalizeAgentMessage,
  type AgentNativeApi,
  type AgentUIMessage,
} from './transport'

const context = () => ({
  session: { id: 'session-a', title: 'CPU check', createdAt: 1, updatedAt: 1, turns: [] },
  responseMeta: {
    provider: 'antigravity' as const,
    providerTitle: 'Google',
    model: 'gemini-3.8-flash',
  },
})
function harness(
  run: (
    request: AgentRunRequest,
    emit: (event: AgentRunEvent) => void
  ) => Promise<{ ok: boolean; runId?: string; error?: string }>
) {
  const listeners = new Set<(event: AgentRunEvent) => void>()
  const emit = (event: AgentRunEvent) => {
    for (const listener of listeners) listener(event)
  }
  const api: AgentNativeApi = {
    agentRun: vi.fn((request) =>
      run(typeof request === 'string' ? { task: request } : request, emit)
    ),
    agentCancel: vi.fn(async () => ({ ok: true })),
    onAgentEvent: (listener) => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
  }
  return {
    api,
    emit,
    listeners,
    chat: new Chat<AgentUIMessage>({ transport: createNativeAgentTransport(api, context) }),
  }
}

describe('AI SDK native agent transport', () => {
  it('handles events before the IPC response and keeps canonical text and successful tool output', async () => {
    const h = harness(async ({ runId, task }, emit) => {
      expect(task).toContain('почему компьютер горячий')
      emit({ type: 'start', runId: runId!, task })
      emit({
        type: 'stage',
        runId: runId!,
        stage: { index: 0, label: 'bash: ps', status: 'running' },
      })
      emit({ type: 'message', runId: runId!, delta: 'CPU' })
      emit({
        type: 'stage',
        runId: runId!,
        stage: { index: 0, label: 'bash: ps', status: 'done', output: '42  12%  Browser' },
      })
      emit({ type: 'answer', runId: runId!, text: 'CPU: 12%. Browser is the busiest process.' })
      emit({ type: 'done', runId: runId! })
      return { ok: true, runId }
    })
    await h.chat.sendMessage({ text: 'почему компьютер горячий' })
    expect(h.chat.status).toBe('ready')
    expect(agentMessageText(h.chat.lastMessage!)).toBe('CPU: 12%. Browser is the busiest process.')
    expect(agentMessageStages(h.chat.lastMessage!)).toEqual([
      { index: 0, label: 'bash: ps', status: 'done', output: '42  12%  Browser' },
    ])
    expect(h.listeners.size).toBe(0)
  })

  it('makes start failures visible and leaves a retryable error instead of a blank running state', async () => {
    const h = harness(async () => ({ ok: false, error: 'Provider disconnected' }))
    await h.chat.sendMessage({ text: 'Inspect CPU' })
    expect(h.chat.status).toBe('error')
    expect(h.chat.error?.message).toBe('Provider disconnected')
    expect(agentMessageError(h.chat.lastMessage!)).toBe('Provider disconnected')
    expect(h.listeners.size).toBe(0)
  })

  it('preserves partial output on stop, cancels only its own run, and ignores late events', async () => {
    let firstRun = ''
    let calls = 0
    const h = harness(async ({ runId, task }, emit) => {
      calls++
      emit({ type: 'start', runId: runId!, task })
      emit({
        type: 'message',
        runId: runId!,
        delta: calls === 1 ? 'Checking CPU...' : 'New result.',
      })
      if (calls === 1) firstRun = runId!
      else emit({ type: 'done', runId: runId! })
      return { ok: true, runId }
    })
    const sending = h.chat.sendMessage({ text: 'Inspect CPU' })
    await vi.waitFor(() => expect(h.chat.status).toBe('streaming'))
    await h.chat.stop()
    await sending
    expect(agentMessageText(h.chat.lastMessage!)).toBe('Checking CPU...')
    expect(h.api.agentCancel).toHaveBeenCalledWith(firstRun)
    expect(h.listeners.size).toBe(0)
    h.emit({ type: 'answer', runId: firstRun, text: 'Stale data' })
    await h.chat.sendMessage({ text: 'Try another check' })
    expect(agentMessageText(h.chat.lastMessage!)).toBe('New result.')
    expect(h.chat.status).toBe('ready')
  })

  it('finalizes interrupted steps and clears approvals before storing a stopped run', () => {
    const message: AgentUIMessage = {
      id: 'partial',
      role: 'assistant',
      parts: [
        { type: 'text', text: 'Checking CPU...' },
        {
          type: 'data-stage',
          id: 'stage-0',
          data: { index: 0, label: 'bash: ps', status: 'running', output: 'partial result' },
        },
        {
          type: 'data-approval',
          id: 'approval',
          data: {
            type: 'approval',
            runId: 'agent-old',
            approvalId: 'a',
            title: 'Allow?',
            command: 'ps',
          },
        },
      ],
    }
    const final = finalizeAgentMessage(message, 'Stopped by you.')
    expect(agentMessageStages(final)[0]).toMatchObject({
      status: 'failed',
      detail: 'Stopped by you.',
      output: 'partial result',
    })
    expect(agentMessageText(final)).toBe('Checking CPU...')
    expect(agentMessageError(final)).toBe('Stopped by you.')
    expect(final.parts.find((part) => part.type === 'data-approval')).toMatchObject({ data: null })
  })

  it('reuses the assistant message id on retry so saved history is updated rather than duplicated', async () => {
    let attempt = 0
    const h = harness(async ({ runId, task }, emit) => {
      emit({ type: 'start', runId: runId!, task })
      if (attempt++ === 0) emit({ type: 'error', runId: runId!, message: 'Temporary failure' })
      else {
        emit({ type: 'answer', runId: runId!, text: 'Recovered response' })
        emit({ type: 'done', runId: runId! })
      }
      return { ok: true, runId }
    })
    await h.chat.sendMessage({ text: 'Inspect CPU' })
    const id = h.chat.lastMessage!.id
    await h.chat.regenerate({ messageId: id })
    expect(h.chat.lastMessage!.id).toBe(id)
    expect(h.chat.messages.filter((message) => message.role === 'assistant')).toHaveLength(1)
    expect(agentMessageText(h.chat.lastMessage!)).toBe('Recovered response')
  })

  it('rejects tool-only completion rather than silently displaying an empty assistant message', async () => {
    const h = harness(async ({ runId, task }, emit) => {
      emit({ type: 'start', runId: runId!, task })
      emit({
        type: 'stage',
        runId: runId!,
        stage: { index: 0, label: 'bash: ps', status: 'done', output: 'CPU: 12%' },
      })
      emit({ type: 'done', runId: runId! })
      return { ok: true, runId }
    })
    await h.chat.sendMessage({ text: 'Inspect CPU' })
    expect(h.chat.error?.message).toBe('Agent finished without a response.')
    expect(agentMessageStages(h.chat.lastMessage!)[0]?.output).toBe('CPU: 12%')
  })
})

describe('shadcn AI SDK helper scenarios', () => {
  it('replays typed stage updates through the same SDK message state used by the real UI', async () => {
    const scenario = createChat<AgentUIMessage>()
      .user('Inspect CPU')
      .assistant(({ writer }) => {
        writer.data({
          type: 'data-stage',
          id: 'stage-0',
          data: { index: 0, label: 'bash: ps', status: 'running' },
        })
        writer.data({
          type: 'data-stage',
          id: 'stage-0',
          data: { index: 0, label: 'bash: ps', status: 'done', output: '42  12%  Browser' },
        })
        writer.text('CPU: 12%. Browser is the busiest process.', { mode: 'instant' })
      })
    const chat = new Chat<AgentUIMessage>({ transport: scenario.transport({ delayMs: 0 }) })
    await chat.sendMessage(scenario.next([])!)
    expect(chat.status).toBe('ready')
    expect(agentMessageStages(chat.lastMessage!)).toHaveLength(1)
    expect(agentMessageStages(chat.lastMessage!)[0]?.status).toBe('done')
    expect(agentMessageText(chat.lastMessage!)).toContain('CPU: 12%')
  })

  it('exercises a provider failure without a network request or a fabricated success answer', async () => {
    const scenario = createChat<AgentUIMessage>().user('Inspect CPU').error('Provider unavailable')
    const chat = new Chat<AgentUIMessage>({ transport: scenario.transport({ delayMs: 0 }) })
    await chat.sendMessage(scenario.next([])!)
    expect(chat.status).toBe('error')
    expect(chat.error?.message).toBe('Provider unavailable')
  })
})
