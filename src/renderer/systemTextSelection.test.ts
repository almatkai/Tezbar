import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

describe('system text selection styles', () => {
  const stylesPath = resolve(__dirname, 'styles.css')
  const stylesContent = readFileSync(stylesPath, 'utf8')

  it('disables user-select and user-drag on system UI by default', () => {
    expect(stylesContent).toContain('user-select: none;')
    expect(stylesContent).toContain('-webkit-user-select: none;')
    expect(stylesContent).toContain('-webkit-user-drag: none;')
  })

  it('allows text selection for editable inputs and markdown content', () => {
    expect(stylesContent).toContain('input,')
    expect(stylesContent).toContain('textarea,')
    expect(stylesContent).toContain('[contenteditable=\'true\']')
    expect(stylesContent).toContain('user-select: text;')
    expect(stylesContent).toContain('.tezbar-md')
  })

  it('explicitly disables text selection on rolling hints', () => {
    expect(stylesContent).toMatch(/\.rolling-command-hint\s*{[^}]*user-select:\s*none;/)
    expect(stylesContent).toMatch(/\.rolling-command-hint__label\s*{[^}]*user-select:\s*none;/)
    expect(stylesContent).toMatch(/\.path-command-hint\s*{[^}]*user-select:\s*none;/)
  })
})
