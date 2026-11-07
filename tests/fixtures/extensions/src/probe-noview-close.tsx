import { closeMainWindow, showHUD } from '@raycast/api'
export default async function Command() { await closeMainWindow(); await showHUD('Probe closed') }
