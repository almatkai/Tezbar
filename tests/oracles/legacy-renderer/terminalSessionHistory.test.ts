import { describe, expect, it } from 'vitest'
import { terminalOutputAfterSnapshot } from './terminalSessionHistory'

describe('terminal session history replay', () => {
  it('does not duplicate startup output already included in the durable snapshot', () => {
    expect(terminalOutputAfterSnapshot('old history\nnew prompt\n', 'new prompt\n')).toBe('')
  })

  it('keeps output that arrived after the native snapshot was read', () => {
    expect(terminalOutputAfterSnapshot('old history\nnew prompt', 'new prompt\nnext line')).toBe(
      '\nnext line',
    )
  })

  it('keeps unrelated buffered output intact', () => {
    expect(terminalOutputAfterSnapshot('old history\n', 'brand new output\n')).toBe(
      'brand new output\n',
    )
  })
})
