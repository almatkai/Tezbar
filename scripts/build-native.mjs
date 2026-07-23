import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')

if (process.platform !== 'darwin') {
  console.log(`No separate native helper build is required on ${process.platform}.`)
  process.exit(0)
}

for (const helper of ['axhelper', 'screenocr', 'color-picker', 'image-colors']) {
  const script = join(root, 'native', helper, 'build.sh')
  const result = spawnSync(script, { cwd: root, stdio: 'inherit' })
  if (result.error) throw result.error
  if (result.status !== 0) process.exit(result.status ?? 1)
}
