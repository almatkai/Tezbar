import { useEffect, useReducer } from 'react'

import { Button, Message, TextField } from './ui/primitives'
import {
  mergePreferenceSetups,
  partitionPreferenceValues,
  preferenceValueKey,
  type PreferenceField,
} from './extensionPreferences'

type EditorState = {
  status: 'loading' | 'ready' | 'saving' | 'error'
  fields: PreferenceField[]
  values: Record<string, unknown>
  error: string | null
}

type EditorAction =
  | { type: 'loading' }
  | { type: 'loaded'; fields: PreferenceField[]; values: Record<string, unknown> }
  | { type: 'changed'; key: string; value: unknown }
  | { type: 'saving' }
  | { type: 'saved' }
  | { type: 'failed'; error: string }

const INITIAL_STATE: EditorState = {
  status: 'loading',
  fields: [],
  values: {},
  error: null,
}

function editorReducer(state: EditorState, action: EditorAction): EditorState {
  switch (action.type) {
    case 'loading':
      return INITIAL_STATE
    case 'loaded':
      return { status: 'ready', fields: action.fields, values: action.values, error: null }
    case 'changed':
      return { ...state, values: { ...state.values, [action.key]: action.value } }
    case 'saving':
      return { ...state, status: 'saving', error: null }
    case 'saved':
      return { ...state, status: 'ready', error: null }
    case 'failed':
      return { ...state, status: 'error', error: action.error }
  }
}

export function ExtensionPreferencesEditor({
  extensionId,
  extensionName,
  commands,
  onMessage,
}: {
  extensionId: string
  extensionName: string
  commands: Array<{ name?: string; title?: string }>
  onMessage: (message: { tone: 'success' | 'error'; text: string }) => void
}): JSX.Element | null {
  const [state, dispatch] = useReducer(editorReducer, INITIAL_STATE)

  useEffect(() => {
    let active = true
    dispatch({ type: 'loading' })
    void Promise.all([
      window.tezbar.getExtensionPreferenceSetup({ extensionId }),
      Promise.all(
        commands
          .filter((command): command is { name: string; title?: string } => Boolean(command.name))
          .map((command) =>
          window.tezbar.getExtensionPreferenceSetup({
            extensionId,
            commandName: command.name,
          }),
          ),
      ),
    ])
      .then(([extensionSetup, commandSetups]) => {
        if (!active || !extensionSetup) return
        dispatch({ type: 'loaded', ...mergePreferenceSetups(extensionSetup, commandSetups) })
      })
      .catch((error: unknown) => {
        if (!active) return
        dispatch({
          type: 'failed',
          error: error instanceof Error ? error.message : 'Could not load extension preferences',
        })
      })
    return () => {
      active = false
    }
  }, [commands, extensionId])

  const save = async (): Promise<void> => {
    dispatch({ type: 'saving' })
    try {
      await Promise.all(
        partitionPreferenceValues(state.fields, state.values).map(({ commandName, values }) =>
          window.tezbar.saveExtensionPreferences({ extensionId, commandName, values }),
        ),
      )
      dispatch({ type: 'saved' })
      onMessage({ tone: 'success', text: `Saved preferences for ${extensionName}` })
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Could not save preferences'
      dispatch({ type: 'failed', error: message })
      onMessage({ tone: 'error', text: message })
    }
  }

  if (state.status === 'loading' || (state.fields.length === 0 && !state.error)) return null

  return (
    <div className="mt-5 rounded-[8px] border border-white/10 bg-white/[0.025] p-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="text-[13px] font-semibold text-ink-2">Extension Preferences</p>
          <p className="mt-1 text-[11px] text-ink-4">API keys and settings are stored locally.</p>
        </div>
        {state.fields.length > 0 ? (
          <Button variant="primary" disabled={state.status === 'saving'} onClick={() => void save()}>
            {state.status === 'saving' ? 'Saving...' : 'Save'}
          </Button>
        ) : null}
      </div>
      {state.error ? <div className="mt-3"><Message tone="error">{state.error}</Message></div> : null}
      <div className="mt-4 grid gap-3">
        {state.fields.map((field) => {
          if (!field.name) return null
          const valueKey = preferenceValueKey(field)
          const value = state.values[valueKey]
          const label = field.title || field.name
          if (field.type === 'checkbox') {
            return (
              <label key={valueKey} className="flex items-center gap-2 text-[12px] text-ink-2">
                <input
                  type="checkbox"
                  checked={value === true || value === 'true'}
                  onChange={(event) => dispatch({
                    type: 'changed',
                    key: valueKey,
                    value: event.target.checked,
                  })}
                />
                <span>
                  {label}
                  {field.commandTitle ? (
                    <span className="ml-2 text-[10px] font-medium text-ink-4">{field.commandTitle}</span>
                  ) : null}
                </span>
              </label>
            )
          }
          return (
            <label key={valueKey} className="block">
              <span className="mb-1 flex items-center gap-2 text-[11px] font-semibold text-ink-3">
                {label}
                {field.commandTitle ? (
                  <span className="rounded-tezbar-chip border border-white/10 bg-white/[0.04] px-1.5 py-0.5 text-[9px] font-medium text-ink-4">
                    {field.commandTitle}
                  </span>
                ) : null}
                {field.required ? <span className="text-[9px] uppercase text-accent-1">Required</span> : null}
              </span>
              {field.type === 'dropdown' ? (
                <select
                  value={String(value ?? '')}
                  onChange={(event) => dispatch({
                    type: 'changed',
                    key: valueKey,
                    value: event.target.value,
                  })}
                  className="glass-field w-full"
                >
                  {(field.data ?? []).map((option) => (
                    <option key={String(option.value ?? option.title)} value={String(option.value ?? option.title ?? '')}>
                      {option.title ?? option.value}
                    </option>
                  ))}
                </select>
              ) : (
                <TextField
                  type={field.type === 'password' ? 'password' : 'text'}
                  value={String(value ?? '')}
                  onChange={(event) => dispatch({
                    type: 'changed',
                    key: valueKey,
                    value: event.target.value,
                  })}
                  placeholder={field.description || label}
                />
              )}
              {field.description ? (
                <span className="mt-1 block text-[10px] text-ink-4">{field.description}</span>
              ) : null}
            </label>
          )
        })}
      </div>
    </div>
  )
}
