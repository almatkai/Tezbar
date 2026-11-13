import { memo, useEffect, useMemo, useRef } from 'react'
import { Marked } from 'marked'
import { markedHighlight } from 'marked-highlight'
import hljs from 'highlight.js/lib/common'
import DOMPurify from 'dompurify'

/**
 * Lightweight markdown renderer tailored for the Tezbar HUD.
 *
 * - GFM (tables, strikethrough, autolinks, task lists) via `marked`
 * - Syntax highlighting via `highlight.js` (common-languages bundle)
 * - HTML is sanitized with DOMPurify before insertion
 * - Streaming-safe: we auto-close an unterminated ``` fence so partial
 *   output from the agent still renders as a code block while it streams.
 * - Code blocks get a header with language + copy-to-clipboard button
 *   (wired in after render via a small effect).
 */

// Rich parsing is reserved for settled content. Re-running syntax highlighting
// over a growing code block on every stream frame is especially expensive.
const MAX_HIGHLIGHT_CHARS = 12_000
const richMarked = new Marked(
  markedHighlight({
    langPrefix: 'hljs language-',
    highlight(code, lang) {
      const language = lang && hljs.getLanguage(lang) ? lang : undefined
      try {
        if (language && code.length <= MAX_HIGHLIGHT_CHARS) {
          return hljs.highlight(code, { language, ignoreIllegals: true }).value
        }
        // Auto-detection runs every registered grammar and large blocks can
        // stall the UI at the exact moment a response finishes. Unlabelled or
        // unusually large code remains readable without highlighting.
        return escapeHtml(code)
      } catch {
        return escapeHtml(code)
      }
    },
  })
)
const streamingMarked = new Marked()

for (const parser of [richMarked, streamingMarked]) {
  parser.setOptions({
    gfm: true,
    breaks: true,
  })
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

/**
 * Count opening code fences that haven't been closed yet so we can auto-close
 * them during a streaming response. This prevents the rest of the partial
 * message from being swallowed as if it were still inside a fence.
 */
function balanceStreamingFences(input: string): string {
  let openFence: { character: '`' | '~'; length: number } | null = null
  const lines = input.split('\n')
  for (const line of lines) {
    const match = /^\s{0,3}(`{3,}|~{3,})/.exec(line)
    if (!match) continue
    const marker = match[1]!
    const character = marker[0] as '`' | '~'
    if (!openFence) {
      openFence = { character, length: marker.length }
    } else if (character === openFence.character && marker.length >= openFence.length) {
      openFence = null
    }
  }
  if (openFence) {
    return `${input}\n${openFence.character.repeat(openFence.length)}`
  }
  return input
}

export type MarkdownProps = {
  text: string
  /**
   * When true, we treat the input as a partial stream and will auto-close
   * dangling code fences so the remainder still renders nicely.
   */
  streaming?: boolean
  className?: string
  /**
   * Optional resolver for image `src` values. Receives the raw src from the
   * markdown and should return the resolved URL, or undefined to leave it
   * unchanged.
   */
  imageSrcResolver?: (src: string) => string | undefined
}

function rewriteImageSrcs(html: string, resolver: (src: string) => string | undefined): string {
  const parser = new DOMParser()
  const doc = parser.parseFromString(html, 'text/html')
  for (const img of doc.querySelectorAll('img')) {
    const raw = img.getAttribute('src')
    if (!raw) continue
    const resolved = resolver(raw)
    if (resolved) img.setAttribute('src', resolved)
  }
  return doc.body.innerHTML
}

function MarkdownView({
  text,
  streaming = false,
  className,
  imageSrcResolver,
}: MarkdownProps): JSX.Element {
  const containerRef = useRef<HTMLDivElement>(null)

  const html = useMemo(() => {
    const source = streaming ? balanceStreamingFences(text) : text
    const parser = streaming ? streamingMarked : richMarked
    let raw = parser.parse(source, { async: false }) as string
    if (imageSrcResolver) {
      raw = rewriteImageSrcs(raw, imageSrcResolver)
    }
    return DOMPurify.sanitize(raw, {
      ADD_ATTR: ['target', 'rel'],
    })
  }, [text, streaming, imageSrcResolver])

  // After each render, upgrade <pre><code> blocks with a language chip +
  // "Copy" button. We attach event listeners via delegation on the container.
  useEffect(() => {
    const root = containerRef.current
    if (!root) return

    if (!streaming) {
      const pres = root.querySelectorAll('pre')
      pres.forEach((pre) => {
        if (pre.dataset.raymesDecorated === '1') return
        const codeEl = pre.querySelector('code')
        if (!codeEl) return
        pre.dataset.raymesDecorated = '1'

        // Derive language from "language-xxx" class.
        const langClass = Array.from(codeEl.classList).find((c) => c.startsWith('language-'))
        const lang = langClass ? langClass.replace('language-', '').replace('hljs', '').trim() : ''

        const header = document.createElement('div')
        header.className = 'tezbar-md-code-header'

        const langLabel = document.createElement('span')
        langLabel.className = 'tezbar-md-code-lang'
        langLabel.textContent = lang || 'code'
        header.appendChild(langLabel)

        const copyBtn = document.createElement('button')
        copyBtn.type = 'button'
        copyBtn.className = 'tezbar-md-code-copy'
        copyBtn.textContent = 'Copy'
        header.appendChild(copyBtn)

        pre.prepend(header)
      })
    }

    const resetTimers = new Set<number>()
    const onCodeCopy = (event: Event): void => {
      const target = event.target
      if (!(target instanceof Element)) return
      const copyBtn = target.closest<HTMLButtonElement>('.tezbar-md-code-copy')
      if (!copyBtn || !root.contains(copyBtn)) return
      const raw = copyBtn.closest('pre')?.querySelector('code')?.textContent ?? ''
      const showResult = (label: 'Copied' | 'Failed'): void => {
        copyBtn.textContent = label
        const resetTimer = window.setTimeout(() => {
          resetTimers.delete(resetTimer)
          if (copyBtn.isConnected) copyBtn.textContent = 'Copy'
        }, 1200)
        resetTimers.add(resetTimer)
      }

      if (window.tezbar && typeof window.tezbar.clipboardWriteText === 'function') {
        window.tezbar.clipboardWriteText(raw).then(
          (result) => showResult(result && result.ok === false ? 'Failed' : 'Copied'),
          () => showResult('Failed')
        )
      } else {
        void navigator.clipboard.writeText(raw).then(
          () => showResult('Copied'),
          () => showResult('Failed')
        )
      }
    }
    root.addEventListener('click', onCodeCopy)

    // Make all links open externally with safe rel.
    const links = root.querySelectorAll('a[href]')
    links.forEach((a) => {
      a.setAttribute('target', '_blank')
      a.setAttribute('rel', 'noopener noreferrer')
    })

    return () => {
      root.removeEventListener('click', onCodeCopy)
      resetTimers.forEach((timer) => window.clearTimeout(timer))
    }
  }, [html, streaming])

  return (
    <div
      ref={containerRef}
      className={className ? `tezbar-md ${className}` : 'tezbar-md'}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  )
}

export const Markdown = memo(MarkdownView)
