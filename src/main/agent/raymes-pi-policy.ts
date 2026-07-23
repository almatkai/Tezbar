import { Type } from '@earendil-works/pi-ai'

type ToolCallEvent = {
  toolName: string
  input?: Record<string, unknown> & { command?: unknown }
}

type ToolCallResult = {
  block?: boolean
  reason?: string
}

type ExtensionContext = {
  ui: {
    confirm(title: string, message: string, opts?: { timeoutMs?: number }): Promise<boolean>
  }
}

type ExtensionAPI = {
  on(
    event: 'tool_call',
    handler: (
      event: ToolCallEvent,
      ctx: ExtensionContext
    ) => ToolCallResult | undefined | Promise<ToolCallResult | undefined>
  ): void
  registerProvider(name: string, config: RaymesPiProviderConfig): void
  registerTool(definition: {
    name: string
    label: string
    description: string
    promptSnippet?: string
    promptGuidelines?: string[]
    parameters: unknown
    execute: (
      toolCallId: string,
      params: { query?: string; limit?: number; resultId?: string; maxChars?: number },
      signal?: AbortSignal
    ) => Promise<{ content: Array<{ type: 'text'; text: string }>; details: unknown }>
  }): void
}

type RaymesPiProviderConfig = {
  baseUrl: string
  apiKey: string
  api: 'openai-completions' | 'anthropic-messages'
  authHeader?: boolean
  models: Array<{
    id: string
    name: string
    reasoning: boolean
    input: Array<'text' | 'image'>
    cost: {
      input: number
      output: number
      cacheRead: number
      cacheWrite: number
    }
    contextWindow: number
    maxTokens: number
    compat?: Record<string, unknown>
  }>
}

function registerRaymesProvider(pi: ExtensionAPI): void {
  const raw = process.env['RAYMES_PI_PROVIDER_JSON']
  if (!raw) return
  try {
    const parsed = JSON.parse(raw) as RaymesPiProviderConfig
    if (!parsed.baseUrl || !parsed.apiKey || !parsed.api || !Array.isArray(parsed.models)) return
    pi.registerProvider('tezbar', parsed)
  } catch {
    /* Ignore malformed bridge env so pi can still start with its own config. */
  }
}

function hasUnsafeShellSyntax(command: string): boolean {
  return /[;|<>`\n]/.test(command) || command.includes('$(') || command.includes('||')
}

function persistedAllowedCommands(): Set<string> {
  const raw = process.env['RAYMES_PI_ALWAYS_ALLOW_JSON']
  if (!raw) return new Set()
  try {
    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) return new Set()
    return new Set(
      parsed
        .filter(
          (entry): entry is string =>
            typeof entry === 'string' && /^[a-z0-9][a-z0-9._+-]{0,63}$/i.test(entry)
        )
        .map((entry) => entry.toLowerCase())
    )
  } catch {
    return new Set()
  }
}

function persistedAllowedExactCommands(): Set<string> {
  const raw = process.env['RAYMES_PI_ALWAYS_ALLOW_EXACT_JSON']
  if (!raw) return new Set()
  try {
    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) return new Set()
    return new Set(
      parsed
        .filter((entry): entry is string => typeof entry === 'string')
        .map((entry) => entry.trim())
        .filter((entry) => entry && entry.length <= 16_384 && !entry.includes('\0'))
    )
  } catch {
    return new Set()
  }
}

function executableName(command: string): string {
  const token = command.trim().split(/\s+/, 1)[0] ?? ''
  return token.slice(token.lastIndexOf('/') + 1).toLowerCase()
}

const SAFE_PIPELINE_COMMANDS = new Set(['ps', 'head', 'tail', 'wc'])

export function isPersistentlyAllowedBash(
  command: string,
  allowedCommands: ReadonlySet<string>
): boolean {
  const trimmed = command.trim()
  if (!trimmed || /[;<>`\n]/.test(trimmed) || trimmed.includes('$(') || trimmed.includes('||')) {
    return false
  }

  const commands = trimmed
    .split(/\s*(?:&&|\|)\s*/)
    .map((part) => part.trim())
    .filter(Boolean)
  if (commands.length === 0) return false

  return commands.every((part) => {
    if (isSimpleCd(part)) return true
    const executable = executableName(part)
    return SAFE_PIPELINE_COMMANDS.has(executable) || allowedCommands.has(executable)
  })
}

function isSimpleCd(command: string): boolean {
  return /^cd\s+(?:"[^"]+"|'[^']+'|[~./A-Za-z0-9_ -]+)$/.test(command.trim())
}

function isSafeGitStatus(command: string): boolean {
  return /^git\s+status(?:\s+[^;&|<>`$()\n]+)*$/.test(command.trim())
}

function isSafeGitClone(command: string): boolean {
  return /^git\s+clone(?:\s+[^;&|<>`$()\n]+)+$/.test(command.trim())
}

function isSafeDirectoryRead(command: string): boolean {
  const trimmed = command.trim()
  return (
    trimmed === 'pwd' ||
    /^ls(?:\s+-[A-Za-z0-9@]+)*(?:\s+(?:"[^"]+"|'[^']+'|[~./A-Za-z0-9_ -]+))*$/.test(trimmed) ||
    /^which\s+[-A-Za-z0-9_ .+/]+$/.test(trimmed) ||
    /^command\s+-v\s+[-A-Za-z0-9_ .+/]+$/.test(trimmed) ||
    /^find\s+(?:\/Applications|~\/Applications)(?:\s+[^;&|<>`$()\n]+)*$/.test(trimmed) ||
    /^mdfind\s+[^;&|<>`$()\n]+$/.test(trimmed)
  )
}

export type IndexedSearchKind = 'launcher' | 'deep'

const MAJOR_HOME_FOLDER_PATTERN =
  /(?:~|\$HOME|\/Users\/[^/\s"']+)\/(Desktop|Documents|Downloads|Pictures|Movies|Music|Library|code)(?=\/|\s|$)/gi

function hasBroadHomeScope(command: string): boolean {
  if (/(?:^|\s)(?:~|\$HOME|\/Users\/[^/\s"']+)(?=\s|$|[|&;<>])/.test(command)) {
    return true
  }

  const roots = new Set<string>()
  for (const match of command.matchAll(MAJOR_HOME_FOLDER_PATTERN)) {
    const root = match[1]?.toLowerCase()
    if (root) roots.add(root)
  }
  return roots.size >= 2
}

/**
 * Keep broad personal-file discovery on Tezbar's indexes. Narrow searches
 * inside the active project remain valid shell work.
 */
export function preferredIndexedSearchForBash(command: string): IndexedSearchKind | null {
  const trimmed = command.trim()
  if (!trimmed) return null
  if (/^(?:\/usr\/bin\/)?mdfind\b/i.test(trimmed)) return 'launcher'
  if (!hasBroadHomeScope(trimmed)) return null

  if (
    /(?:^|[|&;]\s*)(?:\S+\/)?(?:grep|rg|ag|ack)\b/i.test(trimmed) ||
    (/^(?:\S+\/)?find\b/i.test(trimmed) && /-exec\b[\s\S]*(?:grep|rg|ag|ack)\b/i.test(trimmed))
  ) {
    return 'deep'
  }
  if (/^(?:\S+\/)?find\b/i.test(trimmed)) return 'launcher'
  return null
}

export function isAutoAllowedBash(
  command: string,
  allowedCommands: ReadonlySet<string> = persistedAllowedCommands(),
  allowedExactCommands: ReadonlySet<string> = persistedAllowedExactCommands()
): boolean {
  const trimmed = command.trim()
  if (!trimmed) return false
  if (allowedExactCommands.has(trimmed)) return true
  if (isPersistentlyAllowedBash(trimmed, allowedCommands)) return true
  if (hasUnsafeShellSyntax(trimmed)) return false

  const parts = trimmed
    .split(/\s+&&\s+/)
    .map((part) => part.trim())
    .filter(Boolean)
  if (parts.length === 0) return false

  const commandToRun = parts[parts.length - 1]
  if (
    !commandToRun ||
    !(
      isSafeGitStatus(commandToRun) ||
      isSafeGitClone(commandToRun) ||
      isSafeDirectoryRead(commandToRun)
    )
  ) {
    return false
  }

  return parts.slice(0, -1).every(isSimpleCd)
}

export default function raymesPiPolicy(pi: ExtensionAPI): void {
  registerRaymesProvider(pi)

  const knowledgeEndpoint = process.env['TEZBAR_KNOWLEDGE_ENDPOINT']
  const knowledgeToken = process.env['TEZBAR_KNOWLEDGE_TOKEN']
  const hasIndexedSearchTools = Boolean(
    knowledgeEndpoint &&
    knowledgeToken &&
    /^http:\/\/127\.0\.0\.1:\d+\/search$/.test(knowledgeEndpoint)
  )
  let launcherSearchAttempted = false
  let deepSearchAttempted = false

  if (knowledgeEndpoint && knowledgeToken && hasIndexedSearchTools) {
    pi.registerTool({
      name: 'launcher_search',
      label: 'Search Tezbar',
      description:
        'Fast indexed Tezbar search for local files, folders, applications, commands, clipboard items, notes, snippets, and links by name or metadata.',
      promptSnippet: "Search Tezbar's normal launcher index for local items by name or metadata",
      promptGuidelines: [
        'Use launcher_search first when the user asks to find a local file, folder, application, command, clipboard item, note, snippet, or link by name or metadata.',
        'Do not use find, mdfind, or a recursive home-folder shell scan before launcher_search.',
        'Use pc_search instead when the user is looking for text inside a document, PDF, screenshot, or image.',
      ],
      parameters: Type.Object({
        query: Type.String({
          description: 'The file, app, command, note, or other local item to find',
        }),
        limit: Type.Optional(
          Type.Number({ minimum: 1, maximum: 20, description: 'Maximum results (default 10)' })
        ),
      }),
      async execute(_toolCallId, params, signal) {
        const response = await fetch(knowledgeEndpoint.replace(/\/search$/, '/launcher-search'), {
          method: 'POST',
          headers: {
            Authorization: `Bearer ${knowledgeToken}`,
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ query: params.query ?? '', limit: params.limit ?? 10 }),
          signal,
        })
        const payload = (await response.json()) as {
          results?: Array<{
            id: string
            title: string
            subtitle: string
            category: string
            score: number
            target?: string
          }>
          error?: string
        }
        if (!response.ok) throw new Error(payload.error || 'Tezbar search failed')
        const results = payload.results ?? []
        const text =
          results.length === 0
            ? 'No Tezbar launcher results matched this query.'
            : results
                .map((result, index) => {
                  const target = result.target ? `\nTarget: ${result.target}` : ''
                  return `${index + 1}. [${result.category}] ${result.title}\n${result.subtitle}${target}`
                })
                .join('\n\n')
        return { content: [{ type: 'text', text }], details: { results } }
      },
    })

    pi.registerTool({
      name: 'pc_search',
      label: 'Deep Search PC Knowledge',
      description:
        'Searches the user-approved, locally indexed Tezbar knowledge folders. Returns matching source paths, page numbers, and excerpts.',
      promptSnippet:
        'Deep Search inside user-approved local documents, PDFs, screenshots, images, and notes indexed by Tezbar',
      promptGuidelines: [
        'Use pc_search first when the user asks to find text or information inside their documents, PDFs, screenshots, images, or knowledge folders.',
        'Do not use grep, rg, find, or a recursive home-folder shell scan before pc_search.',
        'Use pc_read with a returned result ID when more surrounding content is needed.',
        'Cite the source path and page number returned by pc_search when answering from indexed knowledge.',
      ],
      parameters: Type.Object({
        query: Type.String({ description: 'A focused natural-language or keyword search query' }),
        limit: Type.Optional(
          Type.Number({ minimum: 1, maximum: 20, description: 'Maximum results (default 8)' })
        ),
      }),
      async execute(_toolCallId, params, signal) {
        const response = await fetch(knowledgeEndpoint, {
          method: 'POST',
          headers: {
            Authorization: `Bearer ${knowledgeToken}`,
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ query: params.query ?? '', limit: params.limit ?? 8 }),
          signal,
        })
        const result = (await response.json()) as {
          hits?: Array<{
            chunkId: string
            path: string
            pageNumber?: number
            text: string
            score: number
          }>
          error?: string
        }
        if (!response.ok) throw new Error(result.error || 'Knowledge search failed')
        const hits = result.hits ?? []
        const text =
          hits.length === 0
            ? 'No indexed knowledge matched this query.'
            : hits
                .map((hit, index) => {
                  const page = hit.pageNumber ? ` (page ${hit.pageNumber})` : ''
                  return `${index + 1}. [${hit.chunkId}] ${hit.path}${page}\n${hit.text}`
                })
                .join('\n\n')
        return { content: [{ type: 'text', text }], details: { hits } }
      },
    })

    pi.registerTool({
      name: 'pc_read',
      label: 'Read PC Knowledge Result',
      description:
        'Reads additional nearby content for one result returned by pc_search. It can only access content from user-approved active knowledge folders.',
      parameters: Type.Object({
        resultId: Type.String({ description: 'The result ID returned by pc_search' }),
        maxChars: Type.Optional(
          Type.Number({
            minimum: 500,
            maximum: 50_000,
            description: 'Maximum text characters to return',
          })
        ),
      }),
      async execute(_toolCallId, params, signal) {
        const response = await fetch(knowledgeEndpoint.replace(/\/search$/, '/read'), {
          method: 'POST',
          headers: {
            Authorization: `Bearer ${knowledgeToken}`,
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({
            resultId: params.resultId ?? '',
            maxChars: params.maxChars ?? 12_000,
          }),
          signal,
        })
        const payload = (await response.json()) as {
          result?: { path: string; pageNumber?: number; text: string }
          error?: string
        }
        if (!response.ok || !payload.result) {
          throw new Error(payload.error || 'Knowledge result could not be read')
        }
        const page = payload.result.pageNumber ? ` (page ${payload.result.pageNumber})` : ''
        return {
          content: [
            { type: 'text', text: `${payload.result.path}${page}\n\n${payload.result.text}` },
          ],
          details: payload.result,
        }
      },
    })
  }

  pi.on('tool_call', async (event, ctx) => {
    if (event.toolName === 'launcher_search') {
      launcherSearchAttempted = true
      return undefined
    }
    if (event.toolName === 'pc_search') {
      deepSearchAttempted = true
      return undefined
    }
    if (event.toolName !== 'bash') return undefined

    const command = event.input?.command
    if (typeof command !== 'string') {
      return { block: true, reason: 'Missing bash command.' }
    }

    if (hasIndexedSearchTools) {
      const preferredSearch = preferredIndexedSearchForBash(command)
      if (preferredSearch === 'deep' && !deepSearchAttempted) {
        return {
          block: true,
          reason:
            'Use pc_search (Tezbar Deep Search) before recursively scanning personal files with grep/rg. Shell is only a fallback after Deep Search.',
        }
      }
      if (preferredSearch === 'launcher' && !launcherSearchAttempted) {
        return {
          block: true,
          reason:
            'Use launcher_search (Tezbar normal search) before broadly scanning personal folders with find/mdfind. Shell is only a fallback after indexed search.',
        }
      }
    }

    if (isAutoAllowedBash(command)) return undefined

    const confirmed = await ctx.ui.confirm('Run bash command?', command)
    if (confirmed) return undefined

    return { block: true, reason: 'Bash command was not approved.' }
  })
}
