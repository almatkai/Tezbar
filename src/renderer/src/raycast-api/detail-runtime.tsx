import { useMemo, useState, type PointerEvent as ReactPointerEvent } from 'react'
import type { ExtensionRuntimeNode } from '../../../shared/extensionRuntime'
import { buildColorFormatRows, type Rgba } from '../../colorConverter'
import { Markdown } from '../../ui/Markdown'
import { cx } from '../../ui/primitives'

function normalizeMetadataType(type: string): string {
  return type.replace(/^List\.Item\./, '')
}

function metadataText(value: unknown): string {
  if (typeof value === 'string') return value
  if (typeof value === 'number' || typeof value === 'boolean') return String(value)
  if (value && typeof value === 'object') {
    const candidate = value as { value?: unknown; text?: unknown; title?: unknown }
    if (candidate.value !== undefined) return metadataText(candidate.value)
    if (candidate.text !== undefined) return metadataText(candidate.text)
    if (candidate.title !== undefined) return metadataText(candidate.title)
  }
  return ''
}

function MetadataIcon({ icon }: { icon: unknown }): JSX.Element | null {
  const token = metadataText(icon && typeof icon === 'object' ? (icon as { source?: unknown }).source : icon)
    .replace(/^Icon\./, '')
    .toLowerCase()
  if (!token) return null

  return (
    <span className="inline-grid h-4 w-4 place-items-center text-accent-1" aria-hidden="true">
      <svg viewBox="0 0 24 24" className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth="2">
        {token.includes('download') ? (
          <path d="M12 3v12m0 0 4-4m-4 4-4-4M5 19h14" />
        ) : token.includes('upload') ? (
          <path d="M12 21V9m0 0 4 4m-4-4-4 4M5 5h14" />
        ) : token.includes('server') || token.includes('harddrive') ? (
          <>
            <rect x="3" y="5" width="18" height="14" rx="2" />
            <path d="M7 15h.01M11 15h6" />
          </>
        ) : token.includes('globe') || token.includes('network') ? (
          <>
            <circle cx="12" cy="12" r="9" />
            <path d="M3 12h18M12 3c2.5 2.7 3.7 5.7 3.7 9S14.5 18.3 12 21M12 3c-2.5 2.7-3.7 5.7-3.7 9S9.5 18.3 12 21" />
          </>
        ) : token.includes('link') ? (
          <path d="M10 13a5 5 0 0 0 7.1 0l1.4-1.4a5 5 0 0 0-7.1-7.1L10.5 5M14 11a5 5 0 0 0-7.1 0l-1.4 1.4a5 5 0 0 0 7.1 7.1l.9-.9" />
        ) : (
          <>
            <circle cx="12" cy="12" r="8" />
            <path d="M12 8v8M8 12h8" />
          </>
        )}
      </svg>
    </span>
  )
}

export function MetadataItem({ node }: { node: ExtensionRuntimeNode }): JSX.Element | null {
  const type = normalizeMetadataType(node.type)
  const props = node.props ?? {}

  if (type === 'Detail.Metadata.Label') {
    const title = metadataText(props.title)
    const text = metadataText(props.text)

    if (!text && props.icon) {
      return (
        <div className="flex items-center gap-2 px-3 pb-1 pt-3 text-[12px] font-semibold text-ink-2 first:pt-1">
          <MetadataIcon icon={props.icon} />
          <span>{title}</span>
        </div>
      )
    }

    return (
      <div className="flex items-start justify-between gap-3 px-3 py-1.5 text-[11px]">
        <span className="shrink-0 font-medium text-ink-3">{title}</span>
        <span className="min-w-0 text-right font-semibold text-ink-1">
          {props.icon ? <span className="mr-1.5 inline-block align-[-2px]"><MetadataIcon icon={props.icon} /></span> : null}
          {text}
        </span>
      </div>
    )
  }

  if (type === 'Detail.Metadata.Separator') {
    return <div className="my-1.5 h-px bg-white/10" />
  }

  if (type === 'Detail.Metadata.Link') {
    return (
      <div className="flex items-start justify-between gap-3 px-3 py-1.5 text-[11px]">
        <span className="shrink-0 font-medium text-ink-3">{metadataText(props.title)}</span>
        <a
          href={String(props.target || '')}
          target="_blank"
          rel="noreferrer"
          className="min-w-0 truncate text-right font-semibold text-accent-1 hover:underline"
          onClick={(e) => {
            e.preventDefault()
            const url = String(props.target || '')
            if (url) window.tezbar.openExternalUrl(url)
          }}
        >
          {metadataText(props.text) || metadataText(props.target)}
        </a>
      </div>
    )
  }

  if (type === 'Detail.Metadata.TagList') {
    return (
      <div className="flex items-start justify-between gap-3 px-3 py-1.5 text-[11px]">
        <span className="shrink-0 font-medium text-ink-3">{metadataText(props.title)}</span>
        <div className="flex flex-wrap items-center justify-end gap-1.5">
          {(node.children ?? []).map((child, i) => (
            <span
              key={i}
              className={cx(
                "inline-flex items-center rounded-tezbar-chip border border-white/10 bg-white/5 px-1.5 py-0.5 text-[10px] font-medium transition-colors",
                child.props?.color === 'success' ? "text-green-400 border-green-400/20 bg-green-400/10" :
                  child.props?.color === 'warning' ? "text-amber-400 border-amber-400/20 bg-amber-400/10" :
                    child.props?.color === 'error' ? "text-red-400 border-red-400/20 bg-red-400/10" :
                      "text-ink-2"
              )}
            >
              {child.props?.icon ? <span className="mr-1 inline-block align-[-2px]"><MetadataIcon icon={child.props.icon} /></span> : null}
              {metadataText(child.props?.text)}
            </span>
          ))}
        </div>
      </div>
    )
  }

  return null
}

export function MetadataSidebar({ root }: { root: ExtensionRuntimeNode }): JSX.Element | null {
  const children = root.children ?? []
  if (children.length === 0) return null

  return (
    <aside className="flex w-[280px] shrink-0 flex-col overflow-y-auto border-l border-white/8 bg-black/5 py-3">
      {children.map((child, i) => (
        <MetadataItem key={i} node={child} />
      ))}
    </aside>
  )
}

function markdownFromNode(root: ExtensionRuntimeNode): string {
  if (typeof root.props?.markdown === 'string') return root.props.markdown

  for (const child of root.children ?? []) {
    if (typeof child.props?.markdown === 'string') {
      return child.props.markdown
    }
  }

  return ''
}

function fileUrlFromPath(path: string): string {
  const normalized = path.startsWith('/') ? path : `/${path}`
  return `file://${encodeURI(normalized)}`
}

function resolveMarkdownImageSrc(src: string | undefined, assetsPath: string): string | undefined {
  const raw = String(src || '').trim()
  if (!raw) return src
  if (/^https?:\/\//i.test(raw) || raw.startsWith('data:') || raw.startsWith('file://')) {
    return raw
  }

  const cleanSrc = raw.split(/[?#]/)[0]?.replace(/^\.?\//, '') ?? ''
  if (!cleanSrc) return raw
  if (cleanSrc.startsWith('/')) return fileUrlFromPath(cleanSrc)
  if (!assetsPath) return raw

  return fileUrlFromPath(`${assetsPath.replace(/\/+$/, '')}/${cleanSrc}`)
}

function parseImageOnlyMarkdown(markdown: string, assetsPath: string): {
  src: string
  alt: string
  height?: number
} | null {
  const match = markdown.trim().match(/^!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)$/)
  if (!match) return null

  const rawSrc = match[2] ?? ''
  const heightMatch = rawSrc.match(/[?&]raycast-height=(\d+)/)
  const height = heightMatch ? Number(heightMatch[1]) : undefined
  const src = resolveMarkdownImageSrc(rawSrc, assetsPath)
  if (!src) return null

  return {
    src,
    alt: match[1] ?? '',
    height: Number.isFinite(height) && height && height > 0 ? height : undefined,
  }
}

function hsvToRgb(h: number, s: number, v: number): Rgba {
  const c = v * s
  const x = c * (1 - Math.abs((h / 60) % 2 - 1))
  const m = v - c
  const [rp, gp, bp] =
    h < 60 ? [c, x, 0]
      : h < 120 ? [x, c, 0]
        : h < 180 ? [0, c, x]
          : h < 240 ? [0, x, c]
            : h < 300 ? [x, 0, c]
              : [c, 0, x]

  return {
    r: Math.round((rp + m) * 255),
    g: Math.round((gp + m) * 255),
    b: Math.round((bp + m) * 255),
    a: 1,
    source: 'rgb',
  }
}

function rgbToHsv(color: Rgba): { h: number; s: number; v: number } {
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
  return { h: (h + 360) % 360, s: max === 0 ? 0 : d / max, v: max }
}

function colorFromInitial(value: unknown): Rgba {
  if (!value || typeof value !== 'object') {
    return { r: 255, g: 44, b: 72, a: 1, source: 'rgb' }
  }
  const raw = value as { red?: unknown; green?: unknown; blue?: unknown; alpha?: unknown }
  const toByte = (entry: unknown): number | null => {
    const number = Number(entry)
    if (!Number.isFinite(number)) return null
    return Math.round(Math.max(0, Math.min(255, number <= 1 ? number * 255 : number)))
  }
  const toAlpha = (entry: unknown): number => {
    const number = Number(entry ?? 1)
    if (!Number.isFinite(number)) return 1
    return Math.max(0, Math.min(1, number > 1 ? number / 255 : number))
  }
  const r = toByte(raw.red)
  const g = toByte(raw.green)
  const b = toByte(raw.blue)
  if (r === null || g === null || b === null) {
    return { r: 255, g: 44, b: 72, a: 1, source: 'rgb' }
  }
  return { r, g, b, a: toAlpha(raw.alpha), source: 'rgb' }
}

function ColorWheelDetail({
  image,
  initialColor,
}: {
  image: { src: string; alt: string }
  initialColor?: unknown
}): JSX.Element {
  const [color, setColor] = useState<Rgba>(() => colorFromInitial(initialColor))
  const [copiedKey, setCopiedKey] = useState<string | null>(null)
  const formats = useMemo(() => buildColorFormatRows(color), [color])
  const marker = useMemo(() => {
    const hsv = rgbToHsv(color)
    const radians = (hsv.h * Math.PI) / 180
    const radius = hsv.s * 50
    return {
      left: `${50 + Math.sin(radians) * radius}%`,
      top: `${50 - Math.cos(radians) * radius}%`,
    }
  }, [color])

  const pickFromPointer = (event: ReactPointerEvent<HTMLButtonElement>): void => {
    const rect = event.currentTarget.getBoundingClientRect()
    const size = Math.min(rect.width, rect.height)
    const cx = rect.left + rect.width / 2
    const cy = rect.top + rect.height / 2
    const dx = event.clientX - cx
    const dy = event.clientY - cy
    const radius = Math.sqrt(dx * dx + dy * dy)
    const maxRadius = size / 2
    if (radius > maxRadius) return

    const saturation = Math.min(1, radius / maxRadius)
    const hue = ((Math.atan2(dx, -dy) * 180) / Math.PI + 360) % 360
    setColor(hsvToRgb(hue, saturation, 1))
  }

  const copyFormat = async (key: string, value: string): Promise<void> => {
    await window.tezbar.clipboardWriteText(value)
    setCopiedKey(key)
    window.setTimeout(() => setCopiedKey((current) => (current === key ? null : current)), 900)
  }

  return (
    <div className="grid h-full min-h-[520px] grid-cols-[minmax(0,1fr)_320px] gap-4">
      <div className="flex min-h-0 items-center justify-center overflow-hidden rounded-tezbar-row border border-white/8 bg-black/10 p-5">
        <button
          type="button"
          className="relative aspect-square max-h-full w-full max-w-[720px] cursor-crosshair rounded-full bg-center bg-contain bg-no-repeat outline-none"
          style={{ backgroundImage: `url("${image.src}")` }}
          aria-label={image.alt}
          onPointerDown={(event) => {
            event.currentTarget.setPointerCapture(event.pointerId)
            pickFromPointer(event)
          }}
          onPointerMove={(event) => {
            if (event.buttons === 1) pickFromPointer(event)
          }}
        >
          <span
            className="absolute h-4 w-4 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgba(0,0,0,0.45),0_4px_14px_rgba(0,0,0,0.35)]"
            style={{ ...marker, background: formats[0]?.value ?? '#fff' }}
          />
        </button>
      </div>

      <aside className="flex min-h-0 flex-col overflow-hidden rounded-tezbar-row border border-white/8 bg-black/10">
        <div className="flex items-center gap-3 border-b border-white/8 px-4 py-3">
          <span
            className="h-8 w-8 shrink-0 rounded-tezbar-chip border border-white/15"
            style={{ background: formats[0]?.value ?? '#fff' }}
          />
          <div className="min-w-0">
            <p className="truncate text-[13px] font-semibold text-ink-1">Color Formats</p>
            <p className="truncate font-mono text-[11px] text-ink-3">{formats[0]?.value}</p>
          </div>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-2">
          {formats.map((format) => (
            <button
              key={format.key}
              type="button"
              onClick={() => void copyFormat(format.key, format.value)}
              className="group flex w-full items-start justify-between gap-3 rounded-tezbar-row px-3 py-2.5 text-left transition hover:bg-white/10"
            >
              <span className="min-w-0">
                <span className="block text-[11px] font-semibold uppercase tracking-[0.12em] text-ink-4">
                  {format.title}
                </span>
                <span className="mt-1 block truncate font-mono text-[13px] text-ink-1">
                  {format.value}
                </span>
              </span>
              <span className="mt-4 shrink-0 text-[10px] text-ink-4 opacity-0 transition group-hover:opacity-100">
                {copiedKey === format.key ? 'Copied' : 'Copy'}
              </span>
            </button>
          ))}
        </div>
      </aside>
    </div>
  )
}

export function DetailRuntime({
  root,
  title,
  onBack,
  onRunPrimaryAction,
  onOpenActions,
}: {
  root: ExtensionRuntimeNode
  title: string
  onBack: () => void
  onRunPrimaryAction: () => void
  onOpenActions: () => void
}): JSX.Element {
  const markdown = markdownFromNode(root)
  const assetsPath = typeof root.props?.assetsPath === 'string' ? root.props.assetsPath : ''
  const imageOnly = parseImageOnlyMarkdown(markdown, assetsPath)

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="glass-card mb-2 shrink-0 px-3 py-2">
        <div className="flex items-center gap-2">
          <button type="button" className="btn btn-ghost" onClick={onBack}>
            Back
          </button>
          <div className="text-[12px] font-semibold text-ink-2">{title}</div>
          <div className="ml-auto flex items-center gap-1">
            <button type="button" className="btn btn-ghost" onClick={onRunPrimaryAction}>
              Enter
            </button>
            <button type="button" className="btn btn-ghost" onClick={onOpenActions}>
              Cmd+K
            </button>
          </div>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 overflow-hidden">
        <div className="glass-card min-h-0 flex-1 overflow-y-auto px-4 py-4">
          {imageOnly && imageOnly.alt.toLowerCase().includes('color wheel') ? (
            <ColorWheelDetail image={imageOnly} initialColor={root.props?.initialColor} />
          ) : imageOnly ? (
            <div className="flex h-full min-h-[420px] items-center justify-center">
              <img
                src={imageOnly.src}
                alt={imageOnly.alt}
                className="w-auto max-w-full rounded-tezbar-row object-contain"
                style={{ maxHeight: imageOnly.height ? `${imageOnly.height}px` : '70vh' }}
              />
            </div>
          ) : markdown ? (
            <article className="prose prose-invert max-w-none text-[13px] leading-relaxed">
              <Markdown
                text={markdown}
                className="text-[13px] leading-relaxed"
                imageSrcResolver={(src) => resolveMarkdownImageSrc(src, assetsPath)}
              />
            </article>
          ) : (
            <div className="text-[12px] text-ink-3">No detail content</div>
          )}
        </div>

        {root.metadata ? <MetadataSidebar root={root.metadata} /> : null}
      </div>
    </div>
  )
}
