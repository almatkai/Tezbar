// @vitest-environment jsdom

import { act, createElement, type ComponentProps } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { flushSync } from 'react-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { SearchResult } from '../shared/search'
import CommandBar from './CommandBar'
import { BackendGeneration } from './backendGeneration'

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

function eventSource<T>() {
  const listeners = new Set<(payload: T) => void>()
  return {
    subscribe: (listener: (payload: T) => void) => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    emit: (payload: T) => {
      listeners.forEach((listener) => listener(payload))
    },
  }
}

const results: SearchResult[] = ['First', 'Second'].map((title, index) => ({
  id: `knowledge:fixture-${index}`,
  title: `${title} fixture result`,
  subtitle: 'Indexed file contents',
  category: 'knowledge',
  score: 100 - index,
  action: { type: 'open-file', path: `/fixtures/${index}.txt` },
}))
const homeResults: SearchResult[] = [
  {
    id: 'home:fixture',
    title: 'Home fixture result',
    subtitle: 'Home',
    category: 'commands',
    score: 100,
    action: { type: 'copy-text', text: 'home' },
  },
]
const noop = () => {}
const props: ComponentProps<typeof CommandBar> = {
  hasUnreadReleaseNotes: false,
  onOpenReleaseNotes: noop,
  onOpenAiChat: noop,
  onOpenSettings: noop,
  onOpenExtensionsSettings: noop,
  onConfigureAi: noop,
  onOpenExtensions: noop,
  onOpenExtensionRuntime: noop,
  onOpenSystemMonitor: noop,
  onOpenIndexingPage: noop,
  onOpenClipboardPage: noop,
  onOpenSnippetsPage: noop,
  onOpenNotesPage: noop,
  onOpenEmojiPicker: noop,
  onStartOnboarding: noop,
  onOpenTerminal: noop,
}

type HideEvent = 'native hide' | 'document hidden' | 'both'

// Mount the real component and drive its native bridge/visibility subscriptions.
// Only browser facilities absent from JSDOM and the desktop API are stubbed.
describe('CommandBar hidden-window Deep Search lifecycle', () => {
  let root: Root
  let container: HTMLDivElement
  let hidden: ReturnType<typeof eventSource<void>>
  let shown: ReturnType<typeof eventSource<{ resetUi: boolean }>>
  let launcherInvalidated: ReturnType<typeof eventSource<{ text?: string }>>
  let deep: ReturnType<typeof deferred<SearchResult[]>>
  let home: ReturnType<typeof deferred<SearchResult[]>>
  let searchAll: ReturnType<typeof vi.fn<(query: string) => Promise<SearchResult[]>>>

  beforeEach(() => {
    vi.useFakeTimers()
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true)
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe() {}
        unobserve() {}
        disconnect() {}
      }
    )
    Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', {
      configurable: true,
      value: noop,
    })
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
      measureText: (text: string) => ({ width: text.length * 8 }),
    } as CanvasRenderingContext2D)
    Object.defineProperty(document, 'hidden', { configurable: true, value: false })
    // Keep caches isolated, including on Node versions with native Web Storage.
    const storage = new Map<string, string>()
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value)
      },
      removeItem: (key: string) => {
        storage.delete(key)
      },
      clear: () => storage.clear(),
      key: (index: number) => Array.from(storage.keys())[index] ?? null,
      get length() {
        return storage.size
      },
    } satisfies Storage)
    hidden = eventSource<void>()
    shown = eventSource<{ resetUi: boolean }>()
    launcherInvalidated = eventSource<{ text?: string }>()
    deep = deferred<SearchResult[]>()
    home = deferred<SearchResult[]>()
    searchAll = vi.fn((query: string) =>
      query.startsWith('!') ? deep.promise : query === '' ? home.promise : Promise.resolve([])
    )
    const subscribe = () => noop
    const api = {
      searchAll,
      onWindowHidden: hidden.subscribe,
      onLauncherInvalidated: launcherInvalidated.subscribe,
      onWindowShown: shown.subscribe,
      onUpdateStatus: subscribe,
      onStreamToken: subscribe,
      onStreamDone: subscribe,
      onStreamError: subscribe,
      onLlmConfigChanged: subscribe,
      onKnowledgeStatus: subscribe,
      listBackgroundTasks: async () => [],
      getUpdateStatus: async () => ({ kind: 'idle' as const }),
      getLlmConfig: async () => ({}),
      chatList: async () => [],
      listSearchCandidates: async () => [],
      getAssetIconDataUrl: async () => null,
    } satisfies Partial<Window['tezbar']>
    Object.defineProperty(window, 'tezbar', { configurable: true, value: api })
    container = document.createElement('div')
    document.body.append(container)
    root = createRoot(container)
  })

  afterEach(async () => {
    await act(async () => root.unmount())
    container.remove()
    localStorage.clear()
    Reflect.deleteProperty(document, 'hidden')
    Reflect.deleteProperty(HTMLElement.prototype, 'scrollIntoView')
    Reflect.deleteProperty(window, 'tezbar')
    vi.clearAllTimers()
    vi.useRealTimers()
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  async function mount(initialValue: string) {
    await act(async () => root.render(createElement(CommandBar, { ...props, initialValue })))
  }

  function input() {
    return container.querySelector<HTMLInputElement>('#command-input')!
  }

  async function advance(ms: number) {
    await act(async () => {
      await vi.advanceTimersByTimeAsync(ms)
    })
  }

  async function hide(kind: HideEvent) {
    await act(async () => {
      if (kind !== 'document hidden') hidden.emit()
      if (kind !== 'native hide') {
        Object.defineProperty(document, 'hidden', { configurable: true, value: true })
        document.dispatchEvent(new Event('visibilitychange'))
      }
    })
  }

  async function reopen() {
    await act(async () => {
      Object.defineProperty(document, 'hidden', { configurable: true, value: false })
      document.dispatchEvent(new Event('visibilitychange'))
      shown.emit({ resetUi: false })
    })
  }

  async function type(value: string) {
    await act(async () => {
      // Use the native setter so React sees a genuine user input change.
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(
        input(),
        value
      )
      input().dispatchEvent(new Event('input', { bubbles: true }))
    })
  }

  function region() {
    return container.querySelector<HTMLElement>('[aria-label="Deep Search results"]')
  }

  async function selectSecondResult() {
    await act(async () => {
      input().dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }))
    })
    await act(async () => {
      region()!.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
    })
    expect(region()!.querySelector('[data-selected="true"]')?.textContent).toContain(
      'Second fixture result'
    )
  }

  it('updates result highlight in the same frame when selection commits inside rAF', async () => {
    let nextId = 0
    const frames = new Map<number, FrameRequestCallback>()
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      frames.set(++nextId, callback)
      return nextId
    })
    vi.stubGlobal('cancelAnimationFrame', (id: number) => frames.delete(id))
    vi.spyOn(HTMLElement.prototype, 'offsetTop', 'get').mockImplementation(
      function (this: HTMLElement) {
        return 6 + Array.from(this.parentElement!.children).indexOf(this) * 38
      }
    )
    vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(36)
    searchAll.mockImplementation(async () => results)
    await mount('ordinary fixture')
    await advance(100)
    const frame = async () => {
      const current = [...frames.values()]
      frames.clear()
      await act(async () => current.forEach((callback) => callback(16)))
    }
    await frame()
    requestAnimationFrame(() => {
      // Reproduce the real WKWebView late-rAF commit. flushSync establishes the
      // commit boundary explicitly in act; the next frame must remain pending.
      flushSync(() => input().dispatchEvent(new KeyboardEvent('keydown', {
        key: 'ArrowDown',
        bubbles: true,
      })))
    })
    await frame()
    const row = container.querySelector<HTMLElement>(
      'button.list-row[data-tone="accent"][data-selected="true"]'
    )!.parentElement!
    const highlight = row.parentElement!.parentElement!.querySelector<HTMLElement>('.glide-highlight')!
    expect(row.offsetTop).toBeGreaterThan(6)
    expect(highlight.style.transform).toBe(`translateY(${row.offsetTop}px)`)
    expect(highlight.style.height).toBe(`${row.offsetHeight}px`)
  })

  it('updates an equal-height same-index fresh row replacement without another frame', async () => {
    let nextId = 0
    let top = 6
    const frames = new Map<number, FrameRequestCallback>()
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      frames.set(++nextId, callback)
      return nextId
    })
    vi.stubGlobal('cancelAnimationFrame', (id: number) => frames.delete(id))
    vi.spyOn(HTMLElement.prototype, 'offsetTop', 'get').mockImplementation(() => top)
    vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(36)
    const reply = deferred<SearchResult[]>()
    searchAll.mockImplementation(() => reply.promise)
    await mount('ordinary fixture')
    await advance(100)
    const frame = async () => {
      const current = [...frames.values()]
      frames.clear()
      await act(async () => current.forEach((callback) => callback(16)))
    }
    await frame()
    const selectedRow = () => container.querySelector<HTMLElement>(
      'button.list-row[data-tone="accent"][data-selected="true"]'
    )!.parentElement!
    const previous = selectedRow()
    const highlight = previous.parentElement!.parentElement!.querySelector<HTMLElement>('.glide-highlight')!
    expect(highlight.style.transform).toBe('translateY(6px)')
    requestAnimationFrame(() => {
      top = 20
      reply.resolve(results.map((row, index) => ({
        ...row,
        id: `fresh:${index}`,
        title: `ordinary fixture ${index}`,
        category: 'files',
      })))
    })
    await frame()
    expect(selectedRow()).not.toBe(previous)
    expect(previous.isConnected).toBe(false)
    expect(highlight.style.transform).toBe('translateY(20px)')
    expect(highlight.style.height).toBe('36px')
  })

  it('keeps measured width current across style, deep-mode text and clear changes', async () => {
    let width = 48
    const context = { font: '', measureText: vi.fn(() => ({ width })) }
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(
      context as unknown as CanvasRenderingContext2D
    )
    await mount('!fixture')
    expect(context.measureText).toHaveBeenLastCalledWith('fixture')
    expect(input().parentElement!.style.flexBasis).toBe('64px')
    input().style.fontSize = '19px'
    width = 96
    await act(async () =>
      root.render(
        createElement(CommandBar, {
          ...props,
          initialValue: '!fixture',
          hasUnreadReleaseNotes: true,
        })
      )
    )
    expect(context.font).toContain('19px')
    expect(input().parentElement!.style.flexBasis).toBe('112px')
    await type('')
    expect(input().parentElement!.style.flexBasis).toBe('')
  })

  it('commits fresh result content and icon props and recovers from image errors', async () => {
    let current: SearchResult = {
      id: 'stable-file',
      title: 'Fixture file',
      subtitle: '/fixtures/file.txt',
      category: 'files',
      score: 10,
      action: { type: 'open-file', path: '/fixtures/file.txt' },
    }
    searchAll.mockImplementation(async () => [{ ...current }])
    await mount('fixture')
    await advance(24)
    await type('fixture f')
    await advance(24)
    current = {
      ...current,
      title: 'Fixture file renamed',
      score: 20,
      action: { type: 'open-file', path: '/fixtures/file.txt' },
    }
    await type('fixture fi')
    await advance(24)
    expect(container.querySelector('.list-row__title')?.textContent).toContain('Fixture file renamed')
    current = { ...current, iconDataUrl: 'data:image/png;base64,ZmFrZQ==' }
    await type('fixture fil')
    await advance(24)
    expect(container.querySelector('img')?.getAttribute('src')).toBe(current.iconDataUrl)
    await act(async () => container.querySelector('img')!.dispatchEvent(new Event('error')))
    expect(container.querySelector('img')).toBeNull()
    current = {
      ...current,
      iconDataUrl: 'data:image/png;base64,bmV3',
      action: { type: 'open-file', path: '/fixtures/new.txt' },
    }
    await type('fixture file')
    await advance(24)
    expect(container.querySelector('img')?.getAttribute('src')).toBe(current.iconDataUrl)
    const execute = vi.fn(async () => ({ ok: true, message: '' }))
    Object.assign(window.tezbar, { executeSearchAction: execute })
    await act(async () =>
      container
        .querySelector<HTMLElement>('.list-row__title')!
        .closest<HTMLButtonElement>('button')!
        .click()
    )
    expect(execute).toHaveBeenCalledWith(
      current.action,
      expect.objectContaining({ resultId: current.id })
    )
  })

  it('keeps the latest action when a fresh reply changes no visual row props', async () => {
    let action: SearchResult['action'] = { type: 'copy-text', text: 'old fixture' }
    searchAll.mockImplementation(async () => [
      {
        id: 'stable-command',
        title: 'Fixture command',
        subtitle: 'Same description',
        category: 'commands',
        score: 10,
        action,
      },
    ])
    const execute = vi.fn(async () => ({ ok: true, message: '' }))
    Object.assign(window.tezbar, { executeSearchAction: execute })
    await mount('fixture')
    await advance(24)
    action = { type: 'copy-text', text: 'new fixture' }
    await act(async () => launcherInvalidated.emit({ text: 'fixture' }))
    await advance(24)
    expect(searchAll).toHaveBeenCalledTimes(2)
    await act(async () =>
      container
        .querySelector<HTMLElement>('.list-row__title')!
        .closest<HTMLButtonElement>('button')!
        .click()
    )
    expect(execute).toHaveBeenCalledWith(action, {
      query: 'fixture',
      rank: 1,
      resultId: 'stable-command',
    })
  })

  for (const timing of ['before debounce', 'request in flight'] as const) {
    it.each<HideEvent>(['native hide', 'document hidden', 'both'])(
      `preserves work, hidden completion and selection across repeated %s (${timing})`,
      async (kind) => {
        await mount('!fixture search')
        if (timing === 'request in flight') await advance(200)
        const query = input().value
        await hide(kind)
        expect(input().value).toBe(query)
        await advance(200)
        expect(searchAll.mock.calls).toEqual([['!fixture search']])
        expect(
          container.querySelector('[aria-label="Deep Search is searching for fixture search"]')
        ).not.toBeNull()

        await act(async () => deep.resolve(results))
        expect(document.hidden).toBe(kind !== 'native hide')
        expect(region()?.textContent).toContain('First fixture result')
        await reopen()
        await selectSecondResult()
        for (let cycle = 0; cycle < 2; cycle++) {
          await hide(kind)
          await advance(100)
          await reopen()
          expect(input().value).toBe(query)
          expect(region()?.textContent).toContain('First fixture result')
          expect(region()?.querySelector('[data-selected="true"]')?.textContent).toContain(
            'Second fixture result'
          )
        }
        expect(searchAll.mock.calls).toEqual([['!fixture search']])
      }
    )
  }

  it.each(['ordinary query', '!fixture search'])('dispatches an isolated %s without a typing-length delay', async (query) => {
    await mount(query)
    await advance(24)
    expect(searchAll.mock.calls).toEqual([[query]])
  })

  it('coalesces pending bursts and bounds in-flight work without losing the final query', async () => {
    const first = deferred<SearchResult[]>()
    const final = deferred<SearchResult[]>()
    searchAll.mockImplementation((query) => query === 'first' ? first.promise : final.promise)
    await mount('first')
    await advance(24)
    expect(searchAll.mock.calls).toEqual([['first']])
    for (const query of ['sec', 'second', 'third', 'final query']) {
      await type(query)
      await advance(24)
    }
    expect(searchAll.mock.calls).toEqual([['first']])
    await act(async () => first.resolve(homeResults))
    await advance(24)
    expect(searchAll.mock.calls).toEqual([['first'], ['final query']])
    expect(container.textContent).not.toContain('Home fixture result')
    await act(async () => final.resolve(results))
    expect(container.textContent).toContain('First fixture result')
  })

  it('coalesces native/document hide into one authoritative home refresh', async () => {
    await mount('ordinary query')
    await advance(100)
    searchAll.mockClear()
    await hide('both')
    await advance(100)
    expect(searchAll.mock.calls).toEqual([['']])
  })

  it('suppresses intermediate IME searches and dispatches the committed final value', async () => {
    await mount('one')
    await advance(24)
    searchAll.mockClear()
    await act(async () => input().dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true })))
    await type('intermediate')
    await advance(50)
    await type('committed query')
    await advance(50)
    expect(searchAll).not.toHaveBeenCalled()
    await act(async () => input().dispatchEvent(new CompositionEvent('compositionend', { bubbles: true })))
    await advance(24)
    expect(searchAll.mock.calls).toEqual([['committed query']])
  })

  it('recovers after native hide interrupts IME without a compositionend event', async () => {
    await mount('ordinary query')
    await advance(24)
    searchAll.mockClear()
    await act(async () => input().dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true })))
    await type('intermediate')
    await hide('native hide')
    await reopen()
    await type('final committed query')
    await advance(24)
    expect(searchAll.mock.calls).toEqual([['final committed query']])
  })

  it('drops a pending normal query on hide and reopens with the latest home result', async () => {
    await mount('ordinary query')
    await advance(24)
    searchAll.mockClear()
    await type('never dispatch this')
    await hide('native hide')
    await advance(100)
    await act(async () => home.resolve(homeResults))
    await reopen()
    expect(searchAll.mock.calls).toEqual([['']])
    expect(container.textContent).toContain('Home fixture result')
  })

  it('keeps single-character local matching and does not extend the pending deadline indefinitely', async () => {
    await mount('a')
    await advance(24)
    expect(searchAll).not.toHaveBeenCalled()
    // Keep changing input for longer than one display opportunity. Dispatch
    // must make progress while typing continues, not only after a quiet gap.
    let latest = 'ab'
    for (let i = 0; i < 12; i++) {
      latest += 'c'
      await type(latest)
      await advance(2)
    }
    expect(searchAll.mock.calls.length).toBeGreaterThan(0)
    await advance(24)
    expect(searchAll.mock.calls.at(-1)).toEqual([latest])
  })

  it('refreshes the latest query on invalidation, ignoring an older in-flight reply', async () => {
    const old = deferred<SearchResult[]>()
    searchAll.mockImplementationOnce(() => old.promise).mockResolvedValue(results)
    await mount('ordinary query')
    await advance(24)
    await act(async () => launcherInvalidated.emit({ text: 'unrelated' }))
    await advance(24)
    expect(searchAll).toHaveBeenCalledTimes(1)
    await act(async () => launcherInvalidated.emit({ text: 'ordinary query' }))
    await advance(24)
    expect(searchAll).toHaveBeenCalledTimes(1)
    await act(async () => old.resolve(homeResults))
    await advance(24)
    expect(searchAll.mock.calls).toEqual([['ordinary query'], ['ordinary query']])
    expect(container.textContent).toContain('First fixture result')
    expect(container.textContent).not.toContain('Home fixture result')
  })

  it('does not let an unresolved previous backend generation block recovery', async () => {
    const old = deferred<SearchResult[]>()
    searchAll.mockImplementationOnce(() => old.promise).mockResolvedValue(results)
    const render = async (generation: number) => {
      await act(async () => root.render(createElement(BackendGeneration.Provider, { value: generation },
        createElement(CommandBar, { ...props, initialValue: 'ordinary query' }))))
    }
    await render(0)
    await advance(24)
    await render(1)
    await advance(24)
    expect(searchAll.mock.calls).toEqual([['ordinary query'], ['ordinary query']])
    expect(container.textContent).toContain('First fixture result')
    await act(async () => old.resolve(homeResults))
    expect(container.textContent).not.toContain('Home fixture result')
  })

  it('cancels pending search when entering AI mode and recovers after an error', async () => {
    searchAll.mockRejectedValueOnce(new Error('fixture failure')).mockResolvedValue(results)
    await mount('ordinary query')
    await advance(24)
    expect(container.textContent).toContain('Search unavailable: fixture failure')
    searchAll.mockClear()
    await type('pending query')
    await type(' ask the agent')
    await advance(24)
    expect(searchAll).not.toHaveBeenCalled()
    await type('recovered query')
    await advance(24)
    expect(searchAll.mock.calls).toEqual([['recovered query']])
    expect(container.textContent).not.toContain('Search unavailable: fixture failure')
    expect(container.textContent).toContain('First fixture result')
  })

  it('does not start delayed home warmup after the user enters Deep Search', async () => {
    await mount('ordinary query')
    await advance(100)
    await hide('native hide')
    await reopen()
    await type('!fixture search')
    await advance(200)
    expect(searchAll.mock.calls).toEqual([['ordinary query'], ['!fixture search']])
    await act(async () => deep.resolve(results))
    expect(region()?.textContent).toContain('First fixture result')
  })

  it('does not let in-flight home warmup replace Deep Search results or selection', async () => {
    await mount('ordinary query')
    await advance(100)
    await hide('native hide')
    await advance(60) // The hide-triggered home request is now in flight.
    expect(searchAll).toHaveBeenCalledWith('')
    await reopen()
    await type('!fixture search')
    await advance(200)
    await act(async () => deep.resolve(results))
    await selectSecondResult()
    await act(async () => home.resolve(homeResults))
    expect(input().value).toBe('fixture search')
    expect(region()?.textContent).toContain('First fixture result')
    expect(region()?.textContent).not.toContain('Home fixture result')
    expect(region()?.querySelector('[data-selected="true"]')?.textContent).toContain(
      'Second fixture result'
    )
  })

  it.each<HideEvent>(['native hide', 'document hidden', 'both'])(
    'still resets ordinary search to home on %s',
    async (kind) => {
      await mount('ordinary query')
      await advance(100)
      await hide(kind)
      expect(input().value).toBe('')
      await advance(100)
      await act(async () => home.resolve(homeResults))
      await reopen()
      expect(input().value).toBe('')
      expect(region()).toBeNull()
      expect(container.textContent).toContain('Home fixture result')
    }
  )
  it.each(['query', 'home', 'home warmup', 'home without durable rows'] as const)(
    'does not replay an ephemeral saved-label action from the %s cache before the backend replies',
    async (cache) => {
      const old: SearchResult = {
        id: 'saved:fixture',
        title: 'Saved label before edit',
        subtitle: 'Fixture',
        category: 'extensions',
        score: 430,
        ephemeral: true,
        action: {
          type: 'run-extension-command',
          extensionId: 'com.tezbar.native.fixture',
          commandName: 'lookup',
          title: 'Lookup fixture',
          argumentValues: { value: '3000' },
        },
      }
      const fresh: SearchResult = {
        ...old,
        title: 'Saved label after edit',
        action: {
          type: 'run-extension-command',
          extensionId: 'com.tezbar.native.fixture',
          commandName: 'lookup',
          title: 'Lookup fixture',
          argumentValues: { value: '8080' },
        },
      }
      const query = cache === 'query' ? 'saved' : ''
      const reply = deferred<SearchResult[]>()
      const durable = cache === 'home without durable rows' ? [] : homeResults
      const other: SearchResult = { ...old, id: 'other:fixture', title: 'Other query live result' }
      searchAll.mockImplementation(async (text) => (text === query ? [old, ...durable] : [other]))
      await mount(cache === 'home warmup' ? 'other' : query)
      await advance(100)
      if (cache === 'home warmup') {
        await hide('native hide')
        await advance(100)
        await reopen()
      }
      // The live response must still render dynamic rows.
      expect(container.textContent).toContain(old.title)
      await type('other')
      await advance(100)
      expect(container.textContent).toContain(other.title)
      await act(async () => launcherInvalidated.emit({})) // Label edited/published while another query is active.
      searchAll.mockImplementation((text) =>
        text === query ? reply.promise : Promise.resolve([other])
      )
      await type(query)
      // Check the real render synchronously, before even the debounce has fired.
      expect(container.textContent).not.toContain(old.title)
      expect(container.textContent).not.toContain(other.title)
      for (const item of durable) expect(container.textContent).toContain(item.title)
      await advance(100)
      expect(container.textContent).not.toContain(old.title)
      await act(async () => reply.resolve([fresh, ...durable]))
      expect(container.textContent).toContain(fresh.title)
      expect(container.textContent).not.toContain(old.title)
    }
  )

  it('uses declared argument metadata and preserves clear-then-exit Escape', async () => {
    const row: SearchResult = {
      id: 'extcmd:com.tezbar.native.fixture:lookup', title: 'Lookup fixture', subtitle: 'Fixture', category: 'extensions', score: 20000,
      action: { type: 'run-extension-command', extensionId: 'com.tezbar.native.fixture', commandName: 'lookup', title: 'Lookup fixture',
        commandArgumentDefinitions: [{ name: 'value', title: 'Number', type: 'text', required: true }],
        launcher: { inlineArgument: { name: 'value', title: 'Number', prompt: 'Type a value', validation: { kind: 'integer', min: 1, max: 65535, maxLength: 5, message: 'Invalid number' } } },
      },
    }
    searchAll.mockImplementation(async () => [row])
    await mount('lookup fixture')
    await advance(100)
    const button = container.querySelector<HTMLButtonElement>('[aria-label="Enter Number"]')!
    expect(button).not.toBeNull()
    await act(async () => button.click())
    expect(input().placeholder).toBe('Number')
    await type('a30b00')
    expect(input().value).toBe('3000')
    await act(async () => input().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })))
    expect(input().value).toBe('')
    expect(input().placeholder).toBe('Number')
    await act(async () => input().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })))
    expect(input().value).toBe('lookup fixture')
  })

})
