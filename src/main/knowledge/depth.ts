import type {
  IndexingCapability,
  KnowledgeDepth,
  KnowledgeRoot,
  KnowledgeSettings,
} from '../../shared/knowledge'

export const DEFAULT_KNOWLEDGE_SETTINGS: KnowledgeSettings = {
  depth: 'smart',
  maxConcurrentExtractors: 1,
  maxConcurrentOcrJobs: 1,
  runOnBattery: false,
  onlyRunHeavyJobsWhenIdle: true,
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

