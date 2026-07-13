import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type {
  KnowledgeSnapshot,
  KnowledgeSourceSummary,
  KnowledgeSourcesPage,
} from '../shared/knowledge'
import { Button, cx, Hint, HintBar, Kbd, TextField, ViewHeader } from './ui/primitives'

const PAGE_SIZE = 200

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

function formatDate(value?: number): string {
  if (!value) return 'Not completed yet'
  return new Date(value).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  })
}

function statusLabel(snapshot: KnowledgeSnapshot | null): string {
  const state = snapshot?.status.state
  if (state === 'scanning') return 'Scanning folders'
  if (state === 'indexing') return 'Indexing now'
  if (state === 'paused') return 'Paused'
  if (state === 'failed') return 'Needs attention'
  if (state === 'completed') return 'Up to date'
  return 'Standing by'
}

function sourceMeta(source: KnowledgeSourceSummary): string {
  const pageCoverage = source.totalPageCount > 1
    ? ` · ${source.indexedPageCount}/${source.totalPageCount} pages`
    : ''
  const update = source.indexedAt ? ` · ${formatDate(source.indexedAt)}` : ''
  return `${formatBytes(source.byteSize)}${pageCoverage}${update}`
}

function SourceIcon({ status }: { status: KnowledgeSourceSummary['status'] }): JSX.Element {
  return (
    <span
      aria-hidden
      className={cx(
        'grid h-8 w-8 shrink-0 place-items-center rounded-[9px] border',
        status === 'failed'
          ? 'border-rose-300/20 bg-rose-300/[0.08] text-rose-200'
          : status === 'pending'
            ? 'border-cyan-300/20 bg-cyan-300/[0.08] text-cyan-200'
            : 'border-white/10 bg-white/[0.04] text-ink-3',
      )}
    >
      <svg width="15" height="15" viewBox="0 0 16 16" fill="none">
        <path d="M4 2.25h5l3 3v8.5H4v-11.5Z" stroke="currentColor" strokeWidth="1.15" strokeLinejoin="round" />
        <path d="M9 2.5v3h3M6.25 8h3.5M6.25 10.25h3.5" stroke="currentColor" strokeWidth="1.15" strokeLinecap="round" />
      </svg>
    </span>
  )
}

export default function IndexingView({
  onBack,
  onOpenSettings,
}: {
  onBack: () => void
  onOpenSettings: () => void
}): JSX.Element {
  const rootRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const requestIdRef = useRef(0)
  const [snapshot, setSnapshot] = useState<KnowledgeSnapshot | null>(null)
  const [page, setPage] = useState<KnowledgeSourcesPage>({
    sources: [], total: 0, offset: 0, hasMore: false,
  })
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState(0)
  const [loading, setLoading] = useState(true)

  const loadSources = useCallback(async (offset = 0, append = false): Promise<void> => {
    const requestId = ++requestIdRef.current
    const next = await window.tezbar.listKnowledgeSources({ query, offset, limit: PAGE_SIZE })
    if (requestId !== requestIdRef.current) return
    setPage((current) => append
      ? { ...next, sources: [...current.sources, ...next.sources] }
      : next)
    if (!append) setSelected(0)
  }, [query])

  const loadSnapshot = useCallback(async (): Promise<void> => {
    setSnapshot(await window.tezbar.getKnowledgeSnapshot())
  }, [])

  const refresh = useCallback(async (): Promise<void> => {
    setLoading(true)
    try {
      await Promise.all([loadSnapshot(), loadSources(0, false)])
    } finally {
      setLoading(false)
    }
  }, [loadSnapshot, loadSources])

  useEffect(() => {
    void loadSnapshot().finally(() => setLoading(false))
  }, [loadSnapshot])

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void loadSources(0, false)
    }, 160)
    return () => window.clearTimeout(timer)
  }, [loadSources])

  useEffect(() => window.tezbar.onKnowledgeStatus((status) => {
    setSnapshot((current) => current ? { ...current, status } : current)
    if (status.state === 'completed') void refresh()
  }), [refresh])

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  useEffect(() => {
    const source = page.sources[selected]
    if (!source) return
    document.getElementById(`knowledge-source-${source.id}`)?.scrollIntoView({ block: 'nearest' })
  }, [page.sources, selected])

  const selectedSource = page.sources[selected]
  const progress = Math.round((snapshot?.status.progress ?? 0) * 100)
  const isActive = snapshot?.status.state === 'indexing' || snapshot?.status.state === 'scanning'
  const depth = snapshot?.settings.depth ?? 'smart'

  const metricCards = useMemo(() => [
    {
      label: 'Indexed files',
      value: (snapshot?.status.sourceCount ?? 0).toLocaleString(),
      note: `${formatBytes(snapshot?.status.sourceBytes ?? 0)} source data`,
    },
    {
      label: 'Local index',
      value: formatBytes(snapshot?.storageBytes ?? 0),
      note: `${(snapshot?.status.chunkCount ?? 0).toLocaleString()} searchable chunks`,
    },
    {
      label: 'Pages understood',
      value: (snapshot?.status.totalPageCount ?? 0) > 0
        ? `${(snapshot?.status.indexedPageCount ?? 0).toLocaleString()} / ${(snapshot?.status.totalPageCount ?? 0).toLocaleString()}`
        : '—',
      note: (snapshot?.status.partialSourceCount ?? 0) > 0
        ? `${snapshot?.status.partialSourceCount} partially indexed`
        : 'No partial documents',
    },
    {
      label: 'Last update',
      value: snapshot?.status.lastCompletedAt ? formatDate(snapshot.status.lastCompletedAt) : 'Not yet',
      note: `${depth[0]?.toUpperCase()}${depth.slice(1)} depth`,
    },
  ], [depth, snapshot])

  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>): void => {
    if (event.key === 'Escape') {
      event.preventDefault()
      event.stopPropagation()
      if (query && document.activeElement === inputRef.current) {
        setQuery('')
      } else {
        onBack()
      }
      return
    }
    if ((event.metaKey || event.ctrlKey) && event.key === ',') {
      event.preventDefault()
      onOpenSettings()
      return
    }
    if (event.key === 'ArrowDown' && page.sources.length > 0) {
      event.preventDefault()
      setSelected((current) => Math.min(current + 1, page.sources.length - 1))
      return
    }
    if (event.key === 'ArrowUp' && page.sources.length > 0) {
      event.preventDefault()
      setSelected((current) => Math.max(0, current - 1))
      return
    }
    if (event.key === 'Enter' && selectedSource) {
      event.preventDefault()
      void window.tezbar.shellOpen(selectedSource.path)
    }
  }

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      role="application"
      aria-label="Indexing Status"
      onKeyDown={onKeyDown}
      className="flex h-full min-h-0 w-full flex-col gap-2 outline-none animate-tezbar-scale-in"
    >
      <section className="glass-card shrink-0 overflow-hidden px-4 py-3">
        <ViewHeader
          title="Indexing"
          onBack={onBack}
          trailing={(
            <div className="flex items-center gap-2">
              <Button variant="quiet" disabled={loading} onClick={() => void refresh()}>
                {loading ? 'Refreshing' : 'Refresh'}
              </Button>
              <Button variant="ghost" onClick={onOpenSettings}>Edit in Settings</Button>
            </div>
          )}
        />

        <div className="relative mt-2 overflow-hidden rounded-[14px] border border-white/[0.08] bg-[linear-gradient(120deg,rgba(43,161,207,0.13),rgba(255,255,255,0.025)_58%)] p-3.5">
          <div aria-hidden className="absolute -right-12 -top-20 h-40 w-40 rounded-full bg-cyan-300/10 blur-3xl" />
          <div className="relative flex items-start justify-between gap-5">
            <div className="min-w-0">
              <div className="flex items-center gap-2.5">
                <span className={cx(
                  'h-2 w-2 rounded-full',
                  isActive ? 'animate-pulse bg-cyan-300' : snapshot?.status.state === 'failed'
                    ? 'bg-rose-300' : snapshot?.status.state === 'paused' ? 'bg-amber-300' : 'bg-emerald-300',
                )} />
                <h2 className="font-display text-[15px] font-semibold text-ink-1">{statusLabel(snapshot)}</h2>
                <span className="rounded-full border border-white/[0.08] bg-black/10 px-2 py-0.5 text-[9px] font-semibold uppercase tracking-[0.1em] text-ink-3">
                  {depth}
                </span>
              </div>
              <p className="mt-1.5 max-w-[630px] truncate text-[10.5px] text-ink-3">
                {snapshot?.status.detail ?? 'Preparing local knowledge status…'}
              </p>
            </div>
            <span className="font-mono text-[19px] font-medium tabular-nums text-cyan-100">{progress}%</span>
          </div>
          <div className="relative mt-3 h-1.5 overflow-hidden rounded-full bg-black/20">
            <div
              className="h-full rounded-full bg-[linear-gradient(90deg,#5ea7ff,#6de1c2)] transition-[width] duration-300"
              style={{ width: `${progress}%` }}
            />
          </div>
          <div className="relative mt-1.5 flex justify-between font-mono text-[9.5px] text-ink-4">
            <span>{snapshot?.status.processedSources ?? 0} processed</span>
            <span>{snapshot?.status.queuedSources ?? 0} remaining · {snapshot?.status.failedSources ?? 0} failed</span>
          </div>
        </div>

        <div className="mt-2.5 grid grid-cols-4 gap-2">
          {metricCards.map((metric) => (
            <div key={metric.label} className="rounded-[11px] border border-white/[0.07] bg-white/[0.025] px-3 py-2.5">
              <p className="text-[9px] font-medium uppercase tracking-[0.11em] text-ink-4">{metric.label}</p>
              <p className="mt-1 truncate font-display text-[13px] font-semibold text-ink-1" title={metric.value}>{metric.value}</p>
              <p className="mt-0.5 truncate text-[9.5px] text-ink-4">{metric.note}</p>
            </div>
          ))}
        </div>
      </section>

      <section className="glass-card flex min-h-0 flex-1 flex-col overflow-hidden px-3 py-3">
        <div className="flex shrink-0 items-center gap-2 px-1 pb-2.5">
          <TextField
            ref={inputRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Filter indexed files by name or path…"
            aria-label="Filter indexed files"
            className="h-8 flex-1 text-[11.5px]"
          />
          <span className="shrink-0 font-mono text-[10px] text-ink-4">
            {page.sources.length.toLocaleString()} / {page.total.toLocaleString()}
          </span>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto rounded-[12px] border border-white/[0.07] bg-black/[0.08] p-1.5">
          {page.sources.length > 0 ? (
            <div className="space-y-1">
              {page.sources.map((source, index) => (
                <button
                  key={source.id}
                  id={`knowledge-source-${source.id}`}
                  type="button"
                  onClick={() => setSelected(index)}
                  onDoubleClick={() => void window.tezbar.shellOpen(source.path)}
                  className={cx(
                    'flex w-full items-center gap-3 rounded-[10px] border px-2.5 py-2 text-left transition',
                    index === selected
                      ? 'border-cyan-300/20 bg-cyan-300/[0.075]'
                      : 'border-transparent hover:bg-white/[0.035]',
                  )}
                >
                  <SourceIcon status={source.status} />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[11.5px] font-medium text-ink-1">{source.title}</span>
                    <span className="mt-0.5 block truncate font-mono text-[9.5px] text-ink-4">{source.path}</span>
                  </span>
                  <span className="max-w-[240px] shrink-0 text-right">
                    <span className={cx(
                      'block text-[9px] font-semibold uppercase tracking-[0.08em]',
                      source.status === 'failed' ? 'text-rose-300' : source.status === 'pending' ? 'text-cyan-200' : 'text-emerald-300/80',
                    )}>
                      {source.status}
                    </span>
                    <span className="mt-0.5 block truncate font-mono text-[9px] text-ink-4" title={sourceMeta(source)}>
                      {sourceMeta(source)}
                    </span>
                  </span>
                </button>
              ))}
              {page.hasMore ? (
                <Button
                  variant="quiet"
                  fullWidth
                  className="mt-1"
                  onClick={() => void loadSources(page.sources.length, true)}
                >
                  Load next {Math.min(PAGE_SIZE, page.total - page.sources.length)} files
                </Button>
              ) : null}
            </div>
          ) : (
            <div className="grid h-full min-h-[120px] place-items-center text-center">
              <div>
                <p className="text-[11.5px] text-ink-2">{loading ? 'Loading indexed files…' : 'No files match this filter.'}</p>
                <p className="mt-1 text-[10px] text-ink-4">Folder and depth changes live in Knowledge Settings.</p>
              </div>
            </div>
          )}
        </div>
      </section>

      <div className="glass-card shrink-0 px-3 py-2">
        <HintBar>
          <Hint label="Navigate" keys={<><Kbd>↑</Kbd><Kbd>↓</Kbd></>} />
          <Hint label="Open file" keys={<Kbd>↵</Kbd>} />
          <Hint label="Knowledge Settings" keys={<><Kbd>⌘</Kbd><Kbd>,</Kbd></>} />
          <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
        </HintBar>
      </div>
    </div>
  )
}
