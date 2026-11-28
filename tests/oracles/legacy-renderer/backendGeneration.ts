import { createContext, useContext } from 'react'

export const BackendGeneration = createContext(0)
export const useBackendGeneration = (): number => useContext(BackendGeneration)
