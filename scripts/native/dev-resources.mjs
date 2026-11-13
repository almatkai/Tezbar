// Local preview inputs only. No production manifest, bundle, signing or installation.
import * as fs from 'node:fs/promises'
import path from 'node:path'

export const PROFILE_MARKER = 'com.tezbar.preview.native\n'
export const UI_INPUTS = Object.freeze([
  ['icons', 'dist-backend/resources/icons'],
  ['trayIconTemplate@2x.png', 'trayIconTemplate@2x.png'],
])
const inside = (parent, child) => {
  const relative = path.relative(parent, child)
  return relative !== '' && relative !== '..' && !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative)
}

// Reject links (including junctions on Windows), devices and unbounded trees.
export async function assertResourceTree(directory, io = fs) {
  let count = 0
  async function visit(file, depth) {
    if (++count > 100000 || depth > 32) throw Error('Preview resource traversal limit')
    const stat = await io.lstat(file)
    if (stat.isSymbolicLink()) throw Error(`Symlink in preview resources: ${file}`)
    if (stat.isDirectory()) {
      for (const name of await io.readdir(file)) await visit(path.join(file, name), depth + 1)
    } else if (!stat.isFile()) throw Error(`Non-regular preview resource: ${file}`)
  }
  if (!(await io.lstat(directory)).isDirectory()) throw Error('Preview resources must be a directory (not a symlink)')
  await visit(directory, 0)
}

export async function validateResources(options, {root, sourceCommit, io = fs}) {
  if (options.resources) {
    await assertResourceTree(options.resources, io)
    if ((await io.readFile(path.join(options.resources, '.tezbar-native-source'), 'utf8')).trim() !== sourceCommit)
      throw Error('Prepared preview resources source differs from current Git HEAD')
    return {mode:'prepared-local', source:await io.realpath(options.resources)}
  }
  const source = path.resolve(root, options.uiResources ?? 'resources')
  await assertResourceTree(source, io)
  const canonical = await io.realpath(source)
  if (!inside(await io.realpath(root), canonical)) throw Error('UI-only resources must be inside this source workspace')
  for (const [from] of UI_INPUTS) {
    const stat = await io.lstat(path.join(source, from))
    if (from === 'icons' ? !stat.isDirectory() : !stat.isFile()) throw Error(`Missing UI-only input: ${from}`)
  }
  return {mode:'minimal-ui-only', source:canonical}
}

/** Always creates a fresh sibling profile/resources pair under canonical OS temp.
 * Prepared inputs stay untouched; minimal inputs are copied to the native layout.
 * The HEAD marker is preview compatibility, NOT production provenance. */
export async function preparePreview(input, {sourceCommit, osTemp, io = fs}) {
  const temp = await io.realpath(osTemp)
  const stage = await io.mkdtemp(path.join(temp, 'tezbar-native-dev-'))
  try {
    const profile = path.join(stage, 'profile')
    await io.mkdir(profile, {mode:0o700})
    await io.writeFile(path.join(profile, '.tezbar-native-preview'), PROFILE_MARKER, {flag:'wx', mode:0o600})
    let resources = input.source
    if (input.mode === 'minimal-ui-only') {
      resources = path.join(stage, 'resources')
      await io.mkdir(resources, {mode:0o700})
      for (const [from, to] of UI_INPUTS) {
        await io.mkdir(path.dirname(path.join(resources, to)), {recursive:true, mode:0o700})
        await io.cp(path.join(input.source, from), path.join(resources, to), {recursive:true, force:false, errorOnExist:true, dereference:false})
      }
      // Revalidate the copied tree: links introduced during copying also fail closed.
      await assertResourceTree(resources, io)
      await io.writeFile(path.join(resources, '.tezbar-native-source'), sourceCommit + '\n', {flag:'wx', mode:0o600})
    }
    return {stage, profile, resources}
  } catch (error) {
    await io.rm(stage, {recursive:true, force:true})
    throw error
  }
}
