import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { installWindowDrag } from './windowDrag'

// A small selector fixture keeps these event-lifecycle tests independent of a browser.
class Target {
  constructor(private readonly selectors: string[]) {}
  closest(selector: string): Target | null {
    return selector.split(',').some((part) => this.selectors.includes(part.trim())) ? this : null
  }
  matches(selector: string): boolean {
    return this.selectors.includes(selector)
  }
}

const search = ['.drag-region', '[data-window-search-drag]', '.glass-card > *']
const input = [...search, 'input', '#command-input']
let host: EventTarget
let doc: EventTarget
let dispose: (() => void) | undefined
let start: ReturnType<typeof vi.fn>
let end: ReturnType<typeof vi.fn>

function mouse(type: string, selectors = input, properties: Record<string, unknown> = {}): Event {
  const event = new Event(type, { cancelable: true })
  const values = {
    target: new Target(selectors),
    button: 0,
    buttons: 1,
    detail: 1,
    screenX: 100,
    screenY: 100,
    ...properties,
  }
  for (const [key, value] of Object.entries(values)) {
    Object.defineProperty(event, key, { value })
  }
  host.dispatchEvent(event)
  return event
}

function install(isWindows = false): void {
  dispose = installWindowDrag({ start, end, isWindows })
}

beforeEach(() => {
  host = new EventTarget()
  doc = new EventTarget()
  start = vi.fn().mockResolvedValue(undefined)
  end = vi.fn().mockResolvedValue(undefined)
  vi.stubGlobal('window', host)
  vi.stubGlobal('document', doc)
  vi.stubGlobal('Element', Target)
})

afterEach(() => {
  dispose?.()
  dispose = undefined
  vi.unstubAllGlobals()
})

describe('window dragging from search', () => {
  it('leaves clicks and small movements available for input editing', () => {
    install()
    expect(mouse('mousedown').defaultPrevented).toBe(false)
    mouse('mousemove', input, { screenX: 103 })
    mouse('mouseup')
    mouse('mousemove', input, { screenX: 120 })
    expect(start).not.toHaveBeenCalled()
    expect(end).not.toHaveBeenCalled()
  })

  it.each([{ selectors: input }, { selectors: search }, { selectors: [...search, 'svg'] }])(
    'drags from any non-control search target: %j',
    ({ selectors }) => {
      install()
      mouse('mousedown', selectors)
      mouse('mousemove', selectors, { screenX: 106 })
      mouse('mousemove', selectors, { screenX: 120 })
      expect(start).toHaveBeenCalledTimes(1)
      mouse('mouseup', selectors)
      expect(end).toHaveBeenCalledTimes(1)
    }
  )

  it.each(['button', '.no-drag', '[role="button"]', 'select', '.tezbar-popover', 'input'])(
    'does not hijack a search control: %s',
    (selector) => {
      install()
      mouse('mousedown', [...search, selector])
      mouse('mousemove', search, { screenX: 120 })
      expect(start).not.toHaveBeenCalled()
    }
  )

  it.each([
    { button: 2 },
    { detail: 2 },
    { shiftKey: true },
    { metaKey: true },
    { ctrlKey: true },
    { altKey: true },
  ])('preserves secondary clicks and modified/multi-click selection: %j', (properties) => {
    install()
    mouse('mousedown', input, properties)
    mouse('mousemove', input, { screenX: 120 })
    expect(start).not.toHaveBeenCalled()
  })

  it('cancels a pending gesture if the mouse button is no longer held', () => {
    install()
    mouse('mousedown')
    mouse('mousemove', input, { buttons: 0, screenX: 120 })
    mouse('mousemove', input, { screenX: 130 })
    expect(start).not.toHaveBeenCalled()
  })

  it('ends an active macOS drag on blur', () => {
    install()
    mouse('mousedown')
    mouse('mousemove', input, { screenX: 120 })
    host.dispatchEvent(new Event('blur'))
    expect(end).toHaveBeenCalledTimes(1)
  })

  it('leaves Windows drag completion to the host', () => {
    install(true)
    mouse('mousedown')
    mouse('mousemove', input, { screenX: 120 })
    host.dispatchEvent(new Event('blur'))
    mouse('mouseup')
    expect(start).toHaveBeenCalledTimes(1)
    expect(end).not.toHaveBeenCalled()
  })

  it('removes listeners and ends an active drag on cleanup', () => {
    install()
    mouse('mousedown')
    mouse('mousemove', input, { screenX: 120 })
    dispose?.()
    expect(end).toHaveBeenCalledTimes(1)
    mouse('mousedown')
    mouse('mousemove', input, { screenX: 120 })
    expect(start).toHaveBeenCalledTimes(1)
  })

  it('keeps ordinary chrome draggable but not other inputs or list content', () => {
    install()
    mouse('mousedown', ['.drag-region', 'input'])
    mouse('mousedown', ['.drag-region', '.glass-card > *'])
    mouse('mousedown', [])
    expect(start).not.toHaveBeenCalled()
    mouse('mousedown', ['.drag-region'])
    expect(start).toHaveBeenCalledTimes(1)
  })
})
