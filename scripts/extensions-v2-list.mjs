#!/usr/bin/env node
// Read the compiled application's catalog. JSON alone is only source metadata.
import { spawnSync } from 'node:child_process'
import { resolve } from 'node:path'

const executable = process.env.TEZBAR_NATIVE_CORE_CLI || resolve('src-tauri/target/debug/tezbar')
const result = spawnSync(executable, ['--extensions-v2-list', '--json'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] })
if (result.error || result.status !== 0) {
  console.error(result.error?.message || result.stderr.trim() || `Native CLI exited ${result.status}`)
  process.exit(1)
}
let extensions
try { extensions = JSON.parse(result.stdout) } catch { console.error('Native CLI returned invalid JSON'); process.exit(1) }
if (!Array.isArray(extensions)) { console.error('Native CLI returned an invalid catalog'); process.exit(1) }
if (process.argv.includes('--json')) console.log(JSON.stringify(extensions, null, 2))
else for (const extension of extensions) {
  const available = extension.nativeV2?.commands || extension.commands || []
  const covered = available.filter(command => command.runtime === 'rust').length
  console.log(`${extension.id}\t${extension.installationStatus || (extension.installed ? 'installed' : 'not-installed')}\t${covered}/${available.length} native commands\t${extension.status}`)
}
