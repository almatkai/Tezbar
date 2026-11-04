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
                    title: 'Changed title',
                    annotations: { readOnlyHint: true, destructiveHint: false },
                    outputSchema: { type: 'object', required: ['changedOutput'] },
                    inputSchema: { type: 'object', properties: { path: { type: 'string' } } },
                  },
                ],
              },
              previousCatalog: {
                tools: [
                  {
                    name: 'read',
                    description: 'Original description',
                    title: 'Original title',
                    annotations: { readOnlyHint: false, destructiveHint: true },
                    outputSchema: { type: 'string' },
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
    expect(host.textContent).toContain('Changed title')
    expect(host.textContent).toContain('Original title')
    expect(host.textContent).toContain('readOnlyHint\":true')
    expect(host.textContent).toContain('destructiveHint\":true')
    expect(host.textContent).toContain('changedOutput')
    await act(async () =>
      Array.from(host.querySelectorAll('button'))
        .find((b) => b.textContent === 'Accept changes')!
        .click()
    )
    expect(calls).toContainEqual([
      'accept-review',
      'fixture',
      { connection: { pins: undefined, trustPin: undefined } },
    ])
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
    expect(host.textContent).toContain('Command: "fixture-server"')
    expect(host.textContent).toContain('Arguments: ["--stdio"]')
    expect(host.textContent).toContain('1 keys will be stored as secrets.')
    expect(calls.some(([action]) => ['add', 'test', 'set-enabled'].includes(String(action)))).toBe(
      false
    )
    await click('Add')
    expect(calls).toContainEqual(['add', { previewId: 'fixture-preview' }])
    expect(calls.some(([action]) => ['test', 'set-enabled'].includes(String(action)))).toBe(false)
  }
)

it('preserves typed multiline arguments, cleared timeout and an unsaved instructions draft across reloads', async () => {
  const state: AiExtensionsState = {
    extensions: [],
    globalInstructions: 'saved',
    useAgentsMd: true,
  }
  let changed = () => {}
  const calls: unknown[][] = []
  window.tezbar = {
    listPiExtensions: async () => [],
    onAiExtensionsChanged: (cb: () => void) => {
      changed = cb
      return () => {}
    },
    aiExtensions: async (action: string, ...args: unknown[]) => {
      calls.push([action, ...args])
      if (action === 'list') return state
      if (action === 'add') {
        const record = args[0] as { connections: unknown[] }
        return {
          ...record,
          connections: record.connections.map((c) => ({ ...(c as object), slug: 'retained-slug' })),
        }
      }
      return { ok: false, error: 'fixture start failed' }
    },
  } as unknown as RaymesApi
  const host = document.createElement('div')
  document.body.append(host)
  const root = createRoot(host)
  dispose = () => {
    act(() => root.unmount())
    host.remove()
  }
  await act(async () => root.render(createElement(AiExtensionsSection)))
  const click = async (text: string) =>
    act(async () => {
      ;[...host.querySelectorAll('button')].find((b) => b.textContent === text)!.click()
    })
  const input = async (element: HTMLInputElement | HTMLTextAreaElement, value: string) =>
    act(async () => {
      Object.getOwnPropertyDescriptor(
        element.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype,
        'value'
      )!.set!.call(element, value)
      element.dispatchEvent(new Event('input', { bubbles: true }))
    })
  const instructions = [...host.querySelectorAll('label')]
    .find((l) => l.textContent?.startsWith('Custom instructions'))!
    .querySelector('textarea')!
  await input(instructions, 'unsaved draft')
  await act(async () => changed())
  expect(instructions.value).toBe('unsaved draft')
  await click('Add ▾')
  await click('Add connection')
  const label = (name: string) =>
    [...host.querySelectorAll('label')].find((l) => l.textContent?.trim().startsWith(name))!
  await input(label('Name').querySelector('input')!, 'Multiline')
  await input(label('Command').querySelector('input')!, 'fixture-server')
  const args = label('Arguments, one per line').querySelector('textarea')!
  await input(args, '-y\n')
  expect(args.value).toBe('-y\n')
  await input(args, '-y\nfixture-package')
  const timeout = label('Time limit, seconds').querySelector('input')!
  await input(timeout, '')
  expect(timeout.value).toBe('')
  await click('Test connection')
  const added = calls.find(([action]) => action === 'add')?.[1] as {
    connections: { args: string[]; timeoutSeconds: number }[]
  }
  expect(added.connections[0]?.args).toEqual(['-y', 'fixture-package'])
  expect(added.connections[0]?.timeoutSeconds).toBe(120)
})

it('offers missing-key recovery as an explicit action and reloads its status', async () => {
  let canResetSecrets = true
  const calls: string[] = []
  window.tezbar = {
    listPiExtensions: async () => [],
    aiExtensions: async (action: string) => {
      calls.push(action)
      if (action === 'list')
        return { canResetSecrets, extensions: [], globalInstructions: '', useAgentsMd: true }
      if (action === 'reset-secrets') canResetSecrets = false
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
  await act(async () => root.render(createElement(AiExtensionsSection)))
  expect(calls).not.toContain('reset-secrets')
  expect(host.textContent).toContain('archives the unreadable secrets')
  await act(async () =>
    [...host.querySelectorAll('button')]
      .find((b) => b.textContent === 'Reset saved connection secrets')!
      .click()
  )
  expect(calls.filter((c) => c === 'reset-secrets')).toHaveLength(1)
  expect(host.textContent).not.toContain('Reset saved connection secrets')
})

it('scrolls once per review nonce, resolves the panel, and reviews the same folder again', async () => {
  let listener = () => {}
  let state: AiExtensionsState = {
    extensions: [],
    globalInstructions: '',
    useAgentsMd: true,
    projectReviewId: 'review-1',
    project: {
      folder: '/fixture',
      path: '/fixture/.tezbar/ai-extension.json',
      entryHash: 'hash',
      preview: { manifestVersion: 1, id: 'fixture', name: 'Project fixture', connections: [] },
    },
  }
  const calls: unknown[][] = []
  window.tezbar = {
    listPiExtensions: async () => [],
    onAiExtensionsChanged: (fn: () => void) => {
      listener = fn
      return () => {}
    },
    aiExtensions: async (action: string, ...args: unknown[]) => {
      calls.push([action, ...args])
      if (action === 'list') return structuredClone(state)
      if (action === 'dismiss-project-review' || action === 'trust-project')
        state = { ...state, project: undefined, projectReviewId: undefined }
      return { ok: true }
    },
  } as unknown as RaymesApi
  const scroll = vi.fn()
  Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', {
    configurable: true,
    value: scroll,
  })
  const host = document.createElement('div')
  document.body.append(host)
  const root = createRoot(host)
  dispose = () => {
    act(() => root.unmount())
    host.remove()
    Reflect.deleteProperty(HTMLElement.prototype, 'scrollIntoView')
  }
  await act(async () => root.render(createElement(AiExtensionsSection)))
  expect(scroll).toHaveBeenCalledTimes(1)
  for (let i = 0; i < 3; i++) await act(async () => listener())
  expect(scroll).toHaveBeenCalledTimes(1)
  await act(async () =>
    [...host.querySelectorAll('button')].find((b) => b.textContent === 'Keep disabled')!.click()
  )
  expect(calls).toContainEqual(['dismiss-project-review', 'review-1'])
  expect(host.textContent).not.toContain('/fixture')
  await act(async () => listener())
  expect(scroll).toHaveBeenCalledTimes(1)
  state = {
    ...state,
    projectReviewId: 'review-2',
    project: {
      folder: '/fixture',
      path: '/fixture/.tezbar/ai-extension.json',
      entryHash: 'hash',
      preview: { manifestVersion: 1, id: 'fixture', name: 'Project fixture', connections: [] },
    },
  }
  await act(async () => listener())
  expect(scroll).toHaveBeenCalledTimes(2)
  await act(async () =>
    [...host.querySelectorAll('button')].find((b) => b.textContent === 'Review')!.click()
  )
  await act(async () =>
    [...host.querySelectorAll('button')]
      .find((b) => b.textContent === 'Add for this folder')!
      .click()
  )
  expect(calls).toContainEqual(['trust-project', '/fixture', 'hash', 'review-2'])
  expect(host.textContent).not.toContain('/fixture')
})
