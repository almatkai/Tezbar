export type VaultCategory = 'server' | 'api-key' | 'login' | 'note'

export type VaultItem = {
  id: string
  category: VaultCategory
  title: string
  service?: string
  account?: string
  secret: string
  secondarySecret?: string
  port?: number
  authType?: 'password' | 'ssh-key'
  notes?: string
  tags: string[]
  createdAt: number
  updatedAt: number
}

export type VaultItemHeader = {
  id: string
  category: VaultCategory
  title: string
  service?: string
  account?: string
  port?: number
  authType?: 'password' | 'ssh-key'
  tags: string[]
  createdAt: number
  updatedAt: number
}

export type VaultStatus = {
  initialized: boolean
  unlocked: boolean
  biometricsAvailable: boolean
  autoLockMinutes: number
  itemCount: number
}

export type VaultSetupPayload = {
  password: string
  enableBiometrics?: boolean
}

export type VaultUnlockPayload = {
  password?: string
  useBiometrics?: boolean
}
