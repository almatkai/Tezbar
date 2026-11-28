// @vitest-environment jsdom
import { act, createElement, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, expect, it, vi } from 'vitest'
import { useComposerExtensions } from './ComposerExtensions'
import type { RaymesApi } from '../../../../src/shared/desktop-api'
let dispose = () => {}
afterEach(() => {
  dispose()
  vi.restoreAllMocks()
  document.documentElement.removeAttribute('data-theme')
})
it.each(['dark', 'white'])(
  'inserts argument-bearing prompts in the %s theme without submitting',
  async (theme) => {
    document.documentElement.setAttribute('data-theme', theme)
    const calls: unknown[][] = []
    const send = vi.fn()
    window.tezbar = {
      aiExtensions: async (action: string, ...args: unknown[]) => {
        calls.push([action, ...args])
        if (action === 'composer-items')
          return [
            {
              extensionId: 'docs',
              extensionName: 'Docs',
              kind: 'prompts',
              name: 'triage',
              arguments: [{ name: 'issue', required: true }],
            },
          ]
        return { text: 'Review issue 42', images: [] }
      },
    } as unknown as RaymesApi
    function Harness() {
      const [draft, setDraft] = useState('/')
      const commands = useComposerExtensions(draft, setDraft, '/fixture', () => {})
      return createElement(
        'form',
        { onSubmit: send },
        commands.menu,
        createElement('textarea', {
          value: draft,
          onChange: (e: React.ChangeEvent<HTMLTextAreaElement>) => setDraft(e.target.value),
          onKeyDown: commands.onKeyDown,
          'aria-activedescendant': commands.activeDescendant,
        })
      )
    }
    const host = document.createElement('div')
    document.body.append(host)
    const root = createRoot(host)
    dispose = () => {
      act(() => root.unmount())
      host.remove()
    }
    await act(async () => root.render(createElement(Harness)))
    const textarea = host.querySelector('textarea')!
    expect(textarea.getAttribute('aria-activedescendant')).toBe('ai-command-0')
    await act(async () =>
      textarea.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })
      )
    )
    const arg = host.querySelector('input')!
    expect(arg).toBeTruthy()
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(arg, '42')
      arg.dispatchEvent(new Event('input', { bubbles: true }))
    })
    await act(async () =>
      arg.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })
      )
    )
    expect(textarea.value).toBe('Review issue 42')
    expect(calls).toContainEqual([
      'prompt-resolve',
      expect.objectContaining({ cwd: '/fixture', name: 'triage' }),
      { issue: '42' },
    ])
    expect(send).not.toHaveBeenCalled()
    expect(host.querySelector('[role="listbox"]')).toBeNull()
  }
)
