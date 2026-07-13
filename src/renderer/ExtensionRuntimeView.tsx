import { useCallback, useEffect, useRef, useState } from 'react'
import type {
  ExtensionRunCommandResult,
  ExtensionRuntimeAction,
  ExtensionRuntimeEffect,
  ExtensionRuntimeNode,
} from '../shared/extensionRuntime'
import { Button, Message } from './ui/primitives'
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

function effectNotice(effects: ExtensionRuntimeEffect[] | undefined): ActionNotice | null {
  const latest = [...(effects ?? [])]
    .reverse()
    .find((effect) => effect.kind === 'toast' || effect.kind === 'hud')
  if (!latest) return null

  const title = latest.title?.trim()
  const message = latest.message?.trim()
  const status =
    latest.style === 'failure'
      ? 'Failed'
      : latest.style === 'success'
        ? 'Saved'
        : latest.style === 'animated'
          ? 'Working'
          : 'Done'
  const parts = [status]
  if (title && title.toLowerCase() !== status.toLowerCase()) parts.push(title)
  if (message) parts.push(message)

  return {
    message: parts.join(' · '),
    tone: latest.style === 'failure' ? 'error' : 'success',
  }
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

function useExtensionIcon(iconPath: string): {
  src: string | null
  loading: boolean
  clear: () => void
} {
  const [src, setSrc] = useState<string | null>(null)
  const [loading, setLoading] = useState(Boolean(iconPath))

  useEffect(() => {
    let cancelled = false
    setSrc(null)
    setLoading(Boolean(iconPath))

    if (!iconPath) return

    void window.tezbar
      .getAssetIconDataUrl('extension', iconPath)
      .then((value) => {
        if (!cancelled) setSrc(value)
      })
      .catch(() => {
        if (!cancelled) setSrc(null)
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })

    return () => {
      cancelled = true
    }
  }, [iconPath])

  return { src, loading, clear: () => setSrc(null) }
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
  const extensionIcon = useExtensionIcon(iconPath)
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

  const fieldRowClass = 'grid items-start gap-1.5 sm:grid-cols-[132px_minmax(0,1fr)] sm:gap-4'
  const fieldLabelClass =
    'pt-2 text-left text-[11px] font-medium leading-5 text-ink-3 sm:text-right'
  const fieldControlClass = 'glass-field h-9 px-3 py-0 text-[12.5px]'
  const fieldHintClass = 'mt-1.5 block text-[10.5px] leading-[1.45] text-ink-4'

  const renderPreference = (pref: PreferenceField): JSX.Element | null => {
    const name = pref.name
    if (!name) return null
    const label = pref.title || name
    const value = formValues[name] ?? ''

    if (pref.type === 'checkbox') {
      return (
        <div key={name} className={fieldRowClass}>
          <div className={fieldLabelClass}>{label}</div>
          <label className="flex min-h-9 items-center gap-2.5 text-[12px] font-medium text-ink-2">
            <input
              type="checkbox"
              checked={value === 'true'}
              onChange={(event) => setValue(name, event.target.checked ? 'true' : 'false')}
              className="h-3.5 w-3.5 rounded-[4px] accent-accent"
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
    <div className="glass-card relative flex h-full min-h-0 flex-col overflow-hidden text-white">
      <header className="flex h-12 shrink-0 items-center justify-between gap-3 border-b border-white/[0.065] px-3.5">
        <div className="flex min-w-0 items-center gap-2.5">
          <button
            type="button"
            onClick={onBack}
            className="grid h-7 w-7 shrink-0 place-items-center rounded-tezbar-chip text-ink-3 transition hover:bg-white/[0.06] hover:text-ink-1"
            aria-label="Back"
          >
            <svg
              width="14"
              height="14"
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
          <span className="h-4 w-px bg-white/[0.08]" aria-hidden />
          <p className="truncate text-[11.5px] font-medium text-ink-2">{title}</p>
        </div>
        <span className="rounded-tezbar-chip border border-white/[0.07] bg-white/[0.035] px-2 py-1 text-[9.5px] font-medium uppercase tracking-[0.12em] text-ink-4">
          Extension setup
        </span>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-5 py-6 sm:px-8">
        <div className="mx-auto w-full max-w-[760px]">
          <div className="mb-6 flex items-start gap-4">
            <span className="relative grid h-14 w-14 shrink-0 place-items-center overflow-hidden rounded-[14px] border border-white/[0.1] bg-white/[0.04] shadow-[inset_0_1px_0_rgba(255,255,255,0.06)]">
              {extensionIcon.src ? (
                <img
                  src={extensionIcon.src}
                  alt={`${title} icon`}
                  className="h-full w-full object-cover"
                  draggable={false}
                  onError={extensionIcon.clear}
                />
              ) : extensionIcon.loading ? (
                <span className="h-7 w-7 animate-pulse rounded-[8px] bg-white/[0.07]" />
              ) : (
                <span className="text-[18px] font-semibold text-ink-3" aria-hidden>
                  {title.slice(0, 1).toUpperCase()}
                </span>
              )}
            </span>
            <div className="min-w-0 flex-1 pt-0.5">
              <p className="text-[9.5px] font-semibold uppercase tracking-[0.16em] text-accent-strong">
                One-time setup
              </p>
              <h1 className="mt-1 font-display text-[22px] font-semibold leading-tight tracking-[-0.02em] text-ink-1">
                Welcome to {title}
              </h1>
              <p className="mt-1.5 max-w-[580px] text-[12px] leading-[1.55] text-ink-3">
                Add the settings below to start using this extension. You can change them later from
                Extensions in Settings.
              </p>
            </div>
          </div>

          <section className="overflow-hidden rounded-tezbar-card border border-white/[0.065] bg-black/[0.1]">
            <div className="flex items-center justify-between gap-4 border-b border-white/[0.06] px-4 py-3">
              <div>
                <h2 className="text-[12px] font-semibold text-ink-1">Configuration</h2>
                <p className="mt-0.5 text-[10.5px] text-ink-4">
                  Required fields are marked with an asterisk.
                </p>
              </div>
              <span className="inline-flex shrink-0 items-center gap-1.5 rounded-tezbar-chip border border-emerald-300/[0.14] bg-emerald-400/[0.055] px-2 py-1 text-[9.5px] font-medium text-emerald-200/80">
                <svg
                  width="11"
                  height="11"
                  viewBox="0 0 16 16"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  aria-hidden
                >
                  <path d="M8 1.75 13 3.6v3.7c0 3.1-2.05 5.75-5 6.95-2.95-1.2-5-3.85-5-6.95V3.6L8 1.75Z" />
                  <path d="m5.75 8 1.35 1.35 3.15-3.2" />
                </svg>
                Stored locally
              </span>
            </div>

            <div className="space-y-4 p-4">
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
                  <div className="rounded-tezbar-field border border-rose-400/20 bg-rose-400/[0.07] px-3 py-2">
                    <Message tone="error">{localError}</Message>
                  </div>
                </div>
              ) : null}
            </div>
          </section>

          {hasFields ? (
            <div className="mt-4 flex items-center justify-end gap-2">
              <Button variant="quiet" onClick={onBack}>
                Cancel
              </Button>
              <Button variant="primary" onClick={() => void save()} disabled={saving}>
                {saving ? 'Saving…' : 'Save and continue'}
              </Button>
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

  const showActionNotice = useCallback((message: string, persist = false): void => {
    if (actionNoticeTimerRef.current !== null) {
      window.clearTimeout(actionNoticeTimerRef.current)
      actionNoticeTimerRef.current = null
    }
    const tone = /could not|failed|failure|error/i.test(message) ? 'error' : 'success'
    setActionNotice({ message, tone })
    if (persist) return

    actionNoticeTimerRef.current = window.setTimeout(() => {
      actionNoticeTimerRef.current = null
      setActionNotice(null)
    }, 2400)
  }, [])

  const isFormSurface = state.root.type.startsWith('Form')

  const showActionFailure = useCallback(
    (message: string): void => {
      showActionNotice(`Failed · ${message}`, isFormSurface)
    },
    [isFormSurface, showActionNotice]
  )

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
              if (isFormSurface && invokedAction?.kind === 'submit-form') {
                showActionNotice(`Working · ${invokedAction.title}…`, true)
              }
              const result = await window.tezbar.extensionInvokeAction({
                sessionId: state.sessionId,
                actionId,
                formValues,
              })

              if (!result.ok) {
                setError(result.message)
                showActionFailure(result.message)
                return
              }

              const effectNoticeValue = effectNotice(result.effects)
              const messageNotice = result.message?.trim()
                ? {
                    message: result.message.trim(),
                    tone: /could not|failed|failure|error/i.test(result.message)
                      ? ('error' as const)
                      : ('success' as const),
                  }
                : null
              const fallbackNotice = fallbackActionNotice(invokedAction)
              const notice =
                effectNoticeValue ||
                messageNotice ||
                (fallbackNotice ? { message: fallbackNotice, tone: 'success' as const } : null)
              if (notice) showActionNotice(notice.message, isFormSurface)

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
