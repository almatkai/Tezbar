import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import type { VaultKind } from '../shared/vault'
import { cx } from './ui/primitives'
import { VAULT_ICONS, VAULT_ICON_CHIP_CLASS, VaultIconGlyph, resolveVaultIcon } from './vaultIcons'

export function VaultIconPicker({
  kind,
  icon,
  onChange,
}: {
  kind: VaultKind
  icon?: string
  onChange: (icon: string | undefined) => void
}): JSX.Element {
  const [open, setOpen] = useState(false)
  const [filter, setFilter] = useState('')
  const [activeId, setActiveId] = useState<string | undefined>(icon)
  const [position, setPosition] = useState({ left: 0, top: 0, maxHeight: 0 })
  const chipRef = useRef<HTMLButtonElement>(null)
  const popoverRef = useRef<HTMLDivElement>(null)
  const filterRef = useRef<HTMLInputElement>(null)
  const dialogId = useId()
  const selected = resolveVaultIcon(kind, icon)
  const query = filter.trim().toLowerCase()
  const filtered = VAULT_ICONS.filter((entry) =>
    [entry.label, ...entry.keywords].some((text) => text.toLowerCase().includes(query))
  )
  const active = filtered.find((entry) => entry.id === activeId) ?? filtered[0]

  const close = useCallback(() => {
    setOpen(false)
    chipRef.current?.focus()
  }, [])

  useEffect(() => {
    if (!open) return
    const outside = (event: PointerEvent): void => {
      if (
        !popoverRef.current?.contains(event.target as Node) &&
        !chipRef.current?.contains(event.target as Node)
      )
        close()
    }
    // Capture also covers focus outside the portal, before Vault/App can cancel the draft.
    const escape = (event: KeyboardEvent): void => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      event.stopPropagation()
      close()
    }
    document.addEventListener('pointerdown', outside)
    window.addEventListener('keydown', escape, true)
    filterRef.current?.focus()
    return () => {
      document.removeEventListener('pointerdown', outside)
      window.removeEventListener('keydown', escape, true)
    }
  }, [open, close])

  useLayoutEffect(() => {
    if (!open) return
    const place = (): void => {
      const anchor = chipRef.current?.getBoundingClientRect()
      const popover = popoverRef.current
      if (!anchor || !popover) return
      const below = window.innerHeight - anchor.bottom - 14
      const above = anchor.top - 14
      const height = popover.scrollHeight
      const useAbove = below < height && above > below
      const maxHeight = Math.max(0, useAbove ? above : below)
      setPosition({
        left: Math.max(8, Math.min(anchor.left, window.innerWidth - 272)),
        top: useAbove
          ? Math.max(8, anchor.top - 6 - Math.min(height, maxHeight))
          : anchor.bottom + 6,
        maxHeight,
      })
    }
    place()
    window.addEventListener('resize', place)
    window.addEventListener('scroll', place, true)
    return () => {
      window.removeEventListener('resize', place)
      window.removeEventListener('scroll', place, true)
    }
  }, [open, filter])

  const focusCell = (index: number): void => {
    const entry = filtered[Math.max(0, Math.min(index, filtered.length - 1))]
    if (!entry) return
    setActiveId(entry.id)
    popoverRef.current?.querySelector<HTMLButtonElement>(`[data-icon-id="${entry.id}"]`)?.focus()
  }

  const choose = (next: string | undefined): void => {
    onChange(next)
    close()
  }

  return (
    <>
      <button
        ref={chipRef}
        type="button"
        aria-label="Change icon"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? dialogId : undefined}
        className={cx(VAULT_ICON_CHIP_CLASS, 'transition hover:bg-white/[0.08]')}
        onClick={() => {
          if (open) close()
          else {
            setFilter('')
            setActiveId(selected.id)
            setOpen(true)
          }
        }}
      >
        <VaultIconGlyph entry={selected} />
      </button>
      {open &&
        createPortal(
          <div
            ref={popoverRef}
            id={dialogId}
            role="dialog"
            aria-label="Change icon"
            className="tezbar-popover no-drag fixed z-50 w-[264px] max-w-[calc(100vw-16px)] overflow-y-auto p-3"
            style={position}
            onKeyDown={(event) => event.stopPropagation()}
          >
            <input
              ref={filterRef}
              value={filter}
              placeholder="Filter icons"
              aria-label="Filter icons"
              className="mb-2 h-7 w-full bg-transparent text-[12px] text-ink-1 outline-none placeholder:text-ink-4"
              onChange={(event) => setFilter(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'ArrowDown') {
                  event.preventDefault()
                  focusCell(filtered.findIndex((entry) => entry.id === active?.id))
                }
              }}
            />
            <div className="grid grid-cols-8 gap-0.5">
              {filtered.map((entry, index) => (
                <button
                  key={entry.id}
                  type="button"
                  title={entry.label}
                  aria-label={entry.label}
                  aria-pressed={entry.id === selected.id}
                  data-icon-id={entry.id}
                  tabIndex={entry.id === active?.id ? 0 : -1}
                  className={cx(
                    'grid h-7 w-7 place-items-center rounded-tezbar-chip outline-none transition focus-visible:ring-1 focus-visible:ring-white/20',
                    entry.id === selected.id
                      ? 'bg-white/[0.10] text-ink-1'
                      : 'text-ink-3 hover:bg-white/[0.06]'
                  )}
                  onFocus={() => setActiveId(entry.id)}
                  onClick={() => choose(entry.id)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') {
                      event.preventDefault()
                      choose(entry.id)
                      return
                    }
                    const offset = { ArrowRight: 1, ArrowLeft: -1, ArrowDown: 8, ArrowUp: -8 }[
                      event.key
                    ]
                    if (offset === undefined) return
                    event.preventDefault()
                    if (event.key === 'ArrowUp' && index < 8) filterRef.current?.focus()
                    else focusCell(index + offset)
                  }}
                >
                  <VaultIconGlyph entry={entry} />
                </button>
              ))}
            </div>
            <button
              type="button"
              className="mt-2 text-[11.5px] text-ink-3 transition hover:text-ink-1"
              onClick={() => choose(undefined)}
            >
              Use default
            </button>
          </div>,
          document.body
        )}
    </>
  )
}
