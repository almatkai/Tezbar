import type { Stage } from '../../shared/agent'
import { cx } from '../ui/primitives'

export function AgentStageList({
  stages,
  compact = false,
}: {
  stages: Stage[]
  compact?: boolean
}): JSX.Element {
  return (
    <ol
      className={cx(
        'space-y-1 rounded-tezbar-row border border-white/[0.06] bg-white/[0.02] px-2 py-1.5',
        compact ? 'max-h-28 overflow-y-auto' : '',
      )}
    >
      {stages.map((stage) => (
        <li
          key={`stage:${stage.index}`}
          className="flex items-start gap-2 text-[12px] text-ink-2"
        >
          <span
            aria-hidden
            className={cx(
              'mt-1 h-1.5 w-1.5 shrink-0 rounded-full',
              stage.status === 'running'
                ? 'animate-pulse bg-violet-300'
                : stage.status === 'failed'
                  ? 'bg-rose-400'
                  : 'bg-emerald-400',
            )}
          />
          <span className="min-w-0 flex-1">
            <span className="block truncate font-mono text-[12px] text-ink-1">
              {stage.label}
            </span>
            {stage.detail ? (
              <span className="mt-0.5 block truncate text-[10.5px] text-ink-4">
                {stage.detail}
              </span>
            ) : null}
          </span>
          <span
            className={cx(
              'shrink-0 font-mono text-[9.5px] uppercase tracking-[0.14em]',
              stage.status === 'running'
                ? 'text-violet-200'
                : stage.status === 'failed'
                  ? 'text-rose-300'
                  : 'text-emerald-300',
            )}
          >
            {stage.status}
          </span>
        </li>
      ))}
    </ol>
  )
}
