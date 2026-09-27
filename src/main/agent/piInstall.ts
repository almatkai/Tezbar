/** Detects and installs the `pi` CLI (pi-coding-agent) so onboarding can offer
 *  a real one-click install instead of asking the user to run a command
 *  themselves. `pi` ships on npm as `@mariozechner/pi-coding-agent`; there is
 *  no Homebrew formula, so we install through whichever of npm/pnpm exists.
 *  Detection goes through `resolvePiBinary` — the exact lookup the agent bridge
 *  uses to launch Pi — so "installed" here means the agent can actually run it. */

import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
import { resolvePiBinary } from './bridge'

const execFileAsync = promisify(execFile)

const PI_NPM_PACKAGE = '@mariozechner/pi-coding-agent'
const PI_NPM_PAGE_URL = `https://www.npmjs.com/package/${PI_NPM_PACKAGE}`

export type PiAvailability = {
  installed: boolean
  version?: string
}

export type PiInstallResult =
  | { ok: true; method: 'npm' | 'pnpm'; version?: string }
  | { ok: false; reason: 'no-package-manager'; officialUrl: string }
  | { ok: false; reason: 'install-failed'; officialUrl: string; error: string }

async function commandWorks(bin: string, args: string[]): Promise<boolean> {
  try {
    await execFileAsync(bin, args, { timeout: 8000 })
    return true
  } catch {
    return false
  }
}

export async function checkPiAgent(): Promise<PiAvailability> {
  try {
    const { stdout } = await execFileAsync(resolvePiBinary(), ['--version'], { timeout: 8000 })
    return { installed: true, version: stdout.trim() || undefined }
  } catch {
    return { installed: false }
  }
}

async function installWith(
  method: 'npm' | 'pnpm',
  args: string[]
): Promise<PiInstallResult> {
  try {
    await execFileAsync(method, args, { timeout: 180_000 })
  } catch (error) {
    return {
      ok: false,
      reason: 'install-failed',
      officialUrl: PI_NPM_PAGE_URL,
      error: error instanceof Error ? error.message : String(error),
    }
  }
  // A global install can land in a bin directory that isn't on our PATH;
  // only report success when the agent bridge can really launch it.
  const availability = await checkPiAgent()
  if (!availability.installed) {
    return {
      ok: false,
      reason: 'install-failed',
      officialUrl: PI_NPM_PAGE_URL,
      error: `${method} finished, but Tezbar can't find the pi command on your PATH.`,
    }
  }
  return { ok: true, method, version: availability.version }
}

export async function installPiAgent(): Promise<PiInstallResult> {
  if (await commandWorks('npm', ['--version'])) {
    return installWith('npm', ['install', '-g', PI_NPM_PACKAGE])
  }
  if (await commandWorks('pnpm', ['--version'])) {
    return installWith('pnpm', ['add', '-g', PI_NPM_PACKAGE])
  }
  return { ok: false, reason: 'no-package-manager', officialUrl: PI_NPM_PAGE_URL }
}
