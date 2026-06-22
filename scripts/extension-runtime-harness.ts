import { readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import {
  disposeExtensionSession,
  invokeExtensionAction,
  loadMoreExtensionSession,
  refreshExtensionSession,
  runExtensionCommandFromPackageJson,
  updateSearchText,
} from '../src/main/extension-runner'
import type { ExtensionRunCommandResult } from '../src/shared/extensionRuntime'

type OperationResult = {
  operation: string
  result: unknown
}

function readOption(args: string[], name: string): string | undefined {
  const index = args.indexOf(name)
  return index >= 0 ? args[index + 1] : undefined
}

function dependencyVersions(
  packageJsonPath: string
): Record<string, { requested: string; resolved?: string }> {
  const pkg = JSON.parse(readFileSync(packageJsonPath, 'utf8')) as {
    dependencies?: Record<string, string>
    optionalDependencies?: Record<string, string>
  }
  const dependencies = { ...pkg.dependencies, ...pkg.optionalDependencies }
  const report: Record<string, { requested: string; resolved?: string }> = {}
  for (const [name, requested] of Object.entries(dependencies)) {
    if (name.startsWith('@raycast/')) continue
    try {
      const installed = JSON.parse(
        readFileSync(join(dirname(packageJsonPath), 'node_modules', name, 'package.json'), 'utf8')
      ) as { version?: string }
      report[name] = { requested, resolved: installed.version }
    } catch {
      report[name] = { requested }
    }
  }
  return report
}

async function main(): Promise<void> {
  const [, , packageJsonArg, commandName, ...args] = process.argv
  if (!packageJsonArg || !commandName) {
    throw new Error('Usage: extension-runtime-harness <package.json> <command-name>')
  }

  const packageJsonPath = resolve(packageJsonArg)
  const preferencesRaw = readOption(args, '--preferences')
  const preferences = preferencesRaw
    ? (JSON.parse(preferencesRaw) as Record<string, unknown>)
    : undefined
  const argumentsRaw = readOption(args, '--arguments')
  const argumentValues = argumentsRaw
    ? (JSON.parse(argumentsRaw) as Record<string, string>)
    : undefined
  const dependencies = dependencyVersions(packageJsonPath)
  const initial = await runExtensionCommandFromPackageJson(
    packageJsonPath,
    commandName,
    argumentValues,
    preferences,
    { effectMode: 'record' }
  )
  if (args.length === 0) {
    process.stdout.write(`${JSON.stringify(initial)}\n`)
    if (!initial.ok) process.exitCode = 1
    return
  }

  if (!initial.ok || initial.mode !== 'view') {
    process.stdout.write(
      `${JSON.stringify({ dependencies, initial, operations: [], final: initial })}\n`
    )
    if (!initial.ok) process.exitCode = 1
    return
  }

  const operations: OperationResult[] = []
  let current: ExtensionRunCommandResult = initial
  const record = (operation: string, result: unknown): void => {
    operations.push({ operation, result })
    if (
      result &&
      typeof result === 'object' &&
      'mode' in result &&
      (result.mode === 'view' || result.mode === 'no-view')
    ) {
      current = result as ExtensionRunCommandResult
    }
  }

  if (args.includes('--load-more')) {
    record('load-more', await loadMoreExtensionSession({ sessionId: initial.sessionId }))
  }

  const searchText = readOption(args, '--search')
  if (searchText !== undefined) {
    record('search', await updateSearchText({ sessionId: initial.sessionId, searchText }))
  }

  if (args.includes('--refresh')) {
    const waitMs = Number.parseInt(readOption(args, '--wait-ms') ?? '25', 10)
    const refreshCount = Math.max(
      1,
      Number.parseInt(readOption(args, '--refresh-count') ?? '1', 10)
    )
    for (let index = 0; index < refreshCount; index += 1) {
      await new Promise((resolveDelay) => setTimeout(resolveDelay, Math.max(0, waitMs)))
      record(
        `refresh:${index + 1}`,
        await refreshExtensionSession({ sessionId: initial.sessionId })
      )
    }
  }

  const actionIndexRaw = readOption(args, '--invoke-action-index')
  if (actionIndexRaw !== undefined) {
    const actionIndex = Number.parseInt(actionIndexRaw, 10)
    const action = current.mode === 'view' ? current.actions[actionIndex] : undefined
    if (!action) throw new Error(`No initial action at index ${actionIndexRaw}`)
    const formValuesRaw = readOption(args, '--form-values')
    const formValues = formValuesRaw
      ? (JSON.parse(formValuesRaw) as Record<string, unknown>)
      : undefined
    record(
      `action:${actionIndex}`,
      await invokeExtensionAction({
        sessionId: initial.sessionId,
        actionId: action.id,
        formValues,
      })
    )
  }

  disposeExtensionSession(initial.sessionId)
  process.stdout.write(`${JSON.stringify({ dependencies, initial, operations, final: current })}\n`)
}

void main().catch((error: unknown) => {
  process.stderr.write(
    `${error instanceof Error ? (error.stack ?? error.message) : String(error)}\n`
  )
  process.exitCode = 1
})
