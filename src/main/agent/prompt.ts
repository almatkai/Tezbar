import type { AgentInputImage } from '../../shared/agent'

const MAX_AGENT_IMAGES = 4
const MAX_AGENT_IMAGE_BYTES = 8 * 1024 * 1024
const SUPPORTED_IMAGE_TYPES = new Set<AgentInputImage['mimeType']>([
  'image/png',
  'image/jpeg',
  'image/webp',
])

function rawBase64(data: string): string {
  const trimmed = data.trim()
  const comma = trimmed.indexOf(',')
  return trimmed.startsWith('data:image/') && comma >= 0 ? trimmed.slice(comma + 1) : trimmed
}

function estimatedDecodedBytes(data: string): number {
  const padding = data.endsWith('==') ? 2 : data.endsWith('=') ? 1 : 0
  return Math.max(0, Math.floor((data.length * 3) / 4) - padding)
}

export function normalizeAgentImages(
  images: readonly AgentInputImage[] | undefined
): AgentInputImage[] {
  if (!images?.length) return []
  if (images.length > MAX_AGENT_IMAGES) {
    throw new Error(`Agent accepts at most ${MAX_AGENT_IMAGES} images per prompt`)
  }

  return images.map((image) => {
    if (!SUPPORTED_IMAGE_TYPES.has(image.mimeType)) {
      throw new Error(`Unsupported agent image type: ${image.mimeType}`)
    }
    const data = rawBase64(image.data)
    if (!data || !/^[A-Za-z0-9+/]+={0,2}$/.test(data)) {
      throw new Error('Agent image is not valid base64 data')
    }
    if (estimatedDecodedBytes(data) > MAX_AGENT_IMAGE_BYTES) {
      throw new Error('Agent image exceeds the 8 MB limit')
    }
    return { type: 'image', data, mimeType: image.mimeType }
  })
}

export function buildPromptCommand(
  message: string,
  images: readonly AgentInputImage[] | undefined
): { type: 'prompt'; message: string; images?: AgentInputImage[] } {
  const normalized = normalizeAgentImages(images)
  return normalized.length > 0
    ? { type: 'prompt', message, images: normalized }
    : { type: 'prompt', message }
}
