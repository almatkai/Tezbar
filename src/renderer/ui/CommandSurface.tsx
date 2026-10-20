import type { ButtonHTMLAttributes, FormEventHandler, ReactNode } from 'react'
import type { ExtensionRunCommandResult } from '../../shared/extensionRuntime'
import BackgroundTaskStatus from '../BackgroundTaskStatus'
import { cx, HintBar } from './primitives'

/** Shared chrome for launcher search, AI history, and the open agent conversation. */
export function CommandSurfaceHeader({
  children,
  overlay,
  onSubmit,
}: {
  children: ReactNode
  overlay?: ReactNode
  onSubmit?: FormEventHandler<HTMLFormElement>
}): JSX.Element {
  const row = (
    <div className="launcher-header-row flex h-11 min-w-0 items-center gap-2.5">{children}</div>
  )
  return (
    <header
      data-window-search-drag
      data-command-surface-header
      className="command-surface-header command-search relative z-30 shrink-0 select-none px-3.5"
    >
      {onSubmit ? (
        <form className="relative w-full" onSubmit={onSubmit}>
          {row}
        </form>
      ) : (
        row
      )}
      {overlay}
    </header>
  )
}

export function CommandSurfaceBadge({
  tone,
  label,
  children,
}: {
  tone: 'search' | 'ai' | 'terminal' | 'application' | 'deep'
  label?: string
  children: ReactNode
}): JSX.Element {
  return (
    <span
      role={label ? 'img' : undefined}
      aria-label={label}
      className={cx(
        'launcher-search-badge grid h-6 w-6 shrink-0 place-items-center rounded-[7px] text-white',
        tone === 'ai'
          ? 'command-ai-icon bg-violet-500'
          : tone === 'terminal'
            ? 'command-terminal-icon bg-emerald-600'
            : tone === 'application'
              ? 'bg-transparent'
              : tone === 'deep'
                ? 'bg-cyan-500'
                : 'bg-[#5b5ef0]'
      )}
    >
      {children}
    </span>
  )
}

export function CommandSurfaceAction({
  children,
  className,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement>): JSX.Element {
  return (
    <button
      type="button"
      className={cx(
        'launcher-action no-drag flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-ink-3 transition hover:bg-white/[0.06] hover:text-ink-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-40',
        className
      )}
      {...props}
    >
      {children}
    </button>
  )
}

export type CommandSurfaceTaskActions = {
  onOpenIndexing: () => void
  onOpenExtensionRuntime: (
    initial: Extract<ExtensionRunCommandResult, { ok: true; mode: 'view' }>
  ) => void
  onOpenUpdate: () => void
  onOpenKnowledgeSettings?: () => void
}

export function CommandSurfaceFooter({
  children,
  taskActions,
  status,
  trailing,
}: {
  children: ReactNode
  taskActions?: CommandSurfaceTaskActions
  status?: ReactNode
  trailing?: ReactNode
}): JSX.Element {
  return (
    <footer
      data-command-surface-footer
      className="command-surface-footer launcher-footer mt-auto shrink-0 pl-2 pr-3"
    >
      <div className="launcher-footer-row flex h-8 min-w-0 items-center gap-3">
        <div data-command-surface-status className="flex min-w-0 shrink items-center gap-3">
          {taskActions ? <BackgroundTaskStatus placement="start" {...taskActions} /> : null}
          {status}
        </div>
        <HintBar className="launcher-hints min-w-0 flex-1 flex-nowrap justify-end gap-x-3 overflow-hidden text-[11px]">
          {children}
        </HintBar>
        {trailing}
      </div>
    </footer>
  )
}
