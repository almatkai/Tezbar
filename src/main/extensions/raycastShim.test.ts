import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

/** We can't import the real `electron` module in a vitest (node) env — it
 *  only exposes the main/renderer APIs when loaded from an Electron
 *  process. Stub the surfaces the shim touches with plain objects so the
 *  shim's runtime behavior (not the Electron bindings) is what we test. */
const clipboardState = { text: '' }
vi.mock('@tezbar/desktop-runtime', () => ({
  app: {
    isPackaged: false,
  },
  clipboard: {
    writeText: (value: string): void => {
      clipboardState.text = value
    },
    readText: (): string => clipboardState.text,
    clear: (): void => {
      clipboardState.text = ''
    },
  },
  shell: {
    openExternal: vi.fn(async () => {}),
    openPath: vi.fn(async () => ''),
    showItemInFolder: vi.fn(() => {}),
  },
  nativeImage: {
    createFromBuffer: (buffer: Buffer): Buffer => buffer,
  },
}))

import {
  createRaycastApi,
  createRaycastUtils,
  formatRuntimeFeedback,
  type RuntimeFeedback,
  type ShimContext,
} from './raycastShim'

function makeCtx(): ShimContext {
  const packageRoot = mkdtempSync(join(tmpdir(), 'raycast-shim-test-'))
  return {
    extensionId: 'raycast.demo',
    commandName: 'demo',
    packageRoot,
    feedback: [],
  }
}

describe('raycast shim API surface', () => {
  let ctx: ShimContext

  beforeEach(() => {
    ctx = makeCtx()
  })

  afterEach(() => {
    rmSync(ctx.packageRoot, { recursive: true, force: true })
  })

  it('exposes the major Raycast constants extensions import at module scope', () => {
    const api = createRaycastApi(ctx)
    for (const key of [
      'Toast',
      'Icon',
      'Color',
      'Image',
      'List',
      'Form',
      'Detail',
      'Grid',
      'Action',
      'ActionPanel',
      'MenuBarExtra',
      'Alert',
      'Keyboard',
      'OAuth',
      'BrowserExtension',
      'AI',
      'environment',
      'LocalStorage',
      'Cache',
      'Clipboard',
      'getPreferenceValues',
      'showToast',
      'showHUD',
      'open',
      'openExtensionPreferences',
      'closeMainWindow',
      'popToRoot',
      'confirmAlert',
      'useNavigation',
    ]) {
      expect(api[key]).toBeDefined()
    }
  })

  it('render-tree proxies are callable without throwing', () => {
    const api = createRaycastApi(ctx) as Record<string, any>
    expect(() => api.List.Item()).not.toThrow()
    expect(() => api.Action.Submit()).not.toThrow()
    expect(() => api.Form.TextField()).not.toThrow()
    expect(api.Image.Mask.Circle).toBe('circle')
    expect(api.ImageMask.RoundedRectangle).toBe('roundedRectangle')
  })

  it('LocalStorage persists between reads inside a single run', async () => {
    const api = createRaycastApi(ctx) as Record<string, any>
    await api.LocalStorage.setItem('note', 'hello')
    const value = await api.LocalStorage.getItem('note')
    expect(value).toBe('hello')
    await api.LocalStorage.removeItem('note')
    expect(await api.LocalStorage.getItem('note')).toBeUndefined()
  })

  it('Clipboard.copy/readText round-trips through the Electron stub', async () => {
    const api = createRaycastApi(ctx) as Record<string, any>
    await api.Clipboard.copy('alpha')
    expect(await api.Clipboard.readText()).toBe('alpha')
    await api.Clipboard.copy({ text: 'beta' })
    expect(await api.Clipboard.readText()).toBe('beta')
  })

  it('showToast and showHUD append to the feedback collector', () => {
    const api = createRaycastApi(ctx) as Record<string, any>
    api.showToast({ title: 'Saved', style: 'success' })
    api.showHUD('All good')
    expect(ctx.feedback).toHaveLength(2)
    expect(ctx.feedback[0].kind).toBe('toast')
    expect(ctx.feedback[1].kind).toBe('hud')
  })

  it('getPreferenceValues returns an empty object when no preferences file exists', () => {
    const api = createRaycastApi(ctx) as Record<string, any>
    expect(api.getPreferenceValues()).toEqual({})
  })

  it('raycast-utils hooks return synchronous defaults', () => {
    const utils = createRaycastUtils(ctx) as Record<string, any>
    const [state, setState] = utils.useCachedState('k', 0)
    expect(state).toBe(0)
    expect(typeof setState).toBe('function')
    expect(utils.FormValidation.Required()).toBeUndefined()
    expect(utils.useFetch().isLoading).toBe(false)
  })

  it('formatRuntimeFeedback falls back to a default string', () => {
    expect(formatRuntimeFeedback({ kind: 'hud' } as RuntimeFeedback)).toBe(
      'Extension command completed.',
    )
    expect(
      formatRuntimeFeedback({ kind: 'toast', title: 'Saved', message: 'Done' } as RuntimeFeedback),
    ).toBe('Saved: Done')
  })
})
