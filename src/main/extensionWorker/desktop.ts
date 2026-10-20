import { existsSync, readFileSync } from 'node:fs'
import { homedir, tmpdir } from 'node:os'
import { join } from 'node:path'
import { notify, request, send } from './host'
import type { MessageBoxOptions, NativeImage } from '../desktop-runtime'

export const app = {
  isPackaged: process.env.IS_TAURI === 'true',
  name: 'Tezbar',
  getPath(name: string): string {
    if (name === 'userData') return process.env.APPDATA_DIR || join(homedir(), '.tezbar')
    if (name === 'temp') return process.env.TEMP_DIR || tmpdir()
    if (name === 'home') return homedir()
    return join(homedir(), `.${name}`)
  },
  getAppPath: (): string => process.env.TEZBAR_RESOURCE_DIR || process.cwd(),
  getVersion: (): string => process.env.APP_VERSION || '0.0.0',
  getName: (): string => 'Tezbar',
  hide: (): void => notify('core:platform:visibility', false),
  show: (): void => notify('core:platform:visibility', true),
  focus: (): void => notify('core:platform:visibility', true),
}

let visible = true
const window = {
  webContents: {
    id: 1,
    send: (channel: string, payload: unknown): void => send({ type: 'event', channel, payload }),
    isDestroyed: (): boolean => false,
  },
  isDestroyed: (): boolean => false,
  isVisible: (): boolean => visible,
  hide: (): void => {
    visible = false
    app.hide()
  },
  show: (): void => {
    visible = true
    app.show()
  },
  focus: (): void => app.focus(),
}
export const BrowserWindow = {
  getAllWindows: (): Array<typeof window> => [window],
  getFocusedWindow: (): typeof window => window,
  fromWebContents: (): typeof window => window,
}

export const shell = {
  openExternal: async (target: string): Promise<void> => {
    await request('shell:open', target)
  },
  openPath: async (target: string): Promise<string> => {
    await request('core:platform:open-file', target)
    return ''
  },
  showItemInFolder: (target: string): void => notify('core:platform:reveal-file', target),
}

function image(bytes: Buffer, sourcePath?: string): NativeImage {
  const value: NativeImage = {
    sourcePath,
    isEmpty: () => bytes.length === 0,
    toPNG: () => {
      if (
        bytes.length &&
        (bytes.length < 8 ||
          !bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])))
      ) {
        throw new Error(
          'PNG encoding is required for in-memory clipboard images; use a file path for other image formats'
        )
      }
      return bytes
    },
    getSize: () =>
      bytes.length >= 24 && bytes.readUInt32BE(0) === 0x89504e47
        ? { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) }
        : { width: 0, height: 0 },
    setTemplateImage: () => {},
    resize: () => value,
  }
  return value
}

export const nativeImage = {
  createFromPath: (path: string): NativeImage =>
    image(existsSync(path) ? readFileSync(path) : Buffer.alloc(0), path),
  createFromBuffer: (bytes: Buffer): NativeImage => image(bytes),
  createFromDataURL: (url: string): NativeImage =>
    image(Buffer.from(url.split(',')[1] || '', 'base64')),
  createEmpty: (): NativeImage => image(Buffer.alloc(0)),
}

export const clipboard = {
  clear: async (): Promise<void> => {
    await request('clipboard:write', '')
  },
  readText: (): Promise<string> => request<string>('clipboard:read'),
  writeText: async (text: string): Promise<void> => {
    await request('clipboard:write', text)
  },
  writeTextAsync: async (text: string): Promise<void> => {
    await request('clipboard:write', text)
  },
  write: async (value: { text?: string; html?: string }): Promise<void> => {
    await request('core:platform:clipboard-write', value)
  },
  writeImage: async (value: NativeImage): Promise<void> => {
    if (value.sourcePath) await request('core:platform:clipboard-write', { file: value.sourcePath })
    else
      await request(
        'clipboard:write-png',
        `data:image/png;base64,${value.toPNG().toString('base64')}`
      )
  },
}

export const dialog = {
  showMessageBox: (options: MessageBoxOptions): Promise<{ response: number }> =>
    request('core:platform:confirm', options),
}
