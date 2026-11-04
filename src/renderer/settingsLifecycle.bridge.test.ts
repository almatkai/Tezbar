// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { initTauriBridge } from './tauri-bridge'
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}), emitTo: vi.fn() }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ label: 'settings' }) }))
afterEach(() => {
  vi.clearAllMocks()
  Reflect.deleteProperty(window, '__TAURI_INTERNALS__')
})
it('the window default close consumer acknowledges immediately and drains writes without React', async () => {
  const events = new Map<string, (event: { payload: unknown }) => void>()
  vi.mocked(listen).mockImplementation(async (name, cb) => {
    events.set(String(name), cb as (event: { payload: unknown }) => void)
    return () => {}
  })
  let resolve!: () => void
  const pending = new Promise<void>((done) => {
    resolve = done
  })
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (
      command === 'call_backend' &&
      args &&
      'channel' in args &&
      args.channel === 'llm-config-set'
    )
      await pending
    return null
  })
  Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} })
  initTauriBridge()
  const write = window.tezbar.setLlmConfig({ model: 'fixture' })
  events.get('settings-close-requested')!({ payload: 7 })
  for (let i = 0; i < 10; i++) await Promise.resolve()
  expect(invoke).toHaveBeenCalledWith('ack_settings_close', { generation: 7 })
  expect(invoke).not.toHaveBeenCalledWith('finish_settings_close')
  // Repeated close requests acknowledge busy, never bypass a healthy drain.
  events.get('settings-close-requested')!({ payload: 8 })
  await Promise.resolve()
  expect(invoke).toHaveBeenCalledWith('ack_settings_close', { generation: 8 })
  resolve()
  await write
  for (let i = 0; i < 10; i++) await Promise.resolve()
  expect(invoke).toHaveBeenCalledWith('finish_settings_close')
})
