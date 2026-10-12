import { createHash } from 'node:crypto'
import { readFileSync, existsSync } from 'node:fs'
import { join, basename } from 'node:path'
import {
  APP_IDS,
  PRODUCT_NAMES,
  assertRelease,
  immutableAssetUrl,
  validateManifest,
} from './release-policy.mjs'
import { verifyUpdaterSignature } from '../verify-updater-signature.mjs'

export const REQUIRED_PLATFORMS = ['windows-x86_64-nsis', 'darwin-aarch64', 'darwin-x86_64']

export function validateReceipt(receipt, directory, publicKey) {
  assertRelease(receipt.channel, receipt.version)
  if (
    receipt.appIdentifier !== APP_IDS[receipt.channel] ||
    receipt.productName !== PRODUCT_NAMES[receipt.channel]
  )
    throw new Error('Build identity does not match its channel')
  if (!/^[a-f0-9]{40}$/.test(receipt.sourceCommit))
    throw new Error('Build receipt needs an exact source commit')
  if (receipt.platformVerified !== true)
    throw new Error('Platform code signature/notarization was not verified')
  if (!REQUIRED_PLATFORMS.includes(receipt.platform))
    throw new Error('Unsupported release platform')
  if (!Array.isArray(receipt.artifacts) || !receipt.artifacts.length)
    throw new Error('Receipt has no artifacts')
  for (const item of receipt.artifacts) {
    if (
      typeof item.filename !== 'string' ||
      /[\\/]/.test(item.filename) ||
      ['.', '..'].includes(item.filename) ||
      item.filename !== basename(item.filename) ||
      !/^[a-f0-9]{64}$/.test(item.sha256)
    )
      throw new Error('Invalid artifact receipt')
    const path = join(directory, item.filename)
    if (!existsSync(path)) throw new Error(`Missing artifact ${item.filename}`)
    const bytes = readFileSync(path)
    if (createHash('sha256').update(bytes).digest('hex') !== item.sha256)
      throw new Error('Artifact changed after platform verification')
    if (item.updater) verifyUpdaterSignature(bytes, readFileSync(`${path}.sig`, 'utf8'), publicKey)
  }
  if (receipt.artifacts.filter((item) => item.updater).length !== 1)
    throw new Error('Expected one updater artifact per platform')
  return receipt
}

export function manifestFromReceipts(receipts, notes, publicKey, directory, expected) {
  const platforms = {},
    seen = new Set(),
    filenames = new Set()
  for (const receipt of receipts) {
    validateReceipt(receipt, directory, publicKey)
    if (
      receipt.channel !== expected.channel ||
      receipt.version !== expected.version ||
      receipt.sourceCommit !== expected.sourceCommit
    )
      throw new Error('Release mixes source commits, versions, or channels')
    if (seen.has(receipt.platform)) throw new Error('Duplicate release platform')
    seen.add(receipt.platform)
    for (const artifact of receipt.artifacts) {
      if (filenames.has(artifact.filename))
        throw new Error('Artifact filenames must be unique across platforms')
      filenames.add(artifact.filename)
    }
    const artifact = receipt.artifacts.find((item) => item.updater)
    platforms[receipt.platform] = {
      url: immutableAssetUrl(receipt.version, artifact.filename),
      signature: readFileSync(join(directory, `${artifact.filename}.sig`), 'utf8').trim(),
    }
  }
  for (const platform of REQUIRED_PLATFORMS)
    if (!seen.has(platform)) throw new Error(`Missing required platform ${platform}`)
  return validateManifest(
    {
      version: expected.version,
      channel: expected.channel,
      appIdentifier: APP_IDS[expected.channel],
      sourceCommit: expected.sourceCommit,
      notes,
      pub_date: new Date().toISOString(),
      platforms,
    },
    expected.channel
  )
}
