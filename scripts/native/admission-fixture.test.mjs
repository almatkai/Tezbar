// Isolated toy crate only. No GPUI/workspace build and no product executable launch.
import test from 'node:test'
import assert from 'node:assert/strict'
import {mkdtempSync,mkdirSync,writeFileSync,rmSync,readFileSync} from 'node:fs'
import {tmpdir} from 'node:os'
import {join} from 'node:path'
import {fileURLToPath} from 'node:url'
import {spawnSync} from 'node:child_process'
import {captureBuildMetadata,metadataRecord,GPUI_REV,RUST_VERSION} from './build-metadata.mjs'
import {runCargo} from './cargo-artifact.mjs'

test('static metadata capture handles chunk boundaries, duplicates, truncation and missing records without execution',async()=>{
  const root=mkdtempSync(join(tmpdir(),'tezbar-metadata-bytes-'))
  try {
    const exe=join(root,'fixture')
    const metadata={schema:2,version:'9.8.7-beta.1',sourceCommit:'b'.repeat(40),dirty:false,benchControl:false,runtimeShaders:true,gpuiRev:GPUI_REV,rustVersion:'1.98.1'}
    const record=metadataRecord(metadata)
    writeFileSync(exe,Buffer.concat([Buffer.alloc(65530),record,Buffer.alloc(30)]))
    assert.deepEqual((await captureBuildMetadata(exe)).metadata,metadata)
    for(const bytes of [Buffer.concat([record,record]),record.subarray(0,record.length-5),Buffer.from('missing')]) {
      writeFileSync(exe,bytes);await assert.rejects(captureBuildMetadata(exe))
    }
  } finally {rmSync(root,{recursive:true,force:true})}
})

test('real production payload and quit-budget sources pass in a private toy crate; optimized binary retains truthful static metadata',async()=>{
  const root=mkdtempSync(join(tmpdir(),'tezbar-admission-toy-'))
  try {
    const source=fileURLToPath(new URL('../../apps/tezbar/src/app/',import.meta.url))
    mkdirSync(join(root,'src'))
    // Copy only these pure files. No manifests, lockfiles or targets from the product.
    for(const [file,original] of [['build_metadata.rs','production_launch/build_metadata.rs'],['bundle_validation.rs','production_launch/bundle_validation.rs'],['quit_budget.rs','quit_budget.rs']])
      writeFileSync(join(root,'src',file),readFileSync(join(source,original)))
    writeFileSync(join(root,'src/main.rs'),'mod build_metadata; mod bundle_validation; mod quit_budget; fn main(){println!("{}",serde_json::to_string(&build_metadata::current()).unwrap());}')
    const manifest=join(root,'Cargo.toml')
    writeFileSync(manifest,'[package]\nname="tezbar-native"\nversion="9.8.7-beta.1"\nedition="2021"\n[[bin]]\nname="tezbar"\npath="src/main.rs"\n[features]\nruntime-shaders=[]\nbench-control=[]\n[dependencies]\nserde={version="1",features=["derive"]}\nserde_json="1"\nsha2="0.10"\n[dev-dependencies]\ntempfile="3.27"\n')
    const env={...process.env,CARGO_TARGET_DIR:join(root,'private-target'),CARGO_INCREMENTAL:'0',TEZBAR_NATIVE_GIT_SHA:'b'.repeat(40),TEZBAR_NATIVE_DIRTY:'false',TEZBAR_NATIVE_GPUI_REV:GPUI_REV,TEZBAR_NATIVE_GPUI_POLICY_REV:GPUI_REV,TEZBAR_NATIVE_RUST_VERSION:RUST_VERSION,TEZBAR_NATIVE_RUST_POLICY_VERSION:RUST_VERSION}
    const run=args=>{const r=spawnSync('cargo',args,{cwd:root,env,encoding:'utf8',maxBuffer:4*1024*1024});assert.equal(r.status,0,r.stdout+'\n'+r.stderr);return r.stdout}
    run(['+1.98.1','generate-lockfile','--offline'])
    const tests=run(['+1.98.1','test','--locked','--offline','-j2','--features','runtime-shaders'])
    assert.match(tests,/7 passed; 0 failed/)
    const args=['+1.98.1','build','--release','--locked','--offline','-j2','--message-format=json','--features','runtime-shaders']
    const exe=await runCargo('cargo',args,{cwd:root,manifest,env})
    const first=await captureBuildMetadata(exe)
    assert.equal(first.metadata.sourceCommit,'b'.repeat(40));assert.equal(first.metadata.version,'9.8.7-beta.1');assert.equal(first.metadata.dirty,false);assert.equal(first.metadata.benchControl,false)
    args[args.length-1]='runtime-shaders,bench-control'
    await runCargo('cargo',args,{cwd:root,manifest,env})
    assert.equal((await captureBuildMetadata(exe)).metadata.benchControl,true)
    await runCargo('cargo',args,{cwd:root,manifest,env:{...env,TEZBAR_NATIVE_DIRTY:'true'}})
    assert.equal((await captureBuildMetadata(exe)).metadata.dirty,true)
  } finally {rmSync(root,{recursive:true,force:true})}
})
