import { resolve } from 'node:path'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  resolve: {
    alias: {
      '@tezbar/desktop-runtime': resolve(__dirname, 'src/main/desktop-runtime.ts'),
      'better-sqlite3': resolve(__dirname, 'src/main/better-sqlite3-node-shim.ts'),
      '@earendil-works/pi-ai': resolve(__dirname, 'src/main/agent/pi-ai-test-shim.ts'),
    },
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
})
