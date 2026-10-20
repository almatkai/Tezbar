import { describe, expect, it } from 'vitest'
import type { SearchResult } from '../shared/search'
import {
  MAX_PINNED_COMMANDS,
  PINNED_COMMANDS_KEY,
  insertPinnedCommand,
  isPinnableSearchResult,
  normalizePinnedSlots,
  parsePinnedSlotFromKeyEvent,
  pinnedSlotShortcutKey,
  readPinnedCommands,
  reorderPinnedByInsertIndex,
  writePinnedCommands,
  type PinnedCommand,
} from './pinnedCommands'

function makeResult(
  id: string,
  title: string,
  category: SearchResult['category'] = 'commands'
): SearchResult {
  return {
    id,
    title,
    subtitle: `Subtitle for ${title}`,
    category,
    score: 100,
    action: { type: 'invoke-command', commandId: id },
  }
}

function makePin(id: string, title: string, slot: number): PinnedCommand {
  return {
    id,
    title,
    subtitle: `Subtitle for ${title}`,
    category: 'commands',
    slot,
    action: { type: 'invoke-command', commandId: id },
  }
}

class MemoryStorage implements Storage {
  private store = new Map<string, string>()

  get length(): number {
    return this.store.size
  }

  clear(): void {
    this.store.clear()
  }

  getItem(key: string): string | null {
    return this.store.get(key) ?? null
  }

  key(index: number): string | null {
    return Array.from(this.store.keys())[index] ?? null
  }

  removeItem(key: string): void {
    this.store.delete(key)
  }

  setItem(key: string, value: string): void {
    this.store.set(key, value)
  }
}

describe('pinnedCommands', () => {
  describe('normalizePinnedSlots', () => {
    it('assigns 1-based sequential slot numbers', () => {
      const drafts = [
        {
          id: 'a',
          title: 'A',
          subtitle: '',
          category: 'commands' as const,
          action: { type: 'invoke-command' as const, commandId: 'a' },
        },
        {
          id: 'b',
          title: 'B',
          subtitle: '',
          category: 'commands' as const,
          action: { type: 'invoke-command' as const, commandId: 'b' },
          slot: 99,
        },
      ]
      const normalized = normalizePinnedSlots(drafts)
      expect(normalized.map((p) => ({ id: p.id, slot: p.slot }))).toEqual([
        { id: 'a', slot: 1 },
        { id: 'b', slot: 2 },
      ])
    })

    it('caps pins at MAX_PINNED_COMMANDS', () => {
      const drafts = Array.from({ length: 20 }, (_, i) => ({
        id: `pin-${i}`,
        title: `Pin ${i}`,
        subtitle: '',
        category: 'commands' as const,
        action: { type: 'invoke-command' as const, commandId: `pin-${i}` },
      }))
      const normalized = normalizePinnedSlots(drafts)
      expect(normalized).toHaveLength(MAX_PINNED_COMMANDS)
      expect(normalized[normalized.length - 1]?.slot).toBe(MAX_PINNED_COMMANDS)
    })
  })

  describe('insertPinnedCommand', () => {
    it('appends a new pin to the bottom by default (preserving existing pins)', () => {
      const clipboardPin = makePin('clipboard', 'Clipboard History', 1)
      const existingPins: PinnedCommand[] = [clipboardPin]

      const snippetsResult = makeResult('snippets', 'Snippets')
      const updated = insertPinnedCommand(existingPins, snippetsResult)

      expect(updated).toHaveLength(2)
      // Clipboard remains slot 1 at the top
      expect(updated[0]).toMatchObject({ id: 'clipboard', slot: 1 })
      // Snippets is appended to the bottom as slot 2
      expect(updated[1]).toMatchObject({ id: 'snippets', slot: 2 })
    })

    it('appends a third pin to the bottom as slot 3', () => {
      const pins: PinnedCommand[] = [
        makePin('clipboard', 'Clipboard', 1),
        makePin('snippets', 'Snippets', 2),
      ]

      const killProcessResult = makeResult('kill-process', 'Kill Process')
      const updated = insertPinnedCommand(pins, killProcessResult)

      expect(updated).toHaveLength(3)
      expect(updated[0]).toMatchObject({ id: 'clipboard', slot: 1 })
      expect(updated[1]).toMatchObject({ id: 'snippets', slot: 2 })
      expect(updated[2]).toMatchObject({ id: 'kill-process', slot: 3 })
    })

    it('supports explicit insertIndex for drag and drop insertion', () => {
      const pins: PinnedCommand[] = [
        makePin('clipboard', 'Clipboard', 1),
        makePin('snippets', 'Snippets', 2),
      ]

      const openPorts = makeResult('open-ports', 'Open Ports')
      // Drop at index 0 (top)
      const atTop = insertPinnedCommand(pins, openPorts, 0)
      expect(atTop.map((p) => ({ id: p.id, slot: p.slot }))).toEqual([
        { id: 'open-ports', slot: 1 },
        { id: 'clipboard', slot: 2 },
        { id: 'snippets', slot: 3 },
      ])

      // Drop at index 1 (between 0 and 1)
      const inMiddle = insertPinnedCommand(pins, openPorts, 1)
      expect(inMiddle.map((p) => ({ id: p.id, slot: p.slot }))).toEqual([
        { id: 'clipboard', slot: 1 },
        { id: 'open-ports', slot: 2 },
        { id: 'snippets', slot: 3 },
      ])
    })
  })

  describe('reorderPinnedByInsertIndex', () => {
    it('returns the same array if draggedId is not found', () => {
      const pins = [makePin('a', 'A', 1), makePin('b', 'B', 2)]
      expect(reorderPinnedByInsertIndex(pins, 'missing', 0)).toBe(pins)
    })

    it('moves an item and normalizes slots', () => {
      const pins = [makePin('a', 'A', 1), makePin('b', 'B', 2), makePin('c', 'C', 3)]
      const reordered = reorderPinnedByInsertIndex(pins, 'c', 0)
      expect(reordered.map((p) => ({ id: p.id, slot: p.slot }))).toEqual([
        { id: 'c', slot: 1 },
        { id: 'a', slot: 2 },
        { id: 'b', slot: 3 },
      ])
    })
  })

  describe('parsePinnedSlotFromKeyEvent and shortcut keys', () => {
    it('parses digit keys into slot numbers', () => {
      expect(parsePinnedSlotFromKeyEvent({ code: 'Digit1' })).toBe(1)
      expect(parsePinnedSlotFromKeyEvent({ code: 'Digit9' })).toBe(9)
      expect(parsePinnedSlotFromKeyEvent({ code: 'Digit0' })).toBe(10)
      expect(parsePinnedSlotFromKeyEvent({ code: 'Minus' })).toBe(11)
      expect(parsePinnedSlotFromKeyEvent({ code: 'Equal' })).toBe(12)
      expect(parsePinnedSlotFromKeyEvent({ key: '3' })).toBe(3)
      expect(parsePinnedSlotFromKeyEvent({ key: '0' })).toBe(10)
      expect(parsePinnedSlotFromKeyEvent({ key: 'x' })).toBeNull()
    })

    it('formats slot shortcut keys', () => {
      expect(pinnedSlotShortcutKey(1)).toBe('1')
      expect(pinnedSlotShortcutKey(9)).toBe('9')
      expect(pinnedSlotShortcutKey(10)).toBe('0')
      expect(pinnedSlotShortcutKey(11)).toBe('-')
      expect(pinnedSlotShortcutKey(12)).toBe('=')
    })
  })

  describe('isPinnableSearchResult', () => {
    it('allows regular commands and extensions', () => {
      expect(isPinnableSearchResult(makeResult('cmd', 'Command'))).toBe(true)
    })

    it('rejects calculator, color-converter, and deep-search results', () => {
      expect(isPinnableSearchResult(makeResult('calc', '42', 'calculator'))).toBe(false)
      expect(isPinnableSearchResult(makeResult('color', '#fff', 'color-converter'))).toBe(false)
      expect(isPinnableSearchResult(makeResult('deep-search:query', 'Deep search result'))).toBe(false)
    })
  })

  describe('storage serialization and reading', () => {
    it('reads and writes pinned commands roundtrip', () => {
      const storage = new MemoryStorage()
      const pins = [makePin('a', 'A', 1), makePin('b', 'B', 2)]
      writePinnedCommands(pins, storage)

      const read = readPinnedCommands(storage)
      expect(read).toHaveLength(2)
      expect(read[0]?.id).toBe('a')
      expect(read[0]?.slot).toBe(1)
      expect(read[1]?.id).toBe('b')
      expect(read[1]?.slot).toBe(2)
    })

    it('returns empty array on empty or corrupt storage', () => {
      const storage = new MemoryStorage()
      expect(readPinnedCommands(storage)).toEqual([])

      storage.setItem(PINNED_COMMANDS_KEY, '{invalid json}')
      expect(readPinnedCommands(storage)).toEqual([])
    })
  })
})
