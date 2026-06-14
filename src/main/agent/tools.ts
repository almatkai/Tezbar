/**
 * Tool schema — MIRRORED from pi-coding-agent.
 *
 * Source (read-only, do not re-implement):
 *   ~/Library/pnpm/global/5/.pnpm/@mariozechner+pi-coding-agent@0.67.6_*
 *     /node_modules/@mariozechner/pi-coding-agent/dist/core/tools/index.d.ts
 *
 * pi ships seven built-in tools. The default set used by
 * `createAgentSession({ tools: codingTools })` is `[read, bash, edit, write]`;
 * the CLI can widen that via `--tools read,bash,edit,write,grep,find,ls`.
 *
 * We do NOT re-implement these — pi executes them inside its own process.
 * Tezbar only needs to know the *shape* of each tool so the HUD can render
 * meaningful stage labels from the `tool_execution_start` event's `args`
 * payload. Keep this file in sync with pi upstream when you bump the
 * pnpm-global version.
 *
 * Tool call/result events are delivered by pi's RPC protocol:
 *   tool_execution_start  { toolCallId, toolName, args }
 *   tool_execution_update { toolCallId, toolName, args, partialResult }
 *   tool_execution_end    { toolCallId, toolName, result, isError }
 *
 * See docs/rpc.md § "Events" in the pi package for the full event list.
 */

export type PiToolName = 'read' | 'bash' | 'edit' | 'write' | 'grep' | 'find' | 'ls'

export interface PiToolDescriptor {
  name: PiToolName
  /** Short one-liner for HUD tooltips. */
  description: string
  /** Argument keys in pi's TypeBox schema (kept as string tuples for ergonomics). */
  argKeys: readonly string[]
  /** Whether this tool mutates state. Used for safety prompts in future work. */
  mutates: boolean
  /**
   * Build a compact HUD label from pi's `args` payload.
   * Kept pure so tests can exercise it without spinning up a subprocess.
   */
  label: (args: Record<string, unknown>) => string
}

function str(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback
}

function truncate(value: string, max = 60): string {
  const clean = value.replace(/\s+/g, ' ').trim()
  if (clean.length <= max) return clean
  return `${clean.slice(0, max - 1)}…`
}

export const PI_TOOLS: Record<PiToolName, PiToolDescriptor> = {
  read: {
    name: 'read',
    description: 'Read a file (optional offset/limit for large files)',
    argKeys: ['path', 'offset', 'limit'] as const,
    mutates: false,
    label: (args) => `read ${truncate(str(args.path, '<path>'))}`,
  },
  bash: {
    name: 'bash',
    description: 'Run a shell command (optional timeout ms)',
    argKeys: ['command', 'timeout'] as const,
    mutates: true,
    label: (args) => `bash: ${truncate(str(args.command, '<cmd>'))}`,
  },
  edit: {
    name: 'edit',
    description: 'Apply one or more oldText/newText edits to a file',
    argKeys: ['path', 'edits'] as const,
    mutates: true,
    label: (args) => {
      const edits = Array.isArray(args.edits) ? args.edits.length : 0
      return `edit ${truncate(str(args.path, '<path>'))} (${edits} change${edits === 1 ? '' : 's'})`
    },
  },
  write: {
    name: 'write',
    description: 'Overwrite (or create) a file with full content',
    argKeys: ['path', 'content'] as const,
    mutates: true,
    label: (args) => `write ${truncate(str(args.path, '<path>'))}`,
  },
  grep: {
    name: 'grep',
    description: 'Ripgrep-backed content search (glob / literal / context)',
    argKeys: [
      'pattern',
      'path',
      'glob',
      'ignoreCase',
      'literal',
      'context',
      'limit',
    ] as const,
    mutates: false,
    label: (args) => `grep ${truncate(str(args.pattern, '<pattern>'))}`,
  },
  find: {
    name: 'find',
    description: 'Find files by filename pattern',
    argKeys: ['pattern', 'path', 'limit'] as const,
    mutates: false,
    label: (args) => `find ${truncate(str(args.pattern, '<pattern>'))}`,
  },
  ls: {
    name: 'ls',
    description: 'List directory contents',
    argKeys: ['path', 'limit'] as const,
    mutates: false,
    label: (args) => `ls ${truncate(str(args.path, '.'))}`,
  },
}

/** pi's own default set when no --tools flag is supplied. */
export const PI_DEFAULT_TOOLS: readonly PiToolName[] = ['read', 'bash', 'edit', 'write']

/** Our widened default — includes read-only discovery tools. */
export const RAYMES_DEFAULT_TOOLS: readonly PiToolName[] = [
  'read',
  'bash',
  'edit',
  'write',
  'grep',
  'find',
  'ls',
]

/**
 * Build a HUD label from a raw `tool_execution_start` event. Unknown tool
 * names fall through to a generic label so future pi releases that add
 * tools still render sensibly.
 */
export function labelForToolCall(toolName: string, args: unknown): string {
  const descriptor = PI_TOOLS[toolName as PiToolName]
  if (!descriptor) return `${toolName}`
  const safeArgs = args && typeof args === 'object' ? (args as Record<string, unknown>) : {}
  return descriptor.label(safeArgs)
}
