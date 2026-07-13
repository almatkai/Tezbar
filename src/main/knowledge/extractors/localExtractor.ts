import { execFile } from 'node:child_process'
import { existsSync } from 'node:fs'
import { open } from 'node:fs/promises'
import { extname, join } from 'node:path'
import { promisify } from 'node:util'
import type {
  IndexedImage,
  IndexedPage,
  IndexingCapability,
} from '../../../shared/knowledge'

const execFileAsync = promisify(execFile)
const MAX_EXTRACTED_BYTES = 64 * 1024 * 1024
const MAX_PLAIN_TEXT_EXTRACTED_BYTES = 8 * 1024 * 1024
const MAX_PLAIN_TEXT_SOURCE_BYTES = 64 * 1024 * 1024
const MAX_IMAGE_SOURCE_BYTES = 75 * 1024 * 1024
const MAX_DOCUMENT_SOURCE_BYTES = 150 * 1024 * 1024

const PLAIN_TEXT_EXTENSIONS = new Set([
  '.txt', '.text', '.md', '.mdx', '.rst', '.org', '.csv', '.tsv', '.log', '.json',
  '.jsonl', '.yaml', '.yml', '.toml', '.xml', '.html', '.htm', '.css', '.scss', '.less',
  '.js', '.jsx', '.mjs', '.cjs', '.ts', '.tsx', '.py', '.rb', '.php', '.go', '.rs', '.swift',
  '.java', '.kt', '.kts', '.c', '.h', '.cc', '.cpp', '.hpp', '.cs', '.sh', '.zsh', '.bash',
  '.fish', '.sql', '.graphql', '.gql', '.env.example', '.ini', '.cfg', '.conf', '.properties',
  '.tex', '.bib', '.srt', '.vtt', '.ics', '.vcf', '.diff', '.patch', '.dockerfile', '.gitignore',
])

const IMAGE_EXTENSIONS = new Set([
  '.png', '.jpg', '.jpeg', '.heic', '.heif', '.tif', '.tiff', '.bmp', '.gif', '.webp',
])

const RICH_DOCUMENT_EXTENSIONS = new Set([
  '.rtf', '.rtfd', '.doc', '.docx', '.odt', '.pages', '.ppt', '.pptx', '.odp', '.key',
  '.xls', '.xlsx', '.ods', '.numbers', '.epub',
])

export const INDEXABLE_EXTENSIONS = new Set([
  ...PLAIN_TEXT_EXTENSIONS,
  ...IMAGE_EXTENSIONS,
  ...RICH_DOCUMENT_EXTENSIONS,
  '.pdf',
])

export type LocalExtraction = {
  pages: IndexedPage[]
  images: IndexedImage[]
  completedCapabilities: IndexingCapability[]
  warnings: string[]
  extractor: { id: string; version: string }
  totalPageCount?: number
}

export type LocalExtractionOptions = {
  maxPagesPerDocument: number | null
  enableOcr: boolean
  maxOcrPagesPerDocument: number | null
  ocrEveryPage: boolean
}

type HelperResponse<T> = { ok?: boolean; value?: T; error?: string }

function screenOcrHelperPath(): string {
  const candidates = [
    process.env.SCREENOCR_HELPER_PATH,
    join(process.cwd(), 'native', 'screenocr', 'screenocr-helper'),
  ]
  return candidates.find((candidate): candidate is string => Boolean(candidate && existsSync(candidate))) ?? ''
}

async function runScreenOcr<T>(command: string, input: Record<string, unknown>, signal: AbortSignal): Promise<T> {
  const helper = screenOcrHelperPath()
  if (!helper) throw new Error('The macOS text extraction helper is not available')
  const { stdout } = await execFileAsync(helper, [command, JSON.stringify(input)], {
    encoding: 'utf8',
    maxBuffer: MAX_EXTRACTED_BYTES,
    signal,
  })
  const parsed = JSON.parse(stdout.trim()) as HelperResponse<T>
  if (!parsed.ok) throw new Error(parsed.error || 'Native text extraction failed')
  return parsed.value as T
}

function decodeText(buffer: Buffer): string {
  if (buffer.length === 0) return ''
  const sample = buffer.subarray(0, Math.min(buffer.length, 8_192))
  let nulCount = 0
  for (const value of sample) if (value === 0) nulCount += 1
  if (nulCount / sample.length > 0.02) throw new Error('File appears to contain binary data')
  return buffer.toString('utf8').replace(/^\uFEFF/, '')
}

function stripMarkup(value: string): string {
  return value
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, ' ')
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, ' ')
    .replace(/<[^>]+>/g, ' ')
    .replace(/&nbsp;/gi, ' ')
    .replace(/&amp;/gi, '&')
    .replace(/&lt;/gi, '<')
    .replace(/&gt;/gi, '>')
    .replace(/&quot;/gi, '"')
    .replace(/&#39;/gi, "'")
    .replace(/[ \t]{2,}/g, ' ')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

async function extractPlainText(path: string): Promise<{ text: string; truncated: boolean }> {
  const handle = await open(path, 'r')
  try {
    const stat = await handle.stat()
    const bytesToRead = Math.min(stat.size, MAX_PLAIN_TEXT_EXTRACTED_BYTES)
    const buffer = Buffer.allocUnsafe(bytesToRead)
    const { bytesRead } = await handle.read(buffer, 0, bytesToRead, 0)
    const text = decodeText(buffer.subarray(0, bytesRead))
    return {
      text: /\.(?:html?|xml)$/i.test(path) ? stripMarkup(text) : text.trim(),
      truncated: stat.size > bytesRead,
    }
  } finally {
    await handle.close()
  }
}

async function extractRichDocument(path: string, signal: AbortSignal): Promise<string> {
  if (process.platform === 'darwin' && existsSync('/usr/bin/textutil')) {
    try {
      const { stdout } = await execFileAsync('/usr/bin/textutil', ['-convert', 'txt', '-stdout', path], {
        encoding: 'utf8',
        maxBuffer: MAX_EXTRACTED_BYTES,
        signal,
      })
      if (stdout.trim()) return stdout.trim()
    } catch {
      // Fall through to ZIP/XML extraction for modern document containers.
    }
  }

  if (existsSync('/usr/bin/unzip')) {
    const extension = extname(path).toLowerCase()
    const members = extension === '.epub'
      ? ['*.xhtml', '*.html']
      : extension === '.xlsx' || extension === '.ods' || extension === '.numbers'
        ? ['*.xml']
        : ['*.xml', '*.xhtml']
    const parts: string[] = []
    for (const member of members) {
      try {
        const { stdout } = await execFileAsync('/usr/bin/unzip', ['-p', path, member], {
          encoding: 'utf8',
          maxBuffer: MAX_EXTRACTED_BYTES,
          signal,
        })
        if (stdout.trim()) parts.push(stripMarkup(stdout))
      } catch {
        // Some containers omit a particular member family.
      }
    }
    if (parts.length > 0) return parts.join('\n\n').trim()
  }
  throw new Error('No local extractor is available for this document format')
}

async function extractMetadataText(path: string, signal: AbortSignal): Promise<string> {
  if (process.platform !== 'darwin') return ''
  try {
    const { stdout } = await execFileAsync('/usr/bin/mdls', ['-raw', '-name', 'kMDItemTextContent', path], {
      encoding: 'utf8',
      maxBuffer: MAX_EXTRACTED_BYTES,
      signal,
    })
    const trimmed = stdout.trim()
    return trimmed === '(null)' ? '' : trimmed
  } catch {
    return ''
  }
}

export function isIndexablePath(path: string): boolean {
  const extension = extname(path).toLowerCase()
  if (INDEXABLE_EXTENSIONS.has(extension)) return true
  const filename = path.split('/').pop()?.toLowerCase() ?? ''
  return ['dockerfile', 'makefile', 'license', 'readme', 'changelog'].includes(filename)
}

export function maximumIndexableSourceBytes(path: string): number {
  const extension = extname(path).toLowerCase()
  if (extension === '.pdf' || RICH_DOCUMENT_EXTENSIONS.has(extension)) {
    return MAX_DOCUMENT_SOURCE_BYTES
  }
  if (IMAGE_EXTENSIONS.has(extension)) return MAX_IMAGE_SOURCE_BYTES
  return MAX_PLAIN_TEXT_SOURCE_BYTES
}

export async function extractLocally(
  path: string,
  signal: AbortSignal,
  options: LocalExtractionOptions,
): Promise<LocalExtraction> {
  const extension = extname(path).toLowerCase()
  const warnings: string[] = []

  if (extension === '.pdf') {
    if (process.platform !== 'darwin') {
      const text = await extractMetadataText(path, signal)
      return {
        pages: [{ pageNumber: 1, extractedText: text }],
        images: [],
        completedCapabilities: ['extracted-text'],
        warnings: text ? [] : ['PDF extraction is not available on this platform yet.'],
        extractor: { id: 'metadata-pdf', version: '1.0.0' },
      }
    }
    const pdf = await runScreenOcr<{ pages: IndexedPage[]; totalPages: number }>('extract-pdf', {
      documentPath: path,
      maxPages: options.maxPagesPerDocument ?? 2_000,
      ocrScannedPages: options.enableOcr,
      maxOcrPages: options.maxOcrPagesPerDocument ?? 2_000,
      ocrEveryPage: options.ocrEveryPage,
      languageCorrection: true,
      languages: ['en-US', 'ru-RU'],
    }, signal)
    const pages = pdf.pages
    const usedOcr = pages.some((page) => Boolean(page.ocr?.text))
    return {
      pages,
      images: [],
      completedCapabilities: usedOcr ? ['extracted-text', 'ocr'] : ['extracted-text'],
      warnings,
      extractor: { id: 'macos-pdfkit-vision', version: '1.0.0' },
      totalPageCount: pdf.totalPages,
    }
  }

  if (IMAGE_EXTENSIONS.has(extension)) {
    if (!options.enableOcr) {
      return {
        pages: [], images: [], completedCapabilities: [],
        warnings: ['Image OCR is disabled at the selected Knowledge Depth.'],
        extractor: { id: 'metadata-only-image', version: '1.0.0' },
      }
    }
    if (process.platform !== 'darwin') {
      return {
        pages: [], images: [], completedCapabilities: [],
        warnings: ['Image OCR is not available on this platform yet.'],
        extractor: { id: 'unsupported-image', version: '1.0.0' },
      }
    }
    const ocr = await runScreenOcr<{
      text: string
      blocks: IndexedPage['ocr'] extends infer T ? T extends { blocks: infer B } ? B : never : never
      averageConfidence?: number
      width: number
      height: number
    }>('recognize-file', {
      imagePath: path,
      languageCorrection: true,
      languages: ['en-US', 'ru-RU'],
    }, signal)
    const image: IndexedImage = {
      id: `${path}:image:1`,
      pageNumber: 1,
      ocrText: ocr.text,
      width: ocr.width,
      height: ocr.height,
    }
    return {
      pages: [{
        pageNumber: 1,
        ocr: { text: ocr.text, blocks: ocr.blocks ?? [], averageConfidence: ocr.averageConfidence },
      }],
      images: [image],
      completedCapabilities: ['ocr'],
      warnings,
      extractor: { id: 'macos-vision-image', version: '1.0.0' },
      totalPageCount: 1,
    }
  }

  let text = ''
  if (PLAIN_TEXT_EXTENSIONS.has(extension) || !extension) {
    const extraction = await extractPlainText(path)
    text = extraction.text
    if (extraction.truncated) {
      warnings.push('Only the first 8 MB of this large text file was indexed.')
    }
  } else if (RICH_DOCUMENT_EXTENSIONS.has(extension)) {
    text = await extractRichDocument(path, signal)
  }
  if (!text) text = await extractMetadataText(path, signal)
  if (!text) warnings.push('No textual content could be extracted from this file.')

  return {
    pages: [{ pageNumber: 1, extractedText: text }],
    images: [],
    completedCapabilities: ['extracted-text'],
    warnings,
    extractor: {
      id: RICH_DOCUMENT_EXTENSIONS.has(extension) ? 'system-document-text' : 'plain-text',
      version: '1.0.0',
    },
    totalPageCount: 1,
  }
}
