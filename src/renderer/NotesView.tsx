import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { QuickNoteEntry } from '../shared/quickNotes'
import { Button, Hint, HintBar, Kbd, Message, ViewHeader } from './ui/primitives'
import { GlideList } from './ui/GlideList'

const ALLOWED_RICH_TAGS = new Set(['div', 'br', 'p', 'strong', 'b', 'em', 'i', 'u', 'ul', 'ol', 'li'])
const HTML_LIKE_RE = /<\/?[a-z][\s\S]*>/i

function formatDate(ms: number): string {
  const d = new Date(ms)
  const date = d.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  })
  const time = d.toLocaleTimeString(undefined, {
    hour: 'numeric',
    minute: '2-digit',
  })
  return `${date}, ${time}`
}

function stripMarkdownSyntax(text: string): string {
  return text
    .replace(/\*\*([^*\n]+)\*\*/g, '$1')
    .replace(/__([^_\n]+)__/g, '$1')
    .replace(/\*([^*\n]+)\*/g, '$1')
    .replace(/_([^_\n]+)_/g, '$1')
    .replace(/`([^`\n]+)`/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
    .replace(/^#{1,6}\s+/gm, '')
    .replace(/^\s*[-*+]\s+/gm, '')
    .replace(/^\s*\d+\.\s+/gm, '')
}

function decodeHtmlEntities(text: string): string {
  if (typeof document === 'undefined') return text
  const helper = document.createElement('textarea')
  helper.innerHTML = text
  return helper.value
}

function isLikelyHtml(text: string): boolean {
  return HTML_LIKE_RE.test(text)
}

function sanitizeRichHtml(input: string): string {
  const withoutScripts = input
    .replace(/<script[\s\S]*?<\/script>/gi, '')
    .replace(/<style[\s\S]*?<\/style>/gi, '')

  const cleaned = withoutScripts.replace(/<(\/?)([a-z0-9-]+)(?:\s[^>]*)?>/gi, (_full, slash, tag) => {
    const name = String(tag).toLowerCase()
    if (!ALLOWED_RICH_TAGS.has(name)) return ''
    return `<${slash}${name}>`
  })

  const trimmed = cleaned.trim()
  return trimmed.length > 0 ? trimmed : '<div><br></div>'
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;')
}

function plainTextToRichHtml(text: string): string {
  const cleaned = stripMarkdownSyntax(text).replace(/\r/g, '').trim()
  if (!cleaned) return '<div><br></div>'
  return cleaned
    .split('\n')
    .map((line) => (line.trim().length > 0 ? `<div>${escapeHtml(line)}</div>` : '<div><br></div>'))
    .join('')
}

function normalizeStoredRichText(text: string): string {
  if (isLikelyHtml(text)) return sanitizeRichHtml(text)
  return plainTextToRichHtml(text)
}

function richTextToPlainText(text: string): string {
  const raw = isLikelyHtml(text)
    ? text
      .replace(/<br\s*\/?\s*>/gi, '\n')
      .replace(/<\/(div|p|li|h[1-6])>/gi, '\n')
      .replace(/<li>/gi, '- ')
      .replace(/<[^>]+>/g, '')
    : text
  const decoded = decodeHtmlEntities(raw)
  return stripMarkdownSyntax(decoded)
    .replace(/\r/g, '')
    .replace(/\u00a0/g, ' ')
    .replace(/[ \t]+\n/g, '\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

function noteTitle(text: string): string {
  const firstLine = richTextToPlainText(text).split('\n')[0] ?? ''
  const trimmed = firstLine.trim()
  return trimmed.length > 0 ? trimmed.slice(0, 100) : '(untitled note)'
}

function noteSummary(text: string): string {
  const lines = richTextToPlainText(text).split('\n')
  const body = lines
    .slice(1)
    .join(' ')
    .replace(/\s+/g, ' ')
    .trim()
  if (body.length > 0) return body.slice(0, 180)
  return 'No body yet'
}

function placeCaretAtEnd(element: HTMLElement): void {
  const range = document.createRange()
  range.selectNodeContents(element)
  range.collapse(false)
  const selection = window.getSelection()
  if (!selection) return
  selection.removeAllRanges()
  selection.addRange(range)
}

export default function NotesView({
  onBack,
  initialSelectedNoteId = null,
}: {
  onBack: () => void
  initialSelectedNoteId?: number | null
}): JSX.Element {
  const rootRef = useRef<HTMLDivElement>(null)
  const searchRef = useRef<HTMLInputElement>(null)
  const editorRef = useRef<HTMLDivElement>(null)
  const saveOkTimerRef = useRef<number | null>(null)
  const loadedNoteIdRef = useRef<number | null>(null)

  const [notes, setNotes] = useState<QuickNoteEntry[]>([])
  const [selectedId, setSelectedId] = useState<number | null>(null)
  const [query, setQuery] = useState('')
  const [draftHtml, setDraftHtml] = useState('<div><br></div>')
  const [isEditorFocused, setIsEditorFocused] = useState(false)
  const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved' | 'error'>('idle')
  const [msg, setMsg] = useState<{ tone: 'success' | 'error'; text: string } | null>(null)

  const reload = useCallback(async (): Promise<QuickNoteEntry[]> => {
    const items = await window.tezbar.listQuickNotes()
    setNotes(items)
    return items
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  useEffect(() => {
    if (typeof initialSelectedNoteId !== 'number') return
    setSelectedId(initialSelectedNoteId)
  }, [initialSelectedNoteId])

  useEffect(() => {
    rootRef.current?.focus()
  }, [])

  useEffect(() => {
    return () => {
      if (saveOkTimerRef.current !== null) {
        window.clearTimeout(saveOkTimerRef.current)
      }
    }
  }, [])

  const flash = useCallback((tone: 'success' | 'error', text: string): void => {
    setMsg({ tone, text })
    window.setTimeout(() => setMsg(null), 2200)
  }, [])

  useEffect(() => {
    const onEsc = (e: KeyboardEvent): void => {
      if (e.key !== 'Escape') return
      e.preventDefault()
      e.stopPropagation()
      if (query.length > 0 && document.activeElement === searchRef.current) {
        setQuery('')
        return
      }
      onBack()
    }
    window.addEventListener('keydown', onEsc, true)
    return () => window.removeEventListener('keydown', onEsc, true)
  }, [onBack, query])

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    const sorted = [...notes].sort((a, b) => b.updatedAt - a.updatedAt)
    if (!q) return sorted
    return sorted.filter((note) => {
      const title = noteTitle(note.text).toLowerCase()
      return title.includes(q) || note.text.toLowerCase().includes(q)
    })
  }, [notes, query])

  const currentNote = useMemo(() => {
    if (filtered.length === 0) return null
    if (selectedId === null) return filtered[0] ?? null
    return filtered.find((note) => note.createdAt === selectedId) ?? filtered[0] ?? null
  }, [filtered, selectedId])

  const selectedIndex = useMemo(() => {
    if (!currentNote) return -1
    return filtered.findIndex((note) => note.createdAt === currentNote.createdAt)
  }, [currentNote, filtered])

  useEffect(() => {
    if (filtered.length === 0) return
    if (currentNote && currentNote.createdAt !== selectedId) {
      setSelectedId(currentNote.createdAt)
    }
  }, [currentNote, filtered.length, selectedId])

  useEffect(() => {
    const nextId = currentNote?.createdAt ?? null
    if (loadedNoteIdRef.current === nextId) return
    loadedNoteIdRef.current = nextId
    const normalized = normalizeStoredRichText(currentNote?.text ?? '')
    setDraftHtml(normalized)
    requestAnimationFrame(() => {
      const editor = editorRef.current
      if (!editor) return
      editor.innerHTML = normalized
    })
    setSaveState('idle')
  }, [currentNote])

  useEffect(() => {
    if (!currentNote) return
    const persisted = normalizeStoredRichText(currentNote.text)
    const current = sanitizeRichHtml(draftHtml)
    if (current === persisted) {
      if (saveState === 'saving') setSaveState('idle')
      return
    }

    const noteId = currentNote.createdAt
    const nextText = current
    setSaveState('saving')

    const timer = window.setTimeout(() => {
      void window.tezbar.updateQuickNote(noteId, nextText).then((ok) => {
        if (!ok) {
          setSaveState('error')
          return
        }

        const now = Date.now()
        setNotes((prev) =>
          prev.map((note) =>
            note.createdAt === noteId
              ? {
                ...note,
                text: nextText,
                updatedAt: now,
              }
              : note,
          ),
        )

        setSaveState('saved')
        if (saveOkTimerRef.current !== null) {
          window.clearTimeout(saveOkTimerRef.current)
        }
        saveOkTimerRef.current = window.setTimeout(() => {
          setSaveState((s) => (s === 'saved' ? 'idle' : s))
        }, 1200)
      })
    }, 260)

    return () => window.clearTimeout(timer)
  }, [currentNote, draftHtml, saveState])

  const createNote = useCallback(async (): Promise<void> => {
    const entry = await window.tezbar.appendQuickNote('<div>Untitled note</div>')
    if (!entry) {
      flash('error', 'Could not create note')
      return
    }
    setNotes((prev) => [entry, ...prev])
    setQuery('')
    setSelectedId(entry.createdAt)
    const normalized = normalizeStoredRichText(entry.text)
    setDraftHtml(normalized)
    setSaveState('idle')

    requestAnimationFrame(() => {
      const editor = editorRef.current
      if (!editor) return
      editor.innerHTML = normalized
      editor.focus()
      placeCaretAtEnd(editor)
    })
  }, [flash])

  const deleteSelectedNote = useCallback(async (): Promise<void> => {
    if (!currentNote) return
    const ok = await window.tezbar.deleteQuickNote(currentNote.createdAt)
    if (!ok) {
      flash('error', 'Could not delete note')
      return
    }
    setNotes((prev) => prev.filter((note) => note.createdAt !== currentNote.createdAt))
    flash('success', 'Note deleted')
  }, [currentNote, flash])

  const copySelectedNote = useCallback(async (): Promise<void> => {
    if (!currentNote) {
      flash('error', 'No note selected')
      return
    }

    const plain = richTextToPlainText(draftHtml)
    if (!plain) {
      flash('error', 'Note is empty')
      return
    }

    try {
      const result = await window.tezbar.executeSearchAction({ type: 'copy-text', text: plain })
      if (!result.ok) {
        flash('error', result.message || 'Could not copy note')
        return
      }
      flash('success', 'Copied note')
    } catch {
      flash('error', 'Could not copy note')
    }
  }, [currentNote, draftHtml, flash])

  const selectByIndex = useCallback(
    (index: number): void => {
      const note = filtered[index]
      if (!note) return
      setSelectedId(note.createdAt)
    },
    [filtered],
  )

  const syncDraftFromEditor = useCallback((): void => {
    const editor = editorRef.current
    if (!editor) return
    const cleaned = sanitizeRichHtml(editor.innerHTML)
    if (editor.innerHTML !== cleaned) editor.innerHTML = cleaned
    setDraftHtml(cleaned)
    if (saveState === 'error') setSaveState('idle')
  }, [saveState])

  const applyFormat = useCallback(
    (command: 'bold' | 'italic' | 'underline'): void => {
      const editor = editorRef.current
      if (!editor) return
      editor.focus()
      document.execCommand(command)
      syncDraftFromEditor()
    },
    [syncDraftFromEditor],
  )

  const onRootKeyDown = (e: React.KeyboardEvent<HTMLDivElement>): void => {
    const inEditor =
      e.target instanceof HTMLElement &&
      (e.target === editorRef.current || e.target.closest('[data-note-editor="true"]') !== null)
    const hasCommandMod = e.metaKey || e.ctrlKey

    if (hasCommandMod && e.key.toLowerCase() === 'n') {
      e.preventDefault()
      void createNote()
      return
    }

    if (hasCommandMod && e.key.toLowerCase() === 'd' && !inEditor) {
      e.preventDefault()
      void deleteSelectedNote()
      return
    }

    if (hasCommandMod && e.key === 'Enter') {
      e.preventDefault()
      void copySelectedNote()
      return
    }

    if (inEditor) return
    if (filtered.length === 0) return

    if (e.key === 'ArrowDown') {
      e.preventDefault()
      const next = Math.min((selectedIndex < 0 ? 0 : selectedIndex) + 1, filtered.length - 1)
      selectByIndex(next)
      return
    }

    if (e.key === 'ArrowUp') {
      e.preventDefault()
      const next = Math.max((selectedIndex < 0 ? 0 : selectedIndex) - 1, 0)
      selectByIndex(next)
      return
    }

    if (e.key === 'Enter') {
      e.preventDefault()
      editorRef.current?.focus()
    }
  }

  const onEditorKeyDown = (e: React.KeyboardEvent<HTMLDivElement>): void => {
    const hasCommandMod = e.metaKey || e.ctrlKey
    if (!hasCommandMod) return

    if (!e.shiftKey && e.key.toLowerCase() === 'b') {
      e.preventDefault()
      applyFormat('bold')
      return
    }

    if (!e.shiftKey && e.key.toLowerCase() === 'i') {
      e.preventDefault()
      applyFormat('italic')
      return
    }

    if (!e.shiftKey && e.key.toLowerCase() === 'u') {
      e.preventDefault()
      applyFormat('underline')
    }
  }

  const onEditorInput = useCallback((e: React.FormEvent<HTMLDivElement>): void => {
    const next = sanitizeRichHtml(e.currentTarget.innerHTML)
    if (e.currentTarget.innerHTML !== next) e.currentTarget.innerHTML = next
    setDraftHtml(next)
    if (saveState === 'error') setSaveState('idle')
  }, [saveState])

  const onEditorPaste = useCallback((): void => {
    window.setTimeout(() => {
      syncDraftFromEditor()
    }, 0)
  }, [syncDraftFromEditor])

  const isEditorEmpty = useMemo(() => richTextToPlainText(draftHtml).length === 0, [draftHtml])

  const saveLabel =
    saveState === 'saving'
      ? 'Saving...'
      : saveState === 'saved'
        ? 'Saved'
        : saveState === 'error'
          ? 'Save failed'
          : currentNote
            ? `Updated ${formatDate(currentNote.updatedAt)}`
            : null

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      role="application"
      aria-label="Quick Notes"
      onKeyDown={onRootKeyDown}
      className="flex h-full min-h-0 w-full flex-col gap-2 outline-none animate-tezbar-scale-in"
    >
      <div className="glass-card shrink-0 px-4 py-3 animate-tezbar-scale-in">
        <ViewHeader
          title="Quick Notes"
          onBack={onBack}
          trailing={
            <>
              <Button variant="quiet" onClick={() => void createNote()}>
                New
              </Button>
              <Button
                variant="quiet"
                onClick={() => void deleteSelectedNote()}
                disabled={!currentNote}
              >
                Delete
              </Button>
            </>
          }
        />

        <div className="mt-2 flex items-center gap-2">
          <input
            ref={searchRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search notes..."
            className="h-8 w-full rounded-tezbar-chip border border-white/10 bg-white/[0.04] px-2.5 text-[12px] text-ink-1 placeholder:text-ink-4 outline-none transition focus:border-white/20 focus:bg-white/[0.06]"
          />
        </div>
      </div>

      <div className="glass-card flex min-h-0 flex-1 flex-col overflow-hidden px-2 py-2 animate-tezbar-scale-in">
        <section className="flex min-h-0 flex-1 gap-3">
          <div className="flex min-h-0 w-[44%] max-w-[340px] flex-col overflow-hidden">
            {filtered.length === 0 ? (
              <div className="grid flex-1 place-items-center px-4 text-center">
                <div className="max-w-[240px]">
                  <p className="text-[12px] font-medium text-ink-2">No notes found</p>
                  <p className="mt-1 text-[11px] text-ink-4">
                    {notes.length === 0 ? 'Press Cmd+N to create your first note.' : 'Try a different search.'}
                  </p>
                </div>
              </div>
            ) : (
              <GlideList
                selectedIndex={selectedIndex}
                itemCount={filtered.length}
                className="min-h-0 flex-1 overflow-y-auto"
                listClassName="flex flex-col gap-0.5 py-0.5"
              >
                {filtered.map((note) => {
                  const selected = note.createdAt === currentNote?.createdAt
                  return (
                    <li key={note.createdAt} className="relative z-[1]">
                      <button
                        type="button"
                        onMouseEnter={() => setSelectedId(note.createdAt)}
                        onClick={() => setSelectedId(note.createdAt)}
                        className="flex w-full items-center justify-between gap-3 rounded-tezbar-row px-3 py-2 text-left transition"
                      >
                        <span className="min-w-0 flex-1">
                          <span className={`block truncate text-[13px] font-medium ${selected ? 'text-ink-1' : 'text-ink-2'}`}>
                            {noteTitle(note.text)}
                          </span>
                          <span className="mt-0.5 block truncate text-[11px] text-ink-3">{noteSummary(note.text)}</span>
                        </span>
                        <span className="shrink-0 text-[10px] text-ink-4">{formatDate(note.updatedAt)}</span>
                      </button>
                    </li>
                  )
                })}
              </GlideList>
            )}
          </div>

          <div className="min-h-0 flex-1 overflow-hidden rounded-tezbar-row border border-white/[0.06] bg-white/[0.02]">
            {currentNote ? (
              <div className="flex h-full min-h-0 flex-col">
                <div className="flex items-center justify-end gap-3 border-b border-white/10 px-3 py-2 text-[10.5px] text-ink-4">
                  <span
                    className={
                      saveState === 'error'
                        ? 'text-rose-300'
                        : saveState === 'saving'
                          ? 'text-amber-200'
                          : 'text-ink-4'
                    }
                  >
                    {saveLabel}
                  </span>
                </div>
                <div className="relative min-h-0 flex-1 overflow-hidden p-3">
                  {isEditorEmpty && !isEditorFocused ? (
                    <p className="pointer-events-none absolute left-6 top-6 text-[12px] text-ink-4">
                      Title on first line. Use Cmd+B / Cmd+I / Cmd+U for formatting.
                    </p>
                  ) : null}
                  <div
                    ref={editorRef}
                    data-note-editor="true"
                    contentEditable
                    suppressContentEditableWarning
                    spellCheck={false}
                    onInput={onEditorInput}
                    onKeyDown={onEditorKeyDown}
                    onPaste={onEditorPaste}
                    onFocus={() => setIsEditorFocused(true)}
                    onBlur={() => setIsEditorFocused(false)}
                    className="h-full min-h-[220px] w-full overflow-y-auto rounded-tezbar-row border border-white/10 bg-black/25 p-3 text-[13px] leading-[1.6] text-ink-1 outline-none transition focus:border-white/20"
                  />
                </div>
              </div>
            ) : (
              <div className="grid h-full place-items-center px-6 text-center">
                <p className="text-[12px] text-ink-4">Select a note to edit.</p>
              </div>
            )}
          </div>
        </section>

        {msg ? (
          <div className="mt-2 shrink-0">
            <Message tone={msg.tone}>{msg.text}</Message>
          </div>
        ) : null}
      </div>

      <div className="glass-card flex shrink-0 items-center justify-between gap-3 px-4 py-2 animate-tezbar-scale-in">
        <span className="text-[10.5px] uppercase tracking-[0.14em] text-ink-4">
          {filtered.length} note{filtered.length === 1 ? '' : 's'}
        </span>
        <HintBar>
          <Hint label="Create" keys={<><Kbd>Cmd</Kbd><Kbd>N</Kbd></>} />
          <Hint label="Copy note" keys={<><Kbd>Cmd</Kbd><Kbd>↵</Kbd></>} />
          <Hint label="Bold" keys={<><Kbd>Cmd</Kbd><Kbd>B</Kbd></>} />
          <Hint label="Italic" keys={<><Kbd>Cmd</Kbd><Kbd>I</Kbd></>} />
          <Hint label="Delete" keys={<><Kbd>Cmd</Kbd><Kbd>D</Kbd></>} />
          <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
        </HintBar>
      </div>
    </div>
  )
}
