import { clipboard, nativeImage } from '@tezbar/desktop-runtime'
import { extname } from 'node:path'
import { request } from './host'

/** Both compatibility APIs must await the host's write before returning. */
export async function copyClipboardContent(value: unknown): Promise<void> {
  if (value && typeof value === 'object') {
    const payload = value as { text?: unknown; html?: unknown; file?: unknown }
    if (typeof payload.file === 'string' && payload.file.trim()) {
      const path = payload.file.trim()
      if (
        ['.png', '.jpg', '.jpeg', '.gif', '.tif', '.tiff', '.bmp', '.webp'].includes(
          extname(path).toLowerCase()
        )
      ) {
        const image = nativeImage.createFromPath(path)
        if (image.isEmpty()) throw new Error(`Clipboard image is unavailable: ${path}`)
        await clipboard.writeImage(image)
      } else {
        await clipboard.writeText(path)
      }
      return
    }
    if (typeof payload.html === 'string') {
      await clipboard.write({
        html: payload.html,
        text: typeof payload.text === 'string' ? payload.text : payload.html,
      })
      return
    }
    if (typeof payload.text === 'string') {
      await clipboard.writeText(payload.text)
      return
    }
  }
  await clipboard.writeText(String(value ?? ''))
}

/** Paste the already-written clipboard, preserving image and HTML formats. */
export async function pasteClipboardContent(): Promise<void> {
  await request('core:platform:paste')
}
