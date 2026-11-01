export type VaultKind = 'api-key' | 'token' | 'ssh-key' | 'server' | 'password' | 'note'

export type VaultItem = {
  id: string
  name: string
  value: string
  kind: VaultKind
  note?: string
  createdAt: number
  updatedAt: number
}

/** List entries never carry the value or the note. */
export type VaultItemHeader = Omit<VaultItem, 'value' | 'note'>

export type VaultStatus = {
  supported: boolean
  unlocked: boolean
  itemCount: number
}

export type VaultUnlockResult = { ok: boolean; cancelled?: boolean }

export type VaultSaveInput = {
  id?: string
  name: string
  value: string
  kind: VaultKind
  note?: string
}

/** A value handed to the Vault editor, e.g. from Clipboard History. */
export type VaultDraftSeed = {
  value: string
  /** Clipboard history entry to remove once the value is in the Vault. */
  clipboardEntryId?: string
}

/** Dispatched from App when ⌘N fires while the Vault surface is open. */
export const TEZBAR_NEW_VAULT_ITEM_EVENT = 'tezbar:new-vault-item'
