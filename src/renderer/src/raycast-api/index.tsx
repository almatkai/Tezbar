import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { ExtensionRuntimeAction, ExtensionRuntimeNode } from '../../../shared/extensionRuntime'
import { Action, ActionPanel, ActionPanelOverlay } from './action-runtime'
import { ActionRegistryContext } from './action-runtime-registry'
import { DetailRuntime } from './detail-runtime'
import { FormRuntime, type FormRuntimeHandle } from './form-runtime'
import { GridRuntime } from './grid-runtime'
import { ListRuntime } from './list-runtime'
import {
  Clipboard,
  environment,
  getPreferenceValues,
  NavigationContext,
  open,
  setRuntimeContext,
  showHUD,
  showToast,
  useCachedPromise,
  useFetch,
  useNavigation,
  usePromise,
} from './hooks'

export type ExtensionRuntimeSurfaceProps = {
  sessionId: string
  title: string
  extensionId: string
  commandName: string
  root: ExtensionRuntimeNode
  actions: ExtensionRuntimeAction[]
  actionNotice?: { message: string; tone: 'success' | 'error' } | null
  onBack: () => void
  onSearchTextChanged: (searchText: string) => Promise<void> | void
  onLoadMore: () => Promise<void> | void
  onInvokeAction: (actionId: string, formValues?: Record<string, unknown>) => Promise<void> | void
}

function rootKind(root: ExtensionRuntimeNode): 'list' | 'form' | 'grid' | 'detail' {
  if (root.type.startsWith('Form')) return 'form'
  if (root.type.startsWith('Grid')) return 'grid'
  if (root.type.startsWith('Detail')) return 'detail'
  return 'list'
}

export function ExtensionRuntimeSurface(props: ExtensionRuntimeSurfaceProps): ReactNode {
  const {
    title,
    extensionId,
    commandName,
    root,
    actions,
    actionNotice,
    onBack,
    onSearchTextChanged,
    onLoadMore,
    onInvokeAction,
  } = props
  const [showActions, setShowActions] = useState(false)
  const [actionFilterIds, setActionFilterIds] = useState<string[] | null>(null)
  const formRuntimeRef = useRef<FormRuntimeHandle>(null)

  useEffect(() => {
    setRuntimeContext(extensionId, commandName)
  }, [commandName, extensionId])

  const primaryAction = actions[0]
  const kind = rootKind(root)
  const submitAction = useMemo(
    () =>
      kind === 'form'
        ? actions.find((action) => action.kind === 'submit-form') || primaryAction
        : undefined,
    [actions, kind, primaryAction]
  )
  const actionRegistry = useMemo(() => ({ actions }), [actions])
  const overlayActions = useMemo(() => {
    if (!actionFilterIds) return actions
    const visibleIds = new Set(actionFilterIds)
    return actions.filter((action) => visibleIds.has(action.id))
  }, [actionFilterIds, actions])

  const navApi = useMemo(
    () => ({
      push: () => {
        // Navigation state is maintained in the main-process runtime session.
      },
      pop: () => {
        void onInvokeAction('__nav_pop__')
      },
    }),
    [onInvokeAction]
  )

  useEffect(() => {
    const onKey = (event: KeyboardEvent): void => {
      const isActionShortcut =
        event.metaKey &&
        !event.ctrlKey &&
        !event.altKey &&
        !event.shiftKey &&
        String(event.key || '').toLowerCase() === 'k'

      if (isActionShortcut) {
        event.preventDefault()
        setShowActions((value) => !value)
        return
      }

      if (event.key === 'Enter' && primaryAction && kind === 'detail') {
        event.preventDefault()
        void onInvokeAction(primaryAction.id)
      }
    }

    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [kind, onInvokeAction, primaryAction])

  const onRunPrimaryAction = (actionId?: string, formValues?: Record<string, unknown>): void => {
    const id = actionId || primaryAction?.id
    if (!id) return
    void onInvokeAction(id, formValues)
  }

  const openActions = (ids?: string[]): void => {
    setActionFilterIds(ids && ids.length > 0 ? ids : null)
    setShowActions(true)
  }

  const onSubmitForm = useCallback(
    (values: Record<string, unknown>): void => {
      if (!submitAction) return
      void onInvokeAction(submitAction.id, values)
    },
    [onInvokeAction, submitAction]
  )

  return (
    <NavigationContext.Provider value={navApi}>
      <ActionRegistryContext.Provider value={actionRegistry}>
        <div className="flex h-full min-h-0 flex-col">
          {kind === 'form' ? (
            <FormRuntime
              ref={formRuntimeRef}
              root={root}
              title={title}
              onBack={onBack}
              onSubmitForm={onSubmitForm}
              onChangeField={(actionId, value) => onInvokeAction(actionId, { value })}
              onOpenActions={() => openActions()}
              actionNotice={actionNotice}
            />
          ) : kind === 'grid' ? (
            <GridRuntime
              root={root}
              title={title}
              actions={actions}
              actionNotice={actionNotice}
              onBack={onBack}
              onRunPrimaryAction={onRunPrimaryAction}
              onOpenActions={openActions}
              onSearchTextChanged={onSearchTextChanged}
            />
          ) : kind === 'detail' ? (
            <DetailRuntime
              root={root}
              title={title}
              onBack={onBack}
              onRunPrimaryAction={onRunPrimaryAction}
              onOpenActions={() => openActions()}
            />
          ) : (
            <ListRuntime
              root={root}
              title={title}
              commandName={commandName}
              onBack={onBack}
              onRunPrimaryAction={onRunPrimaryAction}
              actions={actions}
              onOpenActions={() => openActions()}
              onSearchTextChanged={onSearchTextChanged}
              onLoadMore={onLoadMore}
            />
          )}

          {showActions && actions.length > 0 ? (
            <ActionPanelOverlay
              actions={overlayActions}
              onClose={() => {
                setShowActions(false)
                setActionFilterIds(null)
              }}
              onExecute={(action) => {
                setShowActions(false)
                setActionFilterIds(null)
                if (kind === 'form' && action.id === submitAction?.id) {
                  onSubmitForm(formRuntimeRef.current?.getValues() ?? {})
                  return
                }
                void onInvokeAction(action.id)
              }}
            />
          ) : null}
        </div>
      </ActionRegistryContext.Provider>
    </NavigationContext.Provider>
  )
}

export {
  Action,
  ActionPanel,
  Clipboard,
  environment,
  getPreferenceValues,
  open,
  showHUD,
  showToast,
  useCachedPromise,
  useFetch,
  useNavigation,
  usePromise,
}
