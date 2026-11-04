import { useState } from 'react'
import { Button, TextField, TextArea, Message } from '../ui/primitives'
import { extensionRequest as request } from './api'
import type { AiExtension } from '../../shared/aiExtensions'
type Draft = {
  extension: AiExtension | null
  kind: 'prompt' | 'profile' | 'command'
  original?: string
  name: string
  title: string
  text: string
  arguments: string
  tools: string
}
export function AiContributionsEditor({
  extensions,
  onChange,
}: {
  extensions: AiExtension[]
  onChange: () => Promise<void>
}): JSX.Element {
  const [draft, setDraft] = useState<Draft | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const patch = (value: Partial<Draft>): void => setDraft((d) => (d ? { ...d, ...value } : d))
  const edit = (kind: Draft['kind'], extension: AiExtension | null, name = ''): void => {
    const prompt = extension?.prompts?.find((p) => p.name === name)
    const agent = extension?.agents?.find((p) => p.id === name)
    const command = extension?.commands?.find((c) => c.name === name)
    setDraft({
      kind,
      extension,
      original: name || undefined,
      name,
      title: prompt?.title || agent?.name || '',
      text: command?.prompt || prompt?.template || agent?.instructions || '',
      arguments: prompt?.arguments?.map((a) => a.name + (a.required ? '*' : '')).join(', ') || '',
      tools: agent?.tools?.join(', ') || '',
    })
    setError('')
  }
  const mutate = async (task: () => Promise<void>): Promise<void> => {
    setBusy(true)
    setError('')
    try {
      await task()
      await onChange()
      setDraft(null)
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }
  const save = async (): Promise<void> => {
    if (!draft) return
    await mutate(async () => {
      if (!/^[A-Za-z0-9_-]{1,64}$/.test(draft.name))
        throw new Error('Use a name with letters, numbers, underscores or hyphens.')
      const extension = draft.extension ||
        extensions.find((e) => e.id === 'local.contributions') || {
          manifestVersion: 1 as const,
          id: 'local.contributions',
          name: 'Mine',
          connections: [],
          prompts: [],
          commands: [],
          agents: [],
        }
      let patch: Partial<AiExtension>
      if (draft.kind === 'command') {
        if (!extension.prompts?.some((p) => p.name === draft.text))
          throw new Error('Choose a prompt from this extension.')
        patch = {
          commands: [
            ...(extension.commands || []).filter(
              (c) => c.name !== draft.original && c.name !== draft.name
            ),
            { name: draft.name, prompt: draft.text },
          ],
        }
      } else if (draft.kind === 'prompt') {
        const prompt = {
          name: draft.name,
          title: draft.title,
          template: draft.text,
          arguments: draft.arguments
            .split(',')
            .map((a) => a.trim())
            .filter(Boolean)
            .map((a) => ({ name: a.replace(/\*$/, ''), required: a.endsWith('*') })),
        }
        patch = {
          prompts: [
            ...(extension.prompts || []).filter(
              (p) => p.name !== draft.original && p.name !== draft.name
            ),
            prompt,
          ],
          commands: (extension.commands || []).map((c) =>
            c.prompt === draft.original ? { ...c, prompt: draft.name } : c
          ),
        }
      } else {
        patch = {
          agents: [
            ...(extension.agents || []).filter(
              (a) => a.id !== draft.original && a.id !== draft.name
            ),
            {
              id: draft.name,
              name: draft.title || draft.name,
              instructions: draft.text,
              ...(draft.tools.trim()
                ? {
                    tools: draft.tools
                      .split(',')
                      .map((t) => t.trim())
                      .filter(Boolean),
                  }
                : {}),
            },
          ],
        }
      }
      if (extensions.some((e) => e.id === extension.id))
        await request('update', extension.id, patch)
      else {
        await request('add', { ...extension, ...patch })
        await request('set-enabled', extension.id, true)
      }
    })
  }
  const remove = (
    extension: AiExtension,
    kind: 'prompt' | 'profile' | 'command',
    name: string
  ): void => {
    void mutate(async () => {
      await request(
        'update',
        extension.id,
        kind === 'command'
          ? { commands: extension.commands?.filter((c) => c.name !== name) }
          : kind === 'prompt'
            ? {
                prompts: extension.prompts?.filter((p) => p.name !== name),
                commands: extension.commands?.filter((c) => c.prompt !== name),
              }
            : { agents: extension.agents?.filter((a) => a.id !== name) }
      )
    })
  }
  return (
    <div className="space-y-2 border-t border-ink-4/20 pt-3 text-[12px]">
      <div className="flex items-center gap-2">
        <h4 className="flex-1 text-[13px] text-ink-1">Prompts and commands</h4>
        <Button variant="quiet" onClick={() => edit('prompt', null)}>
          New
        </Button>
        <Button variant="quiet" onClick={() => edit('profile', null)}>
          New profile
        </Button>
        <Button variant="quiet" onClick={() => edit('command', null)}>
          New command
        </Button>
      </div>
      {extensions.flatMap((e) => [
        ...(e.prompts || []).map((p) => (
          <div key={`${e.id}:prompt:${p.name}`} className="flex items-center gap-2">
            <span className="flex-1">
              /{p.name}{' '}
              <span className="text-ink-3">
                {p.title} · {e.name}
              </span>
            </span>
            <Button variant="quiet" onClick={() => edit('prompt', e, p.name)}>
              Edit
            </Button>
            <Button variant="quiet" onClick={() => remove(e, 'prompt', p.name)}>
              Remove
            </Button>
          </div>
        )),
        ...(e.commands || [])
          .filter((c) => c.name !== c.prompt)
          .map((c) => (
            <div key={`${e.id}:command:${c.name}`} className="flex items-center gap-2">
              <span className="flex-1">
                /{c.name}{' '}
                <span className="text-ink-3">
                  {c.prompt} · {e.name}
                </span>
              </span>
              <Button variant="quiet" onClick={() => edit('command', e, c.name)}>
                Edit
              </Button>
              <Button variant="quiet" onClick={() => remove(e, 'command', c.name)}>
                Remove
              </Button>
            </div>
          )),
        ...(e.agents || []).map((a) => (
          <div key={`${e.id}:agent:${a.id}`} className="flex items-center gap-2">
            <span className="flex-1">
              {a.name} <span className="text-ink-3">Profile · {e.name}</span>
            </span>
            <Button variant="quiet" onClick={() => edit('profile', e, a.id)}>
              Edit
            </Button>
            <Button variant="quiet" onClick={() => remove(e, 'profile', a.id)}>
              Remove
            </Button>
          </div>
        )),
      ])}
      {draft && (
        <div className="space-y-2 py-2">
          <label className="block">
            Name
            <TextField value={draft.name} onChange={(e) => patch({ name: e.target.value })} />
          </label>
          {draft.kind !== 'command' && (
            <label className="block">
              Title
              <TextField value={draft.title} onChange={(e) => patch({ title: e.target.value })} />
            </label>
          )}
          <label className="block">
            {draft.kind === 'command'
              ? 'Prompt name'
              : draft.kind === 'prompt'
                ? 'Prompt'
                : 'Instructions'}
            <TextArea
              rows={4}
              value={draft.text}
              onChange={(e) => patch({ text: e.target.value })}
            />
          </label>
          {draft.kind === 'prompt' ? (
            <label className="block">
              Arguments, separated by commas. Add * for required arguments.
              <TextField
                value={draft.arguments}
                onChange={(e) => patch({ arguments: e.target.value })}
              />
            </label>
          ) : draft.kind === 'profile' ? (
            <label className="block">
              Allowed tools, separated by commas. Leave empty to use available tools.
              <TextField value={draft.tools} onChange={(e) => patch({ tools: e.target.value })} />
            </label>
          ) : null}
          <Button disabled={busy || !draft.name || !draft.text} onClick={() => void save()}>
            Save
          </Button>
          <Button variant="quiet" onClick={() => setDraft(null)}>
            Cancel
          </Button>
        </div>
      )}
      {error && <Message tone="error">{error}</Message>}
    </div>
  )
}
