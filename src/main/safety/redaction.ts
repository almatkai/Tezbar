export function redactSensitiveText(input: string): string {
  return input
    .replace(/-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----/g, '[REDACTED_PRIVATE_KEY]')
    .replace(/\b(sk-[A-Za-z0-9_-]{12,})\b/g, '[REDACTED_API_KEY]')
    .replace(/\b(gh[pousr]_[A-Za-z0-9_]{12,})\b/g, '[REDACTED_TOKEN]')
    .replace(/\b(AKIA[0-9A-Z]{16})\b/g, '[REDACTED_AWS_ACCESS_KEY]')
    .replace(/\b(bearer\s+)[A-Za-z0-9._~+\/-]{16,}/gi, '$1[REDACTED_TOKEN]')
    .replace(/\b(password|passwd|api[_-]?key|secret|token)\s*[=:]\s*([^\s,;]+)/gi, '$1=[REDACTED]')
}

