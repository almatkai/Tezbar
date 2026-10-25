import {
  app,
  BrowserWindow,
  clipboard,
  desktopCapturer,
  dialog,
  globalShortcut,
  ipcMain,
  nativeImage,
  screen,
  shell,
} from '@tezbar/desktop-runtime'
import type { WebContents } from '@tezbar/desktop-runtime'
import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { decodePngDataUrl } from './clipboardPng'
import { setSuppressBlurHide } from './windowState'
import {
  AGENT_IPC,
  type AgentApprovalDecision,
  type AgentApprovalResponse,
  type AgentInputImage,
  type AgentRunEvent,
  type AgentRunRequest,
  type AgentTimelineItem,
  type Stage,
} from '../shared/agent'
import { CHAT_CONTEXT_MAX_TURNS, CHAT_IPC, type ChatSession, type ChatTurn } from '../shared/chat'
import type { ChatAttachment } from '../shared/chat'
import {
  appendChatTurn,
  clearAllChatSessions,
  deleteChatSession,
  deleteChatTurn,
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
import type { IconAssetKind, SearchAction } from '../shared/search'
import type { Message } from './llm/provider'
import { appIconDataUrl } from './appIcon'
import { imageFileDataUrl, nativeFileIconDataUrl } from './pathIcons'
import { quickLookFiles } from './quickLook'
import { setLauncherContentHeight } from './windowBounds'
import {
  addAgentAlwaysAllowedExactCommand,
  getAgentAlwaysAllowedExactCommands,
  getAgentAlwaysAllowedCommands,
  getSafetyDryRun,
  getUiStateRetentionMs,
  readRawConfig,
  setSafetyDryRun,
  writeConfigPatch,
  getCommandHotkeys,
  setCommandHotkeys,
  getCommandAliases,
  setCommandAliases,
  getDisabledCommands,
  setDisabledCommands,
  getCommandSurfaceTimeoutMs,
  getExtensionRuntimeTimeoutMs,
  getRaymesHotkey,
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
import { isCustomProvider } from '../shared/aiProviders'
import { classifyIntent } from './router'
import {
  executeSearchAction,
  completePath,
  recordDirectoryVisit,
  recordSearchActionUsage,
  getSearchBenchmarkHistory,
  listSearchCandidates,
  reindexExtensions,
  reindexQuickNotes,
  reindexSnippets,
  resolveLauncherDirectory,
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
import { runAiActionMode } from './llm/actionMode'
import type { VoiceModelId } from '../shared/voice'
import type {
  KnowledgeDepth,
  KnowledgeIndexingSpeed,
  KnowledgeRootDepth,
} from '../shared/knowledge'
import { listBackgroundTasks } from './backgroundTasks'
import { countsAsForegroundActivity, noteForegroundActivity } from './foreground'

type IpcHandleListener = Parameters<typeof ipcMain.handle>[1]

/** Register a backend channel.
 *
 *  Requests the UI waits for open the foreground gate, so background indexing
 *  yields between batches instead of making the reply queue behind its work.
 *  Status polling is excluded in `countsAsForegroundActivity`. */
function handleIpc(channel: string, listener: IpcHandleListener): void {
  ipcMain.handle(channel, (event, ...args) => {
    if (countsAsForegroundActivity(channel)) noteForegroundActivity()
    return listener(event, ...args)
  })
}
import { requestPermission, snapshotPermissions } from './permissions/manager'
import { checkPiAgent, installPiAgent } from './agent/piInstall'
import type { PermissionId } from '../shared/permissions'
import { clearSafetyLog, listSafetyLog } from './safety/log'
import { listSafetyDescriptors } from './safety/registry'
import { listNativeCommands } from './nativeCommands/registry'
import { getSystemStats } from './systemStats/service'
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
  TERMINAL_IPC,
  type TerminalAttachRequest,
  type TerminalCreateRequest,
  type TerminalUpdateRequest,
} from '../shared/terminal'
import {
  clearChromiumCache,
  clearClipboardImages,
  getClipboardStorageConfig,
  getStorageBreakdown,
  setClipboardStorageConfig,
  vacuumSearchDatabase,
} from './storage/service'
import { chooseKnowledgeFolder, getKnowledgeService } from './knowledge/service'

const LLM_DEFAULTS = {
  uiStateRetentionMs: 60_000,
} as const

let answerAbort: AbortController | null = null
let agentAbort: AbortController | null = null
let agentRunId: string | null = null
let extensionRunnerModule: typeof import('./extension-runner') | null = null
let terminalModule: typeof import('./terminal/service') | null = null
let agentBridgeModule: typeof import('./agent/bridge') | null = null
let extensionRegistryModule: typeof import('./extension-registry') | null = null
let extensionRegistryProgressAttached = false
let extensionProgressWindow: (() => BrowserWindow | null) | null = null

async function loadExtensionRegistry(): Promise<typeof import('./extension-registry')> {
  extensionRegistryModule ??= await import('./extension-registry')
  if (!extensionRegistryProgressAttached && extensionProgressWindow) {
    extensionRegistryProgressAttached = true
    extensionRegistryModule.extensionRegistryEvents.on('progress', (payload) => {
      const win = extensionProgressWindow?.()
      if (win && !win.isDestroyed()) win.webContents.send('extension:install-progress', payload)
    })
  }
  return extensionRegistryModule
}

async function loadExtensionRunner(): Promise<typeof import('./extension-runner')> {
  extensionRunnerModule ??= await import('./extension-runner')
  return extensionRunnerModule
}

async function loadTerminal(): Promise<typeof import('./terminal/service')> {
  terminalModule ??= await import('./terminal/service')
  return terminalModule
}

async function loadAgentBridge(): Promise<typeof import('./agent/bridge')> {
  agentBridgeModule ??= await import('./agent/bridge')
  return agentBridgeModule
}
const pendingAgentApprovals = new Map<
  string,
  {
    runId: string
    command: string
    suggestedRule?: string
    settle: (approved: boolean) => void
  }
>()
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
            title: 'Quit Tezbar',
            message: 'Quit Tezbar?',
            detail: 'Are you sure you want to quit Tezbar and terminate all background processes?',
            noLink: true,
          })
        : await dialog.showMessageBox({
            type: 'question',
            buttons: ['Cancel', 'Quit'],
            defaultId: 1,
            cancelId: 0,
            title: 'Quit Tezbar',
            message: 'Quit Tezbar?',
            detail: 'Are you sure you want to quit Tezbar and terminate all background processes?',
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
  'You are Tezbar, a helpful assistant. Answer clearly and concisely unless the user asks for more detail.'

type IpcControls = {
  startWindowDragMonitoring: (win: BrowserWindow) => void
  stopWindowDragMonitoring: (win: BrowserWindow) => void
  showCommandBar?: () => void
  hideCommandBar?: () => void
  updateRaymesHotkey?: (accelerator: string) => {
    ok: boolean
    accelerator: string
    error?: string
  }
  updateCommandHotkey?: (
    commandId: string,
    hotkey: string
  ) => {
    ok: boolean
    error?: string
  }
  openAppSurface?: (surface: any) => void
}

function sendAgentEvent(sender: WebContents, event: AgentRunEvent): void {
  if (!sender.isDestroyed()) sender.send(AGENT_IPC.EVENT, event)
}

const PERSISTABLE_READ_COMMANDS = new Set(['grep', 'rg'])
const SAFE_PIPELINE_COMMANDS = new Set(['ps', 'head', 'tail', 'wc'])

function commandName(part: string): string {
  const token = part.trim().split(/\s+/, 1)[0] ?? ''
  return token.slice(token.lastIndexOf('/') + 1).toLowerCase()
}

function suggestedApprovalRule(command: string): string | undefined {
  if (/[;<>`\n]/.test(command) || command.includes('$(') || command.includes('||')) {
    return undefined
  }
  const names = command
    .split(/\s*(?:&&|\|)\s*/)
    .map(commandName)
    .filter((name) => name && name !== 'cd' && !SAFE_PIPELINE_COMMANDS.has(name))
  return names.find((name) => PERSISTABLE_READ_COMMANDS.has(name))
}

function cancelPendingAgentApprovals(runId?: string): void {
  for (const [approvalId, approval] of pendingAgentApprovals) {
    if (runId && approval.runId !== runId) continue
    pendingAgentApprovals.delete(approvalId)
    approval.settle(false)
  }
}

function requestAgentApproval(
  sender: WebContents,
  runId: string,
  signal: AbortSignal,
  request: { title: string; command: string }
): Promise<boolean> {
  const exactCommand = request.command.trim()
  if (exactCommand && getAgentAlwaysAllowedExactCommands().includes(exactCommand)) {
    return Promise.resolve(true)
  }

  const approvalId = `approval-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
  const suggestedRule = suggestedApprovalRule(request.command)

  return new Promise((resolve) => {
    let settled = false
    const timeout = setTimeout(() => settle(false), 5 * 60_000)
    const settle = (approved: boolean): void => {
      if (settled) return
      settled = true
      clearTimeout(timeout)
      pendingAgentApprovals.delete(approvalId)
      resolve(approved)
    }
    pendingAgentApprovals.set(approvalId, {
      runId,
      command: request.command,
      suggestedRule,
      settle,
    })
    signal.addEventListener('abort', () => settle(false), { once: true })
    sendAgentEvent(sender, {
      type: 'approval',
      runId,
      approvalId,
      title: request.title,
      command: request.command,
      suggestedRule,
    })
  })
}

function waitForRetry(ms: number, signal: AbortSignal): Promise<void> {
  if (signal.aborted) return Promise.resolve()
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, ms)
    signal.addEventListener(
      'abort',
      () => {
        clearTimeout(timer)
        resolve()
      },
      { once: true }
    )
  })
}

function canRetryRun(error: unknown, signal: AbortSignal): boolean {
  if (signal.aborted) return false
  const message = error instanceof Error ? error.message : String(error)
  return !/aborted|cancelled|task is empty|invalid base64|unsupported agent image/i.test(message)
}

function startAgentRun(
  sender: WebContents,
  task: string,
  images: readonly AgentInputImage[] = [],
  cwd?: string,
  requestedRunId?: string
): string {
  cancelPendingAgentApprovals()
  agentAbort?.abort()
  agentAbort = new AbortController()
  const ac = agentAbort
  const runId = (agentRunId =
    requestedRunId ?? `agent-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`)
  let emittedOutput = false

  sendAgentEvent(sender, { type: 'start', runId, task })

  const onStage = (stage: Stage): void => {
    emittedOutput = true
    sendAgentEvent(sender, { type: 'stage', runId, stage })
  }
  const onMessageDelta = (delta: string): void => {
    emittedOutput = true
    sendAgentEvent(sender, { type: 'message', runId, delta })
  }
  const onAnswer = (text: string): void => {
    emittedOutput = true
    sendAgentEvent(sender, { type: 'answer', runId, text })
  }

  const onStderrLine = (line: string): void => {
    sendAgentEvent(sender, { type: 'log', runId, source: 'stderr', line })
  }

  void (async () => {
    const [{ getSharedBridge }, { extractTextFromAgentImages }] = await Promise.all([
      loadAgentBridge(),
      import('./agent/imageContext'),
    ])
    const bridge = getSharedBridge()
    const piProvider = getSelectedPiProviderBridge('chat')
    let runTask = task
    let runImages = images
    if (images.length > 0 && piProvider && !piProvider.acceptsImages) {
      onStderrLine('The selected model is text-only. Extracting screen text locally...')
      const extractedText = await extractTextFromAgentImages(images)
      if (!extractedText) {
        throw new Error(
          'The selected model cannot read images, and no text could be extracted from the attached screen. Choose a vision model or remove the attachment.'
        )
      }
      runTask = `${task}\n\nText extracted locally from the attached screen:\n\n${extractedText}`
      runImages = []
    }

    const options = {
      runId,
      model: piProvider?.modelPattern ?? getSelectedPiModelPattern('chat'),
      raymesProviderJson: piProvider?.providerJson,
      raymesAlwaysAllowJson: JSON.stringify(getAgentAlwaysAllowedCommands()),
      raymesAlwaysAllowExactJson: JSON.stringify(getAgentAlwaysAllowedExactCommands()),
      requestApproval: (request: { title: string; command: string }) =>
        requestAgentApproval(sender, runId, ac.signal, request),
      signal: ac.signal,
      onStage,
      onMessageDelta,
      onAnswer,
      onStderrLine,
      images: runImages,
      cwd,
    }

    try {
      await bridge.run(runTask, options)
    } catch (error) {
      if (emittedOutput || !canRetryRun(error, ac.signal)) throw error
      onStderrLine('Pi did not start cleanly. Retrying once...')
      await waitForRetry(350, ac.signal)
      if (ac.signal.aborted) throw error
      await bridge.run(runTask, options)
    }
  })()
    .then(() => {
      sendAgentEvent(sender, { type: 'done', runId })
    })
    .catch((err: unknown) => {
      const message = err instanceof Error ? err.message : String(err)
      sendAgentEvent(sender, { type: 'error', runId, message })
      sendAgentEvent(sender, { type: 'done', runId })
    })
    .finally(() => {
      cancelPendingAgentApprovals(runId)
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

function normalizeChatAttachments(raw: unknown): ChatAttachment[] | undefined {
  if (!Array.isArray(raw)) return undefined
  const attachments: ChatAttachment[] = []
  for (const item of raw.slice(0, 4)) {
    if (!item || typeof item !== 'object') continue
    const attachment = item as Partial<ChatAttachment>
    if (
      attachment.kind !== 'image' ||
      typeof attachment.name !== 'string' ||
      (attachment.mimeType !== 'image/png' &&
        attachment.mimeType !== 'image/jpeg' &&
        attachment.mimeType !== 'image/webp')
    ) {
      continue
    }
    attachments.push({
      kind: 'image',
      name: attachment.name.slice(0, 120),
      mimeType: attachment.mimeType,
      width: typeof attachment.width === 'number' ? attachment.width : undefined,
      height: typeof attachment.height === 'number' ? attachment.height : undefined,
    })
  }
  return attachments.length > 0 ? attachments : undefined
}

function startChatRun(sender: WebContents, turns: ChatTurn[]): string {
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
      let lastError: unknown
      for (let attempt = 0; attempt < 2; attempt += 1) {
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
          lastError = undefined
          break
        } catch (error) {
          lastError = error
          if (fullText || attempt > 0 || !canRetryRun(error, ac.signal)) throw error
          await waitForRetry(350, ac.signal)
        }
      }
      if (lastError) throw lastError
      if (!fullText.trim())
        throw new Error('The model returned an empty response. Please try again.')
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
/** Called from `main/index.ts` on `will-quit` to flush subprocesses. */
export function shutdownIpcHandlers(): void {
  answerAbort?.abort()
  cancelPendingAgentApprovals()
  agentAbort?.abort()
  extensionRunnerModule?.clearAllExtensionSessions()
  agentBridgeModule?.disposeSharedBridge()
  terminalModule?.shutdownTerminalSessions()
}

export function registerIpcHandlers(
  getWindow: () => BrowserWindow | null,
  controls?: IpcControls
): void {
  extensionProgressWindow = getWindow

  handleIpc('llm-config-get', async () => ({
    ...LLM_DEFAULTS,
    ...readLLMConfig(),
    ...readRawConfig(),
    raymesHotkey: getRaymesHotkey(),
    uiStateRetentionMs: getUiStateRetentionMs(),
    extensionRuntimeTimeoutMs: getExtensionRuntimeTimeoutMs(),
    aiModeTimeoutMs: getCommandSurfaceTimeoutMs('aiModeTimeoutMs'),
    terminalModeTimeoutMs: getCommandSurfaceTimeoutMs('terminalModeTimeoutMs'),
  }))

  handleIpc('llm-config-set', async (_event, patch: unknown) => {
    if (!patch || typeof patch !== 'object') return
    const configPatch = { ...(patch as Record<string, unknown>) }
    const requestedHotkey = configPatch.raymesHotkey
    delete configPatch.raymesHotkey

    // Notify every renderer that the LLM config changed, so views that cache
    // `getLlmConfig()` on mount (CommandBar, AgentChatView, ProvidersView) can
    // re-fetch and keep the model picker in sync with Settings edits.
    const broadcast = (): void => {
      for (const win of BrowserWindow.getAllWindows()) {
        if (win.isDestroyed()) continue
        try {
          win.webContents.send('llm-config-changed', {})
        } catch {}
      }
    }

    if (typeof requestedHotkey === 'string' && controls?.updateRaymesHotkey) {
      const result = controls.updateRaymesHotkey(requestedHotkey)
      if (!result.ok) return result
      if (Object.keys(configPatch).length > 0) writeConfigPatch(configPatch)
      invalidateProviderCache()
      broadcast()
      return result
    }

    writeConfigPatch(configPatch)
    invalidateProviderCache()
    broadcast()
  })

  handleIpc('llm-provider-statuses', async () => {
    const cfg = readLLMConfig()
    const ids: ProviderId[] = [
      'openai',
      'openai-compatible',
      'tokenrouter',
      'anthropic',
      'ollama',
      'copilot',
      'gemini',
      'opencode',
      'deepseek',
      'antigravity',
      ...(cfg.customProviders?.map((provider) => provider.id) ?? []),
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

  handleIpc(
    'llm-list-models',
    async (_event, providerId: unknown, baseURLOverride?: unknown, apiKeyOverride?: unknown) => {
      const id = providerId as ProviderId
      const isCustom =
        typeof id === 'string' &&
        (isCustomProvider(id) ||
          id.startsWith('custom:') ||
          readLLMConfig().customProviders?.some(
            (provider) => (provider.id as string) === (id as string)
          ))
      if (
        id !== 'openai' &&
        id !== 'openai-compatible' &&
        id !== 'tokenrouter' &&
        id !== 'anthropic' &&
        id !== 'ollama' &&
        id !== 'copilot' &&
        id !== 'gemini' &&
        id !== 'opencode' &&
        id !== 'deepseek' &&
        id !== 'antigravity' &&
        !isCustom
      )
        return []
      try {
        return await listModelsForProvider(
          id,
          undefined,
          typeof baseURLOverride === 'string' ? baseURLOverride : undefined,
          typeof apiKeyOverride === 'string' ? apiKeyOverride : undefined
        )
      } catch {
        return []
      }
    }
  )

  handleIpc('pi-extensions:list', async () => {
    const { getInstalledPiExtensions } = await import('./agent/piExtensions')
    return getInstalledPiExtensions()
  })

  // Renderer reports its measured content height. We clamp to the launcher
  // bounds and update the window content size programmatically — the user
  // still cannot drag to resize because the BrowserWindow is resizable:false.
  handleIpc('window-set-content-height', async (_event, raw: unknown) => {
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

  handleIpc('pi:check', async () => checkPiAgent())

  handleIpc('pi:install', async () => installPiAgent())

  handleIpc('permissions:snapshot', async () => snapshotPermissions())

  handleIpc('permissions:request', async (_event, raw: unknown) => {
    if (typeof raw !== 'string') {
      throw new Error('Permission id must be a string')
    }
    return requestPermission(raw as PermissionId)
  })

  handleIpc('safety:descriptors', async () => listSafetyDescriptors())

  handleIpc('safety:log', async () => listSafetyLog())

  handleIpc('safety:log-clear', async () => {
    clearSafetyLog()
  })

  handleIpc('safety:dry-run:get', async () => getSafetyDryRun())
  handleIpc('safety:dry-run:set', async (_event, raw: unknown) => {
    setSafetyDryRun(raw === true)
    return getSafetyDryRun()
  })

  handleIpc('native-commands:list', async () => listNativeCommands())
  handleIpc('system-stats:get', async () => getSystemStats())

  handleIpc('clipboard:list', async () => listClipboardEntries())

  handleIpc('clipboard:restore', async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return false
    return restoreClipboardEntry(id)
  })

  handleIpc('clipboard:delete', async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return false
    return deleteClipboardEntry(id)
  })

  handleIpc('clipboard:toggle-pin', async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return false
    return togglePinClipboardEntry(id)
  })

  handleIpc('clipboard:reveal', async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return false
    return revealClipboardEntryInFinder(id)
  })

  handleIpc('clipboard:image', async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return null
    return readClipboardImagePayload(id)
  })

  handleIpc('clipboard:clear', async () => {
    clearClipboardHistory()
  })

  handleIpc('app-icon:data-url', async (_event, raw: unknown) => {
    const appPath = typeof raw === 'string' ? raw.trim() : ''
    if (
      !appPath ||
      (process.platform === 'darwin' && !appPath.endsWith('.app')) ||
      (process.platform === 'win32' &&
        !appPath.startsWith('shell:AppsFolder\\') &&
        !/\.(?:exe|lnk|url)$/i.test(appPath))
    )
      return null
    return (await appIconDataUrl(appPath)) ?? null
  })

  handleIpc('asset-icon:data-url', async (_event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return null
    const payload = raw as { kind?: unknown; path?: unknown }
    const kind = payload.kind as IconAssetKind
    const path = typeof payload.path === 'string' ? payload.path.trim() : ''
    if (!path) return null

    if (kind === 'application') {
      if (
        (process.platform === 'darwin' && !path.endsWith('.app')) ||
        (process.platform === 'win32' &&
          !path.startsWith('shell:AppsFolder\\') &&
          !/\.(?:exe|lnk|url)$/i.test(path))
      )
        return null
      return (await appIconDataUrl(path)) ?? null
    }
    if (kind === 'extension') {
      if (/^https?:\/\//i.test(path)) return path
      return imageFileDataUrl(path) ?? null
    }
    if (kind === 'file') {
      return (await nativeFileIconDataUrl(path)) ?? null
    }
    return null
  })

  handleIpc('snippets:list', async () => listSnippetsForUi())

  handleIpc('snippets:copy', async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return { ok: false, message: 'Invalid snippet' }
    return copySnippetById(id)
  })

  handleIpc('snippets:add', async (_event, payload: unknown) => {
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

  handleIpc('snippets:update', async (_event, id: unknown, payload: unknown) => {
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

  handleIpc('snippets:delete', async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return { ok: false, message: 'Invalid snippet id' }
    const r = deleteUserSnippet(id)
    if (r.ok) await reindexSnippets()
    return r
  })

  handleIpc('notes:list', async () => listQuickNotes())

  handleIpc('notes:append', async (_event, text: unknown) => {
    if (typeof text !== 'string' || !text.trim()) return null
    const entry = addQuickNote(text)
    await reindexQuickNotes()
    return entry
  })

  handleIpc('notes:update', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') return false
    const o = payload as { createdAt?: unknown; text?: unknown }
    if (typeof o.createdAt !== 'number' || typeof o.text !== 'string') return false
    const ok = updateQuickNote(o.createdAt, o.text)
    if (ok) await reindexQuickNotes()
    return ok
  })

  handleIpc('notes:delete', async (_event, createdAt: unknown) => {
    if (typeof createdAt !== 'number') return false
    const ok = deleteQuickNote(createdAt)
    if (ok) await reindexQuickNotes()
    return ok
  })

  handleIpc(TERMINAL_IPC.CREATE, async (event, raw: unknown) => {
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
    if (body.restoreSessionId !== undefined && typeof body.restoreSessionId !== 'string') {
      throw new Error('Invalid terminal session to restore')
    }
    if (body.restoreCommand !== undefined && typeof body.restoreCommand !== 'string') {
      throw new Error('Invalid restored terminal command')
    }
    if (body.name !== undefined && typeof body.name !== 'string') {
      throw new Error('Invalid terminal session name')
    }
    if ((body.initialCommand?.length ?? 0) > 16 * 1024) {
      throw new Error('Initial terminal command is too long')
    }
    const { createTerminalSession } = await loadTerminal()
    return createTerminalSession(event.sender, body as TerminalCreateRequest)
  })

  handleIpc(TERMINAL_IPC.ATTACH, async (event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return null
    const body = raw as Partial<TerminalAttachRequest>
    if (
      typeof body.sessionId !== 'string' ||
      typeof body.cols !== 'number' ||
      typeof body.rows !== 'number'
    ) {
      return null
    }
    const { attachTerminalSession } = await loadTerminal()
    return attachTerminalSession(event.sender, body as TerminalAttachRequest)
  })

  handleIpc(TERMINAL_IPC.DETACH, async (event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return false
    const body = raw as { sessionId?: unknown }
    if (typeof body.sessionId !== 'string') return false
    const { detachTerminalSession } = await loadTerminal()
    return detachTerminalSession(event.sender.id, body.sessionId)
  })

  handleIpc(TERMINAL_IPC.LIST, async (event) => {
    const { listTerminalSessions } = await loadTerminal()
    return listTerminalSessions(event.sender.id)
  })

  handleIpc(TERMINAL_IPC.UPDATE, async (event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return null
    const body = raw as Partial<TerminalUpdateRequest>
    if (typeof body.sessionId !== 'string') return null
    if (body.name !== undefined && typeof body.name !== 'string') return null
    if (body.cwd !== undefined && typeof body.cwd !== 'string') return null
    if (body.lastCommand !== undefined && typeof body.lastCommand !== 'string') return null
    if ((body.lastCommand?.length ?? 0) > 16 * 1024) return null
    const { updateTerminalSession } = await loadTerminal()
    return updateTerminalSession(event.sender.id, body as TerminalUpdateRequest)
  })

  handleIpc('terminal:native-create', async (_event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return null
    const body = raw as {
      sessionId?: unknown
      shell?: unknown
      cwd?: unknown
      initialCommand?: unknown
      name?: unknown
      saveFor?: unknown
      keepAliveFor?: unknown
    }
    if (
      typeof body.sessionId !== 'string' ||
      typeof body.shell !== 'string' ||
      typeof body.cwd !== 'string'
    ) {
      return null
    }
    if (body.initialCommand !== undefined && typeof body.initialCommand !== 'string') return null
    if (body.name !== undefined && typeof body.name !== 'string') return null
    const initialCommand = typeof body.initialCommand === 'string' ? body.initialCommand : undefined
    const name = typeof body.name === 'string' ? body.name : undefined
    const saveFor =
      body.saveFor === 'day' ||
      body.saveFor === 'week' ||
      body.saveFor === 'month' ||
      body.saveFor === 'forever'
        ? body.saveFor
        : undefined
    const keepAliveFor =
      body.keepAliveFor === '3h' ||
      body.keepAliveFor === '8h' ||
      body.keepAliveFor === 'day' ||
      body.keepAliveFor === 'until-stop'
        ? body.keepAliveFor
        : undefined
    if ((initialCommand?.length ?? 0) > 16 * 1024) return null
    const { recordNativeTerminalSession } = await loadTerminal()
    return recordNativeTerminalSession({
      sessionId: body.sessionId,
      shell: body.shell,
      cwd: body.cwd,
      initialCommand,
      name,
      saveFor,
      keepAliveFor,
    })
  })

  handleIpc('terminal:native-attach', async (_event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return null
    const body = raw as { sessionId?: unknown }
    if (typeof body.sessionId !== 'string') return null
    const { markNativeTerminalSessionAttached } = await loadTerminal()
    return markNativeTerminalSessionAttached(body.sessionId)
  })

  handleIpc('terminal:native-restore', async (_event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return null
    const body = raw as { sessionId?: unknown; shell?: unknown; cwd?: unknown }
    if (
      typeof body.sessionId !== 'string' ||
      typeof body.shell !== 'string' ||
      typeof body.cwd !== 'string'
    ) {
      return null
    }
    const { markNativeTerminalSessionRestored } = await loadTerminal()
    return markNativeTerminalSessionRestored({
      sessionId: body.sessionId,
      shell: body.shell,
      cwd: body.cwd,
    })
  })

  handleIpc('terminal:native-exit', async (_event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return null
    const body = raw as { sessionId?: unknown; exitCode?: unknown; signal?: unknown }
    if (typeof body.sessionId !== 'string') return null
    const exitCode =
      typeof body.exitCode === 'number' && Number.isFinite(body.exitCode)
        ? Math.max(0, Math.round(body.exitCode))
        : 1
    const signal =
      typeof body.signal === 'number' && Number.isFinite(body.signal)
        ? Math.max(0, Math.round(body.signal))
        : undefined
    const { markNativeTerminalSessionExited } = await loadTerminal()
    return markNativeTerminalSessionExited(body.sessionId, exitCode, signal)
  })

  handleIpc(TERMINAL_IPC.WRITE, async (event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return false
    const body = raw as { sessionId?: unknown; data?: unknown }
    if (typeof body.sessionId !== 'string' || typeof body.data !== 'string') return false
    const { writeTerminalSession } = await loadTerminal()
    return writeTerminalSession(event.sender.id, body.sessionId, body.data)
  })

  handleIpc(TERMINAL_IPC.RESIZE, async (event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return false
    const body = raw as { sessionId?: unknown; cols?: unknown; rows?: unknown }
    if (
      typeof body.sessionId !== 'string' ||
      typeof body.cols !== 'number' ||
      typeof body.rows !== 'number'
    ) {
      return false
    }
    const { resizeTerminalSession } = await loadTerminal()
    return resizeTerminalSession(event.sender.id, body.sessionId, body.cols, body.rows)
  })

  handleIpc(TERMINAL_IPC.GET_CWD, async (event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return null
    const body = raw as { sessionId?: unknown }
    if (typeof body.sessionId !== 'string') return null
    const { getTerminalSessionCwd } = await loadTerminal()
    return getTerminalSessionCwd(event.sender.id, body.sessionId)
  })

  handleIpc(TERMINAL_IPC.KILL, async (event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return false
    const body = raw as { sessionId?: unknown }
    if (typeof body.sessionId !== 'string') return false
    const { killTerminalSession } = await loadTerminal()
    return killTerminalSession(event.sender.id, body.sessionId)
  })

  handleIpc(TERMINAL_IPC.DELETE, async (event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return false
    const body = raw as { sessionId?: unknown }
    if (typeof body.sessionId !== 'string') return false
    const { deleteTerminalSession } = await loadTerminal()
    return deleteTerminalSession(event.sender.id, body.sessionId)
  })

  handleIpc(TERMINAL_IPC.GET_PROMPT_INFO, async () => {
    const { getTerminalPromptInfo } = await loadTerminal()
    return getTerminalPromptInfo()
  })

  handleIpc('open-external-url', async (_event, url: unknown) => {
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

  handleIpc('github-device-start', async (_event, clientId: unknown) => {
    return startGithubDeviceFlow(typeof clientId === 'string' ? clientId : undefined)
  })

  handleIpc('github-device-poll', async () => {
    const r = await pollGithubDeviceFlow()
    if (r.status === 'success') {
      persistCopilotTokens(r.access_token, r.refresh_token, r.expires_in, r.client_id)
      invalidateProviderCache()
    }
    return r
  })

  handleIpc('github-device-cancel', async () => {
    clearDeviceSession()
  })

  handleIpc(IPC_CHANNELS.QUERY, async (event, input: unknown) => {
    const text = typeof input === 'string' ? input : String(input ?? '')
    console.log('[IPC_CHANNELS.QUERY] received input:', text)
    const intent = await classifyIntent(text)
    console.log('[IPC_CHANNELS.QUERY] classified intent:', intent)
    if (intent.type === 'answer' || intent.type === 'ai') {
      console.log('[IPC_CHANNELS.QUERY] starting streamAnswerToRenderer')
      answerAbort?.abort()
      answerAbort = new AbortController()
      const ac = answerAbort
      void import('./llm/answerStream')
        .then(({ streamAnswerToRenderer }) =>
          streamAnswerToRenderer(event.sender, intent.input, ac.signal)
        )
        .finally(() => {
          if (answerAbort === ac) answerAbort = null
        })
    }
    if (intent.type === 'agent') {
      console.log('[IPC_CHANNELS.QUERY] starting startAgentRun')
      startAgentRun(event.sender, intent.input)
    }
    return intent
  })

  handleIpc('cancel', async () => {
    answerAbort?.abort()
    agentAbort?.abort()
  })

  handleIpc(AGENT_IPC.RUN, async (event, raw: unknown) => {
    const request: AgentRunRequest =
      typeof raw === 'string'
        ? { task: raw }
        : raw && typeof raw === 'object'
          ? (raw as AgentRunRequest)
          : { task: '' }
    const task = typeof request.task === 'string' ? request.task : ''
    if (!task.trim()) {
      return { ok: false, error: 'Task is empty' }
    }
    if (request.cwd !== undefined && typeof request.cwd !== 'string') {
      return { ok: false, error: 'Invalid agent working directory' }
    }
    if (
      request.runId !== undefined &&
      (typeof request.runId !== 'string' || !/^agent-[a-zA-Z0-9-]{1,100}$/.test(request.runId))
    ) {
      return { ok: false, error: 'Invalid agent run id' }
    }
    const images = Array.isArray(request.images) ? request.images : []
    const requestedCwd = request.cwd?.trim() ?? ''
    const cwd = requestedCwd ? (resolveLauncherDirectory(requestedCwd) ?? undefined) : undefined
    if (requestedCwd && !cwd) {
      return { ok: false, error: `Directory does not exist: ${requestedCwd}` }
    }
    const runId = startAgentRun(event.sender, task, images, cwd, request.runId)
    return { ok: true, runId }
  })

  handleIpc(AGENT_IPC.CAPTURE_ACTIVE_SCREEN, async (event) => {
    const display = screen.getDisplayNearestPoint(screen.getCursorScreenPoint())
    const maxWidth = 1600
    const scale = Math.min(1, maxWidth / Math.max(1, display.size.width))
    const thumbnailSize = {
      width: Math.max(1, Math.round(display.size.width * scale)),
      height: Math.max(1, Math.round(display.size.height * scale)),
    }
    const sourceWindow = BrowserWindow.fromWebContents(event.sender)
    const originalOpacity = sourceWindow?.getOpacity() ?? 1

    try {
      sourceWindow?.setContentProtection(true)
      sourceWindow?.setOpacity(0)
      await new Promise((resolve) => setTimeout(resolve, 120))
      const sources = await desktopCapturer.getSources({ types: ['screen'], thumbnailSize })
      const source =
        sources.find((candidate) => candidate.display_id === String(display.id)) ?? sources[0]
      if (!source || source.thumbnail.isEmpty()) {
        throw new Error(
          'The active screen could not be captured. Check Screen Recording permission.'
        )
      }
      const size = source.thumbnail.getSize()
      return {
        type: 'image' as const,
        data: source.thumbnail.toPNG().toString('base64'),
        mimeType: 'image/png' as const,
        width: size.width,
        height: size.height,
      }
    } finally {
      if (sourceWindow && !sourceWindow.isDestroyed()) {
        sourceWindow.setOpacity(originalOpacity)
        sourceWindow.setContentProtection(false)
      }
    }
  })

  handleIpc(AGENT_IPC.CANCEL, async (_event, expectedRunId: unknown) => {
    // Late cancellation from a closed view must not stop a newer conversation.
    if (expectedRunId !== undefined && expectedRunId !== agentRunId) return { ok: true }
    cancelPendingAgentApprovals(agentRunId ?? undefined)
    agentAbort?.abort()
    return { ok: true }
  })

  handleIpc(AGENT_IPC.APPROVE, async (_event, raw: unknown) => {
    if (!raw || typeof raw !== 'object') return { ok: false, error: 'Invalid approval' }
    const response = raw as Partial<AgentApprovalResponse>
    const decision = response.decision as AgentApprovalDecision | undefined
    if (
      typeof response.runId !== 'string' ||
      typeof response.approvalId !== 'string' ||
      (decision !== 'deny' && decision !== 'once' && decision !== 'always')
    ) {
      return { ok: false, error: 'Invalid approval' }
    }

    const pending = pendingAgentApprovals.get(response.approvalId)
    if (!pending || pending.runId !== response.runId) {
      return { ok: false, error: 'This approval is no longer active' }
    }
    if (decision === 'always') {
      addAgentAlwaysAllowedExactCommand(pending.command)
    }
    pending.settle(decision !== 'deny')
    return { ok: true }
  })

  // --- Chat sessions ------------------------------------------------------
  // The renderer owns the conversation state machine (30s continuation window,
  // active session, etc.) and tells us what to persist. We just provide
  // durable storage + list/get/delete/clear operations against sqlite.
  handleIpc(CHAT_IPC.RUN, async (event, rawTurns: unknown) => {
    const turns = normalizeChatTurns(rawTurns)
    if (!turns || turns.length === 0) {
      return { ok: false, error: 'Invalid chat run payload' }
    }
    const runId = startChatRun(event.sender, turns)
    return { ok: true, runId }
  })

  handleIpc(CHAT_IPC.LIST, async (_event, rawLimit: unknown) => {
    const limit =
      typeof rawLimit === 'number' && Number.isFinite(rawLimit) && rawLimit > 0
        ? Math.floor(rawLimit)
        : undefined
    return listChatSessions(limit)
  })

  handleIpc(CHAT_IPC.GET, async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return null
    return getChatSession(id)
  })

  handleIpc(CHAT_IPC.APPEND, async (_event, payload: unknown) => {
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
      typeof s.updatedAt !== 'number' ||
      (s.workingDirectory !== undefined && typeof s.workingDirectory !== 'string')
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
      await upsertChatSession({
        id: s.id,
        title: s.title,
        createdAt: s.createdAt,
        updatedAt: s.updatedAt,
        workingDirectory: s.workingDirectory,
      })
      await appendChatTurn(s.id, {
        id: t.id,
        role: t.role,
        text: t.text,
        responseMeta:
          t.responseMeta &&
          typeof t.responseMeta.provider === 'string' &&
          typeof t.responseMeta.providerTitle === 'string' &&
          typeof t.responseMeta.model === 'string'
            ? {
                provider: t.responseMeta.provider,
                providerTitle: t.responseMeta.providerTitle,
                model: t.responseMeta.model,
                tokenCount:
                  typeof t.responseMeta.tokenCount === 'number'
                    ? Math.max(0, Math.round(t.responseMeta.tokenCount))
                    : undefined,
              }
            : undefined,
        stages: Array.isArray(t.stages) ? (t.stages as Stage[]) : undefined,
        timeline: Array.isArray(t.timeline) ? (t.timeline as AgentTimelineItem[]) : undefined,
        error: typeof t.error === 'string' ? t.error : undefined,
        attachments: normalizeChatAttachments(t.attachments),
        createdAt: t.createdAt,
      })
      return { ok: true }
    } catch (err) {
      return { ok: false, error: err instanceof Error ? err.message : String(err) }
    }
  })

  handleIpc(CHAT_IPC.DELETE_TURN, async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') {
      return { ok: false, error: 'Invalid chat turn delete payload' }
    }
    const body = payload as { sessionId?: unknown; turnId?: unknown }
    if (typeof body.sessionId !== 'string' || typeof body.turnId !== 'string') {
      return { ok: false, error: 'Invalid chat turn delete payload' }
    }
    try {
      return { ok: await deleteChatTurn(body.sessionId, body.turnId) }
    } catch (err) {
      return { ok: false, error: err instanceof Error ? err.message : String(err) }
    }
  })

  handleIpc(CHAT_IPC.UPDATE_TITLE, async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') return { ok: false }
    const body = payload as { id?: unknown; title?: unknown }
    if (typeof body.id !== 'string' || typeof body.title !== 'string') {
      return { ok: false }
    }
    try {
      await updateChatSessionTitle(body.id, body.title)
      return { ok: true }
    } catch {
      return { ok: false }
    }
  })

  handleIpc(CHAT_IPC.DELETE, async (_event, id: unknown) => {
    if (typeof id !== 'string' || !id) return { ok: false }
    return { ok: await deleteChatSession(id) }
  })

  handleIpc(CHAT_IPC.CLEAR, async () => {
    try {
      await clearAllChatSessions()
      return { ok: true }
    } catch {
      return { ok: false }
    }
  })

  handleIpc('app:confirm-quit', async () => confirmQuitRaymes(getWindow))

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

  handleIpc('get-extensions', async () => {
    const { listInstalledExtensions } = await import('./extensions/service')
    return listInstalledExtensions()
  })

  handleIpc('extensions:listInstalled', async () => {
    const { listInstalledExtensions } = await import('./extensions/service')
    return listInstalledExtensions()
  })

  handleIpc('extensions:searchStore', async (_event, query: unknown) => {
    const q = typeof query === 'string' ? query : ''
    const { searchStoreExtensions } = await import('./extensions/service')
    return searchStoreExtensions(q)
  })

  handleIpc('extensions:install', async (_event, extensionId: unknown) => {
    if (typeof extensionId !== 'string' || !extensionId.trim()) {
      throw new Error('A valid extension id is required')
    }
    const { installExtension } = await import('./extensions/service')
    const result = await installExtension(extensionId)
    await reindexExtensions()
    return result
  })

  handleIpc('extensions:uninstall', async (_event, extensionId: unknown) => {
    if (typeof extensionId !== 'string' || !extensionId.trim()) {
      throw new Error('A valid extension id is required')
    }
    const { uninstallExtension } = await import('./extensions/service')
    const result = await uninstallExtension(extensionId)
    await reindexExtensions()
    return result
  })

  handleIpc('extensions:integrity', async (_event, extensionId: unknown) => {
    if (typeof extensionId !== 'string' || !extensionId.trim()) {
      throw new Error('A valid extension id is required')
    }
    const { inspectExtensionIntegrity } = await import('./extensions/service')
    return inspectExtensionIntegrity(extensionId)
  })

  handleIpc('extensions:reinstall', async (_event, extensionId: unknown) => {
    if (typeof extensionId !== 'string' || !extensionId.trim()) {
      throw new Error('A valid extension id is required')
    }
    const { reinstallExtension } = await import('./extensions/service')
    return reinstallExtension(extensionId)
  })

  handleIpc('extensions:install-error', async (_event, extensionId: unknown) => {
    if (typeof extensionId !== 'string' || !extensionId.trim()) return null
    const { getExtensionInstallError } = await import('./extensions/service')
    return getExtensionInstallError(extensionId)
  })

  handleIpc('extension:list', async () => {
    const { listInstalledRegistryExtensions } = await loadExtensionRegistry()
    return listInstalledRegistryExtensions()
  })

  handleIpc('extension:search-store', async (_event, payload: unknown, maybeOptions?: unknown) => {
    let query = ''
    let offset: number | undefined
    let limit: number | undefined

    if (typeof payload === 'string') {
      query = payload
      if (maybeOptions && typeof maybeOptions === 'object') {
        const opt = maybeOptions as { offset?: unknown; limit?: unknown }
        if (typeof opt.offset === 'number') offset = opt.offset
        if (typeof opt.limit === 'number') limit = opt.limit
      }
    } else if (payload && typeof payload === 'object') {
      const body = payload as { query?: unknown; offset?: unknown; limit?: unknown }
      if (typeof body.query === 'string') query = body.query
      if (typeof body.offset === 'number') offset = body.offset
      if (typeof body.limit === 'number') limit = body.limit
    }

    const { searchExtensionCatalog } = await loadExtensionRegistry()
    return searchExtensionCatalog(query, { offset, limit })
  })

  handleIpc('extension:install', async (_event, extensionId: unknown) => {
    if (typeof extensionId !== 'string' || !extensionId.trim()) {
      throw new Error('A valid extension id is required')
    }
    const { installRegistryExtension } = await loadExtensionRegistry()
    const result = await installRegistryExtension(extensionId)
    await reindexExtensions()
    return result
  })

  handleIpc('extension:uninstall', async (_event, extensionId: unknown) => {
    if (typeof extensionId !== 'string' || !extensionId.trim()) {
      throw new Error('A valid extension id is required')
    }
    const { uninstallRegistryExtension } = await loadExtensionRegistry()
    const result = uninstallRegistryExtension(extensionId)
    await reindexExtensions()
    return result
  })

  handleIpc('extension:run-command', async (_event, payload: unknown) => {
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

    const { runExtensionCommand } = await loadExtensionRunner()
    return runExtensionCommand({
      extensionId: body.extensionId,
      commandName: body.commandName,
      argumentValues,
    })
  })

  handleIpc('extension:invoke-action', async (_event, payload: unknown) => {
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
              Array.isArray(value)
                ? value.filter((entry): entry is string => typeof entry === 'string')
                : typeof value === 'string' ||
                    typeof value === 'boolean' ||
                    typeof value === 'number'
                  ? value
                  : String(value ?? ''),
            ])
          )
        : undefined

    const { invokeExtensionAction } = await loadExtensionRunner()
    return invokeExtensionAction({
      sessionId: body.sessionId,
      actionId: body.actionId,
      formValues,
    })
  })

  handleIpc('extension:search-text-changed', async (_event, payload: unknown) => {
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

    const { updateSearchText } = await loadExtensionRunner()
    return updateSearchText({
      sessionId: body.sessionId,
      searchText: body.searchText,
    })
  })

  handleIpc('extension:refresh-session', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') {
      throw new Error('Invalid extension refresh payload')
    }

    const body = payload as { sessionId?: unknown }
    if (typeof body.sessionId !== 'string') {
      throw new Error('sessionId is required')
    }

    const { refreshExtensionSession } = await loadExtensionRunner()
    return refreshExtensionSession({ sessionId: body.sessionId })
  })

  handleIpc('extension:dispose-session', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') return false
    const body = payload as { sessionId?: unknown }
    if (typeof body.sessionId !== 'string') return false
    const { disposeExtensionSession } = await loadExtensionRunner()
    return disposeExtensionSession(body.sessionId)
  })

  handleIpc('extension:load-more', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') {
      throw new Error('Invalid extension pagination payload')
    }
    const body = payload as { sessionId?: unknown }
    if (typeof body.sessionId !== 'string') {
      throw new Error('sessionId is required')
    }
    const { loadMoreExtensionSession } = await loadExtensionRunner()
    return loadMoreExtensionSession({ sessionId: body.sessionId })
  })

  handleIpc('clipboard:read', async () => {
    return clipboard.readText()
  })

  handleIpc('clipboard:write', async (_event, raw: unknown) => {
    const text = typeof raw === 'string' ? raw : String(raw ?? '')
    await clipboard.writeTextAsync(text)
    return { ok: true }
  })

  handleIpc('clipboard:write-png', async (_event, raw: unknown) => {
    const bytes = typeof raw === 'string' ? decodePngDataUrl(raw) : null
    if (!bytes) return { ok: false, error: 'Invalid PNG data URL' }
    const path = join(app.getPath('temp'), 'tezbar-qr-clipboard.png')
    try {
      writeFileSync(path, bytes)
      const image = nativeImage.createFromPath(path)
      if (image.isEmpty()) return { ok: false, error: 'Could not read QR image' }
      clipboard.writeImage(image)
      return { ok: true }
    } catch (error) {
      return {
        ok: false,
        error: error instanceof Error ? error.message : 'Could not copy QR image',
      }
    }
  })

  handleIpc('shell:open', async (_event, raw: unknown) => {
    const target = typeof raw === 'string' ? raw.trim() : ''
    if (!target) return { ok: false }

    await shell.openExternal(target)
    return { ok: true }
  })

  handleIpc('preferences:get', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') return {}
    const body = payload as { extensionId?: unknown; commandName?: unknown }
    if (typeof body.extensionId !== 'string' || !body.extensionId.trim()) return {}

    const commandName = typeof body.commandName === 'string' ? body.commandName : undefined
    const { getExtensionPreferences } = await loadExtensionRegistry()
    return getExtensionPreferences(body.extensionId, commandName)
  })

  handleIpc('preferences:setup', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') return null
    const body = payload as { extensionId?: unknown; commandName?: unknown }
    if (typeof body.extensionId !== 'string' || !body.extensionId.trim()) return null
    const commandName = typeof body.commandName === 'string' ? body.commandName : undefined
    const { getExtensionPreferenceSetup } = await loadExtensionRegistry()
    return getExtensionPreferenceSetup(body.extensionId, commandName)
  })

  handleIpc('preferences:set', async (_event, payload: unknown) => {
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
    const { saveExtensionPreferences } = await loadExtensionRegistry()
    return saveExtensionPreferences(body.extensionId, values, commandName)
  })

  handleIpc('get-installed-extensions-settings-schema', async () => {
    const { getInstalledExtensionsSettingsSchema } = await import('./extension-builder')
    return getInstalledExtensionsSettingsSchema()
  })

  handleIpc('toggle-command-enabled', async (_event, commandId: string, enabled: boolean) => {
    const disabled = { ...getDisabledCommands() }
    if (enabled) {
      delete disabled[commandId]
    } else {
      disabled[commandId] = true
    }
    setDisabledCommands(disabled)
    await reindexExtensions()
    return true
  })

  handleIpc('update-command-hotkey', async (_event, commandId: string, hotkey: string) => {
    if (controls?.updateCommandHotkey) {
      return controls.updateCommandHotkey(commandId, hotkey)
    }
    return { ok: false, error: 'Controls not initialized' }
  })

  handleIpc('get-settings', async () => {
    return {
      commandHotkeys: getCommandHotkeys(),
      commandAliases: getCommandAliases(),
      disabledCommands: getDisabledCommands(),
    }
  })

  handleIpc('save-settings', async (_event, patch: unknown) => {
    if (!patch || typeof patch !== 'object') return {}
    const body = patch as { commandAliases?: Record<string, string> }
    if (body.commandAliases) {
      setCommandAliases(body.commandAliases)
    }
    await reindexExtensions()
    return { ok: true }
  })

  handleIpc('app:open-extensions', async () => {
    if (controls?.openAppSurface) {
      controls.openAppSurface('extensions')
    }
    return true
  })

  handleIpc(IPC_CHANNELS.SEARCH_ALL, async (_event, query: unknown) => {
    const q = typeof query === 'string' ? query : ''
    return searchEverything(q)
  })

  handleIpc(IPC_CHANNELS.SEARCH_CANDIDATES, async () => listSearchCandidates())

  handleIpc('knowledge:snapshot', async () => getKnowledgeService().snapshot())
  handleIpc('knowledge:embedding:set', async (_event, model: unknown) => {
    if (model !== 'built-in' && model !== 'embeddinggemma-2')
      throw new Error('Invalid embedding model')
    return getKnowledgeService().setEmbeddingModel(model)
  })
  handleIpc('knowledge:embedding:download', async () =>
    getKnowledgeService().downloadEmbeddingModel()
  )
  handleIpc('knowledge:embedding:cancel-download', async () =>
    getKnowledgeService().cancelEmbeddingDownload()
  )
  handleIpc('knowledge:embedding:remove', async () => getKnowledgeService().removeEmbeddingModel())

  handleIpc('background-tasks:list', async () => listBackgroundTasks())

  handleIpc('knowledge:sources:list', async (_event, payload: unknown) => {
    const body =
      payload && typeof payload === 'object'
        ? (payload as { query?: unknown; offset?: unknown; limit?: unknown })
        : {}
    return getKnowledgeService().listSources({
      query: typeof body.query === 'string' ? body.query : '',
      offset: typeof body.offset === 'number' ? body.offset : 0,
      limit: typeof body.limit === 'number' ? body.limit : 200,
    })
  })

  handleIpc('knowledge:choose-folder', async () => chooseKnowledgeFolder())

  handleIpc('knowledge:root:add', async (_event, path: unknown) => {
    if (typeof path !== 'string') throw new Error('A folder path is required')
    return getKnowledgeService().addRoot(path)
  })

  handleIpc('knowledge:roots:add-major', async () => getKnowledgeService().addMajorRoots())

  handleIpc('knowledge:root:remove', async (_event, rootId: unknown) => {
    if (typeof rootId !== 'string') throw new Error('A knowledge folder id is required')
    return getKnowledgeService().removeRoot(rootId)
  })

  handleIpc('knowledge:root:set-enabled', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') throw new Error('Invalid knowledge folder update')
    const body = payload as { rootId?: unknown; enabled?: unknown }
    if (typeof body.rootId !== 'string' || typeof body.enabled !== 'boolean') {
      throw new Error('Invalid knowledge folder update')
    }
    return getKnowledgeService().setRootEnabled(body.rootId, body.enabled)
  })

  handleIpc('knowledge:depth:set', async (_event, depth: unknown) => {
    if (typeof depth !== 'string' || !['off', 'basic', 'smart', 'deep'].includes(depth)) {
      throw new Error('Invalid Knowledge Depth')
    }
    return getKnowledgeService().setDepth(depth as KnowledgeDepth)
  })

  handleIpc('knowledge:speed:set', async (_event, speed: unknown) => {
    if (typeof speed !== 'string' || !['slow', 'medium', 'fast'].includes(speed)) {
      throw new Error('Invalid indexing speed')
    }
    return getKnowledgeService().setIndexingSpeed(speed as KnowledgeIndexingSpeed)
  })

  handleIpc('knowledge:root:set-depth', async (_event, payload: unknown) => {
    if (!payload || typeof payload !== 'object') throw new Error('Invalid folder Knowledge Depth')
    const body = payload as { rootId?: unknown; depth?: unknown }
    if (
      typeof body.rootId !== 'string' ||
      typeof body.depth !== 'string' ||
      !['inherit', 'off', 'basic', 'smart', 'deep'].includes(body.depth)
    ) {
      throw new Error('Invalid folder Knowledge Depth')
    }
    return getKnowledgeService().setRootDepth(body.rootId, body.depth as KnowledgeRootDepth)
  })

  handleIpc('knowledge:index:start', async () => getKnowledgeService().startIndexing())
  handleIpc('knowledge:index:pause', async () => getKnowledgeService().pause())
  handleIpc('knowledge:index:resume', async () => getKnowledgeService().resume())

  handleIpc(IPC_CHANNELS.PATH_COMPLETE, async (_event, query: unknown) => {
    const q = typeof query === 'string' ? query : ''
    return completePath(q)
  })

  handleIpc(IPC_CHANNELS.PATH_RESOLVE_DIRECTORY, async (_event, input: unknown) => {
    return typeof input === 'string' ? resolveLauncherDirectory(input) : null
  })

  handleIpc(IPC_CHANNELS.DIRECTORY_VISIT_RECORD, async (_event, path: unknown) => {
    if (typeof path === 'string') recordDirectoryVisit(path)
  })

  handleIpc(IPC_CHANNELS.QUICK_LOOK_FILE, async (_event, input: unknown) => {
    const paths = Array.isArray(input)
      ? input.filter((path): path is string => typeof path === 'string')
      : typeof input === 'string'
        ? [input]
        : []
    return quickLookFiles(paths)
  })

  handleIpc(IPC_CHANNELS.SEARCH_BENCHMARK_RUN, async () => {
    return runSearchBenchmarks()
  })

  handleIpc(IPC_CHANNELS.SEARCH_BENCHMARK_HISTORY, async () => {
    return getSearchBenchmarkHistory()
  })

  handleIpc('currency:frankfurter-latest', async (_event, from: unknown) => {
    if (typeof from !== 'string' || !from.trim()) {
      throw new Error('Frankfurter: currency code required')
    }
    return fetchFrankfurterLatest(from.trim())
  })

  handleIpc(IPC_CHANNELS.SEARCH_EXECUTE, async (_event, payload: unknown) => {
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

  handleIpc(IPC_CHANNELS.SEARCH_RECORD_USAGE, async (_event, payload: unknown) => {
    const request = parseSearchExecuteRequest(payload)
    await recordSearchActionUsage(request.action, request.context)
  })

  handleIpc(IPC_CHANNELS.AI_ACTION, async (_event, payload: unknown) => {
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

  handleIpc(IPC_CHANNELS.VOICE_TTS_SPEAK, async (_event, payload: unknown) => {
    const req = parseVoiceSpeakRequest(payload)
    const { speakText } = await import('./voice/service')
    await speakText(req.text)
    return { ok: true }
  })

  handleIpc(IPC_CHANNELS.VOICE_TTS_STOP, async () => {
    const { stopSpeaking } = await import('./voice/service')
    stopSpeaking()
    return { ok: true }
  })

  handleIpc(IPC_CHANNELS.VOICE_STT_MODES, async () => {
    const { listSttModes } = await import('./voice/service')
    return listSttModes()
  })

  handleIpc(IPC_CHANNELS.VOICE_STT_TRANSCRIBE, async (_event, payload: unknown) => {
    const req = parseVoiceTranscribeRequest(payload)
    const { transcribeAudio } = await import('./voice/service')
    return transcribeAudio(req)
  })

  // Renderer toggles this around Hold-to-Speak so the mic permission sheet
  // or any brief focus change while recording does not hide the launcher.
  handleIpc('window:suppress-blur-hide', async (_event, payload: unknown) => {
    setSuppressBlurHide(payload === true)
    return { ok: true }
  })

  handleIpc(IPC_CHANNELS.VOICE_MODELS_LIST, async () => {
    const { listVoiceModels } = await import('./voice/service')
    return listVoiceModels()
  })

  handleIpc(IPC_CHANNELS.VOICE_MODEL_DOWNLOAD, async (_event, payload: unknown) => {
    const req = parseVoiceModelRequest(payload)
    const { downloadVoiceModel } = await import('./voice/service')
    return downloadVoiceModel(req.modelId as VoiceModelId)
  })

  handleIpc(IPC_CHANNELS.VOICE_MODEL_DELETE, async (_event, payload: unknown) => {
    const req = parseVoiceModelRequest(payload)
    const { deleteVoiceModel } = await import('./voice/service')
    return { modelId: await deleteVoiceModel(req.modelId as VoiceModelId) }
  })

  handleIpc(IPC_CHANNELS.VOICE_MODEL_GET_SELECTED, async () => {
    const { getSelectedVoiceModelId } = await import('./voice/service')
    return { modelId: getSelectedVoiceModelId() }
  })

  handleIpc(IPC_CHANNELS.VOICE_MODEL_SET_SELECTED, async (_event, payload: unknown) => {
    const req = parseVoiceModelRequest(payload)
    const { setSelectedVoiceModelId } = await import('./voice/service')
    return { modelId: setSelectedVoiceModelId(req.modelId as VoiceModelId) }
  })

  handleIpc('window:show', async () => {
    if (controls?.showCommandBar) {
      controls.showCommandBar()
      return
    }
    const win = getWindow()
    if (win) {
      win.show()
      win.focus()
    }
  })

  handleIpc('window:hide', async () => {
    if (controls?.hideCommandBar) {
      controls.hideCommandBar()
      return
    }
    const win = getWindow()
    if (win) {
      controls?.stopWindowDragMonitoring(win)
      win.hide()
    }
  })

  handleIpc('window:close-current', async (event) => {
    const win = BrowserWindow.fromWebContents(event.sender)
    if (win && !win.isDestroyed()) win.close()
  })

  handleIpc('window:snap-drag-start', async () => {
    const win = getWindow()
    if (!win || win.isDestroyed()) return
    controls?.startWindowDragMonitoring(win)
  })

  handleIpc('window:snap-drag-end', async () => {
    const win = getWindow()
    if (!win || win.isDestroyed()) return
    controls?.stopWindowDragMonitoring(win)
  })

  handleIpc('storage:breakdown', async () => {
    return getStorageBreakdown()
  })

  handleIpc('storage:clipboard-config:get', async () => {
    return getClipboardStorageConfig()
  })

  handleIpc('storage:clipboard-config:set', async (_event, payload: unknown) => {
    const patch =
      typeof payload === 'object' && payload !== null ? (payload as Partial<ClipboardConfig>) : {}
    setClipboardStorageConfig(patch)
    return getClipboardStorageConfig()
  })

  handleIpc('storage:clear-clipboard-images', async () => {
    return clearClipboardImages()
  })

  handleIpc('storage:vacuum-search-db', async () => {
    return vacuumSearchDatabase()
  })

  handleIpc('storage:clear-chromium-cache', async () => {
    await clearChromiumCache()
    return { ok: true }
  })
}
