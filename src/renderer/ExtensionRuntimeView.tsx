import { useCallback, useEffect, useRef, useState } from 'react'
import type {
  ExtensionRunCommandResult,
  ExtensionRuntimeAction,
  ExtensionRuntimeNode,
} from '../shared/extensionRuntime'
import { Message } from './ui/primitives'
import { ExtensionRuntimeSurface } from './src/raycast-api'

type PreferenceOption = { title?: string; value?: string }
type PreferenceField = {
  name?: string
  title?: string
  description?: string
  type?: string
  required?: boolean
  default?: unknown
  data?: PreferenceOption[]
}

type RuntimeViewState = {
  sessionId: string
  extensionId: string
  commandName: string
  title: string
  root: ExtensionRuntimeNode
  actions: ExtensionRuntimeAction[]
  message?: string
}

type ActionNotice = {
  message: string
  tone: 'success' | 'error'
}

function fallbackActionNotice(action: ExtensionRuntimeAction | undefined): string | null {
  if (!action) return null
  if (action.kind === 'copy' || /copy (?:gif|image|link|markdown)/i.test(action.title)) {
    return 'Copied to clipboard'
  }
  if (/add to favou?rites?/i.test(action.title)) return 'Added to Favorites'
  if (/remove from favou?rites?/i.test(action.title)) return 'Removed from Favorites'
  return null
}

function fromRunResult(
  result: Extract<ExtensionRunCommandResult, { ok: true; mode: 'view' }>
): RuntimeViewState {
  return {
    sessionId: result.sessionId,
    extensionId: result.extensionId,
    commandName: result.commandName,
    title: result.title,
    root: result.root,
    actions: result.actions,
    message: result.message,
  }
}

function fileUrl(path: string): string {
  return `file://${encodeURI(path)}`
}

function PreferenceSetupView({
  root,
  onBack,
  onSaved,
}: {
  root: ExtensionRuntimeNode
  onBack: () => void
  onSaved: (next: Extract<ExtensionRunCommandResult, { ok: true; mode: 'view' }>) => void
}): JSX.Element {
  const props = root.props ?? {}
  const preferences = Array.isArray(props.preferences)
    ? (props.preferences as PreferenceField[])
    : []
  const values =
    props.values && typeof props.values === 'object'
      ? (props.values as Record<string, unknown>)
      : {}
  const extensionId = typeof props.extensionId === 'string' ? props.extensionId : ''
  const commandName = typeof props.commandName === 'string' ? props.commandName : ''
  const title = typeof props.title === 'string' ? props.title : 'Extension'
  const iconPath = typeof props.iconPath === 'string' ? props.iconPath : ''
  const includeApiKey = props.includeApiKey === true
  const hasFields = includeApiKey || preferences.length > 0

  const initialValues = preferences.reduce<Record<string, string>>(
    (acc, pref) => {
      if (!pref.name) return acc
      const value = values[pref.name] ?? pref.default ?? ''
      acc[pref.name] = typeof value === 'boolean' ? String(value) : String(value ?? '')
      return acc
    },
    includeApiKey ? { apiKey: String(values.apiKey ?? '') } : {}
  )
  const [formValues, setFormValues] = useState<Record<string, string>>(initialValues)
  const [saving, setSaving] = useState(false)
  const [localError, setLocalError] = useState<string | null>(null)

  const setValue = (key: string, value: string): void => {
    setFormValues((prev) => ({ ...prev, [key]: value }))
  }

  const primaryPreferences = preferences.filter(
    (pref) => pref.name === 'lang1' || pref.name === 'lang2'
  )
  const secondaryPreferences = preferences.filter(
    (pref) => pref.name !== 'lang1' && pref.name !== 'lang2'
  )

  const fieldRowClass = 'grid items-start gap-2 sm:grid-cols-[112px_minmax(0,1fr)] sm:gap-4'
  const fieldLabelClass =
    'pt-2.5 text-left text-[11px] font-semibold leading-5 text-ink-3 sm:text-right'
  const fieldControlClass =
    'h-8 w-full rounded-[9px] border border-white/75 bg-white/[0.025] px-2.5 text-[13px] text-ink-1 outline-none transition placeholder:text-ink-4 focus:border-accent-strong focus:bg-white/[0.045] focus:shadow-[0_0_0_2px_rgba(139,141,247,0.18)]'
  const fieldHintClass = 'mt-1.5 block text-[10.5px] leading-4 text-ink-4'

  const renderPreference = (pref: PreferenceField): JSX.Element | null => {
    const name = pref.name
    if (!name) return null
    const label = pref.title || name
    const value = formValues[name] ?? ''

    if (pref.type === 'checkbox') {
      return (
        <div key={name} className={fieldRowClass}>
          <div className={fieldLabelClass}>{label}</div>
          <label className="flex min-h-8 items-center gap-2 text-[12px] font-semibold text-ink-2">
            <input
              type="checkbox"
              checked={value === 'true'}
              onChange={(event) => setValue(name, event.target.checked ? 'true' : 'false')}
              className="h-[13px] w-[13px] rounded-[3px] accent-accent"
            />
            <span className="min-w-0">
              {pref.description || `Enable ${label.toLowerCase()}`}
              {pref.required ? (
                <span className="ml-2 text-[9px] uppercase tracking-[0.14em] text-accent-strong">
                  Required
                </span>
              ) : null}
            </span>
          </label>
        </div>
      )
    }

    if (pref.type === 'dropdown') {
      return (
        <label key={name} className={fieldRowClass}>
          <span className={fieldLabelClass}>
            {label}
            {pref.required ? <span className="ml-1 text-accent-strong">*</span> : null}
          </span>
          <span>
            <select
              value={value}
              onChange={(event) => setValue(name, event.target.value)}
              className={fieldControlClass}
            >
              {(pref.data ?? []).map((option) => {
                const optionValue = String(option.value ?? option.title ?? '')
                return (
                  <option key={`${name}:${optionValue}`} value={optionValue}>
                    {String(option.title ?? optionValue)}
                  </option>
                )
              })}
            </select>
            {pref.description ? <span className={fieldHintClass}>{pref.description}</span> : null}
          </span>
        </label>
      )
    }

    return (
      <label key={name} className={fieldRowClass}>
        <span className={fieldLabelClass}>
          {label}
          {pref.required ? <span className="ml-1 text-accent-strong">*</span> : null}
        </span>
        <span>
          <input
            type={pref.type === 'password' ? 'password' : 'text'}
            value={value}
            onChange={(event) => setValue(name, event.target.value)}
            placeholder={pref.description || label}
            className={fieldControlClass}
          />
          {pref.description ? <span className={fieldHintClass}>{pref.description}</span> : null}
        </span>
      </label>
    )
  }

  const save = async (): Promise<void> => {
    const missing = preferences.find((pref) => {
      if (!pref.required || !pref.name) return false
      return !String(formValues[pref.name] ?? '').trim()
    })
    if (missing) {
      setLocalError(`${missing.title || missing.name} is required.`)
      return
    }

    setSaving(true)
    setLocalError(null)
    try {
      await window.tezbar.saveExtensionPreferences({ extensionId, values: formValues })
      const result = await window.tezbar.extensionRunCommand({ extensionId, commandName })
      if (!result.ok) {
        setLocalError(result.message)
        return
      }
      if (result.mode === 'view') onSaved(result)
    } catch (error) {
      setLocalError(error instanceof Error ? error.message : String(error))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="relative flex h-full min-h-0 flex-col overflow-hidden rounded-[18px] border border-white/[0.07] bg-[#1f1f2a] text-white shadow-[0_24px_70px_rgba(0,0,0,0.42)]">
      <button
        type="button"
        onClick={onBack}
        className="absolute left-4 top-4 z-10 grid h-8 w-8 place-items-center rounded-[9px] text-ink-4 transition hover:bg-white/[0.06] hover:text-ink-1"
        aria-label="Back"
      >
        <svg
          width="15"
          height="15"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2.4"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden
        >
          <path d="m15 18-6-6 6-6" />
        </svg>
      </button>

      <div className="flex min-h-0 flex-1 justify-center overflow-y-auto px-7 pb-8 pt-8">
        <div className="w-full max-w-[600px]">
          <div className="mb-5 flex flex-col items-center text-center">
            {iconPath ? (
              <img
                src={fileUrl(iconPath)}
                alt=""
                className="mb-3 h-8 w-8 rounded-[8px] border border-white/10 bg-black/20 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
              />
            ) : null}
            <h1 className="text-[24px] font-bold leading-tight text-ink-1">Welcome to {title}</h1>
            <p className="mt-3 max-w-[480px] text-[13px] font-semibold leading-6 text-ink-3">
              Before you can start using this command, add a few things to the settings listed
              below.
            </p>
            <span className="mt-3 inline-flex h-7 items-center rounded-[9px] border border-white/[0.06] bg-white/[0.08] px-3 text-[11.5px] font-bold text-ink-2">
              Settings are stored locally
            </span>
          </div>

          <div className="space-y-3">
            {includeApiKey ? (
              <label className={fieldRowClass}>
                <span className={fieldLabelClass}>API Key</span>
                <input
                  type="password"
                  value={formValues.apiKey ?? ''}
                  onChange={(event) => setValue('apiKey', event.target.value)}
                  placeholder="API Key"
                  className={fieldControlClass}
                />
              </label>
            ) : null}

            {primaryPreferences.map(renderPreference)}
            {secondaryPreferences.map(renderPreference)}
            {localError ? (
              <div className={fieldRowClass}>
                <div />
                <Message tone="error">{localError}</Message>
              </div>
            ) : null}
          </div>

          {hasFields ? (
            <div className="mt-6 grid items-center gap-2 sm:grid-cols-[112px_minmax(0,1fr)] sm:gap-4">
              <div />
              <div className="flex justify-end gap-2">
                <button
                  type="button"
                  className="h-8 rounded-[9px] px-3 text-[12px] font-semibold text-ink-3 transition hover:bg-white/[0.06] hover:text-ink-1"
                  onClick={onBack}
                >
                  Cancel
                </button>
                <button
                  type="button"
                  onClick={() => void save()}
                  disabled={saving}
                  className="h-8 rounded-[9px] bg-accent px-4 text-[12px] font-bold text-white transition hover:bg-accent-strong active:scale-[0.98] disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {saving ? 'Saving...' : 'Save and continue'}
                </button>
              </div>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  )
}

export default function ExtensionRuntimeView({
  initial,
  onBack,
}: {
  initial: Extract<ExtensionRunCommandResult, { ok: true; mode: 'view' }>
  onBack: () => void
}): JSX.Element {
  const [state, setState] = useState<RuntimeViewState>(() => fromRunResult(initial))
  const [error, setError] = useState<string | null>(null)
  const [actionNotice, setActionNotice] = useState<ActionNotice | null>(null)
  const searchRequestSeq = useRef(0)
  const disposeTimerRef = useRef<number | null>(null)
  const actionNoticeTimerRef = useRef<number | null>(null)

  const showActionNotice = useCallback((message: string): void => {
    if (actionNoticeTimerRef.current !== null) {
      window.clearTimeout(actionNoticeTimerRef.current)
    }
    const tone = /could not|failed|failure|error/i.test(message) ? 'error' : 'success'
    setActionNotice({ message, tone })
    actionNoticeTimerRef.current = window.setTimeout(() => {
      actionNoticeTimerRef.current = null
      setActionNotice(null)
    }, 2400)
  }, [])

  useEffect(() => {
    setState(fromRunResult(initial))
    setError(null)
  }, [initial])

  useEffect(
    () => () => {
      if (actionNoticeTimerRef.current !== null) {
        window.clearTimeout(actionNoticeTimerRef.current)
      }
    },
    []
  )

  useEffect(() => {
    let cancelled = false
    let timer: number | null = null
    if (disposeTimerRef.current !== null) {
      window.clearTimeout(disposeTimerRef.current)
      disposeTimerRef.current = null
    }

    const poll = async (): Promise<void> => {
      const startedAt = performance.now()
      console.log(`[RuntimeView] Refresh start session=${state.sessionId}`)
      try {
        const result = await window.tezbar.extensionRefreshSession({ sessionId: state.sessionId })
        console.log(
          `[RuntimeView] Refresh complete session=${state.sessionId} after ${Math.round(performance.now() - startedAt)}ms; mode=${
            result.ok ? result.mode : 'error'
          }`
        )
        if (cancelled || (result.ok && result.mode === 'unchanged')) return
        if (!result.ok) {
          setError(result.message)
          return
        }
        if (result.mode === 'no-view') {
          setState((prev) => ({ ...prev, message: result.message }))
          return
        }
        setState({
          sessionId: result.sessionId,
          extensionId: result.extensionId,
          commandName: result.commandName,
          title: result.title,
          root: result.root,
          actions: result.actions,
          message: result.message,
        })
      } catch (error) {
        console.error(
          `[RuntimeView] Refresh failed session=${state.sessionId} after ${Math.round(performance.now() - startedAt)}ms`,
          error
        )
        if (!cancelled) setError(error instanceof Error ? error.message : String(error))
      } finally {
        if (!cancelled) timer = window.setTimeout(() => void poll(), 500)
      }
    }

    timer = window.setTimeout(() => void poll(), 100)

    return () => {
      cancelled = true
      if (timer !== null) window.clearTimeout(timer)
      const sessionId = state.sessionId
      disposeTimerRef.current = window.setTimeout(() => {
        disposeTimerRef.current = null
        void window.tezbar.extensionDisposeSession({ sessionId })
      }, 0)
    }
  }, [state.sessionId])

  const handleSearchTextChanged = useCallback(
    async (searchText: string) => {
      const requestSeq = searchRequestSeq.current + 1
      searchRequestSeq.current = requestSeq
      console.log(`[RuntimeView] Search text changed, sending to sandbox: "${searchText}"`)
      const result = await window.tezbar.extensionSearchTextChanged({
        sessionId: state.sessionId,
        searchText,
      })
      if (requestSeq !== searchRequestSeq.current) return

      if (!result.ok) {
        console.error('[RuntimeView] Search failed:', result.message)
        setError(result.message)
        return
      }

      if (result.mode === 'no-view') {
        console.log('[RuntimeView] Search returned no-view result')
        setState((prev) => ({ ...prev, message: result.message }))
        return
      }

      console.log(
        `[RuntimeView] Search returned view with root type="${result.root.type}", ${result.root.children?.length ?? 0} children`
      )
      setState({
        sessionId: result.sessionId,
        extensionId: result.extensionId,
        commandName: result.commandName,
        title: result.title,
        root: result.root,
        actions: result.actions,
        message: result.message,
      })
    },
    [state.sessionId]
  )

  const handleLoadMore = useCallback(async () => {
    const result = await window.tezbar.extensionLoadMore({ sessionId: state.sessionId })
    if (result.ok && result.mode === 'unchanged') return
    if (!result.ok) {
      setError(result.message)
      return
    }
    if (result.mode === 'no-view') {
      setState((prev) => ({ ...prev, message: result.message }))
      return
    }
    setState({
      sessionId: result.sessionId,
      extensionId: result.extensionId,
      commandName: result.commandName,
      title: result.title,
      root: result.root,
      actions: result.actions,
      message: result.message,
    })
  }, [state.sessionId])

  return (
    <div
      role="application"
      aria-label="Extension Runtime"
      className="flex h-full min-h-0 w-full flex-col gap-2 outline-none animate-tezbar-scale-in"
    >
      <div className="min-h-0 flex-1">
        {state.root.type === 'Tezbar.PreferenceSetup' ? (
          <PreferenceSetupView
            root={state.root}
            onBack={onBack}
            onSaved={(next) => {
              setState(fromRunResult(next))
              setError(null)
            }}
          />
        ) : (
          <ExtensionRuntimeSurface
            sessionId={state.sessionId}
            title={state.title}
            extensionId={state.extensionId}
            commandName={state.commandName}
            root={state.root}
            actions={state.actions}
            actionNotice={actionNotice}
            onBack={onBack}
            onSearchTextChanged={handleSearchTextChanged}
            onLoadMore={handleLoadMore}
            onInvokeAction={async (actionId, formValues) => {
              setError(null)
              const invokedAction = state.actions.find((action) => action.id === actionId)
              const result = await window.tezbar.extensionInvokeAction({
                sessionId: state.sessionId,
                actionId,
                formValues,
              })

              if (!result.ok) {
                setError(result.message)
                return
              }

              const notice = result.message?.trim() || fallbackActionNotice(invokedAction)
              if (notice) showActionNotice(notice)

              if (result.mode === 'no-view') {
                setState((prev) => ({ ...prev, message: undefined }))
                return
              }

              setState({
                sessionId: result.sessionId,
                extensionId: result.extensionId,
                commandName: result.commandName,
                title: result.title,
                root: result.root,
                actions: result.actions,
                message: undefined,
              })
            }}
          />
        )}
      </div>

      {error ? (
        <div className="glass-card shrink-0 px-3 py-2">
          <Message tone="error">{error}</Message>
        </div>
      ) : null}

      {state.message ? (
        <div className="glass-card shrink-0 px-3 py-2">
          <Message>{state.message}</Message>
        </div>
      ) : null}
    </div>
  )
}
