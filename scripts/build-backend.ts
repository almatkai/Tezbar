// scripts/build-backend.ts
import { build } from 'esbuild'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

async function runBuild(): Promise<void> {
  const root = join(__dirname, '..')

  console.log('Building TypeScript backend runner with esbuild...')
  await build({
    entryPoints: {
      main: join(root, 'src/main/server.ts'),
      'knowledge-worker': join(root, 'src/main/knowledge/worker.ts'),
    },
    bundle: true,
    platform: 'node',
    target: 'node22',
    outdir: join(root, 'dist-backend'),
    entryNames: '[name]',
    define: {
      __RAYMES_PI_POLICY_SOURCE__: JSON.stringify(
        readFileSync(join(root, 'src/main/agent/raymes-pi-policy.ts'), 'utf8')
      ),
    },
    alias: {
      '@tezbar/desktop-runtime': join(root, 'src/main/desktop-runtime.ts'),
      'better-sqlite3': join(root, 'src/main/better-sqlite3-shim.ts')
    },
    external: [
      'esbuild',
      'fsevents',
      'bun:sqlite'
    ],
    sourcemap: true,
    minify: false,
    format: 'cjs'
  })
  console.log('TypeScript backend runners built successfully at dist-backend/')
}

runBuild().catch((err) => {
  console.error('Failed to build backend runner:', err)
  process.exit(1)
})
