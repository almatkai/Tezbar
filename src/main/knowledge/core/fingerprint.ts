import { createHash } from 'node:crypto'
import { createReadStream, statSync } from 'node:fs'
import type { SourceFingerprint } from '../../../shared/knowledge'

export async function fingerprintSource(path: string, signal?: AbortSignal): Promise<SourceFingerprint> {
  const stat = statSync(path)
  const hash = createHash('sha256')
  const stream = createReadStream(path)

  await new Promise<void>((resolve, reject) => {
    const abort = (): void => {
      stream.destroy(new Error('Indexing cancelled'))
    }
    signal?.addEventListener('abort', abort, { once: true })
    stream.on('data', (chunk) => hash.update(chunk))
    stream.once('end', resolve)
    stream.once('error', reject)
    stream.once('close', () => signal?.removeEventListener('abort', abort))
  })

  return {
    contentHash: hash.digest('hex'),
    byteSize: stat.size,
    modifiedAt: stat.mtimeMs,
  }
}

export function sourceIdForPath(path: string): string {
  return createHash('sha256').update(path).digest('hex')
}

export function artifactSettingsHash(value: unknown): string {
  return createHash('sha256').update(JSON.stringify(value)).digest('hex')
}
