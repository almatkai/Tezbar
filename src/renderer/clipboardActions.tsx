/** "Convert" action row for the clipboard history view.
 *
 *  Renders a row of small chips (JSON, Base64, Encode, Hash, Count, Case,
 *  QR) under a text entry. Clicking one runs the matching transformer from
 *  `../shared/textTransform` and shows the result inline, with a "Copy
 *  result" button that writes back to the clipboard without replacing the
 *  selected history entry.
 *
 *  State is per-entry — the parent keys this component by entry id so the
 *  result resets when the selection moves.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { ClipboardActionId, TransformResult } from '../shared/textTransform'
import { applyClipboardAction } from '../shared/textTransform'

type ActionSpec = {
  id: ClipboardActionId
  label: string
  /** A short glyph drawn inside the chip's left icon slot. */
  glyph: JSX.Element
}

/* Tiny inline SVG glyphs, matching the CommandIconGlyph line-weight used in
 * CommandBar.tsx (1.2 stroke, 14×14 viewBox). */
const ACTION_SPECS: ActionSpec[] = [
  {
    id: 'json',
    label: 'JSON',
    glyph: (
      <svg width="11" height="11" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <path d="M4.5 2.5c-1 0-1.5.5-1.5 1.5v2c0 .8-.4 1.2-1.2 1.5.8.3 1.2.7 1.2 1.5v2c0 1 .5 1.5 1.5 1.5M9.5 2.5c1 0 1.5.5 1.5 1.5v2c0 .8.4 1.2 1.2 1.5-.8.3-1.2.7-1.2 1.5v2c0 1-.5 1.5-1.5 1.5" />
      </svg>
    ),
  },
  {
    id: 'base64',
    label: 'Base64',
    glyph: (
      <svg width="11" height="11" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <rect x="2" y="2.5" width="10" height="9" rx="1.5" />
        <path d="M4.5 6.5h5M4.5 8.5h3" />
      </svg>
    ),
  },
  {
    id: 'encode',
    label: 'Encode',
    glyph: (
      <svg width="11" height="11" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <path d="M5.5 8.5 8.5 5.5M7.25 3.25l1-1a2.25 2.25 0 0 1 3.18 3.18l-1 1M6.75 10.75l-1 1a2.25 2.25 0 0 1-3.18-3.18l1-1" />
      </svg>
    ),
  },
  {
    id: 'hash',
    label: 'Hash',
    glyph: (
      <svg width="11" height="11" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <path d="M5 2 4 12M10 2 9 12M2.5 5h9M2 9h9" />
      </svg>
    ),
  },
  {
    id: 'count',
    label: 'Count',
    glyph: (
      <svg width="11" height="11" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <path d="M3 2.5h8M3 7h8M3 11.5h5" />
      </svg>
    ),
  },
  {
    id: 'case',
    label: 'Case',
    glyph: (
      <svg width="11" height="11" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <path d="M3 10.5 5.4 3.5h.2L8 10.5M3.8 8.5h3.4M9.5 10.5v-3.8c0-.6.4-1 1-1 1 0 1.5.7 1.5 1.5 0 .9-.6 1.5-1.5 1.6l1.7 1.7" />
      </svg>
    ),
  },
  {
    id: 'qr',
    label: 'QR',
    glyph: (
      <svg width="11" height="11" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <rect x="2" y="2" width="4" height="4" rx=".5" />
        <rect x="8" y="2" width="4" height="4" rx=".5" />
        <rect x="2" y="8" width="4" height="4" rx=".5" />
        <path d="M8 8h1.5M11 8h1M8 11v1M10 10h2v2h-1.5" />
      </svg>
    ),
  },
]

function actionChipClass(active: boolean): string {
  // Mirror CommandBar tones: inactive rows read as neutral chip, active gets
  // the emerald tint used for "content" commands (clipboard/snippets/notes).
  return active
    ? 'inline-flex items-center gap-1 rounded-tezbar-chip border border-emerald-300/25 bg-emerald-300/10 px-1.5 py-0.5 text-[10.5px] font-medium text-emerald-200 transition'
    : 'inline-flex items-center gap-1 rounded-tezbar-chip border border-white/10 bg-white/[0.04] px-1.5 py-0.5 text-[10.5px] text-ink-3 transition hover:border-white/20 hover:bg-white/[0.06] hover:text-ink-1'
}

export function ConvertActionRow({
  sourceText,
  onCopied,
  onError,
}: {
  /** Raw text the transforms should run against. Already de-secret'd by
   *  the caller. */
  sourceText: string
  onCopied: (label: string) => void
  onError: (label: string) => void
}): JSX.Element {
  const [active, setActive] = useState<ClipboardActionId | null>(null)
  const [busy, setBusy] = useState<ClipboardActionId | null>(null)
  const [result, setResult] = useState<TransformResult | null>(null)
  const lastRunId = useRef(0)

  // Reset when the input swaps out — keeps the preview map 1:1 with the
  // row of chips visible above it.
  useEffect(() => {
    lastRunId.current += 1
    setActive(null)
    setBusy(null)
    setResult(null)
  }, [sourceText])

  const runTransform = useCallback(
    async (action: ClipboardActionId) => {
      const runId = ++lastRunId.current
      setActive(action)
      setBusy(action)
      setResult(null)
      try {
        const out = await applyClipboardAction(action, sourceText)
        if (runId !== lastRunId.current) return
        setResult(out)
      } catch (err) {
        if (runId !== lastRunId.current) return
        setResult({
          ok: false,
          error: err instanceof Error ? err.message : 'Transform failed',
        })
      } finally {
        if (runId === lastRunId.current) setBusy(null)
      }
    },
    [sourceText],
  )

  const copyResult = useCallback(async () => {
    if (!result || !result.ok) return
    const res = await window.tezbar.clipboardWriteText(result.output)
    if (res.ok) {
      onCopied('Result copied to clipboard')
    } else {
      onError('Could not copy result')
    }
  }, [result, onCopied, onError])

  const isQr = active === 'qr' && result?.ok
  const qrImageUrl = useMemo(() => {
    if (!isQr || !result || !result.ok) return null
    return result.output
  }, [isQr, result])

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex flex-wrap items-center gap-1">
        {ACTION_SPECS.map((spec) => (
          <button
            key={spec.id}
            type="button"
            disabled={busy === spec.id}
            onClick={() => void runTransform(spec.id)}
            className={actionChipClass(active === spec.id)}
            aria-pressed={active === spec.id}
          >
            <span className="grid place-items-center">{spec.glyph}</span>
            <span>{spec.label}</span>
          </button>
        ))}
      </div>
      {result ? (
        <div className="rounded-tezbar-row border border-white/[0.06] bg-white/[0.02] p-2">
          {result.ok ? (
            qrImageUrl ? (
              <div className="flex flex-col items-start gap-1.5">
                <img
                  src={qrImageUrl}
                  alt="QR code preview"
                  className="h-[120px] w-[120px] rounded-sm border border-white/10 bg-white/90 p-1"
                />
                <ResultFooter url={qrImageUrl} onCopy={() => void copyResult()} />
              </div>
            ) : (
              <div className="flex flex-col gap-1.5">
                <pre className="max-h-40 overflow-y-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-[1.5] text-ink-1">
                  {result.output}
                </pre>
                <ResultFooter onCopy={() => void copyResult()} />
              </div>
            )
          ) : (
            <p className="text-[11px] text-rose-300">{result.error}</p>
          )}
        </div>
      ) : null}
    </div>
  )
}

function ResultFooter({
  onCopy,
  url,
}: {
  onCopy: () => void
  url?: string
}): JSX.Element {
  return (
    <div className="flex items-center justify-between gap-2">
      {url ? (
        <span className="min-w-0 truncate text-[9.5px] text-ink-4">{url}</span>
      ) : (
        <span className="text-[9.5px] uppercase tracking-[0.14em] text-ink-4">Result</span>
      )}
      <button
        type="button"
        onClick={onCopy}
        className="inline-flex items-center gap-1 rounded-tezbar-chip border border-emerald-300/25 bg-emerald-300/10 px-1.5 py-0.5 text-[10.5px] font-medium text-emerald-200 transition hover:border-emerald-300/40 hover:bg-emerald-300/20"
      >
        <svg width="10" height="10" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
          <rect x="4" y="4.5" width="7" height="7" rx="1" />
          <path d="M9 4.5V3.25a1 1 0 0 0-1-1H3.75a1 1 0 0 0-1 1V7.5a1 1 0 0 0 1 1H5" />
        </svg>
        <span>Copy result</span>
      </button>
    </div>
  )
}
