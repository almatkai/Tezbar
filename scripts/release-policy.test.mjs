import { test } from 'node:test'
import assert from 'node:assert/strict'
import { generateKeyPairSync, sign, createHash } from 'node:crypto'
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import {
  assertRelease,
  acceptsRelease,
  compareVersions,
  buildConfiguration,
  immutableAssetUrl,
  validateManifest,
  validatePromotion,
  promotionTargets,
} from './lib/release-policy.mjs'
import {
  validateReceipt,
  manifestFromReceipts,
  REQUIRED_PLATFORMS,
} from './lib/release-artifacts.mjs'
import { verifyUpdaterSignature } from './verify-updater-signature.mjs'
import { assertProtectedEnvironment } from './lib/release-environment.mjs'

test('missing or weakened GitHub environment approval rules fail closed', () => {
  const environment = {
    name: 'release-stable',
    protection_rules: [
      {
        type: 'required_reviewers',
        prevent_self_review: true,
        reviewers: [{ type: 'User', reviewer: { id: 1 } }],
      },
    ],
    deployment_branch_policy: { protected_branches: true, custom_branch_policies: false },
  }
  assertProtectedEnvironment(environment)
  assert.throws(() => assertProtectedEnvironment({ name: 'release-stable' }))
  assert.throws(() => assertProtectedEnvironment({ ...environment, protection_rules: [] }))
  assert.throws(() =>
    assertProtectedEnvironment({ ...environment, deployment_branch_policy: null })
  )
  assert.throws(() =>
    assertProtectedEnvironment({
      ...environment,
      protection_rules: [{ ...environment.protection_rules[0], prevent_self_review: false }],
    })
  )
})

const base = JSON.parse(
  readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8')
)
const manifest = (version = '0.3.0', channel = 'stable') => ({
  version,
  channel,
  appIdentifier: 'com.tezbar.app',
  notes: 'Reviewed release',
  pub_date: '2026-10-12T00:00:00Z',
  platforms: {
    'windows-x86_64-nsis': {
      url: immutableAssetUrl(version, `Tezbar_${version}_x64-setup.exe`),
      signature: 'fixture',
    },
  },
})

test('channels cannot be confused by semver precedence', () => {
  assertRelease('beta', '0.3.0-beta.1')
  assert.throws(() => assertRelease('stable', '0.3.0-beta.1'))
  assert.equal(acceptsRelease('beta', '0.4.0-alpha.1'), false)
  assert.equal(acceptsRelease('beta', '0.3.0'), true)
  assert.equal(acceptsRelease('alpha', '0.3.0'), true)
  assert.equal(acceptsRelease('alpha', '0.3.0-beta.1'), true)
  assert(compareVersions('0.3.0', '0.3.0-beta.20') > 0)
  assert(compareVersions('0.3.0-beta.10', '0.3.0-beta.2') > 0)
  for (const invalid of ['0.3.0-beta.0', '0.3.0-rc.1', '0.3.0+foo', '0.3.0-alpha.01'])
    assert.throws(() => assertRelease('beta', invalid))
})

test('all channels keep the same installation and preserve normal upgrades', () => {
  const stable = buildConfiguration(base, 'stable', '0.3.0')
  const beta = buildConfiguration(base, 'beta', '0.3.0-beta.1')
  const alpha = buildConfiguration(base, 'alpha', '0.3.0-alpha.1')
  assert.equal(stable.identifier, beta.identifier)
  assert.equal(stable.bundle.windows.wix.upgradeCode, 'b6c1d3d9-cb62-5b2a-8c9b-0386470a3947')
  assert.equal(alpha.identifier, stable.identifier)
  assert.equal(alpha.mainBinaryName, stable.mainBinaryName)
  assert.equal(alpha.bundle.windows.wix.upgradeCode, stable.bundle.windows.wix.upgradeCode)
  assert.equal(alpha.app.windows[0].title, 'Tezbar')
  assert.equal(stable.bundle.windows.allowDowngrades, false)
})

test('release promotion follows the channel hierarchy and never rolls it back', () => {
  assert.deepEqual(promotionTargets('stable'), ['stable', 'beta', 'alpha'])
  assert.deepEqual(promotionTargets('beta'), ['beta', 'alpha'])
  assert.deepEqual(promotionTargets('alpha'), ['alpha'])
  const alpha = manifest('0.3.0-alpha.1', 'alpha')
  const beta = manifest('0.3.0-beta.1', 'beta')
  validatePromotion(alpha, beta, 'alpha')
  validatePromotion(beta, manifest(), 'alpha')
  assert.throws(() => validatePromotion(manifest('0.4.0-alpha.1', 'alpha'), beta, 'alpha'))
  assert.throws(() => validatePromotion(manifest(), alpha, 'stable'))
})

test('wrong feeds, identities, assets, and rollbacks are rejected', () => {
  assert.throws(() => validateManifest(manifest('0.4.0-alpha.1', 'alpha'), 'beta'))
  const mixed = manifest()
  mixed.appIdentifier = 'com.tezbar.app.alpha'
  assert.throws(() => validateManifest(mixed, 'stable'))
  const mutable = manifest()
  mutable.platforms['windows-x86_64-nsis'].url =
    'https://github.com/almatkai/Tezbar/releases/latest/download/setup.exe'
  assert.throws(() => validateManifest(mutable, 'stable'))
  assert.throws(() => validatePromotion(manifest('0.3.0'), manifest('0.2.9'), 'stable'))
  assert.throws(() => validatePromotion(manifest('0.3.0'), manifest('0.3.0'), 'stable'))
  validatePromotion(manifest('0.3.0-beta.1', 'beta'), manifest(), 'beta')
})

function signingFixture(bytes) {
  const { publicKey, privateKey } = generateKeyPairSync('ed25519')
  const keyId = Buffer.from('12345678')
  const raw = publicKey.export({ format: 'der', type: 'spki' }).subarray(-32)
  const encodedKey = Buffer.from(
    `untrusted comment: fixture\n${Buffer.concat([Buffer.from('Ed'), keyId, raw]).toString('base64')}`
  ).toString('base64')
  const dataSignature = sign(null, bytes, privateKey)
  const sig = Buffer.concat([Buffer.from('Ed'), keyId, dataSignature])
  const comment = 'timestamp:1 file:fixture.exe'
  const globalSignature = sign(
    null,
    Buffer.concat([dataSignature, Buffer.from(comment)]),
    privateKey
  )
  const encodedSignature = Buffer.from(
    `untrusted comment: fixture\n${sig.toString('base64')}\ntrusted comment: ${comment}\n${globalSignature.toString('base64')}`
  ).toString('base64')
  return { encodedKey, encodedSignature }
}

test('actual signatures and post-verification artifact hashes are enforced', () => {
  const directory = mkdtempSync(join(tmpdir(), 'tezbar-release-test-'))
  const bytes = Buffer.from('signed installer fixture')
  const { encodedKey, encodedSignature } = signingFixture(bytes)
  try {
    verifyUpdaterSignature(bytes, encodedSignature, encodedKey)
    assert.throws(() =>
      verifyUpdaterSignature(Buffer.from('tampered'), encodedSignature, encodedKey)
    )
    const receipt = {
      channel: 'stable',
      version: '0.3.0',
      productName: 'Tezbar',
      appIdentifier: 'com.tezbar.app',
      sourceCommit: 'a'.repeat(40),
      platformVerified: true,
      platform: 'windows-x86_64-nsis',
      artifacts: [
        {
          filename: 'Tezbar_0.3.0_x64-setup.exe',
          sha256: createHash('sha256').update(bytes).digest('hex'),
          updater: true,
        },
      ],
    }
    const path = join(directory, receipt.artifacts[0].filename)
    writeFileSync(path, bytes)
    writeFileSync(`${path}.sig`, encodedSignature)
    validateReceipt(receipt, directory, encodedKey)
    assert.throws(() =>
      validateReceipt({ ...receipt, platformVerified: false }, directory, encodedKey)
    )
    assert.throws(() =>
      manifestFromReceipts([receipt], 'notes', encodedKey, directory, {
        channel: 'stable',
        version: '0.3.0',
        sourceCommit: 'a'.repeat(40),
      })
    )
    writeFileSync(path, 'modified after verification')
    assert.throws(() => validateReceipt(receipt, directory, encodedKey))
    assert.equal(REQUIRED_PLATFORMS.length, 3)
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
})

test('a complete release requires matching signed receipts for every platform', () => {
  const directory = mkdtempSync(join(tmpdir(), 'tezbar-complete-release-'))
  const bytes = Buffer.from('signed cross-platform fixture')
  const { encodedKey, encodedSignature } = signingFixture(bytes)
  const expected = { channel: 'alpha', version: '0.3.0-alpha.1', sourceCommit: 'b'.repeat(40) }
  try {
    const receipts = REQUIRED_PLATFORMS.map((platform) => {
      const suffix =
        platform === 'windows-x86_64-nsis' ? 'x64-setup.exe' : `${platform.slice(7)}.app.tar.gz`
      const filename = `Tezbar_${expected.version}_${suffix}`
      writeFileSync(join(directory, filename), bytes)
      writeFileSync(join(directory, `${filename}.sig`), encodedSignature)
      return {
        ...expected,
        platform,
        productName: 'Tezbar',
        appIdentifier: 'com.tezbar.app',
        platformVerified: true,
        artifacts: [
          { filename, sha256: createHash('sha256').update(bytes).digest('hex'), updater: true },
        ],
      }
    })
    const complete = manifestFromReceipts(
      receipts,
      'Reviewed Alpha notes',
      encodedKey,
      directory,
      expected
    )
    assert.equal(Object.keys(complete.platforms).length, 3)
    assert.equal(complete.sourceCommit, expected.sourceCommit)
    assert.throws(() =>
      manifestFromReceipts([...receipts, receipts[0]], 'notes', encodedKey, directory, expected)
    )
    assert.throws(() =>
      manifestFromReceipts(
        receipts.map((receipt, i) => (i ? receipt : { ...receipt, sourceCommit: 'c'.repeat(40) })),
        'notes',
        encodedKey,
        directory,
        expected
      )
    )
    for (const filename of ['../escape.exe', '..\\escape.exe', '.']) {
      const bad = { ...receipts[0], artifacts: [{ ...receipts[0].artifacts[0], filename }] }
      assert.throws(() => validateReceipt(bad, directory, encodedKey))
    }
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
})
