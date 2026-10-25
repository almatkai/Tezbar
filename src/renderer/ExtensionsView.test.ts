import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { isValidElement, type ReactElement, type ReactNode } from 'react'
import type { ExtensionManifest } from '../shared/extensions'
import ExtensionsView from './ExtensionsView'

const hooks = vi.hoisted(() => ({ current: null as HookHarness | null }))
const images = vi.hoisted(() => ({ cached: vi.fn() }))

vi.mock('./extensionImages', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./extensionImages')>()),
  cachedExtensionImageSource: images.cached,
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

async function catalog(items = [cleanShot, jsonResume]) {
  const view = mount()
  vi.stubGlobal('window', {
    tezbar: {
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
  images.cached.mockReset().mockImplementation((src: string) => Promise.resolve(src))
})
afterEach(() => {
  for (const harness of mounted.splice(0)) harness.unmount()
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

describe('extension catalog icons (live components)', () => {
  it('renders supported icons in both catalog rows and selected details, then uses the cache', async () => {
    const view = await catalog([jsonResume])
    images.cached.mockResolvedValue('blob:json-resume')
    for (const size of ['small', undefined]) {
      const iconHooks = mount()
      const imageHooks = mount()
      const output = renderElement(iconHooks, icon(view.tree, jsonResume.name, size))
      const image = cachedImage(output)
      expect(imgSource(renderElement(imageHooks, image))).toBe(jsonResume.iconUrl)
      imageHooks.commit()
      await Promise.resolve()
      expect(imgSource(renderElement(imageHooks, image))).toBe('blob:json-resume')
    }
    expect(images.cached).toHaveBeenCalledWith(jsonResume.iconUrl, 'icon')
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
