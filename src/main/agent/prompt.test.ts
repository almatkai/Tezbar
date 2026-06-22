import { describe, expect, it } from 'vitest'
import { buildPromptCommand, normalizeAgentImages } from './prompt'

describe('agent prompt images', () => {
  it('strips a data URL prefix for the Pi RPC payload', () => {
    expect(
      buildPromptCommand('inspect', [
        {
          type: 'image',
          data: 'data:image/png;base64,aGVsbG8=',
          mimeType: 'image/png',
          width: 10,
          height: 20,
        },
      ])
    ).toEqual({
      type: 'prompt',
      message: 'inspect',
      images: [{ type: 'image', data: 'aGVsbG8=', mimeType: 'image/png' }],
    })
  })

  it('omits an empty image collection', () => {
    expect(buildPromptCommand('hello', [])).toEqual({ type: 'prompt', message: 'hello' })
  })

  it('rejects malformed image data', () => {
    expect(() =>
      normalizeAgentImages([{ type: 'image', data: 'not base64!', mimeType: 'image/png' }])
    ).toThrow('valid base64')
  })
})
