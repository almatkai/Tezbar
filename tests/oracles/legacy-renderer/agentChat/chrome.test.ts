import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { readFileSync } from 'node:fs'
import { afterEach, describe, expect, it, vi } from 'vitest'
import AgentChatView from '../AgentChatView'

function renderAgent(): string {
  vi.stubGlobal('window', { tezbar: {} })
  return renderToStaticMarkup(
    createElement(AgentChatView, {
      boot: { kind: 'newChat' },
      onBack: () => {},
      onOpenSettings: () => {},
    })
  )
}
afterEach(() => vi.unstubAllGlobals())

describe('agent uses the launcher chrome', () => {
  it('uses the same header and footer geometry rather than its own shell', () => {
    const html = renderAgent()
    expect(html).toContain('data-command-surface-header')
    expect(html).toContain('data-command-surface-footer')
    expect(html).toContain('launcher-header-row')
    expect(html).toContain('launcher-footer-row')
    expect(html).not.toContain('Inspect, find, and work on this computer')
  })

  it('keeps the launcher AI identity and icon controls in the chat header', () => {
    const html = renderAgent()
    expect(html).toContain('aria-label="AI mode"')
    expect(html).toContain('aria-label="Chat history"')
    expect(html).toContain('aria-label="New chat"')
    expect(html).toContain('aria-label="Back"')
    expect(html).not.toContain('>History</span>')
  })

  it('provides the same task-status slot and all working chat shortcuts', () => {
    const html = renderAgent()
    expect(html).toContain('data-command-surface-status')
    expect(html).toContain('Attach screen')
    expect(html).toContain('Providers')
    expect(html).toContain('New chat')
    expect(html).toContain('Back')
    expect(html).toContain('kbd-group')
  })

  it('shares actual components with search and AI history, not copied markup', () => {
    const source = readFileSync(new URL('../CommandBar.tsx', import.meta.url), 'utf8')
    expect(source.includes('<CommandSurfaceHeader')).toBe(true)
    expect(source.includes('<CommandSurfaceFooter')).toBe(true)
  })
})
