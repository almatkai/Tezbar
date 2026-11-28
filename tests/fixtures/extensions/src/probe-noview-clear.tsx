import { clearSearchBar, showHUD } from '@raycast/api'
export default async function Command() { await clearSearchBar(); await showHUD('Probe cleared') }
