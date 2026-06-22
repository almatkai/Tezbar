import { afterEach, describe, expect, it, vi } from 'vitest'

import { app } from './electron-shim'

describe('Tauri Electron app shim', () => {
  const previousIsTauri = process.env.IS_TAURI

  afterEach(() => {
    vi.restoreAllMocks()
    if (previousIsTauri === undefined) delete process.env.IS_TAURI
    else process.env.IS_TAURI = previousIsTauri
  })

  it('asks the Tauri host to hide and restore visible app windows', () => {
    process.env.IS_TAURI = 'true'
    const write = vi.spyOn(process.stdout, 'write').mockImplementation(() => true)

    app.hide()
    app.show()

    expect(write).toHaveBeenNthCalledWith(
      1,
      `${JSON.stringify({ type: 'app_visibility', visible: false })}\n`,
    )
    expect(write).toHaveBeenNthCalledWith(
      2,
      `${JSON.stringify({ type: 'app_visibility', visible: true })}\n`,
    )
  })
})
