// src/shared/textTransformers.ts
//
// Pure transform engine for clipboard entries. Every transform takes a string
// (typically the current clipboard item's text) and returns a tri-state result
// so the ClipboardView can render an inline output / error without any try/
// catch machinery at the call site.
//
// Environment contract: Tauri webview (DOM available, no Node). Tests run in
// Node's vitest environment, so every browser primitive used here (TextEncoder,
// btoa/atob, crypto.subtle) has a Node equivalent or an explicit fallback.

export type TransformResult =
  | { ok: true; result: string }
  | { ok: false; error: string }

export type HashAlgorithm = 'md5' | 'sha256' | 'sha512'

export type CaseStyle =
  | 'camel'
  | 'snake'
  | 'kebab'
  | 'constant'
  | 'title'
  | 'lower'
  | 'upper'

/* ---------------------------------------------------------------- helpers */

const ok = (result: string): TransformResult => ({ ok: true, result })
const err = (error: string): TransformResult => ({ ok: false, error })

const errorMessage = (e: unknown): string =>
  e instanceof Error ? e.message : typeof e === 'string' ? e : String(e)

/** UTF-8 encode without Node's Buffer so the module works the same in the
 *  Tauri webview and in vitest's node environment. */
export function utf8Bytes(input: string): Uint8Array {
  return new TextEncoder().encode(input)
}

export function utf8FromBytes(bytes: Uint8Array): string {
  // fatal:false so truncated sequences / lone surrogates decode to U+FFFD
  // instead of throwing — the caller-friendly behaviour for clipboard text.
  return new TextDecoder('utf-8', { fatal: false }).decode(bytes)
}

function bytesToBinaryString(bytes: Uint8Array): string {
  // Avoid spread-on-large-array stack overflows: chunk.
  let out = ''
  const CHUNK = 0x8000
  for (let i = 0; i < bytes.length; i += CHUNK) {
    out += String.fromCharCode(...bytes.subarray(i, i + CHUNK))
  }
  return out
}

function binaryStringToBytes(bin: string): Uint8Array {
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i) & 0xff
  return out
}

function getBtoa(): (s: string) => string {
  if (typeof btoa === 'function') return btoa
  // Node test env fallback.
  const g = globalThis as unknown as { Buffer?: { from(s: string, e: string): { toString(e: string): string } } }
  if (g.Buffer) return (s) => g.Buffer!.from(s, 'binary').toString('base64')
  throw new Error('base64 encoder unavailable in this environment')
}

function getAtob(): (s: string) => string {
  if (typeof atob === 'function') return atob
  const g = globalThis as unknown as { Buffer?: { from(s: string, e: string): { toString(e: string): string } } }
  if (g.Buffer) return (s) => g.Buffer!.from(s, 'base64').toString('binary')
  throw new Error('base64 decoder unavailable in this environment')
}

/* ------------------------------------------------------------- JSON ----- */

export function jsonFormat(input: string, indent = 2): TransformResult {
  const text = input.trim()
  if (!text) return err('Nothing to format — paste some JSON first.')
  try {
    return ok(JSON.stringify(JSON.parse(text), null, indent))
  } catch (e) {
    return err(`Invalid JSON: ${errorMessage(e)}`)
  }
}

export function jsonMinify(input: string): TransformResult {
  const text = input.trim()
  if (!text) return err('Nothing to minify — paste some JSON first.')
  try {
    return ok(JSON.stringify(JSON.parse(text)))
  } catch (e) {
    return err(`Invalid JSON: ${errorMessage(e)}`)
  }
}

/* ------------------------------------------------------------ base64 ---- */

export function base64Encode(input: string): TransformResult {
  try {
    return ok(getBtoa()(bytesToBinaryString(utf8Bytes(input))))
  } catch (e) {
    return err(`Could not base64-encode: ${errorMessage(e)}`)
  }
}

export function base64Decode(input: string): TransformResult {
  const text = input.trim()
  if (!text) return err('Nothing to decode — paste base64 first.')
  // Accept URL-safe variants and missing padding, which are common in
  // clipboard content copied from JWTs / configs.
  const normalized = text.replace(/-/g, '+').replace(/_/g, '/')
  const padded = normalized + '='.repeat((4 - (normalized.length % 4)) % 4)
  try {
    return ok(utf8FromBytes(binaryStringToBytes(getAtob()(padded))))
  } catch {
    return err('Input is not valid base64.')
  }
}

/* --------------------------------------------------------------- URL ---- */

export function urlEncode(input: string): TransformResult {
  try {
    // encodeURIComponent leaves A-Z a-z 0-9 - _ . ! ~ * ' ( ) — the RFC 3986
    // unreserved + common mark set users expect from "URL encode".
    return ok(encodeURIComponent(input))
  } catch (e) {
    // Lone surrogate.
    return err(`Could not URL-encode: ${errorMessage(e)}`)
  }
}

export function urlDecode(input: string): TransformResult {
  const text = input.trim()
  if (!text) return err('Nothing to decode — paste an encoded string first.')
  try {
    return ok(decodeURIComponent(text.replace(/\+/g, '%20')))
  } catch (e) {
    return err(`Could not URL-decode: ${errorMessage(e)}`)
  }
}

/* -------------------------------------------------------------- hash ---- */

// ---------------------------------------------------------------- MD5 (pure TS)
// crypto.subtle does not implement MD5 (and never will), so a compact RFC 1321
// implementation lives here. Done in 32-bit JS arithmetic so it's identical on
// every JS engine.

const MD5_S = [
  7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
  5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
  4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
  6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
] as const

const MD5_K = (() => {
  const k = new Uint32Array(64)
  for (let i = 0; i < 64; i++) {
    k[i] = Math.floor(Math.abs(Math.sin(i + 1)) * 0x100000000) >>> 0
  }
  return k
})()

export function md5Hex(bytes: Uint8Array): string {
  const bitLen = bytes.length * 8
  // Padding: 0x80 then zeros until length ≡ 56 (mod 64), then 64-bit LE length.
  const paddedLen = (((bytes.length + 8) >> 6) + 1) << 6
  const buf = new Uint8Array(paddedLen)
  buf.set(bytes)
  buf[bytes.length] = 0x80
  const dv = new DataView(buf.buffer)
  dv.setUint32(paddedLen - 8, bitLen >>> 0, true)
  dv.setUint32(paddedLen - 4, Math.floor(bitLen / 0x100000000), true)

  let a0 = 0x67452301
  let b0 = 0xefcdab89
  let c0 = 0x98badcfe
  let d0 = 0x10325476

  for (let off = 0; off < paddedLen; off += 64) {
    let A = a0
    let B = b0
    let C = c0
    let D = d0
    for (let i = 0; i < 64; i++) {
      let F: number
      let g: number
      if (i < 16) {
        F = (B & C) | (~B & D)
        g = i
      } else if (i < 32) {
        F = (D & B) | (~D & C)
        g = (5 * i + 1) % 16
      } else if (i < 48) {
        F = B ^ C ^ D
        g = (3 * i + 5) % 16
      } else {
        F = C ^ (B | ~D)
        g = (7 * i) % 16
      }
      const M = dv.getUint32(off + g * 4, true)
      const tmp = D
      D = C
      C = B
      const s = MD5_S[i] as number
      const sum = (A + F + (MD5_K[i] as number) + M) >>> 0
      B = (B + ((sum << s) | (sum >>> (32 - s)))) >>> 0
      A = tmp
    }
    a0 = (a0 + A) >>> 0
    b0 = (b0 + B) >>> 0
    c0 = (c0 + C) >>> 0
    d0 = (d0 + D) >>> 0
  }

  const out = new Uint8Array(16)
  const odv = new DataView(out.buffer)
  odv.setUint32(0, a0, true)
  odv.setUint32(4, b0, true)
  odv.setUint32(8, c0, true)
  odv.setUint32(12, d0, true)
  return Array.from(out, (b) => b.toString(16).padStart(2, '0')).join('')
}

async function subtleHex(bytes: Uint8Array, algo: 'SHA-256' | 'SHA-512'): Promise<string> {
  const subtle = globalThis.crypto?.subtle
  if (!subtle) throw new Error('crypto.subtle is unavailable in this environment')
  const digest = await subtle.digest(algo, bytes as unknown as BufferSource)
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0')).join('')
}

/**
 * Hash a string. Async because SHA-256/512 go through WebCrypto; MD5 is
 * synchronous internally but shares the promise-based signature so callers
 * have a uniform await.
 */
export async function hashText(input: string, algorithm: HashAlgorithm): Promise<TransformResult> {
  if (!input) return err('Nothing to hash — input is empty.')
  try {
    const bytes = utf8Bytes(input)
    switch (algorithm) {
      case 'md5':
        return ok(md5Hex(bytes))
      case 'sha256':
        return ok(await subtleHex(bytes, 'SHA-256'))
      case 'sha512':
        return ok(await subtleHex(bytes, 'SHA-512'))
      default: {
        // Exhaustiveness guard in case the union is extended later.
        const never: never = algorithm
        return err(`Unsupported hash algorithm: ${String(never)}`)
      }
    }
  } catch (e) {
    return err(`Hash failed: ${errorMessage(e)}`)
  }
}

/** Synchronous MD5 shortcut for callers that never need WebCrypto. */
export function hashMd5(input: string): TransformResult {
  if (!input) return err('Nothing to hash — input is empty.')
  return ok(md5Hex(utf8Bytes(input)))
}

/* -------------------------------------------------------- word count ---- */

export interface WordCount {
  characters: number
  charactersNoSpaces: number
  words: number
  sentences: number
  paragraphs: number
  lines: number
}

export function countText(input: string): WordCount {
  const characters = Array.from(input).length // code points, not UTF-16 units
  const charactersNoSpaces = Array.from(input.replace(/\s/g, '')).length
  const words = (input.match(/[\p{L}\p{N}]+(?:['’\-][\p{L}\p{N}]+)*/gu) ?? []).length
  // Sentence count is a heuristic — terminal punctuation followed by space/EOL.
  const sentences = (input.match(/[^.!?…\n][.!?…]+(?=\s|$)/g) ?? []).length
  const paragraphs = input.split(/\n\s*\n+/).filter((p) => p.trim().length > 0).length
  const lines = input.length === 0 ? 0 : input.split('\n').length
  return { characters, charactersNoSpaces, words, sentences, paragraphs, lines }
}

export function wordCount(input: string): TransformResult {
  if (!input.trim()) return err('Nothing to count — input is empty.')
  const c = countText(input)
  return ok(
    [
      `Words: ${c.words}`,
      `Characters: ${c.characters}`,
      `Characters (no spaces): ${c.charactersNoSpaces}`,
      `Sentences: ${c.sentences}`,
      `Paragraphs: ${c.paragraphs}`,
      `Lines: ${c.lines}`,
    ].join('\n'),
  )
}

/* ------------------------------------------------------- case convert --- */

/**
 * Split free-form text into word tokens, handling spaces, underscores,
 * dashes, and camelCase / PascalCase boundaries. Acronyms are collapsed
 * (`HTTPServer` → ['HTTP', 'Server']) so case round-trips are stable.
 */
export function splitWords(input: string): string[] {
  const pass1 = input
    .replace(/([a-z\d])([A-Z])/g, '$1 $2') // camelX → camel X
    .replace(/([A-Z]+)([A-Z][a-z])/g, '$1 $2') // HTTPServer → HTTP Server
  return pass1
    .split(/[\s_\-.\/\\]+/)
    .map((w) => w.trim())
    .filter((w) => w.length > 0)
}

export function convertCase(input: string, style: CaseStyle): TransformResult {
  const words = splitWords(input)
  if (words.length === 0) return err('Nothing to convert — input has no words.')
  const lower = words.map((w) => w.toLowerCase())
  const capitalized = lower.map((w) => w.charAt(0).toUpperCase() + w.slice(1))
  switch (style) {
    case 'camel':
      return ok(lower[0]! + capitalized.slice(1).join(''))
    case 'snake':
      return ok(lower.join('_'))
    case 'kebab':
      return ok(lower.join('-'))
    case 'constant':
      return ok(lower.join('_').toUpperCase())
    case 'title':
      return ok(capitalized.join(' '))
    case 'lower':
      return ok(lower.join(' '))
    case 'upper':
      return ok(lower.join(' ').toUpperCase())
    default: {
      const never: never = style
      return err(`Unsupported case style: ${String(never)}`)
    }
  }
}

/* ---------------------------------------------------------- timestamp --- */

/**
 * Parse "unix milliseconds" (also tolerates seconds when the number is small
 * enough to be a plausible unix-seconds value) and render ISO-8601 plus a
 * human-readable local time.
 */
export function parseTimestamp(input: string): TransformResult {
  const text = input.trim()
  if (!text) return err('Nothing to parse — paste a unix timestamp first.')
  if (!/^-?\d+(\.\d+)?$/.test(text)) {
    return err('Input is not a numeric unix timestamp.')
  }
  let ms = Number(text)
  if (!Number.isFinite(ms)) return err('Timestamp is out of range.')
  // Heuristic: numbers below ~3e11 are seconds (covers 1970…1973 in ms, and
  // 1970…~mid-5138 in seconds, which is the sane interpretation).
  if (Math.abs(ms) < 1e11) ms = Math.round(ms * 1000)
  const date = new Date(ms)
  if (Number.isNaN(date.getTime())) return err('Timestamp is out of range.')

  const iso = date.toISOString()
  const human = date.toLocaleString(undefined, {
    weekday: 'short',
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
    second: '2-digit',
    timeZoneName: 'short',
  })
  const relative = formatRelative(date.getTime(), Date.now())

  return ok([`Unix (ms): ${ms}`, `ISO 8601: ${iso}`, `Local: ${human}`, `Relative: ${relative}`].join('\n'))
}

export function formatRelative(target: number, now: number): string {
  const diffMs = target - now
  const abs = Math.abs(diffMs)
  const units: Array<[number, Intl.RelativeTimeFormatUnit]> = [
    [60_000, 'second'],
    [3_600_000, 'minute'],
    [86_400_000, 'hour'],
    [604_800_000, 'day'],
    [2_592_000_000, 'week'],
    [31_536_000_000, 'month'],
    [Number.POSITIVE_INFINITY, 'year'],
  ]
  const thresholds: Record<string, number> = {
    second: 1000,
    minute: 60_000,
    hour: 3_600_000,
    day: 86_400_000,
    week: 604_800_000,
    month: 2_592_000_000,
    year: 31_536_000_000,
  }
  const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
  for (const [limit, unit] of units) {
    if (abs < limit) {
      const value = Math.round(diffMs / thresholds[unit]!)
      return rtf.format(value, unit)
    }
  }
  return rtf.format(Math.round(diffMs / 31_536_000_000), 'year')
}

/* ---------------------------------------------------------------- QR ---- *
 * Minimal QR generator (byte mode, EC level M, versions 1..10, mask 0..7
 * auto-picked) rendered as an SVG data URL. No external dependencies.
 *
 * The implementation is a compact port of the classic MIT-licensed
 * "qrcode-generator" algorithm (Kazuhiko Arase), trimmed to byte-mode +
 * level M which is all a clipboard surface needs.
 */

const QR_EC_LEVEL_M = 0 // index into the tables below

// RS block parameters for EC level M, versions 1..10:
// [totalCount, dataCount] pairs per group (≤2 groups).
const QR_RS_BLOCK_TABLE_M: number[][] = [
  // v1
  [1, 26, 16],
  // v2
  [1, 44, 28],
  // v3
  [1, 70, 44],
  // v4
  [2, 50, 32],
  // v5
  [2, 67, 43],
  // v6
  [4, 43, 27],
  // v7
  [4, 32, 20],
  // v8
  [2, 60, 38, 2, 61, 39],
  // v9
  [3, 58, 36, 2, 59, 37],
  // v10
  [4, 69, 43, 1, 70, 44],
]

interface QrMatrix {
  modules: boolean[][]
  size: number
}

class BitBuffer {
  private buffer: number[] = []
  private bitLength = 0
  put(num: number, length: number): void {
    for (let i = 0; i < length; i++) {
      this.putBit(((num >>> (length - i - 1)) & 1) === 1)
    }
  }
  putBit(bit: boolean): void {
    const bufIndex = Math.floor(this.bitLength / 8)
    if (this.buffer.length <= bufIndex) this.buffer.push(0)
    if (bit) this.buffer[bufIndex]! |= 0x80 >>> this.bitLength % 8
    this.bitLength++
  }
  get lengthInBits(): number {
    return this.bitLength
  }
  get bytes(): number[] {
    return this.buffer
  }
}

const G15 = 0b10100110111
const G18 = 0b1111100100101
const G15_MASK = 0b101010000010010

function bchDigit(data: number): number {
  let d = data << 10
  while (bchDigit.raw(d) - bchDigit.raw(G15) >= 0) {
    d ^= G15 << (bchDigit.raw(d) - bchDigit.raw(G15))
  }
  return ((data << 10) | d) ^ G15_MASK
}
namespace bchDigit {
  export function raw(d: number): number {
    let i = 0
    while (d >>> i > 0) i++
    return i - 1
  }
}

function bchDigit18(data: number): number {
  let d = data << 12
  const g = G18
  const len = (x: number) => {
    let i = 0
    while (x >>> i > 0) i++
    return i - 1
  }
  while (len(d) - len(g) >= 0) {
    d ^= g << (len(d) - len(g))
  }
  return (data << 12) | d
}

const EXP_TABLE: number[] = (() => {
  const t = new Array<number>(512)
  let v = 1
  for (let i = 0; i < 255; i++) {
    t[i] = v
    v <<= 1
    if (v & 0x100) v ^= 0x11d
  }
  for (let i = 255; i < 512; i++) t[i] = t[i - 255]!
  return t
})()

const LOG_TABLE: number[] = (() => {
  const t = new Array<number>(256)
  for (let i = 0; i < 255; i++) t[EXP_TABLE[i]!] = i
  return t
})()

function gfMul(a: number, b: number): number {
  if (a === 0 || b === 0) return 0
  return EXP_TABLE[(LOG_TABLE[a]! + LOG_TABLE[b]!) % 255]!
}

function rsGeneratorPoly(ecLength: number): number[] {
  // g(x) = ∏_{i=0..n-1} (x - α^i), coefficients in low-degree-first order.
  let poly = [1]
  for (let i = 0; i < ecLength; i++) {
    const next = new Array<number>(poly.length + 1).fill(0)
    for (let j = 0; j <= poly.length; j++) {
      const highTerm = j > 0 ? poly[j - 1]! : 0
      const lowTerm = j < poly.length ? gfMul(poly[j]!, EXP_TABLE[i]!) : 0
      next[j] = highTerm ^ lowTerm
    }
    poly = next
  }
  return poly
}

function rsEncode(data: number[], ecLength: number): number[] {
  // Generator poly, highest-degree coefficient first (synthetic-division order).
  const gen = rsGeneratorPoly(ecLength).slice().reverse()
  const res = data.concat(new Array<number>(ecLength).fill(0))
  for (let i = 0; i < data.length; i++) {
    const coef = res[i]!
    if (coef !== 0) {
      for (let j = 0; j < gen.length; j++) {
        res[i + j] = res[i + j]! ^ gfMul(gen[j]!, coef)
      }
    }
  }
  return res.slice(data.length)
}

const MASK_CONDITIONS: Array<(r: number, c: number) => boolean> = [
  (r, c) => (r + c) % 2 === 0,
  (r) => r % 2 === 0,
  (_, c) => c % 3 === 0,
  (r, c) => (r + c) % 3 === 0,
  (r, c) => (Math.floor(r / 2) + Math.floor(c / 3)) % 2 === 0,
  (r, c) => ((r * c) % 2) + ((r * c) % 3) === 0,
  (r, c) => (((r * c) % 2) + ((r * c) % 3)) % 2 === 0,
  (r, c) => (((r + c) % 2) + ((r * c) % 3)) % 2 === 0,
]

const ALIGNMENT_POSITIONS: number[][] = [
  [],
  [6, 18],
  [6, 22],
  [6, 26],
  [6, 30],
  [6, 34],
  [6, 22, 38],
  [6, 24, 42],
  [6, 26, 46],
  [6, 28, 50],
]

function lostPoint(modules: (boolean | null)[][]): number {
  const size = modules.length
  let lost = 0
  // Level 1.
  for (let row = 0; row < size; row++) {
    let sameCount = 0
    let dark = modules[row]![0]!
    for (let col = 0; col < size; col++) {
      const current = modules[row]![col]!
      if (current === dark) sameCount++
      else {
        if (sameCount >= 5) lost += 3 + sameCount - 5
        dark = current
        sameCount = 1
      }
    }
    if (sameCount >= 5) lost += 3 + sameCount - 5
  }
  for (let col = 0; col < size; col++) {
    let sameCount = 0
    let dark = modules[0]![col]!
    for (let row = 0; row < size; row++) {
      const current = modules[row]![col]!
      if (current === dark) sameCount++
      else {
        if (sameCount >= 5) lost += 3 + sameCount - 5
        dark = current
        sameCount = 1
      }
    }
    if (sameCount >= 5) lost += 3 + sameCount - 5
  }
  // Level 2: 2x2 blocks.
  for (let row = 0; row < size - 1; row++) {
    for (let col = 0; col < size - 1; col++) {
      const count =
        Number(modules[row]![col]) +
        Number(modules[row + 1]![col]) +
        Number(modules[row]![col + 1]) +
        Number(modules[row + 1]![col + 1])
      if (count === 0 || count === 4) lost += 3
    }
  }
  // Level 3: finder-like patterns.
  for (let row = 0; row < size; row++) {
    for (let col = 0; col < size - 6; col++) {
      const seq = [0, 1, 2, 3, 4, 5, 6].map((i) => modules[row]![col + i])
      if (
        seq[0] && !seq[1] && seq[2] && seq[3] && seq[4] && !seq[5] && seq[6] &&
        ((col >= 4 && !modules[row]![col - 4] && !modules[row]![col - 3] && !modules[row]![col - 2] && !modules[row]![col - 1]) ||
          (col + 10 < size && !modules[row]![col + 7] && !modules[row]![col + 8] && !modules[row]![col + 9] && !modules[row]![col + 10]))
      ) {
        lost += 40
      }
    }
  }
  for (let col = 0; col < size; col++) {
    for (let row = 0; row < size - 6; row++) {
      const seq = [0, 1, 2, 3, 4, 5, 6].map((i) => modules[row + i]![col])
      if (
        seq[0] && !seq[1] && seq[2] && seq[3] && seq[4] && !seq[5] && seq[6] &&
        ((row >= 4 && !modules[row - 4]![col] && !modules[row - 3]![col] && !modules[row - 2]![col] && !modules[row - 1]![col]) ||
          (row + 10 < size && !modules[row + 7]![col] && !modules[row + 8]![col] && !modules[row + 9]![col] && !modules[row + 10]![col]))
      ) {
        lost += 40
      }
    }
  }
  // Level 4: dark module ratio.
  let darkCount = 0
  for (let row = 0; row < size; row++) {
    for (let col = 0; col < size; col++) {
      if (modules[row]![col]) darkCount++
    }
  }
  const ratio = Math.abs((100 * darkCount) / size / size - 50) / 5
  lost += Math.floor(ratio) * 10
  return lost
}

function buildQrMatrix(data: Uint8Array): QrMatrix {
  // Pick the smallest version (1..10) whose level-M byte capacity can hold the
  // data. Capacity in bytes = dataCodewords - mode/length/header overhead.
  let version = 0
  for (let v = 1; v <= 10; v++) {
    const row = QR_RS_BLOCK_TABLE_M[v - 1]!
    let dataCount = 0
    for (let i = 0; i < row.length; i += 3) {
      dataCount += row[i + 2]! * row[i]!
    }
    // Byte mode: 4 mode bits + 8 length bits (v1..9) or 16 (v10+) + data + terminator.
    const lengthBits = v < 10 ? 8 : 16
    const neededBits = 4 + lengthBits + data.length * 8 + 4
    if (dataCount * 8 >= neededBits) {
      version = v
      break
    }
  }
  if (version === 0) {
    throw new Error(
      'Input is too long for QR version 10 (max ~213 bytes at EC level M, byte mode).',
    )
  }

  const size = version * 4 + 17
  const modules: (boolean | null)[][] = Array.from({ length: size }, () =>
    new Array<boolean | null>(size).fill(null),
  )

  const setupPositionProbe = (r: number, c: number) => {
    for (let i = -1; i <= 7; i++) {
      if (r + i <= -1 || size <= r + i) continue
      for (let j = -1; j <= 7; j++) {
        if (c + j <= -1 || size <= c + j) continue
        modules[r + i]![c + j] =
          (0 <= i && i <= 6 && (j === 0 || j === 6)) ||
          (0 <= j && j <= 6 && (i === 0 || i === 6)) ||
          (2 <= i && i <= 4 && 2 <= j && j <= 4)
      }
    }
  }
  const setupTiming = () => {
    for (let i = 8; i < size - 8; i++) {
      if (modules[i]![6] === null) modules[i]![6] = i % 2 === 0
      if (modules[6]![i] === null) modules[6]![i] = i % 2 === 0
    }
  }
  const setupAlignment = () => {
    const pos = ALIGNMENT_POSITIONS[version - 1]!
    for (const r of pos) {
      for (const c of pos) {
        if (modules[r]![c] !== null) continue
        for (let i = -2; i <= 2; i++) {
          for (let j = -2; j <= 2; j++) {
            modules[r + i]![c + j] = Math.max(Math.abs(i), Math.abs(j)) !== 1
          }
        }
      }
    }
  }
  const setupTypeInfo = (test: boolean, maskPattern: number) => {
    const data = (QR_EC_LEVEL_M << 3) | maskPattern
    const bits = bchDigit(data)
    for (let i = 0; i < 15; i++) {
      const bit = !test && ((bits >> i) & 1) === 1
      if (i < 6) modules[i]![8] = bit
      else if (i < 8) modules[i + 1]![8] = bit
      else modules[size - 15 + i]![8] = bit
    }
    for (let i = 0; i < 15; i++) {
      const bit = !test && ((bits >> i) & 1) === 1
      if (i < 8) modules[8]![size - i - 1] = bit
      else if (i < 9) modules[8]![15 - i - 1 + 1] = bit
      else modules[8]![15 - i - 1] = bit
    }
    modules[size - 8]![8] = !test
  }
  const setupTypeNumber = (test: boolean) => {
    if (version < 7) return
    const bits = bchDigit18(version)
    for (let i = 0; i < 18; i++) {
      const bit = !test && ((bits >> i) & 1) === 1
      modules[Math.floor(i / 3)]![(i % 3) + size - 8 - 3] = bit
      modules[(i % 3) + size - 8 - 3]![Math.floor(i / 3)] = bit
    }
  }
  const mapData = (dataList: readonly (number | null)[], maskPattern: number) => {
    let inc = -1
    let row = size - 1
    let bitIndex = 7
    let byteIndex = 0
    const maskFn = MASK_CONDITIONS[maskPattern]!
    for (let col = size - 1; col > 0; col -= 2) {
      if (col === 6) col--
      while (true) {
        for (let c = 0; c < 2; c++) {
          if (modules[row]![col - c] === null) {
            let dark = false
            const byte = dataList[byteIndex]
            if (byte != null && byteIndex < dataList.length) {
              dark = ((byte >>> bitIndex) & 1) === 1
            }
            if (maskFn(row, col - c)) dark = !dark
            modules[row]![col - c] = dark
            bitIndex--
            if (bitIndex === -1) {
              byteIndex++
              bitIndex = 7
            }
          }
        }
        row += inc
        if (row < 0 || size <= row) {
          row -= inc
          inc = -inc
          break
        }
      }
    }
  }
  const createData = (v: number, buffer: BitBuffer): number[] => {
    const row = QR_RS_BLOCK_TABLE_M[v - 1]!
    const dataList = buffer.bytes.slice()
    // Terminator.
    let totalDataCountBits = 0
    for (let i = 0; i < row.length; i += 3) totalDataCountBits += row[i + 2]! * row[i]! * 8
    if (buffer.lengthInBits + 4 <= totalDataCountBits) buffer.put(0, 4)
    while (buffer.lengthInBits % 8 !== 0) buffer.putBit(false)
    const PAD0 = 0xec
    const PAD1 = 0x11
    for (let i = 0; ; i++) {
      if (buffer.bytes.length * 8 >= totalDataCountBits) break
      buffer.put(i % 2 === 0 ? PAD0 : PAD1, 8)
    }
    void dataList
    // Interleave.
    const blocks: { data: number[]; ec: number[] }[] = []
    const dcCount = row.length / 3
    let offset = 0
    const bytes = buffer.bytes
    for (let i = 0; i < dcCount; i++) {
      const count = row[i * 3]!
      const total = row[i * 3 + 1]!
      const data = row[i * 3 + 2]!
      for (let j = 0; j < count; j++) {
        const d = bytes.slice(offset, offset + data)
        offset += data
        blocks.push({ data: d, ec: rsEncode(d, total - data) })
      }
    }
    const out: number[] = []
    const maxData = Math.max(...blocks.map((b) => b.data.length))
    const maxEc = Math.max(...blocks.map((b) => b.ec.length))
    for (let i = 0; i < maxData; i++) {
      for (const b of blocks) if (i < b.data.length) out.push(b.data[i]!)
    }
    for (let i = 0; i < maxEc; i++) {
      for (const b of blocks) if (i < b.ec.length) out.push(b.ec[i]!)
    }
    return out
  }

  // Fixed patterns + function modules. Order matters: alignment BEFORE timing
  // so timing doesn't claim the alignment pattern's cells first (qrcode-generator
  // original does position-probe → position-adjust → timing for this reason).
  setupPositionProbe(0, 0)
  setupPositionProbe(size - 7, 0)
  setupPositionProbe(0, size - 7)
  setupAlignment()
  setupTiming()
  setupTypeNumber(true)
  setupTypeInfo(true, 0)

  // Pick the best mask by penalty.
  let bestMask = 0
  let bestPenalty = Number.POSITIVE_INFINITY
  let bestData: number[] = []
  const buffer = new BitBuffer()
  buffer.put(0b0100, 4) // byte mode
  buffer.put(data.length, version < 10 ? 8 : 16)
  for (const b of data) buffer.put(b, 8)

  for (let mask = 0; mask < 8; mask++) {
    const testModules = modules.map((row) => row.slice())
    const dataCodewords = createData(version, buffer)
    mapData(dataCodewords, mask)
    setupTypeInfo(false, mask)
    setupTypeNumber(false)
    const penalty = lostPoint(modules as (boolean | null)[][])
    if (penalty < bestPenalty) {
      bestPenalty = penalty
      bestMask = mask
      bestData = dataCodewords
    }
    // Restore.
    for (let r = 0; r < size; r++) {
      for (let c = 0; c < size; c++) {
        modules[r]![c] = testModules[r]![c] ?? null
      }
    }
  }

  mapData(bestData, bestMask)
  setupTypeInfo(false, bestMask)
  setupTypeNumber(false)

  return {
    modules: modules.map((row) => row.map((cell) => cell === true)),
    size,
  }
}

/**
 * Generate an SVG data URL QR code for `input`. SVG (not PNG) keeps this
 * pure — no canvas dependency — and works identically in the webview, in
 * tests, and when pasted anywhere that accepts image data URLs.
 */
export function qrCodeDataUrl(input: string): TransformResult {
  if (!input) return err('Nothing to encode — input is empty.')
  let matrix: QrMatrix
  try {
    matrix = buildQrMatrix(utf8Bytes(input))
  } catch (e) {
    return err(errorMessage(e))
  }
  const { modules, size } = matrix
  const quiet = 4 // QR spec quiet zone
  const total = size + quiet * 2
  let path = ''
  for (let r = 0; r < size; r++) {
    for (let c = 0; c < size; c++) {
      if (modules[r]![c]) {
        path += `M${c + quiet},${r + quiet}h1v1h-1z`
      }
    }
  }
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${total} ${total}" shape-rendering="crispEdges">` +
    `<rect width="${total}" height="${total}" fill="#ffffff"/>` +
    `<path d="${path}" fill="#000000"/>` +
    `</svg>`
  const base64 = getBtoa()(svg)
  return ok(`data:image/svg+xml;base64,${base64}`)
}
