export type SystemStatsSnapshot = {
  collectedAt: string
  device: {
    platform?: 'macOS' | 'Windows'
    name: string
    model: string
    chip: string
    osVersion: string
    uptimeSeconds: number
  }
  cpu: {
    model: string
    totalCores: number
    performanceCores?: number
    efficiencyCores?: number
    usagePercent: number
    userPercent: number
    systemPercent: number
    idlePercent: number
    loadAverage: [number, number, number]
  }
  gpu: {
    model: string
    cores?: number
    display?: string
  }
  memory: {
    totalBytes: number
    usedBytes: number
    freeBytes: number
    wiredBytes?: number
    compressedBytes?: number
    swapUsedBytes?: number
    swapTotalBytes?: number
  }
  storage: {
    name: string
    device?: string
    totalBytes: number
    usedBytes: number
    freeBytes: number
    smartStatus?: string
  }
  battery: {
    chargePercent: number
    maximumCapacityPercent?: number
    cycleCount?: number
    condition?: string
    isCharging: boolean
    isPluggedIn: boolean
    timeRemainingMinutes?: number
    temperatureCelsius?: number
    lowPowerMode: boolean
  } | null
  network: {
    interface?: string
    connectionType?: string
    localIp?: string
    ssid?: string
    downloadBytesPerSecond?: number
    uploadBytesPerSecond?: number
  }
  fans: {
    available: boolean
    status: string
    rpms: number[]
    note?: string
  }
  processes: {
    total: number
    running: number
  }
}
