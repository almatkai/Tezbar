import type {
  IndexingBackend,
  IndexingContext,
  IndexingEstimate,
  IndexingRequest,
  IndexingResult,
} from '../../../../shared/knowledge'

export class TezbarCloudIndexingBackend implements IndexingBackend {
  readonly id = 'tezbar-cloud' as const

  async estimate(request: IndexingRequest): Promise<IndexingEstimate> {
    void request
    throw new Error('Tezbar Cloud indexing is not available yet.')
  }

  async index(request: IndexingRequest, context: IndexingContext): Promise<IndexingResult> {
    void request
    void context
    throw new Error('Tezbar Cloud indexing is not available yet.')
  }

  async cancel(jobId: string): Promise<void> {
    void jobId
  }
}
