import Database from 'better-sqlite3'
import { existsSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, expect, it } from 'vitest'

const directories: string[] = []
afterEach(() => {
  for (const directory of directories.splice(0)) rmSync(directory, { recursive: true, force: true })
})

it('reads extension databases without a sqlite3 CLI and enforces readonly access', () => {
  const directory = mkdtempSync(join(tmpdir(), 'tezbar-sql-readonly-'))
  directories.push(directory)
  const filename = join(directory, 'state.vscdb')
  const writable = new Database(filename)
  writable.exec(
    "CREATE TABLE ItemTable (key TEXT, value TEXT); INSERT INTO ItemTable VALUES ('recent', 'こんにちは');"
  )
  writable.close()
  const readonly = new Database(filename, { readonly: true, fileMustExist: true })
  try {
    expect(readonly.prepare('SELECT * FROM ItemTable').all()).toEqual([
      { key: 'recent', value: 'こんにちは' },
    ])
    expect(() => readonly.prepare('DELETE FROM ItemTable').run()).toThrow(/readonly|read-only/i)
  } finally {
    readonly.close()
  }
})

it('does not create a missing extension database', () => {
  const directory = mkdtempSync(join(tmpdir(), 'tezbar-sql-missing-'))
  directories.push(directory)
  const filename = join(directory, 'missing.vscdb')
  expect(() => new Database(filename, { readonly: true, fileMustExist: true })).toThrow()
  expect(existsSync(filename)).toBe(false)
})
