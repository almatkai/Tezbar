import type { CalcResult } from '../calculator'
import { getConversionRate } from './frankfurter'
import { parseCurrencyQuery } from './parseCurrencyQuery'
import { getPreferredDefaultTarget, recordTargetUsage } from './currencyPreferences'

const TAG = '[currency]'

export type CurrencyConversionResult = CalcResult & {
  /** Amount of the source currency (copied from the parsed intent). */
  amount: number
  /** ISO 4217 source currency (e.g. `USD`). */
  from: string
  /** ISO 4217 target currency (e.g. `KZT`). */
  to: string
  /** Source currency amount formatted via the user's locale (e.g. `$1.00`). */
  amountFormatted: string
}

function formatAmount(amount: number, code: string): string {
  try {
    return new Intl.NumberFormat(undefined, {
      style: 'currency',
      currency: code,
      maximumFractionDigits: 4,
    }).format(amount)
  } catch {
    return `${amount} ${code}`
  }
}

export async function convertCurrencyInput(rawInput: string): Promise<CurrencyConversionResult | null> {
  const trimmed = rawInput.trim()
  if (!trimmed) return null

  const defaultTo = getPreferredDefaultTarget()
  const intent = parseCurrencyQuery(trimmed, defaultTo)
  if (!intent) {
    console.debug(TAG, 'convertCurrencyInput: no intent', { trimmed, defaultTo })
    return null
  }

  try {
    console.debug(TAG, 'rate request', intent)
    const rate = await getConversionRate(intent.from, intent.to)
    const converted = intent.amount * rate
    console.debug(TAG, 'rate ok', { ...intent, rate, converted })

    const targetFmt = new Intl.NumberFormat(undefined, {
      style: 'currency',
      currency: intent.to,
      maximumFractionDigits: 2,
    })
    const out = targetFmt.format(converted)
    if (intent.targetExplicit) {
      recordTargetUsage(intent.to)
    }
    return {
      expression: trimmed,
      formatted: out,
      clipboard: out,
      amount: intent.amount,
      from: intent.from,
      to: intent.to,
      amountFormatted: formatAmount(intent.amount, intent.from),
    }
  } catch (err) {
    console.error(TAG, 'rate error', intent, err)
    return null
  }
}
