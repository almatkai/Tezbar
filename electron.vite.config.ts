import { defineConfig, externalizeDepsPlugin } from 'electron-vite'
import react from '@vitejs/plugin-react'
import { resolve } from 'node:path'

export default defineConfig({
  main: {
    plugins: [externalizeDepsPlugin()],
  },
  preload: {
    plugins: [externalizeDepsPlugin()],
  },
  renderer: {
    root: resolve(__dirname, 'src/renderer'),
    plugins: [react()],
    // In some sandboxed environments, binding to IPv6 loopback (::1) is not permitted.
    // Force the dev server to bind to IPv4 loopback so Electron can load the renderer.
    server: {
      host: '127.0.0.1',
    },
  },
})
