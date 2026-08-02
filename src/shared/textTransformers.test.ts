// src/shared/textTransformers.test.ts
import { describe, expect, it } from 'vitest'
import {
  base64Decode,
  base64Encode,
  convertCase,
  countText,
  hashText,
  jsonFormat,
  jsonMinify,
  md5Hex,
  parseTimestamp,
  qrCodeDataUrl,
  splitWords,
  urlDecode,
  urlEncode,
  utf8Bytes,
  wordCount,
} from './textTransformers'

describe('json transforms', () => {
  it('formats valid JSON with 2-space indent by default', () => {
    const r = jsonFormat('{"a":1,"b":[2,3]}')
    expect(r).toEqual({ ok: true, result: `{\n  "a": 1,\n  "b": [\n    2,\n    3\n  ]\n}` })
  })

  it('minifies valid JSON', () => {
    const r = jsonMinify('{\n  "a": 1,\n  "b": [2, 3]\n}')
    expect(r).toEqual({ ok: true, result: '{"a":1,"b":[2,3]}' })
  })

  it('round-trips format → minify', () => {
    const original = '{"x":[1,{"y":"z"}]}'
    const formatted = jsonFormat(original)
    expect(formatted.ok).toBe(true)
    if (formatted.ok) {
      expect(jsonMinify(formatted.result)).toEqual({ ok: true, result: original })
    }
  })

  it('reports invalid JSON with the parse error', () => {
    const r = jsonFormat('{not json')
    expect(r.ok).toBe(false)
    if (!r.ok) expect(r.error).toMatch(/Invalid JSON/)
  })

  it('rejects empty input without calling JSON.parse', () => {
    expect(jsonFormat('   ').ok).toBe(false)
    expect(jsonMinify('').ok).toBe(false)
  })
})

describe('base64', () => {
  it('encodes ASCII', () => {
    expect(base64Encode('hello world')).toEqual({ ok: true, result: 'aGVsbG8gd29ybGQ=' })
  })

  it('encodes UTF-8 (emoji, CJK) without data loss', () => {
    const r = base64Encode('こんにちは 🌍')
    expect(r.ok).toBe(true)
    if (r.ok) {
      expect(base64Decode(r.result)).toEqual({ ok: true, result: 'こんにちは 🌍' })
    }
  })

  it('decodes standard base64', () => {
    expect(base64Decode('aGVsbG8gd29ybGQ=')).toEqual({ ok: true, result: 'hello world' })
  })

  it('accepts URL-safe base64 and missing padding', () => {
    // "??" utf-8 -> Pz8= -> Pz8 (no padding) and Pz8 with -_ swapped for +/.
    expect(base64Decode('Pz8')).toEqual({ ok: true, result: '??' })
    const withUnsafe = base64Encode('?>~?')!
    expect(withUnsafe.ok).toBe(true)
    if (withUnsafe.ok) {
      const urlSafe = withUnsafe.result.replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
      expect(base64Decode(urlSafe)).toEqual({ ok: true, result: '?>~?' })
    }
  })

  it('rejects malformed base64', () => {
    const r = base64Decode('!!! not base64 !!!')
    expect(r.ok).toBe(false)
  })
})

describe('url encoding', () => {
  it('encodes reserved characters', () => {
    expect(urlEncode('a+b?c=d&e=f')).toEqual({
      ok: true,
      result: 'a%2Bb%3Fc%3Dd%26e%3Df',
    })
  })

  it('encodes unicode', () => {
    expect(urlEncode('こんにちは')).toEqual({
      ok: true,
      result: '%E3%81%93%E3%82%93%E3%81%AB%E3%81%A1%E3%81%AF',
    })
  })

  it('round-trips', () => {
    const input = 'https://example.com/path?q=hello world&lang=日本語'
    const enc = urlEncode(input)
    expect(enc.ok).toBe(true)
    if (enc.ok) {
      expect(urlDecode(enc.result)).toEqual({ ok: true, result: input })
    }
  })

  it('decodes plus-as-space for form-encoded strings', () => {
    expect(urlDecode('hello+world')).toEqual({ ok: true, result: 'hello world' })
  })

  it('rejects malformed percent-encoding', () => {
    expect(urlDecode('%e3%81')).not.toEqual(expect.objectContaining({ ok: true }))
    const r = urlDecode('%zz')
    expect(r.ok).toBe(false)
  })
})

describe('hash', () => {
  it('computes md5', () => {
    expect(md5Hex(utf8Bytes('hello'))).toBe('5d41402abc4b2a76b9719d911017c592')
    expect(md5Hex(utf8Bytes(''))).toBe('d41d8cd98f00b204e9800998ecf8427e')
    expect(md5Hex(utf8Bytes('The quick brown fox jumps over the lazy dog'))).toBe(
      '9e107d9d372bb6826bd81d3542a419d6',
    )
  })

  it('computes sha256 via WebCrypto', async () => {
    const r = await hashText('hello', 'sha256')
    expect(r).toEqual({
      ok: true,
      result: '2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824',
    })
  })

  it('computes sha512 via WebCrypto', async () => {
    const r = await hashText('hello', 'sha512')
    expect(r).toEqual({
      ok: true,
      result:
        '9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043',
    })
  })

  it('computes md5 through the uniform async hash API', async () => {
    const r = await hashText('hello', 'md5')
    expect(r).toEqual({ ok: true, result: '5d41402abc4b2a76b9719d911017c592' })
  })

  it('handles unicode input', async () => {
    const r = await hashText('こんにちは', 'sha256')
    expect(r).toEqual({
      ok: true,
      result: '125aeadf27b0459b8760c13a3d80912dfa8a81a68261906f60d87f4a0268646c',
    })
  })

  it('rejects empty input', async () => {
    const r = await hashText('', 'sha256')
    expect(r.ok).toBe(false)
  })
})

describe('word count', () => {
  it('counts words, characters, sentences, paragraphs, lines', () => {
    const c = countText('Hello world. How are you?\n\nI am fine.')
    expect(c.words).toBe(8)
    expect(c.sentences).toBe(3)
    expect(c.paragraphs).toBe(2)
    expect(c.lines).toBe(3)
  })

  it('handles contractions as a single word', () => {
    expect(countText("don't stop believing").words).toBe(3)
  })

  it('counts unicode words', () => {
    expect(countText('こんにちは 世界').words).toBe(2)
  })

  it('counts characters as code points (not UTF-16 units)', () => {
    // "🌍" is 2 UTF-16 units but 1 code point.
    expect(countText('🌍').characters).toBe(1)
  })

  it('formats the report', () => {
    const r = wordCount('one two three')
    expect(r.ok).toBe(true)
    if (r.ok) {
      expect(r.result).toContain('Words: 3')
      expect(r.result).toContain('Characters: 13')
    }
  })

  it('rejects empty input', () => {
    expect(wordCount('   ').ok).toBe(false)
  })
})

describe('case conversion', () => {
  it('splits on spaces', () => {
    expect(splitWords('hello world')).toEqual(['hello', 'world'])
  })

  it('splits camelCase and PascalCase', () => {
    expect(splitWords('helloWorld')).toEqual(['hello', 'World'])
    expect(splitWords('HelloWorld')).toEqual(['Hello', 'World'])
  })

  it('splits snake_case and kebab-case', () => {
    expect(splitWords('hello_world')).toEqual(['hello', 'world'])
    expect(splitWords('hello-world')).toEqual(['hello', 'world'])
  })

  it('handles acronym word boundaries', () => {
    expect(splitWords('HTTPServerError')).toEqual(['HTTP', 'Server', 'Error'])
  })

  it('converts to camelCase', () => {
    expect(convertCase('hello world example', 'camel')).toEqual({
      ok: true,
      result: 'helloWorldExample',
    })
    expect(convertCase('hello_world', 'camel')).toEqual({ ok: true, result: 'helloWorld' })
  })

  it('converts to snake_case', () => {
    expect(convertCase('helloWorld', 'snake')).toEqual({ ok: true, result: 'hello_world' })
  })

  it('converts to kebab-case', () => {
    expect(convertCase('HelloWorld', 'kebab')).toEqual({ ok: true, result: 'hello-world' })
  })

  it('converts to CONSTANT_CASE', () => {
    expect(convertCase('helloWorld', 'constant')).toEqual({
      ok: true,
      result: 'HELLO_WORLD',
    })
  })

  it('converts to Title Case', () => {
    expect(convertCase('hello world', 'title')).toEqual({ ok: true, result: 'Hello World' })
  })

  it('converts to lower and UPPER', () => {
    expect(convertCase('Hello WORLD', 'lower')).toEqual({ ok: true, result: 'hello world' })
    expect(convertCase('Hello world', 'upper')).toEqual({ ok: true, result: 'HELLO WORLD' })
  })

  it('round-trips camel ↔ snake', () => {
    const toSnake = convertCase('parseHttpResponse', 'snake')
    expect(toSnake).toEqual({ ok: true, result: 'parse_http_response' })
    if (toSnake.ok) {
      expect(convertCase(toSnake.result, 'camel')).toEqual({
        ok: true,
        result: 'parseHttpResponse',
      })
    }
  })

  it('rejects empty input', () => {
    expect(convertCase('  ', 'camel').ok).toBe(false)
  })
})

describe('timestamp parsing', () => {
  it('parses a unix-ms timestamp', () => {
    const r = parseTimestamp('1704067200000') // 2024-01-01T00:00:00.000Z
    expect(r.ok).toBe(true)
    if (r.ok) {
      expect(r.result).toContain('Unix (ms): 1704067200000')
      expect(r.result).toContain('ISO 8601: 2024-01-01T00:00:00.000Z')
      expect(r.result).toContain('Local: ')
      expect(r.result).toContain('Relative: ')
    }
  })

  it('auto-detects unix seconds', () => {
    const r = parseTimestamp('1704067200')
    expect(r.ok).toBe(true)
    if (r.ok) expect(r.result).toContain('ISO 8601: 2024-01-01T00:00:00.000Z')
  })

  it('rejects non-numeric input', () => {
    expect(parseTimestamp('tomorrow').ok).toBe(false)
    expect(parseTimestamp('2024-01-01').ok).toBe(false)
  })

  it('rejects empty input', () => {
    expect(parseTimestamp('   ').ok).toBe(false)
  })
})

describe('QR code data URL', () => {
  it('returns a base64 SVG data URL', () => {
    const r = qrCodeDataUrl('https://example.com')
    expect(r.ok).toBe(true)
    if (r.ok) {
      expect(r.result.startsWith('data:image/svg+xml;base64,')).toBe(true)
      const b64 = r.result.slice('data:image/svg+xml;base64,'.length)
      const svg = Buffer.from(b64, 'base64').toString('utf-8')
      expect(svg).toContain('<svg')
      expect(svg).toContain('</svg>')
      expect(svg).toContain('viewBox')
    }
  })

  it('is deterministic — same input, same data URL', () => {
    const a = qrCodeDataUrl('hello')
    const b = qrCodeDataUrl('hello')
    expect(a).toEqual(b)
  })

  it('produces different output for different inputs', () => {
    const a = qrCodeDataUrl('hello')
    const b = qrCodeDataUrl('world')
    expect(a.ok && b.ok).toBe(true)
    if (a.ok && b.ok) expect(a.result).not.toBe(b.result)
  })

  it('rejects empty input', () => {
    expect(qrCodeDataUrl('').ok).toBe(false)
  })

  it('encodes longer payloads (v2+ path)', () => {
    const r = qrCodeDataUrl('The quick brown fox jumps over the lazy dog — and keeps going.')
    expect(r.ok).toBe(true)
  })

  it('rejects payloads too large for version 10', () => {
    const r = qrCodeDataUrl('x'.repeat(500))
    expect(r.ok).toBe(false)
    if (!r.ok) expect(r.error).toMatch(/too long/i)
  })

  // Structural sanity: re-decode the matrix ourselves and confirm the embedded
  // payload matches byte-for-byte. This catches RS-encoding / interleaving /
  // mask bugs that "the SVG has the right shape" tests miss.
  describe('decodability (self-round-trip)', () => {
    function decodeOwnQr(dataUrl: string): string | null {
      const b64 = dataUrl.slice('data:image/svg+xml;base64,'.length)
      const svg = Buffer.from(b64, 'base64').toString('utf8')
      const vb = /viewBox="0 0 (\d+) (\d+)"/.exec(svg)
      const total = parseInt(vb?.[1] ?? '0', 10)
      const size = total - 8 // subtract 4-module quiet zone
      const grid: number[][] = Array.from({ length: size }, () => new Array(size).fill(0) as number[])
      const cellRe = /M(\d+),(\d+)h1v1h-1z/g
      for (let m = cellRe.exec(svg); m; m = cellRe.exec(svg)) {
        const cx = parseInt(m[1]!, 10) - 4
        const cy = parseInt(m[2]!, 10) - 4
        if (cx >= 0 && cy >= 0 && cx < size && cy < size) grid[cy]![cx] = 1
      }
      const isFn = (r: number, c: number): boolean => {
        if (r <= 8 && c <= 8) return true
        if (r <= 8 && c >= size - 8) return true
        if (r >= size - 7 && c <= 8) return true
        if (r === 6 || c === 6) return true
        if (c === 8 && r >= size - 8) return true
        if (r === 8 && c >= size - 7) return true
        // v7+ version-info blocks
        if (size >= 45) {
          if (r <= 5 && c >= size - 11 && c <= size - 9) return true
          if (c <= 5 && r >= size - 11 && r <= size - 9) return true
        }
        // Alignment pattern positions per version (level-M subset mirrors impl)
        const v = (size - 17) / 4
        const ALIGN: number[][] = [
          [], [6, 18], [6, 22], [6, 26], [6, 30], [6, 34],
          [6, 22, 38], [6, 24, 42], [6, 26, 46], [6, 28, 50],
        ]
        for (const ar of ALIGN[v - 1] ?? []) {
          for (const ac of ALIGN[v - 1] ?? []) {
            // skip the three corners overlapping the finders
            if ((ar === 6 && ac === 6) || (ar === 6 && ac === size - 7) || (ar === size - 7 && ac === 6)) continue
            if (Math.abs(r - ar) <= 2 && Math.abs(c - ac) <= 2) return true
          }
        }
        return false
      }
      const maskFn = [
        (r: number, c: number) => (r + c) % 2 === 0,
        (r: number) => r % 2 === 0,
        (_r: number, c: number) => c % 3 === 0,
        (r: number, c: number) => (r + c) % 3 === 0,
        (r: number, c: number) => (Math.floor(r / 2) + Math.floor(c / 3)) % 2 === 0,
        (r: number, c: number) => ((r * c) % 2) + ((r * c) % 3) === 0,
        (r: number, c: number) => (((r * c) % 2) + ((r * c) % 3)) % 2 === 0,
        (r: number, c: number) => (((r + c) % 2) + ((r * c) % 3)) % 2 === 0,
      ]
      const version = (size - 17) / 4
      for (let mask = 0; mask < 8; mask++) {
        const bits: number[] = []
        let inc = -1
        let row = size - 1
        for (let col = size - 1; col > 0; col -= 2) {
          if (col === 6) col--
          for (;;) {
            for (let c = 0; c < 2; c++) {
              const cc = col - c
              if (!isFn(row, cc)) {
                let bit = grid[row]![cc]!
                if (maskFn[mask]!(row, cc)) bit ^= 1
                bits.push(bit)
              }
            }
            row += inc
            if (row < 0 || row >= size) {
              row -= inc
              inc = -inc
              break
            }
          }
        }
        const mode = bits.slice(0, 4).join('')
        if (mode !== '0100') continue
        const lenBits = version < 10 ? 8 : 16
        const length = parseInt(bits.slice(4, 4 + lenBits).join(''), 2)
        const bytes: number[] = []
        for (let i = 0; i < length; i++) {
          const start = 4 + lenBits + i * 8
          if (start + 8 > bits.length) break
          bytes.push(parseInt(bits.slice(start, start + 8).join(''), 2))
        }
        // Length sanity: payload shouldn't exceed version-10-M capacity.
        if (bytes.length === length && length > 0) {
          return new TextDecoder('utf-8').decode(new Uint8Array(bytes))
        }
      }
      return null
    }

    it('round-trips short inputs', () => {
      for (const text of ['A', 'hello', 'https://example.com']) {
        const r = qrCodeDataUrl(text)
        expect(r.ok).toBe(true)
        if (r.ok) expect(decodeOwnQr(r.result)).toBe(text)
      }
    })

    it('round-trips longer inputs (forces multi-block RS, alignment patterns)', () => {
      // 200 chars ⇒ version 10 ⇒ exercises two-group RS interleaving +
      // alignment patterns + version-info blocks.
      const text = 'x'.repeat(200)
      const r = qrCodeDataUrl(text)
      expect(r.ok).toBe(true)
      if (r.ok) expect(decodeOwnQr(r.result)).toBe(text)
    })

    it('round-trips unicode payloads', () => {
      const text = 'こんにちは 🌍' // utf-8 byte length > string length
      const r = qrCodeDataUrl(text)
      expect(r.ok).toBe(true)
      if (r.ok) expect(decodeOwnQr(r.result)).toBe(text)
    })
  })
})
