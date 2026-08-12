/* eslint-disable @typescript-eslint/no-explicit-any, @typescript-eslint/no-unsafe-function-type */
// Runtime-neutral desktop services used by the Tauri backend sidecar.
import { extname, join } from 'node:path'
import { homedir, tmpdir } from 'node:os'
import { execFile, execFileSync, spawn } from 'node:child_process'
import { existsSync, readFileSync, rmSync } from 'node:fs'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)

function runDetached(command: string, args: string[], description: string): void {
  void execFileAsync(command, args).catch((error: unknown) => {
    console.error(`[desktop-runtime] ${description} failed:`, error)
  })
}

export function fileClipboardJavaScript(paths: string[]): string | null {
  const uniquePaths = Array.from(new Set(paths.map((path) => path.trim()).filter(Boolean)))
  if (uniquePaths.length === 0) return null
  return [
    'ObjC.import("AppKit")',
    'ObjC.import("Foundation")',
    `const paths = ${JSON.stringify(uniquePaths)}`,
    'const pasteboard = $.NSPasteboard.generalPasteboard',
    'pasteboard.clearContents',
    'const urls = $.NSMutableArray.alloc.init',
    'for (const path of paths) urls.addObject($.NSURL.fileURLWithPath($(path)))',
    'pasteboard.writeObjects(urls)',
  ].join('; ')
}

export function clipboardImageAppleScript(destinationPath: string): string {
  const escaped = destinationPath.replace(/\\/g, '\\\\').replace(/"/g, '\\\"')
  return [
    `set outputFile to POSIX file "${escaped}"`,
    'set fileRef to open for access outputFile with write permission',
    'set eof fileRef to 0',
    'write (the clipboard as «class PNGf») to fileRef',
    'close access fileRef',
  ].join('\n')
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
    const command =
      process.platform === 'darwin'
        ? 'open'
        : process.platform === 'win32'
          ? 'explorer.exe'
          : 'xdg-open'
    await execFileAsync(command, [url])
  },
  async openPath(target: string): Promise<string> {
    const command =
      process.platform === 'darwin'
        ? 'open'
        : process.platform === 'win32'
          ? 'explorer.exe'
          : 'xdg-open'
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

function pngSize(sourcePath?: string): { width: number; height: number } {
  if (!sourcePath || !existsSync(sourcePath)) return { width: 0, height: 0 }
  try {
    const bytes = readFileSync(sourcePath)
    if (bytes.length < 24 || bytes.readUInt32BE(0) !== 0x89504e47) return { width: 0, height: 0 }
    return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) }
  } catch {
    return { width: 0, height: 0 }
  }
}

function makeNativeImage(sourcePath?: string): NativeImage {
  const image: NativeImage = {
    sourcePath,
    isEmpty: () => !sourcePath || !existsSync(sourcePath),
    setTemplateImage: () => undefined,
    getSize: () => pngSize(sourcePath),
    resize: () => image,
    toPNG: () =>
      sourcePath && existsSync(sourcePath) ? readFileSync(sourcePath) : Buffer.alloc(0),
  }
  return image
}

export type ClipboardSnapshot = {
  text: string
  filePaths: string[]
  hasImage: boolean
  changeCount: number
}

function parseClipboardSnapshot(raw: string): ClipboardSnapshot {
  try {
    const parsed = JSON.parse(raw.trim()) as Partial<ClipboardSnapshot>
    const rawPaths = Array.isArray(parsed.filePaths)
      ? parsed.filePaths
      : typeof parsed.filePaths === 'string'
        ? [parsed.filePaths]
        : []
    return {
      text: typeof parsed.text === 'string' ? parsed.text : '',
      filePaths: Array.from(new Set(rawPaths.map(String).map((path) => path.trim()).filter(Boolean))),
      hasImage: parsed.hasImage === true,
      changeCount: Number.isFinite(Number(parsed.changeCount)) ? Number(parsed.changeCount) : 0,
    }
  } catch {
    return { text: '', filePaths: [], hasImage: false, changeCount: 0 }
  }
}

function readMacClipboardSnapshot(): ClipboardSnapshot {
  const script = [
    'ObjC.import("AppKit")',
    'ObjC.import("Foundation")',
    'const pasteboard = $.NSPasteboard.generalPasteboard',
    'const classes = $.NSArray.arrayWithObject($.NSURL)',
    'const urls = pasteboard.readObjectsForClassesOptions(classes, $.NSDictionary.dictionary)',
    'const filePaths = []',
    'if (urls && urls.count) { for (let i = 0; i < urls.count; i++) filePaths.push(ObjC.unwrap(urls.objectAtIndex(i).path)) }',
    'const text = pasteboard.stringForType($.NSPasteboardTypeString)',
    'const types = pasteboard.types',
    'const hasImage = !!(types && (types.containsObject($.NSPasteboardTypePNG) || types.containsObject($.NSPasteboardTypeTIFF)))',
    'const output = JSON.stringify({ text: text ? ObjC.unwrap(text) : "", filePaths: filePaths, hasImage: hasImage, changeCount: pasteboard.changeCount })',
    'const data = $(output).dataUsingEncoding($.NSUTF8StringEncoding)',
    '$.NSFileHandle.fileHandleWithStandardOutput.writeData(data)',
  ].join('; ')
  try {
    const raw = execFileSync('osascript', ['-l', 'JavaScript', '-e', script], { encoding: 'utf8' })
    return parseClipboardSnapshot(raw)
  } catch {
    return { text: '', filePaths: [], hasImage: false, changeCount: 0 }
  }
}

function readWindowsClipboardSnapshot(): ClipboardSnapshot {
  const script =
    '$ErrorActionPreference="Stop"; Add-Type -AssemblyName System.Windows.Forms; $text=if([System.Windows.Forms.Clipboard]::ContainsText()){[System.Windows.Forms.Clipboard]::GetText()}else{""}; $paths=@(); if([System.Windows.Forms.Clipboard]::ContainsFileDropList()){$paths=@([System.Windows.Forms.Clipboard]::GetFileDropList())}; $hasImage=[System.Windows.Forms.Clipboard]::ContainsImage(); [Console]::Write(([PSCustomObject]@{text=$text;filePaths=$paths;hasImage=$hasImage}|ConvertTo-Json -Compress))'
  try {
    const raw = execFileSync(
      'powershell.exe',
      ['-NoProfile', '-NonInteractive', '-Sta', '-Command', script],
      { encoding: 'utf8', windowsHide: true },
    )
    return parseClipboardSnapshot(raw)
  } catch {
    return { text: '', filePaths: [], hasImage: false, changeCount: 0 }
  }
}

function writeWindowsClipboardFilePaths(paths: string[]): void {
  const script =
    '$ErrorActionPreference="Stop"; Add-Type -AssemblyName System.Windows.Forms; $paths=$env:TEZBAR_CLIPBOARD_FILES|ConvertFrom-Json; $files=New-Object System.Collections.Specialized.StringCollection; foreach($path in @($paths)){[void]$files.Add([string]$path)}; [System.Windows.Forms.Clipboard]::SetFileDropList($files)'
  void execFileAsync(
    'powershell.exe',
    ['-NoProfile', '-NonInteractive', '-Sta', '-Command', script],
    {
      windowsHide: true,
      env: { ...process.env, TEZBAR_CLIPBOARD_FILES: JSON.stringify(paths) },
    },
  ).catch((error: unknown) => {
    console.error('[desktop-runtime] clipboard file copy failed:', error)
  })
}

function readClipboardSnapshot(): ClipboardSnapshot {
  if (process.platform === 'darwin') return readMacClipboardSnapshot()
  if (process.platform === 'win32') return readWindowsClipboardSnapshot()
  try {
    return { text: execFileSync('pbpaste', [], { encoding: 'utf8' }), filePaths: [], hasImage: false, changeCount: 0 }
  } catch {
    return { text: '', filePaths: [], hasImage: false, changeCount: 0 }
  }
}

export const clipboard = {
  readSnapshot(): ClipboardSnapshot {
    return readClipboardSnapshot()
  },
  readFilePaths(): string[] {
    return readClipboardSnapshot().filePaths
  },
  readText(): string {
    try {
      if (process.platform === 'win32') {
        return execFileSync(
          'powershell.exe',
          ['-NoProfile', '-NonInteractive', '-Command', 'Get-Clipboard -Raw'],
          { encoding: 'utf8', windowsHide: true }
        )
      }
      return execFileSync('pbpaste', [], { encoding: 'utf8' })
    } catch {
      return ''
    }
  },
  writeText(text: string): void {
    try {
      if (process.platform === 'win32') {
        const child = spawn('powershell.exe', [
          '-NoProfile',
          '-NonInteractive',
          '-Command',
          'Set-Clipboard -Value ([Console]::In.ReadToEnd())',
        ], { windowsHide: true })
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
    const path = join(app.getPath('temp'), 'tezbar-clipboard-current.png')
    try {
      rmSync(path, { force: true })
      if (process.platform === 'darwin') {
        execFileSync('osascript', ['-e', clipboardImageAppleScript(path)], { stdio: 'ignore' })
      } else if (process.platform === 'win32') {
        const script =
          '$ErrorActionPreference="Stop"; Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; if(![System.Windows.Forms.Clipboard]::ContainsImage()){throw "Clipboard does not contain an image"}; $image=[System.Windows.Forms.Clipboard]::GetImage(); try{$image.Save($env:TEZBAR_CLIPBOARD_IMAGE,[System.Drawing.Imaging.ImageFormat]::Png)}finally{$image.Dispose()}'
        execFileSync(
          'powershell.exe',
          ['-NoProfile', '-NonInteractive', '-Sta', '-Command', script],
          {
            encoding: 'utf8',
            windowsHide: true,
            env: { ...process.env, TEZBAR_CLIPBOARD_IMAGE: path },
          },
        )
      } else {
        return makeNativeImage()
      }
      return makeNativeImage(path)
    } catch {
      return makeNativeImage()
    }
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
  write(payload: { text?: string; html?: string; bookmark?: string; filePaths?: string[] }): void {
    const filePaths = payload.filePaths?.filter(Boolean) ?? []
    if (filePaths.length > 0) {
      if (process.platform === 'darwin') {
        const script = fileClipboardJavaScript(filePaths)
        if (script) runDetached('osascript', ['-l', 'JavaScript', '-e', script], 'clipboard file copy')
        return
      }
      if (process.platform === 'win32') {
        writeWindowsClipboardFilePaths(filePaths)
        return
      }
    }
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
    const defaultIndex = Math.min(Math.max(Number(options.defaultId) || 0, 0), buttons.length - 1)
    const cancelIndex = Math.min(Math.max(Number(options.cancelId) || 0, 0), buttons.length - 1)
    const message = [options.message, options.detail].filter(Boolean).join('\n\n')

    if (process.platform === 'win32') {
      const script = String.raw`Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; $buttons=$env:TEZBAR_DIALOG_BUTTONS | ConvertFrom-Json; $form=New-Object System.Windows.Forms.Form; $form.Text=$env:TEZBAR_DIALOG_TITLE; $form.StartPosition='CenterScreen'; $form.FormBorderStyle='FixedDialog'; $form.MaximizeBox=$false; $form.MinimizeBox=$false; $form.ShowInTaskbar=$true; $form.Width=540; $form.Height=300; $text=New-Object System.Windows.Forms.TextBox; $text.Multiline=$true; $text.ReadOnly=$true; $text.BorderStyle='None'; $text.BackColor=$form.BackColor; $text.Text=$env:TEZBAR_DIALOG_MESSAGE; $text.Left=24; $text.Top=22; $text.Width=476; $text.Height=180; $text.ScrollBars='Vertical'; $form.Controls.Add($text); $x=500; for($i=$buttons.Count-1;$i-ge 0;$i--){ $button=New-Object System.Windows.Forms.Button; $button.Text=[string]$buttons[$i]; $button.Tag=$i; $button.Width=110; $button.Height=32; $x-=120; $button.Left=$x; $button.Top=216; $button.Add_Click({$form.Tag=[int]$this.Tag; $form.Close()}); $form.Controls.Add($button); if($i-eq [int]$env:TEZBAR_DIALOG_DEFAULT){$form.AcceptButton=$button}; if($i-eq [int]$env:TEZBAR_DIALOG_CANCEL){$form.CancelButton=$button} }; $form.Tag=[int]$env:TEZBAR_DIALOG_CANCEL; $form.Add_Shown({$form.Activate()}); [void]$form.ShowDialog(); [Console]::Out.Write([string]$form.Tag)`
      process.stdout.write(`${JSON.stringify({ type: 'window_suppress_blur', value: true })}\n`)
      try {
        const { stdout } = await execFileAsync(
          'powershell.exe',
          [
            '-NoLogo',
            '-NoProfile',
            '-NonInteractive',
            '-ExecutionPolicy',
            'Bypass',
            '-Command',
            script,
          ],
          {
            windowsHide: true,
            env: {
              ...process.env,
              TEZBAR_DIALOG_BUTTONS: JSON.stringify(buttons),
              TEZBAR_DIALOG_DEFAULT: String(defaultIndex),
              TEZBAR_DIALOG_CANCEL: String(cancelIndex),
              TEZBAR_DIALOG_TITLE: String(options.title ?? 'Tezbar'),
              TEZBAR_DIALOG_MESSAGE: message,
            },
          }
        )
        const selected = Number.parseInt(stdout.trim(), 10)
        return {
          response:
            Number.isInteger(selected) && selected >= 0 && selected < buttons.length
              ? selected
              : cancelIndex,
        }
      } catch {
        return { response: cancelIndex }
      } finally {
        process.stdout.write(`${JSON.stringify({ type: 'window_suppress_blur', value: false })}\n`)
      }
    }

    if (process.platform !== 'darwin') return { response: cancelIndex }

    const escapeAppleScript = (value: unknown): string =>
      String(value ?? '')
        .replace(/\\/g, '\\\\')
        .replace(/"/g, '\\"')
    const buttonList = buttons.map((button) => `"${escapeAppleScript(button)}"`).join(', ')
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
