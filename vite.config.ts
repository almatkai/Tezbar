import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { resolve } from 'node:path'
import { readFileSync } from 'node:fs'

const appVersion: string = JSON.parse(
  readFileSync(resolve(__dirname, 'package.json'), 'utf8')
).version
// Every installer carries its own app notes, including when the user is offline.
const appReleaseNotes = readFileSync(
  resolve(__dirname, 'docs/releases', `${appVersion}.md`),
  'utf8'
)

export default defineConfig({
  root: resolve(__dirname, 'src/renderer'),
  plugins: [react()],
  define: {
    'import.meta.env.VITE_APP_VERSION': JSON.stringify(appVersion),
    'import.meta.env.VITE_APP_RELEASE_NOTES': JSON.stringify(appReleaseNotes),
  },
  server: {
    host: '127.0.0.1',
    port: 5174,
    strictPort: true,
  },
  build: {
    outDir: resolve(__dirname, 'dist'),
    emptyOutDir: true,
  },
})
