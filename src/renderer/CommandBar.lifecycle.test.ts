// @vitest-environment jsdom

import { act, createElement, type ComponentProps } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { SearchResult } from '../shared/search'
import CommandBar from './CommandBar'

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
