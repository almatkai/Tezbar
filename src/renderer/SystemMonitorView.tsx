import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import type { SystemStatsSnapshot } from '../shared/systemStats'
import { Button, ViewHeader, cx } from './ui/primitives'

const REFRESH_INTERVAL_MS = 2_500

function formatDecimalBytes(bytes: number): string {
  if (!Number.isFinite(bytes)) return '—'
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(2)} GB`
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`
  return `${Math.round(bytes / 1_000).toLocaleString()} KB`
}

function formatExactBytes(bytes: number): string {
  return `${Math.round(bytes).toLocaleString()} bytes`
}

function formatRate(bytes: number | undefined): string {
  if (bytes === undefined) return 'Measuring…'
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB/s`
  if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(0)} KB/s`
  return `${Math.round(bytes)} B/s`
}

function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86_400)
  const hours = Math.floor((seconds % 86_400) / 3_600)
  const minutes = Math.floor((seconds % 3_600) / 60)
  return [days ? `${days}d` : '', hours ? `${hours}h` : '', `${minutes}m`].filter(Boolean).join(' ')
}

function toneForPercent(value: number): string {
  if (value >= 85) return 'bg-rose-400'
  if (value >= 65) return 'bg-amber-300'
  return 'bg-emerald-300'
}

function Meter({ value, label }: { value: number; label: string }): JSX.Element {
  const normalized = Math.max(0, Math.min(100, value))
  return (
    <div className="mt-3" aria-label={`${label}: ${Math.round(normalized)}%`}>
      <div className="h-1.5 overflow-hidden rounded-full bg-black/25">
        <div
          className={cx(
            'h-full rounded-full transition-[width] duration-500',
            toneForPercent(normalized)
          )}
          style={{ width: `${normalized}%` }}
        />
      </div>
    </div>
  )
}

function StatCard({
  eyebrow,
  value,
  detail,
  accent = 'sky',
  children,
}: {
  eyebrow: string
  value: string
  detail: string
  accent?: 'sky' | 'emerald' | 'amber' | 'rose'
  children?: ReactNode
}): JSX.Element {
  const accents = {
    sky: 'text-sky-200 border-sky-300/20 bg-sky-300/[0.055]',
    emerald: 'text-emerald-200 border-emerald-300/20 bg-emerald-300/[0.055]',
    amber: 'text-amber-100 border-amber-300/20 bg-amber-300/[0.055]',
    rose: 'text-rose-200 border-rose-300/20 bg-rose-300/[0.055]',
  }

  return (
    <section
      className={cx(
        'min-w-0 rounded-[14px] border p-3.5 shadow-[inset_0_1px_0_rgba(255,255,255,0.035)]',
        accents[accent]
      )}
    >
      <p className="text-[9px] font-semibold uppercase tracking-[0.18em] text-ink-4">{eyebrow}</p>
      <p className="mt-1.5 truncate font-display text-[22px] font-semibold leading-none tracking-[-0.035em] text-ink-1">
        {value}
      </p>
      <p className="mt-1.5 min-h-[16px] truncate text-[11px] text-ink-3" title={detail}>
        {detail}
      </p>
      {children}
    </section>
  )
}

function DetailRow({ label, value }: { label: string; value: ReactNode }): JSX.Element {
  return (
    <div className="flex min-w-0 items-center justify-between gap-3 border-b border-white/[0.045] py-2 last:border-0">
      <span className="shrink-0 text-[10.5px] text-ink-4">{label}</span>
      <span
        className="min-w-0 truncate text-right text-[11px] font-medium text-ink-2"
        title={typeof value === 'string' ? value : undefined}
      >
        {value}
      </span>
    </div>
  )
}

function LoadingDashboard(): JSX.Element {
  return (
    <div className="grid grid-cols-3 gap-2.5" aria-label="Loading system information">
      {Array.from({ length: 9 }, (_, index) => (
        <div
          key={index}
          className="h-[108px] animate-pulse rounded-[14px] border border-white/[0.06] bg-white/[0.025]"
        />
      ))}
    </div>
  )
}

export default function SystemMonitorView({ onBack }: { onBack: () => void }): JSX.Element {
  const rootRef = useRef<HTMLDivElement>(null)
  const mountedRef = useRef(true)
  const loadingRef = useRef(false)
  const [snapshot, setSnapshot] = useState<SystemStatsSnapshot | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [autoRefresh, setAutoRefresh] = useState(true)

  const refresh = useCallback(async (): Promise<void> => {
    if (loadingRef.current) return
    loadingRef.current = true
    try {
      const next = await window.tezbar.getSystemStats()
      if (!mountedRef.current) return
      setSnapshot(next)
      setError(null)
    } catch (caught) {
      if (!mountedRef.current) return
      setError(caught instanceof Error ? caught.message : 'Could not read system information.')
    } finally {
      loadingRef.current = false
      if (mountedRef.current) setLoading(false)
    }
  }, [])

  useEffect(() => {
    mountedRef.current = true
    rootRef.current?.focus()
    void refresh()
    return () => {
      mountedRef.current = false
    }
  }, [refresh])

  useEffect(() => {
    if (!autoRefresh) return
    const interval = window.setInterval(() => void refresh(), REFRESH_INTERVAL_MS)
    return () => window.clearInterval(interval)
  }, [autoRefresh, refresh])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      event.stopPropagation()
      onBack()
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [onBack])

  const memoryPercent = snapshot
    ? (snapshot.memory.usedBytes / Math.max(1, snapshot.memory.totalBytes)) * 100
    : 0
  const storagePercent = snapshot
    ? (snapshot.storage.usedBytes / Math.max(1, snapshot.storage.totalBytes)) * 100
    : 0
  const batteryPercent = snapshot?.battery?.chargePercent ?? 0
  const updatedAt = snapshot
    ? new Date(snapshot.collectedAt).toLocaleTimeString([], {
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
      })
    : null

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      role="application"
      aria-label="System Monitor"
      className="flex h-full min-h-0 w-full flex-col gap-2 outline-none animate-tezbar-scale-in"
    >
      <div className="glass-card shrink-0 px-4 py-3">
        <ViewHeader
          title="System Monitor"
          onBack={onBack}
          trailing={
            <div className="flex items-center gap-1.5">
              <button
                type="button"
                role="switch"
                aria-checked={autoRefresh}
                onClick={() => setAutoRefresh((current) => !current)}
                className={cx(
                  'rounded-tezbar-chip border px-2 py-1 text-[10px] transition',
                  autoRefresh
                    ? 'border-emerald-300/20 bg-emerald-300/10 text-emerald-200'
                    : 'border-white/[0.07] bg-white/[0.025] text-ink-4'
                )}
              >
                <span
                  className={cx(
                    'mr-1 inline-block h-1.5 w-1.5 rounded-full',
                    autoRefresh ? 'animate-pulse bg-emerald-300' : 'bg-ink-4'
                  )}
                />
                Live
              </button>
              <Button variant="ghost" onClick={() => void refresh()} disabled={loadingRef.current}>
                Refresh
              </Button>
            </div>
          }
        />

        <div className="mt-2 flex items-end justify-between gap-4 border-t border-white/[0.055] pt-2.5">
          <div className="min-w-0">
            <p className="truncate font-display text-[15px] font-semibold tracking-[-0.015em] text-ink-1">
              {snapshot?.device.name ?? 'Reading this Mac…'}
            </p>
            <p className="mt-0.5 truncate text-[10.5px] text-ink-4">
              {snapshot
                ? `${snapshot.device.chip} · ${snapshot.device.model} · macOS ${snapshot.device.osVersion}`
                : 'Hardware and live activity'}
            </p>
          </div>
          <p className="shrink-0 text-[9.5px] tabular-nums text-ink-4">
            {updatedAt ? `Updated ${updatedAt}` : 'Collecting…'}
          </p>
        </div>
      </div>

      <div className="glass-card min-h-0 flex-1 overflow-y-auto p-3.5">
        {error && !snapshot ? (
          <div className="flex min-h-[260px] flex-col items-center justify-center text-center">
            <p className="font-display text-[14px] font-semibold text-rose-200">
              System information unavailable
            </p>
            <p className="mt-1 max-w-[420px] text-[11px] leading-relaxed text-ink-4">{error}</p>
            <Button className="mt-3" onClick={() => void refresh()}>
              Try again
            </Button>
          </div>
        ) : loading && !snapshot ? (
          <LoadingDashboard />
        ) : snapshot ? (
          <div className="space-y-2.5">
            <div className="grid grid-cols-3 gap-2.5">
              <StatCard
                eyebrow="CPU Load"
                value={`${Math.round(snapshot.cpu.usagePercent)}%`}
                detail={`${snapshot.cpu.totalCores} cores · ${snapshot.cpu.userPercent.toFixed(1)}% user · ${snapshot.cpu.systemPercent.toFixed(1)}% system`}
                accent="sky"
              >
                <Meter value={snapshot.cpu.usagePercent} label="CPU load" />
              </StatCard>
              <StatCard
                eyebrow="Memory"
                value={`${(snapshot.memory.usedBytes / 1024 ** 3).toFixed(1)} GB`}
                detail={`of ${(snapshot.memory.totalBytes / 1024 ** 3).toFixed(0)} GB used · ${formatDecimalBytes(snapshot.memory.freeBytes)} available`}
                accent="emerald"
              >
                <Meter value={memoryPercent} label="Memory used" />
              </StatCard>
              <StatCard
                eyebrow="Battery"
                value={snapshot.battery ? `${snapshot.battery.chargePercent}%` : 'Not found'}
                detail={
                  snapshot.battery
                    ? `${snapshot.battery.isCharging ? 'Charging' : snapshot.battery.isPluggedIn ? 'On power' : 'On battery'} · ${snapshot.battery.condition ?? 'Condition unavailable'}`
                    : 'No internal battery detected'
                }
                accent={batteryPercent <= 20 ? 'rose' : 'amber'}
              >
                <Meter value={batteryPercent} label="Battery charge" />
              </StatCard>
            </div>

            <div className="grid grid-cols-[1.35fr_1fr] gap-2.5">
              <section className="rounded-[14px] border border-white/[0.07] bg-black/[0.11] p-3.5">
                <div className="flex items-start justify-between gap-4">
                  <div>
                    <p className="text-[9px] font-semibold uppercase tracking-[0.18em] text-ink-4">
                      Storage
                    </p>
                    <p className="mt-1.5 font-display text-[17px] font-semibold tracking-[-0.025em] text-ink-1">
                      {formatDecimalBytes(snapshot.storage.freeBytes)} free
                    </p>
                  </div>
                  <span className="rounded-tezbar-chip border border-white/[0.07] bg-white/[0.03] px-2 py-1 text-[9.5px] text-ink-3">
                    {snapshot.storage.smartStatus ?? 'Internal SSD'}
                  </span>
                </div>
                <Meter value={storagePercent} label="Storage used" />
                <div className="mt-2.5 grid grid-cols-2 gap-x-5">
                  <DetailRow
                    label="Free"
                    value={`${formatDecimalBytes(snapshot.storage.freeBytes)} (${formatExactBytes(snapshot.storage.freeBytes)})`}
                  />
                  <DetailRow
                    label="Capacity"
                    value={`${formatDecimalBytes(snapshot.storage.totalBytes)} (${formatExactBytes(snapshot.storage.totalBytes)})`}
                  />
                  <DetailRow label="Used" value={formatDecimalBytes(snapshot.storage.usedBytes)} />
                  <DetailRow
                    label="Drive"
                    value={snapshot.storage.device ?? snapshot.storage.name}
                  />
                </div>
              </section>

              <section className="rounded-[14px] border border-white/[0.07] bg-black/[0.11] p-3.5">
                <p className="text-[9px] font-semibold uppercase tracking-[0.18em] text-ink-4">
                  Battery Health
                </p>
                {snapshot.battery ? (
                  <div className="mt-1.5">
                    <DetailRow
                      label="Maximum capacity"
                      value={
                        snapshot.battery.maximumCapacityPercent === undefined
                          ? '—'
                          : `${snapshot.battery.maximumCapacityPercent}%`
                      }
                    />
                    <DetailRow label="Cycle count" value={snapshot.battery.cycleCount ?? '—'} />
                    <DetailRow label="Condition" value={snapshot.battery.condition ?? '—'} />
                    <DetailRow
                      label="Temperature"
                      value={
                        snapshot.battery.temperatureCelsius === undefined
                          ? '—'
                          : `${snapshot.battery.temperatureCelsius.toFixed(1)} °C`
                      }
                    />
                    <DetailRow
                      label="Remaining"
                      value={
                        snapshot.battery.timeRemainingMinutes
                          ? `${Math.floor(snapshot.battery.timeRemainingMinutes / 60)}h ${snapshot.battery.timeRemainingMinutes % 60}m`
                          : 'Calculating…'
                      }
                    />
                    <DetailRow
                      label="Low Power Mode"
                      value={snapshot.battery.lowPowerMode ? 'On' : 'Off'}
                    />
                  </div>
                ) : (
                  <p className="mt-3 text-[11px] text-ink-4">No battery detected.</p>
                )}
              </section>
            </div>

            <div className="grid grid-cols-3 gap-2.5">
              <section className="rounded-[14px] border border-white/[0.07] bg-black/[0.11] p-3.5">
                <p className="text-[9px] font-semibold uppercase tracking-[0.18em] text-ink-4">
                  Processor & GPU
                </p>
                <div className="mt-1.5">
                  <DetailRow label="CPU" value={snapshot.cpu.model} />
                  <DetailRow
                    label="Core layout"
                    value={
                      snapshot.cpu.performanceCores && snapshot.cpu.efficiencyCores
                        ? `${snapshot.cpu.performanceCores} performance + ${snapshot.cpu.efficiencyCores} efficiency`
                        : `${snapshot.cpu.totalCores} cores`
                    }
                  />
                  <DetailRow
                    label="Load average"
                    value={snapshot.cpu.loadAverage.map((value) => value.toFixed(2)).join(' · ')}
                  />
                  <DetailRow
                    label="GPU"
                    value={`${snapshot.gpu.model}${snapshot.gpu.cores ? ` · ${snapshot.gpu.cores} cores` : ''}`}
                  />
                  <DetailRow label="Display" value={snapshot.gpu.display ?? '—'} />
                </div>
              </section>

              <section className="rounded-[14px] border border-white/[0.07] bg-black/[0.11] p-3.5">
                <p className="text-[9px] font-semibold uppercase tracking-[0.18em] text-ink-4">
                  Network Activity
                </p>
                <div className="mt-2 grid grid-cols-2 gap-2">
                  <div className="rounded-[9px] border border-emerald-300/10 bg-emerald-300/[0.045] p-2">
                    <p className="text-[9px] text-ink-4">Download</p>
                    <p className="mt-1 text-[14px] font-semibold tabular-nums text-emerald-200">
                      {formatRate(snapshot.network.downloadBytesPerSecond)}
                    </p>
                  </div>
                  <div className="rounded-[9px] border border-sky-300/10 bg-sky-300/[0.045] p-2">
                    <p className="text-[9px] text-ink-4">Upload</p>
                    <p className="mt-1 text-[14px] font-semibold tabular-nums text-sky-200">
                      {formatRate(snapshot.network.uploadBytesPerSecond)}
                    </p>
                  </div>
                </div>
                <div className="mt-1.5">
                  <DetailRow
                    label="Connection"
                    value={
                      snapshot.network.connectionType ?? snapshot.network.interface ?? 'Offline'
                    }
                  />
                  <DetailRow label="Local IP" value={snapshot.network.localIp ?? '—'} />
                  <DetailRow
                    label="Wi-Fi"
                    value={snapshot.network.ssid ?? 'Name hidden by macOS'}
                  />
                </div>
              </section>

              <section className="rounded-[14px] border border-white/[0.07] bg-black/[0.11] p-3.5">
                <p className="text-[9px] font-semibold uppercase tracking-[0.18em] text-ink-4">
                  Thermals & Activity
                </p>
                <div className="mt-1.5">
                  <DetailRow
                    label="Fans"
                    value={
                      snapshot.fans.rpms.length
                        ? snapshot.fans.rpms.map((rpm) => `${rpm.toLocaleString()} RPM`).join(' · ')
                        : snapshot.fans.status
                    }
                  />
                  <DetailRow
                    label="Processes"
                    value={`${snapshot.processes.total} total · ${snapshot.processes.running} running`}
                  />
                  <DetailRow label="Uptime" value={formatUptime(snapshot.device.uptimeSeconds)} />
                  <DetailRow
                    label="Compressed"
                    value={formatDecimalBytes(snapshot.memory.compressedBytes ?? 0)}
                  />
                  <DetailRow
                    label="Swap used"
                    value={formatDecimalBytes(snapshot.memory.swapUsedBytes ?? 0)}
                  />
                </div>
                {snapshot.fans.note ? (
                  <p className="mt-2 text-[9.5px] leading-relaxed text-ink-4">
                    {snapshot.fans.note}
                  </p>
                ) : null}
              </section>
            </div>

            {error ? (
              <p className="px-1 text-[10px] text-amber-200">Refresh warning: {error}</p>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  )
}
