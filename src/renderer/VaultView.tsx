import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from 'react'
import type {
  VaultCategory,
  VaultItem,
  VaultItemHeader,
  VaultStatus,
} from '../shared/vault'
import {
  CommandSurfaceBackButton,
  CommandSurfaceChip,
  CommandSurfaceFooter,
  CommandSurfaceHeader,
  CommandSurfaceIcon,
  CommandSurfaceInput,
  CommandSurfaceStatus,
} from './ui/CommandSurface'
import { Kbd, cx } from './ui/primitives'

type VaultFilter = 'all' | VaultCategory

type VaultViewProps = {
  onClose: () => void
}

const CATEGORY_LABELS: Record<VaultCategory, string> = {
  server: 'Servers',
  'api-key': 'API Keys',
  login: 'Logins',
  note: 'Notes',
}

const CATEGORY_ICONS: Record<VaultCategory, string> = {
  server: '🖥',
  'api-key': '🔑',
  login: '👤',
  note: '📝',
}

function generateRandomPassword(length = 20): string {
  const chars =
    'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()_+-=[]{}|;:,.<>?'
  const array = new Uint32Array(length)
  window.crypto.getRandomValues(array)
  return Array.from(array, (x) => chars[x % chars.length]).join('')
}

export default function VaultView({ onClose }: VaultViewProps): JSX.Element {
  const [status, setStatus] = useState<VaultStatus | null>(null)
  const [loading, setLoading] = useState(true)
  const [items, setItems] = useState<VaultItemHeader[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [selectedDetail, setSelectedDetail] = useState<VaultItem | null>(null)
  const [search, setSearch] = useState('')
  const [filter, setFilter] = useState<VaultFilter>('all')

  // Auth / Setup form state
  const [password, setPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [enableBiometrics, setEnableBiometrics] = useState(true)
  const [authError, setAuthError] = useState<string | null>(null)
  const [authBusy, setAuthBusy] = useState(false)

  // Editor modal state
  const [isEditing, setIsEditing] = useState(false)
  const [editItem, setEditItem] = useState<Partial<VaultItem>>({
    category: 'login',
    title: '',
    service: '',
    account: '',
    secret: '',
    secondarySecret: '',
    port: 22,
    authType: 'password',
    notes: '',
    tags: [],
  })

  // Reveal secret state
  const [revealSecret, setRevealSecret] = useState(false)
  const [copyFeedback, setCopyFeedback] = useState(false)

  const searchInputRef = useRef<HTMLInputElement>(null)
  const passwordInputRef = useRef<HTMLInputElement>(null)

  const refreshStatus = useCallback(async () => {
    try {
      const st = await window.tezbar.vaultStatus()
      setStatus(st)
      if (st.unlocked) {
        const list = await window.tezbar.vaultList()
        setItems(list)
        if (list.length > 0 && !selectedId) {
          const first = list[0]
          if (first) {
            setSelectedId(first.id)
          }
        }
      }
    } catch (e) {
      console.error('Failed to get vault status:', e)
    } finally {
      setLoading(false)
    }
  }, [selectedId])

  useEffect(() => {
    refreshStatus()
  }, [refreshStatus])

  // Load details of selected item
  useEffect(() => {
    if (!selectedId || !status?.unlocked) {
      setSelectedDetail(null)
      return
    }
    let active = true
    window.tezbar
      .vaultGet(selectedId)
      .then((item) => {
        if (active) {
          setSelectedDetail(item)
          setRevealSecret(false)
        }
      })
      .catch((e) => {
        console.error('Failed to get item details:', e)
      })
    return () => {
      active = false
    }
  }, [selectedId, status?.unlocked])

  const handleSetup = async (e: FormEvent) => {
    e.preventDefault()
    if (!password.trim()) {
      setAuthError('Please enter a master password')
      return
    }
    if (password !== confirmPassword) {
      setAuthError('Passwords do not match')
      return
    }
    if (password.length < 4) {
      setAuthError('Master password must be at least 4 characters')
      return
    }

    setAuthBusy(true)
    setAuthError(null)
    try {
      await window.tezbar.vaultSetup({
        password,
        enableBiometrics,
      })
      setPassword('')
      setConfirmPassword('')
      await refreshStatus()
    } catch (err: any) {
      setAuthError(err?.message || String(err))
    } finally {
      setAuthBusy(false)
    }
  }

  const handleUnlock = async (e?: FormEvent, useBiometrics = false) => {
    if (e) e.preventDefault()
    setAuthBusy(true)
    setAuthError(null)
    try {
      await window.tezbar.vaultUnlock({
        password: useBiometrics ? undefined : password,
        useBiometrics,
      })
      setPassword('')
      await refreshStatus()
    } catch (err: any) {
      setAuthError(err?.message || 'Incorrect password')
    } finally {
      setAuthBusy(false)
    }
  }

  const handleLock = async () => {
    try {
      await window.tezbar.vaultLock()
      setSelectedDetail(null)
      setSelectedId(null)
      await refreshStatus()
    } catch (e) {
      console.error('Failed to lock vault:', e)
    }
  }

  const handleSaveItem = async (e: FormEvent) => {
    e.preventDefault()
    if (!editItem.title?.trim() || !editItem.secret) {
      return
    }
    try {
      const saved = await window.tezbar.vaultSave({
        ...editItem,
        title: editItem.title.trim(),
        category: editItem.category || 'login',
        secret: editItem.secret,
        tags: editItem.tags || [],
      })
      setIsEditing(false)
      const list = await window.tezbar.vaultList()
      setItems(list)
      setSelectedId(saved.id)
    } catch (err) {
      console.error('Failed to save vault item:', err)
    }
  }

  const handleDeleteItem = async (id: string) => {
    if (!confirm('Are you sure you want to delete this secret?')) return
    try {
      await window.tezbar.vaultDelete(id)
      const list = await window.tezbar.vaultList()
      setItems(list)
      setSelectedId(list[0]?.id || null)
    } catch (err) {
      console.error('Failed to delete vault item:', err)
    }
  }

  const handleCopySecret = async (secret?: string) => {
    const val = secret ?? selectedDetail?.secret
    if (!val) return
    try {
      await navigator.clipboard.writeText(val)
      setCopyFeedback(true)
      setTimeout(() => setCopyFeedback(false), 1500)
    } catch (e) {
      console.error('Clipboard write failed:', e)
    }
  }

  const filteredItems = useMemo(() => {
    const q = search.trim().toLowerCase()
    return items.filter((it) => {
      if (filter !== 'all' && it.category !== filter) return false
      if (!q) return true
      return (
        it.title.toLowerCase().includes(q) ||
        it.service?.toLowerCase().includes(q) ||
        it.account?.toLowerCase().includes(q) ||
        it.tags?.some((t) => t.toLowerCase().includes(q))
      )
    })
  }, [items, filter, search])

  // Handle keyboard navigation
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      if (isEditing) {
        setIsEditing(false)
        e.preventDefault()
      } else {
        onClose()
      }
      return
    }

    if (e.metaKey && e.key === 'l') {
      e.preventDefault()
      handleLock()
      return
    }

    if (e.metaKey && e.key === 'n') {
      e.preventDefault()
      setEditItem({
        category: filter !== 'all' ? filter : 'login',
        title: '',
        service: '',
        account: '',
        secret: '',
        secondarySecret: '',
        port: 22,
        authType: 'password',
        notes: '',
        tags: [],
      })
      setIsEditing(true)
      return
    }

    if (e.metaKey && e.key === 'e' && selectedDetail) {
      e.preventDefault()
      setEditItem(selectedDetail)
      setIsEditing(true)
      return
    }

    if (!isEditing && status?.unlocked) {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        const idx = filteredItems.findIndex((it) => it.id === selectedId)
        if (idx < filteredItems.length - 1 && filteredItems[idx + 1]) {
          setSelectedId(filteredItems[idx + 1]!.id)
        }
      } else if (e.key === 'ArrowUp') {
        e.preventDefault()
        const idx = filteredItems.findIndex((it) => it.id === selectedId)
        if (idx > 0 && filteredItems[idx - 1]) {
          setSelectedId(filteredItems[idx - 1]!.id)
        }
      } else if (e.key === 'Enter') {
        e.preventDefault()
        handleCopySecret()
      }
    }
  }

  if (loading) {
    return (
      <div className="flex h-full w-full flex-col bg-surface-1 select-none" data-vault-panel>
        <CommandSurfaceHeader>
          <CommandSurfaceBackButton onClick={onClose} />
          <CommandSurfaceInput disabled placeholder="Loading Vault…" />
        </CommandSurfaceHeader>
        <div className="flex flex-1 items-center justify-center text-ink-3 text-[13px]">
          Opening Vault…
        </div>
        <CommandSurfaceFooter
          status={<CommandSurfaceStatus title="Vault">Loading</CommandSurfaceStatus>}
        />
      </div>
    )
  }

  // Not initialized: Setup master password
  if (!status?.initialized) {
    return (
      <div
        className="flex h-full w-full flex-col bg-surface-1 select-none"
        data-vault-panel
        onKeyDown={handleKeyDown}
      >
        <CommandSurfaceHeader>
          <CommandSurfaceBackButton onClick={onClose} />
          <div className="text-[14px] font-medium text-ink-1">Set Up Tezbar Vault</div>
        </CommandSurfaceHeader>

        <div className="flex flex-1 items-center justify-center p-6">
          <form
            onSubmit={handleSetup}
            className="flex w-full max-w-sm flex-col gap-4 rounded-xl border border-line/20 bg-surface-2/60 p-6 shadow-xl backdrop-blur-md"
          >
            <div className="flex flex-col items-center gap-2 text-center">
              <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-accent/15 text-2xl text-accent">
                🔒
              </div>
              <h2 className="text-[16px] font-semibold text-ink-1">Create Master Password</h2>
              <p className="text-[12px] text-ink-3">
                Your vault is encrypted with AES-256-GCM. Your master password is never stored in
                plaintext and cannot be recovered if lost.
              </p>
            </div>

            {authError ? (
              <div className="rounded-lg bg-red-500/10 px-3 py-2 text-[12px] text-red-400">
                {authError}
              </div>
            ) : null}

            <div className="flex flex-col gap-2">
              <input
                ref={passwordInputRef}
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Enter master password…"
                autoFocus
                className="w-full rounded-lg border border-line/30 bg-surface-3 px-3 py-2 text-[13px] text-ink-1 placeholder:text-ink-4 outline-none focus:border-accent"
              />
              <input
                type="password"
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.target.value)}
                placeholder="Confirm master password…"
                className="w-full rounded-lg border border-line/30 bg-surface-3 px-3 py-2 text-[13px] text-ink-1 placeholder:text-ink-4 outline-none focus:border-accent"
              />
            </div>

            {status?.biometricsAvailable ? (
              <label className="flex items-center gap-2.5 cursor-pointer text-[12px] text-ink-2">
                <input
                  type="checkbox"
                  checked={enableBiometrics}
                  onChange={(e) => setEnableBiometrics(e.target.checked)}
                  className="rounded border-line/40 text-accent focus:ring-accent"
                />
                <span>Enable Touch ID / Apple Keychain Unlock</span>
              </label>
            ) : null}

            <button
              type="submit"
              disabled={authBusy}
              className="mt-2 w-full rounded-lg bg-accent py-2 text-[13px] font-medium text-white transition hover:bg-accent/90 disabled:opacity-50"
            >
              {authBusy ? 'Creating Vault…' : 'Create Vault'}
            </button>
          </form>
        </div>

        <CommandSurfaceFooter
          status={<CommandSurfaceStatus title="Vault Setup">New</CommandSurfaceStatus>}
        >
          <Kbd>Esc Back</Kbd>
        </CommandSurfaceFooter>
      </div>
    )
  }

  // Locked: Prompt for unlock password or biometrics
  if (!status?.unlocked) {
    return (
      <div
        className="flex h-full w-full flex-col bg-surface-1 select-none"
        data-vault-panel
        onKeyDown={handleKeyDown}
      >
        <CommandSurfaceHeader>
          <CommandSurfaceBackButton onClick={onClose} />
          <div className="text-[14px] font-medium text-ink-1">Vault Locked</div>
        </CommandSurfaceHeader>

        <div className="flex flex-1 items-center justify-center p-6">
          <form
            onSubmit={(e) => handleUnlock(e, false)}
            className="flex w-full max-w-sm flex-col gap-4 rounded-xl border border-line/20 bg-surface-2/60 p-6 shadow-xl backdrop-blur-md"
          >
            <div className="flex flex-col items-center gap-2 text-center">
              <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-amber-500/15 text-2xl text-amber-500">
                🔒
              </div>
              <h2 className="text-[16px] font-semibold text-ink-1">Unlock Vault</h2>
              <p className="text-[12px] text-ink-3">
                Enter your master password to decrypt your credentials.
              </p>
            </div>

            {authError ? (
              <div className="rounded-lg bg-red-500/10 px-3 py-2 text-[12px] text-red-400">
                {authError}
              </div>
            ) : null}

            <input
              ref={passwordInputRef}
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="Master password…"
              autoFocus
              className="w-full rounded-lg border border-line/30 bg-surface-3 px-3 py-2 text-[13px] text-ink-1 placeholder:text-ink-4 outline-none focus:border-accent"
            />

            <div className="flex flex-col gap-2 mt-1">
              <button
                type="submit"
                disabled={authBusy}
                className="w-full rounded-lg bg-accent py-2 text-[13px] font-medium text-white transition hover:bg-accent/90 disabled:opacity-50"
              >
                {authBusy ? 'Unlocking…' : 'Unlock Vault'}
              </button>

              {status?.biometricsAvailable ? (
                <button
                  type="button"
                  onClick={() => handleUnlock(undefined, true)}
                  disabled={authBusy}
                  className="flex items-center justify-center gap-2 w-full rounded-lg border border-line/30 bg-surface-3 py-2 text-[13px] font-medium text-ink-1 hover:bg-surface-4 transition"
                >
                  <span>Touch ID</span>
                </button>
              ) : null}
            </div>
          </form>
        </div>

        <CommandSurfaceFooter
          status={<CommandSurfaceStatus title="Vault">Locked</CommandSurfaceStatus>}
        >
          <Kbd>Esc Back</Kbd>
        </CommandSurfaceFooter>
      </div>
    )
  }

  // Unlocked: Main Vault Explorer
  return (
    <div
      className="flex h-full w-full flex-col bg-surface-1 select-none"
      data-vault-panel
      onKeyDown={handleKeyDown}
    >
      <CommandSurfaceHeader>
        <CommandSurfaceBackButton onClick={onClose} />
        <CommandSurfaceInput
          ref={searchInputRef}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search vault secrets, logins, servers…"
          autoFocus
        />

        <div className="flex items-center gap-1">
          <CommandSurfaceChip active={filter === 'all'} onClick={() => setFilter('all')}>
            All
          </CommandSurfaceChip>
          {(['server', 'api-key', 'login', 'note'] as VaultCategory[]).map((cat) => (
            <CommandSurfaceChip
              key={cat}
              active={filter === cat}
              onClick={() => setFilter(cat)}
            >
              {CATEGORY_LABELS[cat]}
            </CommandSurfaceChip>
          ))}

          <button
            type="button"
            onClick={() => {
              setEditItem({
                category: filter !== 'all' ? filter : 'login',
                title: '',
                service: '',
                account: '',
                secret: '',
                secondarySecret: '',
                port: 22,
                authType: 'password',
                notes: '',
                tags: [],
              })
              setIsEditing(true)
            }}
            className="launcher-mode-chip ml-1 flex items-center gap-1 text-accent border-accent/30 hover:bg-accent/10"
            title="New Secret (⌘N)"
          >
            <CommandSurfaceIcon name="plus" />
            <span>New</span>
          </button>
        </div>
      </CommandSurfaceHeader>

      {/* Main split explorer */}
      <div className="flex flex-1 min-h-0 overflow-hidden">
        {/* Left: Item list */}
        <div className="flex flex-col w-[320px] border-r border-line/20 overflow-y-auto">
          {filteredItems.length === 0 ? (
            <div className="flex flex-1 flex-col items-center justify-center p-6 text-center text-ink-3">
              <span className="text-3xl mb-2">🔐</span>
              <p className="text-[13px] font-medium text-ink-2">No secrets found</p>
              <p className="text-[11px] text-ink-4 mt-1">
                {items.length === 0 ? 'Press + New or ⌘N to store your first secret.' : 'Try a different search query or filter.'}
              </p>
            </div>
          ) : (
            filteredItems.map((item) => {
              const isSelected = item.id === selectedId
              return (
                <div
                  key={item.id}
                  onClick={() => setSelectedId(item.id)}
                  className={cx(
                    'group flex items-center gap-3 px-3 py-2.5 cursor-pointer border-b border-line/10 transition',
                    isSelected ? 'bg-accent/15 text-ink-1' : 'hover:bg-surface-2/40 text-ink-2'
                  )}
                >
                  <span className="text-lg flex-shrink-0" aria-hidden>
                    {CATEGORY_ICONS[item.category] || '🔒'}
                  </span>
                  <div className="flex flex-col min-w-0 flex-1">
                    <span className="truncate text-[13px] font-medium text-ink-1">
                      {item.title}
                    </span>
                    <span className="truncate text-[11px] text-ink-3">
                      {item.service || item.account || CATEGORY_LABELS[item.category]}
                    </span>
                  </div>
                </div>
              )
            })
          )}
        </div>

        {/* Right: Item Detail View */}
        <div className="flex flex-1 flex-col overflow-y-auto p-6">
          {selectedDetail ? (
            <div className="flex flex-col gap-6 max-w-lg">
              {/* Header card */}
              <div className="flex items-start justify-between">
                <div className="flex items-center gap-3">
                  <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-surface-3 text-2xl">
                    {CATEGORY_ICONS[selectedDetail.category] || '🔒'}
                  </div>
                  <div>
                    <h2 className="text-[17px] font-semibold text-ink-1">
                      {selectedDetail.title}
                    </h2>
                    <span className="text-[11px] font-medium text-accent uppercase tracking-wider">
                      {CATEGORY_LABELS[selectedDetail.category]}
                    </span>
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={() => {
                      setEditItem(selectedDetail)
                      setIsEditing(true)
                    }}
                    className="rounded-lg border border-line/30 bg-surface-2 px-2.5 py-1.5 text-[12px] font-medium text-ink-2 hover:bg-surface-3 transition"
                    title="Edit (⌘E)"
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    onClick={() => handleDeleteItem(selectedDetail.id)}
                    className="rounded-lg border border-line/30 bg-surface-2 p-1.5 text-[12px] text-red-400 hover:bg-red-500/10 transition"
                    title="Delete"
                  >
                    <CommandSurfaceIcon name="trash" />
                  </button>
                </div>
              </div>

              {/* Secret payload with reveal & copy */}
              <div className="flex flex-col gap-1.5 rounded-xl border border-line/20 bg-surface-2/40 p-4">
                <div className="flex items-center justify-between text-[11px] text-ink-3">
                  <span className="font-medium uppercase tracking-wider">
                    {selectedDetail.category === 'note' ? 'Secure Note' : 'Secret / Password'}
                  </span>
                  <button
                    type="button"
                    onClick={() => setRevealSecret(!revealSecret)}
                    className="text-accent hover:underline text-[11px]"
                  >
                    {revealSecret ? 'Hide' : 'Reveal'}
                  </button>
                </div>

                <div className="flex items-center gap-2 mt-1">
                  <div className="flex-1 font-mono text-[13px] text-ink-1 break-all select-all">
                    {revealSecret ? (
                      selectedDetail.secret
                    ) : (
                      <span className="tracking-widest text-ink-3">••••••••••••••••</span>
                    )}
                  </div>
                  <button
                    type="button"
                    onClick={() => handleCopySecret()}
                    className={cx(
                      'rounded-lg px-3 py-1.5 text-[12px] font-medium transition',
                      copyFeedback
                        ? 'bg-emerald-500 text-white'
                        : 'bg-accent text-white hover:bg-accent/90'
                    )}
                  >
                    {copyFeedback ? 'Copied!' : 'Copy (↵)'}
                  </button>
                </div>
              </div>

              {/* Extra attributes depending on category */}
              <div className="grid grid-cols-2 gap-3 text-[12px]">
                {selectedDetail.service ? (
                  <div className="flex flex-col gap-1 rounded-lg border border-line/15 bg-surface-2/20 p-3">
                    <span className="text-[10px] uppercase text-ink-4">Service / Host</span>
                    <span className="font-medium text-ink-1 truncate select-all">
                      {selectedDetail.service}
                    </span>
                  </div>
                ) : null}

                {selectedDetail.account ? (
                  <div className="flex flex-col gap-1 rounded-lg border border-line/15 bg-surface-2/20 p-3">
                    <span className="text-[10px] uppercase text-ink-4">Account / User</span>
                    <span className="font-medium text-ink-1 truncate select-all">
                      {selectedDetail.account}
                    </span>
                  </div>
                ) : null}

                {selectedDetail.port ? (
                  <div className="flex flex-col gap-1 rounded-lg border border-line/15 bg-surface-2/20 p-3">
                    <span className="text-[10px] uppercase text-ink-4">Port</span>
                    <span className="font-medium text-ink-1">{selectedDetail.port}</span>
                  </div>
                ) : null}

                {selectedDetail.authType ? (
                  <div className="flex flex-col gap-1 rounded-lg border border-line/15 bg-surface-2/20 p-3">
                    <span className="text-[10px] uppercase text-ink-4">Auth Type</span>
                    <span className="font-medium text-ink-1 capitalize">
                      {selectedDetail.authType}
                    </span>
                  </div>
                ) : null}
              </div>

              {selectedDetail.notes ? (
                <div className="flex flex-col gap-1.5 rounded-lg border border-line/15 bg-surface-2/20 p-3">
                  <span className="text-[10px] uppercase text-ink-4">Notes</span>
                  <div className="whitespace-pre-wrap text-[12px] text-ink-2 select-text">
                    {selectedDetail.notes}
                  </div>
                </div>
              ) : null}

              {selectedDetail.tags && selectedDetail.tags.length > 0 ? (
                <div className="flex flex-wrap gap-1.5">
                  {selectedDetail.tags.map((tag) => (
                    <span
                      key={tag}
                      className="rounded-full bg-surface-3 px-2 py-0.5 text-[10px] text-ink-3"
                    >
                      #{tag}
                    </span>
                  ))}
                </div>
              ) : null}
            </div>
          ) : (
            <div className="flex flex-1 items-center justify-center text-ink-4 text-[13px]">
              Select an item to view secrets
            </div>
          )}
        </div>
      </div>

      {/* Editor Modal */}
      {isEditing ? (
        <div className="absolute inset-0 z-20 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
          <form
            onSubmit={handleSaveItem}
            className="flex w-full max-w-md flex-col gap-3.5 rounded-xl border border-line/30 bg-surface-2 p-5 shadow-2xl"
          >
            <div className="flex items-center justify-between pb-1 border-b border-line/20">
              <span className="text-[14px] font-semibold text-ink-1">
                {editItem.id ? 'Edit Secret' : 'New Vault Secret'}
              </span>
              <button
                type="button"
                onClick={() => setIsEditing(false)}
                className="text-ink-4 hover:text-ink-2 text-sm"
              >
                ✕
              </button>
            </div>

            {/* Category tabs */}
            <div className="flex gap-1">
              {(['server', 'api-key', 'login', 'note'] as VaultCategory[]).map((cat) => (
                <button
                  type="button"
                  key={cat}
                  onClick={() => setEditItem({ ...editItem, category: cat })}
                  className={cx(
                    'flex-1 rounded-md py-1 text-[11px] font-medium transition',
                    editItem.category === cat
                      ? 'bg-accent text-white'
                      : 'bg-surface-3 text-ink-3 hover:text-ink-1'
                  )}
                >
                  {CATEGORY_LABELS[cat]}
                </button>
              ))}
            </div>

            <div className="flex flex-col gap-2.5 text-[12px]">
              <input
                type="text"
                value={editItem.title || ''}
                onChange={(e) => setEditItem({ ...editItem, title: e.target.value })}
                placeholder="Title (e.g. AWS Production, Root Server)…"
                required
                className="rounded-lg border border-line/30 bg-surface-3 px-3 py-1.5 text-ink-1 placeholder:text-ink-4 outline-none focus:border-accent"
              />

              <div className="grid grid-cols-2 gap-2">
                <input
                  type="text"
                  value={editItem.service || ''}
                  onChange={(e) => setEditItem({ ...editItem, service: e.target.value })}
                  placeholder={
                    editItem.category === 'server'
                      ? 'Host / IP (e.g. 1.2.3.4)'
                      : 'Service / URL'
                  }
                  className="rounded-lg border border-line/30 bg-surface-3 px-3 py-1.5 text-ink-1 placeholder:text-ink-4 outline-none focus:border-accent"
                />
                <input
                  type="text"
                  value={editItem.account || ''}
                  onChange={(e) => setEditItem({ ...editItem, account: e.target.value })}
                  placeholder={
                    editItem.category === 'server'
                      ? 'SSH User (e.g. ubuntu)'
                      : 'Username / Key ID'
                  }
                  className="rounded-lg border border-line/30 bg-surface-3 px-3 py-1.5 text-ink-1 placeholder:text-ink-4 outline-none focus:border-accent"
                />
              </div>

              {editItem.category === 'server' ? (
                <div className="grid grid-cols-2 gap-2">
                  <input
                    type="number"
                    value={editItem.port || 22}
                    onChange={(e) => setEditItem({ ...editItem, port: Number(e.target.value) })}
                    placeholder="Port (22)"
                    className="rounded-lg border border-line/30 bg-surface-3 px-3 py-1.5 text-ink-1 placeholder:text-ink-4 outline-none focus:border-accent"
                  />
                  <select
                    value={editItem.authType || 'password'}
                    onChange={(e) =>
                      setEditItem({
                        ...editItem,
                        authType: e.target.value as 'password' | 'ssh-key',
                      })
                    }
                    className="rounded-lg border border-line/30 bg-surface-3 px-3 py-1.5 text-ink-1 outline-none focus:border-accent"
                  >
                    <option value="password">Password Auth</option>
                    <option value="ssh-key">SSH Private Key</option>
                  </select>
                </div>
              ) : null}

              <div className="flex flex-col gap-1">
                <div className="flex items-center justify-between text-[11px] text-ink-3">
                  <span>Secret / Password / Private Key</span>
                  <button
                    type="button"
                    onClick={() =>
                      setEditItem({ ...editItem, secret: generateRandomPassword(24) })
                    }
                    className="text-accent hover:underline"
                  >
                    🎲 Generate Strong
                  </button>
                </div>
                {editItem.category === 'note' ? (
                  <textarea
                    rows={4}
                    value={editItem.secret || ''}
                    onChange={(e) => setEditItem({ ...editItem, secret: e.target.value })}
                    placeholder="Enter confidential notes or private text…"
                    required
                    className="rounded-lg border border-line/30 bg-surface-3 px-3 py-1.5 text-ink-1 placeholder:text-ink-4 outline-none focus:border-accent resize-none font-mono"
                  />
                ) : (
                  <input
                    type="text"
                    value={editItem.secret || ''}
                    onChange={(e) => setEditItem({ ...editItem, secret: e.target.value })}
                    placeholder="Password, API secret, or SSH key…"
                    required
                    className="rounded-lg border border-line/30 bg-surface-3 px-3 py-1.5 text-ink-1 placeholder:text-ink-4 outline-none focus:border-accent font-mono"
                  />
                )}
              </div>

              <textarea
                rows={2}
                value={editItem.notes || ''}
                onChange={(e) => setEditItem({ ...editItem, notes: e.target.value })}
                placeholder="Optional notes or description…"
                className="rounded-lg border border-line/30 bg-surface-3 px-3 py-1.5 text-ink-1 placeholder:text-ink-4 outline-none focus:border-accent resize-none"
              />
            </div>

            <div className="flex justify-end gap-2 pt-2 border-t border-line/20">
              <button
                type="button"
                onClick={() => setIsEditing(false)}
                className="rounded-lg border border-line/30 px-3 py-1.5 text-[12px] font-medium text-ink-3 hover:bg-surface-3 transition"
              >
                Cancel
              </button>
              <button
                type="submit"
                className="rounded-lg bg-accent px-4 py-1.5 text-[12px] font-medium text-white hover:bg-accent/90 transition"
              >
                Save Secret
              </button>
            </div>
          </form>
        </div>
      ) : null}

      <CommandSurfaceFooter
        status={
          <CommandSurfaceStatus title="Vault">
            {`${filteredItems.length} ${filteredItems.length === 1 ? 'item' : 'items'}`}
          </CommandSurfaceStatus>
        }
      >
        <div className="flex items-center gap-2">
          <Kbd>↵ Copy</Kbd>
          <Kbd>⌘N New</Kbd>
          <Kbd>⌘E Edit</Kbd>
          <Kbd>⌘L Lock</Kbd>
          <Kbd>Esc Back</Kbd>
        </div>
      </CommandSurfaceFooter>
    </div>
  )
}
