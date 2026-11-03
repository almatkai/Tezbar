import { useEffect, useRef, useState } from 'react'
import type { AgentApprovalMode } from '../../shared/llmConfig'
import { cx } from '../ui/primitives'

export interface ApprovalModePickerProps {
  mode: AgentApprovalMode
  onSelect: (mode: AgentApprovalMode) => void
  disabled?: boolean
  className?: string
  direction?: 'up' | 'down'
}

const APPROVAL_OPTIONS: Array<{
  id: AgentApprovalMode
  title: string
  subtitle: string
}> = [
  {
    id: 'review',
    title: 'Request Review',
    subtitle: 'Every command requires approval.',
  },
  {
    id: 'default',
    title: 'Default',
    subtitle: 'Ask before sensitive operations.',
  },
  {
    id: 'auto',
    title: 'Auto (Preview)',
    subtitle: 'Automatically decide if commands run.',
  },
  {
    id: 'turbo',
    title: 'Turbo',
    subtitle: 'Run all commands without restrictions.',
  },
]

function getTriggerLabel(mode: AgentApprovalMode): string {
  switch (mode) {
    case 'review':
      return 'Request Review'
    case 'auto':
      return 'Auto (Preview)'
    case 'turbo':
      return 'Turbo'
    case 'default':
    default:
      return 'Default'
  }
}

export function ApprovalModePicker({
  mode,
  onSelect,
  disabled = false,
  className,
  direction = 'down',
}: ApprovalModePickerProps): JSX.Element {
  const [open, setOpen] = useState(false)
  const containerRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (!open) return
    const closeIfOutside = (event: PointerEvent): void => {
      if (!containerRef.current?.contains(event.target as Node)) {
        setOpen(false)
      }
    }
    const closeOnEscape = (event: KeyboardEvent): void => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      event.stopPropagation()
      setOpen(false)
    }
    document.addEventListener('pointerdown', closeIfOutside)
    window.addEventListener('keydown', closeOnEscape, true)
    return () => {
      document.removeEventListener('pointerdown', closeIfOutside)
      window.removeEventListener('keydown', closeOnEscape, true)
    }
  }, [open])

  return (
    <div ref={containerRef} className={cx('relative inline-flex items-center', className)}>
      <button
        type="button"
        disabled={disabled}
        onClick={() => setOpen((prev) => !prev)}
        aria-haspopup="listbox"
        aria-expanded={open}
        className={cx(
          'inline-flex h-7 items-center gap-1.5 rounded-lg border border-white/[0.08] bg-white/[0.04] px-2.5 text-[12px] font-medium text-ink-2 shadow-sm transition hover:border-white/[0.14] hover:bg-white/[0.07] hover:text-ink-1',
          open && 'border-white/[0.18] bg-white/[0.08] text-ink-1',
          disabled && 'pointer-events-none opacity-50'
        )}
        title="Agent permission mode"
      >
        <span>{getTriggerLabel(mode)}</span>
        <svg
          viewBox="0 0 16 16"
          aria-hidden="true"
          className={cx('h-3 w-3 shrink-0 text-ink-4 transition-transform duration-150', open && 'rotate-180')}
        >
          <path d="m4 6 4 4 4-4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>

      {open ? (
        <section
          aria-label="Choose permission mode"
          className={cx(
            'tezbar-popover absolute right-0 z-50 flex w-72 flex-col gap-0.5 overflow-hidden rounded-xl border border-white/10 bg-[#161618]/95 p-1.5 shadow-2xl backdrop-blur-xl animate-tezbar-fade-in',
            direction === 'up' ? 'bottom-9' : 'top-9'
          )}
        >
          {APPROVAL_OPTIONS.map((option) => {
            const isSelected = mode === option.id
            return (
              <button
                key={option.id}
                type="button"
                onClick={() => {
                  onSelect(option.id)
                  setOpen(false)
                }}
                className={cx(
                  'flex w-full items-center justify-between rounded-lg px-3 py-2 text-left transition',
                  isSelected
                    ? 'bg-white/[0.08] text-ink-1'
                    : 'text-ink-2 hover:bg-white/[0.05] hover:text-ink-1'
                )}
              >
                <div className="flex flex-col pr-2">
                  <span className="text-[13px] font-medium text-ink-1 leading-snug">{option.title}</span>
                  <span className="text-[11.5px] text-ink-3 leading-snug mt-0.5">{option.subtitle}</span>
                </div>
                {isSelected ? (
                  <svg
                    viewBox="0 0 16 16"
                    aria-hidden="true"
                    className="h-4 w-4 shrink-0 text-ink-1"
                  >
                    <path
                      d="m3 8.5 3.5 3.5 6.5-7"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="1.8"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    />
                  </svg>
                ) : null}
              </button>
            )
          })}
        </section>
      ) : null}
    </div>
  )
}
