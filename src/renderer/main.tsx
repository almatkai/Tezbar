import { createRoot } from 'react-dom/client'
import { lazy, Suspense } from 'react'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import BackendConnection from './BackendConnection'
import 'highlight.js/styles/atom-one-dark.css'
import './styles.css'
import { initTauriBridge } from './tauri-bridge'

const App = lazy(() => import('./App'))

const rootElement = document.getElementById('root')

// WebView2 consumes mouse events inside CSS app drag regions before React can
// start Tezbar's center-snapping drag. Windows uses the native host's manual
// cursor tracking instead, so mark the document for the CSS override below.
if (navigator.platform.includes('Win')) {
  document.documentElement.classList.add('platform-windows')
}

if (!rootElement) {
  throw new Error('Tezbar renderer root element is missing')
}

// Bridge setup only installs the native API and event subscriptions. It must
// never gate the first React paint: a newly-created Tauri WebView can briefly
// expose incomplete window metadata while WebView2 is starting.
try {
  initTauriBridge()
} catch (error: unknown) {
  console.error('Failed to initialize the Tauri bridge:', error)
}

// WebView2 keeps `document.visibilityState === "visible"` after Tauri hides a
// native window. Without an explicit signal, animations and transparent
// compositing continue consuming GPU/CPU while the launcher is idle.
async function syncMainWindowRendering(): Promise<void> {
  if (!('__TAURI_INTERNALS__' in window)) return
  const appWindow = getCurrentWindow()
  if (appWindow.label !== 'main') return

  let hiddenCheck: ReturnType<typeof setTimeout> | undefined
  const setVisible = (visible: boolean): void => {
    document.documentElement.classList.toggle('tezbar-window-hidden', !visible)
  }
  const readVisibility = async (): Promise<void> => {
    try {
      setVisible(await appWindow.isVisible())
    } catch (error) {
      console.warn('[Tezbar] Failed to read native window visibility:', error)
    }
  }

  await readVisibility()
  const stopShown = await listen('window-shown', () => setVisible(true))
  const stopVisibility = await listen<boolean>('window-visibility', (event) => {
    setVisible(event.payload)
  })
  const stopFocus = await appWindow.onFocusChanged(({ payload: focused }) => {
    clearTimeout(hiddenCheck)
    if (focused) {
      setVisible(true)
      return
    }
    // The native blur handler may hide the launcher after a short grace
    // period, so read visibility once that decision has settled.
    hiddenCheck = setTimeout(() => {
      void readVisibility()
    }, 180)
  })

  window.addEventListener(
    'beforeunload',
    () => {
      clearTimeout(hiddenCheck)
      stopShown()
      stopVisibility()
      stopFocus()
    },
    { once: true }
  )
}

void syncMainWindowRendering().catch((error: unknown) => {
  console.warn('[Tezbar] Failed to synchronize native window rendering:', error)
})

// Native IPC effects are not safe to replay. React StrictMode deliberately
// mounts effects twice in development, which duplicates every startup request
// in each WebView and can make opening Settings look like a backend freeze.
const loading = <div className="glass-shell h-screen p-6 text-[13px] text-ink-2" role="status">Loading Tezbar…</div>
const app = <Suspense fallback={loading}><App /></Suspense>
const isOverlay = new URLSearchParams(window.location.search).get('window') === 'snap-overlay'
createRoot(rootElement).render(isOverlay ? app : <BackendConnection>{app}</BackendConnection>)
