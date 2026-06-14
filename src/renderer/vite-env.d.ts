/// <reference types="vite/client" />

import type { RaymesApi } from '../preload/api'

declare global {
  interface Window {
    tezbar: RaymesApi
  }
}

export { }
