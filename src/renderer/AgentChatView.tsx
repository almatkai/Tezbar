import { Chat, useChat } from '@ai-sdk/react'
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from 'react'
import type { AgentApprovalDecision, AgentInputImage } from '../shared/agent'
import type { AiChatBoot } from '../shared/aiChatSurface'
import { RAYMES_AI_NEW_CHAT_EVENT } from '../shared/aiChatSurface'
import { defaultModels, normalizeProviderModelList, providerTitle } from '../shared/aiProviders'
import {
  type ActiveAgentRunInfo,
  type ChatResponseMeta,
  type ChatSession,
  type ChatSessionSummary,
  type ChatTurn,
} from '../shared/chat'
import { formatLlmErrorMessage } from '../shared/llmErrors'
import type { LlmConfigRecord, ProviderId } from '../shared/llmConfig'
import { ModelPicker } from './ModelPicker'
import { ApprovalModePicker } from './agentChat/ApprovalModePicker'
import { setCommandSurfaceEscapeConsumer } from './escapeGate'
import { Hint, Kbd, cx } from './ui/primitives'
import {
  CommandSurfaceAction,
  CommandSurfaceBadge,
  CommandSurfaceFooter,
  CommandSurfaceHeader,
  type CommandSurfaceTaskActions,
} from './ui/CommandSurface'
import { ChatSquircle } from './launcherIcons'
import { heuristicChatMeta } from '../shared/chatMeta'
import { AgentMessage } from './agentChat/messages'
import { readCachedChatHistory, writeCachedChatHistory } from './agentChat/historyCache'
import { historyPlaceholder, makeChatId } from './agentChat/model'
import { isNearScrollBottom } from './agentChat/streaming'
import {
  agentMessageApproval,
  agentMessageError,
  agentMessageStages,
  agentMessageText,
  agentMessageToTurn,
  chatTurnToAgentMessage,
  createNativeAgentTransport,
  finalizeAgentMessage,
  type AgentUIMessage,
} from './agentChat/transport'

// A retained SDK instance owns each conversation, including hidden/native work.
// Selecting a different conversation only changes the visible subscription.
type AgentConversation = {
  chat: Chat<AgentUIMessage>
  session: ChatSession | null
  responseMeta: ChatResponseMeta
  workingDirectory?: string
  resolvedApproval: string | null
  approvalBusy: boolean
  lastError?: string
  explicitlyStopped: boolean
  preparing: boolean
  submission: string | null
  pendingWrites: number
  unsavedTurns: Set<string>
  unsubscribe: () => void
}

const smallButton =
  'rounded-lg px-2.5 py-1.5 text-[11px] text-ink-3 transition hover:bg-white/[0.06] hover:text-ink-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-40'
function focusInput(): void {
  document.getElementById('ai-chat-input')?.focus()
}
function responseMeta(config: LlmConfigRecord): ChatResponseMeta {
  const provider = config.taskProviderOverrides?.chat ?? config.provider ?? 'ollama'
  return {
    provider,
    providerTitle: providerTitle(provider, config),
    model:
      config.taskModelOverrides?.chat ??
      config.providerSelectedModels?.[provider] ??
      config.model ??
      'default',
  }
}
function enterToSubmit(event: KeyboardEvent<HTMLTextAreaElement>): void {
  if (event.key !== 'Enter' || event.shiftKey || event.nativeEvent.isComposing) return
  event.preventDefault()
  event.currentTarget.form?.requestSubmit()
}

export default function AgentChatView({
  boot,
  onBack,
  onOpenSettings,
  taskActions,
  active = true,
  onActiveRunChange,
}: {
  boot: AiChatBoot
  onBack: () => void
  onOpenSettings: () => void
  taskActions?: CommandSurfaceTaskActions
  active?: boolean
  onActiveRunChange?: (run: ActiveAgentRunInfo | null) => void
}): JSX.Element {
  const [session, setSession] = useState<ChatSession | null>(null)
  const [workingDirectory, setWorkingDirectory] = useState<string | undefined>(
    boot.kind === 'submit' || boot.kind === 'newChat' ? boot.workingDirectory : undefined
  )
  const [history, setHistory] = useState<ChatSessionSummary[]>(() => readCachedChatHistory() ?? [])
  const [historyLoading, setHistoryLoading] = useState(() => readCachedChatHistory() === null)
  const [historyOpen, setHistoryOpen] = useState(false)
  const [config, setConfig] = useState<LlmConfigRecord>({})
  const [pickerOpen, setPickerOpen] = useState(false)
  const [savingModel, setSavingModel] = useState(false)
  const [draft, setDraft] = useState('')
  const [attachment, setAttachment] = useState<AgentInputImage | null>(null)
  const [capturing, setCapturing] = useState(false)
  const [preparing, setPreparing] = useState(false)
  const [uiError, setUiError] = useState<string | null>(null)
  const [saveWarning, setSaveWarning] = useState<string | null>(null)
  const [following, setFollowing] = useState(true)
  const [elapsed, setElapsed] = useState(0)

  const configRef = useRef<LlmConfigRecord>({})
  const modelSaveRef = useRef<Promise<void> | null>(null)
  const viewRevision = useRef(0)
  const mounted = useRef(true)
  const threadRef = useRef<HTMLDivElement>(null)
  const followRef = useRef(true)
  const submittedBoot = useRef<string | null>(null)
  const captureRequest = useRef<object | null>(null)
  const historyRequest = useRef(0)
  const conversations = useRef(new Set<AgentConversation>())
  const selectedConversation = useRef<AgentConversation | null>(null)
  const [conversationRevision, publishConversations] = useState(0)
  const releaseInactiveConversation = useCallback((owner: AgentConversation): void => {
    if (
      selectedConversation.current === owner ||
      owner.preparing ||
      owner.submission ||
      owner.pendingWrites ||
      owner.unsavedTurns.size > 0 ||
      owner.chat.status === 'streaming' ||
      owner.chat.status === 'submitted'
    )
      return
    owner.unsubscribe()
    conversations.current.delete(owner)
    void owner.chat.dispose()
  }, [])
  const notifyConversation = useCallback(
    (owner: AgentConversation): void => {
      if (!mounted.current) return
      if (selectedConversation.current === owner) {
        setSession(owner.session)
        setPreparing(owner.preparing)
      }
      releaseInactiveConversation(owner)
      publishConversations((revision) => revision + 1)
    },
    [releaseInactiveConversation]
  )

  const refreshHistory = useCallback(async (): Promise<ChatSessionSummary[]> => {
    const request = ++historyRequest.current
    const revision = viewRevision.current
    try {
      const rows = await window.tezbar.chatList(40)
      if (mounted.current && request === historyRequest.current) {
        writeCachedChatHistory(rows)
        setHistory(rows)
        setHistoryLoading(false)
      }
      return rows
    } catch {
      if (
        mounted.current &&
        request === historyRequest.current &&
        revision === viewRevision.current
      ) {
        setHistoryLoading(false)
        setSaveWarning(
          'Chat history could not be loaded. Your current response is still available.'
        )
      }
      return readCachedChatHistory() ?? []
    }
  }, [])

  const persistTurn = useCallback(
    async (turn: ChatTurn, owner: AgentConversation): Promise<void> => {
      const current = owner.session
      if (!current) return
      owner.pendingWrites++
      const next: ChatSession = {
        ...current,
        updatedAt: turn.createdAt,
        turns: current.turns.some((item) => item.id === turn.id)
          ? current.turns.map((item) => (item.id === turn.id ? turn : item))
          : [...current.turns, turn],
      }
      owner.session = next
      notifyConversation(owner)
      try {
        const result = await window.tezbar.chatAppend({
          session: {
            id: next.id,
            title: next.title,
            icon: next.icon,
            createdAt: next.createdAt,
            updatedAt: next.updatedAt,
            workingDirectory: next.workingDirectory,
          },
          turn,
        })
        if (!result.ok) throw new Error(result.error || 'Could not save chat')
        owner.unsavedTurns.delete(turn.id)
        if (mounted.current) void refreshHistory()
      } catch {
        owner.unsavedTurns.add(turn.id)
        if (mounted.current && selectedConversation.current === owner)
          setSaveWarning(
            'This conversation could not be saved. Copy any response you want to keep before closing it.'
          )
      } finally {
        owner.pendingWrites--
        notifyConversation(owner)
      }
    },
    [notifyConversation, refreshHistory]
  )

  const generateMeta = useCallback(
    (owner: AgentConversation, prompt: string, answer?: string): void => {
      const current = owner.session
      if (!mounted.current || !current) return
      void window.tezbar
        .chatGenerateMeta({ id: current.id, prompt, answer })
        .then((meta) => {
          if (!mounted.current || !meta?.title || !owner.session) return
          owner.session = {
            ...owner.session,
            title: meta.title,
            icon: meta.icon || owner.session.icon,
          }
          notifyConversation(owner)
          void refreshHistory()
        })
        .catch(() => {})
    },
    [notifyConversation, refreshHistory]
  )

  const createConversation = useCallback(
    (initial: ChatSession | null = null, directory?: string): AgentConversation => {
      let owner: AgentConversation
      const transport = createNativeAgentTransport(window.tezbar, () => {
        if (!owner.session) throw new Error('No conversation is active.')
        return { session: owner.session, responseMeta: owner.responseMeta }
      })
      const chat = new Chat<AgentUIMessage>({
        transport,
        messages: initial?.turns.map((turn) => chatTurnToAgentMessage(turn, initial.id)),
        onError(error) {
          owner.lastError = error.message
        },
        onFinish({ message, isAbort, isError }) {
          const failure =
            isAbort && owner.explicitlyStopped
              ? 'Stopped by you.'
              : isError
                ? agentMessageError(message) || owner.lastError || 'The agent request failed.'
                : undefined
          const final = finalizeAgentMessage(message, failure)
          owner.chat.messages = owner.chat.messages.map((item) =>
            item.id === message.id ? final : item
          )
          // Always persist to this owner, never to whichever session is on screen.
          void persistTurn(agentMessageToTurn(final), owner)
          const current = owner.session
          if (
            !isAbort &&
            !isError &&
            current &&
            current.turns.length <= 2 &&
            selectedConversation.current === owner
          ) {
            const user = current.turns.find((turn) => turn.role === 'user')
            if (user?.text) generateMeta(owner, user.text, agentMessageText(final) || undefined)
          }
        },
      })
      owner = {
        chat,
        session: initial,
        workingDirectory: initial?.workingDirectory ?? directory,
        resolvedApproval: null,
        approvalBusy: false,
        responseMeta: responseMeta(configRef.current),
        explicitlyStopped: false,
        preparing: false,
        submission: null,
        pendingWrites: 0,
        unsavedTurns: new Set(),
        unsubscribe: () => {},
      }
      const unsubscribes = [
        owner.chat['~registerMessagesCallback'](() => notifyConversation(owner), 32),
        owner.chat['~registerStatusCallback'](() => notifyConversation(owner)),
        owner.chat['~registerErrorCallback'](() => notifyConversation(owner)),
      ]
      owner.unsubscribe = () => unsubscribes.forEach((unsubscribe) => unsubscribe())
      conversations.current.add(owner)
      return owner
    },
    [generateMeta, notifyConversation, persistTurn]
  )
  const [owner, setOwner] = useState(() => createConversation())
  if (!selectedConversation.current) selectedConversation.current = owner
  const chat = useChat<AgentUIMessage>({ chat: owner.chat, throttle: 32 })
  const { stop, status } = chat
  const selectConversation = useCallback(
    (next: AgentConversation): void => {
      const previous = selectedConversation.current
      selectedConversation.current = next
      if (previous && previous !== next) releaseInactiveConversation(previous)
      setOwner(next)
      setSession(next.session)
      setWorkingDirectory(next.workingDirectory)
      setPreparing(next.preparing)
    },
    [releaseInactiveConversation]
  )
  const fallbackMessages = useMemo(() => {
    if (chat.messages.length > 0 || !session?.turns?.length) return []
    return session.turns.map((turn) => chatTurnToAgentMessage(turn, session.id))
  }, [chat.messages.length, session])
  const activeMessages = chat.messages.length > 0 ? chat.messages : fallbackMessages
  const busy = preparing || status === 'submitted' || status === 'streaming'
  const lastMessage = activeMessages.at(-1)
  const activeMessage = lastMessage?.role === 'assistant' ? lastMessage : undefined
  const requestedApproval = activeMessage ? agentMessageApproval(activeMessage) : null
  const approval =
    requestedApproval?.approvalId !== owner.resolvedApproval ? requestedApproval : null
  const approvalBusy = owner.approvalBusy
  const lastStage = activeMessage ? agentMessageStages(activeMessage).at(-1) : undefined
  const statusLabel = approval
    ? 'Waiting for your approval'
    : preparing || chat.status === 'submitted'
      ? 'Starting agent…'
      : chat.status === 'streaming'
        ? lastStage?.status === 'running'
          ? lastStage.label
          : 'Generating…'
        : chat.status === 'error' || (activeMessage && agentMessageError(activeMessage))
          ? 'Run failed'
          : 'Ready'

  const stopRun = useCallback(async (): Promise<void> => {
    owner.explicitlyStopped = true
    viewRevision.current++
    owner.submission = null
    owner.preparing = false
    notifyConversation(owner)
    await stop()
    if (mounted.current) focusInput()
  }, [stop, owner, notifyConversation])

  const clearView = useCallback((): void => {
    viewRevision.current++
    setDraft('')
    setAttachment(null)
    setUiError(null)
    setSaveWarning(null)
    setHistoryOpen(false)
    captureRequest.current = null
    setCapturing(false)
    followRef.current = true
    setFollowing(true)
  }, [])
  const resetChat = useCallback(
    (directory?: string): void => {
      clearView()
      selectConversation(createConversation(null, directory))
      if (mounted.current) focusInput()
    },
    [clearView, selectConversation, createConversation]
  )

  const loadSession = useCallback(
    async (id: string): Promise<void> => {
      clearView()
      // Live owners are authoritative, including pending approvals not in SQLite.
      const existing = [...conversations.current].find((item) => item.session?.id === id)
      if (existing) {
        selectConversation(existing)
        focusInput()
        return
      }
      const revision = viewRevision.current
      try {
        const full = await window.tezbar.chatGet(id)
        if (!mounted.current || viewRevision.current !== revision) return
        if (!full) throw new Error('This conversation no longer exists.')
        selectConversation(createConversation(full))
        focusInput()
      } catch (error) {
        if (mounted.current && viewRevision.current === revision)
          setUiError(error instanceof Error ? error.message : 'Could not open conversation.')
      }
    },
    [clearView, selectConversation, createConversation]
  )

  const send = useCallback(
    async (input: string, image?: AgentInputImage | null): Promise<void> => {
      const owner = selectedConversation.current!
      const text = input.trim()
      if (
        !text ||
        owner.submission ||
        captureRequest.current ||
        owner.chat.status === 'streaming' ||
        owner.chat.status === 'submitted'
      )
        return
      owner.explicitlyStopped = false
      const submission = makeChatId()
      const revision = viewRevision.current
      owner.submission = submission
      owner.preparing = true
      notifyConversation(owner)
      setUiError(null)
      owner.resolvedApproval = null
      owner.lastError = undefined
      try {
        await modelSaveRef.current
        const fresh = await window.tezbar.getLlmConfig()
        if (!mounted.current || owner.submission !== submission) return
        configRef.current = fresh
        setConfig(fresh)
        owner.responseMeta = responseMeta(fresh)
        const now = Date.now()
        const meta = heuristicChatMeta(text)
        const isNewSession = !owner.session || owner.session.turns.length === 0
        const current: ChatSession = owner.session
          ? {
              ...owner.session,
              title: owner.session.turns.length === 0 ? meta.title : owner.session.title,
              icon: owner.session.turns.length === 0 ? meta.icon : owner.session.icon || meta.icon,
            }
          : {
              id: makeChatId(),
              title: meta.title,
              icon: meta.icon,
              createdAt: now,
              updatedAt: now,
              workingDirectory: owner.workingDirectory,
              turns: [],
            }
        owner.session = current
        notifyConversation(owner)
        if (isNewSession) generateMeta(owner, text)
        const userTurn: ChatTurn = {
          id: makeChatId(),
          role: 'user',
          text,
          createdAt: now,
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
        }
        await persistTurn(userTurn, owner)
        if (!mounted.current || owner.submission !== submission) return
        if (selectedConversation.current === owner) {
          setDraft('')
          setAttachment(null)
          followRef.current = true
          setFollowing(true)
        }
        owner.preparing = false
        notifyConversation(owner)
        await owner.chat.sendMessage(chatTurnToAgentMessage(userTurn, current.id))
      } catch (error) {
        if (mounted.current && revision === viewRevision.current)
          setUiError(
            formatLlmErrorMessage(
              error instanceof Error ? error.message : 'The agent could not start.'
            )
          )
      } finally {
        if (owner.submission === submission) {
          owner.submission = null
          owner.preparing = false
          notifyConversation(owner)
        }
      }
    },
    [generateMeta, notifyConversation, persistTurn]
  )

  const retry = useCallback(async (): Promise<void> => {
    if (busy || owner.submission) return
    const submission = makeChatId()
    const revision = viewRevision.current
    owner.submission = submission
    owner.preparing = true
    notifyConversation(owner)
    try {
      await modelSaveRef.current
      const fresh = await window.tezbar.getLlmConfig()
      if (!mounted.current || owner.submission !== submission) return
      configRef.current = fresh
      setConfig(fresh)
      owner.responseMeta = responseMeta(fresh)
      owner.lastError = undefined
      owner.resolvedApproval = null
      if (selectedConversation.current === owner) setUiError(null)
      owner.preparing = false
      notifyConversation(owner)
      await owner.chat.regenerate({ messageId: activeMessage?.id })
    } catch (error) {
      if (mounted.current && revision === viewRevision.current)
        setUiError(
          formatLlmErrorMessage(error instanceof Error ? error.message : 'Could not retry.')
        )
    } finally {
      if (owner.submission === submission) {
        owner.submission = null
        owner.preparing = false
        notifyConversation(owner)
      }
    }
  }, [busy, owner, activeMessage?.id, notifyConversation])

  const attachScreen = useCallback(async (): Promise<void> => {
    const owner = selectedConversation.current!
    if (
      owner.preparing ||
      owner.chat.status === 'streaming' ||
      owner.chat.status === 'submitted' ||
      captureRequest.current
    )
      return
    const request = {}
    const revision = viewRevision.current
    captureRequest.current = request
    const isCurrent = (): boolean =>
      mounted.current &&
      captureRequest.current === request &&
      revision === viewRevision.current &&
      selectedConversation.current === owner
    setCapturing(true)
    setUiError(null)
    try {
      const image = await window.tezbar.captureActiveScreen()
      if (!isCurrent()) return
      setAttachment(image)
      focusInput()
    } catch (error) {
      if (isCurrent())
        setUiError(
          error instanceof Error
            ? error.message
            : 'Screen capture failed. Check Screen Recording permission.'
        )
    } finally {
      if (isCurrent()) {
        captureRequest.current = null
        setCapturing(false)
      }
    }
  }, [])

  const resolveApproval = useCallback(
    async (decision: AgentApprovalDecision): Promise<void> => {
      if (!approval || owner.approvalBusy) return
      owner.approvalBusy = true
      notifyConversation(owner)
      setUiError(null)
      try {
        const result = await window.tezbar.agentApprove({
          runId: approval.runId,
          approvalId: approval.approvalId,
          decision,
        })
        if (!result.ok) throw new Error(result.error || 'Could not resolve this approval.')
        owner.resolvedApproval = approval.approvalId
      } catch (error) {
        if (mounted.current && selectedConversation.current === owner)
          setUiError(error instanceof Error ? error.message : 'Could not resolve this approval.')
      } finally {
        owner.approvalBusy = false
        notifyConversation(owner)
      }
    },
    [approval, owner, notifyConversation]
  )

  const selectModel = useCallback(async (provider: ProviderId, modelId: string): Promise<void> => {
    const before = configRef.current
    const patch: LlmConfigRecord = {
      provider,
      model: modelId,
      providerModels: {
        ...before.providerModels,
        [provider]: normalizeProviderModelList(
          provider,
          before.providerModels?.[provider] ?? defaultModels(provider)
        ),
      },
      providerSelectedModels: { ...before.providerSelectedModels, [provider]: modelId },
      taskProviderOverrides: { ...before.taskProviderOverrides, chat: provider },
      taskModelOverrides: { ...before.taskModelOverrides, chat: modelId },
    }
    setSavingModel(true)
    setPickerOpen(false)
    const saving = (async () => {
      const result = await window.tezbar.setLlmConfig(patch)
      if (result && !result.ok)
        throw new Error(result.error || 'Could not save the selected model.')
      const fresh = await window.tezbar.getLlmConfig()
      configRef.current = fresh
      setConfig(fresh)
    })()
    modelSaveRef.current = saving
    try {
      await saving
    } catch (error) {
      setUiError(error instanceof Error ? error.message : 'Could not save the model.')
    } finally {
      setSavingModel(false)
      if (modelSaveRef.current === saving) modelSaveRef.current = null
    }
  }, [])

  const bootKey = JSON.stringify([
    boot.kind,
    boot.nonce,
    boot.kind === 'submit' ? boot.prompt : undefined,
    boot.kind === 'submit' || boot.kind === 'newChat' ? boot.workingDirectory : undefined,
    boot.kind === 'resume' ? boot.sessionId : undefined,
  ])
  useEffect(() => {
    // Navigation is synchronous. History/config hydration never gets to reset
    // an interactive composer or a conversation submitted while it was loading.
    if (boot.kind === 'resume') {
      void loadSession(boot.sessionId)
    } else {
      resetChat(
        boot.kind === 'submit' || boot.kind === 'newChat' ? boot.workingDirectory : undefined
      )
      if (boot.kind === 'screen') void attachScreen()
      if (boot.kind === 'submit' && submittedBoot.current !== bootKey) {
        submittedBoot.current = bootKey
        void send(boot.prompt)
      }
    }
    const revision = viewRevision.current
    void refreshHistory()
    void window.tezbar
      .getLlmConfig()
      .then((fresh) => {
        if (!mounted.current || revision !== viewRevision.current) return
        configRef.current = fresh
        setConfig(fresh)
      })
      .catch((error) => {
        if (mounted.current && revision === viewRevision.current)
          setUiError(error instanceof Error ? error.message : 'Could not initialize the agent.')
      })
    // Boot is an explicit navigation event, not a render-time object identity.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [bootKey])

  useEffect(() => {
    const reload = (): void => {
      void window.tezbar
        .getLlmConfig()
        .then((fresh) => {
          if (mounted.current) {
            configRef.current = fresh
            setConfig(fresh)
          }
        })
        .catch(() => {})
    }
    return window.tezbar.onLlmConfigChanged(reload)
  }, [])
  useEffect(() => {
    if (!busy) return
    const started = Date.now()
    setElapsed(0)
    const timer = setInterval(() => setElapsed(Math.floor((Date.now() - started) / 1000)), 1000)
    return () => clearInterval(timer)
  }, [busy])
  useEffect(() => {
    if (!followRef.current) return
    const frame = requestAnimationFrame(() => {
      const el = threadRef.current
      if (el && followRef.current) el.scrollTop = el.scrollHeight
    })
    return () => cancelAnimationFrame(frame)
  }, [chat.messages, busy, approval])
  useEffect(() => {
    const textarea = document.getElementById('ai-chat-input')
    if (textarea) {
      textarea.style.height = 'auto'
      textarea.style.height = `${Math.min(120, Math.max(28, textarea.scrollHeight))}px`
    }
  }, [draft])
  useEffect(() => {
    const newChat = (): void => {
      void resetChat()
    }
    window.addEventListener(RAYMES_AI_NEW_CHAT_EVENT, newChat)
    return () => window.removeEventListener(RAYMES_AI_NEW_CHAT_EVENT, newChat)
  }, [resetChat])
  useEffect(() => {
    if (!active) {
      // Hiding keeps chat owners alive but invalidates view-only async work.
      viewRevision.current++
      captureRequest.current = null
      setCapturing(false)
    } else if (mounted.current) {
      focusInput()
    }
  }, [active])
  useEffect(() => {
    if (!active) return
    const onKey = (event: globalThis.KeyboardEvent): void => {
      if (approval && event.key === 'Enter') {
        event.preventDefault()
        event.stopImmediatePropagation()
        void resolveApproval(event.metaKey || event.ctrlKey ? 'always' : 'once')
      } else if (
        (event.metaKey || event.ctrlKey) &&
        !event.altKey &&
        !event.shiftKey &&
        event.key === '.' &&
        busy
      ) {
        event.preventDefault()
        event.stopImmediatePropagation()
        void stopRun()
      } else if (
        (event.metaKey || event.ctrlKey) &&
        event.shiftKey &&
        event.key.toLowerCase() === 's'
      ) {
        event.preventDefault()
        event.stopImmediatePropagation()
        void attachScreen()
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [active, approval, resolveApproval, attachScreen, busy, stopRun])
  useEffect(() => {
    if (!active) return
    setCommandSurfaceEscapeConsumer(() => {
      if (historyOpen) {
        setHistoryOpen(false)
        return true
      }
      // Escape navigates to launcher without killing background agent execution or denying approval.
      return false
    })
    return () => setCommandSurfaceEscapeConsumer(null)
  }, [active, historyOpen])

  useEffect(() => {
    const running = (item: AgentConversation): boolean =>
      Boolean(item.session) &&
      (item.preparing || item.chat.status === 'streaming' || item.chat.status === 'submitted')
    const all = [...conversations.current]
    const waiting = (item: AgentConversation): boolean => {
      const message = item.chat.lastMessage
      const pending = message?.role === 'assistant' ? agentMessageApproval(message) : null
      return Boolean(pending && pending.approvalId !== item.resolvedApproval)
    }
    // The launcher has one status slot. Surface pending approvals first, then
    // the visible run, then another background run so none becomes unreachable.
    const current =
      all.find((item) => running(item) && waiting(item)) ??
      (running(owner) ? owner : all.find(running))
    if (!current?.session) {
      onActiveRunChange?.(null)
      return
    }
    const message = current.chat.lastMessage
    const stage = message ? agentMessageStages(message).at(-1) : undefined
    onActiveRunChange?.({
      sessionId: current.session.id,
      sessionTitle: current.session.title,
      sessionIcon: current.session.icon,
      busy: true,
      statusLabel: waiting(current)
        ? 'Waiting for your approval'
        : current.preparing || current.chat.status === 'submitted'
          ? 'Starting agent…'
          : stage?.status === 'running'
            ? stage.label
            : 'Generating…',
      streamingText: message ? agentMessageText(message) || undefined : undefined,
      waitingApproval: waiting(current),
    })
  }, [owner, conversationRevision, onActiveRunChange])
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
      for (const current of conversations.current) {
        current.submission = null
        current.unsubscribe()
        void current.chat.dispose()
      }
    }
  }, [])

  const deleteResponse = async (message: AgentUIMessage): Promise<void> => {
    const current = owner.session
    if (!current || busy) return
    const result = await window.tezbar.chatDeleteTurn(current.id, message.id)
    if (!result.ok) {
      if (mounted.current && selectedConversation.current === owner)
        setUiError(result.error || 'Could not delete this response.')
      return
    }
    owner.chat.messages = owner.chat.messages.filter((item) => item.id !== message.id)
    owner.session = {
      ...owner.session!,
      turns: owner.session!.turns.filter((item) => item.id !== message.id),
    }
    notifyConversation(owner)
    if (mounted.current) void refreshHistory()
  }
  const deleteConversation = async (id: string): Promise<void> => {
    if (selectedConversation.current?.session?.id === id) resetChat()
    const result = await window.tezbar.chatDelete(id)
    if (!result.ok) {
      if (mounted.current) setUiError('Could not delete this conversation.')
    } else {
      const retained = [...conversations.current].find((item) => item.session?.id === id)
      if (retained) {
        // Deletion invalidates the retained copy as well as SQLite. Disposal
        // can finish a live stream, so clear its persistence target first.
        retained.session = null
        retained.submission = null
        retained.unsubscribe()
        await retained.chat.dispose()
        conversations.current.delete(retained)
      }
    }
    if (mounted.current) void refreshHistory()
  }
  const submit = (event: FormEvent): void => {
    event.preventDefault()
    void send(draft, attachment)
  }
  const chatProvider = (config.taskProviderOverrides?.chat ??
    config.provider ??
    'ollama') as ProviderId
  const pickerConfig: LlmConfigRecord = {
    ...config,
    provider: chatProvider,
    model: responseMeta(config).model,
    providerSelectedModels: {
      ...config.providerSelectedModels,
      [chatProvider]: responseMeta(config).model,
    },
  }

  return (
    <div
      aria-label="AI Chat"
      tabIndex={-1}
      className="view-sheet flex h-full min-h-0 w-full flex-col gap-2 outline-none"
    >
      <div className="agent-chat-shell flex min-h-0 flex-1 flex-col overflow-hidden">
        <CommandSurfaceHeader>
          <CommandSurfaceBadge tone="ai" label="AI mode">
            <span aria-hidden className="text-[11px] font-semibold leading-none">
              AI
            </span>
          </CommandSurfaceBadge>
          <h1
            className="min-w-0 flex-1 truncate font-display text-[15px] font-normal text-ink-1"
            title={
              workingDirectory
                ? `${session?.title || 'Agent'} · ${workingDirectory}`
                : session?.title
            }
          >
            {session?.title || 'New chat'}
          </h1>
          <div className={cx('min-w-0 max-w-[45%]', busy ? 'pointer-events-none opacity-50' : '')}>
            <ModelPicker
              config={pickerConfig}
              triggerClassName="max-w-full"
              disabled={busy || savingModel}
              open={pickerOpen}
              onOpenChange={setPickerOpen}
              onSelect={selectModel}
              onConfigure={onOpenSettings}
              onBeforeOpen={() => setHistoryOpen(false)}
            />
          </div>
          <CommandSurfaceAction
            title="Chat history"
            aria-label="Chat history"
            aria-expanded={historyOpen}
            onClick={() => {
              setHistoryOpen(!historyOpen)
              setPickerOpen(false)
              void refreshHistory()
            }}
          >
            <svg
              aria-hidden
              width="16"
              height="16"
              viewBox="0 0 18 18"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.35"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M4.7 4v3h3M4.7 7A4.8 4.8 0 1 1 4.2 10M9 6.5v3l2 1.2" />
            </svg>
          </CommandSurfaceAction>
          <CommandSurfaceAction
            title="New chat (⌘N)"
            aria-label="New chat"
            onClick={() => void resetChat()}
          >
            <svg
              aria-hidden
              width="16"
              height="16"
              viewBox="0 0 18 18"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.35"
              strokeLinecap="round"
            >
              <path d="M9 3.5v11M3.5 9h11" />
            </svg>
          </CommandSurfaceAction>
          <CommandSurfaceAction title="Back (Esc)" aria-label="Back" onClick={onBack}>
            <svg
              aria-hidden
              width="16"
              height="16"
              viewBox="0 0 18 18"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.35"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="m10.8 4.5-4.5 4.5 4.5 4.5" />
            </svg>
          </CommandSurfaceAction>
          {historyOpen ? (
            <div
              className="tezbar-popover absolute right-3 top-full z-40 mt-1 max-h-72 w-[min(360px,90vw)] overflow-auto p-2"
              aria-label="Past chats"
            >
              <div className="flex items-center justify-between px-2 py-1 text-[11px] text-ink-3">
                <span>Conversations</span>
                <button className={smallButton} onClick={() => setHistoryOpen(false)} type="button">
                  Close
                </button>
              </div>
              {!history.length ? (
                <p className="p-3 text-[12px] text-ink-4">{historyPlaceholder(historyLoading)}</p>
              ) : (
                history.map((row) => (
                  <div
                    key={row.id}
                    className={cx(
                      'flex items-center gap-1 rounded-lg',
                      row.id === session?.id ? 'bg-white/[0.06]' : 'hover:bg-white/[0.035]'
                    )}
                  >
                    <button
                      type="button"
                      className="flex min-w-0 flex-1 items-center gap-2.5 px-3 py-2 text-left"
                      onClick={() => void loadSession(row.id)}
                    >
                      <ChatSquircle icon={row.icon} size={20} />
                      <div className="min-w-0 flex-1">
                        <span className="block truncate text-[12px] text-ink-2">
                          {row.title || 'Untitled chat'}
                        </span>
                        <span className="block truncate text-[10.5px] text-ink-4">
                          {row.preview}
                        </span>
                      </div>
                    </button>
                    <button
                      type="button"
                      className={smallButton}
                      aria-label={`Delete ${row.title}`}
                      onClick={() => {
                        void deleteConversation(row.id).catch(() =>
                          setUiError('Could not delete this conversation.')
                        )
                      }}
                    >
                      ×
                    </button>
                  </div>
                ))
              )}
            </div>
          ) : null}
        </CommandSurfaceHeader>

        <div className="relative flex min-h-0 flex-1">
          <div
            ref={threadRef}
            className="agent-chat-thread flex min-h-0 min-w-0 flex-1 flex-col gap-6 overflow-y-auto px-5 py-5"
            onWheel={(event) => {
              if (event.deltaY < 0) {
                followRef.current = false
                setFollowing(false)
              }
            }}
            onScroll={(event) => {
              const value = isNearScrollBottom(event.currentTarget)
              followRef.current = value
              setFollowing(value)
            }}
          >
            {activeMessages.map((message, index) => (
              <AgentMessage
                key={message.id}
                message={message}
                streaming={
                  busy && index === activeMessages.length - 1 && message.role === 'assistant'
                }
                onRetry={
                  !busy && index === activeMessages.length - 1 && message.role === 'assistant'
                    ? () => void retry()
                    : undefined
                }
                onDelete={
                  !busy && message.role === 'assistant'
                    ? () => {
                        void deleteResponse(message).catch(() =>
                          setUiError('Could not delete this response.')
                        )
                      }
                    : undefined
                }
              />
            ))}
            {!activeMessages.length && !busy ? (
              <div className="flex flex-1 flex-col items-start justify-center gap-4 py-8">
                <div className="max-w-md">
                  <h2 className="text-[19px] font-semibold leading-tight text-ink-1">
                    What do you want to check?
                  </h2>
                  <p className="mt-2 text-[12.5px] leading-relaxed text-ink-3">
                    The agent can inspect this computer and use tools to get real answers. Commands
                    that need permission appear here before they run.
                  </p>
                </div>
                <div className="flex flex-wrap gap-2">
                  {[
                    'What is using the most CPU?',
                    'Show listening ports',
                    'Find a file on my computer',
                  ].map((suggestion) => (
                    <button
                      key={suggestion}
                      type="button"
                      className="rounded-lg border border-white/[0.09] px-3 py-2 text-[11.5px] text-ink-2 hover:bg-white/[0.04]"
                      onClick={() => {
                        setDraft(suggestion)
                        focusInput()
                      }}
                    >
                      {suggestion}
                    </button>
                  ))}
                </div>
                <button
                  type="button"
                  className="text-[11px] text-ink-4 underline underline-offset-4 hover:text-ink-2"
                  onClick={onOpenSettings}
                >
                  Configure a provider
                </button>
              </div>
            ) : null}
            {preparing ? <p className="text-[12px] text-ink-3">Preparing your request…</p> : null}
            {chat.error && (!activeMessage || !agentMessageError(activeMessage)) ? (
              <div
                role="alert"
                className="agent-run-error rounded-lg border border-rose-300/15 bg-rose-400/[0.06] p-3 text-[12px] text-rose-200"
              >
                <p>{formatLlmErrorMessage(chat.error.message)}</p>
                <button type="button" className="mt-2 underline" onClick={() => void retry()}>
                  Try again
                </button>
              </div>
            ) : null}
          </div>
          {!following && busy ? (
            <button
              type="button"
              className="absolute bottom-3 left-1/2 -translate-x-1/2 rounded-full border border-white/15 bg-glass-panel px-3 py-1.5 text-[11px] text-ink-2"
              onClick={() => {
                followRef.current = true
                setFollowing(true)
                if (threadRef.current) threadRef.current.scrollTop = threadRef.current.scrollHeight
              }}
            >
              Latest output ↓
            </button>
          ) : null}
        </div>

        {approval ? (
          <section
            aria-label="Command approval"
            className="mx-4 mb-2 shrink-0 rounded-xl border border-amber-200/20 bg-amber-200/[0.05] p-3 shadow-lg"
          >
            <div className="flex items-center justify-between gap-2">
              <p className="text-[12px] font-medium text-ink-1">
                {approval.title || 'Allow this command?'}
              </p>
              <ApprovalModePicker
                mode={config.agentApprovalMode ?? 'default'}
                disabled={approvalBusy}
                direction="up"
                onSelect={async (mode) => {
                  const next = { ...config, agentApprovalMode: mode }
                  setConfig(next)
                  configRef.current = next
                  await window.tezbar.setLlmConfig({ agentApprovalMode: mode })
                  if (mode === 'turbo' || mode === 'auto') {
                    void resolveApproval('once')
                  }
                }}
              />
            </div>
            <pre className="my-2 max-h-28 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-black/10 p-2 font-mono text-[11px] text-ink-2 select-text">
              {approval.command}
            </pre>
            <p className="mb-2 text-[10.5px] text-ink-4">
              Only approve commands you expect this task to run.
            </p>
            <div className="flex flex-wrap items-center gap-2">
              <button
                type="button"
                disabled={approvalBusy}
                className="rounded-lg bg-ink-1 px-3 py-1.5 text-[11px] font-medium text-glass-shell disabled:opacity-40"
                onClick={() => void resolveApproval('once')}
              >
                Allow once
              </button>
              <button
                type="button"
                disabled={approvalBusy}
                className={smallButton}
                onClick={() => void resolveApproval('deny')}
              >
                Deny
              </button>
              <button
                type="button"
                disabled={approvalBusy}
                className={smallButton}
                title="Remember only this exact command"
                onClick={() => void resolveApproval('always')}
              >
                Always allow this command
              </button>
            </div>
          </section>
        ) : null}
        {uiError ? (
          <div
            role="alert"
            className="agent-run-error mx-4 mb-2 flex shrink-0 items-start gap-2 rounded-lg border border-rose-300/15 bg-rose-400/[0.06] p-3 text-[11.5px] text-rose-200"
          >
            <span className="min-w-0 flex-1 whitespace-pre-wrap">{uiError}</span>
            <button type="button" aria-label="Dismiss error" onClick={() => setUiError(null)}>
              ×
            </button>
          </div>
        ) : null}
        {saveWarning ? (
          <p role="status" className="mx-4 mb-2 shrink-0 text-[10.5px] text-amber-300">
            {saveWarning}
          </p>
        ) : null}

        <form onSubmit={submit} className="shrink-0 px-3.5 pb-3.5 pt-2" aria-label="Agent composer">
          <div className="agent-composer mx-auto max-w-[780px] rounded-xl border border-white/[0.12] bg-white/[0.035] p-2 focus-within:border-white/25">
            {attachment ? (
              <div className="mb-2 flex items-center gap-3 px-2 pt-1">
                <img
                  src={`data:${attachment.mimeType};base64,${attachment.data}`}
                  alt="Attached active screen"
                  className="h-12 w-20 rounded-lg object-cover"
                />
                <span className="flex-1 text-[11px] text-ink-3">Active screen</span>
                <button
                  type="button"
                  className={smallButton}
                  aria-label="Remove attached screen"
                  onClick={() => setAttachment(null)}
                >
                  Remove
                </button>
              </div>
            ) : null}
            <div className="flex min-w-0 items-end gap-2">
              <CommandSurfaceAction
                disabled={busy || capturing}
                onClick={() => void attachScreen()}
                aria-label="Attach active screen"
                title="Attach active screen (⌘⇧S)"
              >
                {capturing ? (
                  <span className="h-3.5 w-3.5 animate-spin rounded-full border border-current border-r-transparent motion-reduce:animate-none" />
                ) : (
                  <svg
                    aria-hidden
                    width="16"
                    height="16"
                    viewBox="0 0 18 18"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.35"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  >
                    <path d="m5.2 9.5 3.8-3.8a2.3 2.3 0 0 1 3.2 3.2l-4.5 4.5a3.2 3.2 0 0 1-4.5-4.5l4.7-4.7" />
                  </svg>
                )}
              </CommandSurfaceAction>
              <textarea
                id="ai-chat-input"
                aria-label="Message Agent"
                rows={1}
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
                onKeyDown={enterToSubmit}
                placeholder="Message Agent"
                className="block max-h-[120px] min-h-7 min-w-0 flex-1 resize-none bg-transparent px-1 py-1 text-[13px] leading-5 text-ink-1 outline-none placeholder:text-ink-4"
                autoComplete="off"
                spellCheck={false}
              />
              {busy ? (
                <button
                  type="button"
                  aria-label="Stop agent"
                  title="Stop (⌘.)"
                  className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-white/15 text-ink-1 hover:bg-white/[0.06]"
                  onClick={() => void stopRun()}
                >
                  <svg aria-hidden width="14" height="14" viewBox="0 0 16 16">
                    <rect x="4" y="4" width="8" height="8" rx="1" fill="currentColor" />
                  </svg>
                </button>
              ) : (
                <button
                  type="submit"
                  aria-label="Send message"
                  disabled={!draft.trim() || savingModel || capturing}
                  title={savingModel ? 'Saving model…' : 'Send (Enter)'}
                  className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-ink-1 text-glass-shell disabled:cursor-not-allowed disabled:bg-white/[0.06] disabled:text-ink-4"
                >
                  <svg
                    aria-hidden
                    width="16"
                    height="16"
                    viewBox="0 0 18 18"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.5"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  >
                    <path d="M9 13.5v-9m-3.5 3L9 4l3.5 3.5" />
                  </svg>
                </button>
              )}
            </div>
          </div>
        </form>
      </div>
      <CommandSurfaceFooter
        taskActions={taskActions}
        status={
          busy || statusLabel === 'Run failed' ? (
            <span
              role="status"
              className="max-w-[180px] truncate text-[11px] text-ink-3"
              title={statusLabel}
            >
              {statusLabel}
              {busy ? ` · ${elapsed}s` : ''}
            </span>
          ) : undefined
        }
      >
        <Hint label="Providers" keys={<Kbd>⌘,</Kbd>} />
        <Hint
          label="Attach screen"
          keys={
            <>
              <Kbd>⌘</Kbd>
              <Kbd>⇧</Kbd>
              <Kbd>S</Kbd>
            </>
          }
        />
        {busy ? (
          <Hint
            label="Stop"
            keys={
              <>
                <Kbd>⌘</Kbd>
                <Kbd>.</Kbd>
              </>
            }
          />
        ) : (
          <Hint label="Send" keys={<Kbd>↵</Kbd>} />
        )}
        <Hint
          label="New chat"
          keys={
            <>
              <Kbd>⌘</Kbd>
              <Kbd>N</Kbd>
            </>
          }
        />
        <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
      </CommandSurfaceFooter>
    </div>
  )
}
