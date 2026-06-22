import { execFile } from 'node:child_process'
import { existsSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { promisify } from 'node:util'
import { app } from 'electron'

import type { AgentInputImage } from '../../shared/agent'

const execFileAsync = promisify(execFile)
const MAX_OCR_CHARS = 40_000

type ScreenOcrResponse = {
  ok?: boolean
  value?: string
  error?: string
}

function screenOcrHelperPath(): string | undefined {
  const resourcesPath = (process as NodeJS.Process & { resourcesPath?: string }).resourcesPath
  const candidates = [
    process.env['SCREENOCR_HELPER_PATH'],
    app.isPackaged && resourcesPath
      ? path.join(resourcesPath, 'app.asar.unpacked', 'native', 'screenocr', 'screenocr-helper')
      : undefined,
    path.join(process.cwd(), 'native', 'screenocr', 'screenocr-helper'),
  ]
  return candidates.find((candidate): candidate is string => Boolean(candidate && existsSync(candidate)))
}

function imageExtension(mimeType: AgentInputImage['mimeType']): string {
  if (mimeType === 'image/jpeg') return 'jpg'
  if (mimeType === 'image/webp') return 'webp'
  return 'png'
}

export async function extractTextFromAgentImages(
  images: readonly AgentInputImage[]
): Promise<string> {
  if (process.platform !== 'darwin' || images.length === 0) return ''
  const helperPath = screenOcrHelperPath()
  if (!helperPath) return ''

  const workDir = await mkdtemp(path.join(tmpdir(), 'raymes-agent-image-'))
  try {
    const textBlocks: string[] = []
    for (let index = 0; index < images.length; index += 1) {
      const image = images[index]
      if (!image) continue
      const imagePath = path.join(workDir, `attachment-${index}.${imageExtension(image.mimeType)}`)
      await writeFile(imagePath, Buffer.from(image.data, 'base64'))
      const { stdout } = await execFileAsync(
        helperPath,
        [
          'recognize-text',
          JSON.stringify({
            imagePath,
            fast: false,
            languageCorrection: true,
            ignoreLineBreaks: false,
          }),
        ],
        { timeout: 45_000, maxBuffer: 4 * 1024 * 1024 }
      )
      const response = JSON.parse(stdout.trim()) as ScreenOcrResponse
      if (!response.ok) throw new Error(response.error || 'Local screen text extraction failed')
      if (response.value?.trim()) textBlocks.push(response.value.trim())
    }
    return textBlocks.join('\n\n').slice(0, MAX_OCR_CHARS)
  } finally {
    await rm(workDir, { recursive: true, force: true })
  }
}
