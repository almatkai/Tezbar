import { app } from 'electron'
import { existsSync } from 'node:fs'
import { join } from 'node:path'

export function configurePackagedEsbuildBinary(): void {
  if (!app.isPackaged || process.env.ESBUILD_BINARY_PATH || process.platform !== 'darwin') {
    return
  }

  const packageArch = process.arch === 'arm64' ? 'darwin-arm64' : 'darwin-x64'
  const binaryPath = join(
    process.resourcesPath,
    'app.asar.unpacked',
    'node_modules',
    '@esbuild',
    packageArch,
    'bin',
    'esbuild',
  )

  if (existsSync(binaryPath)) {
    process.env.ESBUILD_BINARY_PATH = binaryPath
  }
}
