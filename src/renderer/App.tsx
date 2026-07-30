import React, { Suspense, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { tryConsumeCommandSurfaceEscape } from './escapeGate'
import CommandBar from './CommandBar'
import { RAYMES_NEW_SNIPPET_EVENT } from '../shared/snippetEvents'
import type { AiChatBoot } from '../shared/aiChatSurface'
import { RAYMES_AI_NEW_CHAT_EVENT, RAYMES_QUICK_NOTE_SHORTCUT_EVENT } from '../shared/aiChatSurface'
import type { ExtensionRunCommandResult } from '../shared/extensionRuntime'
import { DEFAULT_EXTENSION_RUNTIME_TIMEOUT_MS } from '../shared/llmConfig'
import type { TerminalSessionsAction } from '../shared/terminal'
import type { TerminalDefaults } from './terminalPreferences'

const AgentChatView = React.lazy(() => import('./AgentChatView'))
const SettingsView = React.lazy(() => import('./SettingsView'))
const ExtensionsView = React.lazy(() => import('./ExtensionsView'))
const ExtensionRuntimeView = React.lazy(() => import('./ExtensionRuntimeView'))
const OpenPortsView = React.lazy(() => import('./OpenPortsView'))
const SystemMonitorView = React.lazy(() => import('./SystemMonitorView'))
const IndexingView = React.lazy(() => import('./IndexingView'))
const PermissionsView = React.lazy(() => import('./PermissionsView'))
const ClipboardView = React.lazy(() => import('./ClipboardView'))
const NotesView = React.lazy(() => import('./NotesView'))
const SnippetsView = React.lazy(() => import('./SnippetsView'))
const EmojiPickerView = React.lazy(() => import('./EmojiPickerView'))
const TerminalView = React.lazy(() => import('./TerminalView'))
const TerminalSessionsWindow = React.lazy(() => import('./TerminalSessionsWindow'))

const SurfaceFallback = (): JSX.Element => (
  <div className="flex h-full w-full items-center justify-center text-[12px] text-ink-3">
    Loading…
  </div>
)

type Surface =
  | 'command'
  | 'ai-chat'
  | 'settings'
  | 'extensions'
  | 'extension-runtime'
  | 'open-ports'
  | 'system-monitor'
  | 'indexing'
  | 'permissions'
  | 'clipboard'
  | 'snippets'
  | 'notes'
  | 'emoji-picker'
  | 'terminal'

type SettingsTab =
  | 'general'
  | 'ai'
  | 'voice'
  | 'knowledge'
  | 'extensions'
  | 'permissions'
  | 'storage'
  | 'advanced'

const SETTINGS_TAB_STORAGE_KEY = 'tezbar:settings-tab'

function normalizeSettingsTab(tab: unknown): SettingsTab {
  return tab === 'ai' ||
    tab === 'voice' ||
    tab === 'knowledge' ||
    tab === 'extensions' ||
    tab === 'permissions' ||
    tab === 'storage' ||
    tab === 'advanced'
    ? tab
    : 'general'
}

async function openNativeSettings(tab: SettingsTab): Promise<void> {
  window.localStorage.setItem(SETTINGS_TAB_STORAGE_KEY, tab)
  // Opening the native window must not depend on the backend being healthy.
  // The config write is only used to select the initial tab; if the backend
  // is still starting (or has crashed), Settings should still be reachable.
  await window.tezbar.openSettingsWindow()
  void window.tezbar.setLlmConfig({ settingsInitialTab: tab }).catch((error: unknown) => {
    console.warn('[Settings] Failed to persist initial tab:', error)
  })
}

const PANEL_SELECTORS: Record<Exclude<Surface, 'command'>, string> = {
  'ai-chat': '[aria-label="AI Chat"]',
  settings: '[aria-label="Settings"]',
  extensions: '[aria-label="Extensions"]',
  'extension-runtime': '[aria-label="Extension Runtime"]',
  'open-ports': '[aria-label="Open Ports"]',
  'system-monitor': '[aria-label="System Monitor"]',
  indexing: '[aria-label="Indexing Status"]',
  permissions: '[aria-label="Permissions"]',
  clipboard: '[aria-label="Clipboard History"]',
  snippets: '[aria-label="Snippets"]',
  notes: '[aria-label="Quick Notes"]',
  'emoji-picker': '[aria-label="Emoji Picker"]',
  terminal: '[aria-label="Terminal"]',
}

/** How much vertical padding the outer app container adds. Kept in sync
 *  with the `p-2` below so we can report accurate content height to the
 *  main process (otherwise the window would be 16px too short). */
const OUTER_PADDING_PX = 16

const TIMED_SURFACE_CONFIG = {
  'extension-runtime': {
    configKey: 'extensionRuntimeTimeoutMs',
    defaultMs: DEFAULT_EXTENSION_RUNTIME_TIMEOUT_MS,
  },
  'ai-chat': {
    configKey: 'aiModeTimeoutMs',
    defaultMs: DEFAULT_EXTENSION_RUNTIME_TIMEOUT_MS,
  },
  terminal: {
    configKey: 'terminalModeTimeoutMs',
    defaultMs: DEFAULT_EXTENSION_RUNTIME_TIMEOUT_MS,
  },
} as const

function isSettingsWindow(): boolean {
  return (
    new URLSearchParams(window.location.search).get('window') === 'settings' ||
    window.__TEZBAR_WINDOW_LABEL__ === 'settings'
  )
}

function isTerminalSessionsWindow(): boolean {
  return (
    new URLSearchParams(window.location.search).get('window') === 'terminal-sessions' ||
    window.__TEZBAR_WINDOW_LABEL__ === 'terminal-sessions'
  )
}

function isSnapOverlayWindow(): boolean {
  return new URLSearchParams(window.location.search).get('window') === 'snap-overlay'
}

type SnapGuidesState = {
  visible: boolean
  snapX: boolean
  snapY: boolean
  centered: boolean
  targetRect: {
    left: number
    top: number
    right: number
    bottom: number
  } | null
}

function SnapOverlayApp(): JSX.Element {
  const [snapGuides, setSnapGuides] = useState<SnapGuidesState>({
    visible: true,
    snapX: false,
    snapY: false,
    centered: false,
    targetRect: null,
  })

  useEffect(() => {
    let mounted = true
    const unlisten = window.tezbar.onWindowSnapGuides(setSnapGuides)
    // On Windows, the native overlay can be ready a little after its first
    // event is emitted. Fetch the current state so its helper lines render
    // even if that initial event was missed.
    void window.tezbar
      .getWindowSnapGuides()
      .then((guides) => {
        if (mounted) setSnapGuides(guides)
      })
      .catch(() => undefined)
    return () => {
      mounted = false
      unlisten()
    }
  }, [])

  return (
    <div
      aria-hidden
      className={[
        'snap-overlay',
        snapGuides.visible ? 'is-visible' : '',
        snapGuides.snapX ? 'is-snapped-x' : '',
        snapGuides.snapY ? 'is-snapped-y' : '',
        snapGuides.centered ? 'is-centered' : '',
      ]
        .filter(Boolean)
        .join(' ')}
    >
      {snapGuides.targetRect ? (
        <>
          <div
            className="snap-overlay__edge snap-overlay__edge--left"
            style={{ left: `${snapGuides.targetRect.left}%` }}
          />
          <div
            className="snap-overlay__edge snap-overlay__edge--top"
            style={{ top: `${snapGuides.targetRect.top}%` }}
          />
          <div
            className="snap-overlay__edge snap-overlay__edge--right"
            style={{ left: `${snapGuides.targetRect.right}%` }}
          />
          <div
            className="snap-overlay__edge snap-overlay__edge--bottom"
            style={{ top: `${snapGuides.targetRect.bottom}%` }}
          />
        </>
      ) : null}
      <div className="snap-overlay__center-indicator" />
    </div>
  )
}

function SettingsWindowApp(): JSX.Element {
  const [surface, setSurface] = useState<'settings' | 'permissions'>('settings')
  const [settingsTab, setSettingsTab] = useState<SettingsTab>(() => {
    const storedTab = window.localStorage.getItem(SETTINGS_TAB_STORAGE_KEY)
    return normalizeSettingsTab(storedTab ?? 'general')
  })

  useEffect(() => {
    let cancelled = false
    const storedTab = window.localStorage.getItem(SETTINGS_TAB_STORAGE_KEY)
    void window.tezbar
      .getLlmConfig()
      .then((config) => {
        if (!cancelled && storedTab === null) {
          setSettingsTab(normalizeSettingsTab(config.settingsInitialTab))
        }
      })
      .catch((error: unknown) => {
        console.warn('[SettingsWindow] Failed to load initial settings tab:', error)
      })

    const onStorage = (event: StorageEvent): void => {
      if (event.key !== SETTINGS_TAB_STORAGE_KEY) return
      setSettingsTab(normalizeSettingsTab(event.newValue))
      setSurface('settings')
    }
    window.addEventListener('storage', onStorage)
    return () => {
      cancelled = true
      window.removeEventListener('storage', onStorage)
    }
  }, [])

  useEffect(() => {
    // Native code hides the launcher on every Settings activation. Repeat the
    // invariant after first paint as a safeguard for startup and WebView reloads.
    const frame = window.requestAnimationFrame(() => {
      void window.tezbar.hideLauncherForSettings().catch((error: unknown) => {
        console.warn('[SettingsWindow] Failed to hide launcher:', error)
      })
    })
    return () => window.cancelAnimationFrame(frame)
  }, [])

  return (
    <div className="flex h-screen w-full bg-[#1e1f2e]">
      <Suspense fallback={<SurfaceFallback />}>
        {surface === 'permissions' ? (
          <PermissionsView nativeWindow onBack={() => setSurface('settings')} />
        ) : (
          <SettingsView
            key={settingsTab}
            initialTab={settingsTab}
            nativeWindow
            onBack={() => {
              void window.tezbar.closeCurrentWindow()
            }}
            onOpenPermissions={() => setSurface('permissions')}
            onBrowseStore={() => {
              void window.tezbar.openExtensionStore().then(() => {
                void window.tezbar.closeCurrentWindow()
              })
            }}
          />
        )}
      </Suspense>
    </div>
  )
}

function LauncherApp(): JSX.Element {
  const [surface, setSurface] = useState<Surface>('command')
  const [settingsInitialTab, setSettingsInitialTab] = useState<SettingsTab>('general')
  const [openPortsInitialTab, setOpenPortsInitialTab] = useState<'listen' | 'named'>('listen')
  const [notesInitialSelectedId, setNotesInitialSelectedId] = useState<number | null>(null)
  const [commandInitialValue, setCommandInitialValue] = useState('')
  const [commandInitialSelectedChatId, setCommandInitialSelectedChatId] = useState<string | null>(
    null
  )
  const [terminalInitialCommand, setTerminalInitialCommand] = useState<string | undefined>()
  const [terminalInitialSessionId, setTerminalInitialSessionId] = useState<string | undefined>()
  const [terminalWorkingDirectory, setTerminalWorkingDirectory] = useState<string | undefined>()
  const [terminalDefaults, setTerminalDefaults] = useState<TerminalDefaults | undefined>()
  const [aiChatBoot, setAiChatBoot] = useState<AiChatBoot>({ kind: 'panel' })
  const [aiChatKey, setAiChatKey] = useState(0)
  const [extensionRuntimeInitial, setExtensionRuntimeInitial] = useState<Extract<
    ExtensionRunCommandResult,
    { ok: true; mode: 'view' }
  > | null>(null)
  const contentRef = useRef<HTMLDivElement | null>(null)
  const lastReportedHeightRef = useRef<number>(-1)
  const surfaceRef = useRef<Surface>('command')

  const focusSurface = (nextSurface: Surface): void => {
    requestAnimationFrame(() => {
      if (nextSurface !== 'command') {
        const panel = document.querySelector<HTMLElement>(PANEL_SELECTORS[nextSurface])
        if (panel) {
          panel.focus()
          return
        }
      }
      document.getElementById('command-input')?.focus()
    })
  }

  useEffect(() => {
    const off = window.tezbar.onWindowShown(({ resetUi }) => {
      if (resetUi) {
        setSurface('command')
        setExtensionRuntimeInitial(null)
      }
      focusSurface(resetUi ? 'command' : surface)
    })
    return off
  }, [surface])

  useEffect(() => {
    focusSurface(surface)
  }, [surface])

  useEffect(() => {
    surfaceRef.current = surface
  }, [surface])

  useEffect(() => {
    const timeoutConfig = TIMED_SURFACE_CONFIG[surface as keyof typeof TIMED_SURFACE_CONFIG]
    if (!timeoutConfig) return

    let timeoutId: number | null = null
    let cancelled = false
    let timeoutMs = timeoutConfig.defaultMs

    const returnToCommandBar = (): void => {
      if (cancelled) return
      setCommandInitialValue('')
      setExtensionRuntimeInitial(null)
      setCommandInitialSelectedChatId(null)
      setTerminalInitialCommand(undefined)
      setTerminalInitialSessionId(undefined)
      setTerminalWorkingDirectory(undefined)
      setTerminalDefaults(undefined)
      setSurface('command')
    }

    const scheduleReturn = (): void => {
      if (timeoutId !== null) window.clearTimeout(timeoutId)
      if (timeoutMs <= 0) return
      timeoutId = window.setTimeout(returnToCommandBar, timeoutMs)
    }

    const onActivity = (): void => scheduleReturn()
    window.addEventListener('pointerdown', onActivity, true)
    window.addEventListener('keydown', onActivity, true)
    window.addEventListener('input', onActivity, true)
    window.addEventListener('wheel', onActivity, true)

    void window.tezbar
      .getLlmConfig()
      .then((config) => {
        if (cancelled) return
        const configured = config[timeoutConfig.configKey]
        if (typeof configured === 'number' && Number.isFinite(configured) && configured >= 0) {
          timeoutMs = configured
        }
        scheduleReturn()
      })
      .catch(() => scheduleReturn())

    return () => {
      cancelled = true
      if (timeoutId !== null) window.clearTimeout(timeoutId)
      window.removeEventListener('pointerdown', onActivity, true)
      window.removeEventListener('keydown', onActivity, true)
      window.removeEventListener('input', onActivity, true)
      window.removeEventListener('wheel', onActivity, true)
    }
  }, [surface])

  useEffect(() => {
    return window.tezbar.onAppSurfaceOpen((nextSurface) => {
      if (nextSurface === 'settings') {
        void openNativeSettings('general')
        return
      }
      setSurface(nextSurface)
      focusSurface(nextSurface)
    })
  }, [])

  useEffect(() => {
    return window.tezbar.onRunExtensionCommandFromHotkey(({ extensionId, commandName }) => {
      void window.tezbar.extensionRunCommand({ extensionId, commandName }).then((result) => {
        if (result.ok && result.mode === 'view') {
          setCommandInitialValue('')
          setExtensionRuntimeInitial(result)
          setSurface('extension-runtime')
        }
      })
    })
  }, [])

  useEffect(() => {
    const openTerminalFromSessions = (action: TerminalSessionsAction): void => {
      void window.tezbar.terminalSessionsHide()
      setTerminalInitialCommand(undefined)
      setTerminalWorkingDirectory(undefined)
      setTerminalInitialSessionId(action.type === 'select' ? action.sessionId : undefined)
      setCommandInitialValue('')
      setSurface('terminal')
    }

    return window.tezbar.onTerminalSessionsAction(openTerminalFromSessions)
  }, [])

  useEffect(() => {
    const isWindows = navigator.platform.includes('Win')
    let dragActive = false

    const isNoDragTarget = (target: HTMLElement): boolean => {
      return Boolean(
        target.closest(
          '.no-drag, input, textarea, select, button, a[href], [role="button"], [role="menuitem"], [role="option"], [contenteditable="true"], .glass-card > *, .glass-panel > *, .tezbar-popover, .agent-chat-shell > *, .tezbar-settings-window > *, .cleanmymac-sidebar'
        )
      )
    }

    const onMouseDown = (event: MouseEvent): void => {
      if (event.button !== 0) return
      const target = event.target
      if (!(target instanceof HTMLElement)) return
      if (!target.closest('.drag-region')) return
      if (isNoDragTarget(target)) return
      if (!isWindows) dragActive = true
      void window.tezbar.startWindowSnapDrag()
    }

    const endDrag = (): void => {
      if (!dragActive) return
      dragActive = false
      void window.tezbar.endWindowSnapDrag()
    }

    const onVisibilityChange = (): void => {
      if (document.visibilityState !== 'visible') endDrag()
    }

    window.addEventListener('mousedown', onMouseDown, true)
    // Showing the click-through guide WebView can make WebView2 synthesize
    // blur/mouseup/visibility events. On Windows the host tracks the physical
    // button and ends the drag itself, so renderer end signals are unnecessary.
    if (!isWindows) {
      window.addEventListener('mouseup', endDrag, true)
      window.addEventListener('blur', endDrag)
      document.addEventListener('visibilitychange', onVisibilityChange)
    }
    return () => {
      window.removeEventListener('mousedown', onMouseDown, true)
      if (!isWindows) {
        window.removeEventListener('mouseup', endDrag, true)
        window.removeEventListener('blur', endDrag)
        document.removeEventListener('visibilitychange', onVisibilityChange)
      }
    }
  }, [])

  // Local ⌘N / Ctrl+N — route by surface when the app is focused
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      const isCmdOrCtrl = e.metaKey || e.ctrlKey
      if (isCmdOrCtrl && e.key.toLowerCase() === 'n') {
        e.preventDefault()
        e.stopPropagation()
        const s = surfaceRef.current
        if (s === 'ai-chat') {
          window.dispatchEvent(new Event(RAYMES_AI_NEW_CHAT_EVENT))
          return
        }
        if (s === 'snippets') {
          window.dispatchEvent(new Event(RAYMES_NEW_SNIPPET_EVENT))
          return
        }
        window.dispatchEvent(new Event(RAYMES_QUICK_NOTE_SHORTCUT_EVENT))
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [])

  // Global key routing fallback.
  //
  // Each sub-view (Settings, Providers, Permissions, Clipboard, …) attaches
  // its own capture-phase Escape handler so it can express nuance — e.g.
  // "Escape clears the search box before navigating back". They all call
  // stopPropagation() when they handle the event, which skips this
  // handler.
  //
  // When no sub-view handled it (timing edge cases, or a simple view that
  // didn't bother wiring its own listener), we still do the right thing:
  // from any sub-surface we pop back to `command`, and only from the
  // command surface does Escape actually hide the launcher. That
  // guarantee is the "back not close" contract users rely on.
  // The global shortcut Cmd+Escape also hides the window from anywhere.
  //
  // On the command surface, `CommandBar` may still need Escape first
  // (pin picker, pending extension form). It registers a consumer via
  // `escapeGate` so we never hide the window while that UI is open.
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') {
        e.preventDefault()
        if (surface === 'ai-chat') {
          if (tryConsumeCommandSurfaceEscape()) {
            e.stopPropagation()
            return
          }
          setCommandInitialValue(' ')
          setSurface('command')
          return
        }
        if (surface === 'terminal') {
          setCommandInitialValue('>')
          setSurface('command')
          return
        }
        if (surface !== 'command') {
          setSurface('command')
          return
        }
        if (tryConsumeCommandSurfaceEscape()) {
          e.stopPropagation()
          return
        }
        void window.tezbar.hide()
        return
      }
      if ((e.metaKey || e.ctrlKey) && e.key === ',') {
        e.preventDefault()
        void openNativeSettings('ai')
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [surface])

  // Report intrinsic content height rather than only the viewport height.
  // At page zoom > 100%, CSS pixels require more native window pixels;
  // scaling the report prevents footer chrome and wrapped hints from clipping.
  useLayoutEffect(() => {
    const el = contentRef.current
    if (!el) return

    const report = (): void => {
      const cssHeight =
        Math.max(el.getBoundingClientRect().height, el.scrollHeight) + OUTER_PADDING_PX
      const zoomFactor = Math.max(1, window.tezbar.getWindowZoomFactor())
      const measured = Math.ceil(cssHeight * zoomFactor)
      if (measured === lastReportedHeightRef.current) return
      lastReportedHeightRef.current = measured
      void window.tezbar.setWindowContentHeight(measured, zoomFactor)
    }

    report()
    const observer = new ResizeObserver(() => report())
    observer.observe(el)
    window.addEventListener('resize', report)
    window.visualViewport?.addEventListener('resize', report)
    return () => {
      observer.disconnect()
      window.removeEventListener('resize', report)
      window.visualViewport?.removeEventListener('resize', report)
    }
  }, [surface])

  return (
    <div
      className={[
        'glass-shell drag-region flex h-screen w-full p-2',
        surface === 'terminal' ? 'glass-shell--terminal' : '',
      ]
        .filter(Boolean)
        .join(' ')}
    >
      <div
        ref={contentRef}
        key={surface}
        className="relative z-0 flex h-full w-full animate-tezbar-fade-in flex-col"
      >
        <Suspense fallback={<SurfaceFallback />}>
          {surface === 'settings' ? (
            <SettingsView
              initialTab={settingsInitialTab}
              onBack={() => setSurface('command')}
              onOpenPermissions={() => setSurface('permissions')}
              onBrowseStore={() => setSurface('extensions')}
            />
          ) : surface === 'extensions' ? (
            <ExtensionsView onBack={() => setSurface('command')} />
          ) : surface === 'extension-runtime' && extensionRuntimeInitial ? (
            <ExtensionRuntimeView
              initial={extensionRuntimeInitial}
              onBack={() => {
                setSurface('command')
              }}
            />
          ) : surface === 'open-ports' ? (
            <OpenPortsView
              initialTab={openPortsInitialTab}
              onBack={() => {
                setOpenPortsInitialTab('listen')
                setSurface('command')
              }}
            />
          ) : surface === 'system-monitor' ? (
            <SystemMonitorView onBack={() => setSurface('command')} />
          ) : surface === 'indexing' ? (
            <IndexingView
              onBack={() => setSurface('command')}
              onOpenSettings={() => {
                void openNativeSettings('knowledge')
              }}
            />
          ) : surface === 'permissions' ? (
            <PermissionsView onBack={() => setSurface('settings')} />
          ) : surface === 'clipboard' ? (
            <ClipboardView onBack={() => setSurface('command')} />
          ) : surface === 'snippets' ? (
            <SnippetsView onBack={() => setSurface('command')} />
          ) : surface === 'notes' ? (
            <NotesView
              onBack={() => setSurface('command')}
              initialSelectedNoteId={notesInitialSelectedId}
            />
          ) : surface === 'emoji-picker' ? (
            <EmojiPickerView onBack={() => setSurface('command')} />
          ) : surface === 'ai-chat' ? (
            <AgentChatView
              key={aiChatKey}
              boot={aiChatBoot}
              onBack={() => setSurface('command')}
              onOpenSettings={() => {
                void openNativeSettings('ai')
              }}
            />
          ) : surface === 'terminal' ? (
            <TerminalView
              embedded
              initialCommand={terminalInitialCommand}
              initialSessionId={terminalInitialSessionId}
              workingDirectory={terminalWorkingDirectory}
              defaults={terminalDefaults}
              onBack={() => {
                setTerminalInitialCommand(undefined)
                setTerminalInitialSessionId(undefined)
                setTerminalWorkingDirectory(undefined)
                setCommandInitialValue('>')
                setSurface('command')
              }}
            />
          ) : (
            <CommandBar
              initialValue={commandInitialValue}
              initialSelectedChatId={commandInitialSelectedChatId}
              onOpenAiChat={(nextBoot) => {
                setAiChatBoot(nextBoot)
                setCommandInitialValue('')
                setCommandInitialSelectedChatId(
                  nextBoot.kind === 'resume' ? nextBoot.sessionId : null
                )
                setAiChatKey((k) => k + 1)
                setSurface('ai-chat')
              }}
              onOpenSettings={() => {
                setSettingsInitialTab('general')
                void openNativeSettings('general')
              }}
              onOpenExtensionsSettings={() => {
                setSettingsInitialTab('extensions')
                void openNativeSettings('extensions')
              }}
              onConfigureAi={() => {
                void openNativeSettings('ai')
              }}
              onOpenExtensions={() => {
                setCommandInitialValue('')
                setSurface('extensions')
              }}
              onOpenExtensionRuntime={(initial) => {
                setCommandInitialValue('')
                setExtensionRuntimeInitial(initial)
                setSurface('extension-runtime')
              }}
              onOpenPortsPage={(opts) => {
                setOpenPortsInitialTab(opts?.tab ?? 'listen')
                setSurface('open-ports')
              }}
              onOpenSystemMonitor={() => {
                setCommandInitialValue('')
                setSurface('system-monitor')
              }}
              onOpenIndexingPage={() => {
                setCommandInitialValue('')
                setSurface('indexing')
              }}
              onOpenClipboardPage={() => setSurface('clipboard')}
              onOpenSnippetsPage={() => setSurface('snippets')}
              onOpenNotesPage={(opts) => {
                setNotesInitialSelectedId(
                  typeof opts?.createdAt === 'number' ? opts.createdAt : null
                )
                setSurface('notes')
              }}
              onOpenEmojiPicker={() => setSurface('emoji-picker')}
              onOpenTerminal={(initialCommand, workingDirectory, sessionId, defaults) => {
                setTerminalInitialCommand(initialCommand)
                setTerminalInitialSessionId(sessionId)
                setTerminalWorkingDirectory(workingDirectory)
                setTerminalDefaults(defaults)
                setCommandInitialValue('')
                setSurface('terminal')
              }}
            />
          )}
        </Suspense>
      </div>
    </div>
  )
}

export default function App(): JSX.Element {
  if (isSnapOverlayWindow()) {
    return <SnapOverlayApp />
  }
  if (isTerminalSessionsWindow()) {
    return (
      <Suspense fallback={<SurfaceFallback />}>
        <TerminalSessionsWindow />
      </Suspense>
    )
  }
  return isSettingsWindow() ? <SettingsWindowApp /> : <LauncherApp />
}
