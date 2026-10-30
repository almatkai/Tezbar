import { describe, expect, it } from 'vitest'
import { detectSecret } from './vaultDetect'

describe('detectSecret', () => {
  // Fixtures are assembled from parts so no literal looks like a real credential.
  const fake = (prefix: string, body = 'abcdefghijklmnop1234'): string => `${prefix}${body}`
  it.each([
    [fake('sk-' + 'ant-'), 'api-key', 'Anthropic API key'],
    [fake('sk-' + 'proj-'), 'api-key', 'OpenAI API key'],
    [fake('sk-'), 'api-key', 'OpenAI API key'],
    [fake('gh' + 'p_'), 'token', 'GitHub token'],
    [fake('github' + '_pat_'), 'token', 'GitHub token'],
    [fake('gl' + 'pat-'), 'token', 'GitLab token'],
    [fake('AK' + 'IA', 'ABCDEFGHIJKLMNOP'), 'api-key', 'AWS access key'],
    [fake('xo' + 'xb-'), 'token', 'Slack token'],
    [fake('AI' + 'za', 'x'.repeat(35)), 'api-key', 'Google API key'],
    [['eyJhbGciOiJub25lIn0', 'eyJzdWIiOiJ0ZXN0In0', 'c2ln'].join('.'), 'token', 'Token'],
  ])('recognises %s', (value, kind, suggestedName) => {
    expect(detectSecret(value)).toEqual({ kind, suggestedName })
  })

  it('recognises private keys anywhere in a multi-line paste', () => {
    const key = '-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXk\n-----END OPENSSH PRIVATE KEY-----'
    expect(detectSecret(key)).toEqual({ kind: 'ssh-key', suggestedName: 'SSH private key' })
    expect(detectSecret('-----BEGIN RSA PRIVATE KEY-----\nabc').kind).toBe('ssh-key')
  })

  it('names servers after user@host from an ssh command or a bare address', () => {
    expect(detectSecret('ssh -p 2222 deploy@203.0.113.10')).toEqual({
      kind: 'server',
      suggestedName: 'deploy@203.0.113.10',
    })
    expect(detectSecret('ssh root@prod.example.com')).toEqual({
      kind: 'server',
      suggestedName: 'root@prod.example.com',
    })
    expect(detectSecret('ubuntu@10.0.0.4:2222')).toEqual({ kind: 'server', suggestedName: 'ubuntu@10.0.0.4' })
  })

  it('treats text with spaces as a note named by its first line', () => {
    expect(detectSecret('Wi-Fi office\npassword: hunter2')).toEqual({ kind: 'note', suggestedName: 'Wi-Fi office' })
    const long = 'a very long first line that keeps going well past forty characters'
    expect(detectSecret(long).suggestedName).toHaveLength(40)
  })

  it('falls back to a password and suggests nothing for an empty paste', () => {
    expect(detectSecret('hunter2!Xq')).toEqual({ kind: 'password', suggestedName: 'Password' })
    expect(detectSecret('   ')).toEqual({ kind: 'password', suggestedName: '' })
  })
})
