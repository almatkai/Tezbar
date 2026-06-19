/* eslint-disable @typescript-eslint/no-explicit-any */
// src/main/better-sqlite3-shim.ts
// Provides a better-sqlite3-compatible API backed by bun:sqlite.
// Used when the backend is bundled for Bun (Tauri sidecar mode).

// @ts-expect-error -- bun:sqlite is only available at runtime under Bun.
import { Database as BunDatabase } from 'bun:sqlite'

type BindValue = string | number | bigint | Buffer | null | undefined

interface Statement {
  run(...params: BindValue[]): { changes: number; lastInsertRowid: number | bigint }
  get(...params: BindValue[]): unknown
  all(...params: BindValue[]): unknown[]
}

interface DatabaseLike {
  pragma(value: string): void
  exec(sql: string): void
  prepare(sql: string): Statement
  transaction<T extends unknown[]>(fn: (...args: T) => void): (...args: T) => void
}

class StatementShim implements Statement {
  private _stmt: any

  constructor(stmt: any) {
    this._stmt = stmt
  }

  run(...params: BindValue[]): { changes: number; lastInsertRowid: number | bigint } {
    const result = this._stmt.run(...params)
    return {
      changes: result?.changes ?? 0,
      lastInsertRowid: result?.lastInsertRowid ?? 0
    }
  }

  get(...params: BindValue[]): unknown {
    return this._stmt.get(...params) ?? undefined
  }

  all(...params: BindValue[]): unknown[] {
    return this._stmt.all(...params) ?? []
  }
}

class DatabaseShim implements DatabaseLike {
  private _db: any

  constructor(filename: string) {
    this._db = new BunDatabase(filename)
  }

  pragma(value: string): void {
    // bun:sqlite supports PRAGMA via exec
    this._db.exec(`PRAGMA ${value}`)
  }

  exec(sql: string): void {
    this._db.exec(sql)
  }

  prepare(sql: string): Statement {
    const stmt = this._db.prepare(sql)
    return new StatementShim(stmt)
  }

  transaction<T extends unknown[]>(fn: (...args: T) => void): (...args: T) => void {
    return this._db.transaction(fn)
  }
}

export type { DatabaseLike as Database }
export default DatabaseShim
