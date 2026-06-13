import { chmodSync, existsSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'

const require = createRequire(import.meta.url)

let packagePath
try {
  packagePath = require.resolve('node-pty/package.json')
} catch {
  process.exit(0)
}

if (process.platform === 'win32') process.exit(0)

const helperPath = join(
  dirname(packagePath),
  'prebuilds',
  `${process.platform}-${process.arch}`,
  'spawn-helper',
)

if (existsSync(helperPath)) {
  chmodSync(helperPath, 0o755)
}
