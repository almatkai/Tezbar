import { defineConfig } from 'vitest/config'
import baseConfig from '../../vitest.config'

export default defineConfig({
  resolve: baseConfig.resolve,
  test: {
    environment: 'node',
    include: ['tests/manual/installed-extensions.probe.test.ts'],
  },
})
