// Diagnostic build: retain current Rust, replace only image ownership modules
// with their frozen pre-change versions, without mutating source or a checkout.
import base from '../../vite.config'
import { execFileSync } from 'node:child_process'
import { resolve } from 'node:path'
import { defineConfig } from 'vite'
const directory = process.env.TEZBAR_HYBRID_DIST
if (!directory) throw new Error('Set TEZBAR_HYBRID_DIST to an artifact directory')
const paths = ['src/renderer/ExtensionsView.tsx', 'src/renderer/extensionImages.ts']
const modules = new Map(paths.map((path) => [resolve(path), execFileSync('git', ['show', `354689e:${path}`], { encoding: 'utf8' })]))
export default defineConfig({
  ...base,
  plugins: [{ name: 'frozen-image-modules', enforce: 'pre', load(id) { return modules.get(id) } }, ...(base.plugins ?? [])],
  build: { ...base.build, outDir: directory, emptyOutDir: true },
})
