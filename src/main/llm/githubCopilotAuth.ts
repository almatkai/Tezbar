import { writeConfigPatch } from './configStore'

const COPILOT_API = 'https://api.githubcopilot.com'

export type DeviceCodeStartResult = {
  device_code: string
  user_code: string
  verification_uri: string
  expires_in: number
  interval: number
}

let deviceSession: { device_code: string; interval: number; client_id: string } | null = null

export function clearDeviceSession(): void {
  deviceSession = null
}

export async function startGithubDeviceFlow(clientId: string): Promise<DeviceCodeStartResult> {
  const body = new URLSearchParams({
    client_id: clientId,
    scope: 'read:user user:email',
  })
  const res = await fetch('https://github.com/login/device/code', {
    method: 'POST',
    headers: {
      Accept: 'application/json',
      'Content-Type': 'application/x-www-form-urlencoded',
    },
    body: body.toString(),
  })
  if (!res.ok) {
    const t = await res.text()
    throw new Error(`GitHub device code failed: ${res.status} ${t.slice(0, 200)}`)
  }
  const json = (await res.json()) as DeviceCodeStartResult
  if (!json.device_code || !json.user_code || !json.verification_uri) {
    throw new Error('GitHub device code: malformed response')
  }
  deviceSession = {
    device_code: json.device_code,
    interval: Math.max(5, json.interval ?? 5),
    client_id: clientId,
  }
  return json
}

export type PollResult =
  | { status: 'authorization_pending' }
  | { status: 'slow_down' }
  | { status: 'success'; access_token: string; refresh_token?: string; expires_in?: number }
  | { status: 'error'; error: string }

export async function pollGithubDeviceFlow(): Promise<PollResult> {
  if (!deviceSession) {
    return { status: 'error', error: 'No device session. Start sign-in again.' }
  }
  const body = new URLSearchParams({
    client_id: deviceSession.client_id,
    device_code: deviceSession.device_code,
    grant_type: 'urn:ietf:params:oauth:grant-type:device_code',
  })
  const res = await fetch('https://github.com/login/oauth/access_token', {
    method: 'POST',
    headers: {
      Accept: 'application/json',
      'Content-Type': 'application/x-www-form-urlencoded',
    },
    body: body.toString(),
  })
  const json = (await res.json()) as Record<string, unknown>
  const err = typeof json.error === 'string' ? json.error : ''
  if (err === 'authorization_pending') {
    return { status: 'authorization_pending' }
  }
  if (err === 'slow_down') {
    return { status: 'slow_down' }
  }
  if (err && err !== '') {
    deviceSession = null
    return { status: 'error', error: typeof json.error_description === 'string' ? json.error_description : err }
  }
  const access_token = typeof json.access_token === 'string' ? json.access_token : ''
  if (!access_token) {
    return { status: 'error', error: 'No access_token in response' }
  }
  const refresh_token = typeof json.refresh_token === 'string' ? json.refresh_token : undefined
  const expires_in = typeof json.expires_in === 'number' ? json.expires_in : undefined
  deviceSession = null
  return { status: 'success', access_token, refresh_token, expires_in }
}

export async function refreshGithubAccessToken(
  refreshToken: string,
  clientId: string,
  signal?: AbortSignal,
): Promise<{ access_token: string; refresh_token?: string; expires_in?: number }> {
  const body = new URLSearchParams({
    client_id: clientId,
    grant_type: 'refresh_token',
    refresh_token: refreshToken,
  })
  const res = await fetch('https://github.com/login/oauth/access_token', {
    method: 'POST',
    headers: {
      Accept: 'application/json',
      'Content-Type': 'application/x-www-form-urlencoded',
    },
    body: body.toString(),
    signal,
  })
  if (!res.ok) {
    const t = await res.text()
    throw new Error(`Token refresh failed: ${res.status} ${t.slice(0, 200)}`)
  }
  const json = (await res.json()) as Record<string, unknown>
  const access_token = typeof json.access_token === 'string' ? json.access_token : ''
  if (!access_token) {
    throw new Error('Token refresh: missing access_token')
  }
  return {
    access_token,
    refresh_token: typeof json.refresh_token === 'string' ? json.refresh_token : refreshToken,
    expires_in: typeof json.expires_in === 'number' ? json.expires_in : undefined,
  }
}

export function persistCopilotTokens(
  accessToken: string,
  refreshToken?: string,
  expiresInSec?: number,
): void {
  const patch: Record<string, unknown> = {
    copilotGithubToken: accessToken,
  }
  if (refreshToken) patch.copilotRefreshToken = refreshToken
  if (expiresInSec !== undefined) {
    patch.copilotExpiresAt = Date.now() + expiresInSec * 1000
  }
  writeConfigPatch(patch)
}

export async function copilotApiPing(token: string): Promise<boolean> {
  try {
    const res = await fetch(`${COPILOT_API}/models`, {
      method: 'GET',
      headers: {
        Authorization: `Bearer ${token}`,
        'Editor-Version': 'TezBar/0.1.0',
        'Copilot-Integration-Id': 'vscode-chat',
        Accept: 'application/json',
      },
      signal: AbortSignal.timeout(5000),
    })
    return res.ok
  } catch {
    return false
  }
}
