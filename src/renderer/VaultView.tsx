import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from 'react'
import {
  TEZBAR_NEW_VAULT_ITEM_EVENT,
  type VaultItem,
  type VaultItemHeader,
  type VaultKind,
  type VaultStatus,
} from '../shared/vault'
import {
  CommandSurfaceAction,
  CommandSurfaceBackButton,
  CommandSurfaceFooter,
  CommandSurfaceHeader,
  CommandSurfaceIcon,
  CommandSurfaceInput,
  CommandSurfaceStatus,
  CommandSurfaceTitle,
} from './ui/CommandSurface'
import { GlideList } from './ui/GlideList'
import { Button, Hint, Kbd, cx } from './ui/primitives'
import { VAULT_KIND_LABELS, detectSecret } from './vaultDetect'

type Draft = {
  id?: string
  value: string
  name: string
  /** False while the name still follows the detector's suggestion. */
  nameTouched: boolean
  note: string
  showNote: boolean
}

const EMPTY_DRAFT: Draft = { value: '', name: '', nameTouched: false, note: '', showNote: false }
const MASK = '•'.repeat(18)
const fieldClass =
  'w-full rounded-tezbar-row border border-white/10 bg-white/[0.03] px-3 py-2 text-ink-1 outline-none transition placeholder:text-ink-4 focus:border-white/20 focus:bg-white/[0.05]'

function relativeTime(ts: number, now = Date.now()): string {
  const minutes = Math.round((now - ts) / 60_000)
  if (minutes < 1) return 'just now'
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.round(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.round(hours / 24)
  if (days < 7) return `${days}d ago`
  return new Date(ts).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' })
}

function fullDate(ts: number): string {
  return new Date(ts).toLocaleString(undefined, {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  })
}

function KindIcon({ kind }: { kind: VaultKind }): JSX.Element {
  const paths: Record<VaultKind, ReactNode> = {
    'api-key': (
      <>
        <circle cx="5" cy="9" r="2.5" />
        <path d="M7 7.5 12.5 2M10.5 4l1.5 1.5M9 5.5l1.2 1.2" />
      </>
    ),
    token: (
      <>
        <rect x="1.5" y="4" width="11" height="6" rx="1.5" />
        <path d="M4.5 7h5" />
      </>
    ),
    'ssh-key': (
      <>
        <rect x="1.5" y="2.5" width="11" height="9" rx="1.5" />
        <path d="m4 6 1.8 1.5L4 9M7.5 9h2.5" />
      </>
    ),
    server: (
      <>
        <rect x="1.5" y="2" width="11" height="4" rx="1" />
        <rect x="1.5" y="8" width="11" height="4" rx="1" />
        <path d="M4 4h.01M4 10h.01" />
      </>
    ),
    password: (
      <>
        <rect x="2.5" y="6" width="9" height="6" rx="1.5" />
        <path d="M4.5 6V4.5a2.5 2.5 0 0 1 5 0V6" />
      </>
    ),
    note: <path d="M3 3.5h8M3 7h8M3 10.5h5" />,
  }
  return (
    <span
      aria-hidden
      className="grid h-7 w-7 shrink-0 place-items-center rounded-tezbar-chip border border-white/10 bg-white/[0.04] text-ink-3"
    >
      <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round">
        {paths[kind]}
      </svg>
    </span>
  )
}

function LockGlyph(): JSX.Element {
  return (
    <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" aria-hidden className="text-ink-3">
      <rect x="4" y="9" width="12" height="8" rx="2" />
      <path d="M7 9V6.5a3 3 0 0 1 6 0V9" />
    </svg>
  )
}

function Centered({ children }: { children: ReactNode }): JSX.Element {
  return <div className="flex h-full flex-col items-center justify-center gap-1.5 px-6 text-center">{children}</div>
}

/** Grows a textarea with its content up to a cap, then it scrolls. */
function autoGrow(element: HTMLTextAreaElement | null, max = 220): void {
  if (!element) return
  element.style.height = 'auto'
  element.style.height = `${Math.min(element.scrollHeight, max)}px`
}

export default function VaultView({ onClose }: { onClose: () => void }): JSX.Element {
  const [status, setStatus] = useState<VaultStatus | null>(null)
  const [unlocking, setUnlocking] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [items, setItems] = useState<VaultItemHeader[]>([])
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState(0)
  const [detail, setDetail] = useState<VaultItem | null>(null)
  const [revealed, setRevealed] = useState(false)
  const [draft, setDraft] = useState<Draft | null>(null)
  const [pendingDelete, setPendingDelete] = useState<string | null>(null)
  const [flash, setFlash] = useState<string | null>(null)

  const rootRef = useRef<HTMLDivElement>(null)
  const searchRef = useRef<HTMLInputElement>(null)
  const valueRef = useRef<HTMLTextAreaElement>(null)
  const nameRef = useRef<HTMLInputElement>(null)
  const autoUnlockRef = useRef(false)

  const unlocked = Boolean(status?.supported && status.unlocked)

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return items
    return items.filter(
      (item) => item.name.toLowerCase().includes(q) || VAULT_KIND_LABELS[item.kind].toLowerCase().includes(q)
    )
  }, [items, query])
  const current = filtered[Math.min(selected, filtered.length - 1)] ?? null

  const refresh = useCallback(async (): Promise<VaultStatus | null> => {
    try {
      const next = await window.tezbar.vaultStatus()
      setStatus(next)
      if (next.supported && next.unlocked) setItems(await window.tezbar.vaultList())
      return next
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      return null
    }
  }, [])

  const unlock = useCallback(async () => {
    if (unlocking) return
    setUnlocking(true)
    setError(null)
    try {
      const result = await window.tezbar.vaultUnlock()
      if (result.ok) await refresh()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setUnlocking(false)
    }
  }, [refresh, unlocking])

  // Opening a locked vault asks for Touch ID / the Mac password right away.
  useEffect(() => {
    void refresh().then((next) => {
      if (next?.supported && !next.unlocked && !autoUnlockRef.current) {
        autoUnlockRef.current = true
        void unlock()
      }
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // The secret itself is only fetched for the selected item.
  useEffect(() => {
    setRevealed(false)
    setPendingDelete(null)
    if (!unlocked || !current) {
      setDetail(null)
      return
    }
    let cancelled = false
    void window.tezbar
      .vaultGet(current.id)
      .then((item) => {
        if (!cancelled) setDetail(item)
      })
      .catch(() => {
        if (!cancelled) setDetail(null)
      })
    return () => {
      cancelled = true
    }
  }, [current?.id, current?.updatedAt, unlocked]) // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (!flash) return
    const timer = window.setTimeout(() => setFlash(null), 2200)
    return () => window.clearTimeout(timer)
  }, [flash])

  useEffect(() => {
    if (!pendingDelete) return
    const timer = window.setTimeout(() => setPendingDelete(null), 3000)
    return () => window.clearTimeout(timer)
  }, [pendingDelete])

  const startNew = useCallback(() => {
    if (!unlocked) return
    setDraft((existing) => existing ?? { ...EMPTY_DRAFT })
  }, [unlocked])

  // App routes ⌘N here while the Vault is open.
  useEffect(() => {
    const onNew = (): void => startNew()
    window.addEventListener(TEZBAR_NEW_VAULT_ITEM_EVENT, onNew)
    return () => window.removeEventListener(TEZBAR_NEW_VAULT_ITEM_EVENT, onNew)
  }, [startNew])

  useEffect(() => {
    if (draft && !draft.id) valueRef.current?.focus()
    else if (!draft && unlocked) searchRef.current?.focus()
    autoGrow(valueRef.current)
  }, [draft === null, draft?.id, unlocked]) // eslint-disable-line react-hooks/exhaustive-deps

  const startEdit = (): void => {
    if (!detail) return
    setDraft({
      id: detail.id,
      value: detail.value,
      name: detail.name,
      nameTouched: true,
      note: detail.note ?? '',
      showNote: Boolean(detail.note),
    })
  }

  const detected = useMemo(() => detectSecret(draft?.value ?? ''), [draft?.value])
  const draftName = draft ? (draft.nameTouched ? draft.name : detected.suggestedName) : ''

  const save = async (): Promise<void> => {
    if (!draft || !draft.value.trim()) return
    try {
      const saved = await window.tezbar.vaultSave({
        id: draft.id,
        name: draftName.trim(),
        value: draft.value,
        kind: detected.kind,
        note: draft.note,
      })
      setDraft(null)
      setQuery('')
      const list = await window.tezbar.vaultList()
      setItems(list)
      setSelected(Math.max(0, list.findIndex((item) => item.id === saved.id)))
      setFlash('Saved')
    } catch (err) {
      setFlash(err instanceof Error ? err.message : 'Could not save')
    }
  }

  const copy = async (): Promise<void> => {
    if (!current) return
    try {
      await window.tezbar.vaultCopy(current.id)
      setFlash('Copied · clears in 30 s')
    } catch (err) {
      setFlash(err instanceof Error ? err.message : 'Could not copy')
    }
  }

  const remove = async (): Promise<void> => {
    if (!current) return
    if (pendingDelete !== current.id) {
      setPendingDelete(current.id)
      return
    }
    setPendingDelete(null)
    await window.tezbar.vaultDelete(current.id)
    const list = await window.tezbar.vaultList()
    setItems(list)
    setSelected((index) => Math.min(index, Math.max(0, list.length - 1)))
    setFlash('Deleted')
  }

  const lock = async (): Promise<void> => {
    await window.tezbar.vaultLock()
    setDraft(null)
    setDetail(null)
    setItems([])
    autoUnlockRef.current = true
    await refresh()
    rootRef.current?.focus()
  }

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>): void => {
    const mod = event.metaKey || event.ctrlKey
    const key = event.key.toLowerCase()

    if (event.key === 'Escape') {
      event.preventDefault()
      // Stop App's window-level Escape from also leaving the Vault.
      event.stopPropagation()
      if (draft) setDraft(null)
      else onClose()
      return
    }
    if (!unlocked) {
      if (event.key === 'Enter' && status?.supported) {
        event.preventDefault()
        void unlock()
      }
      return
    }
    if (draft) {
      if (mod && event.key === 'Enter') {
        event.preventDefault()
        void save()
      }
      return
    }
    if (mod && key === 'n') {
      event.preventDefault()
      startNew()
    } else if (mod && key === 'e') {
      event.preventDefault()
      startEdit()
    } else if (mod && key === 'l') {
      event.preventDefault()
      void lock()
    } else if (mod && event.key === 'Backspace' && !(query && event.target === searchRef.current)) {
      event.preventDefault()
      void remove()
    } else if (event.key === 'ArrowDown') {
      event.preventDefault()
      setSelected((index) => Math.min(index + 1, Math.max(0, filtered.length - 1)))
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      setSelected((index) => Math.max(index - 1, 0))
    } else if (event.key === 'Enter') {
      event.preventDefault()
      void copy()
    }
  }

  const header = (
    <CommandSurfaceHeader>
      <CommandSurfaceBackButton onClick={draft ? () => setDraft(null) : onClose} />
      {unlocked && !draft ? (
        <>
          <CommandSurfaceInput
            ref={searchRef}
            value={query}
            onChange={(event) => {
              setQuery(event.target.value)
              setSelected(0)
            }}
            placeholder="Search vault…"
            aria-label="Search vault"
          />
          <CommandSurfaceAction title="New ⌘N" aria-label="New secret" onClick={startNew}>
            <CommandSurfaceIcon name="plus" />
          </CommandSurfaceAction>
        </>
      ) : (
        <CommandSurfaceTitle>{draft ? (draft.id ? 'Edit secret' : 'New secret') : 'Vault'}</CommandSurfaceTitle>
      )}
    </CommandSurfaceHeader>
  )

  let body: ReactNode
  if (!status) {
    body = <Centered>{error ? <p className="text-[12px] text-ink-3">{error}</p> : null}</Centered>
  } else if (!status.supported) {
    body = (
      <Centered>
        <p className="text-[12.5px] text-ink-3">Vault is available on macOS for now.</p>
      </Centered>
    )
  } else if (!status.unlocked) {
    body = (
      <Centered>
        <LockGlyph />
        <p className="mt-1 text-[13px] text-ink-2">Vault is locked</p>
        <Button variant="primary" className="mt-2" onClick={() => void unlock()} disabled={unlocking}>
          {unlocking ? 'Waiting…' : 'Unlock'}
        </Button>
        <p className="mt-1 text-[11px] text-ink-4">{error ?? 'Touch ID or your Mac password'}</p>
      </Centered>
    )
  } else {
    body = (
      <section className="flex min-h-0 flex-1 gap-3">
        <div className="flex min-h-0 w-[44%] max-w-[340px] flex-col overflow-hidden">
          {filtered.length === 0 ? (
            <Centered>
              {items.length === 0 ? (
                <>
                  <p className="text-[12.5px] text-ink-3">No secrets yet</p>
                  <p className="text-[11px] text-ink-4">Press ⌘N and paste</p>
                </>
              ) : (
                <p className="text-[12.5px] text-ink-3">Nothing matches “{query.trim()}”</p>
              )}
            </Centered>
          ) : (
            <div className="min-h-0 flex-1 overflow-y-auto pr-0.5">
              <GlideList selectedIndex={selected} itemCount={filtered.length} className="flex flex-col gap-0.5 py-0.5">
                {filtered.map((item, index) => (
                  <li key={item.id} className="relative z-[1]">
                    <button
                      type="button"
                      onMouseEnter={() => !draft && setSelected(index)}
                      onClick={() => {
                        setDraft(null)
                        setSelected(index)
                      }}
                      className="flex w-full items-center gap-2 rounded-tezbar-row px-2 py-1.5 text-left transition"
                    >
                      <KindIcon kind={item.kind} />
                      <div className="min-w-0 flex-1">
                        <p className={cx('truncate text-[12.5px]', index === selected ? 'text-ink-1' : 'text-ink-2')}>
                          {item.name}
                        </p>
                        <p className="mt-0.5 truncate text-[10.5px] text-ink-4">
                          {VAULT_KIND_LABELS[item.kind]} · {relativeTime(item.updatedAt)}
                        </p>
                      </div>
                    </button>
                  </li>
                ))}
              </GlideList>
            </div>
          )}
        </div>

        <div className="sheet-above-bar min-h-0 flex-1 overflow-y-auto border-l border-white/[0.06] p-4">
          {draft ? (
            <div className="flex flex-col gap-3">
              <label className="flex flex-col gap-1.5">
                <span className="text-[11px] text-ink-4">Value</span>
                <textarea
                  ref={valueRef}
                  value={draft.value}
                  rows={2}
                  spellCheck={false}
                  autoComplete="off"
                  placeholder="Paste a password, API key, SSH key…"
                  onChange={(event) => {
                    setDraft({ ...draft, value: event.target.value })
                    autoGrow(event.target)
                  }}
                  className={cx(fieldClass, 'resize-none font-mono text-[12.5px] leading-relaxed')}
                />
              </label>
              <label className="flex flex-col gap-1.5">
                <span className="flex items-center justify-between text-[11px] text-ink-4">
                  <span>Name</span>
                  {draft.value.trim() ? <span>{VAULT_KIND_LABELS[detected.kind]}</span> : null}
                </span>
                <input
                  ref={nameRef}
                  value={draftName}
                  placeholder="Name"
                  onChange={(event) => setDraft({ ...draft, name: event.target.value, nameTouched: true })}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' && !event.metaKey && !event.ctrlKey) {
                      event.preventDefault()
                      void save()
                    }
                  }}
                  className={cx(fieldClass, 'text-[13px]')}
                />
              </label>
              {draft.showNote ? (
                <label className="flex flex-col gap-1.5">
                  <span className="text-[11px] text-ink-4">Note</span>
                  <textarea
                    value={draft.note}
                    rows={2}
                    placeholder="Host, user, where it's used…"
                    onChange={(event) => setDraft({ ...draft, note: event.target.value })}
                    className={cx(fieldClass, 'resize-none text-[12.5px]')}
                  />
                </label>
              ) : (
                <button
                  type="button"
                  onClick={() => setDraft({ ...draft, showNote: true })}
                  className="self-start text-[11.5px] text-ink-3 transition hover:text-ink-1"
                >
                  Add note
                </button>
              )}
              <div className="flex items-center gap-2 pt-1">
                <Button variant="primary" onClick={() => void save()} disabled={!draft.value.trim()}>
                  Save
                </Button>
                <Button variant="quiet" onClick={() => setDraft(null)}>
                  Cancel
                </Button>
              </div>
            </div>
          ) : detail ? (
            <div className="flex flex-col gap-4">
              <div className="flex items-center gap-2.5">
                <KindIcon kind={detail.kind} />
                <div className="min-w-0">
                  <h2 className="truncate text-[14px] font-medium text-ink-1">{detail.name}</h2>
                  <p className="text-[11px] text-ink-4">{VAULT_KIND_LABELS[detail.kind]}</p>
                </div>
              </div>
              <div className="flex items-start gap-2 rounded-tezbar-row border border-white/10 bg-white/[0.03] px-3 py-2">
                <pre
                  className={cx(
                    'min-w-0 flex-1 whitespace-pre-wrap break-all font-mono text-[12.5px] leading-relaxed',
                    revealed ? 'max-h-[180px] select-text overflow-y-auto text-ink-1' : 'select-none text-ink-3'
                  )}
                >
                  {revealed ? detail.value : MASK}
                </pre>
                <div className="flex shrink-0 items-center gap-1">
                  <Button variant="quiet" onClick={() => setRevealed((value) => !value)}>
                    {revealed ? 'Hide' : 'Reveal'}
                  </Button>
                  <Button variant="quiet" onClick={() => void copy()}>
                    Copy
                  </Button>
                </div>
              </div>
              {detail.note ? (
                <p className="whitespace-pre-wrap text-[12.5px] leading-relaxed text-ink-2">{detail.note}</p>
              ) : null}
              <p className="text-[11px] text-ink-4">
                Added {fullDate(detail.createdAt)}
                {detail.updatedAt !== detail.createdAt ? ` · Updated ${fullDate(detail.updatedAt)}` : ''}
              </p>
            </div>
          ) : null}
        </div>
      </section>
    )
  }

  const hints = !unlocked ? (
    <>
      {status?.supported ? <Hint label="Unlock" keys={<Kbd>↵</Kbd>} /> : null}
      <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
    </>
  ) : draft ? (
    <>
      <Hint label="Save" keys={<><Kbd>⌘</Kbd><Kbd>↵</Kbd></>} />
      <Hint label="Cancel" keys={<Kbd>Esc</Kbd>} />
    </>
  ) : (
    <>
      {current ? <Hint label="Copy" keys={<Kbd>↵</Kbd>} /> : null}
      <Hint label="New" keys={<><Kbd>⌘</Kbd><Kbd>N</Kbd></>} />
      {current ? <Hint label="Edit" keys={<><Kbd>⌘</Kbd><Kbd>E</Kbd></>} /> : null}
      {current ? (
        <Hint
          label={pendingDelete === current.id ? 'Press again to delete' : 'Delete'}
          keys={<><Kbd>⌘</Kbd><Kbd>⌫</Kbd></>}
        />
      ) : null}
      <Hint label="Lock" keys={<><Kbd>⌘</Kbd><Kbd>L</Kbd></>} />
      <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
    </>
  )

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      role="application"
      aria-label="Vault"
      data-vault-panel
      onKeyDown={onKeyDown}
      className="view-sheet flex h-full min-h-0 w-full flex-col gap-2 outline-none animate-tezbar-scale-in"
    >
      {header}
      <div className="command-surface-body no-drag flex min-h-0 flex-1 flex-col overflow-hidden px-2 py-2">{body}</div>
      <CommandSurfaceFooter
        status={
          <CommandSurfaceStatus title="Vault">
            {flash ?? (unlocked ? `${items.length} item${items.length === 1 ? '' : 's'}` : status?.supported ? 'Locked' : undefined)}
          </CommandSurfaceStatus>
        }
      >
        {hints}
      </CommandSurfaceFooter>
    </div>
  )
}
