import { join } from 'node:path'

type ExtensionDatabase = {
  loadExtension(path: string): void
}

/** The backend build copies the platform-specific extension beside both bundles. */
export function load(database: ExtensionDatabase): void {
  database.loadExtension(join(__dirname, 'vec0'))
}
