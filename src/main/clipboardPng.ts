const PNG_MAGIC = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])

export function decodePngDataUrl(dataUrl: string): Buffer | null {
  const prefix = 'data:image/png;base64,'
  if (!dataUrl.startsWith(prefix)) return null
  const bytes = Buffer.from(dataUrl.slice(prefix.length), 'base64')
  return bytes.subarray(0, 8).equals(PNG_MAGIC) ? bytes : null
}
