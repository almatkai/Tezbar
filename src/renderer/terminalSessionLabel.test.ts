import { describe, expect, it } from 'vitest'
import {
  commandFromTerminalLine,
  formatTerminalSessionName,
  terminalDirectoryLabel,
} from './terminalSessionLabel'

describe('terminal session labels', () => {
  it('formats the directory and last command with a clear separator', () => {
    expect(formatTerminalSessionName('/Users/almatkairatov/Downloads', 'cd Downloads')).toBe(
      '~/Downloads · cd Downloads',
    )
  })

  it('uses only the current directory name', () => {
    expect(terminalDirectoryLabel('/Users/almatkairatov/Desktop/code/aml/aml-space')).toBe(
      'aml/aml-space',
    )
    expect(terminalDirectoryLabel('/Users/almatkairatov')).toBe('~')
    expect(terminalDirectoryLabel('/')).toBe('/')
  })

  it('reads the shell-rendered command after autocomplete expanded it', () => {
    expect(
      commandFromTerminalLine('almatkairatov@Almats-MacBook-Pro ~ % cd Downloads'),
    ).toBe('cd Downloads')
  })
})
