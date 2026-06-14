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

function fromRunResult(result: Extract<ExtensionRunCommandResult, { ok: true; mode: 'view' }>): RuntimeViewState {
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
  const preferences = Array.isArray(props.preferences) ? props.preferences as PreferenceField[] : []
  const values = props.values && typeof props.values === 'object' ? props.values as Record<string, unknown> : {}
  const extensionId = typeof props.extensionId === 'string' ? props.extensionId : ''
  const commandName = typeof props.commandName === 'string' ? props.commandName : ''
  const title = typeof props.title === 'string' ? props.title : 'Extension'
  const iconPath = typeof props.iconPath === 'string' ? props.iconPath : ''
  const includeApiKey = props.includeApiKey === true

  const initialValues = preferences.reduce<Record<string, string>>((acc, pref) => {
    if (!pref.name) return acc
    const value = values[pref.name] ?? pref.default ?? ''
    acc[pref.name] = typeof value === 'boolean' ? String(value) : String(value ?? '')
    return acc
  }, includeApiKey ? { apiKey: String(values.apiKey ?? '') } : {})
  const [formValues, setFormValues] = useState<Record<string, string>>(initialValues)
  const [saving, setSaving] = useState(false)
  const [localError, setLocalError] = useState<string | null>(null)

  const setValue = (key: string, value: string): void => {
    setFormValues((prev) => ({ ...prev, [key]: value }))
  }

  const primaryPreferences = preferences.filter((pref) => pref.name === 'lang1' || pref.name === 'lang2')
  const secondaryPreferences = preferences.filter((pref) => pref.name !== 'lang1' && pref.name !== 'lang2')

  const renderPreference = (pref: PreferenceField): JSX.Element | null => {
    const name = pref.name
    if (!name) return null
    const label = pref.title || name
    const value = formValues[name] ?? ''

    if (pref.type === 'checkbox') {
      return (
        <label key={name} className="group flex items-center justify-between gap-4 rounded-[14px] border border-white/10 bg-white/[0.045] px-4 py-3 transition hover:border-white/18 hover:bg-white/[0.07]">
          <span className="min-w-0">
            <span className="block text-[13px] font-semibold text-ink-1">{label}</span>
            {pref.description ? <span className="mt-0.5 block text-[11px] text-ink-4">{pref.description}</span> : null}
          </span>
          <input
            type="checkbox"
            checked={value === 'true'}
            onChange={(event) => setValue(name, event.target.checked ? 'true' : 'false')}
            className="h-4 w-4 accent-accent-1"
          />
        </label>
      )
    }

    if (pref.type === 'dropdown') {
      return (
        <label key={name} className="block">
          <span className="mb-2 flex items-center justify-between gap-3 text-[12px] font-semibold text-ink-3">
            <span>{label}</span>
            {pref.required ? <span className="text-[10px] uppercase tracking-[0.16em] text-accent-1">Required</span> : null}
          </span>
          <select
            value={value}
            onChange={(event) => setValue(name, event.target.value)}
            className="h-12 w-full rounded-[14px] border border-white/12 bg-black/25 px-4 text-[14px] font-semibold text-ink-1 outline-none transition focus:border-accent-1/70 focus:bg-black/35"
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
          {pref.description ? <span className="mt-1.5 block text-[11px] text-ink-4">{pref.description}</span> : null}
        </label>
      )
    }

    return (
      <label key={name} className="block">
        <span className="mb-2 block text-[12px] font-semibold text-ink-3">{label}</span>
        <input
          type={pref.type === 'password' ? 'password' : 'text'}
          value={value}
          onChange={(event) => setValue(name, event.target.value)}
          placeholder={pref.description || label}
          className="h-12 w-full rounded-[14px] border border-white/12 bg-black/25 px-4 text-[14px] text-ink-1 outline-none transition placeholder:text-ink-5 focus:border-accent-1/70 focus:bg-black/35"
        />
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
    <div className="relative flex h-full min-h-0 overflow-hidden rounded-[24px] border border-white/10 bg-[#101114] text-ink-1 shadow-[0_24px_80px_rgba(0,0,0,0.35)]">
      <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_50%_0%,rgba(66,133,244,0.18),transparent_38%),linear-gradient(135deg,rgba(52,168,83,0.08),transparent_34%,rgba(251,188,5,0.07)_62%,rgba(234,67,53,0.08))]" />
      <button
        type="button"
        onClick={onBack}
        className="absolute left-5 top-5 z-10 grid h-11 w-11 place-items-center rounded-[14px] border border-white/10 bg-white/[0.08] text-ink-1 transition hover:bg-white/[0.13]"
        aria-label="Back"
      >
        <span className="text-2xl leading-none">‹</span>
      </button>

      <div className="relative z-0 grid h-full w-full grid-cols-[minmax(320px,0.95fr)_minmax(420px,1.35fr)]">
        <section className="flex min-h-0 flex-col justify-between border-r border-white/10 px-12 py-12">
          <div className="mt-16">
            <div className="mb-7 flex items-center gap-3">
              {iconPath ? <img src={fileUrl(iconPath)} alt="" className="h-12 w-12 rounded-[13px]" /> : null}
              <div>
                <p className="text-[11px] font-semibold uppercase tracking-[0.22em] text-accent-1">Setup</p>
                <h1 className="mt-1 text-[34px] font-bold leading-tight text-ink-1">{title}</h1>
              </div>
            </div>
            <p className="max-w-[430px] text-[15px] leading-7 text-ink-3">
              Choose your translation defaults once. TezBar will save them locally and launch the command immediately.
            </p>
          </div>

          <div className="rounded-[18px] border border-white/10 bg-black/20 p-4">
            <p className="text-[11px] font-semibold uppercase tracking-[0.18em] text-ink-4">Connection</p>
            <p className="mt-2 text-[13px] leading-6 text-ink-3">
              This extension uses Google Translate directly. A key can be stored for future API-backed builds, but it is optional here.
            </p>
          </div>
        </section>

        <section className="min-h-0 overflow-y-auto px-12 py-12">
          <div className="mx-auto flex min-h-full max-w-[720px] flex-col justify-center gap-7">
            {includeApiKey ? (
              <label className="block rounded-[20px] border border-accent-1/25 bg-accent-1/[0.08] p-5 shadow-[0_18px_60px_rgba(66,133,244,0.08)]">
                <span className="mb-2 block text-[12px] font-semibold uppercase tracking-[0.16em] text-accent-1">API Key</span>
                <input
                  type="password"
                  value={formValues.apiKey ?? ''}
                  onChange={(event) => setValue('apiKey', event.target.value)}
                  placeholder="Optional Google API key"
                  className="h-12 w-full rounded-[14px] border border-white/12 bg-black/30 px-4 text-[14px] text-ink-1 outline-none transition placeholder:text-ink-5 focus:border-accent-1/70"
                />
              </label>
            ) : null}

            <div className="grid gap-5 md:grid-cols-2">
              {primaryPreferences.map(renderPreference)}
            </div>

            {secondaryPreferences.length > 0 ? (
              <div className="grid gap-4">
                {secondaryPreferences.map(renderPreference)}
              </div>
            ) : null}

            {localError ? <Message tone="error">{localError}</Message> : null}

            <div className="flex items-center justify-end gap-3 border-t border-white/10 pt-5">
              <button type="button" className="btn btn-ghost" onClick={onBack}>
                Cancel
              </button>
              <button
                type="button"
                onClick={() => void save()}
                disabled={saving}
                className="rounded-[14px] bg-ink-1 px-5 py-3 text-[13px] font-bold text-black transition hover:bg-white disabled:cursor-not-allowed disabled:opacity-60"
              >
                {saving ? 'Saving...' : 'Save and Continue'}
              </button>
            </div>
          </div>
        </section>
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
  const searchRequestSeq = useRef(0)
  const disposeTimerRef = useRef<number | null>(null)

  useEffect(() => {
    setState(fromRunResult(initial))
    setError(null)
  }, [initial])

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
          `[RuntimeView] Refresh complete session=${state.sessionId} after ${Math.round(performance.now() - startedAt)}ms; mode=${result.ok ? result.mode : 'error'
          }`,
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
          error,
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

  const handleSearchTextChanged = useCallback(async (searchText: string) => {
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

    console.log(`[RuntimeView] Search returned view with root type="${result.root.type}", ${result.root.children?.length ?? 0} children`)
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
        {state.root.type === 'TezBar.PreferenceSetup' ? (
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
            onBack={onBack}
            onSearchTextChanged={handleSearchTextChanged}
            onLoadMore={handleLoadMore}
            onInvokeAction={async (actionId, formValues) => {
              setError(null)
              const result = await window.tezbar.extensionInvokeAction({
                sessionId: state.sessionId,
                actionId,
                formValues,
              })

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
