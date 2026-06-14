import { useMemo, useState } from 'react'
import type { ExtensionRuntimeNode } from '../../../shared/extensionRuntime'

type FormField = {
  key: string
  type: string
  id: string
  title: string
  placeholder?: string
  options?: Array<{ title: string; value: string }>
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
      type === 'Form.DatePicker' ||
      type === 'Form.PasswordField'

    if (isField) {
      const id = typeof node.props?.id === 'string' && node.props.id.trim().length > 0
        ? node.props.id
        : `${type}:${out.length}`
      const title = typeof node.props?.title === 'string' ? node.props.title : id
      const placeholder = typeof node.props?.placeholder === 'string' ? node.props.placeholder : undefined
      const options = Array.isArray(node.props?.data)
        ? node.props.data
          .map((entry) => {
            const title = typeof (entry as { title?: unknown }).title === 'string'
              ? String((entry as { title?: unknown }).title)
              : ''
            const value = typeof (entry as { value?: unknown }).value === 'string'
              ? String((entry as { value?: unknown }).value)
              : title
            if (!title && !value) return null
            return { title: title || value, value }
          })
          .filter((entry): entry is { title: string; value: string } => entry !== null)
        : undefined

      out.push({
        key: `${id}:${type}`,
        type,
        id,
        title,
        placeholder,
        options,
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
  onSubmitForm: (values: Record<string, string>) => void
  onOpenActions: () => void
}): JSX.Element {
  const fields = useMemo(() => collectFormFields(root), [root])
  const [values, setValues] = useState<Record<string, string>>({})

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
            const current = values[field.id] ?? ''

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
              const checked = current === 'true'
              return (
                <label key={field.key} className="flex items-center gap-2 rounded-tezbar-row bg-white/[0.03] px-3 py-2">
                  <input
                    type="checkbox"
                    checked={checked}
                    onChange={(event) => {
                      setValues((prev) => ({ ...prev, [field.id]: event.target.checked ? 'true' : 'false' }))
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
