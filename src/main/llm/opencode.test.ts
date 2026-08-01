import { EventEmitter } from 'node:events'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const spawnMock = vi.hoisted(() => vi.fn())

vi.mock('node:child_process', async (importOriginal) => {
  const actual = await importOriginal<typeof import('node:child_process')>()
  return { ...actual, spawn: spawnMock }
})

import { OpenCodeProvider } from './opencode'

function successfulChild(output = 'continued answer'): EventEmitter & {
  stdout: EventEmitter
  stderr: EventEmitter
  kill: ReturnType<typeof vi.fn>
} {
  const child = new EventEmitter() as EventEmitter & {
    stdout: EventEmitter
    stderr: EventEmitter
    kill: ReturnType<typeof vi.fn>
  }
  child.stdout = new EventEmitter()
  child.stderr = new EventEmitter()
  child.kill = vi.fn()
  queueMicrotask(() => {
    child.stdout.emit('data', Buffer.from(output))
    child.emit('close', 0)
  })
  return child
}

describe('OpenCodeProvider', () => {
  beforeEach(() => {
    spawnMock.mockReset()
    spawnMock.mockImplementation(() => successfulChild())
  })

  it('passes the full conversation to a newly selected model', async () => {
    const provider = new OpenCodeProvider('opencode/mimo-v2.5-free')
    const stream = await provider.chat([
      { role: 'system', content: 'Answer the quiz question.' },
      { role: 'user', content: 'Q3: How do tools enhance an agent?' },
      { role: 'assistant', content: 'Tools let an agent act beyond text generation.' },
      { role: 'user', content: 'Continue' },
    ])

    const deltas = []
    for await (const delta of stream) deltas.push(delta.text)

    const args = spawnMock.mock.calls[0]?.[1] as string[]
    const prompt = args.at(-1) ?? ''
    expect(deltas).toEqual(['continued answer'])
    expect(prompt).toContain('Answer the quiz question.')
    expect(prompt).toContain('Q3: How do tools enhance an agent?')
    expect(prompt).toContain('Tools let an agent act beyond text generation.')
    expect(prompt).toContain('Continue')
  })
})
