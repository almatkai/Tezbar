import { describe, expect, it, vi } from 'vitest'
import { createLoopDriver, type PiEvent } from './loop'

describe('createLoopDriver', () => {
  it('calls onError when a message ends with an error stopReason', () => {
    const callbacks = {
      onStage: vi.fn(),
      onMessageDelta: vi.fn(),
      onAnswer: vi.fn(),
      onDone: vi.fn(),
      onError: vi.fn(),
    }

    const driver = createLoopDriver(callbacks)
    driver({ type: 'agent_start' })
    driver({ type: 'turn_start' })
    driver({
      type: 'message_start',
      message: {
        role: 'assistant',
        stopReason: 'error',
        errorMessage: 'Connection refused: http://127.0.0.1:8080/v1',
      },
    })
    driver({
      type: 'agent_end',
      messages: [
        {
          role: 'assistant',
          stopReason: 'error',
          errorMessage: 'Connection refused: http://127.0.0.1:8080/v1',
        },
      ],
    })

    expect(callbacks.onError).toHaveBeenCalledWith('Connection refused: http://127.0.0.1:8080/v1')
    expect(callbacks.onAnswer).not.toHaveBeenCalled()
  })

  it('calls onError when agent ends with empty response and no tools', () => {
    const callbacks = {
      onStage: vi.fn(),
      onMessageDelta: vi.fn(),
      onAnswer: vi.fn(),
      onDone: vi.fn(),
      onError: vi.fn(),
    }

    const driver = createLoopDriver(callbacks)
    driver({ type: 'agent_start' })
    driver({ type: 'turn_start' })
    driver({ type: 'agent_end', messages: [] })

    expect(callbacks.onError).toHaveBeenCalledWith('Agent finished without a response.')
    expect(callbacks.onAnswer).not.toHaveBeenCalled()
  })

  it('streams deltas and delivers answer on normal run', () => {
    const callbacks = {
      onStage: vi.fn(),
      onMessageDelta: vi.fn(),
      onAnswer: vi.fn(),
      onDone: vi.fn(),
      onError: vi.fn(),
    }

    const driver = createLoopDriver(callbacks)
    driver({ type: 'agent_start' })
    driver({ type: 'turn_start' })
    driver({
      type: 'message_update',
      assistantMessageEvent: {
        type: 'text_delta',
        delta: 'Hello world',
      },
    })
    driver({ type: 'turn_end' })
    driver({ type: 'agent_end', messages: [] })

    expect(callbacks.onMessageDelta).toHaveBeenCalledWith('Hello world')
    expect(callbacks.onAnswer).toHaveBeenCalledWith('Hello world')
    expect(callbacks.onDone).toHaveBeenCalled()
    expect(callbacks.onError).not.toHaveBeenCalled()
  })

  it('preserves multi-turn output across intermediate tool calls', () => {
    const callbacks = {
      onStage: vi.fn(),
      onMessageDelta: vi.fn(),
      onAnswer: vi.fn(),
      onDone: vi.fn(),
      onError: vi.fn(),
    }

    const driver = createLoopDriver(callbacks)
    driver({ type: 'agent_start' })

    // Turn 1: commentary before tool
    driver({ type: 'turn_start' })
    driver({
      type: 'message_update',
      assistantMessageEvent: { type: 'text_delta', delta: 'Checking files...' },
    })
    driver({
      type: 'message_update',
      assistantMessageEvent: { type: 'text_end', content: 'Checking files...' },
    })
    driver({
      type: 'tool_execution_start',
      toolCallId: 't1',
      toolName: 'read',
      args: { path: 'a.txt' },
    })
    driver({
      type: 'tool_execution_end',
      toolCallId: 't1',
      result: { content: [{ type: 'text', text: 'ok' }] },
    })
    driver({ type: 'turn_end' })

    // Turn 2: final answer after tool
    driver({ type: 'turn_start' })
    driver({
      type: 'message_update',
      assistantMessageEvent: { type: 'text_delta', delta: 'Here is the answer.' },
    })
    driver({
      type: 'message_update',
      assistantMessageEvent: { type: 'text_end', content: 'Here is the answer.' },
    })
    driver({ type: 'turn_end' })
    driver({ type: 'agent_end', messages: [] })

    expect(callbacks.onAnswer).toHaveBeenCalledWith('Checking files...\n\nHere is the answer.')
    expect(callbacks.onDone).toHaveBeenCalled()
  })

  it('recovers the final answer from the authoritative message snapshot', () => {
    const callbacks = {
      onStage: vi.fn(),
      onMessageDelta: vi.fn(),
      onAnswer: vi.fn(),
      onDone: vi.fn(),
      onError: vi.fn(),
    }
    const driver = createLoopDriver(callbacks)
    const events: PiEvent[] = [
      { type: 'agent_start' },
      { type: 'turn_start' },
      { type: 'message_update', assistantMessageEvent: { type: 'text_delta', delta: 'CPU' } },
      {
        type: 'message_end',
        message: {
          role: 'assistant',
          stopReason: 'stop',
          content: [{ type: 'text', text: 'CPU: 12%. The browser is the busiest process.' }],
        },
      },
      { type: 'agent_end', messages: [] },
    ]
    events.forEach(driver)
    expect(callbacks.onAnswer).toHaveBeenCalledWith('CPU: 12%. The browser is the busiest process.')
  })

  it('does not report a tool-only run as a successful answer', () => {
    const callbacks = {
      onStage: vi.fn(),
      onMessageDelta: vi.fn(),
      onAnswer: vi.fn(),
      onDone: vi.fn(),
      onError: vi.fn(),
    }
    const driver = createLoopDriver(callbacks)
    driver({ type: 'agent_start' })
    driver({
      type: 'tool_execution_start',
      toolCallId: 'cpu',
      toolName: 'bash',
      args: { command: 'ps' },
    })
    driver({
      type: 'tool_execution_end',
      toolCallId: 'cpu',
      result: { content: [{ type: 'text', text: 'CPU: 12%' }] },
    })
    driver({ type: 'agent_end', messages: [] })
    expect(callbacks.onError).toHaveBeenCalledWith('Agent finished without a response.')
    expect(callbacks.onDone).not.toHaveBeenCalled()
  })

  it('exposes successful tool output rather than only a done label', () => {
    const callbacks = {
      onStage: vi.fn(),
      onMessageDelta: vi.fn(),
      onAnswer: vi.fn(),
      onDone: vi.fn(),
      onError: vi.fn(),
    }
    const driver = createLoopDriver(callbacks)
    driver({ type: 'agent_start' })
    driver({
      type: 'tool_execution_start',
      toolCallId: 'cpu',
      toolName: 'bash',
      args: { command: 'ps' },
    })
    driver({
      type: 'tool_execution_end',
      toolCallId: 'cpu',
      result: { content: [{ type: 'text', text: 'PID  CPU  Process\n42   12%  Browser' }] },
    })
    expect(callbacks.onStage).toHaveBeenLastCalledWith(
      expect.objectContaining({
        status: 'done',
        output: 'PID  CPU  Process\n42   12%  Browser',
      })
    )
  })

  it('does not mistake pre-tool commentary for a final answer', () => {
    const callbacks = {
      onStage: vi.fn(),
      onMessageDelta: vi.fn(),
      onAnswer: vi.fn(),
      onDone: vi.fn(),
      onError: vi.fn(),
    }
    const driver = createLoopDriver(callbacks)
    driver({ type: 'agent_start' })
    driver({
      type: 'message_update',
      assistantMessageEvent: { type: 'text_delta', delta: 'Checking CPU...' },
    })
    driver({
      type: 'tool_execution_start',
      toolCallId: 'cpu',
      toolName: 'bash',
      args: { command: 'ps' },
    })
    driver({
      type: 'tool_execution_end',
      toolCallId: 'cpu',
      result: { content: [{ type: 'text', text: '12%' }] },
    })
    driver({ type: 'agent_end', messages: [] })
    expect(callbacks.onError).toHaveBeenCalledWith('Agent finished without a response.')
    expect(callbacks.onDone).not.toHaveBeenCalled()
  })

  it('caps tool output and never serializes image payloads into the timeline', () => {
    const callbacks = {
      onStage: vi.fn(),
      onMessageDelta: vi.fn(),
      onAnswer: vi.fn(),
      onDone: vi.fn(),
      onError: vi.fn(),
    }
    const driver = createLoopDriver(callbacks)
    driver({
      type: 'tool_execution_start',
      toolCallId: 't',
      toolName: 'read',
      args: { path: 'image.png' },
    })
    driver({
      type: 'tool_execution_end',
      toolCallId: 't',
      durationMs: 150,
      result: {
        content: [
          { type: 'image', data: 'private-base64-image' },
          { type: 'text', text: 'x'.repeat(10000) },
        ],
      },
    })
    const stage = callbacks.onStage.mock.calls.at(-1)![0]
    expect(stage.output).toHaveLength(8000)
    expect(stage.outputTruncated).toBe(true)
    expect(stage.durationMs).toBe(150)
    expect(JSON.stringify(stage)).not.toContain('private-base64-image')
  })

  it('settles every retry stage and emits completion once', () => {
    const callbacks = {
      onStage: vi.fn(),
      onMessageDelta: vi.fn(),
      onAnswer: vi.fn(),
      onDone: vi.fn(),
      onError: vi.fn(),
    }
    const stages = new Map<number, { status: string }>()
    callbacks.onStage.mockImplementation((stage) => stages.set(stage.index, stage))
    const driver = createLoopDriver(callbacks)
    driver({ type: 'agent_start' })
    driver({ type: 'auto_retry_start', attempt: 1, maxAttempts: 3 })
    driver({ type: 'auto_retry_start', attempt: 2, maxAttempts: 3 })
    driver({ type: 'auto_retry_end', success: true })
    driver({
      type: 'message_end',
      message: {
        role: 'assistant',
        stopReason: 'stop',
        content: [{ type: 'text', text: 'Recovered.' }],
      },
    })
    driver({ type: 'agent_end', willRetry: false, messages: [] })
    expect(callbacks.onDone).not.toHaveBeenCalled()
    driver({ type: 'agent_settled', aborted: false })
    driver({ type: 'agent_settled', aborted: false })
    expect(callbacks.onDone).toHaveBeenCalledTimes(1)
    expect([...stages.values()].some((stage) => stage.status === 'running')).toBe(false)
  })

  it('tracks thinking_delta as a stage and finishes on text_delta', () => {
    const callbacks = {
      onStage: vi.fn(),
      onMessageDelta: vi.fn(),
      onAnswer: vi.fn(),
      onDone: vi.fn(),
      onError: vi.fn(),
    }

    const driver = createLoopDriver(callbacks)
    driver({ type: 'agent_start' })
    driver({ type: 'turn_start' })
    driver({
      type: 'message_update',
      assistantMessageEvent: { type: 'thinking_delta', delta: 'Analyzing query...' },
    })

    expect(callbacks.onStage).toHaveBeenCalledWith(
      expect.objectContaining({ label: 'Thinking…', status: 'running' })
    )

    driver({
      type: 'message_update',
      assistantMessageEvent: { type: 'text_delta', delta: 'Done thinking.' },
    })

    expect(callbacks.onStage).toHaveBeenCalledWith(
      expect.objectContaining({ label: 'Thinking…', status: 'done' })
    )
    driver({ type: 'turn_end' })
    driver({ type: 'agent_end', messages: [] })
    expect(callbacks.onAnswer).toHaveBeenCalledWith('Done thinking.')
  })
})
