import { useMemo, useState } from 'react'
import type { ExtensionRuntimeNode } from '../../../shared/extensionRuntime'

type FormField = {
  key: string
  type: string
  id: string
  title: string
  placeholder?: string
  options?: Array<{ title: string; value: string }>
  allowMultipleSelection?: boolean
}

function collectFormFields(root: ExtensionRuntimeNode): FormField[] {
  const out: FormField[] = []

  const walk = (node: ExtensionRuntimeNode): void => {
    const type = node.type
    const isField =
      type === 'Form.TextField' ||
      type === 'Form.TextArea' ||
      type === 'Form.Checkbox' ||
      type === 'Form.Dropdown' ||
      type === 'Form.TagPicker' ||
      type === 'Form.FilePicker' ||
      type === 'Form.DatePicker' ||
      type === 'Form.PasswordField'

    if (isField) {
      const id = typeof node.props?.id === 'string' && node.props.id.trim().length > 0
        ? node.props.id
        : `${type}:${out.length}`
      const title = typeof node.props?.title === 'string' ? node.props.title : id
      const placeholder = typeof node.props?.placeholder === 'string' ? node.props.placeholder : undefined
      const optionData: unknown[] = Array.isArray(node.props?.data) ? node.props.data : []
      if (!Array.isArray(node.props?.data)) {
        for (const child of node.children ?? []) {
          if (child.type.endsWith('.Item')) optionData.push(child.props)
        }
      }
      const options: Array<{ title: string; value: string }> = []
      for (const entry of optionData) {
        const option = entry as { title?: unknown; value?: unknown }
        const optionTitle = typeof option.title === 'string' ? option.title : ''
        const value = typeof option.value === 'string' ? option.value : optionTitle
        if (optionTitle || value) options.push({ title: optionTitle || value, value })
      }

      out.push({
        key: `${id}:${type}`,
        type,
        id,
        title,
        placeholder,
        options: options.length > 0 ? options : undefined,
        allowMultipleSelection: node.props?.allowMultipleSelection === true,
      })
    }

    for (const child of node.children ?? []) {
      walk(child)
    }
  }

  walk(root)
  return out
}

export function FormRuntime({
  root,
  title,
  onBack,
  onSubmitForm,
  onOpenActions,
}: {
  root: ExtensionRuntimeNode
  title: string
  onBack: () => void
  onSubmitForm: (values: Record<string, unknown>) => void
  onOpenActions: () => void
}): JSX.Element {
  const fields = useMemo(() => collectFormFields(root), [root])
  const [values, setValues] = useState<Record<string, unknown>>({})

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="glass-card mb-2 shrink-0 px-3 py-2">
        <div className="flex items-center gap-2">
          <button type="button" className="btn btn-ghost" onClick={onBack}>
            Back
          </button>
          <div className="text-[12px] font-semibold text-ink-2">{title}</div>
          <div className="ml-auto flex items-center gap-1">
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => onSubmitForm(values)}
            >
              Submit
            </button>
            <button type="button" className="btn btn-ghost" onClick={onOpenActions}>
              Cmd+K
            </button>
          </div>
        </div>
      </div>

      <div className="glass-card min-h-0 flex-1 overflow-y-auto px-4 py-3">
        <div className="mx-auto max-w-2xl space-y-3">
          {fields.map((field) => {
            const rawValue = values[field.id]
            const current = typeof rawValue === 'string' ? rawValue : ''

            if (field.type === 'Form.TextArea') {
              return (
                <label key={field.key} className="block">
                  <span className="mb-1 block text-[11px] text-ink-3">{field.title}</span>
                  <textarea
                    value={current}
                    onChange={(event) => {
                      const next = event.target.value
                      setValues((prev) => ({ ...prev, [field.id]: next }))
                    }}
                    placeholder={field.placeholder}
                    className="glass-field min-h-[80px]"
                  />
                </label>
              )
            }

            if (field.type === 'Form.Checkbox') {
              const checked = rawValue === true
              return (
                <label key={field.key} className="flex items-center gap-2 rounded-tezbar-row bg-white/[0.03] px-3 py-2">
                  <input
                    type="checkbox"
                    checked={checked}
                    onChange={(event) => {
                      setValues((prev) => ({ ...prev, [field.id]: event.target.checked }))
                    }}
                  />
                  <span className="text-[12px] text-ink-2">{field.title}</span>
                </label>
              )
            }

            if (field.type === 'Form.Dropdown') {
              return (
                <label key={field.key} className="block">
                  <span className="mb-1 block text-[11px] text-ink-3">{field.title}</span>
                  <select
                    value={current}
                    onChange={(event) => {
                      const next = event.target.value
                      setValues((prev) => ({ ...prev, [field.id]: next }))
                    }}
                    className="glass-field"
                  >
                    <option value="">Select</option>
                    {(field.options ?? []).map((option) => (
                      <option key={`${field.id}:${option.value}`} value={option.value}>
                        {option.title}
                      </option>
                    ))}
                  </select>
                </label>
              )
            }

            if (field.type === 'Form.TagPicker') {
              const selected = Array.isArray(rawValue)
                ? rawValue.filter((value): value is string => typeof value === 'string')
                : []
              return (
                <label key={field.key} className="block">
                  <span className="mb-1 block text-[11px] text-ink-3">{field.title}</span>
                  <select
                    multiple
                    value={selected}
                    onChange={(event) => {
                      const next = Array.from(event.target.selectedOptions, (option) => option.value)
                      setValues((prev) => ({ ...prev, [field.id]: next }))
                    }}
                    className="glass-field min-h-[96px]"
                  >
                    {(field.options ?? []).map((option) => (
                      <option key={`${field.id}:${option.value}`} value={option.value}>
                        {option.title}
                      </option>
                    ))}
                  </select>
                </label>
              )
            }

            if (field.type === 'Form.FilePicker') {
              const selected = Array.isArray(rawValue)
                ? rawValue.filter((value): value is string => typeof value === 'string')
                : []
              return (
                <label key={field.key} className="block">
                  <span className="mb-1 block text-[11px] text-ink-3">{field.title}</span>
                  <input
                    type="file"
                    multiple={field.allowMultipleSelection}
                    onChange={(event) => {
                      const next = Array.from(event.target.files ?? [], (file) => {
                        const electronFile = file as File & { path?: string }
                        return electronFile.path || file.name
                      })
                      setValues((prev) => ({ ...prev, [field.id]: next }))
                    }}
                    className="glass-field"
                  />
                  {selected.length > 0 ? (
                    <span className="mt-1 block truncate text-[10px] text-ink-4">
                      {selected.join(', ')}
                    </span>
                  ) : null}
                </label>
              )
            }

            const inputType =
              field.type === 'Form.PasswordField'
                ? 'password'
                : field.type === 'Form.DatePicker'
                  ? 'date'
                  : 'text'

            return (
              <label key={field.key} className="block">
                <span className="mb-1 block text-[11px] text-ink-3">{field.title}</span>
                <input
                  type={inputType}
                  value={current}
                  onChange={(event) => {
                    const next = event.target.value
                    setValues((prev) => ({ ...prev, [field.id]: next }))
                  }}
                  placeholder={field.placeholder}
                  className="glass-field"
                />
              </label>
            )
          })}

          {fields.length === 0 ? (
            <div className="rounded-tezbar-row bg-white/[0.03] px-3 py-4 text-[12px] text-ink-3">
              This form did not expose any editable fields.
            </div>
          ) : null}
        </div>
      </div>
    </div>
  )
}
