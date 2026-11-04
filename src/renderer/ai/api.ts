export function extensionRequest<T>(action: string, ...args: unknown[]): Promise<T> {
  return window.tezbar.aiExtensions(action, ...args) as Promise<T>
}
