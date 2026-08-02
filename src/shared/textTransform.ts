/** Text transformers used by the clipboard history "convert" row.
 *
 *  Each action takes a raw string and returns either a transformed string
 *  or an error message to surface to the user. Kept side-effect-free so the
 *  same helpers can be reused from the renderer, the extension sandbox, or
 *  tests without any IPC.
 */

export type ClipboardActionId =
  | 'json'
  | 'base64'
  | 'encode'
  | 'hash'
  | 'count'
  | 'case'
  | 'qr'

export type TransformResult =
  | { ok: true; output: string }
  | { ok: false; error: string }

function bytesToHex(bytes: Uint8Array): string {
  let out = ''
  for (const b of bytes) out += b.toString(16).padStart(2, '0')
  return out
}

function toUtf8Bytes(input: string): Uint8Array {
  return new TextEncoder().encode(input)
}

function fromUtf8Bytes(bytes: Uint8Array): string {
  return new TextDecoder('utf-8', { fatal: false }).decode(bytes)
}

/** Base64 helpers that handle arbitrary unicode (unlike window.btoa). */
function base64Encode(input: string): string {
  let binary = ''
  for (const b of toUtf8Bytes(input)) binary += String.fromCharCode(b)
  return btoa(binary)
}

function base64Decode(input: string): string {
  const clean = input.replace(/\s+/g, '')
  const binary = atob(clean)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
  return fromUtf8Bytes(bytes)
}

function isProbablyBase64(input: string): boolean {
  const clean = input.replace(/\s+/g, '')
  if (clean.length === 0 || clean.length % 4 !== 0) return false
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(clean)) return false
  try {
    atob(clean)
    return true
  } catch {
    return false
  }
}

/** Cheap sync FNV-1a, used when SubtleCrypto isn't available (it requires
 *  a secure context; the renderer usually is one, but tests aren't). */
function fnv1a(input: string): string {
  let hash = 0x811c9dc5
  for (const b of toUtf8Bytes(input)) {
    hash ^= b
    hash = Math.imul(hash, 0x01000193) >>> 0
  }
  return hash.toString(16).padStart(8, '0')
}

async function sha256Hex(input: string): Promise<string> {
  if (typeof crypto !== 'undefined' && crypto.subtle) {
    try {
      const buf = await crypto.subtle.digest('SHA-256', toUtf8Bytes(input) as BufferSource)
      return bytesToHex(new Uint8Array(buf))
    } catch {
      // fall through to FNV-1a
    }
  }
  return `fnv1a:${fnv1a(input)}`
}

function isProbablyUrlEncoded(input: string): boolean {
  return /%[0-9A-Fa-f]{2}/.test(input)
}

function tryParseJson(input: string): { value: unknown } | null {
  try {
    return { value: JSON.parse(input) }
  } catch {
    return null
  }
}

function formatJson(input: string): TransformResult {
  const parsed = tryParseJson(input)
  if (parsed === null) {
    // Not JSON — escape as a JSON string literal so "Encode → JSON encode"
    // always produces valid output.
    return { ok: true, output: JSON.stringify(input) }
  }
  return { ok: true, output: JSON.stringify(parsed.value, null, 2) }
}

function countSummary(input: string): TransformResult {
  const chars = Array.from(input).length
  const words = input.trim().length === 0 ? 0 : input.trim().split(/\s+/).length
  const lines = input.length === 0 ? 0 : input.split(/\r\n|\r|\n/).length
  const bytes = toUtf8Bytes(input).length
  return {
    ok: true,
    output: [
      `Characters : ${chars.toLocaleString()}`,
      `Words      : ${words.toLocaleString()}`,
      `Lines      : ${lines.toLocaleString()}`,
      `Bytes (utf-8): ${bytes.toLocaleString()}`,
    ].join('\n'),
  }
}

function flipCase(input: string): string {
  const lower = input.toLowerCase()
  const upper = input.toUpperCase()
  // If the input is all lower or all upper, invert to the other case.
  // Otherwise toggle per character.
  if (input === lower) return upper
  if (input === upper) return lower
  let out = ''
  for (const ch of input) {
    const l = ch.toLowerCase()
    out += ch === l ? ch.toUpperCase() : l
  }
  return out
}

/** Tiny QR encoder lives in ./textTransformers (no external API, works offline). */
import { qrCodeDataUrl } from './textTransformers'

function qrCodeUrl(input: string): TransformResult {
  const r = qrCodeDataUrl(input)
  if (!r.ok) return { ok: false, error: r.error }
  return { ok: true, output: r.result }
}

/** Public entry point used by ClipboardView. Synchronous where possible,
 *  async only for SHA-256. */
export async function applyClipboardAction(
  action: ClipboardActionId,
  input: string,
): Promise<TransformResult> {
  switch (action) {
    case 'json':
      return formatJson(input)
    case 'base64':
      return isProbablyBase64(input)
        ? { ok: true, output: base64Decode(input) }
        : { ok: true, output: base64Encode(input) }
    case 'encode':
      return isProbablyUrlEncoded(input)
        ? { ok: true, output: decodeURIComponent(input) }
        : { ok: true, output: encodeURIComponent(input) }
    case 'hash':
      return { ok: true, output: await sha256Hex(input) }
    case 'count':
      return countSummary(input)
    case 'case':
      return { ok: true, output: flipCase(input) }
    case 'qr':
      return qrCodeUrl(input)
  }
}
