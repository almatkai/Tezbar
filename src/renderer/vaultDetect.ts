import type { VaultKind } from '../shared/vault'

export type DetectedSecret = { kind: VaultKind; suggestedName: string }

export const VAULT_KIND_LABELS: Record<VaultKind, string> = {
  'api-key': 'API key',
  token: 'Token',
  'ssh-key': 'SSH key',
  server: 'Server',
  password: 'Password',
  note: 'Note',
}

const PREFIXES: Array<[RegExp, VaultKind, string]> = [
  [/^sk-ant-/, 'api-key', 'Anthropic API key'],
  [/^sk-(proj-)?[A-Za-z0-9_-]{8,}/, 'api-key', 'OpenAI API key'],
  [/^(ghp_|gho_|ghu_|ghs_|github_pat_)/, 'token', 'GitHub token'],
  [/^glpat-/, 'token', 'GitLab token'],
  [/^AKIA[0-9A-Z]{16}$/, 'api-key', 'AWS access key'],
  [/^xox[abpr]-/, 'token', 'Slack token'],
  [/^AIza[0-9A-Za-z_-]{30,}$/, 'api-key', 'Google API key'],
  [/^eyJ[\w-]+\.[\w-]+\.[\w-]+$/, 'token', 'Token'],
]

const PRIVATE_KEY = /-----BEGIN [A-Z ]*PRIVATE KEY-----/
const SSH_COMMAND = /^ssh\s+(?:-\S+\s+(?:\S+\s+)?)*?([\w.-]+@[\w.-]+)/
const USER_AT_HOST = /^([\w.-]+@[\w.-]+?)(?::\d+)?$/

/** Guesses what a pasted secret is, so the user can save it without typing a name. */
export function detectSecret(raw: string): DetectedSecret {
  const value = raw.trim()
  if (!value) return { kind: 'password', suggestedName: '' }

  if (PRIVATE_KEY.test(value)) return { kind: 'ssh-key', suggestedName: 'SSH private key' }

  const firstLine = (value.split(/\r?\n/, 1)[0] ?? '').trim()
  const ssh = SSH_COMMAND.exec(firstLine)
  if (ssh?.[1]) return { kind: 'server', suggestedName: ssh[1] }

  if (!/\s/.test(value)) {
    for (const [pattern, kind, name] of PREFIXES) {
      if (pattern.test(value)) return { kind, suggestedName: name }
    }
    const host = USER_AT_HOST.exec(value)
    if (host?.[1]) return { kind: 'server', suggestedName: host[1] }
    return { kind: 'password', suggestedName: 'Password' }
  }

  const name = firstLine.length > 40 ? `${firstLine.slice(0, 39)}…` : firstLine
  return { kind: 'note', suggestedName: name }
}
