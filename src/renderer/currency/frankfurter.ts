import type { FrankfurterLatestResponse } from '../../preload/api'

/**
 * Reads ECB-backed rates from the main process (Node fetch, no CORS).
 */
export async function fetchFrankfurterLatest(from: string): Promise<FrankfurterLatestResponse> {
  return window.tezbar.fetchFrankfurterLatest(from.trim().toUpperCase())
}

export async function getConversionRate(from: string, to: string): Promise<number> {
  const a = from.toUpperCase()
  const b = to.toUpperCase()
  if (a === b) return 1
  const data = await fetchFrankfurterLatest(a)
  const r = data.rates[b]
  if (r === undefined) {
    throw new Error(`No rate ${a} → ${b}`)
  }
  return r
}
