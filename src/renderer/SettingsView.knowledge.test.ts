// @vitest-environment jsdom

import { act, createElement, StrictMode } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { KnowledgeSnapshot, SemanticVariantStatus } from '../shared/knowledge'
import SettingsView from './SettingsView'
import { invoke } from '@tauri-apps/api/core'
import { initTauriBridge } from './tauri-bridge'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}), emitTo: vi.fn() }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ label: 'settings' }) }))

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((done, fail) => {
    resolve = done
    reject = fail
  })
  return { promise, resolve, reject }
}

function snapshot(roots = 4): KnowledgeSnapshot {
  return {
    roots: Array.from({ length: roots }, (_, i) => ({
      id: `root-${i}`,
      path: `/fixtures/Folder${i}`,
      depth: 'inherit',
      processingBackend: 'local',
      enabled: true,
      createdAt: 1,
      updatedAt: 1,
    })),
    settings: {
      depth: 'smart',
      embeddingModel: 'embeddinggemma-2',
      indexingSpeed: 'fast',
      runOnBattery: false,
      onlyRunHeavyJobsWhenIdle: true,
      maxConcurrentExtractors: 1,
      maxConcurrentOcrJobs: 1,
    },
    status: {
      state: 'indexing',
      backend: 'local',
      progress: 0.5,
      detail: 'Indexing fixture',
      queuedSources: 2,
      processedSources: 2,
      failedSources: 0,
      sourceCount: 4,
      chunkCount: 8,
      indexedPageCount: 0,
      totalPageCount: 0,
      partialSourceCount: 0,
      sourceBytes: 10,
    },
    semanticSearch: {
      supported: true,
      modelState: 'ready',
      downloadProgress: 1,
      downloadBytes: 310_000_000,
      downloadTotalBytes: 310_000_000,
      indexing: true,
      indexedChunks: 4,
      totalChunks: 8,
    },
    localBackendAvailable: true,
    cloudBackendAvailable: false,
    storageBytes: 10,
  }
}

function variant(
  id: 'q8_0' | 'q4_k_m',
  overrides: Partial<SemanticVariantStatus> = {}
): SemanticVariantStatus {
  return {
    id,
    weightBytes: id === 'q8_0' ? 310_000_000 : 182_000_000,
    state: 'ready',
    downloadProgress: 1,
    downloadBytes: 0,
    downloadTotalBytes: 0,
    ...overrides,
  }
}
function variantsSnapshot(): KnowledgeSnapshot {
  const result = snapshot()
  result.semanticSearch = {
    ...result.semanticSearch!,
    variants: [variant('q8_0'), variant('q4_k_m')],
    indexVariant: 'q8_0',
  }
  return result
}

const noop = () => {}
const props = { initialTab: 'knowledge' as const, onBack: noop, onOpenPermissions: noop }
const timeoutMs = 10_000

describe('Knowledge settings snapshot recovery (real SettingsView)', () => {
  let root: Root
  let container: HTMLDivElement
  let first: ReturnType<typeof deferred<KnowledgeSnapshot>>
  let getSnapshot: ReturnType<typeof vi.fn<() => Promise<KnowledgeSnapshot>>>
  let download: ReturnType<typeof vi.fn<Window['tezbar']['downloadKnowledgeEmbeddingModel']>>
  let setModel: ReturnType<typeof vi.fn<Window['tezbar']['setKnowledgeEmbeddingModel']>>
  let cancelDownload: ReturnType<typeof vi.fn<Window['tezbar']['cancelKnowledgeEmbeddingDownload']>>
  let removeModel: ReturnType<typeof vi.fn<Window['tezbar']['removeKnowledgeEmbeddingModel']>>
  let setSpeed: ReturnType<typeof vi.fn<Window['tezbar']['setKnowledgeIndexingSpeed']>>
  let startIndexing: ReturnType<typeof vi.fn<Window['tezbar']['startKnowledgeIndexing']>>
  let statusListener: ((status: KnowledgeSnapshot['status']) => void) | undefined

  beforeEach(() => {
    vi.useFakeTimers()
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true)
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
    first = deferred<KnowledgeSnapshot>()
    getSnapshot = vi.fn(() => first.promise)
    Object.defineProperty(document, 'hidden', { configurable: true, value: false })
    download = vi.fn(async () => snapshot())
    setModel = vi.fn(async () => variantsSnapshot())
    cancelDownload = vi.fn(async () => variantsSnapshot())
    removeModel = vi.fn(async () => variantsSnapshot())
    setSpeed = vi.fn(async () => variantsSnapshot())
    startIndexing = vi.fn(async () => variantsSnapshot())
    statusListener = undefined
    Object.defineProperty(window, 'tezbar', {
      configurable: true,
      value: {
        getKnowledgeSnapshot: getSnapshot,
        onKnowledgeStatus: (listener) => {
          statusListener = listener
          return noop
        },
        setKnowledgeEmbeddingModel: setModel,
        cancelKnowledgeEmbeddingDownload: cancelDownload,
        removeKnowledgeEmbeddingModel: removeModel,
        setKnowledgeIndexingSpeed: setSpeed,
        startKnowledgeIndexing: startIndexing,
        setKnowledgeDepth: async () => variantsSnapshot(),
        pauseKnowledgeIndexing: async () => ({
          ...variantsSnapshot(),
          status: { ...snapshot().status, state: 'paused' },
        }),
        resumeKnowledgeIndexing: async () => variantsSnapshot(),
        getLlmConfig: async () => ({}),
        getSafetyDryRun: async () => false,
        listLlmModels: async () => [],
        githubDeviceCancel: async () => {},
        downloadKnowledgeEmbeddingModel: download,
      } satisfies Partial<Window['tezbar']>,
    })
    container = document.createElement('div')
    document.body.append(container)
    root = createRoot(container)
  })

  afterEach(async () => {
    await act(async () => root.unmount())
    container.remove()
    Reflect.deleteProperty(window, 'tezbar')
    vi.clearAllTimers()
    vi.useRealTimers()
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  async function mount(strict = false) {
    await act(async () =>
      root.render(
        strict
          ? createElement(StrictMode, null, createElement(SettingsView, props))
          : createElement(SettingsView, props)
      )
    )
  }
  function button(text: string) {
    const result = Array.from(container.querySelectorAll('button')).find(
      (b) => b.textContent === text
    )
    expect(result, `button ${text}`).toBeDefined()
    return result!
  }
  function engine() {
    return container.querySelector<HTMLSelectElement>('#knowledge-embedding-model')!
  }
  async function click(text: string) {
    await act(async () => button(text).click())
  }
  function group(id: 'q8_0' | 'q4_k_m') {
    return container.querySelector<HTMLElement>(
      `[role="group"][aria-label="${id === 'q8_0' ? 'Standard (Q8_0)' : 'Compact (Q4_K_M)'} model file"]`
    )!
  }
  function named(name: string) {
    const result = container.querySelector<HTMLButtonElement>(`button[aria-label="${name}"]`)
    expect(result, name).not.toBeNull()
    return result!
  }
  async function choose(value: string) {
    await act(async () => {
      engine().value = value
      engine().dispatchEvent(new Event('change', { bubbles: true }))
    })
  }
  async function ready(next = variantsSnapshot()) {
    getSnapshot.mockResolvedValueOnce(next)
    await mount()
  }
  async function advance(ms: number) {
    await act(async () => vi.advanceTimersByTimeAsync(ms))
  }

  it('shows unknown/loading rather than built-in, empty roots, or a mutation in progress', async () => {
    await mount()
    expect(engine().value).toBe('')
    expect(container.textContent).toContain('Loading Knowledge settings…')
    expect(container.textContent).not.toContain('No folders are being content-indexed.')
    expect(container.textContent).not.toContain('Working…')
    expect(container.textContent).not.toContain('0 files')
    expect(button('Refresh status').disabled).toBe(false)
    expect(button('Add Major Folders').disabled).toBe(true)
    expect(container.querySelector('[role="radio"][aria-checked="true"]')).toBeNull()
    await advance(9_000)
    expect(getSnapshot).toHaveBeenCalledTimes(1)
  })

  it('bounds a hung initial load, stops polling, and accepts retry without late overwrite', async () => {
    await mount()
    await advance(timeoutMs)
    expect(container.querySelector('[role="alert"]')?.textContent).toMatch(/timed out/i)
    expect(container.textContent).toContain('Search engine unavailable')
    expect(container.textContent).toContain('Chunking speed unavailable')
    expect(container.textContent).not.toContain('No folders are being content-indexed.')
    expect(engine().value).toBe('')
    expect(button('Retry').disabled).toBe(false)
    await advance(30_000)
    expect(getSnapshot).toHaveBeenCalledTimes(1)
    getSnapshot.mockResolvedValue(snapshot())
    await click('Retry')
    expect(container.textContent).not.toContain('Search engine unavailable')
    expect(container.textContent).not.toContain('Chunking speed unavailable')
    expect(engine().value).toBe('embeddinggemma-2:q8_0')
    expect(engine().disabled).toBe(false)
    expect(container.querySelectorAll('[role="switch"]')).toHaveLength(4)
    expect(container.textContent).toContain('Building semantic index')
    expect(container.textContent).toContain('Standard (Q8_0) · 310 MB')
    expect(container.querySelector('[role="alert"]')).toBeNull()
    await act(async () => first.resolve(snapshot(0)))
    expect(container.querySelectorAll('[role="switch"]')).toHaveLength(4)
    expect(container.textContent).not.toContain('No folders are being content-indexed.')
  })

  it('allows Refresh status to supersede an outstanding initial request', async () => {
    await mount()
    getSnapshot.mockResolvedValue(snapshot())
    await click('Refresh status')
    expect(engine().value).toBe('embeddinggemma-2:q8_0')
    await act(async () => first.reject(new Error('obsolete request failed')))
    expect(container.querySelector('[role="alert"]')).toBeNull()
    expect(container.querySelectorAll('[role="switch"]')).toHaveLength(4)
  })

  it('shows rejection and genuine zero roots only after successful retry', async () => {
    await mount()
    await act(async () => first.reject(new Error('Native snapshot unavailable')))
    expect(container.querySelector('[role="alert"]')?.textContent).toContain(
      'Native snapshot unavailable'
    )
    expect(container.textContent).not.toContain('No folders are being content-indexed.')
    getSnapshot.mockResolvedValue(snapshot(0))
    await click('Retry')
    expect(container.textContent).toContain('No folders are being content-indexed.')
    expect(button('Add Major Folders').disabled).toBe(false)
  })

  it('preserves the installed model, roots and actions through pending/failed refresh', async () => {
    getSnapshot.mockResolvedValueOnce(snapshot())
    await mount()
    const refresh = deferred<KnowledgeSnapshot>()
    getSnapshot.mockReturnValue(refresh.promise)
    await click('Refresh status')
    expect(container.textContent).toContain('Refreshing Knowledge settings…')
    expect(engine().value).toBe('embeddinggemma-2:q8_0')
    expect(engine().disabled).toBe(false)
    expect(button('Remove').disabled).toBe(false)
    expect(button('Pause indexing').disabled).toBe(false)
    expect(container.querySelectorAll('[role="switch"]')).toHaveLength(4)
    await advance(timeoutMs)
    expect(container.querySelector('[role="alert"]')?.textContent).toContain(
      'Showing the last loaded settings'
    )
    expect(engine().value).toBe('embeddinggemma-2:q8_0')
    expect(button('Remove').disabled).toBe(false)
    const recovered = snapshot()
    recovered.settings.indexingSpeed = 'slow'
    getSnapshot.mockResolvedValue(recovered)
    await click('Refresh status')
    expect(container.querySelector('[role="alert"]')).toBeNull()
    await act(async () => refresh.resolve(snapshot(0)))
    expect(container.querySelectorAll('[role="switch"]')).toHaveLength(4)
    expect(container.querySelector('[role="radio"][aria-checked="true"]')?.textContent).toContain(
      'Smart'
    )
    expect(button('Slow (light)').getAttribute('aria-checked')).toBe('true')
  })

  it('preserves supported download availability after a successful built-in snapshot', async () => {
    const builtIn = snapshot(0)
    builtIn.settings.embeddingModel = 'built-in'
    builtIn.semanticSearch = { ...builtIn.semanticSearch!, modelState: 'not-downloaded' }
    getSnapshot.mockResolvedValueOnce(builtIn)
    await mount()
    expect(engine().value).toBe('built-in')
    expect(button('Download').disabled).toBe(false)
    getSnapshot.mockRejectedValue(new Error('refresh failed'))
    await click('Refresh status')
    expect(button('Download').disabled).toBe(false)
    await click('Download')
    expect(download).toHaveBeenCalledTimes(1)
    expect(engine().value).toBe('embeddinggemma-2:q8_0')
  })

  it('polls serially after success, but does not silently retry a polling failure', async () => {
    getSnapshot.mockResolvedValueOnce(snapshot())
    await mount()
    const poll = deferred<KnowledgeSnapshot>()
    getSnapshot.mockReturnValue(poll.promise)
    await advance(6_000)
    expect(getSnapshot).toHaveBeenCalledTimes(2)
    await act(async () => poll.reject(new Error('poll failed')))
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('poll failed')
    await advance(20_000)
    expect(getSnapshot).toHaveBeenCalledTimes(2)
    expect(engine().value).toBe('embeddinggemma-2:q8_0')
  })

  it('invalidates a pending snapshot when a model mutation returns newer data', async () => {
    const builtIn = snapshot()
    builtIn.settings.embeddingModel = 'built-in'
    builtIn.semanticSearch = { ...builtIn.semanticSearch!, modelState: 'not-downloaded' }
    getSnapshot.mockResolvedValueOnce(builtIn)
    await mount()
    await click('Refresh status')
    await click('Download')
    await act(async () => first.resolve(builtIn))
    expect(engine().value).toBe('embeddinggemma-2:q8_0')
    expect(container.textContent).not.toContain('Refreshing Knowledge settings…')
  })

  it('cleans up timers and safely consumes a late rejection after unmount', async () => {
    const setTimeout = vi.spyOn(window, 'setTimeout')
    const clearTimeout = vi.spyOn(window, 'clearTimeout')
    const setInterval = vi.spyOn(window, 'setInterval')
    const clearInterval = vi.spyOn(window, 'clearInterval')
    await mount()
    const timeoutIndex = setTimeout.mock.calls.findIndex((call) => call[1] === timeoutMs)
    const pollIndex = setInterval.mock.calls.findIndex((call) => call[1] === 1500)
    expect(timeoutIndex).toBeGreaterThanOrEqual(0)
    expect(pollIndex).toBeGreaterThanOrEqual(0)
    await act(async () => root.unmount())
    expect(clearTimeout).toHaveBeenCalledWith(setTimeout.mock.results[timeoutIndex]!.value)
    expect(clearInterval).toHaveBeenCalledWith(setInterval.mock.results[pollIndex]!.value)
    await act(async () => first.reject(new Error('after unmount')))
    await advance(30_000)
    expect(getSnapshot).toHaveBeenCalledTimes(1)
    root = createRoot(container)
  })

  it('does not dispatch an obsolete StrictMode load', async () => {
    await mount(true)
    expect(getSnapshot).toHaveBeenCalledTimes(1)
    await act(async () => first.resolve(snapshot()))
    expect(engine().value).toBe('embeddinggemma-2:q8_0')
  })

  it('invalidates a pending load on tab exit before a fresh load on return', async () => {
    await mount()
    await click('Permissions')
    getSnapshot.mockResolvedValue(snapshot())
    await click('Knowledge')
    expect(getSnapshot).toHaveBeenCalledTimes(2)
    await act(async () => first.resolve(snapshot(0)))
    expect(container.querySelectorAll('[role="switch"]')).toHaveLength(4)
  })

  it('handles a synchronous bridge failure and permits explicit recovery', async () => {
    getSnapshot.mockImplementationOnce(() => {
      throw new Error('Bridge not ready')
    })
    await mount()
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('Bridge not ready')
    getSnapshot.mockResolvedValue(snapshot())
    await click('Retry')
    expect(engine().value).toBe('embeddinggemma-2:q8_0')
  })

  it('keeps Enable semantic search available for an installed but unselected model', async () => {
    const installed = snapshot()
    installed.settings.embeddingModel = 'built-in'
    getSnapshot.mockResolvedValueOnce(installed)
    await mount()
    expect(button('Enable semantic search').disabled).toBe(false)
    expect(
      engine().querySelector<HTMLOptionElement>('option[value="embeddinggemma-2:q8_0"]')?.disabled
    ).toBe(false)
    expect(group('q8_0').textContent).not.toContain('Download')
  })

  it('safely finishes an embedding operation after its component unmounts', async () => {
    const builtIn = snapshot()
    builtIn.settings.embeddingModel = 'built-in'
    builtIn.semanticSearch = { ...builtIn.semanticSearch!, modelState: 'not-downloaded' }
    getSnapshot.mockResolvedValueOnce(builtIn)
    const operation = deferred<KnowledgeSnapshot>()
    download.mockReturnValue(operation.promise)
    await mount()
    await click('Download')
    await click('Permissions')
    await act(async () => operation.reject(new Error('late download error')))
    expect(container.querySelector('[role="alert"]')).toBeNull()
  })

  it('does not multiply snapshot requests on ordinary rerenders', async () => {
    await mount()
    await mount()
    await advance(3_000)
    expect(getSnapshot).toHaveBeenCalledTimes(1)
  })

  it('renders legacy Q8 selection and separate model file rows without offering an implicit download', async () => {
    await ready(snapshot())
    expect(engine().value).toBe('embeddinggemma-2:q8_0')
    expect(group('q8_0').textContent).toContain('Standard (Q8_0) · 310 MB')
    expect(group('q8_0').textContent).toContain('Installed · in use')
    expect(group('q4_k_m').textContent).toContain('Not downloaded')
    expect(
      engine().querySelector<HTMLOptionElement>('option[value="embeddinggemma-2:q4_k_m"]')?.disabled
    ).toBe(true)
    expect(download).not.toHaveBeenCalled()
  })

  it('hides Q8 counts while Q4 prepares and preserves file/chunk/page index status', async () => {
    const next = variantsSnapshot()
    next.settings.embeddingVariant = 'q4_k_m'
    next.semanticSearch!.indexedChunks = 120
    next.semanticSearch!.totalChunks = 800
    await ready(next)
    expect(container.textContent).toContain('Preparing semantic index for Compact (Q4_K_M)…')
    expect(container.textContent).not.toContain('120 / 800')
    expect(container.textContent).toContain('4 files')
    expect(container.textContent).toContain('8 chunks')
    next.semanticSearch!.indexVariant = 'q4_k_m'
    getSnapshot.mockResolvedValue(next)
    await click('Refresh status')
    expect(container.textContent).toContain(
      'Building semantic index · Compact (Q4_K_M) · 120 / 800 chunks'
    )
    expect(container.textContent).toContain("Keyword search covers chunks that aren't rebuilt yet.")
    expect(container.textContent).toContain('4 files')
    expect(container.textContent).toContain('8 chunks')
  })

  it('reports startup failure as installed but off and offers retry start and repair', async () => {
    const next = variantsSnapshot()
    next.settings.embeddingVariant = 'q4_k_m'
    next.semanticSearch!.variants![1]!.failure = {
      stage: 'startup',
      message: 'Runtime unavailable',
    }
    await ready(next)
    expect(container.textContent).not.toContain('Semantic index ·')
    expect(container.textContent).toContain(
      "Semantic search is off: Compact (Q4_K_M) couldn't start."
    )
    expect(container.textContent).toContain('Keyword search is in use.')
    expect(group('q4_k_m').textContent).toContain('Retry start')
    expect(group('q4_k_m').textContent).toContain('Repair')
    expect(container.querySelector('[role="alert"]')?.textContent).toContain(
      "Compact (Q4_K_M) is installed but couldn't start: Runtime unavailable"
    )
    await act(async () => named('Retry start Compact (Q4_K_M)').click())
    expect(setModel).toHaveBeenCalledWith('embeddinggemma-2', 'q4_k_m')
  })

  it('repairs integrity failures without exposing hashes', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants![1] = variant('q4_k_m', {
      state: 'error',
      failure: { stage: 'integrity', message: 'bad hash deadbeef01234567' },
    })
    // Native keeps the legacy error field for old clients; don't leak the same
    // integrity details a second time through that compatibility field.
    next.semanticSearch!.error = 'bad hash deadbeef01234567'
    await ready(next)
    expect(group('q4_k_m').textContent).toContain('File check failed')
    expect(group('q4_k_m').textContent).toContain('Remove')
    expect(container.querySelector('[role="alert"]')?.textContent).toContain(
      "Compact (Q4_K_M) didn't pass the file check. Repair downloads it again."
    )
    expect(container.textContent).not.toContain('deadbeef')
    expect(container.textContent).not.toContain('Retry semantic indexing')
    await act(async () => named('Repair Compact (Q4_K_M)').click())
    expect(download).toHaveBeenCalledWith('q4_k_m')
  })

  it('keeps variant download failures separate and leaves healthy Q8 installed', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants![1] = variant('q4_k_m', {
      state: 'error',
      failure: { stage: 'download', message: 'Offline' },
    })
    await ready(next)
    expect(group('q4_k_m').textContent).toContain('Retry download')
    expect(group('q8_0').textContent).toContain('Installed · in use')
    expect(named('Remove Standard (Q8_0)').disabled).toBe(false)
    expect(container.querySelector('[role="alert"]')?.textContent).toContain(
      'Compact (Q4_K_M) download failed: Offline'
    )
  })

  it('renders separate download and indeterminate verification progress outside live regions', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants = [
      variant('q8_0', { state: 'verifying' }),
      variant('q4_k_m', {
        state: 'downloading',
        downloadProgress: 0.42,
        downloadBytes: 81_000_000,
        downloadTotalBytes: 194_000_000,
      }),
    ]
    await ready(next)
    const progress = container.querySelector('progress[aria-label="Compact (Q4_K_M) download"]')!
    expect(progress.getAttribute('value')).toBe('0.42')
    expect(progress.closest('[role="status"]')).toBeNull()
    expect(
      container
        .querySelector('progress[aria-label="Standard (Q8_0) download"]')
        ?.hasAttribute('value')
    ).toBe(false)
    expect(container.textContent).toContain('Downloading Compact (Q4_K_M) · 42% · 81 / 194 MB')
    expect(container.textContent).toContain('Checking Standard (Q8_0) file…')
    expect(named('Cancel Compact (Q4_K_M) download').disabled).toBe(false)
  })

  it('shows unsupported model files without download actions', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.supported = false
    await ready(next)
    for (const id of ['q8_0', 'q4_k_m'] as const) {
      expect(group(id).textContent).toContain('Not available on this platform')
      expect(group(id).querySelector('button')).toBeNull()
    }
    expect(container.textContent).toContain(
      'The local model runtime is not available for this platform yet.'
    )
  })

  it('selects Q4 explicitly without reindexing extracted text or downloading it', async () => {
    await ready()
    await choose('embeddinggemma-2:q4_k_m')
    expect(setModel).toHaveBeenCalledExactlyOnceWith('embeddinggemma-2', 'q4_k_m')
    expect(startIndexing).not.toHaveBeenCalled()
    expect(download).not.toHaveBeenCalled()
  })

  it('bounds a hung switch, reverts pending selection, and locks until a fresh snapshot is applied', async () => {
    await ready()
    setModel.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
    const reconciliation = deferred<KnowledgeSnapshot>()
    getSnapshot.mockReturnValue(reconciliation.promise)
    await choose('embeddinggemma-2:q4_k_m')
    expect(engine().value).toBe('embeddinggemma-2:q4_k_m')
    await advance(15_000)
    expect(container.textContent).toContain(`didn't confirm "Switch to Compact (Q4_K_M)"`)
    expect(engine().value).toBe('embeddinggemma-2:q8_0')
    expect(engine().disabled).toBe(false)
    expect(engine().getAttribute('aria-disabled')).toBe('true')
    expect(named('Remove Standard (Q8_0)').disabled).toBe(true)
    expect(button('Refresh status').disabled).toBe(false)
    const fresh = variantsSnapshot()
    fresh.settings.embeddingVariant = 'q4_k_m'
    await act(async () => reconciliation.resolve(fresh))
    expect(engine().disabled).toBe(false)
    expect(engine().value).toBe('embeddinggemma-2:q4_k_m')
    expect(container.textContent).not.toContain("didn't confirm")
  })

  it.each(['resolve', 'reject'] as const)(
    'ignores a late switch %s after reconciliation',
    async (outcome) => {
      await ready()
      const operation = deferred<KnowledgeSnapshot>()
      setModel.mockReturnValue(operation.promise)
      await choose('embeddinggemma-2:q4_k_m')
      getSnapshot.mockResolvedValue(variantsSnapshot())
      await advance(15_000)
      await act(async () =>
        outcome === 'resolve'
          ? operation.resolve({
              ...variantsSnapshot(),
              settings: { ...snapshot().settings, embeddingModel: 'built-in' },
            })
          : operation.reject(new Error('obsolete mutation'))
      )
      expect(engine().value).toBe('embeddinggemma-2:q8_0')
      expect(container.querySelector('[role="alert"]')).toBeNull()
      expect(engine().disabled).toBe(false)
    }
  )

  it('retains the lock through a failed reconcile until explicit Refresh succeeds', async () => {
    await ready()
    setModel.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
    getSnapshot.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
    await choose('embeddinggemma-2:q4_k_m')
    await advance(25_000)
    expect(container.textContent).toContain(
      "Couldn't check the current state. Use Refresh status to try again."
    )
    expect(engine().disabled).toBe(false)
    expect(engine().getAttribute('aria-disabled')).toBe('true')
    expect(button('Refresh status').disabled).toBe(false)
    const calls = getSnapshot.mock.calls.length
    await advance(30_000)
    expect(getSnapshot).toHaveBeenCalledTimes(calls)
    getSnapshot.mockResolvedValue(variantsSnapshot())
    await click('Refresh status')
    expect(engine().disabled).toBe(false)
    expect(container.textContent).not.toContain("Couldn't check the current state")
  })

  it('does not unlock on an old poll or a status event during reconciliation', async () => {
    await ready()
    const poll = deferred<KnowledgeSnapshot>()
    getSnapshot.mockReturnValue(poll.promise)
    await advance(1_500)
    setModel.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
    await choose('embeddinggemma-2:q4_k_m')
    const fresh = deferred<KnowledgeSnapshot>()
    getSnapshot.mockReturnValue(fresh.promise)
    await advance(15_000)
    expect(getSnapshot.mock.calls.length).toBeGreaterThanOrEqual(3)
    await act(async () => {
      poll.resolve(variantsSnapshot())
      statusListener?.(snapshot().status)
    })
    expect(engine().disabled).toBe(false)
    expect(engine().getAttribute('aria-disabled')).toBe('true')
    await act(async () => fresh.resolve(variantsSnapshot()))
    expect(engine().disabled).toBe(false)
  })

  it('keeps the triggering download focusable, blocks double activation, and permits Refresh', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants![1]!.state = 'not-downloaded'
    await ready(next)
    download.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
    const trigger = named('Download Compact (Q4_K_M)')
    trigger.focus()
    await act(async () => {
      const enter = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })
      trigger.dispatchEvent(enter)
      expect(enter.defaultPrevented).toBe(false)
      trigger.click()
    })
    expect(document.activeElement).toBe(trigger)
    expect(trigger.disabled).toBe(false)
    expect(trigger.getAttribute('aria-disabled')).toBe('true')
    expect(trigger.textContent).toBe('Starting…')
    await act(async () => {
      trigger.click()
      named('Remove Standard (Q8_0)').click()
    })
    expect(download).toHaveBeenCalledTimes(1)
    expect(removeModel).not.toHaveBeenCalled()
    getSnapshot.mockResolvedValue(next)
    await click('Refresh status')
    expect(getSnapshot).toHaveBeenCalledTimes(2)
  })

  it('keeps row button focus when download response turns Download into Cancel', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants![1]!.state = 'not-downloaded'
    await ready(next)
    const downloading = variantsSnapshot()
    downloading.semanticSearch!.variants![1]!.state = 'downloading'
    download.mockResolvedValue(downloading)
    const trigger = named('Download Compact (Q4_K_M)')
    trigger.focus()
    await act(async () => trigger.click())
    expect(named('Cancel Compact (Q4_K_M) download')).toBe(document.activeElement)
    const select = engine()
    const q8Button = named('Remove Standard (Q8_0)')
    const q4Button = named('Cancel Compact (Q4_K_M) download')
    const radio = button('Slow (light)')
    const refresh = button('Refresh status')
    for (const [a, b] of [
      [select, q8Button],
      [q8Button, q4Button],
      [q4Button, radio],
      [radio, refresh],
    ]) {
      expect(a!.compareDocumentPosition(b!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
      expect(a!.tabIndex).toBeGreaterThanOrEqual(0)
    }
  })

  it('lets Cancel supersede a hung operation and ignores its later result', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants![1]!.state = 'downloading'
    await ready(next)
    const remove = deferred<KnowledgeSnapshot>()
    removeModel.mockReturnValue(remove.promise)
    await act(async () => named('Remove Standard (Q8_0)').click())
    expect(named('Cancel Compact (Q4_K_M) download').disabled).toBe(false)
    const cancellation = deferred<KnowledgeSnapshot>()
    cancelDownload.mockReturnValue(cancellation.promise)
    await act(async () => named('Cancel Compact (Q4_K_M) download').click())
    expect(cancelDownload).toHaveBeenCalledExactlyOnceWith('q4_k_m')
    expect(named('Cancelling Compact (Q4_K_M) download…').textContent).toBe('Cancelling…')
    await act(async () => cancellation.resolve(next))
    await act(async () => remove.resolve(snapshot(0)))
    expect(container.querySelectorAll('[role="switch"]')).toHaveLength(4)
    expect(engine().value).toBe('embeddinggemma-2:q8_0')
  })

  it('removes the in-use variant without selecting the other installed variant', async () => {
    await ready()
    const removed = variantsSnapshot()
    removed.settings.embeddingModel = 'built-in'
    removed.semanticSearch!.variants![0]!.state = 'not-downloaded'
    removeModel.mockResolvedValue(removed)
    await act(async () => named('Remove Standard (Q8_0)').click())
    expect(removeModel).toHaveBeenCalledExactlyOnceWith('q8_0')
    expect(engine().value).toBe('built-in')
    expect(group('q4_k_m').textContent).toContain('Installed')
    expect(setModel).not.toHaveBeenCalled()
  })

  it('drops pending state and late rejection across tab unmount/remount', async () => {
    await ready()
    const operation = deferred<KnowledgeSnapshot>()
    setModel.mockReturnValue(operation.promise)
    await choose('embeddinggemma-2:q4_k_m')
    expect(container.textContent).toContain('Switching to Compact (Q4_K_M)…')
    await click('Permissions')
    getSnapshot.mockResolvedValue(variantsSnapshot())
    await click('Knowledge')
    expect(container.textContent).not.toContain('Switching to Compact (Q4_K_M)…')
    await act(async () => operation.reject(new Error('previous mount')))
    expect(container.querySelector('[role="alert"]')).toBeNull()
    expect(engine().disabled).toBe(false)
  })

  it('pauses hidden polling and refreshes exactly once on becoming visible', async () => {
    await ready()
    getSnapshot.mockResolvedValue(variantsSnapshot())
    Object.defineProperty(document, 'hidden', { configurable: true, value: true })
    await act(async () => document.dispatchEvent(new Event('visibilitychange')))
    await advance(9_000)
    expect(getSnapshot).toHaveBeenCalledTimes(1)
    Object.defineProperty(document, 'hidden', { configurable: true, value: false })
    await act(async () => document.dispatchEvent(new Event('visibilitychange')))
    expect(getSnapshot).toHaveBeenCalledTimes(2)
    await advance(100)
    expect(getSnapshot).toHaveBeenCalledTimes(2)
  })

  it('keeps no-snapshot rows nonactionable while Refresh remains available', async () => {
    await mount()
    expect(engine().value).toBe('')
    expect(engine().disabled).toBe(true)
    expect(container.querySelectorAll('[role="group"] button')).toHaveLength(0)
    expect(button('Refresh status').disabled).toBe(false)
  })

  it('keeps unknown-outcome protection when Cancel supersedes reconciliation', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants![1]!.state = 'downloading'
    await ready(next)
    removeModel.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
    getSnapshot.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
    await act(async () => named('Remove Standard (Q8_0)').click())
    await advance(15_000)
    cancelDownload.mockResolvedValue(next)
    await act(async () => named('Cancel Compact (Q4_K_M) download').click())
    expect(engine().disabled).toBe(true)
    expect(named('Remove Standard (Q8_0)').disabled).toBe(true)
    getSnapshot.mockResolvedValue(next)
    await click('Refresh status')
    expect(engine().disabled).toBe(false)
  })

  it('retains explicit retry after visibility returns from a failed snapshot read', async () => {
    await ready()
    getSnapshot.mockRejectedValue(new Error('Offline snapshot'))
    await click('Refresh status')
    const calls = getSnapshot.mock.calls.length
    Object.defineProperty(document, 'hidden', { configurable: true, value: true })
    await act(async () => document.dispatchEvent(new Event('visibilitychange')))
    Object.defineProperty(document, 'hidden', { configurable: true, value: false })
    await act(async () => document.dispatchEvent(new Event('visibilitychange')))
    await advance(5_000)
    expect(getSnapshot).toHaveBeenCalledTimes(calls)
  })

  it('does not show a healthy semantic index for a selected missing or failed variant', async () => {
    const next = variantsSnapshot()
    next.settings.embeddingVariant = 'q4_k_m'
    next.semanticSearch!.indexVariant = 'q4_k_m'
    next.semanticSearch!.indexedChunks = 800
    next.semanticSearch!.totalChunks = 800
    next.semanticSearch!.indexing = false
    next.semanticSearch!.variants![1]!.state = 'not-downloaded'
    await ready(next)
    expect(engine().value).toBe('embeddinggemma-2:q4_k_m')
    expect(container.textContent).toContain(
      "Semantic search is off: Compact (Q4_K_M) isn't installed."
    )
    expect(container.textContent).not.toContain('Semantic index ·')
    const legacy = snapshot()
    legacy.semanticSearch!.indexing = false
    legacy.semanticSearch!.indexedChunks = 8
    legacy.semanticSearch!.error = 'Cannot use this file'
    getSnapshot.mockResolvedValue(legacy)
    await click('Refresh status')
    expect(container.textContent).not.toContain('Semantic index ·')
  })

  it('does not treat an unrelated mutation snapshot as a reconciliation read', async () => {
    await ready()
    setModel.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
    getSnapshot.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
    await choose('embeddinggemma-2:q4_k_m')
    await advance(15_000)
    await act(async () =>
      container
        .querySelector<HTMLButtonElement>('[aria-label="Default Knowledge Depth"] button')!
        .click()
    )
    expect(engine().disabled).toBe(false)
    expect(engine().getAttribute('aria-disabled')).toBe('true')
    getSnapshot.mockResolvedValue(variantsSnapshot())
    await click('Refresh status')
    expect(engine().disabled).toBe(false)
  })

  it('preserves pause and resume indexing without starting text indexing on version selection', async () => {
    const next = variantsSnapshot()
    next.settings.embeddingVariant = 'q4_k_m'
    next.semanticSearch!.indexVariant = 'q4_k_m'
    next.status.state = 'paused'
    await ready(next)
    expect(container.textContent).toContain(
      'Semantic indexing paused · Compact (Q4_K_M) · 4 / 8 chunks'
    )
    expect(container.textContent).toContain(
      'Keyword search stays available. Resume indexing to continue.'
    )
    expect(container.textContent).not.toContain('Start semantic indexing')
    await click('Resume indexing')
    expect(container.textContent).toContain('Building semantic index')
    await click('Pause indexing')
    expect(container.textContent).toContain('Semantic indexing paused')
    expect(startIndexing).not.toHaveBeenCalled()
  })

  it.each(['download', 'remove', 'cancel', 'speed'] as const)(
    'bounds a hung %s and ignores late rejection after timeout',
    async (kind) => {
      const next = variantsSnapshot()
      next.semanticSearch!.variants![1]!.state =
        kind === 'cancel' ? 'downloading' : 'not-downloaded'
      await ready(next)
      const operation = deferred<KnowledgeSnapshot>()
      getSnapshot.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
      const invoke =
        kind === 'download'
          ? download
          : kind === 'remove'
            ? removeModel
            : kind === 'cancel'
              ? cancelDownload
              : setSpeed
      invoke.mockReturnValue(operation.promise)
      await act(async () => {
        if (kind === 'speed') button('Slow (light)').click()
        else
          named(
            kind === 'download'
              ? 'Download Compact (Q4_K_M)'
              : kind === 'remove'
                ? 'Remove Standard (Q8_0)'
                : 'Cancel Compact (Q4_K_M) download'
          ).click()
      })
      await advance(15_000)
      expect(container.textContent).toContain('Checking the current state…')
      expect(engine().disabled).toBe(true)
      await act(async () => operation.reject(new Error('late failure')))
      expect(container.textContent).not.toContain('late failure')
      expect(engine().disabled).toBe(true)
    }
  )

  it('enables the available variant with Standard preference and no download', async () => {
    const next = variantsSnapshot()
    next.settings.embeddingModel = 'built-in'
    next.semanticSearch!.variants![0]!.state = 'not-downloaded'
    await ready(next)
    await act(async () => named('Enable semantic search with Compact (Q4_K_M)').click())
    expect(setModel).toHaveBeenLastCalledWith('embeddinggemma-2', 'q4_k_m')
    getSnapshot.mockResolvedValue({
      ...variantsSnapshot(),
      settings: { ...next.settings, embeddingModel: 'built-in' },
    })
    await click('Refresh status')
    await act(async () => named('Enable semantic search with Standard (Q8_0)').click())
    expect(setModel).toHaveBeenLastCalledWith('embeddinggemma-2', 'q8_0')
    expect(download).not.toHaveBeenCalled()
    expect(startIndexing).not.toHaveBeenCalled()
  })

  it('orders local operation and independent persistent variant failure alerts', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants![0]!.failure = { stage: 'startup', message: 'Q8 runtime issue' }
    next.semanticSearch!.variants![1] = variant('q4_k_m', {
      state: 'error',
      failure: { stage: 'download', message: 'Q4 offline' },
    })
    await ready(next)
    setModel.mockRejectedValue(new Error('Cannot start now'))
    await act(async () => named('Retry start Standard (Q8_0)').click())
    const lines = Array.from(container.querySelectorAll('[role="alert"] p'), (p) => p.textContent)
    expect(lines).toEqual([
      "Couldn't switch to Standard (Q8_0): Cannot start now",
      "Standard (Q8_0) is installed but couldn't start: Q8 runtime issue",
      'Compact (Q4_K_M) download failed: Q4 offline',
    ])
    expect(engine().value).toBe('embeddinggemma-2:q8_0')
    expect(engine().disabled).toBe(false)
  })

  it('uses native legacy failure attribution for repair, startup retry, and unattributed alerts', async () => {
    const damaged = snapshot()
    damaged.semanticSearch = {
      ...damaged.semanticSearch!,
      modelState: 'error',
      errorSource: 'integrity',
      error: 'Installed model failed SHA-256 verification.',
    }
    await ready(damaged)
    expect(group('q8_0').textContent).toContain('File check failed')
    expect(container.textContent).not.toMatch(/SHA-?256|hash/i)
    expect(container.querySelector('[role="alert"]')?.textContent).toBe(
      "Standard (Q8_0) didn't pass the file check. Repair downloads it again."
    )
    expect(engine().selectedOptions[0]!.textContent).toContain(' — needs repair')
    expect(named('Remove Standard (Q8_0)')).toBeDefined()
    await act(async () => named('Repair Standard (Q8_0)').click())
    expect(download).toHaveBeenCalledExactlyOnceWith('q8_0')

    const startup = snapshot()
    startup.semanticSearch = {
      ...startup.semanticSearch!,
      errorSource: 'runtime',
      error: 'spawn failed: ENOENT',
    }
    getSnapshot.mockResolvedValue(startup)
    await click('Refresh status')
    expect(group('q8_0').textContent).toContain("Installed · couldn't start")
    expect(engine().selectedOptions[0]!.textContent).toContain(" — couldn't start")
    expect(container.textContent).toContain(
      "Semantic search is off: Standard (Q8_0) couldn't start."
    )
    expect(container.querySelector('[role="alert"]')?.textContent).toBe(
      "Standard (Q8_0) is installed but couldn't start: spawn failed: ENOENT"
    )
    expect(container.textContent).not.toContain('download failed')
    expect(container.textContent).not.toContain('Retry semantic indexing')
    expect(named('Repair Standard (Q8_0)')).toBeDefined()
    await act(async () => named('Retry start Standard (Q8_0)').click())
    expect(setModel).toHaveBeenCalledExactlyOnceWith('embeddinggemma-2', 'q8_0')

    const unattributed = snapshot()
    unattributed.semanticSearch = {
      ...unattributed.semanticSearch!,
      indexing: false,
      indexedChunks: 8,
      error: 'Index unavailable',
    }
    getSnapshot.mockResolvedValue(unattributed)
    await click('Refresh status')
    expect(container.querySelector('[role="alert"]')?.textContent).toBe('Index unavailable')
    expect(container.textContent).not.toContain('Semantic index ·')
    expect(button('Retry semantic indexing')).toBeDefined()
  })

  function startupVerification(): KnowledgeSnapshot {
    const next = snapshot()
    // Native new_for_variant + initialize, before the installed file check finishes.
    // Object.assign keeps this regression runnable before adding the shared field.
    Object.assign(next.semanticSearch!, {
      modelState: 'not-downloaded',
      integrityState: 'verifying',
      weightBytes: 309_855_456,
      downloadProgress: 0,
      downloadBytes: 0,
      downloadTotalBytes: 321_867_411,
      indexing: false,
      indexedChunks: 800,
      totalChunks: 800,
      error: null,
    })
    return next
  }

  it('shows legacy startup verification truthfully until the installed Q8 check completes (N1)', async () => {
    await ready(startupVerification())
    expect(group('q8_0').textContent).toContain('Checking file…')
    expect(group('q8_0').textContent).not.toContain('Not downloaded')
    expect(engine().selectedOptions[0]!.textContent).toContain(' — verifying')
    expect(container.textContent).toContain('Checking Standard (Q8_0) installation…')
    expect(container.textContent).toContain('Keyword search is in use.')
    expect(container.textContent).not.toContain("isn't installed")
    expect(group('q8_0').querySelector('button')).toBeNull()
    expect(container.querySelector('progress')?.hasAttribute('value')).toBe(false)

    const checked = snapshot()
    Object.assign(checked.semanticSearch!, { integrityState: 'verified', indexedChunks: 8 })
    getSnapshot.mockResolvedValueOnce(checked)
    await click('Refresh status')
    expect(group('q8_0').textContent).toContain('Installed · in use')
    expect(engine().selectedOptions[0]!.textContent).not.toContain(' — verifying')
    expect(download).not.toHaveBeenCalled()
    expect(setModel).not.toHaveBeenCalled()
    expect(startIndexing).not.toHaveBeenCalled()
  })

  it('offers no legacy Q8 download or cancel capable of interrupting startup verification (N1b)', async () => {
    await ready(startupVerification())
    // Exercise any erroneously exposed action before asserting the native boundary.
    await act(async () =>
      group('q8_0')
        .querySelectorAll('button')
        .forEach((b) => b.click())
    )
    expect(download).not.toHaveBeenCalled()
    expect(cancelDownload).not.toHaveBeenCalled()
    expect(removeModel).not.toHaveBeenCalled()
  })

  it.each(['q8_0', 'q4_k_m'] as const)(
    'hides Cancel during structured %s verification and uses truthful installation copy',
    async (id) => {
      const next = variantsSnapshot()
      next.settings.embeddingVariant = id
      next.semanticSearch!.variants!.find((v) => v.id === id)!.state = 'verifying'
      await ready(next)
      expect(group(id).textContent).toContain('Checking file…')
      expect(group(id).querySelector('button')).toBeNull()
      expect(container.textContent).toContain(
        `Checking ${id === 'q8_0' ? 'Standard (Q8_0)' : 'Compact (Q4_K_M)'} installation…`
      )
      expect(container.textContent).not.toContain("isn't installed")
    }
  )

  it('keeps actual legacy downloading cancellable even when integrityState is verifying', async () => {
    const next = startupVerification()
    next.semanticSearch!.modelState = 'downloading'
    await ready(next)
    expect(group('q8_0').textContent).toContain('Downloading · 0%')
    await act(async () => named('Cancel Standard (Q8_0) download').click())
    expect(cancelDownload).toHaveBeenCalledExactlyOnceWith('q8_0')
  })

  it.each(['enable', 'retry-index', 'start-index'] as const)(
    'recovers %s focus to Refresh in its own action row, never Remove (N2/N2b/N2c)',
    async (action) => {
      const next = snapshot()
      if (action === 'enable') next.settings.embeddingModel = 'built-in'
      if (action === 'retry-index') next.semanticSearch!.error = 'Index unavailable'
      if (action === 'start-index') next.semanticSearch!.indexing = false
      await ready(next)
      const operation = deferred<KnowledgeSnapshot>()
      setModel.mockReturnValueOnce(operation.promise)
      const trigger =
        action === 'enable'
          ? named('Enable semantic search with Standard (Q8_0)')
          : button(action === 'retry-index' ? 'Retry semantic indexing' : 'Start semantic indexing')
      const ownRow = trigger.parentElement!
      trigger.focus()
      await act(async () => trigger.click())
      expect(document.activeElement).toBe(trigger)
      await act(async () => operation.resolve(snapshot()))
      expect(trigger.isConnected).toBe(false)
      expect(document.activeElement).toBe(button('Refresh status'))
      expect(ownRow.contains(document.activeElement)).toBe(true)
      getSnapshot.mockResolvedValueOnce(snapshot())
      await act(async () => (document.activeElement as HTMLElement).click())
      expect(removeModel).not.toHaveBeenCalled()
      expect(getSnapshot).toHaveBeenCalledTimes(2)
    }
  )

  it('moves Retry start focus away when the same DOM button becomes destructive Remove', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants![1]!.failure = { stage: 'startup', message: 'spawn failed' }
    await ready(next)
    setModel.mockResolvedValueOnce(variantsSnapshot())
    const trigger = named('Retry start Compact (Q4_K_M)')
    trigger.focus()
    await act(async () => trigger.click())
    expect(group('q4_k_m').contains(document.activeElement)).toBe(true)
    expect(document.activeElement).not.toBe(named('Remove Compact (Q4_K_M)'))
    expect(document.activeElement).toBe(group('q4_k_m'))
    await act(async () => (document.activeElement as HTMLElement).click())
    expect(removeModel).not.toHaveBeenCalled()
  })

  it.each(['body', 'refresh'] as const)(
    'does not reclaim deliberate focus on %s when Retry start morphs into Remove',
    async (destination) => {
      const next = variantsSnapshot()
      next.semanticSearch!.variants![1]!.failure = { stage: 'startup', message: 'spawn failed' }
      await ready(next)
      const operation = deferred<KnowledgeSnapshot>()
      setModel.mockReturnValueOnce(operation.promise)
      const trigger = named('Retry start Compact (Q4_K_M)')
      trigger.focus()
      await act(async () => trigger.click())
      if (destination === 'body') trigger.blur()
      else button('Refresh status').focus()
      await act(async () => operation.resolve(variantsSnapshot()))
      expect(document.activeElement).toBe(
        destination === 'body' ? document.body : button('Refresh status')
      )
    }
  )

  it('recovers a disappearing Repair to its own verifying row when no safe button exists', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants![1]!.failure = { stage: 'startup', message: 'spawn failed' }
    await ready(next)
    const checked = variantsSnapshot()
    checked.semanticSearch!.variants![1]!.state = 'verifying'
    download.mockResolvedValueOnce(checked)
    const trigger = named('Repair Compact (Q4_K_M)')
    trigger.focus()
    await act(async () => trigger.click())
    expect(document.activeElement).toBe(group('q4_k_m'))
    expect(group('q4_k_m').querySelector('button')).toBeNull()
  })

  it('sanitizes an explicitly attributed top-level integrity error with a different variant message', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants![0] = variant('q8_0', {
      state: 'error',
      failure: { stage: 'integrity', message: 'File SHA-256 mismatch' },
    })
    next.semanticSearch!.errorSource = 'integrity'
    next.semanticSearch!.error = 'Installation failed SHA-256 verification'
    await ready(next)
    expect(container.querySelector('[role="alert"]')?.textContent).toBe(
      "Standard (Q8_0) didn't pass the file check. Repair downloads it again."
    )
    expect(container.textContent).not.toContain('SHA-256')
  })

  it('holds a fresh-read barrier when Cancel supersedes a still-running Remove', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants![1]!.state = 'downloading'
    await ready(next)
    const removal = deferred<KnowledgeSnapshot>()
    const read = deferred<KnowledgeSnapshot>()
    removeModel.mockReturnValue(removal.promise)
    await act(async () => named('Remove Standard (Q8_0)').click())
    getSnapshot.mockReturnValue(read.promise)
    await act(async () => named('Cancel Compact (Q4_K_M) download').click())
    expect(getSnapshot).toHaveBeenCalledTimes(2)
    expect(engine().disabled).toBe(true)
    expect(container.textContent).toContain(`didn't confirm "Remove Standard (Q8_0)"`)
    await act(async () => removal.resolve(snapshot(0)))
    expect(container.querySelectorAll('[role="switch"]')).toHaveLength(4)
    expect(engine().disabled).toBe(true)
    await act(async () => read.resolve(next))
    expect(engine().disabled).toBe(false)
  })

  it('keeps a switch trigger focusable and inert through pending, timeout, and failed reconciliation', async () => {
    await ready()
    setModel.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
    getSnapshot.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
    const select = engine()
    select.focus()
    await choose('embeddinggemma-2:q4_k_m')
    // jsdom lacks browser focus fixup when disabled: check both attributes and focus.
    expect(select.disabled).toBe(false)
    expect(select.getAttribute('aria-disabled')).toBe('true')
    await choose('built-in')
    expect(setModel).toHaveBeenCalledTimes(1)
    expect(select.value).toBe('embeddinggemma-2:q4_k_m')
    const key = new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true })
    select.dispatchEvent(key)
    expect(key.defaultPrevented).toBe(true)
    await advance(25_000)
    expect(select.disabled).toBe(false)
    expect(select.getAttribute('aria-disabled')).toBe('true')
    expect(document.activeElement).toBe(select)
    await choose('built-in')
    expect(setModel).toHaveBeenCalledTimes(1)
    expect(select.value).toBe('embeddinggemma-2:q8_0')
  })

  it('keeps a focused Download inert and focusable through timeout and an unconfirmed refresh', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants![1]!.state = 'not-downloaded'
    await ready(next)
    download.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
    const read = deferred<KnowledgeSnapshot>()
    getSnapshot.mockReturnValue(read.promise)
    const trigger = named('Download Compact (Q4_K_M)')
    trigger.focus()
    await act(async () => trigger.click())
    await advance(25_000)
    expect(trigger.disabled).toBe(false)
    expect(trigger.getAttribute('aria-disabled')).toBe('true')
    expect(document.activeElement).toBe(trigger)
    await act(async () => trigger.click())
    expect(download).toHaveBeenCalledTimes(1)
    await click('Refresh status')
    expect(container.textContent).toContain('Checking the current state…')
    expect(container.textContent).not.toContain("Couldn't check the current state.")
    expect(trigger.disabled).toBe(false)
    await act(async () => read.resolve(next))
    expect(trigger.getAttribute('aria-disabled')).toBeNull()
  })

  it.each(['repair', 'remove'] as const)(
    'returns focus to the same model row when its focused %s control disappears',
    async (action) => {
      const next = variantsSnapshot()
      next.semanticSearch!.variants![1] = variant(
        'q4_k_m',
        action === 'repair'
          ? { failure: { stage: 'startup', message: 'spawn failed' } }
          : {
              state: 'error',
              downloadBytes: 50,
              failure: { stage: 'download', message: 'offline' },
            }
      )
      await ready(next)
      const result = variantsSnapshot()
      result.semanticSearch!.variants![1]!.state =
        action === 'repair' ? 'downloading' : 'not-downloaded'
      const operation = deferred<KnowledgeSnapshot>()
      ;(action === 'repair' ? download : removeModel).mockReturnValue(operation.promise)
      const trigger = named(`${action === 'repair' ? 'Repair' : 'Remove'} Compact (Q4_K_M)`)
      trigger.focus()
      await act(async () => trigger.click())
      await act(async () => operation.resolve(result))
      expect(trigger.isConnected).toBe(false)
      expect(group('q4_k_m').contains(document.activeElement)).toBe(true)
      expect((document.activeElement as HTMLButtonElement).disabled).toBe(false)
    }
  )

  it.each(['refresh', 'body'] as const)(
    'does not steal focus when the user moves to %s before its trigger disappears',
    async (destination) => {
      const next = variantsSnapshot()
      next.semanticSearch!.variants![1]!.failure = { stage: 'startup', message: 'spawn failed' }
      await ready(next)
      const operation = deferred<KnowledgeSnapshot>()
      download.mockReturnValue(operation.promise)
      const trigger = named('Repair Compact (Q4_K_M)')
      trigger.focus()
      await act(async () => trigger.click())
      if (destination === 'refresh') button('Refresh status').focus()
      else trigger.blur()
      const expectedFocus = document.activeElement
      const result = variantsSnapshot()
      result.semanticSearch!.variants![1]!.state = 'downloading'
      await act(async () => operation.resolve(result))
      expect(document.activeElement).toBe(expectedFocus)
    }
  )

  it('announces identical local and persisted download failures once in local-first order', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants![1]!.state = 'not-downloaded'
    await ready(next)
    download.mockRejectedValue(new Error('disk full'))
    await act(async () => named('Download Compact (Q4_K_M)').click())
    const failed = variantsSnapshot()
    failed.semanticSearch!.variants![0]!.failure = { stage: 'startup', message: 'spawn failed' }
    failed.semanticSearch!.variants![1] = variant('q4_k_m', {
      state: 'error',
      failure: { stage: 'download', message: 'disk full' },
    })
    getSnapshot.mockResolvedValue(failed)
    await advance(1_500)
    expect(
      Array.from(container.querySelectorAll('[role="alert"] p'), (p) => p.textContent)
    ).toEqual([
      'Compact (Q4_K_M) download failed: disk full',
      "Standard (Q8_0) is installed but couldn't start: spawn failed",
    ])
  })

  it('uses legacy model weights rather than runtime-inclusive totals, with an optional authoritative size', async () => {
    const next = snapshot()
    next.semanticSearch!.downloadTotalBytes = 321_867_411
    await ready(next)
    expect(group('q8_0').textContent).toContain('310 MB')
    expect(group('q4_k_m').textContent).toContain('182 MB')
    getSnapshot.mockResolvedValue({
      ...next,
      semanticSearch: { ...next.semanticSearch!, weightBytes: 311_600_000 },
    })
    await click('Refresh status')
    expect(group('q8_0').textContent).toContain('312 MB')
  })

  it('keeps changing chunk counts outside the semantic transition live region', async () => {
    await ready()
    const host = engine().closest('.space-y-3')!
    const announced = () =>
      Array.from(host.querySelectorAll('[role="status"]'), (node) => node.textContent)
    const initial = announced()
    expect(initial.join('')).toContain('Building semantic index · Standard (Q8_0)')
    expect(initial.join('')).not.toContain('4 / 8')
    const next = variantsSnapshot()
    next.semanticSearch!.indexedChunks = 5
    getSnapshot.mockResolvedValue(next)
    await advance(1_500)
    expect(host.textContent).toContain('Building semantic index · Standard (Q8_0) · 5 / 8 chunks')
    expect(announced()).toEqual(initial)
  })

  it('keeps incomplete progress below 100 percent and hides unknown byte totals', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants![1] = variant('q4_k_m', {
      state: 'downloading',
      downloadProgress: 0.9999,
      downloadTotalBytes: 0,
    })
    await ready(next)
    expect(group('q4_k_m').textContent).toContain('Downloading · 99%')
    expect(container.textContent).toContain('Downloading Compact (Q4_K_M) · 99%')
    expect(container.textContent).not.toContain('0 / 0 MB')
    next.semanticSearch!.variants![1]!.downloadProgress = 1
    getSnapshot.mockResolvedValue(next)
    await click('Refresh status')
    expect(group('q4_k_m').textContent).toContain('Downloading · 99%')
  })

  it('omits the empty model-files heading before a snapshot arrives', async () => {
    await mount()
    expect(container.textContent).not.toContain('Model files')
  })

  it.each(['download', 'remove', 'cancel'] as const)(
    'includes the visible pending %s label and variant in its accessible name',
    async (kind) => {
      const next = variantsSnapshot()
      next.semanticSearch!.variants![1]!.state =
        kind === 'cancel' ? 'downloading' : 'not-downloaded'
      await ready(next)
      const operation = deferred<KnowledgeSnapshot>()
      ;(kind === 'download'
        ? download
        : kind === 'remove'
          ? removeModel
          : cancelDownload
      ).mockReturnValue(operation.promise)
      const trigger = named(
        kind === 'download'
          ? 'Download Compact (Q4_K_M)'
          : kind === 'remove'
            ? 'Remove Standard (Q8_0)'
            : 'Cancel Compact (Q4_K_M) download'
      )
      await act(async () => trigger.click())
      const verb = kind === 'download' ? 'Starting' : kind === 'remove' ? 'Removing' : 'Cancelling'
      expect(trigger.getAttribute('aria-label')).toContain(verb)
      expect(trigger.getAttribute('aria-label')).toContain(
        kind === 'remove' ? 'Standard (Q8_0)' : 'Compact (Q4_K_M)'
      )
      expect(container.textContent).toContain(
        kind === 'download'
          ? 'Starting Compact (Q4_K_M) download…'
          : kind === 'remove'
            ? 'Removing Standard (Q8_0)…'
            : 'Cancelling Compact (Q4_K_M) download…'
      )
    }
  )

  it('requires a post-timeout read when an earlier read completes before React commits it', async () => {
    await ready()
    Object.defineProperty(document, 'hidden', { configurable: true, value: true })
    setModel.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
    await choose('embeddinggemma-2:q4_k_m')
    await advance(14_000)
    const beforeTimeout = deferred<KnowledgeSnapshot>()
    const afterTimeout = deferred<KnowledgeSnapshot>()
    getSnapshot.mockReturnValueOnce(beforeTimeout.promise).mockReturnValue(afterTimeout.promise)
    await click('Refresh status')
    await act(async () => {
      beforeTimeout.resolve(variantsSnapshot())
      // Drain bridge/read continuations, holding React's commit until this act ends.
      for (let turn = 0; turn < 8; turn++) await Promise.resolve()
      vi.advanceTimersByTime(1_000)
    })
    expect(getSnapshot).toHaveBeenCalledTimes(3)
    expect(engine().getAttribute('aria-disabled')).toBe('true')
    expect(named('Remove Standard (Q8_0)').disabled).toBe(true)
    await act(async () => afterTimeout.resolve(variantsSnapshot()))
    expect(engine().getAttribute('aria-disabled')).toBeNull()
  })

  it('keeps a speed trigger focusable through unknown outcome without sending a duplicate request', async () => {
    await ready()
    setSpeed.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
    getSnapshot.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
    const trigger = button('Slow (light)')
    trigger.focus()
    await act(async () => trigger.click())
    await advance(25_000)
    expect(trigger.disabled).toBe(false)
    expect(trigger.getAttribute('aria-disabled')).toBe('true')
    expect(document.activeElement).toBe(trigger)
    await act(async () => trigger.click())
    expect(setSpeed).toHaveBeenCalledTimes(1)
  })

  it('names a pending startup retry with its visible Starting label', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants![0]!.failure = { stage: 'startup', message: 'spawn failed' }
    await ready(next)
    setModel.mockReturnValue(deferred<KnowledgeSnapshot>().promise)
    const trigger = named('Retry start Standard (Q8_0)')
    await act(async () => trigger.click())
    expect(trigger.textContent).toBe('Starting…')
    expect(trigger.getAttribute('aria-label')).toBe('Starting Standard (Q8_0)…')
  })

  it('uses backend weightBytes for row sizes rather than total download bytes', async () => {
    const next = variantsSnapshot()
    next.semanticSearch!.variants![0]!.weightBytes = 311_600_000
    next.semanticSearch!.variants![0]!.downloadTotalBytes = 324_000_000
    next.semanticSearch!.variants![1]!.weightBytes = 183_600_000
    next.semanticSearch!.variants![1]!.downloadTotalBytes = 196_000_000
    await ready(next)
    expect(group('q8_0').textContent).toContain('312 MB')
    expect(group('q4_k_m').textContent).toContain('184 MB')
    expect(group('q8_0').textContent).not.toContain('324 MB')
    expect(group('q4_k_m').textContent).not.toContain('196 MB')
  })
})

describe('Knowledge embedding IPC variant contract (real Tauri bridge)', () => {
  beforeEach(() => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} })
    vi.mocked(invoke).mockClear()
    initTauriBridge()
  })
  afterEach(() => {
    Reflect.deleteProperty(window, '__TAURI_INTERNALS__')
    Reflect.deleteProperty(window, 'tezbar')
    Reflect.deleteProperty(window, '__TEZBAR_WINDOW_LABEL__')
  })

  it.each([undefined, 'q8_0', 'q4_k_m'] as const)(
    'passes model as arg0 and optional variant %s as arg1',
    async (variant) => {
      await window.tezbar.setKnowledgeEmbeddingModel('embeddinggemma-2', variant)
      expect(invoke).toHaveBeenCalledExactlyOnceWith('call_backend', {
        channel: 'knowledge:embedding:set',
        payload: variant === undefined ? ['embeddinggemma-2'] : ['embeddinggemma-2', variant],
      })
    }
  )

  it.each([
    ['downloadKnowledgeEmbeddingModel', 'download'],
    ['cancelKnowledgeEmbeddingDownload', 'cancel-download'],
    ['removeKnowledgeEmbeddingModel', 'remove'],
  ] as const)(
    'preserves no-args %s and sends explicit variant as arg0',
    async (method, channel) => {
      await window.tezbar[method]()
      expect(invoke).toHaveBeenLastCalledWith('call_backend', {
        channel: `knowledge:embedding:${channel}`,
        payload: [],
      })
      await window.tezbar[method]('q4_k_m')
      expect(invoke).toHaveBeenLastCalledWith('call_backend', {
        channel: `knowledge:embedding:${channel}`,
        payload: ['q4_k_m'],
      })
      await window.tezbar[method]('q8_0')
      expect(invoke).toHaveBeenLastCalledWith('call_backend', {
        channel: `knowledge:embedding:${channel}`,
        payload: ['q8_0'],
      })
    }
  )
})
