// Private production worker bundle for the native real-SDK gate. Never writes shared dist-backend.
import { build } from 'esbuild'
import { getLoadablePath } from 'sqlite-vec'
import { cp, mkdir, mkdtemp, realpath, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { tmpdir } from 'node:os'
const fixtures = dirname(fileURLToPath(import.meta.url))
const repo = resolve(fixtures, '../../..')
const owned = await mkdtemp(join(tmpdir(), 'tezbar-m09-fixtures-'))
const bundle = join(owned, 'resources', 'dist-backend')
await mkdir(bundle, { recursive: true })
const result = await build({
  absWorkingDir: repo,
  entryPoints: { 'extension-worker': join(repo, 'src/main/extensionWorker/main.ts') },
  bundle: true, platform: 'node', target: 'node22', outdir: bundle, entryNames: '[name]', metafile: true,
  alias: { '@tezbar/desktop-runtime': join(repo, 'src/main/extensionWorker/desktop.ts'), 'better-sqlite3': join(repo, 'src/main/better-sqlite3-shim.ts') },
  external: ['esbuild', 'fsevents', 'bun:sqlite'], sourcemap: true, minify: false, format: 'cjs',
})
const forbidden = Object.keys(result.metafile.inputs).filter(file => /^src\/main\/(?:ipc\.|server\.|knowledge\/|search\/|agent\/|llm\/|voice\/|storage\/)/.test(file))
if (forbidden.length) throw new Error('Core modules leaked into fixture worker')
const platform = `${process.platform}-${process.arch === 'arm64' ? 'arm64' : 'x64'}`
for (const name of ['esbuild', `@esbuild/${platform}`]) {
  await cp(await realpath(join(repo, 'node_modules', name)), join(bundle, 'node_modules', name), { recursive: true, dereference: true })
}
// Copy the same runtime resources without preparing models, signing, installing,
// or writing any generated file in the checkout.
const suffix = process.platform === 'win32' ? 'dll' : process.platform === 'darwin' ? 'dylib' : 'so'
await cp(getLoadablePath(), join(bundle, 'vec0'))
await cp(getLoadablePath(), join(bundle, `vec0.${suffix}`))
await cp(join(repo, 'resources', 'icons'), join(owned, 'resources', 'resources', 'icons'), { recursive: true })
if (process.platform === 'darwin') {
  // Explicitly use the already provisioned library; never install or discover a
  // binary by executing arbitrary shell text. Missing input fails preparation.
  const candidates = ['/opt/homebrew/opt/sqlite/lib/libsqlite3.dylib', '/usr/local/opt/sqlite/lib/libsqlite3.dylib', '/usr/local/opt/sqlite3/lib/libsqlite3.dylib']
  let copied = false
  for (const library of candidates) { try { await cp(library, join(bundle, 'libsqlite3.dylib')); copied = true; break } catch {} }
  if (!copied) throw new Error('Fixture requires an existing loadable SQLite library')
}
// Each test copies this seed into its own fresh profile; fixture sources are compiled by the real worker.
const extension = join(owned, 'seed', 'extensions', 'm09-native-fixtures')
await mkdir(extension, { recursive: true })
await cp(join(fixtures, 'src'), join(extension, 'src'), { recursive: true })
await cp(join(fixtures, 'package.json'), join(extension, 'package.json'))
await mkdir(join(extension, 'assets'), { recursive: true })
await writeFile(join(extension, 'assets', 'pixel.png'), Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGMQUDD4DwACBAFgjby7cQAAAABJRU5ErkJggg==', 'base64'))
await writeFile(join(owned, 'worker-metafile.json'), JSON.stringify(result.metafile, null, 2))
await writeFile(join(owned, 'prepared.json'), JSON.stringify({ owned, bundle, seed: join(owned, 'seed'), workerEntry: join(repo, 'src/main/extensionWorker/main.ts') }, null, 2))
process.stdout.write(`${join(owned, 'prepared.json')}\n`)
