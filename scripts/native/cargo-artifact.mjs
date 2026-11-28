import { spawn } from 'node:child_process'
import { createInterface } from 'node:readline'
import { realpathSync } from 'node:fs'

/** Cargo is the authority for triples, custom target dirs and build.target.
 * A dependency, test, library, stale conventional path or other manifest is not
 * the native executable. Repeated identical messages are harmless. */
export function artifactCollector(manifest, {check = false} = {}) {
  const expected = realpathSync(manifest), executables = new Set()
  return {
    accept(message) {
      if (message.reason !== 'compiler-artifact' || !message.executable ||
          message.target?.name !== 'tezbar' || !message.target.kind?.includes('bin') ||
          message.profile?.test !== false || !message.manifest_path ||
          realpathSync(message.manifest_path) !== expected) return
      executables.add(realpathSync(message.executable))
    },
    finish() {
      if (check) return null // cargo check deliberately produces no executable.
      if (executables.size !== 1) throw Error(`Expected one tezbar-native bin artifact; Cargo reported ${executables.size}`)
      return [...executables][0]
    },
  }
}

export async function runCargo(command, args, {cwd, manifest, check = false, env = process.env, diagnostic = text => process.stderr.write(text)} = {}) {
  const collector = artifactCollector(manifest, {check})
  const child = spawn(command, args, {cwd, env, stdio:['ignore','pipe','inherit']})
  const done = new Promise(resolve => {
    child.once('error', error => resolve({error}))
    child.once('close', (code, signal) => resolve({code, signal}))
  })
  let failure
  for await (const line of createInterface({input:child.stdout, crlfDelay:Infinity})) {
    if (!line.trim()) continue
    try {
      const message = JSON.parse(line)
      if (message.reason === 'compiler-message' && message.message?.rendered) diagnostic(message.message.rendered)
      collector.accept(message)
    } catch (error) { failure ??= Error(`Invalid native Cargo artifact stream: ${error.message}`) }
  }
  const result = await done
  if (result.error) throw result.error
  if (result.code !== 0) throw Error(`Native Cargo exit ${result.code ?? result.signal}; see compiler diagnostics above`)
  if (failure) throw failure
  return collector.finish()
}
