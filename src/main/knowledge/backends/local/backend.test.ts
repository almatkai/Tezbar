import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { fingerprintSource, sourceIdForPath } from '../../core/fingerprint'
import { LocalIndexingBackend } from './backend'

const cleanup: string[] = []

afterEach(async () => {
  await Promise.all(cleanup.splice(0).map((path) => rm(path, { recursive: true, force: true })))
})

describe('local indexing backend', () => {
  it('extracts, chunks, and embeds textual files without leaving the device', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'tezbar-local-index-'))
    cleanup.push(directory)
    const path = join(directory, 'incident.md')
    await writeFile(path, '# Incident\n\nThe production signing certificate expired on Friday.', 'utf8')
    const fingerprint = await fingerprintSource(path)
    const backend = new LocalIndexingBackend()
    const controller = new AbortController()
    const progress: number[] = []

    const result = await backend.index({
      jobId: 'job-1',
      rootId: 'root-1',
      sourceId: sourceIdForPath(path),
      path,
      fingerprint,
      depth: 'smart',
      requestedCapabilities: ['extracted-text', 'chunks', 'text-embeddings'],
      maxPagesPerDocument: null,
      maxOcrPagesPerDocument: 20,
      ocrEveryPage: false,
    }, {
      signal: controller.signal,
      onProgress: (value) => progress.push(value),
    })

    expect(result.pages[0]?.extractedText).toContain('signing certificate expired')
    expect(result.chunks[0]?.embedding).toHaveLength(384)
    expect(result.completedCapabilities).toEqual(expect.arrayContaining([
      'extracted-text', 'chunks', 'text-embeddings',
    ]))
    expect(progress.at(-1)).toBe(1)
  })

  it('keeps Basic depth lexical-only without generating embeddings', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'tezbar-basic-index-'))
    cleanup.push(directory)
    const path = join(directory, 'notes.txt')
    await writeFile(path, 'A searchable basic knowledge note. password=hunter2', 'utf8')
    const fingerprint = await fingerprintSource(path)
    const backend = new LocalIndexingBackend()

    const result = await backend.index({
      jobId: 'job-basic',
      rootId: 'root-basic',
      sourceId: sourceIdForPath(path),
      path,
      fingerprint,
      depth: 'basic',
      requestedCapabilities: ['extracted-text', 'chunks'],
      maxPagesPerDocument: 20,
      maxOcrPagesPerDocument: 0,
      ocrEveryPage: false,
    }, {
      signal: new AbortController().signal,
      onProgress: () => {},
    })

    expect(result.chunks[0]?.text).toContain('searchable basic')
    expect(result.chunks[0]?.text).toContain('password=[REDACTED]')
    expect(result.chunks[0]?.text).not.toContain('hunter2')
    expect(result.chunks[0]?.embedding).toBeUndefined()
    expect(result.completedCapabilities).not.toContain('text-embeddings')
  })
})
