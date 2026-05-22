type ToolCallEvent = {
  toolName: string
  input?: {
    command?: unknown
  }
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
    handler: (event: ToolCallEvent, ctx: ExtensionContext) => ToolCallResult | undefined | Promise<ToolCallResult | undefined>,
  ): void
  registerProvider(name: string, config: RaymesPiProviderConfig): void
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
    pi.registerProvider('raymes', parsed)
  } catch {
    /* Ignore malformed bridge env so pi can still start with its own config. */
  }
}

function hasUnsafeShellSyntax(command: string): boolean {
  return /[;|<>`\n]/.test(command) || command.includes('$(') || command.includes('||')
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

function isAutoAllowedBash(command: string): boolean {
  const trimmed = command.trim()
  if (!trimmed || hasUnsafeShellSyntax(trimmed)) return false

  const parts = trimmed.split(/\s+&&\s+/).map((part) => part.trim()).filter(Boolean)
  if (parts.length === 0) return false

  const commandToRun = parts[parts.length - 1]
  if (
    !commandToRun ||
    !(isSafeGitStatus(commandToRun) || isSafeGitClone(commandToRun) || isSafeDirectoryRead(commandToRun))
  ) {
    return false
  }

  return parts.slice(0, -1).every(isSimpleCd)
}

export default function raymesPiPolicy(pi: ExtensionAPI): void {
  registerRaymesProvider(pi)

  pi.on('tool_call', async (event, ctx) => {
    if (event.toolName !== 'bash') return undefined

    const command = event.input?.command
    if (typeof command !== 'string') {
      return { block: true, reason: 'Missing bash command.' }
    }

    if (isAutoAllowedBash(command)) return undefined

    const confirmed = await ctx.ui.confirm('Run bash command?', command)
    if (confirmed) return undefined

    return { block: true, reason: 'Bash command was not approved.' }
  })
}
