// Standard release entrypoint: native GPUI only, never a Tauri/WebView build.
import { nativeRelease } from './native/release.mjs'
await nativeRelease(process.argv.slice(2))
