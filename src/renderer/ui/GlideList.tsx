import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react'

/** How long after a wheel/trackpad scroll we suppress the "follow the
 *  selected row" scroll-into-view. Long enough that a single scroll
 *  gesture isn't fought by hover-driven selection, short enough that
 *  subsequent keyboard navigation scrolls immediately. */
const WHEEL_SUPPRESS_MS = 250

/**
 * A list container that renders a single "sliding" highlight rectangle
 * behind the selected row, producing a fluid, springy focus transition as
 * the user arrows through the list.
 *
 * The wrapper (not the inner <ul>) owns any max-height + overflow behavior
 * so that the absolutely-positioned highlight scrolls in lockstep with the
 * list rows — a selected row can never end up hidden below the fold while
 * the highlight sits visible on the edge.
 */
export function GlideList({
  children,
  selectedIndex,
  itemCount,
  followSelected = true,
  className,
  listClassName,
  highlightClassName,
  onScroll,
}: {
  children: ReactNode
  selectedIndex: number
  itemCount: number
  followSelected?: boolean
  className?: string
  listClassName?: string
  highlightClassName?: string
  onScroll?: (event: React.UIEvent<HTMLDivElement>) => void
}): JSX.Element {
  const wrapperRef = useRef<HTMLDivElement>(null)
  const listRef = useRef<HTMLUListElement>(null)
  /** Timestamp (ms) of the most recent user-initiated wheel/trackpad
   *  scroll. While this is fresh we skip the auto `scrollIntoView` below,
   *  because that effect would otherwise snap the scroll position back to
   *  whichever row is currently under the cursor — mousewheel and
   *  hover-to-select were fighting each other. */
  const lastWheelAtRef = useRef<number>(0)
  const [rect, setRect] = useState<{ top: number; height: number; visible: boolean }>({
    top: 0,
    height: 0,
    visible: false,
  })

  // Listen for wheel/trackpad activity on the document. We don't need to
  // target a specific scroll container — any wheel event the user produced
  // means they're driving the scroll manually, and we should yield.
  useEffect(() => {
    const onWheel = (): void => {
      lastWheelAtRef.current = Date.now()
    }
    window.addEventListener('wheel', onWheel, { passive: true })
    return () => window.removeEventListener('wheel', onWheel)
  }, [])

  useLayoutEffect(() => {
    const ul = listRef.current
    const wrapper = wrapperRef.current
    if (!ul || !wrapper) {
      setRect((r) => ({ ...r, visible: false }))
      return
    }
    if (itemCount === 0 || selectedIndex < 0 || selectedIndex >= itemCount) {
      setRect((r) => ({ ...r, visible: false }))
      return
    }
    // Only count actual <li> children (ignore any non-element nodes).
    const items = Array.from(ul.children).filter((c): c is HTMLElement => c instanceof HTMLElement)
    const target = items[selectedIndex]
    if (!target) {
      setRect((r) => ({ ...r, visible: false }))
      return
    }
    // `offsetTop` walks to the nearest positioned ancestor (our wrapper),
    // so the value already represents the row's y within the scrollable
    // content. That keeps the highlight perfectly aligned with the row
    // regardless of how far the user has scrolled the list.
    setRect({
      top: target.offsetTop,
      height: target.offsetHeight,
      visible: true,
    })
  }, [selectedIndex, itemCount, children])

  // Keep the selected row in view. Using `instant` (not "smooth") avoids a
  // stale frame where the highlight appears on a row that's still clipped
  // by the scroll container while the container slowly animates.
  //
  // We intentionally skip the scroll when the user just produced a wheel
  // event: rows under a stationary cursor fire `mouseenter` as the list
  // scrolls past, which re-selects them; calling `scrollIntoView` here
  // would then snap the container back and effectively cancel the user's
  // scroll gesture.
  useLayoutEffect(() => {
    const ul = listRef.current
    if (!ul) return
    if (!followSelected) return
    if (Date.now() - lastWheelAtRef.current < WHEEL_SUPPRESS_MS) return
    const items = Array.from(ul.children).filter((c): c is HTMLElement => c instanceof HTMLElement)
    const target = items[selectedIndex]
    if (!target) return
    target.scrollIntoView({ block: 'nearest', behavior: 'instant' as ScrollBehavior })
  }, [followSelected, selectedIndex])

  return (
    <div ref={wrapperRef} onScroll={onScroll} className={`relative ${className ?? ''}`}>
      <div
        aria-hidden
        className={`glide-highlight ${highlightClassName ?? ''}`}
        style={{
          transform: `translateY(${rect.top}px)`,
          height: rect.height,
          opacity: rect.visible ? 1 : 0,
        }}
      />
      <ul ref={listRef} className={listClassName ?? 'flex flex-col gap-0.5'}>
        {children}
      </ul>
    </div>
  )
}
