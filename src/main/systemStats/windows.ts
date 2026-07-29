import { execFile } from 'node:child_process'
import { statfs } from 'node:fs/promises'
import {
  cpus,
  freemem,
  hostname,
  networkInterfaces,
  release,
  totalmem,
  uptime,
  version,
} from 'node:os'
import { promisify } from 'node:util'
import type { SystemStatsSnapshot } from '../../shared/systemStats'

const execFileAsync = promisify(execFile)
const MEBIBYTE = 1024 ** 2

type JsonRecord = Record<string, unknown>

type CpuTimes = {
  user: number
  nice: number
  system: number
  idle: number
  irq: number
}

type NetworkSample = {
  receivedBytes: number
  sentBytes: number
  sampledAt: number
}

type WindowsStaticStats = Pick<SystemStatsSnapshot, 'device' | 'gpu' | 'storage' | 'fans'> & {
  cpu: Pick<SystemStatsSnapshot['cpu'], 'model' | 'totalCores'>
  batteryHealth: {
    maximumCapacityPercent?: number
    cycleCount?: number
  }
}

let staticStatsPromise: Promise<WindowsStaticStats> | null = null
let previousCpuTimes: CpuTimes | null = null
let previousNetworkSample: NetworkSample | null = null

const STATIC_POWERSHELL = `
$ErrorActionPreference = 'SilentlyContinue'
$computer = Get-CimInstance Win32_ComputerSystem | Select-Object -First 1
$processor = Get-CimInstance Win32_Processor | Select-Object -First 1
$gpu = Get-CimInstance Win32_VideoController | Sort-Object -Property AdapterRAM -Descending | Select-Object -First 1
$os = Get-CimInstance Win32_OperatingSystem | Select-Object -First 1
$driveId = $env:SystemDrive
$drive = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='$driveId'" | Select-Object -First 1
$physical = Get-PhysicalDisk | Sort-Object -Property Size -Descending | Select-Object -First 1
$batteryDesign = (Get-CimInstance -Namespace root/WMI BatteryStaticData | Measure-Object -Property DesignedCapacity -Sum).Sum
$batteryFull = (Get-CimInstance -Namespace root/WMI BatteryFullChargedCapacity | Measure-Object -Property FullChargedCapacity -Sum).Sum
$batteryCycle = (Get-CimInstance -Namespace root/WMI BatteryCycleCount | Measure-Object -Property CycleCount -Sum).Sum
[ordered]@{
  manufacturer = [string]$computer.Manufacturer
  model = [string]$computer.Model
  cpuModel = [string]$processor.Name
  logicalProcessors = [int]$computer.NumberOfLogicalProcessors
  gpuModel = [string]$gpu.Name
  displayWidth = [int]$gpu.CurrentHorizontalResolution
  displayHeight = [int]$gpu.CurrentVerticalResolution
  displayRefreshRate = [int]$gpu.CurrentRefreshRate
  osCaption = [string]$os.Caption
  osVersion = [string]$os.Version
  osBuild = [string]$os.BuildNumber
  storageName = [string]$drive.VolumeName
  storageDevice = [string]$drive.DeviceID
  storageTotalBytes = [double]$drive.Size
  storageFreeBytes = [double]$drive.FreeSpace
  storageHealth = [string]$physical.HealthStatus
  fanSpeeds = @(Get-CimInstance Win32_Fan | ForEach-Object { [double]$_.DesiredSpeed } | Where-Object { $_ -gt 0 })
  batteryDesignCapacity = [double]$batteryDesign
  batteryFullCapacity = [double]$batteryFull
  batteryCycleCount = [double]$batteryCycle
} | ConvertTo-Json -Compress -Depth 5
`

const LIVE_POWERSHELL = `
$ErrorActionPreference = 'SilentlyContinue'
$battery = Get-CimInstance Win32_Battery | Select-Object -First 1
$pageFiles = @(Get-CimInstance Win32_PageFileUsage)
$profile = Get-NetConnectionProfile | Where-Object {
  $_.IPv4Connectivity -ne 'Disconnected' -or $_.IPv6Connectivity -ne 'Disconnected'
} | Select-Object -First 1
$processes = @(Get-Process)
[ordered]@{
  batteryPresent = [bool]($null -ne $battery)
  batteryChargePercent = [double]$battery.EstimatedChargeRemaining
  batteryStatus = [int]$battery.BatteryStatus
  batteryMinutesRemaining = [double]$battery.EstimatedRunTime
  swapTotalMebibytes = [double](($pageFiles | Measure-Object AllocatedBaseSize -Sum).Sum)
  swapUsedMebibytes = [double](($pageFiles | Measure-Object CurrentUsage -Sum).Sum)
  interfaceAlias = [string]$profile.InterfaceAlias
  networkProfileName = [string]$profile.Name
  processTotal = [int]$processes.Count
  processRunning = [int]$processes.Count
} | ConvertTo-Json -Compress -Depth 4
`

async function command(file: string, args: string[]): Promise<string> {
  const { stdout } = await execFileAsync(file, args, {
    timeout: 8_000,
    maxBuffer: 8 * 1024 * 1024,
    windowsHide: true,
  })
  return stdout.trim()
}

async function optionalCommand(file: string, args: string[]): Promise<string> {
  try {
    return await command(file, args)
  } catch {
    return ''
  }
}

async function powershell(script: string): Promise<JsonRecord> {
  const raw = await optionalCommand('powershell.exe', [
    '-NoLogo',
    '-NoProfile',
    '-NonInteractive',
    '-ExecutionPolicy',
    'Bypass',
    '-Command',
    script,
  ])
  if (!raw) return {}
  try {
    const parsed: unknown = JSON.parse(raw)
    return record(parsed)
  } catch {
    return {}
  }
}

function record(value: unknown): JsonRecord {
  return value && typeof value === 'object' ? (value as JsonRecord) : {}
}

function text(value: unknown, fallback = ''): string {
  return typeof value === 'string' && value.trim() ? value.trim() : fallback
}

function finiteNumber(value: unknown, fallback = 0): number {
  const parsed = typeof value === 'number' ? value : Number(value)
  return Number.isFinite(parsed) ? parsed : fallback
}

function positiveNumber(value: unknown): number | undefined {
  const parsed = finiteNumber(value)
  return parsed > 0 ? parsed : undefined
}

function systemDrive(): string {
  const root = process.env.SystemDrive || process.env.SYSTEMDRIVE || 'C:'
  return `${root.replace(/[\\/]+$/, '')}\\`
}

async function loadStaticStats(): Promise<WindowsStaticStats> {
  const payload = await powershell(STATIC_POWERSHELL)
  const cpuRows = cpus()
  const storageTotalBytes = finiteNumber(payload.storageTotalBytes)
  const storageFreeBytes = finiteNumber(payload.storageFreeBytes)
  const manufacturer = text(payload.manufacturer)
  const model = text(payload.model, 'Windows PC')
  const displayWidth = positiveNumber(payload.displayWidth)
  const displayHeight = positiveNumber(payload.displayHeight)
  const displayRefreshRate = positiveNumber(payload.displayRefreshRate)
  const fanSpeeds = Array.isArray(payload.fanSpeeds)
    ? payload.fanSpeeds.map(finiteNumber).filter((rpm) => rpm > 0)
    : positiveNumber(payload.fanSpeeds)
      ? [finiteNumber(payload.fanSpeeds)]
      : []
  const designedCapacity = positiveNumber(payload.batteryDesignCapacity)
  const fullCapacity = positiveNumber(payload.batteryFullCapacity)
  const osVersion = [
    text(payload.osCaption, version()),
    text(payload.osVersion, release()),
    text(payload.osBuild) ? `build ${text(payload.osBuild)}` : '',
  ]
    .filter(Boolean)
    .join(' · ')

  return {
    device: {
      platform: 'Windows',
      name: hostname(),
      model: [manufacturer, model].filter(Boolean).join(' ') || 'Windows PC',
      chip: text(payload.cpuModel, cpuRows[0]?.model || 'Processor'),
      osVersion,
      uptimeSeconds: 0,
    },
    cpu: {
      model: text(payload.cpuModel, cpuRows[0]?.model || 'Processor'),
      totalCores: finiteNumber(payload.logicalProcessors, cpuRows.length),
    },
    gpu: {
      model: text(payload.gpuModel, 'Graphics adapter'),
      display:
        displayWidth && displayHeight
          ? `${displayWidth} × ${displayHeight}${displayRefreshRate ? ` @ ${displayRefreshRate} Hz` : ''}`
          : undefined,
    },
    storage: {
      name: text(payload.storageName, 'System drive'),
      device: text(payload.storageDevice, systemDrive().slice(0, 2)),
      totalBytes: storageTotalBytes,
      usedBytes: Math.max(0, storageTotalBytes - storageFreeBytes),
      freeBytes: storageFreeBytes,
      smartStatus: text(payload.storageHealth) || undefined,
    },
    fans:
      fanSpeeds.length > 0
        ? { available: true, status: 'Active', rpms: fanSpeeds }
        : {
            available: false,
            status: 'Managed by firmware',
            rpms: [],
            note: 'Fan RPM is not exposed by standard Windows hardware APIs on this PC.',
          },
    batteryHealth: {
      maximumCapacityPercent:
        designedCapacity && fullCapacity
          ? Math.min(100, Math.round((fullCapacity / designedCapacity) * 100))
          : undefined,
      cycleCount: positiveNumber(payload.batteryCycleCount),
    },
  }
}

function staticStats(): Promise<WindowsStaticStats> {
  if (!staticStatsPromise) {
    staticStatsPromise = loadStaticStats().catch((error) => {
      staticStatsPromise = null
      throw error
    })
  }
  return staticStatsPromise
}

function aggregateCpuTimes(): CpuTimes {
  return cpus().reduce<CpuTimes>(
    (total, cpu) => ({
      user: total.user + cpu.times.user,
      nice: total.nice + cpu.times.nice,
      system: total.system + cpu.times.sys,
      idle: total.idle + cpu.times.idle,
      irq: total.irq + cpu.times.irq,
    }),
    { user: 0, nice: 0, system: 0, idle: 0, irq: 0 }
  )
}

export function cpuUsageBetween(
  previous: CpuTimes,
  current: CpuTimes
): Pick<
  SystemStatsSnapshot['cpu'],
  'usagePercent' | 'userPercent' | 'systemPercent' | 'idlePercent'
> {
  const user = Math.max(0, current.user + current.nice - previous.user - previous.nice)
  const system = Math.max(
    0,
    current.system + current.irq - previous.system - previous.irq
  )
  const idle = Math.max(0, current.idle - previous.idle)
  const total = user + system + idle
  if (total <= 0) {
    return { usagePercent: 0, userPercent: 0, systemPercent: 0, idlePercent: 100 }
  }
  const userPercent = (user / total) * 100
  const systemPercent = (system / total) * 100
  const idlePercent = (idle / total) * 100
  return {
    usagePercent: Math.min(100, userPercent + systemPercent),
    userPercent,
    systemPercent,
    idlePercent,
  }
}

async function liveCpuUsage(): Promise<ReturnType<typeof cpuUsageBetween>> {
  let previous = previousCpuTimes
  if (!previous) {
    previous = aggregateCpuTimes()
    await new Promise((resolve) => setTimeout(resolve, 120))
  }
  const current = aggregateCpuTimes()
  previousCpuTimes = current
  return cpuUsageBetween(previous, current)
}

export function parseNetstatEthernetStatistics(
  output: string
): Pick<NetworkSample, 'receivedBytes' | 'sentBytes'> | null {
  for (const line of output.split(/\r?\n/)) {
    const match = line.match(/(\d+)\s+(\d+)\s*$/)
    if (!match) continue
    return {
      receivedBytes: finiteNumber(match[1]),
      sentBytes: finiteNumber(match[2]),
    }
  }
  return null
}

function networkRates(sample: NetworkSample | null): {
  downloadBytesPerSecond?: number
  uploadBytesPerSecond?: number
} {
  if (!sample) return {}
  const previous = previousNetworkSample
  previousNetworkSample = sample
  if (!previous) return {}
  const elapsedSeconds = (sample.sampledAt - previous.sampledAt) / 1000
  if (elapsedSeconds <= 0) return {}
  return {
    downloadBytesPerSecond: Math.max(
      0,
      (sample.receivedBytes - previous.receivedBytes) / elapsedSeconds
    ),
    uploadBytesPerSecond: Math.max(
      0,
      (sample.sentBytes - previous.sentBytes) / elapsedSeconds
    ),
  }
}

function activeNetwork(interfaceHint: string): {
  interfaceName?: string
  localIp?: string
  connectionType?: string
} {
  const interfaces = networkInterfaces()
  const interfaceName =
    (interfaceHint && interfaces[interfaceHint] ? interfaceHint : undefined) ??
    Object.entries(interfaces).find(([, addresses]) =>
      addresses?.some((address) => {
        const family = String(address.family)
        return !address.internal && (family === 'IPv4' || family === '4')
      })
    )?.[0]
  const address = interfaceName
    ? interfaces[interfaceName]?.find((candidate) => {
        const family = String(candidate.family)
        return !candidate.internal && (family === 'IPv4' || family === '4')
      })
    : undefined
  return {
    interfaceName,
    localIp: address?.address,
    connectionType: interfaceName
      ? /wi-?fi|wireless|wlan/i.test(interfaceName)
        ? 'Wi-Fi'
        : 'Ethernet'
      : undefined,
  }
}

async function storageSnapshot(
  fixed: SystemStatsSnapshot['storage']
): Promise<SystemStatsSnapshot['storage']> {
  try {
    const stats = await statfs(systemDrive())
    const totalBytes = Number(stats.blocks) * Number(stats.bsize)
    const freeBytes = Number(stats.bavail) * Number(stats.bsize)
    return {
      ...fixed,
      totalBytes,
      usedBytes: Math.max(0, totalBytes - freeBytes),
      freeBytes,
    }
  } catch {
    return fixed
  }
}

function batterySnapshot(
  live: JsonRecord,
  health: WindowsStaticStats['batteryHealth']
): SystemStatsSnapshot['battery'] {
  if (live.batteryPresent !== true) return null
  const status = finiteNumber(live.batteryStatus)
  const minutes = finiteNumber(live.batteryMinutesRemaining)
  const isCharging = status === 6 || status === 7 || status === 8 || status === 9
  const isPluggedIn = isCharging || status === 2 || status === 3
  return {
    chargePercent: Math.min(100, finiteNumber(live.batteryChargePercent)),
    maximumCapacityPercent: health.maximumCapacityPercent,
    cycleCount: health.cycleCount,
    condition:
      health.maximumCapacityPercent === undefined
        ? undefined
        : health.maximumCapacityPercent >= 80
          ? 'Normal'
          : 'Service recommended',
    isCharging,
    isPluggedIn,
    timeRemainingMinutes:
      minutes > 0 && minutes < 715_827_882 ? Math.round(minutes) : undefined,
    lowPowerMode: false,
  }
}

export async function getWindowsSystemStats(): Promise<SystemStatsSnapshot> {
  const [fixed, live, networkOutput, cpu] = await Promise.all([
    staticStats(),
    powershell(LIVE_POWERSHELL),
    optionalCommand('netstat.exe', ['-e']),
    liveCpuUsage(),
  ])
  const totalBytes = totalmem()
  const freeBytes = freemem()
  const interfaceHint = text(live.interfaceAlias)
  const active = activeNetwork(interfaceHint)
  const networkCounters = parseNetstatEthernetStatistics(networkOutput)
  const sample = networkCounters ? { ...networkCounters, sampledAt: Date.now() } : null
  const storage = await storageSnapshot(fixed.storage)
  const swapTotalBytes = positiveNumber(live.swapTotalMebibytes)
  const swapUsedBytes = positiveNumber(live.swapUsedMebibytes)

  return {
    collectedAt: new Date().toISOString(),
    device: {
      ...fixed.device,
      uptimeSeconds: uptime(),
    },
    cpu: {
      ...fixed.cpu,
      ...cpu,
      loadAverage: [0, 0, 0],
    },
    gpu: fixed.gpu,
    memory: {
      totalBytes,
      usedBytes: Math.max(0, totalBytes - freeBytes),
      freeBytes,
      swapTotalBytes: swapTotalBytes ? swapTotalBytes * MEBIBYTE : undefined,
      swapUsedBytes: swapUsedBytes ? swapUsedBytes * MEBIBYTE : undefined,
    },
    storage,
    battery: batterySnapshot(live, fixed.batteryHealth),
    network: {
      interface: active.interfaceName ?? (interfaceHint || undefined),
      connectionType: active.connectionType,
      localIp: active.localIp,
      ssid: text(live.networkProfileName) || undefined,
      ...networkRates(sample),
    },
    fans: fixed.fans,
    processes: {
      total: finiteNumber(live.processTotal),
      running: finiteNumber(live.processRunning),
    },
  }
}

export function clearWindowsSystemStatsCacheForTests(): void {
  staticStatsPromise = null
  previousCpuTimes = null
  previousNetworkSample = null
}
