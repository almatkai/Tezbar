import { execFileSync } from 'node:child_process'
import { assertRelease } from './lib/release-policy.mjs'
const [channel, version, sourceRef] = process.argv.slice(2)
assertRelease(channel, version)
if (!sourceRef || /^(refs\/heads\/|origin\/)?dev$/.test(sourceRef))
  throw new Error('Select a reviewed commit or feature/release ref; dev cannot be a release source')
const git = (args) => execFileSync('git', args, { encoding: 'utf8' }).trim()
const commit = git(['rev-parse', 'HEAD'])
if (channel === 'stable') {
  execFileSync('git', ['merge-base', '--is-ancestor', commit, 'origin/main'])
}
if (git(['status', '--porcelain', '--untracked-files=no']))
  throw new Error('Release source must have no modified tracked files')
console.log(`Validated ${channel} ${version} at ${commit}`)
console.log(git(['log', '--oneline', 'origin/main..HEAD']))
console.log(git(['diff', '--stat', 'origin/main...HEAD']))
