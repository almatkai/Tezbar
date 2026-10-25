import type { LauncherCommandMetadata } from '../shared/search'
type Validation = NonNullable<LauncherCommandMetadata['inlineArgument']>['validation']
export function sanitizeLauncherArgument(text: string, validation?: Validation): string {
  return validation?.kind === 'integer'
    ? text.replace(/[^\d]/g, '').slice(0, validation.maxLength)
    : text
}
export function validateLauncherArgument(text: string, validation?: Validation): string | null {
  if (!validation) return text ? null : 'Enter an argument'
  const value = Number(text)
  return /^\d+$/.test(text) &&
    text.length <= validation.maxLength &&
    Number.isSafeInteger(value) &&
    value >= validation.min &&
    value <= validation.max
    ? null
    : validation.message
}
