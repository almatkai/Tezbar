import { createHash } from 'node:crypto'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { authorizeGoogleOAuth, RAYCAST_GOOGLE_CLIENT_ID } from './google-oauth'
import { registerWindowsOAuthProtocol } from './google-oauth-protocol'

vi.mock('./google-oauth-protocol', () => ({ registerWindowsOAuthProtocol: vi.fn() }))
const cleanup = vi.fn(async () => {})
const register = vi.mocked(registerWindowsOAuthProtocol)

beforeEach(() => {
  register.mockReset().mockResolvedValue(cleanup)
  cleanup.mockClear()
})
afterEach(() => vi.restoreAllMocks())

describe('Google OAuth authorization', () => {
  it('honors an explicit Desktop override before the built-in Raycast mapping', async () => {
    const result = await authorizeGoogleOAuth({
      clientId: RAYCAST_GOOGLE_CLIENT_ID,
      scope: 'openid',
      useLoopbackRedirect: true,
      redirectUri: 'com.raycast:/oauth',
      openUrl: async (url) => {
        const authorization = new URL(url)
        await fetch(
          `${authorization.searchParams.get('redirect_uri')}?code=desktop&state=${authorization.searchParams.get('state')}`
        )
      },
    })
    expect(result.redirectUri).toMatch(/^http:\/\/127\.0\.0\.1:/)
    expect(register).not.toHaveBeenCalled()
  })
  it.each(['desktop-client', '690234628480-another-client'])(
    'keeps %s on loopback even at the Google endpoint',
    async (clientId) => {
      let authorization!: URL
      const result = await authorizeGoogleOAuth({
        clientId,
        scope: 'openid',
        openUrl: async (url) => {
          authorization = new URL(url)
          const callback = new URL(authorization.searchParams.get('redirect_uri')!)
          callback.search = new URLSearchParams({
            code: 'fixture-code',
            state: authorization.searchParams.get('state')!,
          }).toString()
          expect((await fetch(callback)).status).toBe(200)
        },
      })
      expect(result.redirectUri).toMatch(/^http:\/\/127\.0\.0\.1:\d+\/oauth\/callback$/)
      expect(register).not.toHaveBeenCalled()
      expect(createHash('sha256').update(result.codeVerifier).digest('base64url')).toBe(
        authorization.searchParams.get('code_challenge')
      )
    }
  )

  it('returns the Raycast redirect and cancels fallback after automatic protocol callback', async () => {
    const opened: string[] = []
    const result = await authorizeGoogleOAuth({
      clientId: RAYCAST_GOOGLE_CLIENT_ID,
      scope: 'calendar',
      manualFallbackDelayMs: 20,
      openUrl: async (url) => {
        opened.push(url)
        const authorization = new URL(url)
        const callbackUri = register.mock.calls[0][1]
        expect(authorization.searchParams.get('redirect_uri')).toBe('com.raycast:/oauth')
        const forwarded = `com.raycast:/oauth?code=fixture&state=${authorization.searchParams.get('state')}`
        expect((await fetch(`${callbackUri}?url=${encodeURIComponent(forwarded)}`)).status).toBe(
          200
        )
      },
    })
    expect(result.code).toBe('fixture')
    expect(result.redirectUri).toBe('com.raycast:/oauth')
    expect(opened).toHaveLength(1)
    expect(cleanup).toHaveBeenCalledOnce()
    await expect(fetch(register.mock.calls[0][1])).rejects.toThrow()
  })

  it('opens a usable manual form and accepts a normalized complete URL after invalid submissions', async () => {
    let authorization!: URL
    let manual!: string
    const result = await authorizeGoogleOAuth({
      clientId: RAYCAST_GOOGLE_CLIENT_ID,
      scope: 'calendar',
      manualFallbackDelayMs: 0,
      openUrl: async (url) => {
        if (new URL(url).hostname !== '127.0.0.1') {
          authorization = new URL(url)
          return
        }
        manual = url
        const page = await fetch(url)
        const html = await page.text()
        expect(page.headers.get('cache-control')).toBe('no-store')
        expect(page.headers.get('referrer-policy')).toBe('no-referrer')
        expect(html).toContain('method="post"')
        expect(html).toContain('label for="redirect-url"')
        const state = authorization.searchParams.get('state')!
        for (const invalid of [
          'plain-code',
          'com.raycast:/oauth?code=fixture',
          `com.raycast:/oauth?code=fixture&state=wrong`,
          `other:/oauth?code=fixture&state=${state}`,
          `com.raycast:/different?code=fixture&state=${state}`,
        ]) {
          const response = await fetch(url, {
            method: 'POST',
            body: new URLSearchParams({ url: invalid }),
          })
          expect(response.status).toBe(400)
        }
        expect(
          (
            await fetch(url, {
              method: 'POST',
              body: new URLSearchParams({
                url: `com.raycast://oauth?code=manual-code&state=${state}`,
              }),
            })
          ).status
        ).toBe(200)
      },
    })
    expect(result.code).toBe('manual-code')
    expect(cleanup).toHaveBeenCalledOnce()
    await expect(fetch(manual)).rejects.toThrow()
  })

  it('uses manual completion if protocol registration fails', async () => {
    register.mockRejectedValue(new Error('Registry unavailable'))
    const warning = vi.spyOn(console, 'warn').mockImplementation(() => {})
    let state = ''
    const result = await authorizeGoogleOAuth({
      clientId: RAYCAST_GOOGLE_CLIENT_ID,
      scope: 'calendar',
      openUrl: async (url) => {
        const parsed = new URL(url)
        if (parsed.hostname !== '127.0.0.1') {
          state = parsed.searchParams.get('state')!
          return
        }
        await fetch(url, {
          method: 'POST',
          body: new URLSearchParams({ url: `com.raycast:/oauth?code=manual&state=${state}` }),
        })
      },
    })
    expect(result.code).toBe('manual')
    expect(warning).toHaveBeenCalled()
  })

  it.each(['missing', 'wrong'])(
    'rejects %s loopback state without terminating the valid attempt',
    async (state) => {
      const result = await authorizeGoogleOAuth({
        clientId: 'desktop-client',
        scope: 'calendar',
        openUrl: async (url) => {
          const authorization = new URL(url)
          const callbackUri = authorization.searchParams.get('redirect_uri')!
          expect(
            (await fetch(`${callbackUri}?code=bad${state === 'wrong' ? '&state=wrong' : ''}`))
              .status
          ).toBe(400)
          await fetch(`${callbackUri}?code=valid&state=${authorization.searchParams.get('state')}`)
        },
      })
      expect(result.code).toBe('valid')
    }
  )

  it('cleans the protocol and listener when Google declines authorization', async () => {
    await expect(
      authorizeGoogleOAuth({
        clientId: RAYCAST_GOOGLE_CLIENT_ID,
        scope: 'calendar',
        openUrl: async (url) => {
          const state = new URL(url).searchParams.get('state')
          await fetch(
            `${register.mock.calls[0][1]}?url=${encodeURIComponent(`com.raycast:/oauth?error=access_denied&state=${state}`)}`
          )
        },
      })
    ).rejects.toThrow('access_denied')
    expect(cleanup).toHaveBeenCalledOnce()
  })

  it('cleans up when opening the browser fails', async () => {
    await expect(
      authorizeGoogleOAuth({
        clientId: RAYCAST_GOOGLE_CLIENT_ID,
        scope: 'calendar',
        openUrl: () => {
          throw new Error('Browser unavailable')
        },
      })
    ).rejects.toThrow('Browser unavailable')
    expect(cleanup).toHaveBeenCalledOnce()
  })

  it('cleans up after timeout without leaving an active listener', async () => {
    await expect(
      authorizeGoogleOAuth({
        clientId: RAYCAST_GOOGLE_CLIENT_ID,
        scope: 'calendar',
        timeoutMs: 1000,
        openUrl: () => {},
      })
    ).rejects.toThrow('timed out')
    expect(cleanup).toHaveBeenCalledOnce()
    await expect(fetch(register.mock.calls[0][1])).rejects.toThrow()
  })

  it('cancels authorization when its extension session is disposed', async () => {
    const controller = new AbortController()
    await expect(
      authorizeGoogleOAuth({
        clientId: RAYCAST_GOOGLE_CLIENT_ID,
        scope: 'calendar',
        signal: controller.signal,
        openUrl: () => controller.abort(),
      })
    ).rejects.toThrow('cancelled')
    expect(cleanup).toHaveBeenCalledOnce()
  })
})
