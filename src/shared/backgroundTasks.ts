export type BackgroundTaskKind = 'indexing' | 'timer' | 'update'

export type BackgroundTask = {
  id: string
  kind: BackgroundTaskKind
  title: string
  detail: string
  progress?: number
  startedAt?: number
  remainingSeconds?: number
  extensionId?: string
  commandName?: string
}
