// scripts/build-backend.ts
import { build } from 'esbuild'
import {
  chmodSync,
  copyFileSync,
  cpSync,
  existsSync,
  readFileSync,
  realpathSync,
  renameSync,
} from 'node:fs'
import { join } from 'node:path'
import { getLoadablePath } from 'sqlite-vec'

function replaceGeneratedFile(source: string, destination: string): void {
  const temporaryDestination = `${destination}.${process.pid}.tmp`
  copyFileSync(source, temporaryDestination)
  chmodSync(temporaryDestination, 0o644)
  renameSync(temporaryDestination, destination)
}

function copyFileIfChanged(source: string, destination: string): void {
  if (existsSync(destination)) {
    try {
      if (readFileSync(source).equals(readFileSync(destination))) return
    } catch {
      // Fall through to the normal copy so a missing/inaccessible destination
      // still produces the useful underlying filesystem error.
    }
  }
  copyFileSync(source, destination)
}

function copyRuntimePackage(source: string, destination: string, label: string): void {
  if (!existsSync(source)) {
    throw new Error(`Missing ${label} package at ${source}`)
  }
  cpSync(realpathSync(source), destination, {
    recursive: true,
    dereference: true,
    force: true,
  })
}

async function runBuild(): Promise<void> {
  const root = join(__dirname, '..')
  const outputDirectory = join(root, 'dist-backend')

  console.log('Building TypeScript backend runner with esbuild...')
  await build({
    entryPoints: {
      main: join(root, 'src/main/server.ts'),
      'knowledge-worker': join(root, 'src/main/knowledge/worker.ts'),
    },
    bundle: true,
    platform: 'node',
    target: 'node22',
    outdir: outputDirectory,
    entryNames: '[name]',
    define: {
      __RAYMES_PI_POLICY_SOURCE__: JSON.stringify(
        readFileSync(join(root, 'src/main/agent/raymes-pi-policy.ts'), 'utf8')
      ),
    },
    alias: {
      '@tezbar/desktop-runtime': join(root, 'src/main/desktop-runtime.ts'),
      'better-sqlite3': join(root, 'src/main/better-sqlite3-shim.ts'),
      'sqlite-vec': join(root, 'src/main/sqlite-vec-bundled.ts'),
    },
    external: ['esbuild', 'fsevents', 'bun:sqlite'],
    sourcemap: true,
    minify: false,
    format: 'cjs',
  })
  const loadablePath = getLoadablePath()
  const extensionSuffix =
    process.platform === 'win32' ? 'dll' : process.platform === 'darwin' ? 'dylib' : 'so'
  // Keep the historical extensionless copy for compatibility with existing
  // development bundles, but also ship the native suffix required by Windows
  // LoadLibrary and by packaged resource layouts.
  copyFileIfChanged(loadablePath, join(outputDirectory, 'vec0'))
  copyFileIfChanged(loadablePath, join(outputDirectory, `vec0.${extensionSuffix}`))

  // pnpm exposes esbuild through junctions under node_modules. Tauri's
  // resource copier does not follow those junctions, so copy real package
  // contents into the backend bundle before packaging the Windows app.
  const esbuildPlatform =
    process.platform === 'win32'
      ? `win32-${process.arch === 'arm64' ? 'arm64' : 'x64'}`
      : `${process.platform === 'darwin' ? 'darwin' : process.platform}-${process.arch === 'arm64' ? 'arm64' : 'x64'}`
  const bundledNodeModules = join(outputDirectory, 'node_modules')
  copyRuntimePackage(
    join(root, 'node_modules', 'esbuild'),
    join(bundledNodeModules, 'esbuild'),
    'esbuild'
  )
  copyRuntimePackage(
    join(root, 'node_modules', '@esbuild', esbuildPlatform),
    join(bundledNodeModules, '@esbuild', esbuildPlatform),
    `@esbuild/${esbuildPlatform}`
  )

  // Bun's built-in SQLite disables loadable extensions on macOS. Ship a
  // loadable SQLite build beside the backend so sqlite-vec works on machines
  // that do not have Homebrew installed.
  if (process.platform === 'darwin') {
    const sqliteLibrary = [
      process.env.TEZBAR_SQLITE_LIBRARY_PATH,
      '/opt/homebrew/opt/sqlite/lib/libsqlite3.dylib',
      '/usr/local/opt/sqlite/lib/libsqlite3.dylib',
      '/usr/local/opt/sqlite3/lib/libsqlite3.dylib',
    ].find((path): path is string => Boolean(path && existsSync(path)))

    if (!sqliteLibrary) {
      throw new Error(
        'A loadable SQLite library is required for the macOS backend build. Install Homebrew sqlite or set TEZBAR_SQLITE_LIBRARY_PATH.'
      )
    }
    replaceGeneratedFile(sqliteLibrary, join(outputDirectory, 'libsqlite3.dylib'))
  }
  console.log('TypeScript backend runners built successfully at dist-backend/')
}

runBuild().catch((err) => {
  console.error('Failed to build backend runner:', err)
  process.exit(1)
})
