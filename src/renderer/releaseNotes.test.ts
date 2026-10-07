import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  CURRENT_APP_VERSION,
  fetchReleaseNotes,
  LAST_RELEASE_NOTES_VERSION_KEY,
  shouldOfferPostUpdateNotes,
  withReleaseNotesResult,
  RELEASE_NOTES_COMMAND_ID,
} from './releaseNotes'
import type { SearchResult } from '../shared/search'

afterEach(() => {
  vi.unstubAllGlobals()
  vi.unstubAllEnvs()
})

describe('installed-version release notes', () => {
  it('shows the notes bundled with the installed app without a network request', async () => {
    const body = '## Tezbar improvements\n\n- More reliable Windows updates.'
    vi.stubEnv('VITE_APP_RELEASE_NOTES', body)
    const fetch = vi.fn().mockRejectedValue(new Error('Offline'))
    vi.stubGlobal('fetch', fetch)
    expect(await fetchReleaseNotes(CURRENT_APP_VERSION, new AbortController().signal)).toEqual({
      version: CURRENT_APP_VERSION,
      body,
      publishedAt: null,
    })
    expect(fetch).not.toHaveBeenCalled()
  })

  it('does not substitute remote release descriptions when the installed notes are missing', async () => {
    vi.stubEnv('VITE_APP_RELEASE_NOTES', '')
    const fetch = vi.fn()
    vi.stubGlobal('fetch', fetch)
    await expect(
      fetchReleaseNotes(CURRENT_APP_VERSION, new AbortController().signal)
    ).rejects.toThrow('missing its Tezbar release notes')
    expect(fetch).not.toHaveBeenCalled()
  })

  it('fetches the exact tag and preserves Markdown without truncation', async () => {
    const body = '# What’s new\n\n' + '- A change\n'.repeat(20)
    const fetch = vi
      .fn()
      .mockResolvedValue({ ok: true, json: async () => ({ tag_name: 'v0.3.0', body }) })
    vi.stubGlobal('fetch', fetch)
    const signal = new AbortController().signal
    expect((await fetchReleaseNotes('0.3.0', signal)).body).toBe(body)
    expect(fetch).toHaveBeenCalledWith(
      'https://api.github.com/repos/almatkai/Tezbar/releases/tags/v0.3.0',
      expect.objectContaining({ signal })
    )
  })

  it('rejects notes for a different version', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({ tag_name: 'v0.4.0', body: 'Wrong release' }),
      })
    )
    await expect(fetchReleaseNotes('0.3.0', new AbortController().signal)).rejects.toThrow(
      'does not match'
    )
  })

  it('handles missing releases without pretending there are notes', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false }))
    await expect(fetchReleaseNotes('0.3.0', new AbortController().signal)).rejects.toThrow(
      'unavailable'
    )
  })
})

describe('post-update release notes', () => {
  function storage(previousVersion: string | null) {
    let value = previousVersion
    return {
      getItem: vi.fn(() => value),
      setItem: vi.fn((_key: string, next: string) => {
        value = next
      }),
    }
  }

  it('skips a fresh install and records its version', () => {
    const saved = storage(null)
    expect(shouldOfferPostUpdateNotes(saved, true, '0.2.0')).toBe(false)
    expect(saved.setItem).toHaveBeenCalledWith(LAST_RELEASE_NOTES_VERSION_KEY, '0.2.0')
    expect(shouldOfferPostUpdateNotes(saved, false, '0.2.0')).toBe(false)
    expect(shouldOfferPostUpdateNotes(saved, false, '0.3.0')).toBe(true)
  })

  it('shows an upgraded version until the user dismisses it', () => {
    const saved = storage('0.1.0')
    expect(shouldOfferPostUpdateNotes(saved, false, '0.2.0')).toBe(true)
    expect(saved.setItem).not.toHaveBeenCalled()
    saved.setItem(LAST_RELEASE_NOTES_VERSION_KEY, '0.2.0')
    expect(shouldOfferPostUpdateNotes(saved, false, '0.2.0')).toBe(false)
  })

  it('shows notes to existing users upgrading from before version tracking', () => {
    expect(shouldOfferPostUpdateNotes(storage(null), false, '0.2.0')).toBe(true)
  })
})

describe('Show Updated Version launcher command', () => {
  const update: SearchResult = {
    id: 'command:check-for-updates',
    title: 'Check for Updates',
    subtitle: '',
    category: 'commands',
    score: 1_000_000,
    action: { type: 'invoke-command', commandId: 'check-for-updates' },
  }

  it('puts unread app notes first, ahead of the updater command', () => {
    const rows = withReleaseNotesResult([update], '', true, '0.2.2')
    expect(rows.map((row) => row.title)).toEqual(['Show Updated Version', 'Check for Updates'])
    expect(rows[0]?.subtitle).toContain('v0.2.2')
    expect(rows[0]?.action).toEqual({ type: 'invoke-command', commandId: RELEASE_NOTES_COMMAND_ID })
  })

  it('keeps the home list unchanged after notes have been read', () => {
    const rows = [update]
    expect(withReleaseNotesResult(rows, '', false)).toBe(rows)
  })

  it('lets users find the notes again by searching', () => {
    for (const query of ['show updated version', 'release notes', "what's new", 'version']) {
      expect(withReleaseNotesResult([], query, false)[0]?.title).toBe('Show Updated Version')
    }
  })

  it('does not interfere with file, deep, or unrelated searches', () => {
    const rows = [update]
    for (const query of ['/notes', '!release notes', '`tezbar', 'spreadsheet']) {
      expect(withReleaseNotesResult(rows, query, true)).toBe(rows)
    }
  })

  it('does not duplicate an existing release-notes command', () => {
    const rows = withReleaseNotesResult([], '', true)
    expect(withReleaseNotesResult(rows, '', true)).toHaveLength(1)
  })
})
