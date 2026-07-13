import { spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')

if (process.platform !== 'win32') {
  console.error(
    'The Windows installer must be built on Windows. Run `pnpm build:windows` from a Windows 10/11 x64 or ARM64 machine.'
  )
  process.exit(1)
}

const tauriCli = join(root, 'node_modules', '@tauri-apps', 'cli', 'tauri.js')
if (!existsSync(tauriCli)) {
  console.error('Tauri CLI is missing. Run `pnpm install` before building the Windows installer.')
  process.exit(1)
}

for (const command of ['rustc', 'cargo']) {
  const probe = spawnSync(command, ['--version'], { cwd: root, stdio: 'ignore', windowsHide: true })
  if (probe.error || probe.status !== 0) {
    console.error(
      `${command} is missing. Install the Rust MSVC toolchain and Visual Studio C++ Build Tools before building Tezbar.`
    )
    process.exit(1)
  }
}

const result = spawnSync(
  process.execPath,
  [tauriCli, 'build', '--bundles', 'nsis,msi', '--ci', ...process.argv.slice(2)],
  { cwd: root, stdio: 'inherit', windowsHide: true }
)

if (result.error) throw result.error
if (result.status !== 0) process.exit(result.status ?? 1)

console.log('Windows installers are available under src-tauri\\target\\release\\bundle\\.')
