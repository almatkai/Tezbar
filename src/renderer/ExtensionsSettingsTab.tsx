import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Button, cx } from './ui/primitives'
import HotkeyRecorder, { formatShortcutForDisplay } from './HotkeyRecorder'
import { ExtensionPreferencesEditor } from './ExtensionPreferencesEditor'
import type { InstalledRegistryExtension } from '../shared/extensionRuntime'

// ─── Types ──────────────────────────────────────────────────────────────

type ExtensionPreferenceSchema = {
  scope: 'extension' | 'command'
  name: string
  title?: string
  label?: string
  description?: string
  placeholder?: string
  required?: boolean
  type?: string
  default?: unknown
  data?: Array<{ title?: string; value?: string }>
}

type CommandSchema = {
  name: string
  title: string
  description: string
  mode: string
  interval?: string
  disabledByDefault?: boolean
  preferences: ExtensionPreferenceSchema[]
}

type ExtensionSchema = {
  extName: string
  title: string
  description: string
  owner: string
  iconDataUrl?: string
  preferences: ExtensionPreferenceSchema[]
  commands: CommandSchema[]
}

type SelectedTarget = {
  extName: string
  cmdName?: string
}

type Settings = {
  commandHotkeys: Record<string, string>
  commandAliases: Record<string, string>
  disabledCommands: Record<string, boolean>
}

// ─── SVG Icons ──────────────────────────────────────────────────────────

function ChevronIcon({ expanded, className }: { expanded: boolean; className?: string }) {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={cx(
        'transition-transform duration-150',
        expanded ? 'rotate-90' : '',
        className,
      )}
    >
      <polyline points="9 6 15 12 9 18" />
    </svg>
  )
}

function SearchIcon({ className }: { className?: string }) {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      <circle cx="11" cy="11" r="8" />
      <path d="m21 21-4.3-4.3" />
    </svg>
  )
}

function PuzzleIcon({ className }: { className?: string }) {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      <path d="M8 3h3a2 2 0 1 1 4 0h3v5a2 2 0 1 1 0 4v5h-5a2 2 0 1 1-4 0H4v-5a2 2 0 1 1 0-4V3h4Z" />
    </svg>
  )
}

function TerminalIcon({ className }: { className?: string }) {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      <polyline points="4 17 10 11 4 5" />
      <line x1="12" x2="20" y1="19" y2="19" />
    </svg>
  )
}

function EditIcon({ className }: { className?: string }) {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
      <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
    </svg>
  )
}

function TrashIcon({ className }: { className?: string }) {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      <polyline points="3 6 5 6 21 6" />
      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
    </svg>
  )
}

// ─── Helpers ────────────────────────────────────────────────────────────

/** Build a stable command ID string from extension name + command name. */
function commandId(extName: string, cmdName: string): string {
  return `extcmd:${extName}:${cmdName}`
}

function normalizeExtensionId(name: string): string {
  return String(name || '')
    .trim()
    .replace(/^@/, '')
    .replace(/^raycast\./, '')
    .replace(/[\\/]/g, '-')
}

function iconPathToUrl(iconPath?: string): string | undefined {
  if (!iconPath) return undefined
  if (/^(https?:|data:|file:)/i.test(iconPath)) return iconPath
  return `file://${encodeURI(iconPath)}`
}

function registryExtensionToSchema(ext: InstalledRegistryExtension): ExtensionSchema {
  const extName = normalizeExtensionId(ext.slug || ext.id)
  return {
    extName,
    title: ext.name || extName,
    description: ext.description || '',
    owner: ext.owner || ext.author || '',
    iconDataUrl: iconPathToUrl(ext.iconPath),
    preferences: [],
    commands: ext.commands.map((cmd) => ({
      name: cmd.name,
      title: cmd.title || cmd.name,
      description: cmd.description || cmd.subtitle || '',
      mode: cmd.mode || 'view',
      preferences: [],
    })),
  }
}

function mergeExtensionSchemas(
  schemaRows: ExtensionSchema[],
  installedRows: InstalledRegistryExtension[],
): ExtensionSchema[] {
  const byName = new Map<string, ExtensionSchema>()
  for (const schema of schemaRows) {
    byName.set(normalizeExtensionId(schema.extName), schema)
  }
  for (const installed of installedRows) {
    const schema = registryExtensionToSchema(installed)
    if (!schema.extName || byName.has(schema.extName)) continue
    byName.set(schema.extName, schema)
  }
  return [...byName.values()].sort((a, b) => a.title.localeCompare(b.title))
}

function modeLabel(mode: string): string {
  if (mode === 'menu-bar') return 'Menu Bar'
  if (mode === 'no-view') return 'Background'
  return 'Command'
}

// ─── Component ──────────────────────────────────────────────────────────

export interface ExtensionsSettingsTabProps {
  onBrowseStore?: () => void
}

export default function ExtensionsSettingsTab({ onBrowseStore }: ExtensionsSettingsTabProps = {}): JSX.Element {
  const [schemas, setSchemas] = useState<ExtensionSchema[]>([])
  const [settings, setSettings] = useState<Settings>({
    commandHotkeys: {},
    commandAliases: {},
    disabledCommands: {},
  })
  const [isLoading, setIsLoading] = useState(true)
  const [search, setSearch] = useState('')
  const [selected, setSelected] = useState<SelectedTarget | null>(null)
  const [expandedExtensions, setExpandedExtensions] = useState<Record<string, boolean>>({})
  const [hotkeyStatus, setHotkeyStatus] = useState<{
    type: 'idle' | 'success' | 'error'
    text: string
  }>({ type: 'idle', text: '' })
  const [aliasDrafts, setAliasDrafts] = useState<Record<string, string>>({})
  const [editingAliasCommandId, setEditingAliasCommandId] = useState<string | null>(null)
  const [uninstallDialog, setUninstallDialog] = useState<{
    extName: string
    title: string
  } | null>(null)
  const [busyUninstallExtName, setBusyUninstallExtName] = useState<string | null>(null)
  const [extensionActionStatus, setExtensionActionStatus] = useState<{
    type: 'idle' | 'success' | 'error'
    text: string
  }>({ type: 'idle', text: '' })
  const searchInputRef = useRef<HTMLInputElement>(null)
  const detailPanelRef = useRef<HTMLDivElement>(null)

  // ─── Data Loading ───────────────────────────────────────────────────

  const loadData = useCallback(async () => {
    setIsLoading(true)
    try {
      const [schemaRows, installedRows, settingsResult] = await Promise.all([
        window.tezbar.getInstalledExtensionsSettingsSchema(),
        window.tezbar.extensionList(),
        window.tezbar.getSettings(),
      ])
      const mergedSchemas = mergeExtensionSchemas(schemaRows, installedRows)
      setSchemas(mergedSchemas)
      setSettings(settingsResult)

      // Auto-select first extension
      if (mergedSchemas.length > 0) {
        const firstSchema = mergedSchemas[0]
        if (firstSchema) {
          setSelected((prev) => prev || { extName: firstSchema.extName })
        }
      }
    } finally {
      setIsLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadData()
  }, [loadData])

  // Cmd+F focuses search
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const isCmdF = (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'f'
      if (!isCmdF) return
      const target = event.target as HTMLElement | null
      if (target === searchInputRef.current) return
      if (target?.closest('[data-hotkey-recorder]')) return
      event.preventDefault()
      searchInputRef.current?.focus()
      searchInputRef.current?.select()
    }
    window.addEventListener('keydown', handleKeyDown, true)
    return () => window.removeEventListener('keydown', handleKeyDown, true)
  }, [])

  // ─── Filtered Schemas ─────────────────────────────────────────────

  const filteredSchemas = useMemo(() => {
    const q = search.trim().toLowerCase()
    if (!q) return schemas

    return schemas
      .map((schema) => {
        const matchesExtension =
          schema.title.toLowerCase().includes(q) ||
          schema.extName.toLowerCase().includes(q) ||
          schema.description.toLowerCase().includes(q)
        const matchedCommands = schema.commands.filter((cmd) => {
          const cmdId = commandId(schema.extName, cmd.name)
          const alias = String(settings.commandAliases[cmdId] || '').toLowerCase()
          return (
            cmd.title.toLowerCase().includes(q) ||
            cmd.name.toLowerCase().includes(q) ||
            cmd.description.toLowerCase().includes(q) ||
            alias.includes(q)
          )
        })
        if (matchedCommands.length === 0) return null
        if (matchesExtension && matchedCommands.length === schema.commands.length) return schema
        return { ...schema, commands: matchedCommands }
      })
      .filter(Boolean) as ExtensionSchema[]
  }, [schemas, search, settings.commandAliases])

  // ─── Selected Schema Derivations ──────────────────────────────────

  const selectedSchema = useMemo(
    () => filteredSchemas.find((s) => s.extName === selected?.extName) || null,
    [filteredSchemas, selected],
  )

  const selectedCommandSchema = useMemo(() => {
    if (!selectedSchema || !selected?.cmdName) return null
    return selectedSchema.commands.find((c) => c.name === selected.cmdName) || null
  }, [selectedSchema, selected])

  // ─── Command Enable/Disable ───────────────────────────────────────

  const isCommandEnabled = useCallback(
    (extName: string, cmdName: string): boolean => {
      const id = commandId(extName, cmdName)
      return !settings.disabledCommands[id]
    },
    [settings.disabledCommands],
  )

  const toggleCommandEnabled = useCallback(
    async (extName: string, cmdName: string, enabled: boolean) => {
      const id = commandId(extName, cmdName)
      await window.tezbar.toggleCommandEnabled(id, enabled)
      setSettings((prev) => {
        const next = { ...prev.disabledCommands }
        if (enabled) delete next[id]
        else next[id] = true
        return { ...prev, disabledCommands: next }
      })
    },
    [],
  )

  // ─── Hotkey Management ────────────────────────────────────────────

  const setCommandHotkey = useCallback(
    async (extName: string, cmdName: string, hotkey: string) => {
      const id = commandId(extName, cmdName)
      const result = await window.tezbar.updateCommandHotkey(id, hotkey)
      if (!result.ok) {
        setHotkeyStatus({
          type: 'error',
          text: result.error || 'Could not register shortcut.',
        })
        setTimeout(() => setHotkeyStatus({ type: 'idle', text: '' }), 3200)
        return
      }
      setSettings((prev) => {
        const next = { ...prev.commandHotkeys }
        if (hotkey) next[id] = hotkey
        else delete next[id]
        return { ...prev, commandHotkeys: next }
      })
      setHotkeyStatus({
        type: 'success',
        text: hotkey ? 'Shortcut updated.' : 'Shortcut removed.',
      })
      setTimeout(() => setHotkeyStatus({ type: 'idle', text: '' }), 1800)
    },
    [],
  )

  // ─── Alias Management ────────────────────────────────────────────

  const getCommandAlias = useCallback(
    (id: string): string => String(settings.commandAliases[id] || '').trim(),
    [settings.commandAliases],
  )

  const startAliasEditing = useCallback(
    (id: string) => {
      setAliasDrafts((prev) => ({ ...prev, [id]: getCommandAlias(id) }))
      setEditingAliasCommandId(id)
    },
    [getCommandAlias],
  )

  const cancelAliasEditing = useCallback((id: string) => {
    setEditingAliasCommandId((prev) => (prev === id ? null : prev))
    setAliasDrafts((prev) => {
      const next = { ...prev }
      delete next[id]
      return next
    })
  }, [])

  const saveCommandAlias = useCallback(
    async (id: string, draftValue: string) => {
      const trimmed = String(draftValue || '').trim()
      const existing = getCommandAlias(id)
      if (trimmed === existing) {
        cancelAliasEditing(id)
        return
      }
      const nextAliases = { ...settings.commandAliases }
      if (trimmed) nextAliases[id] = trimmed
      else delete nextAliases[id]

      await window.tezbar.saveSettings({ commandAliases: nextAliases })
      setSettings((prev) => ({ ...prev, commandAliases: nextAliases }))
      cancelAliasEditing(id)
    },
    [cancelAliasEditing, getCommandAlias, settings.commandAliases],
  )

  // ─── Extension Uninstall ──────────────────────────────────────────

  const handleUninstallExtension = useCallback(
    async (extName: string, title: string) => {
      setUninstallDialog(null)
      setBusyUninstallExtName(extName)
      try {
        const success = await window.tezbar.extensionUninstall(extName)
        if (success) {
          setExtensionActionStatus({ type: 'success', text: `Uninstalled ${title}.` })
          setTimeout(() => setExtensionActionStatus({ type: 'idle', text: '' }), 2200)
          await loadData()
        } else {
          setExtensionActionStatus({ type: 'error', text: `Failed to uninstall ${title}.` })
          setTimeout(() => setExtensionActionStatus({ type: 'idle', text: '' }), 3200)
        }
      } catch {
        setExtensionActionStatus({ type: 'error', text: `Failed to uninstall ${title}.` })
        setTimeout(() => setExtensionActionStatus({ type: 'idle', text: '' }), 3200)
      } finally {
        setBusyUninstallExtName(null)
      }
    },
    [loadData],
  )

  // ─── Sidebar Toggles ─────────────────────────────────────────────

  const toggleExtensionExpanded = (extName: string) => {
    setExpandedExtensions((prev) => ({ ...prev, [extName]: !prev[extName] }))
  }

  // ─── Scroll to detail panel top on selection change ───────────────

  useEffect(() => {
    detailPanelRef.current?.scrollTo({ top: 0, behavior: 'smooth' })
  }, [selected?.extName, selected?.cmdName])

  // ─── Loading State ────────────────────────────────────────────────

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-[13px] text-ink-4 animate-pulse">Loading extensions…</div>
      </div>
    )
  }

  if (schemas.length === 0) {
    return (
      <div className="flex h-full items-center justify-center bg-white/[0.01]">
        <div className="text-center">
          <PuzzleIcon className="mx-auto mb-2 text-ink-4" />
          <div className="text-[13px] text-ink-3">No extensions installed.</div>
          <div className="mt-1 text-[12px] text-ink-4 mb-4">
            Install extensions from the Store to manage them here.
          </div>
          <Button
            variant="primary"
            onClick={() => {
              if (onBrowseStore) onBrowseStore()
              else void window.tezbar.openExtensionStore()
            }}
          >
            Browse Extension Store
          </Button>
        </div>
      </div>
    )
  }

  // ─── Derive selected details ──────────────────────────────────────

  const selectedCmdId = selected?.cmdName
    ? commandId(selected.extName, selected.cmdName)
    : null
  const selectedCmdHotkey = selectedCmdId ? (settings.commandHotkeys[selectedCmdId] || '') : ''
  const selectedCmdAlias = selectedCmdId ? getCommandAlias(selectedCmdId) : ''
  const selectedCmdEnabled = selected?.cmdName
    ? isCommandEnabled(selected.extName, selected.cmdName)
    : true

  // ─── Render ───────────────────────────────────────────────────────

  return (
    <div className="flex h-full min-h-0 overflow-hidden rounded-xl">
      {/* ─── Left Sidebar: Extension & Command List ─── */}
      <div className="flex w-[320px] min-w-[280px] flex-col border-r border-white/[0.06] bg-white/[0.015]">
        {/* Search */}
        <div className="flex items-center gap-2 border-b border-white/[0.06] px-3 py-2">
          <div className="relative flex-1">
            <SearchIcon className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-ink-4" />
            <input
              ref={searchInputRef}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Filter extensions…"
              className="w-full rounded-lg bg-white/[0.04] border border-white/[0.08] py-1.5 pl-9 pr-3 text-[12.5px] text-ink-1 placeholder:text-ink-4 outline-none focus:border-blue-500/40 focus:bg-white/[0.06] transition-colors"
            />
          </div>
          <Button
            variant="quiet"
            onClick={() => {
              if (onBrowseStore) onBrowseStore()
              else void window.tezbar.openExtensionStore()
            }}
            title="Browse Extension Store"
            className="flex-shrink-0 px-2 h-[30px]"
          >
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              className="text-ink-3"
            >
              <path d="M6 2 3 6v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6l-3-4Z" />
              <path d="M3 6h18" />
              <path d="M16 10a4 4 0 0 1-8 0" />
            </svg>
          </Button>
          {hotkeyStatus.type !== 'idle' && (
            <span
              className={cx(
                'whitespace-nowrap text-[11px] animate-in fade-in duration-200',
                hotkeyStatus.type === 'error' ? 'text-red-400' : 'text-emerald-400',
              )}
            >
              {hotkeyStatus.text}
            </span>
          )}
        </div>

        {extensionActionStatus.type !== 'idle' && (
          <div
            className={cx(
              'px-3 py-1.5 text-[11px] border-b border-white/[0.06]',
              extensionActionStatus.type === 'error'
                ? 'text-red-400 bg-red-500/[0.06]'
                : 'text-emerald-400 bg-emerald-500/[0.06]',
            )}
          >
            {extensionActionStatus.text}
          </div>
        )}

        {/* Extension list */}
        <div className="flex-1 overflow-y-auto custom-scrollbar">
          {filteredSchemas.length === 0 ? (
            <div className="px-4 py-8 text-center text-[12px] text-ink-4">No matching extensions.</div>
          ) : (
            filteredSchemas.map((schema) => (
              <div key={schema.extName}>
                {/* Extension header */}
                <button
                  type="button"
                  aria-expanded={!!expandedExtensions[schema.extName]}
                  onClick={() => {
                    setSelected({ extName: schema.extName })
                    toggleExtensionExpanded(schema.extName)
                  }}
                  className={cx(
                    'group flex w-full items-center gap-2 px-3 py-2 text-left transition-colors',
                    selected?.extName === schema.extName && !selected?.cmdName
                      ? 'bg-blue-500/[0.12] text-ink-1'
                      : 'hover:bg-white/[0.04] text-ink-2',
                  )}
                >
                  <ChevronIcon
                    expanded={!!expandedExtensions[schema.extName]}
                    className="flex-shrink-0 text-ink-4"
                  />
                  {schema.iconDataUrl ? (
                    <img
                      src={schema.iconDataUrl}
                      alt=""
                      className="h-[18px] w-[18px] flex-shrink-0 rounded-[4px]"
                    />
                  ) : (
                    <PuzzleIcon className="flex-shrink-0 text-ink-4" />
                  )}
                  <span className="flex-1 truncate text-[12.5px] font-medium">{schema.title}</span>
                  <span className="text-[10px] text-ink-4 tabular-nums">
                    {schema.commands.length}
                  </span>
                </button>

                {/* Command list */}
                {expandedExtensions[schema.extName] && (
                  <div className="ml-4 border-l border-white/[0.05]">
                    {schema.commands.map((cmd) => {
                      const id = commandId(schema.extName, cmd.name)
                      const enabled = isCommandEnabled(schema.extName, cmd.name)
                      const hotkey = settings.commandHotkeys[id] || ''
                      const isSelected =
                        selected?.extName === schema.extName &&
                        selected?.cmdName === cmd.name

                      return (
                        <button
                          key={cmd.name}
                          onClick={() =>
                            setSelected({ extName: schema.extName, cmdName: cmd.name })
                          }
                          className={cx(
                            'group flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors',
                            isSelected
                              ? 'bg-blue-500/[0.12] text-ink-1'
                              : 'hover:bg-white/[0.04] text-ink-2',
                            !enabled && 'opacity-50',
                          )}
                        >
                          <TerminalIcon className="flex-shrink-0 text-ink-4" />
                          <span className="flex-1 truncate text-[12px]">{cmd.title}</span>
                          {hotkey && (
                            <span className="flex-shrink-0 rounded bg-white/[0.06] px-1.5 py-0.5 text-[10px] text-ink-3 font-mono">
                              {formatShortcutForDisplay(hotkey)}
                            </span>
                          )}
                        </button>
                      )
                    })}
                  </div>
                )}
              </div>
            ))
          )}
        </div>
      </div>

      {/* ─── Right Panel: Detail/Settings ─── */}
      <div ref={detailPanelRef} className="flex-1 overflow-y-auto custom-scrollbar bg-white/[0.01]">
        {!selectedSchema ? (
          <div className="flex h-full items-center justify-center text-[12.5px] text-ink-4">
            Select an extension or command from the sidebar.
          </div>
        ) : selected?.cmdName && selectedCommandSchema ? (
          /* ─── Command Detail ─── */
          <div className="p-5">
            {/* Header */}
            <div className="mb-5">
              <div className="flex items-center gap-2 mb-1">
                <TerminalIcon className="text-ink-3" />
                <h3 className="text-[15px] font-semibold text-ink-1">
                  {selectedCommandSchema.title}
                </h3>
              </div>
              {selectedCommandSchema.description && (
                <p className="text-[12px] text-ink-3 leading-relaxed ml-5">
                  {selectedCommandSchema.description}
                </p>
              )}
              <div className="ml-5 mt-1.5 flex items-center gap-2">
                <span className="rounded-full bg-white/[0.05] px-2 py-0.5 text-[10px] text-ink-4">
                  {modeLabel(selectedCommandSchema.mode)}
                </span>
                <span className="text-[10px] text-ink-4">
                  {selectedSchema.title}
                </span>
              </div>
            </div>

            <div className="space-y-4">
              {/* ─── Enabled Toggle ─── */}
              <div className="rounded-lg bg-white/[0.03] border border-white/[0.06] p-3">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-[12.5px] font-medium text-ink-2">Enabled</div>
                    <div className="text-[11px] text-ink-4 mt-0.5">
                      Show this command in the launcher
                    </div>
                  </div>
                  <label className="relative inline-flex cursor-pointer items-center">
                    <input
                      type="checkbox"
                      checked={selectedCmdEnabled}
                      onChange={(e) =>
                        void toggleCommandEnabled(
                          selected.extName,
                          selected.cmdName!,
                          e.target.checked,
                        )
                      }
                      className="peer sr-only"
                    />
                    <div className="peer h-5 w-9 rounded-full bg-white/[0.08] after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-ink-3 after:transition-all peer-checked:bg-blue-500/60 peer-checked:after:translate-x-full peer-checked:after:bg-white" />
                  </label>
                </div>
              </div>

              {/* ─── Hotkey ─── */}
              <div className="rounded-lg bg-white/[0.03] border border-white/[0.06] p-3">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-[12.5px] font-medium text-ink-2">Hotkey</div>
                    <div className="text-[11px] text-ink-4 mt-0.5">
                      Global keyboard shortcut
                    </div>
                  </div>
                  <HotkeyRecorder
                    value={selectedCmdHotkey}
                    onChange={(hotkey) =>
                      void setCommandHotkey(selected.extName, selected.cmdName!, hotkey)
                    }
                    compact
                  />
                </div>
              </div>

              {/* ─── Alias ─── */}
              <div className="rounded-lg bg-white/[0.03] border border-white/[0.06] p-3">
                <div className="flex items-center justify-between gap-3">
                  <div className="min-w-0">
                    <div className="text-[12.5px] font-medium text-ink-2">Alias</div>
                    <div className="text-[11px] text-ink-4 mt-0.5">
                      Type this in the launcher to quickly find this command
                    </div>
                  </div>
                  {editingAliasCommandId === selectedCmdId ? (
                    <div className="flex items-center gap-1.5">
                      <input
                        autoFocus
                        value={aliasDrafts[selectedCmdId!] ?? ''}
                        onChange={(e) =>
                          setAliasDrafts((prev) => ({
                            ...prev,
                            [selectedCmdId!]: e.target.value,
                          }))
                        }
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') {
                            void saveCommandAlias(
                              selectedCmdId!,
                              aliasDrafts[selectedCmdId!] ?? '',
                            )
                          }
                          if (e.key === 'Escape') cancelAliasEditing(selectedCmdId!)
                        }}
                        onBlur={() =>
                          void saveCommandAlias(
                            selectedCmdId!,
                            aliasDrafts[selectedCmdId!] ?? '',
                          )
                        }
                        className="w-[140px] rounded-md bg-white/[0.06] border border-white/[0.1] px-2 py-1 text-[12px] text-ink-1 placeholder:text-ink-4 outline-none focus:border-blue-500/40"
                        placeholder="e.g. calc"
                      />
                    </div>
                  ) : (
                    <button
                      onClick={() => startAliasEditing(selectedCmdId!)}
                      className="group flex items-center gap-1.5 rounded-md px-2 py-1 text-[12px] text-ink-3 hover:bg-white/[0.04] hover:text-ink-2 transition-colors"
                    >
                      {selectedCmdAlias || (
                        <span className="text-ink-4 italic">None</span>
                      )}
                      <EditIcon className="opacity-0 group-hover:opacity-100 text-ink-4 transition-opacity" />
                    </button>
                  )}
                </div>
              </div>

              {/* ─── Command Preferences ─── */}
              {selectedCommandSchema.preferences.length > 0 && (
                <div className="rounded-lg bg-white/[0.03] border border-white/[0.06] p-3">
                  <div className="text-[12.5px] font-medium text-ink-2 mb-3">Preferences</div>
                  <ExtensionPreferencesEditor
                    extensionId={selected.extName}
                    extensionName={selectedSchema.title}
                    commands={[selectedCommandSchema]}
                    onMessage={(msg) => {
                      setHotkeyStatus({
                        type: msg.tone,
                        text: msg.text,
                      })
                      setTimeout(() => setHotkeyStatus({ type: 'idle', text: '' }), 2000)
                    }}
                  />
                </div>
              )}
            </div>
          </div>
        ) : (
          /* ─── Extension Detail ─── */
          <div className="p-5">
            {/* Header */}
            <div className="mb-5">
              <div className="flex items-center gap-3 mb-2">
                {selectedSchema.iconDataUrl ? (
                  <img
                    src={selectedSchema.iconDataUrl}
                    alt=""
                    className="h-10 w-10 rounded-lg"
                  />
                ) : (
                  <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-gradient-to-br from-blue-500/20 to-purple-500/20 border border-white/[0.08]">
                    <PuzzleIcon className="text-ink-3" />
                  </div>
                )}
                <div>
                  <h3 className="text-[15px] font-semibold text-ink-1">
                    {selectedSchema.title}
                  </h3>
                  {selectedSchema.owner && (
                    <span className="text-[11px] text-ink-4">by {selectedSchema.owner}</span>
                  )}
                </div>
              </div>
              {selectedSchema.description && (
                <p className="text-[12px] text-ink-3 leading-relaxed">
                  {selectedSchema.description}
                </p>
              )}
            </div>

            {/* Commands table */}
            <div className="mb-5">
              <div className="text-[12px] font-semibold text-ink-2 mb-2 uppercase tracking-wider">
                Commands ({selectedSchema.commands.length})
              </div>
              <div className="rounded-lg border border-white/[0.06] overflow-hidden">
                {/* Table header */}
                <div className="grid grid-cols-[1fr_100px_100px_70px] gap-px bg-white/[0.04] px-3 py-1.5 text-[10px] uppercase tracking-wider text-ink-4 border-b border-white/[0.06]">
                  <div>Name</div>
                  <div>Type</div>
                  <div>Hotkey</div>
                  <div className="text-center">Status</div>
                </div>
                {/* Table rows */}
                {selectedSchema.commands.map((cmd) => {
                  const id = commandId(selectedSchema.extName, cmd.name)
                  const enabled = isCommandEnabled(selectedSchema.extName, cmd.name)
                  const hotkey = settings.commandHotkeys[id] || ''

                  return (
                    <button
                      key={cmd.name}
                      onClick={() =>
                        setSelected({ extName: selectedSchema.extName, cmdName: cmd.name })
                      }
                      className={cx(
                        'grid w-full grid-cols-[1fr_100px_100px_70px] gap-px px-3 py-2 text-left text-[12px] transition-colors border-b border-white/[0.04] last:border-b-0',
                        'hover:bg-white/[0.04]',
                        !enabled && 'opacity-50',
                      )}
                    >
                      <div className="flex items-center gap-2 min-w-0">
                        <TerminalIcon className="flex-shrink-0 text-ink-4" />
                        <span className="truncate text-ink-2">{cmd.title}</span>
                      </div>
                      <div className="text-ink-4">{modeLabel(cmd.mode)}</div>
                      <div className="text-ink-3 font-mono text-[10px]">
                        {hotkey ? formatShortcutForDisplay(hotkey) : '—'}
                      </div>
                      <div className="text-center">
                        <span
                          className={cx(
                            'inline-block h-2 w-2 rounded-full',
                            enabled ? 'bg-emerald-400' : 'bg-white/[0.15]',
                          )}
                        />
                      </div>
                    </button>
                  )
                })}
              </div>
            </div>

            {/* Extension Preferences */}
            {selectedSchema.preferences.length > 0 && (
              <div className="mb-5">
                <div className="text-[12px] font-semibold text-ink-2 mb-2 uppercase tracking-wider">
                  Extension Preferences
                </div>
                <div className="rounded-lg bg-white/[0.03] border border-white/[0.06] p-3">
                  <ExtensionPreferencesEditor
                    extensionId={selectedSchema.extName}
                    extensionName={selectedSchema.title}
                    commands={selectedSchema.commands}
                    onMessage={(msg) => {
                      setExtensionActionStatus({
                        type: msg.tone,
                        text: msg.text,
                      })
                      setTimeout(() => setExtensionActionStatus({ type: 'idle', text: '' }), 2000)
                    }}
                  />
                </div>
              </div>
            )}

            {/* Uninstall */}
            <div className="pt-3 border-t border-white/[0.06]">
              <div className="flex items-center justify-between">
                <div>
                  <div className="text-[12px] text-ink-3">Uninstall extension</div>
                  <div className="text-[11px] text-ink-4">Remove this extension and all its commands</div>
                </div>
                <Button
                  variant="quiet"
                  onClick={() =>
                    setUninstallDialog({
                      extName: selectedSchema.extName,
                      title: selectedSchema.title,
                    })
                  }
                  disabled={busyUninstallExtName === selectedSchema.extName}
                >
                  <TrashIcon className="mr-1 text-red-400/80" />
                  Uninstall
                </Button>
              </div>
            </div>
          </div>
        )}
      </div>

      {/* ─── Uninstall Confirmation Dialog ─── */}
      {uninstallDialog && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
          <div className="w-[360px] rounded-xl bg-[#1a1a2e]/95 border border-white/[0.08] p-5 shadow-2xl">
            <h4 className="text-[14px] font-semibold text-ink-1 mb-2">Uninstall Extension</h4>
            <p className="text-[12.5px] text-ink-3 mb-5">
              Are you sure you want to uninstall{' '}
              <strong className="text-ink-1">{uninstallDialog.title}</strong>? This cannot be
              undone.
            </p>
            <div className="flex justify-end gap-2">
              <Button
                variant="quiet"
                onClick={() => setUninstallDialog(null)}
                disabled={busyUninstallExtName !== null}
              >
                Cancel
              </Button>
              <Button
                onClick={() =>
                  void handleUninstallExtension(uninstallDialog.extName, uninstallDialog.title)
                }
                disabled={busyUninstallExtName !== null}
              >
                {busyUninstallExtName ? 'Uninstalling…' : 'Uninstall'}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
