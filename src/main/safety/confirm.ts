import { BrowserWindow, dialog } from '@tezbar/desktop-runtime'
import type { MessageBoxOptions } from '@tezbar/desktop-runtime'
import type { SafetyConfirmResult, SafetyDescriptor } from '../../shared/safety'
import { getSafetyDescriptor } from './registry'

const RISK_LABEL: Record<SafetyDescriptor['risk'], string> = {
  low: 'Low risk',
  medium: 'Use with care',
  high: 'Destructive',
}

export async function confirmSafetyAction(
  window: BrowserWindow | null,
  descriptor: SafetyDescriptor,
  context?: Record<string, unknown>,
  options?: { dryRun?: boolean },
): Promise<SafetyConfirmResult> {
  // Verify the descriptor is registered — refuse unknown actions regardless
  // of how the caller obtained the payload.
  if (!getSafetyDescriptor(descriptor.id)) {
    return { accepted: false }
  }

  // Low-risk actions can still surface a dialog when dry-run is on — the
  // whole point of dry-run is to review what would happen. Outside dry-run
  // mode we keep them silent by honoring `requiresConfirmation`.
  if (!descriptor.requiresConfirmation && !options?.dryRun) {
    return { accepted: true }
  }

  const detailLines: string[] = []
  if (options?.dryRun) {
    detailLines.push('Dry-run mode: no changes will be made.')
  }
  if (descriptor.details) detailLines.push(descriptor.details)
  if (context) {
    for (const [key, value] of Object.entries(context)) {
      if (value === undefined || value === null || value === '') continue
      detailLines.push(`${key}: ${String(value)}`)
    }
  }
  detailLines.push(`Risk: ${RISK_LABEL[descriptor.risk]}`)

  const primaryLabel = options?.dryRun ? `Preview: ${descriptor.title}` : descriptor.title

  const opts: MessageBoxOptions = {
    type: descriptor.risk === 'high' && !options?.dryRun ? 'warning' : 'question',
    buttons: ['Cancel', primaryLabel],
    defaultId: 0,
    cancelId: 0,
    title: primaryLabel,
    message: descriptor.summary,
    detail: detailLines.join('\n'),
    noLink: true,
  }

  const response = window && !window.isDestroyed()
    ? await dialog.showMessageBox(window, opts)
    : await dialog.showMessageBox(opts)

  return { accepted: response.response === 1 }
}
