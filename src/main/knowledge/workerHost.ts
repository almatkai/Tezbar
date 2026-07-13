import { spawn, type ChildProcess } from 'node:child_process'
import { join } from 'node:path'
import type { KnowledgeStatus } from '../../shared/knowledge'

type WorkerExit = {
  code: number | null
  signal: NodeJS.Signals | null
  expected: boolean
}

type KnowledgeWorkerMessage = {
  type?: string
  channel?: string
  payload?: KnowledgeStatus
}

export class KnowledgeWorkerHost {
  private child: ChildProcess | null = null
  private readonly expectedStops = new WeakSet<ChildProcess>()

  constructor(
    private readonly onStatus: (status: KnowledgeStatus) => void,
    private readonly onExit: (exit: WorkerExit) => void
  ) {}

  isRunning(): boolean {
    return this.child !== null
  }

  start(): void {
    if (this.child) return
    const workerPath = join(__dirname, 'knowledge-worker.js')
    const child = spawn(process.execPath, [workerPath], {
      env: { ...process.env, TEZBAR_KNOWLEDGE_WORKER: '1' },
      stdio: ['ignore', 'pipe', 'pipe'],
      windowsHide: true,
    })
    this.child = child

    let stdoutBuffer = ''
    const consumeLine = (line: string): void => {
      if (!line.trim()) return
      try {
        const message = JSON.parse(line) as KnowledgeWorkerMessage
        if (message.type === 'event' && message.channel === 'knowledge:status' && message.payload) {
          this.onStatus(message.payload)
        }
      } catch {
        console.warn('[knowledge-worker] ignored malformed worker output')
      }
    }
    child.stdout?.setEncoding('utf8')
    child.stdout?.on('data', (chunk: string) => {
      stdoutBuffer += chunk
      let newline = stdoutBuffer.indexOf('\n')
      while (newline >= 0) {
        consumeLine(stdoutBuffer.slice(0, newline))
        stdoutBuffer = stdoutBuffer.slice(newline + 1)
        newline = stdoutBuffer.indexOf('\n')
      }
    })

    let stderrBuffer = ''
    child.stderr?.setEncoding('utf8')
    child.stderr?.on('data', (chunk: string) => {
      stderrBuffer += chunk
      let newline = stderrBuffer.indexOf('\n')
      while (newline >= 0) {
        const line = stderrBuffer.slice(0, newline).trim()
        stderrBuffer = stderrBuffer.slice(newline + 1)
        if (line) console.error(`[knowledge-worker] ${line}`)
        newline = stderrBuffer.indexOf('\n')
      }
    })

    child.once('error', (error) => {
      console.error('[knowledge-worker] failed to launch:', error)
    })
    child.once('close', (code, signal) => {
      if (stdoutBuffer.trim()) consumeLine(stdoutBuffer)
      if (stderrBuffer.trim()) console.error(`[knowledge-worker] ${stderrBuffer.trim()}`)
      const expected = this.expectedStops.has(child)
      if (this.child === child) this.child = null
      this.onExit({ code, signal, expected })
    })
  }

  async stop(): Promise<void> {
    const child = this.child
    if (!child) return
    this.expectedStops.add(child)
    await new Promise<void>((resolve) => {
      let settled = false
      const finish = (): void => {
        if (settled) return
        settled = true
        clearTimeout(forceTimer)
        resolve()
      }
      const forceTimer = setTimeout(() => {
        if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL')
      }, 3_000)
      forceTimer.unref()
      child.once('close', finish)
      if (!child.kill('SIGTERM')) {
        if (this.child === child) this.child = null
        finish()
      }
    })
  }

  shutdown(): void {
    const child = this.child
    if (!child) return
    this.expectedStops.add(child)
    child.kill('SIGTERM')
  }
}
