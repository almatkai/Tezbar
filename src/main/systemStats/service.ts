import { execFile } from 'node:child_process'
import { hostname, uptime } from 'node:os'
import { promisify } from 'node:util'
import type { SystemStatsSnapshot } from '../../shared/systemStats'

const execFileAsync = promisify(execFile)

type JsonRecord = Record<string, unknown>

type StaticSystemStats = Pick<SystemStatsSnapshot, 'device' | 'gpu' | 'storage'> & {
  cpu: Pick<
    SystemStatsSnapshot['cpu'],
    'model' | 'totalCores' | 'performanceCores' | 'efficiencyCores'
  >
  batteryHealth: {
    maximumCapacityPercent?: number
    cycleCount?: number
    condition?: string
    lowPowerMode: boolean
  }
}

type NetworkSample = {
  interfaceName: string
  receivedBytes: number
  sentBytes: number
  sampledAt: number
}

let staticStatsPromise: Promise<StaticSystemStats> | null = null
let previousNetworkSample: NetworkSample | null = null

async function command(file: string, args: string[]): Promise<string> {
  const { stdout } = await execFileAsync(file, args, {
    timeout: 8_000,
    maxBuffer: 8 * 1024 * 1024,
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

function record(value: unknown): JsonRecord {
  return value && typeof value === 'object' ? (value as JsonRecord) : {}
}

function firstRecord(value: unknown): JsonRecord {
  return Array.isArray(value) ? record(value[0]) : {}
}

function text(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback
}

function finiteNumber(value: unknown, fallback = 0): number {
  const parsed = typeof value === 'number' ? value : Number(value)
  return Number.isFinite(parsed) ? parsed : fallback
}

function percent(value: unknown): number | undefined {
  const match = String(value ?? '').match(/([\d.]+)\s*%/)
  if (!match) return undefined
  const parsed = Number(match[1])
  return Number.isFinite(parsed) ? parsed : undefined
}

function normalizeProfilerLabel(value: unknown): string | undefined {
  const raw = text(value)
  if (!raw) return undefined
  const aliases: Record<string, string> = {
    Good: 'Normal',
    sppower_battery_health_good: 'Normal',
    Verified: 'Verified',
  }
  return aliases[raw] ?? raw.replace(/^sp[a-z]+_/, '').replaceAll('_', ' ')
}

async function loadStaticStats(): Promise<StaticSystemStats> {
  const raw = await command('system_profiler', [
    'SPHardwareDataType',
    'SPDisplaysDataType',
    'SPPowerDataType',
    'SPStorageDataType',
    '-json',
  ])
  const payload = record(JSON.parse(raw))
  const hardware = firstRecord(payload.SPHardwareDataType)
  const display = firstRecord(payload.SPDisplaysDataType)
  const builtInDisplay = firstRecord(display.spdisplays_ndrvs)
  const powerRows = Array.isArray(payload.SPPowerDataType) ? payload.SPPowerDataType : []
  const batteryRow = record(
    powerRows.find((item) => record(item)._name === 'spbattery_information')
  )
  const batteryHealth = record(batteryRow.sppower_battery_health_info)
  const powerSettings = record(
    powerRows.find((item) => record(item)._name === 'sppower_information')
  )
  const batteryPower = record(powerSettings['Battery Power'])
  const storageRows = Array.isArray(payload.SPStorageDataType) ? payload.SPStorageDataType : []
  const rootStorage = record(
    storageRows.find((item) => record(item).mount_point === '/') ?? storageRows[0]
  )
  const physicalDrive = record(rootStorage.physical_drive)
  const processorMatch = text(hardware.number_processors).match(/proc\s+(\d+):(\d+):(\d+)/)
  const totalCores = processorMatch ? Number(processorMatch[1]) : 0
  const performanceCores = processorMatch ? Number(processorMatch[2]) : undefined
  const efficiencyCores = processorMatch ? Number(processorMatch[3]) : undefined
  const totalBytes = finiteNumber(rootStorage.size_in_bytes)
  const freeBytes = finiteNumber(rootStorage.free_space_in_bytes)

  return {
    device: {
      name: text(hardware.machine_name, hostname()),
      model: text(hardware.machine_model, 'Mac'),
      chip: text(hardware.chip_type, 'Apple Silicon'),
      osVersion: '',
      uptimeSeconds: 0,
    },
    cpu: {
      model: text(hardware.chip_type, 'Apple Silicon'),
      totalCores,
      performanceCores,
      efficiencyCores,
    },
    gpu: {
      model: text(display.sppci_model, text(display._name, 'Apple GPU')),
      cores: finiteNumber(display.sppci_cores) || undefined,
      display: text(builtInDisplay._spdisplays_pixels) || undefined,
    },
    storage: {
      name: text(rootStorage._name, 'Macintosh HD'),
      device: text(physicalDrive.device_name) || undefined,
      totalBytes,
      usedBytes: Math.max(0, totalBytes - freeBytes),
      freeBytes,
      smartStatus: normalizeProfilerLabel(physicalDrive.smart_status),
    },
    batteryHealth: {
      maximumCapacityPercent: percent(batteryHealth.sppower_battery_health_maximum_capacity),
      cycleCount: finiteNumber(batteryHealth.sppower_battery_cycle_count) || undefined,
      condition: normalizeProfilerLabel(batteryHealth.sppower_battery_health),
      lowPowerMode: batteryPower.LowPowerMode === 'Yes',
    },
  }
}

function staticStats(): Promise<StaticSystemStats> {
  if (!staticStatsPromise) {
    staticStatsPromise = loadStaticStats().catch((error) => {
      staticStatsPromise = null
      throw error
    })
  }
  return staticStatsPromise
}

export function parseTopOutput(output: string): {
  cpu: Pick<
    SystemStatsSnapshot['cpu'],
    'usagePercent' | 'userPercent' | 'systemPercent' | 'idlePercent' | 'loadAverage'
  >
  memory: Pick<SystemStatsSnapshot['memory'], 'usedBytes' | 'freeBytes'>
  processes: SystemStatsSnapshot['processes']
} {
  const cpuMatch = output.match(/CPU usage:\s*([\d.]+)% user,\s*([\d.]+)% sys,\s*([\d.]+)% idle/i)
  const loadMatch = output.match(/Load Avg:\s*([\d.]+),\s*([\d.]+),\s*([\d.]+)/i)
  const processMatch = output.match(/Processes:\s*(\d+) total,\s*(\d+) running/i)
  const memoryMatch = output.match(/PhysMem:\s*([\d.]+)([KMG]) used.*?,\s*([\d.]+)([KMG]) unused/i)
  const userPercent = finiteNumber(cpuMatch?.[1])
  const systemPercent = finiteNumber(cpuMatch?.[2])

  return {
    cpu: {
      userPercent,
      systemPercent,
      idlePercent: finiteNumber(cpuMatch?.[3], 100),
      usagePercent: Math.min(100, userPercent + systemPercent),
      loadAverage: [
        finiteNumber(loadMatch?.[1]),
        finiteNumber(loadMatch?.[2]),
        finiteNumber(loadMatch?.[3]),
      ],
    },
    memory: {
      usedBytes: parseBinaryUnit(memoryMatch?.[1], memoryMatch?.[2]),
      freeBytes: parseBinaryUnit(memoryMatch?.[3], memoryMatch?.[4]),
    },
    processes: {
      total: finiteNumber(processMatch?.[1]),
      running: finiteNumber(processMatch?.[2]),
    },
  }
}

function parseBinaryUnit(value: unknown, unit: unknown): number {
  const amount = finiteNumber(value)
  const multiplier = unit === 'G' ? 1024 ** 3 : unit === 'M' ? 1024 ** 2 : 1024
  return amount * multiplier
}

function matchIoregNumber(output: string, key: string): number | undefined {
  const match = output.match(new RegExp(`"${key}"\\s*=\\s*(\\d+)`))
  if (!match) return undefined
  const parsed = Number(match[1])
  return Number.isFinite(parsed) ? parsed : undefined
}

function matchIoregBoolean(output: string, key: string): boolean {
  return new RegExp(`"${key}"\\s*=\\s*Yes`, 'i').test(output)
}

function parseBattery(
  output: string,
  health: StaticSystemStats['batteryHealth']
): SystemStatsSnapshot['battery'] {
  if (!/AppleSmartBattery/.test(output)) return null
  const temperature = matchIoregNumber(output, 'Temperature')
  return {
    chargePercent: matchIoregNumber(output, 'CurrentCapacity') ?? 0,
    maximumCapacityPercent: health.maximumCapacityPercent,
    cycleCount: matchIoregNumber(output, 'CycleCount') ?? health.cycleCount,
    condition: health.condition,
    isCharging: matchIoregBoolean(output, 'IsCharging'),
    isPluggedIn: matchIoregBoolean(output, 'ExternalConnected'),
    timeRemainingMinutes: matchIoregNumber(output, 'TimeRemaining'),
    temperatureCelsius: temperature === undefined ? undefined : temperature / 100,
    lowPowerMode: health.lowPowerMode,
  }
}

function parseVmStat(
  output: string,
  totalBytes: number
): Pick<SystemStatsSnapshot['memory'], 'wiredBytes' | 'compressedBytes'> {
  const pageSize = finiteNumber(output.match(/page size of (\d+) bytes/i)?.[1], 16_384)
  const pages = (label: string): number =>
    finiteNumber(output.match(new RegExp(`${label}:\\s+(\\d+)\\.`))?.[1])
  return {
    wiredBytes: Math.min(totalBytes, pages('Pages wired down') * pageSize),
    compressedBytes: Math.min(totalBytes, pages('Pages occupied by compressor') * pageSize),
  }
}

function parseSwap(
  output: string
): Pick<SystemStatsSnapshot['memory'], 'swapUsedBytes' | 'swapTotalBytes'> {
  const match = output.match(/total = ([\d.]+)([MG])\s+used = ([\d.]+)([MG])/i)
  return {
    swapTotalBytes: parseBinaryUnit(match?.[1], match?.[2]),
    swapUsedBytes: parseBinaryUnit(match?.[3], match?.[4]),
  }
}

function parseDiskFree(output: string, fallback: number): number {
  const dataLine = output
    .split('\n')
    .map((line) => line.trim())
    .find((line) => line.startsWith('/dev/'))
  if (!dataLine) return fallback
  const availableKilobytes = finiteNumber(dataLine.split(/\s+/)[3], Number.NaN)
  return Number.isFinite(availableKilobytes) ? availableKilobytes * 1024 : fallback
}

function parseNetworkBytes(output: string, interfaceName: string): NetworkSample | null {
  const line = output
    .split('\n')
    .find((row) => row.trim().startsWith(`${interfaceName} `) && row.includes('<Link#'))
  if (!line) return null
  const columns = line.trim().split(/\s+/)
  const linkIndex = columns.findIndex((value) => value.startsWith('<Link#'))
  if (linkIndex < 0) return null
  const receivedBytes = finiteNumber(columns[linkIndex + 4], Number.NaN)
  const sentBytes = finiteNumber(columns[linkIndex + 7], Number.NaN)
  if (!Number.isFinite(receivedBytes) || !Number.isFinite(sentBytes)) return null
  return { interfaceName, receivedBytes, sentBytes, sampledAt: Date.now() }
}

function networkRates(sample: NetworkSample | null): {
  downloadBytesPerSecond?: number
  uploadBytesPerSecond?: number
} {
  if (!sample) return {}
  const previous = previousNetworkSample
  previousNetworkSample = sample
  if (!previous || previous.interfaceName !== sample.interfaceName) return {}
  const elapsedSeconds = (sample.sampledAt - previous.sampledAt) / 1000
  if (elapsedSeconds <= 0) return {}
  return {
    downloadBytesPerSecond: Math.max(
      0,
      (sample.receivedBytes - previous.receivedBytes) / elapsedSeconds
    ),
    uploadBytesPerSecond: Math.max(0, (sample.sentBytes - previous.sentBytes) / elapsedSeconds),
  }
}

function parseFans(output: string): SystemStatsSnapshot['fans'] {
  const rpms = [...output.matchAll(/"F\d+Ac"\s*=\s*(\d+)/g)]
    .map((match) => Number(match[1]))
    .filter((rpm) => Number.isFinite(rpm) && rpm > 0)
  if (rpms.length > 0) {
    return { available: true, status: 'Active', rpms }
  }
  return {
    available: false,
    status: 'Managed automatically',
    rpms: [],
    note: 'Fan RPM is not exposed by macOS on this Mac without a privileged sensor helper.',
  }
}

function parseNetworkSummary(
  output: string
): Pick<SystemStatsSnapshot['network'], 'connectionType' | 'localIp' | 'ssid'> {
  const value = (key: string): string | undefined => {
    const raw = output.match(new RegExp(`^\\s*${key}\\s*:\\s*(.+)$`, 'm'))?.[1]?.trim()
    return raw && raw !== '<redacted>' ? raw : undefined
  }
  return {
    connectionType: value('InterfaceType'),
    localIp: output.match(/^\s*0\s*:\s*((?:\d{1,3}\.){3}\d{1,3})\s*$/m)?.[1],
    ssid: value('SSID'),
  }
}

export async function getSystemStats(): Promise<SystemStatsSnapshot> {
  if (process.platform !== 'darwin') {
    throw new Error('System Monitor is currently available on macOS only.')
  }

  const fixed = await staticStats()
  const [
    top,
    vmStat,
    memorySize,
    swap,
    battery,
    defaultRoute,
    networkRows,
    osVersion,
    fans,
    diskUsage,
  ] = await Promise.all([
    command('top', ['-l', '1', '-n', '0', '-stats', 'cpu']),
    command('vm_stat', []),
    command('sysctl', ['-n', 'hw.memsize']),
    optionalCommand('sysctl', ['-n', 'vm.swapusage']),
    optionalCommand('ioreg', ['-r', '-n', 'AppleSmartBattery', '-d', '1']),
    optionalCommand('route', ['-n', 'get', 'default']),
    optionalCommand('netstat', ['-ibn']),
    optionalCommand('sw_vers', ['-productVersion']),
    optionalCommand('ioreg', ['-r', '-c', 'AppleSMC', '-d', '1']),
    optionalCommand('df', ['-k', '/']),
  ])

  const totalBytes = finiteNumber(memorySize)
  const live = parseTopOutput(top)
  const interfaceName = defaultRoute.match(/interface:\s*(\S+)/)?.[1]
  const networkSummary = interfaceName
    ? parseNetworkSummary(await optionalCommand('ipconfig', ['getsummary', interfaceName]))
    : {}
  const sample = interfaceName ? parseNetworkBytes(networkRows, interfaceName) : null
  const freeBytes = Math.min(totalBytes, live.memory.freeBytes)
  const usedBytes = Math.max(0, totalBytes - freeBytes)
  const storageFreeBytes = Math.min(
    fixed.storage.totalBytes,
    parseDiskFree(diskUsage, fixed.storage.freeBytes)
  )

  return {
    collectedAt: new Date().toISOString(),
    device: {
      ...fixed.device,
      osVersion,
      uptimeSeconds: uptime(),
    },
    cpu: { ...fixed.cpu, ...live.cpu },
    gpu: fixed.gpu,
    memory: {
      totalBytes,
      usedBytes,
      freeBytes,
      ...parseVmStat(vmStat, totalBytes),
      ...parseSwap(swap),
    },
    storage: {
      ...fixed.storage,
      freeBytes: storageFreeBytes,
      usedBytes: Math.max(0, fixed.storage.totalBytes - storageFreeBytes),
    },
    battery: parseBattery(battery, fixed.batteryHealth),
    network: {
      interface: interfaceName,
      ...networkSummary,
      ...networkRates(sample),
    },
    fans: parseFans(fans),
    processes: live.processes,
  }
}

export function clearSystemStatsCacheForTests(): void {
  staticStatsPromise = null
  previousNetworkSample = null
}
