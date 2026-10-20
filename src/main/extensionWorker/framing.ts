/** Match the Rust host protocol: bounded LF-delimited UTF-8 JSON records.
 * Unicode line/paragraph separators are data, never record delimiters. */
export const MAX_FRAME_BYTES = 16 * 1024 * 1024

export class FrameReader {
  private buffered = Buffer.alloc(0)
  private readonly decoder = new TextDecoder('utf-8', { fatal: true })

  constructor(
    private readonly accept: (line: string) => void,
    private readonly limit = MAX_FRAME_BYTES
  ) {}

  push(chunk: Uint8Array): void {
    this.buffered = Buffer.concat([this.buffered, chunk])
    for (;;) {
      const newline = this.buffered.indexOf(10)
      if (newline < 0) break
      if (newline > this.limit) throw new Error('Extension protocol frame too large')
      const frame = this.buffered.subarray(0, newline)
      this.buffered = this.buffered.subarray(newline + 1)
      this.accept(this.decoder.decode(frame))
    }
    if (this.buffered.length > this.limit) throw new Error('Extension protocol frame too large')
  }

  finish(): void {
    if (this.buffered.length) throw new Error('Truncated extension protocol frame')
  }
}
