import type { ChatOptions, Delta, LLMProvider, Message, Tool } from './provider'
import { spawn } from 'node:child_process'
import { promisify } from 'node:util'

function formatConversation(messages: Message[]): string {
  return [
    'Continue the conversation below. Follow the system instructions and respond to the final user message.',
    ...messages.map((message) => `${message.role.toUpperCase()}:\n${message.content}`),
  ].join('\n\n')
}

export class OpenCodeProvider implements LLMProvider {
  readonly name = 'opencode'

  constructor(private readonly model: string) {}

  async chat(
    messages: Message[],
    _tools?: Tool[],
    options?: ChatOptions
  ): Promise<AsyncIterable<Delta>> {
    if (!messages.some((message) => message.role === 'user')) {
      throw new Error('OpenCode: no user message found')
    }

    return this.runOpenCode(formatConversation(messages), options?.signal)
  }

  private async *runOpenCode(message: string, signal?: AbortSignal): AsyncGenerator<Delta> {
    const args = ['run', '--model', this.model, '--', message]

    const child = spawn('opencode', args, {
      stdio: ['ignore', 'pipe', 'pipe'],
      env: { ...process.env, TERM: 'dumb', CI: 'true' },
    })

    if (signal) {
      const onAbort = (): void => {
        child.kill()
      }
      signal.addEventListener('abort', onAbort)
      if (signal.aborted) {
        child.kill()
      }
    }

    let output = ''
    let errorOutput = ''

    child.stdout?.on('data', (chunk: Buffer) => {
      output += chunk.toString()
    })

    child.stderr?.on('data', (chunk: Buffer) => {
      errorOutput += chunk.toString()
    })

    const exitCode = await new Promise<number>((resolve) => {
      child.on('close', (code) => resolve(code ?? 1))
    })

    if (exitCode !== 0) {
      throw new Error(`OpenCode CLI error (exit ${exitCode}): ${errorOutput.slice(0, 500)}`)
    }

    if (output.trim()) {
      yield { text: output.trim() }
    }
  }

  async isAvailable(): Promise<boolean> {
    try {
      const { execFile } = await import('node:child_process')
      const execFileAsync = promisify(execFile)
      const { stdout } = await execFileAsync('which', ['opencode'], { timeout: 4000 })
      return stdout.trim().length > 0
    } catch {
      return false
    }
  }
}
