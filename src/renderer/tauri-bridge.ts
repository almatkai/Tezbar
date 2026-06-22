/* eslint-disable @typescript-eslint/no-explicit-any, @typescript-eslint/no-unsafe-function-type */
// src/renderer/tauri-bridge.ts
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { RaymesApi } from '../preload/api'

type TauriRaymesApi = RaymesApi & {
  moveMouse: (x: number, y: number) => Promise<unknown>
  click: (x: number, y: number, button: string) => Promise<unknown>
  doubleClick: (x: number, y: number) => Promise<unknown>
  pressKey: (key: string, mods: string[]) => Promise<unknown>
  typeText: (text: string) => Promise<unknown>
  scroll: (x: number, y: number, dx: number, dy: number) => Promise<unknown>
  screenshot: () => Promise<string>
  isPhysicalKeyDown: (key: string) => Promise<unknown>
}

export async function initTauriBridge(): Promise<void> {
  // Check if we are running inside Tauri
  if (!(window as any).__TAURI_INTERNALS__) {
    return
  }

  const errorMessage = (error: unknown): string => {
    if (error instanceof Error) return error.message
    if (typeof error === 'string' && error.trim()) return error
    try {
      return JSON.stringify(error)
    } catch {
      return String(error)
    }
  }

  const callBackend = (channel: string, ...args: unknown[]): Promise<any> => {
    return invoke('call_backend', { channel, payload: args }).catch((error: unknown) => {
      console.error(`[Tauri bridge] ${channel} failed:`, error)
      throw new Error(errorMessage(error) || `${channel} failed`)
    })
  }

  const listenersMap = new Map<string, Set<Function>>()
  const eventListeners = new Map<string, Promise<any>>()
  const eventBacklog = new Map<string, unknown[]>()
  const persistentChannels = new Set(['agent:event'])

  const ensureEventListener = (channel: string): Promise<any> => {
    const existing = eventListeners.get(channel)
    if (existing) return existing

    const listenPromise = listen(channel, (event: any) => {
      const listeners = listenersMap.get(channel)
      if (listeners?.size) {
        listeners.forEach((listener) => listener(event.payload))
        return
      }

      const queued = eventBacklog.get(channel) ?? []
      queued.push(event.payload)
      eventBacklog.set(channel, queued.slice(-100))
    })
    eventListeners.set(channel, listenPromise)
    return listenPromise
  }

  const setupEventListener = (channel: string, callback: Function) => {
    if (!listenersMap.has(channel)) {
      listenersMap.set(channel, new Set())
    }
    listenersMap.get(channel)!.add(callback)

    void ensureEventListener(channel)

    const queued = eventBacklog.get(channel)
    if (queued?.length) {
      eventBacklog.delete(channel)
      queueMicrotask(() => queued.forEach((payload) => callback(payload)))
    }

    return () => {
      const listeners = listenersMap.get(channel)
      if (listeners) {
        listeners.delete(callback)
        if (listeners.size === 0 && !persistentChannels.has(channel)) {
          eventListeners.get(channel)?.then((unlisten) => unlisten())
          eventListeners.delete(channel)
          listenersMap.delete(channel)
        }
      }
    }
  }

  // Agent runs can begin during the first React effect. Listen before rendering
  // and replay any early events when the chat surface subscribes.
  await ensureEventListener('agent:event')

  const tezbar: TauriRaymesApi = {
    hide: () => invoke('hide_window'),
    show: () => invoke('show_window'),
    openSettingsWindow: () => invoke('open_settings_window_cmd'),
    closeCurrentWindow: () => invoke('close_current_window'),
    query: (text: string) => callBackend('query', text),
    cancel: () => callBackend('cancel'),
    getExtensions: () => callBackend('get-extensions'),
    listInstalledExtensions: () => callBackend('extensions:listInstalled'),
    searchStoreExtensions: (query: string) => callBackend('extensions:searchStore', query),
    installExtension: (extensionId: string) => callBackend('extensions:install', extensionId),
    uninstallExtension: (extensionId: string) => callBackend('extensions:uninstall', extensionId),
    inspectExtension: (extensionId: string) => callBackend('extensions:integrity', extensionId),
    reinstallExtension: (extensionId: string) => callBackend('extensions:reinstall', extensionId),
    getExtensionInstallError: (extensionId: string) =>
      callBackend('extensions:install-error', extensionId),
    extensionList: () => callBackend('extension:list'),
    extensionSearchStore: (query: string) => callBackend('extension:search-store', query),
    extensionInstall: (extensionId: string) => callBackend('extension:install', extensionId),
    extensionUninstall: (extensionId: string) => callBackend('extension:uninstall', extensionId),
    extensionRunCommand: (payload: any) => callBackend('extension:run-command', payload),
    extensionInvokeAction: (payload: any) => callBackend('extension:invoke-action', payload),
    extensionSearchTextChanged: (payload: any) =>
      callBackend('extension:search-text-changed', payload),
    extensionRefreshSession: (payload: any) => callBackend('extension:refresh-session', payload),
    extensionDisposeSession: (payload: any) => callBackend('extension:dispose-session', payload),
    extensionLoadMore: (payload: any) => callBackend('extension:load-more', payload),

    clipboardReadText: () => callBackend('clipboard:read'),
    clipboardWriteText: (text: string) => callBackend('clipboard:write', text),
    shellOpen: (target: string) => callBackend('shell:open', target),
    getAppIconDataUrl: (appPath: string) => callBackend('app-icon:data-url', appPath),
    getAssetIconDataUrl: (kind: any, path: string) =>
      callBackend('asset-icon:data-url', { kind, path }),
    getExtensionPreferences: (payload: any) => callBackend('preferences:get', payload),
    getExtensionPreferenceSetup: (payload: any) => callBackend('preferences:setup', payload),
    saveExtensionPreferences: (payload: any) => callBackend('preferences:set', payload),
    searchAll: (query: string) => callBackend('search:all', query),
    completePath: (query: string) => callBackend('path:complete', query),
    recordDirectoryVisit: (path: string) => callBackend('directory-visit:record', path),
    runSearchBenchmark: () => callBackend('search:benchmark:run'),
    getSearchBenchmarkHistory: () => callBackend('search:benchmark:history'),
    listOpenPorts: () => callBackend('open-ports:list'),
    listNamedPorts: () => callBackend('port-manager:named:list'),
    addNamedPort: (payload: any) => callBackend('port-manager:named:add', payload),
    removeNamedPort: (id: string) => callBackend('port-manager:named:remove', id),
    executeSearchAction: (action: any, context?: any) =>
      callBackend('search:execute', { action, context }),
    runAiAction: (payload: any) => callBackend('ai:action', payload),

    voiceSpeak: (text: string) => callBackend('voice:tts:speak', { text }),
    voiceStop: () => callBackend('voice:tts:stop'),
    voiceTranscribe: (payload: any) => callBackend('voice:stt:transcribe', payload),

    setSuppressBlurHide: async (value: boolean) => {
      await invoke('set_suppress_blur_hide', { value })
      return callBackend('window:suppress-blur-hide', value)
    },
    listVoiceSttModes: () => callBackend('voice:stt:modes'),
    listVoiceModels: () => callBackend('voice:models:list'),
    downloadVoiceModel: (modelId: any) => callBackend('voice:model:download', { modelId }),
    getSelectedVoiceModel: () => callBackend('voice:model:get-selected'),
    setSelectedVoiceModel: (modelId: any) => callBackend('voice:model:set-selected', { modelId }),

    getLlmConfig: () => callBackend('llm-config-get'),
    setLlmConfig: async (patch: any) => {
      if (typeof patch?.raymesHotkey === 'string') {
        try {
          await invoke('update_raymes_shortcut', { shortcutStr: patch.raymesHotkey })
        } catch (error) {
          return {
            ok: false,
            accelerator: patch.raymesHotkey,
            error: error instanceof Error ? error.message : String(error),
          }
        }
      }
      return callBackend('llm-config-set', patch)
    },
    getLlmProviderStatuses: () => callBackend('llm-provider-statuses'),
    listLlmModels: (providerId: any) => callBackend('llm-list-models', providerId),

    getWindowZoomFactor: () => 1,
    setWindowContentHeight: (height: number, zoomFactor: number) =>
      invoke('window_set_content_height', { height, zoomFactor }),
    openExternalUrl: (url: string) => callBackend('open-external-url', url),

    githubDeviceStart: (clientId: string) => callBackend('github-device-start', clientId),
    githubDevicePoll: () => callBackend('github-device-poll'),
    githubDeviceCancel: () => callBackend('github-device-cancel'),

    getPermissions: () => callBackend('permissions:snapshot'),
    requestPermission: (id: any) => callBackend('permissions:request', id),
    getSafetyDescriptors: () => callBackend('safety:descriptors'),
    getSafetyLog: () => callBackend('safety:log'),
    clearSafetyLog: () => callBackend('safety:log-clear'),
    getSafetyDryRun: () => callBackend('safety:dry-run:get'),
    setSafetyDryRun: (value: boolean) => callBackend('safety:dry-run:set', value),

    getNativeCommands: () => callBackend('native-commands:list'),

    listClipboardEntries: () => callBackend('clipboard:list'),
    restoreClipboardEntry: (id: string) => callBackend('clipboard:restore', id),
    deleteClipboardEntry: (id: string) => callBackend('clipboard:delete', id),
    toggleClipboardPin: (id: string) => callBackend('clipboard:toggle-pin', id),
    revealClipboardEntry: (id: string) => callBackend('clipboard:reveal', id),
    readClipboardImage: (id: string) => callBackend('clipboard:image', id),
    clearClipboardHistory: () => callBackend('clipboard:clear'),

    listSnippets: () => callBackend('snippets:list'),
    copySnippet: (id: string) => callBackend('snippets:copy', id),
    addSnippet: (payload: any) => callBackend('snippets:add', payload),
    updateSnippet: (id: string, payload: any) => callBackend('snippets:update', id, payload),
    deleteSnippet: (id: string) => callBackend('snippets:delete', id),

    fetchFrankfurterLatest: (from: string) => callBackend('currency:frankfurter-latest', from),

    listQuickNotes: () => callBackend('notes:list'),
    appendQuickNote: (text: string) => callBackend('notes:append', text),
    updateQuickNote: (createdAt: number, text: string) =>
      callBackend('notes:update', { createdAt, text }),
    deleteQuickNote: (createdAt: number) => callBackend('notes:delete', createdAt),

    terminalCreate: async (request: any) => {
      // Tauri listener registration is asynchronous. Starting the shell before
      // these promises settle drops its prompt and fast initial-command output.
      await Promise.all([eventListeners.get('terminal:data'), eventListeners.get('terminal:exit')])
      return invoke('native_terminal_create', { request })
    },
    terminalWrite: (sessionId: string, data: string) =>
      invoke('native_terminal_write', { sessionId, data }),
    terminalResize: (sessionId: string, cols: number, rows: number) =>
      invoke('native_terminal_resize', { sessionId, cols, rows }),
    terminalKill: (sessionId: string) => invoke('native_terminal_kill', { sessionId }),
    getTerminalPromptInfo: () => callBackend('terminal:get-prompt-info'),

    getStorageBreakdown: () => callBackend('storage:breakdown'),
    getClipboardStorageConfig: () => callBackend('storage:clipboard-config:get'),
    setClipboardStorageConfig: (patch: any) => callBackend('storage:clipboard-config:set', patch),
    clearClipboardImages: () => callBackend('storage:clear-clipboard-images'),
    vacuumSearchDatabase: () => callBackend('storage:vacuum-search-db'),
    clearChromiumCache: () => callBackend('storage:clear-chromium-cache'),

    agentRun: (request) => callBackend('agent:run', request),
    captureActiveScreen: async () => {
      const bytes = (await invoke('screenshot')) as number[]
      const chunkSize = 0x8000
      let binary = ''
      for (let offset = 0; offset < bytes.length; offset += chunkSize) {
        binary += String.fromCharCode(...bytes.slice(offset, offset + chunkSize))
      }
      return {
        type: 'image' as const,
        data: btoa(binary),
        mimeType: 'image/png' as const,
      }
    },
    agentCancel: () => callBackend('agent:cancel'),
    agentApprove: (response) => callBackend('agent:approve', response),

    chatRun: (turns: any[]) => callBackend('chat:run', turns),
    chatList: (limit?: number) => callBackend('chat:list', limit),
    chatGet: (id: string) => callBackend('chat:get', id),
    chatAppend: (payload: any) => callBackend('chat:append', payload),
    chatUpdateTitle: (id: string, title: string) => callBackend('chat:update-title', { id, title }),
    chatDelete: (id: string) => callBackend('chat:delete', id),
    chatClear: () => callBackend('chat:clear'),

    appQuit: async () => {
      const confirmed = await callBackend('app:confirm-quit')
      if (confirmed) await invoke('quit_app')
    },

    onStreamToken: (listener: (token: string) => void) =>
      setupEventListener('stream-token', listener),
    onStreamDone: (listener: () => void) => setupEventListener('stream-done', listener),
    onStreamError: (listener: (msg: string) => void) =>
      setupEventListener('stream-error', listener),
    onWindowShown: (listener: (payload: { resetUi: boolean }) => void) =>
      setupEventListener('window-shown', listener),
    onWindowSnapGuides: (listener: (payload: { visible: boolean; active: boolean }) => void) =>
      setupEventListener('window:snap-guides', listener),
    onVoiceHotkeyHold: (listener: (payload: { phase: 'press' | 'release' }) => void) =>
      setupEventListener('voice:hotkey-hold', listener),
    onTerminalData: (listener: (event: any) => void) =>
      setupEventListener('terminal:data', listener),
    onTerminalExit: (listener: (event: any) => void) =>
      setupEventListener('terminal:exit', listener),
    onQuickNoteSaveShortcut: (listener: () => void) =>
      setupEventListener('notes:quick-save-shortcut', listener),
    onAppSurfaceOpen: (listener: (surface: 'command' | 'settings' | 'clipboard') => void) =>
      setupEventListener('app:open-surface', listener),
    onAgentEvent: (listener: (event: any) => void) => setupEventListener('agent:event', listener),
    onExtensionInstallProgress: (listener: (payload: any) => void) =>
      setupEventListener('extension:install-progress', listener),

    startWindowSnapDrag: () => invoke('start_window_snap_drag'),
    endWindowSnapDrag: () => invoke('end_window_snap_drag'),

    // Intercept native scripting commands directly in Rust core for maximum performance
    moveMouse: (x: number, y: number) => invoke('move_mouse', { x, y }),
    click: (x: number, y: number, button: string) => invoke('click', { x, y, button }),
    doubleClick: (x: number, y: number) => invoke('double_click', { x, y }),
    pressKey: (key: string, mods: string[]) => invoke('press_key', { key, mods }),
    typeText: (text: string) => invoke('type_text', { text }),
    scroll: (x: number, y: number, dx: number, dy: number) => invoke('scroll', { x, y, dx, dy }),
    screenshot: async () => {
      const bytes = (await invoke('screenshot')) as number[]
      // Convert to base64 data URL
      const uint8 = new Uint8Array(bytes)
      let binary = ''
      const len = uint8.byteLength
      for (let i = 0; i < len; i++) {
        binary += String.fromCharCode(uint8[i] ?? 0)
      }
      const base64 = window.btoa(binary)
      return `data:image/png;base64,${base64}`
    },
    isPhysicalKeyDown: (key: string) => invoke('is_physical_key_down', { key }),
  }

  window.tezbar = tezbar

  void tezbar
    .getLlmConfig()
    .then((config) => {
      if (typeof config.raymesHotkey === 'string' && config.raymesHotkey.trim()) {
        return invoke('update_raymes_shortcut', { shortcutStr: config.raymesHotkey })
      }
    })
    .catch((error: unknown) => {
      console.warn('Could not restore the configured global shortcut:', error)
    })
}
