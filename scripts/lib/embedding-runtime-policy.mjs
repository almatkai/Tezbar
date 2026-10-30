// A signed outer app is not evidence that executable bytes inside its embedded
// runtime ZIP were distribution-signed. Fail before touching signing secrets.
export function assertEmbeddingRuntimeDistribution({ platform, arch, unsigned, catalog }) {
  if (platform !== 'darwin' || arch !== 'arm64' || unsigned) return
  if (catalog.distribution !== 'developer-id') {
    throw new Error(
      'Embedded runtime is local-ad-hoc only; stage/review Developer ID runtime pins before a signed distribution build'
    )
  }
}
