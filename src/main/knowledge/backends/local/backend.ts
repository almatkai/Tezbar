import { statSync } from 'node:fs'
import type {
  IndexingBackend,
  IndexingContext,
  IndexingEstimate,
  IndexingRequest,
  IndexingResult,
} from '../../../../shared/knowledge'
import { chunksFromPages, MAX_KNOWLEDGE_CHUNKS_PER_SOURCE } from '../../core/chunker'
import { embedText, FEATURE_EMBEDDING_MODEL } from '../../embeddings/featureEmbedding'
import { extractLocally } from '../../extractors/localExtractor'
import { redactSensitiveText } from '../../../safety/redaction'

export class LocalIndexingBackend implements IndexingBackend {
  readonly id = 'local' as const
  private readonly jobs = new Map<string, AbortController>()

  async estimate(request: IndexingRequest): Promise<IndexingEstimate> {
    const stat = statSync(request.path)
    const estimatedPages = request.path.toLowerCase().endsWith('.pdf')
      ? Math.max(1, Math.ceil(stat.size / 75_000))
      : 1
    return {
      sourceCount: 1,
      byteSize: stat.size,
      estimatedPages,
      estimatedOcrPages: request.requestedCapabilities.includes('ocr') && request.path.match(/\.(?:png|jpe?g|heic|heif|tiff?|bmp|gif|webp)$/i)
        ? 1
        : 0,
      estimatedStorageBytes: Math.min(stat.size * 2, 128 * 1024 * 1024),
    }
  }

  async index(request: IndexingRequest, context: IndexingContext): Promise<IndexingResult> {
    const controller = new AbortController()
    const abort = (): void => controller.abort()
    context.signal.addEventListener('abort', abort, { once: true })
    this.jobs.set(request.jobId, controller)
    try {
      context.onProgress(0.08, 'Extracting text')
      const extraction = await extractLocally(request.path, controller.signal, {
        maxPagesPerDocument: request.maxPagesPerDocument,
        enableOcr: request.requestedCapabilities.includes('ocr'),
        maxOcrPagesPerDocument: request.maxOcrPagesPerDocument,
        ocrEveryPage: request.ocrEveryPage,
      })
      for (const page of extraction.pages) {
        if (page.extractedText) page.extractedText = redactSensitiveText(page.extractedText)
        if (page.ocr) {
          page.ocr.text = redactSensitiveText(page.ocr.text)
          page.ocr.blocks = page.ocr.blocks.map((block) => ({
            ...block,
            text: redactSensitiveText(block.text),
          }))
        }
      }
      for (const image of extraction.images) {
        if (image.ocrText) image.ocrText = redactSensitiveText(image.ocrText)
        if (image.description) image.description = redactSensitiveText(image.description)
      }
      if (controller.signal.aborted) throw new Error('Indexing cancelled')

      context.onProgress(0.62, 'Creating search chunks')
      await context.yieldToInteractiveWork?.()
      const chunks = await chunksFromPages(
        request.sourceId,
        extraction.pages,
        MAX_KNOWLEDGE_CHUNKS_PER_SOURCE,
        context.yieldToInteractiveWork,
      )
      const shouldEmbedText = request.requestedCapabilities.includes('text-embeddings')
      if (shouldEmbedText) {
        context.onProgress(0.76, 'Creating local search vectors')
        for (let index = 0; index < chunks.length; index += 1) {
          if (controller.signal.aborted) throw new Error('Indexing cancelled')
          const chunk = chunks[index]
          if (chunk) chunk.embedding = embedText(chunk.text)
          await new Promise<void>((resolve) => setImmediate(resolve))
          await context.yieldToInteractiveWork?.()
        }
      }

      const completedCapabilities = new Set(extraction.completedCapabilities)
      completedCapabilities.add('chunks')
      if (chunks.length > 0 && shouldEmbedText) completedCapabilities.add('text-embeddings')
      context.onProgress(1, 'Ready')
      return {
        sourceId: request.sourceId,
        sourceHash: request.fingerprint.contentHash,
        extractor: extraction.extractor,
        textEmbeddingModel: chunks.length > 0 && shouldEmbedText
          ? FEATURE_EMBEDDING_MODEL
          : undefined,
        pages: extraction.pages,
        chunks,
        images: extraction.images,
        completedCapabilities: Array.from(completedCapabilities),
        warnings: chunks.length >= MAX_KNOWLEDGE_CHUNKS_PER_SOURCE
          ? [...extraction.warnings, `Limited this source to ${MAX_KNOWLEDGE_CHUNKS_PER_SOURCE} chunks.`]
          : extraction.warnings,
        totalPageCount: extraction.totalPageCount,
      }
    } finally {
      context.signal.removeEventListener('abort', abort)
      this.jobs.delete(request.jobId)
    }
  }

  async cancel(jobId: string): Promise<void> {
    this.jobs.get(jobId)?.abort()
  }
}
