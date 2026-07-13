import { describe, expect, it } from 'vitest'
import {
  moveTerminalSelectionDown,
  terminalSessionAtIndex,
  terminalSessionRestoreOptions,
  terminalSessionShortcutIndex,
} from './terminalSessionSelection'
import type { TerminalSessionSummary } from '../shared/terminal'

describe('terminal session palette selection', () => {
  const sessions = ['first', 'second']

  it('does not implicitly select the first matching session', () => {
    expect(terminalSessionAtIndex(sessions, -1)).toBeUndefined()
    expect(terminalSessionAtIndex(sessions, 0)).toBe('first')
  })

  it('activates the first row only after ArrowDown', () => {
    expect(moveTerminalSelectionDown(-1, sessions.length)).toBe(0)
    expect(moveTerminalSelectionDown(0, sessions.length)).toBe(1)
    expect(moveTerminalSelectionDown(1, sessions.length)).toBe(1)
  })

  it('recovers safely when a previously selected row disappeared', () => {
    expect(moveTerminalSelectionDown(5, sessions.length)).toBe(0)
    expect(moveTerminalSelectionDown(-1, 0)).toBe(-1)
  })

  it('switches sessions with Command or Control plus a number', () => {
    expect(
      terminalSessionShortcutIndex({
        altKey: false,
        code: 'Digit1',
        ctrlKey: false,
        key: '1',
        metaKey: true,
        shiftKey: false,
      })
    ).toBe(0)
    expect(
      terminalSessionShortcutIndex({
        altKey: false,
        code: 'Numpad9',
        ctrlKey: true,
        key: '9',
        metaKey: false,
        shiftKey: false,
      })
    ).toBe(8)
  })

  it('does not treat Option-number symbols or bare numbers as session shortcuts', () => {
    expect(
      terminalSessionShortcutIndex({
        altKey: true,
        code: 'Digit1',
        ctrlKey: false,
        key: '¡',
        metaKey: false,
        shiftKey: false,
      })
    ).toBeNull()
    expect(
      terminalSessionShortcutIndex({
        altKey: false,
        code: 'Digit1',
        ctrlKey: false,
        key: '1',
        metaKey: false,
        shiftKey: false,
      })
    ).toBeNull()
  })

  it('reopens a saved session under the same id so native history is preserved', () => {
    const session: TerminalSessionSummary = {
      sessionId: 'native-terminal-123-4',
      name: 'Downloads · git clone repo',
      cwd: '/Users/test/Downloads',
      shell: '/bin/zsh',
      lastCommand: 'git clone repo',
      createdAt: 1,
      updatedAt: 2,
      lastActiveAt: 2,
      saveFor: 'forever',
      keepAliveFor: 'until-stop',
      status: 'exited',
    }

    expect(terminalSessionRestoreOptions(session)).toEqual({
      restoreSessionId: 'native-terminal-123-4',
      restoreCommand: 'git clone repo',
      workingDirectory: '/Users/test/Downloads',
      name: 'Downloads · git clone repo',
      saveFor: 'forever',
      keepAliveFor: 'until-stop',
    })
  })
})
