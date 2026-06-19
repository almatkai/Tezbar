function apiErrorDetail(raw: string): string | null {
  const bodyStart = raw.indexOf('{')
  if (bodyStart < 0) return null

  try {
    const parsed = JSON.parse(raw.slice(bodyStart)) as {
      error?: { message?: unknown }
    }
    return typeof parsed.error?.message === 'string' ? parsed.error.message.trim() : null
  } catch {
    return null
  }
}

export function formatLlmErrorMessage(raw: string, providerLabel = 'AI provider'): string {
  const trimmed = raw.trim()
  const detail = apiErrorDetail(trimmed)
  const message = detail || trimmed
  const unsupportedModel = message.match(
    /supported API model names are (.+?), but you passed (.+?)(?:\.$|$)/i
  )

  if (unsupportedModel) {
    return `Model "${unsupportedModel[2]}" is not supported by this provider. Choose ${unsupportedModel[1]} and try again.`
  }

  const missingModel = message.match(/Model\s+["']([^"']+)["']\s+not found/i)
  if (missingModel) {
    return `The selected model "${missingModel[1]}" is unavailable. Choose another model in AI settings and try again.`
  }

  if (/pi exited before finishing/i.test(message)) {
    return 'The agent stopped before it could finish. Check the selected model in AI settings and try again.'
  }

  if (!detail) return trimmed
  const status = trimmed.match(/\berror\s+(\d{3})\b/i)?.[1]
  return `${providerLabel} request failed${status ? ` (${status})` : ''}: ${detail}`
}
