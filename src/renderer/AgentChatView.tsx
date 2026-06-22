import {
  type FormEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react'
import type {
  AgentApprovalDecision,
  AgentInputImage,
  AgentRunEvent,
  Stage,
} from '../shared/agent'
import { defaultModels, inferCapabilities, normalizeProviderModelList } from '../shared/aiProviders'
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
import { ModelPicker } from './ModelPicker'
import { buildAgentPromptFromChat, makeChatId, summarizeChatTitle } from './agentChat/model'

function focusChatInput(): void {
  document.getElementById('ai-chat-input')?.focus()
}

function submitComposerOnEnter(event: ReactKeyboardEvent<HTMLTextAreaElement>): void {
  if (event.key !== 'Enter' || event.shiftKey || event.nativeEvent.isComposing) return
  event.preventDefault()
  event.currentTarget.form?.requestSubmit()
}

const QUESTION_PREFIX_RE = /^(what|why|how|who|when|is|are|can|does)\b/i
const AGENT_TASK_RE =
  /\b(cd|git|clone|mkdir|touch|rm|mv|cp|pnpm|npm|yarn|bun|cargo|go|python|node|run|execute|install|build|test|fix|create|open|move|delete|rename|write|edit|commit|push|pull|list|show|find)\b/i
const LOCAL_PATH_RE = /(?:~\/|\.{1,2}\/|\/Users\/|\bdesktop\/|\bdesktop\\|\bcode\/|\bcode\\)/i
const MACHINE_QUERY_RE =
  /\b(i have|my mac|my computer|my system|installed|applications?|apps?|code editors?|editors?|on this machine|on my machine)\b/i
const ATTACH_SCREEN_KEYS = (
  <>
    <Kbd>⌘</Kbd>
    <Kbd>⇧</Kbd>
    <Kbd>S</Kbd>
  </>
)
const NEW_CHAT_KEYS = (
  <>
    <Kbd>⌘</Kbd>
    <Kbd>N</Kbd>
  </>
)

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

type PendingApproval = Extract<AgentRunEvent, { type: 'approval' }>

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
  const [logsOpen, setLogsOpen] = useState(false)
  const [llmConfig, setLlmConfig] = useState<LlmConfigRecord>({})
  const [modelPickerOpen, setModelPickerOpen] = useState(false)
  const [screenAttachment, setScreenAttachment] = useState<AgentInputImage | null>(null)
  const [screenCapturePending, setScreenCapturePending] = useState(false)
  const [screenCaptureError, setScreenCaptureError] = useState<string | null>(null)
  const [pendingApproval, setPendingApproval] = useState<PendingApproval | null>(null)

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
  const pendingApprovalRef = useRef<PendingApproval | null>(null)
  const ignoredRunIdsRef = useRef<Set<string> | null>(null)
  const submittedBootRef = useRef<string | null>(null)
  const completedRunIdsRef = useRef(new Set<string>())

  const [agentStages, setAgentStages] = useState<Stage[]>([])
  const [agentStreamText, setAgentStreamText] = useState('')
  const [agentStatus, setAgentStatus] = useState<'idle' | 'running' | 'done' | 'error'>('idle')
  const [agentError, setAgentError] = useState<string | null>(null)

  useEffect(() => {
    chatSessionRef.current = chatSession
  }, [chatSession])
  useEffect(() => {
    void window.tezbar
      .getLlmConfig()
      .then(async (config) => {
        setLlmConfig(config)
        // Auto-discover models for CLI-based providers (open models, ollama)
        const provider = config.provider ?? 'ollama'
        const needsDiscovery =
          provider === 'opencode' ||
          provider === 'ollama'
        if (!needsDiscovery) return
        try {
          const discovered = await window.tezbar.listLlmModels(provider)
          if (discovered.length > 0) {
            const existing = config.providerModels?.[provider] ?? []
            const existingIds = new Set(existing.map((m) => m.id))
            const newModels: AiProviderModel[] = discovered
              .filter((id) => !existingIds.has(id))
              .map((id) => ({ id, capabilities: inferCapabilities(id) }))
            if (newModels.length > 0) {
              const updated = normalizeProviderModelList(provider, [...existing, ...newModels])
              await window.tezbar.setLlmConfig({
                providerModels: { ...config.providerModels, [provider]: updated },
              } as LlmConfigRecord)
              setLlmConfig((prev) => ({
                ...prev,
                providerModels: { ...prev.providerModels, [provider]: updated },
              }))
            }
          }
        } catch {
          /* discovery is best-effort */
        }
      })
      .catch(() => {
        /* ignore */
      })
  }, [])
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
      const rows = await window.tezbar.chatList(40)
      setChatHistory(rows)
    } catch {
      /* ignore */
    }
  }, [])

  const stopRun = useCallback((): void => {
    if (!currentAgentRunIdRef.current) return
    const ignoredRuns = ignoredRunIdsRef.current ?? new Set<string>()
    ignoredRuns.add(currentAgentRunIdRef.current)
    ignoredRunIdsRef.current = ignoredRuns
    void window.tezbar.agentCancel()
    currentAgentRunIdRef.current = null
    agentStatusRef.current = 'idle'
    agentErrorRef.current = null
    agentStreamTextRef.current = ''
    agentStagesRef.current = []
    pendingApprovalRef.current = null
    setAgentStatus('idle')
    setAgentError(null)
    setAgentStreamText('')
    setAgentStages([])
    setPendingApproval(null)
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
    setLogsOpen(false)
    setPendingApproval(null)
    pendingApprovalRef.current = null
    focusChatInput()
  }, [stopRun])

  const resolveApproval = useCallback(async (decision: AgentApprovalDecision): Promise<void> => {
    const approval = pendingApprovalRef.current
    if (!approval) return
    pendingApprovalRef.current = null
    setPendingApproval(null)
    try {
      const result = await window.tezbar.agentApprove({
        runId: approval.runId,
        approvalId: approval.approvalId,
        decision,
      })
      if (!result.ok && currentAgentRunIdRef.current === approval.runId) {
        agentErrorRef.current = result.error ?? 'Could not resolve command approval'
        setAgentError(agentErrorRef.current)
      }
    } catch (error) {
      if (currentAgentRunIdRef.current === approval.runId) {
        agentErrorRef.current = error instanceof Error ? error.message : String(error)
        setAgentError(agentErrorRef.current)
      }
    }
  }, [])

  const loadChatSession = useCallback(
    async (id: string): Promise<void> => {
      try {
        const full = await window.tezbar.chatGet(id)
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
        setLogsOpen(false)
        focusChatInput()
      } catch {
        /* ignore */
      }
    },
    [stopRun]
  )

  const deleteChatFromHistory = useCallback(
    async (id: string): Promise<void> => {
      try {
        await window.tezbar.chatDelete(id)
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
    [refreshChatHistory]
  )

  const commitUserTurnAndRun = useCallback(
    async (task: string, image?: AgentInputImage | null): Promise<void> => {
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
        attachments: image
          ? [
              {
                kind: 'image',
                name: 'Active screen',
                mimeType: image.mimeType,
                data: image.data,
                width: image.width,
                height: image.height,
              },
            ]
          : undefined,
        createdAt: now,
      }
      const nextSession: ChatSession = {
        ...session,
        updatedAt: now,
        turns: [...session.turns, userTurn],
      }
      setChatSession(nextSession)
      chatSessionRef.current = nextSession

      void window.tezbar
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
      setLogsOpen(false)

      try {
        const result =
          shouldRunAgent(trimmed) || image
            ? await window.tezbar.agentRun({
                task: buildAgentPromptFromChat(nextSession, trimmed),
                images: image ? [image] : undefined,
              })
            : await window.tezbar.chatRun(nextSession.turns)
        if (!result.ok) {
          const error = formatLlmErrorMessage(result.error || 'Run failed to start')
          agentErrorRef.current = error
          agentStatusRef.current = 'error'
          setAgentError(error)
          setAgentStatus('error')
        } else {
          if (result.runId && !currentAgentRunIdRef.current) {
            currentAgentRunIdRef.current = result.runId
          }
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
    [refreshChatHistory]
  )

  const bootKey =
    boot.kind === 'submit'
      ? `submit:${boot.prompt}`
      : boot.kind === 'resume'
        ? `resume:${boot.sessionId}`
        : boot.kind === 'panel'
          ? 'panel'
          : boot.kind

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

        if (boot.kind === 'screen') {
          startNewChat()
          await attachActiveScreen()
          return
        }

        if (boot.kind === 'resume') {
          await loadChatSession(boot.sessionId)
          return
        }

        const rows = await window.tezbar.chatList(40)
        if (cancelled) return
        setChatHistory(rows)

        if (boot.kind === 'panel') {
          const mostRecent = rows[0]
          if (mostRecent && Date.now() - mostRecent.updatedAt < CHAT_CONTINUATION_WINDOW_MS) {
            const full = await window.tezbar.chatGet(mostRecent.id)
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
          if (mostRecent && Date.now() - mostRecent.updatedAt < CHAT_CONTINUATION_WINDOW_MS) {
            session = await window.tezbar.chatGet(mostRecent.id)
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
    return window.tezbar.onAgentEvent((event: AgentRunEvent) => {
      if (ignoredRunIdsRef.current?.has(event.runId)) {
        return
      }

      if (event.type === 'start') {
        if (agentStatusRef.current !== 'running') return
        if (currentAgentRunIdRef.current && currentAgentRunIdRef.current !== event.runId) {
          return
        }
      } else if (event.runId !== currentAgentRunIdRef.current) {
        return
      }

      switch (event.type) {
        case 'start':
          currentAgentRunIdRef.current = event.runId
          pendingApprovalRef.current = null
          agentStagesRef.current = []
          agentStreamTextRef.current = ''
          agentErrorRef.current = null
          agentStatusRef.current = 'running'
          setRunLogs([])
          setLogsOpen(false)
          setAgentStages([])
          setAgentStreamText('')
          setAgentError(null)
          setPendingApproval(null)
          setAgentStatus('running')
          return
        case 'approval':
          pendingApprovalRef.current = event
          setPendingApproval(event)
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
          pendingApprovalRef.current = null
          setPendingApproval(null)
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
          const hadError = agentStatusRef.current === 'error' || agentErrorRef.current !== null
          const nextStatus: 'done' | 'error' = hadError ? 'error' : 'done'
          agentStatusRef.current = nextStatus
          setAgentStatus(nextStatus)
          currentAgentRunIdRef.current = null
          pendingApprovalRef.current = null
          setPendingApproval(null)

          if (activeSession) {
            const hasPayload = finalText.trim() || finalStages.length > 0 || hadError
            const errorText = hadError
              ? (agentErrorRef.current ?? 'Agent finished without a response.')
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
            void window.tezbar
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
    const onApprovalKeyDown = (event: KeyboardEvent): void => {
      const approval = pendingApprovalRef.current
      if (!approval || event.repeat) return
      let decision: AgentApprovalDecision | null = null
      if (event.key === 'Escape') decision = 'deny'
      if (event.key === 'Enter') {
        decision =
          (event.metaKey || event.ctrlKey) && approval.suggestedRule ? 'always' : 'once'
      }
      if (!decision) return
      event.preventDefault()
      event.stopImmediatePropagation()
      void resolveApproval(decision)
    }
    window.addEventListener('keydown', onApprovalKeyDown, true)
    return () => window.removeEventListener('keydown', onApprovalKeyDown, true)
  }, [resolveApproval])

  useEffect(() => {
    setCommandSurfaceEscapeConsumer(() => {
      if (pendingApprovalRef.current) {
        void resolveApproval('deny')
        return true
      }
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
  }, [resolveApproval, stopRun])

  useEffect(() => {
    requestAnimationFrame(() => focusChatInput())
  }, [])

  async function onSubmitMessage(e: FormEvent): Promise<void> {
    e.preventDefault()
    const text = draft.trim()
    if (!text) return
    const attachment = screenAttachment
    setDraft('')
    setScreenAttachment(null)
    await commitUserTurnAndRun(text, attachment)
  }

  const attachActiveScreen = useCallback(async (): Promise<void> => {
    if (screenCapturePending || agentStatus === 'running') return
    setScreenCapturePending(true)
    setScreenCaptureError(null)
    try {
      setScreenAttachment(await window.tezbar.captureActiveScreen())
      requestAnimationFrame(() => focusChatInput())
    } catch (error) {
      setScreenCaptureError(error instanceof Error ? error.message : 'Screen capture failed')
    } finally {
      setScreenCapturePending(false)
    }
  }, [agentStatus, screenCapturePending])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === 's') {
        event.preventDefault()
        event.stopPropagation()
        void attachActiveScreen()
      }
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [attachActiveScreen])

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
    await window.tezbar.setLlmConfig(patch)
    const next = await window.tezbar.getLlmConfig()
    setLlmConfig(next)
  }

  return (
    <div
      aria-label="AI Chat"
      tabIndex={-1}
      className="flex h-full min-h-0 w-full flex-col outline-none"
    >
      <div className="agent-chat-shell flex min-h-0 flex-1 flex-col overflow-hidden animate-tezbar-scale-in">
        <div className="relative flex h-12 shrink-0 items-center justify-between gap-3 border-b border-white/[0.065] px-4">
          <div className="flex min-w-0 flex-1 items-center gap-2.5">
            <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-white/[0.08] bg-white/[0.045] text-ink-2">
              <svg viewBox="0 0 18 18" aria-hidden="true" className="h-4 w-4">
                <path
                  d="M4 5.5h10M4 9h6M4 12.5h8"
                  fill="none"
                  stroke="currentColor"
                  strokeLinecap="round"
                  strokeWidth="1.35"
                />
              </svg>
            </span>
            <div className="min-w-0">
              <p className="text-[11.5px] font-semibold text-ink-1">Agent</p>
              {chatSession ? (
                <p className="truncate text-[10px] text-ink-4">{chatSession.title}</p>
              ) : (
                <p className="text-[10px] text-ink-4">New conversation</p>
              )}
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-1">
            <ModelPicker
              config={llmConfig}
              open={modelPickerOpen}
              onOpenChange={setModelPickerOpen}
              onSelect={selectProviderModel}
              onConfigure={onOpenSettings}
              onBeforeOpen={() => setHistoryOpen(false)}
            />
            {agentStatus === 'running' ? (
              <button
                type="button"
                className="inline-flex h-7 items-center rounded-lg border border-rose-400/25 bg-rose-500/10 px-2 text-[10px] font-medium text-rose-200 transition hover:bg-rose-500/15"
                onClick={() => {
                  stopRun()
                }}
              >
                Stop
              </button>
            ) : null}
            <button
              type="button"
              aria-label="Chat history"
              title="Chat history"
              className="flex h-7 w-7 items-center justify-center rounded-lg text-ink-4 transition hover:bg-white/[0.06] hover:text-ink-1"
              onClick={() => {
                setModelPickerOpen(false)
                setHistoryOpen((v) => !v)
                if (!historyOpen) void refreshChatHistory()
              }}
              aria-expanded={historyOpen}
            >
              <svg viewBox="0 0 18 18" aria-hidden="true" className="h-4 w-4">
                <path
                  d="M9 4.2a4.8 4.8 0 1 1-4.3 2.7M4.7 3.9v3h3"
                  fill="none"
                  stroke="currentColor"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth="1.35"
                />
                <path
                  d="M9 6.2v3l2 1.2"
                  fill="none"
                  stroke="currentColor"
                  strokeLinecap="round"
                  strokeWidth="1.35"
                />
              </svg>
            </button>
            <button
              type="button"
              aria-label="New chat"
              title="New chat"
              className="flex h-7 w-7 items-center justify-center rounded-lg text-ink-4 transition hover:bg-white/[0.06] hover:text-ink-1"
              onClick={() => {
                startNewChat()
              }}
            >
              <svg viewBox="0 0 18 18" aria-hidden="true" className="h-4 w-4">
                <path
                  d="M9 3.5v11M3.5 9h11"
                  fill="none"
                  stroke="currentColor"
                  strokeLinecap="round"
                  strokeWidth="1.35"
                />
              </svg>
            </button>
            <button
              type="button"
              aria-label="Back"
              title="Back"
              className="flex h-7 w-7 items-center justify-center rounded-lg text-ink-4 transition hover:bg-white/[0.06] hover:text-ink-1"
              onClick={() => {
                void onBack()
              }}
            >
              <svg viewBox="0 0 18 18" aria-hidden="true" className="h-4 w-4">
                <path
                  d="m10.8 4.5-4.5 4.5 4.5 4.5"
                  fill="none"
                  stroke="currentColor"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth="1.35"
                />
              </svg>
            </button>
          </div>

          {historyOpen ? (
            <div className="tezbar-popover absolute right-3 top-11 z-40 w-[320px] overflow-hidden p-1.5">
              <div className="flex items-center justify-between px-3 pb-1.5 pt-1 text-[10px] font-semibold uppercase tracking-[0.14em] text-ink-4">
                <span>Past chats</span>
                {chatHistory.length > 0 ? (
                  <button
                    type="button"
                    className="text-[10px] text-ink-4 transition hover:text-rose-300"
                    onClick={() => {
                      void window.tezbar.chatClear().then(() => {
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
                              : 'hover:bg-white/[0.04] text-ink-2 hover:text-ink-1'
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
                            className="mt-0.5 shrink-0 rounded-tezbar-chip border border-transparent px-1.5 py-0.5 text-[10px] text-ink-4 opacity-0 transition hover:border-rose-400/40 hover:text-rose-300 group-hover:opacity-100"
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
          className="agent-chat-thread flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-4 py-5"
        >
          {chatSession && chatSession.turns.length > 0
            ? chatSession.turns.map((turn) =>
                turn.role === 'user' ? (
                  <div key={turn.id} className="flex justify-end">
                    <div className="max-w-[88%] overflow-hidden rounded-tezbar-row border border-white/10 bg-white/[0.055] text-[13.5px] leading-[1.5] text-ink-1">
                      {turn.attachments?.map((attachment, index) =>
                        attachment.data ? (
                          <img
                            key={`${attachment.name}-${index}`}
                            src={`data:${attachment.mimeType};base64,${attachment.data}`}
                            alt={attachment.name}
                            className="max-h-44 w-full object-cover"
                          />
                        ) : (
                          <div
                            key={`${attachment.name}-${index}`}
                            className="flex items-center gap-2 border-b border-white/[0.06] px-3 py-2 text-[11px] text-ink-3"
                          >
                            <span className="h-2 w-2 rounded-sm bg-sky-300/80" />
                            {attachment.name}
                          </div>
                        )
                      )}
                      <p className="px-3 py-2">{turn.text}</p>
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
                )
              )
            : null}

          {agentStatus === 'running' || agentStages.length > 0 || agentStreamText ? (
            <div className="flex flex-col gap-1.5">
              {agentStages.length > 0 ? <AgentStageList stages={agentStages} /> : null}
              {agentStreamText ? (
                <Markdown text={agentStreamText} streaming={agentStatus === 'running'} />
              ) : agentStatus === 'running' ? (
                <p className="tezbar-thinking flex items-center gap-2 text-[12px] text-ink-3">
                  <span className="inline-flex gap-1">
                    <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-ink-3" />
                    <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-ink-3 [animation-delay:120ms]" />
                    <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-ink-3 [animation-delay:240ms]" />
                  </span>
                  Working
                </p>
              ) : null}
            </div>
          ) : null}

          {agentError ? (
            <div
              className="flex items-start gap-2 rounded-xl border border-rose-400/15 bg-rose-500/[0.055] px-3 py-2 text-[11.5px] text-rose-200"
              role="alert"
            >
              <span className="mt-1 h-1.5 w-1.5 shrink-0 rounded-full bg-rose-300" />
              <span className="min-w-0 flex-1 whitespace-pre-wrap">{agentError}</span>
            </div>
          ) : null}

          {runLogs.length > 0 ? (
            <div className="rounded-tezbar-row border border-amber-400/20 bg-amber-500/5">
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
              <div className="flex flex-1 items-center justify-center px-4 py-10 text-center">
                <div className="max-w-[440px] space-y-2 text-ink-3">
                  <span className="mx-auto flex h-9 w-9 items-center justify-center rounded-xl border border-white/[0.08] bg-white/[0.035] text-ink-3">
                    <svg viewBox="0 0 18 18" aria-hidden="true" className="h-4 w-4">
                      <path
                        d="M4 5.5h10M4 9h6M4 12.5h8"
                        fill="none"
                        stroke="currentColor"
                        strokeLinecap="round"
                        strokeWidth="1.35"
                      />
                    </svg>
                  </span>
                  <p className="pt-1 text-[13px] font-medium text-ink-2">What are we working on?</p>
                  <p className="text-[11px] text-ink-4">Ask a question or hand the agent a task.</p>
                </div>
              </div>
            ) : null
          ) : null}
        </div>

        {pendingApproval ? (
          <div className="shrink-0 px-4 pb-1.5">
            <div className="mx-auto max-w-[780px] overflow-hidden rounded-2xl border border-amber-300/20 bg-amber-200/[0.055] shadow-[0_12px_36px_rgba(0,0,0,0.22)]">
              <div className="flex items-center gap-3 px-4 py-3">
                <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-xl border border-amber-200/15 bg-amber-200/[0.07] text-amber-100/80">
                  <svg viewBox="0 0 18 18" aria-hidden="true" className="h-4 w-4">
                    <path
                      d="m5 6 3 3-3 3m5.5 0H14"
                      fill="none"
                      stroke="currentColor"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth="1.4"
                    />
                  </svg>
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block text-[10px] font-medium uppercase tracking-[0.12em] text-amber-100/55">
                    {pendingApproval.title || 'Run command'}
                  </span>
                  <code className="mt-0.5 block truncate font-mono text-[12px] text-ink-1">
                    {pendingApproval.command}
                  </code>
                </span>
                <button
                  type="button"
                  className="hidden shrink-0 items-center gap-2 rounded-xl bg-ink-1 px-3 py-2 text-[11px] font-semibold text-glass-shell transition hover:bg-white sm:inline-flex"
                  onClick={() => void resolveApproval('once')}
                >
                  Run once
                  <span className="font-mono text-[10px] opacity-60">↵</span>
                </button>
              </div>
              <div className="flex items-center gap-1.5 border-t border-white/[0.06] px-3 py-2">
                <button
                  type="button"
                  className="rounded-lg px-2 py-1.5 text-[10.5px] text-ink-3 transition hover:bg-white/[0.055] hover:text-ink-1"
                  onClick={() => void resolveApproval('deny')}
                >
                  Deny <span className="ml-1 font-mono text-[9px] text-ink-4">esc</span>
                </button>
                {pendingApproval.suggestedRule ? (
                  <button
                    type="button"
                    className="rounded-lg px-2 py-1.5 text-[10.5px] text-ink-3 transition hover:bg-white/[0.055] hover:text-ink-1"
                    onClick={() => void resolveApproval('always')}
                  >
                    Always allow {pendingApproval.suggestedRule}
                    <span className="ml-1.5 font-mono text-[9px] text-ink-4">⌘↵</span>
                  </button>
                ) : null}
                <button
                  type="button"
                  className="ml-auto rounded-lg bg-ink-1 px-2.5 py-1.5 text-[10.5px] font-semibold text-glass-shell sm:hidden"
                  onClick={() => void resolveApproval('once')}
                >
                  Run once ↵
                </button>
                <span className="ml-auto hidden text-[10px] text-ink-4 sm:block">
                  Press <span className="font-mono text-ink-2">Enter</span> to run
                </span>
              </div>
            </div>
          </div>
        ) : null}

        <form className="shrink-0 px-4 pb-3 pt-2" onSubmit={(ev) => void onSubmitMessage(ev)}>
          <div className="agent-composer mx-auto max-w-[780px] overflow-hidden rounded-2xl border border-white/[0.11] bg-white/[0.045] shadow-[0_12px_35px_rgba(0,0,0,0.2)] transition focus-within:border-white/[0.2] focus-within:bg-white/[0.055]">
            {screenAttachment ? (
              <div className="flex items-center gap-3 border-b border-white/[0.07] p-2.5 pr-3">
                <div className="relative shrink-0 overflow-hidden rounded-lg border border-white/10">
                  <img
                    src={`data:${screenAttachment.mimeType};base64,${screenAttachment.data}`}
                    alt="Attached active screen"
                    className="h-12 w-20 object-cover"
                  />
                  <span className="absolute bottom-1 left-1 rounded bg-black/70 px-1 py-0.5 text-[8px] font-medium text-white/80">
                    SCREEN
                  </span>
                </div>
                <span className="min-w-0 flex-1">
                  <span className="block text-[11.5px] font-medium text-ink-1">Active screen</span>
                  <span className="mt-0.5 block text-[9.5px] text-ink-4">
                    Attached to your next message
                  </span>
                </span>
                <button
                  type="button"
                  aria-label="Remove attached screen"
                  title="Remove attachment"
                  className="flex h-7 w-7 items-center justify-center rounded-lg text-ink-4 transition hover:bg-white/[0.06] hover:text-rose-200"
                  onClick={() => setScreenAttachment(null)}
                >
                  <svg viewBox="0 0 16 16" aria-hidden="true" className="h-3.5 w-3.5">
                    <path
                      d="m4 4 8 8m0-8-8 8"
                      stroke="currentColor"
                      strokeLinecap="round"
                      strokeWidth="1.35"
                    />
                  </svg>
                </button>
              </div>
            ) : null}
            {screenCaptureError ? (
              <p
                className="border-b border-rose-400/10 px-3 py-2 text-[10.5px] text-rose-300"
                role="alert"
              >
                {screenCaptureError}
              </p>
            ) : null}
            <div className="flex items-end gap-1.5 p-2">
              <button
                type="button"
                aria-label="Attach active screen"
                title="Attach active screen (⌘⇧S)"
                disabled={screenCapturePending || agentStatus === 'running'}
                className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-ink-4 transition hover:bg-white/[0.065] hover:text-ink-1 disabled:cursor-not-allowed disabled:opacity-40"
                onClick={() => void attachActiveScreen()}
              >
                {screenCapturePending ? (
                  <span className="h-3.5 w-3.5 animate-spin rounded-full border border-white/20 border-t-white/70" />
                ) : (
                  <svg viewBox="0 0 18 18" aria-hidden="true" className="h-4 w-4">
                    <path
                      d="M5.2 9.5 9 5.7a2.3 2.3 0 0 1 3.2 3.2l-4.5 4.5a3.2 3.2 0 0 1-4.5-4.5l4.7-4.7"
                      fill="none"
                      stroke="currentColor"
                      strokeLinecap="round"
                      strokeWidth="1.35"
                    />
                  </svg>
                )}
              </button>
              <textarea
                id="ai-chat-input"
                aria-label="Message Agent"
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={submitComposerOnEnter}
                placeholder="Message Agent"
                autoComplete="off"
                spellCheck={false}
                rows={1}
                className="max-h-28 min-h-8 min-w-0 flex-1 resize-none bg-transparent px-1.5 py-1.5 text-[13px] leading-5 text-ink-1 outline-none placeholder:text-ink-4"
              />
              <button
                type="submit"
                aria-label={agentStatus === 'running' ? 'Agent is working' : 'Send message'}
                disabled={!draft.trim() || agentStatus === 'running'}
                className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-ink-1 text-glass-shell transition hover:bg-white disabled:cursor-not-allowed disabled:bg-white/[0.07] disabled:text-ink-4"
              >
                <svg viewBox="0 0 18 18" aria-hidden="true" className="h-4 w-4">
                  <path
                    d="M9 13.5v-9m-3.5 3L9 4l3.5 3.5"
                    fill="none"
                    stroke="currentColor"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth="1.45"
                  />
                </svg>
              </button>
            </div>
          </div>
        </form>
      </div>

      <div className="shrink-0 px-4 py-2 animate-tezbar-scale-in">
        <HintBar>
          <Hint label="Providers" keys={<Kbd>⌘,</Kbd>} />
          <Hint label="Attach screen" keys={ATTACH_SCREEN_KEYS} />
          <Hint label="New chat" keys={NEW_CHAT_KEYS} />
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
