import {
  forwardRef,
  type ButtonHTMLAttributes,
  type FormEventHandler,
  type InputHTMLAttributes,
  type ReactNode,
} from 'react'
import type { ExtensionRunCommandResult } from '../../shared/extensionRuntime'
import BackgroundTaskStatus from '../BackgroundTaskStatus'
import { cx, HintBar } from './primitives'

const inputTypography = 'font-display text-[15px] font-normal text-ink-1'

export const CommandSurfaceInput = forwardRef<
  HTMLInputElement,
  InputHTMLAttributes<HTMLInputElement>
>(function CommandSurfaceInput({ className, ...props }, ref) {
  return (
    <input
      ref={ref}
      className={cx(
        'command-surface-input no-drag w-full min-w-0 flex-1 border-0 bg-transparent p-0 outline-none ring-0 placeholder:text-ink-4 focus:ring-0 select-text',
        inputTypography,
        className
      )}
      {...props}
    />
  )
})

export function CommandSurfaceTitle({ children }: { children: ReactNode }): JSX.Element {
  return <div className={cx('min-w-0 flex-1 truncate', inputTypography)}>{children}</div>
}

export function CommandSurfaceStatus({
  title,
  children,
}: {
  title: ReactNode
  children?: ReactNode
}): JSX.Element {
  return (
    <span className="truncate text-[11px] text-ink-3">
      {title}
      {children != null ? <span className="text-ink-4"> · {children}</span> : null}
    </span>
  )
}

export function CommandSurfaceChip({
  active,
  className,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { active?: boolean }): JSX.Element {
  return (
    <button
      type="button"
      className={cx(
        'launcher-mode-chip no-drag focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
        className
      )}
      aria-pressed={active}
      {...props}
    />
  )
}

export function CommandSurfaceIcon({
  name,
}: {
  name: 'plus' | 'trash' | 'refresh' | 'settings' | 'enter' | 'actions'
}): JSX.Element {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      {name === 'plus' ? (
        <path d="M12 5v14M5 12h14" />
      ) : name === 'trash' ? (
        <path d="M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7M14 10v7" />
      ) : name === 'refresh' ? (
        <path d="M20 7v5h-5M4 17v-5h5M6 6a8 8 0 0 1 13 2l1 4M4 12l1 4a8 8 0 0 0 13 2" />
      ) : name === 'settings' ? (
        <>
          <path d="M4 6h16M4 12h16M4 18h16M8 4v4M16 10v4M10 16v4" />
        </>
      ) : name === 'enter' ? (
        <path d="M20 4v10H4m5-5-5 5 5 5" />
      ) : (
        <>
          <circle cx="5" cy="12" r="1" />
          <circle cx="12" cy="12" r="1" />
          <circle cx="19" cy="12" r="1" />
        </>
      )}
    </svg>
  )
}

/** The chevron is a thin glyph centred in its 24px box; negative margins pull
 * it in so its gap to the input matches the launcher badge's. */
export function CommandSurfaceBackButton(
  props: ButtonHTMLAttributes<HTMLButtonElement>
): JSX.Element {
  return (
    <button
      type="button"
      aria-label="Back"
      className="command-surface-back no-drag -ml-1 -mr-1.5 grid h-6 w-6 shrink-0 place-items-center rounded-[7px] bg-transparent text-ink-3 transition hover:bg-white/[0.06] hover:text-ink-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
      {...props}
    >
      <svg
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.5"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
      >
        <path d="m15 18-6-6 6-6" />
      </svg>
    </button>
  )
}

/** Shared single-sheet chrome for the launcher and full-window views. */
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
  children?: ReactNode
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
