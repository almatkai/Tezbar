import { describe, expect, it } from 'vitest'
import { cosineSimilarity, embedText, FEATURE_EMBEDDING_MODEL } from './featureEmbedding'

describe('local feature embeddings', () => {
  it('produces normalized vectors with the declared dimensions', () => {
    const vector = embedText('Сертификаттың мерзімі аяқталды')
    const magnitude = Math.sqrt(vector.reduce((sum, value) => sum + value * value, 0))
    expect(vector).toHaveLength(FEATURE_EMBEDDING_MODEL.dimensions)
    expect(magnitude).toBeCloseTo(1, 5)
  })

  it('ranks related multilingual character features above unrelated text', () => {
    const query = embedText('expired certificate error')
    const related = embedText('certificate expiration error on login')
    const unrelated = embedText('weekly grocery shopping list')
    expect(cosineSimilarity(query, related)).toBeGreaterThan(cosineSimilarity(query, unrelated))
  })
})
