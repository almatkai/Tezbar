import { EventEmitter } from 'node:events'
import { PassThrough } from 'node:stream'
import { afterEach, describe, expect, it, vi } from 'vitest'

const transport = vi.hoisted(() => ({ spawn: vi.fn(), execFileSync: vi.fn() }))
vi.mock('node:child_process', async (importOriginal) => ({
  ...(await importOriginal<typeof import('node:child_process')>()),
  spawn: transport.spawn,
  execFileSync: transport.execFileSync,
}))

import { clipboard } from './desktop-runtime'

const fixture = 'Привет Қазақша العربية 中文 日本語 한국어 हिन्दी 😀\nsecond line\n'
const utf8Locale = { LANG: 'en_US.UTF-8', LC_CTYPE: 'en_US.UTF-8', LC_ALL: 'en_US.UTF-8' }
const originalPlatform = Object.getOwnPropertyDescriptor(process, 'platform')!

describe('macOS clipboard Unicode transport', () => {
  afterEach(() => {
    vi.restoreAllMocks()
    Object.defineProperty(process, 'platform', originalPlatform)
    vi.unstubAllEnvs()
    vi.clearAllMocks()
  })

  function useMacLocale(locale: string | undefined): void {
    Object.defineProperty(process, 'platform', { ...originalPlatform, value: 'darwin' })
    for (const key of ['LANG', 'LC_CTYPE', 'LC_ALL']) vi.stubEnv(key, locale)
  }

  it.each(['C', undefined])('reads UTF-8 even when the inherited locale is %s', (locale) => {
    useMacLocale(locale)
    transport.execFileSync.mockReturnValue(fixture)

    expect(clipboard.readText()).toBe(fixture)
    expect(transport.execFileSync).toHaveBeenCalledWith(
      'pbpaste',
      [],
      expect.objectContaining({
        encoding: 'utf8',
        env: expect.objectContaining(utf8Locale),
      })
    )
    expect(process.env.LC_ALL).toBe(locale)
  })

  it.each(['C', undefined])(
    'writes UTF-8 and waits for the clipboard process with locale %s',
    async (locale) => {
      useMacLocale(locale)
      const child = Object.assign(new EventEmitter(), { stdin: new PassThrough() })
      const chunks: Buffer[] = []
      child.stdin.on('data', (chunk: Buffer) => chunks.push(chunk))
      transport.spawn.mockReturnValue(child)
      let completed = false

      const pending = clipboard.writeTextAsync(fixture).then(() => {
        completed = true
      })
      await Promise.resolve()
      await Promise.resolve()
      const completedBeforeProcessExit = completed
      child.emit('close', 0)
      await pending

      expect(transport.spawn).toHaveBeenCalledWith(
        'pbcopy',
        [],
        expect.objectContaining({
          env: expect.objectContaining(utf8Locale),
        })
      )
      expect(Buffer.concat(chunks).toString('utf8')).toBe(fixture)
      expect(completedBeforeProcessExit).toBe(false)
      expect(process.env.LC_ALL).toBe(locale)
    }
  )

  it('propagates a failed macOS copy rather than reporting success', async () => {
    useMacLocale('C')
    const child = Object.assign(new EventEmitter(), { stdin: new PassThrough() })
    transport.spawn.mockReturnValue(child)
    const pending = clipboard.writeTextAsync(fixture)
    const assertion = expect(pending).rejects.toThrow('clipboard write failed')
    child.emit('close', 1)
    await assertion
  })
})
