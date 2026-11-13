import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

describe('code block light theme styles', () => {
  const stylesPath = resolve(__dirname, 'styles.css')
  const stylesContent = readFileSync(stylesPath, 'utf8')

  it('defines light theme styles for markdown pre and code blocks', () => {
    expect(stylesContent).toContain(":root[data-theme='white'] .tezbar-md pre")
    expect(stylesContent).toContain(":root[data-theme='white'] .tezbar-md pre code")
    expect(stylesContent).toContain(":root[data-theme='white'] .tezbar-md-code-header")
    expect(stylesContent).toContain(":root[data-theme='white'] .tezbar-md-code-copy")
  })

  it('defines light theme syntax highlighting tokens', () => {
    expect(stylesContent).toContain(":root[data-theme='white'] :is(.tezbar-md pre, pre code) .hljs-keyword")
    expect(stylesContent).toContain(":root[data-theme='white'] :is(.tezbar-md pre, pre code) .hljs-string")
    expect(stylesContent).toContain(":root[data-theme='white'] :is(.tezbar-md pre, pre code) .hljs-comment")
  })
})
