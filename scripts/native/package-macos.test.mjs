// Source fixture suite only. UNRUN in the M11 preparation lane.
import test from 'node:test'
import {metadataRecord,captureBuildMetadata} from './build-metadata.mjs'
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { validateInventory, renderInfoPlist, planMacos } from './package-macos.mjs'
const pin = {source:'/fixture/source',destination:'icon.icns',bytes:1,sha256:'a'.repeat(64),kind:'data'}
test('inventories reject traversal, collisions, missing provenance and symlink-shaped names', () => {
  for (const destination of ['../escape','/Applications/Tezbar.app','a//b','a\\b','a/./b']) assert.throws(()=>validateInventory([{...pin,destination}]))
  assert.throws(()=>validateInventory([pin,{...pin,destination:'ICON.icns'}]))
  assert.throws(()=>validateInventory([{...pin,destination:'a'},{...pin,destination:'a/b'}]))
  assert.throws(()=>validateInventory([{...pin,sha256:''}]))
})
test('plist preserves production metadata and exact permission strings without LSUIElement', async () => {
  const template=await readFile(new URL('../../packaging/macos/Info.plist.in',import.meta.url),'utf8')
  const permissions=await readFile(new URL('../../crates/tezbar-core/src/system/macos-permissions.plist',import.meta.url),'utf8')
  const plist=renderInfoPlist(template,permissions,{identifier:'com.tezbar.preview.native',version:'0.0.1-alpha.1'})
  assert.match(plist,/<string>tezbar<\/string>/);assert.match(plist,/<string>10.13<\/string>/)
  assert.match(plist,/<key>NSHighResolutionCapable<\/key>/);assert.doesNotMatch(plist,/LSUIElement/)
  assert.match(plist,/Tezbar reads and creates calendar events/)
})
test('planning refuses installed app paths and invented identities before any I/O', () => {
  assert.throws(()=>planMacos({version:'0.0.1-alpha.1',channel:'alpha',stage:'/Applications',osTemp:'/fixture/tmp'}))
})

test('actual disposable unsealed assembly copies pinned bytes and fails closed on changed resources',async()=>{
  const {mkdtemp,mkdir,writeFile,rm,realpath}=await import('node:fs/promises')
  const {tmpdir}=await import('node:os');const path=await import('node:path');const {createHash}=await import('node:crypto')
  const {inventoryResources,assembleMacos,RESOURCE_ROOTS,GPUI_REV}=await import('./package-macos.mjs')
  const root=await mkdtemp(path.join(await realpath(tmpdir()),'tezbar-m11-package-'));const source=path.join(root,'source');await mkdir(source)
  try {
    const policy=JSON.parse(await readFile(new URL('../../packaging/macos-signing.json',import.meta.url)))
    const native=Buffer.from('cffaedfe01000000','hex'), payload=Buffer.from('private dummy runtime, never executed')
    for(const [from]of RESOURCE_ROOTS){const file=path.join(source,from,/node_modules$|resources$|icons$|@esbuild$|esbuild$/.test(from)?'fixture':'');await mkdir(path.dirname(file),{recursive:true});await writeFile(file,policy.nativeHelpers.includes(from)?native:from==='build/embedding-runtime/resources'?payload:Buffer.from('fixture'))}
    // Runtime ZIP and executable use explicit private dummy bytes.
    await writeFile(path.join(source,'build/embedding-runtime/resources/embedding-runtime.zip'),payload)
    const metadata={schema:2,version:'0.0.1-alpha.1',sourceCommit:'a'.repeat(40),dirty:true,benchControl:false,runtimeShaders:true,gpuiRev:GPUI_REV,rustVersion:'1.98.1'}
    await writeFile(path.join(source,'binary'),Buffer.concat([native,metadataRecord(metadata)]))
    const files=await inventoryResources(source);const [executable]=await inventoryResources(source,[['binary','tezbar']])
    const catalog={archiveBytes:payload.length,archiveSha256:createHash('sha256').update(payload).digest('hex'),distribution:'local-ad-hoc'}
    const input={version:'0.0.1-alpha.1',channel:'alpha',localPreview:true,stage:path.join(root,'stage'),osTemp:await realpath(tmpdir()),executable,files,policy,catalog,payload:{bytes:catalog.archiveBytes,sha256:catalog.archiveSha256},sourceCommit:'a'.repeat(40),sourceDirty:true,gpuiRev:GPUI_REV,rustVersion:'1.98.1',benchControl:false,runtimeShaders:true}
    const plan=planMacos(input);const template=await readFile(new URL('../../packaging/macos/Info.plist.in',import.meta.url),'utf8'),permissions=await readFile(new URL('../../crates/tezbar-core/src/system/macos-permissions.plist',import.meta.url),'utf8')
    const receipt=await assembleMacos(plan,{template,permissions,osTemp:await realpath(tmpdir())})
    assert.equal(receipt.state,'ASSEMBLED_UNSEALED');assert.equal(receipt.publishable,false);assert.equal(receipt.sourceDirty,true)
    assert.equal(await readFile(path.join(plan.bundle,'Contents/Resources/.tezbar-native-source'),'utf8'),'a'.repeat(40)+'\n')
    const manifest=JSON.parse(await readFile(path.join(plan.bundle,'Contents/Resources/.tezbar-native-production.json')))
    assert.equal(manifest.schema,2);assert.equal(manifest.dirty,true)
    assert.equal(manifest.binaryMetadataSha256,(await captureBuildMetadata(path.join(plan.bundle,'Contents/MacOS/tezbar'))).sha256)
    await assert.rejects(assembleMacos(planMacos({...input,sourceCommit:'b'.repeat(40),stage:path.join(root,'wrong-binary-stage')}),{template,permissions,osTemp:await realpath(tmpdir())}),/metadata differs/)
    // Compiled-Metal production is valid; the actual binary mode must match the plan.
    await writeFile(path.join(source,'binary-metal'),Buffer.concat([native,metadataRecord({...metadata,runtimeShaders:false})]))
    const [metal]=await inventoryResources(source,[['binary-metal','tezbar']])
    const metalPlan=planMacos({...input,executable:metal,runtimeShaders:false,stage:path.join(root,'metal-stage')})
    await assembleMacos(metalPlan,{template,permissions,osTemp:await realpath(tmpdir())})
    const metalManifest=JSON.parse(await readFile(path.join(metalPlan.bundle,'Contents/Resources/.tezbar-native-production.json')))
    assert.equal(metalManifest.runtimeShaders,false)
    await assert.rejects(assembleMacos(planMacos({...input,executable:metal,runtimeShaders:true,stage:path.join(root,'wrong-shader-stage')}),{template,permissions,osTemp:await realpath(tmpdir())}),/metadata differs/)
    await writeFile(files[0].source,'changed source')
    await assert.rejects(assembleMacos(planMacos({...input,stage:path.join(root,'tampered-stage')}),{template,permissions,osTemp:await realpath(tmpdir())}),/Copied bytes/)
  } finally {await rm(root,{recursive:true,force:true})}
})
