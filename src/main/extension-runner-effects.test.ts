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
import { clearAllExtensionSessions, runExtensionCommandFromPackageJson, invokeExtensionAction, refreshExtensionSession, disposeExtensionSession } from './extension-runner'
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


it('no-view controls are best effort even when the native owner refuses them', async () => {
  const isolated=join(root,'no-view');mkdirSync(join(isolated,'src'),{recursive:true});
  writeFileSync(join(isolated,'package.json'),JSON.stringify({name:'no-view',commands:[{name:'index',title:'Index',mode:'no-view'}]}));
  writeFileSync(join(isolated,'src/index.tsx'),`import { closeMainWindow, clearSearchBar, openExtensionPreferences, showHUD } from '@raycast/api';export default async function Command(){await closeMainWindow();await clearSearchBar();await openExtensionPreferences();await showHUD('finished');}`);
  host.control.mockRejectedValue(new Error('Unknown SDK source'));
  const result=await runExtensionCommandFromPackageJson(join(isolated,'package.json'),'index',undefined,{}, {effectMode:'system'});
  expect(result.ok).toBe(true);if (!result.ok) throw new Error('command failed');
  expect(result.effects?.some(e=>e.kind==='hud' && e.message==='finished')).toBe(true);
  host.control.mockResolvedValue(undefined);
});


function backgroundMenuPackage(): string {
  const isolated=join(root,'background-menu');mkdirSync(join(isolated,'src'),{recursive:true})
  const pkg=join(isolated,'package.json')
  writeFileSync(pkg,JSON.stringify({name:'background-menu',commands:[{name:'menu',title:'Menu',mode:'view'},{name:'ordinary',title:'Ordinary',mode:'view'}]}))
  writeFileSync(join(isolated,'src/menu.tsx'),`import React,{useState} from 'react';import {MenuBarExtra} from '@raycast/api';export default function Command(){const [count,setCount]=useState(0);return <MenuBarExtra title={'Count '+count}><MenuBarExtra.Item title="Increment" onAction={()=>setCount(n=>n+1)}/></MenuBarExtra>}`)
  writeFileSync(join(isolated,'src/ordinary.tsx'),`import React from 'react';import {Detail} from '@raycast/api';export default function Command(){return <Detail markdown="Ordinary"/>}`)
  return pkg
}

it('keeps MenuBarExtra alive while ordinary foreground sessions are pruned', async () => {
  clearAllExtensionSessions()
  const pkg=backgroundMenuPackage()
  const menu=await runExtensionCommandFromPackageJson(pkg,'menu',undefined,{}, {effectMode:'system'})
  if(!menu.ok || menu.mode!=='view')throw Error('Menu fixture did not start')
  for(let i=0;i<40;i++){
    const ordinary=await runExtensionCommandFromPackageJson(pkg,'ordinary',undefined,{}, {effectMode:'system'})
    expect(ordinary.ok).toBe(true)
  }
  const reply=await invokeExtensionAction({sessionId:menu.sessionId,actionId:menu.actions[0]!.id})
  if(!reply.ok || reply.mode!=='view')throw Error('Background menu was evicted')
  expect(reply.root.type).toBe('MenuBarExtra');expect(reply.root.props.title).toBe('Count 1')
  expect(disposeExtensionSession(menu.sessionId)).toBe(true)
  expect((await refreshExtensionSession({sessionId:menu.sessionId})).ok).toBe(false)
})

it('refuses an extra background menu at capacity without evicting existing menus', async () => {
  clearAllExtensionSessions()
  const pkg=backgroundMenuPackage()
  const first=await runExtensionCommandFromPackageJson(pkg,'menu',undefined,{}, {effectMode:'system'})
  if(!first.ok || first.mode!=='view')throw Error('First menu did not start')
  for(let i=1;i<64;i++)expect((await runExtensionCommandFromPackageJson(pkg,'menu',undefined,{}, {effectMode:'system'})).ok).toBe(true)
  const overflow=await runExtensionCommandFromPackageJson(pkg,'menu',undefined,{}, {effectMode:'system'})
  expect(overflow.ok).toBe(false)
  if(!overflow.ok)expect(overflow.message).toContain('Too many background menus')
  const existing=await invokeExtensionAction({sessionId:first.sessionId,actionId:first.actions[0]!.id})
  expect(existing.ok).toBe(true)
  clearAllExtensionSessions()
// This integrates 65 separately bundled sessions; concurrent native builds can
// exceed Vitest's default five seconds without changing the capacity contract.
}, 20_000)
