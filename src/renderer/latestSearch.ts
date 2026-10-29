/** One running request and one replaceable pending input, never an IPC backlog.
 * A short task window coalesces input events; it is not a typing debounce (the
 * pending deadline isn't extended by new input). Running IPC cannot be aborted
 * by the renderer; callers still own response/generation guards.
 */
export function latestSearchScheduler(coalesceMs = 8) {
  type Job = { run: () => Promise<unknown>; ready: boolean }
  let pending: Job | undefined
  let running = false
  let timer: ReturnType<typeof setTimeout> | undefined

  const drain = (): void => {
    if (running || !pending?.ready) return
    const job = pending
    pending = undefined
    running = true
    void Promise.resolve()
      .then(job.run)
      .catch(() => undefined) // The caller handles/report errors, including sync throws.
      .finally(() => {
        running = false
        drain()
      })
  }

  return {
    schedule(run: () => Promise<unknown>): () => void {
      const job: Job = { run, ready: false }
      pending = job
      if (timer === undefined) {
        timer = setTimeout(() => {
          timer = undefined
          if (pending) pending.ready = true
          drain()
        }, coalesceMs)
      }
      return () => {
        if (pending !== job) return
        pending = undefined
        // Keep the bounded coalescing deadline across effect replacement. The
        // timer captures no query/result payload and is gone after one window.
      }
    },
  }
}
