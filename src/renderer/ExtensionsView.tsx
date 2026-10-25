import { type ReactNode, useCallback, useEffect, useMemo, useReducer, useRef, useState } from 'react'
import type { ExtensionManifest, ExtensionPlatform } from '../shared/extensions'
import type { ExtensionStorePage } from '../shared/desktop-api'
import { parseGitHubRepositoryUrl } from '../shared/extensionRepository'
import { Button, Hint, HintBar, Kbd, Message, ViewHeader, cx } from './ui/primitives'
import { KNOWN_EXTENSION_APP_MAP, Squircle, normalizeIconKind } from './launcherIcons'
import { GlideList } from './ui/GlideList'
import { ExtensionPreferencesEditor } from './ExtensionPreferencesEditor'
import {
  extensionCatalogReducer,
  INITIAL_EXTENSION_CATALOG_STATE,
  normalizeInstalledExtension,
} from './extensionCatalogState'
import {
  cachedExtensionImageSource,
  extensionImageSource,
  type ExtensionImageVariant,
} from './extensionImages'
import {
  checkedLines,
  canInstallExtension,
  commandStatus,
  describeRuntime,
  installTarget,
  mergeCatalog,
  nativeV2Lines,
  rowSubtitle,
  searchStorePage,
  supportedPlatforms,
  visibleCommands,
} from './extensionRuntimeStatus'

type StoreExtension = ExtensionManifest

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
  if (!ext.id.startsWith('raycast.')) return null
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

function CachedImage({
  src,
  alt = '',
  className,
  variant,
  onError,
}: {
  src: string
  alt?: string
  className?: string
  variant: ExtensionImageVariant
  onError?: () => void
}): ReactNode {
  const normalized = extensionImageSource(src)
  const [resolvedImage, setResolvedImage] = useState({ src: normalized, variant, source: normalized })
  // A prop change renders before its effect runs; never paint the previous
  // extension's cached image during that render.
  const resolvedSrc = resolvedImage.src === normalized && resolvedImage.variant === variant
    ? resolvedImage.source
    : normalized

  useEffect(() => {
    let cancelled = false

    void cachedExtensionImageSource(normalized, variant).then((source) => {
      if (!cancelled) setResolvedImage({ src: normalized, variant, source })
    })

    return () => {
      cancelled = true
    }
  }, [normalized, variant])

  return <img src={resolvedSrc} alt={alt} loading="lazy" decoding="async" className={className} onError={onError} />
}

function installProgressLabel(progress: number): string {
  const percent = Math.round(progress)
  if (percent < 10) return `Preparing ${percent}%`
  if (percent < 45) return `Downloading bundle ${percent}%`
  if (percent < 60) return `Downloading source ${percent}%`
  if (percent < 75) return `Installing dependencies ${percent}%`
  if (percent < 95) return `Building commands ${percent}%`
  return `Finishing ${percent}%`
}

function ExtensionIcon({ ext, size = 'large' }: { ext: StoreExtension; size?: 'small' | 'large' }): JSX.Element {
  // Installed manifests carry a runtime slug that the store type does not declare.
  const slug = normalizeIconKind((ext as StoreExtension & { slug?: string }).slug || ext.id || '')
  const knownAppPath = KNOWN_EXTENSION_APP_MAP[slug]
  const [appIconUrl, setAppIconUrl] = useState<string | null>(null)
  const [failedIconUrl, setFailedIconUrl] = useState<string | null>(null)
  const sizePx = size === 'large' ? 48 : 28
  const roundedClass = size === 'large' ? 'rounded-[12px]' : 'rounded-[7px]'

  useEffect(() => {
    if (!knownAppPath) return
    let cancelled = false
    void window.tezbar?.getAppIconDataUrl(knownAppPath).then((url) => {
      if (!cancelled && url) setAppIconUrl(url)
    }).catch(() => undefined)
    return () => {
      cancelled = true
    }
  }, [knownAppPath])

  if (appIconUrl) {
    return (
      <span
        role="img"
        aria-label={`${ext.name} icon`}
        className={cx('grid shrink-0 place-items-center overflow-hidden bg-transparent', roundedClass)}
        style={{ width: sizePx, height: sizePx }}
      >
        <img
          src={appIconUrl}
          alt=""
          className="h-full w-full object-contain"
          draggable={false}
        />
      </span>
    )
  }

  const iconUrl = ext.iconUrl?.trim() || ''
  const isImageSrc =
    iconUrl &&
    (iconUrl.startsWith('data:image/') ||
      iconUrl.startsWith('blob:') ||
      iconUrl.startsWith('file://'))
  const hasIcon = Boolean(isImageSrc) && failedIconUrl !== iconUrl

  if (hasIcon) {
    return (
      <span
        role="img"
        aria-label={`${ext.name} icon`}
        className={cx('grid shrink-0 place-items-center overflow-hidden bg-transparent', roundedClass)}
        style={{ width: sizePx, height: sizePx }}
      >
        <CachedImage
          src={iconUrl}
          alt=""
          variant="icon"
          className="h-full w-full object-cover"
          onError={() => setFailedIconUrl(iconUrl)}
        />
      </span>
    )
  }

  return (
    <Squircle
      kind={slug || 'extensions'}
      size={sizePx}
      className={roundedClass}
    />
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
  const [catalog, dispatchCatalog] = useReducer(extensionCatalogReducer, INITIAL_EXTENSION_CATALOG_STATE)
  const {
    query,
    loading,
    loadingMore,
    installing,
    store,
    installed,
    selectedId,
    followSelection,
    total,
    hasMore,
  } = catalog
  const msg = catalog.message
  const rootRef = useRef<HTMLDivElement>(null)
  const searchRef = useRef<HTMLInputElement>(null)
  const loadRequestIdRef = useRef(0)
  const pendingNextAfterLoadRef = useRef<{ fromId: string; seen: Set<string> } | null>(null)
  const [repositoryInstalling, setRepositoryInstalling] = useState(false)
  const isWindows =
    typeof navigator !== 'undefined' &&
    (/windows/i.test(navigator.userAgent) || /win/i.test(navigator.platform))
  const platform: ExtensionPlatform = isWindows ? 'Windows' : 'macOS'
  const repository = useMemo(() => parseGitHubRepositoryUrl(query), [query])
  const isUrlQuery = useMemo(() => /^[a-z][a-z0-9+.-]*:\/\//i.test(query.trim()), [query])
  const showMessage = useCallback((message: { tone: 'success' | 'error'; text: string }) => {
    dispatchCatalog({ type: 'message', message })
  }, [])

  useEffect(() => {
    const cleanup = window.tezbar.onExtensionInstallProgress((payload) => {
      dispatchCatalog({ type: 'install-progress', ...payload })
    })
    return cleanup
  }, [])

  const loadStorePage = useCallback(
    async (offset = 0, limit = 25): Promise<ExtensionStorePage> => {
      return searchStorePage(query, isUrlQuery, offset, limit, window.tezbar.extensionSearchStore)
    },
    [isUrlQuery, query],
  )

  const reload = useCallback(async () => {
    const requestId = loadRequestIdRef.current + 1
    loadRequestIdRef.current = requestId
    pendingNextAfterLoadRef.current = null
    dispatchCatalog({ type: 'load-started', requestId })
    try {
      const [installedList, storePage] = await Promise.all([
        window.tezbar.extensionList(),
        loadStorePage(0, 25),
      ])
      if (loadRequestIdRef.current !== requestId) return
      const normalizedInstalled = installedList.map(normalizeInstalledExtension)
      dispatchCatalog({
        type: 'load-succeeded',
        requestId,
        installed: normalizedInstalled,
        store: storePage.items,
        total: storePage.total,
        hasMore: storePage.hasMore,
      })
    } catch (error) {
      if (loadRequestIdRef.current !== requestId) return
      dispatchCatalog({
        type: 'load-failed',
        requestId,
        message: error instanceof Error ? error.message : 'Could not load extensions',
      })
    }
  }, [loadStorePage])

  useEffect(() => window.tezbar.onExtensionCatalogUpdated(() => {
    void reload()
  }), [reload])

  const loadMore = useCallback(
    async (options?: { selectNext?: boolean }) => {
      if (loadingMore || !hasMore || loading) return
      const requestId = loadRequestIdRef.current
      dispatchCatalog({ type: 'load-more-started' })
      try {
        const nextPage = await loadStorePage(store.length, 25)
        if (loadRequestIdRef.current !== requestId) return
        dispatchCatalog({
          type: 'load-more-succeeded',
          items: nextPage.items,
          total: nextPage.total,
          hasMore: nextPage.hasMore,
          selectNext: options?.selectNext,
        })
      } catch (error) {
        if (loadRequestIdRef.current !== requestId) return
        pendingNextAfterLoadRef.current = null
        dispatchCatalog({
          type: 'load-more-failed',
          message: error instanceof Error ? error.message : 'Could not load more extensions',
        })
      }
    },
    [hasMore, loading, loadingMore, loadStorePage, store.length],
  )

  const handleListScroll = useCallback(
    (event: React.UIEvent<HTMLDivElement>) => {
      const { scrollTop, scrollHeight, clientHeight } = event.currentTarget
      if (scrollHeight - (scrollTop + clientHeight) < 250 && hasMore && !loadingMore && !loading) {
        void loadMore()
      }
    },
    [hasMore, loadingMore, loading, loadMore],
  )

  useEffect(() => {
    const timeout = window.setTimeout(() => {
      void reload()
    }, 250)
    return () => window.clearTimeout(timeout)
  }, [reload])

  useEffect(() => {
    if (embedded) {
      rootRef.current?.focus()
      return
    }
    requestAnimationFrame(() => searchRef.current?.focus())
  }, [embedded])

  const installedIds = useMemo(() => new Set(installed.map((i) => i.id)), [installed])
  const rows = useMemo(() => mergeCatalog(store, installed, platform, query), [store, installed, platform, query])

  useEffect(() => {
    const pending = pendingNextAfterLoadRef.current
    if (pending) {
      const previousIndex = rows.findIndex((ext) => ext.id === pending.fromId)
      const next = previousIndex >= 0 && previousIndex + 1 < rows.length
        ? rows[previousIndex + 1]
        : rows.find((ext) => !pending.seen.has(ext.id))
      if (next) {
        pendingNextAfterLoadRef.current = null
        dispatchCatalog({ type: 'selected', id: next.id, follow: true })
        return
      }
      if (!hasMore && !loadingMore) pendingNextAfterLoadRef.current = null
    }
    if (rows.length === 0) {
      dispatchCatalog({ type: 'selected', id: null, follow: true })
      return
    }
    if (!selectedId || !rows.some((ext) => ext.id === selectedId)) {
      dispatchCatalog({ type: 'selected', id: rows[0]?.id ?? null, follow: true })
    }
  }, [hasMore, loadingMore, selectedId, rows])

  const selected = useMemo(() => rows.find((ext) => ext.id === selectedId) ?? rows[0] ?? null, [selectedId, rows])
  const selectedIndex = useMemo(
    () =>
      selected
        ? Math.max(
            0,
            rows.findIndex((ext) => ext.id === selected.id),
          )
        : -1,
    [selected, rows],
  )

  const runInstallAction = useCallback(
    (ext: StoreExtension): void => {
      const isInstalled = installedIds.has(ext.id)
      if (!isInstalled && !canInstallExtension(ext, platform)) return
      const repositoryReference = ext.id === ext.nativeV2?.id
        ? null
        : parseGitHubRepositoryUrl(repositoryUrl(ext) ?? '')
      const target = repositoryReference?.url ?? installTarget(ext)
      if (!isInstalled) {
        dispatchCatalog({ type: 'install-started', id: ext.id })
      }
      const action = isInstalled
        ? window.tezbar.extensionUninstall(ext.id)
        : window.tezbar.extensionInstall(target)
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
    [installedIds, platform, reload],
  )

  const installRepository = useCallback((): void => {
    if (!repository || repositoryInstalling) return

    setRepositoryInstalling(true)
    dispatchCatalog({ type: 'message', message: null })
    void window.tezbar
      .extensionInstall(repository.url)
      .then((installedExtension) => {
        showMessage({
          tone: 'success',
          text: `Installed ${installedExtension.name} from ${repository.owner}/${repository.repository}`,
        })
        return reload()
      })
      .catch((error: unknown) => {
        showMessage({
          tone: 'error',
          text: error instanceof Error ? error.message : 'Could not install this repository',
        })
      })
      .finally(() => {
        setRepositoryInstalling(false)
      })
  }, [reload, repository, repositoryInstalling, showMessage])

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
      if (event.key === 'Enter' && repository) {
        event.preventDefault()
        installRepository()
        return
      }
      if (rows.length === 0) return
      if (event.key === 'ArrowDown') {
        event.preventDefault()
        if (selectedIndex + 1 >= rows.length) {
          if (hasMore) {
            pendingNextAfterLoadRef.current = selected
              ? { fromId: selected.id, seen: new Set(rows.map((row) => row.id)) }
              : null
            void loadMore()
          } else {
            // Infinite loop: loop back to top
            dispatchCatalog({ type: 'selected', id: rows[0]?.id ?? null, follow: true })
          }
          return
        }

        const next = selectedIndex + 1
        dispatchCatalog({ type: 'selected', id: rows[next]?.id ?? null, follow: true })
        if (hasMore && !loadingMore && next >= rows.length - 5) {
          void loadMore()
        }
        return
      }
      if (event.key === 'ArrowUp') {
        event.preventDefault()
        if (selectedIndex <= 0) {
          // Infinite loop: loop back to bottom
          const next = rows.length - 1
          dispatchCatalog({ type: 'selected', id: rows[next]?.id ?? null, follow: true })
          return
        }
        const next = selectedIndex - 1
        dispatchCatalog({ type: 'selected', id: rows[next]?.id ?? null, follow: true })
        return
      }
      if (event.key === 'Enter' && selected) {
        event.preventDefault()
        if (installedIds.has(selected.id) || canInstallExtension(selected, platform)) runInstallAction(selected)
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [
    hasMore,
    installRepository,
    loadingMore,
    loadMore,
    onBack,
    repository,
    runInstallAction,
    selected,
    selectedIndex,
    installedIds,
    platform,
    rows,
  ])

  const screenshots = selected?.screenshotUrls?.filter(Boolean).slice(0, 4) ?? []
  // Show the complete command list. Some extensions (including Color Picker)
  // expose more than six commands, and hiding the rest makes installed
  // functionality look unavailable in the store detail view.
  const commands = selected ? visibleCommands(selected) : []
  const categories = selected?.categories?.filter(Boolean).slice(0, 4) ?? []
  const selectedInstalled = selected ? installedIds.has(selected.id) : false
  const runtimeDescription = selected ? describeRuntime(selected, platform) : null
  const supports = selected ? supportedPlatforms(selected) : []

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      role="application"
      aria-label="Extensions"
      className={cx(
        'extensions-store flex h-full min-h-0 w-full flex-col outline-none animate-tezbar-scale-in',
        !embedded && 'view-sheet'
      )}
    >
      {!embedded ? (
        <div className="glass-card shrink-0 px-4 py-3 animate-tezbar-scale-in">
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
            <input
              ref={searchRef}
              value={query}
              onChange={(event) => {
                pendingNextAfterLoadRef.current = null
                dispatchCatalog({ type: 'query', query: event.target.value })
              }}
              aria-label="Search extensions or paste a GitHub repository URL"
              placeholder={`${isWindows ? 'Search loved extensions' : 'Search extensions'} or paste a GitHub URL…`}
              className="extensions-store-search h-8 w-full rounded-tezbar-chip border border-white/10 bg-white/[0.04] px-2.5 text-[12px] text-ink-1 placeholder:text-ink-4 outline-none transition focus:border-white/20 focus:bg-white/[0.06]"
              autoFocus
            />
          </div>
        </div>
      ) : null}

      <section className="extensions-store-body glass-card grid min-h-0 flex-1 gap-3 overflow-hidden px-2 py-2">
        <aside
          aria-label={isWindows ? 'Loved by Tezbar' : 'Extension catalog'}
          className="flex min-h-0 min-w-0 flex-col overflow-hidden"
        >
          {rows.length === 0 ? (
            <div className="flex min-h-0 flex-1 items-center justify-center text-center">
              <p className="px-3 text-[12px] text-ink-3">
                {repository
                  ? 'GitHub repository ready to install.'
                  : isUrlQuery
                    ? 'Paste a public GitHub repository root URL.'
                    : loading
                      ? 'Loading extensions…'
                      : isWindows
                        ? 'No loved extensions match.'
                        : 'No extensions match.'}
              </p>
            </div>
          ) : (
            <GlideList
              selectedIndex={selectedIndex}
              itemCount={rows.length}
              followSelected={followSelection}
              className="min-h-0 flex-1 overflow-y-auto pr-0.5"
              listClassName="space-y-0.5"
              onScroll={handleListScroll}
            >
              {rows.map((ext) => {
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
                        'relative flex w-full items-center gap-2.5 rounded-tezbar-row px-2 py-1.5 text-left transition',
                        isSelected ? 'text-ink-1' : 'text-ink-2 hover:text-ink-1'
                      )}
                    >
                      <ExtensionIcon ext={ext} size="small" />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-[12px] font-medium">{ext.name}</span>
                        <span className="mt-0.5 block truncate text-[10.5px] text-ink-4">
                          {rowSubtitle(
                            ext,
                            platform,
                            isInstalled
                              ? 'Installed'
                              : ext.id === ext.nativeV2?.id
                                ? 'Tezbar'
                                : isWindows
                                  ? 'GitHub repository'
                                  : formatCount(ext.downloadCount),
                          )}
                        </span>
                      </span>
                    </button>
                  </li>
                )
              })}
              {loadingMore ? (
                <li className="flex items-center justify-center py-2 text-[10px] text-ink-4">
                  Loading more…
                </li>
              ) : null}
            </GlideList>
          )}
        </aside>

        <main
          aria-label="Extension details"
          className="extensions-store-detail sheet-above-bar relative flex min-h-0 min-w-0 flex-col overflow-hidden rounded-tezbar-row border border-white/[0.06] bg-white/[0.02]"
        >
          <div className="min-h-0 flex-1 overflow-hidden">
            {repository ? (
              <div className="flex h-full items-center justify-center px-8">
                <div className="w-full max-w-[540px] py-6">
                  <p className="text-[11px] font-semibold text-ink-4">
                    {isWindows ? 'Loved extension from GitHub' : 'Install from GitHub'}
                  </p>
                  <h2 className="mt-3 break-words text-[20px] font-semibold text-ink-1">
                    {repository.repository}
                  </h2>
                  <p className="mt-1 text-[13px] text-ink-3">
                    {repository.owner}/{repository.repository}
                  </p>
                  <p className="mt-5 text-[13px] leading-relaxed text-ink-2">
                    Tezbar will download this public repository, validate its package manifest and
                    platform support, then install its checked-in command bundles or build them
                    locally.
                  </p>
                  <div className="mt-6 flex items-center gap-3">
                    <Button
                      variant="primary"
                      disabled={repositoryInstalling}
                      onClick={installRepository}
                    >
                      {repositoryInstalling ? 'Installing repository…' : 'Install Repository'}
                    </Button>
                    <Button
                      variant="ghost"
                      disabled={repositoryInstalling}
                      onClick={() => void window.tezbar.openExternalUrl(repository.url)}
                    >
                      Open on GitHub
                    </Button>
                  </div>
                </div>
              </div>
            ) : isUrlQuery ? (
              <div className="flex h-full items-center justify-center px-8 text-center">
                <div>
                  <p className="text-[15px] font-medium text-ink-1">
                    That is not an installable URL.
                  </p>
                  <p className="mt-2 text-[12px] text-ink-3">
                    Use a public repository root such as https://github.com/owner/repository.
                  </p>
                </div>
              </div>
            ) : selected ? (
              <div className="flex h-full min-h-0 flex-col">
                <div className="min-h-0 flex-1 overflow-y-auto">
                  <section className="px-5 pb-4 pt-5">
                    <div className="flex items-center gap-3">
                      <ExtensionIcon ext={selected} />
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-center gap-2">
                          <h2 className="break-words text-[20px] font-semibold leading-tight tracking-[-0.02em] text-ink-1">
                            {selected.name}
                          </h2>
                          {selectedInstalled ? (
                            <span className="rounded-tezbar-chip bg-white/[0.07] px-1.5 py-0.5 text-[10px] font-medium text-ink-3">
                              Installed
                            </span>
                          ) : null}
                        </div>
                        <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-ink-4">
                          <span>{extensionAuthor(selected)}</span>
                          <span>
                            {isWindows ? 'GitHub repository' : formatCount(selected.downloadCount)}
                          </span>
                          <span>v{selected.version}</span>
                        </div>
                      </div>
                    </div>
                    <p className="mt-4 text-[12px] leading-relaxed text-ink-2">
                      {selected.description || 'No description provided.'}
                    </p>
                    {categories.length > 0 ? (
                      <div className="mt-3 flex flex-wrap gap-1.5">
                        {categories.map((category) => (
                          <span
                            key={category}
                            className="rounded-tezbar-chip bg-white/[0.04] px-2 py-1 text-[10px] text-ink-4"
                          >
                            {category}
                          </span>
                        ))}
                      </div>
                    ) : null}
                  </section>

                  <section aria-label="Extension details" className="px-5 pb-5">
                    {Boolean(selected.nativeV2 || selected.runtime === 'rust' || selected.id.startsWith('com.tezbar.native.')) ? (
                      <div className="flex items-center gap-3 rounded-[10px] border border-white/10 bg-white/[0.03] p-3.5">
                        <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-[7px] bg-emerald-500/15 text-emerald-400">
                          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
                            <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
                            <path d="m9 12 2 2 4-4" />
                          </svg>
                        </span>
                        <div className="min-w-0 flex-1">
                          <p className="text-[12px] font-medium text-ink-1">Native Rust Extension</p>
                          <p className="mt-0.5 text-[11px] leading-snug text-ink-3">
                            Executes in-process with zero latency, low memory footprint, and no external runtime dependency.
                          </p>
                        </div>
                      </div>
                    ) : screenshots.length > 0 ? (
                      <div className="extension-screenshots grid grid-cols-2 gap-3">
                        {screenshots.map((url, index) => (
                          <div
                            key={`${url}-${index}`}
                            className="aspect-[16/10] overflow-hidden rounded-[8px] border border-white/10 bg-white/[0.03]"
                          >
                            <CachedImage
                              src={url}
                              alt={`${selected.name} screenshot ${index + 1}`}
                              variant="screenshot"
                              className="h-full w-full object-contain"
                            />
                          </div>
                        ))}
                      </div>
                    ) : (
                      <p className="text-[11px] text-ink-4">No screenshots available.</p>
                    )}
                  </section>

                  <section className="border-t border-white/[0.06]">
                    <div className="px-5 py-4">
                      <div className="divide-y divide-white/[0.06]">
                        <div className="flex items-center gap-3 py-3">
                          <ExtensionIcon ext={selected} size="small" />
                          <div className="min-w-0 flex-1">
                            <p className="text-[11px] font-semibold text-ink-4">Extension ID</p>
                            <p className="mt-0.5 break-all font-mono text-[12px] leading-snug text-ink-2">
                              {selected.id}
                            </p>
                          </div>
                        </div>
                        <div className="flex items-center gap-3 py-3">
                          <AuthorAvatar ext={selected} />
                          <div className="min-w-0 flex-1">
                            <p className="text-[11px] font-semibold text-ink-4">Author</p>
                            <div className="mt-0.5 flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-0.5">
                              <span className="text-[13px] font-medium text-ink-1">
                                {extensionAuthor(selected)}
                              </span>
                              {authorHandle(selected) ? (
                                <span className="font-mono text-[11px] text-ink-4">
                                  @{authorHandle(selected)}
                                </span>
                              ) : null}
                            </div>
                          </div>
                        </div>
                        {runtimeDescription ? (
                          <div className="pl-10 py-3">
                            <p className="text-[11px] font-semibold text-ink-4">Runtime</p>
                            <p className="mt-0.5 text-[12px] text-ink-2">{runtimeDescription.detail}</p>
                          </div>
                        ) : null}
                        {selected.nativeV2 ? (
                          <div className="pl-10 py-3">
                            <p className="text-[11px] font-semibold text-ink-4">Native V2</p>
                            {nativeV2Lines(selected).map((line, index) => (
                              <p
                                key={line}
                                className={index === 0
                                  ? 'mt-0.5 break-all font-mono text-[12px] text-ink-2'
                                  : 'mt-0.5 text-[11px] text-ink-4'}
                              >
                                {line}
                              </p>
                            ))}
                          </div>
                        ) : null}
                        {supports.length > 0 ? (
                          <div className="pl-10 py-3">
                            <p className="text-[11px] font-semibold text-ink-4">Supports</p>
                            <p className="mt-0.5 text-[12px] text-ink-2">{supports.join(', ')}</p>
                          </div>
                        ) : null}
                        {selected.nativeV2 && supports.length > 0 ? (
                          <div className="pl-10 py-3">
                            <p className="text-[11px] font-semibold text-ink-4">Checked</p>
                            {checkedLines(selected, platform).map((line) => (
                              <p key={line} className="mt-0.5 text-[12px] text-ink-2">{line}</p>
                            ))}
                          </div>
                        ) : null}
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

                    <aside className="border-t border-white/[0.06] px-5 py-4">
                      {repositoryUrl(selected) ? (
                        <>
                          <p className="text-[13px] font-semibold text-ink-3">Resources</p>
                          <a
                            href={repositoryUrl(selected)!}
                            onClick={(event) => {
                              event.preventDefault()
                              const url = repositoryUrl(selected)
                              if (url) void window.tezbar.openExternalUrl(url)
                            }}
                            className="mt-2 flex items-center justify-between rounded-[7px] bg-white/[0.03] px-3 py-2 text-[12px] font-medium text-ink-2 transition hover:bg-white/[0.07]"
                          >
                            <span>Open README</span>
                            <span className="text-[20px] text-ink-4">↗</span>
                          </a>
                        </>
                      ) : null}

                      <div className={repositoryUrl(selected) ? 'mt-5' : ''}>
                        <p className="text-[11px] font-semibold text-ink-4">Commands</p>
                        {commands.length > 0 ? (
                          <ul className="mt-2 space-y-1.5">
                            {commands.map((command) => (
                              <li
                                key={command.name || command.title}
                                className="rounded-[7px] bg-white/[0.03] px-3 py-2"
                              >
                                <div className="flex items-center justify-between gap-2">
                                  <p className="truncate text-[12px] font-medium text-ink-2">
                                    {command.title || command.name}
                                  </p>
                                  {command.native ? (
                                    <span className="shrink-0 text-[10px] text-ink-4">
                                      {commandStatus(command.native, platform)}
                                    </span>
                                  ) : null}
                                </div>
                                {command.description ? (
                                  <p className="mt-0.5 line-clamp-2 text-[10.5px] text-ink-4">
                                    {command.description}
                                  </p>
                                ) : null}
                                {command.native?.gap ? (
                                  <p className="mt-0.5 text-[10.5px] text-ink-4">{command.native.gap}</p>
                                ) : null}
                              </li>
                            ))}
                          </ul>
                        ) : (
                          <p className="mt-2 text-[12px] text-ink-4">
                            Command metadata is not available.
                          </p>
                        )}
                      </div>
                    </aside>
                  </section>
                </div>

                <footer className="extension-detail-footer flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-white/[0.06] px-4 py-3">
                  <span className="text-[11px] text-ink-4">
                    {selectedInstalled
                      ? 'Installed extension'
                      : canInstallExtension(selected, platform)
                        ? 'Available to install'
                        : 'Install not available yet'}
                  </span>
                  <Button
                    variant={selectedInstalled ? 'danger' : 'primary'}
                    disabled={loading || !!installing[selected.id] || (!selectedInstalled && !canInstallExtension(selected, platform))}
                    onClick={() => runInstallAction(selected)}
                  >
                    {installing[selected.id] !== undefined
                      ? installProgressLabel(installing[selected.id]!)
                      : selectedInstalled
                        ? 'Remove Extension'
                        : 'Install Extension'}
                  </Button>
                </footer>
              </div>
            ) : (
              <div className="flex h-full items-center justify-center text-[12px] text-ink-3">
                Select an extension to see details.
              </div>
            )}
          </div>
          {msg ? (
            <div role="status" className="extensions-store-message shrink-0 border-t px-4 py-2">
              <Message tone={msg.tone}>{msg.text}</Message>
            </div>
          ) : null}
        </main>
      </section>

      {!embedded ? (
        <div className="glass-card flex shrink-0 items-center justify-between gap-3 px-4 py-2">
          <span className="text-[11px] text-ink-4">
            {total.toLocaleString()} extension{total === 1 ? '' : 's'}
          </span>
          <HintBar>
            <Hint label="Search" keys={<Kbd>/</Kbd>} />
            <Hint
              label="Navigate"
              keys={
                <>
                  <Kbd>↑</Kbd>
                  <Kbd>↓</Kbd>
                </>
              }
            />
            <Hint label="Install / Remove" keys={<Kbd>↵</Kbd>} />
            <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
          </HintBar>
        </div>
      ) : null}
    </div>
  )
}
