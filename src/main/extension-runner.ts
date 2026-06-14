import { app, BrowserWindow, clipboard, nativeImage, shell } from 'electron'
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { open as openFile, rm as removePath, writeFile } from 'node:fs/promises'
import { execFile, spawn as nodeSpawn } from 'node:child_process'
import { createHash } from 'node:crypto'
import { get as httpGet } from 'node:http'
import { get as httpsGet } from 'node:https'
import { homedir } from 'node:os'
import { createRequire, builtinModules } from 'node:module'
import { basename, dirname, extname, join } from 'node:path'
import {
  ReadableStream as NodeReadableStream,
  TransformStream as NodeTransformStream,
  WritableStream as NodeWritableStream,
} from 'node:stream/web'
import { promisify } from 'node:util'
import { deserialize, serialize } from 'node:v8'
import { gunzipSync, gzip } from 'node:zlib'
import vm from 'node:vm'
import { configurePackagedEsbuildBinary } from './esbuild-runtime'
import { setSuppressBlurHide } from './windowState'
import type {
  ExtensionInvokeActionRequest,
  ExtensionInvokeActionResult,
  ExtensionLoadMoreSessionRequest,
  ExtensionRunCommandRequest,
  ExtensionRunCommandResult,
  ExtensionRuntimeAction,
  ExtensionRuntimeNode,
  ExtensionRefreshSessionRequest,
  ExtensionRefreshSessionResult,
  ExtensionSearchTextChangedRequest,
  ExtensionSearchTextChangedResult,
} from '../shared/extensionRuntime'
import {
  getExtensionPreferences,
  getExtensionPreferenceSetup,
  resolveInstalledPackageJsonPath,
  shouldShowExtensionPreferenceSetup,
} from './extension-registry'

type PackageCommand = {
  name?: string
  title?: string
  mode?: string
  path?: string
  entry?: string
  entrypoint?: string
  file?: string
  source?: string
}

type ExtensionPackageJson = {
  name?: string
  title?: string
  commands?: PackageCommand[]
}

type RuntimeFeedback = {
  kind: 'toast' | 'hud'
  style?: string
  title?: string
  message?: string
}

type RuntimeActionHandler = (formValues?: Record<string, string>) => Promise<void> | void

type RuntimeSession = {
  id: string
  extensionId: string
  commandName: string
  title: string
  packageRoot: string
  actionHandlers: Map<string, RuntimeActionHandler>
  currentActions: ExtensionRuntimeAction[]
  feedback: RuntimeFeedback[]
  stack: unknown[]
  preferences: Record<string, unknown>
  searchTextChangeHandler: ((text: string) => void) | null
  commandFn: ((...args: unknown[]) => unknown) | null
  commandArgs: Record<string, string>
  bundledCode: string
  searchText: string
  hookStates: unknown[]
  hookIndex: number
  pendingPromises: Array<Promise<unknown>>
  promiseCache: Map<string, {
    promise?: Promise<unknown>
    data?: unknown
    error?: unknown
    label?: string
    startedAt?: number
  }>
  promiseKeysByHook: Map<number, string>
  cacheRecoveryKeys: Set<string>
  abortControllers: Set<AbortController>
  effectCleanups: Map<number, (() => void)>
  effectDeps: Map<number, unknown[] | undefined>
  hasStateUpdates: boolean
  disposed: boolean
  listItemLimit: number
  hookStateSnapshot: string | null
  pickedColor: {
    red: number
    green: number
    blue: number
    alpha: number
    colorSpace: string
  } | null
}

type JsxNode = {
  __jsx: true
  type: unknown
  props: Record<string, unknown>
  key?: unknown
}

type ReactContextShim<T = unknown> = {
  $$typeof: symbol
  _currentValue: T
  _defaultValue: T
  Provider: (props: { value?: T; children?: unknown }) => unknown
  Consumer: (props: { children?: ((value: T) => unknown) | unknown }) => unknown
}

type RaycastComponentToken = {
  __raycastComponent: true
  name: string
}

const RUNTIME_COMPONENT_LIMIT = 10_000
const RUNTIME_RECURSION_LIMIT = 80
const SESSIONS_SOFT_LIMIT = 30
const INITIAL_RENDER_PASSES = 1
const SEARCH_TEXT_RENDER_PASSES = 1
const LIST_ITEM_PAGE_SIZE = 30
const APPLICATIONS_CACHE_TTL_MS = 30_000
const PROMISE_RESULT_CACHE_TTL_MS = 24 * 60 * 60 * 1000
const PROMISE_RESULT_MEMORY_CACHE_LIMIT = 200
const BUILTIN_SET = new Set<string>(builtinModules)
const JSX_FRAGMENT = Symbol.for('raymes.jsx.fragment')
const REACT_CONTEXT = Symbol.for('react.context')
const execFileAsync = promisify(execFile)
const gzipAsync = promisify(gzip)

const sessions = new Map<string, RuntimeSession>()
const promiseResultMemoryCache = new Map<string, { data: unknown; cachedAt: number }>()

function setPromiseResultMemoryCache(
  key: string,
  value: { data: unknown; cachedAt: number },
): void {
  // Evict oldest entries when the in-memory promise cache grows too large.
  if (
    promiseResultMemoryCache.size >= PROMISE_RESULT_MEMORY_CACHE_LIMIT &&
    !promiseResultMemoryCache.has(key)
  ) {
    const firstKey = promiseResultMemoryCache.keys().next().value
    if (firstKey !== undefined) {
      promiseResultMemoryCache.delete(firstKey)
    }
  }
  // Move to (or insert at) the end to maintain LRU order.
  promiseResultMemoryCache.delete(key)
  promiseResultMemoryCache.set(key, value)
}

let applicationsCache:
  | {
      expiresAt: number
      promise: Promise<Array<{ name: string; path: string; bundleId?: string }>>
    }
  | null = null

type WalkRuntimeOptions = {
  listItemsSeen?: { count: number }
  listItemLimit?: number
  listItemsTruncated?: { value: boolean }
}

const iconProxy = new Proxy(
  {},
  {
    get: (_target, prop) => String(prop),
  },
)

function makeId(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function elapsedMs(startedAt: number): string {
  return `${Date.now() - startedAt}ms`
}

function promiseHookLabel(hookIdx: number, fn: unknown, args: unknown[]): string {
  const source = typeof fn === 'function'
    ? fn.toString().replace(/\s+/g, ' ').slice(0, 90)
    : String(fn).slice(0, 90)
  let serializedArgs = ''
  try {
    serializedArgs = JSON.stringify(args)
  } catch {
    serializedArgs = '[unserializable]'
  }
  return `hook=${hookIdx} fn="${source}" args=${serializedArgs.slice(0, 160)}`
}

function promiseResultCachePath(session: RuntimeSession, key: string): string {
  const digest = createHash('sha256')
    .update(session.bundledCode)
    .update('\0')
    .update(session.extensionId)
    .update('\0')
    .update(session.commandName)
    .update('\0')
    .update(key)
    .digest('hex')
  return join(session.packageRoot, '.raymes-runtime-cache', `${digest}.bin.gz`)
}

function readPromiseResultCache(
  session: RuntimeSession,
  key: string,
): { data: unknown; cachedAt: number } | null {
  const memoryKey = `${session.extensionId}/${session.commandName}:${key}`
  const memoryEntry = promiseResultMemoryCache.get(memoryKey)
  if (memoryEntry && Date.now() - memoryEntry.cachedAt <= PROMISE_RESULT_CACHE_TTL_MS) {
    return memoryEntry
  }

  const cachePath = promiseResultCachePath(session, key)
  try {
    const stats = statSync(cachePath)
    if (Date.now() - stats.mtimeMs > PROMISE_RESULT_CACHE_TTL_MS) return null
    const compressed = readFileSync(cachePath)
    const payload = deserialize(gunzipSync(compressed)) as {
      data: unknown
      cachedAt: number
    }
    setPromiseResultMemoryCache(memoryKey, payload)
    console.log(
      `[usePromise] Persistent cache hit ${session.extensionId}/${session.commandName}; bytes=${compressed.byteLength}`,
    )
    return payload
  } catch {
    return null
  }
}

function writePromiseResultCache(
  session: RuntimeSession,
  key: string,
  data: unknown,
): void {
  if (data === undefined || session.disposed) return

  const cachedAt = Date.now()
  const memoryKey = `${session.extensionId}/${session.commandName}:${key}`
  setPromiseResultMemoryCache(memoryKey, { data, cachedAt })
  const cachePath = promiseResultCachePath(session, key)

  void (async () => {
    const startedAt = Date.now()
    try {
      const encoded = serialize({ data, cachedAt })
      const compressed = await gzipAsync(encoded)
      mkdirSync(dirname(cachePath), { recursive: true })
      await writeFile(cachePath, compressed)
      console.log(
        `[usePromise] Persistent cache write complete after ${elapsedMs(startedAt)}; raw=${encoded.byteLength}, compressed=${compressed.byteLength}`,
      )
    } catch (error) {
      console.warn(
        '[usePromise] Persistent cache write failed:',
        error instanceof Error ? error.message : String(error),
      )
    }
  })()
}

function createLoggedFetch(session: RuntimeSession): typeof fetch {
  return async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const method = String(init?.method ?? (input instanceof Request ? input.method : 'GET')).toUpperCase()
    const url = input instanceof Request ? input.url : String(input)
    const startedAt = Date.now()
    console.log(`[ExtensionFetch] start ${method} ${url}`)

    try {
      const response = await fetch(input, init)
      const contentLength = response.headers.get('content-length') ?? 'unknown'
      console.log(
        `[ExtensionFetch] headers ${method} ${url} after ${elapsedMs(startedAt)}; status=${response.status}, length=${contentLength}`,
      )

      if (!response.body || method === 'HEAD') {
        console.log(`[ExtensionFetch] complete ${method} ${url} after ${elapsedMs(startedAt)}`)
        return response
      }

      let bytes = 0
      let lastLoggedAt = Date.now()
      let lastLoggedBytes = 0
      const monitor = new TransformStream<Uint8Array, Uint8Array>({
        transform(chunk, controller) {
          bytes += chunk.byteLength
          const now = Date.now()
          if (now - lastLoggedAt >= 2_000 || bytes - lastLoggedBytes >= 5 * 1024 * 1024) {
            console.log(
              `[ExtensionFetch] progress ${method} ${url}; bytes=${bytes}, elapsed=${elapsedMs(startedAt)}`,
            )
            lastLoggedAt = now
            lastLoggedBytes = bytes
          }
          controller.enqueue(chunk)
        },
        flush() {
          console.log(
            `[ExtensionFetch] body complete ${method} ${url}; bytes=${bytes}, elapsed=${elapsedMs(startedAt)}`,
          )
        },
      })

      return new Response(response.body.pipeThrough(monitor), {
        status: response.status,
        statusText: response.statusText,
        headers: response.headers,
      })
    } catch (error) {
      console.error(
        `[ExtensionFetch] failed ${method} ${url} after ${elapsedMs(startedAt)}:`,
        error instanceof Error ? error.message : String(error),
      )
      throw error
    }
  }
}

async function recoverIncompleteChunkedCache(
  session: RuntimeSession,
  error: unknown,
  promiseKey: string,
): Promise<boolean> {
  if (!(error instanceof Error) || session.cacheRecoveryKeys.has(promiseKey)) return false

  const missingIndex = error.message.match(
    /ENOENT:.*open ['"]([^'"]+)[/\\]([^/\\]+)[/\\]index\.json['"]/,
  )
  if (!missingIndex) return false

  const indexPath = missingIndex[1]
  const cacheName = missingIndex[2]
  if (!indexPath || !cacheName) return false

  const supportRoot = join(session.packageRoot, '.raymes-support')
  const chunkDirectory = join(indexPath, cacheName)
  const sourcePath = join(indexPath, `${cacheName}.json`)
  if (
    dirname(sourcePath) !== supportRoot ||
    dirname(chunkDirectory) !== supportRoot
  ) {
    return false
  }

  let handle: Awaited<ReturnType<typeof openFile>> | null = null
  try {
    handle = await openFile(sourcePath, 'r')
    const stats = await handle.stat()
    if (stats.size <= 0) return false

    const tailSize = Math.min(stats.size, 4096)
    const tail = Buffer.alloc(tailSize)
    await handle.read(tail, 0, tailSize, stats.size - tailSize)
    const finalCharacter = tail.toString('utf8').trimEnd().at(-1)
    if (finalCharacter === ']' || finalCharacter === '}') return false
  } catch {
    return false
  } finally {
    await handle?.close().catch(() => {})
  }

  session.cacheRecoveryKeys.add(promiseKey)
  await Promise.all([
    removePath(sourcePath, { force: true }),
    removePath(chunkDirectory, { recursive: true, force: true }),
  ])
  console.warn(
    `[Runner] Removed incomplete extension cache "${cacheName}" and scheduled one rebuild.`,
  )
  return true
}

function axiosGetShim(url: string, options?: { responseType?: string }): Promise<{ data: unknown; status: number; headers: Record<string, string> }> {
  if (options?.responseType !== 'stream') {
    return fetch(url).then(async (response) => {
      const text = await response.text()
      let data: unknown = text
      try {
        if (response.headers.get('content-type')?.includes('application/json')) {
          data = JSON.parse(text)
        }
      } catch {
        // Fallback to text
      }
      const headers: Record<string, string> = {}
      response.headers.forEach((v, k) => {
        headers[k] = v
      })
      return { data, status: response.status, headers }
    })
  }

  return new Promise((resolve, reject) => {
    const client = url.startsWith('https:') ? httpsGet : httpGet
    const request = client(url, (response) => {
      const status = response.statusCode ?? 0
      if (status >= 300 && status < 400 && response.headers.location) {
        void axiosGetShim(new URL(response.headers.location, url).toString(), options).then(resolve, reject)
        response.resume()
        return
      }
      if (status < 200 || status >= 300) {
        response.resume()
        reject(new Error(`Request failed with status ${status}`))
        return
      }
      const headers: Record<string, string> = {}
      for (const [k, v] of Object.entries(response.headers)) {
        if (v !== undefined) headers[k] = Array.isArray(v) ? v.join(', ') : v
      }
      resolve({ data: response, status, headers })
    })
    request.on('error', reject)
  })
}

function createFetchModuleShim(): typeof fetch & {
  default: typeof fetch
  fetch: typeof fetch
  Headers: typeof Headers | undefined
  Request: typeof Request | undefined
  Response: typeof Response | undefined
  __esModule: true
} {
  const boundFetch = fetch.bind(globalThis) as typeof fetch & {
    default: typeof fetch
    fetch: typeof fetch
    Headers: typeof Headers | undefined
    Request: typeof Request | undefined
    Response: typeof Response | undefined
    __esModule: true
  }
  boundFetch.default = boundFetch
  boundFetch.fetch = boundFetch
  boundFetch.Headers = globalThis.Headers
  boundFetch.Request = globalThis.Request
  boundFetch.Response = globalThis.Response
  boundFetch.__esModule = true
  return boundFetch
}

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

function nativeColorPickerBinaryPath(): string {
  return join(app.getPath('userData'), 'native', 'color-picker')
}

function nativeColorPickerSourcePath(): string | null {
  const candidates = [
    join(process.cwd(), 'native', 'color-picker', 'main.swift'),
    join(process.cwd(), 'src', 'native', 'color-picker.swift'),
    join(app.getAppPath(), 'native', 'color-picker', 'main.swift'),
    join(app.getAppPath(), 'src', 'native', 'color-picker.swift'),
  ]
  return candidates.find((candidate) => existsSync(candidate)) ?? null
}

async function ensureNativeColorPickerBinary(): Promise<string | null> {
  const binaryPath = nativeColorPickerBinaryPath()
  if (existsSync(binaryPath)) return binaryPath

  const sourcePath = nativeColorPickerSourcePath()
  if (!sourcePath) return null

  const moduleCachePath = join(dirname(binaryPath), 'swift-module-cache')
  mkdirSync(dirname(binaryPath), { recursive: true })
  mkdirSync(moduleCachePath, { recursive: true })
  try {
    await execFileAsync('/usr/bin/swiftc', [
      '-module-cache-path',
      moduleCachePath,
      '-O',
      '-o',
      binaryPath,
      sourcePath,
      '-framework',
      'AppKit',
    ])
    return existsSync(binaryPath) ? binaryPath : null
  } catch (error) {
    console.error('[ColorPicker] Failed to compile native helper:', error)
    return null
  }
}

async function pickColorWithNativeSampler(): Promise<{
  red: number
  green: number
  blue: number
  alpha: number
  colorSpace: string
} | null> {
  const visibleWindows = BrowserWindow.getAllWindows().filter((window) => window.isVisible())
  try {
    const binaryPath = await ensureNativeColorPickerBinary()
    if (!binaryPath) {
      return null
    }

    setSuppressBlurHide(true)
    for (const window of visibleWindows) {
      window.hide()
    }
    app.hide()
    await delay(80)
    const { stdout } = await execFileAsync(binaryPath)
    const trimmed = stdout.trim()
    if (!trimmed || trimmed === 'null') return null

    const parsed = JSON.parse(trimmed) as Record<string, unknown>
    const toUnitRange = (value: unknown): number | null => {
      const numeric = Number(value)
      if (!Number.isFinite(numeric)) return null
      if (numeric > 1) return Math.max(0, Math.min(1, numeric / 255))
      return Math.max(0, Math.min(1, numeric))
    }

    const red = toUnitRange(parsed.red)
    const green = toUnitRange(parsed.green)
    const blue = toUnitRange(parsed.blue)
    const alpha = toUnitRange(parsed.alpha ?? 1)
    if (red === null || green === null || blue === null || alpha === null) return null

    return {
      red,
      green,
      blue,
      alpha,
      colorSpace: typeof parsed.colorSpace === 'string' && parsed.colorSpace.trim()
        ? parsed.colorSpace
        : 'srgb',
    }
  } catch {
    return null
  } finally {
    setSuppressBlurHide(false)
    app.show()
    for (const window of visibleWindows) {
      if (!window.isDestroyed()) {
        window.show()
        window.focus()
      }
    }
  }
}

function colorWheelMarkdown(): string {
  return '![RGB Color Wheel](rgb-color-wheel.webp?&raycast-height=350)'
}

function attachRuntimeRootMetadata(root: ExtensionRuntimeNode, session: RuntimeSession): void {
  root.props = {
    ...(root.props ?? {}),
    assetsPath: join(session.packageRoot, 'assets'),
  }
  if (typeof root.props.markdown === 'string') {
    root.props.markdown = resolveExtensionMarkdownAssets(root.props.markdown, session.packageRoot)
  }
}

function buildPreferenceSetupRoot(
  extensionId: string,
  commandName: string,
): ExtensionRuntimeNode {
  const setup = getExtensionPreferenceSetup(extensionId, commandName)
  return {
    type: 'Raymes.PreferenceSetup',
    props: {
      extensionId: setup.extensionId,
      commandName,
      title: setup.title,
      iconPath: setup.iconPath,
      preferences: setup.preferences,
      values: setup.values,
      includeApiKey: extensionId === 'raycast.google-translate',
    },
    children: [],
  }
}

function parsePackageJson(path: string): ExtensionPackageJson {
  if (!existsSync(path)) {
    throw new Error(`Missing package.json at ${path}`)
  }

  const raw = readFileSync(path, 'utf8')
  const parsed = JSON.parse(raw) as ExtensionPackageJson
  return parsed && typeof parsed === 'object' ? parsed : {}
}

function findCommandInManifest(pkg: ExtensionPackageJson, commandName: string): PackageCommand {
  const command = (pkg.commands ?? []).find((entry) => entry.name === commandName)
  if (!command) {
    throw new Error(`Command not found: ${commandName}`)
  }
  return command
}

function resolveCommandEntry(
  packageRoot: string,
  commandName: string,
  command: PackageCommand,
): string {
  const prebuilt = join(packageRoot, '.sc-build', `${commandName}.js`)
  if (existsSync(prebuilt)) return prebuilt

  const explicit = [command.path, command.entrypoint, command.entry, command.file, command.source]
    .filter((entry): entry is string => typeof entry === 'string' && entry.trim().length > 0)
    .map((entry) => join(packageRoot, entry))

  const src = join(packageRoot, 'src')
  const defaults = [
    join(src, `${commandName}.tsx`),
    join(src, `${commandName}.ts`),
    join(src, `${commandName}.jsx`),
    join(src, `${commandName}.js`),
    join(src, commandName, 'index.tsx'),
    join(src, commandName, 'index.ts'),
    join(src, commandName, 'index.jsx'),
    join(src, commandName, 'index.js'),
    join(src, 'commands', `${commandName}.tsx`),
    join(src, 'commands', `${commandName}.ts`),
    join(src, 'commands', `${commandName}.jsx`),
    join(src, 'commands', `${commandName}.js`),
  ]

  const candidate = [...explicit, ...defaults].find((entry) => existsSync(entry))
  if (!candidate) {
    throw new Error(`Could not resolve entry file for command ${commandName}`)
  }
  return candidate
}

async function bundleCommand(entryPath: string, packageRoot: string): Promise<string> {
  if (entryPath.includes(`${join('.sc-build', '')}`) || entryPath.includes('/.sc-build/')) {
    const prebuilt = readFileSync(entryPath, 'utf8')
    if (!prebuilt.trim()) throw new Error(`Prebuilt extension bundle is empty: ${entryPath}`)
    return prebuilt
  }

  // Lazy-load esbuild so the main process doesn't pay the cost unless an
  // extension is installed without a pre-built bundle. Ideally all extensions
  // are pre-built at install time and this fallback is never hit.
  configurePackagedEsbuildBinary()
  const esbuild = await import('esbuild')

  const result = await esbuild.build({
    entryPoints: [entryPath],
    absWorkingDir: packageRoot,
    bundle: true,
    format: 'cjs',
    platform: 'node',
    write: false,
    target: 'node20',
    external: [
      '@raycast/api',
      '@raycast/utils',
      'react',
      'react/jsx-runtime',
      'react/jsx-dev-runtime',
    ],
    nodePaths: [join(packageRoot, 'node_modules')],
    logLevel: 'silent',
  })

  const output = result.outputFiles?.[0]?.text
  if (!output) {
    throw new Error('esbuild did not produce output')
  }
  return output
}

function createJsxRuntimeShim(): Record<string, unknown> {
  const jsx = (type: unknown, props?: Record<string, unknown>, key?: unknown): JsxNode => ({
    __jsx: true,
    type,
    props: props ?? {},
    key,
  })

  return {
    Fragment: JSX_FRAGMENT,
    jsx,
    jsxs: jsx,
    jsxDEV: jsx,
  }
}

function createReactShim(session: RuntimeSession): Record<string, unknown> {
  const jsxRuntime = createJsxRuntimeShim()
  const jsx = jsxRuntime.jsx as (type: unknown, props?: Record<string, unknown>, key?: unknown) => JsxNode

  const areHookDepsEqual = (prev: unknown[] | undefined, next: unknown[] | undefined): boolean => {
    if (!prev || !next) return false
    if (prev.length !== next.length) return false
    return next.every((value, index) => Object.is(value, prev[index]))
  }

  const runEffect = (
    idx: number,
    sideEffect: () => void | (() => void),
    deps?: unknown[],
    label = 'useEffect',
  ): void => {
    const prevDeps = session.effectDeps.get(idx)
    if (deps && areHookDepsEqual(prevDeps, deps)) return

    const previousCleanup = session.effectCleanups.get(idx)
    if (previousCleanup) {
      try {
        previousCleanup()
      } catch (e) {
        console.error(`[${label}] cleanup threw:`, e)
      }
      session.effectCleanups.delete(idx)
    }

    try {
      const cleanup = sideEffect()
      session.effectDeps.set(idx, deps)
      if (typeof cleanup === 'function') {
        session.effectCleanups.set(idx, cleanup)
      }
    } catch (e) {
      console.error(`[${label}] side effect threw:`, e)
    }
  }

  class Component<P = Record<string, unknown>, S = Record<string, unknown>> {
    props: P
    state: S

    constructor(props: P) {
      this.props = props
      this.state = {} as S
    }

    setState(
      next:
        | Partial<S>
        | S
        | ((previous: S, props: P) => Partial<S> | S | null),
    ): void {
      const resolved =
        typeof next === 'function'
          ? (next as (previous: S, props: P) => S | Partial<S> | null)(this.state, this.props)
          : next
      if (resolved == null) return
      this.state = {
        ...(this.state && typeof this.state === 'object' ? this.state : {}),
        ...(resolved && typeof resolved === 'object' ? resolved : {}),
      } as S
      session.hasStateUpdates = true
    }

    forceUpdate(): void {
      session.hasStateUpdates = true
    }
  }

  const react = {
    Component,
    PureComponent: Component,
    Fragment: JSX_FRAGMENT,
    createElement: (type: unknown, props: Record<string, unknown> | null, ...children: unknown[]) => {
      const nextProps = { ...(props ?? {}) }
      if (children.length === 1) {
        nextProps.children = children[0]
      } else if (children.length > 1) {
        nextProps.children = children
      }
      return jsx(type, nextProps)
    },
    createContext: <T,>(defaultValue: T): ReactContextShim<T> => {
      const context = {
        $$typeof: REACT_CONTEXT,
        _currentValue: defaultValue,
        _defaultValue: defaultValue,
      } as ReactContextShim<T>
      context.Provider = (props: { value?: T; children?: unknown }): unknown => {
        context._currentValue = props.value as T
        return props.children
      }
      context.Consumer = (props: { children?: ((value: T) => unknown) | unknown }): unknown => {
        if (typeof props.children === 'function') {
          return (props.children as (value: T) => unknown)(context._currentValue)
        }
        return props.children ?? null
      }
      return context
    },
    useState: <T,>(initial: T | (() => T)): [T, (next: T | ((prev: T) => T)) => void] => {
      const idx = session.hookIndex++
      if (session.hookStates.length > idx) {
        return session.hookStates[idx] as [T, (next: T | ((prev: T) => T)) => void]
      }
      let value = typeof initial === 'function' ? (initial as () => T)() : initial
      const setState = (next: T | ((prev: T) => T)): void => {
        value = typeof next === 'function' ? (next as (prev: T) => T)(value) : next
        session.hookStates[idx] = [value, setState]
        session.hasStateUpdates = true
      }
      const tuple: [T, (next: T | ((prev: T) => T)) => void] = [value, setState]
      session.hookStates[idx] = tuple
      return tuple
    },
    useEffect: (sideEffect: () => void | (() => void), deps?: unknown[]): void => {
      const idx = session.hookIndex++
      runEffect(idx, sideEffect, deps, 'useEffect')
    },
    useLayoutEffect: (sideEffect: () => void | (() => void), deps?: unknown[]): void => {
      const idx = session.hookIndex++
      runEffect(idx, sideEffect, deps, 'useLayoutEffect')
    },
    useMemo: <T,>(factory: () => T, deps?: unknown[]): T => {
      const idx = session.hookIndex++
      const existing = session.hookStates[idx] as
        | { kind: 'memo'; value: T; deps?: unknown[] }
        | undefined
      if (existing?.kind === 'memo' && deps && areHookDepsEqual(existing.deps, deps)) {
        return existing.value
      }
      const value = factory()
      session.hookStates[idx] = { kind: 'memo', value, deps }
      return value
    },
    useCallback: <T extends (...args: never[]) => unknown>(callback: T, deps?: unknown[]): T => {
      const idx = session.hookIndex++
      const existing = session.hookStates[idx] as
        | { kind: 'callback'; value: T; deps?: unknown[] }
        | undefined
      if (existing?.kind === 'callback' && deps && areHookDepsEqual(existing.deps, deps)) {
        return existing.value
      }
      session.hookStates[idx] = { kind: 'callback', value: callback, deps }
      return callback
    },
    useRef: <T,>(value: T): { current: T } => {
      const idx = session.hookIndex++
      const existing = session.hookStates[idx] as { current: T } | undefined
      if (existing && typeof existing === 'object' && 'current' in existing) {
        return existing
      }
      const ref = { current: value }
      session.hookStates[idx] = ref
      return ref
    },
    useContext: <T,>(context?: ReactContextShim<T>): T | null => {
      session.hookIndex++
      return context && context.$$typeof === REACT_CONTEXT
        ? context._currentValue
        : null
    },
    useReducer: <S, A>(
      reducer: (state: S, action: A) => S,
      initialArg: S,
    ): [S, (action: A) => void] => {
      const idx = session.hookIndex++
      if (session.hookStates.length > idx) {
        return session.hookStates[idx] as [S, (action: A) => void]
      }
      let current = initialArg
      const dispatch = (action: A): void => {
        current = reducer(current, action)
        session.hookStates[idx] = [current, dispatch]
        session.hasStateUpdates = true
      }
      const tuple: [S, (action: A) => void] = [current, dispatch]
      session.hookStates[idx] = tuple
      return tuple
    },
    memo: <T,>(component: T): T => component,
    forwardRef: <T,>(renderer: T): T => renderer,
    isValidElement: (value: unknown): boolean => isJsxNode(value),
  }

  return {
    ...react,
    default: react,
    __esModule: true,
  }
}

function makeToken(name: string): RaycastComponentToken {
  return { __raycastComponent: true, name }
}

function isToken(value: unknown): value is RaycastComponentToken {
  return Boolean(
    value &&
      typeof value === 'object' &&
      (value as { __raycastComponent?: unknown }).__raycastComponent === true &&
      typeof (value as { name?: unknown }).name === 'string',
  )
}

function normalizeActionTitle(typeName: string, props: Record<string, unknown>): string {
  if (typeof props.title === 'string' && props.title.trim().length > 0) {
    return props.title.trim()
  }

  switch (typeName) {
    case 'Action.CopyToClipboard':
      return 'Copy to Clipboard'
    case 'Action.Paste':
      return 'Paste'
    case 'Action.OpenInBrowser':
      return 'Open in Browser'
    case 'Action.Push':
      return 'Open'
    case 'Action.Pop':
      return 'Back'
    case 'Action.ShowInFinder':
      return 'Show in Finder'
    case 'Action.SubmitForm':
      return 'Submit'
    default:
      return 'Action'
  }
}

function stableActionId(index: number, typeName: string, title: string): string {
  const hash = createHash('sha1')
    .update(`${index}:${typeName}:${title}`)
    .digest('hex')
    .slice(0, 12)
  return `ext-action-${index}-${hash}`
}

function parseShortcut(shortcut: unknown): ExtensionRuntimeAction['shortcut'] | undefined {
  if (!shortcut || typeof shortcut !== 'object') return undefined
  const s = shortcut as { modifiers?: unknown; key?: unknown }
  const modifiers = Array.isArray(s.modifiers)
    ? s.modifiers.filter((m): m is string => typeof m === 'string')
    : undefined
  const key = typeof s.key === 'string' ? s.key : undefined
  if (!modifiers && !key) return undefined
  return { modifiers, key }
}

function pushFeedback(session: RuntimeSession, feedback: RuntimeFeedback): void {
  session.feedback.push(feedback)
  if (session.feedback.length > 20) {
    session.feedback.splice(0, session.feedback.length - 20)
  }
}

function createLocalStorageShim(packageRoot: string): Record<string, unknown> {
  const storagePath = join(packageRoot, '.raymes-local-storage.json')

  const readAll = (): Record<string, string> => {
    if (!existsSync(storagePath)) return {}
    try {
      const parsed = JSON.parse(readFileSync(storagePath, 'utf8')) as Record<string, string>
      return parsed && typeof parsed === 'object' ? parsed : {}
    } catch {
      return {}
    }
  }

  const writeAll = (value: Record<string, string>): void => {
    mkdirSync(dirname(storagePath), { recursive: true })
    writeFileSync(storagePath, JSON.stringify(value, null, 2), 'utf8')
  }

  return {
    getItem: async (key: string): Promise<string | undefined> => readAll()[String(key)],
    setItem: async (key: string, value: string): Promise<void> => {
      const all = readAll()
      all[String(key)] = String(value)
      writeAll(all)
    },
    removeItem: async (key: string): Promise<void> => {
      const all = readAll()
      delete all[String(key)]
      writeAll(all)
    },
    clear: async (): Promise<void> => writeAll({}),
    allItems: async (): Promise<Record<string, string>> => readAll(),
  }
}

function createCacheShim(packageRoot: string): new (
  options?: { namespace?: string },
) => {
  get: (key: string) => string | undefined
  set: (key: string, value: string) => void
  has: (key: string) => boolean
  remove: (key: string) => boolean
  clear: (_options?: { notifySubscribers?: boolean }) => void
  subscribe: (subscriber: (key: string | undefined, value: string | undefined) => void) => () => void
  readonly isEmpty: boolean
} {
  return class CacheShim {
    private readonly subscribers = new Set<
      (key: string | undefined, value: string | undefined) => void
    >()

    private readonly storagePath: string

    constructor(options?: { namespace?: string }) {
      const rawNamespace =
        typeof options?.namespace === 'string' && options.namespace.trim().length > 0
          ? options.namespace.trim()
          : 'shared'
      const safeNamespace = rawNamespace.replace(/[^a-z0-9._-]+/gi, '_')
      this.storagePath = join(packageRoot, '.raymes-support', 'cache', `${safeNamespace}.json`)
    }

    private readAll(): Record<string, string> {
      if (!existsSync(this.storagePath)) return {}
      try {
        const parsed = JSON.parse(readFileSync(this.storagePath, 'utf8')) as Record<string, string>
        return parsed && typeof parsed === 'object' ? parsed : {}
      } catch {
        return {}
      }
    }

    private writeAll(value: Record<string, string>): void {
      mkdirSync(dirname(this.storagePath), { recursive: true })
      writeFileSync(this.storagePath, JSON.stringify(value, null, 2), 'utf8')
    }

    private notify(key: string | undefined, value: string | undefined): void {
      for (const subscriber of this.subscribers) {
        try {
          subscriber(key, value)
        } catch {
          // Ignore extension subscriber failures so cache writes remain safe.
        }
      }
    }

    get isEmpty(): boolean {
      return Object.keys(this.readAll()).length === 0
    }

    get(key: string): string | undefined {
      return this.readAll()[String(key)]
    }

    set(key: string, value: string): void {
      const all = this.readAll()
      all[String(key)] = String(value)
      this.writeAll(all)
      this.notify(String(key), String(value))
    }

    has(key: string): boolean {
      return Object.prototype.hasOwnProperty.call(this.readAll(), String(key))
    }

    remove(key: string): boolean {
      const all = this.readAll()
      const normalizedKey = String(key)
      const existed = Object.prototype.hasOwnProperty.call(all, normalizedKey)
      if (!existed) return false
      delete all[normalizedKey]
      this.writeAll(all)
      this.notify(normalizedKey, undefined)
      return true
    }

    clear(options?: { notifySubscribers?: boolean }): void {
      this.writeAll({})
      if (options?.notifySubscribers !== false) {
        this.notify(undefined, undefined)
      }
    }

    subscribe(
      subscriber: (key: string | undefined, value: string | undefined) => void,
    ): () => void {
      this.subscribers.add(subscriber)
      return () => {
        this.subscribers.delete(subscriber)
      }
    }
  }
}

function copyToSystemClipboard(value: unknown): void {
  if (value && typeof value === 'object') {
    const payload = value as { text?: unknown; file?: unknown; html?: unknown }
    if (typeof payload.file === 'string' && payload.file.trim()) {
      const filePath = payload.file.trim()
      const image = nativeImage.createFromPath(filePath)
      if (!image.isEmpty()) {
        clipboard.writeImage(image)
        return
      }
      clipboard.writeText(filePath)
      return
    }
    if (typeof payload.html === 'string' && typeof payload.text === 'string') {
      clipboard.write({ html: payload.html, text: payload.text })
      return
    }
    if (typeof payload.html === 'string') {
      clipboard.write({ html: payload.html, text: payload.html })
      return
    }
    if (typeof payload.text === 'string') {
      clipboard.writeText(payload.text)
      return
    }
  }

  clipboard.writeText(String(value ?? ''))
}

function createRaycastApiShim(session: RuntimeSession): Record<string, unknown> {
  const ListItemDetailMetadata = Object.assign(makeToken('List.Item.Detail.Metadata'), {
    Label: makeToken('List.Item.Detail.Metadata.Label'),
    TagList: Object.assign(makeToken('List.Item.Detail.Metadata.TagList'), {
      Item: makeToken('List.Item.Detail.Metadata.TagList.Item'),
    }),
    Separator: makeToken('List.Item.Detail.Metadata.Separator'),
    Link: makeToken('List.Item.Detail.Metadata.Link'),
  })
  const ListItemDetail = Object.assign(makeToken('List.Item.Detail'), {
    Metadata: ListItemDetailMetadata,
  })

  const List = Object.assign(makeToken('List'), {
    Item: Object.assign(makeToken('List.Item'), {
      Detail: ListItemDetail,
    }),
    Section: makeToken('List.Section'),
    EmptyView: makeToken('List.EmptyView'),
    Dropdown: Object.assign(makeToken('List.Dropdown'), {
      Section: makeToken('List.Dropdown.Section'),
      Item: makeToken('List.Dropdown.Item'),
    }),
  })

  const Form = Object.assign(makeToken('Form'), {
    TextField: makeToken('Form.TextField'),
    TextArea: makeToken('Form.TextArea'),
    Checkbox: makeToken('Form.Checkbox'),
    Dropdown: makeToken('Form.Dropdown'),
    DatePicker: makeToken('Form.DatePicker'),
    PasswordField: makeToken('Form.PasswordField'),
    Separator: makeToken('Form.Separator'),
    Description: makeToken('Form.Description'),
  })

  const Grid = Object.assign(makeToken('Grid'), {
    Item: makeToken('Grid.Item'),
    Section: makeToken('Grid.Section'),
    EmptyView: makeToken('Grid.EmptyView'),
    Dropdown: List.Dropdown,
    Inset: {
      Small: 'small',
      Medium: 'medium',
      Large: 'large',
    },
  })

  const Detail = Object.assign(makeToken('Detail'), {
    Metadata: Object.assign(makeToken('Detail.Metadata'), {
      Label: makeToken('Detail.Metadata.Label'),
      TagList: Object.assign(makeToken('Detail.Metadata.TagList'), {
        Item: makeToken('Detail.Metadata.TagList.Item'),
      }),
      Separator: makeToken('Detail.Metadata.Separator'),
      Link: makeToken('Detail.Metadata.Link'),
    }),
  })

  const MenuBarExtra = Object.assign(makeToken('MenuBarExtra'), {
    Item: makeToken('MenuBarExtra.Item'),
    Section: makeToken('MenuBarExtra.Section'),
    Separator: makeToken('MenuBarExtra.Separator'),
    Submenu: makeToken('MenuBarExtra.Submenu'),
  })

  const Action = Object.assign(makeToken('Action'), {
    CopyToClipboard: makeToken('Action.CopyToClipboard'),
    Paste: makeToken('Action.Paste'),
    OpenInBrowser: makeToken('Action.OpenInBrowser'),
    Push: makeToken('Action.Push'),
    Pop: makeToken('Action.Pop'),
    ShowInFinder: makeToken('Action.ShowInFinder'),
    SubmitForm: makeToken('Action.SubmitForm'),
    Style: {
      Regular: 'regular',
      Destructive: 'destructive',
    },
  })

  const ActionPanel = Object.assign(makeToken('ActionPanel'), {
    Section: makeToken('ActionPanel.Section'),
  })

  class ToastShim {
    static Style = {
      Success: 'success',
      Failure: 'failure',
      Animated: 'animated',
    }

    style?: string
    title: string
    message?: string

    constructor(options: { style?: string; title: string; message?: string }) {
      this.style = options.style
      this.title = options.title
      this.message = options.message
    }

    async show(): Promise<void> {
      pushFeedback(session, {
        kind: 'toast',
        style: this.style,
        title: this.title,
        message: this.message,
      })
    }

    async hide(): Promise<void> {
      // Toasts are represented by the latest runtime feedback in Raymes.
    }
  }

  return {
    List,
    Form,
    Grid,
    Detail,
    MenuBarExtra,
    Action,
    ActionPanel,
    Icon: iconProxy,
    Color: iconProxy,
    Keyboard: {
      Shortcut: {
        Common: {
          Copy: { modifiers: ['cmd'], key: 'c' },
          CopyPath: { modifiers: ['cmd', 'shift'], key: 'c' },
          Refresh: { modifiers: ['cmd'], key: 'r' },
        },
      },
      Key: iconProxy,
    },
    Toast: ToastShim,
    LaunchType: {
      UserInitiated: 'userInitiated',
      Background: 'background',
    },
    environment: {
      raycastVersion: '1.80.0',
      extensionName: session.extensionId,
      commandName: session.commandName,
      isDevelopment: false,
      commandMode: 'view',
      assetsPath: join(session.packageRoot, 'assets'),
      supportPath: join(session.packageRoot, '.raymes-support'),
      get searchText(): string {
        return session.searchText
      },
    },
    LocalStorage: createLocalStorageShim(session.packageRoot),
    Cache: createCacheShim(session.packageRoot),
    runAppleScript,
    Clipboard: {
      copy: async (value: unknown): Promise<void> => {
        copyToSystemClipboard(value)
      },
      paste: async (value: unknown): Promise<void> => {
        copyToSystemClipboard(value)
      },
      read: async (): Promise<{ text?: string }> => {
        const text = clipboard.readText()
        return text ? { text } : {}
      },
      readText: async (): Promise<string> => clipboard.readText(),
    },
    getPreferenceValues: (): Record<string, unknown> => session.preferences,
    launchCommand: async (): Promise<void> => {
      // Background/menu-bar command relaunches are best-effort in Raymes.
    },
    useNavigation: () => ({
      push: (next: unknown): void => {
        session.stack.push(next)
      },
      pop: (): void => {
        if (session.stack.length > 1) {
          session.stack.pop()
        }
      },
    }),
    showToast: async (
      optionsOrStyle: unknown,
      title?: string,
      message?: string,
    ): Promise<ToastShim> => {
      let toast: ToastShim
      if (typeof optionsOrStyle === 'string') {
        toast = new ToastShim({
          style: optionsOrStyle,
          title: title ? String(title) : '',
          message: message ? String(message) : undefined,
        })
      } else {
        const opts = (optionsOrStyle && typeof optionsOrStyle === 'object'
          ? optionsOrStyle
          : {}) as {
          style?: unknown
          title?: unknown
          message?: unknown
        }
        toast = new ToastShim({
          style: typeof opts.style === 'string' ? opts.style : undefined,
          title: typeof opts.title === 'string' ? opts.title : '',
          message: typeof opts.message === 'string' ? opts.message : undefined,
        })
      }
      await toast.show()
      return toast
    },
    showHUD: async (title: unknown): Promise<void> => {
      pushFeedback(session, { kind: 'hud', message: String(title || '') })
    },
    open: async (target: unknown): Promise<void> => {
      if (typeof target !== 'string') return
      if (/^[a-z][a-z0-9+.-]*:\/\//i.test(target) || target.startsWith('mailto:')) {
        await shell.openExternal(target)
      }
    },
    showInFinder: async (path: unknown): Promise<void> => {
      if (typeof path !== 'string') return
      shell.showItemInFolder(path)
    },
    getApplications: async (): Promise<Array<{ name: string; path: string; bundleId?: string }>> => {
      const now = Date.now()
      if (applicationsCache && applicationsCache.expiresAt > now) {
        return applicationsCache.promise
      }

      console.log('[getApplications] Starting Spotlight query for installed apps...')
      const promise = (async (): Promise<Array<{ name: string; path: string; bundleId?: string }>> => {
        try {
          const { stdout } = await execFileAsync(
            '/usr/bin/mdfind',
            ['kMDItemKind == \'Application\''],
            {
              maxBuffer: 10 * 1024 * 1024,
              timeout: 3000,
            },
          )
          const apps = stdout.trim().split('\n')
            .filter((p) => p.endsWith('.app'))
            .map((appPath) => ({ name: basename(appPath, '.app'), path: appPath }))
            .sort((a, b) => a.name.localeCompare(b.name))
          console.log(`[getApplications] mdfind returned ${apps.length} applications`)
          return apps
        } catch (err) {
          console.warn('[getApplications] mdfind failed, falling back to directory scan:', err)
          const apps: Array<{ name: string; path: string }> = []
          const dirs = ['/Applications', '/System/Applications', join(homedir(), 'Applications')]
          for (const dir of dirs) {
            try {
              for (const entry of readdirSync(dir)) {
                if (entry.endsWith('.app')) {
                  apps.push({ name: basename(entry, '.app'), path: join(dir, entry) })
                }
              }
            } catch (dirErr) {
              console.warn(`[getApplications] Could not scan directory ${dir}:`, dirErr)
            }
          }
          console.log(`[getApplications] Directory scan found ${apps.length} applications`)
          return apps.sort((a, b) => a.name.localeCompare(b.name))
        }
      })()

      applicationsCache = {
        expiresAt: now + APPLICATIONS_CACHE_TTL_MS,
        promise,
      }
      return promise
    },
    getFrontmostApplication: async (): Promise<{ name: string; path: string; bundleId?: string } | null> => {
      try {
        const script = 'tell application "System Events" to get name of first application process whose frontmost is true'
        const { stdout } = await execFileAsync('/usr/bin/osascript', ['-e', script], { timeout: 3000 })
        const name = stdout.trim()
        if (name) return { name, path: `/Applications/${name}.app` }
        return null
      } catch {
        return null
      }
    },
    getDefaultApplication: async (): Promise<{ name: string; path: string } | null> => {
      return null
    },
    confirmAlert: async (): Promise<boolean> => true,
    openExtensionPreferences: async (): Promise<void> => {
      // Preferences editing is handled by Raymes settings.
    },
    openCommandPreferences: async (): Promise<void> => {
      // Preferences editing is handled by Raymes settings.
    },
    updateCommandMetadata: async (): Promise<void> => {
      // Raymes does not currently surface dynamic command subtitles, but
      // extensions call this after dependency checks and expect it to exist.
    },
    closeMainWindow: async (): Promise<void> => {},
    popToRoot: async (): Promise<void> => {},
    clearSearchBar: async (): Promise<void> => {},
  }
}

function createRaycastUtilsShim(session: RuntimeSession): Record<string, unknown> {
  const CacheShim = createCacheShim(session.packageRoot)
  const cache = new CacheShim()

  const useCachedState = <T,>(
    key: string,
    initialValue: T | (() => T),
  ): [T, (next: T | ((prev: T) => T)) => void] => {
    const hookIdx = session.hookIndex++
    const existing = session.hookStates[hookIdx] as [T, (next: T | ((prev: T) => T)) => void] | undefined
    if (existing) return existing

    const getInitialValue = (): T => {
      const raw = cache.get(String(key))
      if (typeof raw === 'string') {
        try {
          return JSON.parse(raw) as T
        } catch {
          return raw as T
        }
      }
      return typeof initialValue === 'function' ? (initialValue as () => T)() : initialValue
    }

    let current = getInitialValue()
    const setValue = (next: T | ((prev: T) => T)): void => {
      current = typeof next === 'function' ? (next as (prev: T) => T)(current) : next
      cache.set(String(key), JSON.stringify(current))
      session.hookStates[hookIdx] = [current, setValue]
      session.hasStateUpdates = true
    }
    const tuple: [T, (next: T | ((prev: T) => T)) => void] = [current, setValue]
    session.hookStates[hookIdx] = tuple
    return tuple
  }

  const cacheKey = (fn: unknown, args: unknown[]): string => {
    const fnSig = typeof fn === 'function' ? fn.toString().slice(0, 120) : String(fn)
    let argsKey: string
    try {
      argsKey = JSON.stringify(args)
    } catch {
      argsKey = String(args)
    }
    return `${fnSig}:${argsKey}`
  }

  const makePromiseHook = (persistent = false): ((...args: unknown[]) => unknown) => {
    return (fn: unknown, args?: unknown, options?: unknown) => {
      const hookIdx = session.hookIndex++
      const stableArgs = Array.isArray(args) ? args : []
      const opts = (options && typeof options === 'object' ? options : {}) as {
        initialData?: unknown
        execute?: boolean
        keepPreviousData?: boolean
        abortable?: { current?: AbortController | null }
        onError?: (error: unknown) => unknown
      }
      const key = `${hookIdx}:${cacheKey(fn, stableArgs)}`
      const label = promiseHookLabel(hookIdx, fn, stableArgs)
      const shouldExecute = opts?.execute !== false
      const previousKey = session.promiseKeysByHook.get(hookIdx)
      const previousEntry =
        previousKey && previousKey !== key ? session.promiseCache.get(previousKey) : undefined
      const previousData = opts.keepPreviousData ? previousEntry?.data : undefined

      if (previousKey !== key) {
        opts.abortable?.current?.abort()
        if (previousKey) session.promiseCache.delete(previousKey)
        session.promiseKeysByHook.set(hookIdx, key)
      }

      const schedule = (retainedData: unknown): Promise<unknown> | null => {
        if (!shouldExecute || typeof fn !== 'function' || session.disposed) return null

        const startedAt = Date.now()
        console.log(`[usePromise] Scheduled ${label}`)
        opts.abortable?.current?.abort()
        const controller = new AbortController()
        if (opts.abortable) opts.abortable.current = controller
        session.abortControllers.add(controller)

        let tracked: Promise<unknown>
        tracked = delay(0)
          .then(async () => {
            if (session.disposed || controller.signal.aborted) {
              const error = new Error('Aborted')
              error.name = 'AbortError'
              throw error
            }
            console.log(`[usePromise] Starting ${label}`)
            return await Promise.resolve(
              (fn as (...values: unknown[]) => unknown)(...stableArgs),
            )
          })
          .then(async (data: unknown) => {
            if (session.promiseCache.get(key)?.promise !== tracked) return data

            console.log(
              `[usePromise] Resolved ${label} after ${elapsedMs(startedAt)}; data=${
                Array.isArray(data) ? `array(${data.length})` : typeof data
              }`,
            )
            session.promiseCache.set(key, { data, error: undefined, label })
            if (persistent) writePromiseResultCache(session, key, data)
            if (!session.disposed) session.hasStateUpdates = true
            return data
          })
          .catch(async (error: unknown) => {
            if (session.promiseCache.get(key)?.promise !== tracked) return undefined

            const isAbort =
              controller.signal.aborted ||
              (error instanceof Error && error.name === 'AbortError')
            const recovered =
              !isAbort && await recoverIncompleteChunkedCache(session, error, key)
            if (recovered) {
              console.warn(
                `[usePromise] Recovered ${label} after ${elapsedMs(startedAt)}; retrying on refresh.`,
              )
              session.promiseCache.delete(key)
              if (!session.disposed) session.hasStateUpdates = true
              return undefined
            }

            session.promiseCache.set(key, {
              data: retainedData,
              error: isAbort ? undefined : error,
            })

            if (isAbort) return undefined

            console.error(
              `[usePromise] Rejected ${label} after ${elapsedMs(startedAt)}:`,
              error instanceof Error ? error.message : String(error),
            )
            if (!session.disposed) session.hasStateUpdates = true
            if (typeof opts.onError === 'function') {
              try {
                await Promise.resolve(opts.onError(error))
              } catch (onErrorErr) {
                console.error('[usePromise] onError callback threw:', onErrorErr)
              }
            }
            return undefined
          })
          .finally(() => {
            console.log(`[usePromise] Finished ${label} after ${elapsedMs(startedAt)}`)
            session.abortControllers.delete(controller)
            if (opts.abortable?.current === controller) {
              opts.abortable.current = null
            }
          })

        session.promiseCache.set(key, {
          promise: tracked,
          data: retainedData,
          error: undefined,
          label,
          startedAt,
        })
        return tracked
      }

      let cached = session.promiseCache.get(key)
      if (!cached && shouldExecute) {
        const persistentEntry = persistent ? readPromiseResultCache(session, key) : null
        const retainedData = previousData ?? persistentEntry?.data ?? opts.initialData
        schedule(retainedData)
        cached = session.promiseCache.get(key)
      }

      // If a result or in-flight promise already exists for this hook + args,
      // reuse it instead of spawning duplicate work on every render.
      if (cached) {
        if (cached.promise) {
          session.pendingPromises.push(cached.promise)
          return {
            data: cached.data,
            isLoading: true,
            error: undefined,
            revalidate: async (): Promise<void> => {
              await schedule(cached?.data)
            },
            mutate: async (next?: unknown): Promise<unknown> => {
              const value =
                typeof next === 'function'
                  ? (next as (current: unknown) => unknown)(cached?.data)
                  : next
              session.promiseCache.set(key, { data: value, error: undefined })
              if (!session.disposed) session.hasStateUpdates = true
              return value
            },
            pagination: undefined,
          }
        }
        return {
          data: cached.data,
          isLoading: false,
          error: cached.error,
          revalidate: async (): Promise<void> => {
            await schedule(cached?.data)
          },
          mutate: async (next?: unknown): Promise<unknown> => {
            const value =
              typeof next === 'function'
                ? (next as (current: unknown) => unknown)(cached?.data)
                : next
            session.promiseCache.set(key, { data: value, error: undefined })
            if (!session.disposed) session.hasStateUpdates = true
            return value
          },
          pagination: undefined,
        }
      }

      return {
        data: previousData ?? opts.initialData,
        isLoading: false,
        error: undefined,
        revalidate: async (): Promise<void> => {
          await schedule(previousData ?? opts.initialData)
        },
        mutate: async (next?: unknown): Promise<unknown> => next,
        pagination: undefined,
      }
    }
  }

  return {
    useCachedState,
    usePromise: makePromiseHook(),
    useFetch: makePromiseHook(),
    useCachedPromise: makePromiseHook(true),
    getProgressIcon: (
      progress: number,
      color = '#ff6363',
      options?: { background?: string; backgroundOpacity?: number },
    ): string => {
      const value = Math.max(0, Math.min(1, Number(progress) || 0))
      const radius = 10
      const circumference = 2 * Math.PI * radius
      const offset = circumference * (1 - value)
      const background = options?.background || '#ffffff'
      const opacity = options?.backgroundOpacity ?? 0.16
      const svg = [
        '<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32">',
        `<circle cx="16" cy="16" r="${radius}" fill="none" stroke="${background}" stroke-width="4" opacity="${opacity}"/>`,
        `<circle cx="16" cy="16" r="${radius}" fill="none" stroke="${color}" stroke-width="4" stroke-linecap="round" stroke-dasharray="${circumference}" stroke-dashoffset="${offset}" transform="rotate(-90 16 16)"/>`,
        '</svg>',
      ].join('')
      return `data:image/svg+xml;base64,${Buffer.from(svg).toString('base64')}`
    },
    runAppleScript,
    showFailureToast: (error: unknown): void => {
      pushFeedback(session, {
        kind: 'toast',
        style: 'failure',
        title: error instanceof Error ? error.message : String(error),
      })
    },
  }
}

function isJsxNode(value: unknown): value is JsxNode {
  return Boolean(
    value &&
      typeof value === 'object' &&
      (value as { __jsx?: unknown }).__jsx === true &&
      'type' in (value as Record<string, unknown>) &&
      'props' in (value as Record<string, unknown>),
  )
}

function sanitizeValue(value: unknown): unknown {
  if (value == null) return value
  if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
    return value
  }

  if (Array.isArray(value)) {
    return value
      .map((entry) => sanitizeValue(entry))
      .filter((entry) => entry !== undefined)
  }

  if (typeof value === 'object') {
    const out: Record<string, unknown> = {}
    for (const [key, entry] of Object.entries(value)) {
      if (typeof entry === 'function') continue
      if (key === 'children') continue
      const sanitized = sanitizeValue(entry)
      if (sanitized !== undefined) {
        out[key] = sanitized
      }
    }
    return out
  }

  return undefined
}

function mimeTypeForAsset(path: string): string {
  switch (extname(path).toLowerCase()) {
    case '.svg':
      return 'image/svg+xml'
    case '.png':
      return 'image/png'
    case '.jpg':
    case '.jpeg':
      return 'image/jpeg'
    case '.gif':
      return 'image/gif'
    case '.webp':
      return 'image/webp'
    case '.ico':
      return 'image/x-icon'
    default:
      return 'application/octet-stream'
  }
}

function resolveExtensionMarkdownAssets(markdown: string, packageRoot: string): string {
  return markdown.replace(/!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g, (match, alt: string, rawSrc: string) => {
    const src = String(rawSrc || '').trim()
    if (!src || /^(?:https?:|data:|file:)/i.test(src)) return match

    const cleanSrc = src.split(/[?#]/)[0]?.replace(/^\.?\//, '') ?? ''
    if (!cleanSrc || cleanSrc.startsWith('/') || cleanSrc.includes('..')) return match

    const assetPath = join(packageRoot, 'assets', cleanSrc)
    if (!existsSync(assetPath)) {
      console.warn(`[ExtensionAssets] Missing markdown asset: ${assetPath}`)
      return match
    }

    try {
      const encoded = readFileSync(assetPath).toString('base64')
      console.log(`[ExtensionAssets] Inlined markdown asset: ${assetPath}`)
      return `![${alt}](data:${mimeTypeForAsset(assetPath)};base64,${encoded})`
    } catch {
      return match
    }
  })
}

function registerAction(
  typeName: string,
  props: Record<string, unknown>,
  session: RuntimeSession,
): void {
  const index = session.currentActions.length
  const title = normalizeActionTitle(typeName, props)
  const id = stableActionId(index, typeName, title)

  const kind: ExtensionRuntimeAction['kind'] =
    typeName === 'Action.CopyToClipboard' || typeName === 'Action.Paste'
      ? 'copy'
      : typeName === 'Action.OpenInBrowser'
        ? 'open'
        : typeName === 'Action.Push'
          ? 'push'
          : typeName === 'Action.Pop'
            ? 'pop'
            : typeName === 'Action.SubmitForm'
              ? 'submit-form'
              : typeName === 'Action.ShowInFinder'
                ? 'show-in-finder'
                : 'action'

  const style = typeof props.style === 'string' && props.style.toLowerCase() === 'destructive'
    ? 'destructive'
    : 'default'

  const action: ExtensionRuntimeAction = {
    id,
    title,
    style,
    shortcut: parseShortcut(props.shortcut),
    kind,
  }

  session.currentActions.push(action)

  const handler: RuntimeActionHandler = async (formValues) => {
    if (kind === 'copy') {
      const content = props.content ?? props.title ?? ''
      copyToSystemClipboard(content)
      if (typeof props.onPaste === 'function') {
        await Promise.resolve((props.onPaste as () => unknown)())
      }
    }

    if (kind === 'open') {
      const url = typeof props.url === 'string' ? props.url : ''
      if (url) {
        await shell.openExternal(url)
      }
    }

    if (kind === 'show-in-finder') {
      const path = typeof props.path === 'string' ? props.path : ''
      if (path) {
        shell.showItemInFolder(path)
      }
    }

    if (kind === 'push' && props.target !== undefined) {
      session.stack.push(props.target)
    }

    if (kind === 'pop') {
      if (session.stack.length > 1) {
        session.stack.pop()
      }
    }

    if (kind === 'submit-form' && typeof props.onSubmit === 'function') {
      await Promise.resolve((props.onSubmit as (values?: Record<string, string>) => unknown)(formValues ?? {}))
      return
    }

    if (typeof props.onAction === 'function') {
      await Promise.resolve((props.onAction as () => unknown)())
    }
  }

  session.actionHandlers.set(id, handler)
}

function hasDirectListSections(input: unknown): boolean {
  if (Array.isArray(input)) return input.some((entry) => hasDirectListSections(entry))
  if (!isJsxNode(input)) return false
  if (input.type === JSX_FRAGMENT) return hasDirectListSections(input.props?.children)
  return isToken(input.type) && input.type.name === 'List.Section'
}

function walkRuntimeNodes(
  input: unknown,
  session: RuntimeSession,
  depth: number,
  budget: { remaining: number },
  options: WalkRuntimeOptions = {},
): ExtensionRuntimeNode[] {
  if (budget.remaining <= 0 || depth > RUNTIME_RECURSION_LIMIT) {
    return []
  }

  if (input == null || typeof input === 'boolean') {
    return []
  }

  if (Array.isArray(input)) {
    const nodes: ExtensionRuntimeNode[] = []
    for (const entry of input) {
      if (
        options.listItemsSeen &&
        options.listItemLimit &&
        options.listItemsSeen.count >= options.listItemLimit
      ) {
        if (options.listItemsTruncated) options.listItemsTruncated.value = true
        break
      }
      nodes.push(...walkRuntimeNodes(entry, session, depth, budget, options))
    }
    return nodes
  }

  if (!isJsxNode(input)) {
    return []
  }

  const type = input.type
  const props = input.props ?? {}

  if (type === JSX_FRAGMENT) {
    return walkRuntimeNodes(props.children, session, depth + 1, budget, options)
  }

  if (typeof type === 'function') {
    let rendered: unknown
    try {
      const component = type as {
        (componentProps: Record<string, unknown>): unknown
        prototype?: {
          render?: () => unknown
        }
        getDerivedStateFromProps?: (
          props: Record<string, unknown>,
          state: unknown,
        ) => unknown
      }
      if (typeof component.prototype?.render === 'function') {
        const instance = new (component as unknown as new (
          componentProps: Record<string, unknown>,
        ) => {
          props: Record<string, unknown>
          state: unknown
          render: () => unknown
        })(props)
        const derived = component.getDerivedStateFromProps?.(props, instance.state)
        if (derived && typeof derived === 'object') {
          instance.state = {
            ...(instance.state && typeof instance.state === 'object' ? instance.state : {}),
            ...derived,
          }
        }
        rendered = instance.render()
      } else {
        rendered = component(props)
      }
    } catch (error) {
      console.error('[ExtensionRuntime] Component render failed:', error)
      return []
    }
    return walkRuntimeNodes(rendered, session, depth + 1, budget, options)
  }

  const typeName = isToken(type)
    ? type.name
    : typeof type === 'string'
      ? type
      : ''
  if (!typeName) return []

  if (typeName.startsWith('Action')) {
    if (typeName === 'ActionPanel' || typeName.startsWith('ActionPanel.')) {
      return walkRuntimeNodes(props.children, session, depth + 1, budget, options)
    }
    registerAction(typeName, props, session)
    return []
  }

  if (typeName === 'List.Item' && options.listItemsSeen && options.listItemLimit) {
    if (options.listItemsSeen.count >= options.listItemLimit) {
      return []
    }
    options.listItemsSeen.count += 1
  }

  if (typeName === 'List' && typeof props.onSearchTextChange === 'function') {
    session.searchTextChangeHandler = props.onSearchTextChange as (text: string) => void
  }

  const actionStart = session.currentActions.length
  if (props.actions !== undefined) {
    walkRuntimeNodes(props.actions, session, depth + 1, budget, options)
  }
  const actionIds = session.currentActions.slice(actionStart).map((action) => action.id)

  const metadataNodes = props.metadata !== undefined
    ? walkRuntimeNodes(props.metadata, session, depth + 1, budget, options)
    : []
  const detailNodes = props.detail !== undefined
    ? walkRuntimeNodes(props.detail, session, depth + 1, budget, options)
    : []
  const searchBarAccessoryNodes = props.searchBarAccessory !== undefined
    ? walkRuntimeNodes(props.searchBarAccessory, session, depth + 1, budget, options)
    : []

  budget.remaining -= 1
  const sanitizedProps = sanitizeValue(props) as Record<string, unknown> | undefined
  if (actionIds.length > 0) {
    if (sanitizedProps) {
      sanitizedProps.actionIds = actionIds
    }
  }
  if (detailNodes[0] && sanitizedProps) {
    sanitizedProps.detail = detailNodes[0]
  }
  if (searchBarAccessoryNodes[0] && sanitizedProps) {
    sanitizedProps.searchBarAccessory = searchBarAccessoryNodes[0]
  }
  if (metadataNodes[0] && sanitizedProps) {
    sanitizedProps.metadata = metadataNodes[0]
  }
  if (typeName === 'List.Dropdown' && typeof props.onChange === 'function' && sanitizedProps) {
    const actionId = makeId('list-dropdown')
    sanitizedProps.actionId = actionId
    session.actionHandlers.set(actionId, async (formValues) => {
      await Promise.resolve(
        (props.onChange as (value: string) => unknown)(String(formValues?.value ?? '')),
      )
    })
  }
  if (typeName === 'List' && session.searchTextChangeHandler && sanitizedProps) {
    sanitizedProps.__hasServerSearch = true
  }
  const listItemsTruncated =
    typeName === 'List' ? { value: false } : options.listItemsTruncated
  const childOptions =
    typeName === 'List'
      ? {
          ...options,
          listItemsSeen: hasDirectListSections(props.children) ? undefined : { count: 0 },
          listItemLimit: session.listItemLimit,
          listItemsTruncated,
        }
      : typeName === 'List.Section'
        ? {
            ...options,
            listItemsSeen: { count: 0 },
            listItemLimit: session.listItemLimit,
          }
        : options
  const children = walkRuntimeNodes(props.children, session, depth + 1, budget, childOptions)
  if (typeName === 'List' && sanitizedProps && listItemsTruncated) {
    sanitizedProps.__hasMore =
      listItemsTruncated.value && session.listItemLimit < RUNTIME_COMPONENT_LIMIT
    sanitizedProps.__pageSize = session.listItemLimit
  }
  const node: ExtensionRuntimeNode = {
    type: typeName,
    props: sanitizedProps,
    children,
    metadata: metadataNodes[0],
  }

  return [node]
}

function formatFeedback(feedback: RuntimeFeedback | undefined): string | undefined {
  if (!feedback) return undefined
  if (feedback.kind === 'hud') {
    return feedback.message || undefined
  }
  const title = feedback.title?.trim() || ''
  const message = feedback.message?.trim() || ''
  if (title && message) return `${title}: ${message}`
  return title || message || undefined
}

function renderCurrentView(session: RuntimeSession): ExtensionRunCommandResult | ExtensionInvokeActionResult {
  const startedAt = Date.now()
  console.log(
    `[Runner] renderCurrentView start ${session.extensionId}/${session.commandName}; stack=${session.stack.length}, limit=${session.listItemLimit}`,
  )
  const top = session.stack.at(-1)
  if (!top) {
    return {
      ok: false,
      message: 'No view is available for this extension session.',
    }
  }

  session.actionHandlers.clear()
  session.currentActions = []

  const budget = { remaining: RUNTIME_COMPONENT_LIMIT }
  const nodes = walkRuntimeNodes(top, session, 0, budget)
  console.log(
    `[Runner] walkRuntimeNodes complete after ${elapsedMs(startedAt)}; nodes=${nodes.length}, budgetUsed=${RUNTIME_COMPONENT_LIMIT - budget.remaining}, actions=${session.currentActions.length}`,
  )

  const root: ExtensionRuntimeNode = nodes[0] ?? {
    type: 'Detail',
    props: { markdown: 'This extension returned an empty view.' },
    children: [],
  }
  attachRuntimeRootMetadata(root, session)
  console.log(
    `[Runner] renderCurrentView complete after ${elapsedMs(startedAt)}; root=${root.type}, children=${root.children?.length ?? 0}`,
  )

  return {
    ok: true,
    mode: 'view',
    message: formatFeedback(session.feedback.at(-1)),
    sessionId: session.id,
    extensionId: session.extensionId,
    commandName: session.commandName,
    title: session.title,
    root,
    actions: [...session.currentActions],
  }
}

async function rerenderSessionCommand(
  session: RuntimeSession,
  label: string,
): Promise<ExtensionRunCommandResult | ExtensionInvokeActionResult> {
  if (!session.commandFn) {
    return renderCurrentView(session)
  }

  console.log(`[Runner] ${label}: rerendering ${session.extensionId}/${session.commandName}`)
  const startedAt = Date.now()
  session.hookIndex = 0
  session.pendingPromises = []
  session.actionHandlers.clear()
  session.currentActions = []
  session.feedback = []
  session.hasStateUpdates = false

  console.log(`[Runner] ${label}: command function start`)
  const result = await Promise.resolve(session.commandFn({ arguments: session.commandArgs }))
  console.log(
    `[Runner] ${label}: command function complete after ${elapsedMs(startedAt)}; jsx=${isJsxNode(result)}`,
  )
  session.stack = isJsxNode(result) ? [result] : []
  const view = renderCurrentView(session)
  console.log(`[Runner] ${label}: rerender complete after ${elapsedMs(startedAt)}`)
  return view
}

export async function refreshExtensionSession(
  request: ExtensionRefreshSessionRequest,
): Promise<ExtensionRefreshSessionResult> {
  const sessionId = String(request.sessionId || '').trim()
  if (!sessionId) {
    return { ok: false, message: 'sessionId is required.' }
  }

  const session = sessions.get(sessionId)
  if (!session) {
    return { ok: false, message: 'Extension session not found.' }
  }

  const inFlight = [...session.promiseCache.values()]
    .filter((entry) => entry.promise)
    .map((entry) => `${entry.label ?? 'unknown'} age=${entry.startedAt ? elapsedMs(entry.startedAt) : '?'}`)
  console.log(
    `[Runner] Refresh request ${session.extensionId}/${session.commandName}; stateUpdates=${session.hasStateUpdates}, inFlight=${inFlight.length}${
      inFlight.length ? `\n  ${inFlight.join('\n  ')}` : ''
    }`,
  )
  if (!session.hasStateUpdates) return { ok: true, mode: 'unchanged' }

  return rerenderSessionCommand(session, 'Refresh')
}

export async function loadMoreExtensionSession(
  request: ExtensionLoadMoreSessionRequest,
): Promise<ExtensionRefreshSessionResult> {
  const sessionId = String(request.sessionId || '').trim()
  const session = sessions.get(sessionId)
  if (!session) return { ok: false, message: 'Extension session not found.' }

  const nextLimit = Math.min(
    session.listItemLimit + LIST_ITEM_PAGE_SIZE,
    RUNTIME_COMPONENT_LIMIT,
  )
  if (nextLimit === session.listItemLimit) return { ok: true, mode: 'unchanged' }

  session.listItemLimit = nextLimit
  return rerenderSessionCommand(session, 'Load more')
}

export async function updateSearchText(
  request: ExtensionSearchTextChangedRequest,
): Promise<ExtensionSearchTextChangedResult> {
  const sessionId = String(request.sessionId || '').trim()
  const searchText = String(request.searchText ?? '')
  if (!sessionId) {
    return { ok: false, message: 'sessionId is required.' }
  }

  const session = sessions.get(sessionId)
  if (!session) {
    return { ok: false, message: 'Extension session not found.' }
  }

  session.searchText = searchText
  session.listItemLimit = LIST_ITEM_PAGE_SIZE

  if (session.searchTextChangeHandler) {
    try {
      session.searchTextChangeHandler(searchText)
    } catch (error) {
      return {
        ok: false,
        message: error instanceof Error ? error.message : String(error),
      }
    }
  }

  if (session.commandFn) {
    try {
      const execArgs = { arguments: session.commandArgs }
      let result: unknown

      const searchPass = async (label: string): Promise<unknown> => {
        console.log(`[Runner] ${label}: executing ${session.extensionId}/${session.commandName} search="${searchText}"`)
        session.hookIndex = 0
        session.pendingPromises = []
        session.actionHandlers.clear()
        session.currentActions = []
        session.feedback = []
        session.hasStateUpdates = false
        const r = await Promise.resolve(session.commandFn!(execArgs))
        console.log(
          `[Runner] ${label} complete: ${session.pendingPromises.length} promises, ${session.hookStates.length} states, stateUpdates=${session.hasStateUpdates}`,
        )
        return r
      }

      for (let p = 1; p <= SEARCH_TEXT_RENDER_PASSES; p += 1) {
        result = await searchPass(`Search Pass ${p}`)
        if (!session.hasStateUpdates) break
      }

      const searchInFlight = [...session.promiseCache.values()].filter((entry) => entry.promise).length
      if (searchInFlight > 0) {
        console.log(`[Runner] Search render returned with ${searchInFlight} in-flight promises; refresh will continue.`)
      }

      session.stack = isJsxNode(result) ? [result] : []

      if (session.stack.length > 0) {
        return renderCurrentView(session)
      }
    } catch (error) {
      return {
        ok: false,
        message: error instanceof Error ? error.message : String(error),
      }
    }
  }

  return {
    ok: true,
    mode: 'no-view',
    message: formatFeedback(session.feedback.at(-1)) || '',
  }
}

function cleanupRuntimeSession(session: RuntimeSession): void {
  if (session.disposed) return
  session.disposed = true

  for (const controller of session.abortControllers) {
    controller.abort()
  }
  session.abortControllers.clear()

  for (const cleanup of session.effectCleanups.values()) {
    try {
      cleanup()
    } catch (error) {
      console.warn('[Runner] Extension effect cleanup failed:', error)
    }
  }
  session.effectCleanups.clear()
  session.effectDeps.clear()
  session.promiseCache.clear()
  session.promiseKeysByHook.clear()
  session.cacheRecoveryKeys.clear()
}

function deleteRuntimeSession(sessionId: string): boolean {
  const session = sessions.get(sessionId)
  if (!session) return false
  cleanupRuntimeSession(session)
  return sessions.delete(sessionId)
}

function pruneSessions(): void {
  if (sessions.size <= SESSIONS_SOFT_LIMIT) return
  const ids = [...sessions.keys()]
  const overflow = sessions.size - SESSIONS_SOFT_LIMIT
  for (let i = 0; i < overflow; i += 1) {
    const id = ids[i]
    if (id) deleteRuntimeSession(id)
  }
}

function runBundle(
  code: string,
  packageRoot: string,
  session: RuntimeSession,
): unknown {
  const fileRequire = createRequire(join(packageRoot, 'package.json'))
  const jsxRuntimeShim = createJsxRuntimeShim()
  const reactShim = createReactShim(session)
  const raycastApiShim = createRaycastApiShim(session)
  const raycastUtilsShim = createRaycastUtilsShim(session)

  const customRequire = (specifier: string): unknown => {
    if (specifier === '@raycast/api') return raycastApiShim
    if (specifier === '@raycast/utils') return raycastUtilsShim
    if (specifier === 'react') return reactShim
    if (specifier === 'react/jsx-runtime' || specifier === 'react/jsx-dev-runtime') {
      return jsxRuntimeShim
    }
    if (specifier === 'child_process' || specifier === 'node:child_process') {
      return {
        ...fileRequire(specifier),
        spawn: (...args: Parameters<typeof nodeSpawn>) => {
          const child = nodeSpawn(...args)
          const stdout = child.stdout as (typeof child.stdout & {
            on: (event: string, listener: (...listenerArgs: unknown[]) => void) => typeof child.stdout
          }) | null

          if (stdout) {
            const originalOn = stdout.on.bind(stdout)
            const dataListeners = new Set<(chunk: Buffer) => void>()
            const pendingChunks: Buffer[] = []
            let buffer = ''

            const flushLine = (line: string): void => {
              const trimmed = line.trim()
              if (!trimmed) return
              const payload = Buffer.from(trimmed)
              if (dataListeners.size === 0) {
                pendingChunks.push(payload)
                return
              }
              for (const listener of dataListeners) {
                listener(payload)
              }
            }

            const flushBuffer = (): void => {
              if (!buffer.trim()) return
              flushLine(buffer)
              buffer = ''
            }

            originalOn('data', (chunk: Buffer | string) => {
              buffer += Buffer.isBuffer(chunk) ? chunk.toString('utf8') : String(chunk)
              let newlineIndex = buffer.indexOf('\n')
              while (newlineIndex >= 0) {
                flushLine(buffer.slice(0, newlineIndex))
                buffer = buffer.slice(newlineIndex + 1)
                newlineIndex = buffer.indexOf('\n')
              }
            })
            child.once('exit', flushBuffer)

            stdout.on = ((event: string, listener: (...listenerArgs: unknown[]) => void) => {
              if (event === 'data') {
                const dataListener = listener as (chunk: Buffer) => void
                dataListeners.add(dataListener)
                while (pendingChunks.length > 0) {
                  const chunk = pendingChunks.shift()
                  if (chunk) dataListener(chunk)
                }
                return stdout
              }
              return originalOn(event, listener)
            }) as typeof stdout.on
          }

          return child
        },
      }
    }
    if (specifier === 'raycast-cross-extension') {
      return {
        callbackLaunchCommand: async (): Promise<void> => {},
        launchCommand: async (): Promise<void> => {},
      }
    }
    if (specifier === 'sha256-file') {
      return (filename: string, callback: (error: Error | null, sum?: string) => void): void => {
        try {
          const sum = createHash('sha256').update(readFileSync(filename)).digest('hex')
          callback(null, sum)
        } catch (error) {
          callback(error instanceof Error ? error : new Error(String(error)))
        }
      }
    }
    if (specifier === 'axios') {
      type AxiosShim = {
        get: typeof axiosGetShim
        post: (_url: string, _data?: unknown, _options?: unknown) => Promise<{ data: Record<string, never>; status: number; headers: Record<string, never> }>
        defaults: { headers: { common: Record<string, string> } }
        interceptors: {
          request: { use: () => number; eject: () => void }
          response: { use: () => number; eject: () => void }
        }
        create?: () => AxiosShim
        request?: typeof axiosGetShim
        default?: AxiosShim
        __esModule: true
      }
      const axiosShim: AxiosShim = {
        get: axiosGetShim,
        post: async () => {
          // Some extensions might use POST for speedtest (though unlikely)
          return { data: {}, status: 200, headers: {} }
        },
        defaults: { headers: { common: {} } },
        interceptors: {
          request: { use: () => 0, eject: () => {} },
          response: { use: () => 0, eject: () => {} },
        },
        __esModule: true,
      }
      axiosShim.create = () => axiosShim
      axiosShim.default = axiosShim
      return axiosShim
    }
    if (specifier === 'node-fetch' || specifier === 'cross-fetch') {
      return createFetchModuleShim()
    }
    if (specifier === 'undici') {
      class ProxyAgent {
        readonly uri: string

        constructor(uri: string) {
          this.uri = uri
        }
      }

      const request = async (
        url: string | URL,
        options?: {
          method?: string
          body?: BodyInit | null
          headers?: HeadersInit
        },
      ): Promise<{
        statusCode: number
        headers: Record<string, string>
        body: {
          text: () => Promise<string>
          json: () => Promise<unknown>
          arrayBuffer: () => Promise<ArrayBuffer>
        }
      }> => {
        const response = await fetch(url, {
          method: options?.method,
          body: options?.body,
          headers: options?.headers,
        })
        const headers = Object.fromEntries(response.headers.entries())
        return {
          statusCode: response.status,
          headers,
          body: {
            text: () => response.clone().text(),
            json: () => response.clone().json() as Promise<unknown>,
            arrayBuffer: () => response.clone().arrayBuffer(),
          },
        }
      }

      return {
        request,
        fetch,
        ProxyAgent,
        default: { request, fetch, ProxyAgent },
        __esModule: true,
      }
    }
    if (specifier === 'tar') {
      const extract = async (options: { file?: string; cwd?: string; filter?: (path: string | ((path: string) => boolean)) => boolean }): Promise<void> => {
        if (!options?.file || !options.cwd) throw new Error('tar.extract requires file and cwd')
        mkdirSync(options.cwd, { recursive: true })
        // Use -x (extract), -z (gzip), -f (file). -k (keep old files) can be risky, so we use -o (overwrite) which is default.
        const args = ['-xzf', options.file, '-C', options.cwd]
        
        // Raycast speedtest-net shim usually passes a filter function.
        // If it asks for 'speedtest', we extract specifically that.
        try {
          if (typeof options.filter === 'function') {
             if (options.filter('speedtest')) args.push('speedtest')
          } else if (options.filter === 'speedtest') {
             args.push('speedtest')
          }
        } catch {
          // ignore filter errors
        }
        
        await execFileAsync('/usr/bin/tar', args)
      }
      return {
        extract,
        x: extract,
        default: { extract, x: extract },
        __esModule: true,
      }
    }
    if (specifier === 'extract-zip') {
      const extractZip = async (file: string, options?: { dir?: string }): Promise<void> => {
        const dir = options?.dir
        if (!dir) throw new Error('extract-zip requires dir')
        mkdirSync(dir, { recursive: true })
        await execFileAsync('/usr/bin/unzip', ['-o', file, '-d', dir])
      }
      return {
        default: extractZip,
        __esModule: true,
      }
    }
    if (specifier.startsWith('swift:') || specifier.startsWith('rust:')) {
      const pickColor = async (): Promise<Awaited<ReturnType<typeof pickColorWithNativeSampler>>> => {
        if (session.commandName === 'color-wheel') return null
        const picked = await pickColorWithNativeSampler()
        session.pickedColor = picked
        return picked
      }
      return {
        pickColor,
        pick_color: pickColor,
      }
    }

    if (specifier.startsWith('./') || specifier.startsWith('../') || specifier.startsWith('/')) {
      return fileRequire(specifier)
    }

    if (specifier.startsWith('node:') || BUILTIN_SET.has(specifier)) {
      return fileRequire(specifier)
    }

    return fileRequire(specifier)
  }

  const webGlobals = globalThis as typeof globalThis & {
    fetch?: typeof fetch
    AbortController?: typeof AbortController
    AbortSignal?: typeof AbortSignal
    Headers?: typeof Headers
    Request?: typeof Request
    Response?: typeof Response
    ReadableStream?: typeof NodeReadableStream
    TransformStream?: typeof NodeTransformStream
    WritableStream?: typeof NodeWritableStream
  }
  const loggedFetch = createLoggedFetch(session)

  const context = vm.createContext({
    console,
    Buffer,
    process,
    fetch: loggedFetch,
    AbortController: webGlobals.AbortController,
    AbortSignal: webGlobals.AbortSignal,
    Headers: webGlobals.Headers,
    Request: webGlobals.Request,
    Response: webGlobals.Response,
    ReadableStream: webGlobals.ReadableStream ?? NodeReadableStream,
    TransformStream: webGlobals.TransformStream ?? NodeTransformStream,
    WritableStream: webGlobals.WritableStream ?? NodeWritableStream,
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    TextEncoder,
    TextDecoder,
    URL,
  })

  const runtimeCode = code.replace(
    /\bimport\(\s*(["'])(swift:[^"']+|rust:[^"']+)\1\s*\)/g,
    (_match, quote: string, specifier: string) => `Promise.resolve(require(${quote}${specifier}${quote}))`,
  )
  const wrapped = `(function(exports, require, module, __filename, __dirname) {\n${runtimeCode}\n})`
  const script = new vm.Script(wrapped, {
    filename: join(packageRoot, '.raymes-runtime-bundle.cjs'),
  })

  const fn = script.runInContext(context)
  const mod: { exports: unknown } = { exports: {} }
  fn(mod.exports, customRequire, mod, join(packageRoot, '.raymes-runtime-bundle.cjs'), packageRoot)
  return mod.exports
}

function getCommandExport(moduleExports: unknown): ((props: { arguments: Record<string, string> }) => unknown) | null {
  if (typeof moduleExports === 'function') {
    return moduleExports as (props: { arguments: Record<string, string> }) => unknown
  }

  if (moduleExports && typeof moduleExports === 'object') {
    const exp = moduleExports as { default?: unknown }
    if (typeof exp.default === 'function') {
      return exp.default as (props: { arguments: Record<string, string> }) => unknown
    }
  }

  return null
}

async function runCommandFromPackagePath(
  packageJsonPath: string,
  extensionId: string,
  commandName: string,
  argumentValues: Record<string, string>,
): Promise<ExtensionRunCommandResult> {
  const packageRoot = dirname(packageJsonPath)
  const pkg = parsePackageJson(packageJsonPath)
  const command = findCommandInManifest(pkg, commandName)

  const mode = String(command.mode || '').toLowerCase()
  const title = String(command.title || commandName)
  const entryPath = resolveCommandEntry(packageRoot, commandName, command)
  console.log(`[Runner] Mode=${mode}, title="${title}", entry=${entryPath}`)
  const bundled = await bundleCommand(entryPath, packageRoot)
  console.log(`[Runner] Bundle size: ${bundled.length} chars`)

  const session: RuntimeSession = {
    id: makeId('ext-session'),
    extensionId,
    commandName,
    title,
    packageRoot,
    actionHandlers: new Map(),
    currentActions: [],
    feedback: [],
    stack: [],
    preferences: getExtensionPreferences(extensionId, commandName),
    searchTextChangeHandler: null,
    commandFn: null,
    commandArgs: argumentValues,
    bundledCode: bundled,
    searchText: '',
    hookStates: [],
    hookIndex: 0,
    pendingPromises: [],
    promiseCache: new Map(),
    promiseKeysByHook: new Map(),
    cacheRecoveryKeys: new Set(),
    abortControllers: new Set(),
    effectCleanups: new Map(),
    effectDeps: new Map(),
    hasStateUpdates: false,
    disposed: false,
    listItemLimit: LIST_ITEM_PAGE_SIZE,
    hookStateSnapshot: null,
    pickedColor: null,
  }

  if (shouldShowExtensionPreferenceSetup(extensionId, commandName)) {
    session.stack = [buildPreferenceSetupRoot(extensionId, commandName)]
    sessions.set(session.id, session)
    pruneSessions()
    return renderCurrentView(session)
  }

  const moduleExports = runBundle(bundled, packageRoot, session)
  const commandFn = getCommandExport(moduleExports)
  if (!commandFn) {
    return { ok: false, message: 'Extension command entry is not executable.' }
  }

  session.commandFn = commandFn as (...args: unknown[]) => unknown

  // Initial execution should show the extension view immediately. Async hooks
  // hydrate through the session refresh loop instead of blocking the open.
  const execArgs = { arguments: argumentValues }
  let result: unknown

  const executePass = async (passLabel: string): Promise<void> => {
    console.log(`[Runner] ${passLabel}: executing ${extensionId}/${commandName}`)
    session.hookIndex = 0
    session.pendingPromises = []
    session.actionHandlers.clear()
    session.currentActions = []
    session.feedback = []
    session.hasStateUpdates = false
    result = await Promise.resolve(commandFn(execArgs))
    console.log(
      `[Runner] ${passLabel} complete: ${session.pendingPromises.length} promises, ${session.hookStates.length} hook states, stateUpdates=${session.hasStateUpdates}`,
    )
  }

  for (let p = 1; p <= INITIAL_RENDER_PASSES; p += 1) {
    await executePass(`Pass ${p}`)
    if (!session.hasStateUpdates) break
  }

  const remainingInFlight = [...session.promiseCache.values()].filter((entry) => entry.promise).length
  if (remainingInFlight > 0) {
    console.log(`[Runner] Initial multi-pass exited with ${remainingInFlight} in-flight promises; polling refresh will pick them up.`)
  }

  if (commandName === 'pick-color' && session.pickedColor) {
    session.title = 'Color Wheel'
    session.stack = [{
      __jsx: true,
      type: makeToken('Detail'),
      props: {
        markdown: colorWheelMarkdown(),
        initialColor: session.pickedColor,
      },
    }]
    sessions.set(session.id, session)
    pruneSessions()
    return renderCurrentView(session)
  }

  if (mode === 'no-view' || !isJsxNode(result)) {
    const message = formatFeedback(session.feedback.at(-1)) || ''
    return {
      ok: true,
      mode: 'no-view',
      message,
    }
  }

  session.stack = [result]
  sessions.set(session.id, session)
  pruneSessions()
  return renderCurrentView(session)
}

export async function runExtensionCommand(
  request: ExtensionRunCommandRequest,
): Promise<ExtensionRunCommandResult> {
  const extensionId = String(request.extensionId || '').trim()
  const commandName = String(request.commandName || '').trim()
  console.log(`[Runner] runExtensionCommand called: ${extensionId}/${commandName}`)
  if (!extensionId || !commandName) {
    return { ok: false, message: 'Extension id and command name are required.' }
  }

  const packagePath = resolveInstalledPackageJsonPath(extensionId)
  if (!packagePath) {
    console.error(`[Runner] Extension not installed: ${extensionId}`)
    return { ok: false, message: `Extension is not installed: ${extensionId}` }
  }
  console.log(`[Runner] Found package.json at ${packagePath}`)

  for (const [sessionId, session] of sessions) {
    if (session.extensionId === extensionId && session.commandName === commandName) {
      deleteRuntimeSession(sessionId)
    }
  }

  try {
    return await runCommandFromPackagePath(
      packagePath,
      extensionId,
      commandName,
      request.argumentValues ?? {},
    )
  } catch (error) {
    return {
      ok: false,
      message: error instanceof Error ? error.message : String(error),
    }
  }
}

export async function runExtensionCommandFromPackageJson(
  packageJsonPath: string,
  commandName: string,
  argumentValues?: Record<string, string>,
): Promise<ExtensionRunCommandResult> {
  const normalizedPath = String(packageJsonPath || '').trim()
  const normalizedCommandName = String(commandName || '').trim()
  if (!normalizedPath || !normalizedCommandName) {
    return { ok: false, message: 'packageJsonPath and commandName are required.' }
  }

  const extensionId = `raycast.${dirname(normalizedPath).split('/').pop() || 'external'}`
  try {
    return await runCommandFromPackagePath(
      normalizedPath,
      extensionId,
      normalizedCommandName,
      argumentValues ?? {},
    )
  } catch (error) {
    return {
      ok: false,
      message: error instanceof Error ? error.message : String(error),
    }
  }
}

export async function invokeExtensionAction(
  request: ExtensionInvokeActionRequest,
): Promise<ExtensionInvokeActionResult> {
  const sessionId = String(request.sessionId || '').trim()
  const actionId = String(request.actionId || '').trim()
  if (!sessionId || !actionId) {
    return { ok: false, message: 'sessionId and actionId are required.' }
  }

  const session = sessions.get(sessionId)
  if (!session) {
    return { ok: false, message: 'Extension session not found.' }
  }

  if (actionId === '__nav_pop__') {
    if (session.stack.length > 1) {
      session.stack.pop()
    }
    return renderCurrentView(session)
  }

  const handler = session.actionHandlers.get(actionId)
  if (!handler) {
    return { ok: false, message: 'Action is no longer available in this session.' }
  }

  try {
    await Promise.resolve(handler(request.formValues ?? {}))
    if (session.commandFn && session.hasStateUpdates) {
      return rerenderSessionCommand(session, 'Action')
    }
    if (session.stack.length > 0) {
      return renderCurrentView(session)
    }

    return {
      ok: true,
      mode: 'no-view',
      message: formatFeedback(session.feedback.at(-1)) || '',
    }
  } catch (error) {
    return {
      ok: false,
      message: error instanceof Error ? error.message : String(error),
    }
  }
}

export function disposeExtensionSession(sessionId: string): boolean {
  return deleteRuntimeSession(sessionId)
}

export function clearAllExtensionSessions(): void {
  for (const session of sessions.values()) {
    cleanupRuntimeSession(session)
  }
  sessions.clear()
}
