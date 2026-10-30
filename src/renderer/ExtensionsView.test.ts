import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cloneElement, isValidElement, type ReactElement, type ReactNode } from 'react'
import type { ExtensionManifest } from '../shared/extensions'
import ExtensionsView from './ExtensionsView'

const hooks = vi.hoisted(() => ({ current: null as HookHarness | null }))
const images = vi.hoisted(() => ({ cached: vi.fn(), release: vi.fn() }))

vi.mock('./extensionImages', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./extensionImages')>()),
  acquireExtensionImage: (...args: unknown[]) => ({
    source: images.cached(...args),
    release: images.release,
  }),
}))
vi.mock('react', async (importOriginal) => ({
  ...(await importOriginal<typeof import('react')>()),
  useState: (initial: unknown) => hooks.current!.state(initial),
  useReducer: (reducer: (state: unknown, action: unknown) => unknown, initial: unknown) => {
    const [state, setState] = hooks.current!.state(initial)
    return [state, (action: unknown) => setState((previous: unknown) => reducer(previous, action))]
  },
  useRef: (initial: unknown) => hooks.current!.state({ current: initial })[0],
  useMemo: (factory: () => unknown, deps: unknown[]) => hooks.current!.memo(factory, deps),
  useCallback: (callback: unknown, deps: unknown[]) => hooks.current!.memo(() => callback, deps),
  useEffect: (effect: Effect, deps: unknown[]) => hooks.current!.effect(effect, deps),
}))

type Effect = () => void | (() => void)
type EffectSlot = { deps: unknown[]; cleanup?: () => void }

// Node-only hook driver: execute the actual component functions, preserve state
// across renders, and explicitly separate render from passive-effect commit.
// No JSX/image/selection implementation is copied into this harness.
class HookHarness {
  private slots: unknown[] = []
  private index = 0
  private pending: Array<() => void> = []
  private cleanups = new Set<() => void>()

  state(initial: unknown): [unknown, (next: unknown) => void] {
    const index = this.index++
    if (!(index in this.slots))
      this.slots[index] = typeof initial === 'function' ? initial() : initial
    return [
      this.slots[index],
      (next) => {
        this.slots[index] = typeof next === 'function' ? next(this.slots[index]) : next
      },
    ]
  }

  memo(factory: () => unknown, deps: unknown[]): unknown {
    const index = this.index++
    const previous = this.slots[index] as { deps: unknown[]; value: unknown } | undefined
    if (
      previous &&
      deps.length === previous.deps.length &&
      deps.every((dep, i) => Object.is(dep, previous.deps[i]))
    )
      return previous.value
    const value = factory()
    this.slots[index] = { deps, value }
    return value
  }

  effect(effect: Effect, deps: unknown[]): void {
    const index = this.index++
    const previous = this.slots[index] as EffectSlot | undefined
    if (
      previous &&
      deps.length === previous.deps.length &&
      deps.every((dep, i) => Object.is(dep, previous.deps[i]))
    )
      return
    this.pending.push(() => {
      previous?.cleanup?.()
      if (previous?.cleanup) this.cleanups.delete(previous.cleanup)
      const cleanup = effect() || undefined
      this.slots[index] = { deps, cleanup }
      if (cleanup) this.cleanups.add(cleanup)
    })
  }

  render(render: () => ReactNode): ReactNode {
    this.index = 0
    // Only the latest render's effects can commit, including when an async
    // state update arrives between rendering new props and the effect flush.
    this.pending = []
    hooks.current = this
    try {
      return render()
    } finally {
      hooks.current = null
    }
  }

  commit(): void {
    const pending = this.pending.splice(0)
    for (const effect of pending) effect()
  }

  unmount(): void {
    for (const cleanup of this.cleanups) cleanup()
    this.cleanups.clear()
  }
}

const mounted: HookHarness[] = []
function mount(): HookHarness {
  const harness = new HookHarness()
  mounted.push(harness)
  return harness
}

type Element = ReactElement<Record<string, unknown>>
function elements(node: ReactNode): Element[] {
  if (Array.isArray(node)) return node.flatMap(elements)
  if (!isValidElement<Record<string, unknown>>(node)) return []
  return [node, ...elements(node.props.children as ReactNode)]
}
function find(node: ReactNode, predicate: (element: Element) => boolean): Element {
  const result = elements(node).find(predicate)
  expect(result, 'element must exist in the actual component output').toBeDefined()
  return result!
}
function renderElement(harness: HookHarness, element: Element): ReactNode {
  if (typeof element.type !== 'function') throw new Error('Expected a function component')
  const Component = element.type as (props: Record<string, unknown>) => ReactNode
  return harness.render(() => Component(element.props))
}
function icon(node: ReactNode, name: string, size?: string): Element {
  return find(node, (element) => {
    const ext = element.props.ext as ExtensionManifest | undefined
    return ext?.name === name && element.props.size === size
  })
}
function cachedImage(node: ReactNode): Element {
  return find(
    node,
    (element) => typeof element.type === 'function' && element.props.variant === 'icon'
  )
}
function imgSource(node: ReactNode): unknown {
  return find(node, (element) => element.type === 'img').props.src
}
function deferred() {
  let resolve!: (value: string) => void
  const promise = new Promise<string>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

const cleanShot: ExtensionManifest = {
  id: 'raycast.cleanshotx',
  name: 'CleanShot X',
  description: '',
  author: 'test',
  version: '1',
  iconUrl: 'file:///extensions/icons/cleanshotx.png',
}
const jsonResume: ExtensionManifest = {
  id: 'raycast.json-resume',
  name: 'JSON Resume',
  description: '',
  author: 'test',
  version: '1',
  iconUrl: 'blob:json-resume-original',
}
const mole: ExtensionManifest = {
  ...jsonResume,
  id: 'raycast.mole',
  name: 'Mole',
  iconUrl: 'https://raw.githubusercontent.com/raycast/extensions/main/extensions/mole/assets/extension-icon.png',
}
const amphetamine: ExtensionManifest = {
  ...jsonResume,
  id: 'raycast.amphetamine',
  name: 'Amphetamine',
}

async function catalog(
  items = [cleanShot, jsonResume],
  getAppIconDataUrl?: (path: string) => Promise<string | null>
) {
  const view = mount()
  vi.stubGlobal('window', {
    tezbar: {
      getAppIconDataUrl,
      extensionList: vi.fn().mockResolvedValue([]),
      extensionSearchStore: vi
        .fn()
        .mockResolvedValue({ items, total: items.length, hasMore: false }),
      onExtensionInstallProgress: vi.fn(() => () => {}),
      onExtensionCatalogUpdated: vi.fn(() => () => {}),
    },
    setTimeout,
    clearTimeout,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  })
  const render = () => view.render(() => ExtensionsView({ onBack: () => {}, embedded: true }))
  render()
  view.commit()
  await vi.advanceTimersByTimeAsync(250)
  let tree = render()
  view.commit()
  tree = render()
  view.commit()
  return {
    tree,
    select(name: string) {
      const button = find(
        tree,
        (element) =>
          element.type === 'button' &&
          elements(element.props.children as ReactNode).some(
            (child) => child.props.children === name
          )
      )
      ;(button.props.onClick as () => void)()
      tree = render()
      view.commit()
      return tree
    },
  }
}

beforeEach(() => {
  vi.useFakeTimers()
  images.release.mockReset()
  images.cached.mockReset().mockImplementation((src: string) => Promise.resolve(src))
})
afterEach(() => {
  for (const harness of mounted.splice(0)) harness.unmount()
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

describe('extension catalog icons (live components)', () => {
  it.each([
    jsonResume,
    cleanShot,
    { ...jsonResume, iconUrl: 'data:image/png;base64,aWNvbg==' },
    mole,
    { ...mole, iconUrl: mole.iconUrl!.replace('https:', 'http:') },
  ])('renders $iconUrl in rows and details through the cache', async (extension) => {
    const view = await catalog([extension])
    images.cached.mockResolvedValue('blob:json-resume')
    for (const size of ['small', undefined]) {
      const iconHooks = mount()
      const imageHooks = mount()
      const output = renderElement(iconHooks, icon(view.tree, extension.name, size))
      const image = cachedImage(output)
      const first = renderElement(imageHooks, image)
      if (/^https?:/.test(extension.iconUrl!)) {
        // Remote originals are never painted: they would download the full
        // PNG next to the WebP conversion. A placeholder holds the slot.
        expect(elements(first).some((element) => element.type === 'img')).toBe(false)
        expect(find(first, (element) => 'data-image-pending' in element.props)).toBeDefined()
      } else {
        expect(imgSource(first)).toBe(extension.iconUrl)
      }
      imageHooks.commit()
      await Promise.resolve()
      expect(imgSource(renderElement(imageHooks, image))).toBe('blob:json-resume')
    }
    expect(images.cached).toHaveBeenCalledWith(extension.iconUrl, 'icon')
  })

  it('drops a resolved app icon when selecting Mole, before and after effects', async () => {
    const lookup = vi.fn().mockResolvedValue('data:image/png;base64,cleanshot')
    const view = await catalog([cleanShot, mole], lookup)
    const iconHooks = mount()
    const first = icon(view.tree, cleanShot.name)
    renderElement(iconHooks, first)
    iconHooks.commit()
    await Promise.resolve()
    expect(imgSource(renderElement(iconHooks, first))).toBe('data:image/png;base64,cleanshot')

    const next = icon(view.select(mole.name), mole.name)
    expect(cachedImage(renderElement(iconHooks, next)).props.src).toBe(mole.iconUrl)
    iconHooks.commit()
    expect(cachedImage(renderElement(iconHooks, next)).props.src).toBe(mole.iconUrl)
    expect(lookup).toHaveBeenCalledTimes(1)
  })

  it.each([
    amphetamine,
    { ...cleanShot, id: 'raycast.clean-shot-x', name: 'CleanShot alias' },
    { ...cleanShot, slug: 'amphetamine', name: 'Changed app path' },
  ])('scopes resolved app icons to the current extension and app path: $name', async (extension) => {
    const pending = deferred()
    const lookup = vi.fn()
      .mockResolvedValueOnce('data:image/png;base64,cleanshot')
      .mockReturnValueOnce(pending.promise)
    const view = await catalog(extension.id === cleanShot.id ? [cleanShot] : [cleanShot, extension], lookup)
    const iconHooks = mount()
    const first = icon(view.tree, cleanShot.name)
    renderElement(iconHooks, first)
    iconHooks.commit()
    await Promise.resolve()
    expect(imgSource(renderElement(iconHooks, first))).toBe('data:image/png;base64,cleanshot')

    // Installed metadata can change the runtime slug without changing the ID.
    const next = extension.id === cleanShot.id
      ? cloneElement(first, { ext: extension })
      : icon(view.select(extension.name), extension.name)
    expect(cachedImage(renderElement(iconHooks, next)).props.src).toBe(extension.iconUrl)
    iconHooks.commit()
    expect(cachedImage(renderElement(iconHooks, next)).props.src).toBe(extension.iconUrl)
    expect(lookup).toHaveBeenCalledTimes(2)
    pending.resolve('data:image/png;base64,current')
    await Promise.resolve()
    expect(imgSource(renderElement(iconHooks, next))).toBe('data:image/png;base64,current')
  })

  it.each(['before effects', 'after effects'])('ignores late app-icon results %s', async (timing) => {
    const old = deferred()
    const current = deferred()
    const lookup = vi.fn().mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise)
    const view = await catalog([cleanShot, amphetamine], lookup)
    const iconHooks = mount()
    renderElement(iconHooks, icon(view.tree, cleanShot.name))
    iconHooks.commit()
    const next = icon(view.select(amphetamine.name), amphetamine.name)
    renderElement(iconHooks, next)

    if (timing === 'before effects') {
      old.resolve('data:image/png;base64,stale')
      await Promise.resolve()
      expect(cachedImage(renderElement(iconHooks, next)).props.src).toBe(amphetamine.iconUrl)
    }
    iconHooks.commit()
    current.resolve('data:image/png;base64,current')
    await Promise.resolve()
    expect(imgSource(renderElement(iconHooks, next))).toBe('data:image/png;base64,current')
    old.resolve('data:image/png;base64,stale')
    await Promise.resolve()
    expect(imgSource(renderElement(iconHooks, next))).toBe('data:image/png;base64,current')
  })

  it.each(['empty', 'rejected', 'unavailable'])('keeps the catalog icon when app lookup is %s', async (result) => {
    const lookup = vi.fn().mockResolvedValueOnce('data:image/png;base64,cleanshot')
    const view = await catalog([cleanShot, amphetamine], lookup)
    const iconHooks = mount()
    const first = icon(view.tree, cleanShot.name)
    renderElement(iconHooks, first)
    iconHooks.commit()
    await Promise.resolve()
    expect(imgSource(renderElement(iconHooks, first))).toBe('data:image/png;base64,cleanshot')

    if (result === 'unavailable') delete (window.tezbar as Partial<typeof window.tezbar>).getAppIconDataUrl
    else if (result === 'rejected') lookup.mockRejectedValueOnce(new Error('App not installed'))
    else lookup.mockResolvedValueOnce(null)
    const next = icon(view.select(amphetamine.name), amphetamine.name)
    renderElement(iconHooks, next)
    iconHooks.commit()
    await Promise.resolve()
    await Promise.resolve()
    expect(cachedImage(renderElement(iconHooks, next)).props.src).toBe(amphetamine.iconUrl)
  })

  it('ignores an app-icon lookup that resolves after unmount', async () => {
    const pending = deferred()
    const view = await catalog([cleanShot], () => pending.promise)
    const iconHooks = mount()
    const first = icon(view.tree, cleanShot.name)
    renderElement(iconHooks, first)
    iconHooks.commit()
    iconHooks.unmount()
    pending.resolve('data:image/png;base64,unmounted')
    await Promise.resolve()
    expect(cachedImage(renderElement(iconHooks, first)).props.src).toBe(cleanShot.iconUrl)
  })

  it('never renders the previous extension image while a newly selected supported icon loads', async () => {
    const view = await catalog()
    const iconHooks = mount()
    const imageHooks = mount()
    images.cached.mockResolvedValueOnce('blob:cleanshot')
    const first = cachedImage(renderElement(iconHooks, icon(view.tree, cleanShot.name)))
    renderElement(imageHooks, first)
    imageHooks.commit()
    await Promise.resolve()
    expect(imgSource(renderElement(imageHooks, first))).toBe('blob:cleanshot')

    const next = cachedImage(
      renderElement(iconHooks, icon(view.select(jsonResume.name), jsonResume.name))
    )
    // Assert the render BEFORE passive effects: no old icon may be painted.
    expect(imgSource(renderElement(imageHooks, next))).toBe(jsonResume.iconUrl)
  })

  it('ignores late cache results after selection changes and after unmount', async () => {
    const view = await catalog()
    const old = deferred()
    const current = deferred()
    images.cached.mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise)
    const iconHooks = mount()
    const imageHooks = mount()
    const first = cachedImage(renderElement(iconHooks, icon(view.tree, cleanShot.name)))
    renderElement(imageHooks, first)
    imageHooks.commit()
    const next = cachedImage(
      renderElement(iconHooks, icon(view.select(jsonResume.name), jsonResume.name))
    )
    renderElement(imageHooks, next)
    imageHooks.commit()
    current.resolve('blob:json-resume')
    await Promise.resolve()
    expect(imgSource(renderElement(imageHooks, next))).toBe('blob:json-resume')
    old.resolve('blob:cleanshot-stale')
    await Promise.resolve()
    expect(imgSource(renderElement(imageHooks, next))).toBe('blob:json-resume')

    const pending = deferred()
    images.cached.mockReturnValueOnce(pending.promise)
    renderElement(imageHooks, first)
    imageHooks.commit()
    imageHooks.unmount()
    expect(images.release).toHaveBeenCalledTimes(3)
    pending.resolve('blob:unmounted')
    await Promise.resolve()
    expect(imgSource(renderElement(imageHooks, first))).not.toBe('blob:unmounted')
  })

  it('shows the placeholder for missing/failed icons, and recovers on a different selection', async () => {
    const missing = { ...jsonResume, iconUrl: undefined }
    const view = await catalog([cleanShot, missing])
    const iconHooks = mount()
    const image = cachedImage(renderElement(iconHooks, icon(view.tree, cleanShot.name)))
    ;(image.props.onError as () => void)()
    const failed = renderElement(iconHooks, icon(view.tree, cleanShot.name))
    expect(elements(failed).some((element) => element.props.variant === 'icon')).toBe(false)
    const absent = renderElement(iconHooks, icon(view.select(missing.name), missing.name))
    expect(elements(absent).some((element) => element.props.variant === 'icon')).toBe(false)

    const remoteView = await catalog([cleanShot, jsonResume])
    const next = renderElement(iconHooks, icon(remoteView.select(jsonResume.name), jsonResume.name))
    expect(cachedImage(next).props.src).toBe(jsonResume.iconUrl)
  })
})
