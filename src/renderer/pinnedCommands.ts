import type { SearchResult } from '../shared/search'
import { DEEP_SEARCH_RESULT_PREFIX } from '../shared/searchMode'

export const PINNED_COMMANDS_KEY = 'tezbar:pinned-commands'
export const MAX_PINNED_COMMANDS = 12

export type PinnedCommand = {
  id: string
  title: string
  subtitle: string
  category: SearchResult['category']
  action: SearchResult['action']
  iconDataUrl?: string
  /** ⌥ + number-row key derived from the pin's current order (10→0, 11→-, 12→=). */
  slot: number
}

export type PinnedCommandDraft = Omit<PinnedCommand, 'slot'> & { slot?: number }

export function isPinnableSearchResult(result: SearchResult): boolean {
  return (
    result.category !== 'calculator' &&
    result.category !== 'color-converter' &&
    !result.id.startsWith(DEEP_SEARCH_RESULT_PREFIX)
  )
}

/** Keep shortcuts aligned with the current pin order. */
export function normalizePinnedSlots(pins: PinnedCommandDraft[]): PinnedCommand[] {
  return pins.slice(0, MAX_PINNED_COMMANDS).map((pin, index) => ({ ...pin, slot: index + 1 }))
}

export function reorderPinnedByInsertIndex(
  pins: PinnedCommand[],
  draggedId: string,
  insertIndex: number
): PinnedCommand[] {
  const from = pins.findIndex((p) => p.id === draggedId)
  if (from < 0) return pins
  let to = Math.max(0, Math.min(insertIndex, pins.length))
  const next = [...pins]
  const [item] = next.splice(from, 1)
  if (!item) return pins
  if (from < to) to -= 1
  if (from === to) return pins
  next.splice(to, 0, item)
  return normalizePinnedSlots(next)
}

/**
 * Inserts a new pinned command. Defaults to appending to the end (bottom of rail),
 * ensuring newly pinned items do not displace existing slots/order.
 */
export function insertPinnedCommand(
  pins: PinnedCommand[],
  result: SearchResult,
  insertIndex = pins.length
): PinnedCommand[] {
  const to = Math.max(0, Math.min(insertIndex, pins.length))
  const candidate: PinnedCommandDraft = {
    id: result.id,
    title: result.title,
    subtitle: result.subtitle,
    category: result.category,
    action: result.action,
    ...(result.iconDataUrl ? { iconDataUrl: result.iconDataUrl } : {}),
  }
  return normalizePinnedSlots([
    ...pins.slice(0, to),
    candidate,
    ...pins.slice(to),
  ])
}

export function parsePinnedSlotFromKeyEvent(event: { code?: string; key?: string }): number | null {
  const fromCode = /^Digit([0-9])$/.exec(event.code ?? '')?.[1]
  if (fromCode) return fromCode === '0' ? 10 : Number(fromCode)
  if (event.code === 'Minus') return 11
  if (event.code === 'Equal') return 12
  if (event.key && /^[1-9]$/.test(event.key)) return Number(event.key)
  if (event.key === '0') return 10
  return null
}

export function pinnedSlotShortcutKey(slot: number): string {
  if (slot === 10) return '0'
  if (slot === 11) return '-'
  if (slot === 12) return '='
  return String(slot)
}

export function readPinnedCommands(storage?: Storage): PinnedCommand[] {
  try {
    const targetStorage =
      storage ?? (typeof window !== 'undefined' ? window.localStorage : undefined)
    const raw = targetStorage?.getItem(PINNED_COMMANDS_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []

    const drafts = parsed
      .map((item) => {
        const id = String(item?.id ?? '').trim()
        const title = String(item?.title ?? '').trim()
        const subtitle = String(item?.subtitle ?? '').trim()
        const category = item?.category
        const action = item?.action
        const hasValidAction =
          typeof action === 'object' &&
          action !== null &&
          typeof (action as { type?: unknown }).type === 'string'

        if (!id || !title || typeof category !== 'string' || !hasValidAction) {
          return null
        }

        const rawSlot = item?.slot
        const slotNum =
          typeof rawSlot === 'number' && rawSlot >= 1 && rawSlot <= MAX_PINNED_COMMANDS
            ? Math.floor(rawSlot)
            : undefined
        const iconDataUrl =
          typeof item?.iconDataUrl === 'string' && item.iconDataUrl.trim()
            ? item.iconDataUrl
            : undefined

        return {
          id,
          title,
          subtitle,
          category: category as SearchResult['category'],
          action: action as SearchResult['action'],
          ...(iconDataUrl ? { iconDataUrl } : {}),
          ...(slotNum !== undefined ? { slot: slotNum } : {}),
        } satisfies PinnedCommandDraft
      })
      .filter((item): item is PinnedCommandDraft => item !== null)
      .slice(0, MAX_PINNED_COMMANDS)

    return normalizePinnedSlots(drafts)
  } catch {
    return []
  }
}

export function writePinnedCommands(next: PinnedCommand[], storage?: Storage): void {
  try {
    const targetStorage =
      storage ?? (typeof window !== 'undefined' ? window.localStorage : undefined)
    targetStorage?.setItem(
      PINNED_COMMANDS_KEY,
      JSON.stringify(next.slice(0, MAX_PINNED_COMMANDS))
    )
  } catch {
    // Storage can be unavailable in private/webview contexts.
  }
}
