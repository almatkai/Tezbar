declare module 'better-sqlite3' {
  type BindValue = string | number | bigint | Buffer | null | undefined

  export type Statement = {
    run: (...params: BindValue[]) => { changes: number; lastInsertRowid: number | bigint }
    get: (...params: BindValue[]) => unknown
    all: (...params: BindValue[]) => unknown[]
  }

  export type Database = {
    pragma: (value: string) => void
    exec: (sql: string) => void
    prepare: (sql: string) => Statement
    transaction: <T extends unknown[]>(fn: (...args: T) => void) => (...args: T) => void
  }

  const DatabaseCtor: {
    new (filename: string): Database
  }

  export default DatabaseCtor
}
