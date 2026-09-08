import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { once } from 'node:events'
import { mkdtemp, rm } from 'node:fs/promises'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { dirname, join, relative, resolve } from 'node:path'
import { createInterface } from 'node:readline'
import { fileURLToPath } from 'node:url'

// Run after pnpm build:backend. Uses a fresh data directory and never prints
// clipboard contents, settings, or backend payloads.
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const data = await mkdtemp(join(tmpdir(), 'tezbar-lifecycle-'))
const server = createServer()
let child
let socket
let output
try {
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  const connection = once(server, 'connection', { signal: AbortSignal.timeout(15_000) })
  const started = performance.now()
  child = spawn(process.env.BUN_EXECUTABLE || 'bun', [join(root, 'dist-backend/main.js')], {
    cwd: root,
    env: {
      ...process.env,
      APPDATA_DIR: data,
      IS_TAURI: 'true',
      BACKEND_IPC_PORT: String(server.address().port),
    },
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  })
  const exited = once(child, 'exit', { signal: AbortSignal.timeout(20_000) })
  // Avoid unhandled rejections if startup fails before these promises are awaited.
  void connection.catch(() => {})
  void exited.catch(() => {})
  child.stderr.resume()
  output = createInterface({ input: child.stdout })
  const pong = new Promise((resolvePong) => {
    output.on('line', (line) => {
      try {
        if (JSON.parse(line).type === 'pong') resolvePong()
      } catch {
        /* logs aren't replies */
      }
    })
  })
  ;[socket] = await connection
  // Bun may reset the socket as process.exit closes it after our FIN.
  socket.on('error', () => {})
  const connectedMs = Math.round(performance.now() - started)
  const pinged = performance.now()
  socket.write(`${JSON.stringify({ type: 'ping' })}\n`)
  await Promise.race([
    pong,
    exited.then(() => {
      throw new Error('Backend exited before pong')
    }),
  ])
  const pingMs = Math.round(performance.now() - pinged)
  socket.end()
  const [code] = await exited
  assert.equal(code, 0, 'Backend should shut down cleanly on host disconnect')
  console.log(
    `Backend lifecycle OK: connected in ${connectedMs} ms, pong in ${pingMs} ms, exited on disconnect.`
  )
} finally {
  socket?.destroy()
  server.close()
  output?.close()
  if (child && child.exitCode === null && child.signalCode === null) {
    const stopped = once(child, 'exit')
    child.kill()
    await stopped
  }
  const relativeData = relative(resolve(tmpdir()), resolve(data))
  assert(!relativeData.startsWith('..') && relativeData.startsWith('tezbar-lifecycle-'))
  await rm(data, { recursive: true, force: true, maxRetries: 3 })
}
