import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { ipcMain } from '@tezbar/desktop-runtime'

const runtime = vi.hoisted(() => ({
  signals: new Map<string, AbortSignal>(),
  finish: new Map<string, () => void>(),
}))
vi.mock('./bridge', () => ({
  getSharedBridge: () => ({
    run: (_task: string, options: { runId: string; signal: AbortSignal }) => new Promise<void>((resolve, reject) => {
      runtime.signals.set(options.runId, options.signal)
      runtime.finish.set(options.runId, resolve)
      options.signal.addEventListener('abort', () => reject(new Error('Agent run aborted')), { once: true })
    }),
  }),
  disposeSharedBridge: () => {},
}))
vi.mock('../llm/registry', async (load) => ({
  ...await load<typeof import('../llm/registry')>(),
  getSelectedPiProviderBridge: () => undefined,
  getSelectedPiModelPattern: () => undefined,
}))

beforeAll(async () => {
  const { registerIpcHandlers } = await import('../ipc')
  registerIpcHandlers(() => null)
})
beforeEach(() => {
  runtime.signals.clear()
  runtime.finish.clear()
})
afterAll(async () => {
  const { shutdownIpcHandlers } = await import('../ipc')
  shutdownIpcHandlers()
})

describe('native agent request correlation', () => {
  it('rejects malformed client ids before starting any work', async () => {
    await expect(ipcMain._invoke('agent:run', { runId: 'invalid/id', task: 'Inspect CPU' }))
      .resolves.toMatchObject({ ok: false, error: 'Invalid agent run id' })
    expect(runtime.signals.size).toBe(0)
  })

  it('returns the client id and ignores late cancellation for an older run', async () => {
    await expect(ipcMain._invoke('agent:run', { runId: 'agent-first', task: 'Inspect CPU' }))
      .resolves.toMatchObject({ ok: true, runId: 'agent-first' })
    await vi.waitFor(() => expect(runtime.signals.has('agent-first')).toBe(true))
    await expect(ipcMain._invoke('agent:run', { runId: 'agent-second', task: 'Inspect memory' }))
      .resolves.toMatchObject({ ok: true, runId: 'agent-second' })
    await vi.waitFor(() => expect(runtime.signals.has('agent-second')).toBe(true))
    expect(runtime.signals.get('agent-first')?.aborted).toBe(true)
    await ipcMain._invoke('agent:cancel', 'agent-first')
    expect(runtime.signals.get('agent-second')?.aborted).toBe(false)
    await ipcMain._invoke('agent:cancel', 'agent-second')
    expect(runtime.signals.get('agent-second')?.aborted).toBe(true)
    await new Promise<void>((resolve) => setImmediate(resolve))
  })
})
