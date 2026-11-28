import test from 'node:test'
import assert from 'node:assert/strict'
import * as fs from 'node:fs/promises'
import path from 'node:path'
import {tmpdir} from 'node:os'
import {EventEmitter} from 'node:events'
import {nativeDev, parseDev, cargoCommand, currentSource, previewEnvironment, launchNative, verifyToolchain} from './dev.mjs'
import {PROFILE_MARKER, assertResourceTree} from './dev-resources.mjs'
import {artifactCollector} from './cargo-artifact.mjs'
import {metadataRecord, GPUI_REV} from './build-metadata.mjs'

const SHA = 'a'.repeat(40)
const buildMetadata = overrides => ({schema:2, version:'1.0.0', sourceCommit:SHA, dirty:true, benchControl:false, runtimeShaders:true, gpuiRev:GPUI_REV, rustVersion:'1.98.1', ...overrides})
async function fixture(t, overrides = {}) {
  const base = await fs.mkdtemp(path.join(await fs.realpath(tmpdir()), 'tezbar-dev-test-'))
  t.after(() => fs.rm(base, {recursive:true, force:true}))
  const root = path.join(base, 'workspace with spaces'), osTemp = path.join(base, 'os temp')
  await fs.mkdir(path.join(root, 'resources/icons'), {recursive:true})
  await fs.mkdir(path.join(root, 'apps/tezbar'), {recursive:true})
  await fs.mkdir(osTemp)
  await fs.writeFile(path.join(root, 'resources/icons/icon.png'), 'UI icon')
  await fs.writeFile(path.join(root, 'resources/trayIconTemplate@2x.png'), 'tray')
  await fs.writeFile(path.join(root, 'apps/tezbar/Cargo.toml'), '[package]\nname="tezbar-native"\n')
  const executable = path.join(root, 'custom cargo target tezbar.exe')
  await fs.writeFile(executable, metadataRecord(buildMetadata(overrides)))
  const calls = [], logs = []
  const dependencies = {root, osTemp, io:fs, env:{HOME:'/never-use-as-preview-home', PATH:process.env.PATH,
    TEZBAR_PREVIEW_USER_HOME:'1', TEZBAR_NATIVE_SOURCE_SHA:'b'.repeat(40)},
    source:async () => SHA, toolchain:options => { calls.push({toolchain:options}) },
    log:text => logs.push(text),
    cargo:async (command, args, options) => {
      calls.push({command, args, options})
      // Exercise the shared selector with Cargo-shaped messages and a custom target.
      const collector = artifactCollector(options.manifest, {check:options.check})
      collector.accept({reason:'compiler-artifact', target:{name:'tezbar', kind:['bin']},
        profile:{test:false}, manifest_path:options.manifest, executable})
      return collector.finish()
    },
    launch:async (binary, args, options) => {calls.push({binary, args, options}); return {exitCode:0, signal:null}},
  }
  return {root, osTemp, executable, calls, logs, dependencies}
}

const readOnlyFs = () => new Proxy(fs, {get(target, key) {
  if (['mkdtemp','mkdir','writeFile','cp','rm'].includes(key)) return () => {throw Error(`Unexpected fs mutation: ${key}`)}
  return target[key]
}})

test('strict argument parsing, mode conflicts, no arbitrary app/profile flags', () => {
  assert.equal(parseDev([]).theme, 'system')
  assert.equal(parseDev(['--minimal','--release','--theme','dark']).release, true)
  assert.equal(parseDev(['--check','--command']).check, true)
  for (const args of [['--resources'], ['--resources','--check'], ['--theme','white'], ['--theme',''],
    ['--release','--release'], ['--help','--check'], ['--resources','/stage','--minimal'],
    ['--resources','/stage','--ui-resources','assets'], ['--profile','/real-user'], ['--show'],
    ['--allow-desktop-integration'], ['--launcher-shortcut','Alt+Space'], ['--home','/real-user'],
    ['--control-token-file','token'], ['--'], ['run'], ['--ui-resources','bad\0path']]) {
    assert.throws(() => parseDev(args), undefined, JSON.stringify(args))
  }
})

test('fixed native Cargo command, debug and release/check profiles', () => {
  assert.deepEqual(cargoCommand(parseDev([])), {command:'cargo', args:['+1.98.1','build','--locked','--offline','-j2','-p','tezbar-native','--features','runtime-shaders','--message-format=json']})
  const check = cargoCommand(parseDev(['--release','--check']))
  assert.equal(check.args[1], 'check')
  assert.equal(check.args.at(-1), '--release')
})

test('help is side effect free, including no source or toolchain calls', async () => {
  let output
  const result = await nativeDev(['--help'], {source:() => {throw Error('must not call')}, io:readOnlyFs(), log:text => {output = text}})
  assert.equal(result.exitCode, 0)
  assert.match(output, /minimal UI-only mode/)
  assert.match(output, /No browser, Vite, Tauri, WebView/)
})

test('command plan performs read-only validation, no subprocess build/launch or profile', async t => {
  const f = await fixture(t)
  await nativeDev(['--command','--release'], {...f.dependencies, io:readOnlyFs()})
  const plan = JSON.parse(f.logs[0])
  assert.equal(plan.mode, 'minimal-ui-only')
  assert.equal(plan.publishable, false)
  assert.equal(plan.build.args.at(-1), '--release')
  assert.ok(plan.launch.args.includes('--show'))
  assert.deepEqual(f.calls, [])
  assert.deepEqual(await fs.readdir(f.osTemp), [])
})

test('command/check prints no launch command and performs no build or allocation', async t => {
  const f = await fixture(t)
  await nativeDev(['--command','--check'], {...f.dependencies, io:readOnlyFs()})
  const plan = JSON.parse(f.logs[0])
  assert.equal(plan.build.args[1], 'check')
  assert.equal(plan.launch, null)
  assert.deepEqual(f.calls, [])
})

test('check builds no executable, profile, resources or GUI', async t => {
  const f = await fixture(t)
  assert.deepEqual(await nativeDev(['--check'], {...f.dependencies, io:readOnlyFs()}), {exitCode:0})
  assert.equal(f.calls.length, 2)
  assert.equal(f.calls[1].args[1], 'check')
  assert.equal(f.calls[1].options.check, true)
  assert.deepEqual(await fs.readdir(f.osTemp), [])
})

test('default is a real Cargo-selected native launch with marked isolated profile/UI resources and cleanup', async t => {
  const f = await fixture(t)
  let profile
  const launch = async (binary, args, options) => {
    assert.equal(binary, f.executable)
    assert.deepEqual(args.filter((_, i) => i % 2 === 0).slice(0, 3), ['--profile','--resources','--show'])
    profile = args[1]
    const resources = args[3]
    assert.equal(path.dirname(profile), path.dirname(resources))
    assert.equal(await fs.readFile(path.join(profile, '.tezbar-native-preview'), 'utf8'), PROFILE_MARKER)
    assert.deepEqual(await fs.readdir(profile), ['.tezbar-native-preview'])
    assert.equal(await fs.readFile(path.join(resources, '.tezbar-native-source'), 'utf8'), SHA + '\n')
    assert.equal(await fs.readFile(path.join(resources, 'dist-backend/resources/icons/icon.png'), 'utf8'), 'UI icon')
    assert.equal(await fs.readFile(path.join(resources, 'trayIconTemplate@2x.png'), 'utf8'), 'tray')
    await assert.rejects(fs.stat(path.join(resources, '.tezbar-native-production.json')), {code:'ENOENT'})
    assert.equal(options.env.TEZBAR_PREVIEW_USER_HOME, undefined)
    assert.equal(options.env.TEZBAR_NATIVE_SOURCE_SHA, undefined)
    assert.equal(options.env.HOME, '/never-use-as-preview-home') // Cargo caches preserved; CLI uses profile/home.
    assert.ok(!args.includes('--home') && !args.includes('--allow-desktop-integration'))
    return {exitCode:7}
  }
  assert.deepEqual(await nativeDev(['--theme','dark'], {...f.dependencies, launch}), {exitCode:7})
  assert.match(f.logs.join('\n'), /LOCAL MINIMAL UI-ONLY/)
  assert.match(f.logs.join('\n'), /Build dirty: true/)
  await assert.rejects(fs.stat(profile), {code:'ENOENT'})
  assert.deepEqual(await fs.readdir(f.osTemp), [])
})

test('every run allocates a new profile; exceptions also clean only owned temporary stage', async t => {
  const f = await fixture(t), profiles = []
  await fs.writeFile(path.join(f.osTemp, 'unrelated-file'), 'preserve')
  for (let i = 0; i < 2; i++) await assert.rejects(nativeDev([], {...f.dependencies, launch:async (_, args) => {
    profiles.push(args[1]); throw Error('GUI unavailable')
  }}), /GUI unavailable/)
  assert.notEqual(profiles[0], profiles[1])
  assert.deepEqual(await fs.readdir(f.osTemp), ['unrelated-file'])
})

test('prepared resources are explicit, source-matched, used untouched and never silently downgraded', async t => {
  const f = await fixture(t), prepared = path.join(f.root, 'prepared resources')
  await fs.mkdir(prepared)
  await fs.writeFile(path.join(prepared, '.tezbar-native-source'), SHA + '\n')
  await fs.writeFile(path.join(prepared, 'worker-fixture'), 'caller input')
  await nativeDev(['--resources',prepared], {...f.dependencies, launch:async (_, args) => {
    assert.equal(args[3], await fs.realpath(prepared)); return {exitCode:0}
  }})
  assert.match(f.logs[0], /not production-verified/)
  assert.equal(await fs.readFile(path.join(prepared, 'worker-fixture'), 'utf8'), 'caller input')
  assert.deepEqual((await fs.readdir(prepared)).sort(), ['.tezbar-native-source','worker-fixture'])
  await fs.writeFile(path.join(prepared, '.tezbar-native-source'), 'b'.repeat(40))
  await assert.rejects(nativeDev(['--resources',prepared], f.dependencies), /differs from current Git HEAD/)
  await assert.rejects(nativeDev(['--resources','missing'], f.dependencies), /inputs unavailable/)
})

test('resource relocation requires explicit UI root, not a legacy backend path', async t => {
  const f = await fixture(t)
  await fs.rename(path.join(f.root, 'resources'), path.join(f.root, 'native-ui-assets'))
  await assert.rejects(nativeDev(['--command'], f.dependencies), /inputs unavailable/)
  await nativeDev(['--minimal','--ui-resources','native-ui-assets'], f.dependencies)
  const outside = path.join(f.osTemp, 'foreign-assets')
  await fs.cp(path.join(f.root, 'native-ui-assets'), outside, {recursive:true})
  await assert.rejects(nativeDev(['--ui-resources',outside], f.dependencies), /inside this source workspace/)
})

test('missing UI files fail before build; minimal mode never fabricates assets', async t => {
  const f = await fixture(t)
  await fs.rm(path.join(f.root, 'resources/trayIconTemplate@2x.png'))
  await assert.rejects(nativeDev([], f.dependencies), /inputs unavailable/)
  assert.deepEqual(f.calls, [])
  assert.deepEqual(await fs.readdir(f.osTemp), [])
})

test('symlinked roots/children/source markers fail closed', async t => {
  const f = await fixture(t)
  try {
    await fs.symlink(path.join(f.root, 'resources'), path.join(f.root, 'ui-link'), process.platform === 'win32' ? 'junction' : 'dir')
  } catch (error) {
    if (['EPERM','EACCES'].includes(error.code)) {t.skip('OS disallows fixture symlinks'); return}
    throw error
  }
  await assert.rejects(nativeDev(['--ui-resources','ui-link'], f.dependencies), /not a symlink/)
  await fs.symlink(path.join(f.root, 'resources/icons'), path.join(f.root, 'resources/nested-link'), process.platform === 'win32' ? 'junction' : 'dir')
  await assert.rejects(nativeDev([], f.dependencies), /Symlink in preview resources/)
  await fs.rm(path.join(f.root, 'resources/nested-link'))
  const staged = path.join(f.root, 'staged')
  await fs.mkdir(staged)
  await fs.writeFile(path.join(f.root, 'source-marker'), SHA)
  try { await fs.symlink(path.join(f.root, 'source-marker'), path.join(staged, '.tezbar-native-source')) }
  catch (error) {if (['EPERM','EACCES'].includes(error.code)) return; throw error}
  await assert.rejects(nativeDev(['--resources',staged], f.dependencies), /Symlink in preview resources/)
})

test('copy failure cleans partially staged profile and does not launch', async t => {
  const f = await fixture(t)
  const io = {...fs, cp:async () => {throw Error('fixture copy error')}}
  await assert.rejects(nativeDev([], {...f.dependencies, io}), /fixture copy error/)
  assert.deepEqual(await fs.readdir(f.osTemp), [])
  assert.ok(!f.calls.some(call => call.binary))
})

test('Cargo errors never create a profile or run stale executables', async t => {
  const f = await fixture(t)
  await assert.rejects(nativeDev([], {...f.dependencies, cargo:async () => {throw Error('offline cache missing')}}), /offline cache missing/)
  assert.deepEqual(await fs.readdir(f.osTemp), [])
  assert.ok(!f.calls.some(call => call.binary))
})

test('HEAD changes during build and stale prepared inputs after build prevent launch', async t => {
  const f = await fixture(t)
  let reads = 0
  await assert.rejects(nativeDev([], {...f.dependencies, source:async () => ++reads === 1 ? SHA : 'b'.repeat(40)}), /HEAD changed/)
  const prepared = path.join(f.root, 'prepared')
  await fs.mkdir(prepared)
  await fs.writeFile(path.join(prepared, '.tezbar-native-source'), SHA)
  await assert.rejects(nativeDev(['--resources',prepared], {...f.dependencies, cargo:async () => {
    await fs.writeFile(path.join(prepared, '.tezbar-native-source'), 'b'.repeat(40)); return f.executable
  }}), /source differs/)
  assert.deepEqual(await fs.readdir(f.osTemp), [])
})

test('HEAD change during resource preparation cleans stage and never launches', async t => {
  const f = await fixture(t)
  let reads = 0
  await assert.rejects(nativeDev([], {...f.dependencies, source:async () => ++reads < 3 ? SHA : 'b'.repeat(40)}), /HEAD changed during preview preparation/)
  assert.ok(!f.calls.some(call => call.binary))
  assert.deepEqual(await fs.readdir(f.osTemp), [])
})

test('reject stale, bench-control, non-runtime-shaders and wrong-policy binaries', async t => {
  for (const metadata of [{sourceCommit:'b'.repeat(40)}, {benchControl:true}, {runtimeShaders:false}, {rustVersion:'1.97.0'}, {gpuiRev:'b'.repeat(40)}]) {
    const f = await fixture(t, metadata)
    await assert.rejects(nativeDev([], f.dependencies), /metadata|requirements/)
    assert.deepEqual(await fs.readdir(f.osTemp), [])
  }
})

test('Git identity uses explicit workspace/argv, rejects failed commands and foreign repo', async t => {
  const f = await fixture(t), calls = []
  const run = (command, args, options) => {
    calls.push({command, args, options})
    return {status:0, stdout:args.includes('HEAD') ? SHA + '\n' : args.includes('--show-toplevel') ? f.root + '\n' : 'apps/tezbar/build.rs\n'}
  }
  assert.equal(await currentSource(f.root, {run}), SHA)
  assert.ok(calls.every(call => call.command === 'git' && call.options.cwd === f.root && call.args[0] === '--no-optional-locks'))
  await assert.rejects(currentSource(f.root, {run:() => ({status:1, stderr:'Git unavailable'})}), /Git identity/)
  await assert.rejects(currentSource(f.root, {run:() => ({status:0, stdout:f.osTemp})}), /differs from Git root/)
  await assert.rejects(currentSource(f.root, {run:(c, args, opts) => args.includes('HEAD') ? {status:0, stdout:'fake'} : run(c,args,opts)}), /Invalid source/)
})

test('toolchain preflight cannot opt into install and fails closed without cached exact Rust', () => {
  verifyToolchain({run:(command, args) => {
    assert.equal(command, 'rustup')
    assert.deepEqual(args, ['run','1.98.1','rustc','--version'])
    return {status:0, stdout:'rustc 1.98.1 (fixture)\n'}
  }})
  for (const result of [{status:1}, {error:Error('not found')}, {status:0, stdout:'rustc 1.98.10'}, {status:0, stdout:'rustc 1.97.0'}])
    assert.throws(() => verifyToolchain({run:() => result}), /never installs\/downloads/)
})

test('preflight failure performs no Cargo build or profile creation', async t => {
  const f = await fixture(t)
  await assert.rejects(nativeDev([], {...f.dependencies, toolchain:() => {throw Error('missing Rust')}}), /missing Rust/)
  assert.deepEqual(f.calls, [])
  assert.deepEqual(await fs.readdir(f.osTemp), [])
})

test('environment guards are case-insensitive on Windows too, other toolchain variables survive', () => {
  assert.deepEqual(previewEnvironment({tezbar_preview_user_home:'1', Tezbar_Native_Source_Sha:'fake', CARGO_TARGET_DIR:'custom', PATH:'path'}), {CARGO_TARGET_DIR:'custom', PATH:'path'})
})

test('subprocess fixture: literal spaced Windows executable/argv, wait, signal forwarding and handler cleanup', async () => {
  const signals = new EventEmitter(), child = new EventEmitter(), killed = []
  child.kill = signal => {killed.push(signal); return true}
  const args = ['--profile','C:\\OS Temp\\profile','--resources','C:\\fixtures with spaces\\resources','--show']
  const completion = launchNative('C:\\target with spaces\\tezbar.exe', args, {cwd:'C:\\workspace', env:{PATH:'fixture'}, signals,
    start:(binary, argv, options) => {
      assert.equal(binary, 'C:\\target with spaces\\tezbar.exe')
      assert.deepEqual(argv, args)
      assert.equal(options.shell, false)
      assert.equal(options.stdio, 'inherit')
      return child
    }})
  signals.emit('SIGINT'); signals.emit('SIGTERM')
  assert.deepEqual(killed, ['SIGINT','SIGTERM'])
  child.emit('close', null, 'SIGINT')
  assert.deepEqual(await completion, {exitCode:130, signal:'SIGINT'})
  assert.equal(signals.listenerCount('SIGINT'), 0)
  assert.equal(signals.listenerCount('SIGTERM'), 0)
})

test('spawn failure returns an error and removes termination listeners', async () => {
  const signals = new EventEmitter(), child = new EventEmitter()
  const completion = launchNative('/fixture/tezbar', [], {signals, start:() => child})
  child.emit('error', Error('spawn ENOENT'))
  await assert.rejects(completion, /ENOENT/)
  assert.equal(signals.listenerCount('SIGINT'), 0)
  assert.equal(signals.listenerCount('SIGTERM'), 0)
})
