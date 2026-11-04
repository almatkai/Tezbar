export type AiConnection = {
  id: string
  name: string
  transport: 'stdio' | 'http'
  command?: string
  args?: string[]
  cwd?: string
  headers?: Record<string, string | { secret: string; label?: string }>
  pendingPins?: Record<string, string>
  trustPin?: string
  needsAuth?: boolean
  url?: string
  env?: Record<string, string | { secret: string; label?: string }>
  auth?: {
    type: 'none' | 'token' | 'oauth'
    secret?: string
    header?: string
    clientId?: string
    scopes?: string
  }
  toolPolicy?: 'ask' | 'allowReadOnly' | 'allow'
  allowAllConfirmation?: string
  timeoutSeconds?: number
  tested?: boolean
  needsReview?: boolean
  disabledTools?: Record<string, boolean>
  error?: string
  allowances?: Record<string, string>
  modelNames?: Record<string, string>
  previousCatalog?: {
    tools?: {
      name: string
      description?: string
      title?: string
      annotations?: Record<string, unknown>
      outputSchema?: Record<string, unknown>
      inputSchema?: Record<string, unknown> & { properties?: Record<string, unknown> }
    }[]
  }
  catalog?: {
    tools?: {
      name: string
      description?: string
      title?: string
      annotations?: Record<string, unknown>
      outputSchema?: Record<string, unknown>
      inputSchema?: Record<string, unknown> & { properties?: Record<string, unknown> }
    }[]
    resources?: { name: string }[]
    prompts?: { name: string }[]
    warnings?: string[]
  }
}
export type AiExtension = {
  manifestVersion: 1
  id: string
  name: string
  enabled?: boolean
  error?: string
  connections: AiConnection[]
  instructions?: string
  importWarnings?: string[]
  commands?: { name: string; prompt: string }[]
  agents?: { id: string; name: string; instructions: string; tools?: string[] }[]
  providers?: {
    id: string
    title: string
    baseURL: string
    protocol?: string
    headers?: Record<string, string | { secret: string; label?: string }>
  }[]
  prompts?: {
    name: string
    title?: string
    template: string
    arguments?: { name: string; required?: boolean }[]
  }[]
}
export type AiExtensionsState = {
  canResetSecrets?: boolean
  defaultWorkingFolder?: string
  project?: {
    folder: string
    path: string
    entryHash: string
    error?: string
    preview: AiExtension
  }
  extensions: AiExtension[]
  globalInstructions: string
  useAgentsMd: boolean
}
export type ComposerItem = {
  extensionId: string
  connectionId?: string
  extensionName: string
  kind: string
  name: string
  id?: string
  title?: string
  uri?: string
  arguments?: { name: string; required?: boolean }[]
}
