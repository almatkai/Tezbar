// Parent-pipe EOF also handles a hard-killed backend, preventing orphaned
// native inference processes. This file is copied beside the backend bundle.
// eslint-disable-next-line @typescript-eslint/no-require-imports -- Shared Node/Bun supervisor is shipped as CommonJS.
const { spawn } = require('node:child_process')
const child = spawn(process.argv[2], process.argv.slice(3), {
  windowsHide: true,
  stdio: ['ignore', 'ignore', 'inherit'],
})
let stopping = false
function stop() {
  if (stopping) return
  stopping = true
  child.kill()
}
child.once('error', (error) => {
  console.error(error.message)
  process.exit(1)
})
child.once('exit', (code) => process.exit(stopping ? 0 : (code ?? 1)))
process.on('SIGTERM', stop)
process.on('SIGINT', stop)
if (globalThis.Bun?.stdin) {
  void (async () => {
    const reader = globalThis.Bun.stdin.stream().getReader()
    try {
      while (!(await reader.read()).done) {
        /* lifetime pipe */
      }
    } finally {
      reader.releaseLock()
      stop()
    }
  })().catch(stop)
} else {
  process.stdin.once('end', stop)
  process.stdin.once('error', stop)
  process.stdin.resume()
}
