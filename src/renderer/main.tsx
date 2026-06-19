import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import 'highlight.js/styles/atom-one-dark.css'
import './styles.css'
import { initTauriBridge } from './tauri-bridge'

void initTauriBridge().catch((error: unknown) => {
  console.error('Failed to initialize the Tauri bridge:', error)
}).finally(() => {
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <App />
    </StrictMode>
  )
})
