import type {
  ExtensionManifest,
  ExtensionPlatform,
  ExtensionRuntimeKind,
  InstalledExtension,
  NativeV2Command,
  NativeV2Verification,
} from '../shared/extensions'
import type { ExtensionStorePage } from '../shared/desktop-api'

export function searchStorePage(
  query: string,
  isUrlQuery: boolean,
  offset: number,
  limit: number,
  search: (query: string, options: { offset: number; limit: number }) => Promise<ExtensionStorePage>,
): Promise<ExtensionStorePage> {
  if (isUrlQuery) return Promise.resolve({ items: [], total: 0, offset, limit, hasMore: false })
  return search(query, { offset, limit })
}

export type RuntimeDescription = {
  kind: ExtensionRuntimeKind
  rustCount: number
  total: number
  short: string
  detail: string
}

export function effectiveRuntime(
  ext: ExtensionManifest,
  platform: ExtensionPlatform,
): ExtensionRuntimeKind | null {
  const native = ext.nativeV2
  if (ext.runtime === 'javascript') return 'javascript'
  if (!native) return null
  const total = native.totalCommands
  if (total <= 0 || native.commands.length !== total) return null
  const rustCount = native.commands.filter(
    (command) => command.runtime === 'rust' && command.platforms.includes(platform),
  ).length
  if (rustCount === total) return 'rust'
  if (rustCount > 0) return 'mixed'
  return 'javascript'
}

export function describeRuntime(
  ext: ExtensionManifest,
  platform: ExtensionPlatform,
): RuntimeDescription | null {
  const kind = effectiveRuntime(ext, platform)
  if (!kind) return null
  const native = ext.nativeV2
  const total = native?.totalCommands ?? 0
  const rustCount = native?.commands.filter(
    (command) => command.runtime === 'rust' && command.platforms.includes(platform),
  ).length ?? 0
  if (kind === 'rust') {
    return { kind, rustCount, total, short: 'Rust', detail: `Rust — all ${total} commands run natively` }
  }
  if (kind === 'mixed') {
    const jsCount = native?.commands.filter(
      (command) => command.runtime === 'javascript' && command.platforms.includes(platform),
    ).length ?? 0
    const unavailable = total - rustCount - jsCount
    const parts = [
      jsCount ? `${jsCount} in JavaScript` : '',
      unavailable ? `${unavailable} not available on ${platform}` : '',
    ].filter(Boolean)
    return {
      kind,
      rustCount,
      total,
      short: `Rust ${rustCount} of ${total}`,
      detail: `Mixed — ${rustCount} of ${total} commands run in Rust, ${parts.join(', ')}`,
    }
  }
  const runnableJs = native?.commands.some(
    (command) => command.runtime === 'javascript' && command.platforms.includes(platform),
  ) ?? false
  return {
    kind,
    rustCount,
    total,
    short: native && ext.runtime !== 'javascript' ? `Rust ${rustCount} of ${total}` : 'JavaScript',
    detail: native && !runnableJs
      ? `No commands available on ${platform}`
      : 'JavaScript — runs in the compatibility worker',
  }
}

export function supportedPlatforms(ext: ExtensionManifest): ExtensionPlatform[] {
  const nativePlatforms = ext.nativeV2?.commands.flatMap((command) => command.platforms) ?? []
  const platforms = nativePlatforms.length ? nativePlatforms : ext.platforms ?? []
  return (['macOS', 'Windows'] as const).filter((platform) => platforms.includes(platform))
}

export function verificationForPlatform(
  ext: ExtensionManifest,
  platform: ExtensionPlatform,
): NativeV2Verification | null {
  return ext.nativeV2?.verification
    .filter((entry) => entry.platform === platform && entry.state !== 'not-run' && !!entry.checkedAt)
    .sort((a, b) => (b.checkedAt ?? 0) - (a.checkedAt ?? 0))[0] ?? null
}

export function checkedLines(
  ext: ExtensionManifest,
  currentPlatform: ExtensionPlatform,
  formatDate: (epochMs: number) => string = (epochMs) =>
    new Intl.DateTimeFormat('en-US', { dateStyle: 'medium' }).format(epochMs),
): string[] {
  const platforms = supportedPlatforms(ext).sort(
    (a, b) => Number(b === currentPlatform) - Number(a === currentPlatform),
  )
  return platforms.map((platform) => {
    const check = verificationForPlatform(ext, platform)
    if (!check) return `${platform} — not checked yet`
    const date = formatDate(check.checkedAt!)
    if (check.state === 'failed') {
      return `${platform} — failed ${date}${check.note ? `: ${check.note}` : ''}`
    }
    return `${platform} — passed ${date}${check.method ? ` (${check.method})` : ''}`
  })
}

export function rowSubtitle(
  ext: ExtensionManifest,
  platform: ExtensionPlatform,
  primary: string,
): string {
  const runtime = describeRuntime(ext, platform)
  const failed = verificationForPlatform(ext, platform)?.state === 'failed'
  return [primary, runtime?.short, failed ? 'check failed' : ''].filter(Boolean).join(' · ')
}

export function nativeV2Lines(ext: ExtensionManifest): string[] {
  const native = ext.nativeV2
  if (!native) return []
  const lines = [`${native.id} · v${native.version}`]
  if (native.replaces.length) {
    lines.push(replacementReason(ext) ?? `Replaces ${native.replaces.join(', ')}`)
  }
  if (native.status === 'partial') {
    const unavailable = native.commands.filter((command) => command.runtime === 'unavailable').length
    const javascript = native.commands.filter((command) => command.runtime === 'javascript').length
    const parts = [
      unavailable ? `${unavailable} command${unavailable === 1 ? '' : 's'} not available` : '',
      javascript ? `${javascript} command${javascript === 1 ? '' : 's'} remain in JavaScript` : '',
    ].filter(Boolean)
    lines.push(`Partial port — ${parts.join(', ') || 'command coverage incomplete'}`)
  }
  return lines
}

export function commandStatus(command: NativeV2Command, platform: ExtensionPlatform): string {
  if (command.runtime === 'unavailable' || !command.platforms.includes(platform)) return `Not on ${platform}`
  return command.runtime === 'rust' ? 'Rust' : 'JavaScript'
}

export function visibleCommands(ext: ExtensionManifest): Array<{
  name?: string
  title?: string
  description?: string
  native?: NativeV2Command
}> {
  const commands = (ext.commands ?? []).filter((command) => command.title || command.name)
  const native = ext.nativeV2?.commands ?? []
  const byName = new Map(native.map((command) => [command.name, command]))
  const merged = commands.map((command) => ({ ...command, native: byName.get(command.name ?? '') }))
  const existing = new Set(commands.map((command) => command.name))
  for (const command of native) {
    if (!existing.has(command.name)) merged.push({ name: command.name, title: command.title, native: command })
  }
  return merged
}

export function replacementReason(ext: ExtensionManifest): string | null {
  if (ext.id !== ext.nativeV2?.id) return null
  return ext.nativeV2.replaces.length
    ? `Tezbar replacement for ${ext.nativeV2.replaces.join(', ')}`
    : null
}

export function canInstallExtension(ext: ExtensionManifest, platform: ExtensionPlatform): boolean {
  if (ext.id !== ext.nativeV2?.id) return true
  return ext.nativeV2.status !== 'unavailable' && ext.nativeV2.commands.some(
    (command) => command.runtime !== 'unavailable' && command.platforms.includes(platform),
  )
}

export function installTarget(ext: ExtensionManifest): string {
  return ext.id === ext.nativeV2?.id ? ext.nativeV2.id : ext.id
}

export function mergeCatalog(
  store: ExtensionManifest[],
  installed: InstalledExtension[],
  _platform: ExtensionPlatform,
  query = '',
): ExtensionManifest[] {
  const installedById = new Map(installed.map((extension) => [extension.id, extension]))
  const rows = store.map((extension) => {
    const local = installedById.get(extension.id)
    return local ? {
      ...extension,
      runtime: local.runtime ?? extension.runtime,
      nativeV2: local.runtime === 'javascript' && !local.nativeV2
        ? undefined
        : local.nativeV2 ?? extension.nativeV2,
      platforms: local.platforms ?? extension.platforms,
      commands: local.commands?.length ? local.commands : extension.commands,
    } : extension
  })
  const seen = new Set(rows.map((extension) => extension.id))
  const needle = query.trim().toLocaleLowerCase()
  for (const extension of installed) {
    if (!seen.has(extension.id) && (!needle || [extension.id, extension.name, extension.description, extension.author]
      .some((value) => value?.toLocaleLowerCase().includes(needle)))) {
      rows.push(extension)
      seen.add(extension.id)
    }
  }
  return rows
}
