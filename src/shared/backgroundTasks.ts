export type BackgroundTaskKind = 'indexing' | 'timer'

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
