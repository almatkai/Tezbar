// M11 source-only planner. Importing it does no I/O; CLI assembly/signing is deliberately absent.
import { createHash } from 'node:crypto'
import { createReadStream } from 'node:fs'
import { lstat, readdir, realpath } from 'node:fs/promises'
import path from 'node:path'
import { assertRelease, APP_IDS, PRODUCT_NAMES } from '../lib/release-policy.mjs'
import { assertEmbeddingRuntimeDistribution } from '../lib/embedding-runtime-policy.mjs'

export const GPUI_REV = 'c84af5b99cbc366d675064e52b23664a015b6e35'
export const RESOURCE_ROOTS = Object.freeze([
  ['dist-backend/extension-worker.js', 'dist-backend/extension-worker.js'],
  ['dist-backend/vec0', 'dist-backend/vec0'],
  ['dist-backend/libsqlite3.dylib', 'dist-backend/libsqlite3.dylib'],
  ['dist-backend/node_modules', 'dist-backend/node_modules'],
  ['resources/icons', 'dist-backend/resources/icons'],
  ['build/embedding-runtime/resources', 'embedding-runtime'],
  ['native/axhelper/axhelper', 'native/axhelper/axhelper'],
  ['native/screenocr/screenocr-helper', 'native/screenocr/screenocr-helper'],
  ['native/color-picker/color-picker-helper', 'native/color-picker/color-picker-helper'],
  ['native/image-colors/image-colors-helper', 'native/image-colors/image-colors-helper'],
  ['native/color-picker/windows.ps1', 'native/color-picker/windows.ps1'],
  ['native/image-colors/windows.ps1', 'native/image-colors/windows.ps1'],
  ['node_modules/esbuild', 'node_modules/esbuild'], ['node_modules/@esbuild', 'node_modules/@esbuild'],
  ['licenses/raycast-extensions-MIT.txt', 'licenses/raycast-extensions-MIT.txt'],
  ['resources/trayIconTemplate@2x.png', 'trayIconTemplate@2x.png'],
  ['src-tauri/icons/icon.icns', 'icon.icns'],
])
const sha = (bytes) => createHash('sha256').update(bytes).digest('hex')
function relativeName(value) {
  if (typeof value !== 'string' || !value || value.includes('\\') || value.includes('\0') || value.split('/').some(p => !p || p === '.' || p === '..') || path.posix.isAbsolute(value))
    throw new Error('Resource destination must be a bounded relative path')
  return value
}
const inside = (parent, child) => { const p = path.relative(parent, child); return p && !p.startsWith(`..${path.sep}`) && p !== '..' && !path.isAbsolute(p) }
export function validateInventory(files) {
  if (!Array.isArray(files) || files.length > 100000) throw new Error('Resource inventory limit')
  const names = new Set()
  for (const file of files) {
    relativeName(file.destination)
    const folded = file.destination.toLowerCase()
    if (names.has(folded)) throw new Error('Resource destination collision')
    names.add(folded)
    if (!Number.isSafeInteger(file.bytes) || file.bytes < 0 || !/^[a-f0-9]{64}$/.test(file.sha256) || !path.isAbsolute(file.source) || !['data', 'executable', 'mach-o'].includes(file.kind))
      throw new Error('Incomplete resource pin')
  }
  // Reject file/directory prefix collisions before assembly.
  for (const name of names) {
    const parts = name.split('/')
    for (let i = 1; i < parts.length; i++) if (names.has(parts.slice(0, i).join('/'))) throw new Error('Resource prefix collision')
  }
  return files
}
/** Future build-tool capture only: stream hashing, no subprocesses. Explicit roots only.
 * pnpm symlinks must be dereferenced into reviewed staging first, never silently followed. */
export async function inventoryResources(root, roots = RESOURCE_ROOTS) {
  const files = []
  let entries = 0
  async function visit(source, destination, depth = 0) {
    if (depth > 32 || ++entries > 100000) throw new Error('Resource traversal limit')
    const info = await lstat(source)
    if (info.isSymbolicLink()) throw new Error('Stage symlink-free resources before capture')
    if (info.isDirectory()) {
      for (const name of (await readdir(source)).sort()) await visit(path.join(source, name), `${destination}/${name}`, depth + 1)
    } else if (info.isFile()) {
      const hash = createHash('sha256')
      let bytes = 0, magic = Buffer.alloc(0)
      for await (const block of createReadStream(source)) { bytes += block.length; hash.update(block); if (magic.length < 4) magic = Buffer.concat([magic, block]).subarray(0, 4) }
      const after = await lstat(source)
      if (info.size !== bytes || info.mtimeMs !== after.mtimeMs || info.ino !== after.ino) throw new Error('Resource changed during capture')
      const native = ['feedface','feedfacf','cefaedfe','cffaedfe','cafebabe','bebafeca','cafebabf','bfbafeca'].includes(magic.toString('hex'))
      files.push({ source: await realpath(source), destination, bytes, sha256: hash.digest('hex'), kind: native ? 'mach-o' : info.mode & 0o111 ? 'executable' : 'data' })
    } else throw new Error('Resource is not a regular file')
  }
  for (const [from, to] of roots) await visit(path.resolve(root, from), relativeName(to))
  return validateInventory(files)
}
export function renderInfoPlist(template, permissions, { identifier, version }) {
  if (!['com.tezbar.app','com.tezbar.preview.native'].includes(identifier)) throw new Error('Unexpected bundle identity')
  // Values accepted by assertRelease cannot contain XML metacharacters.
  if (!/^\d+\.\d+\.\d+(?:-(?:alpha|beta)\.\d+)?$/.test(version)) throw new Error('Invalid version')
  const dict = permissions.match(/<dict>([\s\S]*?)<\/dict>/)?.[1]
  if (!dict || /LSUIElement|ActivationPolicy/.test(template + permissions)) throw new Error('Invalid permission merge')
  const expected = ['NSMicrophoneUsageDescription','NSAppleEventsUsageDescription','NSCalendarsUsageDescription','NSCalendarsFullAccessUsageDescription']
  const keys = [...dict.matchAll(/<key>([^<]+)<\/key>/g)].map(m => m[1])
  if (keys.length !== expected.length || expected.some(k => !keys.includes(k))) throw new Error('Permission policy changed; review source')
  const output = template.replaceAll('{{IDENTIFIER}}', identifier).replaceAll('{{VERSION}}', version).replace('{{PERMISSIONS}}', dict.trim())
  if (/{{/.test(output)) throw new Error('Unresolved plist template')
  return output
}
/** All arguments are explicit captured inputs, never environment-derived identity overrides.
 * A plan is NOT a receipt or authorization to build/sign. Existing release gates still apply. */
export function planMacos(input) {
  const { version, channel, localPreview, stage, osTemp, executable, files, policy, catalog, payload, sourceCommit, allowUnsigned = false } = input
  assertRelease(channel, version)
  if (!path.isAbsolute(stage) || !path.isAbsolute(osTemp) || !inside(osTemp, stage)) throw new Error('Use a fresh OS-temp assembly directory')
  if (!/^[a-f0-9]{40}$/.test(sourceCommit) || input.gpuiRev !== GPUI_REV || input.rustVersion !== '1.98.1' || input.benchControl !== false) throw new Error('Unreviewed native build provenance')
  if (policy.identifier !== 'com.tezbar.app' || !policy.teamIdentifier || !policy.designatedRequirement?.includes('certificate') || !Array.isArray(policy.nativeHelpers) || policy.nativeHelpers.length !== 4) throw new Error('Missing original signing policy')
  if (allowUnsigned && (localPreview || channel === 'stable')) throw new Error('Invalid release exception')
  const identifier = localPreview ? 'com.tezbar.preview.native' : APP_IDS[channel]
  validateInventory(files)
  validateInventory([{ ...executable, destination: 'tezbar' }])
  if (executable.kind !== 'mach-o') throw new Error('Native executable must be Mach-O')
  for (const file of files) if (!RESOURCE_ROOTS.some(([,dest]) => file.destination === dest || file.destination.startsWith(`${dest}/`))) throw new Error('Undeclared resource root')
  for (const [,dest] of RESOURCE_ROOTS) if (!files.some(f => f.destination === dest || f.destination.startsWith(`${dest}/`))) throw new Error(`Missing declared resource ${dest}`)
  for (const helper of policy.nativeHelpers) if (!files.some(f => f.destination === helper && f.kind === 'mach-o')) throw new Error('Missing native helper')
  const payloadFile = files.find(f => f.destination === 'embedding-runtime/embedding-runtime.zip')
  if (!payloadFile || payloadFile.bytes !== catalog.archiveBytes || payloadFile.sha256 !== catalog.archiveSha256 || payload.bytes !== catalog.archiveBytes || payload.sha256 !== catalog.archiveSha256) throw new Error('Pinned embedding payload mismatch')
  assertEmbeddingRuntimeDistribution({ platform: 'darwin', arch: 'arm64', unsigned: localPreview || allowUnsigned, catalog })
  const ordered = files.toSorted((a,b) => a.destination.localeCompare(b.destination, 'en'))
  const inventorySha256 = sha(JSON.stringify(ordered.map(({source, ...pin}) => pin)))
  return {
    schema: 1, kind: 'native-assembly-plan', executable: 'tezbar', product: PRODUCT_NAMES[channel], identifier, version, channel, sourceCommit,
    bundle: path.join(stage, 'Tezbar.app'), installedTarget: null,
    inventorySha256, files: ordered, binary: executable, sourceDirty: input.sourceDirty !== false, gpuiRev: GPUI_REV, rustVersion: input.rustVersion,
    workerPolicy: 'Bun compatibility is lazy and core-owned; no first-party JS runtime',
    modelPolicy: 'Pinned runtime ZIP only; user model caches never copied',
    signing: localPreview ? { mode:'ad-hoc', explicitLocalPreview:true, publishable:false } : { mode:'existing-release-pipeline', policy, allowUnsigned, required:['sign native resources', 'verify every Mach-O Team ID', 'seal outer app', 'verify deep strict and exact pinned designated requirement', 'minisign archive in release pipeline'] },
    receipt: { state:'NOT_BUILT', inventorySha256, sourceCommit, localPreview, identityContinuityVerified:false, platformVerified:false },
    gates: ['release-guidelines inventory and exact build SHA', 'fresh non-symlink stage', 'recapture inventory immediately before copy', 'copy + rehash every entry', 'runtime/native signature checks', 'dual-platform receipt and updater continuity'],
  }
}

/** Unsealed assembly implementation for a LATER authorized build. Never invoked at import.
 * No signing/key access/archive/install is implemented here. Parent invokes the existing
 * release pipeline guards around this function; returned output is never publishable. */
export async function assembleMacos(plan, { template, permissions, osTemp }) {
  const { mkdir, open, writeFile } = await import('node:fs/promises')
  const { pipeline } = await import('node:stream/promises')
  const { createWriteStream } = await import('node:fs')
  if (plan.kind !== 'native-assembly-plan' || plan.receipt.state !== 'NOT_BUILT') throw new Error('Assembly requires a reviewed native plan')
  if (!path.isAbsolute(osTemp) || !inside(await realpath(osTemp), path.dirname(plan.bundle))) throw new Error('Assembly stage must be inside canonical OS temp')
  const stage = path.dirname(plan.bundle)
  // Parent directory must exist, be canonical, and be inside OS-temp. Never reuse a stage.
  if (await realpath(path.dirname(stage)) !== path.dirname(stage)) throw new Error('Symlinked assembly parent')
  await mkdir(stage, { mode: 0o700 })
  const contents = path.join(plan.bundle, 'Contents'), resources = path.join(contents, 'Resources')
  await mkdir(path.join(contents,'MacOS'), { recursive:true, mode:0o700 })
  await mkdir(resources, { mode:0o700 })
  const copied = []
  async function copyPinned(pin, target) {
    const stat = await lstat(pin.source)
    if (!stat.isFile() || stat.isSymbolicLink()) throw new Error('Only regular reviewed input files can be copied')
    await mkdir(path.dirname(target), { recursive:true, mode:0o700 })
    const input = await open(pin.source, 'r')
    try {
      if ((await input.stat()).ino !== stat.ino) throw new Error('Source replaced before copy')
      await pipeline(input.createReadStream({ autoClose:false }), createWriteStream(target, { flags:'wx', mode:pin.kind === 'data' ? 0o644 : 0o755 }))
    } finally { await input.close() }
    const hash=createHash('sha256'); let bytes=0
    for await (const block of createReadStream(target)) { hash.update(block);bytes+=block.length }
    const digest=hash.digest('hex')
    if (bytes!==pin.bytes || digest!==pin.sha256) throw new Error('Copied bytes do not match resource pin; retain failed stage for inspection')
    copied.push({ path:path.relative(contents,target),bytes,sha256:digest })
  }
  await copyPinned(plan.binary,path.join(contents,'MacOS/tezbar'))
  for (const file of validateInventory(plan.files)) await copyPinned(file,path.join(resources,file.destination))
  const infoPlist = renderInfoPlist(template, permissions, plan)
  if (/LSUIElement|{{/.test(infoPlist) || !infoPlist.includes(`<string>${plan.identifier}</string>`) || !infoPlist.includes('<string>tezbar</string>')) throw new Error('Unreviewed Info.plist')
  await writeFile(path.join(contents,'Info.plist'),infoPlist,{flag:'wx',mode:0o644})
  copied.push({path:'Info.plist',bytes:Buffer.byteLength(infoPlist),sha256:sha(infoPlist)})
  const provenance={sourceCommit:plan.sourceCommit,dirty:plan.sourceDirty,gpuiRev:plan.gpuiRev,rustVersion:plan.rustVersion,files:plan.files.map(({source,...pin})=>pin)}
  await writeFile(path.join(resources,'.tezbar-native-source'),plan.sourceCommit+'\n',{flag:'wx',mode:0o644})
  await writeFile(path.join(resources,'.tezbar-native-production.json'),JSON.stringify(provenance,null,2)+'\n',{flag:'wx',mode:0o644})
  return { ...plan.receipt, sourceDirty:plan.sourceDirty, state:'ASSEMBLED_UNSEALED', publishable:false, bundle:plan.bundle, files:copied }
}
