import { type ReactNode, useCallback, useEffect, useMemo, useReducer, useRef, useState } from 'react'
import type { ExtensionManifest } from '../shared/extensions'
import {
  Button,
  Hint,
  HintBar,
  Kbd,
  Message,
  TextField,
  ViewHeader,
  cx,
} from './ui/primitives'
import { GlideList } from './ui/GlideList'
import { ExtensionPreferencesEditor } from './ExtensionPreferencesEditor'
import {
  extensionCatalogReducer,
  INITIAL_EXTENSION_CATALOG_STATE,
} from './extensionCatalogState'

type StoreExtension = ExtensionManifest
type ImageVariant = 'icon' | 'avatar' | 'screenshot'
const EXTENSION_IMAGE_CACHE = 'tezbar-extension-images-v1'

const imageObjectUrls = new Map<string, string>()

function formatCount(value: number | undefined): string {
  if (typeof value !== 'number' || !Number.isFinite(value)) return 'unknown installs'
  return `${value.toLocaleString()} installs`
}

function iconLabel(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean)
  if (words.length === 0) return '</>'
  if (words.length === 1) return words[0]!.slice(0, 2).toUpperCase()
  return `${words[0]![0] ?? ''}${words[1]![0] ?? ''}`.toUpperCase()
}

function repositoryUrl(ext: StoreExtension): string | null {
  if (typeof ext.repository === 'string' && ext.repository.trim()) return ext.repository
  const slug = ext.id.replace(/^raycast\./, '')
  return `https://github.com/raycast/extensions/tree/main/extensions/${slug}`
}

function extensionAuthor(ext: StoreExtension): string {
  return ext.author || ext.owner || 'Raycast Community'
}

function authorHandle(ext: StoreExtension): string | null {
  const value = (ext.owner || ext.author || '').trim().replace(/^@/, '')
  if (!/^[a-z0-9](?:[a-z0-9-]{0,37}[a-z0-9])?$/i.test(value)) return null
  return value
}

function imageSrcFromPathOrUrl(value: string): string {
  if (/^(https?:|data:|file:)/i.test(value)) return value
  if (value.startsWith('/')) return `file://${encodeURI(value)}`
  return value
}

function imageCacheKey(src: string, variant: ImageVariant): string {
  return `https://tezbar.local/extension-image-cache/${variant}/${encodeURIComponent(src)}.webp`
}

async function toWebpBlob(source: Blob, variant: ImageVariant): Promise<Blob> {
  const bitmap = await createImageBitmap(source)
  try {
    const maxWidth = variant === 'screenshot' ? 980 : 128
    const scale = Math.min(1, maxWidth / Math.max(bitmap.width, 1))
    const width = Math.max(1, Math.round(bitmap.width * scale))
    const height = Math.max(1, Math.round(bitmap.height * scale))
    const canvas = document.createElement('canvas')
    canvas.width = width
    canvas.height = height
    const context = canvas.getContext('2d')
    if (!context) throw new Error('Canvas is unavailable')
    context.imageSmoothingEnabled = true
    context.imageSmoothingQuality = 'high'
    context.drawImage(bitmap, 0, 0, width, height)

    return await new Promise<Blob>((resolve, reject) => {
      canvas.toBlob(
        (blob) => (blob ? resolve(blob) : reject(new Error('WebP encoding failed'))),
        'image/webp',
        variant === 'screenshot' ? 0.78 : 0.86,
      )
    })
  } finally {
    bitmap.close()
  }
}

function CachedImage({
  src,
  alt = '',
  className,
  variant,
}: {
  src: string
  alt?: string
  className?: string
  variant: ImageVariant
}): ReactNode {
  const normalized = imageSrcFromPathOrUrl(src)
  const [resolvedSrc, setResolvedSrc] = useState(normalized)

  useEffect(() => {
    let cancelled = false
    setResolvedSrc(normalized)

    if (!normalized || normalized.startsWith('data:') || typeof window.caches === 'undefined') return

    const key = imageCacheKey(normalized, variant)
    const cachedObjectUrl = imageObjectUrls.get(key)
    if (cachedObjectUrl) {
      setResolvedSrc(cachedObjectUrl)
      return
    }

    void (async () => {
      try {
        const cache = await window.caches.open(EXTENSION_IMAGE_CACHE)
        const cached = await cache.match(key)
        if (cached) {
          const blob = await cached.blob()
          const objectUrl = URL.createObjectURL(blob)
          imageObjectUrls.set(key, objectUrl)
          if (!cancelled) setResolvedSrc(objectUrl)
          return
        }

        const response = await fetch(normalized)
        if (!response.ok) throw new Error(`Image request failed: ${response.status}`)
        const webp = await toWebpBlob(await response.blob(), variant)
        await cache.put(
          key,
          new Response(webp, {
            headers: {
              'Content-Type': 'image/webp',
              'Cache-Control': 'max-age=31536000, immutable',
            },
          }),
        )
        const objectUrl = URL.createObjectURL(webp)
        imageObjectUrls.set(key, objectUrl)
        if (!cancelled) setResolvedSrc(objectUrl)
      } catch {
        if (!cancelled) setResolvedSrc(normalized)
      }
    })()

    return () => {
      cancelled = true
    }
  }, [normalized, variant])

  return <img src={resolvedSrc} alt={alt} loading="lazy" decoding="async" className={className} />
}

function ExtensionIcon({ ext, size = 'large' }: { ext: StoreExtension; size?: 'small' | 'large' }): JSX.Element {
  const iconUrl = typeof ext.iconUrl === 'string' && ext.iconUrl.trim() ? ext.iconUrl : ''
  const className = size === 'large' ? 'h-20 w-20 text-[24px]' : 'h-7 w-7 text-[10px]'
  return (
    <span
      className={cx(
        'grid shrink-0 place-items-center overflow-hidden rounded-[18px] border border-white/15 bg-[radial-gradient(circle_at_30%_20%,rgba(120,255,180,0.95),rgba(62,210,104,0.72)_42%,rgba(18,93,64,0.6))] font-mono font-bold text-white shadow-[0_18px_60px_rgba(0,0,0,0.25)]',
        className,
      )}
    >
      {iconUrl ? (
        <CachedImage src={iconUrl} alt="" variant="icon" className="h-full w-full object-cover" />
      ) : (
        <span>{iconLabel(ext.name)}</span>
      )}
    </span>
  )
}

function AuthorAvatar({ ext }: { ext: StoreExtension }): JSX.Element {
  const handle = authorHandle(ext)
  const avatarUrl =
    typeof ext.authorIconUrl === 'string' && ext.authorIconUrl.trim()
      ? ext.authorIconUrl
      : handle
        ? `https://github.com/${handle}.png?size=96`
        : ''

  return (
    <span className="grid h-8 w-8 shrink-0 place-items-center overflow-hidden rounded-full border border-white/15 bg-white/[0.06] text-[10px] font-semibold text-ink-2">
      {avatarUrl ? (
        <CachedImage src={avatarUrl} alt="" variant="avatar" className="h-full w-full object-cover" />
      ) : (
        <span>{iconLabel(extensionAuthor(ext)).slice(0, 2)}</span>
      )}
    </span>
  )
}

export default function ExtensionsView({
  onBack,
  embedded = false,
}: {
  onBack: () => void
  embedded?: boolean
}): JSX.Element {
  const [catalog, dispatchCatalog] = useReducer(
    extensionCatalogReducer,
    INITIAL_EXTENSION_CATALOG_STATE,
  )
  const { query, loading, installing, store, installed, selectedId, followSelection } = catalog
  const msg = catalog.message
  const rootRef = useRef<HTMLDivElement>(null)
  const searchRef = useRef<HTMLInputElement>(null)
  const showMessage = useCallback((message: { tone: 'success' | 'error'; text: string }) => {
    dispatchCatalog({ type: 'message', message })
  }, [])

  useEffect(() => {
    const cleanup = window.tezbar.onExtensionInstallProgress((payload) => {
      dispatchCatalog({ type: 'install-progress', ...payload })
    })
    return cleanup
  }, [])

  const reload = useCallback(async () => {
    dispatchCatalog({ type: 'load-started' })
    try {
      const [installedList, storeList] = await Promise.all([
        window.tezbar.extensionList(),
        window.tezbar.extensionSearchStore(query),
      ])
      const normalizedInstalled = installedList.map((entry) => ({
          id: entry.id,
          name: entry.name,
          description: entry.description,
          author: entry.author || 'Raycast Community',
          owner: entry.owner,
          downloadCount: entry.downloadCount,
          version: entry.version,
          installedAt: entry.installedAt,
          iconUrl: entry.iconPath,
          authorIconUrl: entry.authorIconUrl,
        }))
      dispatchCatalog({
        type: 'load-succeeded',
        installed: normalizedInstalled,
        store: storeList,
      })
    } catch (error) {
      dispatchCatalog({
        type: 'load-failed',
        message: error instanceof Error ? error.message : 'Could not load extensions',
      })
    }
  }, [query])

  useEffect(() => {
    void reload()
  }, [reload])

  useEffect(() => {
    rootRef.current?.focus()
  }, [])

  useEffect(() => {
    if (store.length === 0) {
      dispatchCatalog({ type: 'selected', id: null, follow: true })
      return
    }
    if (!selectedId || !store.some((ext) => ext.id === selectedId)) {
      dispatchCatalog({ type: 'selected', id: store[0]?.id ?? null, follow: true })
    }
  }, [selectedId, store])

  const installedIds = useMemo(() => new Set(installed.map((i) => i.id)), [installed])
  const selected = useMemo(
    () => store.find((ext) => ext.id === selectedId) ?? store[0] ?? null,
    [selectedId, store],
  )
  const selectedIndex = useMemo(
    () => (selected ? Math.max(0, store.findIndex((ext) => ext.id === selected.id)) : -1),
    [selected, store],
  )

  const runInstallAction = useCallback(
    (ext: StoreExtension): void => {
      const isInstalled = installedIds.has(ext.id)
      if (!isInstalled) {
        dispatchCatalog({ type: 'install-started', id: ext.id })
      }
      const action = isInstalled
        ? window.tezbar.extensionUninstall(ext.id)
        : window.tezbar.extensionInstall(ext.id)
      void action
        .then(() => {
          dispatchCatalog({
            type: 'install-finished',
            id: ext.id,
            message: {
              tone: 'success',
              text: `${isInstalled ? 'Removed' : 'Installed'} ${ext.name}`,
            },
          })
          return reload()
        })
        .catch((error: unknown) => {
          dispatchCatalog({
            type: 'install-finished',
            id: ext.id,
            message: {
              tone: 'error',
              text: error instanceof Error ? error.message : 'Action failed',
            },
          })
        })
    },
    [installedIds, reload],
  )

  useEffect(() => {
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        event.preventDefault()
        event.stopPropagation()
        onBack()
        return
      }
      if (event.key === '/' && document.activeElement !== searchRef.current) {
        event.preventDefault()
        searchRef.current?.focus()
        return
      }
      if (store.length === 0) return
      if (event.key === 'ArrowDown') {
        event.preventDefault()
        const next = Math.min(selectedIndex + 1, store.length - 1)
        dispatchCatalog({ type: 'selected', id: store[next]?.id ?? null, follow: true })
        return
      }
      if (event.key === 'ArrowUp') {
        event.preventDefault()
        const next = Math.max(selectedIndex - 1, 0)
        dispatchCatalog({ type: 'selected', id: store[next]?.id ?? null, follow: true })
        return
      }
      if (event.key === 'Enter' && selected) {
        event.preventDefault()
        runInstallAction(selected)
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [onBack, runInstallAction, selected, selectedIndex, store])

  const screenshots = selected?.screenshotUrls?.filter(Boolean).slice(0, 4) ?? []
  const commands = selected?.commands?.filter((command) => command.title || command.name).slice(0, 6) ?? []
  const categories = selected?.categories?.filter(Boolean).slice(0, 4) ?? []
  const selectedInstalled = selected ? installedIds.has(selected.id) : false

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      role="application"
      aria-label="Extensions"
      className={cx(
        'flex h-full min-h-0 w-full flex-col outline-none animate-tezbar-scale-in',
        embedded ? 'gap-0' : 'gap-2',
      )}
    >
      {!embedded ? <div className="glass-card shrink-0 px-4 py-3 animate-tezbar-scale-in">
        <ViewHeader
          title="Extensions"
          onBack={onBack}
          trailing={
            <Button variant="ghost" onClick={() => void reload()} disabled={loading}>
              {loading ? 'Refreshing' : 'Refresh'}
            </Button>
          }
        />
        <div className="mt-2">
          <TextField
            ref={searchRef}
            value={query}
            onChange={(event) => dispatchCatalog({ type: 'query', query: event.target.value })}
            placeholder="Search extension store"
            autoFocus
          />
        </div>
      </div> : null}

      <section className="grid min-h-0 flex-1 grid-cols-[224px_minmax(0,1fr)] gap-2 animate-tezbar-scale-in">
        <aside className="glass-card flex min-h-0 flex-col overflow-hidden px-1.5 py-2">
          <div className="flex items-center justify-between px-1.5 pb-2">
            <span className="text-[10px] font-semibold uppercase tracking-[0.16em] text-ink-4">
              Store
            </span>
            <span className="text-[10px] text-ink-4">{loading ? 'Loading…' : store.length}</span>
          </div>
          {store.length === 0 ? (
            <div className="flex min-h-0 flex-1 items-center justify-center text-center">
              <p className="text-[12px] text-ink-3">{loading ? 'Loading extensions…' : 'No extensions match.'}</p>
            </div>
          ) : (
            <GlideList
              selectedIndex={selectedIndex}
              itemCount={store.length}
              followSelected={followSelection}
              className="min-h-0 flex-1 overflow-y-auto pr-0.5"
              listClassName="space-y-0.5"
              highlightClassName="bg-white/[0.075] shadow-[inset_0_0_0_1px_rgba(255,255,255,0.20)]"
            >
              {store.map((ext) => {
                const isSelected = selected?.id === ext.id
                const isInstalled = installedIds.has(ext.id)
                return (
                  <li key={ext.id} className="relative z-[1]">
                    <button
                      type="button"
                      onMouseEnter={() => {
                        dispatchCatalog({ type: 'selected', id: ext.id, follow: false })
                      }}
                      onMouseDown={(event) => event.preventDefault()}
                      onClick={() => {
                        dispatchCatalog({ type: 'selected', id: ext.id, follow: true })
                      }}
                      className={cx(
                        'relative flex w-full items-center gap-2 rounded-tezbar-row px-1.5 py-1.5 text-left transition',
                        isSelected ? 'text-ink-1' : 'text-ink-2 hover:text-ink-1',
                      )}
                    >
                      <ExtensionIcon ext={ext} size="small" />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-[11.5px] font-medium">{ext.name}</span>
                        <span className="mt-0.5 block truncate text-[9.5px] text-ink-4">
                          {isInstalled ? 'Installed' : formatCount(ext.downloadCount)}
                        </span>
                      </span>
                    </button>
                  </li>
                )
              })}
            </GlideList>
          )}
        </aside>

        <main className="glass-card relative min-h-0 overflow-hidden">
          {selected ? (
            <div className="flex h-full min-h-0 flex-col">
              <div className="min-h-0 flex-1 overflow-y-auto">
                <section className="border-b border-white/10 bg-[radial-gradient(circle_at_10%_0%,rgba(114,95,255,0.20),transparent_34%),linear-gradient(180deg,rgba(255,255,255,0.04),rgba(255,255,255,0))] px-8 py-7">
                  <div className="flex items-center gap-6">
                    <ExtensionIcon ext={selected} />
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <h2 className="truncate text-[28px] font-semibold leading-tight text-ink-1">
                          {selected.name}
                        </h2>
                        {selectedInstalled ? (
                          <span className="rounded-tezbar-chip border border-emerald-400/30 bg-emerald-500/10 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.12em] text-emerald-300">
                            Installed
                          </span>
                        ) : null}
                      </div>
                      <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 text-[13px] text-ink-2">
                        <span>{extensionAuthor(selected)}</span>
                        <span className="text-ink-4">|</span>
                        <span>{formatCount(selected.downloadCount)}</span>
                        <span className="text-ink-4">|</span>
                        <span className="font-mono text-[12px] text-ink-3">v{selected.version}</span>
                      </div>
                      {categories.length > 0 ? (
                        <div className="mt-3 flex flex-wrap gap-1.5">
                          {categories.map((category) => (
                            <span
                              key={category}
                              className="rounded-tezbar-chip border border-white/10 bg-white/[0.04] px-2 py-1 text-[10.5px] uppercase tracking-[0.1em] text-ink-3"
                            >
                              {category}
                            </span>
                          ))}
                        </div>
                      ) : null}
                    </div>
                  </div>
                </section>

                <section className="px-8 py-6">
                  {screenshots.length > 0 ? (
                    <div className="grid grid-cols-2 gap-4">
                      {screenshots.map((url, index) => (
                        <div
                          key={`${url}-${index}`}
                          className="aspect-[16/10] overflow-hidden rounded-[8px] border border-white/10 bg-white/[0.03]"
                        >
                          <CachedImage
                            src={url}
                            alt=""
                            variant="screenshot"
                            className="h-full w-full object-cover"
                          />
                        </div>
                      ))}
                    </div>
                  ) : (
                    <div className="rounded-[8px] border border-white/10 bg-white/[0.03] px-5 py-5">
                      <p className="text-[12px] font-semibold uppercase tracking-[0.14em] text-ink-4">
                        Preview
                      </p>
                      <p className="mt-2 text-[13px] leading-relaxed text-ink-3">
                        No screenshots are available for this extension yet. The command list and metadata below can still help you decide whether it belongs in your workflow.
                      </p>
                    </div>
                  )}
                </section>

                <section className="grid grid-cols-[minmax(0,1fr)_300px] border-t border-white/10">
                  <div className="px-8 py-6">
                    <p className="text-[13px] font-semibold text-ink-3">Description</p>
                    <p className="mt-3 max-w-[72ch] text-[16px] leading-relaxed text-ink-1">
                      {selected.description || 'No description provided.'}
                    </p>
                    <div className="mt-5 divide-y divide-white/10 rounded-[8px] border border-white/10 bg-white/[0.025]">
                      <div className="flex items-center gap-3 px-4 py-3">
                        <ExtensionIcon ext={selected} size="small" />
                        <div className="min-w-0 flex-1">
                          <p className="text-[10px] font-semibold uppercase tracking-[0.14em] text-ink-4">
                            Extension ID
                          </p>
                          <p className="mt-0.5 break-all font-mono text-[12px] leading-snug text-ink-2">
                            {selected.id}
                          </p>
                        </div>
                      </div>
                      <div className="flex items-center gap-3 px-4 py-3">
                        <AuthorAvatar ext={selected} />
                        <div className="min-w-0 flex-1">
                          <p className="text-[10px] font-semibold uppercase tracking-[0.14em] text-ink-4">
                            Author
                          </p>
                          <div className="mt-0.5 flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-0.5">
                            <span className="text-[13px] font-medium text-ink-1">
                              {extensionAuthor(selected)}
                            </span>
                            {authorHandle(selected) ? (
                              <span className="font-mono text-[11px] text-ink-4">@{authorHandle(selected)}</span>
                            ) : null}
                          </div>
                        </div>
                      </div>
                    </div>
                    {selectedInstalled ? (
                      <ExtensionPreferencesEditor
                        extensionId={selected.id}
                        extensionName={selected.name}
                        commands={selected.commands ?? []}
                        onMessage={showMessage}
                      />
                    ) : null}
                  </div>

                  <aside className="border-l border-white/10 px-5 py-6">
                    <p className="text-[13px] font-semibold text-ink-3">Resources</p>
                    <a
                      href={repositoryUrl(selected) ?? '#'}
                      onClick={(event) => {
                        event.preventDefault()
                        const url = repositoryUrl(selected)
                        if (url) void window.tezbar.openExternalUrl(url)
                      }}
                      className="mt-3 flex items-center justify-between rounded-[8px] border border-white/10 bg-white/[0.03] px-4 py-3 text-[14px] font-medium text-ink-1 transition hover:bg-white/[0.07]"
                    >
                      <span>Open README</span>
                      <span className="text-[20px] text-ink-4">↗</span>
                    </a>

                    <div className="mt-5">
                      <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-ink-4">
                        Commands
                      </p>
                      {commands.length > 0 ? (
                        <ul className="mt-2 space-y-1.5">
                          {commands.map((command) => (
                            <li
                              key={command.name || command.title}
                              className="rounded-[8px] border border-white/10 bg-white/[0.03] px-3 py-2"
                            >
                              <p className="truncate text-[12px] font-medium text-ink-2">
                                {command.title || command.name}
                              </p>
                              {command.description ? (
                                <p className="mt-0.5 line-clamp-2 text-[10.5px] text-ink-4">
                                  {command.description}
                                </p>
                              ) : null}
                            </li>
                          ))}
                        </ul>
                      ) : (
                        <p className="mt-2 text-[12px] text-ink-4">Command metadata is not available.</p>
                      )}
                    </div>
                  </aside>
                </section>
              </div>

              <footer className="flex shrink-0 items-center gap-3 border-t border-white/10 bg-black/20 px-5 py-3">
                <ExtensionIcon ext={selected} size="small" />
                <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-ink-2">{selected.name}</span>
                <Button
                  variant={selectedInstalled ? 'danger' : 'primary'}
                  disabled={loading || !!installing[selected.id]}
                  onClick={() => runInstallAction(selected)}
                >
                  {installing[selected.id] !== undefined ? `Installing ${Math.round(installing[selected.id]!)}%` : selectedInstalled ? 'Remove Extension' : 'Install Extension'}
                </Button>
                <span className="hidden h-5 w-px bg-white/10 sm:block" />
                <HintBar>
                  <Hint label="Actions" keys={<><Kbd>⌘</Kbd><Kbd>K</Kbd></>} />
                </HintBar>
              </footer>
            </div>
          ) : (
            <div className="flex h-full items-center justify-center text-[12px] text-ink-3">
              Select an extension to see details.
            </div>
          )}
        </main>
      </section>

      {msg ? (
        <div className="glass-card shrink-0 px-4 py-2 animate-tezbar-scale-in">
          <Message tone={msg.tone}>{msg.text}</Message>
        </div>
      ) : null}

      {!embedded ? <div className="glass-card shrink-0 px-4 py-2 animate-tezbar-scale-in">
        <HintBar>
          <Hint label="Search" keys={<Kbd>/</Kbd>} />
          <Hint label="Navigate" keys={<><Kbd>↑</Kbd><Kbd>↓</Kbd></>} />
          <Hint label="Install / Remove" keys={<Kbd>↵</Kbd>} />
          <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
        </HintBar>
      </div> : null}
    </div>
  )
}
