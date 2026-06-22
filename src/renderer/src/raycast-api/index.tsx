import { useEffect, useMemo, useState } from 'react'
import type {
  ExtensionRuntimeAction,
  ExtensionRuntimeNode,
} from '../../../shared/extensionRuntime'
import { Action, ActionPanel, ActionPanelOverlay } from './action-runtime'
import { ActionRegistryContext } from './action-runtime-registry'
import { DetailRuntime } from './detail-runtime'
import { FormRuntime } from './form-runtime'
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

export function ExtensionRuntimeSurface(props: ExtensionRuntimeSurfaceProps): JSX.Element {
  const { title, extensionId, commandName, root, actions, onBack, onSearchTextChanged, onLoadMore, onInvokeAction } = props
  const [showActions, setShowActions] = useState(false)
  const [actionFilterIds, setActionFilterIds] = useState<string[] | null>(null)

  useEffect(() => {
    setRuntimeContext(extensionId, commandName)
  }, [commandName, extensionId])

  const primaryAction = actions[0]
  const kind = rootKind(root)

  const navApi = useMemo(
    () => ({
      push: () => {
        // Navigation state is maintained in the main-process runtime session.
      },
      pop: () => {
        void onInvokeAction('__nav_pop__')
      },
    }),
    [onInvokeAction],
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

      if (event.key === 'Enter' && primaryAction && kind !== 'list') {
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

  const onSubmitForm = (values: Record<string, unknown>): void => {
    const submitAction = actions.find((action) => action.kind === 'submit-form') || primaryAction
    if (!submitAction) return
    void onInvokeAction(submitAction.id, values)
  }

  return (
    <NavigationContext.Provider value={navApi}>
      <ActionRegistryContext.Provider value={{ actions }}>
        <div className="flex h-full min-h-0 flex-col">
          {kind === 'form' ? (
            <FormRuntime
              root={root}
              title={title}
              onBack={onBack}
              onSubmitForm={onSubmitForm}
              onOpenActions={() => openActions()}
            />
          ) : kind === 'grid' ? (
            <GridRuntime
              root={root}
              title={title}
              onBack={onBack}
              onRunPrimaryAction={onRunPrimaryAction}
              onOpenActions={openActions}
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
              actions={actionFilterIds ? actions.filter((action) => actionFilterIds.includes(action.id)) : actions}
              onClose={() => {
                setShowActions(false)
                setActionFilterIds(null)
              }}
              onExecute={(action) => {
                setShowActions(false)
                setActionFilterIds(null)
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
