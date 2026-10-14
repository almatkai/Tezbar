import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { runPowerShellScript } from './powershell-script'

const activeSchemes = new Set<string>()
const quotePowerShell = (value: string): string => `'${value.replace(/'/g, "''")}'`

// Each lease owns its callback address and script. No shared port file can route
// simultaneous authorizations into the wrong extension.
export async function registerWindowsOAuthProtocol(
  redirectUri: string,
  callbackUri: string
): Promise<() => Promise<void>> {
  const scheme = new URL(redirectUri).protocol.slice(0, -1)
  if (process.platform !== 'win32' || activeSchemes.has(scheme)) return async () => {}
  if (!/^[a-z][a-z0-9+.-]*$/i.test(scheme) || ['http', 'https'].includes(scheme)) {
    throw new Error('Invalid OAuth protocol scheme')
  }
  const directory = mkdtempSync(join(tmpdir(), 'tezbar-oauth-'))
  activeSchemes.add(scheme)
  const scriptPath = join(directory, 'bridge.ps1')
  const command = `powershell.exe -WindowStyle Hidden -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "${scriptPath}" -Mode Forward -Url "%1"`
  const script = `param([string]$Mode, [string]$Url)
$ErrorActionPreference = 'Stop'
$base = ${quotePowerShell(`Software\\Classes\\${scheme}`)}
$command = ${quotePowerShell(command)}
$snapshotPath = Join-Path $PSScriptRoot 'registry.clixml'
$registry = [Microsoft.Win32.Registry]::CurrentUser
$paths = @($base, "$base\\shell", "$base\\shell\\open", "$base\\shell\\open\\command")
$fields = @(
  @{ Path = $base; Name = ''; Value = 'URL:${scheme} Protocol' },
  @{ Path = $base; Name = 'URL Protocol'; Value = '' },
  @{ Path = "$base\\shell\\open\\command"; Name = ''; Value = $command }
)
function Restore-Lease {
  if (-not (Test-Path -LiteralPath $snapshotPath)) { return }
  $snapshot = Import-Clixml -LiteralPath $snapshotPath
  $key = $registry.OpenSubKey("$base\\shell\\open\\command")
  try { $owned = $key -and $key.GetValue('') -eq $command } finally { if ($key) { $key.Dispose() } }
  if ($owned) {
    foreach ($field in $snapshot.Fields) {
      $key = $registry.OpenSubKey($field.Path, $true)
      try {
        if ($key -and $key.GetValue($field.Name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) -eq $field.OwnedValue) {
          if ($field.Existed) { $key.SetValue($field.Name, $field.Value, [Microsoft.Win32.RegistryValueKind]$field.Kind) }
          else { $key.DeleteValue($field.Name, $false) }
        }
      } finally { if ($key) { $key.Dispose() } }
    }
  }
  # Delete only empty keys created by this lease, preserving unrelated values.
  foreach ($path in @($snapshot.CreatedKeys | Sort-Object { $_.Length } -Descending)) {
    $key = $registry.OpenSubKey($path)
    try { $empty = $key -and $key.ValueCount -eq 0 -and $key.SubKeyCount -eq 0 } finally { if ($key) { $key.Dispose() } }
    if ($empty) { $registry.DeleteSubKey($path, $false) }
  }
}
if ($Mode -eq 'Forward') {
  if (-not $Url) { exit }
  $encoded = [System.Uri]::EscapeDataString($Url)
  Invoke-RestMethod -Uri (${quotePowerShell(`${callbackUri}?url=`)} + $encoded) -TimeoutSec 5 -UseBasicParsing | Out-Null
} elseif ($Mode -eq 'Restore') {
  Restore-Lease
} elseif ($Mode -eq 'Register') {
  $created = @()
  foreach ($path in $paths) {
    $key = $registry.OpenSubKey($path)
    if ($key) { $key.Dispose() } else { $created += $path }
  }
  $saved = @()
  foreach ($field in $fields) {
    $key = $registry.OpenSubKey($field.Path)
    try {
      $exists = $key -and $key.GetValueNames() -contains $field.Name
      $saved += @{ Path = $field.Path; Name = $field.Name; Existed = [bool]$exists; OwnedValue = $field.Value;
        Value = $(if ($exists) { $key.GetValue($field.Name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) } else { $null });
        Kind = $(if ($exists) { [int]$key.GetValueKind($field.Name) } else { 1 }) }
    } finally { if ($key) { $key.Dispose() } }
  }
  @{ Fields = $saved; CreatedKeys = $created } | Export-Clixml -LiteralPath $snapshotPath
  try {
    # Install our command first so a partial failure can restore the lease.
    foreach ($field in @($fields[2], $fields[0], $fields[1])) {
      $key = $registry.CreateSubKey($field.Path)
      try { $key.SetValue($field.Name, $field.Value, [Microsoft.Win32.RegistryValueKind]::String) } finally { $key.Dispose() }
    }
  } catch { Restore-Lease; throw }
} else { throw 'Unknown OAuth bridge mode' }
`
  let cleaned = false
  const cleanup = async (): Promise<void> => {
    if (cleaned) return
    cleaned = true
    try {
      await runPowerShellScript(`& ${quotePowerShell(scriptPath)} -Mode Restore`)
      rmSync(directory, { recursive: true, force: true })
    } finally {
      activeSchemes.delete(scheme)
    }
  }
  try {
    // Windows PowerShell 5 reads UTF-8 script files correctly only with a BOM.
    writeFileSync(scriptPath, `\uFEFF${script}`, 'utf8')
    await runPowerShellScript(`& ${quotePowerShell(scriptPath)} -Mode Register`)
    return cleanup
  } catch (error) {
    try {
      await cleanup()
    } catch (cleanupError) {
      console.warn('[OAuth] Could not restore protocol lease:', cleanupError)
    }
    throw error
  }
}
