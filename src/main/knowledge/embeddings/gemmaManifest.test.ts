import { describe, expect, it } from 'vitest'
import { gemmaDocument, gemmaQuery, gemmaRuntime, normalizeGemmaVector } from './gemmaManifest'

describe('EmbeddingGemma representations', () => {
  it('truncates projected embeddings and renormalizes them', () => {
    const result = normalizeGemmaVector(Array.from({ length: 768 }, (_, i) => i + 1))
    expect(result).toHaveLength(256)
    expect(result.reduce((sum, v) => sum + v * v, 0)).toBeCloseTo(1, 6)
    expect(result[1] / result[0]).toBeCloseTo(2)
  })

  it('rejects invalid, unprojected, and empty runtime output', () => {
    for (const invalid of [Array(512).fill(1), Array(768).fill(NaN), Array(768).fill(0), null]) {
      expect(() => normalizeGemmaVector(invalid)).toThrow()
    }
  })

  it('uses asymmetric retrieval prompts and preserves document titles', () => {
    expect(gemmaQuery('aurora')).toBe('task: search result | query: aurora')
    expect(gemmaDocument('solar particles', 'Physics')).toBe(
      'title: Physics | text: solar particles'
    )
    expect(gemmaDocument('solar particles')).toBe('title: none | text: solar particles')
  })

  it('only offers pinned runtime artifacts for supported platforms', () => {
    expect(gemmaRuntime('win32', 'x64')?.url).toContain('/b11474/')
    expect(gemmaRuntime('darwin', 'arm64')?.sha256).toHaveLength(64)
    expect(gemmaRuntime('linux', 'arm64')).toBeUndefined()
  })
})
