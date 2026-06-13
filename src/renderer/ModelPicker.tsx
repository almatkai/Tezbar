import { useEffect, useMemo, useRef, useState } from 'react'
import {
  defaultModels,
  isAiProviderConfigured,
  normalizeProviderModelList,
  providerRows,
  providerTitle,
  recommendedModel,
} from '../shared/aiProviders'
import type { LlmConfigRecord, ProviderId } from '../shared/llmConfig'
import { cx } from './ui/primitives'

export function ModelPicker({
  config,
  open,
  onOpenChange,
  onSelect,
  onConfigure,
  onBeforeOpen,
  triggerClassName,
}: {
  config: LlmConfigRecord
  open: boolean
  onOpenChange: (open: boolean) => void
  onSelect: (provider: ProviderId, modelId: string) => void | Promise<void>
  onConfigure: () => void
  onBeforeOpen?: () => void
  triggerClassName?: string
}): JSX.Element {
  const activeProvider = (config.provider ?? 'ollama') as ProviderId
  const activeModel =
    config.providerSelectedModels?.[activeProvider] ??
    config.model ??
    recommendedModel(activeProvider)
  const [previewProvider, setPreviewProvider] = useState<ProviderId>(activeProvider)
  const [modelSearch, setModelSearch] = useState('')
  const pickerRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (!open) return
    const closeIfOutside = (event: PointerEvent): void => {
      if (!pickerRef.current?.contains(event.target as Node)) {
        onOpenChange(false)
      }
    }
    document.addEventListener('pointerdown', closeIfOutside)
    return () => document.removeEventListener('pointerdown', closeIfOutside)
  }, [onOpenChange, open])

  const availableProviders = providerRows(config)
  const previewModels = useMemo(
    () =>
      normalizeProviderModelList(
        previewProvider,
        config.providerModels?.[previewProvider] ?? defaultModels(previewProvider)
      ),
    [config.providerModels, previewProvider]
  )
  const previewConfigured = isAiProviderConfigured(config, previewProvider)
  const filteredPreviewModels = useMemo(() => {
    const query = modelSearch.trim().toLowerCase()
    if (!query) return previewModels
    const providerName = providerTitle(previewProvider, config)
    return previewModels.filter((model) =>
      `${model.id} ${providerName} ${model.capabilities.join(' ')}`
        .toLowerCase()
        .includes(query)
    )
  }, [config, modelSearch, previewModels, previewProvider])

  const configure = (): void => {
    onOpenChange(false)
    onConfigure()
  }

  return (
    <div
      ref={pickerRef}
      className="relative flex h-6 shrink-0 items-center"
      onMouseLeave={() => setPreviewProvider(activeProvider)}
    >
      <button
        type="button"
        className={cx(
          'inline-flex h-6 items-center rounded-raymes-chip border border-white/10 bg-white/[0.03] px-2 text-[10px] font-medium uppercase tracking-[0.12em] text-ink-3 transition hover:border-accent/40 hover:text-ink-1',
          triggerClassName
        )}
        onClick={() => {
          const nextOpen = !open
          if (nextOpen) {
            onBeforeOpen?.()
            setPreviewProvider(activeProvider)
            setModelSearch('')
          }
          onOpenChange(nextOpen)
        }}
      >
        {activeModel}
      </button>
      {open ? (
        <div className="raymes-popover absolute right-0 top-7 z-50 grid h-[390px] w-[560px] grid-cols-[190px_minmax(0,1fr)] overflow-hidden p-1.5">
          <div className="flex min-h-0 flex-col border-r border-white/[0.07] pr-1">
            <ul className="min-h-0 flex-1 overflow-y-auto">
              {availableProviders.map((provider) => {
                const configured = isAiProviderConfigured(config, provider.id)
                const active = provider.id === activeProvider
                const previewed = provider.id === previewProvider
                const selectedModel =
                  config.providerSelectedModels?.[provider.id] ??
                  (active ? activeModel : recommendedModel(provider.id))
                return (
                  <li key={provider.id}>
                    <button
                      type="button"
                      onMouseEnter={() => setPreviewProvider(provider.id)}
                      onFocus={() => setPreviewProvider(provider.id)}
                      className={cx(
                        'flex w-full items-center gap-2 rounded-raymes-row px-2 py-2 text-left transition',
                        previewed
                          ? 'bg-white/[0.06] text-ink-1'
                          : 'text-ink-3 hover:bg-white/[0.04] hover:text-ink-1'
                      )}
                    >
                      <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-raymes-chip border border-white/10 bg-white/[0.04] text-[11px] font-bold uppercase text-ink-2">
                        {provider.title.slice(0, 1)}
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-[12px] font-semibold">
                          {provider.title}
                        </span>
                        <span className="mt-0.5 block truncate text-[10px] text-ink-4">
                          {configured ? selectedModel : 'Not configured'}
                        </span>
                      </span>
                      {active ? (
                        <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-accent" aria-hidden />
                      ) : null}
                    </button>
                  </li>
                )
              })}
            </ul>
            <button
              type="button"
              className="mt-1 flex w-full items-center gap-2 rounded-raymes-row border border-white/10 bg-white/[0.03] px-2 py-2 text-left text-[12px] font-semibold text-ink-2 transition hover:border-accent/40 hover:bg-accent/10 hover:text-ink-1"
              onClick={configure}
            >
              <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-raymes-chip border border-white/10 bg-white/[0.05] text-[16px] leading-none">
                +
              </span>
              Add provider
            </button>
          </div>
          <div className="flex min-w-0 flex-col pl-1.5">
            <div className="flex shrink-0 items-center gap-2 px-2 pb-1 pt-1">
              <div className="min-w-0 flex-1">
                <p className="truncate text-[12px] font-semibold text-ink-1">
                  {providerTitle(previewProvider, config)}
                </p>
                <p className="text-[10px] uppercase tracking-[0.14em] text-ink-4">Models</p>
              </div>
              <input
                type="search"
                value={modelSearch}
                onChange={(event) => setModelSearch(event.target.value)}
                placeholder="Search models..."
                className="h-8 w-[170px] rounded-raymes-field border border-white/10 bg-white/[0.04] px-2 text-[12px] text-ink-1 outline-none placeholder:text-ink-4 focus:border-accent/50"
              />
              <button
                type="button"
                className="h-8 rounded-raymes-chip border border-accent/30 bg-accent/10 px-2.5 text-[11px] font-semibold text-accent-strong transition hover:border-accent/55 hover:bg-accent/18 hover:text-ink-1"
                onClick={configure}
              >
                Configure
              </button>
            </div>
            {!previewConfigured ? (
              <p className="mx-2 mb-1.5 rounded-raymes-row border border-white/[0.07] bg-white/[0.03] px-2 py-1.5 text-[11px] text-ink-4">
                Configure this provider to select a model.
              </p>
            ) : null}
            <ul className="min-h-0 flex-1 overflow-y-auto pr-0.5">
              {filteredPreviewModels.map((model, index) => {
                const selected = previewProvider === activeProvider && model.id === activeModel
                return (
                  <li key={model.id}>
                    <button
                      type="button"
                      disabled={!previewConfigured}
                      className={cx(
                        'w-full rounded-raymes-row px-2 py-2 text-left transition disabled:cursor-not-allowed disabled:opacity-40',
                        selected
                          ? 'bg-accent/12 text-ink-1'
                          : 'text-ink-2 hover:bg-white/[0.04] hover:text-ink-1'
                      )}
                      onClick={() => {
                        onOpenChange(false)
                        void onSelect(previewProvider, model.id)
                      }}
                    >
                      <span className="flex items-start justify-between gap-3">
                        <span className="min-w-0">
                          <span className="block truncate text-[12px] font-semibold">
                            {model.id}
                          </span>
                          <span className="mt-0.5 block truncate text-[10.5px] text-ink-4">
                            {providerTitle(previewProvider, config)}
                          </span>
                        </span>
                        {index < 9 ? (
                          <span className="rounded-raymes-chip bg-white/[0.06] px-1.5 py-0.5 font-mono text-[9px] text-ink-4">
                            #{index + 1}
                          </span>
                        ) : null}
                      </span>
                      <span className="mt-1 flex flex-wrap gap-1">
                        {model.capabilities.map((capability) => (
                          <span
                            key={capability}
                            className="rounded-raymes-chip border border-white/10 bg-white/[0.04] px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-[0.1em] text-ink-3"
                          >
                            {capability}
                          </span>
                        ))}
                        {model.contextWindow ? (
                          <span className="rounded-raymes-chip border border-white/10 bg-white/[0.04] px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-[0.1em] text-ink-3">
                            {model.contextWindow.toLocaleString()} ctx
                          </span>
                        ) : null}
                      </span>
                    </button>
                  </li>
                )
              })}
              {filteredPreviewModels.length === 0 ? (
                <li className="px-2 py-6 text-center text-[12px] text-ink-4">
                  No models match this search.
                </li>
              ) : null}
            </ul>
            <div className="mt-1 shrink-0 border-t border-white/[0.07] px-2 pt-1.5">
              <button
                type="button"
                className="text-[10.5px] font-medium text-accent-strong transition hover:text-accent"
                onClick={configure}
              >
                Configure in Settings
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  )
}
