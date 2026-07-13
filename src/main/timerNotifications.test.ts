import { describe, expect, it } from 'vitest'
import { instrumentTimerNotificationCommand } from './extension-runner'

describe('timer notification command instrumentation', () => {
  it('replaces the Timers extension AppleScript notification with a backend marker', () => {
    const command =
      'sleep 30 ; if [ -f "/tmp/started---30.timer" ]; then osascript -e \'display notification "Timer \\"Tea\\" complete" with title "Ding!"\' ; afplay ding.wav ; rm "/tmp/started---30.timer"; else echo "Timer deleted"; fi'

    const instrumented = instrumentTimerNotificationCommand(command)

    expect(instrumented).not.toContain('display notification')
    expect(instrumented).toContain("printf '%s\\n' '__TEZBAR_TIMER_NOTIFICATION__:")
    expect(instrumented).toContain('afplay ding.wav')
  })

  it('leaves unrelated shell commands unchanged', () => {
    expect(instrumentTimerNotificationCommand('echo hello')).toBe('echo hello')
  })
})
