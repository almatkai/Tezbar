import { createHash, randomBytes } from 'node:crypto'
import { createServer, type ServerResponse } from 'node:http'
import { registerWindowsOAuthProtocol } from './google-oauth-protocol'

export const RAYCAST_GOOGLE_CLIENT_ID =
  '690234628480-bhl8vft6dp81bkv4bq0lf9l6vv7nerq4.apps.googleusercontent.com'
export const GOOGLE_DESKTOP_CLIENT_ID_PREFERENCE = 'tezbarGoogleDesktopClientId'
export const GOOGLE_DESKTOP_CLIENT_SECRET_PREFERENCE = 'tezbarGoogleDesktopClientSecret'

export type GoogleAuthorizationOptions = {
  clientId: string
  scope: string
  redirectUri?: string
  useLoopbackRedirect?: boolean
  authorizationEndpoint?: string
  timeoutMs?: number
  manualFallbackDelayMs?: number
  openUrl: (url: string) => Promise<void> | void
  signal?: AbortSignal
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => {
    return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[character]!
  })
}

function callbackMatches(received: URL, expected: URL): boolean {
  if (received.protocol !== expected.protocol || received.username || received.password)
    return false
  if (['http:', 'https:'].includes(expected.protocol)) {
    return received.origin === expected.origin && received.pathname === expected.pathname
  }
  // Accept both com.raycast:/oauth and the browser's com.raycast://oauth form.
  const path = (url: URL): string => `/${url.host}${url.pathname}`.replace(/^\/\//, '/')
  return path(received) === path(expected)
}

function respond(response: ServerResponse, status: number, body: string): void {
  response.writeHead(status, {
    'content-type': 'text/html; charset=utf-8',
    'cache-control': 'no-store',
    'referrer-policy': 'no-referrer',
    'content-security-policy':
      "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; frame-ancestors 'none'",
    'x-content-type-options': 'nosniff',
  })
  response.end(body)
}

function manualPage(action: string, authorizationUrl: string): string {
  return `<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>Tezbar — Google sign-in</title>
<style>
:root{color-scheme:dark;--bg:#171717;--surface:#202020;--text:#f5f5f5;--muted:#c2c2c2;--border:#666;--accent:#8ecaff}
*{box-sizing:border-box}body{font:16px/1.5 system-ui,sans-serif;background:var(--bg);color:var(--text);margin:0;padding:24px;min-height:100vh;display:grid;place-items:center}
main{width:100%;max-width:560px;background:var(--surface);padding:32px;border:1px solid var(--border);border-radius:8px}
h1{font-size:24px;margin:0 0 16px}p{color:var(--muted)}label{display:block;margin:24px 0 8px}textarea{display:block;width:100%;min-height:112px;padding:12px;background:var(--bg);color:var(--text);border:1px solid var(--border);border-radius:4px;font:inherit;resize:vertical}
button{font:inherit;font-weight:600;padding:12px 16px;margin-top:16px;border:0;border-radius:4px;background:var(--accent);color:#14202b;cursor:pointer}a{color:var(--accent)}:focus-visible{outline:3px solid var(--accent);outline-offset:4px}
@media(max-width:400px){body{padding:16px}main{padding:24px}}
</style></head><body><main><h1>Finish Google sign-in</h1>
<p>Complete sign-in in the Google tab, then allow your browser to open Tezbar when prompted.</p>
<p>If that tab is closed, <a href="${escapeHtml(authorizationUrl)}" target="_blank" rel="noreferrer noopener">open Google sign-in</a>.</p>
<p>If the browser cannot return to Tezbar, copy the entire redirect URL from its address bar and paste it here. Keep this page open until sign-in is complete.</p>
<form method="post" action="${escapeHtml(action)}"><label for="redirect-url">Redirect URL</label>
<textarea id="redirect-url" name="url" required autocomplete="off" spellcheck="false" placeholder="com.raycast:/oauth?code=…&amp;state=…"></textarea>
<button type="submit">Complete sign-in</button></form></main></body></html>`
}

export async function authorizeGoogleOAuth(options: GoogleAuthorizationOptions): Promise<{
  code: string
  redirectUri: string
  codeVerifier: string
}> {
  const state = randomBytes(24).toString('base64url')
  const codeVerifier = randomBytes(48).toString('base64url')
  let resolveCallback!: (code: string) => void
  let rejectCallback!: (error: Error) => void
  let settled = false
  const callback = new Promise<string>((resolve, reject) => {
    resolveCallback = resolve
    rejectCallback = reject
  })
  // Opening the browser or registering a protocol may fail before we await it.
  void callback.catch(() => {})
  let redirectUri = ''
  let authorizationUrl = ''
  let manualUrl = ''
  let cleanupProtocol: (() => Promise<void>) | undefined
  let timeout: ReturnType<typeof setTimeout> | undefined
  let fallback: ReturnType<typeof setTimeout> | undefined
  const finish = (code?: string, error?: Error): void => {
    if (settled) return
    settled = true
    clearTimeout(fallback)
    clearTimeout(timeout)
    if (error) rejectCallback(error)
    else resolveCallback(code!)
  }
  const cancel = (): void => finish(undefined, new Error('OAuth authorization cancelled'))
  const accept = (url: URL, response: ServerResponse, forwarded?: string): void => {
    if (settled) return respond(response, 409, '<p>This sign-in attempt has already finished.</p>')
    let params = url.searchParams
    if (forwarded !== undefined) {
      try {
        const received = new URL(forwarded.trim())
        if (!callbackMatches(received, new URL(redirectUri))) throw new Error('Invalid redirect')
        params = received.searchParams
      } catch {
        return respond(
          response,
          400,
          '<p>Paste the complete redirect URL from this Google sign-in attempt, including code and state. Go back to try again.</p>'
        )
      }
    } else if (!redirectUri.startsWith('http://')) {
      return respond(response, 400, '<p>The complete redirect URL is required.</p>')
    }
    if (params.get('state') !== state) {
      return respond(
        response,
        400,
        '<p>Invalid OAuth state. Use the redirect URL from this sign-in attempt. Go back to try again.</p>'
      )
    }
    const error = params.get('error')
    const code = params.get('code')
    if (error) {
      respond(response, 400, '<p>Google sign-in was declined. Return to Tezbar and try again.</p>')
      return finish(undefined, new Error(error))
    }
    if (!code)
      return respond(
        response,
        400,
        '<p>The redirect URL did not include a code. Go back to try again.</p>'
      )
    respond(
      response,
      200,
      '<!doctype html><html lang="en"><meta charset="utf-8"><title>Tezbar — sign-in received</title><p>Sign-in received. You can close this tab and return to Tezbar.</p></html>'
    )
    finish(code)
  }
  const server = createServer((request, response) => {
    const url = new URL(request.url ?? '/', 'http://127.0.0.1')
    if (url.pathname === '/oauth/manual' && url.searchParams.get('session') === state) {
      if (request.method === 'GET')
        return respond(response, 200, manualPage(manualUrl, authorizationUrl))
      if (request.method === 'POST') {
        if (
          request.headers['content-type']?.split(';')[0] !== 'application/x-www-form-urlencoded'
        ) {
          return respond(response, 415, '<p>Unsupported form format.</p>')
        }
        let body = ''
        let oversized = false
        request.on('data', (chunk: Buffer) => {
          if (oversized) return
          body += chunk.toString('utf8')
          if (body.length > 16_384) {
            oversized = true
            respond(response, 413, '<p>The redirect URL is too long.</p>')
          }
        })
        request.on('end', () => {
          if (!oversized) accept(url, response, new URLSearchParams(body).get('url') ?? '')
        })
        return
      }
    }
    if (url.pathname === '/oauth/callback' && request.method === 'GET') {
      return accept(url, response, url.searchParams.get('url') ?? undefined)
    }
    respond(response, 404, '<p>Not found.</p>')
  })
  try {
    options.signal?.addEventListener('abort', cancel, { once: true })
    if (options.signal?.aborted) throw new Error('OAuth authorization cancelled')
    await new Promise<void>((resolve, reject) => {
      server.once('error', reject)
      server.listen(0, '127.0.0.1', resolve)
    })
    const address = server.address()
    if (!address || typeof address === 'string')
      throw new Error('OAuth callback server did not start')
    const loopbackUri = `http://127.0.0.1:${address.port}/oauth/callback`
    redirectUri = options.useLoopbackRedirect
      ? loopbackUri
      : options.redirectUri ||
        (options.clientId === RAYCAST_GOOGLE_CLIENT_ID ? 'com.raycast:/oauth' : loopbackUri)
    const customScheme = !['http:', 'https:'].includes(new URL(redirectUri).protocol)
    manualUrl = `http://127.0.0.1:${address.port}/oauth/manual?session=${state}`
    const authorization = new URL(
      options.authorizationEndpoint || 'https://accounts.google.com/o/oauth2/v2/auth'
    )
    authorization.search = new URLSearchParams({
      client_id: options.clientId,
      redirect_uri: redirectUri,
      response_type: 'code',
      scope: options.scope,
      state,
      code_challenge: createHash('sha256').update(codeVerifier).digest('base64url'),
      code_challenge_method: 'S256',
      access_type: 'offline',
      prompt: 'consent',
    }).toString()
    authorizationUrl = authorization.toString()
    let protocolFailed = process.platform !== 'win32'
    if (customScheme) {
      try {
        cleanupProtocol = await registerWindowsOAuthProtocol(redirectUri, loopbackUri)
      } catch (error) {
        protocolFailed = true
        console.warn('[OAuth] Automatic redirect unavailable; using the manual page:', error)
      }
    }
    if (options.signal?.aborted) throw new Error('OAuth authorization cancelled')
    timeout = setTimeout(
      () => finish(undefined, new Error('OAuth authorization timed out')),
      Math.max(1_000, options.timeoutMs || 120_000)
    )
    if (customScheme) {
      fallback = setTimeout(
        () => {
          if (!settled)
            void Promise.resolve()
              .then(() => options.openUrl(manualUrl))
              .catch((error: unknown) =>
                finish(undefined, error instanceof Error ? error : new Error(String(error)))
              )
        },
        options.manualFallbackDelayMs ?? (protocolFailed ? 0 : 15_000)
      )
    }
    await options.openUrl(authorizationUrl)
    return { code: await callback, redirectUri, codeVerifier }
  } finally {
    settled = true
    clearTimeout(timeout)
    clearTimeout(fallback)
    options.signal?.removeEventListener('abort', cancel)
    try {
      await cleanupProtocol?.()
    } catch (error) {
      console.warn('[OAuth] Could not restore protocol handler:', error)
    }
    server.closeAllConnections()
    await new Promise<void>((resolve) => server.close(() => resolve()))
  }
}
