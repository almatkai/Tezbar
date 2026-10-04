import { execFile } from 'node:child_process'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)

export type PowerShellScriptOptions = {
  signal?: AbortSignal
  timeout?: number
}

export function powerShellScriptArguments(source: string): string[] {
  // Windows PowerShell expects UTF-16LE for -EncodedCommand, but stdout must
  // match the UTF-8 encoding used by Node/Bun (including non-ASCII paths).
  const script = `[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)\n${source}`
  return [
    '-NoLogo',
    '-NoProfile',
    '-NonInteractive',
    '-ExecutionPolicy',
    'Bypass',
    '-EncodedCommand',
    Buffer.from(script, 'utf16le').toString('base64'),
  ]
}

export async function runPowerShellScript(
  source: string,
  options: PowerShellScriptOptions = {}
): Promise<string> {
  if (process.platform !== 'win32') throw new Error('PowerShell is only available on Windows')
  if (typeof source !== 'string' || !source.trim()) return ''
  const { stdout } = await execFileAsync('powershell.exe', powerShellScriptArguments(source), {
    encoding: 'utf8',
    maxBuffer: 10 * 1024 * 1024,
    timeout: options.timeout ?? 10_000,
    signal: options.signal,
    windowsHide: true,
  })
  return stdout.replace(/\r?\n$/, '')
}
