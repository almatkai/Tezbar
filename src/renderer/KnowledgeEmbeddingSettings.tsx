import { useState } from 'react'
import type { KnowledgeSnapshot } from '../shared/knowledge'

type Props = {
  knowledge: KnowledgeSnapshot | null
  onChange: (snapshot: KnowledgeSnapshot) => void
}

export function KnowledgeEmbeddingSettings({ knowledge, onChange }: Props) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const status = knowledge?.semanticSearch
  const selected = knowledge?.settings.embeddingModel ?? 'built-in'
  const ready = status?.modelState === 'ready'
  const downloading = status?.modelState === 'downloading'
  const unfinished = (status?.indexedChunks ?? 0) < (status?.totalChunks ?? 0)
  const buttonClass =
    'rounded-tezbar-row border border-white/10 bg-white/5 px-3 py-2 text-xs text-ink-1 transition hover:bg-white/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cyan-300/50 disabled:cursor-not-allowed disabled:opacity-50'

  const run = (operation: () => Promise<KnowledgeSnapshot>) => {
    setBusy(true)
    setError(null)
    void operation()
      .then(onChange)
      .catch((failure: unknown) => {
        setError(failure instanceof Error ? failure.message : String(failure))
      })
      .finally(() => setBusy(false))
  }

  return (
    <div className="space-y-3">
      <label className="block text-xs font-medium text-ink-2" htmlFor="knowledge-embedding-model">
        Search engine
      </label>
      <select
        id="knowledge-embedding-model"
        value={selected}
        disabled={busy || !knowledge || downloading}
        onChange={(event) => {
          const model = event.target.value as 'built-in' | 'embeddinggemma-2'
          run(() => window.tezbar.setKnowledgeEmbeddingModel(model))
        }}
        className="glass-field w-full px-3 py-2 text-xs text-ink-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cyan-300/50 disabled:opacity-50"
      >
        <option value="built-in">Built-in lightweight search</option>
        <option value="embeddinggemma-2" disabled={!ready}>
          EmbeddingGemma 2{ready ? '' : ' — download required'}
        </option>
      </select>
      <p className="text-xs leading-relaxed text-ink-3">
        EmbeddingGemma 2 finds related meanings across languages in documents and code. It runs
        locally and works offline after download. Smart and Deep folders use semantic search; Basic
        folders use keyword search.
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          className={buttonClass}
          disabled={busy || !knowledge}
          onClick={() => run(() => window.tezbar.getKnowledgeSnapshot())}
        >
          Refresh status
        </button>
        {!ready && !downloading ? (
          <button
            type="button"
            className={buttonClass}
            disabled={busy || !status?.supported}
            onClick={() => run(() => window.tezbar.downloadKnowledgeEmbeddingModel())}
          >
            {busy
              ? 'Starting…'
              : status?.modelState === 'error'
                ? 'Retry download'
                : 'Download EmbeddingGemma 2'}
          </button>
        ) : null}
        {downloading ? (
          <button
            type="button"
            className={buttonClass}
            disabled={busy}
            onClick={() => run(() => window.tezbar.cancelKnowledgeEmbeddingDownload())}
          >
            Cancel download
          </button>
        ) : null}
        {ready && selected === 'built-in' ? (
          <button
            type="button"
            className={buttonClass}
            disabled={busy}
            onClick={() => run(() => window.tezbar.setKnowledgeEmbeddingModel('embeddinggemma-2'))}
          >
            Enable semantic search
          </button>
        ) : null}
        {ready && selected === 'embeddinggemma-2' && status?.error ? (
          <button
            type="button"
            className={buttonClass}
            disabled={busy}
            onClick={() => run(() => window.tezbar.setKnowledgeEmbeddingModel('embeddinggemma-2'))}
          >
            Retry semantic indexing
          </button>
        ) : null}
        {ready &&
        selected === 'embeddinggemma-2' &&
        unfinished &&
        !status?.indexing &&
        !status?.error &&
        knowledge?.status.state !== 'paused' ? (
          <button
            type="button"
            className={buttonClass}
            disabled={busy}
            onClick={() => run(() => window.tezbar.setKnowledgeEmbeddingModel('embeddinggemma-2'))}
          >
            Start semantic indexing
          </button>
        ) : null}
        {ready || status?.modelState === 'error' ? (
          <button
            type="button"
            className={buttonClass}
            disabled={busy}
            onClick={() => run(() => window.tezbar.removeKnowledgeEmbeddingModel())}
          >
            Remove model
          </button>
        ) : null}
        <span className="text-xs text-ink-4">
          {status
            ? `${Math.ceil(status.downloadTotalBytes / 1_000_000)} MB download · text and code`
            : 'Text and code'}
        </span>
      </div>
      {downloading ? (
        <div role="status" className="space-y-1 text-xs text-ink-3">
          <progress
            aria-label="EmbeddingGemma download"
            value={status.downloadProgress}
            max={1}
            className="h-2 w-full accent-cyan-300"
          />
          <p>Downloading and verifying · {Math.round(status.downloadProgress * 100)}%</p>
        </div>
      ) : null}
      {ready && selected === 'embeddinggemma-2' ? (
        <div role="status" className="text-xs text-ink-3">
          {knowledge?.status.state === 'paused'
            ? 'Semantic indexing paused'
            : status?.indexing
              ? 'Building semantic index'
              : unfinished
                ? 'Preparing semantic index'
                : 'Semantic index'}{' '}
          · {status?.indexedChunks ?? 0} / {status?.totalChunks ?? 0} chunks
          <p className="mt-1 text-ink-4">
            Keyword search stays available while indexing. Pause indexing to pause semantic indexing
            too.
          </p>
        </div>
      ) : null}
      {status && !status.supported ? (
        <p className="text-xs text-ink-3">
          The local model runtime is not available for this platform yet.
        </p>
      ) : null}
      {error || status?.error ? (
        <p role="alert" className="text-xs text-red-300">
          {error ?? status?.error}
        </p>
      ) : null}
    </div>
  )
}
