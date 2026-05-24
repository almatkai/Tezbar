/** Raycast API shim. We expose a pragmatic subset of `@raycast/api` that
 *  covers the pieces no-view extensions actually call at runtime. The goal
 *  is two-fold:
 *
 *  1. Keep the module evaluation side-effect free for commands we can't run
 *     yet (view / menu-bar). Extensions commonly import `List`, `Form`,
 *     `Detail`, `Action`, `ActionPanel`, `Icon`, `Color` at module scope —
 *     a throwing import would make the whole file unloadable in the
 *     compatibility harness too.
 *  2. Actually do something useful for the APIs no-view extensions rely on:
 *     preferences, local storage, clipboard, open, environment, toasts/HUD.
 *
 *  Anything rendering-related is a no-op proxy. Extensions that try to drive
 *  a view from the main process will appear silent; the runtime itself
 *  rejects view/menu-bar modes before eval, so users still see a clear
 *  "not supported yet" message via `executeExtensionCommandRuntime`. */

import { app, clipboard, nativeImage, shell } from 'electron'
import { execFile } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { promisify } from 'node:util'

export type RuntimeFeedback = {
  kind: 'toast' | 'hud'
  style?: string
  title?: string
  message?: string
}

/** Options passed when wiring up a shim for a specific extension run. */
export type ShimContext = {
  extensionId: string
  commandName: string
  /** Absolute path to the extension's installed package root. */
  packageRoot: string
  /** Collector — toasts/HUD pushed here are inspected by the caller to
   *  decide the visible feedback after the command resolves. */
  feedback: RuntimeFeedback[]
}

const TOAST_STYLE = {
  Success: 'success',
  Failure: 'failure',
  Animated: 'animated',
} as const

const execFileAsync = promisify(execFile)

async function runAppleScript(source: string): Promise<string> {
  if (process.platform !== 'darwin') {
    throw new Error('AppleScript is only available on macOS')
  }
  if (typeof source !== 'string' || source.trim().length === 0) {
    return ''
  }

  const { stdout } = await execFileAsync('/usr/bin/osascript', ['-e', source], {
    encoding: 'utf8',
    maxBuffer: 10 * 1024 * 1024,
  })
  return String(stdout).replace(/\r?\n$/, '')
}

/** An identity-style proxy used for rendering primitives. Any property
 *  access returns another proxy; it's callable (returns itself) so
 *  `List.Item`, `Action.Submit`, `<Form.TextField />` etc. all survive
 *  top-level evaluation. We never try to render these — the runtime
 *  refuses view/menu-bar modes before it evaluates user code. */
function createRenderProxy(name: string): unknown {
  const target = function () {
    return undefined
  }
  Object.defineProperty(target, 'name', { value: name })
  return new Proxy(target, {
    get(_t, prop) {
      if (prop === Symbol.toPrimitive) return () => `[Raycast:${name}]`
      if (prop === 'displayName') return name
      if (prop === 'prototype') return {}
      if (typeof prop === 'symbol') return undefined
      return createRenderProxy(`${name}.${String(prop)}`)
    },
    apply() {
      return undefined
    },
    construct() {
      return {}
    },
  })
}

/** Per-extension local storage. Backed by a JSON file so a subsequent run
 *  of the same command sees what the previous run wrote — this is what
 *  Raycast's own `LocalStorage` guarantees. We deliberately keep it
 *  synchronous to match the API surface; extensions `await` it anyway so
 *  the blocking read is fine here. */
function createLocalStorage(packageRoot: string): Record<string, unknown> {
  const file = join(packageRoot, 'localStorage.json')
  const readAll = (): Record<string, string> => {
    try {
      const raw = readFileSync(file, 'utf8')
      const parsed = JSON.parse(raw) as Record<string, string>
      return parsed && typeof parsed === 'object' ? parsed : {}
    } catch {
      return {}
    }
  }
  const writeAll = (value: Record<string, string>): void => {
    mkdirSync(packageRoot, { recursive: true })
    writeFileSync(file, JSON.stringify(value, null, 2), 'utf8')
  }

  return {
    getItem: async (key: string): Promise<string | undefined> => readAll()[key],
    setItem: async (key: string, value: string): Promise<void> => {
      const all = readAll()
      all[key] = value
      writeAll(all)
    },
    removeItem: async (key: string): Promise<void> => {
      const all = readAll()
      delete all[key]
      writeAll(all)
    },
    clear: async (): Promise<void> => {
      writeAll({})
    },
    allItems: async (): Promise<Record<string, string>> => readAll(),
  }
}

function readPreferences(packageRoot: string): Record<string, unknown> {
  const file = join(packageRoot, 'preferences.json')
  try {
    const raw = readFileSync(file, 'utf8')
    const parsed = JSON.parse(raw) as Record<string, unknown>
    return parsed && typeof parsed === 'object' ? parsed : {}
  } catch {
    return {}
  }
}

function createClipboardShim(): Record<string, unknown> {
  return {
    copy: async (value: unknown): Promise<void> => {
      if (value && typeof value === 'object' && 'text' in value) {
        const v = (value as { text?: unknown }).text
        if (typeof v === 'string') {
          clipboard.writeText(v)
          return
        }
      }
      if (typeof value === 'string') clipboard.writeText(value)
      else clipboard.writeText(String(value))
    },
    paste: async (value: unknown): Promise<void> => {
      if (typeof value === 'string') clipboard.writeText(value)
      else if (value && typeof value === 'object' && 'text' in value) {
        const v = (value as { text?: unknown }).text
        if (typeof v === 'string') clipboard.writeText(v)
      }
      // Raycast's `paste` also types the content into the frontmost app.
      // We can't do that safely without Accessibility wiring, so we just
      // copy — extensions tend to show a toast right after either way.
    },
    readText: async (): Promise<string> => clipboard.readText(),
    read: async (): Promise<Record<string, string | undefined>> => ({
      text: clipboard.readText() || undefined,
    }),
    clear: async (): Promise<void> => clipboard.clear(),
  }
}

function createEnvironment(ctx: ShimContext): Record<string, unknown> {
  const supportPath = join(ctx.packageRoot, 'support')
  try {
    mkdirSync(supportPath, { recursive: true })
  } catch {
    // non-fatal
  }
  return {
    appearance: 'dark',
    commandName: ctx.commandName,
    commandMode: 'no-view',
    extensionName: ctx.extensionId,
    raycastVersion: '1.77.0',
    isDevelopment: !app.isPackaged,
    supportPath,
    assetsPath: join(ctx.packageRoot, 'assets'),
    launchType: 'userInitiated',
    textSize: 'medium',
  }
}

export function createRaycastApi(ctx: ShimContext): Record<string, unknown> {
  return {
    Toast: { Style: TOAST_STYLE },
    Icon: createRenderProxy('Icon'),
    Color: createRenderProxy('Color'),
    Image: createRenderProxy('Image'),
    List: createRenderProxy('List'),
    Form: createRenderProxy('Form'),
    Detail: createRenderProxy('Detail'),
    Grid: createRenderProxy('Grid'),
    Action: createRenderProxy('Action'),
    ActionPanel: createRenderProxy('ActionPanel'),
    MenuBarExtra: createRenderProxy('MenuBarExtra'),
    Alert: {
      ActionStyle: { Destructive: 'destructive', Cancel: 'cancel', Default: 'default' },
    },
    Keyboard: {
      Shortcut: { Common: {} },
    },
    OAuth: createRenderProxy('OAuth'),
    BrowserExtension: createRenderProxy('BrowserExtension'),
    AI: {
      ask: async (prompt: string): Promise<string> => {
        // We don't ship a Raycast-AI equivalent. Return the prompt itself so
        // extensions relying on ask() don't crash; they usually only use it
        // as a fallback path.
        return prompt
      },
    },

    environment: createEnvironment(ctx),
    LocalStorage: createLocalStorage(ctx.packageRoot),
    Cache: class {
      private readonly store = new Map<string, string>()

      get(key: string): string | undefined {
        return this.store.get(key)
      }

      set(key: string, value: string): void {
        this.store.set(key, value)
      }

      has(key: string): boolean {
        return this.store.has(key)
      }

      remove(key: string): void {
        this.store.delete(key)
      }

      clear(): void {
        this.store.clear()
      }
    },
    Clipboard: createClipboardShim(),

    getPreferenceValues: (): Record<string, unknown> => readPreferences(ctx.packageRoot),
    getSelectedText: async (): Promise<string> => '',
    getApplications: async (): Promise<Array<Record<string, unknown>>> => [],
    runAppleScript,

    open: async (target: unknown): Promise<void> => {
      if (typeof target !== 'string') return
      if (/^[a-z][a-z0-9+.-]*:\/\//i.test(target) || target.startsWith('mailto:')) {
        await shell.openExternal(target)
      } else {
        const resolved = target.startsWith('~') ? target.replace(/^~/, homedir()) : target
        await shell.openPath(resolved)
      }
    },
    openExtensionPreferences: async (): Promise<void> => {},
    openCommandPreferences: async (): Promise<void> => {},

    showToast: (opts: unknown): Record<string, unknown> => {
      const obj = opts && typeof opts === 'object' ? (opts as Record<string, unknown>) : {}
      ctx.feedback.push({
        kind: 'toast',
        style: typeof obj.style === 'string' ? obj.style : undefined,
        title: typeof obj.title === 'string' ? obj.title : undefined,
        message: typeof obj.message === 'string' ? obj.message : undefined,
      })
      // Real Raycast returns an object with `.hide()` — some extensions
      // chain off of it, so expose a harmless no-op.
      return {
        hide: async (): Promise<void> => {},
        set title(_v: string) {},
        set message(_v: string) {},
        set style(_v: string) {},
      }
    },
    showHUD: async (message: unknown): Promise<void> => {
      ctx.feedback.push({ kind: 'hud', message: String(message ?? '') })
    },
    showInFinder: async (path: unknown): Promise<void> => {
      if (typeof path !== 'string') return
      shell.showItemInFolder(path)
    },
    confirmAlert: async (): Promise<boolean> => true,
    closeMainWindow: async (): Promise<void> => {
      // Hiding the launcher is handled elsewhere; extensions just expect
      // this to exist. The main window will re-open when the user hits
      // the global shortcut again.
    },
    popToRoot: async (): Promise<void> => {
      // Navigation is no-op here because we don't run view commands.
    },
    updateCommandMetadata: async (): Promise<void> => {},
    captureException: (): void => {},

    useNavigation: () => ({ push: () => {}, pop: () => {} }),

    /** Image helper. Electron has its own `nativeImage`; extensions mainly
     *  use this for sizing/base64 conversion. */
    createImage: (buffer: Buffer): unknown => nativeImage.createFromBuffer(buffer),
  }
}

export function createRaycastUtils(ctx: ShimContext): Record<string, unknown> {
  // Basic Raycast-utils subset. React hooks are no-ops; no-view code rarely
  // calls them, but module-level imports from view files still resolve.
  const localStorage = createLocalStorage(ctx.packageRoot) as {
    getItem: (key: string) => Promise<string | undefined>
    setItem: (key: string, value: string) => Promise<void>
  }

  return {
    useCachedState: <T>(_: string, initialValue: T): [T, (next: T | ((prev: T) => T)) => void] => {
      let state = initialValue
      const setState = (next: T | ((prev: T) => T)): void => {
        state = typeof next === 'function' ? (next as (prev: T) => T)(state) : next
      }
      return [state, setState]
    },
    useCachedPromise: () => ({
      data: undefined,
      revalidate: async () => {},
      isLoading: false,
      mutate: async () => {},
      error: undefined,
      pagination: undefined,
    }),
    usePromise: () => ({
      data: undefined,
      isLoading: false,
      revalidate: async () => {},
      mutate: async () => {},
      error: undefined,
    }),
    useFetch: () => ({
      data: undefined,
      isLoading: false,
      revalidate: async () => {},
      error: undefined,
    }),
    useExec: () => ({
      data: undefined,
      isLoading: false,
      error: undefined,
      revalidate: async () => {},
    }),
    useLocalStorage: <T>(key: string, initialValue: T) => {
      let current = initialValue
      void localStorage.getItem(key).then((raw) => {
        if (typeof raw === 'string') {
          try {
            current = JSON.parse(raw) as T
          } catch {
            // ignore corrupt JSON
          }
        }
      })
      return {
        value: current,
        setValue: async (next: T): Promise<void> => {
          current = next
          await localStorage.setItem(key, JSON.stringify(next))
        },
        removeValue: async (): Promise<void> => {
          await localStorage.setItem(key, 'null')
        },
        isLoading: false,
      }
    },
    useForm: () => ({
      itemProps: new Proxy(
        {},
        {
          get: () => ({ value: '', onChange: () => {} }),
        },
      ),
      values: {},
      setValue: () => {},
      setValidationError: () => {},
      reset: () => {},
      focus: () => {},
      handleSubmit: () => async () => true,
    }),
    FormValidation: { Required: () => undefined },
    runAppleScript,
    showFailureToast: (error: unknown): void => {
      ctx.feedback.push({
        kind: 'toast',
        style: 'failure',
        title: error instanceof Error ? error.message : String(error),
      })
    },
    getFavicon: (): unknown => createRenderProxy('Icon'),
  }
}

export function formatRuntimeFeedback(feedback: RuntimeFeedback): string {
  if (feedback.kind === 'hud') {
    return feedback.message ?? 'Extension command completed.'
  }
  const title = feedback.title?.trim() ?? ''
  const message = feedback.message?.trim() ?? ''
  if (title && message) return `${title}: ${message}`
  return title || message || 'Extension command completed.'
}
