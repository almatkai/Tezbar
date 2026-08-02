import { execFile, spawn } from 'node:child_process'
import { randomInt } from 'node:crypto'
import { clipboard } from '@tezbar/desktop-runtime'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { promisify } from 'node:util'
import type { NativeCommandId, NativeCommandResult } from '../../shared/nativeCommands'
import { getNativeCommand, getNativeCommandResultKind } from './registry'

const execFileAsync = promisify(execFile)

async function runAppleScript(source: string): Promise<string> {
  const { stdout } = await execFileAsync('osascript', ['-e', source])
  return stdout.trim()
}

async function runShell(script: string): Promise<string> {
  const { stdout } = await execFileAsync('bash', ['-lc', script])
  return stdout.trim()
}

/** Execute a built-in PowerShell command without loading the user's profile.
 * Commands passed here are constants in this file, never palette input. */
async function runPowerShell(script: string): Promise<string> {
  const { stdout } = await execFileAsync('powershell.exe', [
    '-NoLogo',
    '-NoProfile',
    '-NonInteractive',
    '-ExecutionPolicy',
    'Bypass',
    '-Command',
    script,
  ], { windowsHide: true })
  return stdout.trim()
}

/** Network-adapter state is a privileged Windows operation. Use the UAC
 * elevation flow rather than failing silently for a normal desktop user. */
async function runElevatedPowerShell(script: string): Promise<void> {
  const encoded = Buffer.from(`$ErrorActionPreference = 'Stop'; ${script}`, 'utf16le').toString(
    'base64'
  )
  const launcher = `$process = Start-Process -FilePath 'powershell.exe' -Verb RunAs -Wait -PassThru -ArgumentList @('-NoLogo','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-EncodedCommand','${encoded}'); if ($process.ExitCode -ne 0) { exit $process.ExitCode }`
  await execFileAsync('powershell.exe', [
    '-NoLogo',
    '-NoProfile',
    '-NonInteractive',
    '-ExecutionPolicy',
    'Bypass',
    '-Command',
    launcher,
  ])
}

/** Read the current Windows dark-mode state from the registry. */
async function readWindowsDarkMode(): Promise<boolean> {
  const out = await runPowerShell(
    "(Get-ItemPropertyValue -Path 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize' -Name AppsUseLightTheme -ErrorAction SilentlyContinue)"
  )
  return out.trim() === '0'
}

/** Read the current Windows Wi-Fi adapter state (true = enabled). */
async function readWindowsWifiState(): Promise<boolean> {
  const out = await runPowerShell(
    "$adapter=Get-NetAdapter -IncludeHidden | Where-Object { $_.HardwareInterface -and ($_.NdisPhysicalMedium -eq 'Native 802.11' -or $_.InterfaceDescription -match 'Wireless|Wi-Fi|802\\.11') } | Select-Object -First 1; if($null -eq $adapter){throw 'No Wi-Fi adapter found.'}; $adapter.AdminStatus"
  )
  return out.trim() === 'Up'
}

async function executeWindowsCommand(id: NativeCommandId): Promise<NativeCommandResult | null> {
  switch (id) {
    case 'toggle-dark-mode': {
      const wasOn = await readWindowsDarkMode()
      await runPowerShell(
        "$p='HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize'; $v=(Get-ItemPropertyValue -Path $p -Name AppsUseLightTheme -ErrorAction SilentlyContinue); $n=if($v -eq 0){1}else{0}; Set-ItemProperty -Path $p -Name AppsUseLightTheme -Value $n; Set-ItemProperty -Path $p -Name SystemUsesLightTheme -Value $n"
      )
      const isOn = await readWindowsDarkMode()
      return { ok: true, message: 'Toggled Windows dark mode', state: { isOn, wasOn } }
    }
    case 'start-screen-saver':
      await execFileAsync('rundll32.exe', ['user32.dll,LockWorkStation'])
      return { ok: true, message: 'Screen locked' }
    case 'sleep-display':
      await runPowerShell(
        'Add-Type -TypeDefinition \'using System; using System.Runtime.InteropServices; public static class TezbarDisplay { [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l); }\'; [TezbarDisplay]::SendMessage([IntPtr]0xffff,0x0112,[IntPtr]0xF170,[IntPtr]2) | Out-Null'
      )
      return { ok: true, message: 'Display sleeping' }
    case 'toggle-mute':
    case 'volume-up':
    case 'volume-down': {
      // These virtual-key codes are handled by Windows' active audio endpoint,
      // so they honor the user's selected device and volume-step preference.
      const virtualKey = id === 'toggle-mute' ? 0xad : id === 'volume-up' ? 0xaf : 0xae
      await runPowerShell(
        `Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class TezbarAudio { [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra); }'; [TezbarAudio]::keybd_event(${virtualKey}, 0, 0, [UIntPtr]::Zero); [TezbarAudio]::keybd_event(${virtualKey}, 0, 2, [UIntPtr]::Zero)`
      )
      return {
        ok: true,
        message:
          id === 'toggle-mute'
            ? 'Toggled system mute'
            : id === 'volume-up'
              ? 'Volume up'
              : 'Volume down',
      }
    }
    case 'start-keep-awake': {
      const wasOn = isBackgroundAlive('keep-awake')
      startBackground('keep-awake', 'powershell.exe', [
        '-NoProfile',
        '-NonInteractive',
        '-Command',
        '$wshell=New-Object -ComObject WScript.Shell; while($true){$wshell.SendKeys("{SCROLLLOCK}"); Start-Sleep -Milliseconds 50; $wshell.SendKeys("{SCROLLLOCK}"); Start-Sleep -Seconds 240}',
      ])
      return { ok: true, message: 'Keep Awake is on.', state: { isOn: true, wasOn } }
    }
    case 'stop-keep-awake': {
      const wasOn = isBackgroundAlive('keep-awake')
      const stopped = stopBackground('keep-awake')
      return {
        ok: true,
        message: stopped ? 'Keep Awake turned off.' : 'Keep Awake was not running.',
        state: { isOn: false, wasOn },
      }
    }
    case 'sleep-system':
      await runPowerShell(
        'Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.Application]::SetSuspendState([System.Windows.Forms.PowerState]::Suspend, $false, $false)'
      )
      return { ok: true, message: 'System sleeping' }
    case 'show-network-info': {
      const out = await runPowerShell(
        "Get-NetIPAddress -AddressFamily IPv4 | Where-Object {$_.IPAddress -notlike '127.*'} | Select-Object -First 3 -ExpandProperty IPAddress"
      )
      return {
        ok: true,
        message: out ? `IP: ${out.replace(/\r?\n/g, ', ')}` : 'No network info available',
      }
    }
    case 'show-public-ip': {
      const out = await runPowerShell(
        "(Invoke-RestMethod -UseBasicParsing -TimeoutSec 5 -Uri 'https://api.ipify.org').Trim()"
      )
      return { ok: true, message: `Public IP: ${out}` }
    }
    case 'flush-dns-cache':
      await execFileAsync('ipconfig.exe', ['/flushdns'])
      return { ok: true, message: 'Flushed DNS cache' }
    case 'toggle-vpn-menu':
      await execFileAsync('explorer.exe', ['ms-settings:network-vpn'])
      return { ok: true, message: 'Opened VPN settings' }
    case 'toggle-wifi': {
      const wasOn = await readWindowsWifiState()
      await runElevatedPowerShell(
        "$adapter=Get-NetAdapter -IncludeHidden | Where-Object { $_.HardwareInterface -and ($_.NdisPhysicalMedium -eq 'Native 802.11' -or $_.InterfaceDescription -match 'Wireless|Wi-Fi|802\\.11') } | Select-Object -First 1; if($null -eq $adapter){throw 'No Wi-Fi adapter found.'}; if($adapter.AdminStatus -eq 'Up'){Disable-NetAdapter -Name $adapter.Name -Confirm:$false}else{Enable-NetAdapter -Name $adapter.Name -Confirm:$false}"
      )
      const isOn = await readWindowsWifiState()
      return { ok: true, message: `Wi-Fi ${isOn ? 'enabled' : 'disabled'}`, state: { isOn, wasOn } }
    }
    case 'lock-screen':
      await execFileAsync('rundll32.exe', ['user32.dll,LockWorkStation'])
      return { ok: true, message: 'Screen locked' }
    case 'open-downloads':
      await execFileAsync('explorer.exe', [join(homedir(), 'Downloads')])
      return { ok: true, message: 'Opened Downloads' }
    case 'open-applications':
      await execFileAsync('explorer.exe', ['shell:AppsFolder'])
      return { ok: true, message: 'Opened All Apps' }
    case 'reveal-library':
      await execFileAsync('explorer.exe', [
        process.env.APPDATA ?? join(homedir(), 'AppData', 'Roaming'),
      ])
      return { ok: true, message: 'Opened AppData' }
    case 'copy-current-path': {
      const out = await runPowerShell(
        "$window=(New-Object -ComObject Shell.Application).Windows() | Where-Object { $_.FullName -match 'explorer.exe$' -and $_.Document.Folder.Self.Path } | Select-Object -First 1; if($null -eq $window){throw 'No Explorer folder window is open.'}; $path=$window.Document.Folder.Self.Path; Set-Clipboard -Value $path; $path"
      )
      return { ok: true, message: out }
    }
    case 'empty-trash':
      await runPowerShell('Clear-RecycleBin -Force -ErrorAction Stop')
      return { ok: true, message: 'Emptied Recycle Bin' }
    case 'show-macos-version':
      return {
        ok: true,
        message: await runPowerShell(
          "$os=Get-CimInstance Win32_OperatingSystem; '{0} — version {1}, build {2}' -f $os.Caption,$os.Version,$os.BuildNumber"
        ),
      }
    case 'show-cpu-info':
      return {
        ok: true,
        message: await runPowerShell(
          'Get-CimInstance Win32_Processor | ForEach-Object { "$($_.Name) — $($_.NumberOfCores) cores" }'
        ),
      }
    case 'show-memory-info':
      return {
        ok: true,
        message: await runPowerShell(
          "$os=Get-CimInstance Win32_OperatingSystem; 'Free: {0:N1} GB / Total: {1:N1} GB' -f ($os.FreePhysicalMemory/1MB),($os.TotalVisibleMemorySize/1MB)"
        ),
      }
    case 'show-disk-usage':
      return {
        ok: true,
        message: await runPowerShell(
          "Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' | ForEach-Object { '{0} Free: {1:N1} GB / {2:N1} GB' -f $_.DeviceID,($_.FreeSpace/1GB),($_.Size/1GB) }"
        ),
      }
    case 'show-battery-status': {
      const out = await runPowerShell(
        "Get-CimInstance Win32_Battery -ErrorAction SilentlyContinue | ForEach-Object { 'Battery: {0}%' -f $_.EstimatedChargeRemaining }"
      )
      return { ok: true, message: out || 'No battery detected' }
    }
    case 'list-listening-ports':
      return {
        ok: true,
        message: 'Use Port Manager → Open Ports in Tezbar for a structured, filterable list.',
      }
    case 'git-root': {
      const path = await runPowerShell(
        "$window=(New-Object -ComObject Shell.Application).Windows() | Where-Object { $_.FullName -match 'explorer.exe$' -and $_.Document.Folder.Self.Path } | Select-Object -First 1; if($null -eq $window){throw 'No Explorer folder window is open.'}; $window.Document.Folder.Self.Path"
      )
      const { stdout } = await execFileAsync('git.exe', ['rev-parse', '--show-toplevel'], {
        cwd: path,
      })
      const root = stdout.trim()
      clipboard.writeText(root)
      return { ok: true, message: root }
    }
    default:
      return null
  }
}

/** Long-lived background processes the executor can start/stop (e.g. a
 *  `caffeinate` PID for Keep Awake). Kept in memory only — if the main
 *  process dies, the OS cleans them up with us. */
const backgroundProcesses = new Map<string, number>()

function startBackground(key: string, command: string, args: string[]): void {
  const existing = backgroundProcesses.get(key)
  if (existing && isProcessAlive(existing)) return
  const child = spawn(command, args, { detached: true, stdio: 'ignore' })
  child.unref()
  if (child.pid) backgroundProcesses.set(key, child.pid)
}

function isBackgroundAlive(key: string): boolean {
  const pid = backgroundProcesses.get(key)
  return pid !== undefined && isProcessAlive(pid)
}

function stopBackground(key: string): boolean {
  const pid = backgroundProcesses.get(key)
  if (!pid) return false
  try {
    process.kill(pid, 'SIGTERM')
    backgroundProcesses.delete(key)
    return true
  } catch {
    backgroundProcesses.delete(key)
    return false
  }
}

function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0)
    return true
  } catch {
    return false
  }
}

/** Build a 20-char password from four char classes using a CSPRNG
 *  (`crypto.randomInt`), guaranteeing at least one char per class and
 *  avoiding lookalikes (l, 1, I, O, 0) that cause transcription errors
 *  when reading from a toast. Works identically on macOS and Windows. */
function generatePassword(): string {
  const classes = [
    'ABCDEFGHJKLMNPQRSTUVWXYZ',
    'abcdefghijkmnopqrstuvwxyz',
    '23456789',
    '!@#$%^&*()-_=+[]{}',
  ]
  const all = classes.join('')
  const chars = [
    ...classes.map((cls) => cls[randomInt(cls.length)]),
    ...Array.from({ length: 16 }, () => all[randomInt(all.length)]),
  ]
  // Fisher–Yates so the guaranteed class chars aren't stuck at the front.
  for (let i = chars.length - 1; i > 0; i -= 1) {
    const j = randomInt(i + 1)
    ;[chars[i], chars[j]] = [chars[j], chars[i]]
  }
  return chars.join('')
}

/** Read a macOS boolean preference via `defaults read`. Returns the fallback
 *  when the key doesn't exist (e.g. fresh install). */
async function readMacOSBoolPref(domain: string, key: string, fallback: boolean): Promise<boolean> {
  try {
    const out = await runShell(`defaults read ${domain} ${key} 2>/dev/null || echo ${fallback ? '1' : '0'}`)
    return out.trim() === '1' || out.trim().toLowerCase() === 'true'
  } catch {
    return fallback
  }
}

/** Read the current macOS dark-mode state. The key exists (== 'Dark') only
 *  when dark appearance is active. */
async function readMacOSDarkMode(): Promise<boolean> {
  try {
    const out = await runShell('defaults read NSGlobalDomain AppleInterfaceStyle 2>/dev/null || echo Light')
    return out.trim() === 'Dark'
  } catch {
    return false
  }
}

/** Read desktop-icons visibility (CreateDesktop). */
async function readMacOSDesktopIcons(): Promise<boolean> {
  return readMacOSBoolPref('com.apple.finder', 'CreateDesktop', true)
}

/** Read Dock auto-hide state. */
async function readMacOSAutohideDock(): Promise<boolean> {
  return readMacOSBoolPref('com.apple.dock', 'autohide', false)
}

/** Read menu-bar auto-hide state. */
async function readMacOSAutohideMenuBar(): Promise<boolean> {
  return readMacOSBoolPref('NSGlobalDomain', '_HIHideMenuBar', false)
}

/** Read Bluetooth power state via blueutil. */
async function readMacOSBluetooth(): Promise<boolean> {
  const out = await runShell('blueutil -p')
  return out.trim() === '1'
}

/** Read Wi-Fi power state on the default interface. */
async function readMacOSWifi(): Promise<boolean> {
  const out = await runShell(
    `iface=$(networksetup -listallhardwareports | awk '/Wi-Fi/{getline; print $2; exit}'); if [ -z "$iface" ]; then echo off; else networksetup -getairportpower "$iface" | awk '{print $NF}'; fi`
  )
  return out.trim().toLowerCase() === 'on'
}

export async function executeNativeCommandRaw(
  id: NativeCommandId
): Promise<NativeCommandResult> {
  const descriptor = getNativeCommand(id)
  if (!descriptor) {
    return { ok: false, message: `Unknown command: ${id}` }
  }
  if (descriptor.macOnly && process.platform !== 'darwin') {
    return { ok: false, message: `${descriptor.title} is only available on macOS.` }
  }

  if (process.platform === 'win32') {
    try {
      const result = await executeWindowsCommand(id)
      if (result) return result
    } catch (error) {
      return { ok: false, message: error instanceof Error ? error.message : String(error) }
    }
  }

  try {
    switch (id) {
      case 'toggle-dark-mode': {
        const wasOn = await readMacOSDarkMode()
        const script =
          'tell application "System Events" to tell appearance preferences to set dark mode to not dark mode'
        await runAppleScript(script)
        const isOn = await readMacOSDarkMode()
        return { ok: true, message: 'Toggled Dark Mode', state: { isOn, wasOn } }
      }

      case 'toggle-mute': {
        await runAppleScript(
          'set volume output muted (not (output muted of (get volume settings)))'
        )
        return { ok: true, message: 'Toggled system mute' }
      }

      case 'toggle-hide-desktop-icons': {
        // CreateDesktop=true means icons visible; the toggle's "on" state means hidden.
        const wasOn = !(await readMacOSDesktopIcons())
        const script = `current=$(defaults read com.apple.finder CreateDesktop 2>/dev/null || echo true); if [ "$current" = "false" ]; then defaults write com.apple.finder CreateDesktop true; else defaults write com.apple.finder CreateDesktop false; fi; killall Finder`
        await runShell(script)
        const isOn = !(await readMacOSDesktopIcons())
        return { ok: true, message: 'Toggled desktop icons', state: { isOn, wasOn } }
      }

      case 'toggle-autohide-dock': {
        const wasOn = await readMacOSAutohideDock()
        const script = `current=$(defaults read com.apple.dock autohide 2>/dev/null || echo 0); if [ "$current" = "1" ]; then defaults write com.apple.dock autohide -bool false; else defaults write com.apple.dock autohide -bool true; fi; killall Dock`
        await runShell(script)
        const isOn = await readMacOSAutohideDock()
        return { ok: true, message: 'Toggled Dock auto-hide', state: { isOn, wasOn } }
      }

      case 'toggle-autohide-menu-bar': {
        const wasOn = await readMacOSAutohideMenuBar()
        const script = `current=$(defaults read NSGlobalDomain _HIHideMenuBar 2>/dev/null || echo 0); if [ "$current" = "1" ]; then defaults write NSGlobalDomain _HIHideMenuBar -bool false; else defaults write NSGlobalDomain _HIHideMenuBar -bool true; fi; killall SystemUIServer`
        await runShell(script)
        const isOn = await readMacOSAutohideMenuBar()
        return { ok: true, message: 'Toggled menu bar auto-hide', state: { isOn, wasOn } }
      }

      case 'start-keep-awake': {
        const wasOn = isBackgroundAlive('caffeinate')
        startBackground('caffeinate', 'caffeinate', ['-di'])
        return { ok: true, message: 'Keep Awake is on — system will not sleep.', state: { isOn: true, wasOn } }
      }

      case 'stop-keep-awake': {
        const wasOn = isBackgroundAlive('caffeinate')
        const stopped = stopBackground('caffeinate')
        return {
          ok: true,
          message: stopped ? 'Keep Awake turned off.' : 'Keep Awake was not running.',
          state: { isOn: false, wasOn },
        }
      }

      case 'start-screen-saver': {
        await runShell('open -a ScreenSaverEngine')
        return { ok: true, message: 'Started screen saver' }
      }

      case 'toggle-bluetooth': {
        try {
          const wasOn = await readMacOSBluetooth()
          const next = wasOn ? '0' : '1'
          await runShell(`blueutil -p ${next}`)
          return { ok: true, message: `Bluetooth ${next === '1' ? 'enabled' : 'disabled'}`, state: { isOn: next === '1', wasOn } }
        } catch {
          return {
            ok: false,
            message: 'Bluetooth control requires `blueutil`. Install with `brew install blueutil`.',
          }
        }
      }

      case 'show-network-info': {
        const script = `echo "IP: $(ipconfig getifaddr en0 2>/dev/null || echo n/a)"; echo "Wi-Fi: $(networksetup -getairportnetwork en0 2>/dev/null | sed 's/Current Wi-Fi Network: //')"`
        const out = await runShell(script)
        return { ok: true, message: out || 'No network info available' }
      }

      case 'flush-dns-cache': {
        try {
          await runShell('sudo -n dscacheutil -flushcache && sudo -n killall -HUP mDNSResponder')
          return { ok: true, message: 'Flushed DNS cache' }
        } catch {
          return {
            ok: false,
            message: 'DNS flush requires `sudo`. Run `sudo dscacheutil -flushcache` in Terminal.',
          }
        }
      }

      case 'empty-trash': {
        await runAppleScript('tell application "Finder" to empty the trash')
        return { ok: true, message: 'Emptied Trash' }
      }

      case 'lock-screen': {
        // ⌃⌘Q is the canonical macOS "Lock Screen" shortcut. Using it means
        // we pick up the user's own lock-screen settings (login window,
        // password delay, etc.) instead of just sleeping the display.
        await runAppleScript(
          'tell application "System Events" to keystroke "q" using {command down, control down}'
        )
        return { ok: true, message: 'Screen locked' }
      }

      case 'sleep-display': {
        await runShell('pmset displaysleepnow')
        return { ok: true, message: 'Display sleeping' }
      }

      case 'volume-up': {
        await runAppleScript(
          'set volume output volume (output volume of (get volume settings) + 10)'
        )
        return { ok: true, message: 'Volume up' }
      }

      case 'volume-down': {
        await runAppleScript(
          'set volume output volume (output volume of (get volume settings) - 10)'
        )
        return { ok: true, message: 'Volume down' }
      }

      case 'restart-dock': {
        await runShell('killall Dock')
        return { ok: true, message: 'Dock relaunched' }
      }

      case 'restart-finder': {
        await runShell('killall Finder')
        return { ok: true, message: 'Finder relaunched' }
      }

      case 'restart-menu-bar': {
        await runShell('killall SystemUIServer')
        return { ok: true, message: 'Menu bar relaunched' }
      }

      case 'sleep-system': {
        await runAppleScript('tell application "System Events" to sleep')
        return { ok: true, message: 'System sleeping' }
      }

      case 'toggle-wifi': {
        const wasOn = await readMacOSWifi()
        const script = `iface=$(networksetup -listallhardwareports | awk '/Wi-Fi/{getline; print $2; exit}'); if [ -z "$iface" ]; then exit 1; fi; state=$(networksetup -getairportpower "$iface" | awk '{print $NF}'); if [ "$state" = "On" ]; then networksetup -setairportpower "$iface" off; echo off; else networksetup -setairportpower "$iface" on; echo on; fi`
        const out = await runShell(script)
        const isOn = out.trim().toLowerCase() === 'on'
        return { ok: true, message: `Wi-Fi ${out || 'toggled'}`, state: { isOn, wasOn } }
      }

      case 'show-public-ip': {
        // curl with a short timeout — some networks block ifconfig.me.
        const out = await runShell('curl -m 4 -fsS https://api.ipify.org || echo "(unreachable)"')
        return { ok: true, message: `Public IP: ${out}` }
      }

      case 'toggle-vpn-menu': {
        await runShell('open "x-apple.systempreferences:com.apple.preference.network"')
        return { ok: true, message: 'Opened Network preferences' }
      }

      case 'open-downloads': {
        await runShell('open ~/Downloads')
        return { ok: true, message: 'Opened Downloads' }
      }

      case 'open-applications': {
        await runShell('open /Applications')
        return { ok: true, message: 'Opened Applications' }
      }

      case 'reveal-library': {
        await runShell('open ~/Library')
        return { ok: true, message: 'Opened ~/Library' }
      }

      case 'copy-current-path': {
        const path = await runAppleScript(
          'tell application "Finder" to try\n' +
            'set thePath to POSIX path of (target of front Finder window as alias)\n' +
            'set the clipboard to thePath\n' +
            'return thePath\n' +
            'on error\n' +
            'return ""\n' +
            'end try'
        )
        if (!path) {
          return { ok: false, message: 'No Finder window is open.' }
        }
        return { ok: true, message: path }
      }

      case 'show-macos-version': {
        const out = await runShell('sw_vers && uname -v')
        return { ok: true, message: out }
      }

      case 'show-cpu-info': {
        const out = await runShell(
          'sysctl -n machdep.cpu.brand_string 2>/dev/null; echo "Cores: $(sysctl -n hw.ncpu)"; uptime | awk -F\'load averages:\' \'{print "Load:"$2}\''
        )
        return { ok: true, message: out }
      }

      case 'show-system-monitor': {
        return {
          ok: true,
          message: 'Open System Monitor from the launcher to view live hardware information.',
        }
      }

      case 'show-memory-info': {
        const out = await runShell("memory_pressure | head -n 6; echo; vm_stat | awk 'NR<=6'")
        return { ok: true, message: out }
      }

      case 'show-disk-usage': {
        const out = await runShell('df -h / | tail -n 1')
        return { ok: true, message: out }
      }

      case 'show-battery-status': {
        const out = await runShell('pmset -g batt | tail -n +2')
        return { ok: true, message: out || 'No battery detected' }
      }

      case 'list-listening-ports': {
        // Primary UX is the Port Manager surface in the renderer; this path
        // only runs if something invokes the native command without going
        // through the launcher interception.
        return {
          ok: true,
          message:
            'Use Port Manager → Open Ports in Tezbar for a structured, filterable list. (Raw lsof output is intentionally not shown here.)',
        }
      }

      case 'git-root': {
        const path = await runAppleScript(
          'tell application "Finder" to try\n' +
            'set thePath to POSIX path of (target of front Finder window as alias)\n' +
            'return thePath\n' +
            'on error\n' +
            'return ""\n' +
            'end try'
        )
        if (!path) {
          return { ok: false, message: 'No Finder window is open.' }
        }
        try {
          const root = await runShell(`cd ${JSON.stringify(path)} && git rev-parse --show-toplevel`)
          await runShell(`printf %s ${JSON.stringify(root)} | pbcopy`)
          return { ok: true, message: root }
        } catch {
          return { ok: false, message: `${path} is not inside a git repo.` }
        }
      }

      case 'brew-outdated': {
        try {
          const out = await runShell('brew outdated --quiet')
          return {
            ok: true,
            message: out.trim().length === 0 ? 'All Homebrew formulae are up to date.' : out,
          }
        } catch {
          return { ok: false, message: 'Homebrew is not installed or not in PATH.' }
        }
      }

      case 'open-clipboard-history': {
        // This command is handled in the renderer (it navigates to a
        // dedicated surface). If execution ever lands here it means the
        // interception path was skipped — surface a clear error instead
        // of silently succeeding.
        return {
          ok: false,
          message: 'Clipboard History is a UI navigation — open the launcher to browse it.',
        }
      }

      case 'open-snippets': {
        return {
          ok: false,
          message: 'Snippets is a UI navigation — open the launcher to browse it.',
        }
      }

      case 'open-quick-notes': {
        return {
          ok: false,
          message: 'Quick Notes is a UI navigation — open the launcher to browse it.',
        }
      }

      case 'open-emoji-picker': {
        return {
          ok: false,
          message: 'Emoji Picker is a UI navigation — open the launcher to browse it.',
        }
      }

      case 'generate-password': {
        // Pure Node implementation (crypto + clipboard), so this works
        // identically on macOS and Windows with no shell or AppleScript.
        const password = generatePassword()
        clipboard.writeText(password)
        return { ok: true, message: password }
      }

      case 'quit-tezbar': {
        return {
          ok: false,
          message: 'Quit Tezbar is handled by the launcher so it can show the confirmation dialog.',
        }
      }

      case 'brew-update': {
        try {
          const out = await runShell('brew update')
          return { ok: true, message: out.slice(-400) || 'Homebrew updated.' }
        } catch {
          return { ok: false, message: 'Homebrew is not installed or not in PATH.' }
        }
      }

      default: {
        return {
          ok: false,
          message: `Command ${descriptor.title} is registered but has no executor yet.`,
        }
      }
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    return { ok: false, message: `${descriptor.title} failed: ${message}` }
  }
}

/** Public entry point. Runs the raw executor, then stamps the result kind
 *  from the registry so the launcher can render a styled card for info /
 *  copied / password / toggle results — failures render as an error line. */
export async function executeNativeCommand(id: NativeCommandId): Promise<NativeCommandResult> {
  const result = await executeNativeCommandRaw(id)
  const kind = result.ok ? getNativeCommandResultKind(id) : 'error'
  return kind ? { ...result, kind } : result
}
