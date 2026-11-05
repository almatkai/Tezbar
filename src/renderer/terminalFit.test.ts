import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { observeTerminalFit } from './terminalFit'

function createTerminal() {
  const host = { width: 1200, height: 600 }
  const renders = new Set<() => void>()
  const terminal = {
    cols: 80,
    rows: 24,
    options: { scrollback: 1000 },
    element: { parentElement: host },
    _core: { _renderService: {
      dimensions: { css: { cell: { width: 0, height: 0 } } },
      clear: vi.fn(),
    } },
    resize(cols: number, rows: number) { this.cols = cols; this.rows = rows },
    onRender(callback: () => void) {
      renders.add(callback)
      return { dispose: () => renders.delete(callback) }
    },
  }
  const addon = new FitAddon()
  addon.activate(terminal as unknown as Terminal)
  const syncShell = vi.fn()
  const fit = (): void => {
    addon.fit()
    syncShell(terminal.cols, terminal.rows)
  }
  const rendered = (width = 10, height = 20): void => {
    terminal._core._renderService.dimensions.css.cell = { width, height }
    renders.forEach((callback) => callback())
  }
  const dispose = observeTerminalFit(
    host as unknown as HTMLElement,
    terminal as unknown as Terminal,
    addon,
    fit,
  )
  return { host, terminal, syncShell, rendered, dispose }
}

let frames: Map<number, FrameRequestCallback>
let resizeCallback: ResizeObserverCallback
let disconnect: ReturnType<typeof vi.fn>
let nextFrame = 0
const cleanups: Array<() => void> = []

function flushFrames(): void {
  const pending = [...frames.values()]
  frames.clear()
  pending.forEach((callback) => callback(0))
}

beforeEach(() => {
  frames = new Map()
  disconnect = vi.fn()
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
    frames.set(++nextFrame, callback)
    return nextFrame
  })
  vi.stubGlobal('cancelAnimationFrame', (id: number) => frames.delete(id))
  vi.stubGlobal('ResizeObserver', class {
    constructor(callback: ResizeObserverCallback) { resizeCallback = callback }
    observe() {}
    disconnect = disconnect
  })
  vi.stubGlobal('window', {
    getComputedStyle: (element: { width?: number; height?: number }) => ({
      getPropertyValue: (property: string) => {
        if (property === 'width') return `${element.width}px`
        if (property === 'height') return `${element.height}px`
        return '0px'
      },
    }),
  })
})

afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup())
  vi.unstubAllGlobals()
})

function setup() {
  const result = createTerminal()
  cleanups.push(result.dispose)
  return result
}

describe('terminal fitting', () => {
  it('uses the full width when cell measurement finishes after the initial fit', () => {
    const { terminal, rendered, syncShell } = setup()
    flushFrames()
    expect(terminal.cols).toBe(80)
    rendered()
    flushFrames()
    // The real FitAddon reserves 15px for the scrollbar.
    expect(terminal.cols).toBe(118)
    expect(syncShell).toHaveBeenLastCalledWith(118, 30)
  })

  it('updates the grid and shell when the host grows and shrinks', () => {
    const { host, terminal, rendered, syncShell } = setup()
    rendered()
    flushFrames()
    for (const [width, cols] of [[1600, 158], [500, 48], [1000, 98]] as const) {
      host.width = width
      resizeCallback([], {} as ResizeObserver)
      flushFrames()
      expect(terminal.cols).toBe(cols)
      expect(syncShell).toHaveBeenLastCalledWith(cols, 30)
    }
  })

  it('refits changed font metrics without resizing the host', () => {
    const { terminal, rendered } = setup()
    rendered()
    flushFrames()
    rendered(20, 30)
    flushFrames()
    expect(terminal.cols).toBe(59)
    expect(terminal.rows).toBe(20)
  })

  it('does not schedule another fit for ordinary terminal output', () => {
    const { rendered, syncShell } = setup()
    rendered()
    flushFrames()
    syncShell.mockClear()
    rendered()
    flushFrames()
    expect(syncShell).not.toHaveBeenCalled()
  })

  it('disconnects and cancels pending fits on unmount', () => {
    const { dispose, rendered, syncShell } = setup()
    dispose()
    rendered()
    flushFrames()
    expect(disconnect).toHaveBeenCalledOnce()
    expect(syncShell).not.toHaveBeenCalled()
  })
})
