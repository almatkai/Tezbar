/// <reference types="vite/client" />

import type { RaymesApi } from '../shared/desktop-api'

declare global {
  interface Window {
    tezbar: RaymesApi
    __TEZBAR_WINDOW_LABEL__?: string
  }
}

export { }
