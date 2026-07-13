import { createHash } from 'node:crypto'
import type { IndexedChunk, IndexedPage } from '../../../shared/knowledge'

const TARGET_CHARS = 1_200
const OVERLAP_CHARS = 180
export const MAX_KNOWLEDGE_CHUNKS_PER_SOURCE = 4_000

function normalizeText(value: string): string {
  return value
    .replace(/\u0000/g, '')
    .replace(/[ \t]+\n/g, '\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

async function chunkText(
  sourceId: string,
  text: string,
  pageNumber: number | undefined,
  limit: number,
  yieldToInteractiveWork?: () => Promise<void>,
): Promise<IndexedChunk[]> {
  const normalized = normalizeText(text)
  if (!normalized) return []

  const chunks: IndexedChunk[] = []
  let start = 0
  while (start < normalized.length && chunks.length < limit) {
    let end = Math.min(normalized.length, start + TARGET_CHARS)
    if (end < normalized.length) {
      const paragraph = normalized.lastIndexOf('\n\n', end)
      const sentence = normalized.lastIndexOf('. ', end)
      const boundary = Math.max(paragraph, sentence)
      if (boundary > start + TARGET_CHARS * 0.55) end = boundary + (boundary === sentence ? 1 : 0)
    }
    const value = normalized.slice(start, end).trim()
    if (value) {
      const id = createHash('sha256')
        .update(`${sourceId}:${pageNumber ?? 0}:${start}:${value}`)
        .digest('hex')
      chunks.push({ id, pageNumber, text: value, startOffset: start, endOffset: end })
    }
    if (end >= normalized.length) break
    start = Math.max(start + 1, end - OVERLAP_CHARS)
    if (chunks.length % 32 === 0) {
      await new Promise<void>((resolve) => setImmediate(resolve))
      await yieldToInteractiveWork?.()
    }
  }
  return chunks
}

export async function chunksFromPages(
  sourceId: string,
  pages: IndexedPage[],
  limit = MAX_KNOWLEDGE_CHUNKS_PER_SOURCE,
  yieldToInteractiveWork?: () => Promise<void>,
): Promise<IndexedChunk[]> {
  const chunks: IndexedChunk[] = []
  for (const page of pages) {
    if (chunks.length >= limit) break
    const extracted = page.extractedText?.trim() ?? ''
    const ocr = page.ocr?.text.trim() ?? ''
    const combined = extracted && ocr && !extracted.includes(ocr) ? `${extracted}\n\n${ocr}` : extracted || ocr
    chunks.push(...await chunkText(
      sourceId,
      combined,
      page.pageNumber,
      limit - chunks.length,
      yieldToInteractiveWork,
    ))
  }
  return chunks
}
