/* eslint-disable @typescript-eslint/no-explicit-any, @typescript-eslint/no-unsafe-function-type */
// Runtime-neutral desktop services used by the Tauri backend sidecar.
import { extname, join } from 'node:path'
import { homedir, tmpdir } from 'node:os'
import { execFile, execFileSync, spawn } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)

function runDetached(command: string, args: string[], description: string): void {
  void execFileAsync(command, args).catch((error: unknown) => {
    console.error(`[desktop-runtime] ${description} failed:`, error)
  })
}

export function imageClipboardAppleScript(sourcePath: string): string | null {
  const clipboardClass =
    {
      '.gif': 'GIFf',
      '.jpg': 'JPEG',
      '.jpeg': 'JPEG',
      '.png': 'PNGf',
      '.tif': 'TIFF',
      '.tiff': 'TIFF',
    }[extname(sourcePath).toLowerCase()] ?? null
  if (!clipboardClass) return null

  const escaped = sourcePath.replace(/\\/g, '\\\\').replace(/"/g, '\\"')
  return `set the clipboard to (read (POSIX file "${escaped}") as «class ${clipboardClass}»)`
}

export type Rectangle = { x: number; y: number; width: number; height: number }

export type MessageBoxOptions = {
  type?: 'none' | 'info' | 'error' | 'question' | 'warning'
  buttons?: string[]
  defaultId?: number
  cancelId?: number
  title?: string
  message: string
  detail?: string
  noLink?: boolean
}

export type NativeImage = {
  sourcePath?: string
  isEmpty: () => boolean
  setTemplateImage: (value?: boolean) => void
  getSize: () => { width: number; height: number }
  resize: (options?: { width?: number; height?: number; quality?: string }) => NativeImage
  toPNG: () => Buffer
}

const backendWebContents = {
  id: 1,
  send(channel: string, ...args: unknown[]): void {
    process.stdout.write(`${JSON.stringify({ type: 'event', channel, payload: args[0] })}\n`)
  },
  isDestroyed(): boolean {
    return false
  },
  once(_event: string, _listener: () => void): void {},
}

export type WebContents = typeof backendWebContents

type IpcMainEvent = { sender: WebContents }
type IpcHandler = (event: IpcMainEvent, ...args: any[]) => unknown

class IpcMain {
  // Map containing channel handlers
  _handlers = new Map<string, IpcHandler>()

  handle(channel: string, callback: IpcHandler): void {
    this._handlers.set(channel, callback)
  }

  on(channel: string, callback: IpcHandler): void {
    this._handlers.set(channel, callback)
  }

  // Trigger a registered IPC handler from the outside
  async _invoke(channel: string, ...args: any[]): Promise<any> {
    const handler = this._handlers.get(channel)
    if (!handler) {
      throw new Error(`No handler registered for channel: ${channel}`)
    }
    return handler({ sender: backendWebContents }, ...args)
  }
}

export const ipcMain = new IpcMain()

export const app = {
  isPackaged: process.env.IS_TAURI === 'true',
  name: 'Tezbar',
  getPath(name: string): string {
    if (name === 'userData') {
      return process.env.APPDATA_DIR || join(homedir(), '.tezbar')
    }
    if (name === 'temp') {
      return process.env.TEMP_DIR || tmpdir()
    }
    if (name === 'home') {
      return homedir()
    }
    return join(homedir(), `.${name}`)
  },
  getVersion(): string {
    return process.env.APP_VERSION || '0.0.3'
  },
  getName(): string {
    return 'Tezbar'
  },
  getAppPath(): string {
    return process.cwd()
  },
  focus(_options?: { steal?: boolean }): void {},
  hide(): void {
    if (process.env.IS_TAURI === 'true') {
      process.stdout.write(`${JSON.stringify({ type: 'app_visibility', visible: false })}\n`)
    }
  },
  show(): void {
    if (process.env.IS_TAURI === 'true') {
      process.stdout.write(`${JSON.stringify({ type: 'app_visibility', visible: true })}\n`)
    }
  },
  once(_event: string, _listener: () => void): void {},
  quit(): void {
    process.stdout.write(`${JSON.stringify({ type: 'app_quit' })}\n`)
  },
  exit(_code?: number): void {
    process.stdout.write(`${JSON.stringify({ type: 'app_quit' })}\n`)
  },
}

export const shell = {
  async openExternal(url: string): Promise<void> {
    const command = process.platform === 'darwin' ? 'open' : process.platform === 'win32' ? 'explorer.exe' : 'xdg-open'
    await execFileAsync(command, [url])
  },
  async openPath(target: string): Promise<string> {
    const command = process.platform === 'darwin' ? 'open' : process.platform === 'win32' ? 'explorer.exe' : 'xdg-open'
    try {
      await execFileAsync(command, [target])
      return ''
    } catch (error) {
      return error instanceof Error ? error.message : String(error)
    }
  },
  showItemInFolder(target: string): void {
    if (process.platform === 'darwin') runDetached('open', ['-R', target], 'reveal item')
    else if (process.platform === 'win32') void execFileAsync('explorer.exe', ['/select,', target])
    else runDetached('xdg-open', [join(target, '..')], 'reveal item')
  },
}

function makeNativeImage(sourcePath?: string): NativeImage {
  const image: NativeImage = {
    sourcePath,
    isEmpty: () => !sourcePath || !existsSync(sourcePath),
    setTemplateImage: () => undefined,
    getSize: () => ({ width: 0, height: 0 }),
    resize: () => image,
    toPNG: () =>
      sourcePath && existsSync(sourcePath) ? readFileSync(sourcePath) : Buffer.alloc(0),
  }
  return image
}

export const clipboard = {
  readText(): string {
    try {
      if (process.platform === 'win32') {
        return execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', 'Get-Clipboard -Raw'], { encoding: 'utf8' })
      }
      return execFileSync('pbpaste', [], { encoding: 'utf8' })
    } catch {
      return ''
    }
  },
  writeText(text: string): void {
    try {
      if (process.platform === 'win32') {
        const child = spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', 'Set-Clipboard -Value ([Console]::In.ReadToEnd())'])
        child.stdin.write(text)
        child.stdin.end()
        return
      }
      const child = spawn('pbcopy')
      child.on('error', (error) =>
        console.error('[desktop-runtime] clipboard text copy failed:', error)
      )
      child.stdin.on('error', (error) =>
        console.error('[desktop-runtime] clipboard input failed:', error)
      )
      child.stdin.write(text)
      child.stdin.end()
    } catch (error) {
      console.error('[desktop-runtime] clipboard text copy failed:', error)
    }
  },
  availableFormats(): string[] {
    return this.readText() ? ['text/plain'] : []
  },
  read(_format?: string): string {
    return ''
  },
  readImage(): NativeImage {
    return makeNativeImage()
  },
  writeImage(image: NativeImage): void {
    if (!image.sourcePath || process.platform !== 'darwin') return
    const script = imageClipboardAppleScript(image.sourcePath)
    if (!script) {
      console.error(
        `[desktop-runtime] clipboard image copy failed: unsupported format ${extname(image.sourcePath) || '(none)'}`
      )
      return
    }
    runDetached('osascript', ['-e', script], 'clipboard image copy')
  },
  write(payload: { text?: string; html?: string; bookmark?: string }): void {
    if (payload.text) this.writeText(payload.text)
  },
  clear(): void {
    this.writeText('')
  },
}

export const dialog = {
  async showMessageBox(windowOrOptions: any, maybeOptions?: any): Promise<{ response: number }> {
    const options = maybeOptions ?? windowOrOptions ?? {}
    const buttons: string[] =
      Array.isArray(options.buttons) && options.buttons.length > 0
        ? options.buttons.map(String)
        : ['OK']
    if (process.platform !== 'darwin') return { response: options.cancelId ?? 0 }

    const escapeAppleScript = (value: unknown): string =>
      String(value ?? '')
        .replace(/\\/g, '\\\\')
        .replace(/"/g, '\\"')
    const buttonList = buttons.map((button) => `"${escapeAppleScript(button)}"`).join(', ')
    const defaultIndex = Math.min(Math.max(Number(options.defaultId) || 0, 0), buttons.length - 1)
    const cancelIndex = Math.min(Math.max(Number(options.cancelId) || 0, 0), buttons.length - 1)
    const message = [options.message, options.detail].filter(Boolean).join('\n\n')
    const script = `display dialog "${escapeAppleScript(message)}" with title "${escapeAppleScript(options.title ?? 'Tezbar')}" buttons {${buttonList}} default button "${escapeAppleScript(buttons[defaultIndex])}" cancel button "${escapeAppleScript(buttons[cancelIndex])}"`
    process.stdout.write(`${JSON.stringify({ type: 'window_suppress_blur', value: true })}\n`)
    try {
      const { stdout } = await execFileAsync('osascript', ['-e', script])
      const selected = buttons.findIndex((button) => stdout.includes(`button returned:${button}`))
      return { response: selected >= 0 ? selected : cancelIndex }
    } catch {
      return { response: cancelIndex }
    } finally {
      process.stdout.write(`${JSON.stringify({ type: 'window_suppress_blur', value: false })}\n`)
    }
  },
}

export const session = {
  defaultSession: {
    async clearCache(): Promise<void> {},
    async clearStorageData(_options?: { storages?: string[] }): Promise<void> {},
    setPermissionRequestHandler(): void {},
    setPermissionCheckHandler(): void {},
  },
}

export const screen = {
  getDisplayNearestPoint(_point?: { x: number; y: number }): any {
    return {
      id: 1,
      size: { width: 1920, height: 1080 },
      bounds: { x: 0, y: 0, width: 1920, height: 1080 },
      workArea: { x: 0, y: 0, width: 1920, height: 1080 },
    }
  },
  getCursorScreenPoint(): any {
    return { x: 0, y: 0 }
  },
  getAllDisplays(): any[] {
    return [this.getDisplayNearestPoint()]
  },
}

export const desktopCapturer = {
  async getSources(_options?: {
    types?: Array<'screen' | 'window'>
    thumbnailSize?: { width: number; height: number }
  }): Promise<Array<{ display_id: string; thumbnail: NativeImage }>> {
    // Tauri captures through its Rust command in tauri-bridge.ts. Keeping the
    // backend contract empty makes accidental calls fail with the IPC handler's
    // actionable Screen Recording error instead of a missing-export crash.
    return []
  },
}

export const nativeImage = {
  createFromPath(path: string): NativeImage {
    return makeNativeImage(path)
  },
  createFromDataURL(_dataUrl?: string): NativeImage {
    return makeNativeImage()
  },
  createFromBuffer(_buffer?: Buffer): NativeImage {
    return makeNativeImage()
  },
}

export const Menu = {
  buildFromTemplate(): any {
    return {}
  },
  setApplicationMenu(): void {},
}

export const globalShortcut = {
  register(): boolean {
    return true
  },
  unregister(): void {},
  unregisterAll(): void {},
}

export class BrowserWindow {
  static windows: BrowserWindow[] = []
  static getAllWindows(): BrowserWindow[] {
    return [...BrowserWindow.windows]
  }
  static getFocusedWindow(): BrowserWindow | null {
    return BrowserWindow.windows[0] ?? null
  }
  static fromWebContents(_contents?: WebContents): BrowserWindow | null {
    return BrowserWindow.windows[0] ?? null
  }

  webContents = backendWebContents
  private visible = true
  private opacity = 1
  private contentSize: [number, number] = [760, 640]
  constructor() {
    BrowserWindow.windows.push(this)
  }
  isDestroyed(): boolean {
    return false
  }
  isVisible(): boolean {
    return this.visible
  }
  destroy(): void {
    BrowserWindow.windows = BrowserWindow.windows.filter((window) => window !== this)
  }
  close(): void {
    this.destroy()
  }
  focus(): void {}
  show(): void {
    this.visible = true
  }
  hide(): void {
    this.visible = false
  }
  getContentSize(): [number, number] {
    return this.contentSize
  }
  setContentSize(width: number, height: number, _animate?: boolean): void {
    this.contentSize = [width, height]
  }
  getOpacity(): number {
    return this.opacity
  }
  setOpacity(value: number): void {
    this.opacity = value
  }
  setContentProtection(enabled: boolean): void {
    void enabled
  }
  setMaximumSize(_width?: number, _height?: number): void {}
}

export const webFrame = {
  getZoomFactor(): number {
    return 1
  },
}

export const systemPreferences = {
  isTrusted(_prompt?: boolean): boolean {
    return true
  },
  isTrustedAccessibilityClient(_prompt?: boolean): boolean {
    return false
  },
  getMediaAccessStatus(_type?: string): 'not-determined' {
    return 'not-determined'
  },
  async askForMediaAccess(_type?: string): Promise<boolean> {
    return false
  },
}
