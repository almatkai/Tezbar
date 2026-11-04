// @vitest-environment jsdom
import { act, createElement, Suspense, type ComponentProps, type ReactNode } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import AgentChatView from '../AgentChatView'
import { clearCachedChatHistory } from './historyCache'
import type { AgentInputImage, AgentRunEvent, AgentRunRequest } from '../../shared/agent'
import { createAiChatNavigation, type AiChatBoot } from '../../shared/aiChatSurface'
import type { LlmConfigRecord } from '../../shared/llmConfig'
import type { RaymesApi } from '../../shared/desktop-api'
import type {
  ActiveAgentRunInfo,
  ChatSession,
  ChatSessionSummary,
  ChatTurn,
} from '../../shared/chat'

const noop = () => {}
const deferred = <T>() => {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((r) => (resolve = r))
  return { promise, resolve }
}
const saved = (id: string): ChatSession => ({
  id,
  title: `Saved ${id}`,
  createdAt: 1,
  updatedAt: 2,
  turns: [
    { id: `${id}-user`, role: 'user', text: `Question for ${id}`, createdAt: 1 },
    { id: `${id}-assistant`, role: 'assistant', text: `Answer for ${id}`, createdAt: 2 },
  ],
})
let root: Root, container: HTMLDivElement
let listeners: Set<(event: AgentRunEvent) => void>,
  runs: AgentRunRequest[],
  store: Map<string, ChatSession>,
  api: ReturnType<typeof makeApi>
let boot: AiChatBoot
let unmounted: boolean
const onActiveRunChange = vi.fn<(run: ActiveAgentRunInfo | null) => void>()
const tick = async (ms = 55) => {
  await act(async () => {
    await new Promise((r) => setTimeout(r, ms))
  })
}
const emit = async (event: AgentRunEvent) => {
  await act(async () => listeners.forEach((l) => l(event)))
  await tick()
}
const click = async (selector: string) => {
  const b = container.querySelector(selector) as HTMLButtonElement
  expect(b).toBeTruthy()
  await act(async () => b.click())
  await tick()
}
const render = async (
  nextBoot: AiChatBoot = { kind: 'newChat' },
  extra: Partial<ComponentProps<typeof AgentChatView>> = {}
) => {
  boot = nextBoot
  await act(async () =>
    root.render(
      createElement(AgentChatView, {
        boot,
        onBack: noop,
        onOpenSettings: noop,
        onActiveRunChange,
        ...extra,
      })
    )
  )
  await tick()
}
const send = async (text: string) => {
  const input = container.querySelector('textarea') as HTMLTextAreaElement
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(input, text)
    input.dispatchEvent(new Event('input', { bubbles: true }))
  })
  await act(async () =>
    container
      .querySelector('form')!
      .dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
  )
  await tick()
}
const append = async ({ session, turn }: { session: ChatSession; turn: ChatTurn }) => {
  const current = store.get(session.id) ?? { ...session, turns: [] }
  const turns = current.turns.some((t) => t.id === turn.id)
    ? current.turns.map((t) => (t.id === turn.id ? turn : t))
    : [...current.turns, turn]
  store.set(session.id, { ...current, ...session, turns })
  return { ok: true }
}
beforeEach(() => {
  clearCachedChatHistory()
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true)
  listeners = new Set()
  runs = []
  store = new Map([['B', saved('B')]])
  api = makeApi()
  unmounted = false
  onActiveRunChange.mockClear()
  Object.defineProperty(window, 'tezbar', { configurable: true, value: api })
  container = document.createElement('div')
  document.body.append(container)
  root = createRoot(container)
})
function makeApi() {
  return {
    getLlmConfig: vi.fn(
      async (): Promise<LlmConfigRecord> => ({ provider: 'ollama', model: 'llama3.2' })
    ),
    setLlmConfig: vi.fn(async () => ({ ok: true })),
    onLlmConfigChanged: () => noop,
    chatList: vi.fn(
      async (): Promise<ChatSessionSummary[]> =>
        [...store.values()].map((s) => ({
          ...s,
          turnCount: s.turns.length,
          preview: s.turns.at(-1)?.text ?? '',
        }))
    ),
    chatGet: vi.fn(async (id: string) => structuredClone(store.get(id) ?? null)),
    chatAppend: vi.fn(append),
    chatDelete: vi.fn(async (id: string) => {
      store.delete(id)
      return { ok: true }
    }),
    chatGenerateMeta: vi.fn<RaymesApi['chatGenerateMeta']>(async () => null),
    agentRun: vi.fn(async (req: AgentRunRequest) => {
      runs.push(req)
      listeners.forEach((l) => l({ type: 'start', runId: req.runId!, task: req.task }))
      return { ok: true, runId: req.runId }
    }),
    agentCancel: vi.fn<RaymesApi['agentCancel']>(async () => ({ ok: true })),
    agentApprove: vi.fn<RaymesApi['agentApprove']>(async () => ({ ok: true })),
    onAgentEvent: (l: (event: AgentRunEvent) => void) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
    captureActiveScreen: vi.fn(
      async (): Promise<AgentInputImage> => ({
        type: 'image',
        mimeType: 'image/png',
        data: 'aGVsbG8=',
      })
    ),
  }
}
afterEach(async () => {
  for (const req of runs)
    if (listeners.size) await emit({ type: 'error', runId: req.runId!, message: 'test cleanup' })
  if (!unmounted) await act(async () => root.unmount())
  container.remove()
  vi.unstubAllGlobals()
})

describe('agent conversation ownership', () => {
  it('keeps an unsaved background response available when persistence fails', async () => {
    api.chatAppend.mockImplementation(async (payload) => {
      if (payload.turn.role === 'assistant') return { ok: false }
      return append(payload)
    })
    await render()
    await send('Background A')
    const runId = runs[0]!.runId!
    const aId = [...store.keys()].find((id) => id !== 'B')!
    await render({ kind: 'newChat', nonce: 'B' })
    await emit({ type: 'answer', runId, text: 'Unsaved A result' })
    await emit({ type: 'done', runId })
    await render({ kind: 'resume', sessionId: aId, nonce: 'A-again' })
    expect(container.textContent).toContain('Unsaved A result')
  })

  it('reloads a settled background conversation from saved history after releasing its live owner', async () => {
    await render()
    await send('Background A')
    const runId = runs[0]!.runId!
    const aId = [...store.keys()].find((id) => id !== 'B')!
    await render({ kind: 'newChat', nonce: 'B' })
    await emit({ type: 'answer', runId, text: 'Saved A result' })
    await emit({ type: 'done', runId })
    expect(api.agentCancel).not.toHaveBeenCalled()
    // Once a run is settled, the saved transcript is authoritative on reopening.
    store.get(aId)!.turns.at(-1)!.text = 'Updated saved A result'
    await render({ kind: 'resume', sessionId: aId, nonce: 'A-again' })
    expect(container.textContent).toContain('Updated saved A result')
  })

  it('stop persists partial text and terminal stage state with no active approval', async () => {
    await render()
    await send('First task')
    const runId = runs[0]!.runId!
    await emit({ type: 'message', runId, delta: 'Partial text' })
    await emit({
      type: 'stage',
      runId,
      stage: { index: 0, label: 'Running command', status: 'running' },
    })
    await emit({
      type: 'approval',
      runId,
      approvalId: 'pending',
      command: 'mkdir example',
      title: 'Allow?',
    })
    await click('[aria-label="Stop agent"]')
    expect(api.agentCancel).toHaveBeenCalledWith(runId)
    const session = [...store.values()].find((s) => s.id !== 'B')!
    const turn = session.turns.at(-1)!
    expect(turn.text).toBe('Partial text')
    expect(turn.error).toBe('Stopped by you.')
    expect(turn.stages![0]!.status).toBe('failed')
    expect(container.querySelector('[aria-label="Command approval"]')).toBeNull()
  })
  it('successful retry replaces the failed persisted assistant id', async () => {
    await render()
    await send('First task')
    await emit({ type: 'error', runId: runs[0]!.runId!, message: 'Mock failure' })
    const session = [...store.values()].find((s) => s.id !== 'B')!,
      oldId = session.turns.at(-1)!.id
    const retry = [...container.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Try again'
    )!
    await act(async () => retry.click())
    await tick()
    await emit({ type: 'answer', runId: runs[1]!.runId!, text: 'Success' })
    await emit({ type: 'done', runId: runs[1]!.runId! })
    const turns = store.get(session.id)!.turns
    expect(turns).toHaveLength(2)
    expect(turns.at(-1)!.id).toBe(oldId)
    expect(turns.at(-1)!.text).toBe('Success')
    expect(turns.at(-1)!.error).toBeUndefined()
  })

  it('keeps A output and persistence out of B, even while B starts its own run', async () => {
    await render()
    await send('Start A')
    const first = runs[0]!,
      aId = [...store.keys()].find((id) => id !== 'B')!
    await emit({ type: 'message', runId: first.runId!, delta: 'Private A' })
    await render({ kind: 'resume', sessionId: 'B', nonce: 'B-visit' })
    expect(container.textContent).toContain('Answer for B')
    await emit({ type: 'message', runId: first.runId!, delta: ' more A' })
    expect(container.textContent).not.toContain('Private A')
    await send('Continue B')
    expect(runs).toHaveLength(2)
    expect(runs[1]!.task).toContain('Answer for B')
    expect(runs[1]!.task).not.toContain('Private A')
    expect(api.agentCancel).not.toHaveBeenCalled()
    expect(listeners.size).toBe(2)
    await emit({ type: 'done', runId: first.runId! })
    expect(store.get(aId)!.turns.at(-1)!.text).toBe('Private A more A')
    expect(store.get('B')!.turns.map((t) => t.text)).not.toContain('Private A more A')
    await render({ kind: 'resume', sessionId: aId, nonce: 'A-return' })
    expect(container.textContent).toContain('Private A more A')
    expect(container.textContent).not.toContain('Continue B')
  })
  it('preserves an active approval when resuming its conversation', async () => {
    await render()
    await send('Start A')
    const first = runs[0]!,
      aId = [...store.keys()].find((id) => id !== 'B')!
    await emit({
      type: 'approval',
      runId: first.runId!,
      approvalId: 'approval-A',
      title: 'Allow A?',
      command: 'mkdir example',
    })
    await render({ kind: 'resume', sessionId: aId, nonce: 'A-again' })
    expect(container.querySelector('[aria-label="Command approval"]')).toBeTruthy()
    await render({ kind: 'newChat', nonce: 'another-chat' })
    await send('Independent task')
    expect(runs).toHaveLength(2)
    expect(api.agentCancel).not.toHaveBeenCalled()
    await render({ kind: 'resume', sessionId: aId, nonce: 'A-return' })
    expect(container.querySelector('[aria-label="Command approval"]')).toBeTruthy()
    const allow = [...container.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Allow once'
    )!
    await act(async () => allow.click())
    await tick()
    expect(api.agentApprove).toHaveBeenCalledWith({
      runId: first.runId,
      approvalId: 'approval-A',
      decision: 'once',
    })
    await render({ kind: 'newChat', nonce: 'after-approval' })
    await render({ kind: 'resume', sessionId: aId, nonce: 'A-after-approval' })
    expect(container.querySelector('[aria-label="Command approval"]')).toBeNull()
  })
  it('starts fresh on every launcher new-chat event, including the real caller nonce contract', async () => {
    const first = createAiChatNavigation({ kind: 'newChat' })
    const second = createAiChatNavigation({ kind: 'newChat' })
    expect(second.nonce).not.toBe(first.nonce)
    await render(first)
    await send('First task')
    await emit({ type: 'answer', runId: runs[0]!.runId!, text: 'First answer' })
    await emit({ type: 'done', runId: runs[0]!.runId! })
    await render(second)
    expect(container.textContent).not.toContain('First answer')
    await send('Second task')
    expect(runs[1]!.task).not.toContain('First task')
    expect(runs[1]!.task).not.toContain('First answer')
    expect(store.size).toBe(3)
  })
  it('honors repeat screen and resume nonces while mounted', async () => {
    await render({ kind: 'screen', nonce: 'screen-1' })
    await render({ kind: 'screen', nonce: 'screen-2' })
    expect(api.captureActiveScreen).toHaveBeenCalledTimes(2)
    await render({ kind: 'resume', sessionId: 'B', nonce: 'resume-1' })
    await click('[aria-label="New chat"]')
    await render({ kind: 'resume', sessionId: 'B', nonce: 'resume-2' })
    expect(container.textContent).toContain('Answer for B')
  })
  it('keeps a composer submission when initial history resolves late', async () => {
    const history = deferred<ChatSessionSummary[]>()
    api.chatList.mockImplementationOnce(() => history.promise)
    await render()
    await send('Before history')
    const runId = runs[0]!.runId!
    await act(async () => history.resolve([]))
    await tick()
    expect(container.textContent).toContain('Before history')
    await emit({ type: 'answer', runId, text: 'Still here' })
    await emit({ type: 'done', runId })
    await send('Continue it')
    expect(runs[1]!.task).toContain('Before history')
    expect(runs[1]!.task).toContain('Still here')
  })
  it('ignores stale initialization after a different boot', async () => {
    const config = deferred<LlmConfigRecord>()
    api.getLlmConfig.mockImplementationOnce(() => config.promise)
    await render({ kind: 'newChat', nonce: 'old' })
    await render({ kind: 'screen', nonce: 'new' })
    await act(async () => config.resolve({ provider: 'ollama', model: 'old-config' }))
    await tick()
    expect(runs).toHaveLength(0)
    expect(api.captureActiveScreen).toHaveBeenCalledTimes(1)
    expect(container.querySelector('[alt="Attached active screen"]')).toBeTruthy()
  })
  it('discards a capture that completes after navigation', async () => {
    const capture = deferred<AgentInputImage>()
    api.captureActiveScreen.mockImplementationOnce(() => capture.promise)
    await render({ kind: 'resume', sessionId: 'B' })
    await click('[aria-label="Attach active screen"]')
    await click('[aria-label="New chat"]')
    await act(async () => capture.resolve({ type: 'image', mimeType: 'image/png', data: 'b2xk' }))
    await tick()
    expect(container.querySelector('[alt="Attached active screen"]')).toBeNull()
    await send('Different task')
    expect(runs[0]!.images).toBeUndefined()
  })
  it('deduplicates capture shortcuts and prevents stale cleanup overwriting a newer capture', async () => {
    const old = deferred<AgentInputImage>(),
      fresh = deferred<AgentInputImage>()
    api.captureActiveScreen
      .mockImplementationOnce(() => old.promise)
      .mockImplementationOnce(() => fresh.promise)
    await render()
    const captureShortcut = () =>
      window.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'S', metaKey: true, shiftKey: true })
      )
    await act(async () => {
      captureShortcut()
      captureShortcut()
    })
    await tick()
    expect(api.captureActiveScreen).toHaveBeenCalledTimes(1)
    await click('[aria-label="New chat"]')
    await click('[aria-label="Attach active screen"]')
    expect(api.captureActiveScreen).toHaveBeenCalledTimes(2)
    await act(async () => old.resolve({ type: 'image', mimeType: 'image/png', data: 'b2xk' }))
    await tick()
    expect(container.querySelector('[alt="Attached active screen"]')).toBeNull()
    expect(
      (container.querySelector('[aria-label="Attach active screen"]') as HTMLButtonElement).disabled
    ).toBe(true)
    await act(async () => fresh.resolve({ type: 'image', mimeType: 'image/png', data: 'bmV3' }))
    await tick()
    await send('Use this screen')
    expect(runs[0]!.images?.[0]?.data).toBe('bmV3')
  })

  it('continues hidden runs and publishes background approval status while viewing a different chat', async () => {
    await render()
    await send('Start A')
    const first = runs[0]!,
      aId = [...store.keys()].find((id) => id !== 'B')!
    await render(boot, { active: false })
    await emit({ type: 'message', runId: first.runId!, delta: 'Hidden output' })
    expect(onActiveRunChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ sessionId: aId, busy: true, streamingText: 'Hidden output' })
    )
    await render({ kind: 'resume', sessionId: 'B', nonce: 'B-new' })
    await send('Start B')
    await emit({
      type: 'approval',
      runId: first.runId!,
      approvalId: 'hidden-approval',
      title: 'Allow hidden command?',
      command: 'mkdir example',
    })
    expect(onActiveRunChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ sessionId: aId, waitingApproval: true })
    )
    expect(container.querySelector('[aria-label="Command approval"]')).toBeNull()
    expect(api.agentCancel).not.toHaveBeenCalled()
    await render({ kind: 'resume', sessionId: aId, nonce: 'A-resume' })
    expect(container.querySelector('[aria-label="Command approval"]')).toBeTruthy()
    await emit({ type: 'done', runId: first.runId! })
    expect(onActiveRunChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ sessionId: 'B', busy: true })
    )
  })
  it('keeps cwd and image with their owner if config preparation finishes after starting a new chat', async () => {
    await render({ kind: 'newChat', workingDirectory: '/work/A' })
    await click('[aria-label="Attach active screen"]')
    const config = deferred<LlmConfigRecord>()
    api.getLlmConfig.mockImplementationOnce(() => config.promise)
    await send('Start A later')
    await render({ kind: 'newChat', workingDirectory: '/work/B', nonce: 'B-new' })
    await send('Start B now')
    await act(async () => config.resolve({ provider: 'ollama', model: 'llama3.2' }))
    await tick()
    expect(runs).toHaveLength(2)
    const a = runs.find((run) => run.task.includes('Start A later'))!
    const b = runs.find((run) => run.task.includes('Start B now'))!
    expect(a.cwd).toBe('/work/A')
    expect(a.images?.[0]?.data).toBe('aGVsbG8=')
    expect(b.cwd).toBe('/work/B')
    expect(b.images).toBeUndefined()
    expect(container.textContent).not.toContain('Start A later')
  })
  it('discards capture results after hiding the surface', async () => {
    const capture = deferred<AgentInputImage>()
    api.captureActiveScreen.mockImplementationOnce(() => capture.promise)
    await render()
    await click('[aria-label="Attach active screen"]')
    await render(boot, { active: false })
    await act(async () => capture.resolve({ type: 'image', mimeType: 'image/png', data: 'b2xk' }))
    await tick()
    await render(boot)
    expect(container.querySelector('[alt="Attached active screen"]')).toBeNull()
  })
  it('cancels and unsubscribes all retained streams only on actual unmount', async () => {
    await render()
    await send('Start A')
    await render({ kind: 'newChat', nonce: 'B-new' })
    await send('Start B')
    expect(listeners.size).toBe(2)
    await act(async () => root.unmount())
    unmounted = true
    await tick()
    expect(listeners.size).toBe(0)
    expect(api.agentCancel.mock.calls.map((call) => call[0])).toEqual(
      expect.arrayContaining(runs.map((run) => run.runId))
    )
    expect(api.agentCancel).toHaveBeenCalledTimes(2)
  })
  it('does not resume a deleted conversation from its retained owner', async () => {
    await render({ kind: 'resume', sessionId: 'B' })
    await click('[aria-label="Chat history"]')
    await click('[aria-label="Delete Saved B"]')
    await render({ kind: 'resume', sessionId: 'B', nonce: 'deleted' })
    expect(container.textContent).not.toContain('Answer for B')
    expect(container.textContent).toContain('This conversation no longer exists.')
  })
  it('repeats unnonced launcher commands through App while the real chat stays mounted', async () => {
    // Replace unrelated launcher chrome; App's navigation callback and the
    // complete AgentChatView + AI SDK remain real.
    vi.doMock('../CommandBar', () => ({
      default: ({ onOpenAiChat }: { onOpenAiChat: (boot: AiChatBoot) => void }) =>
        createElement(
          'div',
          null,
          createElement(
            'button',
            { onClick: () => onOpenAiChat({ kind: 'newChat' }) },
            'Launch new chat'
          ),
          createElement(
            'button',
            { onClick: () => onOpenAiChat({ kind: 'screen' }) },
            'Launch screen'
          ),
          createElement(
            'button',
            { onClick: () => onOpenAiChat({ kind: 'resume', sessionId: 'B' }) },
            'Launch B'
          )
        ),
    }))
    vi.doMock('../LauncherAppearance', () => ({ LauncherResizeHandles: () => null }))
    vi.doMock('../LauncherFrame', () => ({
      LauncherFrame: ({ children }: { children: ReactNode }) =>
        createElement('div', null, children),
    }))
    vi.doMock('../useLauncherScaleShortcuts', () => ({ useLauncherScaleShortcuts: noop }))
    const { default: App } = await import('../App')
    api.getLlmConfig.mockResolvedValue({
      provider: 'ollama',
      model: 'llama3.2',
      hasCompletedOnboarding: true,
    })
    Object.assign(api, {
      onWindowShown: () => noop,
      onAppSurfaceOpen: () => noop,
      onRunExtensionCommandFromHotkey: () => noop,
      onTerminalSessionsAction: () => noop,
      getUpdateStatus: async () => ({ kind: 'idle' }),
      onUpdateStatus: () => noop,
      listBackgroundTasks: async () => [],
      onKnowledgeStatus: () => noop,
    })
    const launch = async (label: string) => {
      const button = [...container.querySelectorAll('button')].find(
        (button) => button.textContent === label
      )!
      expect(button).toBeTruthy()
      await act(async () => button.click())
      await tick()
    }
    await act(async () =>
      root.render(createElement(Suspense, { fallback: null }, createElement(App)))
    )
    await tick()
    await launch('Launch new chat')
    await send('First launcher task')
    await emit({ type: 'answer', runId: runs[0]!.runId!, text: 'First launcher answer' })
    await emit({ type: 'done', runId: runs[0]!.runId! })
    await click('[aria-label="Back"]')
    await launch('Launch new chat')
    expect(container.querySelector('h1')?.textContent).toBe('New chat')
    expect(container.textContent).not.toContain('First launcher answer')
    await click('[aria-label="Back"]')
    await launch('Launch screen')
    await click('[aria-label="Back"]')
    await launch('Launch screen')
    expect(api.captureActiveScreen).toHaveBeenCalledTimes(2)
    await click('[aria-label="Back"]')
    await launch('Launch B')
    await click('[aria-label="New chat"]')
    await click('[aria-label="Back"]')
    await launch('Launch B')
    expect(container.textContent).toContain('Answer for B')
    vi.doUnmock('../CommandBar')
    vi.doUnmock('../LauncherAppearance')
    vi.doUnmock('../LauncherFrame')
    vi.doUnmock('../useLauncherScaleShortcuts')
  })
})
