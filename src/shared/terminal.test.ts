import { describe, expect, it } from 'vitest'
import { compactTerminalPath, isAbsoluteTerminalPath, terminalDirectoryLabel } from './terminal'

describe('compactTerminalPath', () => {
  it('compacts a macOS user home while preserving the useful path tail', () => {
    expect(compactTerminalPath('/Users/almatkairatov/Desktop/code/Raymes')).toBe(
      '.../Desktop/code/Raymes'
    )
  })

  it('leaves non-user paths unchanged', () => {
    expect(compactTerminalPath('/tmp/project')).toBe('/tmp/project')
    expect(compactTerminalPath('~')).toBe('~')
  })

  it('compacts Windows user paths using the same display format', () => {
    expect(compactTerminalPath('C:\\Users\\User\\Desktop\\Tezbar')).toBe('.../Desktop/Tezbar')
  })
})

describe('terminalDirectoryLabel', () => {
  it('labels Windows profile directories relative to the user home', () => {
    expect(terminalDirectoryLabel('C:\\Users\\User\\Desktop')).toBe('~/Desktop')
  })
})

describe('isAbsoluteTerminalPath', () => {
  it('recognizes POSIX, Windows drive, and UNC paths', () => {
    expect(isAbsoluteTerminalPath('/Users/almatkairatov/Desktop')).toBe(true)
    expect(isAbsoluteTerminalPath('C:\\Users\\User\\Desktop')).toBe(true)
    expect(isAbsoluteTerminalPath('C:/Users/User/Desktop')).toBe(true)
    expect(isAbsoluteTerminalPath('\\\\server\\share')).toBe(true)
  })

  it('rejects relative paths', () => {
    expect(isAbsoluteTerminalPath('Desktop/project')).toBe(false)
    expect(isAbsoluteTerminalPath('Users/User/Desktop')).toBe(false)
  })
})
