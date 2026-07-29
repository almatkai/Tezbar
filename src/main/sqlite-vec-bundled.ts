import { existsSync } from 'node:fs'
import { join } from 'node:path'

type ExtensionDatabase = {
  loadExtension(path: string): void
}

/** The backend build copies the platform-specific extension beside both bundles. */
export function load(database: ExtensionDatabase): void {
  const suffix =
    process.platform === 'win32' ? 'dll' : process.platform === 'darwin' ? 'dylib' : 'so'
  const platformPath = join(__dirname, `vec0.${suffix}`)
  database.loadExtension(existsSync(platformPath) ? platformPath : join(__dirname, 'vec0'))
}
