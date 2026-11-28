import test from 'node:test'
import assert from 'node:assert/strict'
import { assertEmbeddingRuntimeDistribution } from './lib/embedding-runtime-policy.mjs'

const local = {
  platform: 'darwin',
  arch: 'arm64',
  unsigned: false,
  catalog: { distribution: 'local-ad-hoc' },
}
test('local ad-hoc runtime cannot enter a signed release build', () => {
  assert.throws(() => assertEmbeddingRuntimeDistribution(local), /local-ad-hoc/)
})
test('missing provenance cannot enter a signed release build', () => {
  assert.throws(() => assertEmbeddingRuntimeDistribution({ ...local, catalog: {} }), /local-ad-hoc/)
})
test('explicit local previews stay usable', () => {
  assert.doesNotThrow(() => assertEmbeddingRuntimeDistribution({ ...local, unsigned: true }))
})
test('reviewed Developer ID pin permits the remaining signing/notarization gates', () => {
  assert.doesNotThrow(() =>
    assertEmbeddingRuntimeDistribution({ ...local, catalog: { distribution: 'developer-id' } })
  )
})
for (const [platform, arch] of [
  ['darwin', 'x64'],
  ['win32', 'x64'],
  ['win32', 'arm64'],
  ['linux', 'x64'],
]) {
  test(`${platform}/${arch} retains its existing runtime/distribution policy`, () => {
    assert.doesNotThrow(() => assertEmbeddingRuntimeDistribution({ ...local, platform, arch }))
  })
}
