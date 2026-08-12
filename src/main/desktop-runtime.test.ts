import { afterEach, describe, expect, it, vi } from 'vitest'

import {
  app,
  clipboardImageAppleScript,
  fileClipboardJavaScript,
  imageClipboardAppleScript,
} from './desktop-runtime'

describe('Tauri desktop runtime', () => {
  const previousIsTauri = process.env.IS_TAURI

  afterEach(() => {
    vi.restoreAllMocks()
    if (previousIsTauri === undefined) delete process.env.IS_TAURI
    else process.env.IS_TAURI = previousIsTauri
  })

  it('asks the Tauri host to hide and restore visible app windows', () => {
    process.env.IS_TAURI = 'true'
    const write = vi.spyOn(process.stdout, 'write').mockImplementation(() => true)

    app.hide()
    app.show()

    expect(write).toHaveBeenNthCalledWith(
      1,
      `${JSON.stringify({ type: 'app_visibility', visible: false })}\n`
    )
    expect(write).toHaveBeenNthCalledWith(
      2,
      `${JSON.stringify({ type: 'app_visibility', visible: true })}\n`
    )
  })

  it('builds a file-reference pasteboard payload without copying file bytes', () => {
    const script = fileClipboardJavaScript(['/tmp/report.pdf', '/tmp/report.pdf'])

    expect(script).toContain('const paths = ["/tmp/report.pdf"]')
    expect(script).toContain('pasteboard.writeObjects(urls)')
  })

  it('writes clipboard image data to one reusable temporary file', () => {
    expect(clipboardImageAppleScript('/tmp/clipboard-image.png')).toContain(
      'write (the clipboard as «class PNGf») to fileRef',
    )
  })

  it('preserves the source image type when constructing clipboard AppleScript', () => {
    expect(imageClipboardAppleScript('/tmp/animated.gif')).toBe(
      'set the clipboard to (read (POSIX file "/tmp/animated.gif") as «class GIFf»)'
    )
    expect(imageClipboardAppleScript('/tmp/still.png')).toBe(
      'set the clipboard to (read (POSIX file "/tmp/still.png") as «class PNGf»)'
    )
    expect(imageClipboardAppleScript('/tmp/unsupported.webp')).toBeNull()
  })

  it('escapes paths embedded in clipboard AppleScript', () => {
    expect(imageClipboardAppleScript('/tmp/a "quoted" image.gif')).toContain(
      'POSIX file "/tmp/a \\"quoted\\" image.gif"'
    )
  })
})
