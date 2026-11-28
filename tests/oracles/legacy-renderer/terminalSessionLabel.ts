import { normalizeTerminalCommand } from '../../../src/shared/terminal'

export {
  formatTerminalSessionName,
  normalizeTerminalCommand,
  terminalDirectoryLabel,
} from '../../../src/shared/terminal'

const SHELL_PROMPT_MARKER = /(?:^|\s)[%$#❯➜]\s+(.*)$/

export function commandFromTerminalLine(line: string): string | null {
  const match = line.match(SHELL_PROMPT_MARKER)
  const command = normalizeTerminalCommand(match?.[1])
  return command || null
}
