import { useEffect, useState, type ReactNode } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { BackendGeneration } from './backendGeneration'

type BackendStatus = {
  phase: 'starting' | 'ready' | 'reconnecting' | 'failed'
  detail: string
  revision: number
  generation: number
}

const native = (): boolean => '__TAURI_INTERNALS__' in window

export default function BackendConnection({ children }: { children: ReactNode }): JSX.Element {
  const [status, setStatus] = useState<BackendStatus>({
    phase: native() ? 'starting' : 'ready',
    detail: 'Starting the background service…',
    revision: 0,
    generation: 0,
  })
  const [hasConnected, setHasConnected] = useState(!native())
  const [retrying, setRetrying] = useState(false)
  const [connectionError, setConnectionError] = useState('')

  useEffect(() => {
    if (!native()) return
    let disposed = false
    let unlisten: (() => void) | undefined
    let retryTimer: ReturnType<typeof setTimeout> | undefined
    let latestRevision = -1
    const accept = (next: BackendStatus): void => {
      if (disposed || next.revision < latestRevision) return
      latestRevision = next.revision
      setStatus(next)
      if (next.phase === 'ready') setHasConnected(true)
      setConnectionError('')
    }
    // A slow WebView event registration must not delay the initial status read.
    void invoke<BackendStatus>('get_backend_status')
      .then(accept)
      .catch((error) => {
        if (!disposed) setConnectionError(String(error))
      })
    const connect = async (): Promise<void> => {
      try {
        // Subscribe before reading the snapshot so no transition can be lost.
        const stop = await listen<BackendStatus>('backend-status', (event) => accept(event.payload))
        if (disposed) {
          stop()
          return
        }
        unlisten = stop
        accept(await invoke<BackendStatus>('get_backend_status'))
      } catch (error) {
        unlisten?.()
        unlisten = undefined
        if (disposed) return
        setConnectionError(String(error))
        retryTimer = setTimeout(() => {
          void connect()
        }, 2_000)
      }
    }
    void connect()
    return () => {
      disposed = true
      unlisten?.()
      clearTimeout(retryTimer)
    }
  }, [])

  useEffect(() => {
    if (hasConnected || !native()) return
    const onKey = (event: KeyboardEvent): void => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      void invoke('close_current_window').catch(() => undefined)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [hasConnected])

  const retry = async (): Promise<void> => {
    setRetrying(true)
    try {
      await invoke('restart_backend')
    } catch (error) {
      setConnectionError(String(error))
    } finally {
      setRetrying(false)
    }
  }

  const unavailable = status.phase !== 'ready' || Boolean(connectionError)
  const title = connectionError
    ? 'Cannot reach the background service'
    : status.phase === 'failed'
      ? 'Tezbar needs a moment'
      : hasConnected
        ? 'Reconnecting to Tezbar…'
        : 'Starting Tezbar…'

  return (
    <BackendGeneration.Provider value={status.generation}>
      {/* Keep mounted views and unsaved input intact during a backend restart. */}
      {hasConnected ? children : <div className="glass-shell h-screen w-full" />}
      {unavailable && (
        <div
          className={`backend-connection no-drag ${hasConnected ? 'backend-connection--banner' : ''}`}
        >
          <div className="min-w-0 flex-1" role="status" aria-live="polite">
            <p className="text-[13px] font-medium text-ink-1">{title}</p>
            <p className="mt-1 text-[12px] text-ink-2">
              {status.phase === 'failed' || connectionError
                ? 'We’ll keep trying automatically. You can also retry now.'
                : hasConnected
                  ? 'The background service is restarting. Your input will stay here.'
                  : 'Preparing search and commands. They’ll appear when ready.'}
            </p>
            {(status.phase === 'failed' || connectionError) && (
              <details className="mt-2 text-[11px] text-ink-3">
                <summary className="cursor-pointer">Details</summary>
                <p className="mt-1 max-h-24 overflow-auto break-words">
                  {connectionError || status.detail}
                </p>
              </details>
            )}
          </div>
          <button
            className="backend-connection__retry"
            onClick={() => {
              void retry()
            }}
            disabled={retrying}
          >
            {retrying ? 'Retrying…' : 'Retry now'}
          </button>
        </div>
      )}
    </BackendGeneration.Provider>
  )
}
