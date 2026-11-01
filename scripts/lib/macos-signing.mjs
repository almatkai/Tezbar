import { execFileSync, spawnSync } from 'node:child_process'

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
  return signing
}
