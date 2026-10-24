import { useCallback, useEffect, useRef, useState, type SetStateAction } from 'react'
import type { KnowledgeSnapshot } from '../shared/knowledge'

export const KNOWLEDGE_SETTINGS_TIMEOUT_MS = 10_000
export const KNOWLEDGE_ACTION_TIMEOUT_MS = 15_000

// The native bridge cannot abort a snapshot. Cancel only our interest in it: every
// completion (including rejection) is consumed, but only the current request wins.
export function useKnowledgeSettings(active: boolean) {
  const [knowledge, updateKnowledge] = useState<KnowledgeSnapshot | null>(null)
  const [snapshotSeq, setSnapshotSeq] = useState(0)
  // Read completions advance synchronously, before React commits their render.
  // A timeout in that gap must put its barrier after the completed read.
  const snapshotSeqRef = useRef(0)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const mounted = useRef(false)
  const hasSnapshot = useRef(false)
  const failed = useRef(false)
  const request = useRef<{ cancel: () => void } | null>(null)

  const cancel = useCallback(() => {
    request.current?.cancel()
    request.current = null
  }, [])

  const refresh = useCallback(() => {
    if (!mounted.current) return
    cancel()
    setLoading(true)
    setError(null)
    failed.current = false
    let current = true
    const timer = window.setTimeout(() => {
      finish()
      failed.current = true
      setError('Loading Knowledge settings timed out. Try Refresh status or Retry.')
    }, KNOWLEDGE_SETTINGS_TIMEOUT_MS)
    const pending = {
      cancel: () => {
        current = false
        window.clearTimeout(timer)
      },
    }
    request.current = pending
    const finish = () => {
      pending.cancel()
      request.current = null
      setLoading(false)
    }
    // Also catches synchronous bridge errors and avoids dispatching an invocation
    // that was already superseded during effect cleanup (e.g. React StrictMode).
    void Promise.resolve()
      .then(() => (current ? window.tezbar.getKnowledgeSnapshot() : null))
      .then(
        (snapshot) => {
          if (!current || !snapshot) return
          finish()
          hasSnapshot.current = true
          updateKnowledge(snapshot)
          setSnapshotSeq(++snapshotSeqRef.current)
        },
        (failure: unknown) => {
          if (!current) return
          finish()
          failed.current = true
          const message = failure instanceof Error ? failure.message : String(failure)
          setError(message || 'Could not load Knowledge settings. Try Refresh status or Retry.')
        }
      )
  }, [cancel])

  // Mutation responses are newer than any already-pending snapshot read.
  const setKnowledge = useCallback(
    (next: SetStateAction<KnowledgeSnapshot | null>) => {
      if (!mounted.current) return
      cancel()
      setLoading(false)
      setError(null)
      failed.current = false
      if (typeof next !== 'function') hasSnapshot.current = next !== null
      updateKnowledge(next)
    },
    [cancel]
  )

  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
      cancel()
    }
  }, [cancel])

  useEffect(() => {
    if (!active) return
    refresh()
    const timer = window.setInterval(() => {
      // Poll only a healthy, previously loaded snapshot, serially. A failure or
      // timeout requires an explicit retry instead of a silent retry loop.
      if (!document.hidden && hasSnapshot.current && !failed.current && !request.current) refresh()
    }, 1500)
    const onVisibility = () => {
      // A failed read still requires explicit Retry/Refresh, even after hide/show.
      if (!document.hidden && !failed.current) refresh()
    }
    document.addEventListener('visibilitychange', onVisibility)
    return () => {
      document.removeEventListener('visibilitychange', onVisibility)
      window.clearInterval(timer)
      cancel()
    }
  }, [active, cancel, refresh])

  useEffect(
    () =>
      window.tezbar.onKnowledgeStatus((status) => {
        if (!mounted.current) return
        updateKnowledge((previous) => (previous ? { ...previous, status } : previous))
      }),
    []
  )

  return { knowledge, setKnowledge, loading, error, refresh, snapshotSeq, snapshotSeqRef }
}
