import { randomUUID } from 'node:crypto'

type Pending = {
  resolve: (value: unknown) => void
  reject: (error: Error) => void
  timer: ReturnType<typeof setTimeout>
}

const pending = new Map<string, Pending>()
let connected = true

export function send(message: unknown): void {
  if (connected) process.stdout.write(`${JSON.stringify(message)}\n`)
}

/** The extension process has no application service instances. Requests cross
 * this explicit host boundary; the Rust host validates the channel allowlist. */
export function request<T = unknown>(channel: string, ...payload: unknown[]): Promise<T> {
  if (!connected) return Promise.reject(new Error('Extension host disconnected'))
  const id = randomUUID()
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id)
      reject(new Error(`Extension host request timed out: ${channel}`))
    }, 120_000)
    pending.set(id, { resolve: (value) => resolve(value as T), reject, timer })
    send({ type: 'core-request', id, channel, payload })
  })
}

export function acceptReply(message: { id: string; result?: unknown; error?: unknown }): void {
  const operation = pending.get(message.id)
  if (!operation) return
  pending.delete(message.id)
  clearTimeout(operation.timer)
  if (message.error !== undefined) operation.reject(new Error(String(message.error)))
  else operation.resolve(message.result)
}

export function disconnect(): void {
  connected = false
  for (const operation of pending.values()) {
    clearTimeout(operation.timer)
    operation.reject(new Error('Extension host disconnected'))
  }
  pending.clear()
}

export function notify(channel: string, ...payload: unknown[]): void {
  void request(channel, ...payload).catch((error: unknown) => console.error(error))
}
