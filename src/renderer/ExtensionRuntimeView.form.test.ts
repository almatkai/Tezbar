// @vitest-environment jsdom
import { act, createElement } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type {
  ExtensionInvokeActionResult,
  ExtensionRunCommandResult,
} from '../shared/extensionRuntime'
import ExtensionRuntimeView from './ExtensionRuntimeView'

type View = Extract<ExtensionRunCommandResult, { ok: true; mode: 'view' }>
const initial: View = {
  ok: true,
  mode: 'view',
  sessionId: 'form-a',
  generation: 'a',
  refreshMs: null,
  extensionId: 'perplexity',
  commandName: 'ask',
  title: 'Ask Perplexity',
  root: {
    type: 'Form',
    children: [
      { type: 'Form.TextArea', props: { id: 'prompt', title: 'Ask Anything', value: 'Question' } },
    ],
  },
  actions: [{ id: 'submit', kind: 'submit-form', title: 'Submit' }],
}

function deferred() {
  let resolve!: (result: ExtensionInvokeActionResult) => void
  let reject!: (error: Error) => void
  const promise = new Promise<ExtensionInvokeActionResult>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

describe('form invocation lifecycle', () => {
  let container: HTMLDivElement
  let root: Root
  const invoke = vi.fn<(request: unknown) => Promise<ExtensionInvokeActionResult>>()
  const back = vi.fn()
  const status = () => container.querySelector('[data-command-surface-status]')!
  const submit = () => {
    act(() => container.querySelector<HTMLButtonElement>('[aria-label="Submit form"]')!.click())
  }
  const render = (view = initial) => {
    act(() => root.render(createElement(ExtensionRuntimeView, { initial: view, onBack: back })))
  }

  beforeEach(() => {
    vi.useFakeTimers()
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true)
    vi.stubGlobal('tezbar', {
      extensionInvokeAction: invoke,
      extensionDisposeSession: vi.fn().mockResolvedValue(undefined),
    })
    invoke.mockReset()
    back.mockClear()
    container = document.createElement('div')
    document.body.append(container)
    root = createRoot(container)
  })

  afterEach(async () => {
    act(() => root.unmount())
    await vi.runOnlyPendingTimersAsync()
    container.remove()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  })

  it.each(['no-view', 'view'] as const)(
    'clears silent %s success and permits another submit',
    async (mode) => {
      const pending = deferred()
      invoke.mockReturnValue(pending.promise)
      render()
      submit()
      submit()
      expect(invoke).toHaveBeenCalledExactlyOnceWith({
        sessionId: 'form-a',
        actionId: 'submit',
        formValues: { prompt: 'Question' },
      })
      expect(status().textContent).toBe('Working · Submit…')
      expect(status().querySelector('.animate-spin')).not.toBeNull()
      expect(status().querySelector('.text-ink-3')).not.toBeNull()
      expect(container.querySelector('.sheet-above-bar')).toBeNull()
      await act(async () =>
        pending.resolve(
          mode === 'view'
            ? { ...initial, mode: 'view' }
            : { ok: true, mode: 'no-view', message: '' }
        )
      )
      expect(status().textContent).toBe('')
      await act(async () => submit())
      expect(invoke).toHaveBeenCalledTimes(2)
    }
  )

  it.each(['result', 'throw'] as const)(
    'preserves %s failures and releases the submit guard',
    async (failure) => {
      const pending = deferred()
      invoke.mockReturnValue(pending.promise)
      render()
      submit()
      await act(async () => {
        if (failure === 'throw') pending.reject(new Error('Network unavailable'))
        else pending.resolve({ ok: false, message: 'Network unavailable' })
      })
      expect(status().textContent).toBe('Failed · Network unavailable')
      expect(status().querySelector('.text-rose-300')).not.toBeNull()
      expect(status().querySelector('.animate-spin')).toBeNull()
      invoke.mockResolvedValue({ ok: true, mode: 'no-view', message: '' })
      await act(async () => submit())
      expect(invoke).toHaveBeenCalledTimes(2)
      expect(status().textContent).toBe('')
    }
  )

  it.each([
    { ok: true, mode: 'no-view', message: 'Saved answer' },
    {
      ok: true,
      mode: 'no-view',
      message: '',
      effects: [{ kind: 'toast', style: 'success', title: 'Answer ready' }],
    },
  ] satisfies ExtensionInvokeActionResult[])('keeps a returned notice: %j', async (result) => {
    invoke.mockResolvedValue(result)
    render()
    await act(async () => submit())
    expect(status().textContent).toBe(result.message || 'Saved · Answer ready')
    expect(status().querySelector('.animate-spin')).toBeNull()
  })

  it('clears progress on session replacement and ignores the old invocation without clearing new progress', async () => {
    const first = deferred()
    const second = deferred()
    invoke.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
    render()
    submit()
    render({ ...initial, sessionId: 'form-b', generation: 'b' })
    expect(status().textContent).toBe('')
    submit()
    expect(invoke).toHaveBeenCalledTimes(2)
    await act(async () => first.resolve({ ok: true, mode: 'no-view', message: 'Stale notice' }))
    expect(status().textContent).toBe('Working · Submit…')
    submit()
    expect(invoke).toHaveBeenCalledTimes(2)
    await act(async () => second.resolve({ ok: true, mode: 'no-view', message: '' }))
    expect(status().textContent).toBe('')
  })

  it('keeps Command K actions available from the composer and guards overlay submits too', async () => {
    const pending = deferred()
    invoke.mockReturnValue(pending.promise)
    render()
    submit()
    act(() =>
      container.querySelector('textarea')!.dispatchEvent(
        new KeyboardEvent('keydown', {
          key: 'k',
          metaKey: true,
          bubbles: true,
          cancelable: true,
        })
      )
    )
    const action = Array.from(container.querySelectorAll('button')).find(
      (button) =>
        button.textContent?.includes('Submit') &&
        button.getAttribute('aria-label') !== 'Submit form'
    )
    expect(action).toBeDefined()
    act(() => action!.click())
    expect(invoke).toHaveBeenCalledTimes(1)
    await act(async () => pending.resolve({ ok: true, mode: 'no-view', message: '' }))
    expect(status().textContent).toBe('')
  })
})
