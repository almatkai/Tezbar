import type { SearchResult } from '../shared/search'

export type Rgba = {
  r: number
  g: number
  b: number
  a: number
  source: 'hex' | 'rgb' | 'hsl'
}

export type ColorFormatRow = {
  key: string
  title: string
  value: string
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}

function round(value: number, digits = 3): number {
  const factor = 10 ** digits
  return Math.round(value * factor) / factor
}

function parseAlpha(raw: string | undefined): number {
  if (!raw) return 1
  const value = raw.trim()
  if (value.endsWith('%')) return clamp(Number.parseFloat(value) / 100, 0, 1)
  return clamp(Number.parseFloat(value), 0, 1)
}

function parseRgbChannel(raw: string): number {
  const value = raw.trim()
  if (value.endsWith('%')) return Math.round(clamp(Number.parseFloat(value), 0, 100) * 2.55)
  return Math.round(clamp(Number.parseFloat(value), 0, 255))
}

function parseHexColor(input: string): Rgba | null {
  const match = input.trim().match(/^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i)
  if (!match) return null

  let hex = match[1]!
  if (hex.length === 3 || hex.length === 4) {
    hex = hex.split('').map((ch) => `${ch}${ch}`).join('')
  }

  const r = Number.parseInt(hex.slice(0, 2), 16)
  const g = Number.parseInt(hex.slice(2, 4), 16)
  const b = Number.parseInt(hex.slice(4, 6), 16)
  const a = hex.length === 8 ? round(Number.parseInt(hex.slice(6, 8), 16) / 255) : 1
  if ([r, g, b, a].some((value) => !Number.isFinite(value))) return null
  return { r, g, b, a, source: 'hex' }
}

function parseRgbColor(input: string): Rgba | null {
  const match = input.trim().match(/^rgba?\((.+)\)$/i)
  if (!match) return null

  const body = match[1]!.trim()
  const parts = body.includes(',')
    ? body.split(',').map((part) => part.trim())
    : body.replace(/\s*\/\s*/, ' / ').split(/\s+/)

  const slashIndex = parts.indexOf('/')
  const channels = slashIndex >= 0 ? parts.slice(0, slashIndex) : parts.slice(0, 3)
  const alpha = slashIndex >= 0 ? parts[slashIndex + 1] : parts[3]
  if (channels.length < 3) return null

  const r = parseRgbChannel(channels[0]!)
  const g = parseRgbChannel(channels[1]!)
  const b = parseRgbChannel(channels[2]!)
  const a = parseAlpha(alpha)
  if ([r, g, b, a].some((value) => !Number.isFinite(value))) return null
  return { r, g, b, a, source: 'rgb' }
}

function hslToRgb(h: number, s: number, l: number): Pick<Rgba, 'r' | 'g' | 'b'> {
  const normalizedHue = ((h % 360) + 360) % 360
  const c = (1 - Math.abs(2 * l - 1)) * s
  const x = c * (1 - Math.abs((normalizedHue / 60) % 2 - 1))
  const m = l - c / 2
  const [rp, gp, bp] =
    normalizedHue < 60 ? [c, x, 0]
      : normalizedHue < 120 ? [x, c, 0]
        : normalizedHue < 180 ? [0, c, x]
          : normalizedHue < 240 ? [0, x, c]
            : normalizedHue < 300 ? [x, 0, c]
              : [c, 0, x]

  return {
    r: Math.round((rp + m) * 255),
    g: Math.round((gp + m) * 255),
    b: Math.round((bp + m) * 255),
  }
}

function parseHslColor(input: string): Rgba | null {
  const match = input.trim().match(/^hsla?\((.+)\)$/i)
  if (!match) return null

  const body = match[1]!.trim()
  const parts = body.includes(',')
    ? body.split(',').map((part) => part.trim())
    : body.replace(/\s*\/\s*/, ' / ').split(/\s+/)
  const slashIndex = parts.indexOf('/')
  const channels = slashIndex >= 0 ? parts.slice(0, slashIndex) : parts.slice(0, 3)
  const alpha = slashIndex >= 0 ? parts[slashIndex + 1] : parts[3]
  if (channels.length < 3) return null

  const h = Number.parseFloat(channels[0]!)
  const s = Number.parseFloat(channels[1]!) / 100
  const l = Number.parseFloat(channels[2]!) / 100
  const a = parseAlpha(alpha)
  if ([h, s, l, a].some((value) => !Number.isFinite(value))) return null

  const rgb = hslToRgb(h, clamp(s, 0, 1), clamp(l, 0, 1))
  return { ...rgb, a, source: 'hsl' }
}

function parseColor(input: string): Rgba | null {
  const trimmed = input.trim()
  if (!trimmed) return null
  return parseHexColor(trimmed) ?? parseRgbColor(trimmed) ?? parseHslColor(trimmed)
}

function toHexByte(value: number): string {
  return clamp(Math.round(value), 0, 255).toString(16).padStart(2, '0').toUpperCase()
}

function toHex(color: Rgba): string {
  return `#${toHexByte(color.r)}${toHexByte(color.g)}${toHexByte(color.b)}`
}

function toHexAlpha(color: Rgba): string {
  return `${toHex(color)}${toHexByte(color.a * 255)}`
}

function toRgb(color: Rgba): string {
  return `rgb(${color.r}, ${color.g}, ${color.b})`
}

function toRgbPercent(color: Rgba): string {
  return `rgb(${round((color.r / 255) * 100)}% ${round((color.g / 255) * 100)}% ${round((color.b / 255) * 100)}%)`
}

function toRgba(color: Rgba): string {
  if (color.a >= 1) return `rgba(${color.r}, ${color.g}, ${color.b})`
  return `rgba(${color.r}, ${color.g}, ${color.b}, ${round(color.a)})`
}

function toRgbaPercent(color: Rgba): string {
  const base = `${round((color.r / 255) * 100)}%, ${round((color.g / 255) * 100)}%, ${round((color.b / 255) * 100)}%`
  if (color.a >= 1) return `rgba(${base})`
  return `rgba(${base}, ${round(color.a)})`
}

function toHslParts(color: Rgba): { h: number; s: number; l: number } {
  const r = color.r / 255
  const g = color.g / 255
  const b = color.b / 255
  const max = Math.max(r, g, b)
  const min = Math.min(r, g, b)
  const l = (max + min) / 2
  const d = max - min
  if (d === 0) return { h: 0, s: 0, l: round(l * 100, 3) }

  const s = d / (1 - Math.abs(2 * l - 1))
  let h = 0
  if (max === r) h = 60 * (((g - b) / d) % 6)
  if (max === g) h = 60 * ((b - r) / d + 2)
  if (max === b) h = 60 * ((r - g) / d + 4)
  return {
    h: round((h + 360) % 360, 2),
    s: round(s * 100, 3),
    l: round(l * 100, 3),
  }
}

function toHsl(color: Rgba): string {
  const hsl = toHslParts(color)
  return `hsl(${hsl.h}, ${hsl.s}%, ${hsl.l}%)`
}

function toHsla(color: Rgba): string {
  const hsl = toHslParts(color)
  if (color.a >= 1) return `hsla(${hsl.h}, ${hsl.s}%, ${hsl.l}%)`
  return `hsla(${hsl.h}, ${hsl.s}%, ${hsl.l}%, ${round(color.a)})`
}

function toHsvParts(color: Rgba): { h: number; s: number; v: number } {
  const r = color.r / 255
  const g = color.g / 255
  const b = color.b / 255
  const max = Math.max(r, g, b)
  const min = Math.min(r, g, b)
  const d = max - min
  let h = 0
  if (d !== 0) {
    if (max === r) h = 60 * (((g - b) / d) % 6)
    if (max === g) h = 60 * ((b - r) / d + 2)
    if (max === b) h = 60 * ((r - g) / d + 4)
  }
  return {
    h: round((h + 360) % 360, 2),
    s: round(max === 0 ? 0 : (d / max) * 100, 3),
    v: round(max * 100, 3),
  }
}

function toHsva(color: Rgba): string {
  const hsv = toHsvParts(color)
  if (color.a >= 1) return `color(--hsv ${hsv.h} ${hsv.s}% ${hsv.v}%)`
  return `color(--hsv ${hsv.h} ${hsv.s}% ${hsv.v}% / ${round(color.a)})`
}

export function buildColorFormatRows(color: Rgba): ColorFormatRow[] {
  return [
    { key: 'hex', title: 'HEX', value: toHex(color) },
    { key: 'hex-lower', title: 'HEX Lower Case', value: toHex(color).toLowerCase() },
    { key: 'hex-no-prefix', title: 'HEX No Prefix', value: toHex(color).slice(1).toLowerCase() },
    { key: 'rgb', title: 'RGB', value: toRgb(color) },
    { key: 'rgb-percent', title: 'RGB %', value: toRgbPercent(color) },
    { key: 'rgba', title: 'RGBA', value: toRgba(color) },
    { key: 'rgba-percent', title: 'RGBA %', value: toRgbaPercent(color) },
    { key: 'hsla', title: 'HSLA', value: toHsla(color) },
    { key: 'hsva', title: 'HSVA', value: toHsva(color) },
  ]
}

export function buildColorConversionResults(input: string): SearchResult[] {
  const color = parseColor(input)
  if (!color) return []

  const includeAlpha = color.a < 1
  const formats: ColorFormatRow[] = [
    { key: 'rgb', title: 'RGB', value: toRgb(color) },
    { key: 'hex', title: 'HEX', value: toHex(color) },
    ...(includeAlpha ? [{ key: 'rgba', title: 'RGBA', value: toRgba(color) }] : []),
    ...(includeAlpha ? [{ key: 'hexa', title: 'HEX Alpha', value: toHexAlpha(color) }] : []),
    { key: 'hsl', title: 'HSL', value: toHsl(color) },
    ...(includeAlpha ? [{ key: 'hsla', title: 'HSLA', value: toHsla(color) }] : []),
  ]

  const ordered = color.source === 'hex'
    ? formats
    : [...formats.filter((format) => format.key === 'hex'), ...formats.filter((format) => format.key !== 'hex')]

  return ordered.map((format, index) => ({
    id: `color:${format.key}:${format.value}`,
    title: format.value,
    subtitle: `${format.title} · copy converted color`,
    category: 'color-converter',
    score: 12_000 - index,
    action: { type: 'copy-text', text: format.value },
  }))
}
