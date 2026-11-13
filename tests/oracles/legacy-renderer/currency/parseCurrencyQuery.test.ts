import { describe, expect, it } from 'vitest'
import { looksLikePureArithmetic, parseCurrencyQuery } from './parseCurrencyQuery'

describe('parseCurrencyQuery', () => {
  const kzt = 'KZT'

  it('parses amount + dollar to default currency', () => {
    expect(parseCurrencyQuery('1$', kzt)).toEqual({ amount: 1, from: 'USD', to: 'KZT', targetExplicit: false })
    expect(parseCurrencyQuery('$100', kzt)).toEqual({ amount: 100, from: 'USD', to: 'KZT', targetExplicit: false })
  })

  it('parses Russian preposition and fuzzy ruble', () => {
    expect(parseCurrencyQuery('100$ в рублях', kzt)).toEqual({ amount: 100, from: 'USD', to: 'RUB', targetExplicit: true })
  })

  it('parses English in + code', () => {
    expect(parseCurrencyQuery('100$ in eur', kzt)).toEqual({ amount: 100, from: 'USD', to: 'EUR', targetExplicit: true })
  })

  it('parses 100$ to rub', () => {
    expect(parseCurrencyQuery('100$ to rub', kzt)).toEqual({ amount: 100, from: 'USD', to: 'RUB', targetExplicit: true })
  })

  it('parses explicit usd to eur', () => {
    expect(parseCurrencyQuery('100 usd to eur', kzt)).toEqual({ amount: 100, from: 'USD', to: 'EUR', targetExplicit: true })
  })

  it('parses amount + eur to default when no target', () => {
    expect(parseCurrencyQuery('50 eur', kzt)).toEqual({ amount: 50, from: 'EUR', to: 'KZT', targetExplicit: false })
  })

  it('rejects pure arithmetic', () => {
    expect(parseCurrencyQuery('1+1', kzt)).toBeNull()
    expect(parseCurrencyQuery('sqrt(4)', kzt)).toBeNull()
  })

  it('rejects percentage phrases even with preposition + currency word', () => {
    expect(parseCurrencyQuery('100% в тенге', kzt)).toBeNull()
    expect(parseCurrencyQuery('15% of 200', kzt)).toBeNull()
    expect(parseCurrencyQuery('100%', kzt)).toBeNull()
  })
})

describe('looksLikePureArithmetic', () => {
  it('detects math-only buffers', () => {
    expect(looksLikePureArithmetic('1+1')).toBe(true)
    expect(looksLikePureArithmetic(' 2 * 3 ')).toBe(true)
  })

  it('allows currency hints through', () => {
    expect(looksLikePureArithmetic('1$')).toBe(false)
    expect(looksLikePureArithmetic('100 usd')).toBe(false)
  })
})
