import { app, clipboard } from '@tezbar/desktop-runtime'
import { randomUUID } from 'node:crypto'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { hostname } from 'node:os'
import { join } from 'node:path'
import type { SnippetListRow, SnippetWritePayload } from '../../../shared/snippets'
import type { IndexedDocument, SearchProvider } from './types'
import { captureClipboardSnapshot } from './clipboardProvider'

type Snippet = {
  id: string
  /** Command bar title; trigger is still used for search tokens. */
  label?: string
  trigger: string
  body: string
  scope?: string
  createdAt: number
}

type SnippetsDb = {
  snippets: Snippet[]
}

function snippetsPath(): string {
  const dir = join(app.getPath('userData'), 'search')
  mkdirSync(dir, { recursive: true })
  return join(dir, 'snippets.json')
}

/** Shipped defaults; missing entries are merged into an existing DB on read. */
function defaultSnippets(): Snippet[] {
  const t = Date.now()
  let i = 0
  const next = () => t - i++

  return [
    {
      id: 'snippet:today',
      label: "Get today's date",
      trigger: 'today',
      body: 'Today is ${date}.',
      createdAt: next(),
    },
    {
      id: 'snippet:time',
      label: 'Get current time',
      trigger: 'time',
      body: 'Current time: ${time}',
      createdAt: next(),
    },
    {
      id: 'snippet:datetime',
      label: 'Get date and time',
      trigger: 'datetime',
      body: '${datetime}',
      createdAt: next(),
    },
    {
      id: 'snippet:iso',
      label: 'Insert ISO 8601 timestamp',
      trigger: 'iso',
      body: '${iso}',
      createdAt: next(),
    },
    {
      id: 'snippet:year',
      label: 'Insert current year',
      trigger: 'year',
      body: '${year}',
      createdAt: next(),
    },
    {
      id: 'snippet:epoch',
      label: 'Insert Unix timestamp (seconds)',
      trigger: 'epoch',
      body: '${timestamp}',
      createdAt: next(),
    },
    {
      id: 'snippet:uuid',
      label: 'Insert random UUID',
      trigger: 'uuid',
      body: '${uuid}',
      createdAt: next(),
    },
    {
      id: 'snippet:hostname',
      label: 'Insert this computer’s hostname',
      trigger: 'hostname',
      body: '${hostname}',
      createdAt: next(),
    },
    {
      id: 'snippet:public-ip',
      label: 'Show public IP (Terminal command)',
      trigger: 'myip',
      body:
        '# Prints your public IPv4 — paste into Terminal and press Enter\n' +
        'curl -4s https://api.ipify.org\n' +
        '\n' +
        '# Alternative (IPv4 or IPv6 depending on your network)\n' +
        '# curl -s https://ifconfig.me\n',
      createdAt: next(),
    },
    {
      id: 'snippet:local-ip',
      label: 'Show local IP on macOS (Terminal)',
      trigger: 'localip',
      body:
        '# Wi‑Fi (usually en0 on MacBooks)\n' +
        'ipconfig getifaddr en0\n' +
        '\n' +
        '# If empty, try Ethernet or other interface\n' +
        '# ipconfig getifaddr en1\n' +
        '\n' +
        '# List all IPv4 addresses on the machine\n' +
        '# ifconfig | grep "inet "\n',
      createdAt: next(),
    },
    {
      id: 'snippet:signed',
      label: 'Email sign-off (professional)',
      trigger: 'signed',
      body:
        'Thank you for your time and for looking into this.\n' +
        '\n' +
        'If anything is unclear or you would like more detail, please let me know — ' +
        'I am happy to jump on a quick call or thread.\n' +
        '\n' +
        'Best regards,\n' +
        '\n' +
        '[Your name]\n' +
        '[Role / team — optional]\n' +
        '\n' +
        '—\n' +
        '[Optional: direct line · Slack @handle · calendar link]\n',
      createdAt: next(),
    },
    {
      id: 'snippet:thanks',
      label: 'Short thank-you (chat / email)',
      trigger: 'thanks',
      body:
        'Thanks a lot — I really appreciate the quick help on this.\n' +
        '\n' +
        '[Your name]\n',
      createdAt: next(),
    },
    {
      id: 'snippet:commit',
      label: 'Conventional commit message (full template)',
      trigger: 'commit',
      body:
        'feat(your-scope): short imperative summary (aim for ~50–72 chars)\n' +
        '\n' +
        'Explain why this change exists and the approach you took. Wrap lines\n' +
        'around ~72 characters so `git log` stays easy to read in a terminal.\n' +
        '\n' +
        '- user-visible or technical bullet\n' +
        '- tests / docs / migration notes if relevant\n' +
        '\n' +
        'Refs: #123\n' +
        '# Co-authored-by: Name <name@example.com>\n',
      createdAt: next(),
    },
    {
      id: 'snippet:mdtask',
      label: 'Markdown unchecked task',
      trigger: 'task',
      body: '- [ ] ',
      createdAt: next(),
    },
    {
      id: 'snippet:mdcheck',
      label: 'Markdown checked task',
      trigger: 'done',
      body: '- [x] ',
      createdAt: next(),
    },
    {
      id: 'snippet:standup',
      label: 'Daily stand-up update',
      trigger: 'standup',
      body:
        '**Yesterday**\n' +
        '- \n' +
        '\n' +
        '**Today**\n' +
        '- \n' +
        '\n' +
        '**Blockers**\n' +
        '- None\n',
      createdAt: next(),
    },
    {
      id: 'snippet:meeting',
      label: 'Meeting notes template',
      trigger: 'meeting',
      body:
        '# Meeting — ${datetime}\n' +
        '\n' +
        '**Attendees:** \n' +
        '**Goal:** \n' +
        '\n' +
        '## Agenda\n' +
        '1. \n' +
        '\n' +
        '## Discussion & decisions\n' +
        '- \n' +
        '\n' +
        '## Action items\n' +
        '| Owner | Task | Due |\n' +
        '|-------|------|-----|\n' +
        '|       |      |     |\n',
      createdAt: next(),
    },
    {
      id: 'snippet:blocker',
      label: 'Slack / Teams — blocked message',
      trigger: 'blocked',
      body:
        'Hi — I am blocked on **<short summary>**.\n' +
        '\n' +
        '**What I tried:**\n' +
        '- \n' +
        '\n' +
        '**What I need from you:**\n' +
        '- \n' +
        '\n' +
        'Happy to pair or jump on a quick call. Thanks!\n',
      createdAt: next(),
    },
    {
      id: 'snippet:pr',
      label: 'Pull request description (full)',
      trigger: 'pr',
      body:
        '## Summary\n' +
        'What does this PR change, and why should reviewers care?\n' +
        '\n' +
        '## Type of change\n' +
        '- [ ] Bug fix (non-breaking)\n' +
        '- [ ] New feature\n' +
        '- [ ] Breaking change / migration\n' +
        '- [ ] Docs only\n' +
        '\n' +
        '## How to test\n' +
        '1. \n' +
        '2. \n' +
        '\n' +
        '## Screenshots / recordings\n' +
        '\n' +
        '\n' +
        '## Rollout & risk\n' +
        '- Feature flags:\n' +
        '- Database / cache / infra:\n' +
        '\n' +
        '## Checklist\n' +
        '- [ ] I self-reviewed the diff\n' +
        '- [ ] Tests added or updated where it matters\n' +
        '- [ ] Docs / changelog updated if user-facing\n',
      createdAt: next(),
    },
    {
      id: 'snippet:issue',
      label: 'Bug report (GitHub / Jira style)',
      trigger: 'bugreport',
      body:
        '## Summary\n' +
        'One sentence: what is broken or wrong?\n' +
        '\n' +
        '## Expected behavior\n' +
        '\n' +
        '\n' +
        '## Actual behavior\n' +
        '\n' +
        '\n' +
        '## Steps to reproduce\n' +
        '1. \n' +
        '2. \n' +
        '3. \n' +
        '\n' +
        '## Environment\n' +
        '| Item | Version / details |\n' +
        '|------|-------------------|\n' +
        '| OS / device | |\n' +
        '| Browser (if web) | |\n' +
        '| App / API / commit | |\n' +
        '\n' +
        '## Logs, screenshots, or recordings\n' +
        '\n' +
        '\n' +
        '## Severity / impact\n' +
        'Who is affected and how badly (blocks release, workaround exists, …)?\n',
      createdAt: next(),
    },
    {
      id: 'snippet:changelog',
      label: 'Changelog unreleased entry',
      trigger: 'changelog',
      body:
        '## [Unreleased]\n' +
        '\n' +
        '### Added\n' +
        '- \n' +
        '\n' +
        '### Changed\n' +
        '- \n' +
        '\n' +
        '### Fixed\n' +
        '- \n' +
        '\n' +
        '### Removed\n' +
        '- \n',
      createdAt: next(),
    },
    {
      id: 'snippet:curl-json',
      label: 'curl POST with JSON (template)',
      trigger: 'curljson',
      body:
        "curl -sS -X POST 'https://api.example.com/v1/resource' \\\n" +
        "  -H 'Content-Type: application/json' \\\n" +
        "  -H 'Authorization: Bearer YOUR_TOKEN_HERE' \\\n" +
        "  -d '{\"key\": \"value\"}'\n",
      createdAt: next(),
    },
    {
      id: 'snippet:sql-select',
      label: 'SQL SELECT skeleton',
      trigger: 'sql',
      body:
        'SELECT\n' +
        '  *\n' +
        'FROM your_table\n' +
        'WHERE 1 = 1\n' +
        '  -- AND some_column = :value\n' +
        'ORDER BY created_at DESC\n' +
        'LIMIT 100;\n',
      createdAt: next(),
    },
    {
      id: 'snippet:api-error-json',
      label: 'JSON API error shape',
      trigger: 'apierror',
      body:
        '{\n' +
        '  "error": {\n' +
        '    "code": "VALIDATION_FAILED",\n' +
        '    "message": "Human-readable summary for clients.",\n' +
        '    "details": [\n' +
        '      { "field": "email", "issue": "must be a valid email" }\n' +
        '    ]\n' +
        '  }\n' +
        '}\n',
      createdAt: next(),
    },
    {
      id: 'snippet:review',
      label: 'Code review comment (constructive)',
      trigger: 'review',
      body:
        'Nice work on this part — the approach reads clearly.\n' +
        '\n' +
        'One suggestion: **<topic>** could be simplified by <idea>, because ' +
        '<reason>. Totally optional if you are tight on time.\n' +
        '\n' +
        'Let me know if you want to pair on it.\n',
      createdAt: next(),
    },
    {
      id: 'snippet:localhost',
      label: 'IPv4 localhost',
      trigger: 'localhost',
      body: '127.0.0.1',
      createdAt: next(),
    },
    {
      id: 'snippet:localurl',
      label: 'Local dev URL (HTTPS)',
      trigger: 'localurl',
      body: 'https://127.0.0.1:3000',
      createdAt: next(),
    },
    {
      id: 'snippet:docker-logs',
      label: 'docker compose logs (follow)',
      trigger: 'dlogs',
      body: 'docker compose logs -f --tail=200 SERVICE_NAME\n',
      createdAt: next(),
    },
    {
      id: 'snippet:shrug',
      label: 'Shrug emoji',
      trigger: 'shrug',
      body: '¯\\_(ツ)_/¯',
      createdAt: next(),
    },
  ]
}

function mergeMissingBuiltins(existing: Snippet[], builtins: Snippet[]): Snippet[] {
  const ids = new Set(existing.map((s) => s.id))
  const merged = [...existing]
  for (const b of builtins) {
    if (!ids.has(b.id)) {
      merged.push({ ...b, createdAt: Date.now() })
      ids.add(b.id)
    }
  }
  return merged
}

function readSnippetsDb(): SnippetsDb {
  const builtins = defaultSnippets()
  try {
    const raw = readFileSync(snippetsPath(), 'utf8')
    const parsed = JSON.parse(raw) as Partial<SnippetsDb>
    if (!Array.isArray(parsed.snippets)) return { snippets: builtins }
    return { snippets: mergeMissingBuiltins(parsed.snippets as Snippet[], builtins) }
  } catch {
    const db = { snippets: builtins }
    writeFileSync(snippetsPath(), `${JSON.stringify(db, null, 2)}\n`, 'utf8')
    return db
  }
}

function getBuiltinSnippetIds(): Set<string> {
  return new Set(defaultSnippets().map((s) => s.id))
}

function isBuiltinSnippetId(id: string): boolean {
  return getBuiltinSnippetIds().has(id)
}

function persistSnippetsDb(snippets: Snippet[]): void {
  const dir = join(app.getPath('userData'), 'search')
  mkdirSync(dir, { recursive: true })
  writeFileSync(snippetsPath(), `${JSON.stringify({ snippets }, null, 2)}\n`, 'utf8')
}

const SNIPPET_LABEL_MAX = 200
const SNIPPET_TRIGGER_MAX = 48
const SNIPPET_BODY_MAX = 100_000

function normalizeSnippetBody(body: string): string {
  return body.replace(/\r\n/g, '\n')
}

function validateSnippetWritePayload(
  label: string,
  trigger: string,
  body: string,
): { ok: true } | { ok: false; message: string } {
  const tLabel = label.trim()
  const tTrigger = trigger.trim()
  const tBody = normalizeSnippetBody(body)
  if (tLabel.length === 0) return { ok: false, message: 'Title is required' }
  if (tLabel.length > SNIPPET_LABEL_MAX) return { ok: false, message: `Title must be at most ${SNIPPET_LABEL_MAX} characters` }
  if (tTrigger.length === 0) return { ok: false, message: 'Trigger is required' }
  if (tTrigger.length > SNIPPET_TRIGGER_MAX) {
    return { ok: false, message: `Trigger must be at most ${SNIPPET_TRIGGER_MAX} characters` }
  }
  if (/[\n\r]/.test(tTrigger)) return { ok: false, message: 'Trigger must be a single line' }
  if (tBody.trim().length === 0) return { ok: false, message: 'Body cannot be empty' }
  if (tBody.length > SNIPPET_BODY_MAX) return { ok: false, message: `Body must be at most ${SNIPPET_BODY_MAX} characters` }
  return { ok: true }
}

function triggerTaken(snippets: Snippet[], trigger: string, excludeId: string | null): boolean {
  const want = trigger.trim().toLowerCase()
  return snippets.some((s) => s.id !== excludeId && s.trigger.trim().toLowerCase() === want)
}

export function addUserSnippet(payload: SnippetWritePayload): { ok: boolean; message: string; id?: string } {
  const v = validateSnippetWritePayload(payload.label, payload.trigger, payload.body)
  if (!v.ok) return { ok: false, message: v.message }

  const db = readSnippetsDb()
  const label = payload.label.trim()
  const trigger = payload.trigger.trim()
  const body = normalizeSnippetBody(payload.body)
  if (triggerTaken(db.snippets, trigger, null)) {
    return { ok: false, message: 'Another snippet already uses this trigger' }
  }

  const id = `snippet:user:${randomUUID()}`
  const createdAt = Date.now()
  const next: Snippet[] = [...db.snippets, { id, label, trigger, body, createdAt }]
  persistSnippetsDb(next)
  return { ok: true, message: 'Snippet saved', id }
}

export function updateUserSnippet(
  id: string,
  payload: SnippetWritePayload,
): { ok: boolean; message: string } {
  if (isBuiltinSnippetId(id)) {
    return { ok: false, message: 'Built-in snippets cannot be edited' }
  }
  const v = validateSnippetWritePayload(payload.label, payload.trigger, payload.body)
  if (!v.ok) return { ok: false, message: v.message }

  const db = readSnippetsDb()
  const idx = db.snippets.findIndex((s) => s.id === id)
  if (idx < 0) return { ok: false, message: 'Snippet not found' }

  const label = payload.label.trim()
  const trigger = payload.trigger.trim()
  const body = normalizeSnippetBody(payload.body)
  if (triggerTaken(db.snippets, trigger, id)) {
    return { ok: false, message: 'Another snippet already uses this trigger' }
  }

  const next = db.snippets.map((s, i) =>
    i === idx ? { ...s, label, trigger, body, createdAt: s.createdAt } : s,
  )
  persistSnippetsDb(next)
  return { ok: true, message: 'Snippet updated' }
}

export function deleteUserSnippet(id: string): { ok: boolean; message: string } {
  if (isBuiltinSnippetId(id)) {
    return { ok: false, message: 'Built-in snippets cannot be deleted' }
  }
  const db = readSnippetsDb()
  const next = db.snippets.filter((s) => s.id !== id)
  if (next.length === db.snippets.length) return { ok: false, message: 'Snippet not found' }
  persistSnippetsDb(next)
  return { ok: true, message: 'Snippet removed' }
}

function formatDate(date: Date): string {
  return date.toISOString().slice(0, 10)
}

function formatTime(date: Date): string {
  return date.toTimeString().slice(0, 8)
}

export function interpolateSnippet(input: string, now = new Date()): string {
  return input
    .split('${date}')
    .join(formatDate(now))
    .split('${time}')
    .join(formatTime(now))
    .split('${datetime}')
    .join(`${formatDate(now)} ${formatTime(now)}`)
    .split('${iso}')
    .join(now.toISOString())
    .split('${year}')
    .join(String(now.getFullYear()))
    .split('${timestamp}')
    .join(String(Math.floor(now.getTime() / 1000)))
    .split('${hostname}')
    .join(hostname())
    .replace(/\$\{uuid\}/g, () => randomUUID())
}

/** Strip a leading `;` and capitalize for a compact hint (e.g. `;today` → `Today`). */
function friendlyTriggerDisplay(trigger: string): string {
  const stripped = trigger.replace(/^;/, '').trim()
  if (!stripped) return trigger.trim()
  return stripped.charAt(0).toUpperCase() + stripped.slice(1)
}

/** Built-in snippets shipped before `label` existed — map to clearer titles. */
function resolvedSnippetLabel(snippet: Snippet): string | undefined {
  const fromFile = snippet.label?.trim()
  if (fromFile) return fromFile
  if (snippet.id === 'snippet:today') return "Get today's date"
  if (snippet.id === 'snippet:time') return 'Get current time'
  if (snippet.id === 'snippet:issue') return 'Bug report (GitHub / Jira issue template)'
  return undefined
}

function snippetBodyPreview(body: string): string {
  const oneLine = body.replace(/\s+/g, ' ').trim()
  return oneLine.length > 72 ? `${oneLine.slice(0, 69)}…` : oneLine
}

function snippetRowSubtitle(snippet: Snippet, title: string): string {
  const preview = snippetBodyPreview(snippet.body)
  const trigRaw = snippet.trigger.trim()
  const trigHint = friendlyTriggerDisplay(snippet.trigger)
  let line: string
  if (title === trigRaw) {
    line = preview.length > 0 ? preview : 'Copies text to the clipboard'
  } else {
    line = preview.length > 0 ? `${trigHint} · ${preview}` : trigHint
  }
  if (snippet.scope && snippet.scope !== 'global') {
    line = line.length > 0 ? `${line} · ${snippet.scope}` : snippet.scope
  }
  return line
}

export function listSnippetsForUi(): SnippetListRow[] {
  const db = readSnippetsDb()
  const builtinIds = getBuiltinSnippetIds()
  return db.snippets.map((snippet) => {
    const label = resolvedSnippetLabel(snippet)
    const title = (label ?? snippet.trigger).trim() || snippet.trigger
    return {
      id: snippet.id,
      title,
      subtitle: snippetRowSubtitle(snippet, title),
      trigger: snippet.trigger,
      bodyTemplate: snippet.body,
      resolvedPreview: interpolateSnippet(snippet.body),
      readonly: builtinIds.has(snippet.id),
    }
  })
}

export function copySnippetById(id: string): { ok: boolean; message: string } {
  const db = readSnippetsDb()
  const snippet = db.snippets.find((s) => s.id === id)
  if (!snippet) return { ok: false, message: 'Snippet not found' }
  const text = interpolateSnippet(snippet.body)
  clipboard.writeText(text)
  captureClipboardSnapshot()
  return { ok: true, message: 'Copied to clipboard' }
}

export const snippetsProvider: SearchProvider = {
  providerId: 'snippets',
  async buildDocuments(): Promise<IndexedDocument[]> {
    const db = readSnippetsDb()
    return db.snippets.map((snippet) => {
      const label = resolvedSnippetLabel(snippet)
      const title = (label ?? snippet.trigger).trim() || snippet.trigger
      const tokens = [snippet.trigger, snippet.body, label].filter(Boolean).join(' ')
      return {
        id: snippet.id,
        category: 'snippets',
        title,
        subtitle: snippetRowSubtitle(snippet, title),
        tokens,
        action: { type: 'copy-text', text: interpolateSnippet(snippet.body) },
        updatedAt: snippet.createdAt,
      }
    })
  },
}
