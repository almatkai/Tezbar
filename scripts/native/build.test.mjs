import test from 'node:test'
import assert from 'node:assert/strict'
import {parseNative} from './build.mjs'
test('native build arguments cannot request signing/channel/version or partial packaging',()=>{
 for(const args of [['--channel','alpha'],['--version','1.0.0'],['--allow-unsigned'],['--resources','/fixture'],['--output','/fixture']]) assert.throws(()=>parseNative(args))
 assert.equal(parseNative(['--check']).check,true)
 assert.throws(()=>parseNative(['--local-preview']), /requires/)
})

import {binaryPath,assertSameSource} from './build.mjs'
test('assembly uses Cargo target/profile and refuses source changes',()=>{
 assert.equal(binaryPath('/fixture/custom-target',{release:true},'darwin'),'/fixture/custom-target/release/tezbar')
 assert.equal(binaryPath('/fixture/custom-target',{release:false},'win32'),'/fixture/custom-target/debug/tezbar.exe')
 const before={sourceCommit:'a'.repeat(40),sourceDirty:true,sourceDigest:'one'}
 assertSameSource(before,{...before})
 assert.throws(()=>assertSameSource(before,{...before,sourceDigest:'two'}),/changed during build/)
 const options=parseNative(['--release','--local-preview','--resources','/fixture/resources','--output','/fixture/stage'])
 assert.equal(options.release,true);assert.equal(options.localPreview,true)
})
