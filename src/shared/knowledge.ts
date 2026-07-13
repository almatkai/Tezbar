export type IndexingBackendId = 'local' | 'tezbar-cloud'

export type IndexingCapability =
  | 'extracted-text'
  | 'ocr'
  | 'chunks'
  | 'text-embeddings'
  | 'image-embeddings'
  | 'image-descriptions'

export type KnowledgeDepth = 'off' | 'basic' | 'smart' | 'deep'
export type KnowledgeRootDepth = 'inherit' | KnowledgeDepth

export type KnowledgeSettings = {
  depth: KnowledgeDepth
  runOnBattery: boolean
  onlyRunHeavyJobsWhenIdle: boolean
  maxConcurrentExtractors: number
  maxConcurrentOcrJobs: number
}

export type KnowledgeRoot = {
  id: string
  path: string
  depth: KnowledgeRootDepth
  processingBackend: 'local' | 'cloud' | 'ask'
  enabled: boolean
  createdAt: number
  updatedAt: number
}

export type SourceFingerprint = {
  contentHash: string
  byteSize: number
  modifiedAt: number
}

export type OcrBlock = {
  text: string
  confidence?: number
  bounds?: { x: number; y: number; width: number; height: number }
}

export type IndexedPage = {
  pageNumber: number
  extractedText?: string
  ocr?: {
    text: string
    blocks: OcrBlock[]
    averageConfidence?: number
  }
}

export type IndexedChunk = {
  id: string
  pageNumber?: number
  text: string
  embedding?: number[]
  startOffset?: number
  endOffset?: number
}

export type IndexedImage = {
  id: string
  pageNumber?: number
  ocrText?: string
  description?: string
  embedding?: number[]
  width: number
  height: number
}

export type IndexingRequest = {
  jobId: string
  rootId: string
  sourceId: string
  path: string
  fingerprint: SourceFingerprint
  depth: KnowledgeDepth
  requestedCapabilities: IndexingCapability[]
  maxPagesPerDocument: number | null
  maxOcrPagesPerDocument: number | null
  ocrEveryPage: boolean
}

export type IndexingEstimate = {
  sourceCount: number
  byteSize: number
  estimatedPages: number
  estimatedOcrPages: number
  estimatedStorageBytes: number
}

export type IndexingResult = {
  sourceId: string
  sourceHash: string
  extractor: { id: string; version: string }
  textEmbeddingModel?: { id: string; version: string; dimensions: number }
  imageEmbeddingModel?: { id: string; version: string; dimensions: number }
  pages: IndexedPage[]
  chunks: IndexedChunk[]
  images: IndexedImage[]
  completedCapabilities: IndexingCapability[]
  warnings: string[]
  totalPageCount?: number
}

export type IndexingContext = {
  signal: AbortSignal
  onProgress: (progress: number, detail?: string) => void
  /** Yield heavy local work while launcher/agent searches need the backend. */
  yieldToInteractiveWork?: () => Promise<void>
}

export interface IndexingBackend {
  id: IndexingBackendId
  estimate(request: IndexingRequest): Promise<IndexingEstimate>
  index(request: IndexingRequest, context: IndexingContext): Promise<IndexingResult>
  cancel(jobId: string): Promise<void>
}

export type KnowledgeJobState = 'idle' | 'scanning' | 'indexing' | 'paused' | 'completed' | 'failed'

export type KnowledgeStatus = {
  state: KnowledgeJobState
  backend: IndexingBackendId
  jobId?: string
  progress: number
  detail?: string
  queuedSources: number
  processedSources: number
  failedSources: number
  sourceCount: number
  chunkCount: number
  indexedPageCount: number
  totalPageCount: number
  partialSourceCount: number
  sourceBytes: number
  lastCompletedAt?: number
  error?: string
}

export type KnowledgeSourceSummary = {
  id: string
  path: string
  title: string
  byteSize: number
  modifiedAt: number
  indexedAt?: number
  status: 'pending' | 'indexed' | 'failed'
  error?: string
  totalPageCount: number
  indexedPageCount: number
}

export type KnowledgeSourcesPage = {
  sources: KnowledgeSourceSummary[]
  total: number
  offset: number
  hasMore: boolean
}

export type KnowledgeSearchHit = {
  chunkId: string
  sourceId: string
  path: string
  title: string
  pageNumber?: number
  text: string
  score: number
  lexicalScore: number
  semanticScore: number
}

export type KnowledgeReadResult = {
  resultId: string
  sourceId: string
  path: string
  title: string
  pageNumber?: number
  text: string
}

export type KnowledgeSnapshot = {
  roots: KnowledgeRoot[]
  status: KnowledgeStatus
  localBackendAvailable: boolean
  cloudBackendAvailable: false
  settings: KnowledgeSettings
  storageBytes: number
}
