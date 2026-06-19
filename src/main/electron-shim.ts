/* eslint-disable @typescript-eslint/no-explicit-any, @typescript-eslint/no-unsafe-function-type */
// src/main/electron-shim.ts
import { join } from 'node:path'
import { homedir, tmpdir } from 'node:os'
import { execFile, execFileSync, spawn } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)

const backendWebContents = {
  id: 1,
  send(channel: string, payload: unknown): void {
    process.stdout.write(`${JSON.stringify({ type: 'event', channel, payload })}\n`)
  },
  isDestroyed(): boolean { return false },
  once(): void {},
}

class IpcMain {
  // Map containing channel handlers
  _handlers = new Map<string, Function>()

  handle(channel: string, callback: Function): void {
    this._handlers.set(channel, callback)
  }

  on(channel: string, callback: Function): void {
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
  getAppPath(): string { return process.cwd() },
  focus(): void {},
  hide(): void {},
  show(): void {},
  once(): void {},
  quit(): void {
    process.stdout.write(`${JSON.stringify({ type: 'app_quit' })}\n`)
  },
  exit(): void {
    process.stdout.write(`${JSON.stringify({ type: 'app_quit' })}\n`)
  }
}

export const shell = {
  async openExternal(url: string): Promise<void> {
    const command = process.platform === 'darwin' ? 'open' : 'xdg-open'
    await execFileAsync(command, [url])
  },
  async openPath(target: string): Promise<string> {
    const command = process.platform === 'darwin' ? 'open' : 'xdg-open'
    try {
      await execFileAsync(command, [target])
      return ''
    } catch (error) {
      return error instanceof Error ? error.message : String(error)
    }
  },
  showItemInFolder(target: string): void {
    if (process.platform === 'darwin') void execFileAsync('open', ['-R', target])
    else void execFileAsync('xdg-open', [join(target, '..')])
  },
}

type ShimNativeImage = ReturnType<typeof makeNativeImage>

function makeNativeImage(sourcePath?: string): {
  sourcePath?: string
  isEmpty: () => boolean
  setTemplateImage: () => void
  getSize: () => { width: number; height: number }
  resize: () => ShimNativeImage
  toPNG: () => Buffer
} {
  const image = {
    sourcePath,
    isEmpty: () => !sourcePath || !existsSync(sourcePath),
    setTemplateImage: () => undefined,
    getSize: () => ({ width: 0, height: 0 }),
    resize: () => image,
    toPNG: () => sourcePath && existsSync(sourcePath) ? readFileSync(sourcePath) : Buffer.alloc(0),
  }
  return image
}

export const clipboard = {
  readText(): string {
    try {
      return execFileSync('pbpaste', [], { encoding: 'utf8' })
    } catch {
      return ''
    }
  },
  writeText(text: string): void {
    try {
      const child = spawn('pbcopy')
      child.stdin.write(text)
      child.stdin.end()
    } catch {}
  },
  availableFormats(): string[] { return this.readText() ? ['text/plain'] : [] },
  read(): string { return '' },
  readImage(): ShimNativeImage { return makeNativeImage() },
  writeImage(image: ShimNativeImage): void {
    if (!image.sourcePath || process.platform !== 'darwin') return
    const escaped = image.sourcePath.replace(/\\/g, '\\\\').replace(/"/g, '\\"')
    void execFileAsync('osascript', ['-e', `set the clipboard to (read POSIX file "${escaped}" as PNG picture)`])
  },
  write(payload: { text?: string }): void { if (payload.text) this.writeText(payload.text) },
  clear(): void { this.writeText('') },
}

export const dialog = {
  async showMessageBox(windowOrOptions: any, maybeOptions?: any): Promise<{ response: number }> {
    const options = maybeOptions ?? windowOrOptions ?? {}
    const buttons: string[] = Array.isArray(options.buttons) && options.buttons.length > 0
      ? options.buttons.map(String)
      : ['OK']
    if (process.platform !== 'darwin') return { response: options.cancelId ?? 0 }

    const escapeAppleScript = (value: unknown): string =>
      String(value ?? '').replace(/\\/g, '\\\\').replace(/"/g, '\\"')
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
  }
}

export const session = {
  defaultSession: {
    async clearCache(): Promise<void> {},
    async clearStorageData(): Promise<void> {},
    setPermissionRequestHandler(): void {},
    setPermissionCheckHandler(): void {},
  }
}

export const screen = {
  getDisplayNearestPoint(): any {
    return { id: 1, bounds: { x: 0, y: 0, width: 1920, height: 1080 }, workArea: { x: 0, y: 0, width: 1920, height: 1080 } }
  },
  getCursorScreenPoint(): any {
    return { x: 0, y: 0 }
  },
  getAllDisplays(): any[] {
    return [{ id: 1, bounds: { x: 0, y: 0, width: 1920, height: 1080 }, workArea: { x: 0, y: 0, width: 1920, height: 1080 } }]
  }
}

export const nativeImage = {
  createFromPath(path: string): ShimNativeImage { return makeNativeImage(path) },
  createFromDataURL(): ShimNativeImage { return makeNativeImage() },
}

export const Menu = {
  buildFromTemplate(): any { return {} },
  setApplicationMenu(): void {}
}

export const globalShortcut = {
  register(): boolean { return true },
  unregister(): void {},
  unregisterAll(): void {}
}

export class BrowserWindow {
  static windows: BrowserWindow[] = []
  static getAllWindows(): BrowserWindow[] { return [...BrowserWindow.windows] }
  static getFocusedWindow(): BrowserWindow | null { return BrowserWindow.windows[0] ?? null }
  static fromWebContents(): BrowserWindow | null { return BrowserWindow.windows[0] ?? null }

  webContents = backendWebContents
  private visible = true
  private contentSize: [number, number] = [760, 640]
  constructor() { BrowserWindow.windows.push(this) }
  isDestroyed(): boolean { return false }
  isVisible(): boolean { return this.visible }
  destroy(): void { BrowserWindow.windows = BrowserWindow.windows.filter((window) => window !== this) }
  close(): void { this.destroy() }
  focus(): void {}
  show(): void { this.visible = true }
  hide(): void { this.visible = false }
  getContentSize(): [number, number] { return this.contentSize }
  setContentSize(width: number, height: number): void { this.contentSize = [width, height] }
  setMaximumSize(): void {}
}

export const webFrame = {
  getZoomFactor(): number { return 1 }
}

export const systemPreferences = {
  isTrusted(): boolean { return true },
  isTrustedAccessibilityClient(): boolean { return false },
  async askForMediaAccess(): Promise<boolean> { return false },
}
