import { fuzzyCurrencyFromToken } from './currencySynonyms'

export type CurrencyConversionIntent = {
  amount: number
  from: string
  to: string
  /** true when the user's query explicitly named the target currency. */
  targetExplicit: boolean
}

const SYMBOL_TO_CODE: Record<string, string> = {
  $: 'USD',
  '€': 'EUR',
  '£': 'GBP',
  '¥': 'JPY',
  '₽': 'RUB',
  '₸': 'KZT',
  '₹': 'INR',
  '฿': 'THB',
}

const STOP = new Set([
  'to',
  'in',
  'into',
  'at',
  'the',
  'a',
  'an',
  'of',
  'for',
  'and',
  'or',
  'в',
  'к',
  'на',
  'из',
  'по',
  'от',
  'до',
  'и',
])

/** Split on natural "to / in / в …" even when glued: `100$в рублях`. */
const SPLIT_RE = /\s*(?:into|to|in|→|=>|➝|в|к|на|из)\s+/i

export function looksLikePureArithmetic(input: string): boolean {
  const t = input.trim()
  if (/[$€£¥₽₸₹฿]/.test(t)) return false
  if (/[а-яёА-ЯЁ]/.test(t)) return false
  if (
    /\b(usd|eur|gbp|rub|kzt|chf|jpy|cny|inr|aud|cad|nzd|sek|pln|czk|huf|try|aed|sar|ils|thb|php|myr|idr|vnd|krw|hkd|sgd|mxn|brl|zar|ngn|egp|xof|xaf|uah)\b/i.test(
      t,
    )
  ) {
    return false
  }
  return /^[\d\s.+\-*/^(),%]+$/.test(t)
}

function parseLocalizedNumber(raw: string): number | null {
  const s = raw.replace(/\s/g, '')
  if (!s) return null
  let t = s
  if (t.includes(',') && t.includes('.')) {
    const lastComma = t.lastIndexOf(',')
    const lastDot = t.lastIndexOf('.')
    if (lastComma > lastDot) {
      t = t.replace(/\./g, '').replace(',', '.')
    } else {
      t = t.replace(/,/g, '')
    }
  } else if (t.includes(',') && !t.includes('.')) {
    const parts = t.split(',')
    if (parts.length === 2 && parts[1]!.length <= 2) {
      t = `${parts[0]}.${parts[1]}`
    } else {
      t = t.replace(/,/g, '')
    }
  }
  const n = Number.parseFloat(t)
  return Number.isFinite(n) ? n : null
}

function normalizeCurrencyPhrase(input: string): string {
  return input
    .replace(/(\d)\s*([$€£¥₽₸₹])\s*(?=[вкна])/gi, '$1$2 ')
    .replace(/([$€£¥₽₸₹])\s*(?=[вкна])/gi, '$1 ')
}

function tokenizeForCurrency(rest: string): string[] {
  const cleaned = rest
    .replace(/[→=>➝]/g, ' ')
    .replace(/[^\d\s.a-zA-Zа-яёА-ЯЁ€$£¥₽₸₹฿]/gu, ' ')
  return cleaned
    .split(/\s+/)
    .map((x) => x.trim())
    .filter((x) => x.length > 0 && !STOP.has(x.toLowerCase()))
}

function firstCurrencyInText(text: string): string | null {
  for (const tok of tokenizeForCurrency(text)) {
    const c = fuzzyCurrencyFromToken(tok)
    if (c) return c
  }
  const compact = text.replace(/\s+/g, '')
  if (compact.length >= 3) {
    return fuzzyCurrencyFromToken(compact)
  }
  return null
}

type Extracted = {
  amount: number
  fromSymbol?: string
  rest: string
}

function extractAmountAndSymbols(input: string): Extracted | null {
  const s = input.trim()

  let m = s.match(/^\s*\$\s*([\d]+(?:[.,]\d+)?)\s*/i)
  if (m) {
    const amt = parseLocalizedNumber(m[1]!)
    if (amt === null || !(amt > 0)) return null
    return { amount: amt, fromSymbol: '$', rest: s.slice(m[0].length).trim() }
  }

  m = s.match(/^\s*([\d]+(?:[.,]\d+)?)\s*\$\s*/i)
  if (m) {
    const amt = parseLocalizedNumber(m[1]!)
    if (amt === null || !(amt > 0)) return null
    return { amount: amt, fromSymbol: '$', rest: s.slice(m[0].length).trim() }
  }

  m = s.match(/^\s*([€£₽₸¥₹])\s*([\d]+(?:[.,]\d+)?)\s*/u)
  if (m) {
    const sym = m[1]!
    const amt = parseLocalizedNumber(m[2]!)
    if (amt === null || !(amt > 0)) return null
    return { amount: amt, fromSymbol: sym, rest: s.slice(m[0].length).trim() }
  }

  m = s.match(/^\s*([\d]+(?:[.,]\d+)?)\s*([€£₽₸¥₹])\s*/u)
  if (m) {
    const amt = parseLocalizedNumber(m[1]!)
    const sym = m[2]!
    if (amt === null || !(amt > 0)) return null
    return { amount: amt, fromSymbol: sym, rest: s.slice(m[0].length).trim() }
  }

  m = s.match(/^\s*([\d]+(?:[.,]\d+)?)\s*/)
  if (m) {
    const amt = parseLocalizedNumber(m[1]!)
    if (amt === null || !(amt > 0)) return null
    return { amount: amt, rest: s.slice(m[0].length).trim() }
  }

  return null
}

/**
 * Match amount-with-percent-sign on the left side. `100%`, `15 % of 200` etc.
 * must go to the calculator, never to currency, even if followed by "в тенге".
 */
const LEFT_PERCENT_RE = /\d\s*%/

/**
 * Parse a natural currency conversion phrase.
 * `defaultTo` should come from `inferDefaultCurrencyCode()` (locale-driven).
 */
export function parseCurrencyQuery(input: string, defaultTo: string): CurrencyConversionIntent | null {
  const raw = normalizeCurrencyPhrase(input).trim()
  if (!raw || looksLikePureArithmetic(input)) return null

  const parts = raw
    .split(SPLIT_RE)
    .map((p) => p.trim())
    .filter(Boolean)
  if (parts.length >= 2) {
    const left = parts[0]!
    if (LEFT_PERCENT_RE.test(left)) return null
    const right = parts.slice(1).join(' ')
    const le = extractAmountAndSymbols(left)
    if (!le) return null

    let from: string | null = le.fromSymbol ? (SYMBOL_TO_CODE[le.fromSymbol] ?? null) : null
    if (!from) {
      from = firstCurrencyInText(le.rest) ?? firstCurrencyInText(left)
    }

    const to = firstCurrencyInText(right)
    if (!to) return null
    if (!from) {
      from = defaultTo
    }
    if (from === to) return null
    return { amount: le.amount, from, to, targetExplicit: true }
  }

  if (LEFT_PERCENT_RE.test(raw)) return null

  const ex = extractAmountAndSymbols(raw)
  if (!ex) return null
  if (!(ex.amount > 0)) return null

  const tokens = tokenizeForCurrency(ex.rest)
  const found: string[] = []
  for (const tok of tokens) {
    const c = fuzzyCurrencyFromToken(tok)
    if (c && !found.includes(c)) found.push(c)
  }

  let from: string | null = ex.fromSymbol ? (SYMBOL_TO_CODE[ex.fromSymbol] ?? null) : null
  if (!from) {
    from = found[0] ?? null
  }

  if (!from) return null

  let to = defaultTo
  let targetExplicit = false
  if (found.length >= 2) {
    to = found[1]!
    targetExplicit = true
  } else if (found.length === 1 && found[0] !== from) {
    to = found[0]!
    targetExplicit = true
  }

  if (from === to) return null
  return { amount: ex.amount, from, to, targetExplicit }
}
