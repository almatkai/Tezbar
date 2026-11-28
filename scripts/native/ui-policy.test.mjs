import test from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, symlinkSync, realpathSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { checkEntrypoints, checkCargoTree, forbiddenUiCrate, cargoTreeArgs, checkCheckout } from './ui-policy.mjs'

const nativeCommand = 'cargo run --locked --offline -p tezbar-native --bin tezbar'
const nativeScripts = () => Object.fromEntries(['dev', 'build', 'preview', 'release:build'].map(name => [name, nativeCommand]))
const cleanTree = 'tezbar-native v0.1.0 (/fixture/apps/tezbar)\ngpui v0.2.2 (https://example.invalid/gpui#abc)\nserde v1.0.0\n'
function fixture(scripts = {}, files = {}) {
  const reads = []
  const result = checkEntrypoints({ scripts: { ...nativeScripts(), ...scripts } }, {
    readSource(file) {
      reads.push(file)
      if (!(file in files)) throw Object.assign(Error(`Missing fixture ${file}`), { code: 'ENOENT' })
      return files[file]
    },
  })
  return { ...result, reads }
}
const codes = result => result.issues.map(item => item.code)

test('native default routes, package aliases, env assignments and Node build tools are allowed', () => {
  const result = fixture({
    dev: 'TEZBAR_MODE=preview pnpm run native:dev',
    'native:dev': 'node "scripts/native/dev.mjs"',
    build: 'pnpm exec tsx scripts/backend.ts && pnpm run native:build',
    'native:build': 'node scripts/native/build.mjs --release',
    dist: 'npm run build',
    'predev': 'node scripts/check.mjs',
  }, {
    'scripts/native/dev.mjs': 'import "./build.mjs"',
    'scripts/native/build.mjs': 'import {spawnSync} from "node:child_process"; spawnSync("cargo", ["build", "-p", "tezbar-native"]);',
    'scripts/check.mjs': 'console.log("Tauri/Vite renderer migration policy")',
    'scripts/backend.ts': 'import {build} from "esbuild"; await build({entryPoints:{worker:"src/main/extensionWorker/main.ts"},platform:"node"})',
    'src/main/extensionWorker/main.ts': 'import React from "react"; import "./protocol"; export const tree = React.createElement("List");',
    'src/main/extensionWorker/protocol.ts': 'export const renderTarget = "GPUI";',
  })
  assert.deepEqual(result.issues, [])
  assert.ok(result.scripts.includes('native:build'))
  assert.ok(result.sources.includes('src/main/extensionWorker/protocol.ts'))
})

test('legacy default commands fail, including optional distribution/Windows/release routes', () => {
  for (const [name, command] of [
    ['dev', 'tauri dev'], ['build', 'pnpm exec tauri build'], ['preview', 'vite preview --config vite.config.ts'],
    ['release:build', 'node node_modules/@tauri-apps/cli/tauri.js build'],
    ['build:windows', 'npx @tauri-apps/cli build'], ['dist', 'bun x vite build'], ['release', 'yarn exec electron .'],
    ['start', 'node src/renderer/main.js'],
  ]) {
    const result = fixture({ [name]: command })
    assert.ok(codes(result).some(code => code.startsWith('web-')), `${name}: ${command}: ${JSON.stringify(result.issues)}`)
  }
})

test('only the updater signer utility is allowed, never Tauri app commands or imports', () => {
  const cli = 'node_modules/@tauri-apps/cli/tauri.js'
  assert.deepEqual(fixture({ release: `node ${cli} signer sign artifact.tar.gz` }).issues, [])
  const source = action => `
    import {spawnSync} from 'node:child_process'; import path from 'node:path';
    function runTool(program, args) { return spawnSync(program, args) }
    const deps = {run: runTool};
    deps.run(process.execPath, [path.join(plan.root, '${cli}'), ${action}, artifact.file]);
  `
  const check = body => fixture({ release: 'node scripts/native/release.mjs' }, { 'scripts/native/release.mjs': body })
  assert.deepEqual(check(source("'signer', 'sign'")).issues, [])
  for (const action of ["'build'", "'dev'", "'signer', 'build'", "mode, 'sign'"]) {
    assert.ok(check(source(action)).issues.length, action)
  }
  assert.ok(check(source("'signer', 'sign'") + "import '@tauri-apps/api/window';").issues.length)
  assert.ok(fixture({ release: `node --require preload.cjs ${cli} signer sign artifact.tar.gz` }).issues.length)
})

test('aliases, chains and lifecycle hooks cannot smuggle in a legacy renderer', () => {
  for (const scripts of [
    { build: 'pnpm run hidden', hidden: 'vite build' },
    { build: `${nativeCommand} && tauri build` },
    { prebuild: 'tauri dev' },
    { postbuild: 'pnpm exec vite build' },
    { preview: 'npm run alias', alias: 'bun run another', another: 'electron .' },
  ]) assert.ok(codes(fixture(scripts)).includes('web-command'))
})

test('missing, recursive, unsupported or ambiguous default routes fail closed', () => {
  assert.ok(checkEntrypoints({ scripts: {} }).issues.length >= 4)
  for (const scripts of [
    { dev: 'pnpm run loop', loop: 'pnpm run dev' },
    { dev: 'node -e "require(\'vite\')"' },
    { dev: 'node --require hidden.cjs scripts/native/dev.mjs' },
    { build: '$(cat /tmp/command)' }, { build: 'sh -c "cargo build"' },
    { build: 'cargo build --workspace' }, { build: 'cargo build' },
    { build: 'cargo build -p different-app' }, { build: 'cargo build -p tezbar-native -p different-app' },
    { build: 'cargo build -p tezbar-native --features web-shell' }, { build: 'cargo build -p tezbar-native --all-features' },
    { build: 'cargo build --manifest-path=somewhere/Cargo.toml' }, { preview: 'node missing.mjs' },
    { build: 'pnpm --dir somewhere build' }, { dev: 'env -S "tauri dev"' },
  ]) assert.ok(fixture(scripts).issues.length, JSON.stringify(scripts))
})

test('reachable local imports, dynamic imports, child-process aliases and Node subprocess files are inspected', () => {
  for (const badSource of [
    'import "vite";', 'await import("@tauri-apps/api/window");', 'require("electron");',
    'import {spawnSync as launch} from "node:child_process"; launch("vite", ["build"]);',
    'import cp from "node:child_process"; cp.spawn("tauri", ["build"]);',
    'const {spawnSync: launch} = require("node:child_process"); launch("vite", ["build"]);',
    'const cp = require("child_process"); cp.spawn("tauri", ["build"]);',
    'import {spawnSync} from "node:child_process"; const cli="vite"; spawnSync(cli, ["build"]);',
    'import {spawnSync} from "node:child_process"; const run=spawnSync; run("vite", ["build"]);',
    'import {spawnSync} from "node:child_process"; function runTool(command,args){return spawnSync(command,args)}; const deps={run:runTool}; deps.run("tauri",["build"]);',
    'import {spawnSync} from "node:child_process"; import path from "node:path"; function runTool(command,args){return spawnSync(command,args)}; const deps={run:runTool}; deps.run(process.execPath,[path.join(plan.root,"node_modules/@tauri-apps/cli/tauri.js"),"build"]);',
    'import {spawnSync} from "node:child_process"; import {join} from "node:path"; const root="."; const cli=join(root,"node_modules","@tauri-apps","cli","tauri.js"); spawnSync(process.execPath,[cli,"build"]);',
    'import {execSync} from "node:child_process"; execSync("pnpm run old");',
    'import {spawn} from "node:child_process"; spawn("cargo", ["tauri", "build"]);',
    'import {spawnSync} from "node:child_process"; spawnSync(process.execPath,["scripts/legacy.mjs"]);',
    'import {fork} from "node:child_process"; fork("scripts/legacy.mjs");',
    'import {build} from "esbuild"; await build({entryPoints:["src/renderer/main.tsx"],platform:"browser"});',
  ]) {
    const result = fixture({ build: 'node scripts/native/entry.mjs', old: 'vite build' }, {
      'scripts/native/entry.mjs': 'import "./inner.mjs"',
      'scripts/native/inner.mjs': badSource,
      'scripts/legacy.mjs': 'import "react-dom/client"',
    })
    assert.ok(result.issues.some(i => i.code.startsWith('web-')), `${badSource}: ${JSON.stringify(result.issues)}`)
  }
})

test('a worker may serialize React UI for GPUI but may not mount it in a browser', () => {
  const files = {
    'scripts/native/build.mjs': 'import {build} from "esbuild"; build({entryPoints:["src/main/extensionWorker/main.ts"],platform:"node"});',
    'src/main/extensionWorker/main.ts': 'import React from "react"; import Reconciler from "react-reconciler"; import "./desktop";',
    'src/main/extensionWorker/desktop.ts': 'export const BrowserWindow = {hide: () => send({type:"hide"})}; // GPUI IPC facade\n',
  }
  assert.deepEqual(fixture({ build: 'node scripts/native/build.mjs' }, files).issues, [])
  files['src/main/extensionWorker/desktop.ts'] = 'import {createRoot} from "react-dom/client"; createRoot(document.body);'
  assert.ok(codes(fixture({ build: 'node scripts/native/build.mjs' }, files)).includes('web-import'))
})

test('oracles, unrelated scripts/dependencies, comments and resource config strings are not runtime evidence', () => {
  const result = fixture({ build: 'node scripts/native/build.mjs', 'test:legacy': 'vite test' }, {
    'scripts/native/build.mjs': `
      // tauri build; vite; src/renderer; tests/oracles/legacy-renderer
      import {readFileSync} from 'node:fs';
      const policy = readFileSync('src-tauri/macos-signing.json');
      const hint = 'react-dom is prohibited in app UI';
      /cargo:/.exec('metadata');
    `,
    'tests/oracles/legacy-renderer/main.tsx': 'import "react-dom"',
  })
  assert.deepEqual(result.issues, [])
  assert.ok(!result.reads.some(file => file.startsWith('tests/oracles')))
  assert.ok(codes(fixture({ build: 'node tests/oracles/legacy-renderer/main.tsx' })).includes('web-source'))
})

test('data arguments and Node builder metadata do not become UI executables', () => {
  const result = fixture({ build: 'node scripts/native/build.mjs --policy src-tauri/macos-signing.json --label vite' }, {
    'scripts/native/build.mjs': `
      import {spawnSync} from 'node:child_process';
      spawnSync('git', ['show', 'HEAD:src-tauri/tauri.conf.json']);
      spawnSync('/usr/bin/codesign', ['--entitlements', 'src-tauri/entitlements.plist', '/tmp/native.app']);
      spawnSync('echo', ['tauri', 'vite', 'src/renderer']);
    `,
  })
  assert.deepEqual(result.issues, [])
})

test('subprocess shell wrappers and browser-targeted bundles fail', () => {
  for (const source of [
    'import {spawnSync} from "node:child_process"; spawnSync("sh", ["-c", "vite build"]);',
    'import {build} from "esbuild"; build({entryPoints:["src/worker.ts"], platform:"browser"});',
  ]) assert.ok(fixture({ build: 'node entry.mjs' }, { 'entry.mjs': source, 'src/worker.ts': 'export const worker = true' }).issues.length)
})

test('source traversal is bounded and cannot escape the checkout', () => {
  assert.ok(fixture({ build: 'node ../outside.mjs' }).issues.length)
  assert.ok(fixture({ build: 'node large.mjs' }, { 'large.mjs': '// ' + 'a'.repeat(2 * 1024 * 1024) }).issues.length)
  const result = fixture({ build: 'node a.mjs' }, { 'a.mjs': 'import "./b.mjs"', 'b.mjs': 'import "./a.mjs"' })
  assert.deepEqual(result.issues, [])
  assert.equal(result.sources.length, 2)
})

test('normal Cargo graph rejects direct/transitive Tauri, wry, tao and WebView UI families', () => {
  for (const name of ['tauri', 'tauri-runtime-wry', 'tauri-utils', 'tauri_runtime', 'wry', 'tao', 'web-view', 'webview2-com', 'webview2-com-sys', 'webkit2gtk', 'webkit2gtk-sys', 'objc2-web-kit', 'cef', 'dioxus-desktop']) {
    assert.equal(forbiddenUiCrate(name), true, name)
    assert.ok(codes(checkCargoTree(cleanTree + `${name} v1.2.3 (*)\n`)).includes('web-ui-crate'), name)
  }
  for (const name of ['gpui', 'html5ever', 'simplecss', 'resvg', 'webpki-roots', 'raw-window-handle', 'tray-icon', 'tokio']) assert.equal(forbiddenUiCrate(name), false, name)
  assert.deepEqual(checkCargoTree(cleanTree).issues, [])
})

test('malformed/empty/foreign dependency output cannot count as Cargo verification', () => {
  for (const output of ['', 'tauri v2.0.0\n', 'gpui v1.0.0\n', cleanTree + 'warning: missing tree\n']) assert.ok(codes(checkCargoTree(output)).includes('cargo-output'))
})

function checkoutFixture(callback) {
  const root = mkdtempSync(path.join(tmpdir(), 'tezbar-ui-policy-'))
  try {
    writeFileSync(path.join(root, 'package.json'), JSON.stringify({ scripts: nativeScripts(), devDependencies: { vite: 'fixture', typescript: 'fixture' } }))
    callback(root)
  } finally { rmSync(root, { recursive: true, force: true }) }
}

test('checkout inspection uses only locked offline normal Cargo tree, with explicit cross-platform coverage', () => {
  checkoutFixture(root => {
    for (const target of ['all', 'host', 'x86_64-pc-windows-msvc']) {
      let count = 0
      const result = checkCheckout({ root, target, run(command, args, options) {
        count++
        assert.equal(command, 'cargo')
        assert.deepEqual(args, cargoTreeArgs(target))
        for (const flag of ['--locked', '--offline', '--edges', '--package', '--features']) assert.ok(args.includes(flag))
        assert.equal(args[args.indexOf('--edges') + 1], 'normal')
        assert.equal(args[args.indexOf('--features') + 1], 'runtime-shaders')
        assert.equal(options.env.CARGO_NET_OFFLINE, 'true')
        assert.equal(options.env.RUSTUP_AUTO_INSTALL, '0')
        assert.equal(options.cwd, realpathSync(root))
        assert.ok(options.timeout <= 60_000)
        return { status: 0, stdout: cleanTree }
      } })
      assert.equal(count, 1)
      assert.deepEqual(result.issues, [])
      assert.ok(result.evidence.some(text => text.includes(`(${target},`)))
    }
  })
})

test('offline missing cache, lock drift or tool failure fails without network fallback; source findings survive', () => {
  checkoutFixture(root => {
    writeFileSync(path.join(root, 'package.json'), JSON.stringify({ scripts: { ...nativeScripts(), dev: 'tauri dev' } }))
    for (const error of [
      { status: 101, stderr: 'attempting to make an HTTP request, but --offline was specified' },
      { status: 101, stderr: 'lock file needs to be updated but --locked was passed' },
      { status: null, error: Error('spawnSync cargo ENOENT') },
    ]) {
      let calls = 0
      const result = checkCheckout({ root, run() { calls++; return error } })
      assert.equal(calls, 1)
      assert.ok(codes(result).includes('web-command'))
      assert.ok(codes(result).includes('cargo-unavailable'))
      assert.ok(!result.evidence.some(text => text.includes('dependency graph')))
    }
  })
})

test('checkout source reader refuses symlink escape', () => {
  checkoutFixture(root => {
    const outside = mkdtempSync(path.join(tmpdir(), 'tezbar-ui-policy-outside-'))
    try {
      writeFileSync(path.join(outside, 'entry.mjs'), '// outside checkout')
      mkdirSync(path.join(root, 'scripts'))
      symlinkSync(path.join(outside, 'entry.mjs'), path.join(root, 'scripts/entry.mjs'))
      writeFileSync(path.join(root, 'package.json'), JSON.stringify({ scripts: { ...nativeScripts(), build: 'node scripts/entry.mjs' } }))
      const result = checkCheckout({ root, run: () => ({ status: 0, stdout: cleanTree }) })
      assert.ok(result.issues.some(item => /symlink escapes/.test(item.message)))
    } finally { rmSync(outside, { recursive: true, force: true }) }
  })
})
