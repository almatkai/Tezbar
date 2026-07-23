import { execFile } from 'node:child_process'
import { access } from 'node:fs/promises'
import { basename } from 'node:path'
import { promisify } from 'node:util'
import type { SearchExecuteResult } from '../shared/search'
import { isQuickLookPreviewablePath } from '../shared/quickLook'

const execFileAsync = promisify(execFile)

export async function quickLookFiles(paths: string[]): Promise<SearchExecuteResult> {
  const requestedTargets = [...new Set(paths.map((path) => path.trim()))].filter(
    (path) => path && isQuickLookPreviewablePath(path)
  )
  if (requestedTargets.length === 0) {
    return { ok: false, message: 'The selected item cannot be previewed with Quick Look' }
  }
  if (process.platform !== 'darwin') {
    return { ok: false, message: 'Quick Look is only available on macOS' }
  }

  const availableTargets = (
    await Promise.all(
      requestedTargets.map(async (target) => {
        try {
          await access(target)
          return target
        } catch {
          return null
        }
      })
    )
  ).filter((target): target is string => target !== null)
  if (availableTargets.length === 0) {
    return { ok: false, message: 'The selected file no longer exists' }
  }

  try {
    // qlmanage owns the native preview panel until the user closes it. Awaiting
    // it lets the renderer keep blur-hide suppressed for the panel's lifetime.
    await execFileAsync('/usr/bin/qlmanage', ['-p', ...availableTargets])
    return { ok: true, message: `Previewed ${basename(availableTargets[0])}` }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error || '')
    return {
      ok: false,
      message: message.trim() ? `Could not preview file: ${message}` : 'Could not preview file',
    }
  }
}
