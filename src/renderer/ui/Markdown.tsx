import { useEffect, useMemo, useRef } from 'react'
import { Marked } from 'marked'
import { markedHighlight } from 'marked-highlight'
import hljs from 'highlight.js/lib/common'
import DOMPurify from 'dompurify'

/**
 * Lightweight markdown renderer tailored for the Raymes HUD.
 *
 * - GFM (tables, strikethrough, autolinks, task lists) via `marked`
 * - Syntax highlighting via `highlight.js` (common-languages bundle)
 * - HTML is sanitized with DOMPurify before insertion
 * - Streaming-safe: we auto-close an unterminated ``` fence so partial
 *   output from the agent still renders as a code block while it streams.
 * - Code blocks get a header with language + copy-to-clipboard button
 *   (wired in after render via a small effect).
 */

// Shared Marked instance (configured once).
const marked = new Marked(
  markedHighlight({
    langPrefix: 'hljs language-',
    highlight(code, lang) {
      const language = lang && hljs.getLanguage(lang) ? lang : undefined
      try {
        if (language) {
          return hljs.highlight(code, { language, ignoreIllegals: true }).value
        }
        return hljs.highlightAuto(code).value
      } catch {
        return escapeHtml(code)
      }
    },
  }),
)

marked.setOptions({
  gfm: true,
  breaks: true,
})

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
  let fenceCount = 0
  const lines = input.split('\n')
  for (const line of lines) {
    if (/^\s{0,3}(```|~~~)/.test(line)) fenceCount += 1
  }
  if (fenceCount % 2 === 1) {
    return input + '\n```'
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

export function Markdown({ text, streaming = false, className, imageSrcResolver }: MarkdownProps): JSX.Element {
  const containerRef = useRef<HTMLDivElement>(null)

  const html = useMemo(() => {
    const source = streaming ? balanceStreamingFences(text) : text
    let raw = marked.parse(source, { async: false }) as string
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
      header.className = 'raymes-md-code-header'

      const langLabel = document.createElement('span')
      langLabel.className = 'raymes-md-code-lang'
      langLabel.textContent = lang || 'code'
      header.appendChild(langLabel)

      const copyBtn = document.createElement('button')
      copyBtn.type = 'button'
      copyBtn.className = 'raymes-md-code-copy'
      copyBtn.textContent = 'Copy'
      copyBtn.addEventListener('click', () => {
        const raw = codeEl.textContent ?? ''
        void navigator.clipboard.writeText(raw).then(
          () => {
            copyBtn.textContent = 'Copied'
            window.setTimeout(() => {
              copyBtn.textContent = 'Copy'
            }, 1200)
          },
          () => {
            copyBtn.textContent = 'Failed'
            window.setTimeout(() => {
              copyBtn.textContent = 'Copy'
            }, 1200)
          },
        )
      })
      header.appendChild(copyBtn)

      pre.prepend(header)
    })

    // Make all links open externally with safe rel.
    const links = root.querySelectorAll('a[href]')
    links.forEach((a) => {
      a.setAttribute('target', '_blank')
      a.setAttribute('rel', 'noopener noreferrer')
    })
  }, [html])

  return (
    <div
      ref={containerRef}
      className={className ? `raymes-md ${className}` : 'raymes-md'}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  )
}
