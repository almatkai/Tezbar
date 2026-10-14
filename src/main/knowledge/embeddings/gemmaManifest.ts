// Pinned after llama.cpp's EmbeddingGemma 2 support landed on October 6, 2026.
// Downloads are verified before any executable is launched.
export const GEMMA_MODEL = {
  id: 'google/embeddinggemma-2',
  revision: 'bfcd298762cc34d0357ece5ebdd31791a3a374d8',
  dimensions: 256,
  file: 'embeddinggemma-2-Q8_0.gguf',
  bytes: 309855456,
  sha256: '2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135',
} as const

// Includes quantization, output dimensions, and prompt/preprocessing version.
export const GEMMA_INDEX_KEY = `${GEMMA_MODEL.id}:${GEMMA_MODEL.revision}:q8:256:search-v1`
export const GEMMA_MODEL_URL = `https://huggingface.co/ggml-org/embeddinggemma-2-GGUF/resolve/${GEMMA_MODEL.revision}/${GEMMA_MODEL.file}`

const RUNTIMES: Record<string, { file: string; bytes: number; sha256: string }> = {
  'win32-x64': {
    file: 'llama-b11474-bin-win-cpu-x64.zip',
    bytes: 19440991,
    sha256: '5a39102a1b27d75ab8a9d777c6cf2ce373c468265dc1797a2566dec085501a95',
  },
  'win32-arm64': {
    file: 'llama-b11474-bin-win-cpu-arm64.zip',
    bytes: 12269479,
    sha256: '2874f37ef48f12edfab2346f3857d7c866f4d423b2e9da045baf14312201db71',
  },
  'darwin-arm64': {
    file: 'llama-b11474-bin-macos-arm64.tar.gz',
    bytes: 12011955,
    sha256: 'ae7f002f2b477d14fea8ac4d93438684344cd18ec10d2b0ee9d5518b2a74def5',
  },
  'darwin-x64': {
    file: 'llama-b11474-bin-macos-x64.tar.gz',
    bytes: 11530937,
    sha256: '46381da99834f84ead41c78d091c9137755bac27d9423e56ea1fd0bc51104d60',
  },
  'linux-x64': {
    file: 'llama-b11474-bin-ubuntu-x64.tar.gz',
    bytes: 17737147,
    sha256: '98abe0fac169db55712ce96b0e30adf99fa47fa5386fddef8fef84fe21f4a690',
  },
}

export function gemmaRuntime(platform = process.platform, arch = process.arch) {
  const runtime = RUNTIMES[`${platform}-${arch}`]
  return runtime
    ? {
        ...runtime,
        url: `https://github.com/ggml-org/llama.cpp/releases/download/b11474/${runtime.file}`,
      }
    : undefined
}

export function normalizeGemmaVector(value: unknown): number[] {
  if (
    !Array.isArray(value) ||
    value.length !== 768 ||
    value.some((component) => typeof component !== 'number' || !Number.isFinite(component))
  ) {
    throw new Error('EmbeddingGemma returned an invalid vector')
  }
  const truncated = value.slice(0, GEMMA_MODEL.dimensions) as number[]
  const magnitude = Math.sqrt(truncated.reduce((sum, component) => sum + component * component, 0))
  if (magnitude === 0) throw new Error('EmbeddingGemma returned an empty vector')
  return truncated.map((component) => component / magnitude)
}

export function gemmaDocument(text: string, title?: string): string {
  return `title: ${title?.trim() || 'none'} | text: ${text}`
}

export function gemmaQuery(text: string): string {
  return `task: search result | query: ${text}`
}
