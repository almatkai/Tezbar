import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { tryConsumeCommandSurfaceEscape } from './escapeGate'
import CommandBar from './CommandBar'
import AgentChatView from './AgentChatView'
import SettingsView from './SettingsView'
import ExtensionsView from './ExtensionsView'
import ExtensionRuntimeView from './ExtensionRuntimeView'
import OpenPortsView from './OpenPortsView'
import PermissionsView from './PermissionsView'
import ClipboardView from './ClipboardView'
import NotesView from './NotesView'
import { RAYMES_NEW_SNIPPET_EVENT } from '../shared/snippetEvents'
import type { AiChatBoot } from '../shared/aiChatSurface'
import { RAYMES_AI_NEW_CHAT_EVENT, RAYMES_QUICK_NOTE_SHORTCUT_EVENT } from '../shared/aiChatSurface'
import SnippetsView from './SnippetsView'
import type { ExtensionRunCommandResult } from '../shared/extensionRuntime'
import EmojiPickerView from './EmojiPickerView'

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

  return (
    <div className="flex h-screen w-full bg-[#1e1f2e]">
      {surface === 'permissions' ? (
        <PermissionsView nativeWindow onBack={() => setSurface('settings')} />
      ) : (
        <SettingsView
          nativeWindow
          onBack={() => {
            void window.raymes.closeCurrentWindow()
          }}
          onOpenPermissions={() => setSurface('permissions')}
        />
      )}
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
    const off = window.raymes.onWindowShown(({ resetUi }) => {
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
    const off = window.raymes.onWindowSnapGuides((payload) => {
      setSnapGuides(payload)
    })
    return off
  }, [])

  useEffect(() => {
    return window.raymes.onAppSurfaceOpen((nextSurface) => {
      if (nextSurface === 'settings') {
        void window.raymes.openSettingsWindow()
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
      void window.raymes.startWindowSnapDrag()
    }

    const endDrag = (): void => {
      if (!dragActive) return
      dragActive = false
      void window.raymes.endWindowSnapDrag()
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

  // Global ⌘N — route by surface (snippets → new snippet; AI chat → new chat;
  // command bar → quick-note shortcut event for CommandBar).
  useEffect(() => {
    return window.raymes.onQuickNoteSaveShortcut(() => {
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
    })
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
        void window.raymes.hide()
        return
      }
      if ((e.metaKey || e.ctrlKey) && e.key === ',') {
        e.preventDefault()
        void window.raymes.openSettingsWindow()
        void window.raymes.hide()
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
      const zoomFactor = Math.max(1, window.raymes.getWindowZoomFactor())
      const measured = Math.ceil(cssHeight * zoomFactor)
      if (measured === lastReportedHeightRef.current) return
      lastReportedHeightRef.current = measured
      void window.raymes.setWindowContentHeight(measured, zoomFactor)
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
        className="no-drag relative z-0 flex h-full w-full animate-raymes-fade-in flex-col"
      >
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
              setSettingsInitialTab('ai')
              setSurface('settings')
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
              void window.raymes.openSettingsWindow()
              void window.raymes.hide()
            }}
            onConfigureAi={() => {
              setSettingsInitialTab('ai')
              setSurface('settings')
            }}
            onOpenExtensions={() => setSurface('extensions')}
            onOpenExtensionRuntime={(initial) => {
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
          />
        )}
      </div>
    </div>
  )
}

export default function App(): JSX.Element {
  return isSettingsWindow() ? <SettingsWindowApp /> : <LauncherApp />
}
