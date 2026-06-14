import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { ClipboardEntry, ClipboardImagePayload } from '../shared/clipboard'
import { Hint, HintBar, Kbd, Message, ViewHeader } from './ui/primitives'
import { GlideList } from './ui/GlideList'

/** Extra kind bucket the UI tracks on top of the persisted kinds — a URL
 *  is really a text entry but users think of it as its own category, so
 *  we expose it as a filter chip. */
type Filter = 'all' | 'text' | 'image' | 'url' | 'file'

const URL_RE = /^(https?|ftp|file):\/\/\S+$/i

function isUrlEntry(entry: ClipboardEntry): boolean {
  return entry.kind === 'text' && URL_RE.test(entry.text.trim())
}

function entryKindLabel(entry: ClipboardEntry): string {
  if (entry.kind === 'image') return 'Image'
  if (entry.kind === 'file') return entry.paths.length > 1 ? 'Files' : 'File'
  if (isUrlEntry(entry)) return 'URL'
  return 'Text'
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

function formatDate(ms: number): string {
  const d = new Date(ms)
  const date = d.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  })
  const time = d.toLocaleTimeString(undefined, {
    hour: 'numeric',
    minute: '2-digit',
  })
  return `${date}, ${time}`
}

function basename(path: string): string {
  const idx = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'))
  return idx >= 0 ? path.slice(idx + 1) : path
}

function entryPreviewText(entry: ClipboardEntry): string {
  if (entry.kind === 'text') {
    const line = entry.text.split('\n').find((l) => l.trim().length > 0) ?? entry.text
    return entry.isSecret ? '••••••••' : line.slice(0, 140)
  }
  if (entry.kind === 'image') return `${entry.width} × ${entry.height}`
  return entry.paths.map((p) => basename(p)).join(', ')
}

function entryPrimaryTitle(entry: ClipboardEntry): string {
  if (entry.kind === 'text') {
    return entry.isSecret ? 'Secure item' : entryPreviewText(entry)
  }
  if (entry.kind === 'image') {
    return basename(entry.imagePath) || 'Screenshot'
  }
  const first = entry.paths[0] ? basename(entry.paths[0]) : '(empty)'
  return entry.paths.length > 1 ? `${first} +${entry.paths.length - 1}` : first
}

/* =========================================================================
   Row icons — tiny, consistent glyphs for each kind so the list scans fast
   ========================================================================= */
function RowIcon({
  entry,
  imageUrl,
}: {
  entry: ClipboardEntry
  imageUrl: string | null
}): JSX.Element {
  const base =
    'grid h-7 w-7 shrink-0 place-items-center overflow-hidden rounded-tezbar-chip border border-white/10 bg-white/[0.04] text-ink-3'

  if (entry.kind === 'image' && imageUrl) {
    return (
      <div className={`${base} p-0`}>
        <img src={imageUrl} alt="" className="h-full w-full object-cover" />
      </div>
    )
  }

  if (entry.kind === 'image') {
    return (
      <div className={base} aria-hidden>
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
          <rect x="1.5" y="2.5" width="11" height="9" rx="1.5" stroke="currentColor" strokeWidth="1.2" />
          <circle cx="5" cy="6" r="1" fill="currentColor" />
          <path d="m2.5 10 3-2.5 3 2L11 8l1.5 1.5" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" />
        </svg>
      </div>
    )
  }

  if (entry.kind === 'file') {
    return (
      <div className={base} aria-hidden>
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
          <path
            d="M2.5 3.5a1 1 0 0 1 1-1h2.5l1 1.2H10.5a1 1 0 0 1 1 1V10a1 1 0 0 1-1 1h-7a1 1 0 0 1-1-1V3.5Z"
            stroke="currentColor"
            strokeWidth="1.2"
            strokeLinejoin="round"
          />
        </svg>
      </div>
    )
  }

  if (isUrlEntry(entry)) {
    return (
      <div className={base} aria-hidden>
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
          <path d="M6 8a2 2 0 0 0 2.8 0l2-2a2 2 0 0 0-2.8-2.8l-.6.6" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
          <path d="M8 6a2 2 0 0 0-2.8 0l-2 2a2 2 0 0 0 2.8 2.8l.6-.6" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
        </svg>
      </div>
    )
  }

  return (
    <div className={base} aria-hidden>
      <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
        <path
          d="M3.5 2.5h5l2.5 2.5v6.5a.5.5 0 0 1-.5.5h-7a.5.5 0 0 1-.5-.5v-8.5a.5.5 0 0 1 .5-.5Z"
          stroke="currentColor"
          strokeWidth="1.2"
          strokeLinejoin="round"
        />
        <path d="M5 7h4M5 9h3" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
      </svg>
    </div>
  )
}

/* =========================================================================
   Main view
   ========================================================================= */
export default function ClipboardView({ onBack }: { onBack: () => void }): JSX.Element {
  const rootRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  const [entries, setEntries] = useState<ClipboardEntry[]>([])
  const [selected, setSelected] = useState(0)
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState<Filter>('all')
  const [msg, setMsg] = useState<{ tone: 'success' | 'error'; text: string } | null>(null)

  // Cache of base64 image payloads keyed by entry id. We lazy-load images
  // because bundling every PNG into the first paint would blow up the
  // window's first frame and defeat the whole "instant launcher" feel.
  const [imageCache, setImageCache] = useState<Record<string, ClipboardImagePayload | null>>({})

  const reload = useCallback(async (): Promise<ClipboardEntry[]> => {
    const items = await window.tezbar.listClipboardEntries()
    setEntries(items)
    return items
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  useEffect(() => {
    rootRef.current?.focus()
  }, [])

  // Capture-phase window listener for Escape so the behavior matches
  // every other sub-view (Settings, Providers, …). If a search query is
  // active we clear it first (a familiar mac pattern — Esc peels back
  // the most recent state); otherwise we pop back to the launcher.
  useEffect(() => {
    const onEsc = (e: KeyboardEvent): void => {
      if (e.key !== 'Escape') return
      e.preventDefault()
      e.stopPropagation()
      if (query.length > 0 && document.activeElement === inputRef.current) {
        setQuery('')
        return
      }
      onBack()
    }
    window.addEventListener('keydown', onEsc, true)
    return () => window.removeEventListener('keydown', onEsc, true)
  }, [onBack, query])

  // Filter + search (pinned entries float to the top regardless of filter).
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    const matchesFilter = (entry: ClipboardEntry): boolean => {
      if (filter === 'all') return true
      if (filter === 'text') return entry.kind === 'text' && !isUrlEntry(entry)
      if (filter === 'url') return isUrlEntry(entry)
      if (filter === 'image') return entry.kind === 'image'
      if (filter === 'file') return entry.kind === 'file'
      return true
    }
    const matchesQuery = (entry: ClipboardEntry): boolean => {
      if (!q) return true
      if (entry.kind === 'text') return entry.text.toLowerCase().includes(q)
      if (entry.kind === 'image') return basename(entry.imagePath).toLowerCase().includes(q)
      return entry.paths.some((p) => p.toLowerCase().includes(q))
    }
    const visible = entries.filter((e) => matchesFilter(e) && matchesQuery(e))
    return [...visible].sort((a, b) => {
      if (a.pinned !== b.pinned) return a.pinned ? -1 : 1
      return b.createdAt - a.createdAt
    })
  }, [entries, filter, query])

  // Keep selection in range as the list shrinks/grows.
  useEffect(() => {
    if (filtered.length === 0) {
      setSelected(0)
      return
    }
    if (selected >= filtered.length) setSelected(filtered.length - 1)
  }, [filtered, selected])

  // Hydrate the selected image lazily — one round-trip per image, then
  // cached until the view unmounts or the entry is deleted.
  const currentEntry = filtered[selected]
  useEffect(() => {
    if (!currentEntry || currentEntry.kind !== 'image') return
    if (imageCache[currentEntry.id] !== undefined) return
    let cancelled = false
    void window.tezbar.readClipboardImage(currentEntry.id).then((payload) => {
      if (!cancelled) setImageCache((prev) => ({ ...prev, [currentEntry.id]: payload }))
    })
    return () => {
      cancelled = true
    }
  }, [currentEntry, imageCache])

  const currentImageUrl = useMemo(() => {
    if (!currentEntry || currentEntry.kind !== 'image') return null
    const payload = imageCache[currentEntry.id]
    return payload ? payload.dataUrl : null
  }, [currentEntry, imageCache])

  // --- actions ---------------------------------------------------------
  const flash = useCallback((tone: 'success' | 'error', text: string): void => {
    setMsg({ tone, text })
    window.setTimeout(() => setMsg(null), 2200)
  }, [])

  const copyBack = useCallback(async (entry: ClipboardEntry): Promise<void> => {
    const ok = await window.tezbar.restoreClipboardEntry(entry.id)
    flash(ok ? 'success' : 'error', ok ? 'Copied to clipboard' : 'Could not restore entry')
    if (ok) void window.tezbar.hide()
  }, [flash])

  const deleteEntry = useCallback(async (entry: ClipboardEntry): Promise<void> => {
    const ok = await window.tezbar.deleteClipboardEntry(entry.id)
    if (!ok) {
      flash('error', 'Could not delete entry')
      return
    }
    await reload()
    setImageCache((prev) => {
      if (!(entry.id in prev)) return prev
      const { [entry.id]: _omit, ...rest } = prev
      return rest
    })
  }, [flash, reload])

  const togglePin = useCallback(async (entry: ClipboardEntry): Promise<void> => {
    const ok = await window.tezbar.toggleClipboardPin(entry.id)
    if (!ok) {
      flash('error', 'Could not update pin')
      return
    }
    await reload()
  }, [flash, reload])

  const revealInFinder = useCallback(async (entry: ClipboardEntry): Promise<void> => {
    if (entry.kind === 'text' && !isUrlEntry(entry)) return
    const ok = await window.tezbar.revealClipboardEntry(entry.id)
    flash(ok ? 'success' : 'error', ok ? 'Revealed in Finder' : 'Nothing to reveal')
  }, [flash])

  const clearHistory = useCallback(async (): Promise<void> => {
    if (entries.length === 0) return
    await window.tezbar.clearClipboardHistory()
    setImageCache({})
    await reload()
    flash('success', 'Clipboard history cleared')
  }, [entries.length, flash, reload])

  // --- keyboard --------------------------------------------------------
  // Escape is handled at the window-level in capture phase (see above)
  // so it beats every other listener; the handler below only owns the
  // in-view shortcuts (navigation, copy, pin, delete, …).
  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>): void => {
    const inInput = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement

    if (filtered.length === 0) return

    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setSelected((i) => Math.min(i + 1, filtered.length - 1))
      return
    }

    if (e.key === 'ArrowUp') {
      e.preventDefault()
      setSelected((i) => Math.max(i - 1, 0))
      return
    }

    const entry = filtered[selected]
    if (!entry) return

    if (e.key === 'Enter') {
      e.preventDefault()
      void copyBack(entry)
      return
    }

    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'd') {
      e.preventDefault()
      void deleteEntry(entry)
      return
    }

    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'r') {
      e.preventDefault()
      void revealInFinder(entry)
      return
    }

    if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'c') {
      e.preventDefault()
      void clearHistory()
      return
    }

    if (!inInput && e.key.toLowerCase() === 'p' && !e.metaKey && !e.ctrlKey) {
      e.preventDefault()
      void togglePin(entry)
      return
    }

    // Cmd+1..9 jumps straight to a row.
    if ((e.metaKey || e.ctrlKey) && /^[1-9]$/.test(e.key)) {
      e.preventDefault()
      const target = Number(e.key) - 1
      if (target < filtered.length) {
        setSelected(target)
        void copyBack(filtered[target]!)
      }
    }
  }

  const filters: { id: Filter; label: string }[] = [
    { id: 'all', label: 'All' },
    { id: 'text', label: 'Text' },
    { id: 'image', label: 'Image' },
    { id: 'url', label: 'URL' },
    { id: 'file', label: 'File' },
  ]

  const countsByFilter = useMemo(() => {
    const byKind: Record<Filter, number> = { all: entries.length, text: 0, image: 0, url: 0, file: 0 }
    for (const entry of entries) {
      if (entry.kind === 'image') byKind.image += 1
      else if (entry.kind === 'file') byKind.file += 1
      else if (isUrlEntry(entry)) byKind.url += 1
      else byKind.text += 1
    }
    return byKind
  }, [entries])

  const primaryActionLabel = useMemo(() => {
    if (!currentEntry) return null
    if (currentEntry.kind === 'image') return 'Copy image'
    if (currentEntry.kind === 'file') return 'Reveal in Finder'
    if (isUrlEntry(currentEntry)) return 'Copy URL'
    return 'Paste back'
  }, [currentEntry])

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      role="application"
      aria-label="Clipboard History"
      onKeyDown={onKeyDown}
      className="flex h-full min-h-0 w-full flex-col gap-2 outline-none animate-tezbar-scale-in"
    >
      <div className="glass-card shrink-0 px-4 py-3 animate-tezbar-scale-in">
        <ViewHeader
          title="Clipboard History"
          onBack={onBack}
        />

        {/* Search + filter chips */}
        <div className="mt-2 flex items-center gap-2">
          <div className="relative flex-1">
            <input
              ref={inputRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search clipboard history…"
              className="h-8 w-full rounded-tezbar-chip border border-white/10 bg-white/[0.04] px-2.5 text-[12px] text-ink-1 placeholder:text-ink-4 outline-none transition focus:border-white/20 focus:bg-white/[0.06]"
            />
          </div>
          <div className="flex items-center gap-1">
            {filters.map((f) => {
              const active = filter === f.id
              const count = countsByFilter[f.id]
              return (
                <button
                  key={f.id}
                  type="button"
                  onClick={() => setFilter(f.id)}
                  className={
                    active
                      ? 'rounded-tezbar-chip bg-white/[0.12] px-2 py-1 text-[11px] font-medium text-ink-1 transition'
                      : 'rounded-tezbar-chip px-2 py-1 text-[11px] text-ink-3 transition hover:bg-white/[0.06] hover:text-ink-1'
                  }
                >
                  <span>{f.label}</span>
                  {count > 0 && f.id !== 'all' ? (
                    <span className="ml-1 text-ink-4">{count}</span>
                  ) : null}
                </button>
              )
            })}
          </div>
        </div>
      </div>

      {/* Split pane */}
      <div className="glass-card flex min-h-0 flex-1 flex-col overflow-hidden px-2 py-2 animate-tezbar-scale-in">
        <section className="flex min-h-0 flex-1 gap-3">
          {/* Left: list */}
          <div className="flex min-h-0 w-[44%] max-w-[340px] flex-col overflow-hidden">
            {filtered.length === 0 ? (
              <EmptyState query={query} totalItems={entries.length} />
            ) : (
              <div className="min-h-0 flex-1 overflow-y-auto pr-0.5">
                <GlideList
                  selectedIndex={selected}
                  itemCount={filtered.length}
                  className="flex flex-col gap-0.5 py-0.5"
                >
                  {filtered.map((entry, i) => (
                    <ClipboardRow
                      key={entry.id}
                      entry={entry}
                      index={i}
                      selected={i === selected}
                      imageUrl={entry.kind === 'image' ? imageCache[entry.id]?.dataUrl ?? null : null}
                      onHover={() => setSelected(i)}
                      onClick={() => {
                        setSelected(i)
                        void copyBack(entry)
                      }}
                    />
                  ))}
                </GlideList>
              </div>
            )}
          </div>

          {/* Right: preview */}
          <div className="min-h-0 flex-1 overflow-hidden rounded-tezbar-row border border-white/[0.06] bg-white/[0.02]">
            {currentEntry ? (
              <PreviewPane entry={currentEntry} imageUrl={currentImageUrl} />
            ) : (
              <div className="grid h-full place-items-center px-6 text-center">
                <p className="text-[12px] text-ink-4">Your clipboard history preview will show up here.</p>
              </div>
            )}
          </div>
        </section>

        {msg ? (
          <div className="mt-2 shrink-0">
            <Message tone={msg.tone}>{msg.text}</Message>
          </div>
        ) : null}
      </div>

      <div className="glass-card flex shrink-0 items-center justify-between gap-3 px-4 py-2 animate-tezbar-scale-in">
        <span className="text-[10.5px] uppercase tracking-[0.14em] text-ink-4">
          {filtered.length} item{filtered.length === 1 ? '' : 's'}
          {filter !== 'all' ? ` · ${filters.find((f) => f.id === filter)?.label}` : ''}
        </span>
        <HintBar>
          <Hint label={primaryActionLabel ?? 'Copy'} keys={<Kbd>↵</Kbd>} />
          <Hint label="Pin" keys={<Kbd>P</Kbd>} />
          <Hint label="Delete" keys={<><Kbd>⌘</Kbd><Kbd>D</Kbd></>} />
          <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
        </HintBar>
      </div>
    </div>
  )
}

/* =========================================================================
   Row
   ========================================================================= */
function ClipboardRow({
  entry,
  index,
  selected,
  imageUrl,
  onHover,
  onClick,
}: {
  entry: ClipboardEntry
  index: number
  selected: boolean
  imageUrl: string | null
  onHover: () => void
  onClick: () => void
}): JSX.Element {
  const subtitle = useMemo(() => {
    if (entry.kind === 'image') return `${entry.width} × ${entry.height} · ${formatBytes(entry.byteSize)}`
    if (entry.kind === 'file') {
      return entry.paths.length > 1 ? `${entry.paths.length} paths` : entry.paths[0] ?? ''
    }
    if (isUrlEntry(entry)) return 'URL'
    return entry.charCount > 40 ? `${entry.charCount.toLocaleString()} chars` : ''
  }, [entry])

  return (
    <li className="relative z-[1]">
      <button
        type="button"
        onMouseEnter={onHover}
        onClick={onClick}
        className="flex w-full items-center gap-2 rounded-tezbar-row px-2 py-1.5 text-left transition"
      >
        <RowIcon entry={entry} imageUrl={imageUrl} />
        <div className="min-w-0 flex-1">
          <p className={`truncate text-[12.5px] ${selected ? 'text-ink-1' : 'text-ink-2'}`}>
            {entryPrimaryTitle(entry)}
          </p>
          {subtitle ? (
            <p className="mt-0.5 truncate text-[10.5px] text-ink-4">{subtitle}</p>
          ) : null}
        </div>
        <div className="flex items-center gap-1">
          {entry.pinned ? (
            <span title="Pinned" className="grid h-4 w-4 place-items-center rounded-tezbar-chip text-amber-300/80" aria-hidden>
              <svg width="10" height="10" viewBox="0 0 10 10" fill="currentColor">
                <path d="M5 .5 6 3l2.6.4-1.9 1.8.5 2.6L5 6.6 2.8 7.8l.5-2.6L1.4 3.4 4 3 5 .5Z" />
              </svg>
            </span>
          ) : null}
          {index < 9 ? (
            <span className="hidden items-center gap-0.5 font-mono text-[9px] text-ink-4 sm:inline-flex">
              <Kbd>⌘</Kbd>
              <Kbd>{index + 1}</Kbd>
            </span>
          ) : null}
        </div>
      </button>
    </li>
  )
}

/* =========================================================================
   Preview pane
   ========================================================================= */
function PreviewPane({
  entry,
  imageUrl,
}: {
  entry: ClipboardEntry
  imageUrl: string | null
}): JSX.Element {
  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        {entry.kind === 'text' ? (
          <TextPreview entry={entry} />
        ) : entry.kind === 'image' ? (
          <ImagePreview entry={entry} imageUrl={imageUrl} />
        ) : (
          <FilePreview entry={entry} />
        )}
      </div>
      <div className="hairline" />
      <div className="flex items-center justify-between gap-3 px-4 py-2 text-[11px] text-ink-4">
        <span>{entryKindLabel(entry)}</span>
        <span>{formatDate(entry.createdAt)}</span>
      </div>
    </div>
  )
}

function TextPreview({ entry }: { entry: Extract<ClipboardEntry, { kind: 'text' }> }): JSX.Element {
  if (entry.isSecret) {
    return (
      <div className="grid h-full place-items-center text-center">
        <div>
          <p className="text-[13px] font-medium text-ink-1">Secure item</p>
          <p className="mt-1 text-[11px] text-ink-4">This value is hidden. Press Enter to paste it back.</p>
        </div>
      </div>
    )
  }
  const isUrl = URL_RE.test(entry.text.trim())
  return (
    <pre
      className={`whitespace-pre-wrap break-words font-mono text-[12px] leading-[1.55] text-ink-1 ${isUrl ? 'text-sky-200' : ''
        }`}
    >
      {entry.text}
    </pre>
  )
}

function ImagePreview({
  entry,
  imageUrl,
}: {
  entry: Extract<ClipboardEntry, { kind: 'image' }>
  imageUrl: string | null
}): JSX.Element {
  return (
    <div className="flex h-full flex-col items-center gap-2">
      <div className="flex min-h-0 flex-1 items-center justify-center overflow-hidden rounded-tezbar-row border border-white/10 bg-black/20 p-2">
        {imageUrl ? (
          <img
            src={imageUrl}
            alt="Clipboard image"
            className="max-h-full max-w-full object-contain"
          />
        ) : (
          <p className="text-[11px] text-ink-4">Loading preview…</p>
        )}
      </div>
      <p className="text-[10.5px] text-ink-4">
        {entry.width} × {entry.height} · {formatBytes(entry.byteSize)}
      </p>
    </div>
  )
}

function FilePreview({ entry }: { entry: Extract<ClipboardEntry, { kind: 'file' }> }): JSX.Element {
  return (
    <ul className="flex flex-col gap-1">
      {entry.paths.map((path) => (
        <li
          key={path}
          className="flex items-center gap-2 rounded-tezbar-row border border-white/[0.06] bg-white/[0.02] px-2.5 py-1.5"
        >
          <svg width="12" height="12" viewBox="0 0 14 14" fill="none" className="text-ink-3" aria-hidden>
            <path
              d="M2.5 3.5a1 1 0 0 1 1-1h2.5l1 1.2H10.5a1 1 0 0 1 1 1V10a1 1 0 0 1-1 1h-7a1 1 0 0 1-1-1V3.5Z"
              stroke="currentColor"
              strokeWidth="1.2"
              strokeLinejoin="round"
            />
          </svg>
          <div className="min-w-0">
            <p className="truncate text-[12px] text-ink-1">{basename(path)}</p>
            <p className="truncate text-[10.5px] text-ink-4">{path}</p>
          </div>
        </li>
      ))}
    </ul>
  )
}

/* =========================================================================
   Empty state
   ========================================================================= */
function EmptyState({ query, totalItems }: { query: string; totalItems: number }): JSX.Element {
  if (totalItems === 0) {
    return (
      <div className="grid flex-1 place-items-center text-center">
        <div className="max-w-[240px]">
          <div className="mx-auto grid h-8 w-8 place-items-center rounded-full border border-white/10 bg-white/[0.04] text-ink-3">
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden>
              <rect x="3" y="2" width="8" height="10" rx="1.5" stroke="currentColor" strokeWidth="1.2" />
              <path d="M5.5 2V1.5A.5.5 0 0 1 6 1h2a.5.5 0 0 1 .5.5V2" stroke="currentColor" strokeWidth="1.2" />
            </svg>
          </div>
          <p className="mt-2 text-[12px] font-medium text-ink-2">Clipboard is empty</p>
          <p className="mt-1 text-[11px] text-ink-4">
            Copy something from any app and it will show up here.
          </p>
        </div>
      </div>
    )
  }

  return (
    <div className="grid flex-1 place-items-center text-center">
      <div className="max-w-[240px]">
        <p className="text-[12px] font-medium text-ink-2">No matches</p>
        <p className="mt-1 text-[11px] text-ink-4">
          Nothing matches {query ? `"${query}"` : 'this filter'}.
        </p>
      </div>
    </div>
  )
}
