/** Static audit only: never executes extension code or reads user storage. */
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { basename, join, resolve } from 'node:path'
import ts from 'typescript'
import { LOVED_EXTENSIONS } from '../src/shared/lovedExtensions'

const root = resolve(process.argv[2] || '/tmp/tezbar-full-catalog-audit')
const output = process.argv[3] || '/tmp/tezbar-catalog-source-audit.json'
function sources(directory: string): string[] {
  if (!existsSync(directory)) return []
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const file = join(directory, entry.name)
    if (entry.isDirectory()) return sources(file)
    return entry.isFile() && /\.[cm]?[jt]sx?$/.test(file) && !/\.d\.ts$/.test(file) ? [file] : []
  })
}
const results = LOVED_EXTENSIONS.map((extension) => {
  const name = basename(extension.repository ?? '')
  const directory = join(root, name)
  const packagePath = join(directory, 'package.json')
  if (!existsSync(packagePath)) return { name, error: 'missing package.json' }
  const manifest = JSON.parse(readFileSync(packagePath, 'utf8'))
  const refs: Record<string, Set<string>> = { api: new Set(), utils: new Set() }
  const native: string[] = []
  for (const file of sources(join(directory, 'src'))) {
    const source = readFileSync(file, 'utf8')
    const code = ts.transpileModule(source, {
      fileName: file,
      compilerOptions: {
        module: ts.ModuleKind.CommonJS,
        jsx: ts.JsxEmit.React,
        target: ts.ScriptTarget.ES2022,
      },
    }).outputText
    for (const direct of code.matchAll(
      /require\(["']@raycast\/(api|utils)["']\)\s*\)*\s*\.\s*(\w+)/g
    ))
      refs[direct[1]].add(direct[2])
    for (const alias of code.matchAll(
      /\b(\w+)\s*=\s*require\(["']@raycast\/(api|utils)["']\)(?!\s*\)*\s*\.)/g
    )) {
      for (const ref of code.matchAll(new RegExp(`\\b${alias[1]}\\.(\\w+)`, 'g')))
        refs[alias[2]].add(ref[1])
    }
    if (/swift:|rust:|afplay|osascript|exec(?:Sync|File|FileSync)?\(|spawn(?:Sync)?\(/.test(source))
      native.push(file.slice(directory.length + 1))
  }
  const commandNames = (manifest.commands ?? []).map((command: { name: string }) => command.name)
  return {
    name,
    id: extension.id,
    platforms: manifest.platforms ?? manifest.tezbar?.platforms ?? null,
    commands: manifest.commands ?? [],
    catalogMissingCommands: (extension.commands ?? [])
      .filter((command) => !commandNames.includes(command.name))
      .map((command) => command.name),
    refs: Object.fromEntries(
      Object.entries(refs).map(([key, values]) => [key, [...values].sort()])
    ),
    nativeFiles: native,
  }
})
writeFileSync(output, JSON.stringify(results, null, 2))
console.log(
  `Static inventory: ${results.length} packages; ${results.filter((entry) => 'error' in entry).length} unavailable`
)
for (const entry of results) {
  if ('error' in entry) console.log(`${entry.name}: ${entry.error}`)
  else if (entry.catalogMissingCommands.length)
    console.log(
      `${entry.name}: missing catalog commands ${entry.catalogMissingCommands.join(', ')}`
    )
}
