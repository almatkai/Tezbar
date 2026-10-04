import { defineConfig } from 'vitest/config'
import baseConfig from '../../vitest.config'

// Opt-in integration probes: require the sibling Tezbar Extensions checkout.
// Windows branch simulation on macOS is NOT validation on a real Windows host.
// Some assertions deliberately expose currently unfixed portability defects.
export default defineConfig({
  resolve: baseConfig.resolve,
  test: {
    environment: 'node',
    include: ['tests/manual/extension-portability.probe.test.ts'],
  },
})
