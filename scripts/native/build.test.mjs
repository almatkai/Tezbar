import test from 'node:test'
import assert from 'node:assert/strict'
import {parseNative} from './build.mjs'
test('native build arguments cannot request release/signing/channel or partial packaging',()=>{
 for(const args of [['--channel','alpha'],['--version','1.0.0'],['--allow-unsigned'],['--resources','/fixture'],['--output','/fixture']]) assert.throws(()=>parseNative(args))
 assert.equal(parseNative(['--check']).check,true)
 assert.equal(parseNative(['--local-preview']).localPreview,true)
})
