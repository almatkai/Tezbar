import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from 'react'
import {
  AI_CAPABILITIES,
  DEFAULT_PROVIDER_MODELS,
  RECOMMENDED_AI_MODEL,
  defaultBaseUrl,
  defaultModels,
  inferCapabilities,
  isCustomProvider,
  normalizeProviderModelList,
  providerRows,
  recommendedModel,
} from '../shared/aiProviders'
import type {
  AiModelCapability,
  AiProviderConfig,
  AiProviderModel,
  CustomAiProvider,
  LlmConfigRecord,
  LlmTask,
  ProviderId,
} from '../shared/llmConfig'
import type { VoiceModel, VoiceModelId } from '../shared/voice'
import {
  Button,
  cx,
  FieldLabel,
  Hint,
  HintBar,
  Kbd,
  Message,
  SelectField,
  TextArea,
  TextField,
} from './ui/primitives'
import { CurrencySettings } from './CurrencySettings'

type SettingsTab = 'general' | 'ai' | 'voice' | 'permissions' | 'storage' | 'advanced'

const SETTINGS_TABS: Array<{ id: SettingsTab; label: string; icon: string }> = [
  { id: 'general', label: 'General', icon: 'gear' },
  { id: 'ai', label: 'AI', icon: 'spark' },
  { id: 'voice', label: 'Voice', icon: 'mic' },
  { id: 'permissions', label: 'Permissions', icon: 'lock' },
  { id: 'storage', label: 'Storage', icon: 'database' },
  { id: 'advanced', label: 'Advanced', icon: 'tool' },
]

const DEFAULT_RAYMES_HOTKEY = 'Alt+Space'

const KEY_ACCELERATORS: Record<string, string> = {
  ' ': 'Space',
  ArrowDown: 'Down',
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
  ArrowUp: 'Up',
}

const CODE_ACCELERATORS: Record<string, string> = {
  Space: 'Space',
  Enter: 'Enter',
  Tab: 'Tab',
  ArrowDown: 'Down',
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
  ArrowUp: 'Up',
  Backquote: '`',
  Minus: '-',
  BracketLeft: '[',
  BracketRight: ']',
  Backslash: '\\',
  Semicolon: ';',
  Quote: "'",
  Comma: ',',
  Period: '.',
  Slash: '/',
}

function acceleratorFromKeyEvent(event: ReactKeyboardEvent): string | null {
  const key =
    (event.code.startsWith('Key') ? event.code.slice(3) : null) ??
    (event.code.startsWith('Digit') ? event.code.slice(5) : null) ??
    (/^F(?:[1-9]|1[0-9]|2[0-4])$/.test(event.code) ? event.code : null) ??
    (event.code === 'Equal' && event.shiftKey ? 'Plus' : CODE_ACCELERATORS[event.code]) ??
    KEY_ACCELERATORS[event.key] ??
    event.key
  if (['Alt', 'Control', 'Meta', 'Shift'].includes(key)) return null

  const normalizedKey = key.length === 1 && /[a-z]/i.test(key) ? key.toUpperCase() : key

  const modifiers = [
    event.metaKey ? 'Command' : null,
    event.ctrlKey ? 'Control' : null,
    event.altKey ? 'Alt' : null,
    event.shiftKey ? 'Shift' : null,
  ].filter((value): value is string => value !== null)

  if (modifiers.length === 0 && !/^F(?:[1-9]|1[0-9]|2[0-4])$/.test(normalizedKey)) {
    return null
  }
  return [...modifiers, normalizedKey].join('+')
}

function hotkeyDisplay(accelerator: string): string {
  const labels: Record<string, string> = {
    Alt: '⌥',
    Command: '⌘',
    CommandOrControl: navigator.platform.includes('Mac') ? '⌘' : 'Ctrl',
    Control: '⌃',
    Shift: '⇧',
    Space: 'Space',
    Up: '↑',
    Down: '↓',
    Left: '←',
    Right: '→',
  }
  return accelerator
    .split('+')
    .map((part) => labels[part] ?? part)
    .join(' ')
}

function isValidStoredAccelerator(accelerator: string): boolean {
  return (
    accelerator.length > 0 &&
    /^[\x20-\x7E]+$/.test(accelerator) &&
    accelerator.split('+').every((part) => part.trim().length > 0)
  )
}

function SettingsIcon({ name, className }: { name: string; className?: string }): JSX.Element {
  const common = {
    width: 20,
    height: 20,
    viewBox: '0 0 24 24',
    fill: 'none',
    stroke: 'currentColor',
    strokeWidth: 2,
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
    'aria-hidden': true,
    className,
  }

  if (name === 'spark') {
    return (
      <svg {...common}>
        <path d="M12 3l1.7 5.1L19 10l-5.3 1.9L12 17l-1.7-5.1L5 10l5.3-1.9L12 3Z" />
        <path d="M19 15l.8 2.2L22 18l-2.2.8L19 21l-.8-2.2L16 18l2.2-.8L19 15Z" />
      </svg>
    )
  }
  if (name === 'mic') {
    return (
      <svg {...common}>
        <path d="M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3Z" />
        <path d="M5 11a7 7 0 0 0 14 0" />
        <path d="M12 18v3" />
      </svg>
    )
  }
  if (name === 'lock') {
    return (
      <svg {...common}>
        <rect x="5" y="10" width="14" height="10" rx="2" />
        <path d="M8 10V7a4 4 0 0 1 8 0v3" />
      </svg>
    )
  }
  if (name === 'tool') {
    return (
      <svg {...common}>
        <path d="M14.7 6.3a4 4 0 0 0-5 5L4 17v3h3l5.7-5.7a4 4 0 0 0 5-5l-2.4 2.4-3-3 2.4-2.4Z" />
      </svg>
    )
  }
  if (name === 'database') {
    return (
      <svg {...common}>
        <ellipse cx="12" cy="6" rx="9" ry="3" />
        <path d="M3 6v12c0 1.7 4 3 9 3s9-1.3 9-3V6" />
        <path d="M3 12c0 1.7 4 3 9 3s9-1.3 9-3" />
      </svg>
    )
  }
  return (
    <svg {...common}>
      <path d="M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Z" />
      <path d="M12 2v3" />
      <path d="M12 19v3" />
      <path d="m4.9 4.9 2.1 2.1" />
      <path d="m17 17 2.1 2.1" />
      <path d="M2 12h3" />
      <path d="M19 12h3" />
      <path d="m4.9 19.1 2.1-2.1" />
      <path d="m17 7 2.1-2.1" />
    </svg>
  )
}

function SettingsRow({
  label,
  detail,
  children,
  className,
}: {
  label: string
  detail?: string
  children: ReactNode
  className?: string
}): JSX.Element {
  return (
    <div className={cx('grid grid-cols-[150px_minmax(0,1fr)] items-start gap-4 py-3', className)}>
      <div className="pt-1 text-right text-[12px] font-semibold text-ink-3">{label}</div>
      <div className="min-w-0">
        {children}
        {detail ? <p className="mt-1 text-[11.5px] leading-snug text-ink-4">{detail}</p> : null}
      </div>
    </div>
  )
}

function Divider(): JSX.Element {
  return <div className="-mx-5 border-t border-white/[0.07]" />
}

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
      <svg viewBox="0 0 28 28" className={progress === null ? 'h-7 w-7 animate-spin' : 'h-7 w-7'}>
        <circle
          cx="14"
          cy="14"
          r={radius}
          fill="none"
          stroke="rgba(255,255,255,0.15)"
          strokeWidth="3"
        />
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

function isProviderConfigured(provider: ProviderId, apiKey: string, baseURL: string, models: AiProviderModel[]): boolean {
  const hasModel = models.length > 0
  if (provider === 'ollama') return hasModel && Boolean(baseURL.trim())
  if (provider === 'opencode') return hasModel
  return hasModel && Boolean(apiKey.trim())
}

function readProviderConfig(config: LlmConfigRecord, provider: ProviderId): AiProviderConfig {
  return config.providerConfigs?.[provider] ?? {}
}

function capabilityLabel(capability: AiModelCapability): string {
  return AI_CAPABILITIES.find((item) => item.id === capability)?.label ?? capability
}

export default function SettingsView({
  onBack,
  onOpenPermissions,
  initialTab = 'general',
  nativeWindow = false,
}: {
  onBack: () => void
  onOpenPermissions: () => void
  initialTab?: SettingsTab
  nativeWindow?: boolean
}): JSX.Element {
  const rootRef = useRef<HTMLDivElement>(null)
  const [retentionSec, setRetentionSec] = useState('60')
  const [memoryEnabled, setMemoryEnabled] = useState(true)
  const [memoryMaxItems, setMemoryMaxItems] = useState('3')
  const [actionPermissionRequired, setActionPermissionRequired] = useState(true)
  const [actionRedactionEnabled, setActionRedactionEnabled] = useState(true)
  const [aiProvider, setAiProvider] = useState<ProviderId>('ollama')
  const [aiApiKey, setAiApiKey] = useState('')
  const [aiBaseURL, setAiBaseURL] = useState('')
  const [aiProviderConfigs, setAiProviderConfigs] =
    useState<Partial<Record<ProviderId, AiProviderConfig>>>({})
  const [aiModel, setAiModel] = useState(RECOMMENDED_AI_MODEL.ollama)
  const [aiProviderModels, setAiProviderModels] =
    useState<Partial<Record<ProviderId, AiProviderModel[]>>>(DEFAULT_PROVIDER_MODELS)
  const [aiProviderSelectedModels, setAiProviderSelectedModels] =
    useState<Partial<Record<ProviderId, string>>>({})
  const [customProviders, setCustomProviders] = useState<CustomAiProvider[]>([])
  const [addProviderOpen, setAddProviderOpen] = useState(false)
  const [newProviderName, setNewProviderName] = useState('')
  const [newProviderBaseURL, setNewProviderBaseURL] = useState('')
  const [newProviderModel, setNewProviderModel] = useState('')
  const [aiTaskProviderOverrides, setAiTaskProviderOverrides] =
    useState<Partial<Record<LlmTask, ProviderId>>>({})
  const [aiTaskModelOverrides, setAiTaskModelOverrides] =
    useState<Partial<Record<LlmTask, string>>>({})
  const [aiNewModelId, setAiNewModelId] = useState('')
  const [aiModelsLoading, setAiModelsLoading] = useState(false)
  const [safetyDryRun, setSafetyDryRunState] = useState(false)
  const [voiceModes, setVoiceModes] = useState<string[]>([])
  const [voiceModels, setVoiceModels] = useState<VoiceModel[]>([])
  const [selectedVoiceModelId, setSelectedVoiceModelId] =
    useState<VoiceModelId>('moonshine-base-en')
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab)
  const [msg, setMsg] = useState<{ tone: 'success' | 'error'; text: string } | null>(null)
  const [raymesHotkey, setRaymesHotkeyState] = useState(DEFAULT_RAYMES_HOTKEY)
  const [hotkeyRecording, setHotkeyRecording] = useState(false)
  const [hotkeyMessage, setHotkeyMessage] = useState<{
    tone: 'success' | 'error'
    text: string
  } | null>(null)
  const [storageBreakdown, setStorageBreakdown] = useState<{
    totalBytes: number
    items: Array<{ id: string; label: string; bytes: number; paths: string[] }>
  } | null>(null)
  const [storageLoading, setStorageLoading] = useState(false)
  const [clipboardWatchEnabled, setClipboardWatchEnabled] = useState(true)
  const [clipboardCaptureImages, setClipboardCaptureImages] = useState(false)
  const [clipboardMaxImageMegapixels, setClipboardMaxImageMegapixels] = useState('2')

  const loadAiModels = useCallback(async (provider: ProviderId) => {
    setAiModelsLoading(true)
    try {
      const models = await window.tezbar.listLlmModels(provider)
      if (models.length > 0) {
        setAiProviderModels((prev) => {
          const existing = prev[provider] ?? defaultModels(provider)
          const discovered = models.map((id) => ({ id, capabilities: inferCapabilities(id) }))
          return {
            ...prev,
            [provider]: normalizeProviderModelList(provider, [...existing, ...discovered]),
          }
        })
      }
    } catch {
      /* Model discovery is optional; user-managed models remain available. */
    } finally {
      setAiModelsLoading(false)
    }
  }, [])

  const refreshVoiceModels = useCallback(async () => {
    const [models, selected] = await Promise.all([
      window.tezbar.listVoiceModels().catch(() => []),
      window.tezbar
        .getSelectedVoiceModel()
        .catch(() => ({ modelId: 'moonshine-base-en' as VoiceModelId })),
    ])

    const normalized = models.length > 0 ? models : FALLBACK_VOICE_MODELS
    const selectedExists = normalized.some((model) => model.id === selected.modelId)
    const selectedModelId = selectedExists
      ? selected.modelId
      : (normalized[0]?.id ?? 'moonshine-base-en')

    setVoiceModels(normalized)
    setSelectedVoiceModelId(selectedModelId)
  }, [])

  const loadStorage = useCallback(async () => {
    setStorageLoading(true)
    try {
      const [breakdown, clipboardCfg] = await Promise.all([
        window.tezbar.getStorageBreakdown(),
        window.tezbar.getClipboardStorageConfig(),
      ])
      setStorageBreakdown(breakdown)
      setClipboardWatchEnabled(clipboardCfg.watchEnabled)
      setClipboardCaptureImages(clipboardCfg.captureImages)
      setClipboardMaxImageMegapixels(String(clipboardCfg.maxImageMegapixels))
    } catch (err) {
      console.warn('[Settings] Failed to load storage breakdown:', err)
    } finally {
      setStorageLoading(false)
    }
  }, [])

  const reload = useCallback(async () => {
    const c = (await window.tezbar.getLlmConfig()) as LlmConfigRecord
    const provider = c.provider ?? 'ollama'
    const configuredProviders = c.customProviders ?? []
    const ms = typeof c.uiStateRetentionMs === 'number' ? c.uiStateRetentionMs : 60_000
    setRetentionSec(String(Math.max(0, Math.round(ms / 1000))))
    setMemoryEnabled(c.memoryEnabled !== false)
    setMemoryMaxItems(String(Math.max(0, Math.round(c.memoryMaxItems ?? 3))))
    setActionPermissionRequired(c.aiActionRequirePermission !== false)
    setActionRedactionEnabled(c.aiActionRedactionEnabled !== false)
    const providerConfig = readProviderConfig(c, provider)
    setAiProviderConfigs(c.providerConfigs ?? {})
    setCustomProviders(configuredProviders)
    setAiProvider(provider)
    setAiApiKey(
      provider === 'gemini'
        ? (providerConfig.geminiApiKey ?? c.geminiApiKey ?? c.apiKey ?? '')
        : provider === 'copilot'
          ? (providerConfig.copilotGithubToken ?? c.copilotGithubToken ?? '')
          : (providerConfig.apiKey ?? c.apiKey ?? '')
    )
    setAiBaseURL(
      provider === 'openai-compatible'
        ? (providerConfig.openaiCompatibleBaseURL ?? c.openaiCompatibleBaseURL ?? c.baseURL ?? defaultBaseUrl(provider))
        : isCustomProvider(provider)
          ? (providerConfig.openaiCompatibleBaseURL ?? providerConfig.baseURL ?? '')
          : (providerConfig.baseURL ?? c.baseURL ?? defaultBaseUrl(provider))
    )
    const providerModels: Partial<Record<ProviderId, AiProviderModel[]>> = { ...DEFAULT_PROVIDER_MODELS }
    for (const row of providerRows(c)) {
      providerModels[row.id] = normalizeProviderModelList(
        row.id,
        c.providerModels?.[row.id] ?? defaultModels(row.id)
      )
    }
    const selectedModels = c.providerSelectedModels ?? {}
    const selectedModel =
      selectedModels[provider] ?? c.model ?? providerModels[provider]?.[0]?.id ?? recommendedModel(provider)
    setAiProviderModels(providerModels)
    setAiProviderSelectedModels(selectedModels)
    setAiTaskProviderOverrides(c.taskProviderOverrides ?? {})
    setAiTaskModelOverrides(c.taskModelOverrides ?? {})
    setAiModel(selectedModel)

    const [dryRun, modes] = await Promise.all([
      window.tezbar.getSafetyDryRun().catch(() => false),
      window.tezbar.listVoiceSttModes().catch(() => []),
      refreshVoiceModels(),
    ])

    setSafetyDryRunState(Boolean(dryRun))
    setVoiceModes(modes)
    const savedHotkey = c.raymesHotkey ?? DEFAULT_RAYMES_HOTKEY
    if (isValidStoredAccelerator(savedHotkey)) {
      setRaymesHotkeyState(savedHotkey)
    } else {
      setRaymesHotkeyState(DEFAULT_RAYMES_HOTKEY)
      void window.tezbar.setLlmConfig({ raymesHotkey: DEFAULT_RAYMES_HOTKEY })
    }
    void loadAiModels(provider)
  }, [loadAiModels, refreshVoiceModels])

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
    if (activeTab !== 'storage') return
    void loadStorage()
  }, [activeTab, loadStorage])

  useEffect(() => {
    const onEsc = (e: KeyboardEvent): void => {
      if (e.key !== 'Escape') return
      if (hotkeyRecording) {
        setHotkeyRecording(false)
        setHotkeyMessage(null)
        return
      }
      e.preventDefault()
      e.stopPropagation()
      onBack()
    }
    window.addEventListener('keydown', onEsc, true)
    return () => window.removeEventListener('keydown', onEsc, true)
  }, [hotkeyRecording, onBack])

  const saveHotkey = useCallback(async (accelerator: string) => {
    try {
      const result = await window.tezbar.setLlmConfig({ raymesHotkey: accelerator })
      if (!result || typeof result !== 'object' || !('ok' in result)) {
        setRaymesHotkeyState(accelerator)
        setHotkeyRecording(false)
        setHotkeyMessage({
          tone: 'success',
          text: 'Saved. Restart Tezbar once to activate this shortcut.',
        })
        return
      }
      setRaymesHotkeyState(result.accelerator)
      setHotkeyRecording(false)
      setHotkeyMessage(
        result.ok
          ? { tone: 'success', text: 'Shortcut updated.' }
          : { tone: 'error', text: result.error ?? 'Could not save shortcut.' }
      )
    } catch {
      setHotkeyRecording(false)
      setHotkeyMessage({ tone: 'error', text: 'Could not save shortcut.' })
    }
  }, [])

  const selectAiProvider = (provider: ProviderId): void => {
    const currentProviderConfig: AiProviderConfig =
      aiProvider === 'gemini'
        ? { ...aiProviderConfigs[aiProvider], geminiApiKey: aiApiKey, baseURL: aiBaseURL }
        : aiProvider === 'copilot'
          ? { ...aiProviderConfigs[aiProvider], copilotGithubToken: aiApiKey }
          : aiProvider === 'openai-compatible' || isCustomProvider(aiProvider)
            ? { ...aiProviderConfigs[aiProvider], apiKey: aiApiKey, openaiCompatibleBaseURL: aiBaseURL }
            : { ...aiProviderConfigs[aiProvider], apiKey: aiApiKey, baseURL: aiBaseURL }
    const nextProviderConfig = aiProviderConfigs[provider] ?? {}
    setAiProviderSelectedModels((prev) => ({ ...prev, [aiProvider]: aiModel }))
    setAiProviderConfigs((prev) => ({ ...prev, [aiProvider]: currentProviderConfig }))
    setAiProvider(provider)
    setAiModel(aiProviderSelectedModels[provider] ?? aiProviderModels[provider]?.[0]?.id ?? recommendedModel(provider))
    setAiApiKey(
      provider === 'gemini'
        ? (nextProviderConfig.geminiApiKey ?? '')
        : provider === 'copilot'
          ? (nextProviderConfig.copilotGithubToken ?? '')
          : (nextProviderConfig.apiKey ?? '')
    )
    setAiBaseURL(
      provider === 'openai-compatible' || isCustomProvider(provider)
        ? (nextProviderConfig.openaiCompatibleBaseURL ?? defaultBaseUrl(provider))
        : (nextProviderConfig.baseURL ?? defaultBaseUrl(provider))
    )
    setAiNewModelId('')
    void loadAiModels(provider)
  }

  const buildAiProviderPatch = (): LlmConfigRecord => {
    const provider = aiProvider
    const model = aiModel.trim() || recommendedModel(provider)
    const baseURL = aiBaseURL.trim()
    const apiKey = aiApiKey.trim()
    const providerModels = {
      ...aiProviderModels,
      [provider]: normalizeProviderModelList(provider, aiProviderModels[provider] ?? defaultModels(provider)),
    }
    const providerSelectedModels = { ...aiProviderSelectedModels, [provider]: model }
    const providerConfig: AiProviderConfig =
      provider === 'gemini'
        ? { ...aiProviderConfigs[provider], geminiApiKey: apiKey, baseURL: baseURL || defaultBaseUrl(provider) }
        : provider === 'copilot'
          ? { ...aiProviderConfigs[provider], copilotGithubToken: aiApiKey }
          : provider === 'openai-compatible' || isCustomProvider(provider)
            ? {
              ...aiProviderConfigs[provider],
              apiKey,
              openaiCompatibleBaseURL: baseURL || defaultBaseUrl(provider),
            }
            : { ...aiProviderConfigs[provider], apiKey, baseURL: baseURL || defaultBaseUrl(provider) }
    const providerConfigs = { ...aiProviderConfigs, [provider]: providerConfig }
    const patch: LlmConfigRecord = {
      provider,
      customProviders,
      model,
      providerConfigs,
      providerModels,
      providerSelectedModels,
      taskProviderOverrides: { ...aiTaskProviderOverrides, chat: provider },
      taskModelOverrides: { ...aiTaskModelOverrides, chat: model },
    }

    if (provider === 'openai' || provider === 'anthropic' || provider === 'deepseek') {
      patch.apiKey = apiKey
      if (baseURL) patch.baseURL = baseURL
    }
    if (provider === 'openai-compatible') {
      patch.apiKey = apiKey
      patch.openaiCompatibleBaseURL = baseURL || defaultBaseUrl(provider)
    }
    if (provider === 'gemini') {
      patch.geminiApiKey = apiKey
      patch.baseURL = baseURL || defaultBaseUrl(provider)
    }
    if (provider === 'ollama') {
      patch.baseURL = baseURL || defaultBaseUrl(provider)
    }
    if (isCustomProvider(provider)) {
      patch.apiKey = apiKey
      patch.openaiCompatibleBaseURL = baseURL
    }
    if (provider === 'copilot') {
      patch.copilotGithubToken = aiApiKey
    }

    return patch
  }

  const addAiModel = (): void => {
    const id = aiNewModelId.trim()
    if (!id) return
    const current = aiProviderModels[aiProvider] ?? defaultModels(aiProvider)
    const normalized = normalizeProviderModelList(aiProvider, [
      ...current,
      { id, capabilities: inferCapabilities(id) },
    ])
    const nextSelected = normalized.some((model) => model.id === id)
      ? id
      : (normalized[0]?.id ?? recommendedModel(aiProvider))
    setAiProviderModels((prev) => ({ ...prev, [aiProvider]: normalized }))
    setAiProviderSelectedModels((prev) => ({ ...prev, [aiProvider]: nextSelected }))
    setAiModel(nextSelected)
    setAiNewModelId('')
  }

  const removeAiModel = (id: string): void => {
    setAiProviderModels((prev) => {
      const nextModels = (prev[aiProvider] ?? defaultModels(aiProvider)).filter(
        (model) => model.id !== id
      )
      const normalized = normalizeProviderModelList(aiProvider, nextModels)
      const nextSelected = normalized[0]?.id ?? recommendedModel(aiProvider)
      if (aiModel === id) {
        setAiModel(nextSelected)
        setAiProviderSelectedModels((selected) => ({ ...selected, [aiProvider]: nextSelected }))
      }
      return { ...prev, [aiProvider]: normalized }
    })
  }

  const toggleAiModelCapability = (modelId: string, capability: AiModelCapability): void => {
    setAiProviderModels((prev) => {
      const current = prev[aiProvider] ?? defaultModels(aiProvider)
      return {
        ...prev,
        [aiProvider]: current.map((model) => {
          if (model.id !== modelId) return model
          const hasCapability = model.capabilities.includes(capability)
          return {
            ...model,
            capabilities: hasCapability
              ? model.capabilities.filter((item) => item !== capability)
              : [...model.capabilities, capability],
          }
        }),
      }
    })
  }

  const addCustomProvider = (): void => {
    const title = newProviderName.trim()
    const baseURL = newProviderBaseURL.trim()
    const modelId = newProviderModel.trim()
    if (!title || !baseURL || !modelId) {
      setMsg({ tone: 'error', text: 'Provider name, endpoint, and initial model are required' })
      return
    }
    const slug = title.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '') || 'provider'
    let id: `custom:${string}` = `custom:${slug}`
    let suffix = 2
    while (customProviders.some((provider) => provider.id === id)) {
      id = `custom:${slug}-${suffix}`
      suffix += 1
    }
    const provider: CustomAiProvider = { id, title, subtitle: 'Custom OpenAI-compatible endpoint' }
    const models = normalizeProviderModelList(id, [{ id: modelId, capabilities: inferCapabilities(modelId) }])
    setCustomProviders((prev) => [...prev, provider])
    setAiProviderModels((prev) => ({ ...prev, [id]: models }))
    setAiProviderSelectedModels((prev) => ({ ...prev, [id]: modelId }))
    setAiProviderConfigs((prev) => ({ ...prev, [id]: { apiKey: '', openaiCompatibleBaseURL: baseURL } }))
    setAiProvider(id)
    setAiApiKey('')
    setAiBaseURL(baseURL)
    setAiModel(modelId)
    setAddProviderOpen(false)
    setNewProviderName('')
    setNewProviderBaseURL('')
    setNewProviderModel('')
    setMsg({ tone: 'success', text: 'Custom provider added. Add its API key, then save AI Settings.' })
  }

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
    void window.tezbar
      .setLlmConfig({
        ...buildAiProviderPatch(),
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

  const renderVoiceModels = (): JSX.Element => (
    <ul className="space-y-2">
      {voiceModels.map((model) => {
        const inProgress = model.status === 'downloading'
        const downloaded = model.status === 'downloaded'
        const canDownload = !downloaded && !inProgress
        const weightsOnDisk = model.downloadedBytes > 0
        const stageLabel =
          model.stage === 'installing-runtime'
            ? `Installing ${model.runtime.label}...`
            : model.stage === 'downloading-weights'
              ? 'Downloading model weights...'
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
                  <span className="rounded-tezbar-chip border border-emerald-400/30 bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.12em] text-emerald-300">
                    Ready
                  </span>
                ) : null}
                {canDownload ? (
                  <Button
                    variant="primary"
                    onClick={() => {
                      void window.tezbar
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
            {stageLabel ? <p className="mt-1.5 text-[11px] text-ink-3">{stageLabel}</p> : null}
            {!downloaded ? (
              <div className="mt-1.5 flex flex-wrap items-center gap-1.5 text-[11px]">
                <span
                  className={
                    model.runtime.ready
                      ? 'rounded-tezbar-chip border border-emerald-400/30 bg-emerald-500/10 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-[0.12em] text-emerald-300'
                      : 'rounded-tezbar-chip border border-amber-400/30 bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-[0.12em] text-amber-200'
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
              <pre className="mt-1.5 max-h-24 overflow-y-auto whitespace-pre-wrap rounded-tezbar-chip border border-rose-400/30 bg-rose-500/10 px-2 py-1 text-[11px] text-rose-200">
                {model.errorMessage}
              </pre>
            ) : null}
          </li>
        )
      })}
    </ul>
  )

  const currentAiModels = aiProviderModels[aiProvider] ?? defaultModels(aiProvider)
  const configured = isProviderConfigured(aiProvider, aiApiKey, aiBaseURL, currentAiModels)
  const availableProviders = providerRows({ customProviders })

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      role="application"
      aria-label="Settings"
      className="flex h-full min-h-0 w-full flex-col outline-none animate-tezbar-scale-in"
    >
      <div
        className={cx(
          'tezbar-settings-window flex min-h-0 flex-1 flex-col overflow-hidden',
          nativeWindow && 'tezbar-settings-window--native'
        )}
      >
        <header className="drag-region relative shrink-0 border-b border-white/[0.07] px-4 pb-2 pt-2">
          {!nativeWindow ? (
            <div className="absolute left-4 top-3 flex gap-2" aria-hidden>
              <span className="h-3 w-3 rounded-full bg-[#ff5f57]" />
              <span className="h-3 w-3 rounded-full bg-[#febc2e]" />
              <span className="h-3 w-3 rounded-full bg-[#28c840]" />
            </div>
          ) : null}
          <h1 className="mb-2 text-center text-[13px] font-semibold text-ink-2">Tezbar Settings</h1>
          <nav className="no-drag flex items-end justify-center gap-2">
            {SETTINGS_TABS.map((tab) => (
              <button
                key={tab.id}
                type="button"
                onClick={() => setActiveTab(tab.id)}
                className={cx(
                  'flex h-[54px] min-w-[86px] flex-col items-center justify-center gap-1 rounded-tezbar-row border text-[12px] font-semibold transition',
                  activeTab === tab.id
                    ? 'border-white/12 bg-white/[0.075] text-ink-1'
                    : 'border-transparent text-ink-3 hover:bg-white/[0.045] hover:text-ink-1'
                )}
              >
                <SettingsIcon name={tab.icon} className="h-5 w-5" />
                <span>{tab.label}</span>
              </button>
            ))}
          </nav>
        </header>

        <main className="no-drag min-h-0 flex-1 overflow-y-auto px-5 py-4">
          {activeTab === 'general' ? (
            <div className="mx-auto max-w-[610px]">
              <SettingsRow
                label="Startup"
                detail="Keeps the launcher surface alive between quick open and close cycles."
              >
                <label className="flex items-center gap-2 text-[12.5px] text-ink-2">
                  <input type="checkbox" checked readOnly />
                  Launch Tezbar at login
                </label>
              </SettingsRow>
              <Divider />
              <SettingsRow
                label="Tezbar Hotkey"
                detail={
                  hotkeyRecording
                    ? 'Press a modifier and another key. Escape cancels; Backspace restores the default.'
                    : 'Global shortcut that opens the focused command palette. Click to change it.'
                }
              >
                <button
                  type="button"
                  aria-label="Record Tezbar hotkey"
                  aria-pressed={hotkeyRecording}
                  onClick={() => {
                    setHotkeyRecording(true)
                    setHotkeyMessage(null)
                  }}
                  onBlur={() => setHotkeyRecording(false)}
                  onKeyDown={(event) => {
                    if (!hotkeyRecording) return
                    event.preventDefault()
                    event.stopPropagation()
                    if (event.key === 'Escape') {
                      setHotkeyRecording(false)
                      setHotkeyMessage(null)
                      return
                    }
                    if (event.key === 'Backspace' || event.key === 'Delete') {
                      void saveHotkey(DEFAULT_RAYMES_HOTKEY)
                      return
                    }
                    const accelerator = acceleratorFromKeyEvent(event)
                    if (!accelerator) return
                    void saveHotkey(accelerator)
                  }}
                  className={cx(
                    'flex min-h-9 w-full max-w-[280px] items-center justify-center rounded-tezbar-field border px-3 py-2 text-[13px] font-semibold transition',
                    hotkeyRecording
                      ? 'border-accent/70 bg-accent/15 text-ink-1 shadow-[0_0_0_3px_rgba(124,119,255,0.12)]'
                      : 'border-white/[0.08] bg-white/[0.10] text-ink-1 hover:bg-white/[0.14]'
                  )}
                >
                  {hotkeyRecording ? 'Press shortcut…' : hotkeyDisplay(raymesHotkey)}
                </button>
                {hotkeyMessage ? (
                  <p
                    className={cx(
                      'mt-1.5 text-[11px]',
                      hotkeyMessage.tone === 'success' ? 'text-emerald-300' : 'text-rose-300'
                    )}
                  >
                    {hotkeyMessage.text}
                  </p>
                ) : null}
              </SettingsRow>
              <Divider />
              <SettingsRow label="Last Screen">
                <div className="flex items-center gap-2.5">
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
                  <Button variant="primary" onClick={save}>
                    Save
                  </Button>
                </div>
                {msg ? (
                  <div className="mt-2">
                    <Message tone={msg.tone}>{msg.text}</Message>
                  </div>
                ) : null}
              </SettingsRow>
              <Divider />
              <SettingsRow label="Appearance">
                <div className="flex gap-3">
                  {['Light', 'Dark', 'System'].map((mode) => (
                    <button
                      key={mode}
                      type="button"
                      className={cx(
                        'flex h-[58px] w-[72px] flex-col items-center justify-center gap-1 rounded-tezbar-row border text-[12px] font-semibold transition',
                        mode === 'System'
                          ? 'border-accent/70 bg-white/[0.08] text-ink-1'
                          : 'border-white/10 bg-white/[0.035] text-ink-3 hover:text-ink-1'
                      )}
                    >
                      <span className="h-6 w-6 rounded-full border border-current" />
                      {mode}
                    </button>
                  ))}
                </div>
              </SettingsRow>
              <Divider />
              <div className="py-3">
                <CurrencySettings />
              </div>
            </div>
          ) : null}

          {activeTab === 'ai' ? (
            <div className="mx-auto max-w-[610px]">
              <SettingsRow
                label="Provider"
                detail={availableProviders.find((provider) => provider.id === aiProvider)?.subtitle}
              >
                <div className="space-y-3">
                  <div className="flex items-center gap-2.5">
                    <SelectField
                      value={aiProvider}
                      onChange={(event) => selectAiProvider(event.target.value as ProviderId)}
                      className="max-w-[280px]"
                    >
                      {availableProviders.map((provider) => (
                        <option key={provider.id} value={provider.id}>
                          {provider.title}
                        </option>
                      ))}
                    </SelectField>
                    <span
                      className={cx(
                        'rounded-tezbar-chip border px-2 py-1 font-mono text-[10px] uppercase tracking-[0.14em]',
                        configured
                          ? 'border-emerald-400/30 bg-emerald-500/10 text-emerald-300'
                          : 'border-amber-400/30 bg-amber-500/10 text-amber-200'
                      )}
                    >
                      {configured ? 'Configured' : 'Not configured'}
                    </span>
                    <Button variant="ghost" onClick={() => setAddProviderOpen((open) => !open)}>
                      {addProviderOpen ? 'Cancel' : 'Add provider'}
                    </Button>
                  </div>
                  {addProviderOpen ? (
                    <div className="space-y-2 rounded-tezbar-row border border-white/10 bg-white/[0.025] p-3">
                      <TextField
                        value={newProviderName}
                        onChange={(event) => setNewProviderName(event.target.value)}
                        placeholder="Provider name"
                      />
                      <TextField
                        value={newProviderBaseURL}
                        onChange={(event) => setNewProviderBaseURL(event.target.value)}
                        placeholder="https://your-provider.example/v1"
                        spellCheck={false}
                      />
                      <div className="flex items-center gap-2">
                        <TextField
                          value={newProviderModel}
                          onChange={(event) => setNewProviderModel(event.target.value)}
                          placeholder="Initial model id"
                          spellCheck={false}
                        />
                        <Button variant="primary" onClick={addCustomProvider}>
                          Create
                        </Button>
                      </div>
                    </div>
                  ) : null}
                </div>
              </SettingsRow>
              <Divider />
              {aiProvider !== 'ollama' && aiProvider !== 'opencode' ? (
                <>
                  <SettingsRow
                    label={aiProvider === 'copilot' ? 'GitHub Token' : 'API Key'}
                    detail={
                      aiProvider === 'copilot'
                        ? 'Use a GitHub token or OAuth access token with Copilot Chat access.'
                        : 'Stored in the local Tezbar config and used by the selected provider.'
                    }
                  >
                    {aiProvider === 'copilot' ? (
                      <TextArea
                        value={aiApiKey}
                        onChange={(event) => setAiApiKey(event.target.value)}
                        placeholder="ghp_... or OAuth access token"
                        spellCheck={false}
                      />
                    ) : (
                      <TextField
                        type="password"
                        autoComplete="off"
                        value={aiApiKey}
                        onChange={(event) => setAiApiKey(event.target.value)}
                        placeholder={
                          aiProvider === 'anthropic'
                            ? 'sk-ant-...'
                            : aiProvider === 'gemini'
                              ? 'AIza...'
                              : 'sk-...'
                        }
                      />
                    )}
                  </SettingsRow>
                  <Divider />
                </>
              ) : null}
              {aiProvider !== 'copilot' && aiProvider !== 'opencode' ? (
                <>
                  <SettingsRow
                    label="Base URL"
                    detail={
                      aiProvider === 'ollama'
                        ? 'Point this at your local Ollama server.'
                        : 'Leave the default unless your provider uses a custom endpoint.'
                    }
                  >
                    <TextField
                      value={aiBaseURL}
                      onChange={(event) => setAiBaseURL(event.target.value)}
                      placeholder={defaultBaseUrl(aiProvider)}
                    />
                  </SettingsRow>
                  <Divider />
                </>
              ) : null}
              <SettingsRow
                label="Models"
                detail="Add every model you want available for this provider, then choose the selected one."
              >
                <div className="space-y-3">
                  <div className="flex items-center gap-2">
                    <TextField
                      value={aiNewModelId}
                      onChange={(event) => setAiNewModelId(event.target.value)}
                      onKeyDown={(event) => {
                        if (event.key === 'Enter') {
                          event.preventDefault()
                          addAiModel()
                        }
                      }}
                      placeholder="Add model id"
                      spellCheck={false}
                    />
                    <Button variant="ghost" onClick={addAiModel}>
                      Add
                    </Button>
                    <Button
                      variant="quiet"
                      disabled={aiModelsLoading}
                      onClick={() => void loadAiModels(aiProvider)}
                    >
                      {aiModelsLoading ? 'Loading...' : 'Refresh'}
                    </Button>
                  </div>
                  <ul className="space-y-2">
                    {currentAiModels.map((model) => (
                      <li
                        key={model.id}
                        className={cx(
                          'rounded-tezbar-row border px-3 py-2 transition',
                          aiModel === model.id
                            ? 'border-accent/45 bg-accent/10'
                            : 'border-white/10 bg-white/[0.025]'
                        )}
                      >
                        <div className="flex items-start justify-between gap-3">
                          <button
                            type="button"
                            className="min-w-0 flex-1 text-left"
                            onClick={() => {
                              setAiModel(model.id)
                              setAiProviderSelectedModels((prev) => ({ ...prev, [aiProvider]: model.id }))
                            }}
                          >
                            <span className="block truncate text-[12.5px] font-semibold text-ink-1">
                              {model.id}
                            </span>
                            <span className="mt-1 flex flex-wrap gap-1">
                              {model.capabilities.map((capability) => (
                                <span
                                  key={capability}
                                  className="rounded-tezbar-chip border border-white/10 bg-white/[0.04] px-1.5 py-0.5 text-[9.5px] font-medium uppercase tracking-[0.1em] text-ink-3"
                                >
                                  {capabilityLabel(capability)}
                                </span>
                              ))}
                              {model.contextWindow ? (
                                <span className="rounded-tezbar-chip border border-white/10 bg-white/[0.04] px-1.5 py-0.5 text-[9.5px] font-medium uppercase tracking-[0.1em] text-ink-3">
                                  {model.contextWindow.toLocaleString()} ctx
                                </span>
                              ) : null}
                            </span>
                          </button>
                          <Button variant="quiet" onClick={() => removeAiModel(model.id)}>
                            Remove
                          </Button>
                        </div>
                        <div className="mt-2 flex flex-wrap gap-2">
                          {AI_CAPABILITIES.map((capability) => (
                            <label
                              key={capability.id}
                              className="flex items-center gap-1.5 text-[11px] text-ink-3"
                            >
                              <input
                                type="checkbox"
                                checked={model.capabilities.includes(capability.id)}
                                onChange={() => toggleAiModelCapability(model.id, capability.id)}
                              />
                              {capability.label}
                            </label>
                          ))}
                        </div>
                      </li>
                    ))}
                  </ul>
                </div>
              </SettingsRow>
              <Divider />
              <SettingsRow
                label="AI Memory"
                detail="Controls retrieval of past notes and conversations during AI responses."
              >
                <div className="flex items-center gap-3">
                  <label className="flex items-center gap-2 text-[12.5px] text-ink-2">
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
              </SettingsRow>
              <Divider />
              <SettingsRow label="Action Mode">
                <div className="space-y-2">
                  <label className="flex items-center gap-2 text-[12.5px] text-ink-2">
                    <input
                      type="checkbox"
                      checked={actionPermissionRequired}
                      onChange={(e) => setActionPermissionRequired(e.target.checked)}
                    />
                    Require explicit permission
                  </label>
                  <label className="flex items-center gap-2 text-[12.5px] text-ink-2">
                    <input
                      type="checkbox"
                      checked={actionRedactionEnabled}
                      onChange={(e) => setActionRedactionEnabled(e.target.checked)}
                    />
                    Redact sensitive context by default
                  </label>
                </div>
              </SettingsRow>
              <div className="mt-2 flex justify-end">
                <Button variant="primary" onClick={save}>
                  Save AI Settings
                </Button>
              </div>
            </div>
          ) : null}

          {activeTab === 'voice' ? (
            <div className="mx-auto max-w-[610px]">
              <SettingsRow label="Detected Modes">
                <p className="text-[12.5px] text-ink-2">
                  {voiceModes.length > 0 ? voiceModes.join(', ') : 'none'}
                </p>
              </SettingsRow>
              <Divider />
              <SettingsRow label="STT Model">
                <select
                  id="voice-model-selector"
                  value={selectedVoiceModelId}
                  onChange={(event) => {
                    const modelId = event.target.value as VoiceModelId
                    setSelectedVoiceModelId(modelId)
                    void window.tezbar
                      .setSelectedVoiceModel(modelId)
                      .then(() => refreshVoiceModels())
                      .catch(() => {
                        setMsg({ tone: 'error', text: 'Could not update voice model selection' })
                      })
                  }}
                  className="glass-field max-w-[360px]"
                >
                  {voiceModels.map((model) => (
                    <option key={model.id} value={model.id}>
                      {model.name}
                      {model.status === 'downloaded' ? ' (ready)' : ''}
                    </option>
                  ))}
                </select>
              </SettingsRow>
              <Divider />
              <div className="py-3">{renderVoiceModels()}</div>
              <p className="text-[12px] text-ink-3">
                Downloading a model also installs its runtime so Hold-to-Speak works locally.
              </p>
            </div>
          ) : null}

          {activeTab === 'permissions' ? (
            <div className="mx-auto max-w-[610px]">
              <SettingsRow
                label="System Access"
                detail="Accessibility, Automation, Input Monitoring, Microphone, Calendar, and Screen Recording."
              >
                <div className="flex items-center gap-2.5">
                  <Button variant="primary" onClick={onOpenPermissions}>
                    Open Permissions
                  </Button>
                  <span className="text-[12px] text-ink-3">Live status and repair steps.</span>
                </div>
              </SettingsRow>
            </div>
          ) : null}

          {activeTab === 'storage' ? (
            <div className="mx-auto max-w-[610px]">
              <div className="mb-3 flex justify-end">
                <Button variant="quiet" disabled={storageLoading} onClick={() => void loadStorage()}>
                  {storageLoading ? 'Calculating…' : 'Refresh usage'}
                </Button>
              </div>
              <SettingsRow
                label="Clipboard"
                detail="Clipboard history is text-only by default. Image capture is opt-in and size-capped."
              >
                <div className="space-y-3">
                  <label className="flex items-center gap-2 text-[12.5px] text-ink-2">
                    <input
                      type="checkbox"
                      checked={clipboardWatchEnabled}
                      onChange={(e) => {
                        setClipboardWatchEnabled(e.target.checked)
                        void window.tezbar
                          .setClipboardStorageConfig({ watchEnabled: e.target.checked })
                          .then((cfg) => setClipboardWatchEnabled(cfg.watchEnabled))
                      }}
                    />
                    Watch clipboard history
                  </label>
                  <label className="flex items-center gap-2 text-[12.5px] text-ink-2">
                    <input
                      type="checkbox"
                      checked={clipboardCaptureImages}
                      onChange={(e) => {
                        setClipboardCaptureImages(e.target.checked)
                        void window.tezbar
                          .setClipboardStorageConfig({ captureImages: e.target.checked })
                          .then((cfg) => setClipboardCaptureImages(cfg.captureImages))
                      }}
                    />
                    Capture copied images
                  </label>
                  <div className="flex items-center gap-2">
                    <span className="text-[12px] text-ink-3">Max image size</span>
                    <TextField
                      type="number"
                      min={0.5}
                      step={0.5}
                      value={clipboardMaxImageMegapixels}
                      onChange={(e) => setClipboardMaxImageMegapixels(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') {
                          e.preventDefault()
                          const value = Number(clipboardMaxImageMegapixels)
                          if (Number.isFinite(value) && value > 0) {
                            void window.tezbar
                              .setClipboardStorageConfig({ maxImageMegapixels: value })
                              .then((cfg) => setClipboardMaxImageMegapixels(String(cfg.maxImageMegapixels)))
                          }
                        }
                      }}
                      className="w-20 text-center font-mono tabular-nums"
                    />
                    <span className="text-[12px] text-ink-3">megapixels</span>
                    <Button
                      variant="primary"
                      onClick={() => {
                        const value = Number(clipboardMaxImageMegapixels)
                        if (Number.isFinite(value) && value > 0) {
                          void window.tezbar
                            .setClipboardStorageConfig({ maxImageMegapixels: value })
                            .then((cfg) => setClipboardMaxImageMegapixels(String(cfg.maxImageMegapixels)))
                        }
                      }}
                    >
                      Save
                    </Button>
                  </div>
                </div>
              </SettingsRow>
              <Divider />
              {storageBreakdown ? (
                <>
                  <SettingsRow
                    label="Breakdown"
                    detail={`Tracked storage: ${formatBytes(storageBreakdown.totalBytes)}`}
                  >
                    <ul className="space-y-2">
                      {storageBreakdown.items.map((item) => (
                        <li
                          key={item.id}
                          className="flex items-center justify-between gap-3 rounded-tezbar-row border border-white/10 bg-white/[0.025] px-3 py-2"
                        >
                          <span className="text-[12.5px] text-ink-2">{item.label}</span>
                          <span className="font-mono text-[12px] text-ink-3">{formatBytes(item.bytes)}</span>
                        </li>
                      ))}
                    </ul>
                  </SettingsRow>
                  <Divider />
                </>
              ) : null}
              <SettingsRow label="Cleanup">
                <div className="flex flex-wrap gap-2">
                  <Button
                    variant="ghost"
                    onClick={() => {
                      void window.tezbar.clearClipboardImages().then(() => void loadStorage())
                    }}
                  >
                    Clear clipboard images
                  </Button>
                  <Button
                    variant="ghost"
                    onClick={() => {
                      void window.tezbar.vacuumSearchDatabase().then(() => void loadStorage())
                    }}
                  >
                    Vacuum search DB
                  </Button>
                  <Button
                    variant="ghost"
                    onClick={() => {
                      void window.tezbar.clearChromiumCache().then(() => void loadStorage())
                    }}
                  >
                    Clear Chromium cache
                  </Button>
                </div>
              </SettingsRow>
            </div>
          ) : null}

          {activeTab === 'advanced' ? (
            <div className="mx-auto max-w-[610px]">
              <SettingsRow
                label="Safety Dry-Run"
                detail="Preview safety-aware shell, extension-install, and native system actions without executing them."
              >
                <div className="flex items-center gap-2.5">
                  <Button
                    variant={safetyDryRun ? 'primary' : 'ghost'}
                    onClick={async () => {
                      const next = !safetyDryRun
                      setSafetyDryRunState(next)
                      await window.tezbar.setSafetyDryRun(next)
                      setMsg({
                        tone: 'success',
                        text: next ? 'Dry-run mode enabled' : 'Dry-run mode disabled',
                      })
                    }}
                  >
                    {safetyDryRun ? 'Dry-run is ON' : 'Turn dry-run ON'}
                  </Button>
                  <span className="text-[12px] text-ink-3">
                    {safetyDryRun
                      ? 'Safety-aware actions are previewed and logged.'
                      : 'Safety-aware actions execute after any required confirmation.'}
                  </span>
                </div>
              </SettingsRow>
              <Divider />
              <SettingsRow
                label="Danger Zone"
                detail="Quit the application and terminate all background processes."
              >
                <Button
                  variant="danger"
                  onClick={() => {
                    void window.tezbar.appQuit()
                  }}
                >
                  Quit Tezbar
                </Button>
              </SettingsRow>
            </div>
          ) : null}
        </main>

        <footer className="no-drag shrink-0 border-t border-white/[0.07] px-4 py-2">
          <HintBar>
            <Hint label="Save" keys={<Kbd>Enter</Kbd>} />
            <Hint label={nativeWindow ? 'Close' : 'Back'} keys={<Kbd>Esc</Kbd>} />
          </HintBar>
        </footer>
      </div>
    </div>
  )
}
