import type { ChatTransport, UIMessage, UIMessageChunk } from 'ai'
import type { AgentInputImage, AgentRunEvent, Stage } from '../../shared/agent'
import type { ChatResponseMeta, ChatSession, ChatTurn } from '../../shared/chat'
import type { RaymesApi } from '../../shared/desktop-api'
import { buildAgentPromptFromChat, makeChatId } from './model'

export type AgentMessageData = {
  stage: Stage
  answer: string
  approval: Extract<AgentRunEvent, { type: 'approval' }> | null
  diagnostics: string[]
  error: string
}
export type AgentMessageMetadata = Partial<ChatResponseMeta> & {
  sessionId: string
  createdAt: number
}
export type AgentUIMessage = UIMessage<AgentMessageMetadata, AgentMessageData>
export type AgentNativeApi = Pick<RaymesApi, 'agentRun' | 'agentCancel' | 'onAgentEvent'>

export function agentMessageText(message: AgentUIMessage): string {
  const final = message.parts.find((part) => part.type === 'data-answer')
  if (final?.type === 'data-answer') return final.data
  return message.parts.flatMap((part) => (part.type === 'text' ? [part.text] : [])).join('\n\n')
}
export function agentMessageStages(message: AgentUIMessage): Stage[] {
  return message.parts.flatMap((part) => (part.type === 'data-stage' ? [part.data] : []))
}
export function agentMessageError(message: AgentUIMessage): string | undefined {
  const part = message.parts.find((part) => part.type === 'data-error')
  return part?.type === 'data-error' ? part.data : undefined
}
export function agentMessageApproval(message: AgentUIMessage): AgentMessageData['approval'] {
  const part = message.parts.find((part) => part.type === 'data-approval')
  return part?.type === 'data-approval' ? part.data : null
}
export function finalizeAgentMessage(message: AgentUIMessage, failure?: string): AgentUIMessage {
  const error = failure || agentMessageError(message)
  const parts: AgentUIMessage['parts'] = message.parts.map((part) => {
    if (part.type === 'data-approval') return { ...part, data: null }
    if (part.type === 'data-stage' && part.data.status === 'running') {
      return {
        ...part,
        data: {
          ...part.data,
          status: 'failed',
          detail: error || 'Run ended before this step finished.',
        },
      }
    }
    if (part.type === 'data-error' && error) return { ...part, data: error }
    return part
  })
  if (error && !parts.some((part) => part.type === 'data-error'))
    parts.push({ type: 'data-error', id: 'error', data: error })
  return { ...message, parts }
}

export function chatTurnToAgentMessage(turn: ChatTurn, sessionId: string): AgentUIMessage {
  const parts: AgentUIMessage['parts'] = []
  const stages =
    turn.stages ??
    turn.timeline?.flatMap((item) => (item.type === 'stage' ? [item.stage] : [])) ??
    []
  for (const stage of stages)
    parts.push({ type: 'data-stage', id: `stage-${stage.index}`, data: stage })
  // The stored canonical response wins over any old, partial timeline text.
  const body =
    turn.text ||
    turn.timeline?.flatMap((item) => (item.type === 'text' ? [item.text] : [])).join('\n\n')
  if (body) parts.push({ type: 'text', text: body })
  if (turn.error) parts.push({ type: 'data-error', id: 'error', data: turn.error })
  for (const image of turn.attachments ?? []) {
    if (image.data)
      parts.push({
        type: 'file',
        mediaType: image.mimeType,
        filename: image.name,
        url: `data:${image.mimeType};base64,${image.data}`,
      })
  }
  const message: AgentUIMessage = {
    id: turn.id,
    role: turn.role,
    parts,
    metadata: { ...turn.responseMeta, sessionId, createdAt: turn.createdAt },
  }
  return turn.role === 'assistant'
    ? finalizeAgentMessage(
        message,
        turn.error || (!body?.trim() ? 'This saved run has no final response.' : undefined)
      )
    : message
}
export function agentMessageToTurn(message: AgentUIMessage, error?: string): ChatTurn {
  const stages = agentMessageStages(message)
  return {
    id: message.id,
    role: message.role === 'user' ? 'user' : 'assistant',
    text: agentMessageText(message),
    createdAt: message.metadata?.createdAt ?? Date.now(),
    responseMeta:
      message.metadata?.provider && message.metadata.providerTitle && message.metadata.model
        ? {
            provider: message.metadata.provider,
            providerTitle: message.metadata.providerTitle,
            model: message.metadata.model,
            tokenCount: message.metadata.tokenCount,
          }
        : undefined,
    stages: stages.length ? stages : undefined,
    error: error || agentMessageError(message),
  }
}

/** Real native IPC transport. No mock responses or HTTP/provider credentials in the WebView. */
export function createNativeAgentTransport(
  api: AgentNativeApi,
  getContext: () => { session: ChatSession; responseMeta: ChatResponseMeta }
): ChatTransport<AgentUIMessage> {
  let activeCancel: (() => void) | undefined
  return {
    reconnectToStream: async () => null,
    close: () => activeCancel?.(),
    async sendMessages({ messages, abortSignal, trigger, messageId }) {
      if (abortSignal?.aborted) throw new DOMException('Run cancelled', 'AbortError')
      const context = getContext()
      const lastUserIndex = messages.map((message) => message.role).lastIndexOf('user')
      const lastUser = messages[lastUserIndex]
      if (!lastUser) throw new Error('Enter a message to start the agent.')
      const prompt = agentMessageText(lastUser)
      const turns = messages
        .slice(0, lastUserIndex + 1)
        .map((message) => agentMessageToTurn(message))
      const task = buildAgentPromptFromChat({ ...context.session, turns }, prompt)
      const images: AgentInputImage[] = lastUser.parts.flatMap((part) => {
        if (part.type !== 'file') return []
        const match = /^data:(image\/(?:png|jpeg|webp));base64,(.+)$/s.exec(part.url)
        return match
          ? [{ type: 'image', mimeType: match[1] as AgentInputImage['mimeType'], data: match[2]! }]
          : []
      })
      const runId = `agent-${makeChatId()}`
      let cancel = (): void => {}
      const stream = new ReadableStream<UIMessageChunk>({
        start(controller) {
          let closed = false
          let unsubscribe = (): void => {}
          let textOpen = false
          let streamedText = ''
          let finalAnswer: string | undefined
          const diagnostics: string[] = []
          const stages = new Map<number, Stage>()
          const emit = (chunk: UIMessageChunk): void => {
            if (!closed) controller.enqueue(chunk)
          }
          const endText = (): void => {
            if (textOpen) {
              emit({ type: 'text-end', id: 'response' })
              textOpen = false
            }
          }
          const cleanup = (): void => {
            clearTimeout(startupTimeout)
            unsubscribe()
            abortSignal?.removeEventListener('abort', cancel)
            if (activeCancel === cancel) activeCancel = undefined
          }
          const end = (): void => {
            if (!closed) {
              closed = true
              cleanup()
              controller.close()
            }
          }
          const fail = (error: unknown): void => {
            if (closed) return
            const message = error instanceof Error ? error.message : String(error)
            emit({ type: 'data-error', id: 'error', data: message })
            endText()
            emit({ type: 'error', errorText: message })
            end()
          }
          const stopNative = (): void => {
            void api.agentCancel(runId).catch(() => {})
          }
          cancel = (): void => {
            if (closed) return
            for (const stage of stages.values()) {
              if (stage.status === 'running')
                emit({
                  type: 'data-stage',
                  id: `stage-${stage.index}`,
                  data: { ...stage, status: 'failed', detail: 'Stopped by you.' },
                })
            }
            emit({ type: 'data-approval', id: 'approval', data: null })
            emit({ type: 'data-error', id: 'error', data: 'Stopped by you.' })
            endText()
            emit({ type: 'abort', reason: 'Stopped by you.' })
            end()
            stopNative()
          }
          activeCancel = cancel
          const startupTimeout = setTimeout(() => {
            fail(
              new Error(
                'The agent did not start within 30 seconds. Check the selected provider and try again.'
              )
            )
            stopNative()
          }, 30_000)
          emit({
            type: 'start',
            messageId: trigger === 'regenerate-message' && messageId ? messageId : makeChatId(),
            messageMetadata: {
              ...context.responseMeta,
              sessionId: context.session.id,
              createdAt: Date.now(),
            },
          })
          // Subscribe before requesting a run. Its id is already known, so even
          // synchronous events and late events from an older run are unambiguous.
          unsubscribe = api.onAgentEvent((event) => {
            if (closed || event.runId !== runId) return
            switch (event.type) {
              case 'start':
                clearTimeout(startupTimeout)
                break
              case 'message':
                if (!textOpen) {
                  emit({ type: 'text-start', id: 'response' })
                  textOpen = true
                }
                streamedText += event.delta
                emit({ type: 'text-delta', id: 'response', delta: event.delta })
                break
              case 'stage':
                stages.set(event.stage.index, event.stage)
                emit({ type: 'data-stage', id: `stage-${event.stage.index}`, data: event.stage })
                break
              case 'approval':
                emit({ type: 'data-approval', id: 'approval', data: event })
                break
              case 'log':
                diagnostics.push(event.line.slice(0, 2_000))
                if (diagnostics.length > 80) diagnostics.shift()
                emit({ type: 'data-diagnostics', id: 'diagnostics', data: diagnostics.slice() })
                break
              case 'answer':
                finalAnswer = event.text
                emit({ type: 'data-answer', id: 'answer', data: event.text })
                break
              case 'error':
                fail(new Error(event.message))
                break
              case 'done':
                if (!(finalAnswer ?? streamedText).trim()) {
                  fail(new Error('Agent finished without a response.'))
                  break
                }
                emit({ type: 'data-approval', id: 'approval', data: null })
                endText()
                emit({ type: 'finish', finishReason: 'stop' })
                end()
                break
            }
          })
          abortSignal?.addEventListener('abort', cancel, { once: true })
          if (abortSignal?.aborted) {
            cancel()
            return
          }
          void Promise.resolve()
            .then(() => {
              if (closed || abortSignal?.aborted) return { ok: false, error: 'Run cancelled' }
              return api.agentRun({
                runId,
                task,
                cwd: context.session.workingDirectory,
                images: images.length ? images : undefined,
              })
            })
            .then((result) => {
              if (closed) return
              if (!result.ok) fail(new Error(result.error || 'The agent could not start.'))
              else if (result.runId !== runId)
                fail(
                  new Error(
                    'The agent returned an unexpected run id. Restart Tezbar and try again.'
                  )
                )
            })
            .catch(fail)
        },
        cancel() {
          cancel()
        },
      })
      return stream
    },
  }
}
