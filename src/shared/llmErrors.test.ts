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

  it('turns model startup failures into a concise action', () => {
    const raw =
      'Native model: startup failed:\nWarning: No models match pattern "tezbar/deepseek-v4-pro"\nError: Model "tezbar/deepseek-v4-pro" not found.'

    expect(formatLlmErrorMessage(raw)).toBe(
      'The selected model "tezbar/deepseek-v4-pro" is unavailable. Choose another model in AI settings and try again.'
    )
  })

  it.each([
    'Failed to write to backend runner process; recovery is in progress',
    'Backend runner is still recovering',
    'Backend runner stopped before replying',
  ])('explains a background service disconnect without raw transport jargon: %s', (raw) => {
    expect(formatLlmErrorMessage(raw)).toBe(
      'The background service disconnected. Tezbar is reconnecting. Wait a moment, then try again. Your message has been kept.'
    )
  })

  it('preserves a labelled native error', () => {
    expect(formatLlmErrorMessage('Native model: response incomplete')).toBe(
      'Native model: response incomplete'
    )
  })
})
