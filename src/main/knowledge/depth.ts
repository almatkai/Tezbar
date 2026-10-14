import type {
  IndexingCapability,
  KnowledgeDepth,
  KnowledgeIndexingSpeed,
  KnowledgeRoot,
  KnowledgeSettings,
} from '../../shared/knowledge'

export const DEFAULT_KNOWLEDGE_SETTINGS: KnowledgeSettings = {
  depth: 'smart',
  embeddingModel: 'built-in',
  indexingSpeed: 'medium',
  maxConcurrentExtractors: 1,
  maxConcurrentOcrJobs: 1,
  runOnBattery: false,
  onlyRunHeavyJobsWhenIdle: true,
}

export type KnowledgeSpeedProfile = {
  gemmaBatchSize: number
  gemmaDelayMs: number
  gemmaMaxThreads: number
  workerFileDelayMs: number
  vectorBackfillBatchSize: number
  vectorBackfillDelayMs: number
}

export const SPEED_PROFILES: Record<KnowledgeIndexingSpeed, KnowledgeSpeedProfile> = {
  slow: {
    gemmaBatchSize: 4,
    gemmaDelayMs: 350,
    gemmaMaxThreads: 2,
    workerFileDelayMs: 120,
    vectorBackfillBatchSize: 128,
    vectorBackfillDelayMs: 500,
  },
  medium: {
    gemmaBatchSize: 8,
    gemmaDelayMs: 75,
    gemmaMaxThreads: 4,
    workerFileDelayMs: 40,
    vectorBackfillBatchSize: 256,
    vectorBackfillDelayMs: 250,
  },
  fast: {
    gemmaBatchSize: 16,
    gemmaDelayMs: 0,
    gemmaMaxThreads: 8,
    workerFileDelayMs: 0,
    vectorBackfillBatchSize: 512,
    vectorBackfillDelayMs: 50,
  },
}

export type KnowledgeDepthProfile = {
  depth: Exclude<KnowledgeDepth, 'off'>
  requestedCapabilities: IndexingCapability[]
  maxPagesPerDocument: number | null
  maxOcrPagesPerDocument: number | null
  ocrEveryPage: boolean
}

const PROFILES: Record<Exclude<KnowledgeDepth, 'off'>, KnowledgeDepthProfile> = {
  basic: {
    depth: 'basic',
    requestedCapabilities: ['extracted-text', 'chunks'],
    maxPagesPerDocument: 20,
    maxOcrPagesPerDocument: 0,
    ocrEveryPage: false,
  },
  smart: {
    depth: 'smart',
    requestedCapabilities: ['extracted-text', 'ocr', 'chunks', 'text-embeddings'],
    maxPagesPerDocument: null,
    maxOcrPagesPerDocument: 20,
    ocrEveryPage: false,
  },
  deep: {
    depth: 'deep',
    requestedCapabilities: ['extracted-text', 'ocr', 'chunks', 'text-embeddings'],
    maxPagesPerDocument: null,
    maxOcrPagesPerDocument: null,
    ocrEveryPage: true,
  },
}

export function effectiveKnowledgeDepth(
  root: KnowledgeRoot,
  settings: KnowledgeSettings,
): KnowledgeDepth {
  return root.depth === 'inherit' ? settings.depth : root.depth
}

export function profileForDepth(depth: KnowledgeDepth): KnowledgeDepthProfile | null {
  return depth === 'off' ? null : PROFILES[depth]
}

export function indexingProfileKey(profile: KnowledgeDepthProfile): string {
  return JSON.stringify({
    depth: profile.depth,
    capabilities: profile.requestedCapabilities,
    maxPagesPerDocument: profile.maxPagesPerDocument,
    maxOcrPagesPerDocument: profile.maxOcrPagesPerDocument,
    ocrEveryPage: profile.ocrEveryPage,
  })
}
