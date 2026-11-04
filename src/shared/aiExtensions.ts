export type AiConnection = {
  id: string
  name: string
  transport: 'stdio' | 'http'
  command?: string
  args?: string[]
  cwd?: string
  url?: string
  env?: Record<string, string | { secret: string; label?: string }>
  auth?: { type: 'none' | 'token'; secret?: string }
  toolPolicy?: 'ask' | 'allowReadOnly' | 'allow'
  allowAllConfirmation?: string
  timeoutSeconds?: number
  tested?: boolean
  needsReview?: boolean
  catalog?: {
    tools?: { name: string; description?: string }[]
    prompts?: { name: string }[]
    warnings?: string[]
  }
}
export type AiExtension = {
  manifestVersion: 1
  id: string
  name: string
  enabled?: boolean
  connections: AiConnection[]
  instructions?: string
  prompts?: {
    name: string
    title?: string
    template: string
    arguments?: { name: string; required?: boolean }[]
  }[]
}
export type AiExtensionsState = {
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
