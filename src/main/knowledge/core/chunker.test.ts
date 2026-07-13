import { describe, expect, it } from 'vitest'
import { chunksFromPages } from './chunker'

describe('knowledge chunker', () => {
  it('combines extracted text and OCR without duplicating identical content', async () => {
    const chunks = await chunksFromPages('source-1', [
      {
        pageNumber: 3,
        extractedText: 'Account certificate expired.',
        ocr: { text: 'Account certificate expired.', blocks: [] },
      },
    ])

    expect(chunks).toHaveLength(1)
    expect(chunks[0]?.pageNumber).toBe(3)
    expect(chunks[0]?.text).toBe('Account certificate expired.')
  })

  it('creates stable, overlapping chunks for long content', async () => {
    const text = Array.from({ length: 240 }, (_, index) => `Sentence ${index} has searchable text.`).join(' ')
    const first = await chunksFromPages('source-2', [{ pageNumber: 1, extractedText: text }])
    const second = await chunksFromPages('source-2', [{ pageNumber: 1, extractedText: text }])

    expect(first.length).toBeGreaterThan(2)
    expect(first.map((chunk) => chunk.id)).toEqual(second.map((chunk) => chunk.id))
    expect(first[0]?.endOffset).toBeGreaterThan(first[1]?.startOffset ?? Number.MAX_SAFE_INTEGER)
  })

  it('caps pathological sources before they can monopolize indexing', async () => {
    const text = Array.from({ length: 500 }, (_, index) => `Generated row ${index}.`).join(' ')
    const chunks = await chunksFromPages('source-capped', [{ pageNumber: 1, extractedText: text }], 2)
    expect(chunks).toHaveLength(2)
  })

  it('yields while chunking large sources so interactive searches can run', async () => {
    let yieldCount = 0
    const text = Array.from({ length: 2_000 }, (_, index) => `Searchable generated row ${index}.`).join(' ')
    await chunksFromPages(
      'source-yielding',
      [{ pageNumber: 1, extractedText: text }],
      Number.MAX_SAFE_INTEGER,
      async () => { yieldCount += 1 },
    )
    expect(yieldCount).toBeGreaterThan(0)
  })
})
