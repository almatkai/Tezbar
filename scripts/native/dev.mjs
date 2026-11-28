// GPUI local development entrypoint. Importing this module never starts a process.
import {spawn, spawnSync} from 'node:child_process'
import * as fs from 'node:fs/promises'
import path from 'node:path'
import {fileURLToPath} from 'node:url'
import {tmpdir, constants} from 'node:os'
import {runCargo} from './cargo-artifact.mjs'
import {captureBuildMetadata} from './build-metadata.mjs'
import {preparePreview, validateResources} from './dev-resources.mjs'

const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..')
export const HELP = `Usage: node scripts/native/dev.mjs [options]
Build and run the native GPUI app in a fresh, marked OS-temp preview profile.
No browser, Vite, Tauri, WebView, signing, installation or production launch.

  --help                   Show this help; no build or launch
  --check                  Cargo check only; no executable/profile/GUI
  --command                Print a JSON command plan; no build/profile/GUI
  --release                Use Cargo's release profile (still LOCAL preview)
  --minimal                Explicit local minimal UI-only mode (also the default)
  --ui-resources <dir>      UI input root inside this workspace (default: resources)
                           Requires icons/ and trayIconTemplate@2x.png
  --resources <dir>         Prepared native-layout resources; requires a matching
                           .tezbar-native-source Git HEAD marker, no symlinks
  --theme system|light|dark Preview theme (default: system)

No arbitrary app flags or existing user profiles are accepted. Desktop integration,
launcher hotkeys and real user HOME opt-ins stay disabled. Each run loses its profile
on exit. Minimal mode has UI assets only: extension workers, native helpers, vector
SQLite and embedding runtime are NOT staged; dependent features are unavailable.
Prepared resources are caller-provided local inputs, NOT verified production assets.
Offline Rust 1.98.1 and cached Cargo dependencies are prerequisites; missing inputs
fail rather than download, build web assets, or silently fall back to another UI.
`

export function parseDev(args) {
  const options = {help:false, check:false, command:false, release:false, minimal:false, resources:null, uiResources:null, theme:'system'}
  const seen = new Set()
  for (let i = 0; i < args.length; i++) {
    const flag = args[i]
    if (seen.has(flag)) throw Error(`Duplicate native dev argument: ${flag}`)
    seen.add(flag)
    if (['--help','--check','--command','--release','--minimal'].includes(flag)) options[flag.slice(2)] = true
    else if (['--resources','--ui-resources','--theme'].includes(flag)) {
      const value = args[++i]
      if (!value || value.startsWith('-') || value.includes('\0')) throw Error(`Value required for ${flag}`)
      options[flag === '--ui-resources' ? 'uiResources' : flag.slice(2)] = value
    } else throw Error(`Unknown native dev argument: ${flag}; use --help`)
  }
  if (!['system','light','dark'].includes(options.theme)) throw Error('Theme must be system, light or dark')
  if (options.resources && (options.minimal || options.uiResources)) throw Error('--resources conflicts with --minimal/--ui-resources')
  if (options.help && args.length !== 1) throw Error('--help must be used alone')
  return options
}

export function cargoCommand(options) {
  const args = ['+1.98.1', options.check ? 'check' : 'build', '--locked', '--offline', '-j2', '-p', 'tezbar-native', '--features', 'runtime-shaders', '--message-format=json']
  if (options.release) args.push('--release')
  return {command:'cargo', args}
}

// Don't let inherited environment silently turn a preview into real-home access,
// or let the build embed an overridden SHA. Keep HOME/CARGO_HOME for offline caches.
export function previewEnvironment(environment) {
  return Object.fromEntries(Object.entries(environment).filter(([key]) =>
    !['TEZBAR_PREVIEW_USER_HOME', 'TEZBAR_NATIVE_SOURCE_SHA'].includes(key.toUpperCase())))
}

// `rustup run` without --install fails for an absent toolchain instead of fetching it.
export function verifyToolchain({run = spawnSync, env = process.env, cwd} = {}) {
  const result = run('rustup', ['run','1.98.1','rustc','--version'], {cwd, env, encoding:'utf8', windowsHide:true})
  if (result.error || result.status !== 0 || !/^rustc 1\.98\.1(?:\s|$)/.test(result.stdout ?? ''))
    throw Error('Cached Rust 1.98.1 toolchain required; native dev never installs/downloads it')
}

export async function currentSource(root, {io = fs, run = spawnSync, env = process.env} = {}) {
  const git = args => {
    const result = run('git', ['--no-optional-locks', ...args], {cwd:root, env, encoding:'utf8', windowsHide:true})
    if (result.error || result.status !== 0) throw Error(`Cannot read native source Git identity: ${result.error?.message ?? result.stderr ?? result.status}`)
    return result.stdout.trim()
  }
  if (await io.realpath(git(['rev-parse','--show-toplevel'])) !== await io.realpath(root)) throw Error('Native workspace differs from Git root')
  git(['ls-files','--error-unmatch','--','apps/tezbar/build.rs'])
  const sourceCommit = git(['rev-parse','HEAD'])
  if (!/^[a-f0-9]{40}$/.test(sourceCommit)) throw Error('Invalid source Git HEAD')
  return sourceCommit
}

/** Inherit the console and wait, forwarding termination without a shell/detach.
 * Returning the exit status lets tests exercise lifecycle without starting GPUI. */
export async function launchNative(executable, args, {cwd, env, start = spawn, signals = process} = {}) {
  const child = start(executable, args, {cwd, env, stdio:'inherit', shell:false})
  const interrupt = () => child.kill('SIGINT')
  const terminate = () => child.kill('SIGTERM')
  signals.on('SIGINT', interrupt)
  signals.on('SIGTERM', terminate)
  try {
    return await new Promise((resolve, reject) => {
      child.once('error', reject)
      child.once('close', (code, signal) => resolve({exitCode:code ?? (128 + (constants.signals[signal] ?? 1)), signal}))
    })
  } finally {
    signals.removeListener('SIGINT', interrupt)
    signals.removeListener('SIGTERM', terminate)
  }
}

/** All subprocess and filesystem boundaries are injectable. Tests never run cargo/GUI. */
export async function nativeDev(args, dependencies = {}) {
  const options = parseDev(args)
  const {root = workspace, io = fs, osTemp = tmpdir(), env = process.env,
    cargo = runCargo, metadata = captureBuildMetadata, launch = launchNative,
    source = currentSource, toolchain = verifyToolchain,
    log = text => process.stdout.write(text + '\n')} = dependencies
  if (options.help) { log(HELP); return {exitCode:0} }
  const environment = previewEnvironment(env)
  const sourceCommit = await source(root, {io, env:environment})
  const resolved = {...options, resources:options.resources ? path.resolve(root, options.resources) : null}
  let input
  try { input = await validateResources(resolved, {root, sourceCommit, io}) }
  catch (error) { throw Error(`Native preview inputs unavailable: ${error.message}. Use --help for explicit local resource modes.`) }
  const build = cargoCommand(options)
  if (options.command) {
    const stage = path.join(osTemp, '<fresh-tezbar-native-dev-directory>')
    log(JSON.stringify({kind:'local-native-preview-plan', mode:input.mode, sourceCommit, publishable:false,
      preflight:{command:'rustup', args:['run','1.98.1','rustc','--version']}, build:{...build, cwd:root},
      launch:options.check ? null : {command:'<Cargo-reported tezbar executable>', args:['--profile',path.join(stage,'profile'),'--resources',options.resources ? input.source : path.join(stage,'resources'),'--show','--theme',options.theme]},
      uiSource:input.source, note:'Paths in angle brackets are allocated after a successful build; no files created.'}, null, 2))
    return {exitCode:0}
  }
  log(input.mode === 'minimal-ui-only'
    ? 'LOCAL MINIMAL UI-ONLY GPUI PREVIEW: no worker/helpers/vector SQLite/embedding runtime; dependent features unavailable.'
    : 'LOCAL GPUI PREVIEW: explicit caller-prepared resources (not production-verified).')
  await toolchain({cwd:root, env:environment})
  const executable = await cargo(build.command, build.args, {cwd:root, manifest:path.join(root,'apps/tezbar/Cargo.toml'), check:options.check, env:environment})
  if (await source(root, {io, env:environment}) !== sourceCommit) throw Error('Git HEAD changed during native build; retry')
  if (options.check) return {exitCode:0}
  const {metadata:embedded} = await metadata(executable)
  if (embedded.sourceCommit !== sourceCommit || embedded.benchControl !== false || embedded.runtimeShaders !== true || embedded.rustVersion !== '1.98.1')
    throw Error('Cargo executable metadata differs from local native preview requirements')
  // Check the explicit inputs again after the build. Never relabel stale prepared assets.
  input = await validateResources(resolved, {root, sourceCommit, io})
  const preview = await preparePreview(input, {sourceCommit, osTemp, io})
  try {
    if (await source(root, {io, env:environment}) !== sourceCommit) throw Error('Git HEAD changed during preview preparation; retry')
    log(`Ephemeral preview profile: ${preview.profile}\nPreview resources: ${preview.resources}\nBuild dirty: ${embedded.dirty}; local only, not publishable.`)
    return await launch(executable, ['--profile',preview.profile,'--resources',preview.resources,'--show','--theme',options.theme], {cwd:root, env:environment})
  } finally {
    await io.rm(preview.stage, {recursive:true, force:true})
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { process.exitCode = (await nativeDev(process.argv.slice(2))).exitCode }
  catch (error) { process.stderr.write(`Native dev: ${error.message}\n`); process.exitCode = 1 }
}
