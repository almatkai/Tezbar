export type PreferenceField = {
  name?: string
  title?: string
  description?: string
  type?: string
  required?: boolean
  data?: Array<{ title?: string; value?: string }>
  commandName?: string
  commandTitle?: string
}

export type PreferenceSetup = {
  preferences: PreferenceField[]
  values: Record<string, unknown>
}

export function preferenceValueKey(field: PreferenceField): string {
  return `${field.commandName ?? '$extension'}:${field.name ?? ''}`
}

export function mergePreferenceSetups(
  extensionSetup: PreferenceSetup,
  commandSetups: Array<PreferenceSetup | null>,
): { fields: PreferenceField[]; values: Record<string, unknown> } {
  const fields = [...extensionSetup.preferences]
  const values: Record<string, unknown> = {}
  for (const field of extensionSetup.preferences) {
    if (field.name) values[preferenceValueKey(field)] = extensionSetup.values[field.name]
  }
  for (const setup of commandSetups) {
    if (!setup) continue
    for (const field of setup.preferences) {
      if (!field.name || !field.commandName) continue
      fields.push(field)
      values[preferenceValueKey(field)] = setup.values[field.name]
    }
  }
  return { fields, values }
}

export function partitionPreferenceValues(
  fields: PreferenceField[],
  editorValues: Record<string, unknown>,
): Array<{ commandName?: string; values: Record<string, unknown> }> {
  const scopes = new Map<string | undefined, Record<string, unknown>>()
  for (const field of fields) {
    if (!field.name) continue
    const values = scopes.get(field.commandName) ?? {}
    values[field.name] = editorValues[preferenceValueKey(field)]
    scopes.set(field.commandName, values)
  }
  return [...scopes].map(([commandName, values]) => ({ commandName, values }))
}
