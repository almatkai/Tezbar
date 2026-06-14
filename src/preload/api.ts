import type { AgentRunEvent } from '../shared/agent'
import type { ChatSession, ChatSessionSummary, ChatTurn } from '../shared/chat'
import type { Intent } from '../shared/intent'
import type { ClipboardEntry, ClipboardImagePayload } from '../shared/clipboard'
import type {
  ExtensionIntegrityReport,
  ExtensionManifest,
  InstalledExtension,
} from '../shared/extensions'
import type {
  ExtensionDisposeSessionRequest,
  ExtensionInvokeActionResult,
  ExtensionLoadMoreSessionRequest,
  ExtensionRefreshSessionRequest,
  ExtensionRefreshSessionResult,
  ExtensionRunCommandResult,
  ExtensionSearchTextChangedResult,
  InstalledRegistryExtension,
} from '../shared/extensionRuntime'
import type { LlmConfigRecord, ProviderId } from '../shared/llmConfig'
import type { NativeCommandDescriptor } from '../shared/nativeCommands'
import type { PermissionId, PermissionStatus, PermissionsSnapshot } from '../shared/permissions'
import type { SafetyDescriptor, SafetyLogEntry } from '../shared/safety'
import type { NamedPortEntry } from '../shared/portManager'
import type { QuickNoteEntry } from '../shared/quickNotes'
import type { SnippetListRow, SnippetWritePayload } from '../shared/snippets'
import type {
  OpenPortProcess,
  PathCompletionItem,
  SearchAction,
  SearchBenchmarkReport,
  SearchExecuteContext,
  SearchExecuteResult,
  SearchResult,
} from '../shared/search'
import type { VoiceModel, VoiceModelId } from '../shared/voice'
import type {
  TerminalCreateRequest,
  TerminalCreateResult,
  TerminalDataEvent,
  TerminalExitEvent,
  TerminalPromptInfo,
} from '../shared/terminal'

export type ProviderConnectionStatuses = Record<ProviderId, boolean>

export type FrankfurterLatestResponse = {
  base: string
  date: string
  rates: Record<string, number>
}

export type GithubPollResult =
  | { status: 'authorization_pending' }
  | { status: 'slow_down' }
  | { status: 'success'; access_token: string; refresh_token?: string; expires_in?: number }
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
    formValues?: Record<string, string>
  }) => Promise<ExtensionInvokeActionResult>
  extensionSearchTextChanged: (payload: {
    sessionId: string
    searchText: string
  }) => Promise<ExtensionSearchTextChangedResult>
  extensionRefreshSession: (
    payload: ExtensionRefreshSessionRequest
  ) => Promise<ExtensionRefreshSessionResult>
  extensionDisposeSession: (
    payload: ExtensionDisposeSessionRequest
  ) => Promise<boolean>
  extensionLoadMore: (
    payload: ExtensionLoadMoreSessionRequest
  ) => Promise<ExtensionRefreshSessionResult>
  clipboardReadText: () => Promise<string>
  clipboardWriteText: (text: string) => Promise<{ ok: boolean }>
  shellOpen: (target: string) => Promise<{ ok: boolean }>
  getAppIconDataUrl: (appPath: string) => Promise<string | null>
  getExtensionPreferences: (payload: {
    extensionId: string
    commandName?: string
  }) => Promise<Record<string, unknown>>
  saveExtensionPreferences: (payload: {
    extensionId: string
    commandName?: string
    values: Record<string, unknown>
  }) => Promise<Record<string, unknown>>
  searchAll: (query: string) => Promise<SearchResult[]>
  completePath: (query: string) => Promise<PathCompletionItem[]>
  runSearchBenchmark: () => Promise<SearchBenchmarkReport>
  getSearchBenchmarkHistory: () => Promise<SearchBenchmarkReport[]>
  listOpenPorts: () => Promise<OpenPortProcess[]>
  listNamedPorts: () => Promise<NamedPortEntry[]>
  addNamedPort: (payload: { name: string; port: number }) => Promise<NamedPortEntry | null>
  removeNamedPort: (id: string) => Promise<boolean>
  executeSearchAction: (
    action: SearchAction,
    context?: SearchExecuteContext
  ) => Promise<SearchExecuteResult>
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
  githubDeviceStart: (clientId: string) => Promise<{
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
  onWindowSnapGuides: (
    listener: (payload: { visible: boolean; active: boolean }) => void
  ) => () => void
  /** Alt+Space held after opening the launcher — same pipeline as the Hold to speak control. */
  onVoiceHotkeyHold: (listener: (payload: { phase: 'press' | 'release' }) => void) => () => void
  getPermissions: () => Promise<PermissionsSnapshot>
  requestPermission: (id: PermissionId) => Promise<PermissionStatus>
  getSafetyDescriptors: () => Promise<SafetyDescriptor[]>
  getSafetyLog: () => Promise<SafetyLogEntry[]>
  clearSafetyLog: () => Promise<void>
  getSafetyDryRun: () => Promise<boolean>
  setSafetyDryRun: (value: boolean) => Promise<boolean>
  getNativeCommands: () => Promise<NativeCommandDescriptor[]>
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
  terminalWrite: (sessionId: string, data: string) => Promise<boolean>
  terminalResize: (sessionId: string, cols: number, rows: number) => Promise<boolean>
  terminalKill: (sessionId: string) => Promise<boolean>
  getTerminalPromptInfo: () => Promise<TerminalPromptInfo>
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
  /** Fired when the user presses ⌘N / Ctrl+N (global) — save command-bar text to notes. */
  onQuickNoteSaveShortcut: (listener: () => void) => () => void
  /** Fired from the top-bar tray menu to open a built-in TezBar surface. */
  onAppSurfaceOpen: (
    listener: (surface: 'command' | 'settings' | 'clipboard') => void
  ) => () => void
  /** Kick off a pi-backed agent run. Events stream via `onAgentEvent`. */
  agentRun: (task: string) => Promise<{ ok: boolean; runId?: string; error?: string }>
  /** Abort the currently running agent task, if any. */
  agentCancel: () => Promise<{ ok: boolean }>
  /** Subscribe to agent run events (stages, message deltas, answers, errors). */
  onAgentEvent: (listener: (event: AgentRunEvent) => void) => () => void
  /** Subscribe to extension install progress updates (0-100). */
  onExtensionInstallProgress: (
    listener: (payload: { id: string; progress: number }) => void
  ) => () => void
  /** Chat session history (AI-mode multi-turn conversations). */
  chatRun: (turns: ChatTurn[]) => Promise<{ ok: boolean; runId?: string; error?: string }>
  chatList: (limit?: number) => Promise<ChatSessionSummary[]>
  chatGet: (id: string) => Promise<ChatSession | null>
  chatAppend: (payload: {
    session: Pick<ChatSession, 'id' | 'title' | 'createdAt' | 'updatedAt'>
    turn: ChatTurn
  }) => Promise<{ ok: boolean; error?: string }>
  chatUpdateTitle: (id: string, title: string) => Promise<{ ok: boolean }>
  chatDelete: (id: string) => Promise<{ ok: boolean }>
  chatClear: () => Promise<{ ok: boolean }>
  appQuit: () => Promise<void>
}
