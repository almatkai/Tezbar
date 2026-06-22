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

function ProviderMark({ label }: { label: string }) {
  return (
    <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-md border border-white/10 bg-white/[0.045] text-[9px] font-semibold text-ink-2">
      {label.slice(0, 1).toUpperCase()}
    </span>
  )
}

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
  const searchRef = useRef<HTMLInputElement | null>(null)

  useEffect(() => {
    if (!open) return
    const closeIfOutside = (event: PointerEvent): void => {
      if (!pickerRef.current?.contains(event.target as Node)) onOpenChange(false)
    }
    const closeOnEscape = (event: KeyboardEvent): void => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      event.stopPropagation()
      onOpenChange(false)
    }
    document.addEventListener('pointerdown', closeIfOutside)
    window.addEventListener('keydown', closeOnEscape, true)
    requestAnimationFrame(() => searchRef.current?.focus())
    return () => {
      document.removeEventListener('pointerdown', closeIfOutside)
      window.removeEventListener('keydown', closeOnEscape, true)
    }
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
    return previewModels.filter((model) =>
      `${model.id} ${model.capabilities.join(' ')}`.toLowerCase().includes(query)
    )
  }, [modelSearch, previewModels])

  const configure = (): void => {
    onOpenChange(false)
    onConfigure()
  }

  return (
    <div ref={pickerRef} className="relative flex h-7 shrink-0 items-center">
      <button
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        className={cx(
          'group inline-flex h-7 max-w-[260px] items-center gap-1.5 rounded-lg border border-white/[0.08] bg-white/[0.035] px-2 text-[11px] text-ink-2 transition hover:border-white/[0.16] hover:bg-white/[0.06] hover:text-ink-1',
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
        <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-emerald-400/80" />
        <span className="truncate font-medium">{activeModel}</span>
        <svg
          viewBox="0 0 16 16"
          aria-hidden="true"
          className={cx('h-3 w-3 shrink-0 text-ink-4 transition', open && 'rotate-180')}
        >
          <path d="m4 6 4 4 4-4" fill="none" stroke="currentColor" strokeWidth="1.5" />
        </svg>
      </button>

      {open ? (
        <section
          aria-label="Choose model"
          className="tezbar-popover absolute right-0 top-9 z-50 flex max-h-[430px] w-[min(410px,calc(100vw-48px))] flex-col overflow-hidden p-1.5"
        >
          <div className="flex items-center gap-2 border-b border-white/[0.07] p-1.5 pb-2">
            <svg viewBox="0 0 16 16" aria-hidden="true" className="h-3.5 w-3.5 text-ink-4">
              <circle cx="7" cy="7" r="4" fill="none" stroke="currentColor" strokeWidth="1.4" />
              <path d="m10 10 3 3" stroke="currentColor" strokeWidth="1.4" />
            </svg>
            <input
              ref={searchRef}
              type="search"
              value={modelSearch}
              onChange={(event) => setModelSearch(event.target.value)}
              placeholder="Search models"
              className="h-7 min-w-0 flex-1 bg-transparent text-[12px] text-ink-1 outline-none placeholder:text-ink-4"
            />
            <span className="rounded-md border border-white/[0.08] px-1.5 py-0.5 text-[9px] text-ink-4">
              esc
            </span>
          </div>

          <div className="flex shrink-0 gap-1 overflow-x-auto px-1 py-2">
            {availableProviders.map((provider) => {
              const configured = isAiProviderConfigured(config, provider.id)
              const selected = provider.id === previewProvider
              return (
                <button
                  key={provider.id}
                  type="button"
                  className={cx(
                    'inline-flex h-7 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[10.5px] transition',
                    selected
                      ? 'bg-white/[0.09] text-ink-1'
                      : configured
                        ? 'text-ink-3 hover:bg-white/[0.05] hover:text-ink-1'
                        : 'text-ink-4 hover:bg-white/[0.05] hover:text-ink-2'
                  )}
                  onClick={() => {
                    setPreviewProvider(provider.id)
                    setModelSearch('')
                  }}
                >
                  <ProviderMark label={provider.title} />
                  {provider.title}
                </button>
              )
            })}
          </div>

          <div className="flex items-center justify-between px-2 pb-1.5">
            <div>
              <p className="text-[11.5px] font-medium text-ink-1">
                {providerTitle(previewProvider, config)}
              </p>
              <p className="text-[9.5px] text-ink-4">
                {previewConfigured ? `${previewModels.length} models available` : 'Setup required'}
              </p>
            </div>
            <button
              type="button"
              className="text-[10.5px] text-ink-3 transition hover:text-ink-1"
              onClick={configure}
            >
              Configure
            </button>
          </div>

          <ul className="min-h-0 flex-1 overflow-y-auto px-0.5 pb-1">
            {filteredPreviewModels.map((model) => {
              const selected = previewProvider === activeProvider && model.id === activeModel
              return (
                <li key={model.id}>
                  <button
                    type="button"
                    aria-current={selected ? 'true' : undefined}
                    disabled={!previewConfigured}
                    className={cx(
                      'flex w-full items-center gap-2 rounded-lg px-2 py-2 text-left transition disabled:cursor-not-allowed disabled:opacity-40',
                      selected
                        ? 'bg-white/[0.075] text-ink-1'
                        : 'text-ink-2 hover:bg-white/[0.045] hover:text-ink-1'
                    )}
                    onClick={() => {
                      onOpenChange(false)
                      void onSelect(previewProvider, model.id)
                    }}
                  >
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[12px] font-medium">{model.id}</span>
                      <span className="mt-0.5 block truncate text-[9.5px] text-ink-4">
                        {[
                          ...model.capabilities,
                          model.contextWindow
                            ? `${Math.round(model.contextWindow / 1000)}k context`
                            : null,
                        ]
                          .filter(Boolean)
                          .join(' · ')}
                      </span>
                    </span>
                    {selected ? (
                      <svg
                        viewBox="0 0 16 16"
                        aria-hidden="true"
                        className="h-4 w-4 text-emerald-300"
                      >
                        <path
                          d="m3.5 8.2 2.7 2.7 6.3-6.3"
                          fill="none"
                          stroke="currentColor"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                          strokeWidth="1.7"
                        />
                      </svg>
                    ) : null}
                  </button>
                </li>
              )
            })}
            {filteredPreviewModels.length === 0 ? (
              <li className="px-3 py-8 text-center text-[11px] text-ink-4">No matching models</li>
            ) : null}
          </ul>

          <button
            type="button"
            className="mt-1 flex items-center justify-between border-t border-white/[0.07] px-2 py-2 text-left text-[10.5px] text-ink-3 transition hover:text-ink-1"
            onClick={configure}
          >
            <span>Manage providers and API keys</span>
            <span aria-hidden>↗</span>
          </button>
        </section>
      ) : null}
    </div>
  )
}
