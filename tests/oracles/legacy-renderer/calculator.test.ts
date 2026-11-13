import { describe, expect, it } from 'vitest'
import { evaluateExpression } from './calculator'

describe('evaluateExpression', () => {
  it('evaluates basic arithmetic', () => {
    expect(evaluateExpression('42 * 3.14')?.formatted).toBeCloseTo(131.88, 2)
    expect(evaluateExpression('2 + 3 * 4')?.formatted).toBe('14')
    expect(evaluateExpression('(2 + 3) * 4')?.formatted).toBe('20')
  })

  it('evaluates functions', () => {
    expect(evaluateExpression('sqrt(144)')?.formatted).toBe('12')
    expect(evaluateExpression('sin(90°)')?.formatted).toBeCloseTo(1, 6)
    expect(evaluateExpression('abs(-5)')?.formatted).toBe('5')
    expect(evaluateExpression('ln(e)')?.formatted).toBe('1')
    expect(evaluateExpression('max(1, 2)')?.formatted).toBe('2')
    expect(evaluateExpression('pow(2, 3)')?.formatted).toBe('8')
  })

  it('evaluates percent of', () => {
    expect(evaluateExpression('5% of 100')?.formatted).toBe('5')
    expect(evaluateExpression('12.5% of 80')?.formatted).toBe('10')
  })

  it('evaluates unit conversions', () => {
    expect(evaluateExpression('1 km to m')?.formatted).toBe('1000')
    expect(evaluateExpression('1 kg to g')?.formatted).toBe('1000')
    expect(evaluateExpression('32 f to c')?.formatted).toBeCloseTo(0, 6)
  })

  it('rejects plain words and non-math inputs', () => {
    expect(evaluateExpression('hello')).toBeNull()
    expect(evaluateExpression('pictures')).toBeNull()
    expect(evaluateExpression('5')).toBeNull()
  })

  it('rejects shell metacharacters', () => {
    expect(evaluateExpression('1 + 1; rm -rf /')).toBeNull()
    expect(evaluateExpression('$(whoami)')).toBeNull()
  })
})
