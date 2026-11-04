import { AiContributionsEditor } from './AiContributionsEditor'
import { extensionRequest as request } from './api'
import { useCallback, useEffect, useState, useRef } from 'react'
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
function connectionPreview(c: AiConnection): string {
  const command =
    c.transport === 'stdio'
      ? `Command: ${JSON.stringify(c.command)}\nArguments: ${JSON.stringify(c.args || [])}\nWorking folder: ${c.cwd || '(inherited)'}`
      : `URL: ${c.url}`
  const fields = (['env', 'headers'] as const).flatMap((kind) =>
    Object.entries(c[kind] || {}).map(
      ([name, value]) =>
        `${kind === 'env' ? 'Variable' : 'Header'} ${name}: ${typeof value === 'string' && !/token|secret|password|api.?key|authorization|credential|cookie/i.test(name) ? value : '[Secret — saved separately]'}${/^(PATH|NODE_OPTIONS|NPM_CONFIG_|DYLD_|LD_|PYTHON|GIT_|BASH_ENV|PERL5OPT|RUBYOPT)/i.test(name) ? ' [Changes what runs — review before Test]' : ''}`
    )
  )
  if (c.auth?.type && c.auth.type !== 'none')
    fields.push(`Header ${c.auth.header || 'Authorization'}: [Secret — saved separately]`)
  return [command, ...fields].join('\n')
}
export function AiExtensionsSection(): JSX.Element {
  const [state, setState] = useState<AiExtensionsState>({
    extensions: [],
    globalInstructions: '',
    useAgentsMd: true,
  })
  const describeConnection = (c: AiConnection): string =>
    connectionPreview({ ...c, cwd: c.cwd || state.defaultWorkingFolder })
  const [panel, setPanel] = useState<'menu' | 'connection' | 'paste' | 'project' | null>(null)
  const [draft, setDraft] = useState<AiConnection>(blank)
  const [argsText, setArgsText] = useState('')
  const [timeoutText, setTimeoutText] = useState('120')
  const instructionsDirty = useRef(false)
  const [instructionsDraft, setInstructionsDraft] = useState('')
  const [token, setToken] = useState('')
  const [editing, setEditing] = useState<AiExtension | null>(null)
  const [variables, setVariables] = useState<
    { name: string; value: string; secret: boolean; saved?: boolean }[]
  >([])
  const [signIn, setSignIn] = useState<{ extension: string; connection: string } | null>(null)
  const [information, setInformation] = useState('')
  const [providerSecrets, setProviderSecrets] = useState<Record<string, string>>({})

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
  const [allTools, setAllTools] = useState<Record<string, boolean>>({})
  const [logs, setLogs] = useState<Record<string, string[] | undefined>>({})
  const loadRevision = useRef(0)
  const scrolledReview = useRef<string>()
  const load = useCallback(async () => {
    const revision = ++loadRevision.current
    const next = await request<AiExtensionsState>('list')
    if (revision !== loadRevision.current) return
    setState(next)
    if (!instructionsDirty.current) setInstructionsDraft(next.globalInstructions)
  }, [])
  useEffect(() => {
    if (!state.projectReviewId || scrolledReview.current === state.projectReviewId) return
    scrolledReview.current = state.projectReviewId
    if (state.project)
      document.getElementById('ai-extensions-section')?.scrollIntoView?.({ block: 'start' })
  }, [state.projectReviewId, state.project])
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
    setArgsText(connection.args?.join('\n') || '')
    setTimeoutText(String(connection.timeoutSeconds ?? 120))
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
      args: argsText.split('\n').filter(Boolean),
      timeoutSeconds: timeoutText.trim() ? Number(timeoutText) : 120,
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
      else {
        const saved = await request<AiExtension>('add', record)
        record.connections = saved.connections
      }
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
    <section
      id="ai-extensions-section"
      aria-label="AI Extensions"
      className="space-y-3 py-3 text-[12px] text-ink-2"
    >
      {state.canResetSecrets && (
        <div role="alert">
          <p>
            The saved connection key is missing. Reset archives the unreadable secrets; enter
            connection credentials again afterward.
          </p>
          <Button
            variant="danger"
            onClick={() => {
              void request('reset-secrets')
                .then(load)
                .catch((e) => setError(String(e)))
            }}
          >
            Reset saved connection secrets
          </Button>
        </div>
      )}
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
              setArgsText('')
              setTimeoutText('120')
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
                e.currentTarget.value = ''
                setPreview(null)
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
                Time limit, seconds
                <TextField
                  type="number"
                  min={1}
                  max={1800}
                  value={timeoutText}
                  onChange={(e) => setTimeoutText(e.target.value)}
                />
              </label>
              <label className="block">
                Arguments, one per line
                <TextArea value={argsText} onChange={(e) => setArgsText(e.target.value)} />
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
                    value={timeoutText}
                    onChange={(e) => setTimeoutText(e.target.value)}
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
            {describeConnection({
              ...draft,
              args: argsText.split('\n').filter(Boolean),
              env: Object.fromEntries(
                variables.map((v) => [v.name, v.secret ? { secret: v.name } : v.value])
              ),
            })}
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
                  {describeConnection(c)}
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
                <pre className="whitespace-pre-wrap break-all text-[11px]" key={p.id}>
                  {describeConnection({
                    id: p.id,
                    name: p.title,
                    transport: 'http',
                    url: p.baseURL,
                    headers: p.headers,
                  })}
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
          <Button
            variant="quiet"
            onClick={() =>
              void run('project', async () => {
                await request('dismiss-project-review', state.projectReviewId)
                setPanel(null)
              })
            }
          >
            {state.project.error ? 'Dismiss' : 'Keep disabled'}
          </Button>
          {panel === 'project' &&
            (state.project.error ? (
              <Message tone="error">{state.project.error}</Message>
            ) : (
              <div className="space-y-2 py-2">
                <strong>{state.project.preview.name}</strong>
                {state.project.preview.connections?.map((c) => (
                  <pre key={c.id} className="whitespace-pre-wrap break-all text-[11px]">
                    {describeConnection(c)}
                  </pre>
                ))}
                <pre className="whitespace-pre-wrap break-all text-[11px]">
                  {JSON.stringify(
                    {
                      instructions: state.project.preview.instructions,
                      prompts: state.project.preview.prompts,
                      commands: state.project.preview.commands,
                      agents: state.project.preview.agents,
                      providers: state.project.preview.providers,
                    },
                    null,
                    2
                  )}
                </pre>
                <Button
                  onClick={() =>
                    void run('project', async () => {
                      await request(
                        'trust-project',
                        state.project!.folder,
                        state.project!.entryHash,
                        state.projectReviewId
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
      {signIn && (
        <Button
          onClick={() =>
            void run('cancel-signin', async () => {
              await request('oauth-cancel', signIn.extension, signIn.connection)
              setSignIn(null)
            })
          }
        >
          Cancel sign-in
        </Button>
      )}
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
              {e.connections.some((c) => c.needsAuth && c.auth?.type === 'oauth') && (
                <Button
                  variant="quiet"
                  onClick={() => {
                    const c = e.connections.find((c) => c.needsAuth && c.auth?.type === 'oauth')!
                    void run(e.id, async () => {
                      const result = await request<{ url: string }>('oauth-start', e.id, c.id)
                      setSignIn({ extension: e.id, connection: c.id })
                      await window.tezbar.openExternalUrl(result.url)
                    })
                  }}
                >
                  Sign in
                </Button>
              )}

              <span
                role={failure ? 'alert' : undefined}
                className={failure ? 'ai-extension-error' : 'text-ink-3'}
              >
                {busy === e.id
                  ? 'Connecting…'
                  : failure
                    ? e.connections.some((c) => c.needsAuth)
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
                    <pre className="whitespace-pre-wrap break-all">{describeConnection(c)}</pre>
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
                    {c.catalog?.tools?.slice(0, allTools[c.id] ? undefined : 8).map((t) => (
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
                    {(c.catalog?.tools?.length || 0) > 8 && !allTools[c.id] && (
                      <Button onClick={() => setAllTools((s) => ({ ...s, [c.id]: true }))}>
                        Show all {c.catalog?.tools?.length}
                      </Button>
                    )}
                  </div>
                ))}
                {changed &&
                  e.connections
                    .filter((c) => c.needsReview)
                    .map((c) => (
                      <div key={c.id} className="space-y-1 text-[11px]">
                        {(c.catalog?.tools || []).map((t) => {
                          const old = c.previousCatalog?.tools?.find((o) => o.name === t.name)
                          if (old && JSON.stringify(old) === JSON.stringify(t)) return null
                          return (
                            <div key={t.name}>
                              <strong>
                                {old ? 'Changed' : 'Added'}: {t.name}
                              </strong>
                              {old && (
                                <p>
                                  Before: {old.description || 'No description'} · Fields:{' '}
                                  {JSON.stringify(old.inputSchema || {})} · Title:{' '}
                                  {old.title || 'None'} · Annotations (readOnlyHint /
                                  destructiveHint): {JSON.stringify(old.annotations || {})} · Output
                                  schema: {JSON.stringify(old.outputSchema || {})}
                                </p>
                              )}
                              <p>
                                After: {t.description || 'No description'} · Fields:{' '}
                                {JSON.stringify(t.inputSchema || {})} · Title: {t.title || 'None'} ·
                                Annotations (readOnlyHint / destructiveHint):{' '}
                                {JSON.stringify(t.annotations || {})} · Output schema:{' '}
                                {JSON.stringify(t.outputSchema || {})}
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
                {e.providers?.map((provider) => (
                  <div key={provider.id} className="space-y-2">
                    <p>
                      {provider.title} · {provider.baseURL}
                    </p>
                    <pre className="whitespace-pre-wrap break-all text-[11px]">
                      {describeConnection({
                        id: provider.id,
                        name: provider.title,
                        transport: 'http',
                        url: provider.baseURL,
                        headers: provider.headers,
                      })}
                    </pre>
                    {Object.entries(provider.headers || {})
                      .filter(
                        (entry): entry is [string, { secret: string; label?: string }] =>
                          typeof entry[1] !== 'string'
                      )
                      .map(([header, reference]) => {
                        const key = JSON.stringify([e.id, provider.id, reference.secret])
                        return (
                          <label className="block" key={header}>
                            {reference.label || header}
                            <TextField
                              type="password"
                              autoComplete="off"
                              value={providerSecrets[key] || ''}
                              onChange={(event) =>
                                setProviderSecrets((values) => ({
                                  ...values,
                                  [key]: event.target.value,
                                }))
                              }
                            />
                            <Button
                              disabled={!providerSecrets[key] || !!busy}
                              onClick={() =>
                                void run(e.id, async () => {
                                  await request(
                                    'set-secret',
                                    e.id,
                                    reference.secret,
                                    providerSecrets[key],
                                    { providerId: provider.id }
                                  )
                                  setProviderSecrets((values) => ({ ...values, [key]: '' }))
                                  setInformation(
                                    'Key saved. Enable the extension to use this provider.'
                                  )
                                })
                              }
                            >
                              Save key
                            </Button>
                          </label>
                        )
                      })}
                  </div>
                ))}
                {e.connections.map((c) => (
                  <p className="text-[11px] text-ink-3" key={`${c.id}:summary`}>
                    Asks:{' '}
                    <SelectField
                      aria-label={`Tool policy for ${c.name}`}
                      value={c.toolPolicy || 'ask'}
                      onChange={(event) =>
                        edit(e, {
                          ...c,
                          toolPolicy: event.target.value as AiConnection['toolPolicy'],
                        })
                      }
                    >
                      <option value="ask">Ask every time</option>
                      <option value="allowReadOnly">
                        Allow read-only tools, based on the program's claim
                      </option>
                      <option value="allow">Allow all</option>
                    </SelectField>
                    · {c.catalog?.resources?.length || 0} files · Prompts:{' '}
                    {c.catalog?.prompts?.map((p) => p.name).join(', ') || 'none'}
                  </p>
                ))}
                <div className="flex gap-2">
                  {changed && (
                    <>
                      <Button
                        onClick={() =>
                          void run(e.id, () =>
                            request(
                              'accept-review',
                              e.id,
                              Object.fromEntries(
                                e.connections
                                  .filter((c) => c.needsReview)
                                  .map((c) => [c.id, { pins: c.pendingPins, trustPin: c.trustPin }])
                              )
                            )
                          )
                        }
                      >
                        Accept changes
                      </Button>
                      <Button
                        onClick={() => void run(e.id, () => request('set-enabled', e.id, false))}
                      >
                        Keep disabled
                      </Button>
                    </>
                  )}
                  <Button onClick={() => void test(e.id)}>
                    {failure ? 'Try again' : 'Test connection'}
                  </Button>
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
            value={instructionsDraft}
            onChange={(e) => {
              instructionsDirty.current = true
              setInstructionsDraft(e.target.value)
            }}
            onBlur={() =>
              void run('instructions', () =>
                request('set-instructions', instructionsDraft, state.useAgentsMd).then(() => {
                  instructionsDirty.current = false
                })
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
                request('set-instructions', instructionsDraft, checked)
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
