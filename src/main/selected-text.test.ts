import { afterEach, describe, expect, it, vi } from 'vitest'
const { execute, powershell } = vi.hoisted(() => ({ execute: vi.fn(), powershell: vi.fn() }))
vi.mock('node:util', () => ({ promisify: () => execute }))
vi.mock('./powershell-script', () => ({ runPowerShellScript: powershell }))
import { macSelectedTextScript, readSelectedText, windowsSelectedTextScript } from './selected-text'
const descriptor = Object.getOwnPropertyDescriptor(process, 'platform')!
afterEach(() => {
  Object.defineProperty(process, 'platform', descriptor)
  execute.mockReset()
  powershell.mockReset()
})
function platform(value: string) {
  Object.defineProperty(process, 'platform', { ...descriptor, value })
}

describe('extension selected text', () => {
  it('reads the macOS accessibility selection without using the clipboard', async () => {
    platform('darwin')
    execute.mockResolvedValue({ stdout: 'こんにちは\nsecond\n' })
    expect(await readSelectedText()).toBe('こんにちは\nsecond')
    expect(execute).toHaveBeenCalledWith(
      '/usr/bin/osascript',
      ['-e', macSelectedTextScript],
      expect.objectContaining({ timeout: 3000 })
    )
    expect(macSelectedTextScript).toContain('AXSelectedText')
    expect(macSelectedTextScript).not.toContain('keystroke')
  })
  it('uses the readonly Windows TextPattern selection, not Ctrl+C', async () => {
    platform('win32')
    powershell.mockResolvedValue(Buffer.from('selected text\r\n').toString('base64'))
    expect(await readSelectedText()).toBe('selected text\r\n')
    expect(powershell).toHaveBeenCalledWith(windowsSelectedTextScript, { timeout: 3000 })
    expect(windowsSelectedTextScript).toContain('GetSelection()')
    expect(windowsSelectedTextScript).not.toMatch(/SendKeys|Clipboard/)
  })
  it('rejects absent selections instead of returning clipboard contents', async () => {
    platform('win32')
    powershell.mockResolvedValue('')
    await expect(readSelectedText()).rejects.toThrow('No text is selected')
  })
  it('propagates accessibility permission errors', async () => {
    platform('darwin')
    execute.mockRejectedValue(new Error('Access denied'))
    await expect(readSelectedText()).rejects.toThrow('Access denied')
  })
})
