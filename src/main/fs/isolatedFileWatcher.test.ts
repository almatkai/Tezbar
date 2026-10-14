import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { expect, it } from 'vitest'
import { watchFolder } from './isolatedFileWatcher'

it('shares recursive watches and lets a remaining subscriber receive nested file changes', async () => {
  const root = await mkdtemp(join(tmpdir(), 'tezbar-watch-'))
  const events: string[] = []
  const stopFirst = watchFolder(root, () => {})
  const stopSecond = watchFolder(root, (filename) => {
    if (filename) events.push(filename)
  })
  try {
    await new Promise((resolve) => setTimeout(resolve, 250))
    stopFirst()
    await mkdir(join(root, 'nested'))
    const deadline = Date.now() + 5_000
    while (!events.some((name) => name.endsWith('document.txt')) && Date.now() < deadline) {
      await writeFile(join(root, 'nested/document.txt'), 'watch progress')
      await new Promise((resolve) => setTimeout(resolve, 100))
    }
    expect(events.some((name) => name.endsWith('document.txt'))).toBe(true)
  } finally {
    stopFirst()
    stopSecond()
    await rm(root, { recursive: true, force: true })
  }
})
