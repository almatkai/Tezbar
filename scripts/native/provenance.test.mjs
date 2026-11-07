import test from 'node:test'
import {sourceSnapshot} from './build.mjs'
import assert from 'node:assert/strict'
import {mkdtempSync,mkdirSync,writeFileSync,readFileSync,rmSync,cpSync,symlinkSync,unlinkSync} from 'node:fs'
import {tmpdir} from 'node:os'
import {join} from 'node:path'
import {spawnSync} from 'node:child_process'
test('real native build script rechecks dependency edits and refuses unattested archives',()=>{
 const root=mkdtempSync(join(tmpdir(),'tezbar-build-provenance-'))
 try {
  const run=(exe,args,extra={})=>{const r=spawnSync(exe,args,{cwd:root,encoding:'utf8',env:{...process.env,CARGO_TARGET_DIR:join(root,'target'),CARGO_INCREMENTAL:'0',TEZBAR_RELEASE_NOTES_PATH:join(root,'notes.md'),...extra}});assert.equal(r.status,0,r.stderr);return r.stdout.trim()}
  for(const d of ['apps/tezbar/src','dependency/src'])mkdirSync(join(root,d),{recursive:true})
  writeFileSync(join(root,'Cargo.toml'),'[workspace]\nmembers=["apps/tezbar","dependency"]\nresolver="2"\n')
  writeFileSync(join(root,'apps/tezbar/Cargo.toml'),'[package]\nname="native-provenance-fixture"\nversion="0.0.0"\nedition="2021"\n[dependencies]\ndependency={path="../../dependency"}\n')
  writeFileSync(join(root,'apps/tezbar/build.rs'),readFileSync(new URL('../../apps/tezbar/build.rs',import.meta.url)))
  writeFileSync(join(root,'apps/tezbar/src/main.rs'),'fn main(){println!("{}:{}",env!("TEZBAR_NATIVE_DIRTY"),dependency::value());}')
  writeFileSync(join(root,'dependency/Cargo.toml'),'[package]\nname="dependency"\nversion="0.0.0"\nedition="2021"\n')
  writeFileSync(join(root,'dependency/src/lib.rs'),'pub fn value()->u8{1}')
  writeFileSync(join(root,'notes.md'),'private fixture notes\n')
  writeFileSync(join(root,'.gitignore'),'target\n')
  run('cargo',['+1.98.1','generate-lockfile','--offline'])
  run('git',['init','-q']);run('git',['add','.']);run('git',['-c','user.name=Fixture','-c','user.email=fixture@localhost','-c','commit.gpgsign=false','commit','-qm','fixture'])
  const command=['+1.98.1','run','--locked','--offline','-q','-p','native-provenance-fixture']
  assert.equal(run('cargo',command),'false:1')
  writeFileSync(join(root,'dependency/src/lib.rs'),'pub fn value()->u8{2}')
  assert.equal(run('cargo',command),'true:2')
  run('git',['add','.']);run('git',['-c','user.name=Fixture','-c','user.email=fixture@localhost','-c','commit.gpgsign=false','commit','-qm','dependency'])
  assert.equal(run('cargo',command),'false:2')
  const external=mkdtempSync(join(tmpdir(),'tezbar-untracked-notes-'))
  try {
   writeFileSync(join(external,'notes.md'),'external notes\n')
   assert.equal(run('cargo',command,{TEZBAR_RELEASE_NOTES_PATH:join(external,'notes.md')}),'true:2')
   assert.equal(run('cargo',command),'false:2')
  } finally {rmSync(external,{recursive:true,force:true})}
  // Reuse this real workspace inside an ignored directory of another clean repo.
  // Git must not attest the outer repository as the native workspace's source.
  const ignored=mkdtempSync(join(tmpdir(),'tezbar-outer-provenance-'))
  try {
   const git=args=>{const r=spawnSync('git',args,{cwd:ignored,encoding:'utf8'});assert.equal(r.status,0,r.stderr)}
   mkdirSync(join(ignored,'out'))
   writeFileSync(join(ignored,'.gitignore'),'out/\n')
   git(['init','-q']);git(['add','.']);git(['-c','user.name=Fixture','-c','user.email=fixture@localhost','-c','commit.gpgsign=false','commit','-qm','outer'])
   const nested=join(ignored,'out/native')
   cpSync(root,nested,{recursive:true,filter:path=>path!==join(root,'.git')&&!path.startsWith(join(root,'.git')+'/')&&path!==join(root,'target')&&!path.startsWith(join(root,'target')+'/')})
   const nestedEnv={...process.env,CARGO_TARGET_DIR:join(ignored,'out/target'),CARGO_INCREMENTAL:'0',TEZBAR_RELEASE_NOTES_PATH:join(nested,'notes.md')}
   const nestedRun=()=>{const r=spawnSync('cargo',command,{cwd:nested,encoding:'utf8',env:nestedEnv});assert.equal(r.status,0,r.stderr);return r.stdout.trim()}
   assert.equal(nestedRun(),'true:2')
   writeFileSync(join(nested,'dependency/src/lib.rs'),'pub fn value()->u8{3}')
   assert.equal(nestedRun(),'true:3')
   assert.throws(()=>sourceSnapshot(nested),/workspace root/)
  } finally {rmSync(ignored,{recursive:true,force:true})}
  symlinkSync('missing-one',join(root,'broken'))
  const first=sourceSnapshot(root)
  unlinkSync(join(root,'broken'));symlinkSync('missing-two',join(root,'broken'))
  assert.notEqual(first.sourceDigest,sourceSnapshot(root).sourceDigest)
  unlinkSync(join(root,'broken'))
  // This is an archive fixture, not a product checkout. An explicit source SHA
  // alone is not an attestation of the archive's actual content.
  rmSync(join(root,'.git'),{recursive:true})
  const r=spawnSync('cargo',command,{cwd:root,encoding:'utf8',env:{...process.env,CARGO_TARGET_DIR:join(root,'target'),TEZBAR_RELEASE_NOTES_PATH:join(root,'notes.md'),TEZBAR_NATIVE_SOURCE_SHA:'a'.repeat(40)}})
  assert.equal(r.status,0,r.stderr);assert.equal(r.stdout.trim(),'true:2')
 } finally {rmSync(root,{recursive:true,force:true})}
})
