import { describe, expect, it } from 'vitest'

import { parseGitHubRepositoryUrl } from './extensionRepository'

describe('parseGitHubRepositoryUrl', () => {
  it('normalizes a public repository URL', () => {
    expect(
      parseGitHubRepositoryUrl(' https://github.com/almatkai/tezbar-color-picker-extension.git/ ')
    ).toEqual({
      owner: 'almatkai',
      repository: 'tezbar-color-picker-extension',
      url: 'https://github.com/almatkai/tezbar-color-picker-extension',
    })
  })

  it('rejects non-GitHub and nested GitHub URLs', () => {
    expect(parseGitHubRepositoryUrl('https://example.com/owner/repo')).toBeNull()
    expect(parseGitHubRepositoryUrl('https://github.com/owner/repo/tree/main')).toBeNull()
  })

  it('rejects insecure and credentialed URLs', () => {
    expect(parseGitHubRepositoryUrl('http://github.com/owner/repo')).toBeNull()
    expect(parseGitHubRepositoryUrl('https://token@github.com/owner/repo')).toBeNull()
  })
})
