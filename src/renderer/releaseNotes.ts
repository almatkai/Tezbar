import { version as packageVersion } from '../../package.json'

export const CURRENT_APP_VERSION = import.meta.env.VITE_APP_VERSION ?? packageVersion
export const LAST_RELEASE_NOTES_VERSION_KEY = 'tezbar:release-notes-version'

export type ReleaseNotes = { version: string; body: string; publishedAt: string | null }

export function shouldShowPostUpdateNotes(
  storage: Pick<Storage, 'getItem' | 'setItem'>,
  firstInstall: boolean,
  version: string = CURRENT_APP_VERSION
): boolean {
  if (firstInstall) {
    storage.setItem(LAST_RELEASE_NOTES_VERSION_KEY, version)
    return false
  }
  return storage.getItem(LAST_RELEASE_NOTES_VERSION_KEY) !== version
}

export async function fetchReleaseNotes(
  version: string,
  signal: AbortSignal
): Promise<ReleaseNotes> {
  if (version === CURRENT_APP_VERSION) {
    const body: unknown = import.meta.env.VITE_APP_RELEASE_NOTES
    if (typeof body !== 'string' || !body.trim()) {
      throw new Error('This build is missing its Tezbar release notes.')
    }
    return { version, body, publishedAt: null }
  }

  const response = await fetch(
    `https://api.github.com/repos/almatkai/Tezbar/releases/tags/v${encodeURIComponent(version)}`,
    { signal, headers: { Accept: 'application/vnd.github+json' } }
  )
  if (!response.ok) throw new Error('Release notes are unavailable right now.')
  const release = (await response.json()) as {
    tag_name?: string
    body?: string
    published_at?: string
  }
  if (release.tag_name !== `v${version}`) throw new Error('Release version does not match.')
  return {
    version,
    body: typeof release.body === 'string' ? release.body : '',
    publishedAt: release.published_at ?? null,
  }
}
