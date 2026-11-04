import { AiContributionsEditor } from './AiContributionsEditor'
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
  const [panel, setPanel] = useState<'menu' | 'connection' | 'paste' | 'project' | null>(null)
  const [draft, setDraft] = useState<AiConnection>(blank)
  const [token, setToken] = useState('')
  const [editing, setEditing] = useState<AiExtension | null>(null)
  const [variables, setVariables] = useState<
    { name: string; value: string; secret: boolean; saved?: boolean }[]
  >([])
  const [signIn, setSignIn] = useState<{ extension: string; connection: string } | null>(null)
  const [information, setInformation] = useState('')

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
  const [removing, setRemoving] = useState<string | null>(null)
  const [logs, setLogs] = useState<Record<string, string[] | undefined>>({})
  const load = useCallback(
    async () =>
      setState(
        await request<AiExtensionsState>(
          'list',
          sessionStorage.getItem('tezbar-ai-extension-review') || undefined
        )
      ),
    []
  )
  useEffect(() => {
    void load().catch((e) => setError(String(e)))
    void window.tezbar
      .listPiExtensions()
      .then((items) => setLegacy(items.filter((i) => !i.enabled).map((i) => i.name)))
      .catch(() => {})
  }, [load])
  useEffect(() => {
    if (!signIn) return
    let cancelled = false
    const timer = setInterval(() => {
      void request<{ status: string; error?: string }>(
        'oauth-status',
        signIn.extension,
        signIn.connection
      )
        .then((status) => {
          if (cancelled || status.status === 'pending') return
          setSignIn(null)
          if (status.status === 'success') {
            setInformation('Signed in. Test the connection to continue.')
            void load()
          } else if (status.error) setError(status.error)
        })
        .catch((e) => {
          if (!cancelled) {
            setError(String(e))
            setSignIn(null)
          }
        })
    }, 1000)
    return () => {
      cancelled = true
      clearInterval(timer)
    }
  }, [signIn, load])
  useEffect(
    () =>
      window.tezbar.onAiExtensionsChanged?.(() => {
        void load().catch((e) => setError(String(e)))
      }),
    [load]
  )
  const edit = (extension: AiExtension, connection: AiConnection): void => {
    setEditing(extension)
    setDraft({ ...connection })
    setVariables(
      Object.entries(connection.env || {}).map(([name, value]) => ({
        name,
        value: typeof value === 'string' ? value : '',
        secret: typeof value !== 'string',
        saved: typeof value !== 'string',
      }))
    )
    setToken('')
    setPanel('connection')
  }
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
    const connection = {
      ...draft,
      env: Object.fromEntries(
        variables
          .filter((v) => v.name.trim())
          .map((v) => [
            v.name.trim(),
            v.secret ? { secret: v.name.trim(), label: v.name.trim() } : v.value,
          ])
      ),
    }
    if (connection.auth?.type === 'token')
      connection.auth = { ...connection.auth, secret: 'access-token' }
    const record: AiExtension = editing
      ? {
          ...editing,
          name: editing.connections.length === 1 ? connection.name : editing.name,
          connections: editing.connections.map((c) => (c.id === connection.id ? connection : c)),
        }
      : {
          manifestVersion: 1,
          id: `user.${crypto.randomUUID()}`,
          name: draft.name,
          connections: [connection],
        }
    await run('new', async () => {
      if (editing)
        await request('update', record.id, { name: record.name, connections: record.connections })
      else await request('add', record)
      if (token) await request('set-secret', record.id, 'access-token', token, draft.id)
      for (const variable of variables)
        if (variable.secret && variable.value)
          await request('set-secret', record.id, variable.name.trim(), variable.value, draft.id)
      setDetails(record.id)
      setEditing(record)
      const result = await request<{ ok: boolean; error?: string }>('test', record.id)
      if (!result.ok)
        throw new Error(result.error || "Couldn't connect. Check the command or web address.")
      setToken('')
      setVariables([])
      setEditing(null)
      setPanel(null)
      return result
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
              setEditing(null)
              setVariables([])
              setToken('')
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
                <Button
                  variant="quiet"
                  onClick={() =>
                    void window.tezbar
                      .chooseKnowledgeFolder()
                      .then((folder) => {
                        if (folder) patch({ cwd: folder })
                      })
                      .catch((e) => setError(String(e)))
                  }
                >
                  Browse
                </Button>
              </label>
              <fieldset className="space-y-2">
                <legend>Variables</legend>
                {variables.map((v, i) => (
                  <div key={i} className="flex items-center gap-2">
                    <TextField
                      aria-label={`Variable ${i + 1} name`}
                      value={v.name}
                      onChange={(e) =>
                        setVariables((rows) =>
                          rows.map((row, n) => (n === i ? { ...row, name: e.target.value } : row))
                        )
                      }
                    />
                    <TextField
                      aria-label={`Variable ${i + 1} value`}
                      type={v.secret ? 'password' : 'text'}
                      autoComplete="off"
                      placeholder={v.saved ? 'Saved' : ''}
                      value={v.value}
                      onChange={(e) =>
                        setVariables((rows) =>
                          rows.map((row, n) => (n === i ? { ...row, value: e.target.value } : row))
                        )
                      }
                    />
                    <label>
                      <input
                        type="checkbox"
                        checked={v.secret}
                        onChange={(e) =>
                          setVariables((rows) =>
                            rows.map((row, n) =>
                              n === i ? { ...row, secret: e.target.checked } : row
                            )
                          )
                        }
                      />{' '}
                      Secret
                    </label>
                    <Button
                      variant="quiet"
                      onClick={() => setVariables((rows) => rows.filter((_, n) => n !== i))}
                    >
                      Remove
                    </Button>
                  </div>
                ))}
                <Button
                  variant="quiet"
                  onClick={() =>
                    setVariables((rows) => [...rows, { name: '', value: '', secret: false }])
                  }
                >
                  Add variable
                </Button>
              </fieldset>
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
                Sign-in
                <SelectField
                  value={draft.auth?.type || 'none'}
                  onChange={(e) =>
                    patch({
                      auth: {
                        ...draft.auth,
                        type: e.target.value as NonNullable<AiConnection['auth']>['type'],
                      },
                    })
                  }
                >
                  <option value="none">None</option>
                  <option value="token">Access token</option>
                  <option value="oauth">Sign in with browser</option>
                </SelectField>
              </label>
              {draft.auth?.type === 'token' && (
                <label className="block">
                  Access token
                  <TextField
                    type="password"
                    autoComplete="off"
                    value={token}
                    placeholder={editing ? 'Saved' : ''}
                    onChange={(e) => setToken(e.target.value)}
                  />
                </label>
              )}
              <details>
                <summary>Advanced</summary>
                <label className="block">
                  Header name
                  <TextField
                    value={draft.auth?.header || 'Authorization'}
                    onChange={(e) =>
                      patch({
                        auth: {
                          ...draft.auth,
                          type: draft.auth?.type || 'none',
                          header: e.target.value,
                        },
                      })
                    }
                  />
                </label>
                {draft.auth?.type === 'oauth' && (
                  <label className="block">
                    Client ID
                    <TextField
                      value={draft.auth.clientId || ''}
                      onChange={(e) =>
                        patch({ auth: { ...draft.auth, type: 'oauth', clientId: e.target.value } })
                      }
                    />
                  </label>
                )}
                {draft.auth?.type === 'oauth' && (
                  <label className="block">
                    Requested access
                    <TextField
                      value={draft.auth.scopes || ''}
                      onChange={(e) => patch({ auth: { ...draft.auth!, scopes: e.target.value } })}
                    />
                  </label>
                )}
                <label className="block">
                  Time limit, seconds
                  <TextField
                    type="number"
                    min={1}
                    max={1800}
                    value={draft.timeoutSeconds || 120}
                    onChange={(e) => patch({ timeoutSeconds: Number(e.target.value) })}
                  />
                </label>
              </details>
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
              {preview.manifest.importWarnings?.map((w) => (
                <p key={w} className="text-ink-3">
                  {w}
                </p>
              ))}
              {preview.manifest.prompts?.map((p) => (
                <p key={p.name}>
                  /{p.name} · {p.title || 'Prompt'}
                </p>
              ))}
              {preview.manifest.agents?.map((p) => (
                <p key={p.id}>{p.name} · Profile</p>
              ))}
              {preview.manifest.providers?.map((p) => (
                <p key={p.id}>
                  {p.title} · {p.baseURL}
                </p>
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
      {state.project && (
        <div className="border-y border-ink-4/20 py-2">
          <div className="flex items-center gap-2">
            <span className="flex-1">This folder has AI extensions. Not running.</span>
            <Button
              variant="quiet"
              onClick={() => setPanel(panel === 'project' ? null : 'project')}
            >
              Review
            </Button>
          </div>
          <p className="text-[11px] text-ink-3">{state.project.folder}</p>
          {panel === 'project' &&
            (state.project.error ? (
              <Message tone="error">{state.project.error}</Message>
            ) : (
              <div className="space-y-2 py-2">
                <strong>{state.project.preview.name}</strong>
                {state.project.preview.connections?.map((c) => (
                  <pre key={c.id} className="whitespace-pre-wrap break-all text-[11px]">
                    {c.transport === 'stdio' ? [c.command, ...(c.args || [])].join(' ') : c.url}
                  </pre>
                ))}
                <Button
                  onClick={() =>
                    void run('project', async () => {
                      await request(
                        'trust-project',
                        state.project!.folder,
                        state.project!.entryHash
                      )
                      setPanel(null)
                    })
                  }
                >
                  Add for this folder
                </Button>
              </div>
            ))}
        </div>
      )}
      {error && <Message tone="error">{error}</Message>}
      {information && <Message tone="success">{information}</Message>}
      {!state.extensions.length && <p className="text-ink-3">No extensions yet.</p>}
      {state.extensions.map((e) => {
        const tested = e.connections.every((c) => c.tested)
        const failure = e.connections.find((c) => c.error)?.error || e.error
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
              <span className={failure ? 'text-rose-300' : 'text-ink-3'}>
                {busy === e.id
                  ? 'Connecting…'
                  : failure
                    ? /sign.in required/i.test(failure)
                      ? 'Sign in required'
                      : `Couldn't start: ${failure}`
                    : changed
                      ? 'Tools changed. Review'
                      : tested
                        ? `${tools.length} tools · ${prompts.length} prompts`
                        : 'Not tested yet'}
              </span>
              <Button variant="quiet" onClick={() => setDetails(details === e.id ? null : e.id)}>
                {changed ? 'Review' : 'Details ›'}
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
                    <div className="flex gap-2">
                      <Button variant="quiet" onClick={() => edit(e, c)}>
                        Edit connection
                      </Button>
                      {c.auth?.type === 'oauth' && (
                        <Button
                          disabled={!!signIn}
                          onClick={() =>
                            void run(e.id, async () => {
                              const result = await request<{ url: string }>(
                                'oauth-start',
                                e.id,
                                c.id
                              )
                              setSignIn({ extension: e.id, connection: c.id })
                              await window.tezbar.openExternalUrl(result.url)
                            })
                          }
                        >
                          Sign in
                        </Button>
                      )}
                    </div>
                    {c.catalog?.warnings?.map((w, i) => (
                      <Message tone="error" key={i}>
                        {w}
                      </Message>
                    ))}
                    {c.catalog?.tools?.map((t) => (
                      <label className="flex gap-2 py-1" key={t.name}>
                        <input
                          type="checkbox"
                          checked={!c.disabledTools?.[t.name]}
                          onChange={(ev) =>
                            void run(e.id, () =>
                              request('set-tool', e.id, t.name, ev.target.checked, c.id)
                            )
                          }
                        />
                        <span>
                          {t.name} <span className="text-ink-3">{t.description}</span>
                          {c.modelNames?.[t.name] && (
                            <span className="block font-mono text-[11px] text-ink-3">
                              Profile tool name: {c.modelNames[t.name]}
                            </span>
                          )}
                        </span>
                        {c.allowances?.[t.name] && (
                          <Button
                            variant="quiet"
                            onClick={() =>
                              void run(e.id, () => request('clear-allowance', e.id, c.id, t.name))
                            }
                          >
                            Always allowed ×
                          </Button>
                        )}
                      </label>
                    ))}
                  </div>
                ))}
                {changed &&
                  e.connections
                    .filter((c) => c.needsReview)
                    .map((c) => (
                      <div key={c.id} className="space-y-1 text-[11px]">
                        {(c.catalog?.tools || []).map((t) => {
                          const old = c.previousCatalog?.tools?.find((o) => o.name === t.name)
                          return (
                            <div key={t.name}>
                              <strong>
                                {old ? 'Changed' : 'Added'}: {t.name}
                              </strong>
                              {old && (
                                <p>
                                  Before: {old.description || 'No description'} · Fields:{' '}
                                  {Object.keys(old.inputSchema?.properties || {}).join(', ') ||
                                    'none'}
                                </p>
                              )}
                              <p>
                                After: {t.description || 'No description'} · Fields:{' '}
                                {Object.keys(t.inputSchema?.properties || {}).join(', ') || 'none'}
                              </p>
                            </div>
                          )
                        })}
                        {c.previousCatalog?.tools
                          ?.filter((t) => !c.catalog?.tools?.some((n) => n.name === t.name))
                          .map((t) => (
                            <p key={t.name}>Removed: {t.name}</p>
                          ))}
                      </div>
                    ))}
                {e.connections.map((c) => (
                  <p className="text-[11px] text-ink-3" key={`${c.id}:summary`}>
                    Asks:{' '}
                    {c.toolPolicy === 'allow'
                      ? 'Allow all'
                      : c.toolPolicy === 'allowReadOnly'
                        ? "Allow read-only tools, based on the program's claim"
                        : 'Ask every time'}{' '}
                    · {c.catalog?.resources?.length || 0} files · Prompts:{' '}
                    {c.catalog?.prompts?.map((p) => p.name).join(', ') || 'none'}
                  </p>
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
                    variant="quiet"
                    onClick={() =>
                      void request<string[]>('logs', e.id)
                        .then((lines) =>
                          setLogs((l) => ({ ...l, [e.id]: l[e.id] ? undefined : lines }))
                        )
                        .catch((e) => setError(String(e)))
                    }
                  >
                    Show log
                  </Button>
                  <Button variant="danger" onClick={() => setRemoving(e.id)}>
                    Remove
                  </Button>
                </div>
                {logs[e.id] && (
                  <pre className="whitespace-pre-wrap break-all text-[11px]">
                    {logs[e.id]!.join('\n') || 'No log messages.'}
                  </pre>
                )}
                {removing === e.id && (
                  <div>
                    <p>Remove {e.name} and delete its saved secrets?</p>
                    <Button
                      variant="danger"
                      onClick={() => void run(e.id, () => request('remove', e.id))}
                    >
                      Remove
                    </Button>
                    <Button variant="quiet" onClick={() => setRemoving(null)}>
                      Cancel
                    </Button>
                  </div>
                )}
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
      <AiContributionsEditor extensions={state.extensions} onChange={load} />
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
