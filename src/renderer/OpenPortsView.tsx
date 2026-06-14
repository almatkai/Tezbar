import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { NamedPortEntry } from '../shared/portManager'
import type { OpenPortProcess } from '../shared/search'
import {
  Button,
  Hint,
  HintBar,
  Kbd,
  Message,
  TextField,
  ViewHeader,
} from './ui/primitives'
import { GlideList } from './ui/GlideList'

type Panel = 'listen' | 'named'

export default function OpenPortsView({
  onBack,
  initialTab = 'listen',
}: {
  onBack: () => void
  initialTab?: Panel
}): JSX.Element {
  const rootRef = useRef<HTMLDivElement>(null)
  const [panel, setPanel] = useState<Panel>(initialTab)
  const [query, setQuery] = useState('')
  const [rows, setRows] = useState<OpenPortProcess[]>([])
  const [named, setNamed] = useState<NamedPortEntry[]>([])
  const [selected, setSelected] = useState(0)
  const [loading, setLoading] = useState(false)
  const [msg, setMsg] = useState<{ tone: 'success' | 'error'; text: string } | null>(null)

  const [newName, setNewName] = useState('')
  const [newPort, setNewPort] = useState('')

  const reloadPorts = useCallback(async () => {
    setLoading(true)
    try {
      const items = await window.tezbar.listOpenPorts()
      setRows(items)
    } finally {
      setLoading(false)
    }
  }, [])

  const reloadNamed = useCallback(async () => {
    const items = await window.tezbar.listNamedPorts()
    setNamed(items)
  }, [])

  useEffect(() => {
    void reloadPorts()
    void reloadNamed()
  }, [reloadPorts, reloadNamed])

  useEffect(() => {
    rootRef.current?.focus()
  }, [])

  useEffect(() => {
    const onEsc = (e: KeyboardEvent): void => {
      if (e.key !== 'Escape') return
      e.preventDefault()
      e.stopPropagation()
      onBack()
    }

    window.addEventListener('keydown', onEsc, true)
    return () => window.removeEventListener('keydown', onEsc, true)
  }, [onBack])

  const portToLabel = useMemo(() => {
    const m = new Map<number, string>()
    for (const entry of named) {
      if (!m.has(entry.port)) m.set(entry.port, entry.name)
    }
    return m
  }, [named])

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return rows

    return rows.filter((row) => {
      const processMatch = row.process.toLowerCase().includes(q)
      const userMatch = row.user.toLowerCase().includes(q)
      const pidMatch = row.pid.toLowerCase().includes(q)
      const portMatch = row.ports.some((port) => String(port).includes(q))
      const namedMatch = row.ports.some((port) => {
        const label = portToLabel.get(port)
        return label?.toLowerCase().includes(q)
      })
      return processMatch || userMatch || pidMatch || portMatch || namedMatch
    })
  }, [query, rows, portToLabel])

  useEffect(() => {
    if (filtered.length === 0) {
      setSelected(0)
      return
    }
    if (selected >= filtered.length) {
      setSelected(filtered.length - 1)
    }
  }, [filtered, selected])

  const killPort = useCallback(
    async (port: number) => {
      const result = await window.tezbar.executeSearchAction({
        type: 'run-extension-command',
        extensionId: 'raycast.port-manager',
        commandName: 'kill-listening-process',
        title: 'Kill Process Listening On',
        argumentValues: { port: String(port) },
      })

      setMsg({ tone: result.ok ? 'success' : 'error', text: result.message })
      if (result.ok) {
        void reloadPorts()
      }
    },
    [reloadPorts],
  )

  const addNamed = useCallback(async (): Promise<void> => {
    const portNum = Number(newPort.trim())
    if (!newName.trim() || !Number.isFinite(portNum) || portNum < 1 || portNum > 65535) {
      setMsg({ tone: 'error', text: 'Enter a label and a valid port (1–65535).' })
      return
    }
    const created = await window.tezbar.addNamedPort({ name: newName.trim(), port: Math.floor(portNum) })
    if (!created) {
      setMsg({ tone: 'error', text: 'Could not save named port.' })
      return
    }
    setNewName('')
    setNewPort('')
    setMsg({ tone: 'success', text: `Saved “${created.name}” → ${created.port}` })
    void reloadNamed()
  }, [newName, newPort, reloadNamed])

  const removeNamed = useCallback(
    async (id: string): Promise<void> => {
      const ok = await window.tezbar.removeNamedPort(id)
      if (!ok) {
        setMsg({ tone: 'error', text: 'Could not remove entry.' })
        return
      }
      void reloadNamed()
    },
    [reloadNamed],
  )

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>): void => {
    if (panel !== 'listen') return
    if (filtered.length === 0) return

    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setSelected((i) => Math.min(i + 1, filtered.length - 1))
      return
    }

    if (e.key === 'ArrowUp') {
      e.preventDefault()
      setSelected((i) => Math.max(i - 1, 0))
      return
    }

    if (e.key === 'Enter') {
      e.preventDefault()
      const row = filtered[selected]
      const firstPort = row?.ports[0]
      if (!row || firstPort === undefined) return
      void killPort(firstPort)
    }
  }

  const headerTitle = panel === 'listen' ? 'Open Ports' : 'Named Ports'

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      role="application"
      aria-label="Open Ports"
      onKeyDown={onKeyDown}
      className="flex h-full min-h-0 w-full flex-col gap-2 outline-none animate-tezbar-scale-in"
    >
      <div className="glass-card shrink-0 px-4 py-3 animate-tezbar-scale-in">
        <ViewHeader
          title={headerTitle}
          onBack={onBack}
          trailing={
            panel === 'listen' ? (
              <Button variant="ghost" onClick={() => void reloadPorts()} disabled={loading}>
                {loading ? 'Refreshing' : 'Refresh'}
              </Button>
            ) : null
          }
        />

        <div className="mt-2 flex gap-1">
          <button
            type="button"
            onClick={() => setPanel('listen')}
            className={
              panel === 'listen'
                ? 'rounded-tezbar-chip bg-white/[0.12] px-2.5 py-1 text-[11px] font-medium text-ink-1'
                : 'rounded-tezbar-chip px-2.5 py-1 text-[11px] text-ink-3 transition hover:bg-white/[0.06] hover:text-ink-1'
            }
          >
            Listening
          </button>
          <button
            type="button"
            onClick={() => setPanel('named')}
            className={
              panel === 'named'
                ? 'rounded-tezbar-chip bg-white/[0.12] px-2.5 py-1 text-[11px] font-medium text-ink-1'
                : 'rounded-tezbar-chip px-2.5 py-1 text-[11px] text-ink-3 transition hover:bg-white/[0.06] hover:text-ink-1'
            }
          >
            Named
          </button>
        </div>
      </div>

      {panel === 'listen' ? (
        <div className="glass-card flex min-h-0 flex-1 flex-col overflow-hidden px-4 py-3 animate-tezbar-scale-in">
          <TextField
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search by process, user, pid, port, or saved name"
          />
          <div className="hairline my-2" />

          <section className="min-h-0 flex-1 overflow-y-auto pr-0.5">
            {filtered.length === 0 ? (
              <div className="flex min-h-[120px] items-center justify-center">
                <p className="text-[12px] text-ink-3">No listening ports found.</p>
              </div>
            ) : (
              <GlideList
                selectedIndex={selected}
                itemCount={filtered.length}
                className="flex flex-col gap-0.5 py-0.5"
              >
                {filtered.map((row, i) => (
                  <li key={`${row.process}:${row.pid}:${row.user}`} className="relative z-[1]">
                    <div
                      className="flex items-center justify-between gap-3 rounded-tezbar-row px-3 py-2.5 transition"
                      onMouseEnter={() => setSelected(i)}
                    >
                      <div className="min-w-0">
                        <p className="truncate text-[13px] font-medium text-ink-1">{row.process}</p>
                        <p className="mt-0.5 truncate text-[11px] text-ink-3">
                          <span className="text-ink-4">{row.user}</span>
                          <span className="mx-1.5 text-ink-4">·</span>
                          <span className="font-mono tabular-nums text-ink-3">PID {row.pid}</span>
                        </p>
                      </div>
                      <div className="flex flex-wrap justify-end gap-1">
                        {row.ports.map((port) => {
                          const label = portToLabel.get(port)
                          return (
                            <button
                              key={`${row.pid}:${port}`}
                              type="button"
                              onClick={() => void killPort(port)}
                              title={label ? `${label} · kill listener on ${port}` : `Kill listener on port ${port}`}
                              className="group inline-flex items-center gap-1 rounded-tezbar-chip border border-white/10 bg-white/[0.04] px-2 py-0.5 font-mono text-[11px] text-ink-1 transition hover:border-rose-400/40 hover:bg-rose-500/15 hover:text-rose-200"
                            >
                              {label ? (
                                <span className="max-w-[72px] truncate text-[10px] font-sans font-medium text-amber-200/90">
                                  {label}
                                </span>
                              ) : null}
                              <span>{port}</span>
                            </button>
                          )
                        })}
                      </div>
                    </div>
                  </li>
                ))}
              </GlideList>
            )}
          </section>
        </div>
      ) : (
        <>
          <div className="glass-card shrink-0 px-4 py-3 animate-tezbar-scale-in">
            <div className="flex flex-wrap items-end gap-2">
              <div className="min-w-[120px] flex-1">
                <label className="mb-1 block text-[10px] font-medium uppercase tracking-[0.14em] text-ink-4">
                  Label
                </label>
                <TextField value={newName} onChange={(e) => setNewName(e.target.value)} placeholder="e.g. API server" />
              </div>
              <div className="w-24">
                <label className="mb-1 block text-[10px] font-medium uppercase tracking-[0.14em] text-ink-4">
                  Port
                </label>
                <TextField
                  value={newPort}
                  onChange={(e) => setNewPort(e.target.value)}
                  placeholder="3000"
                  inputMode="numeric"
                />
              </div>
              <Button variant="primary" className="mb-0.5" onClick={() => void addNamed()}>
                Add
              </Button>
            </div>
          </div>

          <section className="glass-card min-h-0 flex-1 space-y-1 overflow-y-auto px-4 py-3 pr-[calc(0.5rem+2px)] animate-tezbar-scale-in">
            {named.length === 0 ? (
              <div className="flex min-h-[100px] items-center justify-center">
                <p className="text-center text-[12px] text-ink-3">
                  No named ports yet. Add a friendly label for ports you use often — they show on the Listening tab.
                </p>
              </div>
            ) : (
              named.map((entry) => (
                <div
                  key={entry.id}
                  className="flex items-center justify-between gap-2 rounded-tezbar-row border border-white/[0.06] bg-white/[0.02] px-3 py-2"
                >
                  <div className="min-w-0">
                    <p className="truncate text-[13px] font-medium text-ink-1">{entry.name}</p>
                    <p className="mt-0.5 font-mono text-[11px] text-ink-3">Port {entry.port}</p>
                  </div>
                  <Button variant="ghost" onClick={() => void removeNamed(entry.id)}>
                    Remove
                  </Button>
                </div>
              ))
            )}
          </section>
        </>
      )}

      {msg ? (
        <div className="glass-card shrink-0 px-4 py-2 animate-tezbar-scale-in">
          <Message tone={msg.tone}>{msg.text}</Message>
        </div>
      ) : null}

      <div className="glass-card shrink-0 px-4 py-2 animate-tezbar-scale-in">
        <HintBar>
          {panel === 'listen' ? (
            <>
              <Hint label="Navigate" keys={<><Kbd>↑</Kbd><Kbd>↓</Kbd></>} />
              <Hint label="Kill first port" keys={<Kbd>↵</Kbd>} />
            </>
          ) : null}
          <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
        </HintBar>
      </div>
    </div>
  )
}
