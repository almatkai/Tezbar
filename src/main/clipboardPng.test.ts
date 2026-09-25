import { describe, expect, it } from 'vitest'
import { decodePngDataUrl } from './clipboardPng'

describe('decodePngDataUrl', () => {
  it('accepts a PNG payload and rejects other data URLs', () => {
    const bytes = decodePngDataUrl('data:image/png;base64,iVBORw0KGgo=')
    expect(bytes?.subarray(0, 8).toString('hex')).toBe('89504e470d0a1a0a')
    expect(decodePngDataUrl('data:image/svg+xml;base64,PHN2Zy8+')).toBeNull()
  })
})
