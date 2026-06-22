import { describe, expect, it } from 'vitest'
import { compactTerminalPath } from './terminal'

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
})
