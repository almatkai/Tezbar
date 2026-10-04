import { afterEach, describe, expect, it, vi } from 'vitest'

const { execute } = vi.hoisted(() => ({ execute: vi.fn() }))
vi.mock('node:util', () => ({ promisify: () => execute }))
import { powerShellScriptArguments, runPowerShellScript } from './powershell-script'

const originalPlatform = Object.getOwnPropertyDescriptor(process, 'platform')!
afterEach(() => {
  Object.defineProperty(process, 'platform', originalPlatform)
  execute.mockReset()
})

function windows(): void {
  Object.defineProperty(process, 'platform', { ...originalPlatform, value: 'win32' })
}

describe('PowerShell extension bridge', () => {
  it('uses encoded UTF-16LE scripts instead of shell interpolation', () => {
    const source = `Write-Output "C:\\Users\\O'Brien\\こんにちは"`
    const args = powerShellScriptArguments(source)
    expect(args.slice(0, -1)).toEqual([
      '-NoLogo',
      '-NoProfile',
      '-NonInteractive',
      '-ExecutionPolicy',
      'Bypass',
      '-EncodedCommand',
    ])
    expect(Buffer.from(args.at(-1)!, 'base64').toString('utf16le')).toBe(
      `[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)\n${source}`
    )
  })

  it('passes cancellation and timeout; preserves multiline UTF-8 output', async () => {
    windows()
    const signal = new AbortController().signal
    execute.mockResolvedValue({ stdout: 'こんにちは\r\nsecond\r\n' })
    expect(await runPowerShellScript('Write-Output test', { signal, timeout: 1234 })).toBe(
      'こんにちは\r\nsecond'
    )
    expect(execute).toHaveBeenCalledWith(
      'powershell.exe',
      powerShellScriptArguments('Write-Output test'),
      expect.objectContaining({ signal, timeout: 1234, encoding: 'utf8', windowsHide: true })
    )
  })

  it('rejects execution failures', async () => {
    windows()
    execute.mockRejectedValue(new Error('PowerShell failed'))
    await expect(runPowerShellScript('throw "failed"')).rejects.toThrow('PowerShell failed')
  })

  it('does not spawn empty scripts', async () => {
    windows()
    expect(await runPowerShellScript(' ')).toBe('')
    expect(execute).not.toHaveBeenCalled()
  })

  it('does not attempt to execute Windows PowerShell on macOS', async () => {
    Object.defineProperty(process, 'platform', { ...originalPlatform, value: 'darwin' })
    await expect(runPowerShellScript('Write-Output test')).rejects.toThrow(
      'only available on Windows'
    )
    expect(execute).not.toHaveBeenCalled()
  })
})
