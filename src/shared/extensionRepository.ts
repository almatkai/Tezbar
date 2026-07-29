export type GitHubRepositoryReference = {
  owner: string
  repository: string
  url: string
}

/**
 * Accept only public GitHub repository root URLs. Keeping this parser strict
 * prevents arbitrary URLs from reaching the extension download pipeline.
 */
export function parseGitHubRepositoryUrl(value: string): GitHubRepositoryReference | null {
  const raw = String(value || '').trim()
  if (!raw) return null

  let parsed: URL
  try {
    parsed = new URL(raw)
  } catch {
    return null
  }

  if (parsed.protocol !== 'https:' || parsed.username || parsed.password || parsed.port) return null
  const hostname = parsed.hostname.toLowerCase()
  if (hostname !== 'github.com' && hostname !== 'www.github.com') return null
  if (parsed.search || parsed.hash) return null

  const segments = parsed.pathname.split('/').filter(Boolean)
  if (segments.length !== 2) return null

  const owner = segments[0] || ''
  const repository = (segments[1] || '').replace(/\.git$/i, '')
  if (!/^[a-z0-9](?:[a-z0-9-]{0,37}[a-z0-9])?$/i.test(owner)) return null
  if (!/^[a-z0-9._-]+$/i.test(repository)) return null

  return {
    owner,
    repository,
    url: `https://github.com/${owner}/${repository}`,
  }
}
