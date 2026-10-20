import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { initTauriBridge } from './tauri-bridge'

const transport = vi.hoisted(() => ({
  invoke: vi.fn(),
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
  ready: new Map<string, () => void>(),
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke: transport.invoke }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ label: 'main' }) }))
vi.mock('@tauri-apps/api/event', () => ({
  emitTo: vi.fn(),
  listen: (channel: string, handler: (event: { payload: unknown }) => void) => {
    transport.handlers.set(channel, handler)
    return new Promise<() => void>((resolve) => {
      transport.ready.set(channel, () => resolve(() => transport.handlers.delete(channel)))
    })
  },
}))

beforeEach(() => {
  transport.handlers.clear()
  transport.ready.clear()
  transport.invoke.mockReset().mockResolvedValue({ sessionId: 'native-1', summary: { status: 'running' } })
  vi.stubGlobal('window', { __TAURI_INTERNALS__: {}, location: { search: '' } })
  initTauriBridge()
})
afterEach(() => vi.unstubAllGlobals())

describe('native terminal transport', () => {
  it('waits for both event listeners and forwards all lifecycle metadata in one create', async () => {
    const request = { cols: 80, rows: 24, name: 'Custom name', saveFor: 'forever' as const, keepAliveFor: 'until-stop' as const }
    const created = window.tezbar.terminalCreate(request)
    await Promise.resolve()
    expect(transport.invoke).not.toHaveBeenCalled()
    transport.ready.get('terminal:data')!()
    await Promise.resolve()
    expect(transport.invoke).not.toHaveBeenCalled()
    transport.ready.get('terminal:exit')!()
    const result = await created
    expect(result.summary.status).toBe('running')
    expect(transport.invoke.mock.calls).toEqual([['native_terminal_create', { request }]])
    transport.handlers.get('terminal:exit')!({ payload: { sessionId: 'native-1', exitCode: 0 } })
    expect(transport.invoke).toHaveBeenCalledTimes(1)
  })

  it('uses native attach without a follow-up summary write', async () => {
    transport.invoke.mockResolvedValueOnce(null)
    const request = { sessionId: 'gone', cols: 80, rows: 24 }
    const attached = window.tezbar.terminalAttach(request)
    transport.ready.get('terminal:data')!()
    transport.ready.get('terminal:exit')!()
    expect(await attached).toBeNull()
    expect(transport.invoke.mock.calls).toEqual([['native_terminal_attach', { request }]])
  })

  it('registers launcher shortcut changes only through the Rust config transaction', async () => {
    await window.tezbar.setLlmConfig({ raymesHotkey: 'Alt+Space' })
    expect(transport.invoke.mock.calls).toEqual([['call_backend', { channel: 'llm-config-set', payload: [{ raymesHotkey: 'Alt+Space' }] }]])
  })

  it('delivers extension hotkey payload unchanged and unsubscribes', async () => {
    const listener = vi.fn()
    const unsubscribe = window.tezbar.onRunExtensionCommandFromHotkey(listener)
    const payload = { extensionId: 'raycast.example', commandName: 'open' }
    transport.handlers.get('run-extension-command-from-hotkey')!({ payload })
    expect(listener).toHaveBeenCalledWith(payload)
    transport.ready.get('run-extension-command-from-hotkey')!()
    unsubscribe()
    await Promise.resolve()
    expect(transport.handlers.has('run-extension-command-from-hotkey')).toBe(false)
  })
})
