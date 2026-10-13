// Explicit workflow-only publication. Local validation is available via --dry-run.
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { readFileSync, readdirSync, writeFileSync, mkdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import {
  assertRelease,
  compareVersions,
  validateManifest,
  validatePromotion,
  REPOSITORY,
  promotionTargets,
} from './lib/release-policy.mjs'
import { manifestFromReceipts } from './lib/release-artifacts.mjs'
import { verifyReleaseEnvironments } from './lib/release-environment.mjs'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const options = {},
  flags = new Set()
for (let i = 2; i < process.argv.length; i++) {
  const key = process.argv[i].replace(/^--/, '')
  if (['dry-run', 'resume'].includes(key)) flags.add(key)
  else if (
    ['channel', 'version', 'artifacts', 'notes', 'source-commit', 'output'].includes(key) &&
    process.argv[i + 1]
  )
    options[key] = process.argv[++i]
  else throw new Error(`Unknown publication argument ${process.argv[i]}`)
}
assertRelease(options.channel, options.version)
if (!/^[a-f0-9]{40}$/.test(options['source-commit'] ?? ''))
  throw new Error('An exact source commit is required')
const directory = resolve(root, options.artifacts ?? 'release-artifacts')
const notes = readFileSync(
  resolve(root, options.notes ?? `docs/releases/${options.version}.md`),
  'utf8'
)
const receipts = readdirSync(directory)
  .filter((name) => /^receipt-.+\.json$/.test(name))
  .map((name) => JSON.parse(readFileSync(join(directory, name), 'utf8')))
const publicKey = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8')).plugins
  .updater.pubkey
let manifest = manifestFromReceipts(receipts, notes, publicKey, directory, {
  channel: options.channel,
  version: options.version,
  sourceCommit: options['source-commit'],
})
const output = resolve(root, options.output ?? 'build/release-publication')
mkdirSync(output, { recursive: true })
const manifestPath = join(output, 'latest.json')
writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`)
if (flags.has('dry-run')) {
  console.log(`Validated ${options.channel} ${options.version}; manifest: ${manifestPath}`)
} else {
  if (
    process.env.GITHUB_ACTIONS !== 'true' ||
    process.env.GITHUB_REPOSITORY !== REPOSITORY ||
    process.env.RELEASE_PUBLICATION_APPROVED !== 'true'
  )
    throw new Error('Publication requires the protected release workflow')
  verifyReleaseEnvironments(options.channel)
  const gh = (args) =>
    execFileSync('gh', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] })
  const listReleases = () =>
    JSON.parse(
      gh(['api', '--paginate', '--slurp', `repos/${REPOSITORY}/releases?per_page=100`])
    ).flat()
  // Read channel assets with the octet-stream API; unlike a browser URL this is authenticated and uncached.
  const readPointer = (channel) => {
    const releases = listReleases()
    const release = releases.find((item) => item.tag_name === `channel-${channel}`)
    if (!release) return null
    const asset = release.assets.find((item) => item.name === 'latest.json')
    if (!asset) throw new Error(`Channel ${channel} has no manifest`)
    return JSON.parse(
      gh([
        'api',
        '-H',
        'Accept: application/octet-stream',
        `repos/${REPOSITORY}/releases/assets/${asset.id}`,
      ])
    )
  }
  const tag = `v${options.version}`
  const releases = listReleases()
  const previousRelease = releases.find((release) => release.tag_name === tag)
  if (previousRelease && !flags.has('resume'))
    throw new Error(
      'Version already has a release; use --resume only with its original verified artifacts'
    )
  if (options.channel === 'stable') {
    for (const release of releases.filter(
      (item) => !item.draft && !item.prerelease && /^v\d+\.\d+\.\d+$/.test(item.tag_name)
    )) {
      if (
        release.tag_name !== tag &&
        compareVersions(options.version, release.tag_name.slice(1)) <= 0
      )
        throw new Error('Stable publication would roll back the public release history')
    }
  }
  const refs = execFileSync(
    'git',
    ['ls-remote', '--tags', 'origin', `refs/tags/${tag}`, `refs/tags/${tag}^{}`],
    { encoding: 'utf8' }
  )
    .trim()
    .split('\n')
    .filter(Boolean)
  const taggedCommit = refs.find((line) => line.endsWith('^{}')) ?? refs[0]
  if (taggedCommit && taggedCommit.split(/\s+/)[0] !== options['source-commit'])
    throw new Error('Existing tag points to a different source commit')
  const assets = receipts.flatMap((receipt) =>
    receipt.artifacts.flatMap((artifact) =>
      artifact.updater
        ? [join(directory, artifact.filename), join(directory, `${artifact.filename}.sig`)]
        : [join(directory, artifact.filename)]
    )
  )
  if (previousRelease) {
    if (previousRelease.prerelease !== (options.channel !== 'stable'))
      throw new Error('Existing release has the wrong publication category')
    const remoteManifestAsset = previousRelease.assets.find((item) => item.name === 'latest.json')
    if (!remoteManifestAsset)
      throw new Error('Incomplete draft: delete the unpublished draft explicitly before rebuilding')
    const remote = validateManifest(
      JSON.parse(
        gh([
          'api',
          '-H',
          'Accept: application/octet-stream',
          `repos/${REPOSITORY}/releases/assets/${remoteManifestAsset.id}`,
        ])
      ),
      options.channel
    )
    if (
      remote.version !== manifest.version ||
      remote.sourceCommit !== manifest.sourceCommit ||
      remote.notes !== manifest.notes ||
      JSON.stringify(remote.platforms) !== JSON.stringify(manifest.platforms)
    )
      throw new Error('Recovery artifacts do not match the existing immutable release')
    for (const path of assets) {
      const filename = path.split(/[\\/]/).at(-1)
      const asset = previousRelease.assets.find((item) => item.name === filename)
      if (!asset)
        throw new Error(
          `Existing release is missing ${filename}; never repair published artifacts in place`
        )
      const bytes = execFileSync(
        'gh',
        [
          'api',
          '-H',
          'Accept: application/octet-stream',
          `repos/${REPOSITORY}/releases/assets/${asset.id}`,
        ],
        { maxBuffer: 1024 * 1024 * 1024 }
      )
      if (
        createHash('sha256').update(bytes).digest('hex') !==
        createHash('sha256').update(readFileSync(path)).digest('hex')
      )
        throw new Error(`Existing artifact differs: ${filename}`)
    }
    manifest = remote
    writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`)
  }
  const existing = readPointer(options.channel)
  if (existing?.version === manifest.version && flags.has('resume')) {
    validateManifest(existing, options.channel)
    if (
      existing.sourceCommit !== manifest.sourceCommit ||
      JSON.stringify(existing.platforms) !== JSON.stringify(manifest.platforms)
    )
      throw new Error('Existing pointer conflicts with the recovered release')
  } else validatePromotion(existing, manifest, options.channel)
  if (!previousRelease)
    gh([
      'release',
      'create',
      tag,
      ...assets,
      manifestPath,
      '--repo',
      REPOSITORY,
      '--target',
      options['source-commit'],
      '--title',
      `Tezbar ${options.version}`,
      '--notes-file',
      resolve(root, options.notes ?? `docs/releases/${options.version}.md`),
      '--draft',
      options.channel === 'stable' ? '--latest=false' : '--prerelease',
    ])
  gh([
    'release',
    'edit',
    tag,
    '--repo',
    REPOSITORY,
    '--draft=false',
    options.channel === 'stable' ? '--latest' : '--latest=false',
  ])
  const updatePointer = (channel) => {
    const pointerTag = `channel-${channel}`
    if (!releases.some((release) => release.tag_name === pointerTag))
      gh([
        'release',
        'create',
        pointerTag,
        manifestPath,
        '--repo',
        REPOSITORY,
        '--target',
        options['source-commit'],
        '--title',
        `Tezbar ${channel} update feed`,
        '--notes',
        'Mutable updater manifest; installers are stored in immutable versioned releases.',
        '--prerelease',
        '--latest=false',
      ])
    else gh(['release', 'upload', pointerTag, manifestPath, '--repo', REPOSITORY, '--clobber'])
  }
  updatePointer(options.channel)
  for (const channel of promotionTargets(options.channel).slice(1)) {
    const previous = readPointer(channel)
    if (previous) validateManifest(previous, channel)
    if (!previous || compareVersions(manifest.version, previous.version) > 0) updatePointer(channel)
  }
  console.log(`Published ${tag} and advanced ${options.channel}`)
}
