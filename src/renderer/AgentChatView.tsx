import {
  type FormEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react'
import type { AgentRunEvent, Stage } from '../shared/agent'
import {
  defaultModels,
  isAiProviderConfigured,
  normalizeProviderModelList,
  providerRows,
  providerTitle,
  recommendedModel,
} from '../shared/aiProviders'
import {
  CHAT_CONTINUATION_WINDOW_MS,
  type ChatSession,
  type ChatSessionSummary,
  type ChatTurn,
} from '../shared/chat'
import type { AiChatBoot } from '../shared/aiChatSurface'
import { RAYMES_AI_NEW_CHAT_EVENT } from '../shared/aiChatSurface'
import { formatLlmErrorMessage } from '../shared/llmErrors'
import type { AiProviderModel, LlmConfigRecord, ProviderId } from '../shared/llmConfig'
import { Hint, HintBar, Kbd, cx } from './ui/primitives'
import { Markdown } from './ui/Markdown'
import { setCommandSurfaceEscapeConsumer } from './escapeGate'
import { AgentStageList } from './agentChat/shared'
import {
  buildAgentPromptFromChat,
  makeChatId,
  summarizeChatTitle,
} from './agentChat/model'

function focusChatInput(): void {
  document.getElementById('ai-chat-input')?.focus()
}

const QUESTION_PREFIX_RE = /^(what|why|how|who|when|is|are|can|does)\b/i
const AGENT_TASK_RE =
  /\b(cd|git|clone|mkdir|touch|rm|mv|cp|pnpm|npm|yarn|bun|cargo|go|python|node|run|execute|install|build|test|fix|create|open|move|delete|rename|write|edit|commit|push|pull|list|show|find)\b/i
const LOCAL_PATH_RE = /(?:~\/|\.{1,2}\/|\/Users\/|\bdesktop\/|\bdesktop\\|\bcode\/|\bcode\\)/i
const MACHINE_QUERY_RE =
  /\b(i have|my mac|my computer|my system|installed|applications?|apps?|code editors?|editors?|on this machine|on my machine)\b/i

function shouldRunAgent(message: string): boolean {
  const trimmed = message.trim()
  if (!trimmed) return false
  if (/```|[$>] /.test(trimmed)) return true
  if (LOCAL_PATH_RE.test(trimmed)) return true
  if (MACHINE_QUERY_RE.test(trimmed)) return true
  if (trimmed.endsWith('?') || QUESTION_PREFIX_RE.test(trimmed)) return false
  return AGENT_TASK_RE.test(trimmed)
}

function modelsForProvider(cfg: LlmConfigRecord, provider: ProviderId): AiProviderModel[] {
  return normalizeProviderModelList(
    provider,
    cfg.providerModels?.[provider] ?? defaultModels(provider)
  )
}

export default function AgentChatView({
  boot,
  onBack,
  onOpenSettings,
}: {
  boot: AiChatBoot
  onBack: () => void
  onOpenSettings: () => void
}): JSX.Element {
  const [chatSession, setChatSession] = useState<ChatSession | null>(null)
  const [chatHistory, setChatHistory] = useState<ChatSessionSummary[]>([])
  const [historyOpen, setHistoryOpen] = useState(false)
  const [draft, setDraft] = useState('')
  const [runLogs, setRunLogs] = useState<string[]>([])
  const [logsOpen, setLogsOpen] = useState(true)
  const [llmConfig, setLlmConfig] = useState<LlmConfigRecord>({})
  const [modelPickerOpen, setModelPickerOpen] = useState(false)
  const [hoveredProvider, setHoveredProvider] = useState<ProviderId | null>(null)
  const [modelSearch, setModelSearch] = useState('')
  const modelPickerRef = useRef<HTMLDivElement | null>(null)

  const chatThreadRef = useRef<HTMLDivElement>(null)
  const historyOpenRef = useRef(false)
  useEffect(() => {
    historyOpenRef.current = historyOpen
  }, [historyOpen])

  const chatSessionRef = useRef<ChatSession | null>(null)
  const agentStreamTextRef = useRef('')
  const agentStagesRef = useRef<Stage[]>([])
  const agentStatusRef = useRef<'idle' | 'running' | 'done' | 'error'>('idle')
  const agentErrorRef = useRef<string | null>(null)
  const currentAgentRunIdRef = useRef<string | null>(null)
  const submittedBootRef = useRef<string | null>(null)
  const completedRunIdsRef = useRef(new Set<string>())

  const [agentStages, setAgentStages] = useState<Stage[]>([])
  const [agentStreamText, setAgentStreamText] = useState('')
  const [agentStatus, setAgentStatus] = useState<
    'idle' | 'running' | 'done' | 'error'
  >('idle')
  const [agentError, setAgentError] = useState<string | null>(null)

  const activeProvider = (llmConfig.provider ?? 'ollama') as ProviderId
  const previewProvider = hoveredProvider ?? activeProvider
  const activeModel =
    llmConfig.providerSelectedModels?.[activeProvider] ??
    llmConfig.model ??
    recommendedModel(activeProvider)
  const availableProviders = providerRows(llmConfig)
  const previewModels = modelsForProvider(llmConfig, previewProvider)
  const previewConfigured = isAiProviderConfigured(llmConfig, previewProvider)
  const filteredPreviewModels = previewModels.filter((model) => {
    const query = modelSearch.trim().toLowerCase()
    if (!query) return true
    const haystack = `${model.id} ${providerTitle(previewProvider, llmConfig)} ${model.capabilities.join(' ')}`
      .toLowerCase()
    return haystack.includes(query)
  })

  useEffect(() => {
    chatSessionRef.current = chatSession
  }, [chatSession])
  useEffect(() => {
    void window.raymes.getLlmConfig().then((config) => setLlmConfig(config)).catch(() => {
      /* ignore */
    })
  }, [])
  useEffect(() => {
    if (!modelPickerOpen) return
    const closeIfOutside = (event: PointerEvent): void => {
      if (!modelPickerRef.current?.contains(event.target as Node)) {
        setModelPickerOpen(false)
      }
    }
    document.addEventListener('pointerdown', closeIfOutside)
    return () => document.removeEventListener('pointerdown', closeIfOutside)
  }, [modelPickerOpen])
  useEffect(() => {
    agentStreamTextRef.current = agentStreamText
  }, [agentStreamText])
  useEffect(() => {
    agentStagesRef.current = agentStages
  }, [agentStages])
  useEffect(() => {
    agentStatusRef.current = agentStatus
  }, [agentStatus])
  useEffect(() => {
    agentErrorRef.current = agentError
  }, [agentError])

  const refreshChatHistory = useCallback(async (): Promise<void> => {
    try {
      const rows = await window.raymes.chatList(40)
      setChatHistory(rows)
    } catch {
      /* ignore */
    }
  }, [])

  const stopRun = useCallback((): void => {
    if (!currentAgentRunIdRef.current) return
    void window.raymes.agentCancel()
    currentAgentRunIdRef.current = null
    agentStatusRef.current = 'idle'
    agentErrorRef.current = null
    agentStreamTextRef.current = ''
    agentStagesRef.current = []
    setAgentStatus('idle')
    setAgentError(null)
    setAgentStreamText('')
    setAgentStages([])
    focusChatInput()
  }, [])

  const startNewChat = useCallback((): void => {
    if (currentAgentRunIdRef.current) {
      stopRun()
    }
    setChatSession(null)
    chatSessionRef.current = null
    setAgentStages([])
    setAgentStreamText('')
    setAgentError(null)
    setAgentStatus('idle')
    setHistoryOpen(false)
    setRunLogs([])
    focusChatInput()
  }, [stopRun])

  const loadChatSession = useCallback(async (id: string): Promise<void> => {
    try {
      const full = await window.raymes.chatGet(id)
      if (!full) return
      stopRun()
      setChatSession(full)
      chatSessionRef.current = full
      setAgentStages([])
      setAgentStreamText('')
      setAgentError(null)
      setAgentStatus('idle')
      setHistoryOpen(false)
      setRunLogs([])
      focusChatInput()
    } catch {
      /* ignore */
    }
  }, [stopRun])

  const deleteChatFromHistory = useCallback(
    async (id: string): Promise<void> => {
      try {
        await window.raymes.chatDelete(id)
        if (chatSessionRef.current?.id === id) {
          setChatSession(null)
          chatSessionRef.current = null
          setAgentStages([])
          setAgentStreamText('')
          setAgentError(null)
          setAgentStatus('idle')
        }
        void refreshChatHistory()
      } catch {
        /* ignore */
      }
    },
    [refreshChatHistory],
  )

  const commitUserTurnAndRun = useCallback(
    async (task: string): Promise<void> => {
      const trimmed = task.trim()
      if (!trimmed || agentStatusRef.current === 'running') return
      agentStatusRef.current = 'running'
      const now = Date.now()
      const existing = chatSessionRef.current

      const session: ChatSession = existing
        ? existing
        : {
            id: makeChatId(),
            title: summarizeChatTitle(trimmed),
            createdAt: now,
            updatedAt: now,
            turns: [],
          }

      const userTurn: ChatTurn = {
        id: makeChatId(),
        role: 'user',
        text: trimmed,
        createdAt: now,
      }
      const nextSession: ChatSession = {
        ...session,
        updatedAt: now,
        turns: [...session.turns, userTurn],
      }
      setChatSession(nextSession)
      chatSessionRef.current = nextSession

      void window.raymes
        .chatAppend({
          session: {
            id: nextSession.id,
            title: nextSession.title,
            createdAt: nextSession.createdAt,
            updatedAt: nextSession.updatedAt,
          },
          turn: userTurn,
        })
        .then(() => refreshChatHistory())

      setAgentError(null)
      setAgentStages([])
      setAgentStreamText('')
      setAgentStatus('running')
      setRunLogs([])

      try {
        const result = shouldRunAgent(trimmed)
          ? await window.raymes.agentRun(buildAgentPromptFromChat(nextSession, trimmed))
          : await window.raymes.chatRun(nextSession.turns)
        if (!result.ok) {
          const error = formatLlmErrorMessage(result.error || 'Run failed to start')
          agentErrorRef.current = error
          agentStatusRef.current = 'error'
          setAgentError(error)
          setAgentStatus('error')
        }
      } catch (err) {
        const error = formatLlmErrorMessage(
          err instanceof Error ? err.message : 'Run failed to start'
        )
        agentErrorRef.current = error
        agentStatusRef.current = 'error'
        setAgentError(error)
        setAgentStatus('error')
      }
    },
    [refreshChatHistory],
  )

  const bootKey =
    boot.kind === 'submit'
      ? `submit:${boot.prompt}`
      : boot.kind === 'resume'
        ? `resume:${boot.sessionId}`
        : boot.kind === 'panel'
          ? 'panel'
          : 'newChat'

  // First paint: honour boot + hydrate history.
  useEffect(() => {
    let cancelled = false
    void (async () => {
      try {
        await refreshChatHistory()
        if (cancelled) return

        if (boot.kind === 'newChat') {
          startNewChat()
          return
        }

        if (boot.kind === 'resume') {
          await loadChatSession(boot.sessionId)
          return
        }

        const rows = await window.raymes.chatList(40)
        if (cancelled) return
        setChatHistory(rows)

        if (boot.kind === 'panel') {
          const mostRecent = rows[0]
          if (
            mostRecent &&
            Date.now() - mostRecent.updatedAt < CHAT_CONTINUATION_WINDOW_MS
          ) {
            const full = await window.raymes.chatGet(mostRecent.id)
            if (!cancelled && full) {
              setChatSession(full)
              chatSessionRef.current = full
            }
          }
          return
        }

        if (boot.kind === 'submit') {
          let session: ChatSession | null = null
          const mostRecent = rows[0]
          if (
            mostRecent &&
            Date.now() - mostRecent.updatedAt < CHAT_CONTINUATION_WINDOW_MS
          ) {
            session = await window.raymes.chatGet(mostRecent.id)
          }
          if (cancelled) return
          if (session) {
            chatSessionRef.current = session
            setChatSession(session)
          }
          if (submittedBootRef.current === bootKey) return
          submittedBootRef.current = bootKey
          await commitUserTurnAndRun(boot.prompt)
        }
      } catch {
        /* ignore */
      }
    })()
    return () => {
      cancelled = true
    }
    // bootKey replaces `boot` so parent object identity does not retrigger.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [bootKey])

  useEffect(() => {
    const el = chatThreadRef.current
    if (!el) return
    el.scrollTop = el.scrollHeight
  }, [chatSession, agentStreamText, agentStages.length, runLogs.length])

  useEffect(() => {
    return window.raymes.onAgentEvent((event: AgentRunEvent) => {
      if (
        event.type !== 'start' &&
        currentAgentRunIdRef.current !== null &&
        event.runId !== currentAgentRunIdRef.current
      ) {
        return
      }

      switch (event.type) {
        case 'start':
          currentAgentRunIdRef.current = event.runId
          agentStagesRef.current = []
          agentStreamTextRef.current = ''
          agentErrorRef.current = null
          agentStatusRef.current = 'running'
          setRunLogs([])
          setAgentStages([])
          setAgentStreamText('')
          setAgentError(null)
          setAgentStatus('running')
          return
        case 'log':
          if (event.source === 'stderr') {
            setRunLogs((prev) => [...prev.slice(-400), event.line])
          }
          return
        case 'stage': {
          const prev = agentStagesRef.current
          const idx = prev.findIndex((s) => s.index === event.stage.index)
          const next = idx < 0 ? [...prev, event.stage] : prev.slice()
          if (idx >= 0) next[idx] = event.stage
          agentStagesRef.current = next
          setAgentStages(next)
          return
        }
        case 'message': {
          const next = agentStreamTextRef.current + event.delta
          agentStreamTextRef.current = next
          setAgentStreamText(next)
          return
        }
        case 'answer':
          agentStreamTextRef.current = event.text
          setAgentStreamText(event.text)
          return
        case 'error':
          agentErrorRef.current = formatLlmErrorMessage(event.message)
          agentStatusRef.current = 'error'
          setAgentError(agentErrorRef.current)
          setAgentStatus('error')
          return
        case 'done': {
          if (completedRunIdsRef.current.has(event.runId)) return
          completedRunIdsRef.current.add(event.runId)
          const finalText = agentStreamTextRef.current
          const finalStages = agentStagesRef.current.slice()
          const activeSession = chatSessionRef.current
          const hadError =
            agentStatusRef.current === 'error' || agentErrorRef.current !== null
          const nextStatus: 'done' | 'error' = hadError ? 'error' : 'done'
          agentStatusRef.current = nextStatus
          setAgentStatus(nextStatus)
          currentAgentRunIdRef.current = null

          if (activeSession) {
            const hasPayload = finalText.trim() || finalStages.length > 0 || hadError
            const errorText = hadError
              ? agentErrorRef.current ?? 'Agent finished without a response.'
              : undefined
            const fallbackError = hasPayload ? errorText : 'Agent finished without a response.'
            const turn: ChatTurn = {
              id: makeChatId(),
              role: 'assistant',
              text: finalText,
              stages: finalStages.length > 0 ? finalStages : undefined,
              error: fallbackError,
              createdAt: Date.now(),
            }
            const nextSession: ChatSession = {
              ...activeSession,
              updatedAt: turn.createdAt,
              turns: [...activeSession.turns, turn],
            }
            chatSessionRef.current = nextSession
            setChatSession(nextSession)
            void window.raymes
              .chatAppend({
                session: {
                  id: nextSession.id,
                  title: nextSession.title,
                  createdAt: nextSession.createdAt,
                  updatedAt: nextSession.updatedAt,
                },
                turn,
              })
              .then(() => refreshChatHistory())
            agentStreamTextRef.current = ''
            agentStagesRef.current = []
            agentErrorRef.current = null
            setAgentStreamText('')
            setAgentStages([])
            setAgentError(null)
          }
          return
        }
        default:
          return
      }
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    const onNewChat = (): void => {
      startNewChat()
    }
    window.addEventListener(RAYMES_AI_NEW_CHAT_EVENT, onNewChat)
    return () => window.removeEventListener(RAYMES_AI_NEW_CHAT_EVENT, onNewChat)
  }, [startNewChat])

  useEffect(() => {
    setCommandSurfaceEscapeConsumer(() => {
      if (currentAgentRunIdRef.current) {
        stopRun()
        return true
      }
      if (historyOpenRef.current) {
        setHistoryOpen(false)
        focusChatInput()
        return true
      }
      return false
    })
    return () => {
      setCommandSurfaceEscapeConsumer(null)
    }
  }, [stopRun])

  useEffect(() => {
    requestAnimationFrame(() => focusChatInput())
  }, [])

  async function onSubmitMessage(e: FormEvent): Promise<void> {
    e.preventDefault()
    const text = draft.trim()
    if (!text) return
    setDraft('')
    await commitUserTurnAndRun(text)
  }

  async function selectProviderModel(provider: ProviderId, modelId: string): Promise<void> {
    const models = modelsForProvider(llmConfig, provider)
    const providerModels = {
      ...llmConfig.providerModels,
      [provider]: models,
    }
    const providerSelectedModels = {
      ...llmConfig.providerSelectedModels,
      [provider]: modelId,
    }
    const patch: LlmConfigRecord = {
      provider,
      model: modelId,
      providerModels,
      providerSelectedModels,
      taskProviderOverrides: { ...llmConfig.taskProviderOverrides, chat: provider },
      taskModelOverrides: { ...llmConfig.taskModelOverrides, chat: modelId },
    }
    setLlmConfig((prev) => ({ ...prev, ...patch }))
    setModelPickerOpen(false)
    await window.raymes.setLlmConfig(patch)
    const next = await window.raymes.getLlmConfig()
    setLlmConfig(next)
  }

  return (
    <div
      aria-label="AI Chat"
      tabIndex={-1}
      className="flex h-full min-h-0 w-full flex-col gap-2 outline-none"
    >
      <div className="glass-card flex min-h-0 flex-1 flex-col overflow-hidden px-4 py-3 animate-raymes-scale-in">
        <div className="relative mb-3 flex shrink-0 items-center justify-between gap-2">
          <div className="min-w-0 flex-1">
            <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-violet-200">
              AI Chat
            </p>
            {chatSession ? (
              <p className="truncate text-[12.5px] text-ink-1">{chatSession.title}</p>
            ) : (
              <p className="text-[12.5px] text-ink-3">Ask anything</p>
            )}
          </div>
          <div className="flex shrink-0 items-center gap-1.5">
            <div
              ref={modelPickerRef}
              className="relative"
              onMouseLeave={() => {
                setHoveredProvider(null)
              }}
            >
              <button
                type="button"
                className="inline-flex h-6 items-center rounded-raymes-chip border border-white/10 bg-white/[0.03] px-2 text-[10px] font-medium uppercase tracking-[0.12em] text-ink-3 transition hover:text-ink-1"
                onClick={() => {
                  setHistoryOpen(false)
                  setModelPickerOpen((open) => !open)
                  setHoveredProvider(activeProvider)
                }}
              >
                {activeModel}
              </button>
              {modelPickerOpen ? (
                <div className="raymes-popover absolute right-0 top-7 z-30 grid h-[390px] w-[560px] grid-cols-[190px_minmax(0,1fr)] overflow-hidden p-1.5">
                  <div className="flex min-h-0 flex-col border-r border-white/[0.07] pr-1">
                    <ul className="min-h-0 flex-1 overflow-y-auto">
                      {availableProviders.map((provider) => {
                        const configured = isAiProviderConfigured(llmConfig, provider.id)
                        const active = provider.id === activeProvider
                        const hovered = provider.id === previewProvider
                        const selectedModel =
                          llmConfig.providerSelectedModels?.[provider.id] ??
                          (provider.id === activeProvider ? activeModel : recommendedModel(provider.id))
                        return (
                          <li key={provider.id}>
                            <button
                              type="button"
                              onMouseEnter={() => setHoveredProvider(provider.id)}
                              onFocus={() => setHoveredProvider(provider.id)}
                              className={cx(
                                'flex w-full items-center gap-2 rounded-raymes-row px-2 py-2 text-left transition',
                                hovered
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
                      onClick={() => {
                        setModelPickerOpen(false)
                        onOpenSettings()
                      }}
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
                          {providerTitle(previewProvider, llmConfig)}
                        </p>
                        <p className="text-[10px] uppercase tracking-[0.14em] text-ink-4">
                          Models
                        </p>
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
                        onClick={() => {
                          setModelPickerOpen(false)
                          onOpenSettings()
                        }}
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
                                void selectProviderModel(previewProvider, model.id)
                              }}
                            >
                              <span className="flex items-start justify-between gap-3">
                                <span className="min-w-0">
                                  <span className="block truncate text-[12px] font-semibold">
                                    {model.id}
                                  </span>
                                  <span className="mt-0.5 block truncate text-[10.5px] text-ink-4">
                                    {providerTitle(previewProvider, llmConfig)}
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
                        onClick={() => {
                          setModelPickerOpen(false)
                          onOpenSettings()
                        }}
                      >
                        Configure in Settings
                      </button>
                    </div>
                  </div>
                </div>
              ) : null}
            </div>
            {agentStatus === 'running' ? (
              <button
                type="button"
                className="inline-flex h-6 items-center rounded-raymes-chip border border-rose-400/35 bg-rose-500/10 px-2 text-[10px] font-semibold uppercase tracking-[0.12em] text-rose-200 transition hover:border-rose-300/60 hover:bg-rose-500/16 hover:text-rose-100"
                onClick={() => {
                  stopRun()
                }}
              >
                Stop
              </button>
            ) : null}
            <button
              type="button"
              className="rounded-raymes-chip border border-white/10 bg-white/[0.03] px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.12em] text-ink-3 transition hover:text-ink-1"
              onClick={() => {
                setModelPickerOpen(false)
                setHistoryOpen((v) => !v)
                if (!historyOpen) void refreshChatHistory()
              }}
              aria-expanded={historyOpen}
            >
              History
            </button>
            <button
              type="button"
              className="rounded-raymes-chip border border-white/10 bg-white/[0.03] px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.12em] text-ink-3 transition hover:text-ink-1"
              onClick={() => {
                startNewChat()
              }}
            >
              New
            </button>
            <button
              type="button"
              className="rounded-raymes-chip border border-white/10 bg-white/[0.03] px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.12em] text-ink-3 transition hover:text-ink-1"
              onClick={() => {
                void onBack()
              }}
            >
              Back
            </button>
          </div>

          {historyOpen ? (
            <div className="glass-card absolute right-6 top-[72px] z-20 w-[320px] overflow-hidden py-1.5 shadow-[0_12px_40px_rgba(0,0,0,0.45)]">
              <div className="flex items-center justify-between px-3 pb-1.5 pt-1 text-[10px] font-semibold uppercase tracking-[0.14em] text-ink-4">
                <span>Past chats</span>
                {chatHistory.length > 0 ? (
                  <button
                    type="button"
                    className="text-[10px] text-ink-4 transition hover:text-rose-300"
                    onClick={() => {
                      void window.raymes.chatClear().then(() => {
                        setChatHistory([])
                        setHistoryOpen(false)
                      })
                    }}
                  >
                    Clear all
                  </button>
                ) : null}
              </div>
              <ul className="max-h-64 overflow-y-auto">
                {chatHistory.length === 0 ? (
                  <li className="px-3 py-2 text-[12px] text-ink-4">No saved chats yet.</li>
                ) : (
                  chatHistory.map((row) => {
                    const isActive = chatSession?.id === row.id
                    return (
                      <li key={row.id} className="group relative">
                        <button
                          type="button"
                          className={cx(
                            'flex w-full items-start justify-between gap-3 px-3 py-2 text-left transition',
                            isActive
                              ? 'bg-violet-500/10 text-ink-1'
                              : 'hover:bg-white/[0.04] text-ink-2 hover:text-ink-1',
                          )}
                          onClick={() => {
                            void loadChatSession(row.id)
                          }}
                        >
                          <span className="min-w-0 flex-1">
                            <span className="block truncate text-[12.5px] font-medium">
                              {row.title || 'Untitled chat'}
                            </span>
                            {row.preview ? (
                              <span className="mt-0.5 block truncate text-[11px] text-ink-4">
                                {row.preview}
                              </span>
                            ) : null}
                          </span>
                          <span
                            role="button"
                            tabIndex={-1}
                            aria-label="Delete chat"
                            className="mt-0.5 shrink-0 rounded-raymes-chip border border-transparent px-1.5 py-0.5 text-[10px] text-ink-4 opacity-0 transition hover:border-rose-400/40 hover:text-rose-300 group-hover:opacity-100"
                            onClick={(ev) => {
                              ev.stopPropagation()
                              void deleteChatFromHistory(row.id)
                            }}
                          >
                            ✕
                          </span>
                        </button>
                      </li>
                    )
                  })
                )}
              </ul>
            </div>
          ) : null}
        </div>

        <div
          ref={chatThreadRef}
          className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pb-2"
        >
          {chatSession && chatSession.turns.length > 0
            ? chatSession.turns.map((turn) =>
                turn.role === 'user' ? (
                  <div key={turn.id} className="flex justify-end">
                    <div className="max-w-[88%] rounded-raymes-row border border-violet-400/30 bg-violet-500/12 px-3 py-2 text-[13.5px] leading-[1.5] text-ink-1">
                      {turn.text}
                    </div>
                  </div>
                ) : (
                  <div key={turn.id} className="flex flex-col gap-1.5">
                    {turn.stages && turn.stages.length > 0 ? (
                      <AgentStageList stages={turn.stages} compact />
                    ) : null}
                    {turn.text ? (
                      <Markdown text={turn.text} />
                    ) : turn.error ? null : (
                      <p className="text-[12.5px] italic text-ink-4">(no text response)</p>
                    )}
                    {turn.error ? (
                      <p className="text-[11.5px] text-rose-300" role="alert">
                        {formatLlmErrorMessage(turn.error)}
                      </p>
                    ) : null}
                  </div>
                ),
              )
            : null}

          {agentStatus === 'running' || agentStages.length > 0 || agentStreamText ? (
            <div className="flex flex-col gap-1.5">
              {agentStages.length > 0 ? <AgentStageList stages={agentStages} /> : null}
              {agentStreamText ? (
                <Markdown text={agentStreamText} streaming={agentStatus === 'running'} />
              ) : agentStatus === 'running' ? (
                <p className="raymes-thinking flex items-center gap-2 text-[12px] text-ink-3">
                  <span className="inline-flex gap-1">
                    <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-ink-3" />
                    <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-ink-3 [animation-delay:120ms]" />
                    <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-ink-3 [animation-delay:240ms]" />
                  </span>
                  Planning
                </p>
              ) : null}
            </div>
          ) : null}

          {agentError ? (
            <p className="text-[11.5px] text-rose-300" role="alert">
              {agentError}
            </p>
          ) : null}

          {runLogs.length > 0 ? (
            <div className="rounded-raymes-row border border-amber-400/20 bg-amber-500/5">
              <button
                type="button"
                className="flex w-full items-center justify-between px-2 py-1.5 text-left text-[10px] font-semibold uppercase tracking-[0.12em] text-amber-200/90"
                onClick={() => setLogsOpen((v) => !v)}
                aria-expanded={logsOpen}
              >
                Agent log (stderr)
                <span className="text-ink-4">{logsOpen ? '▼' : '▶'}</span>
              </button>
              {logsOpen ? (
                <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-words px-2 pb-2 font-mono text-[10px] leading-relaxed text-amber-100/85">
                  {runLogs.join('\n')}
                </pre>
              ) : null}
            </div>
          ) : null}

          {!chatSession || chatSession.turns.length === 0 ? (
            agentStatus === 'idle' && !agentStreamText ? (
              <div className="flex flex-1 items-center justify-center px-4 py-6 text-center">
                <div className="max-w-[440px] space-y-2 text-ink-3">
                  <p className="text-[13px] text-ink-2">AI chat</p>
                  <p className="text-[12px] text-ink-4">
                    Type below and press Enter.
                  </p>
                </div>
              </div>
            ) : null
          ) : null}
        </div>

        <form
          className="shrink-0 border-t border-white/[0.06] pt-3"
          onSubmit={(ev) => void onSubmitMessage(ev)}
        >
          <input
            id="ai-chat-input"
            type="text"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="Message the agent…"
            autoComplete="off"
            spellCheck={false}
            className="w-full rounded-raymes-row border border-white/10 bg-white/[0.04] px-3 py-2 font-display text-[14px] text-ink-1 outline-none ring-0 placeholder:text-ink-4 focus:border-violet-400/40"
          />
        </form>
      </div>

      <div className="glass-card shrink-0 px-4 py-2 animate-raymes-scale-in">
        <HintBar>
          <Hint label="Providers" keys={<Kbd>⌘,</Kbd>} />
          <Hint label="New chat" keys={<><Kbd>⌘</Kbd><Kbd>N</Kbd></>} />
          {agentStatus === 'running' ? (
            <Hint label="Stop" keys={<Kbd>Esc</Kbd>} />
          ) : (
            <Hint label="Send" keys={<Kbd>↵</Kbd>} />
          )}
          <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
        </HintBar>
      </div>
    </div>
  )
}
