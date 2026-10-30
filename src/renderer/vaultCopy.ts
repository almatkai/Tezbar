/** Copies a vault secret from a search result, asking for Touch ID first when
 *  the vault is locked. Returns the message to show, or null if cancelled. */
export async function copyVaultSecret(id: string): Promise<string | null> {
  try {
    const status = await window.tezbar.vaultStatus()
    if (!status.unlocked) {
      const unlocked = await window.tezbar.vaultUnlock()
      if (!unlocked.ok) return null
    }
    await window.tezbar.vaultCopy(id)
    return 'Copied · clears in 30 s'
  } catch (err) {
    return err instanceof Error ? err.message : 'Could not copy the secret'
  }
}
