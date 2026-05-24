import React, { useMemo, useState, useEffect, useRef } from 'react'
import type { ExtensionRuntimeNode } from '../../../shared/extensionRuntime'
import { Hint, HintBar, Kbd, ViewHeader } from '../../ui/primitives'

type ListRow = {
  id: string
  title: string
  subtitle: string
  icon?: {
    fileIcon?: string
  }
  section?: string
  actionIds?: string[]
}

function cleanSubtitle(value: unknown): string {
  const subtitle = typeof value === 'string' ? value.trim() : ''
  return /^by\s*$/i.test(subtitle) ? '' : subtitle
}

function parseRowIcon(value: unknown): ListRow['icon'] | undefined {
  if (!value || typeof value !== 'object') return undefined
  const icon = value as { fileIcon?: unknown }
  return typeof icon.fileIcon === 'string' && icon.fileIcon.trim()
    ? { fileIcon: icon.fileIcon.trim() }
    : undefined
}

function parseListRows(node: ExtensionRuntimeNode): ListRow[] {
  const rows: ListRow[] = []

  const walk = (entry: ExtensionRuntimeNode, section?: string): void => {
    if (entry.type === 'List.Item') {
      const title = typeof entry.props?.title === 'string' ? entry.props.title : 'Untitled'
      const subtitle = cleanSubtitle(entry.props?.subtitle)
      const icon = parseRowIcon(entry.props?.icon)
      const id = typeof entry.props?.id === 'string' ? entry.props.id : `${section || 'list'}:${rows.length}`
      const actionIds = Array.isArray(entry.props?.actionIds)
        ? entry.props.actionIds.filter((value): value is string => typeof value === 'string')
        : undefined
      rows.push({ id, title, subtitle, icon, section, actionIds })
      return
    }

    if (entry.type === 'List.Section') {
      const nextSection = typeof entry.props?.title === 'string' ? entry.props.title : section
      for (const child of entry.children ?? []) {
        walk(child, nextSection)
      }
      return
    }

    for (const child of entry.children ?? []) {
      walk(child, section)
    }
  }

  walk(node)
  return rows
}

function FileIcon({ path, title }: { path: string; title: string }): JSX.Element {
  const [src, setSrc] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    setSrc(null)
    void window.raymes.getAppIconDataUrl(path)
      .then((value) => {
        if (!cancelled) setSrc(value)
      })
      .catch(() => {
        if (!cancelled) setSrc(null)
      })
    return () => {
      cancelled = true
    }
  }, [path])

  if (src) {
    return <img src={src} alt="" className="h-7 w-7 shrink-0 rounded-[7px]" draggable={false} />
  }

  return (
    <span className="grid h-7 w-7 shrink-0 place-items-center rounded-[7px] border border-white/10 bg-white/[0.04] text-[11px] font-semibold text-ink-3">
      {title.slice(0, 1).toUpperCase()}
    </span>
  )
}

function groupBySection(rows: ListRow[]): { section: string | undefined; items: ListRow[] }[] {
  const groups: { section: string | undefined; items: ListRow[] }[] = []
  let currentSection: string | undefined | null = null

  for (const row of rows) {
    if (row.section !== currentSection || groups.length === 0) {
      currentSection = row.section
      groups.push({ section: row.section, items: [] })
    }
    const last = groups[groups.length - 1]
    if (last) last.items.push(row)
  }

  return groups
}

function clientSideFilter(rows: ListRow[], query: string): ListRow[] {
  const q = query.trim().toLowerCase()
  if (!q) return rows
  return rows.filter((row) => {
    return [row.title, row.subtitle, row.section]
      .filter((value): value is string => Boolean(value))
      .some((value) => value.toLowerCase().includes(q))
  })
}

export function ListRuntime({
  root,
  title,
  onBack,
  onRunPrimaryAction,
  onSearchTextChanged,
}: {
  root: ExtensionRuntimeNode
  title: string
  onBack: () => void
  onRunPrimaryAction: (actionId?: string) => void
  onOpenActions: () => void
  onSearchTextChanged: (searchText: string) => Promise<void> | void
}): JSX.Element {
  const inputRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLDivElement>(null)

  const rows = useMemo(() => {
    const parsed = parseListRows(root)
    console.log(`[ListRuntime] Received root type="${root.type}", parsed ${parsed.length} rows`, root.children?.length ?? 0, 'children')
    return parsed
  }, [root])
  const searchBarPlaceholder = useMemo(() => {
    return typeof root.props?.searchBarPlaceholder === 'string' && root.props.searchBarPlaceholder.trim().length > 0
      ? root.props.searchBarPlaceholder
      : 'Search applications'
  }, [root.props])

  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState(0)

  const hasServerSearch = onSearchTextChanged !== undefined

  const debounceTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const lastSentQuery = useRef('')

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  useEffect(() => {
    if (!hasServerSearch) return
    if (query === lastSentQuery.current) return
    if (debounceTimer.current) {
      clearTimeout(debounceTimer.current)
    }
    debounceTimer.current = setTimeout(() => {
      lastSentQuery.current = query
      void onSearchTextChanged(query)
    }, 200)
    return () => {
      if (debounceTimer.current) {
        clearTimeout(debounceTimer.current)
      }
    }
  }, [query, hasServerSearch, onSearchTextChanged])

  const filteredRows = useMemo(() => {
    if (hasServerSearch) return rows
    return clientSideFilter(rows, query)
  }, [hasServerSearch, rows, query])

  const groupedSections = useMemo(() => groupBySection(filteredRows), [filteredRows])

  useEffect(() => {
    if (selected >= filteredRows.length) {
      setSelected(Math.max(0, filteredRows.length - 1))
    }
  }, [filteredRows.length, selected])

  useEffect(() => {
    const el = listRef.current?.querySelector(`[data-idx="${selected}"]`)
    if (el) {
      el.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
    }
  }, [selected])

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>): void => {
    if (filteredRows.length === 0) return

    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setSelected((i) => Math.min(i + 1, filteredRows.length - 1))
      return
    }

    if (e.key === 'ArrowUp') {
      e.preventDefault()
      setSelected((i) => Math.max(i - 1, 0))
      return
    }

    if (e.key === 'Enter' && !e.repeat) {
      e.preventDefault()
      onRunPrimaryAction(filteredRows[selected]?.actionIds?.[0])
      return
    }
  }

  const emptyView = useMemo(() => {
    const candidate = (root.children ?? []).find((entry) => entry.type === 'List.EmptyView')
    if (!candidate) return null
    return {
      title: typeof candidate.props?.title === 'string' ? candidate.props.title : 'No results',
      description:
        typeof candidate.props?.description === 'string' ? candidate.props.description : '',
    }
  }, [root.children])

  const hasQuery = query.trim().length > 0

  return (
    <div
      className="flex h-full min-h-0 w-full flex-col gap-2 outline-none animate-raymes-scale-in"
      onKeyDown={onKeyDown}
    >
      <div className="glass-card shrink-0 px-4 py-3 animate-raymes-scale-in">
        <ViewHeader title={title} onBack={onBack} />

        <div className="mt-2">
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(event) => {
              setQuery(event.target.value)
              setSelected(0)
            }}
            placeholder={searchBarPlaceholder}
            className="h-8 w-full rounded-raymes-chip border border-white/10 bg-white/[0.04] px-2.5 text-[12px] text-ink-1 placeholder:text-ink-4 outline-none transition focus:border-white/20 focus:bg-white/[0.06]"
            aria-label={searchBarPlaceholder}
          />
        </div>
      </div>

      <div ref={listRef} className="glass-card flex min-h-0 flex-1 flex-col overflow-hidden px-2 py-2 animate-raymes-scale-in">
        {filteredRows.length === 0 ? (
          <div className="flex h-full items-center justify-center text-center">
            <div>
              <p className="text-[13px] text-ink-2">
                {hasQuery ? 'No matching applications' : emptyView?.title || 'No list items'}
              </p>
              {hasQuery ? (
                <p className="mt-1 text-[11px] text-ink-4">Try a different app name or keyword.</p>
              ) : emptyView?.description ? (
                <p className="mt-1 text-[11px] text-ink-4">{emptyView.description}</p>
              ) : null}
            </div>
          </div>
        ) : (
          <div className="min-h-0 flex-1 overflow-y-auto px-1 py-1">
            {groupedSections.map((group, groupIdx) => (
              <div key={groupIdx} className="mb-1">
                {group.section ? (
                  <div className="px-2 pb-1 pt-1 text-[11px] font-medium tracking-[0.08em] text-ink-4 select-none">
                    {group.section}
                  </div>
                ) : null}
                <ul className="flex flex-col gap-0.5">
                  {group.items.map((row, localIdx) => {
                    const globalIdx = groupedSections
                      .slice(0, groupIdx)
                      .reduce((sum, g) => sum + g.items.length, 0) + localIdx
                    return (
                      <li key={row.id}>
                        <button
                          type="button"
                          data-idx={globalIdx}
                          onMouseEnter={() => setSelected(globalIdx)}
                          onClick={() => onRunPrimaryAction(row.actionIds?.[0])}
                          className={`w-full rounded-raymes-row px-3 py-2 text-left transition ${
                            globalIdx === selected ? 'bg-white/15 text-ink-1' : 'text-ink-2 hover:bg-white/8'
                          }`}
                        >
                          <span className="flex min-w-0 items-center gap-2.5">
                            {row.icon?.fileIcon ? (
                              <FileIcon path={row.icon.fileIcon} title={row.title} />
                            ) : null}
                            <span className="min-w-0 flex-1">
                              <p className="truncate text-[13px] font-medium">{row.title}</p>
                              {row.subtitle ? <p className="truncate text-[11px] text-ink-3">{row.subtitle}</p> : null}
                            </span>
                          </span>
                        </button>
                      </li>
                    )
                  })}
                </ul>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="glass-card flex shrink-0 items-center justify-between gap-3 px-4 py-2 animate-raymes-scale-in">
        <span className="text-[10.5px] uppercase tracking-[0.14em] text-ink-4">
          {filteredRows.length} item{filteredRows.length === 1 ? '' : 's'}
          {hasQuery ? ` · filtered` : ''}
        </span>
        <HintBar>
          <Hint label="Select" keys={<><Kbd>↑</Kbd><Kbd>↓</Kbd></>} />
          <Hint label="Run" keys={<Kbd>↵</Kbd>} />
          <Hint label="Actions" keys={<><Kbd>⌘</Kbd><Kbd>K</Kbd></>} />
          <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
        </HintBar>
      </div>
    </div>
  )
}
