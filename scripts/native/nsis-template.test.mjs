// Source-only fixtures. UNRUN; text parity is not installer acceptance.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { windowsRehearsalPlan } from './windows-rehearsal-plan.mjs'
test('locked template delta removes only bootstrap and retains all installer contracts', async () => {
  const upstream=await readFile(new URL('../../docs/native-m11/upstream/installer.nsi',import.meta.url),'utf8')
  const native=await readFile(new URL('../../packaging/windows/installer.nsi',import.meta.url),'utf8')
  const expected=upstream.replace(/Section WebView2\n[\s\S]*?(?=Section Install\n)/,'').split('\n').filter(l=>!/^!define (WEBVIEW2|INSTALLWEBVIEW2|MINIMUMWEBVIEW2)/.test(l)).join('\n')
  assert.equal(native.slice(native.indexOf('Unicode true')),expected)
  for (const key of ['UNINSTKEY','MANUPRODUCTKEY','ALLOWDOWNGRADES','RestorePreviousInstallLocation','CreateOrUpdateStartMenuShortcut','/UPDATE','/P','/R','/ARGS','nsis_tauri_utils::RunAsUser']) assert.ok(native.includes(key),key)
  assert.doesNotMatch(native,/WEBVIEW2|Section WebView2/)
})
test('rehearsal proposal contains no executable commands and names disposable context', () => {
  const plan=windowsRehearsalPlan({sandboxRoot:'C:\\m11-sandbox',snapshotId:'disposable-m11',originalVersion:'fixture-original',candidateVersion:'fixture-native'})
  assert.deepEqual(plan.executableCommands,[])
  assert.equal(plan.expected.uninstallKey,'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Tezbar')
  assert.deepEqual(plan.cases[1].expectedFragments,['/P','/R','/UPDATE','/ARGS'])
})
