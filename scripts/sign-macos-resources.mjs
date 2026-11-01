import { readFileSync } from 'node:fs'
import { signMacOSResources } from './lib/macos-signing.mjs'

const [directory, identity] = process.argv.slice(2)
if (process.platform !== 'darwin' || !directory || !identity)
  throw new Error('Usage on macOS: sign-macos-resources DIRECTORY CERTIFICATE')
const policy = JSON.parse(
  readFileSync(new URL('../src-tauri/macos-signing.json', import.meta.url), 'utf8')
)
const resources = signMacOSResources(directory, identity, policy)
console.log(
  `Signed and verified ${resources.length} native resources for team ${policy.teamIdentifier}`
)
