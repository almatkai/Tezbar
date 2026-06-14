import { execFile } from 'node:child_process'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)

export type CommandPermission = 'none' | 'system-control' | 'clipboard' | 'network'
export type ConfirmationPolicy = 'never' | 'recommended' | 'required'

export type CommandContext = {
  commandId: string
  payload?: Record<string, unknown>
}

export type CommandResult = {
  ok: boolean
  message: string
}

type CommandDefinition = {
  id: string
  title: string
  permission: CommandPermission
  confirmation: ConfirmationPolicy
  analyticsKey: string
  handler: (payload?: Record<string, unknown>) => Promise<CommandResult>
}

function osascriptCommandHandler(script: string, successMessage: string): CommandDefinition['handler'] {
  return async () => {
    // Pass the script as an argument rather than through bash -lc to avoid
    // shell interpretation entirely.
    await execFileAsync('/usr/bin/osascript', ['-e', script])
    return { ok: true, message: successMessage }
  }
}

export class CommandBus {
  private readonly commands = new Map<string, CommandDefinition>()

  constructor() {
    this.registerBuiltins()
  }

  private register(def: CommandDefinition): void {
    this.commands.set(def.id, def)
  }

  private registerBuiltins(): void {
    this.register({
      id: 'system.dark-mode.on',
      title: 'Enable dark mode',
      permission: 'system-control',
      confirmation: 'recommended',
      analyticsKey: 'system.dark_mode_on',
      handler: osascriptCommandHandler(
        'tell application "System Events" to tell appearance preferences to set dark mode to true',
        'Dark mode enabled',
      ),
    })

    this.register({
      id: 'system.dark-mode.off',
      title: 'Disable dark mode',
      permission: 'system-control',
      confirmation: 'recommended',
      analyticsKey: 'system.dark_mode_off',
      handler: osascriptCommandHandler(
        'tell application "System Events" to tell appearance preferences to set dark mode to false',
        'Dark mode disabled',
      ),
    })

    this.register({
      id: 'speech.read-aloud',
      title: 'Read text aloud',
      permission: 'none',
      confirmation: 'never',
      analyticsKey: 'speech.read_aloud',
      handler: async (payload) => {
        const text = String(payload?.text ?? '').trim()
        if (!text) {
          return { ok: false, message: 'No text provided for read-aloud' }
        }
        await execFileAsync('say', [text])
        return { ok: true, message: 'Reading aloud' }
      },
    })
  }

  async execute(context: CommandContext): Promise<CommandResult> {
    const command = this.commands.get(context.commandId)
    if (!command) {
      return { ok: false, message: `Unknown command: ${context.commandId}` }
    }
    return command.handler(context.payload)
  }
}

export const commandBus = new CommandBus()
