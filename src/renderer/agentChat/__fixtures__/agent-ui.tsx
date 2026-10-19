/** Dev-only, scripted UI regression fixture. Not imported by the production app. */
import { createRoot } from 'react-dom/client'
import { createChat } from '@shadcn/helpers/ai-sdk'
import AgentChatView from '../../AgentChatView'
import type { AgentRunEvent, AgentRunRequest } from '../../../shared/agent'
import type { RaymesApi } from '../../../shared/desktop-api'
import type { ChatSession, ChatSessionSummary, ChatTurn } from '../../../shared/chat'
import type { AgentUIMessage } from '../transport'
import '../../styles.css'

const scenario = new URLSearchParams(location.search).get('scenario') || 'normal'
const listeners = new Set<(event: AgentRunEvent) => void>()
const sessions = new Map<string, ChatSession>()
const approvals = new Map<string, (allowed: boolean) => void>()
const runs = new Map<string, AbortController>()
const calls: string[] = []
const emit = (event: AgentRunEvent) => {
  for (const listener of listeners) listener(event)
}
let config = {
  provider: 'openai',
  model: 'fixture-model',
  providerModels: { openai: [{ id: 'fixture-model', capabilities: ['text', 'code'] }] },
}
const report =
  '## CPU check\n\n| Process | CPU |\n| --- | ---: |\n| Browser | 12% |\n| Editor | 4% |\n\nThe browser is the busiest process in this scripted example. No processes were stopped.'

async function execute(request: AgentRunRequest): Promise<void> {
  const runId = request.runId!
  const abort = new AbortController()
  runs.set(runId, abort)
  emit({ type: 'start', runId, task: request.task })
  const script = createChat<AgentUIMessage>().user(request.task)
  if (scenario === 'error')
    script.error('Provider unavailable. Check your connection and try again.')
  else
    script.assistant(({ writer }) => {
      if (scenario === 'empty') return
      if (scenario === 'approval')
        writer.data({
          type: 'data-approval',
          id: 'approval',
          data: {
            type: 'approval',
            runId,
            approvalId: 'fixture-approval',
            title: 'Allow CPU inspection?',
            command: 'ps -axo pid,pcpu,comm',
          },
        })
      writer.data({
        type: 'data-stage',
        id: 'stage-0',
        data: {
          index: 0,
          label: 'bash: ps -axo pid,pcpu,comm',
          status: 'running',
          toolName: 'bash',
        },
      })
      writer.sleep(scenario === 'cancel' ? 5_000 : scenario === 'fast' ? 0 : 200)
      writer.data({
        type: 'data-stage',
        id: 'stage-0',
        data: {
          index: 0,
          label: 'bash: ps -axo pid,pcpu,comm',
          status: 'done',
          toolName: 'bash',
          durationMs: 200,
          output: 'PID  CPU  Process\n42   12%  Browser\n51    4%  Editor',
        },
      })
      writer.text(report, { mode: scenario === 'fast' ? 'instant' : 'stream' })
    })
  let answer = ''
  let previewSent = false
  try {
    const stream = await script
      .transport({ delayMs: 8 })
      .sendMessages({
        trigger: 'submit-message',
        chatId: runId,
        messageId: undefined,
        messages: script.get(1),
        abortSignal: abort.signal,
      })
    const reader = stream.getReader()
    while (true) {
      const { value: chunk, done } = await reader.read()
      if (done || abort.signal.aborted) break
      if (chunk.type === 'text-delta') {
        answer += chunk.delta
        if (scenario !== 'fast' || !previewSent) {
          emit({ type: 'message', runId, delta: scenario === 'fast' ? 'CPU' : chunk.delta })
          previewSent = true
        }
      } else if (chunk.type === 'data-stage') {
        emit({
          type: 'stage',
          runId,
          stage: chunk.data as Extract<AgentRunEvent, { type: 'stage' }>['stage'],
        })
      } else if (chunk.type === 'data-approval') {
        const approval = chunk.data as Extract<AgentRunEvent, { type: 'approval' }>
        emit(approval)
        const allowed = await new Promise<boolean>((resolve) => {
          approvals.set(approval.approvalId, resolve)
        })
        if (!allowed) {
          emit({
            type: 'answer',
            runId,
            text: 'I did not run the command because you denied permission.',
          })
          emit({ type: 'done', runId })
          await reader.cancel()
          return
        }
      } else if (chunk.type === 'error') {
        emit({ type: 'error', runId, message: chunk.errorText })
        emit({ type: 'done', runId })
        return
      }
    }
    if (!abort.signal.aborted) {
      if (answer) emit({ type: 'answer', runId, text: answer })
      emit({ type: 'done', runId })
    }
  } catch (error) {
    if (!abort.signal.aborted) {
      emit({ type: 'error', runId, message: String(error) })
      emit({ type: 'done', runId })
    }
  } finally {
    runs.delete(runId)
  }
}

const api: Partial<RaymesApi> = {
  getLlmConfig: async () => config as Awaited<ReturnType<RaymesApi['getLlmConfig']>>,
  setLlmConfig: async (patch) => {
    config = { ...config, ...patch } as typeof config
  },
  listLlmModels: async () => ['fixture-model'],
  onLlmConfigChanged: () => () => {},
  onWindowShown: () => () => {},
  chatList: async () =>
    [...sessions.values()].map(
      ({ turns, ...item }): ChatSessionSummary => ({
        ...item,
        turnCount: turns.length,
        preview: turns.at(-1)?.text || '',
      })
    ),
  chatGet: async (id) => sessions.get(id) ?? null,
  chatAppend: async ({ session, turn }) => {
    const old = sessions.get(session.id)?.turns ?? []
    const turns: ChatTurn[] = old.some((item) => item.id === turn.id)
      ? old.map((item) => (item.id === turn.id ? turn : item))
      : [...old, turn]
    sessions.set(session.id, { ...session, turns })
    return { ok: true }
  },
  chatDeleteTurn: async (id, turnId) => {
    const session = sessions.get(id)
    if (session) session.turns = session.turns.filter((turn) => turn.id !== turnId)
    return { ok: true }
  },
  chatDelete: async (id) => {
    sessions.delete(id)
    return { ok: true }
  },
  agentRun: async (request) => {
    if (typeof request === 'string') throw new Error('Expected a correlated native request')
    calls.push(request.runId!)
    void execute(request)
    return { ok: true, runId: request.runId }
  },
  agentCancel: async (id) => {
    if (id) runs.get(id)?.abort()
    for (const resolve of approvals.values()) resolve(false)
    approvals.clear()
    return { ok: true }
  },
  agentApprove: async ({ approvalId, decision }) => {
    approvals.get(approvalId)?.(decision !== 'deny')
    approvals.delete(approvalId)
    return { ok: true }
  },
  onAgentEvent: (listener) => {
    listeners.add(listener)
    return () => {
      listeners.delete(listener)
    }
  },
  captureActiveScreen: async () => {
    throw new Error('Screen capture is intentionally disabled in this scripted fixture.')
  },
}
window.tezbar = new Proxy(api, {
  get(target, key) {
    if (key in target) return Reflect.get(target, key)
    throw new Error(`Unexpected native method in fixture: ${String(key)}`)
  },
}) as RaymesApi
Object.assign(window, { __agentFixture: { calls, sessions, listeners, runs } })
document.documentElement.dataset.theme =
  new URLSearchParams(location.search).get('theme') === 'light' ? 'white' : 'dark'
const style = document.createElement('style')
style.textContent =
  'html,body,#fixture-root{height:100%;margin:0}body{background:#17181d}#fixture-root{display:flex;flex-direction:column}.fixture-note{flex-shrink:0;padding:6px 12px;background:#39311b;color:#f0d990;font:11px system-ui}.fixture-app{min-height:0;flex:1;padding:8px;display:flex;flex-direction:column}:root[data-theme="white"] body{background:#f7f8fa}'
document.head.append(style)
createRoot(document.getElementById('fixture-root')!).render(
  <>
    <div className="fixture-note">
      Scripted UI regression fixture · @shadcn/helpers/ai-sdk · not live machine data
    </div>
    <div className="fixture-app">
      <AgentChatView
        boot={{ kind: 'newChat' }}
        onBack={() => calls.push('back')}
        onOpenSettings={() => calls.push('settings')}
      />
    </div>
  </>
)
