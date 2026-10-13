import { createHash } from 'node:crypto'

export const REPOSITORY = 'almatkai/Tezbar'
export const APP_IDS = {
  stable: 'com.tezbar.app',
  beta: 'com.tezbar.app',
  alpha: 'com.tezbar.app',
}
export const PRODUCT_NAMES = { stable: 'Tezbar', beta: 'Tezbar', alpha: 'Tezbar' }
export const CHANNELS = ['stable', 'beta', 'alpha']

export function parseVersion(version) {
  const match = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(alpha|beta)\.([1-9]\d*))?$/.exec(
    version
  )
  if (!match) throw new Error('Use X.Y.Z, X.Y.Z-alpha.N, or X.Y.Z-beta.N; N must be positive')
  return {
    version,
    core: match.slice(1, 4),
    channel: match[4] ?? 'stable',
    sequence: match[5] ?? '0',
  }
}

export function compareVersions(left, right) {
  const a = parseVersion(left),
    b = parseVersion(right)
  for (let i = 0; i < 3; i++) {
    const difference = BigInt(a.core[i]) - BigInt(b.core[i])
    if (difference) return difference > 0 ? 1 : -1
  }
  const rank = { alpha: 0, beta: 1, stable: 2 }
  if (rank[a.channel] !== rank[b.channel]) return rank[a.channel] - rank[b.channel]
  const difference = BigInt(a.sequence) - BigInt(b.sequence)
  return difference === 0n ? 0 : difference > 0n ? 1 : -1
}

export function acceptsRelease(feed, version) {
  if (!CHANNELS.includes(feed)) throw new Error('Unknown update channel')
  const channel = parseVersion(version).channel
  return (
    feed === 'alpha' ||
    (feed === 'beta' ? channel === 'beta' || channel === 'stable' : channel === feed)
  )
}

export function assertRelease(channel, version) {
  if (!CHANNELS.includes(channel) || parseVersion(version).channel !== channel) {
    throw new Error('Release version and channel disagree')
  }
}

export function promotionTargets(channel) {
  if (!CHANNELS.includes(channel)) throw new Error('Unknown release channel')
  return CHANNELS.slice(CHANNELS.indexOf(channel))
}

function wixUpgradeCode(product) {
  const namespace = Buffer.from('6ba7b8109dad11d180b400c04fd430c8', 'hex')
  const hash = createHash('sha1')
    .update(namespace)
    .update(`${product}.exe.app.x64`)
    .digest()
    .subarray(0, 16)
  hash[6] = (hash[6] & 0x0f) | 0x50
  hash[8] = (hash[8] & 0x3f) | 0x80
  const value = hash.toString('hex')
  return `${value.slice(0, 8)}-${value.slice(8, 12)}-${value.slice(12, 16)}-${value.slice(16, 20)}-${value.slice(20)}`
}

export function buildConfiguration(base, channel, version) {
  assertRelease(channel, version)
  const productName = PRODUCT_NAMES[channel]
  return {
    version,
    productName,
    identifier: APP_IDS[channel],
    mainBinaryName: 'tezbar',
    app: {
      windows: base.app.windows.map((window) => ({
        ...window,
        title: window.label === 'main' ? productName : window.title,
      })),
    },
    bundle: {
      windows: { allowDowngrades: false, wix: { upgradeCode: wixUpgradeCode(productName) } },
    },
    plugins: {
      updater: {
        endpoints: [
          `https://github.com/${REPOSITORY}/releases/download/channel-${channel}/latest.json`,
        ],
      },
    },
  }
}

export function immutableAssetUrl(version, filename) {
  parseVersion(version)
  if (!filename || filename !== filename.split(/[\\/]/).at(-1))
    throw new Error('Artifact must be a filename')
  return `https://github.com/${REPOSITORY}/releases/download/v${version}/${encodeURIComponent(filename)}`
}

export function validateManifest(manifest, feed) {
  if (!manifest || typeof manifest !== 'object' || !acceptsRelease(feed, manifest.version))
    throw new Error('Manifest version does not belong in this channel')
  const { channel } = parseVersion(manifest.version)
  if (manifest.channel !== channel || manifest.appIdentifier !== APP_IDS[channel])
    throw new Error('Manifest identity does not match the release')
  if (!manifest.notes?.trim() || !Number.isFinite(Date.parse(manifest.pub_date)))
    throw new Error('Release notes and publication date are required')
  if (!manifest.platforms || !Object.keys(manifest.platforms).length)
    throw new Error('Release has no update artifacts')
  const prefix = `https://github.com/${REPOSITORY}/releases/download/v${manifest.version}/`
  for (const [platform, artifact] of Object.entries(manifest.platforms)) {
    if (!/^(windows|darwin|linux)-(x86_64|aarch64)(-nsis|-msi|-appimage)?$/.test(platform))
      throw new Error(`Unknown platform ${platform}`)
    if (
      typeof artifact.url !== 'string' ||
      !artifact.url.startsWith(prefix) ||
      !artifact.signature?.trim()
    )
      throw new Error('Update artifact URL or signature is invalid')
    const filename = decodeURIComponent(new URL(artifact.url).pathname.split('/').at(-1))
    if (
      !filename.startsWith(`${PRODUCT_NAMES[channel]}_`) &&
      filename !== `${PRODUCT_NAMES[channel]}.app.tar.gz`
    )
      throw new Error('Installer name belongs to a different app')
    if (artifact.url !== immutableAssetUrl(manifest.version, filename))
      throw new Error('Artifact URL is not canonical')
  }
  return manifest
}

export function validatePromotion(previous, next, feed) {
  validateManifest(next, feed)
  if (previous) {
    validateManifest(previous, feed)
    if (compareVersions(next.version, previous.version) <= 0)
      throw new Error('A channel can only advance to a newer release')
  }
}
