export type ExtensionRegistryCommand = {
  name: string
  title: string
  subtitle?: string
  description?: string
  mode?: string
  argumentDefinitions?: Array<{
    name: string
    required?: boolean
    type?: string
    placeholder?: string
    title?: string
    data?: Array<{ title?: string; value?: string }>
  }>
}

export type InstalledRegistryExtension = {
  id: string
  slug: string
  name: string
  version: string
  description: string
  author?: string
  owner?: string
  authorIconUrl?: string
  downloadCount?: number
  iconPath?: string | undefined
  packageJsonPath: string
  extensionPath: string
  commands: ExtensionRegistryCommand[]
  installedAt: number
}

export type ExtensionRuntimeAction = {
  id: string
  title: string
  style?: 'default' | 'destructive'
  shortcut?: {
    modifiers?: string[]
    key?: string
  }
  kind?: 'action' | 'copy' | 'open' | 'push' | 'pop' | 'submit-form' | 'show-in-finder'
}

export type ExtensionRuntimeEffect = {
  kind: 'clipboard' | 'open' | 'show-in-finder' | 'toast' | 'hud' | 'apple-script'
  value?: string
  style?: string
  title?: string
  message?: string
}

export type ExtensionRuntimeNode = {
  type: string
  props?: Record<string, unknown>
  children?: ExtensionRuntimeNode[]
  metadata?: ExtensionRuntimeNode
}

export type ExtensionRunCommandRequest = {
  extensionId: string
  commandName: string
  argumentValues?: Record<string, string>
}

export type ExtensionRunCommandResult =
  | {
      ok: true
      mode: 'no-view'
      message: string
      effects?: ExtensionRuntimeEffect[]
    }
  | {
      ok: true
      mode: 'view'
      message?: string
      sessionId: string
      extensionId: string
      commandName: string
      title: string
      root: ExtensionRuntimeNode
      actions: ExtensionRuntimeAction[]
      effects?: ExtensionRuntimeEffect[]
    }
  | {
      ok: false
      message: string
    }

export type ExtensionInvokeActionRequest = {
  sessionId: string
  actionId: string
  formValues?: Record<string, unknown>
}

export type ExtensionInvokeActionResult =
  | {
      ok: true
      mode: 'no-view'
      message: string
      effects?: ExtensionRuntimeEffect[]
    }
  | {
      ok: true
      mode: 'view'
      message?: string
      sessionId: string
      extensionId: string
      commandName: string
      title: string
      root: ExtensionRuntimeNode
      actions: ExtensionRuntimeAction[]
      effects?: ExtensionRuntimeEffect[]
    }
  | {
      ok: false
      message: string
    }

export type ExtensionSearchTextChangedRequest = {
  sessionId: string
  searchText: string
}

export type ExtensionRefreshSessionRequest = {
  sessionId: string
}

export type ExtensionRefreshSessionResult =
  | ExtensionRunCommandResult
  | {
      ok: true
      mode: 'unchanged'
    }

export type ExtensionDisposeSessionRequest = {
  sessionId: string
}

export type ExtensionLoadMoreSessionRequest = {
  sessionId: string
}

export type ExtensionSearchTextChangedResult =
  | {
      ok: true
      mode: 'no-view'
      message: string
      effects?: ExtensionRuntimeEffect[]
    }
  | {
      ok: true
      mode: 'view'
      message?: string
      sessionId: string
      extensionId: string
      commandName: string
      title: string
      root: ExtensionRuntimeNode
      actions: ExtensionRuntimeAction[]
      effects?: ExtensionRuntimeEffect[]
    }
  | {
      ok: false
      message: string
    }

export type ExtensionRuntimePreferencesRequest = {
  extensionId: string
  commandName?: string
}
