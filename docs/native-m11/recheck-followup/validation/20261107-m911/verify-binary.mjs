import {captureBuildMetadata,GPUI_REV} from '/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent/scripts/native/build-metadata.mjs'
import {spawnSync} from 'node:child_process'
import {createHash} from 'node:crypto'
import {readFileSync} from 'node:fs'
import assert from 'node:assert/strict'
const [executable,benchArg]=process.argv.slice(2),bench=benchArg==='true'
assert.ok(executable);assert.ok(['true','false'].includes(benchArg))
const captured=await captureBuildMetadata(executable)
const run=(exe,args)=>{const r=spawnSync(exe,args,{encoding:'utf8'});assert.equal(r.status,0,r.stderr);return r.stdout}
const version=JSON.parse(run(executable,['--version']))
const rust=run('rustc',['+1.98.1','-vV'])
assert.deepEqual(Object.keys(version).sort(),['version','source','dirty','rustc_commit','preview','gpui_rev','runtime_shaders','bench_control'].sort())
for(const key of ['version','source','dirty','rustc_commit','gpui_rev'])assert.equal(typeof version[key],'string')
for(const key of ['preview','runtime_shaders','bench_control'])assert.equal(typeof version[key],'boolean')
assert.equal(captured.metadata.sourceCommit,'6ffa4e50fc0a45927319f9d791677ed40f78a948')
assert.equal(captured.metadata.dirty,true);assert.equal(captured.metadata.benchControl,bench)
assert.equal(captured.metadata.runtimeShaders,true);assert.equal(captured.metadata.rustVersion,'1.98.1')
assert.equal(captured.metadata.gpuiRev,GPUI_REV)
assert.equal(version.version,captured.metadata.version);assert.equal(version.source,captured.metadata.sourceCommit)
assert.equal(version.dirty,'true');assert.equal(version.preview,true)
assert.equal(version.gpui_rev,captured.metadata.gpuiRev);assert.equal(version.runtime_shaders,true);assert.equal(version.bench_control,bench)
assert.equal(version.rustc_commit,rust.match(/^commit-hash: (.+)$/m)[1])
const links=run('otool',['-L',executable]);assert.ok(!/WebKit|JavaScriptCore/.test(links))
console.log(JSON.stringify({executable,sha256:createHash('sha256').update(readFileSync(executable)).digest('hex'),captured,version,rustc:rust,links,assertions:'Actual native binary static metadata matches --version types/fields, dirty=true and exact build features; no WebKit/JavaScriptCore. Only --version executed; production dirty/bench refusal never stripped record. No clean/production admission claimed.'},null,2))
