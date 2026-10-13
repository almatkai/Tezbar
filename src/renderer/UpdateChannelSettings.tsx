import { useEffect, useState } from 'react'
import {
  channelDescription,
  LAST_UPDATE_CHECK_KEY,
  type ReleaseChannel,
  type UpdateChannelInfo,
} from '../shared/updater'

export function UpdateChannelSettings({
  updateBusy,
  onChannelChanged,
}: {
  updateBusy: boolean
  onChannelChanged?: () => void
}): JSX.Element {
  const [info, setInfo] = useState<UpdateChannelInfo | null>(null)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let mounted = true
    void window.tezbar
      .getUpdateChannel()
      .then((value) => {
        if (mounted) setInfo(value)
      })
      .catch((failure: unknown) => {
        if (mounted) setError(failure instanceof Error ? failure.message : String(failure))
      })
    return () => {
      mounted = false
    }
  }, [])

  const select = (channel: ReleaseChannel) => {
    setSaving(true)
    setError(null)
    void window.tezbar
      .setUpdateChannel(channel)
      .then((next) => {
        setInfo(next)
        // A check in the old channel is not a fresh check in the new one.
        try {
          window.localStorage.removeItem(LAST_UPDATE_CHECK_KEY)
        } catch {
          /* Native preference was saved successfully. */
        }
        onChannelChanged?.()
      })
      .catch((failure: unknown) => {
        setError(failure instanceof Error ? failure.message : String(failure))
      })
      .finally(() => setSaving(false))
  }

  return (
    <div className="space-y-2 border-b border-white/10 pb-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <label htmlFor="update-channel" className="text-xs font-medium text-ink-2">
          Release channel
        </label>
        <select
          id="update-channel"
          className="glass-field px-3 py-2 text-xs"
          aria-describedby="update-channel-description"
          value={info?.channel ?? 'stable'}
          disabled={!info || saving || updateBusy}
          onChange={(event) => select(event.target.value as ReleaseChannel)}
        >
          <option value="stable">Stable — recommended</option>
          <option value="beta">Beta — early releases</option>
          <option value="alpha">Alpha — experimental</option>
        </select>
      </div>
      <p
        id="update-channel-description"
        role="status"
        className="text-xs leading-relaxed text-ink-3"
      >
        {saving
          ? 'Saving release channel…'
          : info
            ? channelDescription(info)
            : error
              ? 'Release-channel settings are unavailable.'
              : 'Loading release channel…'}
      </p>
      {error ? (
        <p role="alert" className="text-xs text-rose-300">
          {error}
        </p>
      ) : null}
    </div>
  )
}
