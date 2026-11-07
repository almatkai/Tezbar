import test from 'node:test'
import assert from 'node:assert/strict'
import {mkdtempSync, mkdirSync, writeFileSync, rmSync, realpathSync} from 'node:fs'
import {tmpdir} from 'node:os'
import {join} from 'node:path'
import {spawnSync} from 'node:child_process'
import {artifactCollector,runCargo} from './cargo-artifact.mjs'

test('Cargo artifact selection ignores stale paths, dependencies, libraries and tests',()=>{
  const root=mkdtempSync(join(tmpdir(),'tezbar-artifact-unit-'))
  try {
    const manifest=join(root,'Cargo.toml'), other=join(root,'other.toml')
    for(const file of [manifest,other,'stale','triple-bin','another-bin'].map(p=>p.startsWith(root)?p:join(root,p))) writeFileSync(file,'fixture')
    const message={reason:'compiler-artifact',manifest_path:manifest,target:{name:'tezbar',kind:['bin']},profile:{test:false},executable:join(root,'triple-bin')}
    const collect=artifactCollector(manifest)
    for(const ignored of [{...message,manifest_path:other},{...message,target:{name:'dependency',kind:['bin']}},{...message,target:{name:'tezbar',kind:['lib']}},{...message,profile:{test:true}},{...message,executable:null}]) collect.accept(ignored)
    assert.throws(()=>collect.finish(),/reported 0/)
    collect.accept(message);collect.accept(message)
    assert.equal(collect.finish(),realpathSync(join(root,'triple-bin')))
    collect.accept({...message,executable:join(root,'another-bin')})
    assert.throws(()=>collect.finish(),/reported 2/)
    assert.equal(artifactCollector(manifest,{check:true}).finish(),null)
  } finally {rmSync(root,{recursive:true,force:true})}
})

test('real toy Cargo JSON honors a configured triple and CARGO_TARGET_DIR, ignoring a stale non-triple executable',async()=>{
  const root=mkdtempSync(join(tmpdir(),'tezbar-artifact-cargo-'))
  try {
    const manifest=join(root,'Cargo.toml'), target=join(root,'private-target')
    mkdirSync(join(root,'src'));mkdirSync(join(root,'.cargo'))
    writeFileSync(manifest,'[package]\nname="tezbar-native"\nversion="0.0.0"\nedition="2021"\n[[bin]]\nname="tezbar"\npath="src/main.rs"\n')
    writeFileSync(join(root,'src/main.rs'),'fn main(){}')
    const rust=spawnSync('rustc',['+1.98.1','-vV'],{encoding:'utf8'})
    assert.equal(rust.status,0,rust.stderr)
    const triple=rust.stdout.match(/^host: (.+)$/m)[1]
    writeFileSync(join(root,'.cargo/config.toml'),`[build]\ntarget="${triple}"\n`)
    mkdirSync(join(target,'debug'),{recursive:true})
    writeFileSync(join(target,'debug/tezbar'),'stale binary must never be packaged')
    const env={...process.env,CARGO_TARGET_DIR:target,CARGO_INCREMENTAL:'0'}
    const lock=spawnSync('cargo',['+1.98.1','generate-lockfile','--offline'],{cwd:root,env,encoding:'utf8'})
    assert.equal(lock.status,0,lock.stderr)
    const args=['+1.98.1','build','--locked','--offline','-j2','--message-format=json']
    const executable=await runCargo('cargo',args,{cwd:root,manifest,env})
    assert.equal(executable,realpathSync(join(target,triple,'debug',process.platform==='win32'?'tezbar.exe':'tezbar')))
    assert.notEqual(executable,join(target,'debug/tezbar'))
    assert.equal(await runCargo('cargo',args.map(a=>a==='build'?'check':a),{cwd:root,manifest,env,check:true}),null)
    writeFileSync(join(root,'src/main.rs'),'fn main(){missing_function();}')
    let diagnostic=''
    await assert.rejects(runCargo('cargo',args,{cwd:root,manifest,env,diagnostic:text=>diagnostic+=text}),/Native Cargo exit/)
    assert.match(diagnostic,/missing_function/)
  } finally {rmSync(root,{recursive:true,force:true})}
})
