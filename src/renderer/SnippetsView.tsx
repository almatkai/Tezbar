import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { RAYMES_NEW_SNIPPET_EVENT } from '../shared/snippetEvents'
import type { SnippetListRow } from '../shared/snippets'
import { GlideList } from './ui/GlideList'
import { Button, FieldLabel, Hint, HintBar, Kbd, Message, TextArea, TextField, ViewHeader } from './ui/primitives'

type EditorState = null | 'create' | { mode: 'edit'; id: string }

export default function SnippetsView({ onBack }: { onBack: () => void }): JSX.Element {
  const rootRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const [rows, setRows] = useState<SnippetListRow[]>([])
  const [selected, setSelected] = useState(0)
  const [query, setQuery] = useState('')
  const [msg, setMsg] = useState<{ tone: 'success' | 'error'; text: string } | null>(null)
  const [editor, setEditor] = useState<EditorState>(null)
  const [form, setForm] = useState({ label: '', trigger: '', body: '' })

  const reload = useCallback(async (): Promise<SnippetListRow[]> => {
    const items = await window.tezbar.listSnippets()
    setRows(items)
    return items
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  useEffect(() => {
    const onEsc = (e: KeyboardEvent): void => {
      if (e.key !== 'Escape') return
      if (editor) {
        e.preventDefault()
        e.stopPropagation()
        setEditor(null)
        return
      }
      e.preventDefault()
      e.stopPropagation()
      if (query.length > 0 && document.activeElement === inputRef.current) {
        setQuery('')
        return
      }
      onBack()
    }
    window.addEventListener('keydown', onEsc, true)
    return () => window.removeEventListener('keydown', onEsc, true)
  }, [onBack, query, editor])

  // ⌘N is handled by a main-process global shortcut (quick notes). App.tsx
  // forwards that IPC as a window event while this surface is visible.
  useEffect(() => {
    const onNew = (): void => {
      setForm({ label: '', trigger: '', body: '' })
      setEditor('create')
      setMsg(null)
    }
    window.addEventListener(RAYMES_NEW_SNIPPET_EVENT, onNew)
    return () => window.removeEventListener(RAYMES_NEW_SNIPPET_EVENT, onNew)
  }, [])

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    const next = !q
      ? [...rows]
      : rows.filter(
        (r) =>
          r.title.toLowerCase().includes(q) ||
          r.subtitle.toLowerCase().includes(q) ||
          r.trigger.toLowerCase().includes(q) ||
          r.bodyTemplate.toLowerCase().includes(q) ||
          r.resolvedPreview.toLowerCase().includes(q),
      )
    next.sort((a, b) => {
      if (a.readonly !== b.readonly) return a.readonly ? 1 : -1
      return a.title.localeCompare(b.title)
    })
    return next
  }, [rows, query])

  const current = filtered[selected] ?? null

  useEffect(() => {
    if (filtered.length === 0) {
      setSelected(0)
      return
    }
    setSelected((i) => (i >= filtered.length ? filtered.length - 1 : i))
  }, [filtered.length])

  const copyRow = useCallback(async (row: SnippetListRow): Promise<void> => {
    if (editor) return
    const r = await window.tezbar.copySnippet(row.id)
    setMsg({ tone: r.ok ? 'success' : 'error', text: r.message })
    if (r.ok) void window.tezbar.hide()
  }, [editor])

  const openCreate = (): void => {
    setForm({ label: '', trigger: '', body: '' })
    setEditor('create')
    setMsg(null)
  }

  const openEdit = (row: SnippetListRow): void => {
    if (row.readonly) return
    setForm({ label: row.title, trigger: row.trigger, body: row.bodyTemplate })
    setEditor({ mode: 'edit', id: row.id })
    setMsg(null)
  }

  const saveSnippet = async (): Promise<void> => {
    const payload = { label: form.label, trigger: form.trigger, body: form.body }
    if (editor === 'create') {
      const r = await window.tezbar.addSnippet(payload)
      setMsg({ tone: r.ok ? 'success' : 'error', text: r.message })
      if (r.ok) {
        await reload()
        setEditor(null)
      }
      return
    }
    if (editor && typeof editor === 'object' && editor.mode === 'edit') {
      const r = await window.tezbar.updateSnippet(editor.id, payload)
      setMsg({ tone: r.ok ? 'success' : 'error', text: r.message })
      if (r.ok) {
        await reload()
        setEditor(null)
      }
    }
  }

  const deleteSelected = useCallback(async (): Promise<void> => {
    if (!current || current.readonly || editor) return
    if (!window.confirm(`Delete snippet “${current.title}”? This cannot be undone.`)) return
    const r = await window.tezbar.deleteSnippet(current.id)
    setMsg({ tone: r.ok ? 'success' : 'error', text: r.message })
    if (r.ok) {
      await reload()
      setSelected(0)
    }
  }, [current, editor, reload])

  useEffect(() => {
    if (editor !== null) return
    const id = requestAnimationFrame(() => {
      rootRef.current?.focus({ preventScroll: true })
    })
    return () => cancelAnimationFrame(id)
  }, [editor])

  useEffect(() => {
    if (editor) return
    const onKey = (e: KeyboardEvent): void => {
      if (filtered.length === 0) return

      const cur = filtered[selected] ?? null

      if (e.key === 'ArrowDown') {
        e.preventDefault()
        e.stopPropagation()
        setSelected((i) => Math.min(i + 1, filtered.length - 1))
        return
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        e.stopPropagation()
        setSelected((i) => Math.max(i - 1, 0))
        return
      }
      if (e.key === 'Enter') {
        e.preventDefault()
        e.stopPropagation()
        const row = filtered[selected]
        if (row) void copyRow(row)
        return
      }

      const meta = e.metaKey || e.ctrlKey
      if (meta && e.key.toLowerCase() === 'e' && cur && !cur.readonly) {
        e.preventDefault()
        e.stopPropagation()
        openEdit(cur)
        return
      }
      if (meta && (e.key === 'Backspace' || e.key === 'Delete') && cur && !cur.readonly) {
        e.preventDefault()
        e.stopPropagation()
        void deleteSelected()
        return
      }

      if (meta && /^[1-9]$/.test(e.key)) {
        e.preventDefault()
        e.stopPropagation()
        const target = Number(e.key) - 1
        if (target < filtered.length) {
          setSelected(target)
          const row = filtered[target]
          if (row) void copyRow(row)
        }
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [editor, filtered, selected, copyRow, openEdit, deleteSelected, current])

  const editorTitle = editor === 'create' ? 'New snippet' : editor ? 'Edit snippet' : ''

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      role="application"
      aria-label="Snippets"
      className="flex h-full min-h-0 w-full flex-col gap-2 outline-none animate-tezbar-scale-in"
    >
      <div className="glass-card shrink-0 px-4 py-3 animate-tezbar-scale-in">
        <ViewHeader
          title="Snippets"
          onBack={onBack}
          trailing={
            editor ? null : (
              <Button type="button" variant="primary" className="!px-2.5 !py-1 text-[11px]" onClick={openCreate}>
                New snippet
              </Button>
            )
          }
        />
        {!editor ? (
          <div className="relative mt-2">
            <input
              ref={inputRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search snippets…"
              className="h-8 w-full rounded-tezbar-chip border border-white/10 bg-white/[0.04] px-2.5 text-[12px] text-ink-1 placeholder:text-ink-4 outline-none transition focus:border-white/20 focus:bg-white/[0.06]"
            />
          </div>
        ) : null}
      </div>

      <div className="glass-card flex min-h-0 flex-1 flex-col overflow-x-hidden overflow-y-auto px-2 py-2 animate-tezbar-scale-in">
        {editor ? (
          <form
            className="flex min-h-0 flex-1 flex-col gap-2 px-2 pb-2 pt-1"
            onSubmit={(e) => {
              e.preventDefault()
              void saveSnippet()
            }}
          >
            <h2 className="shrink-0 text-[12px] font-semibold text-ink-2">{editorTitle}</h2>
            <div className="shrink-0 space-y-1">
              <FieldLabel htmlFor="snippet-title">Title</FieldLabel>
              <TextField
                id="snippet-title"
                value={form.label}
                onChange={(e) => setForm((f) => ({ ...f, label: e.target.value }))}
                placeholder="e.g. Stand-up reply"
                autoFocus
              />
            </div>
            <div className="shrink-0 space-y-1">
              <FieldLabel htmlFor="snippet-trigger">Trigger</FieldLabel>
              <TextField
                id="snippet-trigger"
                value={form.trigger}
                onChange={(e) => setForm((f) => ({ ...f, trigger: e.target.value }))}
                placeholder="Short search key, e.g. standup"
              />
              <p className="text-[10px] text-ink-4">Single line, unique among snippets. Used in search and the list hint.</p>
            </div>
            <div className="flex min-h-0 flex-1 flex-col gap-1">
              <FieldLabel htmlFor="snippet-body" className="shrink-0">
                Body
              </FieldLabel>
              <div className="box-border flex min-h-0 flex-1 flex-col rounded-tezbar-chip border border-white/12 bg-white/[0.02] p-px">
                <TextArea
                  id="snippet-body"
                  value={form.body}
                  onChange={(e) => setForm((f) => ({ ...f, body: e.target.value }))}
                  placeholder="Text copied when the snippet runs. Use ${date}, ${time}, ${uuid}, … for dynamic parts."
                  className="!min-h-0 flex-1 resize-y overflow-auto rounded-[10px] border-0 bg-black/30 px-2 py-2 font-mono text-[12px] !shadow-none outline-none ring-0 focus:!shadow-none focus:ring-0"
                />
              </div>
            </div>
            <div className="flex shrink-0 flex-wrap items-center gap-2 border-t border-white/[0.08] pt-2">
              <Button type="submit" variant="primary" className="!px-3 !py-1.5 text-[11px]">
                Save
              </Button>
              <Button
                type="button"
                variant="quiet"
                className="!px-3 !py-1.5 text-[11px]"
                onClick={() => setEditor(null)}
              >
                Cancel
              </Button>
            </div>
          </form>
        ) : (
          <section className="flex min-h-0 flex-1 gap-3">
            <div className="flex min-h-0 w-[44%] max-w-[340px] flex-col overflow-hidden">
              {filtered.length === 0 ? (
                <div className="grid flex-1 place-items-center px-4 text-center">
                  <p className="text-[12px] text-ink-4">
                    {rows.length === 0 ? 'No snippets yet. Create one with “New snippet”.' : 'No snippets match your search.'}
                  </p>
                </div>
              ) : (
                <div className="min-h-0 flex-1 overflow-y-auto pr-0.5">
                  <GlideList
                    selectedIndex={selected}
                    itemCount={filtered.length}
                    className="flex flex-col gap-0.5 py-0.5"
                  >
                    {filtered.map((row, i) => (
                      <li key={row.id} className="relative z-[1]">
                        <button
                          type="button"
                          onMouseEnter={() => setSelected(i)}
                          onClick={() => {
                            setSelected(i)
                            void copyRow(row)
                          }}
                          className="flex w-full flex-col gap-0.5 rounded-tezbar-row px-2 py-1.5 text-left transition hover:bg-white/[0.04]"
                        >
                          <span className="flex items-center gap-1.5">
                            <span
                              className={`min-w-0 flex-1 truncate text-left text-[12.5px] ${i === selected ? 'text-ink-1' : 'text-ink-2'}`}
                            >
                              {row.title}
                            </span>
                            {row.readonly ? (
                              <span className="shrink-0 rounded bg-white/[0.06] px-1 py-0.5 text-[9px] uppercase tracking-wide text-ink-4">
                                Built-in
                              </span>
                            ) : null}
                          </span>
                          {row.subtitle ? (
                            <span className="truncate text-[11px] text-ink-4">{row.subtitle}</span>
                          ) : null}
                        </button>
                      </li>
                    ))}
                  </GlideList>
                </div>
              )}
            </div>

            <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-tezbar-row border border-white/[0.06] bg-white/[0.02]">
              {current ? (
                <div className="flex h-full min-h-0 flex-col p-3">
                  <div className="mb-2 flex shrink-0 items-center justify-between gap-2">
                    <p className="text-[11px] font-medium uppercase tracking-[0.12em] text-ink-4">Preview</p>
                    {!current.readonly ? (
                      <div className="flex items-center gap-1">
                        <Button type="button" variant="quiet" className="!px-2 !py-0.5 text-[10px]" onClick={() => openEdit(current)}>
                          Edit
                        </Button>
                        <Button type="button" variant="danger" className="!px-2 !py-0.5 text-[10px]" onClick={() => void deleteSelected()}>
                          Delete
                        </Button>
                      </div>
                    ) : null}
                  </div>
                  <pre className="min-h-0 flex-1 overflow-auto whitespace-pre-wrap break-words rounded-tezbar-chip border border-white/[0.06] bg-black/20 p-2.5 font-mono text-[11.5px] leading-relaxed text-ink-2">
                    {current.resolvedPreview}
                  </pre>
                  {current.bodyTemplate !== current.resolvedPreview ? (
                    <p className="mt-2 shrink-0 truncate text-[10px] text-ink-4" title={current.bodyTemplate}>
                      Template: {current.bodyTemplate}
                    </p>
                  ) : null}
                </div>
              ) : (
                <div className="grid h-full place-items-center px-6 text-center">
                  <p className="text-[12px] text-ink-4">Select a snippet to preview expanded text.</p>
                </div>
              )}
            </div>
          </section>
        )}

        {msg ? (
          <div className="mt-2 shrink-0 px-1">
            <Message tone={msg.tone}>{msg.text}</Message>
          </div>
        ) : null}
      </div>

      <div className="glass-card flex shrink-0 items-center justify-between gap-3 px-4 py-2 animate-tezbar-scale-in">
        <span className="text-[10.5px] uppercase tracking-[0.14em] text-ink-4">
          {editor ? (editor === 'create' ? 'New snippet' : 'Editing') : `${filtered.length} snippet${filtered.length === 1 ? '' : 's'}`}
        </span>
        <HintBar>
          {editor ? (
            <>
              <Hint label="Save" keys={<Kbd>↵</Kbd>} />
              <Hint label="Cancel" keys={<Kbd>Esc</Kbd>} />
            </>
          ) : (
            <>
              <Hint label="New" keys={<><Kbd>⌘</Kbd><Kbd>N</Kbd></>} />
              <Hint label="Copy" keys={<Kbd>↵</Kbd>} />
              {current && !current.readonly ? (
                <>
                  <Hint label="Edit" keys={<><Kbd>⌘</Kbd><Kbd>E</Kbd></>} />
                  <Hint label="Delete" keys={<><Kbd>⌘</Kbd><Kbd>⌫</Kbd></>} />
                </>
              ) : null}
              <Hint label="Jump" keys={<><Kbd>⌘</Kbd><Kbd>1-9</Kbd></>} />
              <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
            </>
          )}
        </HintBar>
      </div>
    </div>
  )
}
