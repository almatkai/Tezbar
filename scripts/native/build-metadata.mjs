import { createReadStream } from 'node:fs'
import { lstat } from 'node:fs/promises'
import { createHash } from 'node:crypto'
export const GPUI_REV = 'c84af5b99cbc366d675064e52b23664a015b6e35'
const start = Buffer.from('\0TEZBAR_NATIVE_BUILD_V2\0'), end = Buffer.from('\0TEZBAR_NATIVE_BUILD_END\0')
export function metadataRecord(metadata) {
  return Buffer.concat([start, Buffer.from(JSON.stringify(metadata)), end])
}
export async function captureBuildMetadata(executable) {
  const stat = await lstat(executable)
  if (!stat.isFile() || stat.isSymbolicLink()) throw Error('Native executable must be a regular file')
  let pending = Buffer.alloc(0), captured
  for await (const chunk of createReadStream(executable)) {
    pending = Buffer.concat([pending, chunk])
    for (;;) {
      const at = pending.indexOf(start)
      if (at < 0) { pending = pending.subarray(Math.max(0, pending.length - start.length)); break }
      pending = pending.subarray(at)
      if (pending.length <= start.length) break
      if (pending[start.length] !== 123) { pending = pending.subarray(start.length); continue }
      const stop = pending.indexOf(end, start.length)
      if (stop < 0) {
        if (pending.length > 4096) throw Error('Native metadata record too large')
        break
      }
      if (stop - start.length > 4096 || captured) throw Error('Ambiguous native binary metadata')
      const json = pending.subarray(start.length, stop)
      const metadata = JSON.parse(json)
      const keys=['schema','version','sourceCommit','dirty','benchControl','runtimeShaders','gpuiRev','rustVersion']
      if (Object.keys(metadata).length !== keys.length || keys.some(k=>!(k in metadata)) ||
          metadata.schema !== 2 || !/^[0-9a-f]{40}$/i.test(metadata.sourceCommit) ||
          !/^\d+\.\d+\.\d+(?:-(?:alpha|beta)\.\d+)?$/.test(metadata.version) ||
          typeof metadata.dirty !== 'boolean' || typeof metadata.benchControl !== 'boolean' ||
          typeof metadata.runtimeShaders !== 'boolean' || metadata.gpuiRev !== GPUI_REV || metadata.rustVersion !== '1.98.1')
        throw Error('Invalid native binary metadata')
      captured = {metadata, sha256:createHash('sha256').update(json).digest('hex')}
      pending = pending.subarray(stop + end.length)
    }
  }
  if (pending.subarray(0,start.length).equals(start) && pending[start.length] === 123) throw Error('Truncated native binary metadata')
  if (!captured) throw Error('Native binary metadata missing (requires schema 2 build)')
  return captured
}
