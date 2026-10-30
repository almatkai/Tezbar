import { spawnSync, execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  readdirSync,
  copyFileSync,
  existsSync,
  unlinkSync,
} from 'node:fs'
import { dirname, join, resolve, basename } from 'node:path'
import { fileURLToPath } from 'node:url'
import { buildConfiguration, PRODUCT_NAMES, resolveBuildMode } from './lib/release-policy.mjs'
import { verifyUpdaterSignature } from './verify-updater-signature.mjs'
import { assertEmbeddingRuntimeDistribution } from './lib/embedding-runtime-policy.mjs'

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
const channel = values.channel ?? 'stable'
const version = values.version ?? base.version
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
const notesPath = resolve(root, values.notes ?? `docs/releases/${version}.md`)
if (!readFileSync(notesPath, 'utf8').trim()) throw new Error('Release notes must not be empty')
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
      {
        channel,
        version,
        identifier: override.identifier,
        productName: override.productName,
        configPath,
        notesPath,
        localPreview: mode.localPreview,
        unsigned: mode.unsigned,
      },
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

  assertEmbeddingRuntimeDistribution({
    platform: process.platform,
    arch: process.arch,
    unsigned: mode.unsigned,
    catalog: JSON.parse(
      readFileSync(join(root, 'src-tauri/runtime/embedding-macos-arm64.json'), 'utf8')
    ),
  })

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
  const bundle = existsSync(join(root, 'src-tauri/target/release/bundle')) ? join(root, 'src-tauri/target/release/bundle') : join(root, 'target/release/bundle')
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
      } else if (
        (entry.name.startsWith(`${product}_${version}_`) && /\.(exe|dmg)$/.test(entry.name)) ||
        entry.name === `${product}.app.tar.gz`
      )
        paths.push(file)
    }
  }
  collect(bundle)
  const updater = paths.filter((path) =>
    process.platform === 'win32' ? path.endsWith('.exe') : path.endsWith('.app.tar.gz')
  )
  if (updater.length !== 1)
    throw new Error(
      'Expected exactly one updater artifact; clean stale bundle output before building'
    )
  let platformVerified = false
  if (!mode.unsigned) {
    if (process.platform === 'win32') {
      execFileSync(
        'powershell.exe',
        [
          '-NoLogo',
          '-NoProfile',
          '-NonInteractive',
          '-Command',
          "$s=Get-AuthenticodeSignature -LiteralPath $env:TEZBAR_RELEASE_ARTIFACT; if($s.Status -ne 'Valid' -or $s.SignerCertificate.Thumbprint -ne $env:WINDOWS_CERTIFICATE_THUMBPRINT){throw 'Invalid Windows code signature'}",
        ],
        { windowsHide: true, env: { ...process.env, TEZBAR_RELEASE_ARTIFACT: updater[0] } }
      )
    } else {
      const app = join(bundle, 'macos', `${product}.app`)
      execFileSync('codesign', ['--verify', '--deep', '--strict', app])
      execFileSync('spctl', ['--assess', '--type', 'execute', app])
      execFileSync('xcrun', ['stapler', 'validate', app])
    }
    platformVerified = true
  }
  const artifacts = paths.map((path) => {
    const isUpdater = path === updater[0]
    const filename = path.endsWith('.app.tar.gz')
      ? `${product}_${version}_${process.arch === 'arm64' ? 'aarch64' : 'x86_64'}.app.tar.gz`
      : basename(path)
    const bytes = readFileSync(path)
    copyFileSync(path, join(directory, filename))
    if (isUpdater) {
      const sigPath = `${path}.sig`
      if (existsSync(sigPath)) {
        try { unlinkSync(sigPath) } catch {}
      }
      if (process.env.TAURI_SIGNING_PRIVATE_KEY) {
        spawnSync(
          process.execPath,
          [join(root, 'node_modules/@tauri-apps/cli/tauri.js'), 'signer', 'sign', path],
          {
            env: {
              ...process.env,
              TAURI_SIGNING_PRIVATE_KEY: process.env.TAURI_SIGNING_PRIVATE_KEY,
              TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD || '',
            },
            stdio: 'inherit',
          }
        )
      }
      if (existsSync(sigPath)) {
        verifyUpdaterSignature(
          bytes,
          readFileSync(sigPath, 'utf8'),
          base.plugins.updater.pubkey
        )
        copyFileSync(sigPath, join(directory, `${filename}.sig`))
      } else if (!mode.unsigned) {
        throw new Error('Updater signature is missing')
      }
    }
    return {
      filename,
      sha256: createHash('sha256').update(bytes).digest('hex'),
      updater: isUpdater,
    }
  })
  writeFileSync(
    join(directory, `receipt-${platform}.json`),
    JSON.stringify(
      {
        channel,
        version,
        productName: product,
        appIdentifier: override.identifier,
        platform,
        platformVerified,
        distribution: mode.localPreview
          ? 'local-preview'
          : mode.unsigned
            ? 'testing-preview'
            : 'signed-release',
        sourceCommit: execFileSync('git', ['rev-parse', 'HEAD'], {
          cwd: root,
          encoding: 'utf8',
        }).trim(),
        artifacts,
      },
      null,
      2
    )
  )
  console.log(`${mode.unsigned ? 'Local installer' : 'Verified release'} artifacts: ${directory}`)
}
