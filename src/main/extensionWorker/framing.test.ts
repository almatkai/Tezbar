import { describe, expect, it } from 'vitest'
import { FrameReader } from './framing'

describe('extension worker LF framing', () => {
  it('preserves Unicode separators and UTF-8 split at every byte, across multiple records', () => {
    const values = ['ASCII', 'Қазақша\u2028日本語🐎\u2029', 'CR\rLF\n']
    const lines: string[] = []
    const frames = new FrameReader((line) => lines.push(JSON.parse(line)))
    for (const byte of Buffer.from(
      values.map((value) => JSON.stringify(value)).join('\n') + '\n'
    )) {
      frames.push(Uint8Array.of(byte))
    }
    frames.finish()
    expect(lines).toEqual(values)
  })

  it('bounds complete and incomplete records by bytes, allowing multiple bounded records', () => {
    const frames = new FrameReader(() => {}, 4)
    frames.push(Buffer.from('1234\n1234\n'))
    expect(() => frames.push(Buffer.from('🐎x'))).toThrow('too large')
    expect(() => new FrameReader(() => {}, 4).push(Buffer.from('12345\n'))).toThrow('too large')
  })

  it('rejects truncated records and invalid UTF-8 instead of silently changing data', () => {
    const frames = new FrameReader(() => {})
    frames.push(Buffer.from('{}'))
    expect(() => frames.finish()).toThrow('Truncated')
    expect(() => new FrameReader(() => {}).push(Uint8Array.of(0xff, 10))).toThrow()
  })
})
