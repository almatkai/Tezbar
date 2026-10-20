import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { ipcMain } from '@tezbar/desktop-runtime'

/** Chat storage is mocked: this test only checks how the backend treats the
 *  request, not how history is read from disk. */
vi.mock('./chat/sessionStore', async (load) => ({
  ...(await load<typeof import('./chat/sessionStore')>()),
  listChatSessions: async () => [],
}))
vi.mock('./llm/registry', async (load) => ({
  ...(await load<typeof import('./llm/registry')>()),
  getSelectedPiProviderBridge: () => undefined,
  getSelectedPiModelPattern: () => undefined,
}))

beforeAll(async () => {
  const { registerIpcHandlers } = await import('./ipc')
  registerIpcHandlers(() => null)
})
beforeEach(async () => {
  const { resetForegroundGate } = await import('./foreground')
  resetForegroundGate()
})
afterAll(async () => {
  const { shutdownIpcHandlers } = await import('./ipc')
  shutdownIpcHandlers()
})

describe('foreground gate wiring', () => {
  it('holds background work while a history request is being answered', async () => {
    const { foregroundActive } = await import('./foreground')

    await ipcMain._invoke('chat:list')

    expect(foregroundActive()).toBe(true)
  })

  it('keeps the gate closed for requests that are only status polling', async () => {
    const { foregroundActive } = await import('./foreground')

    await ipcMain._invoke('background-tasks:list')

    expect(foregroundActive()).toBe(false)
  })
})
