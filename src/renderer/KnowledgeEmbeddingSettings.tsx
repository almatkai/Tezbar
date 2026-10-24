import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import type {
  EmbeddingVariantId,
  KnowledgeSnapshot,
  SemanticSearchStatus,
  SemanticVariantStatus,
} from '../shared/knowledge'
import { KNOWLEDGE_ACTION_TIMEOUT_MS } from './useKnowledgeSettings'
import { cx } from './ui/primitives'

type Props = {
  knowledge: KnowledgeSnapshot | null
  onChange: (snapshot: KnowledgeSnapshot) => void
  onRefresh: () => void
  loading: boolean
  loadError: string | null
  snapshotSeq: number
  snapshotSeqRef: { readonly current: number }
}

type Pending = {
  token: number
  kind: 'switch' | 'download' | 'cancel' | 'remove' | 'speed'
  variant?: EmbeddingVariantId
  target?: string
  trigger: string
  label: string
}
type Phase =
  | { type: 'idle' }
  | { type: 'pending'; op: Pending }
  | { type: 'reconciling' | 'unconfirmed'; label: string; afterSeq: number }

const variantIds = ['q8_0', 'q4_k_m'] as const
const variantName = (id: EmbeddingVariantId) =>
  id === 'q8_0' ? 'Standard (Q8_0)' : 'Compact (Q4_K_M)'
const modelValue = (variant: EmbeddingVariantId) => `embeddinggemma-2:${variant}`
const megabytes = (bytes: number) => Math.round(bytes / 1_000_000)
const legacyWeightBytes = { q8_0: 309_855_456, q4_k_m: 181_695_104 } as const
const downloadPercent = (progress: number) => Math.max(0, Math.min(99, Math.floor(progress * 100)))

function pendingLabel(op: Pending) {
  const name = op.variant ? variantName(op.variant) : 'Built-in lightweight search'
  switch (op.kind) {
    case 'switch':
      return `Switching to ${name}…`
    case 'download':
      return `Starting ${name} download…`
    case 'cancel':
      return `Cancelling ${name} download…`
    case 'remove':
      return `Removing ${name}…`
    case 'speed':
      return 'Changing chunking speed…'
  }
}

// Only old snapshots need artifact metadata defaults. New backends supply the
// weight size separately from downloadTotalBytes (which can include the runtime).
export function semanticVariants(status: SemanticSearchStatus): SemanticVariantStatus[] {
  return variantIds.map((id) => {
    const supplied = status.variants?.find((variant) => variant.id === id)
    if (supplied) return supplied
    const legacy = !status.variants && id === 'q8_0'
    const failure: SemanticVariantStatus['failure'] =
      !legacy || !status.error
        ? undefined
        : status.errorSource === 'integrity'
          ? { stage: 'integrity', message: status.error }
          : status.errorSource === 'runtime' && status.modelState === 'ready'
            ? { stage: 'startup', message: status.error }
            : status.errorSource === 'download' ||
                (!status.errorSource && status.modelState === 'error')
              ? { stage: 'download', message: status.error }
              : undefined
    return {
      id,
      weightBytes: legacy
        ? (status.weightBytes ?? legacyWeightBytes[id])
        : !status.variants
          ? legacyWeightBytes[id]
          : 0,
      state: legacy
        ? status.integrityState === 'verifying' && status.modelState !== 'downloading'
          ? 'verifying'
          : failure?.stage === 'integrity' && status.modelState !== 'downloading'
            ? 'error'
            : status.modelState
        : 'not-downloaded',
      downloadProgress: legacy ? status.downloadProgress : 0,
      downloadBytes: legacy ? status.downloadBytes : 0,
      downloadTotalBytes: legacy ? status.downloadTotalBytes : 0,
      failure,
    }
  })
}

function optionSuffix(variant: SemanticVariantStatus) {
  if (variant.failure?.stage === 'startup') return " — couldn't start"
  switch (variant.state) {
    case 'not-downloaded':
      return ' — download required'
    case 'downloading':
      return ' — downloading'
    case 'verifying':
      return ' — verifying'
    case 'error':
      return ' — needs repair'
    default:
      return ''
  }
}

export function KnowledgeEmbeddingSettings({
  knowledge,
  onChange,
  onRefresh,
  loading,
  loadError,
  snapshotSeq,
  snapshotSeqRef,
}: Props) {
  const mounted = useRef(false)
  const token = useRef(0)
  const timer = useRef<number | null>(null)
  const lockTrigger = useRef<string | null>(null)
  const focusedOperation = useRef<{
    element: HTMLElement
    trigger: string
    container: HTMLElement
    focusedBeforeRemoval?: boolean
  } | null>(null)
  // A cancel response must not erase the need for a fresh read after an unknown
  // outcome. Keep that barrier even when a safe Cancel supersedes the phase.
  const unknownOutcome = useRef<{ label: string; afterSeq: number } | null>(null)
  const [phase, setPhase] = useState<Phase>({ type: 'idle' })
  const current = useRef<Phase>(phase)
  const transition = (next: Phase) => {
    current.current = next
    setPhase(next)
  }
  const clearTimer = () => {
    if (timer.current !== null) window.clearTimeout(timer.current)
    timer.current = null
  }
  useEffect(() => {
    mounted.current = true
    const onFocus = (event: FocusEvent) => {
      const focused = focusedOperation.current
      if (
        event.target !== focused?.element &&
        !(
          event.target === document.body &&
          focused?.focusedBeforeRemoval &&
          !focused.element.isConnected
        )
      ) {
        focusedOperation.current = null
      }
    }
    document.addEventListener('focusin', onFocus)
    return () => {
      mounted.current = false
      document.removeEventListener('focusin', onFocus)
      token.current += 1
      clearTimer()
    }
  }, [])
  useLayoutEffect(() => {
    const focused = focusedOperation.current
    // A same-slot Retry start can become Remove without unmounting. Recover
    // within the trigger's own row in either case, never onto a destructive action.
    const becameRemove =
      focused?.element.dataset.knowledgeTrigger?.startsWith('remove:') &&
      focused.element.dataset.knowledgeTrigger !== focused.trigger
    if (focused && (!focused.element.isConnected || becameRemove)) {
      const stillOwnsFocus = becameRemove
        ? document.activeElement === focused.element
        : focused.focusedBeforeRemoval && document.activeElement === document.body
      if (stillOwnsFocus && focused.container.isConnected) {
        const safeButton = focused.container.querySelector<HTMLButtonElement>(
          'button:not(:disabled):not([aria-disabled="true"]):not([data-knowledge-trigger^="remove:"])'
        )
        ;(safeButton ?? focused.container).focus()
      }
      focusedOperation.current = null
    }
    if (phase.type === 'idle') {
      lockTrigger.current = null
      focusedOperation.current = null
    }
  })
  useEffect(() => {
    if (phase.type !== 'reconciling' && phase.type !== 'unconfirmed') return
    if (snapshotSeq > phase.afterSeq) {
      unknownOutcome.current = null
      transition({ type: 'idle' })
    } else if (loadError && !loading && phase.type === 'reconciling') {
      transition({ ...phase, type: 'unconfirmed' })
    }
  }, [snapshotSeq, phase, loadError, loading])

  const [error, setError] = useState<string | null>(null)
  const status = knowledge?.semanticSearch
  const variants = status ? semanticVariants(status) : []
  const selectedVariant = knowledge?.settings.embeddingVariant ?? 'q8_0'
  const selectedModel = knowledge?.settings.embeddingModel
  const selected = !knowledge
    ? ''
    : selectedModel === 'built-in'
      ? 'built-in'
      : modelValue(selectedVariant)
  const selectedStatus = variants.find((variant) => variant.id === selectedVariant)
  const locked = phase.type !== 'idle'
  const pending = phase.type === 'pending' ? phase.op : null
  const speed = knowledge?.settings.indexingSpeed
  const ready = selectedStatus?.state === 'ready'
  const startupFailure = selectedStatus?.failure?.stage === 'startup'
  const semanticError =
    status?.error &&
    !variants.some(
      (variant) => variant.id !== selectedVariant && variant.failure?.message === status.error
    )
  // Only legacy payloads may imply the variant of the vector counts.
  const indexVariant = status?.indexVariant ?? (!status?.variants ? 'q8_0' : undefined)
  const countsMatch = indexVariant === selectedVariant
  const unfinished = !countsMatch || (status?.indexedChunks ?? 0) < (status?.totalChunks ?? 0)
  const installed = variants.find((variant) => variant.state === 'ready' && !variant.failure)
  const buttonClass =
    'rounded-tezbar-row border border-white/10 bg-white/5 px-3 py-2 text-xs text-ink-1 transition hover:bg-white/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cyan-300/50 disabled:cursor-not-allowed disabled:opacity-50 aria-disabled:cursor-wait aria-disabled:opacity-50'

  const run = (op: Omit<Pending, 'token'>, operation: () => Promise<KnowledgeSnapshot>) => {
    // Cancel can supersede an unknown operation. Repeated activation of its own
    // focusable pending button is still ignored, synchronously before rerender.
    if (!mounted.current || (current.current.type !== 'idle' && op.kind !== 'cancel')) return
    if (current.current.type === 'pending' && current.current.op.trigger === op.trigger) return
    if (current.current.type === 'pending' && current.current.op.kind !== 'cancel') {
      unknownOutcome.current = {
        label: current.current.op.label,
        afterSeq: snapshotSeqRef.current,
      }
    }
    clearTimer()
    lockTrigger.current = op.trigger
    const element = document.activeElement
    const container = element?.closest<HTMLElement>('[data-model-file], [data-knowledge-actions]')
    if (
      container &&
      element instanceof HTMLElement &&
      element.dataset.knowledgeTrigger === op.trigger
    ) {
      focusedOperation.current = { element, trigger: op.trigger, container }
    }
    const next = { ...op, token: ++token.current }
    transition({ type: 'pending', op: next })
    setError(null)
    const isCurrent = () =>
      mounted.current &&
      current.current.type === 'pending' &&
      current.current.op.token === next.token &&
      token.current === next.token
    timer.current = window.setTimeout(() => {
      if (!isCurrent()) return
      clearTimer()
      unknownOutcome.current = {
        label: unknownOutcome.current?.label ?? next.label,
        afterSeq: snapshotSeqRef.current,
      }
      transition({ type: 'reconciling', ...unknownOutcome.current })
      onRefresh()
    }, KNOWLEDGE_ACTION_TIMEOUT_MS)
    const settle = () => {
      const barrier = unknownOutcome.current
      if (barrier) {
        // Read after the superseding operation, rather than trusting its mutation
        // snapshot to establish the outcome of an older, still-running request.
        unknownOutcome.current = { ...barrier, afterSeq: snapshotSeqRef.current }
        transition({ type: 'reconciling', ...unknownOutcome.current })
        onRefresh()
      } else transition({ type: 'idle' })
    }
    void Promise.resolve()
      .then(() => (isCurrent() ? operation() : null))
      .then(
        (snapshot) => {
          if (!isCurrent() || !snapshot) return
          clearTimer()
          onChange(snapshot)
          settle()
        },
        (failure: unknown) => {
          if (!isCurrent()) return
          clearTimer()
          settle()
          const message = failure instanceof Error ? failure.message : String(failure)
          setError(
            op.kind === 'download'
              ? `${variantName(op.variant!)} download failed: ${message}`
              : `Couldn't ${op.label[0]!.toLowerCase()}${op.label.slice(1)}: ${message}`
          )
        }
      )
  }

  const switchModel = (variant: EmbeddingVariantId, trigger: string) =>
    run(
      {
        kind: 'switch',
        variant,
        target: modelValue(variant),
        trigger,
        label: `Switch to ${variantName(variant)}`,
      },
      () => window.tezbar.setKnowledgeEmbeddingModel('embeddinggemma-2', variant)
    )

  const actionButton = (
    trigger: string,
    text: string,
    action: () => void,
    options: {
      name?: string
      safe?: boolean
      pendingText?: string
      disabled?: boolean
    } = {}
  ) => {
    const ownPending = pending?.trigger === trigger
    const ownLock =
      locked &&
      (lockTrigger.current === trigger ||
        focusedOperation.current?.element.dataset.knowledgeTrigger === trigger)
    return (
      <button
        ref={(element) => {
          const focused = focusedOperation.current
          if (!element && focused?.trigger === trigger) {
            focused.focusedBeforeRemoval = document.activeElement === focused.element
          }
        }}
        type="button"
        className={buttonClass}
        data-knowledge-trigger={trigger}
        aria-label={
          ownPending && pending
            ? pending.kind === 'switch'
              ? `Starting ${variantName(pending.variant!)}…`
              : pendingLabel(pending)
            : options.name
        }
        aria-disabled={ownLock || undefined}
        disabled={options.disabled || (!ownLock && locked && !options.safe)}
        onClick={() => {
          if (!ownPending && (!locked || options.safe)) action()
        }}
      >
        {ownPending ? (options.pendingText ?? 'Starting…') : text}
      </button>
    )
  }

  let semanticLine = ''
  let semanticCounts = ''
  let keywordLine = ''
  if (status && selectedModel === 'embeddinggemma-2' && selectedStatus) {
    const name = variantName(selectedVariant)
    const counts = `${status.indexedChunks.toLocaleString('en-US')} / ${status.totalChunks.toLocaleString('en-US')} chunks`
    if (selectedStatus.state === 'verifying') {
      semanticLine = `Checking ${name} installation…`
      keywordLine = 'Keyword search is in use.'
    } else if (
      !ready ||
      (selectedStatus.failure && !startupFailure) ||
      (semanticError && !startupFailure)
    ) {
      semanticLine = `Semantic search is off: ${name} ${selectedStatus.failure || selectedStatus.state === 'error' || semanticError ? 'needs repair.' : "isn't installed."}`
      keywordLine = 'Keyword search is in use.'
    } else if (startupFailure) {
      semanticLine = `Semantic search is off: ${name} couldn't start.`
      keywordLine = 'Keyword search is in use.'
    } else if (!countsMatch) {
      semanticLine = `Preparing semantic index for ${name}…`
      keywordLine = "Keyword search covers chunks that aren't rebuilt yet."
    } else if (knowledge?.status.state === 'paused') {
      semanticLine = `Semantic indexing paused · ${name}`
      semanticCounts = counts
      keywordLine = 'Keyword search stays available. Resume indexing to continue.'
    } else if (status.indexing || unfinished) {
      semanticLine = `${status.indexing ? 'Building' : 'Preparing'} semantic index · ${name}`
      semanticCounts = counts
      keywordLine = "Keyword search covers chunks that aren't rebuilt yet."
    } else {
      semanticLine = `Semantic index · ${name}`
      semanticCounts = counts
    }
  }
  const errors = error ? [error] : []
  for (const variant of variants) {
    if (!variant.failure) continue
    const name = variantName(variant.id)
    const { stage, message } = variant.failure
    errors.push(
      stage === 'integrity'
        ? `${name} didn't pass the file check. Repair downloads it again.`
        : stage === 'startup'
          ? `${name} is installed but couldn't start: ${message}`
          : `${name} download failed: ${message}`
    )
  }
  // A new backend can mirror a variant failure into the legacy error field.
  // Render only unattributed errors verbatim, especially after an integrity check.
  if (
    status?.error &&
    status.errorSource !== 'integrity' &&
    !variants.some((variant) => variant.failure?.message === status.error)
  ) {
    errors.push(status.error)
  }

  return (
    <div className="space-y-3">
      <label className="block text-xs font-medium text-ink-2" htmlFor="knowledge-embedding-model">
        Search engine
      </label>
      <select
        id="knowledge-embedding-model"
        value={pending?.kind === 'switch' ? pending.target : selected}
        data-knowledge-trigger="select"
        aria-disabled={(locked && lockTrigger.current === 'select') || undefined}
        disabled={!knowledge || (locked && lockTrigger.current !== 'select')}
        onKeyDown={(event) => {
          if (
            current.current.type !== 'idle' &&
            ['ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown', ' ', 'Enter'].includes(
              event.key
            )
          ) {
            event.preventDefault()
          }
        }}
        onPointerDown={(event) => {
          if (current.current.type !== 'idle') event.preventDefault()
        }}
        onChange={(event) => {
          if (current.current.type !== 'idle') {
            event.currentTarget.value = pending?.kind === 'switch' ? pending.target! : selected
            return
          }
          const value = event.target.value
          if (value === 'built-in')
            run(
              {
                kind: 'switch',
                target: value,
                trigger: 'select',
                label: 'Switch to Built-in lightweight search',
              },
              () => window.tezbar.setKnowledgeEmbeddingModel('built-in')
            )
          else switchModel(value === modelValue('q4_k_m') ? 'q4_k_m' : 'q8_0', 'select')
        }}
        className="glass-field w-full px-3 py-2 text-xs text-ink-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cyan-300/50 disabled:opacity-50 aria-disabled:cursor-wait aria-disabled:opacity-50"
      >
        {!knowledge ? (
          <option value="">
            {loadError && !loading ? 'Search engine unavailable' : 'Loading search engine…'}
          </option>
        ) : null}
        <option value="built-in">Built-in lightweight search</option>
        {variantIds.map((id) => {
          const variant = variants.find((item) => item.id === id)
          return (
            <option
              key={id}
              value={modelValue(id)}
              disabled={!status?.supported || variant?.state !== 'ready'}
            >
              EmbeddingGemma 2 · {variantName(id)}
              {variant ? optionSuffix(variant) : ' — download required'}
            </option>
          )
        })}
      </select>
      <p className="text-xs leading-relaxed text-ink-3">
        EmbeddingGemma 2 finds related meanings across languages in documents and code. It runs
        locally and works offline after download. Standard (Q8_0) is the default; Compact (Q4_K_M)
        uses a smaller download. Switching versions rebuilds semantic search only — extracted text,
        OCR and keyword search are kept. Smart and Deep folders use semantic search; Basic folders
        use keyword search.
      </p>
      <div className="space-y-1.5 pt-1">
        {variants.length ? <p className="text-xs font-medium text-ink-2">Model files</p> : null}
        {variants.map((variant) => {
          const { id, state, failure } = variant
          const name = variantName(id)
          const inUse = selectedModel === 'embeddinggemma-2' && selectedVariant === id
          const startup = failure?.stage === 'startup'
          const fileState = !status?.supported
            ? 'Not available on this platform'
            : state === 'not-downloaded'
              ? 'Not downloaded'
              : state === 'downloading'
                ? `Downloading · ${downloadPercent(variant.downloadProgress)}%`
                : state === 'verifying'
                  ? 'Checking file…'
                  : state === 'error'
                    ? failure?.stage === 'integrity'
                      ? 'File check failed'
                      : 'Download failed'
                    : startup
                      ? "Installed · couldn't start"
                      : inUse
                        ? 'Installed · in use'
                        : 'Installed'
          const downloadText =
            state === 'error'
              ? failure?.stage === 'integrity'
                ? 'Repair'
                : 'Retry download'
              : 'Download'
          const downloadFile = () =>
            run(
              {
                kind: 'download',
                variant: id,
                trigger: `download:${id}`,
                label: `Download ${name}`,
              },
              () => window.tezbar.downloadKnowledgeEmbeddingModel(id)
            )
          const removeFile = () =>
            run(
              { kind: 'remove', variant: id, trigger: `remove:${id}`, label: `Remove ${name}` },
              () => window.tezbar.removeKnowledgeEmbeddingModel(id)
            )
          return (
            <div
              key={id}
              data-model-file={id}
              tabIndex={-1}
              role="group"
              aria-label={`${name} model file`}
              className="flex flex-wrap items-center justify-between gap-2"
            >
              <div>
                <p className="text-xs text-ink-2">
                  {name}
                  <span className="text-ink-4"> · {megabytes(variant.weightBytes)} MB</span>
                </p>
                <p className="text-xs text-ink-3">{fileState}</p>
              </div>
              {status?.supported ? (
                <div className="flex items-center gap-2">
                  {state === 'verifying'
                    ? null
                    : state === 'downloading'
                      ? actionButton(
                          `cancel:${id}`,
                          'Cancel',
                          () =>
                            run(
                              {
                                kind: 'cancel',
                                variant: id,
                                trigger: `cancel:${id}`,
                                label: `Cancel ${name} download`,
                              },
                              () => window.tezbar.cancelKnowledgeEmbeddingDownload(id)
                            ),
                          {
                            name: `Cancel ${name} download`,
                            safe: true,
                            pendingText: 'Cancelling…',
                          }
                        )
                      : state === 'ready' && !startup
                        ? actionButton(`remove:${id}`, 'Remove', removeFile, {
                            name: `Remove ${name}`,
                            pendingText: 'Removing…',
                          })
                        : startup
                          ? actionButton(
                              `start:${id}`,
                              'Retry start',
                              () => switchModel(id, `start:${id}`),
                              { name: `Retry start ${name}` }
                            )
                          : actionButton(`download:${id}`, downloadText, downloadFile, {
                              name: `${downloadText} ${name}`,
                            })}
                  {startup
                    ? actionButton(`download:${id}`, 'Repair', downloadFile, {
                        name: `Repair ${name}`,
                      })
                    : null}
                  {state === 'error' &&
                  (failure?.stage === 'integrity' || variant.downloadBytes > 0)
                    ? actionButton(`remove:${id}`, 'Remove', removeFile, {
                        name: `Remove ${name}`,
                        pendingText: 'Removing…',
                      })
                    : null}
                </div>
              ) : null}
            </div>
          )
        })}
        {variants
          .filter((variant) => variant.state === 'downloading' || variant.state === 'verifying')
          .map((variant) => (
            <div key={variant.id} className="space-y-1 text-xs text-ink-3">
              <progress
                aria-label={`${variantName(variant.id)} download`}
                value={variant.state === 'verifying' ? undefined : variant.downloadProgress}
                max={1}
                className="h-2 w-full accent-cyan-300"
              />
              <p>
                {variant.state === 'verifying'
                  ? `Checking ${variantName(variant.id)} file…`
                  : `Downloading ${variantName(variant.id)} · ${downloadPercent(variant.downloadProgress)}%${variant.downloadTotalBytes > 0 ? ` · ${megabytes(variant.downloadBytes)} / ${megabytes(variant.downloadTotalBytes)} MB` : ''}`}
              </p>
            </div>
          ))}
      </div>
      <div className="flex flex-col gap-1.5 pt-1 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <label
            className="block text-xs font-medium text-ink-2"
            htmlFor="knowledge-speed-selector"
          >
            Chunking speed
          </label>
          <p className="text-[11px] text-ink-4">
            {!knowledge
              ? loadError && !loading
                ? 'Chunking speed unavailable'
                : 'Loading chunking speed…'
              : speed === 'slow'
                ? 'Slow & light: minimal CPU & battery use, quiet pauses'
                : speed === 'fast'
                  ? 'Fast & heavy: maximum throughput, higher CPU usage'
                  : 'Medium: balanced throughput and system load'}
          </p>
        </div>
        <div
          id="knowledge-speed-selector"
          role="radiogroup"
          aria-label="Chunking speed"
          className="inline-flex shrink-0 self-start rounded-lg border border-white/10 bg-white/[0.03] p-0.5 text-xs sm:self-auto"
        >
          {(
            [
              { id: 'slow', label: 'Slow (light)' },
              { id: 'medium', label: 'Medium' },
              { id: 'fast', label: 'Fast (heavy)' },
            ] as const
          ).map(({ id, label }) => {
            const ownLock = locked && lockTrigger.current === `speed:${id}`
            return (
              <button
                key={id}
                type="button"
                role="radio"
                aria-checked={speed === id}
                aria-disabled={ownLock || undefined}
                disabled={!knowledge || (locked && !ownLock)}
                onClick={() =>
                  run(
                    {
                      kind: 'speed',
                      target: id,
                      trigger: `speed:${id}`,
                      label: 'Change chunking speed',
                    },
                    () => window.tezbar.setKnowledgeIndexingSpeed(id)
                  )
                }
                className={cx(
                  'rounded-md px-2.5 py-1 text-[11px] font-medium transition focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-cyan-300/50 aria-disabled:cursor-wait aria-disabled:opacity-50',
                  speed === id
                    ? 'bg-cyan-400/20 text-cyan-200 shadow-sm'
                    : 'text-ink-3 hover:text-ink-1'
                )}
              >
                {label}
              </button>
            )
          })}
        </div>
      </div>
      <div data-knowledge-actions tabIndex={-1} className="flex flex-wrap items-center gap-2">
        <button type="button" className={buttonClass} onClick={onRefresh}>
          Refresh status
        </button>
        {status?.supported && installed && selectedModel === 'built-in'
          ? actionButton(
              'enable',
              'Enable semantic search',
              () => switchModel(installed.id, 'enable'),
              {
                name: `Enable semantic search with ${variantName(installed.id)}`,
                pendingText: 'Starting…',
              }
            )
          : null}
        {status?.supported &&
        ready &&
        !startupFailure &&
        selectedModel === 'embeddinggemma-2' &&
        semanticError
          ? actionButton('retry-index', 'Retry semantic indexing', () =>
              switchModel(selectedVariant, 'retry-index')
            )
          : null}
        {status?.supported &&
        ready &&
        !startupFailure &&
        selectedModel === 'embeddinggemma-2' &&
        unfinished &&
        !status.indexing &&
        !semanticError &&
        knowledge?.status.state !== 'paused'
          ? actionButton('start-index', 'Start semantic indexing', () =>
              switchModel(selectedVariant, 'start-index')
            )
          : null}
      </div>
      {semanticLine ? (
        <div className="text-xs text-ink-3">
          <span role="status">{semanticLine}</span>
          {semanticCounts ? ` · ${semanticCounts}` : null}
          {keywordLine ? <p className="mt-1 text-ink-4">{keywordLine}</p> : null}
        </div>
      ) : null}
      {phase.type === 'pending' ? (
        <p role="status" className="text-xs text-ink-3">
          {pendingLabel(phase.op)}
        </p>
      ) : phase.type === 'reconciling' || (phase.type === 'unconfirmed' && loading) ? (
        <p
          role="status"
          className="text-xs text-ink-3"
        >{`Tezbar didn't confirm "${phase.label}" in time. Checking the current state…`}</p>
      ) : phase.type === 'unconfirmed' ? (
        <p role="status" className="text-xs text-ink-3">
          Couldn't check the current state. Use Refresh status to try again.
        </p>
      ) : null}
      {status && !status.supported ? (
        <p className="text-xs text-ink-3">
          The local model runtime is not available for this platform yet.
        </p>
      ) : null}
      {errors.length ? (
        <div role="alert" className="text-xs text-red-300">
          {[...new Set(errors)].map((message, i) => (
            <p key={i}>{message}</p>
          ))}
        </div>
      ) : null}
    </div>
  )
}
