import { describe, expect, it } from 'vitest'
import { formatLlmErrorMessage } from './llmErrors'

describe('formatLlmErrorMessage', () => {
  it('turns model mismatch response bodies into an actionable message', () => {
    const raw =
      'OpenAI error 400: {"error":{"message":"The supported API model names are deepseek-v4-pro or deepseek-v4-flash, but you passed gemini-2.0-flash."}}'

    expect(formatLlmErrorMessage(raw)).toBe(
      'Model "gemini-2.0-flash" is not supported by this provider. Choose deepseek-v4-pro or deepseek-v4-flash and try again.'
    )
  })

  it('extracts JSON API messages without displaying response payloads', () => {
    expect(
      formatLlmErrorMessage(
        'Gemini error 401: {"error":{"message":"API key is invalid."}}',
        'Gemini'
      )
    ).toBe('Gemini request failed (401): API key is invalid.')
  })
})

