// Tooling/fixture tests only: no Cargo builds, platform signing, secrets, installer or update acceptance.
import test from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, mkdir, writeFile, readFile, rm, realpath, appendFile, readdir } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { createHash, generateKeyPairSync, sign } from 'node:crypto'
import { fileURLToPath } from 'node:url'
import { execFileSync } from 'node:child_process'
import { parseRelease, platformPlan, cargoPlan, assertMetadata, prepareRelease, nativeRelease, WINDOWS_ROOTS, renderWindowsInstaller } from './release.mjs'
import { RESOURCE_ROOTS, inventoryResources } from './package-macos.mjs'
import { metadataRecord, GPUI_REV, RUST_VERSION } from './build-metadata.mjs'

const ROOT=fileURLToPath(new URL('../../',import.meta.url))
const sha=bytes=>createHash('sha256').update(bytes).digest('hex')
const native=Buffer.from('cffaedfe01000000','hex')
const source={sourceCommit:'a'.repeat(40),sourceDirty:false,sourceDigest:'b'.repeat(64)}
const metadata={schema:2,version:'0.0.1-beta.1',sourceCommit:source.sourceCommit,dirty:false,benchControl:false,runtimeShaders:false,gpuiRev:GPUI_REV,rustVersion:RUST_VERSION}
const keyPair=generateKeyPairSync('ed25519'), keyId=Buffer.from('12345678')
const publicKey=Buffer.from(`untrusted comment: fixture\n${Buffer.concat([Buffer.from('Ed'),keyId,keyPair.publicKey.export({type:'spki',format:'der'}).subarray(-32)]).toString('base64')}`).toString('base64')
function signature(bytes) {
  const dataSignature=sign(null,bytes,keyPair.privateKey), packet=Buffer.concat([Buffer.from('Ed'),keyId,dataSignature]), comment='fixture'
  const global=sign(null,Buffer.concat([dataSignature,Buffer.from(comment)]),keyPair.privateKey)
  return Buffer.from(`untrusted comment: fixture\n${packet.toString('base64')}\ntrusted comment: ${comment}\n${global.toString('base64')}`).toString('base64')
}
async function fixture(t,{platform='darwin'}={}) {
  const temp=await mkdtemp(path.join(await realpath(tmpdir()),'tezbar-release-test-')),root=path.join(temp,'source'),resources=path.join(temp,'reviewed'),output=path.join(temp,'output')
  await mkdir(root);await mkdir(resources)
  t.after(()=>rm(temp,{recursive:true,force:true}))
  const put=async (base,name,bytes)=>{const file=path.join(base,name);await mkdir(path.dirname(file),{recursive:true});await writeFile(file,bytes);return file}
  const policy=JSON.parse(await readFile(path.join(ROOT,'packaging/macos-signing.json')))
  const base=JSON.parse(await readFile(path.join(ROOT,'packaging/legacy-app-policy.json')))
  base.version=metadata.version;base.plugins.updater.pubkey=publicKey
  await put(root,'package.json',JSON.stringify({version:metadata.version}))
  await put(root,'apps/tezbar/Cargo.toml',`[package]\nname = "tezbar-native"\nversion = "${metadata.version}"\n`)
  await put(root,'packaging/legacy-app-policy.json',JSON.stringify(base))
  await put(root,'packaging/macos-signing.json',JSON.stringify(policy))
  await put(root,`docs/releases/${metadata.version}.md`,'Fixture release notes, not a release')
  for(const file of ['packaging/macos/Info.plist.in','crates/tezbar-core/src/system/macos-permissions.plist','packaging/windows/installer.nsi','packaging/windows/utils.nsh','packaging/windows/FileAssociation.nsh']) await put(root,file,await readFile(path.join(ROOT,file)))
  const roots=platform==='darwin'?RESOURCE_ROOTS:WINDOWS_ROOTS
  for (const [from,to] of roots) {
    const directory=/node_modules$|resources$|icons$|@esbuild$|esbuild$/.test(from)
    await put(resources,path.join(from,directory?'fixture':''),policy.nativeHelpers.includes(to)||to==='dist-backend/libsqlite3.dylib'||to==='dist-backend/vec0'?(platform==='darwin'?native:Buffer.from('MZfixture resource')):Buffer.from('fixture resource'))
  }
  const payload=Buffer.from('fixture archive, never run')
  if(platform==='darwin') await put(resources,'build/embedding-runtime/resources/embedding-runtime.zip',payload)
  await put(root,'packaging/runtime/embedding-macos-arm64.json',JSON.stringify({archiveBytes:payload.length,archiveSha256:sha(payload),distribution:'local-ad-hoc'}))
  const binary=await put(temp,'cargo-artifact',Buffer.concat([native,metadataRecord(metadata)])),calls=[]
  const args=['--channel','beta','--version',metadata.version,'--allow-unsigned','--resources',resources,'--output',output]
  const stages=new Set()
  t.after(async()=>{for(const stage of stages)await rm(stage,{recursive:true,force:true})})
  const env={HOME:path.join(temp,'no-keys'),TAURI_SIGNING_PRIVATE_KEY:'fixture-adapter-only'}
  const run=async (command,argv,options)=>{
    calls.push({command,argv,options})
    if(command==='codesign' && argv.at(-1).endsWith('.app')) stages.add(path.dirname(path.dirname(argv.at(-1))))
    if(command==='security') return `1) ${'C'.repeat(40)} "${base.bundle.macOS.signingIdentity}"\n1 valid identities found`
    if(command==='hdiutil') await writeFile(argv.at(-1),'fixture dmg')
    if(command==='tar') await writeFile(argv[argv.indexOf('-czf')+1],'fixture updater archive')
    if(argv.includes('signer')) await writeFile(argv.at(-1)+'.sig',signature(await readFile(argv.at(-1))))
    return ''
  }
  const verifyMac=async (app,pin)=>{
    calls.push({command:'verifyMac',argv:[app]})
    assert.equal(pin.designatedRequirement,policy.designatedRequirement)
    return {identifier:pin.identifier,teamIdentifier:pin.teamIdentifier,designatedRequirement:pin.designatedRequirement,identityContinuityVerified:true,nativeResources:pin.nativeHelpers.map(p=>({path:p,teamIdentifier:pin.teamIdentifier}))}
  }
  let app
  const adapters={root,platform,arch:platform==='darwin'?'arm64':'x64',env,snapshot:()=>source,run,log:()=>{},cargo:async (command,argv,options)=>{calls.push({command,argv,options});return binary},verifyMac,signResources:async directory=>{app=path.dirname(path.dirname(directory));stages.add(path.dirname(path.dirname(app)));for(const file of (await inventoryResources(directory,roots.map(([,to])=>[to,to]))).filter(f=>f.kind==='mach-o'))await appendFile(file.source,'signed-resource')}}
  return {temp,root,resources,output,binary,args,adapters,calls,put,policy,base,stages,get app(){return app}}
}

test('strict release parsing preserves channel/version and refuses ambiguous/legacy switches',()=>{
  assert.throws(()=>parseRelease([]),/--resources/)
  for(const args of [['--channel','stable','--allow-unsigned'],['--channel','beta','--local-preview','--allow-unsigned'],['--app','tauri'],['--features','bench-control'],['--channel','beta','--channel','alpha'],['--output','--dry-run']]) assert.throws(()=>parseRelease([...args,'--resources','/fixture','--output','/out']))
  assert.equal(parseRelease(['--app','native','--channel','beta','--resources','/fixture','--output','/out','--allow-unsigned']).allowUnsigned,true)
})

test('release and Windows help work without resources, builds or signing on any host', async()=>{
  const fail=()=>assert.fail('help must not inspect/build/sign')
  assert.deepEqual(await nativeRelease(['--help'],{root:'/absent',snapshot:fail,run:fail,cargo:fail,log:()=>{}}),{state:'HELP'})
  for (const wrapper of ['scripts/build-release.mjs','scripts/build-windows.mjs']) {
    assert.match(execFileSync(process.execPath,[wrapper,'--help'],{cwd:ROOT,encoding:'utf8'}),/native GPUI only/)
  }
  await assert.rejects(nativeRelease(['--help','--dry-run'],{snapshot:fail,run:fail,cargo:fail,log:()=>{}}),/Unknown/)
})

test('explicit Cargo GPUI artifact plan is locked/offline/production-only; platforms fail closed',()=>{
  assert.equal(platformPlan('win32','x64').target,'x86_64-pc-windows-msvc')
  assert.equal(platformPlan('darwin','arm64').platform,'darwin-aarch64')
  for(const pair of [['linux','x64'],['darwin','x64'],['win32','arm64']])assert.throws(()=>platformPlan(...pair))
  const args=cargoPlan('aarch64-apple-darwin')
  for(const arg of ['--locked','--offline','--release','tezbar-native','--message-format=json'])assert.ok(args.includes(arg))
  assert.ok(!args.includes('--features'));assert.ok(!args.join(' ').includes('tauri'))
  for(const altered of [{schema:1},{sourceCommit:'c'.repeat(40)},{version:'1.0.0'},{dirty:true},{benchControl:true},{runtimeShaders:true},{gpuiRev:'f'.repeat(40)},{rustVersion:'unknown'}])assert.throws(()=>assertMetadata({...metadata,...altered},{version:metadata.version,...source}))
})

test('dry-run reads reviewed files but writes nothing and runs no commands, even on dirty source',async t=>{
  const f=await fixture(t)
  const plan=await nativeRelease([...f.args,'--dry-run'],{...f.adapters,snapshot:()=>({...source,sourceDirty:true}),cargo:()=>assert.fail('must not build'),run:()=>assert.fail('must not inspect certificates or discover keys')})
  assert.equal(plan.state,'NOT_BUILT');assert.equal(plan.sourceDirty,true);assert.equal(plan.distribution,'testing-preview')
  assert.equal(existsSync(f.output),false);assert.equal(f.calls.length,0)
})

test('preflight rejects mismatched committed version, missing resources, changed runtime pin and missing certificate pin',async t=>{
  const f=await fixture(t)
  await assert.rejects(prepareRelease(parseRelease(f.args.map(v=>v===metadata.version?'0.0.2-beta.1':v)),f.adapters),/version/)
  await rm(path.join(f.resources,'dist-backend/extension-worker.js'))
  await assert.rejects(prepareRelease(parseRelease(f.args),f.adapters),/ENOENT/)
  await f.put(f.resources,'dist-backend/extension-worker.js','fixture')
  await f.put(f.resources,'build/embedding-runtime/resources/embedding-runtime.zip','tampered')
  await assert.rejects(prepareRelease(parseRelease(f.args),f.adapters),/payload mismatch/)
  const catalog=JSON.parse(await readFile(path.join(f.root,'packaging/runtime/embedding-macos-arm64.json')))
  catalog.archiveSha256=sha(Buffer.from('tampered'));catalog.archiveBytes=8
  await f.put(f.root,'packaging/runtime/embedding-macos-arm64.json',JSON.stringify(catalog))
  await f.put(f.root,'packaging/macos-signing.json',JSON.stringify({...f.policy,teamIdentifier:''}))
  await assert.rejects(prepareRelease(parseRelease(f.args),f.adapters),/signing policy/)
  assert.equal(f.calls.length,0)
})

test('public prerelease builds reject dirty/untracked source and absent pinned certificate before Cargo/output',async t=>{
  const f=await fixture(t)
  await assert.rejects(nativeRelease(f.args,{...f.adapters,snapshot:()=>({...source,sourceDirty:true})}),/clean source/)
  await assert.rejects(nativeRelease(f.args,{...f.adapters,run:async()=>''}),/certificate is unavailable/)
  await assert.rejects(nativeRelease(f.args,{...f.adapters,env:{...f.adapters.env,APPLE_SIGNING_IDENTITY:'-'}}),/ad-hoc/)
  assert.equal(existsSync(f.output),false)
  assert.equal(f.calls.some(c=>c.command==='cargo'),false)
})

test('controlled native assembly signs copied helpers, recaptures hashes, verifies updater and writes truthful testing receipt',async t=>{
  const f=await fixture(t)
  const receipt=await nativeRelease(f.args,f.adapters)
  const stage=path.dirname(path.dirname(f.app));t.after(()=>rm(stage,{recursive:true,force:true}))
  assert.equal(receipt.distribution,'testing-preview');assert.equal(receipt.platformVerified,false)
  assert.equal(receipt.macOSSigning.identityContinuityVerified,true);assert.equal(receipt.binaryMetadata.schema,2)
  assert.equal(receipt.sourceDirty,false);assert.equal(receipt.artifacts.length,2)
  assert.notEqual(receipt.inventorySha256,receipt.packagedInventorySha256)
  const manifest=JSON.parse(await readFile(path.join(f.app,'Contents/Resources/.tezbar-native-production.json')))
  const helper=manifest.files.find(p=>p.destination===f.policy.nativeHelpers[0])
  assert.equal(helper.sha256,sha(await readFile(path.join(f.app,'Contents/Resources',helper.destination))))
  assert.equal((await readFile(path.join(f.resources,f.policy.nativeHelpers[0]))).equals(native),true,'reviewed input never signed or changed')
  const persisted=JSON.parse(await readFile(path.join(f.output,'receipt-darwin-aarch64.json')))
  assert.deepEqual(receipt,persisted)
  const cargo=f.calls.find(c=>c.command==='cargo');assert.equal(cargo.options.manifest,path.join(f.root,'apps/tezbar/Cargo.toml'))
  const outer=f.calls.find(c=>c.command==='codesign')
  assert.ok(outer.argv.includes(`=designated => ${f.policy.designatedRequirement}`))
  const admission=f.calls.find(c=>c.argv.includes('--verify-installation'))
  assert.equal(admission.command,path.join(f.app,'Contents/MacOS/tezbar'))
  assert.ok(f.calls.indexOf(admission)>f.calls.indexOf(outer))
  assert.ok(f.calls.indexOf(admission)<f.calls.findIndex(c=>c.command==='hdiutil'))
  const signer=f.calls.find(c=>c.argv.includes('signer'));assert.deepEqual(signer.argv.slice(1,3),['signer','sign'])
  assert.equal(f.calls.some(c=>c.argv.includes('build') && c.command!=='cargo'),false)
  assert.equal(f.calls.some(c=>c.command==='spctl'),false,'testing-preview is not Gatekeeper/notarization acceptance')
})

test('explicit local-preview stays nonpublishable and never discovers certificate/updater keys',async t=>{
  const f=await fixture(t)
  const receipt=await nativeRelease([...f.args.filter(arg=>arg!=='--allow-unsigned'),'--local-preview'],{...f.adapters,env:{HOME:path.join(f.temp,'nonexistent-home')},signResources:()=>assert.fail('preview must not use certificate resources'),verifyMac:()=>assert.fail('no pinned identity claim')})
  assert.equal(receipt.distribution,'local-preview');assert.equal(receipt.publishable,false)
  assert.equal(receipt.appIdentifier,'com.tezbar.preview.native');assert.equal(receipt.platformVerified,false)
  assert.equal(receipt.macOSSigning,undefined)
  assert.equal(f.calls.some(c=>c.command==='security' || c.argv.includes('signer')),false)
  assert.equal(f.calls.some(c=>c.argv.includes('--verify-installation')),false)
  assert.equal(f.calls.find(c=>c.command==='codesign').argv[2],'-')
})

test('a signed app rejected by its own startup admission cannot become a release artifact',async t=>{
  const f=await fixture(t)
  await assert.rejects(nativeRelease(f.args,{...f.adapters,run:async(command,argv,options)=>{
    if(argv.includes('--verify-installation')) throw Error('Production startup admission rejected')
    return f.adapters.run(command,argv,options)
  }}),/Production startup admission rejected/)
  assert.deepEqual(await readdir(f.output),[])
  assert.equal(f.calls.some(c=>c.command==='hdiutil' || c.argv.includes('signer')),false)
})

test('resource or metadata mutation never writes a release receipt',async t=>{
  const f=await fixture(t)
  await assert.rejects(nativeRelease(f.args,{...f.adapters,cargo:async()=>{await appendFile(path.join(f.resources,'dist-backend/extension-worker.js'),'mutation');return f.binary}}),/inventory changed/)
  assert.equal(existsSync(f.output),false)
  await f.put(f.temp,'wrong-binary',Buffer.concat([native,metadataRecord({...metadata,dirty:true})]))
  await assert.rejects(nativeRelease(f.args,{...f.adapters,cargo:async()=>path.join(f.temp,'wrong-binary')}),/schema-2 metadata/)
  assert.equal(existsSync(f.output),false)
})

test('invalid updater signatures and failed pinned identity checks cannot produce receipts',async t=>{
  const f=await fixture(t)
  await assert.rejects(nativeRelease(f.args,{...f.adapters,verifyMac:async()=>{throw Error('pinned requirement rejected')}}),/pinned requirement/)
  const tempFiles=await readdir(f.output);assert.deepEqual(tempFiles,[])
  // A second fresh output exercises the independent updater integrity gate.
  const second=path.join(f.temp,'second-output'), args=f.args.map(v=>v===f.output?second:v)
  await assert.rejects(nativeRelease(args,{...f.adapters,run:async(command,argv,opts)=>{await f.adapters.run(command,argv,opts);if(argv.includes('signer'))await writeFile(argv.at(-1)+'.sig','not a signature');return command==='security'?`1) ${'C'.repeat(40)} "${f.base.bundle.macOS.signingIdentity}"`:''}}),/signature|key|base64|Invalid/i)
  assert.equal(existsSync(path.join(second,'receipt-darwin-aarch64.json')),false)
  // All commands above were adapters; no codesign or real signer ran.
})

test('Windows planning and template rendering retain native installer contract without WebView',async t=>{
  const f=await fixture(t,{platform:'win32'})
  const plan=await nativeRelease([...f.args,'--dry-run'],f.adapters)
  assert.equal(plan.platform,'windows-x86_64-nsis');assert.match(plan.required.join(' '),/NSIS 3.11/)
  await assert.rejects(nativeRelease(f.args,f.adapters),/--nsis-tools/)
  assert.equal(f.calls.length,0)
  const template=await readFile(path.join(f.root,'packaging/windows/installer.nsi'),'utf8')
  const context=Object.fromEntries([...template.matchAll(/{{(\w+)}}/g)].map(([,key])=>[key,'']))
  Object.assign(context,{language_file:'C:\\reviewed\\English.nsh',manufacturer:'tezbar',product_name:'Tezbar',version:metadata.version,version_with_build:'0.0.1.0',main_binary_name:'tezbar',main_binary_path:'C:\\stage\\tezbar.exe',install_mode:'currentUser',allow_downgrades:'false',arch:'x64'})
  const rendered=renderWindowsInstaller(template,context,[{destination:'dist-backend/extension-worker.js',source:'C:\\stage\\extension-worker.js'},{destination:'native/color-picker/windows.ps1',source:'C:\\stage\\windows.ps1'}])
  assert.doesNotMatch(rendered,/{{|WEBVIEW2|Section WebView/)
  for(const key of ['UNINSTKEY','MANUPRODUCTKEY','RestorePreviousInstallLocation','CreateOrUpdateStartMenuShortcut','/UPDATE','/ARGS','nsis_tauri_utils::RunAsUser','tezbar.exe','extension-worker.js'])assert.ok(rendered.includes(key),key)
  assert.throws(()=>renderWindowsInstaller(template+'\n{{unknown}}',context,[]),/Unknown NSIS context/)
  assert.throws(()=>renderWindowsInstaller(template,{...context,main_binary_path:'C:\\evil$\\x.exe'},[]),/Unsafe NSIS/)
})

test('controlled Windows PE-to-NSIS flow uses native template, updater signature and testing receipt',async t=>{
  const f=await fixture(t,{platform:'win32'})
  await writeFile(f.binary,Buffer.concat([Buffer.from('MZ'),metadataRecord(metadata)]))
  let script
  const receipt=await nativeRelease(f.args,{...f.adapters,kit:async()=>({root:path.join(f.temp,'fixture-offline-kit'),inventorySha256:'fixture-kit-pin'}),run:async(command,argv,opts)=>{
    f.calls.push({command,argv,opts})
    if(command.endsWith('makensis.exe')) {
      f.stages.add(opts.cwd)
      script=await readFile(path.join(opts.cwd,'installer.nsi'),'utf8')
      const out=/!define OUTFILE "([^"]+)"/.exec(script)[1]
      await writeFile(out,'fixture NSIS binary, never executed')
      return ''
    }
    if(argv.includes('signer')) await writeFile(argv.at(-1)+'.sig',signature(await readFile(argv.at(-1))))
    return ''
  }})
  assert.equal(receipt.platform,'windows-x86_64-nsis');assert.equal(receipt.platformVerified,false)
  assert.equal(receipt.distribution,'testing-preview');assert.equal(receipt.artifacts.length,1)
  assert.match(script,/!define MAINBINARYNAME "tezbar"/)
  assert.match(script,/\.tezbar-native-production\.json/)
  assert.doesNotMatch(script,/Section WebView|WEBVIEW2/)
  assert.equal(f.calls.some(c=>c.command==='signtool'),false)
  assert.equal(f.calls.filter(c=>c.argv.includes('signer')).length,1)
})

test('signed Windows command plan signs main binary, uninstaller and installer and verifies observed certificate',async t=>{
  const f=await fixture(t,{platform:'win32'})
  await writeFile(f.binary,Buffer.concat([Buffer.from('MZ'),metadataRecord(metadata)]))
  const calls=[]
  const receipt=await nativeRelease(f.args.filter(arg=>arg!=='--allow-unsigned'),{...f.adapters,env:{...f.adapters.env,WINDOWS_CERTIFICATE_THUMBPRINT:'D'.repeat(40)},kit:async()=>({root:path.join(f.temp,'fixture-kit'),inventorySha256:'fixture-kit-pin'}),run:async(command,argv,opts)=>{
    calls.push({command,argv,opts})
    if(command.endsWith('makensis.exe')) {
      f.stages.add(opts.cwd)
      const script=await readFile(path.join(opts.cwd,'installer.nsi'),'utf8')
      assert.match(script,/!define UNINSTALLERSIGNCOMMAND "signtool sign \/sha1 D{40}/)
      assert.match(script,/!uninstfinalize/)
      await writeFile(/!define OUTFILE "([^"]+)"/.exec(script)[1],'fixture signed installer')
    }
    if(argv.includes('signer')) await writeFile(argv.at(-1)+'.sig',signature(await readFile(argv.at(-1))))
    return ''
  }})
  assert.equal(receipt.distribution,'signed-release');assert.equal(receipt.platformVerified,true,'adapter attestation only, NOT real platform acceptance')
  assert.equal(calls.filter(c=>c.command==='signtool').length,2)
  assert.equal(calls.filter(c=>c.command==='powershell.exe').length,3)
  assert.ok(calls.find(c=>c.command==='powershell.exe').argv.at(-1).includes('Cert:\\CurrentUser\\My\\'))
})

test('unsigned policy never permits Stable or local-ad-hoc embedded runtime on the protected signed path',async t=>{
  const f=await fixture(t)
  await assert.rejects(nativeRelease(f.args.filter(arg=>arg!=='--allow-unsigned'),f.adapters),/local-ad-hoc/)
  assert.equal(f.calls.length,0,'runtime distribution rejection precedes certificates/keys/builds')
})

test('standard build entrypoints are thin GPUI wrappers, not legacy builders',async()=>{
  for(const name of ['build-release.mjs','build-windows.mjs']) {
    const text=await readFile(path.join(ROOT,'scripts',name),'utf8')
    assert.match(text,/nativeRelease/);assert.doesNotMatch(text,/src-tauri|spawnSync|tauri\.js|vite|frontendDist/)
  }
})
