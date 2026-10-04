import Database from 'better-sqlite3'
/** Raycast SQL APIs are read-only: never create or mutate a user's database. */
export async function executeExtensionSQL<T = unknown>(
  databasePath: string,
  query: string
): Promise<T[]> {
  const database = new Database(databasePath, { readonly: true, fileMustExist: true })
  try {
    return database.prepare(query).all() as T[]
  } finally {
    database.close()
  }
}
