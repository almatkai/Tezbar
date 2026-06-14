import {
  forwardRef,
  type ButtonHTMLAttributes,
  type HTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
} from 'react'

/** Compose class names. Ignores falsy values. */
export function cx(...values: Array<string | false | null | undefined>): string {
  return values.filter(Boolean).join(' ')
}

/* =========================================================================
   Button
   ========================================================================= */
type ButtonVariant = 'primary' | 'ghost' | 'quiet' | 'danger'

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant
  fullWidth?: boolean
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = 'ghost', fullWidth, className, type = 'button', ...rest },
  ref,
) {
  const base = 'btn'
  const v =
    variant === 'primary'
      ? 'btn-primary'
      : variant === 'danger'
        ? 'btn-danger'
        : variant === 'quiet'
          ? 'btn-quiet'
          : 'btn-ghost'
  const w = fullWidth ? 'w-full' : ''
  return <button ref={ref} type={type} className={cx(base, v, w, className)} {...rest} />
})

/* =========================================================================
   Text field / Select / Textarea
   ========================================================================= */
type TextFieldProps = InputHTMLAttributes<HTMLInputElement>

export const TextField = forwardRef<HTMLInputElement, TextFieldProps>(function TextField(
  { className, ...rest },
  ref,
) {
  return <input ref={ref} className={cx('glass-field', className)} {...rest} />
})

type TextAreaProps = TextareaHTMLAttributes<HTMLTextAreaElement>

export const TextArea = forwardRef<HTMLTextAreaElement, TextAreaProps>(function TextArea(
  { className, ...rest },
  ref,
) {
  return <textarea ref={ref} className={cx('glass-field min-h-[72px] resize-y font-mono text-[12px]', className)} {...rest} />
})

type SelectFieldProps = SelectHTMLAttributes<HTMLSelectElement>

export const SelectField = forwardRef<HTMLSelectElement, SelectFieldProps>(function SelectField(
  { className, children, ...rest },
  ref,
) {
  return (
    <select ref={ref} className={cx('glass-field', className)} {...rest}>
      {children}
    </select>
  )
})

/* =========================================================================
   Field label — lowercase, refined, tracked
   ========================================================================= */
export function FieldLabel({
  children,
  htmlFor,
  className,
}: {
  children: ReactNode
  htmlFor?: string
  className?: string
}): JSX.Element {
  return (
    <label
      htmlFor={htmlFor}
      className={cx(
        'mb-1.5 block text-[10px] font-medium uppercase tracking-[0.14em] text-ink-3',
        className,
      )}
    >
      {children}
    </label>
  )
}

/* =========================================================================
   Kbd — tiny key cap
   ========================================================================= */
export function Kbd({ children }: { children: ReactNode }): JSX.Element {
  return <kbd className="kbd">{children}</kbd>
}

/* =========================================================================
   Status dots — the animated "connected" indicator
   ========================================================================= */
export function StatusDots({ title }: { title?: string }): JSX.Element {
  return (
    <span className="status-dots" title={title ?? 'Connected'} aria-hidden>
      <span />
      <span />
      <span />
    </span>
  )
}

/* =========================================================================
   View header — reused across Providers, Settings, Extensions, OpenPorts
   ========================================================================= */
export function ViewHeader({
  title,
  trailing,
  onBack,
  backLabel,
}: {
  title: string
  trailing?: ReactNode
  onBack?: () => void
  backLabel?: string
}): JSX.Element {
  return (
    <header className="flex items-center justify-between gap-3 pb-1.5">
      <div className="flex min-w-0 items-center gap-2.5">
        {onBack ? (
          <button
            type="button"
            onClick={onBack}
            aria-label={backLabel ?? 'Back'}
            className="flex h-6 w-6 shrink-0 items-center justify-center rounded-tezbar-chip text-ink-3 transition hover:bg-white/[0.06] hover:text-ink-1"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
              <path d="m15 18-6-6 6-6" />
            </svg>
          </button>
        ) : null}
        <div className="min-w-0">
          <h1 className="truncate font-display text-[13px] font-semibold leading-[1.1] tracking-[0.01em] text-ink-1">
            {title}
          </h1>
        </div>
      </div>
      {trailing ? <div className="flex shrink-0 items-center gap-1.5">{trailing}</div> : null}
    </header>
  )
}

/* =========================================================================
   Hint bar — small inline keyboard legend
   ========================================================================= */
export function HintBar({ children, className }: { children: ReactNode; className?: string }): JSX.Element {
  return (
    <div className={cx('flex flex-wrap items-center gap-x-3 gap-y-1 text-[10.5px] text-ink-4', className)}>
      {children}
    </div>
  )
}

export function Hint({ label, keys }: { label: string; keys: ReactNode }): JSX.Element {
  return (
    <span className="inline-flex items-center gap-1.5">
      <span className="inline-flex items-center gap-0.5">{keys}</span>
      <span className="text-ink-3">{label}</span>
    </span>
  )
}

/* =========================================================================
   Inline message / banner
   ========================================================================= */
type MessageTone = 'neutral' | 'success' | 'error' | 'info'

export function Message({ tone = 'neutral', children }: { tone?: MessageTone; children: ReactNode }): JSX.Element {
  const toneClass =
    tone === 'success'
      ? 'text-emerald-300'
      : tone === 'error'
        ? 'text-rose-300'
        : tone === 'info'
          ? 'text-accent-strong'
          : 'text-ink-2'
  return (
    <p className={cx('text-[11.5px] leading-snug', toneClass)} role={tone === 'error' ? 'alert' : 'status'}>
      {children}
    </p>
  )
}

/* =========================================================================
   Section — for grouping settings
   ========================================================================= */
export function Section({
  title,
  description,
  children,
  className,
}: {
  title?: string
  description?: ReactNode
  children: ReactNode
  className?: string
}): JSX.Element {
  return (
    <section className={cx('space-y-2', className)}>
      {title ? (
        <h2 className="text-[11px] font-semibold tracking-tight text-ink-2">{title}</h2>
      ) : null}
      {description ? <p className="text-[12px] leading-relaxed text-ink-3">{description}</p> : null}
      {children}
    </section>
  )
}

/* =========================================================================
   Atmospheric accents — subtle glass glows for empty negative space
   ========================================================================= */
export function AmbientGlow({ className }: { className?: string } & HTMLAttributes<HTMLDivElement>): JSX.Element {
  return (
    <div
      aria-hidden
      className={cx(
        'pointer-events-none absolute inset-0 overflow-hidden [mask-image:linear-gradient(180deg,black,transparent)]',
        className,
      )}
    >
      <div className="absolute -left-20 -top-24 h-56 w-56 rounded-full bg-accent/25 blur-3xl" />
      <div className="absolute -right-16 -top-20 h-48 w-48 rounded-full bg-cyan-400/15 blur-3xl" />
    </div>
  )
}
