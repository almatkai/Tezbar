const DIMENSIONS = 384

export const FEATURE_EMBEDDING_MODEL = {
  id: 'tezbar-multilingual-feature-v1',
  version: '1.0.0',
  dimensions: DIMENSIONS,
} as const

function hash(value: string, seed: number): number {
  let current = seed >>> 0
  for (let index = 0; index < value.length; index += 1) {
    current ^= value.charCodeAt(index)
    current = Math.imul(current, 16777619)
  }
  return current >>> 0
}

/**
 * A dependency-free, multilingual character/word feature vector. This is the
 * always-available local fallback, not a neural model. Keeping it behind the
 * embedding interface lets a downloaded neural model replace it without a DB
 * or search API migration.
 */
export function embedText(value: string): number[] {
  const normalized = ` ${value.toLocaleLowerCase().normalize('NFKC').replace(/\s+/g, ' ').trim()} `
  const vector = new Float32Array(DIMENSIONS)
  const addFeature = (feature: string): void => {
    const bucket = hash(feature, 2166136261) % DIMENSIONS
    const sign = (hash(feature, 0x9e3779b9) & 1) === 0 ? 1 : -1
    vector[bucket] += sign
  }
  const words = normalized.trim().split(' ').filter(Boolean)
  for (const word of words) {
    addFeature(`w:${word}`)
    for (let size = 2; size <= 4; size += 1) {
      for (let index = 0; index + size <= word.length; index += 1) {
        addFeature(`c:${word.slice(index, index + size)}`)
      }
    }
  }
  let magnitude = 0
  for (const component of vector) magnitude += component * component
  magnitude = Math.sqrt(magnitude) || 1
  return Array.from(vector, (component) => component / magnitude)
}

export function cosineSimilarity(left: readonly number[], right: readonly number[]): number {
  const length = Math.min(left.length, right.length)
  if (length === 0) return 0
  let dot = 0
  let leftMagnitude = 0
  let rightMagnitude = 0
  for (let index = 0; index < length; index += 1) {
    const a = left[index] ?? 0
    const b = right[index] ?? 0
    dot += a * b
    leftMagnitude += a * a
    rightMagnitude += b * b
  }
  if (leftMagnitude === 0 || rightMagnitude === 0) return 0
  return dot / Math.sqrt(leftMagnitude * rightMagnitude)
}
