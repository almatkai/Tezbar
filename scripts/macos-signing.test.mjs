import test from 'node:test'
import assert from 'node:assert/strict'
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  macOSSigningIdentity,
  validateMacOSSigning,
  verifyMacOSSigning,
  signMacOSResources,
  verifyMacOSResources,
} from './lib/macos-signing.mjs'

const policy = JSON.parse(
  readFileSync(new URL('../src-tauri/macos-signing.json', import.meta.url), 'utf8')
)
const metadata = (hash = 'a'.repeat(40)) => `Identifier=${policy.identifier}
Authority=Apple Development: fixture
TeamIdentifier=${policy.teamIdentifier}
CDHash=${hash}
designated => ${policy.designatedRequirement}
`

test('certificate identity stays fixed while build hashes change', () => {
  const first = validateMacOSSigning(metadata(), policy)
  const next = validateMacOSSigning(metadata('b'.repeat(40)), policy)
  assert.notEqual(first.codeHash, next.codeHash)
  assert.equal(first.designatedRequirement, next.designatedRequirement)
  assert.equal(next.identityContinuityVerified, true)
})

test('ad-hoc, different-team, different-app, and changed requirements fail closed', () => {
  for (const changed of [
    metadata() + 'Signature=adhoc\n',
    metadata().replace(policy.teamIdentifier, 'OTHERTEAM1'),
    metadata().replace('Identifier=com.tezbar.app', 'Identifier=com.other.app'),
    metadata().replace(policy.designatedRequirement, 'cdhash H"' + 'a'.repeat(40) + '"'),
    metadata().replace(/^Authority=.*\n/m, ''),
  ])
    assert.throws(() => validateMacOSSigning(changed, policy), /identity changed or is ad-hoc/)
  assert.throws(() => macOSSigningIdentity({ identity: '-' }), /persistent signing certificate/)
  assert.throws(() => macOSSigningIdentity({}), /persistent signing certificate/)
  assert.equal(macOSSigningIdentity({ localPreview: true }), '-')
  const weak = 'identifier "com.tezbar.app"'
  assert.throws(
    () =>
      validateMacOSSigning(metadata().replace(policy.designatedRequirement, weak), {
        ...policy,
        designatedRequirement: weak,
      }),
    /identity changed or is ad-hoc/
  )
})

test(
  'beta build preview preserves certificate signing even with --allow-unsigned',
  {
    skip: process.platform !== 'darwin',
  },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'tezbar-signing-policy-'))
    try {
      execFileSync(process.execPath, [
        'scripts/build-release.mjs',
        '--channel',
        'beta',
        '--version',
        '0.3.0-beta.3',
        '--notes',
        'docs/releases/0.3.0-beta.3.md',
        '--allow-unsigned',
        '--dry-run',
        '--output',
        directory,
      ])
      const config = JSON.parse(readFileSync(join(directory, 'tauri-release.json'), 'utf8'))
      assert.notEqual(
        config.bundle.macOS.signingIdentity,
        '-',
        'A distributed beta must retain a certificate identity across updates'
      )
    } finally {
      rmSync(directory, { recursive: true, force: true })
    }
  }
)

test(
  'distributed macOS builds reject an explicit ad-hoc override before building',
  {
    skip: process.platform !== 'darwin',
  },
  () => {
    assert.throws(
      () =>
        execFileSync(
          process.execPath,
          [
            'scripts/build-release.mjs',
            '--channel',
            'beta',
            '--version',
            '0.3.0-beta.3',
            '--allow-unsigned',
            '--dry-run',
          ],
          { env: { ...process.env, APPLE_SIGNING_IDENTITY: '-' }, stdio: 'pipe' }
        ),
      /persistent signing certificate/
    )
  }
)

test(
  'two real certificate-signed builds satisfy the pinned identity, while ad-hoc does not',
  {
    skip: process.platform !== 'darwin' || !process.env.TEZBAR_SIGNING_TEST_IDENTITY,
  },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'tezbar-signing-native-'))
    try {
      const builds = []
      for (const version of [1, 2]) {
        const source = join(directory, `build-${version}.c`)
        const binary = join(directory, `build-${version}`)
        writeFileSync(
          source,
          `#include <stdio.h>\nint main(void){puts("build ${version}");return 0;}\n`
        )
        execFileSync('clang', [source, '-o', binary])
        execFileSync('codesign', [
          '--force',
          '--sign',
          process.env.TEZBAR_SIGNING_TEST_IDENTITY,
          '--identifier',
          policy.identifier,
          '--options',
          'runtime',
          '--timestamp=none',
          binary,
        ])
        builds.push(verifyMacOSSigning(binary, policy))
      }
      assert.notEqual(builds[0].codeHash, builds[1].codeHash)
      assert.equal(builds[0].designatedRequirement, builds[1].designatedRequirement)
      const binary = join(directory, 'build-2')
      execFileSync('codesign', ['--force', '--sign', '-', binary])
      assert.throws(() => verifyMacOSSigning(binary, policy), /identity changed or is ad-hoc/)
    } finally {
      rmSync(directory, { recursive: true, force: true })
    }
  }
)

test(
  'a hardened certificate-signed process loads extensionless native resources from the same team',
  {
    skip: process.platform !== 'darwin' || !process.env.TEZBAR_SIGNING_TEST_IDENTITY,
  },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'tezbar-library-signing-'))
    try {
      const resources = join(directory, 'resources')
      mkdirSync(resources)
      const library = join(resources, 'vec0')
      const source = join(directory, 'library.c')
      writeFileSync(source, 'int fixture(void){return 42;}\n')
      execFileSync('clang', ['-dynamiclib', source, '-o', library])
      const loaderSource = join(directory, 'loader.c')
      const loader = join(directory, 'loader')
      writeFileSync(
        loaderSource,
        '#include <dlfcn.h>\n#include <stdio.h>\nint main(int argc,char**argv){void*h=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);if(!h){puts(dlerror());return 1;}dlclose(h);return 0;}\n'
      )
      execFileSync('clang', [loaderSource, '-o', loader])
      execFileSync('codesign', [
        '--force',
        '--sign',
        process.env.TEZBAR_SIGNING_TEST_IDENTITY,
        '--identifier',
        policy.identifier,
        '--options',
        'runtime',
        '--timestamp=none',
        loader,
      ])
      const before = spawnSync(loader, [library], { encoding: 'utf8' })
      assert.notEqual(before.status, 0)
      assert.match(before.stdout + before.stderr, /Team ID|mapped file|code signature/)
      assert.throws(
        () => verifyMacOSResources(resources, policy),
        /must share the app signing team/
      )
      const signed = signMacOSResources(resources, process.env.TEZBAR_SIGNING_TEST_IDENTITY, policy)
      assert.deepEqual(signed, [{ path: 'vec0', teamIdentifier: policy.teamIdentifier }])
      const after = spawnSync(loader, [library], { encoding: 'utf8' })
      assert.equal(after.status, 0, after.stdout + after.stderr)
    } finally {
      rmSync(directory, { recursive: true, force: true })
    }
  }
)
