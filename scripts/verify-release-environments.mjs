import { verifyReleaseEnvironments } from './lib/release-environment.mjs'
verifyReleaseEnvironments(process.argv[2])
console.log('Signing and publication approval rules verified')
