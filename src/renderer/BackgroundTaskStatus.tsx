import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { BackgroundTask } from '../shared/backgroundTasks'
import type { ExtensionRunCommandResult } from '../shared/extensionRuntime'
import { cx } from './ui/primitives'

type ExtensionRuntimeViewPayload = Extract<ExtensionRunCommandResult, { ok: true; mode: 'view' }>

function formatDuration(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds))
  const hours = Math.floor(total / 3_600)
  const minutes = Math.floor((total % 3_600) / 60)
  const remainingSeconds = total % 60
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, '0')}:${String(remainingSeconds).padStart(2, '0')}`
  }
  return `${minutes}:${String(remainingSeconds).padStart(2, '0')}`
}

function TaskIcon({
  kind,
  compact = false,
}: {
  kind: BackgroundTask['kind']
  compact?: boolean
}): JSX.Element {
  const size = compact ? 13 : 15
  if (kind === 'indexing') {
    return (
      <svg
        width={size}
        height={size}
        viewBox="0 0 16 16"
        fill="none"
        className="animate-spin"
        aria-hidden
      >
        <circle
          cx="8"
          cy="8"
          r="5.5"
          stroke="currentColor"
          strokeOpacity="0.22"
          strokeWidth="1.5"
        />
        <path
          d="M8 2.5a5.5 5.5 0 0 1 5.5 5.5"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
        />
      </svg>
    )
  }

  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" aria-hidden>
      <circle cx="8" cy="8.5" r="5" stroke="currentColor" strokeWidth="1.3" />
      <path
        d="M6.25 2.25h3.5M8 5.6v3.15l2 1.15"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
    </svg>
  )
}

function taskValue(task: BackgroundTask): string {
  if (task.kind === 'indexing') return `${Math.round((task.progress ?? 0) * 100)}%`
  return formatDuration(task.remainingSeconds ?? 0)
}

export default function BackgroundTaskStatus({
  onOpenIndexing,
  onOpenExtensionRuntime,
}: {
  onOpenIndexing: () => void
  onOpenExtensionRuntime: (initial: ExtensionRuntimeViewPayload) => void
}): JSX.Element | null {
  const rootRef = useRef<HTMLDivElement>(null)
  const refreshInFlightRef = useRef(false)
  const mountedRef = useRef(true)
  const [tasks, setTasks] = useState<BackgroundTask[]>([])
  const [open, setOpen] = useState(false)

  const refresh = useCallback(async (): Promise<void> => {
    // Indexing can emit progress much faster than the backend round trip. Keep
    // this poll single-flight so status bursts cannot build an IPC backlog.
    if (refreshInFlightRef.current) return
    refreshInFlightRef.current = true
    try {
      const nextTasks = await window.tezbar.listBackgroundTasks()
      if (mountedRef.current) setTasks(nextTasks)
    } catch (error) {
      console.warn('[BackgroundTaskStatus] Failed to refresh background tasks:', error)
    } finally {
      refreshInFlightRef.current = false
    }
  }, [])

  useEffect(() => {
    mountedRef.current = true
    void refresh()
    const interval = window.setInterval(() => void refresh(), 1_000)
    const offKnowledge = window.tezbar.onKnowledgeStatus(() => void refresh())
    return () => {
      mountedRef.current = false
      window.clearInterval(interval)
      offKnowledge()
    }
  }, [refresh])

  useEffect(() => {
    if (!open) return
    const onPointerDown = (event: PointerEvent): void => {
      if (event.target instanceof Node && !rootRef.current?.contains(event.target)) setOpen(false)
    }
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      event.stopPropagation()
      setOpen(false)
    }
    window.addEventListener('pointerdown', onPointerDown, true)
    window.addEventListener('keydown', onKeyDown, true)
    return () => {
      window.removeEventListener('pointerdown', onPointerDown, true)
      window.removeEventListener('keydown', onKeyDown, true)
    }
  }, [open])

  useEffect(() => {
    if (tasks.length === 0) setOpen(false)
  }, [tasks.length])

  const primaryTask = useMemo(
    () => tasks.find((task) => task.kind === 'indexing') ?? tasks[0],
    [tasks]
  )

  if (!primaryTask) return null

  const openTask = async (task: BackgroundTask): Promise<void> => {
    setOpen(false)
    if (task.kind === 'indexing') {
      onOpenIndexing()
      return
    }
    if (!task.extensionId || !task.commandName) return
    const result = await window.tezbar.extensionRunCommand({
      extensionId: task.extensionId,
      commandName: task.commandName,
    })
    if (result.ok && result.mode === 'view') onOpenExtensionRuntime(result)
  }

  return (
    <div ref={rootRef} className="no-drag relative ml-auto shrink-0">
      {open ? (
        <div
          role="dialog"
          aria-label="Background tasks"
          className="absolute bottom-[calc(100%+10px)] right-0 z-50 w-[310px] overflow-hidden rounded-[15px] border border-white/[0.1] bg-[#11141c]/95 shadow-[0_20px_60px_rgba(0,0,0,0.48)] backdrop-blur-2xl animate-tezbar-scale-in"
        >
          <div className="flex items-center justify-between border-b border-white/[0.07] px-3.5 py-2.5">
            <div>
              <p className="font-display text-[11.5px] font-semibold text-ink-1">
                Background tasks
              </p>
              <p className="mt-0.5 text-[9.5px] text-ink-4">
                {tasks.length} {tasks.length === 1 ? 'task' : 'tasks'} running
              </p>
            </div>
            <span
              aria-hidden
              className="inline-flex h-6 w-6 items-center justify-center rounded-full bg-cyan-300/[0.08] text-cyan-200"
            >
              <svg width="13" height="13" viewBox="0 0 16 16" fill="none">
                <path
                  d="M3 4.25h10M3 8h10M3 11.75h10"
                  stroke="currentColor"
                  strokeWidth="1.25"
                  strokeLinecap="round"
                />
                <circle cx="5" cy="4.25" r="1" fill="#11141c" stroke="currentColor" />
                <circle cx="10.5" cy="8" r="1" fill="#11141c" stroke="currentColor" />
                <circle cx="7" cy="11.75" r="1" fill="#11141c" stroke="currentColor" />
              </svg>
            </span>
          </div>

          <div className="p-1.5">
            {tasks.map((task) => {
              const progress = Math.round((task.progress ?? 0) * 100)
              return (
                <button
                  key={task.id}
                  type="button"
                  onClick={() => void openTask(task)}
                  className="group flex w-full items-center gap-3 rounded-[11px] px-2.5 py-2 text-left transition-colors hover:bg-white/[0.055] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-cyan-300/50"
                >
                  <span
                    className={cx(
                      'grid h-8 w-8 shrink-0 place-items-center rounded-[9px] border',
                      task.kind === 'indexing'
                        ? 'border-cyan-300/20 bg-cyan-300/[0.08] text-cyan-200'
                        : 'border-amber-300/20 bg-amber-300/[0.08] text-amber-200'
                    )}
                  >
                    <TaskIcon kind={task.kind} />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="flex items-center justify-between gap-3">
                      <span className="truncate text-[11.5px] font-medium text-ink-1">
                        {task.title}
                      </span>
                      <span className="shrink-0 font-mono text-[10.5px] tabular-nums text-ink-2">
                        {taskValue(task)}
                      </span>
                    </span>
                    <span className="mt-0.5 block truncate text-[9.5px] text-ink-4">
                      {task.detail}
                    </span>
                    {task.kind === 'indexing' ? (
                      <span className="mt-1.5 block h-1 overflow-hidden rounded-full bg-white/[0.06]">
                        <span
                          className="block h-full rounded-full bg-[linear-gradient(90deg,#5ea7ff,#6de1c2)] transition-[width] duration-300"
                          style={{ width: `${progress}%` }}
                        />
                      </span>
                    ) : null}
                  </span>
                  <svg
                    className="shrink-0 text-ink-4 transition group-hover:translate-x-0.5 group-hover:text-ink-2"
                    width="12"
                    height="12"
                    viewBox="0 0 16 16"
                    fill="none"
                    aria-hidden
                  >
                    <path
                      d="m6 3.5 4.5 4.5L6 12.5"
                      stroke="currentColor"
                      strokeWidth="1.4"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    />
                  </svg>
                </button>
              )
            })}
          </div>
        </div>
      ) : null}

      <button
        type="button"
        aria-expanded={open}
        aria-label={`${primaryTask.title} ${taskValue(primaryTask)}. Show all background tasks`}
        onClick={() => setOpen((current) => !current)}
        className={cx(
          'group inline-flex h-7 items-center gap-1 rounded-[9px] border px-2 text-[10.5px] transition focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-cyan-300/50',
          primaryTask.kind === 'indexing'
            ? 'border-cyan-300/[0.14] bg-cyan-300/[0.055] text-cyan-100 hover:border-cyan-300/25 hover:bg-cyan-300/[0.09]'
            : 'border-amber-300/[0.14] bg-amber-300/[0.05] text-amber-100 hover:border-amber-300/25 hover:bg-amber-300/[0.08]'
        )}
      >
        <TaskIcon kind={primaryTask.kind} compact />
        <span className="max-w-[120px] truncate font-medium">{primaryTask.title}</span>
        <span className="ml-0.5 font-mono tabular-nums opacity-80">{taskValue(primaryTask)}</span>
        <svg
          width="13"
          height="13"
          viewBox="0 0 16 16"
          fill="none"
          aria-hidden
          className="ml-1 opacity-[0.65]"
        >
          <path
            d="M3 4.5h10M3 8h10M3 11.5h10"
            stroke="currentColor"
            strokeWidth="1.25"
            strokeLinecap="round"
          />
        </svg>
        {tasks.length > 1 ? (
          <span className="grid h-4 min-w-4 place-items-center rounded-full bg-current/10 px-1 font-mono text-[8.5px] tabular-nums">
            {tasks.length}
          </span>
        ) : null}
      </button>
    </div>
  )
}
