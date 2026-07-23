import { type KeyboardEvent, type ReactNode, useEffect, useMemo, useRef, useState } from 'react'
import type { ExtensionRuntimeAction, ExtensionRuntimeNode } from '../../../shared/extensionRuntime'
import { Hint, HintBar, Kbd, cx } from '../../ui/primitives'
import { colorFromGridContent } from './grid-content'

type GridItem = {
  id: string
  title: string
  subtitle: string
  image?: string
  color?: string
  actionIds: string[]
}

type GridAccessory = {
  actionId?: string
  value?: string
  options: Array<{ title: string; value: string }>
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

function parseGridAccessory(value: unknown): GridAccessory | null {
  if (!value || typeof value !== 'object' || !('type' in value)) return null
  const node = value as ExtensionRuntimeNode
  if (node.type !== 'List.Dropdown') return null

  const options: GridAccessory['options'] = []
  const walk = (entry: ExtensionRuntimeNode): void => {
    if (entry.type === 'List.Dropdown.Item') {
      const title = textValue(entry.props?.title)
      const value = entry.props?.value !== undefined ? textValue(entry.props.value) : title
      if (title || value) options.push({ title: title || value, value })
      return
    }
    for (const child of entry.children ?? []) walk(child)
  }
  walk(node)

  return {
    actionId: typeof node.props?.actionId === 'string' ? node.props.actionId : undefined,
    value: node.props?.value !== undefined ? textValue(node.props.value) : undefined,
    options,
  }
}

function imageSourceFromContent(content: unknown): string | undefined {
  if (!content || typeof content !== 'object') return undefined
  const value = (content as { value?: unknown }).value
  const source =
    value && typeof value === 'object'
      ? (value as { source?: unknown }).source
      : (content as { source?: unknown }).source

  if (typeof source === 'string') return source
  if (source && typeof source === 'object') {
    const themed = source as { dark?: unknown; light?: unknown }
    if (typeof themed.dark === 'string') return themed.dark
    if (typeof themed.light === 'string') return themed.light
  }
  return undefined
}

function collectGridItems(root: ExtensionRuntimeNode): GridItem[] {
  const out: GridItem[] = []

  const walk = (node: ExtensionRuntimeNode): void => {
    if (node.type === 'Grid.Item') {
      const id = typeof node.props?.id === 'string' ? node.props.id : `grid:${out.length}`
      const title = typeof node.props?.title === 'string' ? node.props.title : 'Untitled'
      const subtitle = typeof node.props?.subtitle === 'string' ? node.props.subtitle : ''
      const image = imageSourceFromContent(node.props?.content)
      const color = colorFromGridContent(node.props?.content)
      const actionIds = Array.isArray(node.props?.actionIds)
        ? node.props.actionIds.filter((value): value is string => typeof value === 'string')
        : []
      out.push({ id, title, subtitle, image, color, actionIds })
      return
    }

    for (const child of node.children ?? []) {
      walk(child)
    }
  }

  walk(root)
  return out
}

function gridItemMatches(item: GridItem, query: string): boolean {
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean)
  if (terms.length === 0) return true
  const haystack = `${item.title} ${item.subtitle}`.toLowerCase()
  return terms.every((term) => haystack.includes(term))
}

export function GridRuntime({
  root,
  title,
  actions,
  actionNotice,
  onBack,
  onRunPrimaryAction,
  onOpenActions,
  onSearchTextChanged,
}: {
  root: ExtensionRuntimeNode
  title: string
  actions: ExtensionRuntimeAction[]
  actionNotice?: { message: string; tone: 'success' | 'error' } | null
  onBack: () => void
  onRunPrimaryAction: (actionId?: string, formValues?: Record<string, unknown>) => void
  onOpenActions: (actionIds?: string[]) => void
  onSearchTextChanged: (searchText: string) => Promise<void> | void
}): ReactNode {
  const items = useMemo(() => collectGridItems(root), [root])
  const initialQuery = typeof root.props?.searchText === 'string' ? root.props.searchText : ''
  const [query, setQuery] = useState(initialQuery)
  const [selected, setSelected] = useState(0)
  const searchAccessory = useMemo(
    () => parseGridAccessory(root.props?.searchBarAccessory),
    [root.props?.searchBarAccessory]
  )
  const [accessoryValue, setAccessoryValue] = useState(searchAccessory?.options[0]?.value ?? '')
  const inputRef = useRef<HTMLInputElement>(null)
  const debounceTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const lastSentQuery = useRef(initialQuery)
  const hasServerSearch = root.props?.__hasServerSearch === true
  const usesServerFiltering = hasServerSearch || root.props?.filtering === false
  const columnCount =
    typeof root.props?.columns === 'number'
      ? Math.max(2, Math.min(8, Math.round(root.props.columns)))
      : 4
  const searchBarPlaceholder =
    typeof root.props?.searchBarPlaceholder === 'string' && root.props.searchBarPlaceholder.trim()
      ? root.props.searchBarPlaceholder
      : title || 'Search'
  const filteredItems = useMemo(
    () => (usesServerFiltering ? items : items.filter((item) => gridItemMatches(item, query))),
    [items, query, usesServerFiltering]
  )
  const selectedItem = filteredItems[selected]
  const selectedActionIds = useMemo(() => new Set(selectedItem?.actionIds ?? []), [selectedItem])
  const favoriteAction = actions.find(
    (action) => selectedActionIds.has(action.id) && /favou?rites?/i.test(action.title)
  )
  const primaryActionLabel = /gif/i.test(`${title} ${searchBarPlaceholder}`) ? 'Copy GIF' : 'Run'

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  useEffect(() => {
    if (typeof root.props?.searchText === 'string') {
      setQuery(root.props.searchText)
    }
  }, [root.props?.searchText])

  useEffect(() => {
    setAccessoryValue((current) => {
      if (!searchAccessory || searchAccessory.options.length === 0) return ''
      if (
        searchAccessory.value !== undefined &&
        searchAccessory.options.some((option) => option.value === searchAccessory.value)
      ) {
        return searchAccessory.value
      }
      return searchAccessory.options.some((option) => option.value === current)
        ? current
        : (searchAccessory.options[0]?.value ?? '')
    })
  }, [searchAccessory])

  useEffect(() => {
    if (query === lastSentQuery.current) return
    if (debounceTimer.current) clearTimeout(debounceTimer.current)
    debounceTimer.current = setTimeout(() => {
      lastSentQuery.current = query
      void onSearchTextChanged(query)
    }, 200)
    return () => {
      if (debounceTimer.current) clearTimeout(debounceTimer.current)
    }
  }, [onSearchTextChanged, query])

  useEffect(() => {
    if (selected >= filteredItems.length) {
      setSelected(Math.max(0, filteredItems.length - 1))
    }
  }, [filteredItems.length, selected])

  const runSelected = (): void => {
    if (selectedItem) onRunPrimaryAction(selectedItem.actionIds[0])
  }

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>): void => {
    const isActionsShortcut =
      event.metaKey &&
      !event.ctrlKey &&
      !event.altKey &&
      !event.shiftKey &&
      event.key.toLowerCase() === 'k'
    if (isActionsShortcut) {
      event.preventDefault()
      event.stopPropagation()
      onOpenActions(selectedItem?.actionIds)
      return
    }

    const isFavoriteShortcut =
      event.metaKey &&
      !event.ctrlKey &&
      !event.altKey &&
      !event.shiftKey &&
      event.key.toLowerCase() === 'l'
    if (isFavoriteShortcut && favoriteAction) {
      event.preventDefault()
      onRunPrimaryAction(favoriteAction.id)
      return
    }

    if (filteredItems.length === 0) return

    if (event.key === 'ArrowDown') {
      event.preventDefault()
      setSelected((index) => Math.min(index + columnCount, filteredItems.length - 1))
      return
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault()
      setSelected((index) => Math.max(index - columnCount, 0))
      return
    }
    if (event.key === 'ArrowRight' && event.target !== inputRef.current) {
      event.preventDefault()
      setSelected((index) => Math.min(index + 1, filteredItems.length - 1))
      return
    }
    if (event.key === 'ArrowLeft' && event.target !== inputRef.current) {
      event.preventDefault()
      setSelected((index) => Math.max(index - 1, 0))
      return
    }
    if (event.key === 'Enter' && !event.repeat) {
      event.preventDefault()
      runSelected()
    }
  }

  return (
    <div
      role="application"
      tabIndex={-1}
      className="flex h-full min-h-0 flex-col outline-none"
      onKeyDown={onKeyDown}
    >
      <div className="glass-card mb-2 shrink-0 px-3 py-2">
        <div className="flex items-center gap-2">
          <button type="button" className="btn btn-ghost" onClick={onBack}>
            Back
          </button>
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(event) => {
              setQuery(event.target.value)
              setSelected(0)
            }}
            placeholder={searchBarPlaceholder}
            aria-label={searchBarPlaceholder}
            className="h-8 min-w-0 flex-1 rounded-tezbar-chip border border-white/10 bg-white/[0.04] px-3 text-[12px] font-semibold text-ink-1 outline-none transition placeholder:text-ink-4 focus:border-white/20 focus:bg-white/[0.06]"
          />
          {searchAccessory && searchAccessory.options.length > 0 ? (
            <select
              value={accessoryValue}
              onChange={(event) => {
                const next = event.target.value
                setAccessoryValue(next)
                setSelected(0)
                if (searchAccessory.actionId) {
                  onRunPrimaryAction(searchAccessory.actionId, { value: next })
                }
              }}
              className={cx(
                'h-8 max-w-[220px] shrink-0 rounded-tezbar-chip border border-white/10 bg-white/[0.04] px-3 text-[12px] font-semibold text-ink-1 outline-none transition',
                'hover:bg-white/[0.06] focus:border-white/20 focus:bg-white/[0.06]'
              )}
              aria-label="GIF provider"
            >
              {searchAccessory.options.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.title}
                </option>
              ))}
            </select>
          ) : null}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-1 py-2">
        {filteredItems.length === 0 ? (
          <div className="flex h-full items-center justify-center text-[12px] text-ink-3">
            {query.trim() ? 'No matching grid items' : 'No grid items'}
          </div>
        ) : (
          <div
            className="grid gap-3"
            style={{ gridTemplateColumns: `repeat(${columnCount}, minmax(0, 1fr))` }}
          >
            {filteredItems.map((item, index) => (
              <button
                key={item.id}
                type="button"
                onMouseEnter={() => setSelected(index)}
                onClick={() => onRunPrimaryAction(item.actionIds[0])}
                onContextMenu={(event) => {
                  event.preventDefault()
                  setSelected(index)
                  onOpenActions(item.actionIds)
                }}
                className={`group min-h-[132px] rounded-[10px] border p-3 text-left transition ${
                  selected === index
                    ? 'border-accent-1/55 bg-white/[0.075] text-ink-1 shadow-[0_0_0_1px_rgba(139,116,255,0.18)]'
                    : 'border-white/[0.075] bg-transparent text-ink-2 hover:border-white/16 hover:bg-white/[0.045]'
                }`}
              >
                <div
                  className={cx(
                    'mx-auto mb-3 grid aspect-square w-full max-w-[88px] place-items-center rounded-[8px] border transition',
                    item.color
                      ? 'border-white/15 shadow-[inset_0_1px_0_rgba(255,255,255,0.18),0_8px_22px_rgba(0,0,0,0.16)]'
                      : 'border-transparent bg-[#242936] p-4 group-hover:bg-[#2b3040]'
                  )}
                  style={item.color ? { backgroundColor: item.color } : undefined}
                >
                  {item.image ? (
                    <img
                      src={item.image}
                      alt=""
                      className="h-full w-full object-contain"
                      loading="lazy"
                    />
                  ) : item.color ? null : (
                    <span className="text-[20px] font-semibold text-ink-4">
                      {item.title.slice(0, 1).toUpperCase()}
                    </span>
                  )}
                </div>
                <p className="truncate text-[12px] font-semibold">{item.title}</p>
                {item.subtitle ? (
                  <p className="truncate text-[11px] text-ink-4">{item.subtitle}</p>
                ) : null}
              </button>
            ))}
          </div>
        )}
      </div>

      {actionNotice ? (
        <div
          role="status"
          aria-live="polite"
          className={`shrink-0 px-3 pb-1 pt-2 text-[11px] font-medium ${
            actionNotice.tone === 'error' ? 'text-rose-300' : 'text-ink-2'
          }`}
        >
          {actionNotice.message}
        </div>
      ) : null}

      <div className="glass-card mt-2 flex shrink-0 items-center justify-between gap-3 px-3 py-2">
        {favoriteAction ? (
          <HintBar>
            <Hint
              label={favoriteAction.title}
              keys={
                <>
                  <Kbd>⌘</Kbd>
                  <Kbd>L</Kbd>
                </>
              }
            />
          </HintBar>
        ) : (
          <span />
        )}
        <HintBar className="justify-end">
          <Hint label={primaryActionLabel} keys={<Kbd>↵</Kbd>} />
          <Hint
            label="Actions"
            keys={
              <>
                <Kbd>⌘</Kbd>
                <Kbd>K</Kbd>
              </>
            }
          />
        </HintBar>
      </div>
    </div>
  )
}
