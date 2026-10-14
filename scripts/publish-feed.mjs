// Publishes latest.json manifest and advances channel pointer releases (channel-beta, channel-alpha, channel-stable)
import { execFileSync } from 'node:child_process'
import { readFileSync, existsSync, writeFileSync, mkdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import {
  assertRelease,
  compareVersions,
  validateManifest,
  REPOSITORY,
  promotionTargets,
  immutableAssetUrl,
} from './lib/release-policy.mjs'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const options = {}
for (let i = 2; i < process.argv.length; i++) {
  const key = process.argv[i].replace(/^--/, '')
  if (['channel', 'version', 'source-commit', 'artifacts'].includes(key) && process.argv[i + 1]) {
    options[key] = process.argv[++i]
  } else {
    throw new Error(`Unknown argument ${process.argv[i]}`)
  }
}

if (!options.channel || !options.version) {
  throw new Error('Usage: node scripts/publish-feed.mjs --channel <channel> --version <version> [--source-commit <commit>] [--artifacts <dir>]')
}

assertRelease(options.channel, options.version)

const gh = (args, options = {}) =>
  execFileSync('gh', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'], ...options })

const tag = `v${options.version}`
const releaseJson = JSON.parse(gh(['release', 'view', tag, '--repo', REPOSITORY, '--json', 'assets,body,targetCommitish']))
const sourceCommit = options['source-commit'] || releaseJson.targetCommitish

const notes = existsSync(resolve(root, `docs/releases/${options.version}.md`))
  ? readFileSync(resolve(root, `docs/releases/${options.version}.md`), 'utf8')
  : releaseJson.body || `Tezbar ${options.version}`

// Find or read signatures
const artifactsDir = options.artifacts ? resolve(root, options.artifacts) : null

function getAssetContent(filename) {
  if (artifactsDir && existsSync(join(artifactsDir, filename))) {
    return readFileSync(join(artifactsDir, filename), 'utf8')
  }
  const asset = releaseJson.assets.find((item) => item.name === filename)
  if (!asset) return null
  return execFileSync('gh', ['release', 'download', tag, '--repo', REPOSITORY, '--pattern', filename, '-O', '-'], {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'inherit'],
  })
}

const platforms = {}

// Check macOS arm64
const macTar = `Tezbar_${options.version}_aarch64.app.tar.gz`
const macSig = getAssetContent(`${macTar}.sig`)
if (macSig) {
  platforms['darwin-aarch64'] = {
    url: immutableAssetUrl(options.version, macTar),
    signature: macSig.trim(),
  }
}

// Check Windows x64
const winExe = `Tezbar_${options.version}_x64-setup.exe`
const winSig = getAssetContent(`${winExe}.sig`)
if (winSig) {
  platforms['windows-x86_64'] = {
    url: immutableAssetUrl(options.version, winExe),
    signature: winSig.trim(),
  }
  platforms['windows-x86_64-nsis'] = {
    url: immutableAssetUrl(options.version, winExe),
    signature: winSig.trim(),
  }
}

if (!Object.keys(platforms).length) {
  throw new Error('No signed updater artifacts found for this release')
}

const manifest = validateManifest(
  {
    version: options.version,
    channel: options.channel,
    appIdentifier: 'com.tezbar.app',
    notes,
    pub_date: new Date().toISOString(),
    platforms,
  },
  options.channel
)

const outputDir = resolve(root, 'build/release-feed')
mkdirSync(outputDir, { recursive: true })
const manifestPath = join(outputDir, 'latest.json')
writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`)

console.log(`Generated manifest for ${options.channel} ${options.version}:`)
console.log(JSON.stringify(manifest, null, 2))

// Upload latest.json to the release tag
gh(['release', 'upload', tag, manifestPath, '--repo', REPOSITORY, '--clobber'])
console.log(`Uploaded latest.json to ${tag}`)

// Update pointer releases for this channel and promotions
const listPointers = () => {
  try {
    return JSON.parse(gh(['api', '--paginate', '--slurp', `repos/${REPOSITORY}/releases?per_page=100`])).flat()
  } catch {
    return []
  }
}

const allReleases = listPointers()

function updatePointer(targetChannel) {
  const pointerTag = `channel-${targetChannel}`
  const exists = allReleases.some((r) => r.tag_name === pointerTag)
  if (!exists) {
    gh([
      'release',
      'create',
      pointerTag,
      manifestPath,
      '--repo',
      REPOSITORY,
      '--target',
      sourceCommit,
      '--title',
      `Tezbar ${targetChannel} update feed`,
      '--notes',
      'Mutable updater manifest; installers are stored in immutable versioned releases.',
      '--prerelease',
      '--latest=false',
    ])
    console.log(`Created pointer release ${pointerTag}`)
  } else {
    gh(['release', 'upload', pointerTag, manifestPath, '--repo', REPOSITORY, '--clobber'])
    console.log(`Updated pointer release ${pointerTag}`)
  }
}

for (const targetChannel of promotionTargets(options.channel)) {
  updatePointer(targetChannel)
}

console.log(`Successfully published feed for ${options.channel} ${options.version}!`)
