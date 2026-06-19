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

  it('turns pi model startup failures into a concise action', () => {
    const raw =
      'pi exited before finishing:\nWarning: No models match pattern "tezbar/deepseek-v4-pro"\nError: Model "tezbar/deepseek-v4-pro" not found.'

    expect(formatLlmErrorMessage(raw)).toBe(
      'The selected model "tezbar/deepseek-v4-pro" is unavailable. Choose another model in AI settings and try again.'
    )
  })

  it('does not expose raw pi diagnostics for an unknown startup failure', () => {
    expect(formatLlmErrorMessage('pi exited before finishing:\ninternal stack detail')).toBe(
      'The agent stopped before it could finish. Check the selected model in AI settings and try again.'
    )
  })
})
