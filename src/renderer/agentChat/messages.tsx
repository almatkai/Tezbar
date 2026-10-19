import { Markdown } from '../ui/Markdown'
import { formatLlmErrorMessage } from '../../shared/llmErrors'
import { AgentStageList } from './shared'
import {
  agentMessageError,
  agentMessageStages,
  agentMessageText,
  type AgentUIMessage,
} from './transport'

export function AgentMessage({
  message,
  streaming,
  onRetry,
  onDelete,
}: {
  message: AgentUIMessage
  streaming: boolean
  onRetry?: () => void
  onDelete?: () => void
}): JSX.Element {
  const text = agentMessageText(message)
  const stages = agentMessageStages(message)
  const error = agentMessageError(message)
  const stopped = error === 'Stopped by you.'
  const diagnostics = message.parts.find((part) => part.type === 'data-diagnostics')
  if (message.role === 'user') {
    return (
      <article aria-label="Your message" className="flex justify-end">
        <div className="agent-user-bubble max-w-[90%] rounded-2xl border border-white/[0.09] bg-white/[0.045] px-4 py-3 text-[13px] leading-relaxed text-ink-1">
          {message.parts.map((part, index) =>
            part.type === 'file' ? (
              <img
                key={index}
                src={part.url}
                alt={part.filename || 'Attached screen'}
                className="mb-2 max-h-40 rounded-lg object-contain"
              />
            ) : null
          )}
          <p className="whitespace-pre-wrap break-words select-text">{text}</p>
        </div>
      </article>
    )
  }
  return (
    <article aria-label="Agent response" className="agent-result min-w-0 space-y-3">
      <div className="flex items-center gap-2 text-[11px] text-ink-4">
        <span className="font-semibold text-ink-2">Agent</span>
        {message.metadata?.model ? (
          <span className="truncate" title="Requested model">
            {message.metadata.model}
          </span>
        ) : null}
        {streaming ? (
          <span className="ml-auto" role="status">
            Working…
          </span>
        ) : null}
      </div>
      {stages.length ? <AgentStageList stages={stages} /> : null}
      {text ? (
        <Markdown
          text={text}
          streaming={streaming && !message.parts.some((part) => part.type === 'data-answer')}
        />
      ) : null}
      {streaming && !text && !stages.length ? (
        <p className="text-[12px] text-ink-3" role="status">
          Waiting for the agent to respond…
        </p>
      ) : null}
      {error ? (
        <div
          role="alert"
          className={
            stopped
              ? 'rounded-lg border border-white/10 bg-white/[0.025] p-3 text-[12px] text-ink-3'
              : 'agent-run-error rounded-lg border border-rose-300/15 bg-rose-400/[0.06] p-3 text-[12px] text-rose-200'
          }
        >
          <p className="whitespace-pre-wrap break-words">{formatLlmErrorMessage(error)}</p>
          {onRetry ? (
            <button
              type="button"
              className="mt-2 rounded-md border border-white/10 px-2.5 py-1 text-ink-1 hover:bg-white/[0.06]"
              onClick={onRetry}
            >
              Try again
            </button>
          ) : null}
        </div>
      ) : null}
      {diagnostics?.type === 'data-diagnostics' && diagnostics.data.length ? (
        <details className="text-[11px] text-ink-4">
          <summary className="cursor-pointer">Diagnostics ({diagnostics.data.length})</summary>
          <pre className="mt-2 max-h-40 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-white/[0.025] p-3 font-mono text-[10.5px] select-text">
            {diagnostics.data.join('\n')}
          </pre>
        </details>
      ) : null}
      {!streaming ? (
        <div className="flex gap-3 text-[10.5px] text-ink-4">
          {text ? (
            <button
              type="button"
              className="hover:text-ink-1"
              onClick={() => void navigator.clipboard.writeText(text).catch(() => {})}
            >
              Copy response
            </button>
          ) : null}
          {onDelete ? (
            <button type="button" className="hover:text-rose-300" onClick={onDelete}>
              Delete response
            </button>
          ) : null}
        </div>
      ) : null}
    </article>
  )
}
