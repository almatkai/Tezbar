import { useCallback, useEffect, useRef, useState } from 'react'
import type { LlmConfigRecord } from '../shared/llmConfig'
import type { VoiceModel, VoiceModelId } from '../shared/voice'
import {
  Button,
  FieldLabel,
  Hint,
  HintBar,
  Kbd,
  Message,
  Section,
  TextField,
  ViewHeader,
} from './ui/primitives'
import { CurrencySettings } from './CurrencySettings'

const FALLBACK_VOICE_MODELS: VoiceModel[] = [
  {
    id: 'moonshine-base-en',
    name: 'Moonshine Base (English)',
    family: 'moonshine',
    description: 'Low-latency Moonshine STT model from Moonshine AI.',
    homepageUrl: 'https://github.com/moonshine-ai/moonshine',
    estimatedSizeMb: 140,
    status: 'not-downloaded',
    stage: 'idle',
    progress: 0,
    downloadedBytes: 0,
    totalBytes: null,
    selected: true,
    runtime: {
      label: 'Moonshine (Python)',
      ready: false,
      installCommand: 'pip install moonshine-voice',
    },
  },
  {
    id: 'whisper-base',
    name: 'Whisper Base (English, whisper.cpp)',
    family: 'whisper',
    description: 'Fast whisper.cpp ggml model — good for quick dictation.',
    homepageUrl: 'https://huggingface.co/ggerganov/whisper.cpp',
    estimatedSizeMb: 150,
    status: 'not-downloaded',
    stage: 'idle',
    progress: 0,
    downloadedBytes: 0,
    totalBytes: null,
    selected: false,
    runtime: {
      label: 'whisper.cpp',
      ready: false,
      installCommand: 'brew install whisper-cpp',
    },
  },
  {
    id: 'whisper-small',
    name: 'Whisper Small (English, whisper.cpp)',
    family: 'whisper',
    description: 'Higher-accuracy whisper.cpp ggml model — a bit slower, noticeably better.',
    homepageUrl: 'https://huggingface.co/ggerganov/whisper.cpp',
    estimatedSizeMb: 490,
    status: 'not-downloaded',
    stage: 'idle',
    progress: 0,
    downloadedBytes: 0,
    totalBytes: null,
    selected: false,
    runtime: {
      label: 'whisper.cpp',
      ready: false,
      installCommand: 'brew install whisper-cpp',
    },
  },
]

function ProgressRing({ progress }: { progress: number | null }): JSX.Element {
  const radius = 11
  const circumference = 2 * Math.PI * radius
  const clamped = progress === null ? 0.2 : Math.max(0, Math.min(1, progress))
  const dashOffset = circumference * (1 - clamped)

  return (
    <span className="relative inline-flex h-7 w-7 items-center justify-center" aria-hidden>
      <svg
        viewBox="0 0 28 28"
        className={progress === null ? 'h-7 w-7 animate-spin' : 'h-7 w-7'}
      >
        <circle cx="14" cy="14" r={radius} fill="none" stroke="rgba(255,255,255,0.15)" strokeWidth="3" />
        <circle
          cx="14"
          cy="14"
          r={radius}
          fill="none"
          stroke="rgb(139, 141, 247)"
          strokeWidth="3"
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={dashOffset}
          transform="rotate(-90 14 14)"
        />
      </svg>
      <span className="absolute text-[9px] font-mono text-ink-2">
        {progress === null ? '…' : `${Math.round(clamped * 100)}`}
      </span>
    </span>
  )
}

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

export default function SettingsView({
  onBack,
  onOpenPermissions,
}: {
  onBack: () => void
  onOpenPermissions: () => void
}): JSX.Element {
  const rootRef = useRef<HTMLDivElement>(null)
  const [retentionSec, setRetentionSec] = useState('60')
  const [memoryEnabled, setMemoryEnabled] = useState(true)
  const [memoryMaxItems, setMemoryMaxItems] = useState('3')
  const [actionPermissionRequired, setActionPermissionRequired] = useState(true)
  const [actionRedactionEnabled, setActionRedactionEnabled] = useState(true)
  const [safetyDryRun, setSafetyDryRunState] = useState(false)
  const [voiceModes, setVoiceModes] = useState<string[]>([])
  const [voiceModels, setVoiceModels] = useState<VoiceModel[]>([])
  const [selectedVoiceModelId, setSelectedVoiceModelId] = useState<VoiceModelId>('moonshine-base-en')
  const [msg, setMsg] = useState<{ tone: 'success' | 'error'; text: string } | null>(null)

  const refreshVoiceModels = useCallback(async () => {
    const [models, selected] = await Promise.all([
      window.raymes.listVoiceModels().catch(() => []),
      window.raymes.getSelectedVoiceModel().catch(() => ({ modelId: 'moonshine-base-en' as VoiceModelId })),
    ])

    const normalized = models.length > 0 ? models : FALLBACK_VOICE_MODELS
    const selectedExists = normalized.some((model) => model.id === selected.modelId)
    const selectedModelId = selectedExists ? selected.modelId : normalized[0]?.id ?? 'moonshine-base-en'

    setVoiceModels(normalized)
    setSelectedVoiceModelId(selectedModelId)
  }, [])

  const reload = useCallback(async () => {
    const c = (await window.raymes.getLlmConfig()) as LlmConfigRecord
    const ms = typeof c.uiStateRetentionMs === 'number' ? c.uiStateRetentionMs : 60_000
    setRetentionSec(String(Math.max(0, Math.round(ms / 1000))))
    setMemoryEnabled(c.memoryEnabled !== false)
    setMemoryMaxItems(String(Math.max(0, Math.round(c.memoryMaxItems ?? 3))))
    setActionPermissionRequired(c.aiActionRequirePermission !== false)
    setActionRedactionEnabled(c.aiActionRedactionEnabled !== false)

    const [dryRun, modes] = await Promise.all([
      window.raymes.getSafetyDryRun().catch(() => false),
      window.raymes.listVoiceSttModes().catch(() => []),
      refreshVoiceModels(),
    ])

    setSafetyDryRunState(Boolean(dryRun))
    setVoiceModes(modes)
  }, [refreshVoiceModels])

  useEffect(() => {
    void reload()
  }, [reload])

  useEffect(() => {
    rootRef.current?.focus()
  }, [])

  useEffect(() => {
    if (!voiceModels.some((model) => model.status === 'downloading')) return
    const timer = window.setInterval(() => {
      void refreshVoiceModels()
    }, 700)
    return () => window.clearInterval(timer)
  }, [refreshVoiceModels, voiceModels])

  useEffect(() => {
    const onEsc = (e: KeyboardEvent): void => {
      if (e.key !== 'Escape') return
      e.preventDefault()
      e.stopPropagation()
      onBack()
    }
    window.addEventListener('keydown', onEsc, true)
    return () => window.removeEventListener('keydown', onEsc, true)
  }, [onBack])

  const save = (): void => {
    const n = Number(retentionSec)
    const m = Number(memoryMaxItems)
    if (!Number.isFinite(n) || n < 0) {
      setMsg({ tone: 'error', text: 'Enter a number greater than or equal to 0' })
      return
    }
    if (!Number.isFinite(m) || m < 0) {
      setMsg({ tone: 'error', text: 'Memory items must be 0 or more' })
      return
    }
    void window.raymes
      .setLlmConfig({
        uiStateRetentionMs: Math.round(n * 1000),
        memoryEnabled,
        memoryMaxItems: Math.round(m),
        aiActionRequirePermission: actionPermissionRequired,
        aiActionRedactionEnabled: actionRedactionEnabled,
      })
      .then(() => {
        setMsg({ tone: 'success', text: 'Saved' })
        void reload()
      })
      .catch(() => setMsg({ tone: 'error', text: 'Could not save' }))
  }

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      role="application"
      aria-label="Settings"
      className="flex h-full min-h-0 w-full flex-col gap-2 outline-none animate-raymes-scale-in"
    >
      <div className="glass-card shrink-0 px-4 py-3 animate-raymes-scale-in">
        <ViewHeader title="Settings" onBack={onBack} />
      </div>

      <div className="glass-card min-h-0 flex-1 overflow-y-auto px-4 py-3 pr-[calc(0.5rem+2px)] animate-raymes-scale-in">
        <Section
          title="Remember last screen"
          description={
            <>
              After you close the palette, reopening within this window keeps the same view. Set to{' '}
              <span className="rounded-raymes-chip border border-white/10 bg-white/[0.04] px-1 font-mono text-[10.5px] text-ink-2">
                0
              </span>{' '}
              to always open the command bar.
            </>
          }
        >
          <div className="mt-1 flex items-center gap-2.5">
            <FieldLabel htmlFor="palette-retention" className="sr-only">
              Seconds to remember palette screen
            </FieldLabel>
            <TextField
              id="palette-retention"
              type="number"
              min={0}
              step={1}
              value={retentionSec}
              onChange={(e) => setRetentionSec(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault()
                  save()
                }
              }}
              className="w-20 text-center font-mono tabular-nums"
            />
            <span className="text-[12px] text-ink-3">seconds</span>
            <div className="ml-auto">
              <Button variant="primary" onClick={save}>
                Save
              </Button>
            </div>
          </div>
          {msg ? (
            <div className="mt-2">
              <Message tone={msg.tone}>{msg.text}</Message>
            </div>
          ) : null}
        </Section>

        <Section
          title="AI Memory"
          description="Controls retrieval of past notes/conversations during AI responses."
        >
          <div className="mt-2 flex items-center gap-3">
            <label className="flex items-center gap-2 text-[12px] text-ink-2">
              <input
                type="checkbox"
                checked={memoryEnabled}
                onChange={(e) => setMemoryEnabled(e.target.checked)}
              />
              Enable memory retrieval
            </label>
            <div className="ml-auto flex items-center gap-2">
              <span className="text-[12px] text-ink-3">max items</span>
              <TextField
                type="number"
                min={0}
                step={1}
                value={memoryMaxItems}
                onChange={(e) => setMemoryMaxItems(e.target.value)}
                className="w-16 text-center font-mono tabular-nums"
              />
            </div>
          </div>
        </Section>

        <CurrencySettings />

        <Section
          title="AI Action Mode"
          description="Require explicit permission before automation and redact sensitive text by default."
        >
          <div className="mt-2 space-y-2">
            <label className="flex items-center gap-2 text-[12px] text-ink-2">
              <input
                type="checkbox"
                checked={actionPermissionRequired}
                onChange={(e) => setActionPermissionRequired(e.target.checked)}
              />
              Require explicit permission
            </label>
            <label className="flex items-center gap-2 text-[12px] text-ink-2">
              <input
                type="checkbox"
                checked={actionRedactionEnabled}
                onChange={(e) => setActionRedactionEnabled(e.target.checked)}
              />
              Redact sensitive context by default
            </label>
          </div>
        </Section>

        <Section
          title="Voice"
          description={`Detected STT modes: ${voiceModes.length > 0 ? voiceModes.join(', ') : 'none'}`}
        >
          <div className="mt-2 space-y-3">
            <div className="flex items-center gap-2">
              <FieldLabel htmlFor="voice-model-selector" className="mb-0">
                STT Model
              </FieldLabel>
              <select
                id="voice-model-selector"
                value={selectedVoiceModelId}
                onChange={(event) => {
                  const modelId = event.target.value as VoiceModelId
                  setSelectedVoiceModelId(modelId)
                  void window.raymes
                    .setSelectedVoiceModel(modelId)
                    .then(() => refreshVoiceModels())
                    .catch(() => {
                      setMsg({ tone: 'error', text: 'Could not update voice model selection' })
                    })
                }}
                className="glass-field max-w-[280px]"
              >
                {voiceModels.map((model) => (
                  <option key={model.id} value={model.id}>
                    {model.name}
                    {model.status === 'downloaded' ? ' (ready)' : ''}
                  </option>
                ))}
              </select>
            </div>

            <ul className="space-y-2">
              {voiceModels.map((model) => {
                const inProgress = model.status === 'downloading'
                const downloaded = model.status === 'downloaded'
                const canDownload = !downloaded && !inProgress
                const weightsOnDisk = model.downloadedBytes > 0

                // What the user sees instead of a generic "Downloading…" —
                // installing a Homebrew package feels very different from
                // streaming a weights file and they should know which one
                // is taking its time.
                const stageLabel =
                  model.stage === 'installing-runtime'
                    ? `Installing ${model.runtime.label}…`
                    : model.stage === 'downloading-weights'
                      ? 'Downloading model weights…'
                      : null

                return (
                  <li key={model.id} className="glass-inset px-3 py-2">
                    <div className="flex items-center justify-between gap-3">
                      <div className="min-w-0">
                        <p className="truncate text-[12.5px] font-medium text-ink-1">{model.name}</p>
                        <p className="mt-0.5 truncate text-[11px] text-ink-3">
                          {model.family} · ~{model.estimatedSizeMb} MB
                          {weightsOnDisk ? ` · ${formatBytes(model.downloadedBytes)} on disk` : ''}
                        </p>
                      </div>
                      <div className="flex shrink-0 items-center gap-2">
                        {inProgress ? <ProgressRing progress={model.progress} /> : null}
                        {downloaded ? (
                          <span className="rounded-raymes-chip border border-emerald-400/30 bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.12em] text-emerald-300">
                            Ready
                          </span>
                        ) : null}
                        {canDownload ? (
                          <Button
                            variant="primary"
                            onClick={() => {
                              void window.raymes
                                .downloadVoiceModel(model.id)
                                .then(() => refreshVoiceModels())
                                .catch((error: unknown) => {
                                  setMsg({
                                    tone: 'error',
                                    text: error instanceof Error ? error.message : 'Download failed',
                                  })
                                })
                            }}
                          >
                            {model.runtime.ready ? 'Download' : 'Install & download'}
                          </Button>
                        ) : null}
                      </div>
                    </div>

                    {stageLabel ? (
                      <p className="mt-1.5 text-[11px] text-ink-3">{stageLabel}</p>
                    ) : null}

                    {/* Runtime status badge — surfaces the *other half* of
                        provisioning (binary/package) that used to silently
                        break hold-to-speak even when weights were on disk. */}
                    {!downloaded ? (
                      <div className="mt-1.5 flex flex-wrap items-center gap-1.5 text-[11px]">
                        <span
                          className={
                            model.runtime.ready
                              ? 'rounded-raymes-chip border border-emerald-400/30 bg-emerald-500/10 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-[0.12em] text-emerald-300'
                              : 'rounded-raymes-chip border border-amber-400/30 bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-[0.12em] text-amber-200'
                          }
                        >
                          {model.runtime.ready
                            ? `${model.runtime.label} ready`
                            : `${model.runtime.label} not installed`}
                        </span>
                        {!model.runtime.ready ? (
                          <code className="truncate font-mono text-[10.5px] text-ink-3">
                            {model.runtime.installCommand}
                          </code>
                        ) : null}
                      </div>
                    ) : null}

                    {model.status === 'error' && model.errorMessage ? (
                      <pre className="mt-1.5 max-h-24 overflow-y-auto whitespace-pre-wrap rounded-raymes-chip border border-rose-400/30 bg-rose-500/10 px-2 py-1 text-[11px] text-rose-200">
                        {model.errorMessage}
                      </pre>
                    ) : null}
                  </li>
                )
              })}
            </ul>

            <p className="text-[12px] text-ink-3">
              Downloading a model will also install its runtime — whisper.cpp via Homebrew or Moonshine via pip — so Hold-to-Speak works without any extra setup. Everything runs locally; the browser-based Web Speech API is not used (it does not work inside Electron).
            </p>
          </div>
        </Section>

        <Section
          title="System permissions"
          description="Review and repair macOS privacy permissions Raymes needs — Accessibility, Automation, Input Monitoring, Microphone, Calendar, Screen Recording."
        >
          <div className="mt-2 flex items-center gap-2.5">
            <Button variant="primary" onClick={onOpenPermissions}>
              Open Permissions
            </Button>
            <span className="text-[12px] text-ink-3">
              Opens the permissions inspector with live status for each capability.
            </span>
          </div>
        </Section>

        <Section
          title="Safety dry-run"
          description="When on, destructive actions (kill port, empty trash, run shell, etc.) show a confirmation dialog and are recorded in the safety log — but never execute. Useful while testing commands."
        >
          <div className="mt-2 flex items-center gap-2.5">
            <Button
              variant={safetyDryRun ? 'primary' : 'ghost'}
              onClick={async () => {
                const next = !safetyDryRun
                setSafetyDryRunState(next)
                await window.raymes.setSafetyDryRun(next)
                setMsg({
                  tone: 'success',
                  text: next ? 'Dry-run mode enabled' : 'Dry-run mode disabled',
                })
              }}
            >
              {safetyDryRun ? 'Dry-run is ON' : 'Turn dry-run ON'}
            </Button>
            <span className="text-[12px] text-ink-3">
              {safetyDryRun ? 'No destructive action will be executed.' : 'Destructive actions execute normally.'}
            </span>
          </div>
        </Section>

        <Section title="Danger Zone" description="Quit the application and terminate all background processes.">
          <div className="mt-2">
            <Button
              variant="danger"
              onClick={() => {
                void window.raymes.appQuit()
              }}
            >
              Quit Raymes
            </Button>
          </div>
        </Section>
      </div>

      <div className="glass-card shrink-0 px-4 py-2 animate-raymes-scale-in">
        <HintBar>
          <Hint label="Save" keys={<Kbd>↵</Kbd>} />
          <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
        </HintBar>
      </div>
    </div>
  )
}
