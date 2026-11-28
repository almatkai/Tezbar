import { describe, expect, it, vi } from 'vitest'
import { qrDataUrlToPngDataUrl } from './qrImage'

describe('qrDataUrlToPngDataUrl', () => {
  it('rasterizes the generated SVG into a PNG-backed canvas image', async () => {
    const drawImage = vi.fn()
    const toDataURL = vi.fn(() => 'data:image/png;base64,cXI=')
    const source = { onload: null as (() => void) | null, onerror: null as (() => void) | null, src: '' }
    const canvas = { width: 0, height: 0, getContext: vi.fn(() => ({ drawImage })), toDataURL }

    const pending = qrDataUrlToPngDataUrl(
      'data:image/svg+xml;base64,PHN2Zy8+',
      {
        createImage: () => source as unknown as HTMLImageElement,
        createCanvas: () => canvas as unknown as HTMLCanvasElement,
      },
    )
    source.onload?.()

    await expect(pending).resolves.toBe('data:image/png;base64,cXI=')
    expect(source.src).toBe('data:image/svg+xml;base64,PHN2Zy8+')
    expect(canvas.width).toBe(512)
    expect(canvas.height).toBe(512)
    expect(drawImage).toHaveBeenCalledWith(source, 0, 0, 512, 512)
    expect(toDataURL).toHaveBeenCalledWith('image/png')
  })
})
