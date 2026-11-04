// @vitest-environment jsdom
import { act, createElement } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, expect, it, vi } from 'vitest'
import { AiExtensionsSection } from './AiExtensionsSection'
import type { RaymesApi } from '../../shared/desktop-api'
import type { AiExtensionsState } from '../../shared/aiExtensions'
let dispose = (): void => {}
afterEach(() => {
  dispose()
  vi.restoreAllMocks()
})
it('requires a successful test before enabling and keeps the list single-column', async () => {
  let state: AiExtensionsState = {
    extensions: [
      {
        manifestVersion: 1,
        id: 'fixture',
        name: 'Fixture tools',
        connections: [
          { id: 'connection', name: 'Fixture', transport: 'stdio', command: 'fixture-server' },
        ],
      },
    ],
    globalInstructions: '',
    useAgentsMd: true,
  }
  const calls: string[] = []
  window.tezbar = {
    listPiExtensions: async () => [],
    aiExtensions: async (action: string) => {
      calls.push(action)
      if (action === 'list') return state
      if (action === 'test') {
        state = {
          ...state,
          extensions: [
            {
              ...state.extensions[0]!,
              connections: [
                {
                  ...state.extensions[0]!.connections[0]!,
                  tested: true,
                  catalog: { tools: [{ name: 'read' }], prompts: [] },
                },
              ],
            },
          ],
        }
        return { ok: true }
      }
      return { ok: true }
    },
  } as unknown as RaymesApi
  const host = document.createElement('div')
  document.body.append(host)
  const root = createRoot(host)
  dispose = () => {
    act(() => root.unmount())
    host.remove()
  }
  await act(async () => {
    root.render(createElement(AiExtensionsSection))
  })
  const checkbox = host.querySelector<HTMLInputElement>('input[aria-label="Enable Fixture tools"]')!
  expect(checkbox.disabled).toBe(true)
  expect(host.textContent).toContain('Not tested yet')
  await act(async () => {
    Array.from(host.querySelectorAll('button'))
      .find((b) => b.textContent === 'Test')!
      .click()
  })
  expect(checkbox.disabled).toBe(false)
  expect(host.textContent).toContain('1 tools · 0 prompts')
  await act(async () => checkbox.click())
  expect(calls).toContain('set-enabled')
  expect(host.querySelector('.grid')).toBe(null)
})
