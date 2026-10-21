import type { ReactNode } from 'react'
import { cx } from './ui/primitives'

/* Launcher icon set: 24px line glyphs drawn white on a solid colored
 * squircle. Keys are command icon kinds first, then result categories as a
 * fallback, so every row gets a tile even without a dedicated glyph. */

const SLATE = '#64748b'
const ZINC = '#71717a'
const GRAPHITE = '#27272a'
const BLUE = '#3b82f6'
const SKY = '#0ea5e9'
const CYAN = '#06b6d4'
const TEAL = '#14b8a6'
const GREEN = '#22c55e'
const AMBER = '#f59e0b'
const ORANGE = '#f97316'
const ROSE = '#f43f5e'
const PINK = '#ec4899'
const VIOLET = '#8b5cf6'
const INDIGO = '#6366f1'

const COLOR_PALETTE = [
  BLUE,
  SKY,
  CYAN,
  TEAL,
  GREEN,
  AMBER,
  ORANGE,
  ROSE,
  PINK,
  VIOLET,
  INDIGO,
  SLATE,
]

export const KNOWN_EXTENSION_APP_MAP: Record<string, string> = {
  cleanshotx: '/Applications/CleanShot X.app',
  cleanshot: '/Applications/CleanShot X.app',
  'clean-shot': '/Applications/CleanShot X.app',
  'clean-shot-x': '/Applications/CleanShot X.app',
  amphetamine: '/Applications/Amphetamine.app',
}

function hashString(str: string): number {
  let hash = 0
  for (let i = 0; i < str.length; i++) {
    hash = (hash << 5) - hash + str.charCodeAt(i)
    hash |= 0
  }
  return Math.abs(hash)
}

const FOLDER =
  'M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z'
const SPEAKER =
  'M11 4.7a.7.7 0 0 0-1.2-.5L6.4 7.6A1.4 1.4 0 0 1 5.4 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.4a1.4 1.4 0 0 1 1 .4l3.4 3.4a.7.7 0 0 0 1.2-.5Z'

const GRID = (
  <>
    <rect width="7" height="7" x="3" y="3" rx="1.5" />
    <rect width="7" height="7" x="14" y="3" rx="1.5" />
    <rect width="7" height="7" x="14" y="14" rx="1.5" />
    <rect width="7" height="7" x="3" y="14" rx="1.5" />
  </>
)
const BLOCKS = (
  <>
    <rect width="7" height="7" x="14" y="3" rx="1" />
    <path d="M10 21V8a1 1 0 0 0-1-1H4a1 1 0 0 0-1 1v12a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-5a1 1 0 0 0-1-1H3" />
  </>
)
const NOTE = (
  <>
    <path d="M15.5 3H5a2 2 0 0 0-2 2v14c0 1.1.9 2 2 2h14a2 2 0 0 0 2-2V8.5Z" />
    <path d="M15 3v6h6" />
  </>
)
const BRACES = (
  <>
    <path d="M8 3H7a2 2 0 0 0-2 2v5a2 2 0 0 1-2 2 2 2 0 0 1 2 2v5c0 1.1.9 2 2 2h1" />
    <path d="M16 21h1a2 2 0 0 0 2-2v-5c0-1.1.9-2 2-2a2 2 0 0 1-2-2V5a2 2 0 0 0-2-2h-1" />
  </>
)
const CLIPBOARD = (
  <>
    <rect width="8" height="4" x="8" y="2" rx="1" />
    <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" />
  </>
)
const TERMINAL = (
  <>
    <path d="m4 17 6-6-6-6" />
    <path d="M12 19h8" />
  </>
)
const MONITOR = (
  <>
    <rect width="20" height="14" x="2" y="3" rx="2" />
    <path d="M8 21h8M12 17v4" />
  </>
)
const PANEL = <rect width="18" height="18" x="3" y="3" rx="2" />
const FILE = (
  <>
    <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
    <path d="M14 2v4a2 2 0 0 0 2 2h4M16 13H8M16 17H8M10 9H8" />
  </>
)
const LINK = (
  <>
    <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
    <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
  </>
)

const CAMERA = (
  <>
    <path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z" />
    <circle cx="12" cy="13" r="3" />
  </>
)

const COFFEE = (
  <>
    <path d="M17 8h1a4 4 0 1 1 0 8h-1" />
    <path d="M3 8h14v9a4 4 0 0 1-4 4H7a4 4 0 0 1-4-4Z" />
    <line x1="6" y1="2" x2="6" y2="4" />
    <line x1="10" y1="2" x2="10" y2="4" />
    <line x1="14" y1="2" x2="14" y2="4" />
  </>
)

const HEADPHONES = (
  <>
    <path d="M3 14h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-7a9 9 0 0 1 18 0v7a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3" />
  </>
)

const PIPETTE = (
  <>
    <path d="m14 7 3 3" />
    <path d="M4 17 2 22l5-2" />
    <path d="m19.5 8.5-5-5a2.12 2.12 0 0 0-3 0L3.5 11.5a2.12 2.12 0 0 0 0 3l5 5a2.12 2.12 0 0 0 3 0l8-8a2.12 2.12 0 0 0 0-3Z" />
  </>
)

const CALENDAR = (
  <>
    <rect width="18" height="18" x="3" y="4" rx="2" />
    <line x1="16" y1="2" x2="16" y2="6" />
    <line x1="8" y1="2" x2="8" y2="6" />
    <line x1="3" y1="10" x2="21" y2="10" />
    <rect width="2" height="2" x="8" y="14" rx="0.5" />
    <rect width="2" height="2" x="14" y="14" rx="0.5" />
  </>
)

const SEARCH = (
  <>
    <circle cx="11" cy="11" r="8" />
    <line x1="21" y1="21" x2="16.65" y2="16.65" />
  </>
)

const SPARKLES = (
  <>
    <path d="m12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3Z" />
  </>
)

const SPEEDOMETER = (
  <>
    <path d="M12 2v2M4.93 4.93l1.41 1.41M2 12h2M20 12h2M17.66 6.34l1.41-1.41" />
    <path d="M12 18a6 6 0 1 0 0-12 6 6 0 0 0 0 12Z" />
    <path d="m14 10-3 3" />
  </>
)

const STOPWATCH = (
  <>
    <line x1="10" y1="2" x2="14" y2="2" />
    <line x1="12" y1="14" x2="12" y2="10" />
    <circle cx="12" cy="14" r="8" />
  </>
)

const PICTURE = (
  <>
    <rect width="18" height="18" x="3" y="3" rx="2" ry="2" />
    <circle cx="9" cy="9" r="2" />
    <path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" />
  </>
)

const ETHERNET = (
  <>
    <rect width="18" height="14" x="3" y="5" rx="2" />
    <path d="M7 15v4M11 15v4M15 15v4M7 9h10" />
  </>
)

const PULSE = (
  <>
    <path d="M22 12h-4l-3 9L9 3l-3 9H2" />
  </>
)

const ICONS: Record<string, { color: string; glyph: ReactNode }> = {
  // Tezbar and native commands
  settings: {
    color: SLATE,
    glyph: (
      <>
        <path d="M20 7h-9M14 17H5" />
        <circle cx="17" cy="17" r="3" />
        <circle cx="7" cy="7" r="3" />
      </>
    ),
  },
  onboarding: {
    color: INDIGO,
    glyph: <path d="M4 22V4s1-1 4-1 5 2 8 2 4-1 4-1v11s-1 1-4 1-5-2-8-2-4 1-4 1" />,
  },
  'deep-search': {
    color: CYAN,
    glyph: (
      <>
        <path d="M3 7V5a2 2 0 0 1 2-2h2M17 3h2a2 2 0 0 1 2 2v2M21 17v2a2 2 0 0 1-2 2h-2M7 21H5a2 2 0 0 1-2-2v-2" />
        <circle cx="12" cy="12" r="3" />
        <path d="m16 16-1.9-1.9" />
      </>
    ),
  },
  indexing: {
    color: SKY,
    glyph: (
      <>
        <ellipse cx="12" cy="5" rx="9" ry="3" />
        <path d="M3 5v14a9 3 0 0 0 18 0V5M3 12a9 3 0 0 0 18 0" />
      </>
    ),
  },
  extensions: { color: ORANGE, glyph: BLOCKS },
  snippets: { color: TEAL, glyph: BRACES },
  notes: { color: AMBER, glyph: NOTE },
  emoji: {
    color: AMBER,
    glyph: (
      <>
        <circle cx="12" cy="12" r="10" />
        <path d="M8 14s1.5 2 4 2 4-2 4-2M9 9h.01M15 9h.01" />
      </>
    ),
  },
  clipboard: { color: SLATE, glyph: CLIPBOARD },
  terminal: { color: GRAPHITE, glyph: TERMINAL },
  key: {
    color: GREEN,
    glyph: (
      <>
        <path d="m15.5 7.5 2.3 2.3a1 1 0 0 0 1.4 0l2.1-2.1a1 1 0 0 0 0-1.4L19 4M21 2l-9.6 9.6" />
        <circle cx="7.5" cy="15.5" r="5.5" />
      </>
    ),
  },
  moon: { color: VIOLET, glyph: <path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z" /> },
  display: { color: BLUE, glyph: MONITOR },
  'display-sleep': {
    color: BLUE,
    glyph: (
      <>
        {MONITOR}
        <path d="M10 7.5h4l-4 4h4" />
      </>
    ),
  },
  volume: {
    color: PINK,
    glyph: (
      <>
        <path d={SPEAKER} />
        <path d="M16 9a5 5 0 0 1 0 6M19.4 18.4a9 9 0 0 0 0-12.8" />
      </>
    ),
  },
  'volume-muted': {
    color: PINK,
    glyph: (
      <>
        <path d={SPEAKER} />
        <path d="m22 9-6 6M16 9l6 6" />
      </>
    ),
  },
  desktop: {
    color: BLUE,
    glyph: (
      <>
        {MONITOR}
        <path d="M6 7h2M6 10.5h2" />
      </>
    ),
  },
  dock: {
    color: INDIGO,
    glyph: (
      <>
        {PANEL}
        <path d="M3 15h18M8 18h.01M12 18h.01M16 18h.01" />
      </>
    ),
  },
  'menu-bar': {
    color: INDIGO,
    glyph: (
      <>
        {PANEL}
        <path d="M3 9h18" />
      </>
    ),
  },
  finder: {
    color: SKY,
    glyph: (
      <>
        {PANEL}
        <path d="M12 3c-1.5 3-2 6-1.5 10H13M8 8.5v1M16 8.5v1M7.5 16c2.5 1.5 6.5 1.5 9 0" />
      </>
    ),
  },
  awake: {
    color: AMBER,
    glyph: COFFEE,
  },
  sleep: { color: VIOLET, glyph: <path d="M4 5h6l-6 7h6M14 11h6l-6 8h6" /> },
  bluetooth: { color: BLUE, glyph: <path d="m7 7 10 10-5 5V2l5 5L7 17" /> },
  wifi: {
    color: CYAN,
    glyph: (
      <path d="M12 20h.01M2 8.8a15 15 0 0 1 20 0M5 12.9a10 10 0 0 1 14 0M8.5 16.4a5 5 0 0 1 7 0" />
    ),
  },
  network: {
    color: CYAN,
    glyph: (
      <>
        <circle cx="12" cy="12" r="10" />
        <path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20M2 12h20" />
      </>
    ),
  },
  ip: {
    color: CYAN,
    glyph: (
      <>
        <path d="M20 10c0 5-5.5 10.2-7.4 11.8a1 1 0 0 1-1.2 0C9.5 20.2 4 15 4 10a8 8 0 0 1 16 0" />
        <circle cx="12" cy="10" r="3" />
      </>
    ),
  },
  dns: {
    color: CYAN,
    glyph: (
      <>
        <rect width="20" height="8" x="2" y="2" rx="2" />
        <rect width="20" height="8" x="2" y="14" rx="2" />
        <path d="M6 6h.01M6 18h.01" />
      </>
    ),
  },
  vpn: {
    color: GREEN,
    glyph: (
      <>
        <path d="M20 13c0 5-3.5 7.5-7.7 9a1 1 0 0 1-.6 0C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.2-2.7a1.2 1.2 0 0 1 1.6 0C14.5 3.8 17 5 19 5a1 1 0 0 1 1 1Z" />
        <path d="m9 12 2 2 4-4" />
      </>
    ),
  },
  trash: {
    color: ROSE,
    glyph: <path d="M3 6h18M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />,
  },
  lock: {
    color: ZINC,
    glyph: (
      <>
        <rect width="18" height="11" x="3" y="11" rx="2" />
        <path d="M7 11V7a5 5 0 0 1 10 0v4" />
      </>
    ),
  },
  'folder-downloads': {
    color: SKY,
    glyph: (
      <>
        <path d={FOLDER} />
        <path d="M12 10v6M15 13l-3 3-3-3" />
      </>
    ),
  },
  applications: { color: INDIGO, glyph: GRID },
  library: {
    color: ORANGE,
    glyph: <path d="M3 22h18M6 18v-7M10 18v-7M14 18v-7M18 18v-7M12 2l8 5H4Z" />,
  },
  'copy-path': {
    color: SLATE,
    glyph: (
      <>
        <rect width="14" height="14" x="8" y="8" rx="2" />
        <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
      </>
    ),
  },
  quit: { color: ROSE, glyph: <path d="M12 2v10M18.4 6.6a9 9 0 1 1-12.8 0" /> },
  'kill-process': { color: ROSE, glyph: <path d="M12 2v10M18.4 6.6a9 9 0 1 1-12.8 0" /> },
  macos: {
    color: ZINC,
    glyph: (
      <path d="M18 5a2 2 0 0 1 2 2v8.5a2 2 0 0 0 .2.9l1.1 2.1a1 1 0 0 1-.9 1.5H3.6a1 1 0 0 1-.9-1.5l1.1-2.1a2 2 0 0 0 .2-.9V7a2 2 0 0 1 2-2ZM20 16H4" />
    ),
  },
  cpu: {
    color: GRAPHITE,
    glyph: PULSE,
  },
  'system-monitor': {
    color: GRAPHITE,
    glyph: PULSE,
  },
  memory: {
    color: TEAL,
    glyph: (
      <>
        <path d="M2 7a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2Z" />
        <path d="M6 19v-3M10 19v-3M14 19v-3M18 19v-3M8 11V9M12 11V9M16 11V9" />
      </>
    ),
  },
  disk: {
    color: TEAL,
    glyph: (
      <>
        <path d="M22 12H2M5.5 5.1 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.5-6.9A2 2 0 0 0 16.8 4H7.2a2 2 0 0 0-1.7 1.1Z" />
        <path d="M6 16h.01M10 16h.01" />
      </>
    ),
  },
  battery: {
    color: GREEN,
    glyph: (
      <path d="M15 7h1a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2h-2M6 7H4a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2h1M11 7l-3 5h4l-3 5M22 11v2" />
    ),
  },
  ports: {
    color: ORANGE,
    glyph: ETHERNET,
  },
  git: {
    color: ORANGE,
    glyph: (
      <>
        <path d="M6 3v12M18 9a9 9 0 0 1-9 9" />
        <circle cx="18" cy="6" r="3" />
        <circle cx="6" cy="18" r="3" />
      </>
    ),
  },
  brew: {
    color: AMBER,
    glyph: (
      <path d="M6 2v2M10 2v2M14 2v2M16 8a1 1 0 0 1 1 1v8a4 4 0 0 1-4 4H7a4 4 0 0 1-4-4V9a1 1 0 0 1 1-1h14a4 4 0 1 1 0 8h-1" />
    ),
  },
  update: { color: GREEN, glyph: <path d="M12 17V3M6 11l6 6 6-6M19 21H5" /> },
  chat: { color: VIOLET, glyph: <path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z" /> },

  // Native and bundled extensions
  amphetamine: { color: AMBER, glyph: COFFEE },
  coffee: { color: AMBER, glyph: COFFEE },
  'audio-device': { color: PINK, glyph: HEADPHONES },
  audio: { color: PINK, glyph: HEADPHONES },
  cleanshotx: { color: INDIGO, glyph: CAMERA },
  cleanshot: { color: INDIGO, glyph: CAMERA },
  'clean-shot': { color: INDIGO, glyph: CAMERA },
  'clean-shot-x': { color: INDIGO, glyph: CAMERA },
  'color-picker': { color: ROSE, glyph: PIPETTE },
  colorpicker: { color: ROSE, glyph: PIPETTE },
  'gif-search': { color: CYAN, glyph: PICTURE },
  gif: { color: CYAN, glyph: PICTURE },
  gifs: { color: CYAN, glyph: PICTURE },
  'google-calendar': { color: BLUE, glyph: CALENDAR },
  calendar: { color: BLUE, glyph: CALENDAR },
  'google-search': { color: BLUE, glyph: SEARCH },
  perplexity: { color: TEAL, glyph: SPARKLES },
  'port-manager': { color: ORANGE, glyph: ETHERNET },
  speedtest: { color: GREEN, glyph: SPEEDOMETER },
  'speed-test': { color: GREEN, glyph: SPEEDOMETER },
  timers: { color: AMBER, glyph: STOPWATCH },
  timer: { color: AMBER, glyph: STOPWATCH },

  // Result categories and path completions
  directory: { color: SKY, glyph: <path d={FOLDER} /> },
  file: { color: BLUE, glyph: FILE },
  files: { color: BLUE, glyph: FILE },
  knowledge: { color: BLUE, glyph: FILE },
  application: { color: INDIGO, glyph: GRID },
  store: { color: ORANGE, glyph: BLOCKS },
  'quick-notes': { color: AMBER, glyph: NOTE },
  'quick-links': { color: BLUE, glyph: LINK },
  commands: { color: GRAPHITE, glyph: TERMINAL },
  'native-command': { color: GRAPHITE, glyph: TERMINAL },
  'mac-cli': { color: GRAPHITE, glyph: TERMINAL },
}

export function normalizeIconKind(kind: string): string {
  return kind
    .toLowerCase()
    .trim()
    .replace(/^com\.tezbar\.native\./, '')
    .replace(/^raycast\./, '')
    .replace(/^com\.tezbar\./, '')
}

export function hasCustomLauncherIcon(kind: string): boolean {
  const normalized = normalizeIconKind(kind)
  return normalized in ICONS || kind in ICONS
}

export function launcherIcon(kind: string): { color: string; glyph: ReactNode } {
  const normalized = normalizeIconKind(kind)
  if (ICONS[normalized]) return ICONS[normalized]
  if (ICONS[kind]) return ICONS[kind]

  // Dynamic deterministic squircle for arbitrary extension slugs
  const color = COLOR_PALETTE[hashString(normalized) % COLOR_PALETTE.length] ?? BLUE
  return { color, glyph: BLOCKS }
}

export function LauncherGlyph({
  kind,
  size = 14,
}: {
  kind: string
  size?: number
}): JSX.Element {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.1"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      {launcherIcon(kind).glyph}
    </svg>
  )
}

/** Solid colored squircle with a white glyph — the shared list icon tile. */
export function Squircle({
  kind,
  size = 22,
  className,
}: {
  kind: string
  size?: number
  className?: string
}): JSX.Element {
  const icon = launcherIcon(kind)
  return (
    <span
      aria-hidden
      className={cx('squircle grid shrink-0 place-items-center text-white', className)}
      style={{
        width: size,
        height: size,
        borderRadius: Math.round(size * 0.27),
        backgroundColor: icon.color,
      }}
    >
      <LauncherGlyph kind={kind} size={Math.round(size * 0.58)} />
    </span>
  )
}
