import { execFileSync } from 'node:child_process'
import { CHANNELS, REPOSITORY } from './release-policy.mjs'

export function assertProtectedEnvironment(environment) {
  const reviewers = environment.protection_rules?.find((rule) => rule.type === 'required_reviewers')
  const branches = environment.deployment_branch_policy
  if (
    !reviewers?.reviewers?.length ||
    reviewers.prevent_self_review !== true ||
    (!branches?.protected_branches && !branches?.custom_branch_policies)
  ) {
    throw new Error(
      `${environment.name}: required reviewers, prevention of self-review, and branch restrictions must be configured`
    )
  }
}

export function verifyReleaseEnvironments(channel) {
  if (!CHANNELS.includes(channel)) throw new Error('Unknown publication channel')
  for (const name of ['release-build', `release-${channel}`]) {
    const environment = JSON.parse(
      execFileSync('gh', ['api', `repos/${REPOSITORY}/environments/${name}`], {
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'inherit'],
      })
    )
    assertProtectedEnvironment(environment)
  }
}
