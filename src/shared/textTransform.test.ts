// src/shared/textTransform.test.ts
import { describe, expect, it } from 'vitest'
import { applyClipboardAction } from './textTransform'

describe('applyClipboardAction', () => {
  it('formats JSON', async () => {
    const r = await applyClipboardAction('json', '{"a":1}')
    expect(r.ok).toBe(true)
    if (r.ok) expect(r.output).toContain('\n  "a": 1\n')
  })

  it('encodes then decodes base64', async () => {
    const enc = await applyClipboardAction('base64', 'hello')
    expect(enc.ok).toBe(true)
    if (!enc.ok) return
    const dec = await applyClipboardAction('base64', enc.output)
    expect(dec.ok).toBe(true)
    if (dec.ok) expect(dec.output).toBe('hello')
  })

  it('url-encodes and decodes', async () => {
    const enc = await applyClipboardAction('encode', 'a b&c')
    expect(enc.ok).toBe(true)
    if (!enc.ok) return
    expect(enc.output).toBe('a%20b%26c')
    const dec = await applyClipboardAction('encode', enc.output)
    expect(dec.ok).toBe(true)
    if (dec.ok) expect(dec.output).toBe('a b&c')
  })

  it('counts words/chars/lines', async () => {
    const r = await applyClipboardAction('count', 'one two\nthree')
    expect(r.ok).toBe(true)
    if (!r.ok) return
    expect(r.output).toContain('Characters : 13')
    expect(r.output).toContain('Words      : 3')
    expect(r.output).toContain('Lines      : 2')
  })

  it('flips case', async () => {
    const r = await applyClipboardAction('case', 'hello')
    expect(r.ok).toBe(true)
    if (r.ok) expect(r.output).toBe('HELLO')
  })

  it('hashes with sha256', async () => {
    const r = await applyClipboardAction('hash', 'test')
    expect(r.ok).toBe(true)
    if (r.ok) expect(r.output).toMatch(/^[a-f0-9]{64}$/)
  })

  it('generates QR data URL', async () => {
    const r = await applyClipboardAction('qr', 'https://example.com')
    expect(r.ok).toBe(true)
    if (r.ok) expect(r.output).toMatch(/^data:image\/svg\+xml/)
  })

  it('generates QR for whitespace-only input (renderer handles trimming)', async () => {
    // qrCodeDataUrl accepts any string; trimming is the caller's job
    const r = await applyClipboardAction('qr', '   ')
    expect(r.ok).toBe(true)
    if (r.ok) expect(r.output).toMatch(/^data:image\/svg/)
  })
})
