import { contextBridge, ipcRenderer, type IpcRendererEvent } from 'electron'
import type { ProviderId } from '../shared/llmConfig'
import { AGENT_IPC, type AgentRunEvent } from '../shared/agent'
import { CHAT_IPC, type ChatSession, type ChatTurn } from '../shared/chat'
import { IPC_CHANNELS } from '../shared/ipc'
import type { PermissionId } from '../shared/permissions'
import type { SearchAction, SearchExecuteContext } from '../shared/search'
import type { FrankfurterLatestResponse } from './api'
import type { VoiceModelId } from '../shared/voice'

contextBridge.exposeInMainWorld('raymes', {
  hide: () => ipcRenderer.invoke('window:hide'),
  show: () => ipcRenderer.invoke('window:show'),
  query: (text: string) => ipcRenderer.invoke(IPC_CHANNELS.QUERY, text),
  cancel: () => ipcRenderer.invoke('cancel'),
  getExtensions: () => ipcRenderer.invoke('get-extensions'),
  listInstalledExtensions: () => ipcRenderer.invoke('extensions:listInstalled'),
  searchStoreExtensions: (query: string) => ipcRenderer.invoke('extensions:searchStore', query),
  installExtension: (extensionId: string) => ipcRenderer.invoke('extensions:install', extensionId),
  uninstallExtension: (extensionId: string) => ipcRenderer.invoke('extensions:uninstall', extensionId),
  inspectExtension: (extensionId: string) =>
    ipcRenderer.invoke('extensions:integrity', extensionId),
  reinstallExtension: (extensionId: string) =>
    ipcRenderer.invoke('extensions:reinstall', extensionId),
  getExtensionInstallError: (extensionId: string) =>
    ipcRenderer.invoke('extensions:install-error', extensionId),
  extensionList: () => ipcRenderer.invoke('extension:list'),
  extensionSearchStore: (query: string) => ipcRenderer.invoke('extension:search-store', query),
  extensionInstall: (extensionId: string) => ipcRenderer.invoke('extension:install', extensionId),
  extensionUninstall: (extensionId: string) => ipcRenderer.invoke('extension:uninstall', extensionId),
  extensionRunCommand: (payload: {
    extensionId: string
    commandName: string
    argumentValues?: Record<string, string>
  }) => ipcRenderer.invoke('extension:run-command', payload),
  extensionInvokeAction: (payload: {
    sessionId: string
    actionId: string
    formValues?: Record<string, string>
  }) => ipcRenderer.invoke('extension:invoke-action', payload),
  extensionSearchTextChanged: (payload: {
    sessionId: string
    searchText: string
  }) => ipcRenderer.invoke('extension:search-text-changed', payload),
  clipboardReadText: () => ipcRenderer.invoke('clipboard:read'),
  clipboardWriteText: (text: string) => ipcRenderer.invoke('clipboard:write', text),
  shellOpen: (target: string) => ipcRenderer.invoke('shell:open', target),
  getExtensionPreferences: (payload: { extensionId: string; commandName?: string }) =>
    ipcRenderer.invoke('preferences:get', payload),
  searchAll: (query: string) => ipcRenderer.invoke(IPC_CHANNELS.SEARCH_ALL, query),
  runSearchBenchmark: () => ipcRenderer.invoke(IPC_CHANNELS.SEARCH_BENCHMARK_RUN),
  getSearchBenchmarkHistory: () => ipcRenderer.invoke(IPC_CHANNELS.SEARCH_BENCHMARK_HISTORY),
  listOpenPorts: () => ipcRenderer.invoke('open-ports:list'),
  listNamedPorts: () => ipcRenderer.invoke('port-manager:named:list'),
  addNamedPort: (payload: { name: string; port: number }) =>
    ipcRenderer.invoke('port-manager:named:add', payload),
  removeNamedPort: (id: string) => ipcRenderer.invoke('port-manager:named:remove', id),
  executeSearchAction: (action: SearchAction, context?: SearchExecuteContext) =>
    ipcRenderer.invoke(IPC_CHANNELS.SEARCH_EXECUTE, { action, context }),
  runAiAction: (payload: {
    instruction: string
    selectedText?: string
    appContext?: string
    allowAutomation?: boolean
    redactSensitive?: boolean
  }) => ipcRenderer.invoke(IPC_CHANNELS.AI_ACTION, payload),
  voiceSpeak: (text: string) => ipcRenderer.invoke(IPC_CHANNELS.VOICE_TTS_SPEAK, { text }),
  voiceStop: () => ipcRenderer.invoke(IPC_CHANNELS.VOICE_TTS_STOP),
  voiceTranscribe: (payload: { audioBytes: ArrayBuffer; mimeType?: string; language?: string }) =>
    ipcRenderer.invoke(IPC_CHANNELS.VOICE_STT_TRANSCRIBE, payload),
  setSuppressBlurHide: (value: boolean) => ipcRenderer.invoke('window:suppress-blur-hide', value),
  listVoiceSttModes: () => ipcRenderer.invoke(IPC_CHANNELS.VOICE_STT_MODES),
  listVoiceModels: () => ipcRenderer.invoke(IPC_CHANNELS.VOICE_MODELS_LIST),
  downloadVoiceModel: (modelId: VoiceModelId) =>
    ipcRenderer.invoke(IPC_CHANNELS.VOICE_MODEL_DOWNLOAD, { modelId }),
  getSelectedVoiceModel: () => ipcRenderer.invoke(IPC_CHANNELS.VOICE_MODEL_GET_SELECTED),
  setSelectedVoiceModel: (modelId: VoiceModelId) =>
    ipcRenderer.invoke(IPC_CHANNELS.VOICE_MODEL_SET_SELECTED, { modelId }),
  onStreamToken: (listener: (token: string) => void) => {
    const handler = (_event: IpcRendererEvent, token: string): void => {
      listener(token)
    }
    ipcRenderer.on('stream-token', handler)
    return (): void => {
      ipcRenderer.removeListener('stream-token', handler)
    }
  },
  onStreamDone: (listener: () => void) => {
    const handler = (): void => {
      listener()
    }
    ipcRenderer.on('stream-done', handler)
    return (): void => {
      ipcRenderer.removeListener('stream-done', handler)
    }
  },
  onStreamError: (listener: (message: string) => void) => {
    const handler = (_event: IpcRendererEvent, message: string): void => {
      listener(message)
    }
    ipcRenderer.on('stream-error', handler)
    return (): void => {
      ipcRenderer.removeListener('stream-error', handler)
    }
  },
  getLlmConfig: () => ipcRenderer.invoke('llm-config-get'),
  setLlmConfig: (patch: Record<string, unknown>) => ipcRenderer.invoke('llm-config-set', patch),
  getLlmProviderStatuses: () => ipcRenderer.invoke('llm-provider-statuses'),
  listLlmModels: (providerId: ProviderId) => ipcRenderer.invoke('llm-list-models', providerId),
  setWindowContentHeight: (height: number) => ipcRenderer.invoke('window-set-content-height', height),
  openExternalUrl: (url: string) => ipcRenderer.invoke('open-external-url', url),
  githubDeviceStart: (clientId: string) => ipcRenderer.invoke('github-device-start', clientId),
  githubDevicePoll: () => ipcRenderer.invoke('github-device-poll'),
  githubDeviceCancel: () => ipcRenderer.invoke('github-device-cancel'),
  onWindowShown: (listener: (payload: { resetUi: boolean }) => void) => {
    const handler = (_event: IpcRendererEvent, payload: { resetUi?: boolean }): void => {
      listener({ resetUi: payload?.resetUi === true })
    }
    ipcRenderer.on('window-shown', handler)
    return (): void => {
      ipcRenderer.removeListener('window-shown', handler)
    }
  },
  startWindowSnapDrag: () => ipcRenderer.invoke('window:snap-drag-start'),
  endWindowSnapDrag: () => ipcRenderer.invoke('window:snap-drag-end'),
  onWindowSnapGuides: (listener: (payload: { visible: boolean; active: boolean }) => void) => {
    const channel = 'window:snap-guides'
    const handler = (
      _event: IpcRendererEvent,
      payload: { visible?: boolean; active?: boolean },
    ): void => {
      listener({ visible: payload?.visible === true, active: payload?.active === true })
    }
    ipcRenderer.on(channel, handler)
    return (): void => {
      ipcRenderer.removeListener(channel, handler)
    }
  },
  onVoiceHotkeyHold: (listener: (payload: { phase: 'press' | 'release' }) => void) => {
    const channel = 'voice:hotkey-hold'
    const handler = (_event: IpcRendererEvent, payload: { phase?: string }): void => {
      if (payload?.phase === 'press' || payload?.phase === 'release') {
        listener({ phase: payload.phase })
      }
    }
    ipcRenderer.on(channel, handler)
    return (): void => {
      ipcRenderer.removeListener(channel, handler)
    }
  },
  getPermissions: () => ipcRenderer.invoke('permissions:snapshot'),
  requestPermission: (id: PermissionId) => ipcRenderer.invoke('permissions:request', id),
  getSafetyDescriptors: () => ipcRenderer.invoke('safety:descriptors'),
  getSafetyLog: () => ipcRenderer.invoke('safety:log'),
  clearSafetyLog: () => ipcRenderer.invoke('safety:log-clear'),
  getSafetyDryRun: () => ipcRenderer.invoke('safety:dry-run:get'),
  setSafetyDryRun: (value: boolean) => ipcRenderer.invoke('safety:dry-run:set', value),
  getNativeCommands: () => ipcRenderer.invoke('native-commands:list'),
  listClipboardEntries: () => ipcRenderer.invoke('clipboard:list'),
  restoreClipboardEntry: (id: string) => ipcRenderer.invoke('clipboard:restore', id),
  deleteClipboardEntry: (id: string) => ipcRenderer.invoke('clipboard:delete', id),
  toggleClipboardPin: (id: string) => ipcRenderer.invoke('clipboard:toggle-pin', id),
  revealClipboardEntry: (id: string) => ipcRenderer.invoke('clipboard:reveal', id),
  readClipboardImage: (id: string) => ipcRenderer.invoke('clipboard:image', id),
  clearClipboardHistory: () => ipcRenderer.invoke('clipboard:clear'),
  listSnippets: () => ipcRenderer.invoke('snippets:list'),
  copySnippet: (id: string) => ipcRenderer.invoke('snippets:copy', id),
  addSnippet: (payload: { label: string; trigger: string; body: string }) =>
    ipcRenderer.invoke('snippets:add', payload),
  updateSnippet: (id: string, payload: { label: string; trigger: string; body: string }) =>
    ipcRenderer.invoke('snippets:update', id, payload),
  deleteSnippet: (id: string) => ipcRenderer.invoke('snippets:delete', id),
  fetchFrankfurterLatest: (from: string): Promise<FrankfurterLatestResponse> =>
    ipcRenderer.invoke('currency:frankfurter-latest', from),
  listQuickNotes: () => ipcRenderer.invoke('notes:list'),
  appendQuickNote: (text: string) => ipcRenderer.invoke('notes:append', text),
  updateQuickNote: (createdAt: number, text: string) =>
    ipcRenderer.invoke('notes:update', { createdAt, text }),
  deleteQuickNote: (createdAt: number) => ipcRenderer.invoke('notes:delete', createdAt),
  onQuickNoteSaveShortcut: (listener: () => void) => {
    const channel = 'notes:quick-save-shortcut'
    const handler = (): void => {
      listener()
    }
    ipcRenderer.on(channel, handler)
    return (): void => {
      ipcRenderer.removeListener(channel, handler)
    }
  },
  agentRun: (task: string): Promise<{ ok: boolean; runId?: string; error?: string }> =>
    ipcRenderer.invoke(AGENT_IPC.RUN, task),
  agentCancel: (): Promise<{ ok: boolean }> => ipcRenderer.invoke(AGENT_IPC.CANCEL),
  onAgentEvent: (listener: (event: AgentRunEvent) => void) => {
    const handler = (_event: IpcRendererEvent, payload: AgentRunEvent): void => {
      if (payload && typeof payload === 'object' && typeof payload.type === 'string') {
        listener(payload)
      }
    }
    ipcRenderer.on(AGENT_IPC.EVENT, handler)
    return (): void => {
      ipcRenderer.removeListener(AGENT_IPC.EVENT, handler)
    }
  },
  onExtensionInstallProgress: (listener: (payload: { id: string; progress: number }) => void) => {
    const handler = (_event: IpcRendererEvent, payload: { id: string; progress: number }): void => {
      if (payload && typeof payload.id === 'string' && typeof payload.progress === 'number') {
        listener(payload)
      }
    }
    ipcRenderer.on('extension:install-progress', handler)
    return (): void => {
      ipcRenderer.removeListener('extension:install-progress', handler)
    }
  },
  chatRun: (turns: ChatTurn[]) => ipcRenderer.invoke(CHAT_IPC.RUN, turns),
  chatList: (limit?: number) => ipcRenderer.invoke(CHAT_IPC.LIST, limit),
  chatGet: (id: string) => ipcRenderer.invoke(CHAT_IPC.GET, id),
  chatAppend: (payload: {
    session: Pick<ChatSession, 'id' | 'title' | 'createdAt' | 'updatedAt'>
    turn: ChatTurn
  }) => ipcRenderer.invoke(CHAT_IPC.APPEND, payload),
  chatUpdateTitle: (id: string, title: string) =>
    ipcRenderer.invoke(CHAT_IPC.UPDATE_TITLE, { id, title }),
  chatDelete: (id: string) => ipcRenderer.invoke(CHAT_IPC.DELETE, id),
  chatClear: () => ipcRenderer.invoke(CHAT_IPC.CLEAR),
})
