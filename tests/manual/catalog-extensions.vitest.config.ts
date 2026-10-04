import { defineConfig } from 'vitest/config'
import { resolve } from 'node:path'
export default defineConfig({
  resolve: {
    alias: {
      '@tezbar/desktop-runtime': resolve(__dirname, '../../src/main/desktop-runtime.ts'),
      'better-sqlite3': resolve(__dirname, '../../src/main/better-sqlite3-node-shim.ts'),
    },
  },
  test: {
    include: ['tests/manual/catalog-extensions.probe.test.ts'],
    environment: 'node',
    maxWorkers: 1,
    testTimeout: 15_000,
  },
})
