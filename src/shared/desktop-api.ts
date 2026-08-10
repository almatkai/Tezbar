import type {
  AgentApprovalResponse,
  AgentInputImage,
  AgentRunEvent,
  AgentRunRequest,
} from './agent'
import type { ChatSession, ChatSessionSummary, ChatTurn } from './chat'
import type { BackgroundTask } from './backgroundTasks'
import type { Intent } from './intent'
import type { ClipboardEntry, ClipboardImagePayload } from './clipboard'
import type { ExtensionIntegrityReport, ExtensionManifest, InstalledExtension } from './extensions'
import type {
  ExtensionDisposeSessionRequest,
  ExtensionInvokeActionResult,
  ExtensionLoadMoreSessionRequest,
  ExtensionRefreshSessionRequest,
  ExtensionRefreshSessionResult,
  ExtensionRunCommandResult,
  ExtensionSearchTextChangedResult,
  InstalledRegistryExtension,
} from './extensionRuntime'
import type { LlmConfigRecord, ProviderId } from './llmConfig'
import type {
  KnowledgeDepth,
  KnowledgeRootDepth,
  KnowledgeSnapshot,
  KnowledgeSourcesPage,
  KnowledgeStatus,
} from './knowledge'
import type { NativeCommandDescriptor } from './nativeCommands'
import type { AppUpdateStatus } from './updater'
import type { SystemStatsSnapshot } from './systemStats'
import type { PermissionId, PermissionStatus, PermissionsSnapshot } from './permissions'
import type { SafetyDescriptor, SafetyLogEntry } from './safety'
import type { NamedPortEntry } from './portManager'
import type { QuickNoteEntry } from './quickNotes'
import type { SnippetListRow, SnippetWritePayload } from './snippets'
import type {
  OpenPortProcess,
  PathCompletionItem,
  SearchAction,
  SearchBenchmarkReport,
  SearchExecuteContext,
  SearchExecuteResult,
  SearchResult,
} from './search'
import type { VoiceModel, VoiceModelId } from './voice'
import type {
  TerminalAttachRequest,
  TerminalAttachResult,
  TerminalCreateRequest,
  TerminalCreateResult,
  TerminalDataEvent,
  TerminalExitEvent,
  TerminalSessionsAction,
  TerminalSessionSummary,
  TerminalUpdateRequest,
  TerminalPromptInfo,
} from './terminal'

export type ProviderConnectionStatuses = Record<ProviderId, boolean>

export type FrankfurterLatestResponse = {
  base: string
  date: string
  rates: Record<string, number>
}

/** Renderer-to-host API implemented by the Tauri bridge. */
export type GithubPollResult =
  | { status: 'authorization_pending' }
  | { status: 'slow_down' }
  | {
      status: 'success'
      access_token: string
      refresh_token?: string
      expires_in?: number
      client_id: string
    }
  | { status: 'error'; error: string }

export type HotkeyUpdateResult = {
  ok: boolean
  accelerator: string
  error?: string
}

export type RaymesApi = {
  hide: () => Promise<void>
  show: () => Promise<void>
  openSettingsWindow: () => Promise<void>
  hideLauncherForSettings: () => Promise<void>
  closeCurrentWindow: () => Promise<void>
  query: (text: string) => Promise<Intent>
  cancel: () => Promise<unknown>
  getExtensions: () => Promise<InstalledExtension[]>
  listInstalledExtensions: () => Promise<InstalledExtension[]>
  searchStoreExtensions: (query: string) => Promise<ExtensionManifest[]>
  installExtension: (extensionId: string) => Promise<InstalledExtension>
  uninstallExtension: (extensionId: string) => Promise<boolean>
  inspectExtension: (extensionId: string) => Promise<ExtensionIntegrityReport>
  reinstallExtension: (extensionId: string) => Promise<ExtensionIntegrityReport>
  getExtensionInstallError: (extensionId: string) => Promise<string | null>
  extensionList: () => Promise<InstalledRegistryExtension[]>
  extensionSearchStore: (query: string) => Promise<ExtensionManifest[]>
  extensionInstall: (extensionId: string) => Promise<InstalledRegistryExtension>
  extensionUninstall: (extensionId: string) => Promise<boolean>
  extensionRunCommand: (payload: {
    extensionId: string
    commandName: string
    argumentValues?: Record<string, string>
  }) => Promise<ExtensionRunCommandResult>
  extensionInvokeAction: (payload: {
    sessionId: string
    actionId: string
    formValues?: Record<string, unknown>
  }) => Promise<ExtensionInvokeActionResult>
  extensionSearchTextChanged: (payload: {
    sessionId: string
    searchText: string
  }) => Promise<ExtensionSearchTextChangedResult>
  extensionRefreshSession: (
    payload: ExtensionRefreshSessionRequest
  ) => Promise<ExtensionRefreshSessionResult>
  extensionDisposeSession: (payload: ExtensionDisposeSessionRequest) => Promise<boolean>
  extensionLoadMore: (
    payload: ExtensionLoadMoreSessionRequest
  ) => Promise<ExtensionRefreshSessionResult>
  clipboardReadText: () => Promise<string>
  clipboardWriteText: (text: string) => Promise<{ ok: boolean }>
  shellOpen: (target: string) => Promise<{ ok: boolean }>
  getAppIconDataUrl: (appPath: string) => Promise<string | null>
  getAssetIconDataUrl: (
    kind: import('../shared/search').IconAssetKind,
    path: string
  ) => Promise<string | null>
  getExtensionPreferences: (payload: {
    extensionId: string
    commandName?: string
  }) => Promise<Record<string, unknown>>
  getExtensionPreferenceSetup: (payload: { extensionId: string; commandName?: string }) => Promise<{
    extensionId: string
    commandName?: string
    title: string
    preferences: Array<{
      name?: string
      title?: string
      description?: string
      type?: string
      required?: boolean
      default?: unknown
      data?: Array<{ title?: string; value?: string }>
      commandName?: string
      commandTitle?: string
    }>
    values: Record<string, unknown>
    hasSavedPreferences: boolean
  } | null>
  saveExtensionPreferences: (payload: {
    extensionId: string
    commandName?: string
    values: Record<string, unknown>
  }) => Promise<Record<string, unknown>>
  searchAll: (query: string) => Promise<SearchResult[]>
  listSearchCandidates: () => Promise<SearchResult[]>
  completePath: (query: string) => Promise<PathCompletionItem[]>
  resolveDirectory: (path: string) => Promise<string | null>
  recordDirectoryVisit: (path: string) => Promise<void>
  runSearchBenchmark: () => Promise<SearchBenchmarkReport>
  getSearchBenchmarkHistory: () => Promise<SearchBenchmarkReport[]>
  listOpenPorts: () => Promise<OpenPortProcess[]>
  listNamedPorts: () => Promise<NamedPortEntry[]>
  addNamedPort: (payload: { name: string; port: number }) => Promise<NamedPortEntry | null>
  removeNamedPort: (id: string) => Promise<boolean>
  quickLookFiles: (paths: string[]) => Promise<SearchExecuteResult>
  executeSearchAction: (
    action: SearchAction,
    context?: SearchExecuteContext
  ) => Promise<SearchExecuteResult>
  recordSearchActionUsage: (action: SearchAction, context?: SearchExecuteContext) => Promise<void>
  runAiAction: (payload: {
    instruction: string
    selectedText?: string
    appContext?: string
    allowAutomation?: boolean
    redactSensitive?: boolean
  }) => Promise<{ ok: boolean; output: string }>
  voiceSpeak: (text: string) => Promise<{ ok: boolean }>
  voiceStop: () => Promise<{ ok: boolean }>
  voiceTranscribe: (payload: {
    audioBytes: ArrayBuffer
    mimeType?: string
    language?: string
  }) => Promise<
    { ok: true; text: string; engine: string } | { ok: false; error: string; hint?: string }
  >
  setSuppressBlurHide: (value: boolean) => Promise<{ ok: boolean }>
  listVoiceSttModes: () => Promise<string[]>
  listVoiceModels: () => Promise<VoiceModel[]>
  downloadVoiceModel: (modelId: VoiceModelId) => Promise<VoiceModel>
  deleteVoiceModel: (modelId: VoiceModelId) => Promise<{ modelId: VoiceModelId }>
  getSelectedVoiceModel: () => Promise<{ modelId: VoiceModelId }>
  setSelectedVoiceModel: (modelId: VoiceModelId) => Promise<{ modelId: VoiceModelId }>
  onStreamToken: (listener: (token: string) => void) => () => void
  onStreamDone: (listener: () => void) => () => void
  onStreamError: (listener: (message: string) => void) => () => void
  getLlmConfig: () => Promise<LlmConfigRecord>
  setLlmConfig: (patch: LlmConfigRecord) => Promise<void | HotkeyUpdateResult>
  getLlmProviderStatuses: () => Promise<ProviderConnectionStatuses>
  listLlmModels: (providerId: ProviderId) => Promise<string[]>
  getWindowZoomFactor: () => number
  setWindowContentHeight: (height: number, zoomFactor: number) => Promise<void>
  openExternalUrl: (url: string) => Promise<void>
  githubDeviceStart: (clientId?: string) => Promise<{
    device_code: string
    user_code: string
    verification_uri: string
    expires_in: number
    interval: number
  }>
  githubDevicePoll: () => Promise<GithubPollResult>
  githubDeviceCancel: () => Promise<void>
  onWindowShown: (listener: (payload: { resetUi: boolean }) => void) => () => void
  startWindowSnapDrag: () => Promise<void>
  endWindowSnapDrag: () => Promise<void>
  getWindowSnapGuides: () => Promise<{
    visible: boolean
    snapX: boolean
    snapY: boolean
    centered: boolean
    targetRect: { left: number; top: number; right: number; bottom: number } | null
  }>
  onWindowSnapGuides: (
    listener: (payload: {
      visible: boolean
      snapX: boolean
      snapY: boolean
      centered: boolean
      targetRect: { left: number; top: number; right: number; bottom: number } | null
    }) => void
  ) => () => void
  getPermissions: () => Promise<PermissionsSnapshot>
  requestPermission: (id: PermissionId) => Promise<PermissionStatus>
  getSafetyDescriptors: () => Promise<SafetyDescriptor[]>
  getSafetyLog: () => Promise<SafetyLogEntry[]>
  clearSafetyLog: () => Promise<void>
  getSafetyDryRun: () => Promise<boolean>
  setSafetyDryRun: (value: boolean) => Promise<boolean>
  getNativeCommands: () => Promise<NativeCommandDescriptor[]>
  getSystemStats: () => Promise<SystemStatsSnapshot>
  listClipboardEntries: () => Promise<ClipboardEntry[]>
  restoreClipboardEntry: (id: string) => Promise<boolean>
  deleteClipboardEntry: (id: string) => Promise<boolean>
  toggleClipboardPin: (id: string) => Promise<boolean>
  revealClipboardEntry: (id: string) => Promise<boolean>
  readClipboardImage: (id: string) => Promise<ClipboardImagePayload | null>
  clearClipboardHistory: () => Promise<void>
  listSnippets: () => Promise<SnippetListRow[]>
  copySnippet: (id: string) => Promise<{ ok: boolean; message: string }>
  addSnippet: (
    payload: SnippetWritePayload
  ) => Promise<{ ok: boolean; message: string; id?: string }>
  updateSnippet: (
    id: string,
    payload: SnippetWritePayload
  ) => Promise<{ ok: boolean; message: string }>
  deleteSnippet: (id: string) => Promise<{ ok: boolean; message: string }>
  /** ECB rates via Frankfurter (main process; avoids renderer CORS). */
  fetchFrankfurterLatest: (from: string) => Promise<FrankfurterLatestResponse>
  listQuickNotes: () => Promise<QuickNoteEntry[]>
  appendQuickNote: (text: string) => Promise<QuickNoteEntry | null>
  updateQuickNote: (createdAt: number, text: string) => Promise<boolean>
  deleteQuickNote: (createdAt: number) => Promise<boolean>
  terminalCreate: (request: TerminalCreateRequest) => Promise<TerminalCreateResult>
  terminalAttach: (request: TerminalAttachRequest) => Promise<TerminalAttachResult | null>
  terminalDetach: (sessionId: string) => Promise<boolean>
  terminalList: () => Promise<TerminalSessionSummary[]>
  terminalUpdate: (request: TerminalUpdateRequest) => Promise<TerminalSessionSummary | null>
  terminalWrite: (sessionId: string, data: string) => Promise<boolean>
  terminalResize: (sessionId: string, cols: number, rows: number) => Promise<boolean>
  terminalGetCwd: (sessionId: string) => Promise<string | null>
  terminalKill: (sessionId: string) => Promise<boolean>
  terminalDelete: (sessionId: string) => Promise<boolean>
  getTerminalPromptInfo: () => Promise<TerminalPromptInfo>
  terminalSessionsShow: () => Promise<void>
  terminalSessionsHide: () => Promise<void>
  terminalSessionsSync: () => Promise<void>
  terminalSessionsAction: (action: TerminalSessionsAction) => Promise<void>
  onTerminalSessionsAction: (listener: (action: TerminalSessionsAction) => void) => () => void
  onTerminalData: (listener: (event: TerminalDataEvent) => void) => () => void
  onTerminalExit: (listener: (event: TerminalExitEvent) => void) => () => void
  getStorageBreakdown: () => Promise<{
    totalBytes: number
    items: Array<{ id: string; label: string; bytes: number; paths: string[] }>
  }>
  getClipboardStorageConfig: () => Promise<{
    watchEnabled: boolean
    captureImages: boolean
    maxImageMegapixels: number
  }>
  setClipboardStorageConfig: (patch: {
    watchEnabled?: boolean
    captureImages?: boolean
    maxImageMegapixels?: number
  }) => Promise<{
    watchEnabled: boolean
    captureImages: boolean
    maxImageMegapixels: number
  }>
  clearClipboardImages: () => Promise<{ removed: number; freedBytes: number }>

  vacuumSearchDatabase: () => Promise<{ beforeBytes: number; afterBytes: number }>
  clearChromiumCache: () => Promise<{ ok: boolean }>
  listBackgroundTasks: () => Promise<BackgroundTask[]>
  getKnowledgeSnapshot: () => Promise<KnowledgeSnapshot>
  listKnowledgeSources: (input?: {
    query?: string
    offset?: number
    limit?: number
  }) => Promise<KnowledgeSourcesPage>
  chooseKnowledgeFolder: () => Promise<string | null>
  addKnowledgeRoot: (path: string) => Promise<KnowledgeSnapshot>
  addMajorKnowledgeRoots: () => Promise<KnowledgeSnapshot>
  removeKnowledgeRoot: (rootId: string) => Promise<KnowledgeSnapshot>
  setKnowledgeRootEnabled: (rootId: string, enabled: boolean) => Promise<KnowledgeSnapshot>
  setKnowledgeDepth: (depth: KnowledgeDepth) => Promise<KnowledgeSnapshot>
  setKnowledgeRootDepth: (rootId: string, depth: KnowledgeRootDepth) => Promise<KnowledgeSnapshot>
  startKnowledgeIndexing: () => Promise<KnowledgeSnapshot>
  pauseKnowledgeIndexing: () => Promise<KnowledgeSnapshot>
  resumeKnowledgeIndexing: () => Promise<KnowledgeSnapshot>
  onKnowledgeStatus: (listener: (status: KnowledgeStatus) => void) => () => void
  /** Fired when the user presses ⌘N / Ctrl+N (global) — save command-bar text to notes. */
  onQuickNoteSaveShortcut: (listener: () => void) => () => void
  /** Fired from the top-bar tray menu to open a built-in Tezbar surface. */
  onAppSurfaceOpen: (
    listener: (surface: 'command' | 'settings' | 'clipboard' | 'extensions') => void
  ) => () => void
  /** Kick off a pi-backed agent run. Events stream via `onAgentEvent`. */
  agentRun: (
    request: string | AgentRunRequest
  ) => Promise<{ ok: boolean; runId?: string; error?: string }>
  /** Capture the display under the pointer while temporarily hiding the Raymes window. */
  captureActiveScreen: () => Promise<AgentInputImage>
  /** Abort the currently running agent task, if any. */
  agentCancel: () => Promise<{ ok: boolean }>
  /** Resolve an in-chat command approval prompt. */
  agentApprove: (response: AgentApprovalResponse) => Promise<{ ok: boolean; error?: string }>
  /** Subscribe to agent run events (stages, message deltas, answers, errors). */
  onAgentEvent: (listener: (event: AgentRunEvent) => void) => () => void
  /** Subscribe to extension install progress updates (0-100). */
  onExtensionInstallProgress: (
    listener: (payload: { id: string; progress: number }) => void
  ) => () => void
  /** Fired on every successful `setLlmConfig` write — views caching config should re-fetch. */
  onLlmConfigChanged: (listener: () => void) => () => void
  /** Chat session history (AI-mode multi-turn conversations). */
  chatRun: (turns: ChatTurn[]) => Promise<{ ok: boolean; runId?: string; error?: string }>
  chatList: (limit?: number) => Promise<ChatSessionSummary[]>
  chatGet: (id: string) => Promise<ChatSession | null>
  chatAppend: (payload: {
    session: Pick<
      ChatSession,
      'id' | 'title' | 'createdAt' | 'updatedAt' | 'workingDirectory'
    >
    turn: ChatTurn
  }) => Promise<{ ok: boolean; error?: string }>
  chatUpdateTitle: (id: string, title: string) => Promise<{ ok: boolean }>
  chatDeleteTurn: (sessionId: string, turnId: string) => Promise<{ ok: boolean; error?: string }>
  chatDelete: (id: string) => Promise<{ ok: boolean }>
  chatClear: () => Promise<{ ok: boolean }>
  /** Open extensions store surface in the main window. */
  openExtensionStore: () => Promise<boolean>
  /** Get installed extensions with their commands and preference schema. */
  getInstalledExtensionsSettingsSchema: () => Promise<
    Array<{
      extName: string
      title: string
      description: string
      owner: string
      iconDataUrl?: string
      preferences: Array<{
        scope: 'extension' | 'command'
        name: string
        title?: string
        label?: string
        description?: string
        placeholder?: string
        required?: boolean
        type?: string
        default?: unknown
        data?: Array<{ title?: string; value?: string }>
      }>
      commands: Array<{
        name: string
        title: string
        description: string
        mode: string
        interval?: string
        disabledByDefault?: boolean
        preferences: Array<{
          scope: 'extension' | 'command'
          name: string
          title?: string
          label?: string
          description?: string
          placeholder?: string
          required?: boolean
          type?: string
          default?: unknown
          data?: Array<{ title?: string; value?: string }>
        }>
      }>
    }>
  >
  /** Update a command's global hotkey (Electron accelerator string). */
  updateCommandHotkey: (
    commandId: string,
    hotkey: string
  ) => Promise<{ ok: boolean; error?: string }>
  /** Enable or disable a command. */
  toggleCommandEnabled: (commandId: string, enabled: boolean) => Promise<boolean>
  /** Get persisted settings (command hotkeys, aliases, disabled commands). */
  getSettings: () => Promise<{
    commandHotkeys: Record<string, string>
    commandAliases: Record<string, string>
    disabledCommands: Record<string, boolean>
  }>
  /** Save settings patch (aliases, etc.). */
  saveSettings: (patch: { commandAliases?: Record<string, string> }) => Promise<{ ok: boolean }>
  /** Fired when a command is triggered via its global hotkey (view mode only). */
  onRunExtensionCommandFromHotkey: (
    listener: (payload: { extensionId: string; commandName: string }) => void
  ) => () => void
  /** Quit the app (shows the native quit confirmation). */
  appQuit: () => Promise<void>

  // ---------------------------------------------------------------- updates

  /** Current app-update tracker state. */
  getUpdateStatus: () => Promise<AppUpdateStatus>
  /** Check GitHub for a newer stable release (beta/prerelease versions are skipped). */
  checkForUpdates: () => Promise<AppUpdateStatus>
  /** Download and install the update found by checkForUpdates. */
  downloadAndInstallUpdate: () => Promise<AppUpdateStatus>
  /** Restart into the freshly installed update. */
  restartApp: () => Promise<void>
  /** Open a releases page URL in the system browser. */
  openReleasePage: (url: string) => Promise<void>
  /** Subscribe to native update-tracker status changes. */
  onUpdateStatus: (listener: (status: AppUpdateStatus) => void) => () => void
}
