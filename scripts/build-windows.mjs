// Windows installer entrypoint: same guarded native GPUI release pipeline.
import { nativeRelease } from './native/release.mjs'
if (process.platform !== 'win32' && !(process.argv.length === 3 && process.argv[2] === '--help')) throw Error('The native Windows installer must be built on Windows x64; no cross-platform or Tauri fallback')
await nativeRelease(process.argv.slice(2))
