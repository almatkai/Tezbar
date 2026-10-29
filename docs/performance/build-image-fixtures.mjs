// Compile the exact before/after production image module for opt-in WebView
// retention measurements. No substitute implementation or production export.
import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { transformSync } from 'esbuild'
const output = process.argv[2]
if (!output) throw new Error('Provide a temporary artifact directory')
const before = execFileSync('git', ['show', '354689e:src/renderer/extensionImages.ts'], {
  encoding: 'utf8',
})
const after = readFileSync('src/renderer/extensionImages.ts', 'utf8')
for (const [name, source] of [
  ['baseline', before],
  ['candidate', after],
]) {
  const result = transformSync(source, {
    loader: 'ts',
    format: 'iife',
    globalName: 'TezbarImageFixture',
    target: 'safari15',
  })
  writeFileSync(
    `${output}/image-${name}.js`,
    `${result.code}\nwindow.__tezbarImageFixture = TezbarImageFixture;\n`
  )
}
