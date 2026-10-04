import { dialog } from '@tezbar/desktop-runtime'
export type ExtensionAlertOptions = {
  title?: string
  message?: string
  primaryAction?: { title?: string; style?: string; onAction?: () => unknown }
  dismissAction?: { title?: string; onAction?: () => unknown }
}
/** Never approve a destructive action without an explicit user response. */
export async function confirmExtensionAlert(options: ExtensionAlertOptions = {}): Promise<boolean> {
  const result = await dialog.showMessageBox({
    title: String(options.title || 'Confirm'),
    message: String(options.title || 'Confirm'),
    detail: options.message ? String(options.message) : undefined,
    buttons: [
      String(options.dismissAction?.title || 'Cancel'),
      String(options.primaryAction?.title || 'OK'),
    ],
    defaultId: 0,
    cancelId: 0,
    type: options.primaryAction?.style === 'destructive' ? 'warning' : 'question',
    noLink: true,
  })
  const accepted = result.response === 1
  const callback = accepted ? options.primaryAction?.onAction : options.dismissAction?.onAction
  if (callback) await Promise.resolve(callback())
  return accepted
}
