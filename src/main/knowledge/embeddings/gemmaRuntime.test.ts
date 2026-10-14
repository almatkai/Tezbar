import { randomUUID } from 'node:crypto'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { GemmaRuntime } from './gemmaRuntime'

afterEach(() => vi.unstubAllGlobals())

describe('optional model installation', () => {
  it('rejects queued inference during model removal', async () => {
    process.env.APPDATA_DIR = join(tmpdir(), `tezbar-gemma-remove-${randomUUID()}`)
    const runtime = new GemmaRuntime()
    try {
      const query = runtime.embedQuery('solar particles')
      const removal = runtime.remove()
      await expect(query).rejects.toThrow('being removed')
      await removal
      expect(runtime.snapshot().modelState).toBe('not-downloaded')
    } finally {
      runtime.shutdown()
    }
  })
  it('does not fetch anything until explicitly downloaded', () => {
    process.env.APPDATA_DIR = join(tmpdir(), `tezbar-gemma-download-${randomUUID()}`)
    const fetch = vi.fn()
    vi.stubGlobal('fetch', fetch)
    const runtime = new GemmaRuntime()
    expect(runtime.snapshot().modelState).toBe('not-downloaded')
    expect(fetch).not.toHaveBeenCalled()
    runtime.shutdown()
  })

  it('rejects corrupted downloads and lets the user remove incomplete files', async () => {
    process.env.APPDATA_DIR = join(tmpdir(), `tezbar-gemma-corrupt-${randomUUID()}`)
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(new Uint8Array(100)))
    )
    const runtime = new GemmaRuntime()
    try {
      runtime.startDownload()
      await vi.waitFor(() => expect(runtime.snapshot().modelState).toBe('error'))
      expect(runtime.snapshot().error).toContain('checksum')
      await expect(runtime.embedQuery('solar')).rejects.toThrow('Download EmbeddingGemma')
      await runtime.remove()
      expect(runtime.snapshot().modelState).toBe('not-downloaded')
    } finally {
      runtime.shutdown()
    }
  })
})
