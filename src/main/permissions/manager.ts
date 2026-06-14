import { shell, systemPreferences } from 'electron'
import type {
  PermissionDescriptor,
  PermissionId,
  PermissionState,
  PermissionStatus,
  PermissionsSnapshot,
} from '../../shared/permissions'

/** Catalog of every native capability Tezbar may need. The renderer shows
 *  these in the Permissions view; the main process owns detection and the
 *  guided remediation links. */
const DESCRIPTORS: Record<PermissionId, PermissionDescriptor> = {
  accessibility: {
    id: 'accessibility',
    title: 'Accessibility',
    summary: 'Synthesize keystrokes, control windows, automate UI.',
    rationale:
      'Needed to automate the active app: move windows, click through menus, send keystrokes to focused controls.',
    settingsUrl: 'x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility',
    remediation:
      'Open System Settings → Privacy & Security → Accessibility and enable Tezbar.',
  },
  automation: {
    id: 'automation',
    title: 'Automation (Apple Events)',
    summary: 'Talk to other apps via AppleScript / Apple Events.',
    rationale:
      'Required for AppleScript-based commands (toggle dark mode, empty trash, control Music/Finder).',
    settingsUrl: 'x-apple.systempreferences:com.apple.preference.security?Privacy_Automation',
    remediation:
      'Open System Settings → Privacy & Security → Automation and allow Tezbar to control the target app.',
  },
  'input-monitoring': {
    id: 'input-monitoring',
    title: 'Input Monitoring',
    summary: 'Observe keyboard and mouse events globally.',
    rationale:
      'Used for global hotkeys and key-capture flows (e.g. global Alt+Space, keystroke recording).',
    settingsUrl:
      'x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent',
    remediation:
      'Open System Settings → Privacy & Security → Input Monitoring and enable Tezbar.',
  },
  microphone: {
    id: 'microphone',
    title: 'Microphone',
    summary: 'Capture audio for voice commands.',
    rationale: 'Voice-activated commands and transcription features require microphone access.',
    settingsUrl: 'x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone',
    remediation:
      'Open System Settings → Privacy & Security → Microphone and enable Tezbar.',
  },
  calendar: {
    id: 'calendar',
    title: 'Calendar',
    summary: 'Read and create events for calendar-aware commands.',
    rationale:
      'Calendar-related extensions and the built-in "next meeting" command need access to your Calendar database.',
    settingsUrl: 'x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars',
    remediation:
      'Open System Settings → Privacy & Security → Calendars and enable Tezbar.',
  },
  'screen-recording': {
    id: 'screen-recording',
    title: 'Screen Recording',
    summary: 'Capture screen content for screenshots and window vision.',
    rationale:
      'Needed for screenshot-based flows, window snapshots, and visual automation helpers.',
    settingsUrl: 'x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture',
    remediation:
      'Open System Settings → Privacy & Security → Screen Recording and enable Tezbar.',
  },
}

function mapMediaStatus(value: string): PermissionState {
  switch (value) {
    case 'granted':
      return 'granted'
    case 'denied':
      return 'denied'
    case 'restricted':
      return 'restricted'
    case 'not-determined':
    case 'unknown':
      return 'not-determined'
    default:
      return 'not-determined'
  }
}

function probeAccessibility(promptIfNeeded = false): PermissionState {
  if (process.platform !== 'darwin') return 'unsupported'
  try {
    return systemPreferences.isTrustedAccessibilityClient(promptIfNeeded) ? 'granted' : 'denied'
  } catch {
    return 'not-determined'
  }
}

/** Electron's TS types only list the three kinds it always had, but at
 *  runtime on macOS `getMediaAccessStatus` also accepts 'calendar',
 *  'reminders', 'contacts', and 'screen'. Cast at the boundary and keep
 *  the non-standard kinds in one place so it's obvious we're stepping
 *  outside the published type surface on purpose. */
type ExtendedMediaType = 'microphone' | 'camera' | 'calendar' | 'screen'
function getStatus(type: ExtendedMediaType): PermissionState {
  if (process.platform !== 'darwin') return 'unsupported'
  try {
    return mapMediaStatus(
      systemPreferences.getMediaAccessStatus(type as 'microphone' | 'camera'),
    )
  } catch {
    return 'not-determined'
  }
}

function probePermission(id: PermissionId): PermissionState {
  switch (id) {
    case 'accessibility':
      return probeAccessibility(false)
    case 'microphone':
      return getStatus('microphone')
    case 'calendar':
      return getStatus('calendar')
    case 'screen-recording':
      return getStatus('screen')
    case 'automation':
    case 'input-monitoring':
      // macOS does not expose a direct probe for these — the only reliable
      // signal is observing a failed attempt. Report not-determined so the
      // UI nudges the user to check the settings page manually.
      return process.platform === 'darwin' ? 'not-determined' : 'unsupported'
    default:
      return 'unsupported'
  }
}

export function snapshotPermissions(): PermissionsSnapshot {
  const statuses: PermissionStatus[] = (Object.keys(DESCRIPTORS) as PermissionId[]).map((id) => ({
    descriptor: DESCRIPTORS[id],
    state: probePermission(id),
    checkedAt: Date.now(),
  }))

  return {
    platform: process.platform,
    statuses,
  }
}

export async function requestPermission(id: PermissionId): Promise<PermissionStatus> {
  const descriptor = DESCRIPTORS[id]
  if (!descriptor) {
    throw new Error(`Unknown permission: ${id}`)
  }

  if (process.platform !== 'darwin') {
    return { descriptor, state: 'unsupported', checkedAt: Date.now() }
  }

  try {
    if (id === 'accessibility') {
      systemPreferences.isTrustedAccessibilityClient(true)
    } else if (id === 'microphone') {
      await systemPreferences.askForMediaAccess('microphone')
    } else if (descriptor.settingsUrl) {
      // All remaining categories require the user to flip the toggle in
      // System Settings; we deep-link them to the right pane.
      await shell.openExternal(descriptor.settingsUrl)
    }
  } catch {
    // Swallow: return a fresh probe regardless of how the request went.
  }

  return { descriptor, state: probePermission(id), checkedAt: Date.now() }
}
