/**
 * Lightweight, safe expression evaluator for the command-bar calculator.
 *
 * Replaces the full mathjs dependency with a tiny whitelist-based evaluator
 * that covers the arithmetic, functions, and unit conversions Raymes actually
 * uses. No external math library required.
 */

export type CalcResult = {
  /** Original trimmed input (what the user typed). */
  expression: string
  /** Display string — e.g. "3.1415". */
  formatted: string
  /** Value suitable for clipboard. Always a plain string. */
  clipboard: string
}

const OPERATOR_RE = /[+\-*/^%()]/
const FUNCTION_CALL_RE = /[a-zA-Z_][a-zA-Z0-9_]*\s*\(/
const UNIT_CONVERSION_RE = /^\s*(-?\d+(?:\.\d+)?(?:e[+-]?\d+)?)\s*([a-zA-Z°]+)\s+(?:to|in)\s+([a-zA-Z°]+)\s*$/i
const PERCENT_OF_RE = /^(\d+(?:\.\d+)?)\s*%\s+of\s+(\d+(?:\.\d+)?)$/i

const ALLOWED_FUNCTIONS: Record<string, string> = {
  sqrt: 'Math.sqrt',
  cbrt: 'Math.cbrt',
  sin: 'Math.sin',
  cos: 'Math.cos',
  tan: 'Math.tan',
  asin: 'Math.asin',
  acos: 'Math.acos',
  atan: 'Math.atan',
  log: 'Math.log',
  ln: 'Math.log',
  log10: 'Math.log10',
  log2: 'Math.log2',
  abs: 'Math.abs',
  floor: 'Math.floor',
  ceil: 'Math.ceil',
  round: 'Math.round',
  trunc: 'Math.trunc',
  exp: 'Math.exp',
  max: 'Math.max',
  min: 'Math.min',
  pow: 'Math.pow',
  sign: 'Math.sign',
}

const ALLOWED_CONSTANTS: Record<string, string> = {
  pi: 'Math.PI',
  e: 'Math.E',
}

// Whitelist of characters that may appear in a numeric/math expression.
const SAFE_EXPR_RE = /^[0-9a-zA-Z_+\-*/%^().,\s°]+$/

const LENGTH_UNITS: Record<string, number> = {
  m: 1,
  km: 1000,
  cm: 0.01,
  mm: 0.001,
  mi: 1609.344,
  yd: 0.9144,
  ft: 0.3048,
  in: 0.0254,
}

const MASS_UNITS: Record<string, number> = {
  g: 1,
  kg: 1000,
  mg: 0.001,
  lb: 453.59237,
  oz: 28.34952,
}

function looksLikeMath(input: string): boolean {
  if (OPERATOR_RE.test(input)) return true
  if (FUNCTION_CALL_RE.test(input)) return true
  if (UNIT_CONVERSION_RE.test(input)) return true
  if (PERCENT_OF_RE.test(input)) return true
  return false
}

function convertTemperature(value: number, from: string, to: string): number | null {
  const normalize = (u: string): 'c' | 'f' | 'k' | null => {
    const lower = u.toLowerCase()
    if (lower === 'c' || lower === 'celsius' || lower === '°c') return 'c'
    if (lower === 'f' || lower === 'fahrenheit' || lower === '°f') return 'f'
    if (lower === 'k' || lower === 'kelvin' || lower === '°k') return 'k'
    return null
  }
  const nf = normalize(from)
  const nt = normalize(to)
  if (!nf || !nt) return null

  let celsius: number
  if (nf === 'c') celsius = value
  else if (nf === 'f') celsius = (value - 32) * (5 / 9)
  else celsius = value - 273.15

  if (nt === 'c') return celsius
  if (nt === 'f') return celsius * (9 / 5) + 32
  return celsius + 273.15
}

function convertUnits(value: number, from: string, to: string): number | null {
  const f = from.toLowerCase().replace(/°/g, '')
  const t = to.toLowerCase().replace(/°/g, '')

  const temp = convertTemperature(value, from, to)
  if (temp !== null) return temp

  if (LENGTH_UNITS[f] !== undefined && LENGTH_UNITS[t] !== undefined) {
    return (value * LENGTH_UNITS[f]!) / LENGTH_UNITS[t]!
  }
  if (MASS_UNITS[f] !== undefined && MASS_UNITS[t] !== undefined) {
    return (value * MASS_UNITS[f]!) / MASS_UNITS[t]!
  }
  return null
}

function preprocessExpression(input: string): string {
  // Tokenize identifiers and rewrite allowed functions/constants.
  let expr = input.replace(/[a-zA-Z_][a-zA-Z0-9_]*/g, (token) => {
    const lower = token.toLowerCase()
    if (ALLOWED_FUNCTIONS[lower]) return ALLOWED_FUNCTIONS[lower]
    if (ALLOWED_CONSTANTS[lower]) return ALLOWED_CONSTANTS[lower]
    throw new Error(`Unknown identifier: ${token}`)
  })

  // Replace the degree symbol with * (Math.PI / 180) so that sin(45°)
  // becomes Math.sin(45 * (Math.PI / 180)).
  expr = expr.replace(/°/g, ' * (Math.PI / 180)')

  // Replace ^ with ** for JavaScript exponentiation.
  expr = expr.replace(/\^/g, '**')

  return expr
}

function evaluateMath(expression: string): number | null {
  const trimmed = expression.trim()
  if (!SAFE_EXPR_RE.test(trimmed)) return null

  try {
    const js = preprocessExpression(trimmed)
    const result = new Function('Math', `return (${js})`)(Math)
    if (typeof result === 'number' && Number.isFinite(result)) return result
    return null
  } catch {
    return null
  }
}

export function evaluateExpression(rawInput: string): CalcResult | null {
  const expression = rawInput.trim()
  if (!expression) return null

  const percentMatch = PERCENT_OF_RE.exec(expression)
  if (percentMatch) {
    const pct = parseFloat(percentMatch[1]!)
    const of = parseFloat(percentMatch[2]!)
    if (!Number.isFinite(pct) || !Number.isFinite(of)) return null
    const result = (pct / 100) * of
    const formatted = String(result)
    return { expression, formatted, clipboard: formatted }
  }

  const unitMatch = UNIT_CONVERSION_RE.exec(expression)
  if (unitMatch) {
    const value = parseFloat(unitMatch[1]!)
    const from = unitMatch[2]!
    const to = unitMatch[3]!
    if (!Number.isFinite(value)) return null
    const converted = convertUnits(value, from, to)
    if (converted === null) return null
    const formatted = String(converted)
    return { expression, formatted, clipboard: formatted }
  }

  if (!looksLikeMath(expression)) return null

  const result = evaluateMath(expression)
  if (result === null) return null

  const formatted = String(result)
  if (formatted === expression) return null
  return { expression, formatted, clipboard: formatted }
}
