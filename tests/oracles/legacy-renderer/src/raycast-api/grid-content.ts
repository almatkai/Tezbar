export function colorFromGridContent(content: unknown): string | undefined {
  if (!content || typeof content !== 'object') return undefined
  const wrappedValue = (content as { value?: unknown }).value
  const value = wrappedValue && typeof wrappedValue === 'object' ? wrappedValue : content
  const color = (value as { color?: unknown }).color

  if (typeof color === 'string' && color.trim()) return color.trim()
  if (!color || typeof color !== 'object') return undefined

  const adaptiveColor = color as { dark?: unknown; light?: unknown }
  const dark = typeof adaptiveColor.dark === 'string' ? adaptiveColor.dark.trim() : ''
  const light = typeof adaptiveColor.light === 'string' ? adaptiveColor.light.trim() : ''
  return dark || light || undefined
}
