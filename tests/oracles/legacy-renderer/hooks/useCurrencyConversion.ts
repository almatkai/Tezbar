import { useEffect, useState } from 'react'
import { convertCurrencyInput, type CurrencyConversionResult } from '../currency/currencyConversion'
import { parseCurrencyQuery } from '../currency/parseCurrencyQuery'
import { getPreferredDefaultTarget } from '../currency/currencyPreferences'

const DEBOUNCE_MS = 110
const TAG = '[currency]'

/**
 * Async currency row for the command bar: natural phrases + Frankfurter rates.
 * Skips work when the buffer cannot be a conversion (fast path via parse).
 */
export function useCurrencyConversion(
  value: string,
  isSlashInput: boolean,
): CurrencyConversionResult | null {
  const [result, setResult] = useState<CurrencyConversionResult | null>(null)

  useEffect(() => {
    if (isSlashInput) {
      setResult(null)
      return
    }

    const trimmed = value.trim()
    if (!trimmed) {
      setResult(null)
      return
    }

    const defaultTo = getPreferredDefaultTarget()
    const intent = parseCurrencyQuery(trimmed, defaultTo)
    console.debug(TAG, 'parse', { input: trimmed, defaultTo, intent })
    if (!intent) {
      setResult(null)
      return
    }

    if (typeof window === 'undefined' || !window.tezbar?.fetchFrankfurterLatest) {
      console.warn(TAG, 'preload missing: window.tezbar.fetchFrankfurterLatest is undefined — rebuild preload')
      setResult(null)
      return
    }

    let alive = true
    const timer = setTimeout(() => {
      void convertCurrencyInput(trimmed)
        .then((row) => {
          console.debug(TAG, 'result', { input: trimmed, row })
          if (alive) setResult(row)
        })
        .catch((err) => {
          console.error(TAG, 'convert failed', err)
          if (alive) setResult(null)
        })
    }, DEBOUNCE_MS)

    return () => {
      alive = false
      clearTimeout(timer)
    }
  }, [value, isSlashInput])

  return result
}
