import { createContext, useContext } from 'react'
import type { ExtensionRuntimeAction } from '../../../../../src/shared/extensionRuntime'

export type ActionRegistryShape = {
  actions: ExtensionRuntimeAction[]
}

export const ActionRegistryContext = createContext<ActionRegistryShape>({
  actions: [],
})

export function useCollectedActions(): {
  collectedActions: ExtensionRuntimeAction[]
} {
  const ctx = useContext(ActionRegistryContext)
  return { collectedActions: ctx.actions }
}

export function useActionRegistration(): null {
  // Runtime actions are serialized in the main process and delivered as plain
  // descriptors, so there is nothing to register in the renderer.
  return null
}
