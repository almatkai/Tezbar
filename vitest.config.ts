import { resolve } from 'node:path'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  resolve: {
    alias: {
      '@tezbar/desktop-runtime': resolve(__dirname, 'src/main/desktop-runtime.ts'),
      'better-sqlite3': resolve(__dirname, 'src/main/better-sqlite3-node-shim.ts'),
    },
  },
  test: {
    // Each worker loads a TS/React runtime and some spawn esbuild/PowerShell.
    // Bound concurrent integration processes instead of oversubscribing Windows.
    maxWorkers: process.platform === 'win32' ? 2 : undefined,
    globalSetup: ['scripts/vitest-global-setup.ts'],
    environment: 'node',
    include: ['src/**/*.test.ts', 'tests/oracles/**/*.test.ts'],
  },
})
