/**
 * Bridge to pi-coding-agent's RPC mode.
 *
 * pi ships an RPC transport documented at:
 *   <pi-coding-agent>/docs/rpc.md
 *
 * Invocation:
 *   pi --mode rpc [--no-session|--session <path>] [--model <pat>] [...]
 *
 * Protocol (quote from docs/rpc.md § Framing):
 *
 *   • Commands: JSON objects sent to stdin, one per line
 *   • Responses: JSON objects with `type: "response"` indicating
 *     command success/failure
 *   • Events: Agent events streamed to stdout as JSON lines
 *   • Strict JSONL with LF (\n) as the only record delimiter.
 *     Strip an optional trailing \r. Do NOT use Node's readline — it
 *     also splits on U+2028 / U+2029 which are valid inside JSON.
 *
 * We spawn the binary the same way the user already does (`pi`), so it
 * picks up their `~/.pi/agent` auth + model settings. One subprocess per
 * run keeps things simple; pi's own sessions (under `~/.pi/agent/…`)
 * handle any cross-run continuity if we ever enable it.
 */

import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { once } from 'node:events'
import { existsSync } from 'node:fs'
import { homedir } from 'node:os'
import path from 'node:path'
import { app } from 'electron'

import { createLoopDriver, type PiEvent } from './loop'
import type { AgentInputImage, Stage } from '../../shared/agent'
import { observe, type Observation } from './observer'
import { buildPromptCommand } from './prompt'

const PI_BIN_CANDIDATES = [
  // Where pnpm installs global bins for this user (matches `which pi`
  // at the time this bridge was written). We resolve at runtime so a
  // reinstall or version bump does not require a rebuild.
  path.join(homedir(), 'Library', 'pnpm', 'pi'),
  path.join(homedir(), '.local', 'share', 'pnpm', 'pi'),
]

const OPENCODE_PI_EXTENSION = path.join(
  homedir(),
  '.pi',
  'agent',
  'extensions',
  'opencode',
  'index.ts'
)

function resolveRaymesPiExtension(): string | undefined {
  const resourcesPath = (process as NodeJS.Process & { resourcesPath?: string }).resourcesPath
  const candidates = [
    process.env['RAYMES_PI_EXTENSION'],
    path.join(process.cwd(), 'src', 'main', 'agent', 'raymes-pi-policy.ts'),
    ...(app.isPackaged && resourcesPath
      ? [path.join(resourcesPath, 'agent', 'raymes-pi-policy.ts')]
      : []),
  ]
  return candidates.find((candidate): candidate is string =>
    Boolean(candidate && existsSync(candidate))
  )
}

function resolvePiBinary(override?: string): string {
  if (override && override.trim()) return override.trim()
  const envOverride = process.env['RAYMES_PI_BIN']
  if (envOverride && envOverride.trim()) return envOverride.trim()
  // First candidate is the canonical one on this machine; if neither
  // exists we still try `pi` on PATH, which is what the user expects.
  for (const candidate of PI_BIN_CANDIDATES) {
    if (existsSync(candidate)) return candidate
  }
  return 'pi'
}

export interface BridgeRunOptions {
  runId?: string
  cwd?: string
  /** Model pattern, forwarded as `--model`. */
  model?: string
  /** Tezbar-owned pi provider definition, passed through the child env. */
  raymesProviderJson?: string
  /** Persisted command families that the Tezbar policy may run without prompting. */
  raymesAlwaysAllowJson?: string
  /** Resolve a Pi extension confirmation inside the active chat surface. */
  requestApproval?: (request: { title: string; command: string }) => Promise<boolean>
  /** Additional pi CLI args (advanced). */
  extraArgs?: readonly string[]
  /** If set, pi runs with `--no-session`. Default: true (ephemeral runs). */
  ephemeral?: boolean
  /** Called for each HUD stage transition. */
  onStage?: (stage: Stage) => void
  /** Streaming assistant text (one delta per call). */
  onMessageDelta?: (delta: string) => void
  /** Final assistant text (once per run, before onDone). */
  onAnswer?: (text: string) => void
  /** Cancel via `AbortSignal`; the bridge sends `abort` then kills on timeout. */
  signal?: AbortSignal
  /** Override the pi binary path for tests. */
  piBin?: string
  /** Each stderr line from pi (diagnostics; not JSONL protocol traffic). */
  onStderrLine?: (line: string) => void
  /** Images included with the initial Pi RPC prompt. */
  images?: readonly AgentInputImage[]
  /** Prevent a stuck provider or tool call from keeping the run alive forever. */
  timeoutMs?: number
}

export interface BridgeRunResult {
  runId: string
  answer: string
  stages: Stage[]
}

export interface Bridge {
  run(task: string, options?: BridgeRunOptions): Promise<BridgeRunResult>
  /** Low-level helper — mostly for observer.ts and tests. */
  query<T = unknown>(
    command: { type: string; [k: string]: unknown },
    options?: { signal?: AbortSignal; piBin?: string; cwd?: string; timeoutMs?: number }
  ): Promise<T>
  observe(options?: { cwd?: string; piBin?: string }): Promise<Observation>
  dispose(): void
}

type PendingResponse = {
  resolve: (value: unknown) => void
  reject: (error: Error) => void
}

interface RpcSessionHandle {
  child: ChildProcessWithoutNullStreams
  pending: Map<string, PendingResponse>
  onEvent: (event: PiEvent) => void
  stderrBuffer: string[]
  closed: boolean
  requestApproval?: BridgeRunOptions['requestApproval']
}

function makeId(): string {
  return randomUUID()
}

function writeCommand(child: ChildProcessWithoutNullStreams, command: object): void {
  const line = `${JSON.stringify(command)}\n`
  child.stdin.write(line)
}

function spawnRpc(options: {
  cwd: string
  piBin: string
  ephemeral: boolean
  model?: string
  raymesProviderJson?: string
  raymesAlwaysAllowJson?: string
  extraArgs: readonly string[]
}): ChildProcessWithoutNullStreams {
  const args: string[] = ['--mode', 'rpc']
  if (options.ephemeral) args.push('--no-session')
  args.push('--no-extensions')
  if (options.model) args.push('--model', options.model)
  const raymesPiExtension = resolveRaymesPiExtension()
  if (raymesPiExtension) args.push('--extension', raymesPiExtension)
  if (options.model?.startsWith('opencode/') && existsSync(OPENCODE_PI_EXTENSION)) {
    args.push('--extension', OPENCODE_PI_EXTENSION)
  }
  args.push(...options.extraArgs)

  // stdio: stdin/stdout for the protocol, stderr captured for diagnostics.
  const child = spawn(options.piBin, args, {
    cwd: options.cwd,
    env: {
      ...process.env,
      ...(options.raymesProviderJson
        ? { RAYMES_PI_PROVIDER_JSON: options.raymesProviderJson }
        : {}),
      ...(options.raymesAlwaysAllowJson
        ? { RAYMES_PI_ALWAYS_ALLOW_JSON: options.raymesAlwaysAllowJson }
        : {}),
    },
    stdio: ['pipe', 'pipe', 'pipe'],
  }) as ChildProcessWithoutNullStreams

  child.stdout.setEncoding('utf8')
  child.stderr.setEncoding('utf8')

  return child
}

function shouldSuppressPiStderr(line: string): boolean {
  return /^Warning: No models match pattern "(?:kiro-cli\/|opencode\/opencode\/)[^"]+"$/.test(
    line.trim()
  )
}

async function handleExtensionUiRequest(
  handle: RpcSessionHandle,
  msg: {
    id?: string
    method?: string
    title?: string
    message?: string
    options?: string[]
    placeholder?: string
    prefill?: string
  }
): Promise<void> {
  const id = msg.id
  if (typeof id !== 'string') return

  if (msg.method === 'confirm') {
    const title = msg.title || 'Allow command?'
    const command = msg.message || ''
    let confirmed = false
    try {
      confirmed = (await handle.requestApproval?.({ title, command })) ?? false
    } catch {
      confirmed = false
    }
    writeCommand(handle.child, {
      type: 'extension_ui_response',
      id,
      confirmed,
    })
    return
  }

  writeCommand(handle.child, { type: 'extension_ui_response', id, cancelled: true })
}

/**
 * Stream a newline-delimited JSON reader that handles multi-chunk lines
 * correctly. Pi's RPC docs explicitly ban Node's `readline` because it
 * splits on U+2028/U+2029, which are legal inside JSON strings.
 */
function attachLineReader(stream: NodeJS.ReadableStream, onLine: (line: string) => void): void {
  let buffer = ''
  stream.on('data', (chunk: Buffer | string) => {
    buffer += typeof chunk === 'string' ? chunk : chunk.toString('utf8')
    let newlineAt = buffer.indexOf('\n')
    while (newlineAt >= 0) {
      const raw = buffer.slice(0, newlineAt)
      buffer = buffer.slice(newlineAt + 1)
      const line = raw.endsWith('\r') ? raw.slice(0, -1) : raw
      if (line.length > 0) onLine(line)
      newlineAt = buffer.indexOf('\n')
    }
  })
}

function attachHandlers(handle: RpcSessionHandle, onStderrLine?: (line: string) => void): void {
  attachLineReader(handle.child.stdout, (line) => {
    let parsed: unknown
    try {
      parsed = JSON.parse(line)
    } catch {
      return
    }
    if (!parsed || typeof parsed !== 'object') return
    const msg = parsed as {
      type?: string
      id?: string
      success?: boolean
      error?: string
      data?: unknown
    }
    if (msg.type === 'response' && typeof msg.id === 'string') {
      const pending = handle.pending.get(msg.id)
      if (!pending) return
      handle.pending.delete(msg.id)
      if (msg.success === true) {
        pending.resolve(parsed)
      } else {
        pending.reject(new Error(msg.error || 'pi rpc command failed'))
      }
      return
    }
    if (msg.type === 'extension_ui_request') {
      void handleExtensionUiRequest(handle, msg)
      return
    }
    if (typeof msg.type === 'string') {
      handle.onEvent(parsed as PiEvent)
    }
  })

  attachLineReader(handle.child.stderr, (line) => {
    if (shouldSuppressPiStderr(line)) return
    handle.stderrBuffer.push(line)
    onStderrLine?.(line)
    // Keep only the last 50 lines so we do not leak memory on chatty runs.
    if (handle.stderrBuffer.length > 50) handle.stderrBuffer.shift()
  })

  handle.child.on('close', () => {
    handle.closed = true
    const pendings = Array.from(handle.pending.values())
    for (let i = 0; i < pendings.length; i++) {
      const pending = pendings[i]
      if (pending) pending.reject(new Error('pi rpc session closed before response'))
    }
    handle.pending.clear()
  })

  handle.child.on('error', (err) => {
    handle.closed = true
    const pendings = Array.from(handle.pending.values())
    for (let i = 0; i < pendings.length; i++) {
      const pending = pendings[i]
      if (pending) pending.reject(err)
    }
    handle.pending.clear()
  })
}

async function sendAndAwait<T>(
  handle: RpcSessionHandle,
  command: { type: string; [k: string]: unknown },
  timeoutMs: number
): Promise<T> {
  if (handle.closed) throw new Error('pi rpc session already closed')
  const id = makeId()
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      handle.pending.delete(id)
      reject(new Error(`pi rpc command timed out after ${timeoutMs}ms: ${command.type}`))
    }, timeoutMs)
    handle.pending.set(id, {
      resolve: (value) => {
        clearTimeout(timer)
        resolve(value as T)
      },
      reject: (err) => {
        clearTimeout(timer)
        reject(err)
      },
    })
    writeCommand(handle.child, { ...command, id })
  })
}

export function createBridge(): Bridge {
  const ownedChildren = new Set<ChildProcessWithoutNullStreams>()

  function trackChild(child: ChildProcessWithoutNullStreams): void {
    ownedChildren.add(child)
    child.on('close', () => ownedChildren.delete(child))
  }

  return {
    async run(task, options = {}) {
      if (!task.trim()) {
        throw new Error('agent.run: task is empty')
      }

      const runId = options.runId ?? makeId()
      const cwd = options.cwd ?? process.cwd()
      const piBin = resolvePiBinary(options.piBin)
      const ephemeral = options.ephemeral !== false

      const stages: Stage[] = []
      let finalAnswer = ''

      const driver = createLoopDriver({
        onStage: (stage) => {
          // Replace the stage at the same index if it already exists;
          // otherwise append. This preserves ordering for the HUD.
          const existing = stages.findIndex((s) => s.index === stage.index)
          if (existing >= 0) stages[existing] = stage
          else stages.push(stage)
          options.onStage?.(stage)
        },
        onMessageDelta: (delta) => {
          options.onMessageDelta?.(delta)
        },
        onAnswer: (text) => {
          finalAnswer = text
          options.onAnswer?.(text)
        },
        onDone: () => {
          /* handled by the promise chain below */
        },
        onError: (message) => {
          throw new Error(message)
        },
      })

      const child = spawnRpc({
        cwd,
        piBin,
        ephemeral,
        model: options.model,
        raymesProviderJson: options.raymesProviderJson,
        raymesAlwaysAllowJson: options.raymesAlwaysAllowJson,
        extraArgs: options.extraArgs ?? [],
      })
      trackChild(child)
      console.log('[tezbar:agent] spawn', {
        runId,
        piBin,
        cwd,
        ephemeral,
        model: options.model ?? '(default)',
        taskChars: task.length,
      })

      let agentEndResolved = false
      let agentEnded: () => void = () => undefined
      const agentEndPromise = new Promise<void>((resolve) => {
        agentEnded = () => {
          if (agentEndResolved) return
          agentEndResolved = true
          resolve()
        }
      })

      const handle: RpcSessionHandle = {
        child,
        pending: new Map(),
        stderrBuffer: [],
        closed: false,
        requestApproval: options.requestApproval,
        onEvent: (event) => {
          driver(event)
          if (event.type === 'agent_end') agentEnded()
        },
      }
      attachHandlers(handle, options.onStderrLine)

      const onAbort = (): void => {
        if (handle.closed) return
        // Best-effort: tell pi to abort, then give it 500ms before SIGTERM.
        try {
          writeCommand(child, { type: 'abort', id: makeId() })
        } catch {
          /* ignore — we're about to kill anyway */
        }
        setTimeout(() => {
          if (!handle.closed && !child.killed) child.kill('SIGTERM')
        }, 500)
      }
      options.signal?.addEventListener('abort', onAbort, { once: true })

      try {
        await sendAndAwait(handle, buildPromptCommand(task, options.images), 15_000)

        let runTimeout: NodeJS.Timeout | undefined
        try {
          await Promise.race([
            agentEndPromise,
            once(child, 'close').then(() => undefined),
            new Promise<never>((_resolve, reject) => {
              runTimeout = setTimeout(
                () =>
                  reject(
                    new Error(`Agent run timed out after ${options.timeoutMs ?? 15 * 60_000}ms`)
                  ),
                options.timeoutMs ?? 15 * 60_000
              )
            }),
          ])
        } finally {
          if (runTimeout) clearTimeout(runTimeout)
        }

        if (options.signal?.aborted) {
          throw new Error('Agent run aborted')
        }

        if (handle.closed && !agentEndResolved) {
          const tail = handle.stderrBuffer.slice(-8).join('\n').trim()
          throw new Error(
            tail ? `pi exited before finishing:\n${tail}` : 'pi exited before finishing'
          )
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error)
        const tail = handle.stderrBuffer
          .filter((line) => !message.includes(line))
          .slice(-6)
          .join('\n')
          .trim()
        throw new Error(tail ? `${message}\n${tail}` : message)
      } finally {
        options.signal?.removeEventListener('abort', onAbort)
        if (!handle.closed) {
          try {
            writeCommand(child, { type: 'abort', id: makeId() })
          } catch {
            /* ignore */
          }
          child.stdin.end()
          // Close the process politely so the next run starts cleanly.
          setTimeout(() => {
            if (!handle.closed && !child.killed) child.kill('SIGTERM')
          }, 500)
        }
      }

      return { runId, answer: finalAnswer, stages }
    },

    async query(command, queryOptions = {}) {
      const cwd = queryOptions.cwd ?? process.cwd()
      const piBin = resolvePiBinary(queryOptions.piBin)
      const timeoutMs = queryOptions.timeoutMs ?? 10_000

      const child = spawnRpc({ cwd, piBin, ephemeral: true, extraArgs: [] })
      trackChild(child)

      const handle: RpcSessionHandle = {
        child,
        pending: new Map(),
        stderrBuffer: [],
        closed: false,
        requestApproval: undefined,
        onEvent: () => undefined,
      }
      attachHandlers(handle)

      try {
        const result = await sendAndAwait<unknown>(handle, command, timeoutMs)
        return result as never
      } finally {
        try {
          writeCommand(child, { type: 'abort', id: makeId() })
        } catch {
          /* ignore */
        }
        child.stdin.end()
        setTimeout(() => {
          if (!handle.closed && !child.killed) child.kill('SIGTERM')
        }, 250)
      }
    },

    async observe(observeOptions = {}) {
      const cwd = observeOptions.cwd ?? process.cwd()
      const piBin = resolvePiBinary(observeOptions.piBin)
      return observe(cwd, (command) => this.query(command, { cwd, piBin, timeoutMs: 5_000 }))
    },

    dispose() {
      const children = Array.from(ownedChildren.values())
      for (let i = 0; i < children.length; i++) {
        const child = children[i]
        if (child && !child.killed) child.kill('SIGTERM')
      }
      ownedChildren.clear()
    },
  }
}

/** Lazily-created process-wide bridge; disposed on app quit from `ipc.ts`. */
let sharedBridge: Bridge | undefined
export function getSharedBridge(): Bridge {
  if (!sharedBridge) sharedBridge = createBridge()
  return sharedBridge
}

export function disposeSharedBridge(): void {
  sharedBridge?.dispose()
  sharedBridge = undefined
}
