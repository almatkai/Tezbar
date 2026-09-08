/** LRU for data URLs. Bound both entry count and retained UTF-16 string bytes. */
export class StringCache extends Map<string, string | null> {
  private bytes = 0

  constructor(
    private readonly maxEntries = 256,
    private readonly maxBytes = 4 * 1024 * 1024
  ) {
    super()
  }

  override get(key: string): string | null | undefined {
    const value = super.get(key)
    if (value !== undefined) {
      super.delete(key)
      super.set(key, value)
    }
    return value
  }

  override set(key: string, value: string | null): this {
    this.delete(key)
    const bytes = 2 * (key.length + (value?.length ?? 0))
    if (bytes > this.maxBytes || this.maxEntries < 1) return this
    super.set(key, value)
    this.bytes += bytes
    while (this.size > this.maxEntries || this.bytes > this.maxBytes) {
      this.delete(this.keys().next().value!)
    }
    return this
  }

  override delete(key: string): boolean {
    if (!this.has(key)) return false
    this.bytes -= 2 * (key.length + (super.get(key)?.length ?? 0))
    return super.delete(key)
  }

  override clear(): void {
    super.clear()
    this.bytes = 0
  }
}
