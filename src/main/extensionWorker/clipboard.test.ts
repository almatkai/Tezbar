import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  writeText: vi.fn(),
  write: vi.fn(),
  writeImage: vi.fn(),
  image: { isEmpty: () => false },
  createFromPath: vi.fn(),
  request: vi.fn(),
}))
vi.mock('@tezbar/desktop-runtime', () => ({
  clipboard: { writeText: mocks.writeText, write: mocks.write, writeImage: mocks.writeImage },
  nativeImage: { createFromPath: mocks.createFromPath },
}))
vi.mock('./host', () => ({ request: mocks.request }))
import { copyClipboardContent, pasteClipboardContent } from './clipboard'

beforeEach(() => {
  vi.resetAllMocks()
  mocks.createFromPath.mockReturnValue(mocks.image)
})

describe('native host clipboard adaptation', () => {
  it('awaits the host write before requesting paste', async () => {
    let complete!: () => void
    mocks.writeText.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          complete = resolve
        })
    )
    const operation = (async () => {
      await copyClipboardContent('Қазақша\u2028日本語🐎')
      await pasteClipboardContent()
    })()
    expect(mocks.request).not.toHaveBeenCalled()
    complete()
    await operation
    expect(mocks.writeText).toHaveBeenCalledWith('Қазақша\u2028日本語🐎')
    expect(mocks.request).toHaveBeenCalledWith('core:platform:paste')
  })

  it('preserves HTML and image formats, while ordinary files are copied as paths', async () => {
    await copyClipboardContent({ text: 'hello', html: '<b>hello</b>' })
    expect(mocks.write).toHaveBeenCalledWith({ text: 'hello', html: '<b>hello</b>' })
    await copyClipboardContent({ file: '/fixture/horse.JPG' })
    expect(mocks.writeImage).toHaveBeenCalledWith(mocks.image)
    await copyClipboardContent({ file: '/fixture/readme.txt' })
    expect(mocks.createFromPath).toHaveBeenCalledTimes(1)
    expect(mocks.writeText).toHaveBeenCalledWith('/fixture/readme.txt')
  })

  it('propagates host failures and does not paste after a failed write', async () => {
    mocks.writeText.mockRejectedValue(new Error('host denied'))
    await expect(
      (async () => {
        await copyClipboardContent('text')
        await pasteClipboardContent()
      })()
    ).rejects.toThrow('host denied')
    expect(mocks.request).not.toHaveBeenCalled()
    mocks.request.mockRejectedValue(new Error('paste denied'))
    await expect(pasteClipboardContent()).rejects.toThrow('paste denied')
  })

  it('rejects a missing image instead of claiming the image was copied', async () => {
    mocks.createFromPath.mockReturnValue({ isEmpty: () => true })
    await expect(copyClipboardContent({ file: '/missing.png' })).rejects.toThrow('unavailable')
    expect(mocks.writeText).not.toHaveBeenCalled()
    expect(mocks.writeImage).not.toHaveBeenCalled()
  })
})
