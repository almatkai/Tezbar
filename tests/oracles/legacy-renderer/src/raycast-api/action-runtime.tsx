export { ActionPanelOverlay } from './action-runtime-overlay'

function EmptyComponent(): null {
  return null
}

export const ActionPanel = EmptyComponent

export const Action = Object.assign(EmptyComponent, {
  CopyToClipboard: EmptyComponent,
  OpenInBrowser: EmptyComponent,
  Push: EmptyComponent,
  Pop: EmptyComponent,
  ShowInFinder: EmptyComponent,
  SubmitForm: EmptyComponent,
})
