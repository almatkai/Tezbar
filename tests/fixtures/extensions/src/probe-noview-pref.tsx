import { Clipboard } from '@raycast/api'
export default async function Command() { await Clipboard.copy('probe side effect') }
