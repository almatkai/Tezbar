import { chmodSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import { createServer } from 'node:http'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterAll, describe, expect, it, vi } from 'vitest'

const userData = mkdtempSync(join(tmpdir(), 'raymes-runner-user-'))

vi.mock('electron', () => ({
  app: {
    isPackaged: false,
    getPath: () => userData,
  },
  BrowserWindow: class {},
  clipboard: {
    readText: () => '',
    writeText: () => undefined,
    writeImage: () => undefined,
  },
  nativeImage: {
    createFromPath: () => ({ isEmpty: () => true }),
  },
  shell: {
    openExternal: async () => undefined,
    showItemInFolder: () => undefined,
  },
}))

vi.mock('./llm/extensionAI', () => ({
  askExtensionAI: async (prompt: string) => `AI:${prompt}`,
}))

import {
  clearAllExtensionSessions,
  invokeExtensionAction,
  loadMoreExtensionSession,
  refreshExtensionSession,
  runExtensionCommand,
  runExtensionCommandFromPackageJson,
  updateSearchText,
} from './extension-runner'
import {
  getExtensionPreferenceSetup,
  saveExtensionPreferences,
  shouldShowExtensionPreferenceSetup,
} from './extension-registry'

afterAll(() => {
  clearAllExtensionSessions()
  rmSync(userData, { recursive: true, force: true })
})

describe('extension preference onboarding', () => {
  it('requires first-run credentials before entering the extension', async () => {
    const extensionId = 'raycast.credential-gate-fixture'
    const extensionRoot = join(userData, 'extensions', 'credential-gate-fixture')
    mkdirSync(join(extensionRoot, 'src'), { recursive: true })
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'credential-gate-fixture',
        title: 'Credential Gate Fixture',
        preferences: [
          { name: 'apiKey', title: 'API Key', type: 'password', required: false },
        ],
        commands: [{
          name: 'index',
          title: 'Index',
          mode: 'view',
          preferences: [
            { name: 'resultLimit', title: 'Result Limit', type: 'dropdown', required: false },
          ],
        }],
      })
    )
    writeFileSync(
      join(extensionRoot, 'src', 'index.tsx'),
      `import React from 'react'
       import { Detail } from '@raycast/api'
       export default function Command() { return <Detail markdown="Ready" /> }`
    )

    const setup = getExtensionPreferenceSetup(extensionId, 'index')
    expect(setup.hasSavedPreferences).toBe(false)
    expect(setup.preferences).toEqual([
      expect.objectContaining({ name: 'apiKey' }),
      expect.objectContaining({
        name: 'resultLimit',
        commandName: 'index',
        commandTitle: 'Index',
      }),
    ])
    expect(shouldShowExtensionPreferenceSetup(extensionId, 'index')).toBe(true)
    const blocked = await runExtensionCommand({ extensionId, commandName: 'index' })
    expect(blocked.ok).toBe(true)
    if (!blocked.ok || blocked.mode !== 'view') return
    expect(blocked.root.type).toBe('Tezbar.PreferenceSetup')

    saveExtensionPreferences(extensionId, { apiKey: 'configured-key' })
    const unlocked = await runExtensionCommand({ extensionId, commandName: 'index' })
    expect(unlocked.ok).toBe(true)
    if (!unlocked.ok || unlocked.mode !== 'view') return
    expect(unlocked.root.type).toBe('Detail')
  })
})

describe('extension runtime list pagination', () => {
  it('runs paginated promise loaders and resets them when dependencies change', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'raymes-server-pagination-extension-'))
    mkdirSync(join(extensionRoot, '.sc-build'))
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'server-pagination-fixture',
        title: 'Server Pagination Fixture',
        commands: [{ name: 'index', title: 'Index', mode: 'view' }],
      })
    )
    writeFileSync(
      join(extensionRoot, '.sc-build', 'index.js'),
      `const React = require('react').default
       const { List } = require('@raycast/api')
       const { usePromise } = require('@raycast/utils')
       module.exports.default = function Command() {
         const [query, setQuery] = React.useState('first')
         const { data = [], isLoading, pagination } = usePromise(
           (value) => async ({ page }) => {
             await new Promise((resolve) => setTimeout(resolve, 15))
             return { data: [value + '-' + page + '-a', value + '-' + page + '-b'], hasMore: page === 0 }
           },
           [query]
         )
         return React.createElement(List, { isLoading, pagination, onSearchTextChange: setQuery },
           data.map((title) => React.createElement(List.Item, { key: title, title })))
       }`
    )

    try {
      const initial = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index'
      )
      expect(initial.ok, JSON.stringify(initial)).toBe(true)
      if (!initial.ok || initial.mode !== 'view') return

      await new Promise((resolve) => setTimeout(resolve, 30))
      const firstPage = await refreshExtensionSession({ sessionId: initial.sessionId })
      expect(firstPage.ok, JSON.stringify(firstPage)).toBe(true)
      if (!firstPage.ok || firstPage.mode !== 'view') return
      expect(firstPage.root.children.map((child) => child.props.title)).toEqual([
        'first-0-a',
        'first-0-b',
      ])
      expect(firstPage.root.props.__hasMore).toBe(true)

      const [loadOne, loadDuplicate] = await Promise.all([
        loadMoreExtensionSession({ sessionId: initial.sessionId }),
        loadMoreExtensionSession({ sessionId: initial.sessionId }),
      ])
      const loaded = loadOne.mode === 'view' ? loadOne : loadDuplicate
      expect(loaded.ok, JSON.stringify(loaded)).toBe(true)
      if (!loaded.ok || loaded.mode !== 'view') return
      expect(loaded.root.children.map((child) => child.props.title)).toEqual([
        'first-0-a',
        'first-0-b',
        'first-1-a',
        'first-1-b',
      ])
      expect(loaded.root.props.__hasMore).toBe(false)
      expect(await loadMoreExtensionSession({ sessionId: initial.sessionId })).toEqual({
        ok: true,
        mode: 'unchanged',
      })

      const searched = await updateSearchText({
        sessionId: initial.sessionId,
        searchText: 'second',
      })
      expect(searched.ok, JSON.stringify(searched)).toBe(true)
      await new Promise((resolve) => setTimeout(resolve, 30))
      const resetPage = await refreshExtensionSession({ sessionId: initial.sessionId })
      expect(resetPage.ok, JSON.stringify(resetPage)).toBe(true)
      if (!resetPage.ok || resetPage.mode !== 'view') return
      expect(resetPage.root.children.map((child) => child.props.title)).toEqual([
        'second-0-a',
        'second-0-b',
      ])
      expect(resetPage.root.props.__hasMore).toBe(true)
    } finally {
      rmSync(extensionRoot, { recursive: true, force: true })
    }
  })

  it('supports external stores and configured Axios instances', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'raymes-modern-react-extension-'))
    mkdirSync(join(extensionRoot, '.sc-build'))
    const server = createServer((request, response) => {
      expect(request.url).toBe('/value')
      expect(request.headers.authorization).toBe('Bearer fixture-token')
      response.writeHead(200, { 'content-type': 'application/json' })
      response.end(JSON.stringify({ value: 'remote-ready' }))
    })
    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
    const address = server.address()
    if (!address || typeof address === 'string') throw new Error('Fixture server did not start')
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'modern-react-fixture',
        title: 'Modern React Fixture',
        commands: [{ name: 'index', title: 'Index', mode: 'view' }],
      })
    )
    writeFileSync(
      join(extensionRoot, '.sc-build', 'index.js'),
      `const React = require('react').default
       const axios = require('axios').default
       const { Detail } = require('@raycast/api')
       let snapshot = 'initial'
       const listeners = new Set()
       const store = {
         getSnapshot: () => snapshot,
         subscribe: (listener) => { listeners.add(listener); return () => listeners.delete(listener) },
         set: (value) => { snapshot = value; for (const listener of listeners) listener() },
       }
       const client = axios.create({
         baseURL: 'http://127.0.0.1:${address.port}',
         headers: { Authorization: 'Bearer fixture-token' },
       })
       module.exports.default = function Command() {
         const current = React.useSyncExternalStore(store.subscribe, store.getSnapshot)
         const [remote, setRemote] = React.useState('loading')
         React.useEffect(() => {
           Promise.all([
             client.get('/value'),
             axios({
               baseURL: 'http://127.0.0.1:${address.port}',
               url: '/value',
               method: 'GET',
               data: { ignoredByAxios: true },
               headers: { Authorization: 'Bearer fixture-token' },
             }),
           ]).then(([created, callable]) => {
             setRemote(created.data.value + ':' + callable.data.value)
             store.set('changed')
           })
         }, [])
         return React.createElement(Detail, { markdown: current + ':' + remote })
       }`
    )

    try {
      const initial = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index'
      )
      expect(initial.ok, JSON.stringify(initial)).toBe(true)
      if (!initial.ok || initial.mode !== 'view') return
      expect(initial.root.props.markdown).toBe('initial:loading')

      await new Promise((resolve) => setTimeout(resolve, 40))
      const refreshed = await refreshExtensionSession({ sessionId: initial.sessionId })
      expect(refreshed.ok, JSON.stringify(refreshed)).toBe(true)
      if (!refreshed.ok || refreshed.mode !== 'view') return
      expect(refreshed.root.props.markdown).toBe('changed:remote-ready:remote-ready')
    } finally {
      await new Promise<void>((resolve) => server.close(() => resolve()))
      rmSync(extensionRoot, { recursive: true, force: true })
    }
  })

  it('bridges ScreenOCR Swift imports to the packaged helper', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'raymes-screenocr-extension-'))
    const helperPath = join(extensionRoot, 'screenocr-helper')
    mkdirSync(join(extensionRoot, '.sc-build'))
    writeFileSync(
      helperPath,
      '#!/bin/sh\nprintf \'%s\\n\' \'{"ok":true,"value":"recognized fixture text"}\'\n'
    )
    chmodSync(helperPath, 0o755)
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'screenocr-fixture',
        title: 'ScreenOCR Fixture',
        commands: [{ name: 'index', title: 'Index', mode: 'no-view' }],
      })
    )
    writeFileSync(
      join(extensionRoot, '.sc-build', 'index.js'),
      `const { Clipboard } = require('@raycast/api')
       const { recognizeText } = require('swift:../swift')
       module.exports.default = async function Command() {
         await Clipboard.copy(await recognizeText(true, false, true, false, false, [], ['en-US'], false))
       }`
    )

    const previousHelperPath = process.env.SCREENOCR_HELPER_PATH
    process.env.SCREENOCR_HELPER_PATH = helperPath
    try {
      const result = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index',
        undefined,
        undefined,
        { effectMode: 'record' }
      )
      expect(result.ok, JSON.stringify(result)).toBe(true)
      if (!result.ok || result.mode !== 'no-view') return
      expect(result.effects).toContainEqual({
        kind: 'clipboard',
        value: 'recognized fixture text',
      })
    } finally {
      if (previousHelperPath === undefined) delete process.env.SCREENOCR_HELPER_PATH
      else process.env.SCREENOCR_HELPER_PATH = previousHelperPath
      rmSync(extensionRoot, { recursive: true, force: true })
    }
  })

  it('supports form, frecency, Google OAuth, and SQL utility APIs', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'raymes-utils-extension-'))
    const databasePath = join(extensionRoot, 'fixture.sqlite')
    mkdirSync(join(extensionRoot, 'src'))
    execFileSync('/usr/bin/sqlite3', [databasePath, 'create table items (name text); insert into items values ("ready");'])
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'utils-fixture',
        title: 'Utils Fixture',
        commands: [{ name: 'index', title: 'Index', mode: 'view' }],
      })
    )
    writeFileSync(
      join(extensionRoot, 'src', 'index.tsx'),
      `import React from 'react'
       import { Detail } from '@raycast/api'
       import { OAuthService, useForm, useFrecencySorting, useSQL } from '@raycast/utils'
       export default function Command() {
         const form = useForm({ initialValues: { query: '' }, validation: { query: (value) => value ? null : 'Required' }, onSubmit: () => true })
         const ranking = useFrecencySorting([{ id: 'one' }])
         const sql = useSQL(${JSON.stringify(databasePath)}, 'select name from items')
         const google = OAuthService.google({ clientId: 'fixture', scope: 'openid' })
         return <Detail markdown={[typeof form.handleSubmit, ranking.data.length, typeof google.authorize, sql.data?.[0]?.name || 'loading'].join(':')} />
       }`
    )

    try {
      const initial = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index'
      )
      expect(initial.ok).toBe(true)
      if (!initial.ok || initial.mode !== 'view') return
      expect(initial.root.props.markdown).toBe('function:1:function:loading')

      await new Promise((resolve) => setTimeout(resolve, 25))
      const refreshed = await refreshExtensionSession({ sessionId: initial.sessionId })
      expect(refreshed.ok).toBe(true)
      if (!refreshed.ok || refreshed.mode !== 'view') return
      expect(refreshed.root.props.markdown).toBe('function:1:function:ready')
    } finally {
      rmSync(extensionRoot, { recursive: true, force: true })
    }
  })

  it('exposes every client-filtered item while search is active', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'raymes-search-extension-'))
    mkdirSync(join(extensionRoot, 'src'))
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'search-fixture',
        title: 'Search Fixture',
        commands: [{ name: 'index', title: 'Index', mode: 'view' }],
      })
    )
    writeFileSync(
      join(extensionRoot, 'src', 'index.tsx'),
      `import React from 'react'
       import { List } from '@raycast/api'
       export default function Command() {
         return <List>{Array.from({ length: 31 }, (_, index) =>
           <List.Item key={index} title={index === 30 ? 'Cursor' : 'App ' + index} />
         )}</List>
       }`
    )

    try {
      const initial = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index'
      )
      expect(initial.ok, JSON.stringify(initial)).toBe(true)
      if (!initial.ok || initial.mode !== 'view') return
      expect(initial.root.children).toHaveLength(30)
      expect(initial.root.props.__hasMore).toBe(true)

      const searched = await updateSearchText({
        sessionId: initial.sessionId,
        searchText: 'cursor',
      })
      expect(searched.ok, JSON.stringify(searched)).toBe(true)
      if (!searched.ok || searched.mode !== 'view') return
      expect(searched.root.children).toHaveLength(31)
      expect(searched.root.children.at(-1)?.props.title).toBe('Cursor')
      expect(searched.root.props.__hasMore).toBe(false)

      const cleared = await updateSearchText({ sessionId: initial.sessionId, searchText: '' })
      expect(cleared.ok, JSON.stringify(cleared)).toBe(true)
      if (!cleared.ok || cleared.mode !== 'view') return
      expect(cleared.root.children).toHaveLength(30)
      expect(cleared.root.props.__hasMore).toBe(true)
    } finally {
      rmSync(extensionRoot, { recursive: true, force: true })
    }
  })

  it('keeps actions on the final visible row', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'raymes-list-extension-'))
    mkdirSync(join(extensionRoot, 'src'))
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'pagination-fixture',
        title: 'Pagination Fixture',
        commands: [{ name: 'index', title: 'Index', mode: 'view' }],
      })
    )
    writeFileSync(
      join(extensionRoot, 'src', 'index.tsx'),
      `import React from 'react'
       import { Action, ActionPanel, List } from '@raycast/api'
       export default function Command() {
         return <List>{Array.from({ length: 31 }, (_, index) =>
           <List.Item key={index} title={String(index)} actions={
             <ActionPanel><Action.CopyToClipboard title={'Copy ' + index} content={String(index)} /></ActionPanel>
           } />
         )}</List>
       }`
    )

    try {
      const result = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index'
      )
      expect(result.ok, JSON.stringify(result)).toBe(true)
      if (!result.ok || result.mode !== 'view') return

      expect(result.root.children).toHaveLength(30)
      expect(result.actions).toHaveLength(30)
      for (const child of result.root.children) {
        expect(child.props.actionIds).toHaveLength(1)
      }
    } finally {
      rmSync(extensionRoot, { recursive: true, force: true })
    }
  })

  it('reports component render failures instead of returning an empty success view', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'raymes-broken-extension-'))
    mkdirSync(join(extensionRoot, 'src'))
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'broken-fixture',
        title: 'Broken Fixture',
        commands: [{ name: 'index', title: 'Index', mode: 'view' }],
      })
    )
    writeFileSync(
      join(extensionRoot, 'src', 'index.tsx'),
      `import React from 'react'
       import { List } from '@raycast/api'
       function Broken() { throw new Error('fixture exploded') }
       export default function Command() { return <List><Broken /></List> }`
    )

    try {
      const result = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index'
      )
      expect(result).toEqual({
        ok: false,
        message: 'Extension render failed: fixture exploded',
      })
    } finally {
      rmSync(extensionRoot, { recursive: true, force: true })
    }
  })

  it('shares the page limit across list sections', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'raymes-section-extension-'))
    mkdirSync(join(extensionRoot, 'src'))
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'section-fixture',
        title: 'Section Fixture',
        commands: [{ name: 'index', title: 'Index', mode: 'view' }],
      })
    )
    writeFileSync(
      join(extensionRoot, 'src', 'index.tsx'),
      `import React from 'react'
       import { Action, ActionPanel, List } from '@raycast/api'
       const rows = (prefix: string, count: number) => Array.from({ length: count }, (_, index) =>
         <List.Item key={prefix + index} title={prefix + index} actions={
           <ActionPanel><Action.CopyToClipboard title={'Copy ' + index} content={String(index)} /></ActionPanel>
         } />
       )
       export default function Command() {
         return <List><List.Section title="First">{rows('a', 5)}</List.Section><List.Section title="Second">{rows('b', 31)}</List.Section></List>
       }`
    )

    try {
      const result = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index'
      )
      expect(result.ok, JSON.stringify(result)).toBe(true)
      if (!result.ok || result.mode !== 'view') return

      expect(result.root.children).toHaveLength(2)
      expect(result.root.children.reduce((sum, section) => sum + section.children.length, 0)).toBe(
        30
      )
      expect(result.root.props.__hasMore).toBe(true)
      expect(result.actions).toHaveLength(30)
    } finally {
      rmSync(extensionRoot, { recursive: true, force: true })
    }
  })

  it('uses isolated preferences for explicit package-path runs', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'raymes-preference-extension-'))
    mkdirSync(join(extensionRoot, 'src'))
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'preference-fixture',
        title: 'Preference Fixture',
        commands: [{ name: 'index', title: 'Index', mode: 'view' }],
      })
    )
    writeFileSync(
      join(extensionRoot, 'src', 'index.tsx'),
      `import React from 'react'
       import { Detail, getPreferenceValues } from '@raycast/api'
       export default function Command() {
         const values = getPreferenceValues() as { token?: string }
         return <Detail markdown={values.token || 'missing'} />
       }`
    )

    try {
      const result = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index',
        undefined,
        { token: 'isolated-value' }
      )
      expect(result.ok, JSON.stringify(result)).toBe(true)
      if (!result.ok || result.mode !== 'view') return
      expect(result.root.props.markdown).toBe('isolated-value')
    } finally {
      rmSync(extensionRoot, { recursive: true, force: true })
    }
  })

  it('supports personal-token OAuth services and access-token wrappers', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'raymes-oauth-extension-'))
    mkdirSync(join(extensionRoot, 'src'))
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'oauth-fixture',
        title: 'OAuth Fixture',
        commands: [{ name: 'index', title: 'Index', mode: 'view' }],
      })
    )
    writeFileSync(
      join(extensionRoot, 'src', 'index.tsx'),
      `import React from 'react'
       import { Detail, getPreferenceValues } from '@raycast/api'
       import { getAccessToken, OAuthService, withAccessToken } from '@raycast/utils'
       const preferences = getPreferenceValues() as { token?: string }
       let authorized = ''
       const service = OAuthService.github({
         personalAccessToken: preferences.token,
         onAuthorize: ({ token }: { token: string }) => { authorized = token },
       })
       function Command() {
         return <Detail markdown={getAccessToken().token + ':' + authorized} />
       }
       export default withAccessToken(service)(Command)`
    )

    try {
      const result = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index',
        undefined,
        { token: 'fixture-token' }
      )
      expect(result.ok, JSON.stringify(result)).toBe(true)
      if (!result.ok || result.mode !== 'view') return
      expect(result.root.props.markdown).toBe('fixture-token:fixture-token')
    } finally {
      rmSync(extensionRoot, { recursive: true, force: true })
    }
  })

  it('completes and persists loopback PKCE authorization for Google OAuth services', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'raymes-google-oauth-extension-'))
    mkdirSync(join(extensionRoot, '.sc-build'))
    let authorizationRequests = 0
    let tokenRequests = 0
    const oauthServer = createServer((request, response) => {
      const requestUrl = new URL(request.url ?? '/', 'http://127.0.0.1')
      if (requestUrl.pathname === '/authorize') {
        authorizationRequests += 1
        expect(requestUrl.searchParams.get('client_id')).toBe('fixture-client')
        expect(requestUrl.searchParams.get('code_challenge_method')).toBe('S256')
        expect(requestUrl.searchParams.get('code_challenge')).toBeTruthy()
        const redirectUri = requestUrl.searchParams.get('redirect_uri')
        const state = requestUrl.searchParams.get('state')
        expect(redirectUri).toMatch(/^http:\/\/127\.0\.0\.1:\d+\/oauth\/callback$/)
        response.writeHead(302, {
          location: `${redirectUri}?code=fixture-code&state=${encodeURIComponent(state ?? '')}`,
        })
        response.end()
        return
      }
      if (requestUrl.pathname === '/token') {
        tokenRequests += 1
        let body = ''
        request.on('data', (chunk) => {
          body += String(chunk)
        })
        request.on('end', () => {
          const parameters = new URLSearchParams(body)
          expect(parameters.get('grant_type')).toBe('authorization_code')
          expect(parameters.get('code')).toBe('fixture-code')
          expect(parameters.get('code_verifier')).toBeTruthy()
          response.writeHead(200, { 'content-type': 'application/json' })
          response.end(
            JSON.stringify({
              access_token: 'google-access',
              refresh_token: 'google-refresh',
              expires_in: 3600,
            })
          )
        })
        return
      }
      response.writeHead(404).end()
    })
    await new Promise<void>((resolve) => oauthServer.listen(0, '127.0.0.1', resolve))
    const address = oauthServer.address()
    if (!address || typeof address === 'string') throw new Error('OAuth fixture server failed')
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'google-oauth-fixture',
        title: 'Google OAuth Fixture',
        commands: [{ name: 'index', title: 'Index', mode: 'view' }],
      })
    )
    writeFileSync(
      join(extensionRoot, '.sc-build', 'index.js'),
      `const React = require('react').default
       const { Detail } = require('@raycast/api')
       const { getAccessToken, OAuthService, withAccessToken } = require('@raycast/utils')
       const service = OAuthService.google({
         clientId: 'fixture-client',
         scope: 'openid calendar',
         authorizationEndpoint: 'http://127.0.0.1:${address.port}/authorize',
         tokenEndpoint: 'http://127.0.0.1:${address.port}/token',
         timeoutMs: 5000,
         openAuthorizationUrl: async (url) => {
           const authorization = await fetch(url, { redirect: 'manual' })
           await fetch(authorization.headers.get('location'))
         },
       })
       function Command() {
         return React.createElement(Detail, { markdown: getAccessToken().token })
       }
       module.exports.default = withAccessToken(service)(Command)`
    )

    try {
      const first = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index',
        undefined,
        {}
      )
      expect(first.ok, JSON.stringify(first)).toBe(true)
      if (!first.ok || first.mode !== 'view') return
      expect(first.root.props.markdown).toBe('google-access')
      expect(first.effects).toHaveLength(1)
      expect(first.effects[0]?.kind).toBe('open')

      const second = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index',
        undefined,
        {}
      )
      expect(second.ok, JSON.stringify(second)).toBe(true)
      if (!second.ok || second.mode !== 'view') return
      expect(second.root.props.markdown).toBe('google-access')
      expect(second.effects).toEqual([])
      expect(authorizationRequests).toBe(1)
      expect(tokenRequests).toBe(1)
    } finally {
      await new Promise<void>((resolve) => oauthServer.close(() => resolve()))
      rmSync(extensionRoot, { recursive: true, force: true })
    }
  })

  it('runs AI.ask and useAI through the configured extension AI bridge', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'raymes-ai-extension-'))
    mkdirSync(join(extensionRoot, 'src'))
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'ai-fixture',
        title: 'AI Fixture',
        commands: [{ name: 'index', title: 'Index', mode: 'view' }],
      })
    )
    writeFileSync(
      join(extensionRoot, 'src', 'index.tsx'),
      `import React from 'react'
       import { AI, Detail } from '@raycast/api'
       import { useAI } from '@raycast/utils'
       export default function Command() {
         const hook = useAI('hook prompt') as { data?: string; isLoading: boolean }
         void AI.ask('direct prompt')
         return <Detail markdown={hook.isLoading ? 'loading' : hook.data} />
       }`
    )

    try {
      const initial = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index'
      )
      expect(initial.ok, JSON.stringify(initial)).toBe(true)
      if (!initial.ok || initial.mode !== 'view') return
      await new Promise((resolve) => setTimeout(resolve, 5))
      const refreshed = await import('./extension-runner').then(({ refreshExtensionSession }) =>
        refreshExtensionSession({ sessionId: initial.sessionId })
      )
      expect(refreshed.ok, JSON.stringify(refreshed)).toBe(true)
      if (!refreshed.ok || refreshed.mode !== 'view') return
      expect(refreshed.root.props.markdown).toBe('AI:hook prompt')
    } finally {
      rmSync(extensionRoot, { recursive: true, force: true })
    }
  })

  it('keeps a pushed target when an initial effect also updates state', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'raymes-push-extension-'))
    mkdirSync(join(extensionRoot, 'src'))
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'push-fixture',
        title: 'Push Fixture',
        commands: [{ name: 'index', title: 'Index', mode: 'view' }],
      })
    )
    writeFileSync(
      join(extensionRoot, 'src', 'index.tsx'),
      `import React, { useEffect, useState } from 'react'
       import { Action, ActionPanel, Form, List } from '@raycast/api'
       export default function Command() {
         const [, setReady] = useState(false)
         useEffect(() => { void Promise.resolve().then(() => setReady(true)) }, [])
         return <List><List.Item title="Create" actions={<ActionPanel>
           <Action.Push title="Open Form" target={<Form><Form.TextField id="name" /></Form>} />
         </ActionPanel>} /></List>
       }`
    )

    try {
      const initial = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index',
        undefined,
        {}
      )
      expect(initial.ok, JSON.stringify(initial)).toBe(true)
      if (!initial.ok || initial.mode !== 'view') return
      await Promise.resolve()
      const pushed = await invokeExtensionAction({
        sessionId: initial.sessionId,
        actionId: initial.actions[0].id,
      })
      expect(pushed.ok, JSON.stringify(pushed)).toBe(true)
      if (!pushed.ok || pushed.mode !== 'view') return
      expect(pushed.root.type).toBe('Form')
    } finally {
      rmSync(extensionRoot, { recursive: true, force: true })
    }
  })

  it('records no-view effects without touching the desktop', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'raymes-effect-extension-'))
    mkdirSync(join(extensionRoot, 'src'))
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'effect-fixture',
        title: 'Effect Fixture',
        commands: [{ name: 'index', title: 'Index', mode: 'no-view' }],
      })
    )
    writeFileSync(
      join(extensionRoot, 'src', 'index.ts'),
      `import { Clipboard, open, showHUD, showToast, Toast } from '@raycast/api'
       export default async function Command() {
         await Clipboard.copy('fixture text')
         await open('https://example.com/path')
         await showToast({ style: Toast.Style.Success, title: 'Finished' })
         await showHUD('All done')
       }`
    )

    try {
      const result = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index',
        undefined,
        {},
        { effectMode: 'record' }
      )
      expect(result.ok, JSON.stringify(result)).toBe(true)
      if (!result.ok || result.mode !== 'no-view') return
      expect(result.effects).toEqual([
        { kind: 'clipboard', value: 'fixture text' },
        { kind: 'open', value: 'https://example.com/path' },
        { kind: 'toast', style: 'success', title: 'Finished', message: undefined },
        { kind: 'hud', message: 'All done' },
      ])
      expect(result.message).toBe('All done')
    } finally {
      rmSync(extensionRoot, { recursive: true, force: true })
    }
  })

  it('persists PKCE token responses for OAuth extensions', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'raymes-pkce-extension-'))
    mkdirSync(join(extensionRoot, 'src'))
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'pkce-fixture',
        title: 'PKCE Fixture',
        commands: [{ name: 'index', title: 'Index', mode: 'view' }],
      })
    )
    writeFileSync(
      join(extensionRoot, 'src', 'index.tsx'),
      `import React from 'react'
       import { Detail, OAuth } from '@raycast/api'
       const client = new OAuth.PKCEClient({ providerId: 'fixture', providerName: 'Fixture' })
       export default async function Command() {
         await client.setTokens({ access_token: 'access', refresh_token: 'refresh', expires_in: 3600 })
         const tokens = await client.getTokens()
         return <Detail markdown={tokens?.accessToken + ':' + tokens?.refreshToken + ':' + tokens?.isExpired()} />
       }`
    )

    try {
      const result = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index',
        undefined,
        {}
      )
      expect(result.ok, JSON.stringify(result)).toBe(true)
      if (!result.ok || result.mode !== 'view') return
      expect(result.root.props.markdown).toBe('access:refresh:false')
    } finally {
      rmSync(extensionRoot, { recursive: true, force: true })
    }
  })

  it('defers effects until the component render has completed', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'raymes-effect-order-extension-'))
    mkdirSync(join(extensionRoot, 'src'))
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'effect-order-fixture',
        title: 'Effect Order Fixture',
        commands: [{ name: 'index', title: 'Index', mode: 'view' }],
      })
    )
    writeFileSync(
      join(extensionRoot, 'src', 'index.tsx'),
      `import React, { useEffect, useState } from 'react'
       import { Detail } from '@raycast/api'
       export default function Command() {
         const [value, setValue] = useState('initial')
         useEffect(() => setValue(readValue()), [])
         const readValue = () => 'ready'
         return <Detail markdown={value} />
       }`
    )

    try {
      const initial = await runExtensionCommandFromPackageJson(
        join(extensionRoot, 'package.json'),
        'index',
        undefined,
        {}
      )
      expect(initial.ok, JSON.stringify(initial)).toBe(true)
      if (!initial.ok || initial.mode !== 'view') return
      expect(initial.root.props.markdown).toBe('initial')
      const refreshed = await import('./extension-runner').then(({ refreshExtensionSession }) =>
        refreshExtensionSession({ sessionId: initial.sessionId })
      )
      expect(refreshed.ok, JSON.stringify(refreshed)).toBe(true)
      if (!refreshed.ok || refreshed.mode !== 'view') return
      expect(refreshed.root.props.markdown).toBe('ready')
    } finally {
      rmSync(extensionRoot, { recursive: true, force: true })
    }
  })
})
