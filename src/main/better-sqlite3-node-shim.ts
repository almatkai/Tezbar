import { DatabaseSync } from 'node:sqlite'

type BindValue = string | number | bigint | Buffer | null | undefined
type NodeBindValue = string | number | bigint | Uint8Array | null

function normalizeParams(params: BindValue[]): NodeBindValue[] {
  return params.map((value) => (value === undefined ? null : value))
}

class StatementShim {
  constructor(private readonly statement: ReturnType<DatabaseSync['prepare']>) {}

  run(...params: BindValue[]): { changes: number; lastInsertRowid: number | bigint } {
    const result = this.statement.run(...normalizeParams(params))
    return { changes: Number(result.changes), lastInsertRowid: result.lastInsertRowid }
  }

  get(...params: BindValue[]): unknown {
    return this.statement.get(...normalizeParams(params))
  }

  all(...params: BindValue[]): unknown[] {
    return this.statement.all(...normalizeParams(params))
  }
}

class DatabaseShim {
  private readonly database: DatabaseSync

  constructor(filename: string) {
    this.database = new DatabaseSync(filename, { allowExtension: true })
  }

  pragma(value: string): void {
    this.database.exec(`PRAGMA ${value}`)
  }

  exec(sql: string): void {
    this.database.exec(sql)
  }

  prepare(sql: string): StatementShim {
    return new StatementShim(this.database.prepare(sql))
  }

  loadExtension(path: string): void {
    this.database.loadExtension(path)
  }

  transaction<T extends unknown[]>(fn: (...args: T) => void): (...args: T) => void {
    return (...args: T): void => {
      this.database.exec('BEGIN IMMEDIATE')
      try {
        fn(...args)
        this.database.exec('COMMIT')
      } catch (error) {
        this.database.exec('ROLLBACK')
        throw error
      }
    }
  }
}

export default DatabaseShim
