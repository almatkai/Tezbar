import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'

// Core lifecycle now runs in Rust. This fixture starts every service against a
// temporary profile, checks persistence across restart and rejects internal IPC.
// It deliberately contains no extension runtime or generated JavaScript bundle.
const child = spawn(
  'cargo',
  [
    'test',
    '--locked',
    '--lib',
    'native_core::tests::native_profile_survives_restart_without_an_extension_runtime',
    '--',
    '--exact',
  ],
  {
    cwd: fileURLToPath(new URL('../src-tauri/', import.meta.url)),
    stdio: 'inherit',
  }
)
child.on('error', (error) => {
  console.error(error)
  process.exitCode = 1
})
child.on('exit', (code) => {
  process.exitCode = code ?? 1
})
