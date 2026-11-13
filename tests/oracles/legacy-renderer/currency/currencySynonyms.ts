import Fuse from 'fuse.js'

/** One synonym per row so Fuse can score individual strings. */
export type CurrencySynonymRow = { code: string; term: string }

/** Curated synonyms + ISO codes; fuzzy handles declensions/typos nearby. */
export const CURRENCY_SYNONYMS: CurrencySynonymRow[] = [
  { code: 'USD', term: 'usd' },
  { code: 'USD', term: 'dollar' },
  { code: 'USD', term: 'dollars' },
  { code: 'USD', term: 'доллар' },
  { code: 'USD', term: 'доллара' },
  { code: 'USD', term: 'долларов' },
  { code: 'USD', term: 'бакс' },
  { code: 'USD', term: 'баксы' },
  { code: 'EUR', term: 'eur' },
  { code: 'EUR', term: 'euro' },
  { code: 'EUR', term: 'euros' },
  { code: 'EUR', term: 'евро' },
  { code: 'GBP', term: 'gbp' },
  { code: 'GBP', term: 'pound' },
  { code: 'GBP', term: 'pounds' },
  { code: 'GBP', term: 'sterling' },
  { code: 'GBP', term: 'фунт' },
  { code: 'GBP', term: 'фунта' },
  { code: 'GBP', term: 'фунтов' },
  { code: 'KZT', term: 'kzt' },
  { code: 'KZT', term: 'tenge' },
  { code: 'KZT', term: 'тенге' },
  { code: 'KZT', term: 'тг' },
  { code: 'RUB', term: 'rub' },
  { code: 'RUB', term: 'ruble' },
  { code: 'RUB', term: 'rubles' },
  { code: 'RUB', term: 'руб' },
  { code: 'RUB', term: 'рубль' },
  { code: 'RUB', term: 'рубля' },
  { code: 'RUB', term: 'рублей' },
  { code: 'RUB', term: 'рублях' },
  { code: 'RUB', term: 'рублём' },
  { code: 'CHF', term: 'chf' },
  { code: 'CHF', term: 'franc' },
  { code: 'CHF', term: 'swiss' },
  { code: 'JPY', term: 'jpy' },
  { code: 'JPY', term: 'yen' },
  { code: 'JPY', term: 'йена' },
  { code: 'CNY', term: 'cny' },
  { code: 'CNY', term: 'yuan' },
  { code: 'CNY', term: 'rmb' },
  { code: 'CNY', term: 'юань' },
  { code: 'INR', term: 'inr' },
  { code: 'INR', term: 'rupee' },
  { code: 'INR', term: 'rupiah' },
  { code: 'TRY', term: 'try' },
  { code: 'TRY', term: 'lira' },
  { code: 'TRY', term: 'лира' },
  { code: 'UAH', term: 'uah' },
  { code: 'UAH', term: 'hryvnia' },
  { code: 'UAH', term: 'гривна' },
  { code: 'UAH', term: 'гривны' },
  { code: 'UAH', term: 'гривен' },
  { code: 'PLN', term: 'pln' },
  { code: 'PLN', term: 'zloty' },
  { code: 'PLN', term: 'złoty' },
  { code: 'CZK', term: 'czk' },
  { code: 'CZK', term: 'koruna' },
  { code: 'SEK', term: 'sek' },
  { code: 'SEK', term: 'krona' },
  { code: 'NOK', term: 'nok' },
  { code: 'DKK', term: 'dkk' },
  { code: 'DKK', term: 'krone' },
  { code: 'HUF', term: 'huf' },
  { code: 'HUF', term: 'forint' },
  { code: 'RON', term: 'ron' },
  { code: 'RON', term: 'leu' },
  { code: 'BGN', term: 'bgn' },
  { code: 'AUD', term: 'aud' },
  { code: 'CAD', term: 'cad' },
  { code: 'NZD', term: 'nzd' },
  { code: 'SGD', term: 'sgd' },
  { code: 'HKD', term: 'hkd' },
  { code: 'MXN', term: 'mxn' },
  { code: 'MXN', term: 'mexican' },
  { code: 'BRL', term: 'brl' },
  { code: 'BRL', term: 'real' },
  { code: 'ZAR', term: 'zar' },
  { code: 'ZAR', term: 'rand' },
  { code: 'AED', term: 'aed' },
  { code: 'AED', term: 'dirham' },
  { code: 'SAR', term: 'sar' },
  { code: 'SAR', term: 'riyal' },
  { code: 'ILS', term: 'ils' },
  { code: 'ILS', term: 'shekel' },
  { code: 'THB', term: 'thb' },
  { code: 'THB', term: 'baht' },
  { code: 'KRW', term: 'krw' },
  { code: 'KRW', term: 'won' },
  { code: 'TWD', term: 'twd' },
  { code: 'IDR', term: 'idr' },
  { code: 'MYR', term: 'myr' },
  { code: 'PHP', term: 'php' },
  { code: 'PHP', term: 'peso' },
  { code: 'VND', term: 'vnd' },
  { code: 'VND', term: 'dong' },
  { code: 'EGP', term: 'egp' },
  { code: 'EGP', term: 'egpound' },
]

const fuse = new Fuse(CURRENCY_SYNONYMS, {
  keys: ['term'],
  threshold: 0.42,
  ignoreLocation: true,
  minMatchCharLength: 2,
  isCaseSensitive: false,
  includeScore: true,
})

const ISO_CODE_RE = /^[A-Za-z]{3}$/

/** Map a raw token to ISO 4217, or null if no confident match. */
export function fuzzyCurrencyFromToken(token: string): string | null {
  const t = token.trim()
  if (t.length < 2) return null
  if (ISO_CODE_RE.test(t)) return t.toUpperCase()
  const hits = fuse.search(t, { limit: 1 })
  const best = hits[0]
  if (!best) return null
  // Fuse: 0 = perfect, 1 = worst — score may be omitted without includeScore
  const sc = best.score ?? 0
  if (sc > 0.38) return null
  const row = best.item
  return row?.code ?? null
}
