import { format } from 'node:util'
import { FrameReader } from './framing'
import { acceptReply, disconnect, send } from './host'
import { refreshHostSnapshot } from './services'
import { parseGitHubRepositoryUrl } from '../../shared/extensionRepository'

// stdout is exclusively a protocol stream. Extension logs cannot corrupt it.
for (const method of ['log', 'info', 'warn', 'error', 'debug'] as const) {
  console[method] = (...args: unknown[]): void => {
    process.stderr.write(`${format(...args)}\n`)
  }
}

async function main(): Promise<void> {
  const [runner, registry, legacy] = await Promise.all([
    import('../extension-runner'),
    import('../extension-registry'),
    import('../extensions/service'),
  ])
  registry.extensionRegistryEvents.on('progress', (payload: unknown) =>
    send({ type: 'event', channel: 'extension:install-progress', payload })
  )

  async function invoke(channel: string, args: unknown[]): Promise<unknown> {
    const body = args[0] as Record<string, unknown> | undefined
    const id = (): string => {
      if (
        typeof args[0] !== 'string' ||
        !/^(?:raycast\.)?[a-zA-Z0-9][a-zA-Z0-9._-]*$/.test(args[0])
      ) {
        throw new Error('A valid extension id is required')
      }
      return args[0]
    }
    const session = (): { sessionId: string } => {
      if (typeof body?.sessionId !== 'string') throw new Error('sessionId is required')
      return { sessionId: body.sessionId }
    }
    switch (channel) {
      case 'extension:run-command': {
        if (typeof body?.extensionId !== 'string' || typeof body.commandName !== 'string')
          throw new Error('extensionId and commandName are required')
        await refreshHostSnapshot()
        const argumentValues =
          body.argumentValues && typeof body.argumentValues === 'object'
            ? Object.fromEntries(
                Object.entries(body.argumentValues).map(([key, value]) => [
                  key,
                  String(value ?? ''),
                ])
              )
            : undefined
        return runner.runExtensionCommand({
          extensionId: body.extensionId,
          commandName: body.commandName,
          argumentValues,
        })
      }
      case 'extension:invoke-action': {
        if (typeof body?.actionId !== 'string') throw new Error('actionId is required')
        const formValues =
          body.formValues && typeof body.formValues === 'object'
            ? Object.fromEntries(
                Object.entries(body.formValues).map(([key, value]) => [
                  key,
                  Array.isArray(value)
                    ? value.filter((entry): entry is string => typeof entry === 'string')
                    : ['string', 'number', 'boolean'].includes(typeof value)
                      ? value
                      : String(value ?? ''),
                ])
              )
            : undefined
        return runner.invokeExtensionAction({ ...session(), actionId: body.actionId, formValues })
      }
      case 'extension:search-text-changed':
        return runner.updateSearchText({ ...session(), searchText: String(body?.searchText ?? '') })
      case 'extension:refresh-session':
        return runner.refreshExtensionSession(session())
      case 'extension:load-more':
        return runner.loadMoreExtensionSession(session())
      case 'extension:dispose-session':
        return runner.disposeExtensionSession(session().sessionId)
      case 'extension:install': {
        const repository = typeof args[0] === 'string' ? parseGitHubRepositoryUrl(args[0]) : null
        return registry.installRegistryExtension(repository?.url || id())
      }
      case 'extensions:install': {
        const extensionId = id()
        const result = await legacy.installExtension(extensionId)
        await legacy.reinstallExtension(extensionId)
        return result
      }
      case 'extensions:reinstall':
        return legacy.reinstallExtension(id())
      case 'core:extensions:shutdown':
        runner.clearAllExtensionSessions()
        return true
      default:
        throw new Error(`Extension Worker does not handle ${channel}`)
    }
  }

  function acceptFrame(line: string): void {
    let frame: {
      type?: string
      id: string
      channel?: string
      payload?: unknown
      result?: unknown
      error?: unknown
    }
    try {
      frame = JSON.parse(line)
    } catch {
      throw new Error('Invalid extension protocol frame')
    }
    if (frame.type === 'core-reply') {
      acceptReply(frame)
      return
    }
    if (
      frame.type !== 'invoke' ||
      typeof frame.id !== 'string' ||
      typeof frame.channel !== 'string'
    )
      return
    void invoke(frame.channel, Array.isArray(frame.payload) ? frame.payload : []).then(
      (result) => send({ type: 'reply', id: frame.id, result: result ?? null }),
      (error: unknown) =>
        send({
          type: 'reply',
          id: frame.id,
          error: error instanceof Error ? error.message : String(error),
        })
    )
  }
  send({ type: 'ready', protocol: 1 })
  // readline may split Unicode separators inside JSON strings (including
  // Node 26). Match the Rust host: LF bytes delimit UTF-8 records.
  const frames = new FrameReader(acceptFrame)
  const bun = (
    globalThis as unknown as { Bun?: { stdin: { stream: () => ReadableStream<Uint8Array> } } }
  ).Bun
  if (bun) {
    const reader = bun.stdin.stream().getReader()
    try {
      for (;;) {
        const { done, value } = await reader.read()
        if (done) break
        frames.push(value)
      }
    } finally {
      reader.releaseLock()
    }
  } else {
    for await (const chunk of process.stdin)
      frames.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk))
  }
  frames.finish()
  disconnect()
  runner.clearAllExtensionSessions()
  process.exit(0)
}

void main().catch((error: unknown) => {
  console.error(error)
  process.exit(1)
})
