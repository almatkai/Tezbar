export type SearchCategory =
  | 'applications'
  | 'files'
  | 'clipboard'
  | 'quick-notes'
  | 'extensions'
  | 'store'
  | 'mac-cli'
  | 'native-command'
  | 'commands'
  | 'snippets'
  | 'quick-links'
  | 'calculator'
  | 'color-converter'

export type ExtensionCommandArgument = {
  name: string
  required?: boolean
  type?: string
  placeholder?: string
  title?: string
  data?: Array<{ title?: string; value?: string }>
}

export type IconAssetKind = 'application' | 'extension' | 'file'

export type SearchAction =
  | { type: 'open-app'; appName: string }
  | { type: 'open-file'; path: string }
  | { type: 'open-with-app'; path: string; appName?: string }
  | { type: 'copy-text'; text: string }
  | { type: 'copy-and-paste-text'; text: string }
  | { type: 'add-note'; text: string }
  | { type: 'open-url'; url: string }
  | { type: 'install-extension'; extensionId: string }
  | {
      type: 'run-extension-command'
      extensionId: string
      commandName: string
      title: string
      iconPath?: string
      argumentName?: string
      argumentValue?: string
      commandArgumentDefinitions?: ExtensionCommandArgument[]
      argumentValues?: Record<string, string>
    }
  | {
      type: 'invoke-command'
      commandId: string
      payload?: Record<string, string | number | boolean>
    }
  | { type: 'run-shell'; command: string }
  | { type: 'run-native-command'; commandId: string }

export type SearchResult = {
  id: string
  title: string
  subtitle: string
  category: SearchCategory
  score: number
  action: SearchAction
  iconDataUrl?: string
}

export type PathCompletionItem = {
  id: string
  title: string
  subtitle: string
  kind: 'directory' | 'file' | 'application'
  section?: 'recommended' | 'default' | 'applications'
  badge?: string
  value: string
  path?: string
  appPath?: string
  appName?: string
  iconDataUrl?: string
  applicationAction?: 'open' | 'open-with'
  score: number
}

export type SearchExecuteResult = {
  ok: boolean
  message: string
}

export type SearchExecuteContext = {
  query?: string
  rank?: number
  resultId?: string
}

export type OpenPortProcess = {
  process: string
  user: string
  pid: string
  ports: number[]
}

export type SearchBenchmarkReport = {
  generatedAt: number
  benchmarkSize: number
  precisionAt5: number
  precisionAt10: number
  clickThroughRank: number
}
