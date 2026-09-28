import type { ExtensionManifest, InstalledExtension } from '../shared/extensions'

export type CatalogMessage = { tone: 'success' | 'error'; text: string }

export type ExtensionCatalogState = {
  query: string
  loading: boolean
  loadingMore: boolean
  loadRequestId: number | null
  installing: Record<string, number>
  store: ExtensionManifest[]
  installed: InstalledExtension[]
  selectedId: string | null
  followSelection: boolean
  message: CatalogMessage | null
  total: number
  hasMore: boolean
}

export type ExtensionCatalogAction =
  | { type: 'query'; query: string }
  | { type: 'load-started'; requestId: number }
  | {
      type: 'load-succeeded'
      requestId: number
      store: ExtensionManifest[]
      installed: InstalledExtension[]
      total?: number
      hasMore?: boolean
    }
  | { type: 'load-failed'; requestId: number; message: string }
  | { type: 'load-more-started' }
  | {
      type: 'load-more-succeeded'
      items: ExtensionManifest[]
      total?: number
      hasMore?: boolean
      selectNext?: boolean
    }
  | { type: 'load-more-failed'; message?: string }
  | { type: 'selected'; id: string | null; follow: boolean }
  | { type: 'install-progress'; id: string; progress: number }
  | { type: 'install-started'; id: string }
  | { type: 'install-finished'; id: string; message: CatalogMessage }
  | { type: 'message'; message: CatalogMessage | null }

export const INITIAL_EXTENSION_CATALOG_STATE: ExtensionCatalogState = {
  query: '',
  loading: false,
  loadingMore: false,
  loadRequestId: null,
  installing: {},
  store: [],
  installed: [],
  selectedId: null,
  followSelection: true,
  message: null,
  total: 0,
  hasMore: false,
}

function withoutInstalling(
  installing: Record<string, number>,
  id: string,
): Record<string, number> {
  const next = { ...installing }
  delete next[id]
  return next
}

export function extensionCatalogReducer(
  state: ExtensionCatalogState,
  action: ExtensionCatalogAction,
): ExtensionCatalogState {
  switch (action.type) {
    case 'query':
      return { ...state, query: action.query }
    case 'load-started':
      return { ...state, loading: true, loadingMore: false, loadRequestId: action.requestId }
    case 'load-succeeded':
      if (state.loadRequestId !== action.requestId) return state
      return {
        ...state,
        loading: false,
        loadingMore: false,
        loadRequestId: null,
        store: action.store,
        installed: action.installed,
        total: typeof action.total === 'number' ? action.total : action.store.length,
        hasMore: action.hasMore ?? false,
      }
    case 'load-failed':
      if (state.loadRequestId !== action.requestId) return state
      return {
        ...state,
        loading: false,
        loadingMore: false,
        loadRequestId: null,
        message: { tone: 'error', text: action.message },
      }
    case 'load-more-started':
      return { ...state, loadingMore: true }
    case 'load-more-succeeded': {
      const nextStore = [...state.store, ...action.items]
      const nextSelectedId =
        action.selectNext && action.items.length > 0
          ? action.items[0]?.id ?? state.selectedId
          : state.selectedId
      return {
        ...state,
        loadingMore: false,
        store: nextStore,
        total: typeof action.total === 'number' ? action.total : nextStore.length,
        hasMore: action.hasMore ?? false,
        selectedId: nextSelectedId,
        followSelection: action.selectNext ? true : state.followSelection,
      }
    }
    case 'load-more-failed':
      return {
        ...state,
        loadingMore: false,
        message: action.message ? { tone: 'error', text: action.message } : state.message,
      }
    case 'selected':
      return { ...state, selectedId: action.id, followSelection: action.follow }
    case 'install-progress':
      return {
        ...state,
        installing:
          action.progress >= 100
            ? withoutInstalling(state.installing, action.id)
            : { ...state.installing, [action.id]: action.progress },
      }
    case 'install-started':
      return { ...state, installing: { ...state.installing, [action.id]: 5 } }
    case 'install-finished':
      return {
        ...state,
        installing: withoutInstalling(state.installing, action.id),
        message: action.message,
      }
    case 'message':
      return { ...state, message: action.message }
  }
}
