import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterAll, expect, it, vi } from 'vitest'

const host = vi.hoisted(() => ({ copy: vi.fn(), open: vi.fn(), reveal: vi.fn(), control: vi.fn(async () => undefined) }))
const root = mkdtempSync(join(tmpdir(), 'tezbar-m03b-effects-'))
vi.mock('@tezbar/desktop-runtime', () => ({
  app: { isPackaged: false, getPath: () => root, getAppPath: () => process.cwd(), hide() {}, show() {} },
  BrowserWindow: class { static getAllWindows(): never[] { return [] } },
  clipboard: { readText: () => '', writeText: host.copy },
  extensionControl: host.control,
  nativeImage: { createFromPath: () => ({ isEmpty: () => true }) },
  shell: { openExternal: host.open, showItemInFolder: host.reveal },
}))
vi.mock('./extensionWorker/windowState', () => ({ setSuppressBlurHide: vi.fn() }))
import { clearAllExtensionSessions, runExtensionCommandFromPackageJson, invokeExtensionAction, refreshExtensionSession } from './extension-runner'
afterAll(() => { clearAllExtensionSessions(); rmSync(root, { recursive: true, force: true }) })

it('sequences sliding execution records without replaying system effects on rerender or polls', async () => {
  mkdirSync(join(root, 'src'))
  writeFileSync(join(root, 'package.json'), JSON.stringify({ name: 'effect-fixture', title: 'Effects', commands: [{ name: 'index', title: 'Index', mode: 'view' }] }))
  writeFileSync(join(root, 'src/index.tsx'), `import React from 'react'; import { Detail, Action, ActionPanel, Clipboard, open, showInFinder, showHUD } from '@raycast/api';
    export default function Command() { return <Detail markdown="Fixture" actions={<ActionPanel><Action title="Run" onAction={async () => {
      await Clipboard.copy('x'.repeat(10000)); await open('https://example.test'); await showInFinder('/fixture/file'); await showHUD('identical');
    }} /></ActionPanel>} /> }`)
  const initial = await runExtensionCommandFromPackageJson(join(root, 'package.json'), 'index', undefined, {}, { effectMode: 'system' })
  expect(initial.ok).toBe(true)
  if (!initial.ok || initial.mode !== 'view') throw new Error('Expected fixture view')
  expect(initial.effectSequence).toBe(0)
  for (let i = 1; i <= 60; i++) {
    const reply = await invokeExtensionAction({ sessionId: initial.sessionId, actionId: initial.actions[0]!.id })
    if (!reply.ok || reply.mode !== 'view') throw new Error('Expected action view')
    expect(reply.effectSequence).toBe(i * 4)
    expect(reply.effects).toHaveLength(Math.min(50, i * 4))
    expect(reply.effects?.at(-1)).toEqual({ kind: 'hud', message: 'identical' })
    const poll = await refreshExtensionSession({ sessionId: initial.sessionId })
    expect(poll).toEqual({ ok: true, mode: 'unchanged' })
    expect(host.copy).toHaveBeenCalledTimes(i)
    expect(host.open).toHaveBeenCalledTimes(i)
    expect(host.reveal).toHaveBeenCalledTimes(i)
    expect(host.control).toHaveBeenCalledTimes(i)
    expect(host.control).toHaveBeenLastCalledWith(initial.sessionId, 'hud', 'identical', 0)
  }
  expect(host.copy).toHaveBeenLastCalledWith('x'.repeat(10000))
})
