import { extensionRequest as request } from './api'
import { useCallback, useEffect, useState } from 'react'
import { Button, TextField, TextArea, SelectField, Message } from '../ui/primitives'
import {
  type AiConnection,
  type AiExtension,
  type AiExtensionsState,
} from '../../shared/aiExtensions'
const blank = (): AiConnection => ({
  id: crypto.randomUUID(),
  name: '',
  transport: 'stdio',
  command: '',
  args: [],
  toolPolicy: 'ask',
})
export function AiExtensionsSection(): JSX.Element {
  const [state, setState] = useState<AiExtensionsState>({
    extensions: [],
    globalInstructions: '',
    useAgentsMd: true,
  })
  const [panel, setPanel] = useState<'menu' | 'connection' | 'paste' | null>(null)
  const [draft, setDraft] = useState<AiConnection>(blank)
  const [token, setToken] = useState('')
  const [paste, setPaste] = useState('')
  const [preview, setPreview] = useState<{
    previewId: string
    manifest: AiExtension
    secretCount: number
  } | null>(null)
  const [details, setDetails] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState('')
  const [legacy, setLegacy] = useState<string[]>([])
  const load = useCallback(async () => setState(await request<AiExtensionsState>('list')), [])
  useEffect(() => {
    void load().catch((e) => setError(String(e)))
    void window.tezbar
      .listPiExtensions()
      .then((items) => setLegacy(items.filter((i) => !i.enabled).map((i) => i.name)))
      .catch(() => {})
  }, [load])
  const run = async (name: string, task: () => Promise<unknown>): Promise<void> => {
    setBusy(name)
    setError('')
    try {
      const result = (await task()) as { ok?: boolean; error?: string }
      if (result?.ok === false)
        throw new Error(result.error || "Couldn't connect. Check the command or web address.")
      await load()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(null)
    }
  }
  const test = (id: string): Promise<void> => run(id, () => request('test', id))
  const patch = (p: Partial<AiConnection>): void => setDraft((d) => ({ ...d, ...p }))
  const addConnection = async (): Promise<void> => {
    const record: AiExtension = {
      manifestVersion: 1,
      id: `user.${crypto.randomUUID()}`,
      name: draft.name,
      connections: [
        {
          ...draft,
          ...(token ? { auth: { type: 'token' as const, secret: 'access-token' } } : {}),
        },
      ],
    }
    await run('new', async () => {
      await request('add', record)
      if (token) await request('set-secret', record.id, 'access-token', token, draft.id)
      setToken('')
      setPanel(null)
      setDetails(record.id)
      return request('test', record.id)
    })
  }
  return (
    <section aria-label="AI Extensions" className="space-y-3 py-3 text-[12px] text-ink-2">
      <div className="flex items-center justify-between">
        <h3 className="text-[13px] font-semibold text-ink-1">AI Extensions</h3>
        <Button variant="quiet" onClick={() => setPanel(panel ? null : 'menu')}>
          Add ▾
        </Button>
      </div>
      <p className="text-ink-3">
        Add tools, prompts and instructions to AI mode. Extensions run on this computer with your
        permissions. Tezbar asks before using their tools.
      </p>
      {panel === 'menu' && (
        <div className="flex gap-2">
          <Button
            onClick={() => {
              setDraft(blank())
              setPanel('connection')
            }}
          >
            Add connection
          </Button>
          <Button
            onClick={() => {
              setPreview(null)
              setPanel('paste')
            }}
          >
            Paste configuration…
          </Button>
          <label className="btn btn-ghost cursor-pointer">
            Import file…
            <input
              className="sr-only"
              type="file"
              accept=".json"
              onChange={(e) => {
                const file = e.target.files?.[0]
                if (file) {
                  setPanel('paste')
                  void file
                    .text()
                    .then(setPaste)
                    .catch((e) => setError(String(e)))
                }
              }}
            />
          </label>
        </div>
      )}
      {panel === 'connection' && (
        <div className="space-y-3 border-y border-ink-4/20 py-3">
          <label className="block">
            Name
            <TextField value={draft.name} onChange={(e) => patch({ name: e.target.value })} />
          </label>
          <fieldset className="flex gap-4">
            <legend>Type</legend>
            {(['stdio', 'http'] as const).map((type) => (
              <label key={type}>
                <input
                  type="radio"
                  checked={draft.transport === type}
                  onChange={() => patch({ transport: type })}
                />{' '}
                {type === 'stdio' ? 'Program on this computer' : 'Web address'}
              </label>
            ))}
          </fieldset>
          <p className="text-[11px] text-ink-3">
            Connect any program or service that supports MCP.
          </p>
          {draft.transport === 'stdio' ? (
            <>
              <label className="block">
                Command
                <TextField
                  className="font-mono"
                  value={draft.command || ''}
                  onChange={(e) => patch({ command: e.target.value })}
                />
              </label>
              <label className="block">
                Arguments, one per line
                <TextArea
                  value={draft.args?.join('\n') || ''}
                  onChange={(e) => patch({ args: e.target.value.split('\n').filter(Boolean) })}
                />
              </label>
              <label className="block">
                Working folder
                <TextField
                  value={draft.cwd || ''}
                  onChange={(e) => patch({ cwd: e.target.value })}
                />
              </label>
            </>
          ) : (
            <>
              <label className="block">
                URL
                <TextField
                  value={draft.url || ''}
                  onChange={(e) => patch({ url: e.target.value })}
                />
              </label>
              <label className="block">
                Access token
                <TextField
                  type="password"
                  autoComplete="off"
                  value={token}
                  onChange={(e) => setToken(e.target.value)}
                />
              </label>
            </>
          )}
          <label className="block">
            Ask before using tools
            <SelectField
              value={draft.toolPolicy}
              onChange={(e) => patch({ toolPolicy: e.target.value as AiConnection['toolPolicy'] })}
            >
              <option value="ask">Ask every time</option>
              <option value="allowReadOnly">Allow read-only tools (the program's own claim)</option>
              <option value="allow">Allow all</option>
            </SelectField>
          </label>
          {draft.toolPolicy === 'allow' && (
            <label className="block">
              Type {draft.name} to allow all of its tools without asking.
              <TextField
                value={draft.allowAllConfirmation || ''}
                onChange={(e) => patch({ allowAllConfirmation: e.target.value })}
              />
            </label>
          )}
          <p>Tezbar will run this with your permissions:</p>
          <pre className="whitespace-pre-wrap break-all text-[11px]">
            {draft.transport === 'stdio'
              ? [draft.command, ...(draft.args || [])].join(' ')
              : draft.url}
          </pre>
          <div className="flex gap-2">
            <Button
              disabled={
                !!busy ||
                !draft.name.trim() ||
                (draft.toolPolicy === 'allow' && draft.allowAllConfirmation !== draft.name)
              }
              onClick={() => void addConnection()}
            >
              Test connection
            </Button>
            <Button
              variant="quiet"
              onClick={() => {
                setPanel(null)
                setToken('')
              }}
            >
              Cancel
            </Button>
          </div>
        </div>
      )}
      {panel === 'paste' && (
        <div className="space-y-2">
          <TextArea
            aria-label="Configuration"
            rows={7}
            value={paste}
            onChange={(e) => {
              setPaste(e.target.value)
              setPreview(null)
            }}
          />
          <Button
            onClick={() =>
              void run('preview', async () => {
                setPreview(await request('parse-import', paste))
              })
            }
          >
            Preview
          </Button>
          {preview && (
            <div>
              <strong>{preview.manifest.name}</strong>
              {preview.manifest.connections.map((c) => (
                <pre className="whitespace-pre-wrap break-all" key={c.id}>
                  {c.transport === 'stdio' ? [c.command, ...(c.args || [])].join(' ') : c.url}
                </pre>
              ))}
              {preview.secretCount > 0 && (
                <p>{preview.secretCount} keys will be stored as secrets.</p>
              )}
              <Button
                onClick={() =>
                  void run('import', async () => {
                    await request('add', { previewId: preview.previewId })
                    setPanel(null)
                    setPaste('')
                    setPreview(null)
                  })
                }
              >
                Add
              </Button>
            </div>
          )}
        </div>
      )}
      {error && <Message tone="error">{error}</Message>}
      {!state.extensions.length && <p className="text-ink-3">No extensions yet.</p>}
      {state.extensions.map((e) => {
        const tested = e.connections.every((c) => c.tested)
        const changed = e.connections.some((c) => c.needsReview)
        const tools = e.connections.flatMap((c) => c.catalog?.tools || [])
        const prompts = e.connections.flatMap((c) => c.catalog?.prompts || [])
        return (
          <div key={e.id} className="border-t border-ink-4/20 py-2">
            <div className="flex items-center gap-3">
              <input
                aria-label={`Enable ${e.name}`}
                type="checkbox"
                checked={!!e.enabled}
                disabled={!tested || changed || busy === e.id}
                onChange={(ev) =>
                  void run(e.id, () => request('set-enabled', e.id, ev.target.checked))
                }
              />
              <span className="flex-1 text-ink-1">{e.name}</span>
              <span className="text-ink-3">
                {busy === e.id
                  ? 'Connecting…'
                  : changed
                    ? 'Tools changed. Review'
                    : tested
                      ? `${tools.length} tools · ${prompts.length} prompts`
                      : 'Not tested yet'}
              </span>
              <Button variant="quiet" onClick={() => setDetails(details === e.id ? null : e.id)}>
                Details ›
              </Button>
              {!tested && <Button onClick={() => void test(e.id)}>Test</Button>}
            </div>
            {details === e.id && (
              <div className="space-y-2 py-3 pl-6">
                {e.connections.map((c) => (
                  <div key={c.id}>
                    <pre className="whitespace-pre-wrap break-all">
                      {c.transport === 'stdio' ? [c.command, ...(c.args || [])].join(' ') : c.url}
                    </pre>
                    {c.catalog?.warnings?.map((w, i) => (
                      <Message tone="error" key={i}>
                        {w}
                      </Message>
                    ))}
                    {c.catalog?.tools?.map((t) => (
                      <label className="flex gap-2 py-1" key={t.name}>
                        <input
                          type="checkbox"
                          defaultChecked
                          onChange={(ev) =>
                            void run(e.id, () =>
                              request('set-tool', e.id, t.name, ev.target.checked)
                            )
                          }
                        />
                        <span>
                          {t.name} <span className="text-ink-3">{t.description}</span>
                        </span>
                      </label>
                    ))}
                  </div>
                ))}
                <div className="flex gap-2">
                  {changed && (
                    <Button onClick={() => void run(e.id, () => request('accept-review', e.id))}>
                      Accept changes
                    </Button>
                  )}
                  <Button onClick={() => void test(e.id)}>Test connection</Button>
                  {tested && !e.enabled && (
                    <Button
                      onClick={() => void run(e.id, () => request('set-enabled', e.id, true))}
                    >
                      Enable
                    </Button>
                  )}
                  <Button
                    variant="danger"
                    onClick={() => void run(e.id, () => request('remove', e.id))}
                  >
                    Remove
                  </Button>
                </div>
              </div>
            )}
          </div>
        )
      })}
      <div className="space-y-2 border-t border-ink-4/20 pt-3">
        <h4 className="text-[13px] text-ink-1">Instructions</h4>
        <label className="block">
          Custom instructions
          <TextArea
            rows={4}
            value={state.globalInstructions}
            onChange={(e) => setState((s) => ({ ...s, globalInstructions: e.target.value }))}
            onBlur={() =>
              void run('instructions', () =>
                request('set-instructions', state.globalInstructions, state.useAgentsMd)
              )
            }
          />
        </label>
        <label>
          <input
            type="checkbox"
            checked={state.useAgentsMd}
            onChange={(e) => {
              const checked = e.target.checked
              setState((s) => ({ ...s, useAgentsMd: checked }))
              void run('instructions', () =>
                request('set-instructions', state.globalInstructions, checked)
              )
            }}
          />{' '}
          Use AGENTS.md from the working folder
        </label>
      </div>
      {!!legacy.length && (
        <details className="text-ink-3">
          <summary>Pi extensions ({legacy.length}, not loaded)</summary>
          <p>
            Tezbar no longer runs Pi extensions. Add an equivalent connection to use its features.
          </p>
          {legacy.map((name) => (
            <p key={name}>{name}</p>
          ))}
        </details>
      )}
    </section>
  )
}
