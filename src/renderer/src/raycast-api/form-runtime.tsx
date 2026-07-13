import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import type { ExtensionRuntimeNode } from '../../../shared/extensionRuntime'
import { Hint, HintBar, Kbd, ViewHeader } from '../../ui/primitives'

type FormField = {
  key: string
  type: string
  id: string
  title: string
  label?: string
  placeholder?: string
  info?: string
  error?: string
  value?: unknown
  options?: Array<{ title: string; value: string }>
  allowMultipleSelection?: boolean
  actionId?: string
}

type FormItem =
  | { key: string; type: 'separator' }
  | { key: string; type: 'description'; title?: string; text?: string }
  | { key: string; type: 'field'; field: FormField }

const FIELD_ROW_CLASS = 'grid items-start gap-2 sm:grid-cols-[180px_minmax(0,1fr)] sm:gap-5'
const FIELD_TITLE_CLASS =
  'pt-2 text-left text-[11.5px] font-medium leading-5 text-ink-3 sm:text-right'

function collectOptionData(node: ExtensionRuntimeNode): Array<{ title: string; value: string }> {
  const options: Array<{ title: string; value: string }> = []

  const addOption = (entry: unknown): void => {
    const option = entry as { title?: unknown; value?: unknown }
    const optionTitle = typeof option.title === 'string' ? option.title : ''
    const value = typeof option.value === 'string' ? option.value : optionTitle
    if (optionTitle || value) options.push({ title: optionTitle || value, value })
  }

  if (Array.isArray(node.props?.data)) {
    for (const entry of node.props.data) addOption(entry)
  }

  const walkChildren = (children: ExtensionRuntimeNode[] | undefined): void => {
    for (const child of children ?? []) {
      if (child.type.endsWith('.Item')) addOption(child.props)
      else walkChildren(child.children)
    }
  }
  walkChildren(node.children)

  return options
}

function collectFormItems(root: ExtensionRuntimeNode): FormItem[] {
  const out: FormItem[] = []

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
      const id =
        typeof node.props?.id === 'string' && node.props.id.trim().length > 0
          ? node.props.id
          : `${type}:${out.length}`
      const title = typeof node.props?.title === 'string' ? node.props.title : id
      const placeholder =
        typeof node.props?.placeholder === 'string' ? node.props.placeholder : undefined
      const options = collectOptionData(node)

      out.push({
        key: `${id}:${type}`,
        type: 'field',
        field: {
          key: `${id}:${type}`,
          type,
          id,
          title,
          label: typeof node.props?.label === 'string' ? node.props.label : undefined,
          placeholder,
          info: typeof node.props?.info === 'string' ? node.props.info : undefined,
          error: typeof node.props?.error === 'string' ? node.props.error : undefined,
          value: node.props?.value,
          options: options.length > 0 ? options : undefined,
          allowMultipleSelection: node.props?.allowMultipleSelection === true,
          actionId: typeof node.props?.actionId === 'string' ? node.props.actionId : undefined,
        },
      })
      return
    }

    if (type === 'Form.Separator') {
      out.push({ key: `separator:${out.length}`, type: 'separator' })
      return
    }

    if (type === 'Form.Description') {
      out.push({
        key: `description:${out.length}`,
        type: 'description',
        title: typeof node.props?.title === 'string' ? node.props.title : undefined,
        text: typeof node.props?.text === 'string' ? node.props.text : undefined,
      })
      return
    }

    for (const child of node.children ?? []) {
      walk(child)
    }
  }

  walk(root)
  return out
}

function initialValues(
  fields: FormField[],
  previous?: Record<string, unknown>
): Record<string, unknown> {
  const next: Record<string, unknown> = {}

  for (const field of fields) {
    if (field.value !== undefined) {
      next[field.id] = field.value
    } else if (previous && Object.prototype.hasOwnProperty.call(previous, field.id)) {
      next[field.id] = previous[field.id]
    } else if (field.type === 'Form.Checkbox') {
      next[field.id] = false
    } else if (field.type === 'Form.FilePicker' || field.type === 'Form.TagPicker') {
      next[field.id] = []
    } else {
      next[field.id] = ''
    }
  }

  return next
}

function displayFileName(path: string): string {
  return path.split(/[\\/]/).at(-1) || path
}

function FormFieldFeedback({ field }: { field: FormField }): ReactNode {
  if (!field.error && !field.info) return null
  return (
    <span
      className={`mt-1.5 block text-[10.5px] leading-[1.45] ${
        field.error ? 'text-rose-300' : 'text-ink-4'
      }`}
    >
      {field.error || field.info}
    </span>
  )
}

export type FormRuntimeHandle = {
  getValues: () => Record<string, unknown>
}

type FormRuntimeProps = {
  root: ExtensionRuntimeNode
  title: string
  onBack: () => void
  onSubmitForm: (values: Record<string, unknown>) => void
  onChangeField: (actionId: string, value: unknown) => Promise<void> | void
  onOpenActions: () => void
  actionNotice?: { message: string; tone: 'success' | 'error' } | null
}

export const FormRuntime = forwardRef<FormRuntimeHandle, FormRuntimeProps>(function FormRuntime(
  { root, title, onBack, onSubmitForm, onChangeField, onOpenActions, actionNotice },
  ref
) {
  const items = useMemo(() => collectFormItems(root), [root])
  const fields = useMemo(
    () => items.flatMap((item) => (item.type === 'field' ? [item.field] : [])),
    [items]
  )
  const [values, setValues] = useState<Record<string, unknown>>(() => initialValues(fields))
  const filePickerOpenRef = useRef(false)

  useEffect(() => {
    setValues((previous) => initialValues(fields, previous))
  }, [fields])

  useImperativeHandle(ref, () => ({ getValues: () => values }), [values])

  const releaseFilePickerBlurSuppression = useCallback((): void => {
    if (!filePickerOpenRef.current) return
    filePickerOpenRef.current = false
    void window.tezbar.setSuppressBlurHide(false).catch(() => undefined)
  }, [])

  const prepareForFilePicker = useCallback((): void => {
    if (filePickerOpenRef.current) return
    filePickerOpenRef.current = true
    void window.tezbar.setSuppressBlurHide(true).catch(() => {
      filePickerOpenRef.current = false
    })
  }, [])

  useEffect(() => {
    let releaseTimer: number | undefined
    const releaseAfterDialogCloses = (): void => {
      if (!filePickerOpenRef.current) return
      window.clearTimeout(releaseTimer)
      releaseTimer = window.setTimeout(releaseFilePickerBlurSuppression, 120)
    }

    window.addEventListener('focus', releaseAfterDialogCloses)
    return () => {
      window.removeEventListener('focus', releaseAfterDialogCloses)
      window.clearTimeout(releaseTimer)
      releaseFilePickerBlurSuppression()
    }
  }, [releaseFilePickerBlurSuppression])

  const updateValue = (field: FormField, value: unknown): void => {
    setValues((previous) => ({ ...previous, [field.id]: value }))
    if (field.actionId) void onChangeField(field.actionId, value)
  }

  return (
    <form
      className="flex h-full min-h-0 flex-col"
      onSubmit={(event) => {
        event.preventDefault()
        onSubmitForm(values)
      }}
      onKeyDownCapture={(event) => {
        if (
          event.key !== 'Enter' ||
          event.repeat ||
          event.nativeEvent.isComposing ||
          event.target instanceof HTMLTextAreaElement
        ) {
          return
        }
        event.preventDefault()
        onSubmitForm(values)
      }}
    >
      <div className="glass-card mb-2 shrink-0 px-4 py-3">
        <ViewHeader title={title} />
      </div>

      <div className="glass-card min-h-0 flex-1 overflow-y-auto px-5 py-5 sm:px-8">
        <div className="mx-auto max-w-[900px] space-y-4">
          {items.map((item) => {
            if (item.type === 'separator') {
              return <div key={item.key} className="-mx-8 border-t border-white/[0.065]" />
            }

            if (item.type === 'description') {
              return (
                <div key={item.key} className={FIELD_ROW_CLASS}>
                  <div className={item.title ? FIELD_TITLE_CLASS : ''}>{item.title}</div>
                  <p className="py-1.5 text-[11px] leading-5 text-ink-4">{item.text}</p>
                </div>
              )
            }

            const field = item.field
            const rawValue = values[field.id]
            const current = typeof rawValue === 'string' ? rawValue : ''
            const controlId = `runtime-form-${field.key.replace(/[^a-z0-9_-]/gi, '-')}`

            if (field.type === 'Form.TextArea') {
              return (
                <div key={field.key} className={FIELD_ROW_CLASS}>
                  <label htmlFor={controlId} className={FIELD_TITLE_CLASS}>
                    {field.title}
                  </label>
                  <div>
                    <textarea
                      id={controlId}
                      value={current}
                      onChange={(event) => updateValue(field, event.target.value)}
                      placeholder={field.placeholder}
                      className="glass-field min-h-[88px] resize-y"
                    />
                    <FormFieldFeedback field={field} />
                  </div>
                </div>
              )
            }

            if (field.type === 'Form.Checkbox') {
              const checked = rawValue === true
              return (
                <div key={field.key} className={FIELD_ROW_CLASS}>
                  <div className={FIELD_TITLE_CLASS}>{field.title}</div>
                  <div>
                    <label
                      htmlFor={controlId}
                      className="flex min-h-9 cursor-pointer items-center gap-2.5 text-[12px] font-medium text-ink-2"
                    >
                      <input
                        id={controlId}
                        type="checkbox"
                        checked={checked}
                        onChange={(event) => updateValue(field, event.target.checked)}
                        className="h-4 w-4 rounded-[5px] accent-accent"
                      />
                      <span>{field.label || field.title}</span>
                    </label>
                    <FormFieldFeedback field={field} />
                  </div>
                </div>
              )
            }

            if (field.type === 'Form.Dropdown') {
              return (
                <div key={field.key} className={FIELD_ROW_CLASS}>
                  <label htmlFor={controlId} className={FIELD_TITLE_CLASS}>
                    {field.title}
                  </label>
                  <div>
                    <select
                      id={controlId}
                      value={current}
                      onChange={(event) => updateValue(field, event.target.value)}
                      className="glass-field h-10 py-0"
                    >
                      {!field.options?.some((option) => option.value === '') ? (
                        <option value="">Select</option>
                      ) : null}
                      {(field.options ?? []).map((option) => (
                        <option key={`${field.id}:${option.value}`} value={option.value}>
                          {option.title}
                        </option>
                      ))}
                    </select>
                    <FormFieldFeedback field={field} />
                  </div>
                </div>
              )
            }

            if (field.type === 'Form.TagPicker') {
              const selected = Array.isArray(rawValue)
                ? rawValue.filter((value): value is string => typeof value === 'string')
                : []
              return (
                <div key={field.key} className={FIELD_ROW_CLASS}>
                  <label htmlFor={controlId} className={FIELD_TITLE_CLASS}>
                    {field.title}
                  </label>
                  <div>
                    <select
                      id={controlId}
                      multiple
                      value={selected}
                      onChange={(event) =>
                        updateValue(
                          field,
                          Array.from(event.target.selectedOptions, (option) => option.value)
                        )
                      }
                      className="glass-field min-h-[96px]"
                    >
                      {(field.options ?? []).map((option) => (
                        <option key={`${field.id}:${option.value}`} value={option.value}>
                          {option.title}
                        </option>
                      ))}
                    </select>
                    <FormFieldFeedback field={field} />
                  </div>
                </div>
              )
            }

            if (field.type === 'Form.FilePicker') {
              const selected = Array.isArray(rawValue)
                ? rawValue.filter((value): value is string => typeof value === 'string')
                : []
              return (
                <div key={field.key} className={FIELD_ROW_CLASS}>
                  <label htmlFor={controlId} className={FIELD_TITLE_CLASS}>
                    {field.title}
                  </label>
                  <div>
                    <input
                      id={controlId}
                      type="file"
                      multiple={field.allowMultipleSelection}
                      onPointerDown={prepareForFilePicker}
                      onClick={prepareForFilePicker}
                      onChange={(event) => {
                        const next = Array.from(event.target.files ?? [], (file) => {
                          const desktopFile = file as File & { path?: string }
                          return desktopFile.path || file.name
                        })
                        updateValue(field, next)
                        releaseFilePickerBlurSuppression()
                      }}
                      className="glass-field cursor-pointer py-1.5 file:mr-3 file:cursor-pointer file:rounded-tezbar-chip file:border-0 file:bg-white/[0.09] file:px-3 file:py-1 file:text-[11px] file:font-medium file:text-ink-1 hover:file:bg-white/[0.13]"
                    />
                    {selected.length > 0 ? (
                      <div className="mt-2 flex flex-wrap gap-1.5">
                        {selected.map((path) => (
                          <span
                            key={path}
                            title={path}
                            className="max-w-full truncate rounded-tezbar-chip border border-emerald-300/[0.14] bg-emerald-400/[0.06] px-2 py-1 text-[10.5px] text-emerald-100/85"
                          >
                            {displayFileName(path)}
                          </span>
                        ))}
                      </div>
                    ) : null}
                    <FormFieldFeedback field={field} />
                  </div>
                </div>
              )
            }

            const inputType =
              field.type === 'Form.PasswordField'
                ? 'password'
                : field.type === 'Form.DatePicker'
                  ? 'date'
                  : 'text'

            return (
              <div key={field.key} className={FIELD_ROW_CLASS}>
                <label htmlFor={controlId} className={FIELD_TITLE_CLASS}>
                  {field.title}
                </label>
                <div>
                  <input
                    id={controlId}
                    type={inputType}
                    value={current}
                    onChange={(event) => updateValue(field, event.target.value)}
                    placeholder={field.placeholder}
                    className="glass-field h-10 py-0"
                  />
                  <FormFieldFeedback field={field} />
                </div>
              </div>
            )
          })}

          {fields.length === 0 ? (
            <div className="rounded-tezbar-row bg-white/[0.03] px-3 py-4 text-[12px] text-ink-3">
              This form did not expose any editable fields.
            </div>
          ) : null}
        </div>
      </div>

      {actionNotice ? (
        <div
          role="status"
          aria-live="polite"
          className={`glass-card mt-2 shrink-0 px-4 py-2 text-[11px] font-medium ${
            actionNotice.tone === 'error' ? 'text-rose-300' : 'text-emerald-200'
          }`}
        >
          {actionNotice.message}
        </div>
      ) : null}

      <div className="glass-card mt-2 flex shrink-0 items-center justify-between gap-3 px-4 py-2">
        <button
          type="button"
          onClick={onBack}
          className="rounded-tezbar-chip px-1.5 py-1 transition hover:bg-white/[0.06]"
          aria-label="Back"
        >
          <HintBar>
            <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
          </HintBar>
        </button>
        <div className="flex items-center gap-1">
          <button
            type="submit"
            className="rounded-tezbar-chip px-1.5 py-1 transition hover:bg-white/[0.06]"
            aria-label="Submit form"
          >
            <HintBar>
              <Hint label="Submit" keys={<Kbd>↵</Kbd>} />
            </HintBar>
          </button>
          <button
            type="button"
            onClick={onOpenActions}
            className="rounded-tezbar-chip px-1.5 py-1 transition hover:bg-white/[0.06]"
            aria-label="Open actions"
          >
            <HintBar>
              <Hint
                label="Actions"
                keys={
                  <>
                    <Kbd>⌘</Kbd>
                    <Kbd>K</Kbd>
                  </>
                }
              />
            </HintBar>
          </button>
        </div>
      </div>
    </form>
  )
})
