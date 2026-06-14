import type { NativeCommandDescriptor, NativeCommandId } from '../../shared/nativeCommands'

/** Typed catalog of native macOS capabilities the palette ships with.
 *  Replaces the old README-scraping strategy with a first-class registry
 *  where every entry has a deterministic implementation strategy. */
const DESCRIPTORS: Record<NativeCommandId, NativeCommandDescriptor> = {
  'toggle-dark-mode': {
    id: 'toggle-dark-mode',
    title: 'Toggle Dark Mode',
    subtitle: 'Switch between light and dark appearance.',
    category: 'display',
    strategy: 'applescript',
    keywords: ['dark', 'light', 'appearance', 'theme', 'mode'],
    macOnly: true,
  },
  'start-screen-saver': {
    id: 'start-screen-saver',
    title: 'Start Screen Saver',
    subtitle: 'Launch the screen saver now.',
    category: 'display',
    strategy: 'shell',
    keywords: ['screensaver', 'screen', 'saver', 'lock'],
    macOnly: true,
  },
  'sleep-display': {
    id: 'sleep-display',
    title: 'Sleep Display',
    subtitle: 'Put just the display to sleep.',
    category: 'power',
    strategy: 'shell',
    keywords: ['sleep', 'display', 'screen', 'off'],
    macOnly: true,
  },

  'toggle-mute': {
    id: 'toggle-mute',
    title: 'Toggle Mute',
    subtitle: 'Mute or unmute the system output volume.',
    category: 'audio',
    strategy: 'applescript',
    keywords: ['mute', 'unmute', 'sound', 'audio', 'volume'],
    macOnly: true,
  },
  'volume-up': {
    id: 'volume-up',
    title: 'Volume Up',
    subtitle: 'Raise system output volume by one step.',
    category: 'audio',
    strategy: 'applescript',
    keywords: ['volume', 'louder', 'up'],
    macOnly: true,
  },
  'volume-down': {
    id: 'volume-down',
    title: 'Volume Down',
    subtitle: 'Lower system output volume by one step.',
    category: 'audio',
    strategy: 'applescript',
    keywords: ['volume', 'quieter', 'down'],
    macOnly: true,
  },

  'toggle-hide-desktop-icons': {
    id: 'toggle-hide-desktop-icons',
    title: 'Toggle Hide Desktop Icons',
    subtitle: 'Hide or show files on the Finder desktop.',
    category: 'desktop',
    strategy: 'shell',
    keywords: ['hide', 'desktop', 'icons', 'clean', 'finder'],
    macOnly: true,
  },
  'toggle-autohide-dock': {
    id: 'toggle-autohide-dock',
    title: 'Toggle Autohide Dock',
    subtitle: 'Flip the Dock auto-hide preference.',
    category: 'desktop',
    strategy: 'shell',
    keywords: ['dock', 'autohide', 'hide', 'bar'],
    macOnly: true,
  },
  'toggle-autohide-menu-bar': {
    id: 'toggle-autohide-menu-bar',
    title: 'Toggle Autohide Menu Bar',
    subtitle: 'Flip the macOS menu-bar auto-hide preference.',
    category: 'desktop',
    strategy: 'shell',
    keywords: ['menu', 'bar', 'autohide', 'notch'],
    macOnly: true,
  },
  'restart-dock': {
    id: 'restart-dock',
    title: 'Restart Dock',
    subtitle: 'Relaunch the Dock process.',
    category: 'desktop',
    strategy: 'shell',
    keywords: ['dock', 'restart', 'relaunch'],
    macOnly: true,
  },
  'restart-finder': {
    id: 'restart-finder',
    title: 'Restart Finder',
    subtitle: 'Relaunch the Finder process.',
    category: 'desktop',
    strategy: 'shell',
    keywords: ['finder', 'restart', 'relaunch'],
    macOnly: true,
  },
  'restart-menu-bar': {
    id: 'restart-menu-bar',
    title: 'Restart Menu Bar',
    subtitle: 'Relaunch SystemUIServer (fixes frozen menu bar).',
    category: 'desktop',
    strategy: 'shell',
    keywords: ['menu', 'bar', 'restart', 'systemuiserver'],
    macOnly: true,
  },

  'start-keep-awake': {
    id: 'start-keep-awake',
    title: 'Keep Awake',
    subtitle: 'Prevent system sleep until you stop it.',
    category: 'power',
    strategy: 'shell',
    keywords: ['keep', 'awake', 'caffeinate', 'no', 'sleep'],
    restoreId: 'stop-keep-awake',
    macOnly: true,
  },
  'stop-keep-awake': {
    id: 'stop-keep-awake',
    title: 'Stop Keep Awake',
    subtitle: 'Allow the system to sleep again.',
    category: 'power',
    strategy: 'shell',
    keywords: ['stop', 'awake', 'caffeinate', 'sleep'],
    macOnly: true,
  },
  'sleep-system': {
    id: 'sleep-system',
    title: 'Sleep Mac',
    subtitle: 'Put the Mac to sleep now.',
    category: 'power',
    strategy: 'applescript',
    keywords: ['sleep', 'mac', 'suspend', 'idle'],
    macOnly: true,
  },

  'toggle-bluetooth': {
    id: 'toggle-bluetooth',
    title: 'Toggle Bluetooth',
    subtitle: 'Turn Bluetooth on or off (requires blueutil).',
    category: 'network',
    strategy: 'shell',
    keywords: ['bluetooth', 'bt', 'airpods', 'wireless'],
    macOnly: true,
  },
  'toggle-wifi': {
    id: 'toggle-wifi',
    title: 'Toggle Wi-Fi',
    subtitle: 'Turn Wi-Fi on or off on the default interface.',
    category: 'network',
    strategy: 'shell',
    keywords: ['wifi', 'wireless', 'network', 'toggle'],
    macOnly: true,
  },
  'show-network-info': {
    id: 'show-network-info',
    title: 'Show Network Info',
    subtitle: 'Display current IP addresses and Wi-Fi SSID.',
    category: 'network',
    strategy: 'shell',
    keywords: ['network', 'ip', 'wifi', 'ssid', 'info'],
    macOnly: true,
  },
  'show-public-ip': {
    id: 'show-public-ip',
    title: 'Show Public IP',
    subtitle: 'Look up the public IPv4 address of this connection.',
    category: 'network',
    strategy: 'shell',
    keywords: ['ip', 'public', 'external', 'wan'],
    macOnly: false,
  },
  'flush-dns-cache': {
    id: 'flush-dns-cache',
    title: 'Flush DNS Cache',
    subtitle: 'Clear the macOS resolver and mDNSResponder caches.',
    category: 'network',
    strategy: 'shell',
    keywords: ['dns', 'flush', 'cache', 'network', 'resolver'],
    macOnly: true,
  },
  'toggle-vpn-menu': {
    id: 'toggle-vpn-menu',
    title: 'Open VPN Menu',
    subtitle: 'Open the menu-bar VPN/Network control.',
    category: 'network',
    strategy: 'shell',
    keywords: ['vpn', 'network', 'menu'],
    macOnly: true,
  },

  'empty-trash': {
    id: 'empty-trash',
    title: 'Empty Trash',
    subtitle: 'Permanently delete everything in the Trash.',
    category: 'system',
    strategy: 'applescript',
    keywords: ['trash', 'empty', 'delete', 'clean'],
    destructive: true,
    macOnly: true,
  },
  'lock-screen': {
    id: 'lock-screen',
    title: 'Lock Screen',
    subtitle: 'Lock the current session.',
    category: 'system',
    strategy: 'applescript',
    keywords: ['lock', 'screen', 'session', 'away'],
    macOnly: true,
  },
  'open-downloads': {
    id: 'open-downloads',
    title: 'Open Downloads Folder',
    subtitle: 'Reveal ~/Downloads in Finder.',
    category: 'files',
    strategy: 'shell',
    keywords: ['downloads', 'folder', 'finder'],
    macOnly: true,
  },
  'open-applications': {
    id: 'open-applications',
    title: 'Open Applications Folder',
    subtitle: 'Reveal /Applications in Finder.',
    category: 'files',
    strategy: 'shell',
    keywords: ['applications', 'apps', 'finder'],
    macOnly: true,
  },
  'reveal-library': {
    id: 'reveal-library',
    title: 'Open ~/Library',
    subtitle: 'Reveal the hidden Library folder in Finder.',
    category: 'files',
    strategy: 'shell',
    keywords: ['library', 'hidden', 'finder'],
    macOnly: true,
  },
  'copy-current-path': {
    id: 'copy-current-path',
    title: 'Copy Path of Frontmost Finder Window',
    subtitle: 'Copy the path of the folder open in Finder.',
    category: 'files',
    strategy: 'applescript',
    keywords: ['path', 'finder', 'copy', 'directory'],
    macOnly: true,
  },
  'quit-tezbar': {
    id: 'quit-tezbar',
    title: 'Quit Tezbar',
    subtitle: 'Quit Tezbar and terminate all background processes.',
    category: 'system',
    strategy: 'native-helper',
    keywords: ['quit', 'tezbar', 'exit', 'close', 'shutdown', 'terminate', 'app'],
    macOnly: false,
  },

  'show-macos-version': {
    id: 'show-macos-version',
    title: 'Show macOS Version',
    subtitle: 'Print kernel, build, and macOS version.',
    category: 'dev',
    strategy: 'shell',
    keywords: ['macos', 'version', 'kernel', 'build'],
    macOnly: true,
  },
  'show-cpu-info': {
    id: 'show-cpu-info',
    title: 'Show CPU Info',
    subtitle: 'Display CPU brand, cores, and load averages.',
    category: 'dev',
    strategy: 'shell',
    keywords: ['cpu', 'processor', 'cores', 'load'],
    macOnly: true,
  },
  'show-memory-info': {
    id: 'show-memory-info',
    title: 'Show Memory Pressure',
    subtitle: 'Display current memory pressure and free memory.',
    category: 'dev',
    strategy: 'shell',
    keywords: ['memory', 'ram', 'pressure', 'free'],
    macOnly: true,
  },
  'show-disk-usage': {
    id: 'show-disk-usage',
    title: 'Show Disk Usage',
    subtitle: 'Display disk capacity and free space.',
    category: 'dev',
    strategy: 'shell',
    keywords: ['disk', 'storage', 'free', 'usage'],
    macOnly: true,
  },
  'show-battery-status': {
    id: 'show-battery-status',
    title: 'Show Battery Status',
    subtitle: 'Display battery capacity and charging state.',
    category: 'dev',
    strategy: 'shell',
    keywords: ['battery', 'charge', 'power', 'percent'],
    macOnly: true,
  },
  'list-listening-ports': {
    id: 'list-listening-ports',
    title: 'List Listening Ports',
    subtitle: 'Open Port Manager with a structured list (same as Open Ports).',
    category: 'dev',
    strategy: 'shell',
    keywords: ['ports', 'lsof', 'listen', 'listening', 'tcp', 'dev', 'port manager'],
    macOnly: true,
  },
  'git-root': {
    id: 'git-root',
    title: 'Git: Copy Repo Root',
    subtitle: 'Copy the root of the git repo open in Finder.',
    category: 'dev',
    strategy: 'applescript',
    keywords: ['git', 'root', 'repo', 'copy'],
    macOnly: true,
  },
  'brew-outdated': {
    id: 'brew-outdated',
    title: 'Homebrew: Show Outdated',
    subtitle: 'List formulae that have updates available.',
    category: 'dev',
    strategy: 'shell',
    keywords: ['brew', 'homebrew', 'outdated', 'updates'],
    macOnly: true,
  },
  'brew-update': {
    id: 'brew-update',
    title: 'Homebrew: Update',
    subtitle: 'Refresh Homebrew formula metadata.',
    category: 'dev',
    strategy: 'shell',
    keywords: ['brew', 'homebrew', 'update', 'refresh'],
    macOnly: true,
  },

  // This command has no main-process implementation — it's intercepted in
  // the renderer and navigates to the dedicated clipboard surface. Keeping
  // it in the registry means it participates in ranking, intent routing,
  // and fuzzy search like every other command.
  'open-clipboard-history': {
    id: 'open-clipboard-history',
    title: 'Clipboard History',
    subtitle: 'Browse everything you have copied — text, images, files.',
    category: 'productivity',
    strategy: 'native-helper',
    keywords: ['clipboard', 'history', 'paste', 'copy', 'pasteboard'],
    macOnly: false,
  },
  'open-snippets': {
    id: 'open-snippets',
    title: 'Snippets',
    subtitle: 'Browse, copy, and create your own text snippets (dates, UUIDs, templates, …).',
    category: 'productivity',
    strategy: 'native-helper',
    keywords: ['snippet', 'snippets', 'template', 'templates', 'text', 'boilerplate', 'expander', 'macro'],
    macOnly: false,
  },
  'open-quick-notes': {
    id: 'open-quick-notes',
    title: 'Quick Notes',
    subtitle: 'View and edit saved notes with rich text; first line is the title.',
    category: 'productivity',
    strategy: 'native-helper',
    keywords: ['notes', 'quick notes', 'notepad', 'rich text', 'memo', 'jot'],
    macOnly: false,
  },
  'open-emoji-picker': {
    id: 'open-emoji-picker',
    title: 'Emoji Picker',
    subtitle: 'Browse and copy emojis by name, mood, and category.',
    category: 'productivity',
    strategy: 'native-helper',
    keywords: ['emoji', 'smiley', 'symbol', 'icon', 'face', 'emoticon'],
    macOnly: false,
  },
}

export function getNativeCommand(id: NativeCommandId): NativeCommandDescriptor | null {
  return Object.prototype.hasOwnProperty.call(DESCRIPTORS, id) ? DESCRIPTORS[id] : null
}

export function listNativeCommands(): NativeCommandDescriptor[] {
  return Object.values(DESCRIPTORS)
}

export function searchNativeCommands(query: string, limit = 8): NativeCommandDescriptor[] {
  const q = query.trim().toLowerCase()
  if (!q) return listNativeCommands().slice(0, limit)

  type Ranked = { descriptor: NativeCommandDescriptor; score: number }
  const ranked: Ranked[] = []

  for (const descriptor of listNativeCommands()) {
    const title = descriptor.title.toLowerCase()
    const subtitle = descriptor.subtitle.toLowerCase()
    let score = -1
    if (title === q) score = 400
    else if (title.startsWith(q)) score = 260
    else if (title.includes(q)) score = 180
    else if (subtitle.includes(q)) score = 120
    else if (descriptor.keywords.some((kw) => kw.includes(q))) score = 90

    if (score >= 0) ranked.push({ descriptor, score })
  }

  ranked.sort((a, b) => b.score - a.score)
  return ranked.slice(0, limit).map((entry) => entry.descriptor)
}
