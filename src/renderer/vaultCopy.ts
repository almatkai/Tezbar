export type VaultCopyOutcome =
  | { status: 'copied'; message: string }
  | { status: 'cancelled' }
  | { status: 'error'; message: string }

/** Copies a vault secret from a search result, asking for Touch ID first when
 *  the vault is locked. `onPrompt` fires right before the system prompt. */
export async function copyVaultSecret(id: string, onPrompt?: () => void): Promise<VaultCopyOutcome> {
  try {
    const status = await window.tezbar.vaultStatus()
    if (!status.unlocked) {
      onPrompt?.()
      const unlocked = await window.tezbar.vaultUnlock()
      if (!unlocked.ok) return { status: 'cancelled' }
    }
    await window.tezbar.vaultCopy(id)
    return { status: 'copied', message: 'Copied · clears in 30 s' }
  } catch (err) {
    return { status: 'error', message: err instanceof Error ? err.message : 'Could not copy the secret' }
  }
}
