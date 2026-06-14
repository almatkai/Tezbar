/** Typed registry of built-in native macOS commands the palette ships with.
 *  This is a local replacement for the fragile README-scraping pipeline we
 *  had for mac-cli — every command maps to an explicit implementation
 *  strategy so we can reason about safety, output, and platform support. */

export type NativeCommandCategory =
  | 'system'
  | 'display'
  | 'audio'
  | 'network'
  | 'power'
  | 'desktop'
  | 'input'
  | 'dev'
  | 'files'
  | 'productivity'

export type NativeCommandStrategy =
  | 'applescript'
  | 'shell'
  | 'native-helper'

export type NativeCommandId =
  // display / appearance
  | 'toggle-dark-mode'
  | 'start-screen-saver'
  | 'sleep-display'
  // audio
  | 'toggle-mute'
  | 'volume-up'
  | 'volume-down'
  // desktop chrome
  | 'toggle-hide-desktop-icons'
  | 'toggle-autohide-dock'
  | 'toggle-autohide-menu-bar'
  | 'restart-dock'
  | 'restart-finder'
  | 'restart-menu-bar'
  // power
  | 'start-keep-awake'
  | 'stop-keep-awake'
  | 'sleep-system'
  // network
  | 'toggle-bluetooth'
  | 'toggle-wifi'
  | 'show-network-info'
  | 'show-public-ip'
  | 'flush-dns-cache'
  | 'toggle-vpn-menu'
  // files / system
  | 'empty-trash'
  | 'lock-screen'
  | 'open-downloads'
  | 'open-applications'
  | 'reveal-library'
  | 'copy-current-path'
  | 'quit-tezbar'
  // dev helpers
  | 'show-macos-version'
  | 'show-cpu-info'
  | 'show-memory-info'
  | 'show-disk-usage'
  | 'show-battery-status'
  | 'list-listening-ports'
  | 'git-root'
  | 'brew-outdated'
  | 'brew-update'
  // productivity
  | 'open-clipboard-history'
  | 'open-snippets'
  | 'open-quick-notes'
  | 'open-emoji-picker'

export type NativeCommandDescriptor = {
  id: NativeCommandId
  title: string
  subtitle: string
  category: NativeCommandCategory
  strategy: NativeCommandStrategy
  /** Lowercase search keywords for ranking and slash completion. */
  keywords: string[]
  /** If true the command goes through the safety layer before running. */
  destructive?: boolean
  /** Optional: a paired command that restores the state (for toggles that
   *  aren't self-inverse). */
  restoreId?: NativeCommandId
  /** If true the command only applies on macOS. */
  macOnly?: boolean
}

export type NativeCommandResult = {
  ok: boolean
  message: string
}
