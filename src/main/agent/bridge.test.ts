import { spawn } from 'node:child_process'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createBridge, type Bridge } from './bridge'

const fixture = vi.hoisted(() => ({ path: '' }))
vi.mock('node:child_process', async (importOriginal) => {
  const actual = await importOriginal<typeof import('node:child_process')>()
  return {
    ...actual,
    spawn: vi.fn((_command, _args, options) =>
      actual.spawn(process.execPath, [fixture.path], options)
    ),
  }
})
vi.mock('../knowledge/agent/gateway', () => ({ ensureKnowledgeAgentGateway: async () => null }))

function script(body: string, disposition = 'started'): string {
  return `
    let buffer = '';
    const emit = (event) => process.stdout.write(JSON.stringify(event) + '\\n');
    const text = (value) => ({ role: 'assistant', stopReason: 'stop', content: [{ type: 'text', text: value }] });
    process.stdin.setEncoding('utf8');
    process.stdin.on('data', (chunk) => {
      buffer += chunk;
      let n;
      while ((n = buffer.indexOf('\\n')) >= 0) {
        const command = JSON.parse(buffer.slice(0, n));
        buffer = buffer.slice(n + 1);
        if (command.type === 'prompt') {
          emit({ id: command.id, type: 'response', success: true, data: { disposition: '${disposition}' } });
          ${body}
        }
      }
    });
    process.stdin.on('end', () => process.exit(0));
  `
}

describe('Pi RPC run lifecycle', () => {
  let directory: string
  let bridge: Bridge
  beforeEach(() => {
    directory = mkdtempSync(join(tmpdir(), 'tezbar-pi-rpc-'))
    fixture.path = join(directory, 'pi-fixture.cjs')
    bridge = createBridge()
    vi.mocked(spawn).mockClear()
    // A failing spy assertion must never dump real inherited credentials.
    for (const key of Object.keys(process.env)) {
      if (/TOKEN|API_KEY|SECRET|PASSWORD|AUTH/i.test(key)) vi.stubEnv(key, '<REDACTED>')
    }
  })
  afterEach(() => {
    bridge.dispose()
    rmSync(directory, { recursive: true, force: true })
    vi.unstubAllEnvs()
  })

  it('waits for a transient provider retry to settle instead of failing on the first agent_end', async () => {
    writeFileSync(
      fixture.path,
      script(`
      emit({ type: 'agent_start' });
      emit({ type: 'message_end', message: { role: 'assistant', stopReason: 'error', errorMessage: '503 overloaded', content: [] } });
      emit({ type: 'agent_end', willRetry: true, messages: [] });
      emit({ type: 'auto_retry_start', attempt: 1, maxAttempts: 3 });
      setTimeout(() => {
        emit({ type: 'agent_start' });
        emit({ type: 'turn_start' });
        emit({ type: 'message_end', message: text('CPU: 12%. Browser is the busiest process.') });
        emit({ type: 'auto_retry_end', success: true });
        emit({ type: 'agent_end', willRetry: false, messages: [text('CPU: 12%. Browser is the busiest process.')] });
        emit({ type: 'agent_settled', aborted: false });
      }, 30);
    `)
    )
    await expect(
      bridge.run('Inspect CPU', { piBin: 'fixture', timeoutMs: 1500 })
    ).resolves.toMatchObject({
      answer: 'CPU: 12%. Browser is the busiest process.',
    })
  })

  it('keeps automatic follow-up output after a low-level run has ended', async () => {
    writeFileSync(
      fixture.path,
      script(`
      emit({ type: 'agent_start' });
      emit({ type: 'message_update', assistantMessageEvent: { type: 'text_delta', delta: 'Checking...' } });
      emit({ type: 'agent_end', willRetry: false, messages: [] });
      setTimeout(() => {
        emit({ type: 'agent_start' });
        emit({ type: 'turn_start' });
        emit({ type: 'message_update', assistantMessageEvent: { type: 'text_delta', delta: 'Final result.' } });
        emit({ type: 'agent_end', willRetry: false, messages: [] });
        emit({ type: 'agent_settled', aborted: false });
      }, 30);
    `)
    )
    await expect(
      bridge.run('Inspect CPU', { piBin: 'fixture', timeoutMs: 1500 })
    ).resolves.toMatchObject({
      answer: 'Checking...\n\nFinal result.',
    })
  })

  it('does not wait indefinitely when an extension consumes a prompt without starting a run', async () => {
    writeFileSync(fixture.path, script('', 'handled'))
    await expect(
      bridge.run('/extension-command', { piBin: 'fixture', timeoutMs: 80 })
    ).rejects.toThrow(/handled.*without.*run/i)
  })

  it('does not start a subprocess for an already cancelled request', async () => {
    writeFileSync(
      fixture.path,
      script(`emit({ type: 'agent_start' }); emit({ type: 'agent_end', messages: [] });`)
    )
    const controller = new AbortController()
    controller.abort()
    await expect(
      bridge.run('Inspect CPU', { piBin: 'fixture', signal: controller.signal, timeoutMs: 80 })
    ).rejects.toThrow(/abort|cancel/i)
    expect(vi.mocked(spawn).mock.calls.length).toBe(0)
  })
})
