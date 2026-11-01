import { execFileSync, spawnSync } from 'node:child_process'
import { existsSync, readdirSync, openSync, readSync, closeSync } from 'node:fs'
import { join, relative } from 'node:path'

function* machOFiles(directory) {
  if (!existsSync(directory)) return
  const magic = new Set([
    'feedface',
    'feedfacf',
    'cefaedfe',
    'cffaedfe',
    'cafebabe',
    'bebafeca',
    'cafebabf',
    'bfbafeca',
  ])
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const file = join(directory, entry.name)
    if (entry.isDirectory()) yield* machOFiles(file)
    else if (entry.isFile()) {
      const handle = openSync(file, 'r')
      const bytes = Buffer.alloc(4)
      let native
      try {
        native = readSync(handle, bytes, 0, 4, 0) === 4 && magic.has(bytes.toString('hex'))
      } finally {
        closeSync(handle)
      }
      if (native) yield file
    }
  }
}

export function verifyMacOSResources(directory, policy) {
  const resources = []
  for (const file of machOFiles(directory)) {
    const details = spawnSync('codesign', ['-d', '--verbose=4', '-r-', file], { encoding: 'utf8' })
    const metadata = details.stdout + details.stderr
    const team = metadata.match(/^TeamIdentifier=(.+)$/m)?.[1]?.trim()
    const requirement = metadata.match(/^(?:# )?designated => (.+)$/m)?.[1]?.trim()
    if (
      details.status !== 0 ||
      team !== policy.teamIdentifier ||
      /^Signature=adhoc$/m.test(metadata) ||
      !requirement ||
      /\bcdhash\b/.test(requirement) ||
      !/\bcertificate\b/.test(requirement)
    )
      throw new Error(`Native resource must share the app signing team: ${file}`)
    execFileSync('codesign', ['--verify', '--strict', file])
    resources.push({ path: relative(directory, file), teamIdentifier: team })
  }
  return resources
}

export function signMacOSResources(directory, identity, policy) {
  const certificate = macOSSigningIdentity({ identity })
  for (const file of machOFiles(directory))
    execFileSync('codesign', ['--force', '--sign', certificate, '--options', 'runtime', file])
  return verifyMacOSResources(directory, policy)
}

export function macOSSigningIdentity({ localPreview, identity }) {
  const selected = identity?.trim()
  if (localPreview) return selected || '-'
  if (!selected || selected === '-')
    throw new Error(
      'macOS releases require a persistent signing certificate; ad-hoc signing is local-preview only'
    )
  return selected
}

export function validateMacOSSigning(metadata, policy) {
  const value = (key) => metadata.match(new RegExp(`^${key}=(.+)$`, 'm'))?.[1]?.trim()
  const designatedRequirement = metadata.match(/^(?:# )?designated => (.+)$/m)?.[1]?.trim()
  if (
    value('Signature') === 'adhoc' ||
    !value('Authority') ||
    !policy.teamIdentifier ||
    value('TeamIdentifier') !== policy.teamIdentifier ||
    value('Identifier') !== policy.identifier ||
    !designatedRequirement ||
    /\bcdhash\b/.test(designatedRequirement) ||
    !/\banchor apple generic\b/.test(designatedRequirement) ||
    !/\bcertificate\b/.test(designatedRequirement) ||
    designatedRequirement !== policy.designatedRequirement
  )
    throw new Error(
      'macOS signing identity changed or is ad-hoc. Review docs/MACOS_SIGNING.md; do not publish this build.'
    )
  return {
    identifier: value('Identifier'),
    teamIdentifier: value('TeamIdentifier'),
    authority: value('Authority'),
    designatedRequirement,
    codeHash: value('CDHash'),
    identityContinuityVerified: true,
  }
}

export function verifyMacOSSigning(app, policy) {
  const result = spawnSync('codesign', ['-d', '--verbose=4', '-r-', app], { encoding: 'utf8' })
  if (result.error || result.status !== 0)
    throw result.error ?? new Error(`Cannot inspect macOS signature: ${result.stderr}`)
  const signing = validateMacOSSigning(result.stdout + result.stderr, policy)
  // Check the actual sealed app against the committed requirement, rather than
  // relying on its displayed certificate name or the updater archive signature.
  execFileSync('codesign', [
    '--verify',
    '--deep',
    '--strict',
    `-R=${policy.designatedRequirement}`,
    app,
  ])
  const resourcesDirectory = join(app, 'Contents', 'Resources')
  if (existsSync(resourcesDirectory))
    for (const helper of policy.nativeHelpers ?? [])
      if (!existsSync(join(resourcesDirectory, helper)))
        throw new Error(`Missing native helper in packaged app: ${helper}`)
  return {
    ...signing,
    nativeResources: verifyMacOSResources(resourcesDirectory, policy),
  }
}
