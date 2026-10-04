import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import Database from 'better-sqlite3'
import { afterAll, beforeEach, describe, expect, it, vi } from 'vitest'
const scratch = mkdtempSync(join(tmpdir(), 'tezbar-contracts-'))
const showDialog = vi.fn(async () => ({ response: 0 }))
const openExternal = vi.fn(async () => {})
let nativeCalls = 0
vi.mock('@tezbar/desktop-runtime', () => ({
  app: {
    isPackaged: false,
    getPath: () => scratch,
    getAppPath: () => process.cwd(),
    hide() {},
    show() {},
  },
  BrowserWindow: class {
    static getAllWindows() {
      return []
    }
  },
  clipboard: { readText: () => 'fixture', writeText() {}, writeImage() {} },
  nativeImage: { createFromPath: () => ({ isEmpty: () => true }) },
  shell: { openExternal: (url: string) => openExternal(url), showItemInFolder() {} },
  dialog: { showMessageBox: (options: unknown) => showDialog(options) },
}))
vi.mock('node:child_process', async (original) => ({
  ...(await original<typeof import('node:child_process')>()),
  execFile: (...args: unknown[]) => {
    nativeCalls++
    const callback = args.at(-1)
    if (typeof callback === 'function')
      callback(null, { stdout: '/blocked-selected-file\n', stderr: '' })
  },
}))
vi.mock('./llm/extensionAI', () => ({ askExtensionAI: async () => 'fixture' }))
import {
  clearAllExtensionSessions,
  invokeExtensionAction,
  refreshExtensionSession,
  runExtensionCommandFromPackageJson,
} from './extension-runner'
let number = 0
function fixture(code: string, extra: Record<string, unknown> = {}, mode = 'no-view'): string {
  const root = join(scratch, `fixture-${number++}`)
  mkdirSync(join(root, '.sc-build'), { recursive: true })
  writeFileSync(
    join(root, 'package.json'),
    JSON.stringify({
      name: 'fixture',
      platforms: ['macOS', 'Windows'],
      commands: [{ name: 'index', mode }],
      ...extra,
    })
  )
  writeFileSync(join(root, '.sc-build/index.js'), code)
  return root
}
async function run(root: string, mode: 'system' | 'record' = 'record') {
  return runExtensionCommandFromPackageJson(join(root, 'package.json'), 'index', {}, undefined, {
    effectMode: mode,
  })
}
beforeEach(() => {
  showDialog.mockReset().mockResolvedValue({ response: 0 })
  openExternal.mockClear()
  nativeCalls = 0
  clearAllExtensionSessions()
})
afterAll(() => {
  clearAllExtensionSessions()
  rmSync(scratch, { recursive: true, force: true })
})

describe('extension compatibility and safety contracts', () => {
  const confirmation = `const api = require('@raycast/api'); module.exports.default = async () => {
    const accepted = await api.confirmAlert({ title: 'Remove fixture?', message: 'Synthetic data only', primaryAction: { title: 'Remove', style: api.Alert.ActionStyle.Destructive, onAction: () => api.Clipboard.copy('primary') }, dismissAction: { title: 'Keep', onAction: () => api.Clipboard.copy('dismiss') } });
    if (accepted) await api.Clipboard.copy('accepted');
  };`
  it('record mode never auto-accepts confirmations or invokes user callbacks', async () => {
    const result = await run(fixture(confirmation))
    expect(result.ok).toBe(true)
    if (!result.ok) return
    expect(result.effects?.filter((effect) => effect.kind === 'clipboard')).toEqual([])
    expect(showDialog).not.toHaveBeenCalled()
  })
  it('system mode honours dismissal instead of silently approving destructive work', async () => {
    const result = await run(fixture(confirmation), 'system')
    expect(result.ok).toBe(true)
    expect(showDialog).toHaveBeenCalledWith(
      expect.objectContaining({ buttons: ['Keep', 'Remove'], cancelId: 0, defaultId: 0 })
    )
    if (!result.ok) return
    expect(
      result.effects?.filter((effect) => effect.kind === 'clipboard').map((effect) => effect.value)
    ).toEqual(['dismiss'])
  })
  it('system approval invokes the primary callback exactly once', async () => {
    showDialog.mockResolvedValue({ response: 1 })
    const result = await run(fixture(confirmation), 'system')
    expect(result.ok).toBe(true)
    if (!result.ok) return
    expect(
      result.effects?.filter((effect) => effect.kind === 'clipboard').map((effect) => effect.value)
    ).toEqual(['primary', 'accepted'])
  })
  it('nested launchCommand inherits record mode and returns recorded child effects', async () => {
    const root = fixture(
      `const api = require('@raycast/api'); module.exports.default = async () => api.launchCommand({ name: 'child' });`,
      {
        commands: [
          { name: 'index', mode: 'no-view' },
          { name: 'child', mode: 'no-view' },
        ],
      }
    )
    writeFileSync(
      join(root, '.sc-build/child.js'),
      `const api = require('@raycast/api'); module.exports.default = async () => api.open('https://example.invalid/fixture');`
    )
    const result = await run(root)
    expect(result.ok).toBe(true)
    expect(openExternal).not.toHaveBeenCalled()
    if (!result.ok) return
    expect(result.effects).toContainEqual({
      kind: 'open',
      value: 'https://example.invalid/fixture',
    })
  })
  it('record mode does not inspect the real Finder/Explorer selection', async () => {
    const result = await run(
      fixture(
        `const api = require('@raycast/api'); module.exports.default = async () => { await api.getSelectedFinderItems(); };`
      )
    )
    expect(result.ok).toBe(true)
    expect(nativeCalls).toBe(0)
  })
  it.each(['darwin', 'win32'])(
    'preference defaults select the %s value, not the platform map',
    async (platform) => {
      const previous = Object.getOwnPropertyDescriptor(process, 'platform')!
      Object.defineProperty(process, 'platform', { value: platform })
      try {
        const result = await run(
          fixture(
            `const api = require('@raycast/api'); module.exports.default = async () => api.Clipboard.copy(api.getPreferenceValues().path);`,
            {
              preferences: [
                {
                  name: 'path',
                  type: 'textfield',
                  default: { macOS: '/mac/fixture', Windows: 'C:\\fixture' },
                },
              ],
            }
          )
        )
        expect(result.ok).toBe(true)
        if (!result.ok) return
        expect(result.effects?.find((effect) => effect.kind === 'clipboard')?.value).toBe(
          platform === 'darwin' ? '/mac/fixture' : 'C:\\fixture'
        )
      } finally {
        Object.defineProperty(process, 'platform', previous)
      }
    }
  )
  it('executeSQL reads a fixture and rejects writes without corrupting the database', async () => {
    const root =
      fixture(`const api = require('@raycast/api'); const utils = require('@raycast/utils'); module.exports.default = async () => {
      const file = api.environment.supportPath + '/fixture.db';
      const rows = await utils.executeSQL(file, 'SELECT value FROM items');
      let blocked = false; try { await utils.executeSQL(file, 'DELETE FROM items'); } catch { blocked = true; }
      await api.Clipboard.copy(JSON.stringify({ rows, blocked }));
    };`)
    mkdirSync(join(root, '.tezbar-support'))
    const db = new Database(join(root, '.tezbar-support/fixture.db'))
    db.exec("CREATE TABLE items (value TEXT); INSERT INTO items VALUES ('fixture');")
    db.close()
    const result = await run(root)
    expect(result.ok, result.ok ? '' : result.message).toBe(true)
    if (!result.ok) return
    expect(
      JSON.parse(result.effects?.find((effect) => effect.kind === 'clipboard')?.value || '{}')
    ).toEqual({ rows: [{ value: 'fixture' }], blocked: true })
  })
  it('legacy Geohash actions render and run their copy/open callbacks', async () => {
    const root = fixture(
      `const React = require('react'); const api = require('@raycast/api'); module.exports.default = () => React.createElement(api.List, null, React.createElement(api.List.Item, { title: 'fixture', actions: React.createElement(api.ActionPanel, null,
      React.createElement(api.CopyToClipboardAction, { title: 'Copy fixture', content: 'value', onCopy: () => api.Clipboard.copy('copied') }),
      React.createElement(api.OpenInBrowserAction, { title: 'Open fixture', url: 'https://example.invalid/', onOpen: () => api.Clipboard.copy('opened') }),
      React.createElement(api.PushAction, { title: 'Push fixture', target: React.createElement(api.Detail, { markdown: 'pushed' }) })
    ) }));`,
      {},
      'view'
    )
    const result = await run(root)
    expect(result.ok).toBe(true)
    if (!result.ok || result.mode !== 'view') return
    expect(result.actions.map((action) => action.kind)).toEqual(['copy', 'open', 'push'])
    for (const [title, marker] of [
      ['Copy fixture', 'copied'],
      ['Open fixture', 'opened'],
    ]) {
      const action = result.actions.find((action) => action.title === title)!
      const invoked = await invokeExtensionAction({
        sessionId: result.sessionId,
        actionId: action.id,
      })
      expect(invoked.ok).toBe(true)
      if (invoked.ok) expect(invoked.effects).toContainEqual({ kind: 'clipboard', value: marker })
    }
    const push = await invokeExtensionAction({
      sessionId: result.sessionId,
      actionId: result.actions[2].id,
    })
    expect(push.ok && push.mode === 'view' && push.root.props?.markdown).toBe('pushed')
  })
  it('useLocalStorage reads the same backing store as LocalStorage', async () => {
    const root = fixture(
      `const React = require('react'); const api = require('@raycast/api'); const { useLocalStorage } = require('@raycast/utils'); let initialized = false; module.exports.default = async () => {
      if (!initialized) { await api.LocalStorage.setItem('shared', JSON.stringify(['fixture'])); initialized = true; }
      const { value, isLoading } = useLocalStorage('shared', []);
      return React.createElement(api.Detail, { markdown: JSON.stringify({ value, isLoading }) });
    };`,
      {},
      'view'
    )
    const result = await run(root)
    expect(result.ok).toBe(true)
    if (!result.ok || result.mode !== 'view') return
    let current = result
    await vi.waitFor(
      async () => {
        const refreshed = await refreshExtensionSession({ sessionId: result.sessionId })
        if (refreshed.ok && refreshed.mode === 'view') current = refreshed
        expect(JSON.parse(String(current.root.props?.markdown))).toEqual({
          value: ['fixture'],
          isLoading: false,
        })
      },
      { timeout: 500 }
    )
  })
  it('useForm does not persist password field values in the general cache', async () => {
    const root = fixture(
      `const React = require('react'); const api = require('@raycast/api'); const { useForm } = require('@raycast/utils'); module.exports.default = () => {
      const form = useForm({ initialValues: { password: '' } });
      if (form.values.password !== 'synthetic-sensitive-marker') form.setValue('password', 'synthetic-sensitive-marker');
      return React.createElement(api.Form, null, React.createElement(api.Form.PasswordField, { ...form.itemProps.password }));
    };`,
      {},
      'view'
    )
    const result = await run(root)
    expect(result.ok).toBe(true)
    const cache = join(root, '.tezbar-support/cache/shared.json')
    expect(existsSync(cache) ? readFileSync(cache, 'utf8') : '').not.toContain(
      'synthetic-sensitive-marker'
    )
  })
})
