import { createHash, createPublicKey, verify } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export function verifyUpdaterSignature(bytes, encodedSignature, encodedPublicKey) {
  const publicKey = Buffer.from(encodedPublicKey, 'base64').toString('utf8').trim().split('\n')
  const signature = Buffer.from(encodedSignature.trim(), 'base64')
    .toString('utf8')
    .trim()
    .split('\n')
  const keyBytes = Buffer.from(publicKey[1], 'base64')
  const signatureBytes = Buffer.from(signature[1], 'base64')
  if (
    keyBytes.length !== 42 ||
    signatureBytes.length !== 74 ||
    !keyBytes.subarray(2, 10).equals(signatureBytes.subarray(2, 10))
  ) {
    throw new Error('Updater signature does not match the configured signing key')
  }
  const key = createPublicKey({
    key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), keyBytes.subarray(10)]),
    format: 'der',
    type: 'spki',
  })
  const algorithm = signatureBytes.subarray(0, 2).toString()
  if (!['ED', 'Ed'].includes(algorithm)) throw new Error('Unsupported signature algorithm')
  const message = algorithm === 'ED' ? createHash('blake2b512').update(bytes).digest() : bytes
  if (!verify(null, message, key, signatureBytes.subarray(10)))
    throw new Error('Invalid artifact signature')
  const prefix = 'trusted comment: '
  if (
    !signature[2].startsWith(prefix) ||
    !verify(
      null,
      Buffer.concat([signatureBytes.subarray(10), Buffer.from(signature[2].slice(prefix.length))]),
      key,
      Buffer.from(signature[3], 'base64')
    )
  )
    throw new Error('Invalid trusted comment signature')
  return true
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [artifact, signaturePath, configPath = 'src-tauri/tauri.conf.json'] = process.argv.slice(2)
  if (!artifact || !signaturePath)
    throw new Error('Usage: verify-updater-signature ARTIFACT SIGNATURE [CONFIG]')
  const config = JSON.parse(readFileSync(configPath, 'utf8'))
  verifyUpdaterSignature(
    readFileSync(artifact),
    readFileSync(signaturePath, 'utf8'),
    config.plugins.updater.pubkey
  )
  console.log('Updater signature verified against the app public key')
}
