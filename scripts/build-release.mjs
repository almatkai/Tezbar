import { spawnSync, execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  readdirSync,
  copyFileSync,
  existsSync,
} from 'node:fs'
import { dirname, join, resolve, basename } from 'node:path'
import { fileURLToPath } from 'node:url'
import { buildConfiguration, PRODUCT_NAMES, resolveBuildMode } from './lib/release-policy.mjs'
import { verifyUpdaterSignature } from './verify-updater-signature.mjs'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const values = {},
  flags = new Set()
const booleanFlags = ['dry-run', 'allow-unsigned', 'local-preview']
for (let i = 2; i < process.argv.length; i++) {
  const arg = process.argv[i]
  if (arg === '--') continue
  const key = arg.replace(/^--/, '')
  if (booleanFlags.includes(key)) flags.add(key)
  else if (
    ['channel', 'version', 'notes', 'output'].includes(key) &&
    process.argv[i + 1] &&
    !process.argv[i + 1].startsWith('--')
  )
    values[key] = process.argv[++i]
  else throw new Error(`Unknown or incomplete release argument ${arg}`)
}
const base = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8'))
const channel = values.channel ?? 'stable',
  version = values.version ?? base.version
const override = buildConfiguration(base, channel, version)
const mode = resolveBuildMode(channel, {
  allowUnsigned: flags.has('allow-unsigned'),
  localPreview: flags.has('local-preview'),
})
const directory = resolve(
  root,
  values.output ??
    `build/release-${mode.localPreview ? 'local-preview-' : ''}${channel}-${version}-${process.platform}-${process.arch}`
)
const notesPath = resolve(root, values.notes ?? `docs/releases/${version}.md`)\nif (!readFileSync(notesPath, 'utf8').trim()) throw new Error('Release notes must not be empty')
mkdirSync(directory, { recursive: true })
const configPath = join(directory, 'tauri-release.json')
if (mode.unsigned) {
  override.bundle.createUpdaterArtifacts = false
  if (process.platform === 'darwin') override.bundle.macOS = { signingIdentity: '-' }
}
if (process.platform === 'win32' && process.env.WINDOWS_CERTIFICATE_THUMBPRINT) {
  override.bundle.windows.certificateThumbprint = process.env.WINDOWS_CERTIFICATE_THUMBPRINT
  override.bundle.windows.digestAlgorithm = 'sha256'
  override.bundle.windows.timestampUrl = 'http://timestamp.digicert.com'
}
if (!mode.unsigned && process.platform === 'darwin' && process.env.APPLE_SIGNING_IDENTITY)
  override.bundle.macOS = { signingIdentity: process.env.APPLE_SIGNING_IDENTITY }
writeFileSync(configPath, `${JSON.stringify(override, null, 2)}\n`)
if (flags.has('dry-run')) {
  console.log(
    JSON.stringify(
      {\n        channel,\n        version,\n        identifier: override.identifier,\n        productName: override.productName,\n        configPath,\n        notesPath,\n        localPreview: mode.localPreview,\n        unsigned: mode.unsigned,\n      },
      null,
      2
    )
  )
} else {
  if (!['win32', 'darwin'].includes(process.platform))
    throw new Error('Native release builds require Windows or macOS')
  if (
    !['x64', 'arm64'].includes(process.arch) ||
    (process.platform === 'win32' && process.arch !== 'x64')
  )
    throw new Error('Unsupported release architecture')

  // Auto-detect updater signing key from ~/.tauri/tezbar.key if not set in environment
  const localKeyPath = join(process.env.USERPROFILE || process.env.HOME || '', '.tauri', 'tezbar.key')
  if (!process.env.TAURI_SIGNING_PRIVATE_KEY && existsSync(localKeyPath)) {
    process.env.TAURI_SIGNING_PRIVATE_KEY = readFileSync(localKeyPath, 'utf8')
    if (process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD === undefined) {
      process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ''
    }
  }

  if (!mode.unsigned) {
    if (
      execFileSync('git', ['status', '--porcelain', '--untracked-files=no'], {
        cwd: root,
        encoding: 'utf8',
      }).trim()
    )
      throw new Error('Signed release builds require a clean source checkout')
    if (!process.env.TAURI_SIGNING_PRIVATE_KEY)
      throw new Error('An updater signing key is required')
    if (process.platform === 'win32' && !process.env.WINDOWS_CERTIFICATE_THUMBPRINT)
      throw new Error('A Windows code-signing certificate is required')
    if (
      process.platform === 'darwin' &&
      (!process.env.APPLE_SIGNING_IDENTITY?.startsWith('Developer ID Application:') ||
        !process.env.APPLE_API_KEY ||
        !process.env.APPLE_API_ISSUER ||
        !process.env.APPLE_API_KEY_PATH)
    )
      throw new Error('Developer ID signing and notarization credentials are required')
  }
  const helpers = spawnSync(process.execPath, [join(root, 'scripts/build-native.mjs')], {
    cwd: root,
    stdio: 'inherit',
    windowsHide: true,
  })
  if (helpers.error || helpers.status !== 0)
    throw helpers.error ?? new Error('Native helper build failed')
  const result = spawnSync(
    process.execPath,
    [
      join(root, 'node_modules/@tauri-apps/cli/tauri.js'),
      'build',
      '--ci',
      '--config',
      configPath,
      '--bundles',
      process.platform === 'win32' ? 'nsis' : 'app,dmg',
      '--',
      '--locked',
    ],
    {
      cwd: root,
      stdio: 'inherit',
      windowsHide: true,
      env: {
        ...process.env,
        TEZBAR_RELEASE_VERSION: version,
        TEZBAR_RELEASE_NOTES_PATH: notesPath,
        TEZBAR_RELEASE_CHANNEL: channel,
      },
    }
  )
  if (result.error || result.status !== 0)
    throw result.error ?? new Error('Native release build failed')
  const bundle = join(root, 'src-tauri/target/release/bundle')
  const platform =
    process.platform === 'win32'
      ? 'windows-x86_64-nsis'
      : `darwin-${process.arch === 'arm64' ? 'aarch64' : 'x86_64'}`
  const product = PRODUCT_NAMES[channel]
  if (mode.unsigned && process.platform === 'darwin') {
    execFileSync(
      'tar',
      [
        '--exclude=._*',
        '-czf',
        join(bundle, 'macos', `${product}.app.tar.gz`),
        '-C',
        join(bundle, 'macos'),
        `${product}.app`,
      ],
      {
        env: {
          ...process.env,
          COPYFILE_DISABLE: '1',
        },
      }
    )
  }
  const paths = []
  const collect = (path) => {
    for (const entry of readdirSync(path, { withFileTypes: true })) {
      const file = join(path, entry.name)
      if (entry.isDirectory()) {
        if (!entry.name.endsWith('.app')) collect(file)
      } else if (\n        (entry.name.startsWith(`${product}_${version}_`) && /\\.(exe|dmg)$/.test(entry.name)) ||\n        entry.name === `${product}.app.tar.gz`\n      )\n        paths.push(file)\n    }\n  }\n  collect(bundle)\n  const updater = paths.filter((path) =>\n    process.platform === 'win32' ? path.endsWith('.exe') : path.endsWith('.app.tar.gz')\n  )\n  if (updater.length !== 1)\n    throw new Error(\n      'Expected exactly one updater artifact; clean stale bundle output before building'\n    )\n  let platformVerified = false\n  if (!mode.unsigned) {\n    if (process.platform === 'win32') {\n      execFileSync(\n        'powershell.exe',\n        [\n          '-NoLogo',\n          '-NoProfile',\n          '-NonInteractive',\n          '-Command',\n          \"$s=Get-AuthenticodeSignature -LiteralPath $env:TEZBAR_RELEASE_ARTIFACT; if($s.Status -ne 'Valid' -or $s.SignerCertificate.Thumbprint -ne $env:WINDOWS_CERTIFICATE_THUMBPRINT){throw 'Invalid Windows code signature'}\",\n        ],\n        { windowsHide: true, env: { ...process.env, TEZBAR_RELEASE_ARTIFACT: updater[0] } }\n      )\n    } else {\n      const app = join(bundle, 'macos', `${product}.app`)\n      execFileSync('codesign', ['--verify', '--deep', '--strict', app])\n      execFileSync('spctl', ['--assess', '--type', 'execute', app])\n      execFileSync('xcrun', ['stapler', 'validate', app])\n    }\n    platformVerified = true\n  }\n  const artifacts = paths.map((path) => {\n    const isUpdater = path === updater[0]\n    const filename = path.endsWith('.app.tar.gz')\n      ? `${product}_${version}_${process.arch === 'arm64' ? 'aarch64' : 'x86_64'}.app.tar.gz`\n      : basename(path)\n    const bytes = readFileSync(path)\n    copyFileSync(path, join(directory, filename))\n    if (isUpdater) {\n      const sigPath = `${path}.sig`\n      if (!existsSync(sigPath) && process.env.TAURI_SIGNING_PRIVATE_KEY) {\n        spawnSync(\n          process.execPath,\n          [join(root, 'node_modules/@tauri-apps/cli/tauri.js'), 'signer', 'sign', path],\n          { \n            env: {\n              ...process.env,\n              TAURI_SIGNING_PRIVATE_KEY: process.env.TAURI_SIGNING_PRIVATE_KEY,\n              TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD || '',\n            },\n            stdio: 'inherit',\n          }\n        )\n      }\n      if (existsSync(sigPath)) {\n        verifyUpdaterSignature(\n          bytes,\n          readFileSync(sigPath, 'utf8'),\n          base.plugins.updater.pubkey\n        )\n        copyFileSync(sigPath, join(directory, `${filename}.sig`))\n      } else if (!mode.unsigned) {\n        throw new Error('Updater signature is missing')\n      }\n    }\n    return {\n      filename,\n      sha256: createHash('sha256').update(bytes).digest('hex'),\n      updater: isUpdater,\n    }\n  })\n  writeFileSync(\n    join(directory, `receipt-${platform}.json`),\n    JSON.stringify(\n      {\n        channel,\n        version,\n        productName: product,\n        appIdentifier: override.identifier,\n        platform,\n        platformVerified,\n        distribution: mode.localPreview\n          ? 'local-preview'\n          : mode.unsigned\n            ? 'testing-preview'\n            : 'signed-release',\n        sourceCommit: execFileSync('git', ['rev-parse', 'HEAD'], {\n          cwd: root,\n          encoding: 'utf8',\n        }).trim(),\n        artifacts,\n      },\n      null,\n      2\n    )\n  )\n  console.log(`${mode.unsigned ? 'Local installer' : 'Verified release'} artifacts: ${directory}`)\n}\n