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

it.each(['dark', 'white'])(
  'reviews changed tools and edits saved secrets in the %s theme',
  async (theme) => {
    document.documentElement.setAttribute('data-theme', theme)
    const calls: unknown[][] = []
    const state: AiExtensionsState = {
      globalInstructions: 'Fixture instructions',
      useAgentsMd: true,
      extensions: [
        {
          manifestVersion: 1,
          id: 'fixture',
          name: 'Fixture tools',
          enabled: true,
          connections: [
            {
              id: 'connection',
              name: 'Fixture',
              transport: 'stdio',
              command: 'fixture-server',
              tested: true,
              needsReview: true,
              env: { FIXTURE_TOKEN: { secret: 'fixture-token', label: 'Fixture token' } },
              catalog: {
                tools: [
                  {
                    name: 'read',
                    description: 'Changed description',
                    inputSchema: { type: 'object', properties: { path: { type: 'string' } } },
                  },
                ],
              },
              previousCatalog: {
                tools: [
                  {
                    name: 'read',
                    description: 'Original description',
                    inputSchema: { type: 'object', properties: { file: { type: 'string' } } },
                  },
                ],
              },
            },
          ],
        },
      ],
    }
    state.extensions[0]!.providers = [
      {
        id: 'custom:fixture',
        title: 'Fixture model',
        baseURL: 'https://example.invalid/v1',
        headers: { Authorization: { secret: 'MODEL_AUTH', label: 'Model access key' } },
      },
    ]
    window.tezbar = {
      listPiExtensions: async () => [],
      aiExtensions: async (action: string, ...args: unknown[]) => {
        calls.push([action, ...args])
        return action === 'list' ? state : { ok: true }
      },
    } as unknown as RaymesApi
    const host = document.createElement('div')
    document.body.append(host)
    const root = createRoot(host)
    dispose = () => {
      act(() => root.unmount())
      host.remove()
      document.documentElement.removeAttribute('data-theme')
    }
    await act(async () => root.render(createElement(AiExtensionsSection)))
    expect(host.textContent).toContain('Tools changed. Review')
    expect(
      host.querySelector<HTMLInputElement>('input[aria-label="Enable Fixture tools"]')!.disabled
    ).toBe(true)
    await act(async () =>
      Array.from(host.querySelectorAll('button'))
        .find((b) => b.textContent === 'Review')!
        .click()
    )
    expect(host.textContent).toContain('Before: Original description')
    expect(host.textContent).toContain('After: Changed description')
    await act(async () =>
      Array.from(host.querySelectorAll('button'))
        .find((b) => b.textContent === 'Accept changes')!
        .click()
    )
    expect(calls).toContainEqual(['accept-review', 'fixture'])
    await act(async () =>
      Array.from(host.querySelectorAll('button'))
        .find((b) => b.textContent === 'Edit connection')!
        .click()
    )
    const secret = host.querySelector<HTMLInputElement>('input[aria-label="Variable 1 value"]')!
    expect(secret.type).toBe('password')
    expect(secret.value).toBe('')
    expect(secret.placeholder).toBe('Saved')
    expect(host.innerHTML).not.toContain('fixture-token')
    const providerKey = Array.from(host.querySelectorAll('label'))
      .find((label) => label.textContent?.startsWith('Model access key'))!
      .querySelector('input')!
    expect(providerKey.type).toBe('password')
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(
        providerKey,
        'fixture-provider-key'
      )
      providerKey.dispatchEvent(new Event('input', { bubbles: true }))
    })
    await act(async () =>
      Array.from(host.querySelectorAll('button'))
        .find((button) => button.textContent === 'Save key')!
        .click()
    )
    expect(calls).toContainEqual([
      'set-secret',
      'fixture',
      'MODEL_AUTH',
      'fixture-provider-key',
      { providerId: 'custom:fixture' },
    ])
    expect(providerKey.value).toBe('')
    expect(host.textContent).toContain('Key saved.')
    expect(host.querySelector('.grid')).toBeNull()
  }
)

it.each(['dark', 'white'])(
  'previews imported configuration without testing or enabling in the %s theme',
  async (theme) => {
    document.documentElement.setAttribute('data-theme', theme)
    const calls: unknown[][] = []
    const state: AiExtensionsState = { extensions: [], globalInstructions: '', useAgentsMd: true }
    window.tezbar = {
      listPiExtensions: async () => [],
      aiExtensions: async (action: string, ...args: unknown[]) => {
        calls.push([action, ...args])
        if (action === 'list') return state
        if (action === 'parse-import')
          return {
            previewId: 'fixture-preview',
            manifest: {
              manifestVersion: 1,
              id: 'import',
              name: 'Imported program',
              connections: [
                {
                  id: 'server',
                  name: 'Server',
                  transport: 'stdio',
                  command: 'fixture-server',
                  args: ['--stdio'],
                },
              ],
            },
            secretCount: 1,
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
      document.documentElement.removeAttribute('data-theme')
    }
    const click = async (text: string) =>
      act(async () =>
        Array.from(host.querySelectorAll('button'))
          .find((b) => b.textContent === text)!
          .click()
      )
    await act(async () => root.render(createElement(AiExtensionsSection)))
    await click('Add ▾')
    await click('Paste configuration…')
    const input = host.querySelector<HTMLTextAreaElement>('textarea[aria-label="Configuration"]')!
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(
        input,
        '{"mcpServers":{}}'
      )
      input.dispatchEvent(new Event('input', { bubbles: true }))
    })
    await click('Preview')
    expect(host.textContent).toContain('fixture-server --stdio')
    expect(host.textContent).toContain('1 keys will be stored as secrets.')
    expect(calls.some(([action]) => ['add', 'test', 'set-enabled'].includes(String(action)))).toBe(
      false
    )
    await click('Add')
    expect(calls).toContainEqual(['add', { previewId: 'fixture-preview' }])
    expect(calls.some(([action]) => ['test', 'set-enabled'].includes(String(action)))).toBe(false)
  }
)
