/**
 * Persistent preferences for currency conversion.
 *
 * Tracks how often the user picks each target currency so the default
 * target drifts toward the one they actually use. Stored in `localStorage`
 * so it lives per-user without adding an IPC/config store.
 *
 * Data shape:
 *   {
 *     pinned: "KZT" | null,                // explicit override from Settings
 *     usage: { KZT: { count: 7, lastUsedAt: ... }, EUR: {...} }
 *   }
 */

const STORAGE_KEY = 'tezbar:currency:prefs:v1'
/** Initial fallback until the user has any usage/override. */
export const INITIAL_DEFAULT_TARGET = 'EUR'

type UsageRecord = { count: number; lastUsedAt: number }

type Prefs = {
  pinned: string | null
  usage: Record<string, UsageRecord>
}

type StorageLike = {
  getItem: (key: string) => string | null
  setItem: (key: string, value: string) => void
  removeItem: (key: string) => void
}

/** In-memory shim used in non-browser contexts (tests, SSR). */
function memoryStorage(): StorageLike {
  const store = new Map<string, string>()
  return {
    getItem: (k) => (store.has(k) ? store.get(k)! : null),
    setItem: (k, v) => {
      store.set(k, v)
    },
    removeItem: (k) => {
      store.delete(k)
    },
  }
}

let storageOverride: StorageLike | null = null

/** Tests use this to swap in an in-memory store. */
export function __setCurrencyPrefsStorage(s: StorageLike | null): void {
  storageOverride = s
  cache = null
}

function getStorage(): StorageLike {
  if (storageOverride) return storageOverride
  if (typeof globalThis !== 'undefined' && (globalThis as { localStorage?: StorageLike }).localStorage) {
    return (globalThis as unknown as { localStorage: StorageLike }).localStorage
  }
  return memoryStorage()
}

let cache: Prefs | null = null

function isUsageRecord(value: unknown): value is UsageRecord {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as UsageRecord).count === 'number' &&
    typeof (value as UsageRecord).lastUsedAt === 'number'
  )
}

function readPrefs(): Prefs {
  if (cache) return cache
  try {
    const raw = getStorage().getItem(STORAGE_KEY)
    if (!raw) {
      cache = { pinned: null, usage: {} }
      return cache
    }
    const parsed = JSON.parse(raw) as Partial<Prefs>
    const pinned =
      typeof parsed.pinned === 'string' && /^[A-Z]{3}$/.test(parsed.pinned) ? parsed.pinned : null
    const usage: Record<string, UsageRecord> = {}
    if (parsed.usage && typeof parsed.usage === 'object') {
      for (const [k, v] of Object.entries(parsed.usage)) {
        if (/^[A-Z]{3}$/.test(k) && isUsageRecord(v)) {
          usage[k] = { count: Math.max(0, Math.floor(v.count)), lastUsedAt: Math.floor(v.lastUsedAt) }
        }
      }
    }
    cache = { pinned, usage }
  } catch {
    cache = { pinned: null, usage: {} }
  }
  return cache
}

function writePrefs(next: Prefs): void {
  cache = next
  try {
    getStorage().setItem(STORAGE_KEY, JSON.stringify(next))
  } catch {
    /* quota / privacy mode — ignore */
  }
}

/** Record that the user explicitly picked this currency as the conversion target. */
export function recordTargetUsage(code: string): void {
  const c = code.trim().toUpperCase()
  if (!/^[A-Z]{3}$/.test(c)) return
  const prefs = readPrefs()
  const existing = prefs.usage[c]
  const next: Prefs = {
    pinned: prefs.pinned,
    usage: {
      ...prefs.usage,
      [c]: {
        count: (existing?.count ?? 0) + 1,
        lastUsedAt: Date.now(),
      },
    },
  }
  writePrefs(next)
}

/** Returns target currencies sorted by frequency (desc), then recency (desc). */
export function listTargetUsage(): Array<{ code: string; count: number; lastUsedAt: number }> {
  const prefs = readPrefs()
  return Object.entries(prefs.usage)
    .map(([code, r]) => ({ code, count: r.count, lastUsedAt: r.lastUsedAt }))
    .sort((a, b) => {
      if (b.count !== a.count) return b.count - a.count
      return b.lastUsedAt - a.lastUsedAt
    })
}

/** Explicit override chosen in Settings — always wins over usage stats. */
export function getPinnedDefault(): string | null {
  return readPrefs().pinned
}

export function setPinnedDefault(code: string | null): void {
  const prefs = readPrefs()
  const next: Prefs = {
    pinned: code && /^[A-Z]{3}$/.test(code.toUpperCase()) ? code.toUpperCase() : null,
    usage: prefs.usage,
  }
  writePrefs(next)
}

/**
 * Effective default target used when the user's query omits one. Resolution:
 *   1) Pinned override from Settings  → use it
 *   2) Most-used target in history    → use the top one
 *   3) Fallback constant              → INITIAL_DEFAULT_TARGET ("EUR")
 */
export function getPreferredDefaultTarget(): string {
  const prefs = readPrefs()
  if (prefs.pinned) return prefs.pinned
  const top = listTargetUsage()[0]
  return top?.code ?? INITIAL_DEFAULT_TARGET
}
