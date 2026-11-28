/** GPUI integration gate, not a binary/link audit or a general JS/shell interpreter.
 * Checks default package routes (including aliases and lifecycle hooks), their local
 * JS/TS imports and literal subprocess/bundler inputs, and Cargo's normal tree.
 * Arbitrary computed JS, external package implementations and Rust source behavior
 * still need review. Node/esbuild/React serialization in the isolated extension
 * worker is compatible with GPUI; browser mounting and WebView dependencies are not.
 * Unreachable legacy files/test oracles and README/comment text are not scanned.
 */
import { spawnSync } from 'node:child_process'
import { readFileSync, realpathSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import ts from 'typescript'

export const APPLICATION_ENTRYPOINTS = Object.freeze([
  'dev', 'build', 'preview', 'release:build', 'release', 'build:windows', 'dist', 'start',
])
const REQUIRED = ['dev', 'build', 'preview', 'release:build']
const MAX_FILES = 256
const MAX_SOURCE_BYTES = 2 * 1024 * 1024
const normalize = value => value.replaceAll('\\', '/')
const webModule = value => /^(?:@tauri-apps\/|vite(?:\/|$)|@vitejs\/|electron(?:\/|$)|react-dom(?:\/|$)|@webview\/)/i.test(value)
const webPath = value => /(?:^|\/)(?:src\/renderer|src-tauri|tests\/oracles\/legacy-renderer)(?:\/|$)/i.test(normalize(value))
const webCommand = value => /^(?:tauri|vite|electron)(?:\.(?:js|cjs|mjs|exe|cmd))?$/i.test(path.posix.basename(normalize(value))) || /(?:^|\/)@tauri-apps\//i.test(normalize(value))
// Retain the established updater signature format without allowing a Tauri app
// entrypoint. Only Node's CLI signer subcommand is a non-UI build utility.
const updaterSigner = (program, argv) => path.posix.basename(normalize(program)).replace(/\.exe$/, '') === 'node' &&
  /(?:^|\/)node_modules\/@tauri-apps\/cli\/tauri\.js$/.test(normalize(argv[0] ?? '')) &&
  argv[1] === 'signer' && argv[2] === 'sign'
const issue = (code, location, message) => ({ code, location, message })

// Intentionally bounded package-script grammar: argv, quotes, env assignments and
// command separators. Substitutions, redirects and shell control flow fail closed.
function commands(source) {
  const result = [], words = []
  let word = '', quote = '', started = false
  const flush = () => { if (started) words.push(word); word = ''; started = false }
  for (let i = 0; i < source.length; i++) {
    const c = source[i]
    if (quote) {
      if (c === quote) quote = ''
      else if (quote === '"' && /[$`]/.test(c)) throw Error('Shell interpolation is outside the route grammar')
      else word += c
      continue
    }
    if (c === '"' || c === "'") { quote = c; started = true }
    else if (c === '\\') { if (++i === source.length) throw Error('Trailing escape'); word += source[i]; started = true }
    else if (c === '\n' || /[;&|]/.test(c)) {
      flush(); if (words.length) result.push(words.splice(0))
    } else if (/\s/.test(c)) flush()
    else if (/[$`<>()]/.test(c)) throw Error('Use explicit argv/native scripts instead of shell expansion or control flow')
    else { word += c; started = true }
  }
  if (quote) throw Error('Unclosed quote')
  flush(); if (words.length) result.push(words)
  if (!result.length) throw Error('Empty application route')
  return result
}

/** Pure except for the injected source reader: fixtures need no filesystem/Cargo.
 * readSource accepts repository-relative POSIX names, throws for missing files.
 */
export function checkEntrypoints(packageJson, { readSource, entrypoints = APPLICATION_ENTRYPOINTS } = {}) {
  const issues = [], scripts = packageJson.scripts ?? {}, seenScripts = new Set(), seenFiles = new Set()
  let sourceBytes = 0
  const fail = (code, location, message) => issues.push(issue(code, location, message))
  function localFile(name, from = '') {
    const file = path.posix.normalize(normalize(name))
    if (file.startsWith('../') || path.posix.isAbsolute(file)) throw Error(`Source escapes checkout: ${file}`)
    if (webPath(file) || webCommand(file)) { fail('web-source', from || file, `Prohibited application entrypoint/import source: ${file}`); return }
    if (seenFiles.has(file)) return
    if (seenFiles.size >= MAX_FILES) throw Error(`Reachable source exceeds ${MAX_FILES} files`)
    if (!/\.[cm]?[jt]sx?$/.test(file)) throw Error(`Unsupported executable source ${file}; use an inspectable JS/TS native wrapper`)
    if (!readSource) throw Error(`No source reader for ${file}`)
    const source = readSource(file)
    sourceBytes += Buffer.byteLength(source)
    if (sourceBytes > MAX_SOURCE_BYTES) throw Error('Reachable source exceeds 2 MiB')
    seenFiles.add(file)
    inspectSource(file, source)
  }
  function moduleFile(specifier, file) {
    if (webModule(specifier)) { fail('web-import', file, `UI/build route imports ${specifier}`); return }
    if (!specifier.startsWith('.')) return // Node/tool/extension packages are not UI by themselves.
    const base = path.posix.normalize(path.posix.join(path.posix.dirname(file), specifier))
    if (path.posix.extname(base)) return localFile(base, file)
    for (const candidate of [base + '.ts', base + '.tsx', base + '.mjs', base + '.js', base + '/index.ts', base + '/index.js']) {
      try { readSource(candidate) } catch (error) { if (error.code === 'ENOENT') continue; throw error }
      return localFile(candidate, file)
    }
    throw Error(`Cannot resolve ${specifier} from ${file}`)
  }
  function script(name, stack = []) {
    if (stack.includes(name)) { fail('script-cycle', name, `Script cycle: ${[...stack, name].join(' -> ')}`); return }
    if (seenScripts.has(name)) return
    if (typeof scripts[name] !== 'string') { fail('missing-script', name, 'Application script is missing'); return }
    seenScripts.add(name)
    for (const hook of [`pre${name}`, name, `post${name}`]) {
      if (hook !== name && scripts[hook] !== undefined) script(hook, [...stack, name])
      if (hook !== name) continue
      try { for (const argv of commands(scripts[name])) command(argv, name, [...stack, name]) }
      catch (error) { fail('unsupported-route', name, error.message) }
    }
  }
  function command(input, location, stack = []) {
    const argv = [...input]
    while (/^[A-Za-z_][A-Za-z_0-9]*=/.test(argv[0] ?? '')) argv.shift()
    if (argv[0] === 'env') return command(argv.slice(1), location, stack)
    const program = argv.shift() ?? '', executable = path.posix.basename(normalize(program)).replace(/\.(?:exe|cmd)$/, '')
    if (updaterSigner(program, argv)) return
    if (webCommand(program) || webModule(program) || webPath(program)) {
      fail('web-command', location, `Application route invokes a web UI tool/source: ${input.join(' ')}`); return
    }
    if (['pnpm', 'npm', 'bun', 'yarn'].includes(executable)) {
      if (['exec', 'dlx', 'x'].includes(argv[0])) return command(argv.slice(1), location, stack)
      if (argv[0] === 'run' || argv[0] === 'run-script') argv.shift()
      if (argv[0] === '--') argv.shift()
      return script(argv[0], stack)
    }
    if (executable === 'npx') return command(argv, location, stack)
    if (['node', 'tsx'].includes(executable)) {
      if (argv[0] === '--import' && argv[1] === 'tsx') argv.splice(0, 2)
      if (!argv[0] || argv[0].startsWith('-')) throw Error('Node routes must name a source file (no eval/preload)')
      return localFile(argv[0], location)
    }
    if (executable === 'cargo') {
      if (argv[0]?.startsWith('+')) argv.shift()
      if (webCommand(argv[0] ?? '')) { fail('web-command', location, `Cargo invokes ${argv[0]}`); return }
      if (!['run', 'build', 'check'].includes(argv[0])) throw Error('Application Cargo routes must run/build/check tezbar-native')
      const packages = [], manifests = [], features = []
      for (let i = 1; i < argv.length && argv[i] !== '--'; i++) {
        const [option, inline] = argv[i].split('=', 2)
        if (['-p', '--package', '--manifest-path', '--features', '-F'].includes(option)) {
          const value = inline ?? argv[++i] ?? ''
          if (['-p', '--package'].includes(option)) packages.push(value)
          else if (option === '--manifest-path') manifests.push(normalize(value))
          else features.push(...value.split(/[,\s]+/))
        }
      }
      if ((!packages.length && !manifests.length) || packages.some(p => p !== 'tezbar-native') ||
          manifests.some(m => m !== 'apps/tezbar/Cargo.toml') || argv.some(a => ['--workspace', '--all', '--all-features'].includes(a)) ||
          features.some(f => f !== 'runtime-shaders')) throw Error('Cargo route must select only tezbar-native with default/runtime-shaders features (the inspected graph)')
      return
    }
    throw Error(`Unrecognized application route ${program}; use an explicit native JS/TS or Cargo entrypoint`)
  }
  function inspectSource(file, source) {
    const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true)
    if (ast.parseDiagnostics.length) throw Error(`Cannot parse ${file}: ${ast.parseDiagnostics[0].messageText}`)
    const bindings = new Map(), launchers = new Map(), processNamespaces = new Set(), adapterMethods = new Set(), functions = []
    const processMethods = new Set(['spawn', 'spawnSync', 'exec', 'execSync', 'execFile', 'execFileSync', 'fork'])
    const walk = (node, visit) => { visit(node); ts.forEachChild(node, child => walk(child, visit)) }
    walk(ast, node => {
      if (ts.isFunctionDeclaration(node) && node.name && node.body) functions.push([node.name.text, node.body])
      if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name) && node.initializer &&
          (ts.isArrowFunction(node.initializer) || ts.isFunctionExpression(node.initializer))) functions.push([node.name.text, node.initializer.body])
      if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name) && node.initializer) bindings.set(node.name.text, node.initializer)
      if (ts.isVariableDeclaration(node) && node.initializer && ts.isCallExpression(node.initializer) &&
          node.initializer.expression.getText(ast) === 'require' && ['node:child_process', 'child_process'].includes(node.initializer.arguments[0]?.text)) {
        if (ts.isIdentifier(node.name)) processNamespaces.add(node.name.text)
        if (ts.isObjectBindingPattern(node.name)) for (const binding of node.name.elements) {
          const method = binding.propertyName?.getText(ast) ?? binding.name.getText(ast)
          if (processMethods.has(method)) launchers.set(binding.name.getText(ast), method)
        }
      }
      if (ts.isImportDeclaration(node)) {
        const module = node.moduleSpecifier.text, clause = node.importClause
        if (['node:child_process', 'child_process'].includes(module)) {
          if (clause?.name) processNamespaces.add(clause.name.text)
          if (clause?.namedBindings && ts.isNamespaceImport(clause.namedBindings)) processNamespaces.add(clause.namedBindings.name.text)
          for (const specifier of clause?.namedBindings?.elements ?? []) {
            const method = specifier.propertyName?.text ?? specifier.name.text
            if (processMethods.has(method)) launchers.set(specifier.name.text, method)
          }
        }
        for (const specifier of clause?.namedBindings?.elements ?? []) {
          if ((specifier.propertyName?.text ?? specifier.name.text) === 'runCargo' && module.endsWith('/cargo-artifact.mjs')) launchers.set(specifier.name.text, 'runCargo')
        }
      }
    })
    const launcherFor = expression => ts.isIdentifier(expression) ? launchers.get(expression.text) :
      ts.isPropertyAccessExpression(expression) ?
        processNamespaces.has(expression.expression.getText(ast)) && processMethods.has(expression.name.text) ? expression.name.text :
          adapterMethods.has(expression.name.text) ? launchers.get(expression.name.text) : undefined : undefined
    // Follow simple forwarding helpers and injectable adapters (run=spawnSync,
    // {run:runTool}, deps.run). Eight passes bound this small build-tool graph.
    for (let pass = 0; pass < 8; pass++) {
      let changed = false
      const add = (name, kind) => { if (kind && !launchers.has(name)) { launchers.set(name, kind); changed = true } }
      walk(ast, node => {
        if ((ts.isVariableDeclaration(node) || ts.isBindingElement(node) || ts.isParameter(node)) && ts.isIdentifier(node.name) && node.initializer && ts.isIdentifier(node.initializer)) add(node.name.text, launchers.get(node.initializer.text))
        if (ts.isPropertyAssignment(node) && ts.isIdentifier(node.initializer) && launchers.has(node.initializer.text)) {
          const name = node.name.getText(ast).replaceAll(/["']/g, '')
          adapterMethods.add(name); add(name, launchers.get(node.initializer.text))
        }
      })
      for (const [name, body] of functions) walk(body, node => { if (ts.isCallExpression(node)) add(name, launcherFor(node.expression)) })
      if (!changed) break
    }
    // Resolve literal argv/path fragments through local constants, not arbitrary
    // evaluation. No source module is imported/executed by this gate.
    function strings(node, seen = new Set()) {
      if (!node) return []
      if (ts.isStringLiteralLike(node)) return [node.text]
      if (ts.isIdentifier(node)) {
        if (node.text === '__dirname') return [path.posix.dirname(file)]
        if (seen.has(node.text)) return []
        return strings(bindings.get(node.text), new Set([...seen, node.text]))
      }
      if (ts.isPropertyAccessExpression(node) && node.getText(ast) === 'process.execPath') return ['node']
      if (ts.isPropertyAssignment(node)) return strings(node.initializer, seen)
      if (ts.isCallExpression(node) && node.expression.getText(ast) === 'fileURLToPath' && node.arguments[0]?.getText(ast) === 'import.meta.url') return [file]
      if (ts.isCallExpression(node) && /^(?:path\.)?dirname$/.test(node.expression.getText(ast))) return strings(node.arguments[0], seen).map(path.posix.dirname)
      if (ts.isCallExpression(node) && /^(?:(?:path\.)?(?:join|resolve))$/.test(node.expression.getText(ast))) {
        const parts = node.arguments.map(a => strings(a, seen))
        if (parts.every(p => p.length === 1)) return [path.posix.normalize(parts.flat().join('/'))]
        // A dynamic root must not hide a literal UI-tool suffix. Don't turn
        // other partial paths into invented executable/source filenames.
        const suffix = parts.slice(parts.findLastIndex(p => p.length !== 1) + 1).flat().join('/')
        return webCommand(suffix) || webPath(suffix) ? [suffix] : []
      }
      if (ts.isNewExpression(node) && node.expression.getText(ast) === 'URL' && node.arguments?.[1]?.getText(ast) === 'import.meta.url') {
        return strings(node.arguments[0], seen).map(value => path.posix.join(path.posix.dirname(file), value))
      }
      if (ts.isCallExpression(node) && node.expression.getText(ast) === 'fileURLToPath') return strings(node.arguments[0], seen)
      if (ts.isArrayLiteralExpression(node)) return node.elements.flatMap(child => strings(child, seen))
      if (ts.isObjectLiteralExpression(node)) return node.properties.flatMap(child => strings(child, seen))
      if (ts.isConditionalExpression(node)) return [...strings(node.whenTrue, seen), ...strings(node.whenFalse, seen)]
      if (ts.isBinaryExpression(node)) {
        const left = strings(node.left, seen), right = strings(node.right, seen)
        return node.operatorToken.kind === ts.SyntaxKind.PlusToken && left.length === 1 && right.length === 1 ? [left[0] + right[0]] : [...left, ...right]
      }
      if (ts.isParenthesizedExpression(node) || ts.isAsExpression(node) || ts.isNonNullExpression(node) || ts.isSpreadElement(node)) return strings(node.expression, seen)
      return [] // Do not invent argv by collecting strings from arbitrary calls/conditions.
    }
    function moduleReference(node) {
      const values = strings(node)
      for (const value of values) moduleFile(value, file)
    }
    walk(ast, node => {
      if ((ts.isImportDeclaration(node) && !node.importClause?.isTypeOnly) || ts.isExportDeclaration(node)) {
        if (node.moduleSpecifier) moduleReference(node.moduleSpecifier)
      }
      if (!ts.isCallExpression(node)) return
      const name = node.expression.getText(ast).split('.').at(-1)
      const launcher = launcherFor(node.expression)
      if (name === 'require' || node.expression.kind === ts.SyntaxKind.ImportKeyword) moduleReference(node.arguments[0])
      if (launcher) {
        const programs = strings(node.arguments[0]), args = strings(node.arguments[1])
        if (programs.length === 1 && updaterSigner(programs[0], args)) return
        for (const value of strings(node.arguments[0])) {
          if (webCommand(value) || webModule(value) || webPath(value)) fail('web-subprocess', file, `Subprocess references web UI: ${value}`)
        }
        // exec('pnpm build:frontend') must follow the same alias graph as package scripts.
        if (['exec', 'execSync'].includes(launcher)) {
          for (const value of strings(node.arguments[0])) for (const argv of commands(value)) command(argv, file)
        } else if (launcher === 'fork') {
          for (const value of strings(node.arguments[0])) localFile(value, file)
        } else {
          if (programs.some(program => ['sh', 'bash', 'zsh', 'cmd', 'powershell', 'pwsh'].includes(path.posix.basename(program)))) throw Error('Subprocess shell routes need an inspectable argv/native wrapper')
          if (programs.length === 1 && ['pnpm', 'npm', 'node', 'tsx', 'bun', 'yarn', 'npx', 'cargo', 'env'].includes(path.posix.basename(programs[0])) && args.length) command([programs[0], ...args], file)
        }
      }
      // esbuild-style entryPoints are inputs, not README strings. A Node worker
      // may use React to serialize UI descriptions for GPUI without react-dom.
      for (const argument of node.arguments) {
        if (!ts.isObjectLiteralExpression(argument)) continue
        const hasEntries = argument.properties.some(p => ts.isPropertyAssignment(p) && p.name.getText(ast).replaceAll(/["']/g, '') === 'entryPoints')
        if (!hasEntries) continue
        for (const property of argument.properties) {
          if (!ts.isPropertyAssignment(property)) continue
          const key = property.name.getText(ast).replaceAll(/["']/g, '')
          if (key === 'entryPoints' || key === 'alias') for (const value of strings(property.initializer)) localFile(value, file)
          if (key === 'platform' && strings(property.initializer).includes('browser')) fail('web-bundle', file, 'Application bundler targets a browser renderer')
        }
      }
    })
  }
  for (const name of entrypoints) if (REQUIRED.includes(name) || scripts[name] !== undefined) script(name)
  return { issues, scripts: [...seenScripts].sort(), sources: [...seenFiles].sort() }
}

export function forbiddenUiCrate(name) {
  const value = name.toLowerCase().replaceAll('_', '-')
  return /^(?:tauri(?:-|$)|wry$|tao$|web-?view(?:2)?(?:-|$)|webkit(?:2gtk|6)?(?:-|$)|javascriptcore-rs(?:-|$)|objc2-web-kit$|cef(?:-|$)|dioxus-desktop$)/.test(value)
}

/** Input is cargo tree --prefix none --format {p}, not Cargo.lock or source grep.
 * Includes normal transitive edges only; build/dev crates aren't binary UI proof.
 */
export function checkCargoTree(output) {
  const issues = [], names = new Set()
  for (const line of output.trim().split(/\r?\n/)) {
    const match = /^([a-zA-Z0-9_-]+) v\S+(?: .*)?$/.exec(line)
    if (!match) { issues.push(issue('cargo-output', 'Cargo normal tree', `Unrecognized cargo tree line: ${line}`)); continue }
    names.add(match[1])
  }
  for (const required of ['tezbar-native', 'gpui']) if (!names.has(required)) issues.push(issue('cargo-output', 'Cargo normal tree', `Missing ${required} in selected application tree`))
  for (const name of names) if (forbiddenUiCrate(name)) issues.push(issue('web-ui-crate', 'Cargo normal tree', `Forbidden runtime UI dependency: ${name}`))
  return { issues, packages: names.size }
}

export function cargoTreeArgs(target = 'all') {
  return ['tree', '--locked', '--offline', '--manifest-path', 'apps/tezbar/Cargo.toml', '--package', 'tezbar-native',
    '--edges', 'normal', '--prefix', 'none', '--format', '{p}', '--features', 'runtime-shaders',
    ...(target === 'host' ? [] : ['--target', target])]
}

/** Inject runner in tests; never build, run a GUI, fetch, sign or inspect binaries. */
export function checkCheckout({ root, target = 'all', run = spawnSync } = {}) {
  const issues = [], evidence = []
  const canonicalRoot = realpathSync(root ?? fileURLToPath(new URL('../..', import.meta.url)))
  const readSource = file => {
    const absolute = realpathSync(path.resolve(canonicalRoot, file))
    if (!absolute.startsWith(canonicalRoot + path.sep)) throw Error(`Source symlink escapes checkout: ${file}`)
    return readFileSync(absolute, 'utf8')
  }
  try {
    const result = checkEntrypoints(JSON.parse(readSource('package.json')), { readSource })
    issues.push(...result.issues)
    evidence.push(`Source routes: ${result.scripts.length} package scripts, ${result.sources.length} reachable JS/TS files`)
  } catch (error) { issues.push(issue('source-inspection', 'checkout', error.message)) }
  const result = run('cargo', cargoTreeArgs(target), { cwd: canonicalRoot, encoding: 'utf8', timeout: 60_000, maxBuffer: 16 * 1024 * 1024,
    env: { ...process.env, CARGO_NET_OFFLINE: 'true', CARGO_TERM_COLOR: 'never', RUSTUP_AUTO_INSTALL: '0' } })
  if (result.error || result.status !== 0) {
    issues.push(issue('cargo-unavailable', `Cargo normal tree (${target})`, `Unverified; offline locked inspection failed: ${result.error?.message ?? result.stderr?.trim() ?? `exit ${result.status}`}`))
  } else {
    const tree = checkCargoTree(result.stdout)
    issues.push(...tree.issues)
    evidence.push(`Cargo normal dependency graph (${target}, default features + runtime-shaders): ${tree.packages} packages`)
  }
  return { issues, evidence }
}

export function main(args = process.argv.slice(2)) {
  let target = 'all', root
  if (!args.includes('--check')) { console.error('Usage: node scripts/native/ui-policy.mjs --check [--target all|host|TRIPLE] [--root DIR]'); return 2 }
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--check') continue
    if (['--root', '--target'].includes(args[i]) && args[i + 1] && !args[i + 1].startsWith('--')) {
      if (args[i++] === '--root') root = args[i]; else target = args[i]
    } else { console.error(`Unknown/incomplete argument: ${args[i]}`); return 2 }
  }
  try {
    const result = checkCheckout({ root, target })
    for (const line of result.evidence) console.log(line)
    for (const item of result.issues) console.error(`[${item.code}] ${item.location}: ${item.message}`)
    console.log(`GPUI source/build policy ${result.issues.length ? 'FAIL' : 'PASS'}. Dependency/source evidence only; no binary link audit performed.`)
    return result.issues.length ? 1 : 0
  } catch (error) { console.error(`UI policy inspection failed: ${error.message}`); return 1 }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exitCode = main()
