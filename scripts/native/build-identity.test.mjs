import test from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'
import { GPUI_REV, RUST_VERSION } from './build-metadata.mjs'

test('native build identity observes compiler and resolved GPUI instead of policy labels', () => {
  const root = mkdtempSync(join(tmpdir(), 'tezbar-observed-identity-'))
  try {
    const manifest = join(root, 'apps/tezbar'), out = join(root, 'out')
    mkdirSync(manifest, { recursive: true }); mkdirSync(out)
    writeFileSync(join(root, 'notes.md'), 'Fixture release notes\n')
    writeFileSync(join(root, 'rust-toolchain.toml'), `[toolchain]\nchannel = "${RUST_VERSION}"\n`)
    writeFileSync(join(manifest, 'Cargo.toml'), `[dependencies]\ngpui = { git = "https://github.com/zed-industries/zed", rev = "${GPUI_REV}" }\n`)
    const lock = rev => writeFileSync(join(root, 'Cargo.lock'), `version = 4\n[[package]]\nname = "gpui"\nversion = "0.2.2"\nsource = "git+https://github.com/zed-industries/zed?rev=${rev}#${rev}"\n`)
    lock(GPUI_REV)
    const run = (exe, args, env = {}) => {
      const r = spawnSync(exe, args, { cwd: root, encoding: 'utf8', env: { ...process.env, ...env } })
      assert.equal(r.status, 0, r.stderr); return r.stdout
    }
    const build = join(root, process.platform === 'win32' ? 'build.exe' : 'build')
    const compiler = join(root, process.platform === 'win32' ? 'compiler.exe' : 'compiler')
    run('rustc', ['+1.98.1', '--edition=2024', fileURLToPath(new URL('../../apps/tezbar/build.rs', import.meta.url)), '-o', build])
    writeFileSync(join(root, 'compiler.rs'), 'fn main(){println!("commit-hash: fixture\\nrelease: {}",std::env::var("FIXTURE_RUST_VERSION").unwrap());}')
    run('rustc', ['+1.98.1', join(root, 'compiler.rs'), '-o', compiler])
    const identity = release => {
      const stdout = run(build, [], { CARGO_MANIFEST_DIR: manifest, OUT_DIR: out, CARGO_PKG_VERSION: '0.0.0', RUSTC: compiler, FIXTURE_RUST_VERSION: release, TEZBAR_RELEASE_NOTES_PATH: join(root, 'notes.md'), TEZBAR_NATIVE_SOURCE_SHA: 'a'.repeat(40) })
      return Object.fromEntries(stdout.split('\n').filter(l => l.startsWith('cargo:rustc-env=')).map(l => { const [key, ...value] = l.slice('cargo:rustc-env='.length).split('='); return [key, value.join('=')] }))
    }
    let result = identity(RUST_VERSION)
    assert.equal(result.TEZBAR_NATIVE_RUST_VERSION, RUST_VERSION)
    assert.equal(result.TEZBAR_NATIVE_GPUI_REV, GPUI_REV)
    assert.equal(result.TEZBAR_NATIVE_IDENTITY_ATTESTED, 'true')
    result = identity('1.98.0')
    assert.equal(result.TEZBAR_NATIVE_RUST_VERSION, '1.98.0')
    assert.equal(result.TEZBAR_NATIVE_IDENTITY_ATTESTED, 'false')
    assert.equal(result.TEZBAR_NATIVE_DIRTY, 'true')
    lock('b'.repeat(40))
    result = identity(RUST_VERSION)
    assert.equal(result.TEZBAR_NATIVE_GPUI_REV, 'b'.repeat(40))
    assert.equal(result.TEZBAR_NATIVE_IDENTITY_ATTESTED, 'false')
    assert.equal(result.TEZBAR_NATIVE_DIRTY, 'true')
  } finally { rmSync(root, { recursive: true, force: true }) }
})
