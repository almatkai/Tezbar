/* eslint-disable @typescript-eslint/no-require-imports */
// Keep recursive directory enumeration out of the backend's IPC event loop.
const { watch } = require('node:fs')
const { createInterface } = require('node:readline')
const watchers = new Map()
const input = createInterface({ input: process.stdin })
input.on('line', (line) => {
  try {
    const message = JSON.parse(line)
    if (typeof message.root !== 'string') return
    if (message.type === 'remove') {
      watchers.get(message.root)?.close()
      watchers.delete(message.root)
    } else if (message.type === 'add' && !watchers.has(message.root)) {
      const watcher = watch(message.root, { recursive: true }, (_, filename) => {
        process.stdout.write(
          JSON.stringify({ root: message.root, filename: filename?.toString() ?? null }) + '\n'
        )
      })
      watcher.on('error', () => {
        watcher.close()
        watchers.delete(message.root)
      })
      watchers.set(message.root, watcher)
    }
  } catch {
    // An inaccessible root keeps its existing index and can be rescanned manually.
  }
})
input.on('close', () => {
  for (const watcher of watchers.values()) watcher.close()
  process.exit(0)
})
