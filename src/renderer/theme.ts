export type TezbarThemePreference = 'white' | 'dark' | 'system'
export type ResolvedTezbarTheme = 'white' | 'dark'

export const TEZBAR_THEME_STORAGE_KEY = 'tezbar:theme'

export function isTezbarThemePreference(value: unknown): value is TezbarThemePreference {
  return value === 'white' || value === 'dark' || value === 'system'
}

export function resolveTezbarTheme(
  preference: TezbarThemePreference,
  systemPrefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches
): ResolvedTezbarTheme {
  if (preference !== 'system') return preference
  return systemPrefersDark ? 'dark' : 'white'
}

export function applyTezbarTheme(preference: TezbarThemePreference): ResolvedTezbarTheme {
  const resolved = resolveTezbarTheme(preference)
  document.documentElement.dataset.theme = resolved
  document.documentElement.style.colorScheme = resolved === 'white' ? 'light' : 'dark'
  return resolved
}
