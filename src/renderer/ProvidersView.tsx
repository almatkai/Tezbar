import { useCallback, useEffect, useRef, useState } from 'react'
import type { LlmConfigRecord, ProviderId } from '../shared/llmConfig'
import type { ProviderConnectionStatuses } from '../preload/api'
import {
  Button,
  FieldLabel,
  Hint,
  HintBar,
  Kbd,
  Message,
  SelectField,
  StatusDots,
  TextArea,
  TextField,
  ViewHeader,
} from './ui/primitives'
import { GlideList } from './ui/GlideList'

const ROWS: { id: ProviderId; title: string; subtitle: string }[] = [
  { id: 'openai', title: 'OpenAI', subtitle: 'Chat Completions API or compatible' },
  { id: 'deepseek', title: 'DeepSeek', subtitle: 'DeepSeek-V4, V3, and R1 via the official API' },
  { id: 'openai-compatible', title: 'OpenAI Compatible', subtitle: 'Any endpoint that speaks OpenAI Chat API' },
  { id: 'gemini', title: 'Gemini', subtitle: 'Google Gemini via OpenAI-compatible endpoint' },
  { id: 'anthropic', title: 'Anthropic', subtitle: 'Claude via the official API' },
  { id: 'ollama', title: 'Ollama', subtitle: 'Local models running on this machine' },
  { id: 'copilot', title: 'GitHub Copilot', subtitle: 'PAT, OAuth, or device flow sign-in' },
  { id: 'opencode', title: 'OpenCode', subtitle: 'opencode.ai via CLI' },
]

const DEFAULT_MODEL: Record<ProviderId, string> = {
  openai: 'gpt-4o-mini',
  deepseek: 'deepseek-v4-flash',
  'openai-compatible': 'gpt-4o-mini',
  gemini: 'gemini-2.0-flash',
  anthropic: 'claude-3-5-haiku-20241022',
  ollama: 'llama3.2',
  copilot: 'gpt-4o',
  opencode: 'opencode/big-pickle',
}

type Panel = 'list' | { type: 'detail'; id: ProviderId }

type DetailProps = {
  id: ProviderId
  cfg: LlmConfigRecord
  connected: boolean
  onBack: () => void
  onReload: () => Promise<void>
}

function ProviderDetail({ id, cfg, connected, onBack, onReload }: DetailProps): JSX.Element {
  const [apiKey, setApiKey] = useState('')
  const [baseURL, setBaseURL] = useState('')
  const [model, setModel] = useState('')
  const [copilotToken, setCopilotToken] = useState('')
  const [oauthClientId, setOauthClientId] = useState('')
  const [msg, setMsg] = useState<{ tone: 'success' | 'error' | 'neutral'; text: string } | null>(null)
  const [deviceBusy, setDeviceBusy] = useState(false)
  const [modelOptions, setModelOptions] = useState<string[]>([])
  const [modelsLoading, setModelsLoading] = useState(false)

  const loadModels = useCallback(async () => {
    setModelsLoading(true)
    try {
      const list = await window.raymes.listLlmModels(id)
      setModelOptions(list)
    } catch {
      setModelOptions([])
    } finally {
      setModelsLoading(false)
    }
  }, [id])

  useEffect(() => {
    setApiKey(cfg.apiKey ?? '')
    setBaseURL(cfg.baseURL ?? '')
    setModel(cfg.model ?? DEFAULT_MODEL[id])
    setCopilotToken(cfg.copilotGithubToken ?? '')
    setOauthClientId(cfg.githubOAuthClientId ?? '')
    setMsg(null)
  }, [id, cfg])

  useEffect(() => {
    void loadModels()
  }, [loadModels])

  async function saveCredentials(): Promise<void> {
    setMsg(null)
    const patch: LlmConfigRecord = {}
    if (id === 'openai' || id === 'anthropic') {
      patch.apiKey = apiKey
      if (baseURL.trim()) patch.baseURL = baseURL.trim()
      patch.model = model.trim() || DEFAULT_MODEL[id]
    }
    if (id === 'deepseek') {
      patch.apiKey = apiKey
      patch.baseURL = baseURL.trim() || 'https://api.deepseek.com'
      patch.model = model.trim() || DEFAULT_MODEL[id]
    }
    if (id === 'openai-compatible') {
      patch.apiKey = apiKey
      patch.openaiCompatibleBaseURL = baseURL.trim() || 'https://api.openai.com/v1'
      patch.model = model.trim() || DEFAULT_MODEL[id]
    }
    if (id === 'gemini') {
      patch.geminiApiKey = apiKey
      patch.baseURL = baseURL.trim() || 'https://generativelanguage.googleapis.com/v1beta/openai'
      patch.model = model.trim() || DEFAULT_MODEL[id]
    }
    if (id === 'ollama') {
      patch.baseURL = baseURL.trim() || 'http://localhost:11434'
      patch.model = model.trim() || DEFAULT_MODEL[id]
    }
    if (id === 'copilot') {
      patch.copilotGithubToken = copilotToken
      patch.githubOAuthClientId = oauthClientId
      patch.model = model.trim() || DEFAULT_MODEL[id]
    }
    if (id === 'opencode') {
      patch.model = model.trim() || DEFAULT_MODEL[id]
    }
    await window.raymes.setLlmConfig(patch)
    await onReload()
    void loadModels()
    setMsg({ tone: 'success', text: 'Saved' })
  }

  async function activate(): Promise<void> {
    await window.raymes.setLlmConfig({ provider: id })
    await onReload()
    setMsg({ tone: 'success', text: 'Active provider updated' })
  }

  async function startDeviceSignIn(): Promise<void> {
    setMsg(null)
    const cid = oauthClientId.trim()
    if (!cid) {
      setMsg({ tone: 'error', text: 'Add a GitHub OAuth App Client ID first.' })
      return
    }
    setDeviceBusy(true)
    try {
      await window.raymes.setLlmConfig({ githubOAuthClientId: cid })
      const start = await window.raymes.githubDeviceStart(cid)
      setMsg({ tone: 'neutral', text: `Open GitHub and enter code ${start.user_code}` })
      await window.raymes.openExternalUrl(start.verification_uri)
      const wait = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms))
      await wait(Math.max(1000, start.interval * 1000))
      let finished = false
      let polls = 0
      while (!finished && polls < 120) {
        polls += 1
        const r = await window.raymes.githubDevicePoll()
        if (r.status === 'success') {
          setMsg({ tone: 'success', text: 'GitHub sign-in complete' })
          await onReload()
          void loadModels()
          finished = true
          break
        }
        if (r.status === 'error') {
          setMsg({ tone: 'error', text: r.error })
          finished = true
          break
        }
        const extra = r.status === 'slow_down' ? 5000 : 0
        await wait(extra + Math.max(1000, start.interval * 1000))
      }
      if (!finished) {
        setMsg({ tone: 'error', text: 'Device sign-in timed out' })
      }
    } catch (e) {
      setMsg({ tone: 'error', text: e instanceof Error ? e.message : String(e) })
    } finally {
      setDeviceBusy(false)
    }
  }

  useEffect(() => {
    return () => {
      void window.raymes.githubDeviceCancel()
    }
  }, [])

  const title = ROWS.find((r) => r.id === id)?.title ?? id

  return (
    <div className="flex h-full min-h-0 w-full flex-col gap-2">
      <div className="glass-card shrink-0 px-4 py-3 animate-raymes-scale-in">
        <ViewHeader
          title={title}
          onBack={onBack}
          backLabel="Back to providers"
          trailing={
            <>
              {connected ? <StatusDots /> : null}
              <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-ink-4">
                {connected ? 'Active' : 'Offline'}
              </span>
            </>
          }
        />
      </div>
      <div className="glass-card min-h-0 flex-1 overflow-y-auto px-4 py-3 pr-[calc(0.5rem+2px)] animate-raymes-scale-in">
        {id === 'openai' || id === 'anthropic' || id === 'openai-compatible' || id === 'gemini' || id === 'deepseek' ? (
          <div className="space-y-3">
            <div>
              <FieldLabel>API key</FieldLabel>
              <TextField
                type="password"
                autoComplete="off"
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder={
                  id === 'openai'
                    ? 'sk-…'
                    : id === 'anthropic'
                      ? 'sk-ant-…'
                      : id === 'gemini'
                        ? 'AIza…'
                        : id === 'deepseek'
                          ? 'sk-…'
                          : 'provider key'
                }
              />
            </div>
            <div>
              <FieldLabel>Base URL <span className="normal-case tracking-normal text-ink-4">(optional)</span></FieldLabel>
              <TextField
                value={baseURL}
                onChange={(e) => setBaseURL(e.target.value)}
                placeholder={
                  id === 'openai'
                    ? 'https://api.openai.com/v1'
                    : id === 'anthropic'
                      ? 'https://api.anthropic.com'
                      : id === 'gemini'
                        ? 'https://generativelanguage.googleapis.com/v1beta/openai'
                        : id === 'deepseek'
                          ? 'https://api.deepseek.com'
                          : 'https://api.openai.com/v1'
                }
              />
            </div>
            <ModelPicker
              id={id}
              model={model}
              setModel={setModel}
              modelOptions={modelOptions}
              modelsLoading={modelsLoading}
              onRefresh={() => void loadModels()}
            />
          </div>
        ) : null}

        {id === 'ollama' ? (
          <div className="space-y-3">
            <div>
              <FieldLabel>Base URL</FieldLabel>
              <TextField
                value={baseURL}
                onChange={(e) => setBaseURL(e.target.value)}
                placeholder="http://localhost:11434"
              />
            </div>
            <ModelPicker
              id={id}
              model={model}
              setModel={setModel}
              modelOptions={modelOptions}
              modelsLoading={modelsLoading}
              onRefresh={() => void loadModels()}
            />
          </div>
        ) : null}

        {id === 'opencode' ? (
          <div className="space-y-3">
            <p className="text-[12px] leading-relaxed text-ink-3">
              Uses the local <code className="font-mono text-ink-2">opencode</code> CLI to access opencode.ai models.
              Make sure the CLI is installed and configured.
            </p>
            <ModelPicker
              id={id}
              model={model}
              setModel={setModel}
              modelOptions={modelOptions}
              modelsLoading={modelsLoading}
              onRefresh={() => void loadModels()}
            />
          </div>
        ) : null}

        {id === 'copilot' ? (
          <div className="space-y-3">
            <p className="text-[12px] leading-relaxed text-ink-3">
              Use a GitHub token with Copilot Chat access, or sign in via device flow using a{' '}
              <button
                type="button"
                className="text-accent-strong underline-offset-2 transition hover:underline"
                onClick={() =>
                  void window.raymes.openExternalUrl('https://github.com/settings/developers#oauth-apps')
                }
              >
                public OAuth App
              </button>
              .
            </p>
            <div>
              <FieldLabel>GitHub token</FieldLabel>
              <TextArea
                value={copilotToken}
                onChange={(e) => setCopilotToken(e.target.value)}
                placeholder="ghp_… or OAuth access token"
                spellCheck={false}
              />
            </div>
            <div>
              <FieldLabel>OAuth Client ID</FieldLabel>
              <TextField
                className="font-mono"
                value={oauthClientId}
                onChange={(e) => setOauthClientId(e.target.value)}
                placeholder="Iv1.…"
              />
            </div>
            <ModelPicker
              id={id}
              model={model}
              setModel={setModel}
              modelOptions={modelOptions}
              modelsLoading={modelsLoading}
              onRefresh={() => void loadModels()}
            />
            <Button
              disabled={deviceBusy}
              onClick={() => void startDeviceSignIn()}
              fullWidth
            >
              {deviceBusy ? 'Waiting for GitHub…' : 'Sign in with GitHub'}
            </Button>
          </div>
        ) : null}

        {msg ? (
          <div className="mt-3">
            <Message tone={msg.tone === 'success' ? 'success' : msg.tone === 'error' ? 'error' : 'neutral'}>
              {msg.text}
            </Message>
          </div>
        ) : null}
      </div>

      <div className="glass-card shrink-0 px-4 py-3 animate-raymes-scale-in">
        <div className="flex gap-2">
          <Button variant="primary" fullWidth onClick={() => void saveCredentials()}>
            Save
          </Button>
          <Button fullWidth onClick={() => void activate()}>
            Use this provider
          </Button>
        </div>
      </div>
    </div>
  )
}

function ModelPicker({
  id,
  model,
  setModel,
  modelOptions,
  modelsLoading,
  onRefresh,
}: {
  id: ProviderId
  model: string
  setModel: (v: string) => void
  modelOptions: string[]
  modelsLoading: boolean
  onRefresh: () => void
}): JSX.Element {
  if (id === 'deepseek') {
    const options = [
      { id: 'deepseek-v4-flash', label: 'DeepSeek-V4 Flash' },
      { id: 'deepseek-v4-pro', label: 'DeepSeek-V4 Pro' },
      { id: 'deepseek-chat', label: 'DeepSeek Chat (deprecated)' },
      { id: 'deepseek-reasoner', label: 'DeepSeek Reasoner (deprecated)' },
    ]

    return (
      <div>
        <FieldLabel>Model</FieldLabel>
        <SelectField value={model} onChange={(e) => setModel(e.target.value)}>
          {options.map((opt) => (
            <option key={opt.id} value={opt.id}>
              {opt.label}
            </option>
          ))}
          {modelOptions
            .filter((m) => !options.some((o) => o.id === m))
            .map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
        </SelectField>
      </div>
    )
  }

  return (
    <div>
      <div className="flex items-baseline justify-between">
        <FieldLabel>Model</FieldLabel>
        <button
          type="button"
          disabled={modelsLoading}
          onClick={onRefresh}
          className="mb-1.5 text-[10.5px] font-medium text-accent-strong transition hover:text-accent disabled:opacity-40"
        >
          {modelsLoading ? 'Loading…' : 'Refresh'}
        </button>
      </div>
      <TextField
        list={`raymes-models-${id}`}
        value={model}
        onChange={(e) => setModel(e.target.value)}
        placeholder={modelsLoading ? 'Loading models…' : 'Pick or type a model id'}
        spellCheck={false}
      />
      <datalist id={`raymes-models-${id}`}>
        {modelOptions.map((m) => (
          <option key={m} value={m} />
        ))}
      </datalist>
    </div>
  )
}

export default function ProvidersView({ onBack }: { onBack: () => void }): JSX.Element {
  const [panel, setPanel] = useState<Panel>('list')
  const [selected, setSelected] = useState(0)
  const [cfg, setCfg] = useState<LlmConfigRecord>({})
  const [statuses, setStatuses] = useState<ProviderConnectionStatuses>({
    openai: false,
    deepseek: false,
    'openai-compatible': false,
    gemini: false,
    anthropic: false,
    ollama: false,
    copilot: false,
    opencode: false,
  })
  const rootRef = useRef<HTMLDivElement>(null)

  const reload = useCallback(async () => {
    const c = (await window.raymes.getLlmConfig()) as LlmConfigRecord
    setCfg(c)
    const st = await window.raymes.getLlmProviderStatuses()
    setStatuses(st)
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  useEffect(() => {
    rootRef.current?.focus()
  }, [panel])

  useEffect(() => {
    const onEsc = (e: KeyboardEvent): void => {
      if (e.key !== 'Escape') return
      e.preventDefault()
      e.stopPropagation()
      if (panel === 'list') {
        onBack()
      } else {
        setPanel('list')
      }
    }
    window.addEventListener('keydown', onEsc, true)
    return () => window.removeEventListener('keydown', onEsc, true)
  }, [panel, onBack])

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>): void => {
    if (panel === 'list') {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        setSelected((i) => Math.min(i + 1, ROWS.length - 1))
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        setSelected((i) => Math.max(i - 1, 0))
      }
      if (e.key === 'Enter') {
        e.preventDefault()
        setPanel({ type: 'detail', id: ROWS[selected]?.id ?? 'openai' })
      }
    }
  }

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      role="application"
      aria-label="Providers"
      onKeyDown={onKeyDown}
      className="flex h-full min-h-0 w-full flex-col gap-2 outline-none animate-raymes-scale-in"
    >
      {panel === 'list' ? (
        <>
          <div className="glass-card shrink-0 px-4 py-3 animate-raymes-scale-in">
            <ViewHeader
              title="Providers"
              onBack={onBack}
              trailing={
                <span className="rounded-raymes-chip border border-white/10 bg-white/[0.04] px-2 py-0.5 font-mono text-[10px] uppercase tracking-[0.14em] text-ink-3">
                  {cfg.provider ?? 'ollama'}
                </span>
              }
            />
          </div>
          <div className="glass-card min-h-0 flex-1 flex flex-col overflow-hidden px-2 py-2 animate-raymes-scale-in">
            <div className="min-h-0 flex-1 overflow-y-auto pr-0.5">
              <GlideList
                selectedIndex={selected}
                itemCount={ROWS.length}
                className="flex flex-col gap-0.5 py-1"
              >
                {ROWS.map((row, idx) => (
                  <li key={row.id} className="relative z-[1]">
                    <button
                      type="button"
                      onClick={() => setSelected(idx)}
                      onDoubleClick={() => setPanel({ type: 'detail', id: row.id })}
                      onMouseEnter={() => setSelected(idx)}
                      className="flex w-full items-center justify-between gap-3 rounded-raymes-row px-3 py-2.5 text-left transition"
                    >
                      <span className="min-w-0 flex-1">
                        <span className="block text-[13.5px] font-medium leading-tight text-ink-1">
                          {row.title}
                        </span>
                        <span className="mt-1 block truncate text-[11.5px] leading-snug text-ink-3">
                          {row.subtitle}
                        </span>
                      </span>
                      <span className="flex shrink-0 items-center gap-2">
                        {cfg.provider === row.id ? (
                          <span className="rounded-raymes-chip border border-accent/30 bg-accent/10 px-1.5 py-0.5 text-[9.5px] font-semibold uppercase tracking-[0.14em] text-accent-strong">
                            Active
                          </span>
                        ) : null}
                        {statuses[row.id] ? <StatusDots /> : null}
                      </span>
                    </button>
                  </li>
                ))}
              </GlideList>
            </div>
          </div>
          <div className="glass-card shrink-0 px-4 py-2 animate-raymes-scale-in">
            <HintBar>
              <Hint label="Navigate" keys={<><Kbd>↑</Kbd><Kbd>↓</Kbd></>} />
              <Hint label="Open" keys={<Kbd>↵</Kbd>} />
              <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
            </HintBar>
          </div>
        </>
      ) : (
        <ProviderDetail
          id={panel.id}
          cfg={cfg}
          connected={statuses[panel.id] === true}
          onBack={() => setPanel('list')}
          onReload={reload}
        />
      )}
    </div>
  )
}
