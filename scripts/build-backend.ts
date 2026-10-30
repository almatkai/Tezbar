// scripts/build-backend.ts
import { build } from 'esbuild'
import { execFileSync } from 'node:child_process'
import {
  chmodSync,
  copyFileSync,
  cpSync,
  existsSync,
  readFileSync,
  realpathSync,
  renameSync,
  rmSync,
  writeFileSync,
  mkdirSync,
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
  const target = process.env.TAURI_ENV_TARGET_TRIPLE ?? process.env.CARGO_BUILD_TARGET
  const embeddingResources = join(root, 'build/embedding-runtime/resources')
  mkdirSync(embeddingResources, { recursive: true })
  copyFileSync(join(root, 'licenses/llama.cpp-MIT.txt'), join(embeddingResources, 'LICENSE'))
  const needsEmbeddingPayload = target
    ? target === 'aarch64-apple-darwin'
    : process.platform === 'darwin' && process.arch === 'arm64'
  if (needsEmbeddingPayload) {
    execFileSync('python3', [join(root, 'scripts/prepare-embedding-runtime.py')], {
      cwd: root,
      stdio: 'inherit',
    })
  }

  console.log('Building isolated Extension Worker...')
  const result = await build({
    entryPoints: {
      'extension-worker': join(root, 'src/main/extensionWorker/main.ts'),
    },
    bundle: true,
    platform: 'node',
    target: 'node22',
    outdir: outputDirectory,
    entryNames: '[name]',
    metafile: true,
    plugins: [
      {
        name: 'native-core-boundary',
        setup(builder) {
          builder.onResolve(
            { filter: /(?:llm\/extensionAI|search\/providers\/appsProvider|selected-text)$/ },
            () => ({
              path: join(root, 'src/main/extensionWorker/services.ts'),
            })
          )
          builder.onResolve({ filter: /(?:^|\/)windowState$/ }, () => ({
            path: join(root, 'src/main/extensionWorker/windowState.ts'),
          }))
        },
      },
    ],
    alias: {
      '@tezbar/desktop-runtime': join(root, 'src/main/extensionWorker/desktop.ts'),
      'better-sqlite3': join(root, 'src/main/better-sqlite3-shim.ts'),
      'sqlite-vec': join(root, 'src/main/sqlite-vec-bundled.ts'),
    },
    external: ['esbuild', 'fsevents', 'bun:sqlite'],
    sourcemap: true,
    minify: false,
    format: 'cjs',
  })
  // Enforce isolation at build time, including transitive dynamic imports.
  const forbidden = Object.keys(result.metafile.inputs).filter((file) =>
    /^src\/main\/(?:ipc\.|server\.|knowledge\/|search\/|agent\/|llm\/|voice\/|storage\/)/.test(file)
  )
  if (forbidden.length)
    throw new Error(`Core modules leaked into Extension Worker: ${forbidden.join(', ')}`)
  writeFileSync(
    join(outputDirectory, 'extension-worker.meta.json'),
    JSON.stringify(result.metafile, null, 2)
  )
  for (const obsolete of [
    'main.js',
    'main.js.map',
    'knowledge-worker.js',
    'knowledge-worker.js.map',
    'gemma-supervisor.cjs',
    'watch-supervisor.cjs',
  ]) {
    rmSync(join(outputDirectory, obsolete), { force: true })
  }
  mkdirSync(join(outputDirectory, 'agent'), { recursive: true })
  copyFileIfChanged(
    join(root, 'src/main/agent/raymes-pi-policy.ts'),
    join(outputDirectory, 'agent/raymes-pi-policy.ts')
  )
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
  console.log('Extension Worker built successfully at dist-backend/')
}

runBuild().catch((err) => {
  console.error('Failed to build backend runner:', err)
  process.exit(1)
})
