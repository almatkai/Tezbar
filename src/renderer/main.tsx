import { createRoot } from 'react-dom/client'
import App from './App'
import 'highlight.js/styles/atom-one-dark.css'
import './styles.css'
import { initTauriBridge } from './tauri-bridge'

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

// Native IPC effects are not safe to replay. React StrictMode deliberately
// mounts effects twice in development, which duplicates every startup request
// in each WebView and can make opening Settings look like a backend freeze.
createRoot(rootElement).render(<App />)
