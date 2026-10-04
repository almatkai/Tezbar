import { useCallback, useEffect, useState } from 'react'
import type { AppUpdateStatus } from '../shared/updater'
import {
  readAutoUpdatePreference,
  RELEASES_PAGE_URL,
  writeAutoUpdatePreference,
} from '../shared/updater'
import { Button } from './ui/primitives'

const CURRENT_VERSION: string = import.meta.env.VITE_APP_VERSION ?? '0.2.0-beta.4'

type Status = AppUpdateStatus

function renderNotes(notes: string): string[] {
  return notes
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(0, 12)
}

export function AppUpdateSettings(): JSX.Element {
  const [status, setStatus] = useState<Status>({ kind: 'idle' })
  const [autoUpdate, setAutoUpdate] = useState<boolean>(() =>
    readAutoUpdatePreference(window.localStorage)
  )

  useEffect(() => {
    let mounted = true
    void window.tezbar
      .getUpdateStatus()
      .then((next) => {
        if (mounted && next) setStatus(next)
      })
      .catch(() => undefined)
    const off = window.tezbar.onUpdateStatus((next) => {
      if (mounted && next) setStatus(next)
    })
    return () => {
      mounted = false
      off()
    }
  }, [])

  const check = useCallback(async () => {
    setStatus({ kind: 'checking' })
    try {
      setStatus(await window.tezbar.checkForUpdates())
    } catch (error) {
      setStatus({
        kind: 'error',
        message: error instanceof Error ? error.message : String(error),
      })
    }
  }, [])

  const install = useCallback(async () => {
    try {
      setStatus(await window.tezbar.downloadAndInstallUpdate())
    } catch (error) {
      setStatus({
        kind: 'error',
        message: error instanceof Error ? error.message : String(error),
      })
    }
  }, [])

  const restart = useCallback(() => {
    void window.tezbar.restartApp()
  }, [])

  const openRelease = useCallback((url: string) => {
    void window.tezbar.openReleasePage(url || RELEASES_PAGE_URL)
  }, [])

  const busy = status.kind === 'checking' || status.kind === 'downloading'

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2.5">
        <span className="rounded-tezbar-chip border border-white/10 bg-white/[0.05] px-2 py-1 text-[11px] font-semibold text-ink-2">
          v{CURRENT_VERSION}
        </span>

        {status.kind !== 'ready' && (
          <Button
            variant="primary"
            onClick={() => void check()}
            disabled={busy}
            className="min-w-[140px]"
          >
            {status.kind === 'checking'
              ? 'Checking…'
              : status.kind === 'downloading'
                ? 'Downloading…'
                : 'Check for Updates'}
          </Button>
        )}

        {status.kind === 'available' && (
          <>
            <Button variant="primary" onClick={() => void install()}>
              Download & Install
            </Button>
            <Button
              variant="ghost"
              onClick={() => openRelease(status.releaseUrl || RELEASES_PAGE_URL)}
            >
              What's New
            </Button>
          </>
        )}

        {status.kind === 'ready' && (
          <Button variant="primary" onClick={restart}>
            Relaunch to Update
          </Button>
        )}

        <Button
          variant="ghost"
          onClick={() => openRelease(RELEASES_PAGE_URL)}
          title="Download latest installer from GitHub Releases"
        >
          Download Installer
        </Button>

        {status.kind === 'error' && (
          <Button variant="ghost" onClick={() => void check()}>
            Retry
          </Button>
        )}
      </div>

      <div className="pt-2 border-t border-white/[0.06]">
        <label className="flex items-start gap-2.5 cursor-pointer select-none">
          <input
            type="checkbox"
            checked={autoUpdate}
            onChange={(e) => {
              const next = e.target.checked
              setAutoUpdate(next)
              writeAutoUpdatePreference(window.localStorage, next)
            }}
            className="mt-0.5 h-4 w-4 rounded border-white/20 bg-white/10 text-accent focus:ring-accent/40"
          />
          <div>
            <span className="block text-[12px] font-medium text-ink-2">
              Automatically download updates
            </span>
            <span className="block text-[11px] leading-snug text-ink-4">
              When enabled, updates will download automatically in the background. When disabled, updates will appear as the first item in your search bar so you can choose when to download.
            </span>
          </div>
        </label>
      </div>

      {status.kind === 'upToDate' && (
        <p className="text-[12px] text-emerald-300">
          You're on the latest version ({status.version}).
        </p>
      )}

      {status.kind === 'available' && (
        <div className="rounded-tezbar-row border border-white/10 bg-white/[0.03] p-3">
          <div className="mb-1.5 flex items-center justify-between">
            <span className="text-[12px] font-semibold text-ink-1">
              v{status.version} available
            </span>
            <span className="text-[10px] uppercase tracking-[0.08em] text-emerald-300">
              Update Available
            </span>
          </div>
          {renderNotes(status.notes).length > 0 ? (
            <ul className="list-disc space-y-0.5 pl-4 text-[11.5px] leading-snug text-ink-3">
              {renderNotes(status.notes).map((line, i) => (
                <li key={i}>{line.replace(/^[-*•]\s*/, '')}</li>
              ))}
            </ul>
          ) : (
            <p className="text-[11.5px] text-ink-4">No release notes provided.</p>
          )}
        </div>
      )}

      {status.kind === 'downloading' && (
        <div>
          <div className="h-1.5 w-full overflow-hidden rounded-full bg-white/[0.06]">
            <div
              className="h-full rounded-full bg-accent transition-[width]"
              style={{
                width:
                  status.total && status.total > 0
                    ? `${Math.round((status.downloaded / status.total) * 100)}%`
                    : '40%',
              }}
            />
          </div>
          <p className="mt-1 text-[11px] text-ink-4">
            {status.total
              ? `${(status.downloaded / 1024 / 1024).toFixed(1)} / ${(status.total / 1024 / 1024).toFixed(1)} MB`
              : `${(status.downloaded / 1024 / 1024).toFixed(1)} MB downloaded`}
          </p>
        </div>
      )}

      {status.kind === 'ready' && (
        <p className="text-[12px] text-emerald-300">
          v{status.version} downloaded — relaunch to finish installing.
        </p>
      )}

      {status.kind === 'error' && (
        <div className="rounded-tezbar-row border border-rose-500/25 bg-rose-500/10 p-3 space-y-2">
          <p className="text-[12px] font-medium text-rose-300">
            Update check failed: {status.message}
          </p>
          <div className="flex gap-2">
            <Button variant="ghost" onClick={() => void check()}>
              Retry
            </Button>
            <Button variant="ghost" onClick={() => openRelease(RELEASES_PAGE_URL)}>
              Open GitHub Releases
            </Button>
          </div>
        </div>
      )}
    </div>
  )
}
