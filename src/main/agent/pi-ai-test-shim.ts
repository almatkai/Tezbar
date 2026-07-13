type Schema = Record<string, unknown>

export const Type = {
  Object(properties: Record<string, unknown>): Schema {
    return { type: 'object', properties }
  },
  String(options: Schema = {}): Schema {
    return { type: 'string', ...options }
  },
  Number(options: Schema = {}): Schema {
    return { type: 'number', ...options }
  },
  Optional(schema: Schema): Schema {
    return schema
  },
}
