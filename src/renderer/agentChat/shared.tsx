import type { Stage } from '../../shared/agent'
import { cx } from '../ui/primitives'

export function AgentStageRow({ stage }: { stage: Stage }): JSX.Element {
  const stopped = stage.detail === 'Stopped by you.'
  const failed = stage.status === 'failed' && !stopped
  const running = stage.status === 'running'
  return (
    <details
      className="agent-tool-step min-w-0 rounded-lg border border-white/[0.075] bg-white/[0.02]"
      open={failed}
    >
      <summary className="flex cursor-pointer list-none items-center gap-2 px-3 py-2 text-[12px] outline-none focus-visible:ring-2 focus-visible:ring-accent [&::-webkit-details-marker]:hidden">
        <span
          aria-hidden
          className={cx(
            'flex h-4 w-4 shrink-0 items-center justify-center',
            failed ? 'text-rose-300' : running || stopped ? 'text-ink-3' : 'text-emerald-400'
          )}
        >
          {running ? (
            <span className="h-3 w-3 animate-spin rounded-full border border-current border-r-transparent motion-reduce:animate-none" />
          ) : failed ? (
            '!'
          ) : stopped ? (
            '–'
          ) : (
            '✓'
          )}
        </span>
        <span className="min-w-0 flex-1 break-words font-mono text-[11.5px] text-ink-2">
          {stage.label}
        </span>
        <span className={cx('shrink-0 text-[10.5px]', failed ? 'text-rose-300' : 'text-ink-4')}>
          {stopped
            ? 'Stopped'
            : running
              ? 'Running'
              : failed
                ? 'Failed'
                : stage.output
                  ? 'View output'
                  : 'Completed'}
          {stage.durationMs !== undefined ? ` · ${(stage.durationMs / 1000).toFixed(1)}s` : ''}
        </span>
      </summary>
      {stage.output ? (
        <pre
          tabIndex={0}
          aria-label={`${stage.label} output`}
          className="max-h-60 overflow-auto whitespace-pre-wrap break-words border-t border-white/[0.06] p-3 font-mono text-[11px] leading-relaxed text-ink-2 select-text"
        >
          {stage.output}
        </pre>
      ) : (
        <p className="px-3 pb-3 text-[11.5px] text-ink-3">
          {stage.detail ||
            (running ? 'Waiting for tool output…' : 'This step returned no text output.')}
        </p>
      )}
      {stage.output && stage.detail ? (
        <p className="px-3 pb-3 text-[11px] text-rose-300">{stage.detail}</p>
      ) : null}
      {stage.outputTruncated ? (
        <p className="px-3 pb-3 text-[10.5px] text-ink-4">
          Output limited to 8,000 characters. Ask for a narrower result to inspect more.
        </p>
      ) : null}
    </details>
  )
}

export function AgentStageList({
  stages,
  compact = false,
}: {
  stages: Stage[]
  compact?: boolean
}): JSX.Element {
  return (
    <div className={cx('min-w-0 space-y-1.5', compact ? 'max-h-44 overflow-y-auto' : '')}>
      {stages.map((stage) => (
        <AgentStageRow key={stage.index} stage={stage} />
      ))}
    </div>
  )
}
