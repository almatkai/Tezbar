import React, { useMemo, useState, useEffect, useRef } from 'react'
import type {
  ExtensionRuntimeAction,
  ExtensionRuntimeNode,
} from '../../../shared/extensionRuntime'
import { Hint, HintBar, Kbd, ViewHeader } from '../../ui/primitives'
import { Markdown } from '../../ui/Markdown'
import { MetadataItem, MetadataSidebar } from './detail-runtime'

type ListRow = {
  id: string
  title: string
  subtitle: string
  icon?: {
    fileIcon?: string
    source?: unknown
  }
  accessories?: Array<{ text?: unknown; tag?: unknown; date?: unknown; icon?: unknown }>
  detail?: ExtensionRuntimeNode
  section?: string
  actionIds?: string[]
}

type ListAccessory = {
  actionId?: string
  options: Array<{ title: string; value: string }>
}

function parseListAccessory(value: unknown): ListAccessory | null {
  if (!value || typeof value !== 'object' || !('type' in value)) return null
  const node = value as ExtensionRuntimeNode
  if (node.type !== 'List.Dropdown') return null

  const options: ListAccessory['options'] = []
  const walk = (entry: ExtensionRuntimeNode): void => {
    if (entry.type === 'List.Dropdown.Item') {
      const title = textValue(entry.props?.title)
      const value = textValue(entry.props?.value) || title
      if (title || value) options.push({ title: title || value, value })
      return
    }
    for (const child of entry.children ?? []) walk(child)
  }
  walk(node)

  return {
    actionId: typeof node.props?.actionId === 'string' ? node.props.actionId : undefined,
    options,
  }
}

function cleanSubtitle(value: unknown): string {
  const subtitle = typeof value === 'string' ? value.trim() : ''
  return /^by\s*$/i.test(subtitle) ? '' : subtitle
}

function parseRowIcon(value: unknown): ListRow['icon'] | undefined {
  if (typeof value === 'string' && value.trim()) {
    return { source: value.trim() }
  }
  if (!value || typeof value !== 'object') return undefined
  const icon = value as { fileIcon?: unknown }
  if (typeof icon.fileIcon === 'string' && icon.fileIcon.trim()) {
    return { fileIcon: icon.fileIcon.trim() }
  }
  if ('source' in icon) return { source: (icon as { source?: unknown }).source }
  return undefined
}

function textValue(value: unknown): string {
  if (typeof value === 'string') return value
  if (typeof value === 'number' || typeof value === 'boolean') return String(value)
  if (value && typeof value === 'object') {
    const candidate = value as { value?: unknown; text?: unknown }
    if (candidate.value !== undefined) return textValue(candidate.value)
    if (candidate.text !== undefined) return textValue(candidate.text)
  }
  return ''
}

function parseAccessories(value: unknown): ListRow['accessories'] | undefined {
  if (!Array.isArray(value)) return undefined
  return value
    .filter((entry): entry is Record<string, unknown> => Boolean(entry) && typeof entry === 'object')
    .map((entry) => ({
      text: entry.text,
      tag: entry.tag,
      date: entry.date,
      icon: entry.icon,
    }))
}

function parseDetailFromChildren(entry: ExtensionRuntimeNode): ExtensionRuntimeNode | undefined {
  const propDetail = entry.props?.detail
  if (propDetail && typeof propDetail === 'object' && 'type' in propDetail) {
    return propDetail as ExtensionRuntimeNode
  }
  return (entry.children ?? []).find((child) => child.type === 'List.Item.Detail')
}

function parseListRows(node: ExtensionRuntimeNode): ListRow[] {
  const rows: ListRow[] = []

  const walk = (entry: ExtensionRuntimeNode, section?: string): void => {
    if (entry.type === 'List.Item') {
      const title = typeof entry.props?.title === 'string' ? entry.props.title : 'Untitled'
      const subtitle = cleanSubtitle(entry.props?.subtitle)
      const icon = parseRowIcon(entry.props?.icon)
      const id = typeof entry.props?.id === 'string' ? entry.props.id : `${section || 'list'}:${rows.length}`
      const accessories = parseAccessories(entry.props?.accessories)
      const detail = parseDetailFromChildren(entry)
      const actionIds = Array.isArray(entry.props?.actionIds)
        ? entry.props.actionIds.filter((value): value is string => typeof value === 'string')
        : undefined
      rows.push({ id, title, subtitle, icon, accessories, detail, section, actionIds })
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

function SymbolIcon({ icon, title }: { icon: unknown; title: string }): JSX.Element {
  const token = textValue(icon && typeof icon === 'object' ? (icon as { source?: unknown }).source : icon)
    .replace(/^Icon\./, '')
    .toLowerCase()
  const label = token || title.slice(0, 1).toLowerCase()

  return (
    <span className="grid h-5 w-5 shrink-0 place-items-center text-accent-1" aria-hidden="true">
      <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="2">
        {label.includes('check') ? (
          <>
            <circle cx="12" cy="12" r="9" />
            <path d="m8 12 2.6 2.6L16.5 9" />
          </>
        ) : label.includes('xmark') || label.includes('close') ? (
          <>
            <circle cx="12" cy="12" r="9" />
            <path d="m9 9 6 6m0-6-6 6" />
          </>
        ) : label.includes('minus') ? (
          <>
            <circle cx="12" cy="12" r="9" />
            <path d="M8 12h8" />
          </>
        ) : label.includes('exclamation') ? (
          <>
            <circle cx="12" cy="12" r="9" />
            <path d="M12 7v6m0 4h.01" />
          </>
        ) : label.includes('globe') || label.includes('network') || label.includes('wifi') ? (
          <>
            <circle cx="12" cy="12" r="9" />
            <path d="M3 12h18M12 3c2.5 2.7 3.7 5.7 3.7 9S14.5 18.3 12 21M12 3c-2.5 2.7-3.7 5.7-3.7 9S9.5 18.3 12 21" />
          </>
        ) : label.includes('download') ? (
          <path d="M12 3v12m0 0 4-4m-4 4-4-4M5 19h14" />
        ) : label.includes('upload') ? (
          <path d="M12 21V9m0 0 4 4m-4-4-4 4M5 5h14" />
        ) : label.includes('link') ? (
          <path d="M10 13a5 5 0 0 0 7.1 0l1.4-1.4a5 5 0 0 0-7.1-7.1L10.5 5M14 11a5 5 0 0 0-7.1 0l-1.4 1.4a5 5 0 0 0 7.1 7.1l.9-.9" />
        ) : label.includes('phone') || label.includes('voice') ? (
          <path d="M22 16.9v3a2 2 0 0 1-2.2 2A19.8 19.8 0 0 1 2.1 4.2 2 2 0 0 1 4.1 2h3a2 2 0 0 1 2 1.7l.5 2.8a2 2 0 0 1-.6 1.8L7.9 9.4a16 16 0 0 0 6.7 6.7l1.1-1.1a2 2 0 0 1 1.8-.6l2.8.5a2 2 0 0 1 1.7 2Z" />
        ) : label.includes('video') || label.includes('play') ? (
          <>
            <rect x="3" y="6" width="18" height="12" rx="2" />
            <path d="m10 9 5 3-5 3V9Z" />
          </>
        ) : label.includes('server') || label.includes('harddrive') ? (
          <>
            <rect x="3" y="5" width="18" height="14" rx="2" />
            <path d="M7 15h.01M11 15h6" />
          </>
        ) : label.includes('question') || label.includes('ping') ? (
          <>
            <circle cx="12" cy="12" r="9" />
            <path d="M9.5 9a2.8 2.8 0 0 1 5.2 1.4c0 2-2.7 2.2-2.7 4M12 18h.01" />
          </>
        ) : (
          <>
            <circle cx="12" cy="12" r="8" />
            <path d="M12 8v8M8 12h8" />
          </>
        )}
      </svg>
    </span>
  )
}

function RowIcon({ row }: { row: ListRow }): JSX.Element | null {
  if (row.icon?.fileIcon) return <FileIcon path={row.icon.fileIcon} title={row.title} />
  if (typeof row.icon?.source === 'string' && /^(?:data:image|https?:|file:)/i.test(row.icon.source)) {
    return <img src={row.icon.source} alt="" className="h-5 w-5 shrink-0" draggable={false} />
  }
  if (row.icon?.source) return <SymbolIcon icon={row.icon.source} title={row.title} />
  return null
}

function accessoryText(accessory: NonNullable<ListRow['accessories']>[number]): string {
  const tag = textValue(accessory.tag)
  if (tag) return tag
  const text = textValue(accessory.text)
  if (text) return text
  if (accessory.date) {
    const date = new Date(String(accessory.date))
    if (!Number.isNaN(date.getTime())) return date.toLocaleDateString()
  }
  return ''
}

function Accessory({
  accessory,
}: {
  accessory: NonNullable<ListRow['accessories']>[number]
}): JSX.Element | null {
  const value = accessoryText(accessory)
  if (!value) return null
  const isTag = Boolean(accessory.tag)
  const tagColor = textValue(
    accessory.tag && typeof accessory.tag === 'object'
      ? (accessory.tag as { color?: unknown }).color
      : '',
  ).toLowerCase()
  const colorClass =
    tagColor.includes('blue')
      ? 'bg-blue-500/20 text-blue-300'
      : tagColor.includes('orange')
        ? 'bg-orange-500/20 text-orange-300'
        : tagColor.includes('red')
          ? 'bg-red-500/20 text-red-300'
          : tagColor.includes('green')
            ? 'bg-emerald-500/20 text-emerald-300'
            : 'bg-white/10 text-ink-2'

  return (
    <span
      className={
        isTag
          ? `max-w-[190px] shrink-0 truncate rounded-[8px] px-2 py-1 text-[11px] font-medium ${colorClass}`
          : 'max-w-[240px] shrink-0 truncate text-[12px] text-ink-3'
      }
    >
      {value}
    </span>
  )
}

function markdownFromListDetail(detail: ExtensionRuntimeNode | undefined): string {
  if (!detail) return ''
  if (typeof detail.props?.markdown === 'string') return detail.props.markdown
  for (const child of detail.children ?? []) {
    if (typeof child.props?.markdown === 'string') return child.props.markdown
  }
  return ''
}

function metadataFromListDetail(detail: ExtensionRuntimeNode | undefined): ExtensionRuntimeNode | undefined {
  if (!detail) return undefined
  const propMetadata = detail.props?.metadata
  if (propMetadata && typeof propMetadata === 'object' && 'type' in propMetadata) {
    return propMetadata as ExtensionRuntimeNode
  }
  if (detail.metadata) return detail.metadata
  return (detail.children ?? []).find((child) => child.type === 'List.Item.Detail.Metadata')
}

function InlineMetadata({ root }: { root: ExtensionRuntimeNode }): JSX.Element {
  return (
    <div className="h-full overflow-y-auto px-4 py-4">
      <div className="max-w-[520px]">
        {(root.children ?? []).map((child, index) => (
          <MetadataItem key={index} node={child} />
        ))}
      </div>
    </div>
  )
}

function ListDetailPane({ row }: { row?: ListRow }): JSX.Element {
  const markdown = markdownFromListDetail(row?.detail)
  const metadata = metadataFromListDetail(row?.detail)

  if (!row?.detail) {
    return (
      <div className="flex h-full items-center justify-center px-6 text-center text-[12px] text-ink-4">
        Select an item to see details.
      </div>
    )
  }

  return (
    <div className="flex h-full min-h-0 overflow-hidden">
      {metadata && !markdown ? (
        <InlineMetadata root={metadata} />
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
        {markdown ? (
          <article className="prose prose-invert max-w-none text-[13px] leading-relaxed">
            <Markdown text={markdown} className="text-[13px] leading-relaxed" />
          </article>
        ) : metadata ? null : (
          <div className="text-[12px] text-ink-4">No detail content</div>
        )}
        </div>
      )}
      {metadata && markdown ? <MetadataSidebar root={metadata} /> : null}
    </div>
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
  actions,
  onSearchTextChanged,
  onLoadMore,
}: {
  root: ExtensionRuntimeNode
  title: string
  onBack: () => void
  onRunPrimaryAction: (actionId?: string, formValues?: Record<string, string>) => void
  actions: ExtensionRuntimeAction[]
  onOpenActions: () => void
  onSearchTextChanged: (searchText: string) => Promise<void> | void
  onLoadMore: () => Promise<void> | void
}): JSX.Element {
  const inputRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLDivElement>(null)

  const rows = useMemo(() => {
    const parsed = parseListRows(root)
    return parsed
  }, [root])
  const searchBarPlaceholder = useMemo(() => {
    return typeof root.props?.searchBarPlaceholder === 'string' && root.props.searchBarPlaceholder.trim().length > 0
      ? root.props.searchBarPlaceholder
      : 'Search applications'
  }, [root.props])
  const navigationTitle =
    typeof root.props?.navigationTitle === 'string' && root.props.navigationTitle.trim()
      ? root.props.navigationTitle
      : title
  const searchAccessory = useMemo(
    () => parseListAccessory(root.props?.searchBarAccessory),
    [root.props?.searchBarAccessory],
  )
  const hasServerSearch = onSearchTextChanged !== undefined && root.props?.__hasServerSearch === true
  const hasMore = root.props?.__hasMore === true
  const shouldShowSearch = hasServerSearch || Boolean(searchAccessory) || !root.props?.navigationTitle

  const [query, setQuery] = useState(
    typeof root.props?.searchText === 'string' ? root.props.searchText : '',
  )
  const [selected, setSelected] = useState(0)
  const [accessoryValue, setAccessoryValue] = useState(
    searchAccessory?.options[0]?.value ?? '',
  )
  const [loadingMore, setLoadingMore] = useState(false)

  const debounceTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const lastSentQuery = useRef('')
  const loadingMoreRef = useRef(false)

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  useEffect(() => {
    if (typeof root.props?.searchText === 'string') {
      setQuery(root.props.searchText)
    }
  }, [root.props?.searchText])

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
  const selectedRow = filteredRows[selected]
  const selectedAction = actions.find((action) => selectedRow?.actionIds?.[0] === action.id)
  const hasDetails = filteredRows.some((row) => row.detail)

  const requestMore = async (): Promise<void> => {
    if (!hasMore || loadingMoreRef.current) return
    loadingMoreRef.current = true
    setLoadingMore(true)
    try {
      await onLoadMore()
    } finally {
      loadingMoreRef.current = false
      setLoadingMore(false)
    }
  }

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
      const next = Math.min(selected + 1, filteredRows.length - 1)
      setSelected(next)
      if (hasMore && next >= filteredRows.length - 3) void requestMore()
      return
    }

    if (e.key === 'ArrowUp') {
      e.preventDefault()
      setSelected((i) => Math.max(i - 1, 0))
      return
    }

    if (e.key === 'Enter' && !e.repeat) {
      e.preventDefault()
      const actionId = filteredRows[selected]?.actionIds?.[0]
      if (actionId) onRunPrimaryAction(actionId)
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
  const isFirstTimePackageLoad =
    filteredRows.length === 0 &&
    emptyView?.title === 'Loading Packages'

  return (
    <div
      className="flex h-full min-h-0 w-full flex-col gap-2 outline-none animate-raymes-scale-in"
      onKeyDown={onKeyDown}
    >
      <div className="glass-card shrink-0 px-4 py-3 animate-raymes-scale-in">
        <ViewHeader title={navigationTitle} onBack={onBack} />

        {shouldShowSearch ? (
          <div className="mt-2 flex items-center gap-2">
            <div className="min-w-0 flex-1">
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
            {searchAccessory && searchAccessory.options.length > 0 ? (
              <div className="flex shrink-0 items-center gap-1">
                {searchAccessory.options.map((option) => {
                  const active = accessoryValue === option.value
                  return (
                    <button
                      key={option.value}
                      type="button"
                      onClick={() => {
                        setAccessoryValue(option.value)
                        setSelected(0)
                        if (searchAccessory.actionId) {
                          onRunPrimaryAction(searchAccessory.actionId, { value: option.value })
                        }
                      }}
                      className={
                        active
                          ? 'rounded-raymes-chip bg-white/[0.12] px-2 py-1 text-[11px] font-medium text-ink-1 transition'
                          : 'rounded-raymes-chip px-2 py-1 text-[11px] text-ink-3 transition hover:bg-white/[0.06] hover:text-ink-1'
                      }
                    >
                      {option.title}
                    </button>
                  )
                })}
              </div>
            ) : null}
          </div>
        ) : null}
      </div>

      <div
        ref={listRef}
        className={`glass-card min-h-0 flex-1 overflow-hidden animate-raymes-scale-in ${
          hasDetails ? 'grid grid-cols-[minmax(280px,40%)_minmax(0,1fr)] px-2 py-2' : 'flex flex-col px-4 py-3'
        }`}
      >
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
              {isFirstTimePackageLoad ? (
                <p className="mx-auto mt-3 max-w-sm text-[11px] leading-5 text-ink-3">
                  First-time setup may take a few minutes while the package catalog is downloaded and indexed.
                  Future searches will open much faster from the local cache.
                </p>
              ) : null}
            </div>
          </div>
        ) : (
          <>
          <div className={hasDetails ? 'min-h-0 overflow-y-auto pr-2' : 'min-h-0 flex-1 overflow-y-auto pr-0.5'}>
            {groupedSections.map((group, groupIdx) => (
              <div key={groupIdx} className="mb-1">
                {group.section ? (
                  <div className="px-3 pb-1.5 pt-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-ink-4 select-none">
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
                          onClick={() => {
                            setSelected(globalIdx)
                            if (row.actionIds?.[0]) onRunPrimaryAction(row.actionIds[0])
                          }}
                          className={`w-full rounded-raymes-row px-3 py-2.5 text-left transition ${
                            globalIdx === selected ? 'bg-white/[0.16] text-ink-1' : 'text-ink-2 hover:bg-white/[0.06]'
                          }`}
                        >
                          <span className="flex min-w-0 items-center gap-2.5">
                            <RowIcon row={row} />
                            <span className="min-w-0 flex-1">
                              <p className="truncate text-[13px] font-medium">{row.title}</p>
                              {row.subtitle ? <p className="truncate text-[11px] text-ink-3">{row.subtitle}</p> : null}
                            </span>
                            {row.accessories?.map((accessory, index) => {
                              return <Accessory key={index} accessory={accessory} />
                            })}
                          </span>
                        </button>
                      </li>
                    )
                  })}
                </ul>
              </div>
            ))}
          </div>
          {hasDetails ? (
            <div className="min-h-0 overflow-hidden rounded-raymes-row border border-white/[0.06] bg-white/[0.02]">
              <ListDetailPane row={selectedRow} />
            </div>
          ) : null}
          </>
        )}
      </div>

      <div className="glass-card flex shrink-0 items-center justify-between gap-3 px-4 py-2 animate-raymes-scale-in">
        <span className="flex items-center gap-2 text-[10.5px] font-medium uppercase tracking-[0.14em] text-ink-4">
          {filteredRows.length} item{filteredRows.length === 1 ? '' : 's'}
          {hasMore ? (
            <button
              type="button"
              disabled={loadingMore}
              onClick={() => void requestMore()}
              className="ml-1 text-[10.5px] font-semibold normal-case tracking-normal text-ink-2 transition hover:text-ink-1 disabled:opacity-50"
            >
              {loadingMore ? 'Loading...' : 'Load more'}
            </button>
          ) : null}
        </span>
        <HintBar>
          <Hint label="Select" keys={<><Kbd>↑</Kbd><Kbd>↓</Kbd></>} />
          <Hint label={selectedAction?.title || 'Run'} keys={<Kbd>↵</Kbd>} />
          <Hint label="Actions" keys={<><Kbd>⌘</Kbd><Kbd>K</Kbd></>} />
          <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
        </HintBar>
      </div>
    </div>
  )
}
