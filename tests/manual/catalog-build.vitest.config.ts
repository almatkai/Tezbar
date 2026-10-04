import { defineConfig } from 'vitest/config'
import { resolve } from 'node:path'
export default defineConfig({
  resolve: {
    alias: { '@tezbar/desktop-runtime': resolve(__dirname, '../../src/main/desktop-runtime.ts') },
  },
  test: {
    include: [
      'tests/manual/catalog-build.probe.test.ts',
      'tests/manual/catalog-deps.probe.test.ts',
    ],
    environment: 'node',
    maxWorkers: 1,
  },
})
