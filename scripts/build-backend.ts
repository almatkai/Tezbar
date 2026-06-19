// scripts/build-backend.ts
import { build } from 'esbuild'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

async function runBuild(): Promise<void> {
  const root = join(__dirname, '..')

  console.log('Building TypeScript backend runner with esbuild...')
  await build({
    entryPoints: [join(root, 'src/main/server.ts')],
    bundle: true,
    platform: 'node',
    target: 'node22',
    outfile: join(root, 'dist-backend/main.js'),
    define: {
      __RAYMES_PI_POLICY_SOURCE__: JSON.stringify(
        readFileSync(join(root, 'src/main/agent/raymes-pi-policy.ts'), 'utf8')
      ),
    },
    alias: {
      electron: join(root, 'src/main/electron-shim.ts'),
      'better-sqlite3': join(root, 'src/main/better-sqlite3-shim.ts')
    },
    external: [
      'node-pty',
      'fsevents',
      'bun:sqlite'
    ],
    sourcemap: true,
    minify: false,
    format: 'cjs'
  })
  console.log('TypeScript backend runner built successfully at dist-backend/main.js')
}

runBuild().catch((err) => {
  console.error('Failed to build backend runner:', err)
  process.exit(1)
})
