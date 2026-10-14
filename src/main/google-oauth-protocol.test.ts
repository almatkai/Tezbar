import { execFileSync } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname } from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { registerWindowsOAuthProtocol } from './google-oauth-protocol'
import { powerShellScriptArguments, runPowerShellScript } from './powershell-script'

vi.mock('./powershell-script', async (original) => ({
  ...(await original<typeof import('./powershell-script')>()),
  runPowerShellScript: vi.fn(),
}))

const run = (script: string): string =>
  execFileSync(
    'powershell.exe',
    powerShellScriptArguments(`$ProgressPreference = 'SilentlyContinue'\n${script}`),
    {
      encoding: 'utf8',
      windowsHide: true,
      timeout: 10_000,
    }
  ).trim()
const invoke = vi.mocked(runPowerShellScript)
let registryRoot = ''
let bridgeDirectory = ''

beforeEach(() => {
  registryRoot = `Software\\TezbarOAuthTests-${randomUUID()}`
  bridgeDirectory = ''
  invoke.mockReset().mockImplementation(async (source) => {
    const match = /& '(.+)' -Mode Register/.exec(source)
    if (match) {
      const scriptPath = match[1].replace(/''/g, "'")
      bridgeDirectory = dirname(scriptPath)
      // Exercise real Windows registry APIs in an isolated test key. Never
      // register a URL handler under Software\Classes on the test machine.
      const script = readFileSync(scriptPath, 'utf8')
      expect(script).toContain("$base = 'Software\\Classes\\com.raycast'")
      writeFileSync(
        scriptPath,
        script.replace("$base = 'Software\\Classes\\com.raycast'", `$base = '${registryRoot}'`)
      )
    }
    return run(source)
  })
})

afterEach(() => {
  if (process.platform === 'win32' && registryRoot.startsWith('Software\\TezbarOAuthTests-')) {
    run(`[Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree('${registryRoot}', $false)`)
  }
})

describe.skipIf(process.platform !== 'win32')(
  'temporary Windows Google OAuth protocol lease',
  () => {
    it('restores the previous handler, value types, and unrelated registry data', async () => {
      run(`
      $root = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('${registryRoot}')
      $root.SetValue('', '%USERPROFILE%\\Original', [Microsoft.Win32.RegistryValueKind]::ExpandString)
      $root.SetValue('URL Protocol', 'original-marker')
      $root.SetValue('Unrelated', 'keep-me'); $root.Dispose()
      $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('${registryRoot}\\shell\\open\\command')
      $key.SetValue('', 'original-handler'); $key.Dispose()
    `)
      const cleanup = await registerWindowsOAuthProtocol(
        'com.raycast:/oauth',
        'http://127.0.0.1:43210/oauth/callback'
      )
      const script = readFileSync(`${bridgeDirectory}/bridge.ps1`, 'utf8')
      expect(script).toContain('http://127.0.0.1:43210/oauth/callback?url=')
      expect(script).not.toContain('tezbar-oauth.port')
      await cleanup()
      const result = JSON.parse(
        run(`
      $root = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('${registryRoot}')
      $key = $root.OpenSubKey('shell\\open\\command')
      @{ Default = $root.GetValue('', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames);
         Kind = [string]$root.GetValueKind(''); Marker = $root.GetValue('URL Protocol');
         Unrelated = $root.GetValue('Unrelated'); Command = $key.GetValue('') } | ConvertTo-Json -Compress
      $key.Dispose(); $root.Dispose()
    `)
      )
      expect(result).toEqual({
        Default: '%USERPROFILE%\\Original',
        Kind: 'ExpandString',
        Marker: 'original-marker',
        Unrelated: 'keep-me',
        Command: 'original-handler',
      })
      expect(existsSync(bridgeDirectory)).toBe(false)
      await cleanup()
      expect(invoke).toHaveBeenCalledTimes(2)
    })

    it('removes newly created keys and temporary files after authorization', async () => {
      const cleanup = await registerWindowsOAuthProtocol(
        'com.raycast:/oauth',
        'http://127.0.0.1:43211/oauth/callback'
      )
      await cleanup()
      expect(
        run(
          `$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('${registryRoot}'); if ($key) { $key.Dispose(); 'exists' } else { 'absent' }`
        )
      ).toBe('absent')
      expect(existsSync(bridgeDirectory)).toBe(false)
    })

    it('preserves a handler changed by another application during authorization', async () => {
      const cleanup = await registerWindowsOAuthProtocol(
        'com.raycast:/oauth',
        'http://127.0.0.1:43212/oauth/callback'
      )
      run(
        `$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('${registryRoot}\\shell\\open\\command', $true); $key.SetValue('', 'new-handler'); $key.Dispose()`
      )
      await cleanup()
      expect(
        run(
          `$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('${registryRoot}\\shell\\open\\command'); $key.GetValue(''); $key.Dispose()`
        )
      ).toBe('new-handler')
      expect(existsSync(bridgeDirectory)).toBe(false)
    })

    it('does not overwrite an active lease and releases it after browser failure', async () => {
      const cleanup = await registerWindowsOAuthProtocol(
        'com.raycast:/oauth',
        'http://127.0.0.1:43213/oauth/callback'
      )
      const second = await registerWindowsOAuthProtocol(
        'com.raycast:/oauth',
        'http://127.0.0.1:43214/oauth/callback'
      )
      expect(invoke).toHaveBeenCalledOnce()
      await second()
      expect(invoke).toHaveBeenCalledOnce()
      await cleanup()
      invoke.mockRejectedValueOnce(new Error('Registration failed'))
      await expect(
        registerWindowsOAuthProtocol('com.raycast:/oauth', 'http://127.0.0.1:43215/oauth/callback')
      ).rejects.toThrow('Registration failed')
      const next = await registerWindowsOAuthProtocol(
        'com.raycast:/oauth',
        'http://127.0.0.1:43216/oauth/callback'
      )
      await next()
    })
  }
)
