import { useEffect, useMemo, useRef, useState } from 'react'
import { EMOJI_CATEGORIES, EMOJI_DATA, type EmojiCategory } from './emoji/emojiData'
import { searchEmojis } from './emoji/emojiSearch'
import { Hint, HintBar, Kbd, ViewHeader, cx } from './ui/primitives'

const RECENT_EMOJI_KEY = 'tezbar:recent-emojis'
const RECENT_LIMIT = 24
const GRID_COLS = 8

type CategoryFilter = 'All' | EmojiCategory

function readRecentEmojis(): string[] {
  try {
    const raw = window.localStorage.getItem(RECENT_EMOJI_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed
      .map((value) => String(value || '').trim())
      .filter(Boolean)
      .slice(0, RECENT_LIMIT)
  } catch {
    return []
  }
}

function writeRecentEmojis(values: string[]): void {
  window.localStorage.setItem(RECENT_EMOJI_KEY, JSON.stringify(values.slice(0, RECENT_LIMIT)))
}

function upsertRecentEmoji(nextEmoji: string): string[] {
  const next = [nextEmoji, ...readRecentEmojis().filter((value) => value !== nextEmoji)].slice(0, RECENT_LIMIT)
  writeRecentEmojis(next)
  return next
}

export default function EmojiPickerView({ onBack }: { onBack: () => void }): JSX.Element {
  const [query, setQuery] = useState('')
  const [category, setCategory] = useState<CategoryFilter>('All')
  const [selected, setSelected] = useState(0)
  const [recent, setRecent] = useState<string[]>([])
  const rootRef = useRef<HTMLDivElement | null>(null)
  const inputRef = useRef<HTMLInputElement | null>(null)
  const selectedButtonRef = useRef<HTMLButtonElement | null>(null)
  const gridRef = useRef<HTMLDivElement | null>(null)
  // Mirror of `selected` kept in a ref so wheel handler can read it synchronously.
  const selectedRef = useRef(0)

  useEffect(() => {
    setRecent(readRecentEmojis())
    rootRef.current?.focus()
    requestAnimationFrame(() => inputRef.current?.focus())
  }, [])

  useEffect(() => {
    const onEsc = (event: KeyboardEvent): void => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      event.stopPropagation()
      if (query.trim().length > 0) {
        setQuery('')
        return
      }
      onBack()
    }
    window.addEventListener('keydown', onEsc, true)
    return () => window.removeEventListener('keydown', onEsc, true)
  }, [onBack, query])

  const searchResults = useMemo(() => searchEmojis(query, category), [query, category])

  const grouped = useMemo(() => {
    const byCategory = new Map<EmojiCategory, string[]>()
    for (const cat of EMOJI_CATEGORIES) byCategory.set(cat, [])
    for (const item of EMOJI_DATA) {
      const bucket = byCategory.get(item.category)
      if (!bucket) continue
      bucket.push(item.char)
    }
    return byCategory
  }, [])

  const recentRows = useMemo(() => {
    const known = new Set(EMOJI_DATA.map((entry) => entry.char))
    return recent.filter((emoji) => known.has(emoji))
  }, [recent])

  /**
   * A flat list of emojis that exactly mirrors what is rendered on screen.
   * Arrow-key navigation indexes into this list so selection always matches
   * the visible grid regardless of which view mode is active.
   */
  const visibleEmojis = useMemo((): string[] => {
    const q = query.trim()
    if (q.length > 0) {
      // Search mode — flat result list
      return searchResults.map((e) => e.char)
    }
    if (category !== 'All') {
      // Single category browse
      return grouped.get(category) ?? []
    }
    // "All" browse: recent rows first, then every category in order
    const out: string[] = [...recentRows]
    for (const cat of EMOJI_CATEGORIES) {
      out.push(...(grouped.get(cat) ?? []))
    }
    return out
  }, [query, category, searchResults, grouped, recentRows])

  useEffect(() => {
    if (visibleEmojis.length === 0) {
      setSelected(0)
      return
    }
    setSelected((index) => Math.min(index, visibleEmojis.length - 1))
  }, [visibleEmojis.length])

  // Keep selectedRef in sync with state so event handlers always read the latest value.
  useEffect(() => {
    selectedRef.current = selected
  })

  const canUseRecentShortcuts = query.trim().length === 0 && recentRows.length > 0

  const copyEmoji = async (emoji: string): Promise<void> => {
    setRecent(upsertRecentEmoji(emoji))
    // Hide the window first so the previous app regains focus before
    // the main process fires the ⌘V paste keystroke via AppleScript.
    await window.tezbar.hide()
    await window.tezbar.executeSearchAction({ type: 'copy-and-paste-text', text: emoji })
  }

  const onKeyDown = (event: React.KeyboardEvent<HTMLElement>): void => {
    if ((event.metaKey || event.ctrlKey) && /^[1-9]$/.test(event.key) && canUseRecentShortcuts) {
      const index = Number(event.key) - 1
      const target = recentRows[index]
      if (target) {
        event.preventDefault()
        void copyEmoji(target)
      }
      return
    }

    if (visibleEmojis.length === 0) return

    if (event.key === 'ArrowRight') {
      event.preventDefault()
      setSelected((index) => Math.min(index + 1, visibleEmojis.length - 1))
      return
    }
    if (event.key === 'ArrowLeft') {
      event.preventDefault()
      setSelected((index) => Math.max(index - 1, 0))
      return
    }
    if (event.key === 'ArrowDown') {
      event.preventDefault()
      setSelected((index) => Math.min(index + GRID_COLS, visibleEmojis.length - 1))
      return
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault()
      setSelected((index) => Math.max(index - GRID_COLS, 0))
      return
    }
    if (event.key === 'Enter') {
      event.preventDefault()
      const emoji = visibleEmojis[selected]
      if (emoji) void copyEmoji(emoji)
    }
  }

  // Native wheel listener with passive:false so we can call preventDefault.
  useEffect(() => {
    const el = gridRef.current
    if (!el) return
    const handler = (event: WheelEvent): void => {
      if (visibleEmojis.length === 0) return
      const delta = event.deltaY > 0 ? GRID_COLS : -GRID_COLS
      const current = selectedRef.current
      const next = Math.min(Math.max(current + delta, 0), visibleEmojis.length - 1)
      if (next !== current) {
        event.preventDefault()
        setSelected(next)
      }
    }
    el.addEventListener('wheel', handler, { passive: false })
    return () => el.removeEventListener('wheel', handler)
  }, [visibleEmojis])

  useEffect(() => {
    selectedButtonRef.current?.scrollIntoView({
      block: 'nearest',
      inline: 'nearest',
    })
  }, [selected])

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      role="application"
      aria-label="Emoji Picker"
      onKeyDown={onKeyDown}
      className="flex h-full min-h-0 w-full flex-col gap-2 outline-none animate-tezbar-scale-in"
    >
      <div className="glass-card shrink-0 px-4 py-3 animate-tezbar-scale-in">
        <ViewHeader title="Emoji Picker" onBack={onBack} />
        <input
          ref={inputRef}
          value={query}
          onChange={(event) => {
            setQuery(event.target.value)
            setSelected(0)
          }}
          placeholder="Search emojis by name, mood, or slang..."
          className="mt-2 h-8 w-full rounded-tezbar-chip border border-white/10 bg-white/[0.04] px-2.5 text-[12px] text-ink-1 placeholder:text-ink-4 outline-none transition focus:border-white/20 focus:bg-white/[0.06]"
        />
        <div className="mt-2 flex flex-wrap items-center gap-1">
          {(['All', ...EMOJI_CATEGORIES] as CategoryFilter[]).map((chip) => {
            const active = category === chip
            return (
              <button
                key={chip}
                type="button"
                onClick={() => {
                  setCategory(chip)
                  setSelected(0)
                }}
                className={cx(
                  'rounded-tezbar-chip px-2 py-1 text-[11px] transition',
                  active
                    ? 'bg-white/[0.12] font-medium text-ink-1'
                    : 'text-ink-3 hover:bg-white/[0.06] hover:text-ink-1',
                )}
              >
                {chip}
              </button>
            )
          })}
        </div>
      </div>

      <div ref={gridRef} className="glass-card min-h-0 flex-1 overflow-hidden px-3 py-3 animate-tezbar-scale-in">
        {query.trim().length > 0 ? (
          searchResults.length > 0 ? (
            <div className="grid min-h-0 grid-cols-8 gap-1 overflow-y-auto">
              {searchResults.map((entry, index) => (
                <button
                  key={`${entry.char}-${entry.name}`}
                  type="button"
                  ref={index === selected ? selectedButtonRef : null}
                  title={entry.name}
                  onMouseEnter={() => setSelected(index)}
                  onClick={() => {
                    void copyEmoji(entry.char)
                  }}
                  className={cx(
                    'grid h-9 place-items-center rounded-tezbar-chip border text-[20px] transition',
                    index === selected
                      ? 'border-accent/60 bg-accent/15'
                      : 'border-white/10 bg-white/[0.03] hover:border-white/20 hover:bg-white/[0.07]',
                  )}
                >
                  {entry.char}
                </button>
              ))}
            </div>
          ) : (
            <div className="grid h-full place-items-center text-center">
              <p className="text-[12px] text-ink-4">No emojis found for that query.</p>
            </div>
          )
        ) : category === 'All' ? (
          <div className="min-h-0 space-y-3 overflow-y-auto pr-1">
            {recentRows.length > 0 ? (
              <section>
                <p className="mb-1 text-[11px] font-medium text-ink-3">Recently Used</p>
                <div className="flex flex-wrap gap-1">
                  {recentRows.map((emoji, index) => (
                    <button
                      key={`recent:${emoji}`}
                      type="button"
                      ref={index === selected ? selectedButtonRef : null}
                      onMouseEnter={() => setSelected(index)}
                      onClick={() => {
                        void copyEmoji(emoji)
                      }}
                      className={cx(
                        'grid h-9 w-9 place-items-center rounded-tezbar-chip border text-[20px] transition',
                        index === selected
                          ? 'border-accent/60 bg-accent/15'
                          : 'border-white/10 bg-white/[0.03] hover:border-white/20 hover:bg-white/[0.07]',
                      )}
                    >
                      {emoji}
                    </button>
                  ))}
                </div>
              </section>
            ) : null}
            {EMOJI_CATEGORIES.map((cat) => {
              const values = grouped.get(cat) ?? []
              if (values.length === 0) return null
              // Offset index to account for recent rows and preceding categories
              const catOffset = recentRows.length + EMOJI_CATEGORIES.slice(0, EMOJI_CATEGORIES.indexOf(cat)).reduce(
                (acc, c) => acc + (grouped.get(c)?.length ?? 0),
                0,
              )
              return (
                <section key={cat}>
                  <p className="mb-1 text-[11px] font-medium text-ink-3">{cat}</p>
                  <div className="grid grid-cols-8 gap-1">
                    {values.map((emoji, i) => {
                      const index = catOffset + i
                      return (
                        <button
                          key={`${cat}:${emoji}`}
                          type="button"
                          ref={index === selected ? selectedButtonRef : null}
                          onMouseEnter={() => setSelected(index)}
                          onClick={() => {
                            void copyEmoji(emoji)
                          }}
                          className={cx(
                            'grid h-9 place-items-center rounded-tezbar-chip border text-[20px] transition',
                            index === selected
                              ? 'border-accent/60 bg-accent/15'
                              : 'border-white/10 bg-white/[0.03] hover:border-white/20 hover:bg-white/[0.07]',
                          )}
                        >
                          {emoji}
                        </button>
                      )
                    })}
                  </div>
                </section>
              )
            })}
          </div>
        ) : (
          <div className="min-h-0 overflow-y-auto">
            <section>
              <p className="mb-1 text-[11px] font-medium text-ink-3">{category}</p>
              <div className="grid grid-cols-8 gap-1">
                {(grouped.get(category) ?? []).map((emoji, index) => (
                  <button
                    key={`${category}:${emoji}`}
                    type="button"
                    ref={index === selected ? selectedButtonRef : null}
                    onMouseEnter={() => setSelected(index)}
                    onClick={() => {
                      void copyEmoji(emoji)
                    }}
                    className={cx(
                      'grid h-9 place-items-center rounded-tezbar-chip border text-[20px] transition',
                      index === selected
                        ? 'border-accent/60 bg-accent/15'
                        : 'border-white/10 bg-white/[0.03] hover:border-white/20 hover:bg-white/[0.07]',
                    )}
                  >
                    {emoji}
                  </button>
                ))}
              </div>
            </section>
          </div>
        )}
      </div>

      <div className="glass-card shrink-0 px-4 py-2 animate-tezbar-scale-in">
        <HintBar>
          <Hint label="Navigate" keys={<><Kbd>↑</Kbd><Kbd>↓</Kbd><Kbd>←</Kbd><Kbd>→</Kbd></>} />
          <Hint label="Copy" keys={<Kbd>↵</Kbd>} />
          <Hint label="Recent" keys={<><Kbd>⌘</Kbd><Kbd>1-9</Kbd></>} />
          <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
        </HintBar>
      </div>
    </div>
  )
}
