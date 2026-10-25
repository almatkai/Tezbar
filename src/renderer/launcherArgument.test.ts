import { describe, expect, it } from 'vitest'
import { sanitizeLauncherArgument, validateLauncherArgument } from './launcherArgument'
const validation = {
  kind: 'integer' as const,
  min: 1,
  max: 65535,
  maxLength: 5,
  message: 'Invalid value',
}
describe('declarative launcher arguments', () => {
  it('retains numeric input filtering and range validation', () => {
    expect(sanitizeLauncherArgument('a30b00x99', validation)).toBe('30009')
    for (const text of ['1', '3000', '65535'])
      expect(validateLauncherArgument(text, validation)).toBeNull()
    for (const text of ['', '0', '65536', '3000;kill', '1.5'])
      expect(validateLauncherArgument(text, validation)).toBe('Invalid value')
    expect(sanitizeLauncherArgument('雪 café')).toBe('雪 café')
  })
})
