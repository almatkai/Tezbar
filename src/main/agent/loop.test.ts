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

    expect(callbacks.onError).toHaveBeenCalledWith(
      'Connection refused: http://127.0.0.1:8080/v1'
    )
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
})
