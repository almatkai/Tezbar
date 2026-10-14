import { spawn, type ChildProcess } from 'node:child_process'
import { homedir } from 'node:os'
import { join } from 'node:path'

type Listener = (filename: string | null) => void
const listeners = new Map<string, Set<Listener>>()
let child: ChildProcess | null = null
const children = new Set<ChildProcess>()
process.once('exit', () => {
  for (const owned of children) owned.kill('SIGKILL')
})

function send(type: 'add' | 'remove', root: string): void {
  child?.stdin?.write(`${JSON.stringify({ type, root })}\n`)
}

function start(): void {
  if (child) return
  const processChild = spawn(process.execPath, [join(__dirname, 'watch-supervisor.cjs')], {
    cwd: homedir(),
    stdio: ['pipe', 'pipe', 'ignore'],
    windowsHide: true,
  })
  child = processChild
  children.add(processChild)
  // Parent EOF closes the supervisor; it must never keep a quitting backend alive.
  processChild.unref()
  processChild.stdin?.on('error', () => {})
  let buffer = ''
  processChild.stdout?.setEncoding('utf8')
  processChild.stdout?.on('data', (chunk: string) => {
    buffer += chunk
    let newline = buffer.indexOf('\n')
    while (newline >= 0) {
      const line = buffer.slice(0, newline)
      buffer = buffer.slice(newline + 1)
      try {
        const event = JSON.parse(line) as { root: string; filename: unknown }
        if (event.filename === null || typeof event.filename === 'string') {
          for (const listener of listeners.get(event.root) ?? []) listener(event.filename)
        }
      } catch {
        /* Ignore incomplete/malformed watcher messages. */
      }
      newline = buffer.indexOf('\n')
    }
  })
  processChild.once('error', () => {
    if (child === processChild) child = null
  })
  processChild.once('close', () => {
    children.delete(processChild)
    if (child === processChild) child = null
  })
  for (const root of listeners.keys()) send('add', root)
}

/** One supervised process shares folder watches without blocking semantic indexing or IPC. */
export function watchFolder(root: string, listener: Listener): () => void {
  const existing = listeners.get(root)
  const subscriptions = existing ?? new Set<Listener>()
  subscriptions.add(listener)
  listeners.set(root, subscriptions)
  if (!child) start()
  else if (!existing) send('add', root)
  let stopped = false
  return () => {
    if (stopped) return
    stopped = true
    subscriptions.delete(listener)
    if (subscriptions.size) return
    listeners.delete(root)
    send('remove', root)
    if (!listeners.size && child) {
      const stopped = child
      child = null
      stopped.stdin?.end()
      // A blocked filesystem call may prevent EOF handling.
      const timer = setTimeout(() => stopped.kill('SIGKILL'), 3_000)
      timer.unref()
      stopped.once('close', () => clearTimeout(timer))
    }
  }
}
