/// <reference types="vite/client" />

import type { RaymesApi } from '../../../src/shared/desktop-api'

declare global {
  interface Window {
    tezbar: RaymesApi
    __TEZBAR_WINDOW_LABEL__?: string
  }
}

export { }
