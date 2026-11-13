// Window-owned drains and write ordering survive Settings tab remounts, including
// a queued getLlmConfig that has not reached setLlmConfig in the bridge yet.
type Owner = { drain: () => Promise<void>; retry: () => Promise<void>; retired: boolean }
type Lifecycle = {
  owners: Set<Owner>
  queue: Promise<unknown>
  revision: number
  saved: Map<string, number>
}
const windows = new WeakMap<object, Lifecycle>()
function lifecycle(): Lifecycle {
  let state = windows.get(window.tezbar)
  if (!state) {
    state = { owners: new Set(), queue: Promise.resolve(), revision: 0, saved: new Map() }
    windows.set(window.tezbar, state)
  }
  return state
}
export function enqueueSettingsWrite(
  work: () => Promise<unknown>,
  providers: string[] = [],
  origin?: number
): { promise: Promise<unknown>; revision: number } {
  const state = lifecycle()
  // Retrying an old intent must not supersede a newer draft merely because
  // its disk write happens later. Queue order and intent order are separate.
  const revision = origin ?? ++state.revision
  const promise = state.queue
    .catch(() => {})
    .then(work)
    .then((result) => {
      for (const provider of providers)
        state.saved.set(provider, Math.max(state.saved.get(provider) ?? 0, revision))
      return result
    })
  state.queue = promise
  return { promise, revision }
}
export function hasNewerSettingsSave(provider: string, revision: number): boolean {
  return (lifecycle().saved.get(provider) ?? 0) > revision
}
export function registerSettingsOwner(drain: Owner['drain'], retry: Owner['retry']): () => void {
  const { owners } = lifecycle()
  const owner = { drain, retry, retired: false }
  owners.add(owner)
  return () => {
    owner.retired = true
    void owner.drain().then(
      () => owners.delete(owner),
      () => {}
    )
  }
}
export async function drainSettingsOwners(): Promise<void> {
  const { owners } = lifecycle()
  const failed: Owner[] = []
  for (const owner of owners) {
    try {
      await owner.drain()
      if (owner.retired) owners.delete(owner)
    } catch {
      failed.push(owner)
    }
  }
  // Let newer owners flush even when an older draft failed. A successful later
  // edit of that provider supersedes the older failure, without replaying it.
  for (const owner of failed) {
    await owner.drain()
    if (owner.retired) owners.delete(owner)
  }
}
export async function retryRetiredSettingsOwners(): Promise<void> {
  for (const owner of lifecycle().owners) if (owner.retired) await owner.retry()
}
