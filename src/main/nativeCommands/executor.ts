import { execFile, spawn } from 'node:child_process'
import { promisify } from 'node:util'
import type { NativeCommandId, NativeCommandResult } from '../../shared/nativeCommands'
import { getNativeCommand } from './registry'

const execFileAsync = promisify(execFile)

async function runAppleScript(source: string): Promise<string> {
  const { stdout } = await execFileAsync('osascript', ['-e', source])
  return stdout.trim()
}

async function runShell(script: string): Promise<string> {
  const { stdout } = await execFileAsync('bash', ['-lc', script])
  return stdout.trim()
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

export async function executeNativeCommand(id: NativeCommandId): Promise<NativeCommandResult> {
  const descriptor = getNativeCommand(id)
  if (!descriptor) {
    return { ok: false, message: `Unknown command: ${id}` }
  }
  if (descriptor.macOnly && process.platform !== 'darwin') {
    return { ok: false, message: `${descriptor.title} is only available on macOS.` }
  }

  try {
    switch (id) {
      case 'toggle-dark-mode': {
        const script =
          'tell application "System Events" to tell appearance preferences to set dark mode to not dark mode'
        await runAppleScript(script)
        return { ok: true, message: 'Toggled Dark Mode' }
      }

      case 'toggle-mute': {
        await runAppleScript('set volume output muted (not (output muted of (get volume settings)))')
        return { ok: true, message: 'Toggled system mute' }
      }

      case 'toggle-hide-desktop-icons': {
        const script = `current=$(defaults read com.apple.finder CreateDesktop 2>/dev/null || echo true); if [ "$current" = "false" ]; then defaults write com.apple.finder CreateDesktop true; else defaults write com.apple.finder CreateDesktop false; fi; killall Finder`
        await runShell(script)
        return { ok: true, message: 'Toggled desktop icons' }
      }

      case 'toggle-autohide-dock': {
        const script = `current=$(defaults read com.apple.dock autohide 2>/dev/null || echo 0); if [ "$current" = "1" ]; then defaults write com.apple.dock autohide -bool false; else defaults write com.apple.dock autohide -bool true; fi; killall Dock`
        await runShell(script)
        return { ok: true, message: 'Toggled Dock auto-hide' }
      }

      case 'toggle-autohide-menu-bar': {
        const script = `current=$(defaults read NSGlobalDomain _HIHideMenuBar 2>/dev/null || echo 0); if [ "$current" = "1" ]; then defaults write NSGlobalDomain _HIHideMenuBar -bool false; else defaults write NSGlobalDomain _HIHideMenuBar -bool true; fi; killall SystemUIServer`
        await runShell(script)
        return { ok: true, message: 'Toggled menu bar auto-hide' }
      }

      case 'start-keep-awake': {
        startBackground('caffeinate', 'caffeinate', ['-di'])
        return { ok: true, message: 'Keep Awake is on — system will not sleep.' }
      }

      case 'stop-keep-awake': {
        const stopped = stopBackground('caffeinate')
        return {
          ok: true,
          message: stopped ? 'Keep Awake turned off.' : 'Keep Awake was not running.',
        }
      }

      case 'start-screen-saver': {
        await runShell('open -a ScreenSaverEngine')
        return { ok: true, message: 'Started screen saver' }
      }

      case 'toggle-bluetooth': {
        try {
          const current = await runShell('blueutil -p')
          const next = current === '1' ? '0' : '1'
          await runShell(`blueutil -p ${next}`)
          return { ok: true, message: `Bluetooth ${next === '1' ? 'enabled' : 'disabled'}` }
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
          'tell application "System Events" to keystroke "q" using {command down, control down}',
        )
        return { ok: true, message: 'Screen locked' }
      }

      case 'sleep-display': {
        await runShell('pmset displaysleepnow')
        return { ok: true, message: 'Display sleeping' }
      }

      case 'volume-up': {
        await runAppleScript('set volume output volume (output volume of (get volume settings) + 10)')
        return { ok: true, message: 'Volume up' }
      }

      case 'volume-down': {
        await runAppleScript('set volume output volume (output volume of (get volume settings) - 10)')
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
        const script = `iface=$(networksetup -listallhardwareports | awk '/Wi-Fi/{getline; print $2; exit}'); if [ -z "$iface" ]; then exit 1; fi; state=$(networksetup -getairportpower "$iface" | awk '{print $NF}'); if [ "$state" = "On" ]; then networksetup -setairportpower "$iface" off; echo off; else networksetup -setairportpower "$iface" on; echo on; fi`
        const out = await runShell(script)
        return { ok: true, message: `Wi-Fi ${out || 'toggled'}` }
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
          'end try',
        )
        if (!path) {
          return { ok: false, message: 'No Finder window is open.' }
        }
        return { ok: true, message: `Copied: ${path}` }
      }

      case 'show-macos-version': {
        const out = await runShell('sw_vers && uname -v')
        return { ok: true, message: out }
      }

      case 'show-cpu-info': {
        const out = await runShell(
          "sysctl -n machdep.cpu.brand_string 2>/dev/null; echo \"Cores: $(sysctl -n hw.ncpu)\"; uptime | awk -F'load averages:' '{print \"Load:\"$2}'",
        )
        return { ok: true, message: out }
      }

      case 'show-memory-info': {
        const out = await runShell(
          "memory_pressure | head -n 6; echo; vm_stat | awk 'NR<=6'",
        )
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
            'Use Port Manager → Open Ports in TezBar for a structured, filterable list. (Raw lsof output is intentionally not shown here.)',
        }
      }

      case 'git-root': {
        const path = await runAppleScript(
          'tell application "Finder" to try\n' +
          'set thePath to POSIX path of (target of front Finder window as alias)\n' +
          'return thePath\n' +
          'on error\n' +
          'return ""\n' +
          'end try',
        )
        if (!path) {
          return { ok: false, message: 'No Finder window is open.' }
        }
        try {
          const root = await runShell(`cd ${JSON.stringify(path)} && git rev-parse --show-toplevel`)
          await runShell(`printf %s ${JSON.stringify(root)} | pbcopy`)
          return { ok: true, message: `Copied repo root: ${root}` }
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

      case 'quit-tezbar': {
        return {
          ok: false,
          message: 'Quit TezBar is handled by the launcher so it can show the confirmation dialog.',
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
        return { ok: false, message: `Command ${descriptor.title} is registered but has no executor yet.` }
      }
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    return { ok: false, message: `${descriptor.title} failed: ${message}` }
  }
}
