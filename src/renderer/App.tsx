import React, { Suspense, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { tryConsumeCommandSurfaceEscape } from './escapeGate'
import CommandBar from './CommandBar'
import { RAYMES_NEW_SNIPPET_EVENT } from '../shared/snippetEvents'
import type { AiChatBoot } from '../shared/aiChatSurface'
import { RAYMES_AI_NEW_CHAT_EVENT, RAYMES_QUICK_NOTE_SHORTCUT_EVENT } from '../shared/aiChatSurface'
import type { ExtensionRunCommandResult } from '../shared/extensionRuntime'
import type { TerminalPromptInfo } from '../shared/terminal'
import { Hint, HintBar, Kbd } from './ui/primitives'

const AgentChatView = React.lazy(() => import('./AgentChatView'))
const SettingsView = React.lazy(() => import('./SettingsView'))
const ExtensionsView = React.lazy(() => import('./ExtensionsView'))
const ExtensionRuntimeView = React.lazy(() => import('./ExtensionRuntimeView'))
const OpenPortsView = React.lazy(() => import('./OpenPortsView'))
const PermissionsView = React.lazy(() => import('./PermissionsView'))
const ClipboardView = React.lazy(() => import('./ClipboardView'))
const NotesView = React.lazy(() => import('./NotesView'))
const SnippetsView = React.lazy(() => import('./SnippetsView'))
const EmojiPickerView = React.lazy(() => import('./EmojiPickerView'))
const TerminalView = React.lazy(() => import('./TerminalView'))

const SurfaceFallback = (): JSX.Element => (
  <div className="flex h-full w-full items-center justify-center text-[12px] text-ink-3">Loading…</div>
)

type Surface =
  | 'command'
  | 'ai-chat'
  | 'settings'
  | 'extensions'
  | 'extension-runtime'
  | 'open-ports'
  | 'permissions'
  | 'clipboard'
  | 'snippets'
  | 'notes'
  | 'emoji-picker'
  | 'terminal'

type SettingsTab = 'general' | 'ai' | 'voice' | 'permissions' | 'storage' | 'advanced'

const SETTINGS_TAB_STORAGE_KEY = 'tezbar:settings-tab'

function normalizeSettingsTab(tab: unknown): SettingsTab {
  return tab === 'ai' || tab === 'voice' || tab === 'permissions' || tab === 'storage' || tab === 'advanced'
    ? tab
    : 'general'
}

async function openNativeSettings(tab: SettingsTab): Promise<void> {
  window.localStorage.setItem(SETTINGS_TAB_STORAGE_KEY, tab)
  await window.tezbar.setLlmConfig({ settingsInitialTab: tab })
  await window.tezbar.openSettingsWindow()
  await window.tezbar.hide()
}

const PANEL_SELECTORS: Record<Exclude<Surface, 'command'>, string> = {
  'ai-chat': '[aria-label="AI Chat"]',
  settings: '[aria-label="Settings"]',
  extensions: '[aria-label="Extensions"]',
  'extension-runtime': '[aria-label="Extension Runtime"]',
  'open-ports': '[aria-label="Open Ports"]',
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

function isSettingsWindow(): boolean {
  return new URLSearchParams(window.location.search).get('window') === 'settings'
}

function SettingsWindowApp(): JSX.Element {
  const [surface, setSurface] = useState<'settings' | 'permissions'>('settings')
  const [settingsTab, setSettingsTab] = useState<SettingsTab | null>(null)

  useEffect(() => {
    let cancelled = false
    void window.tezbar.getLlmConfig().then((config) => {
      if (!cancelled) setSettingsTab(normalizeSettingsTab(config.settingsInitialTab))
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

  if (settingsTab === null) {
    return <div className="h-screen w-full bg-[#1e1f2e]" />
  }

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
          />
        )}
      </Suspense>
    </div>
  )
}

function LauncherApp(): JSX.Element {
  const [snapGuides, setSnapGuides] = useState<{ visible: boolean; active: boolean }>({
    visible: false,
    active: false,
  })
  const [surface, setSurface] = useState<Surface>('command')
  const [settingsInitialTab, setSettingsInitialTab] = useState<'general' | 'ai' | 'voice' | 'permissions' | 'advanced'>('general')
  const [openPortsInitialTab, setOpenPortsInitialTab] = useState<'listen' | 'named'>('listen')
  const [notesInitialSelectedId, setNotesInitialSelectedId] = useState<number | null>(null)
  const [commandInitialValue, setCommandInitialValue] = useState('')
  const [commandInitialSelectedChatId, setCommandInitialSelectedChatId] = useState<string | null>(
    null
  )
  const [terminalInitialCommand, setTerminalInitialCommand] = useState<string | undefined>()
  const [terminalPromptInfo, setTerminalPromptInfo] = useState<TerminalPromptInfo | null>(null)
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
    if (surface === 'terminal') {
      void window.tezbar.getTerminalPromptInfo().then(setTerminalPromptInfo)
    }
  }, [surface])

  useEffect(() => {
    const off = window.tezbar.onWindowSnapGuides((payload) => {
      setSnapGuides(payload)
    })
    return off
  }, [])

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
    let dragActive = false

    const isNoDragTarget = (target: HTMLElement): boolean => {
      return Boolean(
        target.closest(
          '.no-drag, input, textarea, select, button, a[href], [role="button"], [role="menuitem"], [role="option"], [contenteditable="true"]'
        )
      )
    }

    const onMouseDown = (event: MouseEvent): void => {
      if (event.button !== 0) return
      const target = event.target
      if (!(target instanceof HTMLElement)) return
      if (!target.closest('.drag-region')) return
      if (isNoDragTarget(target)) return
      dragActive = true
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
    window.addEventListener('mouseup', endDrag, true)
    window.addEventListener('blur', endDrag)
    document.addEventListener('visibilitychange', onVisibilityChange)
    return () => {
      window.removeEventListener('mousedown', onMouseDown, true)
      window.removeEventListener('mouseup', endDrag, true)
      window.removeEventListener('blur', endDrag)
      document.removeEventListener('visibilitychange', onVisibilityChange)
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
          if (tryConsumeCommandSurfaceEscape()) return
          setCommandInitialValue(' ')
          setSurface('command')
          return
        }
        if (surface !== 'command') {
          setSurface('command')
          return
        }
        if (tryConsumeCommandSurfaceEscape()) {
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
      const cssHeight = Math.max(el.getBoundingClientRect().height, el.scrollHeight) + OUTER_PADDING_PX
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
    <div className="glass-shell drag-region flex h-screen w-full p-2">
      <div
        aria-hidden
        className={[
          'window-snap-guides',
          snapGuides.visible ? 'is-visible' : '',
          snapGuides.active ? 'is-active' : '',
        ]
          .filter(Boolean)
          .join(' ')}
      >
        <div className="window-snap-guides__zone window-snap-guides__zone--horizontal" />
        <div className="window-snap-guides__zone window-snap-guides__zone--vertical" />
        <div className="window-snap-guides__horizontal" />
        <div className="window-snap-guides__tick window-snap-guides__tick--top" />
        <div className="window-snap-guides__tick window-snap-guides__tick--bottom" />
        <div className="window-snap-guides__label">Snap zone</div>
      </div>
      <div
        ref={contentRef}
        key={surface}
        className="no-drag relative z-0 flex h-full w-full animate-tezbar-fade-in flex-col"
      >
        <Suspense fallback={<SurfaceFallback />}>
          {surface === 'settings' ? (
            <SettingsView
              initialTab={settingsInitialTab}
              onBack={() => setSurface('command')}
              onOpenPermissions={() => setSurface('permissions')}
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
            <div className="flex h-full w-full flex-col gap-2">
              <div className="glass-card relative z-30 shrink-0 px-4 py-3 animate-tezbar-scale-in">
                <div className="flex items-center gap-3">
                  <span className="text-emerald-300">
                    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden>
                      <rect x="1.5" y="1.5" width="11" height="11" rx="2" stroke="currentColor" strokeWidth="1.3" />
                      <path d="M4 5.5L6.5 7L4 8.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
                      <path d="M8 9.5H10.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
                    </svg>
                  </span>
                  <span className="inline-flex shrink-0 items-center gap-1 rounded-tezbar-chip border border-emerald-400/35 bg-emerald-500/15 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-emerald-200">
                    <span className="font-mono text-[11px] leading-none">&gt;_</span>
                    Terminal
                  </span>
                  <span className="shrink-0 font-mono text-[13px] text-emerald-300/80">
                    {terminalPromptInfo ? `${terminalPromptInfo.user}@${terminalPromptInfo.host} ${terminalPromptInfo.dir} %` : ''}
                  </span>
                  <span className="font-mono text-[15px] text-ink-1">
                    {terminalInitialCommand}
                  </span>
                </div>
              </div>
              <TerminalView
                embedded
                initialCommand={terminalInitialCommand}
                onBack={() => {
                  setTerminalInitialCommand(undefined)
                  setTerminalPromptInfo(null)
                  setCommandInitialValue('')
                  setSurface('command')
                }}
              />
              <div className="glass-card shrink-0 px-4 py-2 animate-tezbar-scale-in">
                <HintBar>
                  <Hint label="Close" keys={<Kbd>Esc</Kbd>} />
                  <Hint label="Hide window" keys={<><Kbd>Esc</Kbd><Kbd>⌘</Kbd></>} />
                </HintBar>
              </div>
            </div>
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
              onOpenClipboardPage={() => setSurface('clipboard')}
              onOpenSnippetsPage={() => setSurface('snippets')}
              onOpenNotesPage={(opts) => {
                setNotesInitialSelectedId(typeof opts?.createdAt === 'number' ? opts.createdAt : null)
                setSurface('notes')
              }}
              onOpenEmojiPicker={() => setSurface('emoji-picker')}
              onOpenTerminal={(initialCommand) => {
                setTerminalInitialCommand(initialCommand)
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
  return isSettingsWindow() ? <SettingsWindowApp /> : <LauncherApp />
}
