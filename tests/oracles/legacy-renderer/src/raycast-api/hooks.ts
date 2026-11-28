import { createContext, useContext, useEffect, useMemo, useRef, useState } from 'react'
import { clipboardRead, clipboardWrite, getPreferences, openShellTarget } from './ipc-bridge'

type NavigationApi = {
  push: (view: unknown) => void
  pop: () => void
}

export const NavigationContext = createContext<NavigationApi>({
  push: () => {},
  pop: () => {},
})

export function useNavigation(): NavigationApi {
  return useContext(NavigationContext)
}

const runtimeContext = {
  extensionId: '',
  commandName: '',
}

export function setRuntimeContext(extensionId: string, commandName: string): void {
  runtimeContext.extensionId = extensionId
  runtimeContext.commandName = commandName
  environment.extensionName = extensionId
  environment.commandName = commandName
}

export async function showToast(options: {
  title: string
  message?: string
  style?: string
}): Promise<void> {
  window.dispatchEvent(new CustomEvent('raycast-runtime:toast', { detail: options }))
}

export async function showHUD(title: string): Promise<void> {
  window.dispatchEvent(new CustomEvent('raycast-runtime:hud', { detail: { title } }))
}

export async function getPreferenceValues(): Promise<Record<string, unknown>> {
  if (!runtimeContext.extensionId) return {}
  return getPreferences({
    extensionId: runtimeContext.extensionId,
    commandName: runtimeContext.commandName || undefined,
  })
}

export const Clipboard = {
  copy: async (value: string): Promise<void> => clipboardWrite(String(value ?? '')),
  paste: async (value: string): Promise<void> => clipboardWrite(String(value ?? '')),
  read: async (): Promise<{ text?: string }> => {
    const text = await clipboardRead()
    return text ? { text } : {}
  },
}

export async function open(url: string): Promise<void> {
  await openShellTarget(url)
}

export const environment: {
  raycastVersion: string
  extensionName: string
  commandName: string
  isDevelopment: boolean
} = {
  raycastVersion: '1.80.0',
  extensionName: '',
  commandName: '',
  isDevelopment: false,
}

export function usePromise<T>(factory: () => Promise<T>, deps: unknown[] = []): {
  data: T | undefined
  isLoading: boolean
  error: unknown
  revalidate: () => Promise<void>
} {
  const [data, setData] = useState<T | undefined>(undefined)
  const [isLoading, setIsLoading] = useState(true)
  const [error, setError] = useState<unknown>(undefined)

  const revalidate = async (): Promise<void> => {
    setIsLoading(true)
    setError(undefined)
    try {
      const value = await factory()
      setData(value)
    } catch (err) {
      setError(err)
    } finally {
      setIsLoading(false)
    }
  }

  useEffect(() => {
    void revalidate()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps)

  return { data, isLoading, error, revalidate }
}

export function useFetch<T = string>(url: string | undefined, deps: unknown[] = []): {
  data: T | undefined
  isLoading: boolean
  error: unknown
  revalidate: () => Promise<void>
} {
  return usePromise<T>(
    async () => {
      if (!url) throw new Error('Missing URL')
      const response = await fetch(url)
      if (!response.ok) throw new Error(`Request failed: ${response.status}`)
      return (await response.json()) as T
    },
    [url, ...deps],
  )
}

const cachedPromiseStore = new Map<string, unknown>()

export function useCachedPromise<T>(
  key: string,
  factory: () => Promise<T>,
  deps: unknown[] = [],
): {
  data: T | undefined
  isLoading: boolean
  error: unknown
  revalidate: () => Promise<void>
} {
  const cacheKey = useMemo(() => key, [key])
  const inFlightRef = useRef<Promise<T> | null>(null)

  return usePromise<T>(
    async () => {
      if (cachedPromiseStore.has(cacheKey)) {
        return cachedPromiseStore.get(cacheKey) as T
      }

      if (!inFlightRef.current) {
        inFlightRef.current = factory()
      }

      const value = await inFlightRef.current
      cachedPromiseStore.set(cacheKey, value)
      inFlightRef.current = null
      return value
    },
    [cacheKey, ...deps],
  )
}
