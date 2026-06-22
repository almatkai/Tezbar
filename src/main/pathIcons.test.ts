import { describe, expect, it } from 'vitest'
import { fileIconDataUrl, folderIconDataUrl } from './pathIcons'

function decodeSvg(dataUrl: string): string {
  return Buffer.from(dataUrl.split(',')[1] ?? '', 'base64').toString('utf8')
}

describe('path icons', () => {
  it('uses a JavaScript document icon for .js files', () => {
    expect(decodeSvg(fileIconDataUrl('/tmp/index.js'))).toContain('>JS</text>')
  })

  it('uses the file extension for unknown document types', () => {
    expect(decodeSvg(fileIconDataUrl('/tmp/model.xyz'))).toContain('>XYZ</text>')
  })

  it('provides a folder icon', () => {
    expect(decodeSvg(folderIconDataUrl)).toContain('#62a8ed')
  })
})
