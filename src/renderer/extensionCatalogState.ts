import type { ExtensionManifest, InstalledExtension } from '../shared/extensions'

export type CatalogMessage = { tone: 'success' | 'error'; text: string }

export type ExtensionCatalogState = {
  query: string
  loading: boolean
  installing: Record<string, number>
  store: ExtensionManifest[]
  installed: InstalledExtension[]
  selectedId: string | null
  followSelection: boolean
  message: CatalogMessage | null
}

export type ExtensionCatalogAction =
  | { type: 'query'; query: string }
  | { type: 'load-started' }
  | { type: 'load-succeeded'; store: ExtensionManifest[]; installed: InstalledExtension[] }
  | { type: 'load-failed'; message: string }
  | { type: 'selected'; id: string | null; follow: boolean }
  | { type: 'install-progress'; id: string; progress: number }
  | { type: 'install-started'; id: string }
  | { type: 'install-finished'; id: string; message: CatalogMessage }
  | { type: 'message'; message: CatalogMessage | null }

export const INITIAL_EXTENSION_CATALOG_STATE: ExtensionCatalogState = {
  query: '',
  loading: false,
  installing: {},
  store: [],
  installed: [],
  selectedId: null,
  followSelection: true,
  message: null,
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
      return { ...state, loading: true }
    case 'load-succeeded':
      return { ...state, loading: false, store: action.store, installed: action.installed }
    case 'load-failed':
      return {
        ...state,
        loading: false,
        message: { tone: 'error', text: action.message },
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
      return { ...state, installing: { ...state.installing, [action.id]: 1 } }
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
