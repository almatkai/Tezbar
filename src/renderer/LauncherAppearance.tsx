import { useCallback, useEffect, useRef, useState } from 'react'
import type { LauncherLayout, LauncherLayoutPatch, ResizeEdge } from '../shared/windowLayout'

function useLauncherLayout() {
  const [layout, setLayout] = useState<LauncherLayout | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    let mounted = true
    const update = (next: LauncherLayout) => {
      if (mounted) setLayout(next)
    }
    const off = window.tezbar.onLauncherLayoutChanged(update)
    void window.tezbar
      .getLauncherLayout()
      .then(update)
      .catch(() => {
        if (mounted) setError('Could not load launcher appearance.')
      })
    return () => {
      mounted = false
      off()
    }
  }, [])
  const change = useCallback(async (patch: LauncherLayoutPatch) => {
    setBusy(true)
    setError(null)
    try {
      setLayout(await window.tezbar.setLauncherLayout(patch))
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure))
    } finally {
      setBusy(false)
    }
  }, [])
  return { layout, error, busy, change }
}

export function LauncherAppearanceSettings(): JSX.Element {
  const { layout, error, busy, change } = useLauncherLayout()
  const [width, setWidth] = useState('')
  const [height, setHeight] = useState('')
  useEffect(() => {
    if (layout) {
      setWidth(String(Math.round(layout.width)))
      setHeight(String(Math.round(layout.height)))
    }
  }, [layout])
  const field =
    'glass-field w-20 px-2 py-1.5 text-xs text-ink-1 focus-visible:ring-1 focus-visible:ring-cyan-300/50'
  const button =
    'rounded-lg border border-white/10 bg-white/5 px-2.5 py-1.5 text-xs text-ink-2 hover:bg-white/10 focus-visible:ring-1 focus-visible:ring-cyan-300/50 disabled:opacity-40'
  return (
    <div className="space-y-3">
      <p className="text-[11px] text-ink-3">
        {layout ? `Launcher display: ${layout.displayName}` : 'Loading display…'}
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <label htmlFor="launcher-interface-scale" className="text-xs text-ink-2">
          Interface scale
        </label>
        <select
          id="launcher-interface-scale"
          aria-label="Launcher interface scale"
          className="glass-field px-2 py-1.5 text-xs text-ink-1"
          value={layout ? Math.round(layout.zoom * 100) : 100}
          disabled={!layout || busy}
          onChange={(event) => void change({ zoom: Number(event.target.value) / 100 })}
        >
          {Array.from({ length: 26 }, (_, index) => 75 + index * 5).map((percent) => (
            <option key={percent} value={percent}>
              {percent}%
            </option>
          ))}
        </select>
        <button
          className={button}
          disabled={!layout || busy}
          onClick={() => void change({ zoom: 1 })}
        >
          100%
        </button>
      </div>
      <form
        className="flex flex-wrap items-center gap-2"
        onSubmit={(event) => {
          event.preventDefault()
          if (Number(width) > 0 && Number(height) > 0) {
            void change({ width: Number(width), height: Number(height) })
          }
        }}
      >
        <label className="text-xs text-ink-2">
          Width{' '}
          <input
            aria-label="Launcher width"
            type="number"
            min="1"
            required
            value={width}
            className={field}
            disabled={!layout || busy}
            onChange={(event) => setWidth(event.target.value)}
          />
        </label>
        <label className="text-xs text-ink-2">
          Height{' '}
          <input
            aria-label="Launcher height"
            type="number"
            min="1"
            required
            value={height}
            className={field}
            disabled={!layout || busy}
            onChange={(event) => setHeight(event.target.value)}
          />
        </label>
        <button type="submit" className={button} disabled={!layout || busy}>
          Apply
        </button>
      </form>
      <p className="text-[11px] leading-relaxed text-ink-4">
        Drag any edge or corner to resize. Size, position and scale are remembered for each display.
        Use ⌘/Ctrl +/− or 0 to change scale; add Alt in Terminal to keep its text zoom separate.
      </p>
      <button
        className={button}
        disabled={!layout || busy}
        onClick={() => void change({ reset: true })}
      >
        Reset size & scale for this display
      </button>
      {error ? (
        <p role="alert" className="text-xs text-rose-300">
          {error}
        </p>
      ) : null}
    </div>
  )
}

export function LauncherAppearanceButton(): JSX.Element {
  const [open, setOpen] = useState(false)
  const root = useRef<HTMLDivElement>(null)
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const cancelClose = () => {
    if (closeTimer.current !== null) clearTimeout(closeTimer.current)
    closeTimer.current = null
  }
  useEffect(
    () => () => {
      if (closeTimer.current !== null) clearTimeout(closeTimer.current)
    },
    []
  )
  useEffect(() => {
    if (!open) return
    const close = (event: PointerEvent) => {
      if (event.target instanceof Node && !root.current?.contains(event.target)) setOpen(false)
    }
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      event.stopImmediatePropagation()
      setOpen(false)
    }
    window.addEventListener('pointerdown', close, true)
    window.addEventListener('keydown', onKey, true)
    return () => {
      window.removeEventListener('pointerdown', close, true)
      window.removeEventListener('keydown', onKey, true)
    }
  }, [open])
  return (
    <div
      ref={root}
      className="no-drag relative shrink-0"
      onPointerEnter={() => {
        cancelClose()
        setOpen(true)
      }}
      onPointerLeave={() => {
        cancelClose()
        closeTimer.current = setTimeout(() => {
          if (!root.current?.contains(document.activeElement)) setOpen(false)
        }, 180)
      }}
      onFocusCapture={() => {
        cancelClose()
        setOpen(true)
      }}
      onBlurCapture={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) {
          cancelClose()
          setOpen(false)
        }
      }}
    >
      <button
        type="button"
        aria-label="Launcher size and interface scale"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen(true)}
        className="group grid h-7 w-6 place-items-center rounded-full focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-cyan-300/50"
      >
        <span
          aria-hidden
          className="h-1.5 w-1.5 rounded-full bg-ink-4 transition-colors group-hover:bg-cyan-200 group-focus-visible:bg-cyan-200"
        />
      </button>
      {open ? (
        <div
          role="dialog"
          aria-label="Launcher appearance"
          className="absolute bottom-[calc(100%+10px)] right-0 z-50 w-[330px] max-w-[calc(100vw-24px)] rounded-[15px] border border-white/10 bg-[#11141c]/95 p-3.5 shadow-[0_20px_60px_rgba(0,0,0,0.48)] backdrop-blur-2xl before:absolute before:inset-x-0 before:-bottom-3 before:h-3"
        >
          <p className="mb-3 text-xs font-semibold text-ink-1">Size & scale</p>
          <LauncherAppearanceSettings />
        </div>
      ) : null}
    </div>
  )
}

const edges: { edge: ResizeEdge; className: string }[] = [
  { edge: 'North', className: 'inset-x-4 top-0 h-1.5 cursor-ns-resize' },
  { edge: 'South', className: 'inset-x-4 bottom-0 h-1.5 cursor-ns-resize' },
  { edge: 'West', className: 'inset-y-4 left-0 w-1.5 cursor-ew-resize' },
  { edge: 'East', className: 'inset-y-4 right-0 w-1.5 cursor-ew-resize' },
  { edge: 'NorthWest', className: 'left-0 top-0 h-4 w-4 cursor-nwse-resize' },
  { edge: 'NorthEast', className: 'right-0 top-0 h-4 w-4 cursor-nesw-resize' },
  { edge: 'SouthWest', className: 'bottom-0 left-0 h-4 w-4 cursor-nesw-resize' },
  { edge: 'SouthEast', className: 'bottom-0 right-0 h-4 w-4 cursor-nwse-resize' },
]

export function LauncherResizeHandles(): JSX.Element {
  return (
    <>
      {edges.map(({ edge, className }) => (
        <div
          key={edge}
          aria-hidden
          className={`no-drag absolute z-[100] ${className}`}
          onMouseDown={(event) => {
            if (event.button !== 0) return
            event.preventDefault()
            event.stopPropagation()
            void window.tezbar
              .startLauncherResize(edge)
              .catch((error) => console.warn('[Resize]', error))
          }}
        />
      ))}
    </>
  )
}
