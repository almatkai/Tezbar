import { app, BrowserWindow, clipboard, dialog, globalShortcut, ipcMain, shell } from 'electron'
import { setSuppressBlurHide } from './windowState'
import { AGENT_IPC, type AgentRunEvent, type Stage } from '../shared/agent'
import { CHAT_CONTEXT_MAX_TURNS, CHAT_IPC, type ChatSession, type ChatTurn } from '../shared/chat'
import { disposeSharedBridge, getSharedBridge } from './agent/bridge'
import {
  appendChatTurn,
  clearAllChatSessions,
  deleteChatSession,
  getChatSession,
  listChatSessions,
  updateChatSessionTitle,
  upsertChatSession,
} from './chat/sessionStore'
import {
  IPC_CHANNELS,
  parseAiActionRequest,
  parseSearchExecuteRequest,
  parseVoiceModelRequest,
  parseVoiceSpeakRequest,
  parseVoiceTranscribeRequest,
} from '../shared/ipc'
import type { SearchAction } from '../shared/search'
import type { Message } from './llm/provider'
import { appIconDataUrl } from './appIcon'
import { streamAnswerToRenderer } from './llm/answerStream'
import { setLauncherContentHeight } from './windowBounds'
import {
  getSafetyDryRun,
  getUiStateRetentionMs,
  readRawConfig,
  setSafetyDryRun,
  writeConfigPatch,
} from './llm/configStore'
import {
  clearDeviceSession,
  persistCopilotTokens,
  pollGithubDeviceFlow,
  startGithubDeviceFlow,
} from './llm/githubCopilotAuth'
import { listModelsForProvider } from './llm/listModels'
import {
  buildProviderForId,
  getProviderForTask,
  getSelectedPiProviderBridge,
  getSelectedPiModelPattern,
  invalidateProviderCache,
  readLLMConfig,
} from './llm/registry'
import type { ProviderId } from '../shared/llmConfig'
import { classifyIntent } from './router'
import {
  getExtensionInstallError,
  inspectExtensionIntegrity,
  installExtension,
  listInstalledExtensions,
  reinstallExtension,
  searchStoreExtensions,
  uninstallExtension,
} from './extensions/service'
import {
  extensionRegistryEvents,
  getExtensionPreferences as getRegistryExtensionPreferences,
  installRegistryExtension,
  listInstalledRegistryExtensions,
  searchExtensionCatalog,
  saveExtensionPreferences as saveRegistryExtensionPreferences,
  uninstallRegistryExtension,
} from './extension-registry'
import {
  clearAllExtensionSessions,
  disposeExtensionSession,
  invokeExtensionAction,
  loadMoreExtensionSession,
  refreshExtensionSession,
  runExtensionCommand,
} from './extension-runner'
import {
  executeSearchAction,
  completePath,
  getSearchBenchmarkHistory,
  listOpenPorts,
  reindexExtensions,
  reindexQuickNotes,
  reindexSnippets,
  runSearchBenchmarks,
  searchEverything,
} from './search/service'
import {
  addQuickNote,
  deleteQuickNote,
  listQuickNotes,
  updateQuickNote,
} from './search/providers/notesProvider'
import { fetchFrankfurterLatest } from './currency/frankfurter'
import { addNamedPort, listNamedPorts, removeNamedPort } from './portManager/namedPortsStore'
import { runAiActionMode } from './llm/actionMode'
import {
  downloadVoiceModel,
  getSelectedVoiceModelId,
  listSttModes,
  listVoiceModels,
  setSelectedVoiceModelId,
  speakText,
  stopSpeaking,
  transcribeAudio,
} from './voice/service'
import type { VoiceModelId } from '../shared/voice'
import { requestPermission, snapshotPermissions } from './permissions/manager'
import type { PermissionId } from '../shared/permissions'
import { clearSafetyLog, listSafetyLog } from './safety/log'
import { listSafetyDescriptors } from './safety/registry'
import { listNativeCommands } from './nativeCommands/registry'
import {
  clearClipboardHistory,
  deleteClipboardEntry,
  listClipboardEntries,
  readClipboardImagePayload,
  restoreClipboardEntry,
  revealClipboardEntryInFinder,
  togglePinClipboardEntry,
  type ClipboardConfig,
} from './search/providers/clipboardProvider'
import {
  addUserSnippet,
  copySnippetById,
  deleteUserSnippet,
  listSnippetsForUi,
  updateUserSnippet,
} from './search/providers/snippetsProvider'
import {
  createTerminalSession,
  getTerminalPromptInfo,
  killTerminalSession,
  resizeTerminalSession,
  shutdownTerminalSessions,
  writeTerminalSession,
} from './terminal/service'
import { TERMINAL_IPC, type TerminalCreateRequest } from '../shared/terminal'
import {
  clearChromiumCache,
  clearClipboardImages,
  getClipboardStorageConfig,
  getStorageBreakdown,
  setClipboardStorageConfig,
  vacuumSearchDatabase,
} from './storage/service'

const LLM_DEFAULTS = {
  uiStateRetentionMs: 60_000,
} as const

let answerAbort: AbortController | null = null
let agentAbort: AbortController | null = null
let agentRunId: string | null = null
let quitConfirmationOpen = false
let quitConfirmed = false

function quitRaymesNow(): void {
  quitConfirmed = true
  globalShortcut.unregisterAll()
  BrowserWindow.getAllWindows().forEach((window) => {
    if (!window.isDestroyed()) window.hide()
  })
  app.quit()
  setTimeout(() => {
    app.exit(0)
  }, 500)
}

async function confirmQuitRaymes(getWindow: () => BrowserWindow | null): Promise<boolean> {
  if (quitConfirmed) return true
  if (quitConfirmationOpen) return false

  quitConfirmationOpen = true
  const win = getWindow()
  setSuppressBlurHide(true)
  try {
    if (win && !win.isDestroyed()) {
      win.show()
      win.focus()
    }
    app.focus({ steal: true })
    const result =
      win && !win.isDestroyed()
        ? await dialog.showMessageBox(win, {
            type: 'question',
            buttons: ['Cancel', 'Quit'],
            defaultId: 1,
            cancelId: 0,
            title: 'Quit Raymes',
            message: 'Quit Raymes?',
            detail: 'Are you sure you want to quit Raymes and terminate all background processes?',
            noLink: true,
          })
        : await dialog.showMessageBox({
            type: 'question',
            buttons: ['Cancel', 'Quit'],
            defaultId: 1,
            cancelId: 0,
            title: 'Quit Raymes',
            message: 'Quit Raymes?',
            detail: 'Are you sure you want to quit Raymes and terminate all background processes?',
            noLink: true,
          })

    quitConfirmed = result.response === 1
    return quitConfirmed
  } finally {
    quitConfirmationOpen = false
    if (!quitConfirmed) {
      setSuppressBlurHide(false)
    }
  }
}

const CHAT_SYSTEM_PROMPT =
  'You are Raymes, a helpful assistant. Answer clearly and concisely unless the user asks for more detail.'

type IpcControls = {
  startWindowDragMonitoring: (win: BrowserWindow) => void
  stopWindowDragMonitoring: (win: BrowserWindow) => void
  updateRaymesHotkey?: (accelerator: string) => {
    ok: boolean
    accelerator: string
    error?: string
  }
}

function sendAgentEvent(sender: Electron.WebContents, event: AgentRunEvent): void {
  if (!sender.isDestroyed()) sender.send(AGENT_IPC.EVENT, event)
}

function startAgentRun(sender: Electron.WebContents, task: string): string {
  agentAbort?.abort()
  agentAbort = new AbortController()
  const ac = agentAbort
  const runId =
    (agentRunId = `agent-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`)
  const bridge = getSharedBridge()
  const piProvider = getSelectedPiProviderBridge()

  sendAgentEvent(sender, { type: 'start', runId, task })

  const onStage = (stage: Stage): void => sendAgentEvent(sender, { type: 'stage', runId, stage })
  const onMessageDelta = (delta: string): void =>
    sendAgentEvent(sender, { type: 'message', runId, delta })
  const onAnswer = (text: string): void => sendAgentEvent(sender, { type: 'answer', runId, text })

  const onStderrLine = (line: string): void => {
    sendAgentEvent(sender, { type: 'log', runId, source: 'stderr', line })
  }

  console.log('[raymes:agent] run', { runId, taskPreview: task.slice(0, 120) })

  void bridge
    .run(task, {
      runId,
      model: piProvider?.modelPattern ?? getSelectedPiModelPattern(),
      raymesProviderJson: piProvider?.providerJson,
      signal: ac.signal,
      onStage,
      onMessageDelta,
      onAnswer,
      onStderrLine,
    })
    .then(() => {
      sendAgentEvent(sender, { type: 'done', runId })
    })
    .catch((err: unknown) => {
      const message = err instanceof Error ? err.message : String(err)
      sendAgentEvent(sender, { type: 'error', runId, message })
      sendAgentEvent(sender, { type: 'done', runId })
    })
    .finally(() => {
      if (agentAbort === ac) agentAbort = null
      if (agentRunId === runId) agentRunId = null
    })

  return runId
}

function normalizeChatTurns(raw: unknown): ChatTurn[] | null {
  if (!Array.isArray(raw)) return null
  const turns: ChatTurn[] = []
  for (const item of raw) {
    if (!item || typeof item !== 'object') return null
    const turn = item as Partial<ChatTurn>
    if (
      typeof turn.id !== 'string' ||
      (turn.role !== 'user' && turn.role !== 'assistant') ||
      typeof turn.text !== 'string' ||
      typeof turn.createdAt !== 'number'
    ) {
      return null
    }
    turns.push({
      id: turn.id,
      role: turn.role,
      text: turn.text,
      createdAt: turn.createdAt,
    })
  }
  return turns
}

function startChatRun(sender: Electron.WebContents, turns: ChatTurn[]): string {
  agentAbort?.abort()
  agentAbort = new AbortController()
  const ac = agentAbort
  const runId =
    (agentRunId = `chat-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`)
  const contextTurns = turns.slice(-CHAT_CONTEXT_MAX_TURNS)
  const messages: Message[] = [
    { role: 'system', content: CHAT_SYSTEM_PROMPT },
    ...contextTurns.map((turn) => ({ role: turn.role, content: turn.text }) satisfies Message),
  ]

  sendAgentEvent(sender, { type: 'start', runId, task: turns.at(-1)?.text ?? '' })

  void (async () => {
    let fullText = ''
    try {
      const provider = getProviderForTask('chat')
      const stream = await provider.chat(messages, undefined, { signal: ac.signal })
      for await (const delta of stream) {
        if (ac.signal.aborted) return
        if (delta.text) {
          fullText += delta.text
          sendAgentEvent(sender, { type: 'message', runId, delta: delta.text })
        }
      }
      sendAgentEvent(sender, { type: 'answer', runId, text: fullText })
    } catch (err) {
      if (!ac.signal.aborted) {
        sendAgentEvent(sender, {
          type: 'error',
          runId,
          message: err instanceof Error ? err.message : String(err),
        })
      }
    } finally {
      if (!ac.signal.aborted) sendAgentEvent(sender, { type: 'done', runId })
      if (agentAbort === ac) agentAbort = null
      if (agentRunId === runId) agentRunId = null
    }
  })()

  return runId
}

/** Called from `main/index.ts` on `will-quit` to flush subprocesses. */
export function shutdownIpcHandlers(): void {
  answerAbort?.abort()
  agentAbort?.abort()
  clearAllExtensionSessions()
  disposeSharedBridge()
  shutdownTerminalSessions()
}

export function registerIpcHandlers(
  getWindow: () => BrowserWindow | null,
  controls?: IpcControls
): void {
  extensionRegistryEvents.on('progress', (payload) => {
    const win = getWindow()
    if (win && !win.isDestroyed()) {
      win.webContents.send('extension:install-progress', payload)
    }
  })

  ipcMain.handle('llm-config-get', async () => ({
    ...LLM_DEFAULTS,
    ...readLLMConfig(),
    ...readRawConfig(),
    uiStateRetentionMs: getUiStateRetentionMs(),
  }))

  ipcMain.handle('llm-config-set', async (_event, patch: unknown) => {
    if (!patch || typeof patch !== 'object') return
    const configPatch = { ...(patch as Record<string, unknown>) }
    const requestedHotkey = configPatch.raymesHotkey
    delete configPatch.raymesHotkey

    if (typeof requestedHotkey === 'string' && controls?.updateRaymesHotkey) {
      const result = controls.updateRaymesHotkey(requestedHotkey)
      if (!result.ok) return result
      if (Object.keys(configPatch).length > 0) writeConfigPatch(configPatch)
      invalidateProviderCache()
      return result
    }

    writeConfigPatch(configPatch)
    invalidateProviderCache()
  })

  ipcMain.handle('llm-provider-statuses', async () => {
    const cfg = readLLMConfig()
    const ids: ProviderId[] = [
      'openai',
      'openai-compatible',
      'anthropic',
      'ollama',
      'copilot',
      'gemini',
      'opencode',
      'deepseek',
    ]
    const entries = await Promise.all(
      ids.map(async (id) => {
        try {
          const ok = await buildProviderForId(id, cfg).isAvailable()
          return [id, ok] as const
        } catch {
          return [id, false] as const
        }
      })
    )
    return Object.fromEntries(entries) as Record<ProviderId, boolean>
  })

  ipcMain.handle('llm-list-models', async (_event, providerId: unknown) => {
    const id = providerId as ProviderId
    const customProvider =
      typeof id === 'string' && readLLMConfig().customProviders?.some((provider) => provider.id === id)
    if (
      id !== 'openai' &&
      id !== 'openai-compatible' &&
      id !== 'anthropic' &&
      id !== 'ollama' &&
      id !== 'copilot' &&
      id !== 'gemini' &&
      id !== 'opencode' &&
      id !== 'deepseek' &&
      !customProvider
    )
      return []
    try {
      return await listModelsForProvider(id)
    } catch {
      return []
    }
  })

  // Renderer reports its measured content height. We clamp to the launcher
  // bounds and update the window content size programmatically — the user
  // still cannot drag to resize because the BrowserWindow is resizable:false.
  ipcMain.handle('window-set-content-height', async (_event, raw: unknown) => {
    const win = getWindow()
    if (!win || win.isDestroyed()) return
    const payload =
      raw && typeof raw === 'object'
        ? (raw as { height?: unknown; zoomFactor?: unknown })
        : { height: raw, zoomFactor: 1 }
    const height = typeof payload.height === 'number' ? payload.height : Number(payload.height)
    const zoomFactor =
      typeof payload.zoomFactor === 'number' ? payload.zoomFactor : Number(payload.zoomFactor)
    if (!Number.isFinite(height)) return
    setLauncherContentHeight(win, height, zoomFactor)
  })

  ipcMain.handle('permissions:snapshot', async () => snapshotPermissions())

  ipcMain.handle('permissions:request', async (_event, raw: unknown) => {
    if (typeof raw !== 'string') {
      throw new Error('Permission id must be a string')
    }
    return requestPermission(raw as PermissionId)
  })

  ipcMain.handle('safety:descriptors', async () => listSafetyDescriptors())

  ipcMain.handle('safety:log', async () => listSafetyLog())

  ipcMain.handle('safety:log-clear', async () => {
    clearSafetyLog()
  })

  ipcMain.handle('safety:dry-run:get', async () => getSafetyDryRun())
  ipcMain.handle('safety:dry-run:set', async (_event, raw: unknown) => {
    setSafetyDryRun(raw === true)
    return getSafetyDryRun()
  })

  ipcMain.handle('native-commands:list', async () => listNativeCommands())

  ipcMain.handle('clipboard:list', async () => listClipboardEntries())

  ipcMain.handle('clipboard:restore', async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return false
    return restoreClipboardEntry(id)
  })

  ipcMain.handle('clipboard:delete', async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return false
    return deleteClipboardEntry(id)
  })

  ipcMain.handle('clipboard:toggle-pin', async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return false
    return togglePinClipboardEntry(id)
  })

  ipcMain.handle('clipboard:reveal', async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return false
    return revealClipboardEntryInFinder(id)
  })

  ipcMain.handle('clipboard:image', async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return null
    return readClipboardImagePayload(id)
  })

  ipcMain.handle('clipboard:clear', async () => {
    clearClipboardHistory()
  })

  ipcMain.handle('app-icon:data-url', async (_event, raw: unknown) => {
    const appPath = typeof raw === 'string' ? raw.trim() : ''
    if (!appPath.endsWith('.app')) return null
    return (await appIconDataUrl(appPath)) ?? null
  })

  ipcMain.handle('snippets:list', async () => listSnippetsForUi())

  ipcMain.handle('snippets:copy', async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return { ok: false, message: 'Invalid snippet' }
    return copySnippetById(id)
  })

  ipcMain.handle('snippets:add', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') return { ok: false, message: 'Invalid payload' }
    const o = payload as { label?: unknown; trigger?: unknown; body?: unknown }
    const r = addUserSnippet({
      label: typeof o.label === 'string' ? o.label : '',
      trigger: typeof o.trigger === 'string' ? o.trigger : '',
      body: typeof o.body === 'string' ? o.body : '',
    })
    if (r.ok) await reindexSnippets()
    return r
  })

  ipcMain.handle('snippets:update', async (_event, id: unknown, payload: unknown) => {
    if (typeof id !== 'string' || !id) return { ok: false, message: 'Invalid snippet id' }
    if (!payload || typeof payload !== 'object') return { ok: false, message: 'Invalid payload' }
    const o = payload as { label?: unknown; trigger?: unknown; body?: unknown }
    const r = updateUserSnippet(id, {
      label: typeof o.label === 'string' ? o.label : '',
      trigger: typeof o.trigger === 'string' ? o.trigger : '',
      body: typeof o.body === 'string' ? o.body : '',
    })
    if (r.ok) await reindexSnippets()
    return r
  })

  ipcMain.handle('snippets:delete', async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return { ok: false, message: 'Invalid snippet id' }
    const r = deleteUserSnippet(id)
    if (r.ok) await reindexSnippets()
    return r
  })

  ipcMain.handle('notes:list', async () => listQuickNotes())

  ipcMain.handle('notes:append', async (_event, text: unknown) => {
    if (typeof text !== 'string' || !text.trim()) return null
    const entry = addQuickNote(text)
    await reindexQuickNotes()
    return entry
  })

  ipcMain.handle('notes:update', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') return false
    const o = payload as { createdAt?: unknown; text?: unknown }
    if (typeof o.createdAt !== 'number' || typeof o.text !== 'string') return false
    const ok = updateQuickNote(o.createdAt, o.text)
    if (ok) await reindexQuickNotes()
    return ok
  })

  ipcMain.handle('notes:delete', async (_event, createdAt: unknown) => {
    if (typeof createdAt !== 'number') return false
    const ok = deleteQuickNote(createdAt)
    if (ok) await reindexQuickNotes()
    return ok
  })

  ipcMain.handle(TERMINAL_IPC.CREATE, async (event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') throw new Error('Invalid terminal request')
    const body = raw as Partial<TerminalCreateRequest>
    if (typeof body.cols !== 'number' || typeof body.rows !== 'number') {
      throw new Error('Terminal dimensions are required')
    }
    if (body.cwd !== undefined && typeof body.cwd !== 'string') {
      throw new Error('Invalid terminal working directory')
    }
    if (body.initialCommand !== undefined && typeof body.initialCommand !== 'string') {
      throw new Error('Invalid initial terminal command')
    }
    if ((body.initialCommand?.length ?? 0) > 16 * 1024) {
      throw new Error('Initial terminal command is too long')
    }
    return createTerminalSession(event.sender, body as TerminalCreateRequest)
  })

  ipcMain.handle(TERMINAL_IPC.WRITE, async (event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return false
    const body = raw as { sessionId?: unknown; data?: unknown }
    if (typeof body.sessionId !== 'string' || typeof body.data !== 'string') return false
    return writeTerminalSession(event.sender.id, body.sessionId, body.data)
  })

  ipcMain.handle(TERMINAL_IPC.RESIZE, async (event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return false
    const body = raw as { sessionId?: unknown; cols?: unknown; rows?: unknown }
    if (
      typeof body.sessionId !== 'string' ||
      typeof body.cols !== 'number' ||
      typeof body.rows !== 'number'
    ) {
      return false
    }
    return resizeTerminalSession(event.sender.id, body.sessionId, body.cols, body.rows)
  })

  ipcMain.handle(TERMINAL_IPC.KILL, async (event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return false
    const body = raw as { sessionId?: unknown }
    if (typeof body.sessionId !== 'string') return false
    return killTerminalSession(event.sender.id, body.sessionId)
  })

  ipcMain.handle(TERMINAL_IPC.GET_PROMPT_INFO, async () => {
    return getTerminalPromptInfo()
  })

  ipcMain.handle('open-external-url', async (_event, url: unknown) => {
    if (typeof url !== 'string') return
    let parsed: URL
    try {
      parsed = new URL(url)
    } catch {
      return
    }
    if (parsed.protocol !== 'https:') return
    if (parsed.hostname !== 'github.com' && !parsed.hostname.endsWith('.github.com')) return
    await shell.openExternal(url)
  })

  ipcMain.handle('github-device-start', async (_event, clientId: unknown) => {
    if (typeof clientId !== 'string' || !clientId.trim()) {
      throw new Error('GitHub OAuth Client ID is required for device sign-in.')
    }
    return startGithubDeviceFlow(clientId.trim())
  })

  ipcMain.handle('github-device-poll', async () => {
    const r = await pollGithubDeviceFlow()
    if (r.status === 'success') {
      persistCopilotTokens(r.access_token, r.refresh_token, r.expires_in)
      invalidateProviderCache()
    }
    return r
  })

  ipcMain.handle('github-device-cancel', async () => {
    clearDeviceSession()
  })

  ipcMain.handle(IPC_CHANNELS.QUERY, async (event, input: unknown) => {
    const text = typeof input === 'string' ? input : String(input ?? '')
    console.log('[IPC_CHANNELS.QUERY] received input:', text)
    const intent = await classifyIntent(text)
    console.log('[IPC_CHANNELS.QUERY] classified intent:', intent)
    if (intent.type === 'answer' || intent.type === 'ai') {
      console.log('[IPC_CHANNELS.QUERY] starting streamAnswerToRenderer')
      answerAbort?.abort()
      answerAbort = new AbortController()
      const ac = answerAbort
      void streamAnswerToRenderer(event.sender, intent.input, ac.signal).finally(() => {
        if (answerAbort === ac) answerAbort = null
      })
    }
    if (intent.type === 'agent') {
      console.log('[IPC_CHANNELS.QUERY] starting startAgentRun')
      startAgentRun(event.sender, intent.input)
    }
    return intent
  })

  ipcMain.handle('cancel', async () => {
    answerAbort?.abort()
    agentAbort?.abort()
  })

  ipcMain.handle(AGENT_IPC.RUN, async (event, raw: unknown) => {
    const task = typeof raw === 'string' ? raw : String(raw ?? '')
    if (!task.trim()) {
      return { ok: false, error: 'Task is empty' }
    }
    const runId = startAgentRun(event.sender, task)
    return { ok: true, runId }
  })

  ipcMain.handle(AGENT_IPC.CANCEL, async () => {
    agentAbort?.abort()
    return { ok: true }
  })

  // --- Chat sessions ------------------------------------------------------
  // The renderer owns the conversation state machine (30s continuation window,
  // active session, etc.) and tells us what to persist. We just provide
  // durable storage + list/get/delete/clear operations against sqlite.
  ipcMain.handle(CHAT_IPC.RUN, async (event, rawTurns: unknown) => {
    const turns = normalizeChatTurns(rawTurns)
    if (!turns || turns.length === 0) {
      return { ok: false, error: 'Invalid chat run payload' }
    }
    const runId = startChatRun(event.sender, turns)
    return { ok: true, runId }
  })

  ipcMain.handle(CHAT_IPC.LIST, async (_event, rawLimit: unknown) => {
    const limit =
      typeof rawLimit === 'number' && Number.isFinite(rawLimit) && rawLimit > 0
        ? Math.floor(rawLimit)
        : undefined
    return listChatSessions(limit)
  })

  ipcMain.handle(CHAT_IPC.GET, async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return null
    return getChatSession(id)
  })

  ipcMain.handle(CHAT_IPC.APPEND, async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') {
      return { ok: false, error: 'Invalid chat append payload' }
    }
    const body = payload as {
      session?: Partial<ChatSession>
      turn?: Partial<ChatTurn>
    }
    const s = body.session
    const t = body.turn
    if (
      !s ||
      typeof s.id !== 'string' ||
      typeof s.title !== 'string' ||
      typeof s.createdAt !== 'number' ||
      typeof s.updatedAt !== 'number'
    ) {
      return { ok: false, error: 'Invalid session' }
    }
    if (
      !t ||
      typeof t.id !== 'string' ||
      (t.role !== 'user' && t.role !== 'assistant') ||
      typeof t.text !== 'string' ||
      typeof t.createdAt !== 'number'
    ) {
      return { ok: false, error: 'Invalid turn' }
    }
    try {
      upsertChatSession({
        id: s.id,
        title: s.title,
        createdAt: s.createdAt,
        updatedAt: s.updatedAt,
      })
      appendChatTurn(s.id, {
        id: t.id,
        role: t.role,
        text: t.text,
        stages: Array.isArray(t.stages) ? (t.stages as Stage[]) : undefined,
        error: typeof t.error === 'string' ? t.error : undefined,
        createdAt: t.createdAt,
      })
      return { ok: true }
    } catch (err) {
      return { ok: false, error: err instanceof Error ? err.message : String(err) }
    }
  })

  ipcMain.handle(CHAT_IPC.UPDATE_TITLE, async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') return { ok: false }
    const body = payload as { id?: unknown; title?: unknown }
    if (typeof body.id !== 'string' || typeof body.title !== 'string') {
      return { ok: false }
    }
    try {
      updateChatSessionTitle(body.id, body.title)
      return { ok: true }
    } catch {
      return { ok: false }
    }
  })

  ipcMain.handle(CHAT_IPC.DELETE, async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return { ok: false }
    return { ok: deleteChatSession(id) }
  })

  ipcMain.handle(CHAT_IPC.CLEAR, async () => {
    try {
      clearAllChatSessions()
      return { ok: true }
    } catch {
      return { ok: false }
    }
  })

  ipcMain.handle('app:confirm-quit', async () => confirmQuitRaymes(getWindow))

  ipcMain.on('app:quit-confirmed', () => {
    if (!quitConfirmed) return
    quitRaymesNow()
  })

  ipcMain.on('app:request-quit', () => {
    void confirmQuitRaymes(getWindow)
      .then((confirmed) => {
        if (confirmed) quitRaymesNow()
      })
      .catch(() => {
        setSuppressBlurHide(false)
      })
  })

  ipcMain.handle('get-extensions', async () => {
    return listInstalledExtensions()
  })

  ipcMain.handle('extensions:listInstalled', async () => {
    return listInstalledExtensions()
  })

  ipcMain.handle('extensions:searchStore', async (_event, query: unknown) => {
    const q = typeof query === 'string' ? query : ''
    return searchStoreExtensions(q)
  })

  ipcMain.handle('extensions:install', async (_event, extensionId: unknown) => {
    if (typeof extensionId !== 'string' || !extensionId.trim()) {
      throw new Error('A valid extension id is required')
    }
    const result = await installExtension(extensionId)
    await reindexExtensions()
    return result
  })

  ipcMain.handle('extensions:uninstall', async (_event, extensionId: unknown) => {
    if (typeof extensionId !== 'string' || !extensionId.trim()) {
      throw new Error('A valid extension id is required')
    }
    const result = await uninstallExtension(extensionId)
    await reindexExtensions()
    return result
  })

  ipcMain.handle('extensions:integrity', async (_event, extensionId: unknown) => {
    if (typeof extensionId !== 'string' || !extensionId.trim()) {
      throw new Error('A valid extension id is required')
    }
    return inspectExtensionIntegrity(extensionId)
  })

  ipcMain.handle('extensions:reinstall', async (_event, extensionId: unknown) => {
    if (typeof extensionId !== 'string' || !extensionId.trim()) {
      throw new Error('A valid extension id is required')
    }
    return reinstallExtension(extensionId)
  })

  ipcMain.handle('extensions:install-error', async (_event, extensionId: unknown) => {
    if (typeof extensionId !== 'string' || !extensionId.trim()) return null
    return getExtensionInstallError(extensionId)
  })

  ipcMain.handle('extension:list', async () => {
    return listInstalledRegistryExtensions()
  })

  ipcMain.handle('extension:search-store', async (_event, query: unknown) => {
    const q = typeof query === 'string' ? query : ''
    return searchExtensionCatalog(q)
  })

  ipcMain.handle('extension:install', async (_event, extensionId: unknown) => {
    if (typeof extensionId !== 'string' || !extensionId.trim()) {
      throw new Error('A valid extension id is required')
    }
    const result = await installRegistryExtension(extensionId)
    await reindexExtensions()
    return result
  })

  ipcMain.handle('extension:uninstall', async (_event, extensionId: unknown) => {
    if (typeof extensionId !== 'string' || !extensionId.trim()) {
      throw new Error('A valid extension id is required')
    }
    const result = uninstallRegistryExtension(extensionId)
    await reindexExtensions()
    return result
  })

  ipcMain.handle('extension:run-command', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') {
      throw new Error('Invalid extension run payload')
    }

    const body = payload as {
      extensionId?: unknown
      commandName?: unknown
      argumentValues?: unknown
    }

    if (typeof body.extensionId !== 'string' || typeof body.commandName !== 'string') {
      throw new Error('extensionId and commandName are required')
    }

    const argumentValues =
      body.argumentValues && typeof body.argumentValues === 'object'
        ? Object.fromEntries(
            Object.entries(body.argumentValues as Record<string, unknown>).map(([key, value]) => [
              key,
              typeof value === 'string' ? value : String(value ?? ''),
            ])
          )
        : undefined

    return runExtensionCommand({
      extensionId: body.extensionId,
      commandName: body.commandName,
      argumentValues,
    })
  })

  ipcMain.handle('extension:invoke-action', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') {
      throw new Error('Invalid extension action payload')
    }

    const body = payload as {
      sessionId?: unknown
      actionId?: unknown
      formValues?: unknown
    }

    if (typeof body.sessionId !== 'string' || typeof body.actionId !== 'string') {
      throw new Error('sessionId and actionId are required')
    }

    const formValues =
      body.formValues && typeof body.formValues === 'object'
        ? Object.fromEntries(
            Object.entries(body.formValues as Record<string, unknown>).map(([key, value]) => [
              key,
              typeof value === 'string' ? value : String(value ?? ''),
            ])
          )
        : undefined

    return invokeExtensionAction({
      sessionId: body.sessionId,
      actionId: body.actionId,
      formValues,
    })
  })

  ipcMain.handle('extension:search-text-changed', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') {
      throw new Error('Invalid extension search payload')
    }

    const body = payload as {
      sessionId?: unknown
      searchText?: unknown
    }

    if (typeof body.sessionId !== 'string' || typeof body.searchText !== 'string') {
      throw new Error('sessionId and searchText are required')
    }

    const { updateSearchText } = await import('./extension-runner')
    return updateSearchText({
      sessionId: body.sessionId,
      searchText: body.searchText,
    })
  })

  ipcMain.handle('extension:refresh-session', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') {
      throw new Error('Invalid extension refresh payload')
    }

    const body = payload as { sessionId?: unknown }
    if (typeof body.sessionId !== 'string') {
      throw new Error('sessionId is required')
    }

    return refreshExtensionSession({ sessionId: body.sessionId })
  })

  ipcMain.handle('extension:dispose-session', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') return false
    const body = payload as { sessionId?: unknown }
    if (typeof body.sessionId !== 'string') return false
    return disposeExtensionSession(body.sessionId)
  })

  ipcMain.handle('extension:load-more', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') {
      throw new Error('Invalid extension pagination payload')
    }
    const body = payload as { sessionId?: unknown }
    if (typeof body.sessionId !== 'string') {
      throw new Error('sessionId is required')
    }
    return loadMoreExtensionSession({ sessionId: body.sessionId })
  })

  ipcMain.handle('clipboard:read', async () => {
    return clipboard.readText()
  })

  ipcMain.handle('clipboard:write', async (_event, raw: unknown) => {
    const text = typeof raw === 'string' ? raw : String(raw ?? '')
    clipboard.writeText(text)
    return { ok: true }
  })

  ipcMain.handle('shell:open', async (_event, raw: unknown) => {
    const target = typeof raw === 'string' ? raw.trim() : ''
    if (!target) return { ok: false }

    await shell.openExternal(target)
    return { ok: true }
  })

  ipcMain.handle('preferences:get', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') return {}
    const body = payload as { extensionId?: unknown; commandName?: unknown }
    if (typeof body.extensionId !== 'string' || !body.extensionId.trim()) return {}

    const commandName = typeof body.commandName === 'string' ? body.commandName : undefined
    return getRegistryExtensionPreferences(body.extensionId, commandName)
  })

  ipcMain.handle('preferences:set', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') return {}
    const body = payload as {
      extensionId?: unknown
      commandName?: unknown
      values?: unknown
    }
    if (typeof body.extensionId !== 'string' || !body.extensionId.trim()) return {}
    const values =
      body.values && typeof body.values === 'object' ? (body.values as Record<string, unknown>) : {}
    const commandName = typeof body.commandName === 'string' ? body.commandName : undefined
    return saveRegistryExtensionPreferences(body.extensionId, values, commandName)
  })

  ipcMain.handle(IPC_CHANNELS.SEARCH_ALL, async (_event, query: unknown) => {
    const q = typeof query === 'string' ? query : ''
    return searchEverything(q)
  })

  ipcMain.handle(IPC_CHANNELS.PATH_COMPLETE, async (_event, query: unknown) => {
    const q = typeof query === 'string' ? query : ''
    return completePath(q)
  })

  ipcMain.handle(IPC_CHANNELS.SEARCH_BENCHMARK_RUN, async () => {
    return runSearchBenchmarks()
  })

  ipcMain.handle(IPC_CHANNELS.SEARCH_BENCHMARK_HISTORY, async () => {
    return getSearchBenchmarkHistory()
  })

  ipcMain.handle('currency:frankfurter-latest', async (_event, from: unknown) => {
    if (typeof from !== 'string' || !from.trim()) {
      throw new Error('Frankfurter: currency code required')
    }
    return fetchFrankfurterLatest(from.trim())
  })

  ipcMain.handle('open-ports:list', async () => {
    return listOpenPorts()
  })

  ipcMain.handle('port-manager:named:list', async () => listNamedPorts())

  ipcMain.handle('port-manager:named:add', async (_event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return null
    const o = raw as Record<string, unknown>
    const name = typeof o.name === 'string' ? o.name : ''
    const port = typeof o.port === 'number' ? o.port : Number(o.port)
    return addNamedPort(name, port)
  })

  ipcMain.handle('port-manager:named:remove', async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id.trim()) return false
    return removeNamedPort(id.trim())
  })

  ipcMain.handle(IPC_CHANNELS.SEARCH_EXECUTE, async (_event, payload: unknown) => {
    try {
      const request = parseSearchExecuteRequest(payload)
      return executeSearchAction(request.action, request.context)
    } catch {
      if (!payload || typeof payload !== 'object') {
        throw new Error('Invalid search action payload')
      }
      return executeSearchAction(payload as SearchAction)
    }
  })

  ipcMain.handle(IPC_CHANNELS.AI_ACTION, async (_event, payload: unknown) => {
    const req = parseAiActionRequest(payload)
    const cfg = readLLMConfig()

    if (cfg.aiActionRequirePermission !== false && req.allowAutomation !== true) {
      return {
        ok: false,
        output: 'Action mode requires explicit permission. Retry with allowAutomation=true.',
      }
    }

    return runAiActionMode({
      ...req,
      redactSensitive: req.redactSensitive ?? cfg.aiActionRedactionEnabled !== false,
    })
  })

  ipcMain.handle(IPC_CHANNELS.VOICE_TTS_SPEAK, async (_event, payload: unknown) => {
    const req = parseVoiceSpeakRequest(payload)
    await speakText(req.text)
    return { ok: true }
  })

  ipcMain.handle(IPC_CHANNELS.VOICE_TTS_STOP, async () => {
    stopSpeaking()
    return { ok: true }
  })

  ipcMain.handle(IPC_CHANNELS.VOICE_STT_MODES, async () => {
    return listSttModes()
  })

  ipcMain.handle(IPC_CHANNELS.VOICE_STT_TRANSCRIBE, async (_event, payload: unknown) => {
    const req = parseVoiceTranscribeRequest(payload)
    return transcribeAudio(req)
  })

  // Renderer toggles this around Hold-to-Speak so the mic permission sheet
  // or any brief focus change while recording does not hide the launcher.
  ipcMain.handle('window:suppress-blur-hide', async (_event, payload: unknown) => {
    setSuppressBlurHide(payload === true)
    return { ok: true }
  })

  ipcMain.handle(IPC_CHANNELS.VOICE_MODELS_LIST, async () => {
    return listVoiceModels()
  })

  ipcMain.handle(IPC_CHANNELS.VOICE_MODEL_DOWNLOAD, async (_event, payload: unknown) => {
    const req = parseVoiceModelRequest(payload)
    return downloadVoiceModel(req.modelId as VoiceModelId)
  })

  ipcMain.handle(IPC_CHANNELS.VOICE_MODEL_GET_SELECTED, async () => {
    return { modelId: getSelectedVoiceModelId() }
  })

  ipcMain.handle(IPC_CHANNELS.VOICE_MODEL_SET_SELECTED, async (_event, payload: unknown) => {
    const req = parseVoiceModelRequest(payload)
    return { modelId: setSelectedVoiceModelId(req.modelId as VoiceModelId) }
  })

  ipcMain.handle('window:show', async () => {
    const win = getWindow()
    if (win) {
      win.show()
      win.focus()
    }
  })

  ipcMain.handle('window:hide', async () => {
    const win = getWindow()
    if (win) win.hide()
  })

  ipcMain.handle('window:close-current', async (event) => {
    const win = BrowserWindow.fromWebContents(event.sender)
    if (win && !win.isDestroyed()) win.close()
  })

  ipcMain.handle('window:snap-drag-start', async () => {
    const win = getWindow()
    if (!win || win.isDestroyed()) return
    controls?.startWindowDragMonitoring(win)
  })

  ipcMain.handle('window:snap-drag-end', async () => {
    const win = getWindow()
    if (!win || win.isDestroyed()) return
    controls?.stopWindowDragMonitoring(win)
  })

  ipcMain.handle('storage:breakdown', async () => {
    return getStorageBreakdown()
  })

  ipcMain.handle('storage:clipboard-config:get', async () => {
    return getClipboardStorageConfig()
  })

  ipcMain.handle('storage:clipboard-config:set', async (_event, payload: unknown) => {
    const patch = typeof payload === 'object' && payload !== null ? (payload as Partial<ClipboardConfig>) : {}
    setClipboardStorageConfig(patch)
    return getClipboardStorageConfig()
  })

  ipcMain.handle('storage:clear-clipboard-images', async () => {
    return clearClipboardImages()
  })

  ipcMain.handle('storage:vacuum-search-db', async () => {
    return vacuumSearchDatabase()
  })

  ipcMain.handle('storage:clear-chromium-cache', async () => {
    await clearChromiumCache()
    return { ok: true }
  })
}
