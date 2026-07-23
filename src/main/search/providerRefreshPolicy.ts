const VOLATILE_SEARCH_PROVIDER_IDS = new Set([
  'commands',
  'clipboard',
  'notes',
  'snippets',
  'quick-links',
])

/**
 * Providers in this group are cheap and reflect data that can change while
 * Raymes is running. Files, apps, and extensions have their own lifecycle and
 * must not be pulled into the interactive refresh path.
 */
export function selectVolatileSearchProviders<T extends { providerId: string }>(
  providers: readonly T[]
): T[] {
  return providers.filter((provider) => VOLATILE_SEARCH_PROVIDER_IDS.has(provider.providerId))
}
