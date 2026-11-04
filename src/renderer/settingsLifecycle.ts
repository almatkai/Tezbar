// Window-owned drains survive Settings tab remounts, including a queued
// getLlmConfig that has not reached the bridge's setLlmConfig tracker yet.
type Owner = { drain: () => Promise<void>; retry: () => Promise<void>; retired: boolean }
const windows = new WeakMap<object, Set<Owner>>()
function windowOwners(): Set<Owner> {
  let owners = windows.get(window.tezbar)
  if (!owners) {
    owners = new Set()
    windows.set(window.tezbar, owners)
  }
  return owners
}
export function registerSettingsOwner(drain: Owner['drain'], retry: Owner['retry']): () => void {
  const owners = windowOwners()
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
  const owners = windowOwners()
  for (const owner of owners) {
    await owner.drain()
    if (owner.retired) owners.delete(owner)
  }
}
export async function retryRetiredSettingsOwners(): Promise<void> {
  for (const owner of windowOwners()) if (owner.retired) await owner.retry()
}
