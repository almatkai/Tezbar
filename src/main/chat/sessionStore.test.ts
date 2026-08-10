import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'

const appDataDirectory = mkdtempSync(join(tmpdir(), 'tezbar-chat-store-'))
const previousAppDataDirectory = process.env.APPDATA_DIR

beforeAll(() => {
  process.env.APPDATA_DIR = appDataDirectory
})

afterAll(() => {
  if (previousAppDataDirectory === undefined) delete process.env.APPDATA_DIR
  else process.env.APPDATA_DIR = previousAppDataDirectory
  rmSync(appDataDirectory, { recursive: true, force: true })
})

describe('chat session store', () => {
  it('persists the Pi working directory with session history', async () => {
    const { getChatSession, listChatSessions, upsertChatSession } = await import('./sessionStore')
    await upsertChatSession({
      id: 'directory-chat',
      title: 'Directory chat',
      createdAt: 1,
      updatedAt: 1,
      workingDirectory: '/Users/dev/Desktop/code/aml',
    })

    await expect(getChatSession('directory-chat')).resolves.toMatchObject({
      id: 'directory-chat',
      workingDirectory: '/Users/dev/Desktop/code/aml',
    })
    await expect(listChatSessions()).resolves.toEqual([
      expect.objectContaining({
        id: 'directory-chat',
        workingDirectory: '/Users/dev/Desktop/code/aml',
      }),
    ])
  })
})
