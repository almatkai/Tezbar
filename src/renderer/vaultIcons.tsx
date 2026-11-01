/* eslint-disable react-refresh/only-export-components -- The shared icon registry intentionally colocates glyph data and renderers. */
import type { ReactNode } from 'react'
import type { VaultKind } from '../shared/vault'

export type VaultIcon = { id: string; label: string; keywords: string[]; paths: ReactNode }

/** Stable ids are stored in the vault; keep them independent of display labels. */
export const VAULT_ICONS: readonly VaultIcon[] = [
  {
    id: 'api-key',
    label: 'Key',
    keywords: ['api', 'credential'],
    paths: (
      <>
        <circle cx="5" cy="9" r="2.5" />
        <path d="M7 7.5 12.5 2M10.5 4l1.5 1.5M9 5.5l1.2 1.2" />
      </>
    ),
  },
  {
    id: 'token',
    label: 'Token',
    keywords: ['access', 'credential'],
    paths: (
      <>
        <rect x="1.5" y="4" width="11" height="6" rx="1.5" />
        <path d="M4.5 7h5" />
      </>
    ),
  },
  {
    id: 'ssh-key',
    label: 'Terminal',
    keywords: ['ssh', 'shell', 'key'],
    paths: (
      <>
        <rect x="1.5" y="2.5" width="11" height="9" rx="1.5" />
        <path d="m4 6 1.8 1.5L4 9M7.5 9h2.5" />
      </>
    ),
  },
  {
    id: 'server',
    label: 'Server',
    keywords: ['hosting', 'machine'],
    paths: (
      <>
        <rect x="1.5" y="2" width="11" height="4" rx="1" />
        <rect x="1.5" y="8" width="11" height="4" rx="1" />
        <path d="M4 4h.01M4 10h.01" />
      </>
    ),
  },
  {
    id: 'password',
    label: 'Lock',
    keywords: ['password', 'private', 'secure'],
    paths: (
      <>
        <rect x="2.5" y="6" width="9" height="6" rx="1.5" />
        <path d="M4.5 6V4.5a2.5 2.5 0 0 1 5 0V6" />
      </>
    ),
  },
  {
    id: 'note',
    label: 'Note',
    keywords: ['text', 'memo'],
    paths: <path d="M3 3.5h8M3 7h8M3 10.5h5" />,
  },
  {
    id: 'database',
    label: 'Database',
    keywords: ['data', 'storage', 'sql'],
    paths: (
      <>
        <ellipse cx="7" cy="3" rx="4.5" ry="1.5" />
        <path d="M2.5 3v8c0 2 9 2 9 0V3M2.5 7c0 2 9 2 9 0" />
      </>
    ),
  },
  {
    id: 'cloud',
    label: 'Cloud',
    keywords: ['hosting', 'storage', 'remote'],
    paths: <path d="M4 11h6a2.5 2.5 0 0 0 .5-5 3.5 3.5 0 0 0-6.8-1A3 3 0 0 0 4 11Z" />,
  },
  {
    id: 'globe',
    label: 'Globe',
    keywords: ['web', 'internet', 'world', 'domain'],
    paths: (
      <>
        <circle cx="7" cy="7" r="5" />
        <ellipse cx="7" cy="7" rx="2" ry="5" />
        <path d="M2 7h10" />
      </>
    ),
  },
  {
    id: 'mail',
    label: 'Mail',
    keywords: ['email', 'inbox', 'message'],
    paths: (
      <>
        <rect x="1.5" y="3" width="11" height="8" rx="1" />
        <path d="m2 4 5 4 5-4" />
      </>
    ),
  },
  {
    id: 'credit-card',
    label: 'Credit card',
    keywords: ['payment', 'billing', 'finance'],
    paths: (
      <>
        <rect x="1.5" y="3" width="11" height="8" rx="1.5" />
        <path d="M1.5 6h11M4 9h2" />
      </>
    ),
  },
  {
    id: 'bank',
    label: 'Bank',
    keywords: ['finance', 'account', 'money'],
    paths: <path d="m1.5 5 5.5-3.5L12.5 5h-11ZM3 7v3M7 7v3M11 7v3M2 12h10" />,
  },
  {
    id: 'wallet',
    label: 'Wallet',
    keywords: ['money', 'payment', 'finance'],
    paths: (
      <>
        <path d="M11 5V2.5L3 4a1.5 1.5 0 0 0-1 1.5V11a1 1 0 0 0 1 1h9V5H3" />
        <path d="M12 7H9v3h3M10.5 8.5h.01" />
      </>
    ),
  },
  {
    id: 'wifi',
    label: 'Wi-Fi',
    keywords: ['wireless', 'network', 'internet'],
    paths: (
      <path d="M1.5 4.5a8.5 8.5 0 0 1 11 0M3.5 7a5.5 5.5 0 0 1 7 0M5.5 9.5a2 2 0 0 1 3 0M7 12h.01" />
    ),
  },
  {
    id: 'router',
    label: 'Router',
    keywords: ['network', 'internet', 'modem'],
    paths: (
      <>
        <rect x="1.5" y="7" width="11" height="5" rx="1" />
        <path d="M3.5 7V2M10.5 7V2M4 9.5h.01M7 9.5h.01" />
      </>
    ),
  },
  {
    id: 'phone',
    label: 'Phone',
    keywords: ['mobile', 'device'],
    paths: (
      <>
        <rect x="3.5" y="1.5" width="7" height="11" rx="1.5" />
        <path d="M6 10h2" />
      </>
    ),
  },
  {
    id: 'laptop',
    label: 'Laptop',
    keywords: ['computer', 'device'],
    paths: (
      <>
        <rect x="3" y="2" width="8" height="7" rx="1" />
        <path d="m3 9-1.5 3h11L11 9M6 10.5h2" />
      </>
    ),
  },
  {
    id: 'desktop',
    label: 'Desktop',
    keywords: ['computer', 'monitor', 'device'],
    paths: (
      <>
        <rect x="1.5" y="2" width="11" height="7.5" rx="1" />
        <path d="M7 9.5V12M4.5 12h5" />
      </>
    ),
  },
  {
    id: 'shield',
    label: 'Shield',
    keywords: ['security', 'protection', 'safe'],
    paths: <path d="m7 1.5 5 2v4c0 2.5-3 4.5-5 5-2-.5-5-2.5-5-5v-4l5-2ZM5 7l1.5 1.5L9 6" />,
  },
  {
    id: 'fingerprint',
    label: 'Fingerprint',
    keywords: ['biometric', 'identity', 'authentication'],
    paths: (
      <path d="M2 8V6a5 5 0 0 1 10 0v2M4 10V6a3 3 0 0 1 6 0v3c0 1.5-.5 2.5-1 3M6 12c.5-1 .5-2 .5-3V6.5a.5.5 0 0 1 1 0V9M2 10v1" />
    ),
  },
  {
    id: 'user',
    label: 'User',
    keywords: ['person', 'profile', 'account'],
    paths: (
      <>
        <circle cx="7" cy="4" r="2.5" />
        <path d="M2.5 12v-1a4.5 3.5 0 0 1 9 0v1" />
      </>
    ),
  },
  {
    id: 'users',
    label: 'Users',
    keywords: ['people', 'team', 'group'],
    paths: (
      <>
        <circle cx="5" cy="4" r="2" />
        <path d="M1.5 12v-1a3.5 3 0 0 1 7 0v1M9 2a2 2 0 0 1 0 4M10 8a3 3 0 0 1 2.5 3v1" />
      </>
    ),
  },
  {
    id: 'building',
    label: 'Building',
    keywords: ['office', 'company', 'work'],
    paths: (
      <>
        <rect x="3" y="1.5" width="8" height="11" rx=".5" />
        <path d="M5 4h.5M8.5 4H9M5 6.5h.5M8.5 6.5H9M6 12.5V10h2v2.5" />
      </>
    ),
  },
  {
    id: 'home',
    label: 'Home',
    keywords: ['house', 'personal'],
    paths: <path d="m1.5 6 5.5-4.5L12.5 6M3 5v7h8V5M6 12V8h2v4" />,
  },
  {
    id: 'git-branch',
    label: 'Git branch',
    keywords: ['repository', 'source', 'version'],
    paths: (
      <>
        <circle cx="4" cy="2.5" r="1.5" />
        <circle cx="4" cy="11.5" r="1.5" />
        <circle cx="10" cy="3.5" r="1.5" />
        <path d="M4 4v6M10 5v1a3 3 0 0 1-3 3H4" />
      </>
    ),
  },
  {
    id: 'code',
    label: 'Code',
    keywords: ['developer', 'programming', 'source'],
    paths: <path d="m4 4-3 3 3 3M10 4l3 3-3 3M8 2 6 12" />,
  },
  {
    id: 'box',
    label: 'Box',
    keywords: ['package', 'container', 'storage'],
    paths: (
      <path d="m7 1.5 5 2.8v5.4l-5 2.8-5-2.8V4.3l5-2.8ZM2 4.3l5 2.8 5-2.8M7 7.1v5.4M4.5 2.9l5 2.8" />
    ),
  },
  {
    id: 'rocket',
    label: 'Rocket',
    keywords: ['launch', 'startup', 'deploy'],
    paths: (
      <>
        <path d="M5 9C5 5 8 2 12 2c0 4-3 7-7 7ZM5 6H3l-1 4 3-1M8 9v2l-4 1 1-3M3 11l-1 1" />
        <circle cx="9" cy="5" r="1" />
      </>
    ),
  },
  {
    id: 'bolt',
    label: 'Bolt',
    keywords: ['power', 'energy', 'fast'],
    paths: <path d="m8 1.5-6 7h4l-1 4 7-7H8v-4Z" />,
  },
  {
    id: 'link',
    label: 'Link',
    keywords: ['url', 'connection', 'web'],
    paths: (
      <path d="m5.5 5 2-2a2.5 2.5 0 0 1 3.5 3.5l-2 2M8.5 9l-2 2A2.5 2.5 0 0 1 3 7.5l2-2M5 9l4-4" />
    ),
  },
  {
    id: 'bookmark',
    label: 'Bookmark',
    keywords: ['saved', 'favorite'],
    paths: <path d="M3.5 2h7v10l-3.5-2-3.5 2V2Z" />,
  },
  {
    id: 'star',
    label: 'Star',
    keywords: ['favorite', 'important'],
    paths: <path d="m7 1.5 1.7 3.6 4 .5-2.9 2.8.7 4L7 10.5l-3.5 1.9.7-4L1.3 5.6l4-.5L7 1.5Z" />,
  },
  {
    id: 'heart',
    label: 'Heart',
    keywords: ['favorite', 'love'],
    paths: <path d="M7 12 2.2 7.3A3.2 3.2 0 0 1 7 3a3.2 3.2 0 0 1 4.8 4.3L7 12Z" />,
  },
  {
    id: 'flag',
    label: 'Flag',
    keywords: ['mark', 'milestone'],
    paths: <path d="M3 12.5v-11M3 2h8l-2 3 2 3H3" />,
  },
  {
    id: 'bell',
    label: 'Bell',
    keywords: ['notification', 'alert'],
    paths: <path d="M3.5 5a3.5 3.5 0 0 1 7 0v3l1.5 2H2l1.5-2V5ZM5.5 12a1.8 1.8 0 0 0 3 0M7 1v.5" />,
  },
  {
    id: 'calendar',
    label: 'Calendar',
    keywords: ['date', 'schedule', 'event'],
    paths: (
      <>
        <rect x="2" y="3" width="10" height="9" rx="1" />
        <path d="M4.5 1.5v3M9.5 1.5v3M2 6h10M5 8.5h.01M8.5 8.5h.01" />
      </>
    ),
  },
  {
    id: 'folder',
    label: 'Folder',
    keywords: ['directory', 'files'],
    paths: (
      <path d="M1.5 11V3a1 1 0 0 1 1-1H6l1.5 2h4a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1Z" />
    ),
  },
  {
    id: 'document',
    label: 'Document',
    keywords: ['file', 'paper', 'text'],
    paths: <path d="M3 1.5h5l3 3v8H3v-11ZM8 1.5v3h3M5 7h4M5 9.5h4" />,
  },
  {
    id: 'camera',
    label: 'Camera',
    keywords: ['photo', 'image', 'picture'],
    paths: (
      <>
        <path d="M1.5 5a1 1 0 0 1 1-1H4l1-2h4l1 2h1.5a1 1 0 0 1 1 1v6h-11V5Z" />
        <circle cx="7" cy="7.5" r="2.5" />
      </>
    ),
  },
  {
    id: 'gamepad',
    label: 'Gamepad',
    keywords: ['game', 'controller', 'play'],
    paths: (
      <path d="M4.5 4h5c2 0 2.5 2 3 6 .2 1.5-1 2-2 1L9 9H5l-1.5 2c-1 1-2.2.5-2-1 .5-4 1-6 3-6ZM4 5.5v3M2.5 7h3M9 6.5h.01M11 7.5h.01" />
    ),
  },
  {
    id: 'cart',
    label: 'Cart',
    keywords: ['shopping', 'store', 'purchase'],
    paths: (
      <>
        <path d="M1 2h2l1.5 7h6L12 4H3.5" />
        <circle cx="5" cy="11.5" r="1" />
        <circle cx="10" cy="11.5" r="1" />
      </>
    ),
  },
  {
    id: 'gift',
    label: 'Gift',
    keywords: ['present', 'reward'],
    paths: (
      <>
        <path d="M2.5 6.5V12h9V6.5M7 4.5V12M7 4.5H4.5A1.5 1.5 0 1 1 6 3l1 1.5Zm0 0h2.5A1.5 1.5 0 1 0 8 3L7 4.5Z" />
        <rect x="1.5" y="4.5" width="11" height="2" rx=".5" />
      </>
    ),
  },
  {
    id: 'plane',
    label: 'Plane',
    keywords: ['travel', 'flight'],
    paths: (
      <path d="m12.5 1.5-3 11-2.5-4-4-2.5 11-4.5ZM7 8.5l5.5-7M7 8.5 6 12l-1.5-2.5L2 8l3.5-1" />
    ),
  },
]

export const VAULT_DEFAULT_ICONS: Record<VaultKind, string> = {
  'api-key': 'api-key',
  token: 'token',
  'ssh-key': 'ssh-key',
  server: 'server',
  password: 'password',
  note: 'note',
}

export const VAULT_ICON_CHIP_CLASS =
  'grid h-7 w-7 shrink-0 place-items-center rounded-tezbar-chip border border-white/10 bg-white/[0.04] text-ink-3'

export function resolveVaultIcon(kind: VaultKind, icon?: string): VaultIcon {
  return (
    VAULT_ICONS.find((entry) => entry.id === icon) ??
    VAULT_ICONS.find((entry) => entry.id === VAULT_DEFAULT_ICONS[kind])!
  )
}

export function VaultIconGlyph({ entry }: { entry: VaultIcon }): JSX.Element {
  return (
    <svg
      aria-hidden
      width="14"
      height="14"
      viewBox="0 0 14 14"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.2}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {entry.paths}
    </svg>
  )
}

export function VaultItemIcon({ kind, icon }: { kind: VaultKind; icon?: string }): JSX.Element {
  return (
    <span aria-hidden className={VAULT_ICON_CHIP_CLASS}>
      <VaultIconGlyph entry={resolveVaultIcon(kind, icon)} />
    </span>
  )
}
