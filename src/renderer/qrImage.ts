const QR_EXPORT_SIZE = 512

type QrRasterDependencies = {
  createImage?: () => HTMLImageElement
  createCanvas?: () => HTMLCanvasElement
}

/** Convert the generated SVG QR into a PNG data URL for native saving.
 * WKWebView/Tauri does not reliably handle its context-menu download action
 * for inline SVG data URLs, but a PNG produced by canvas can be saved by the
 * native host without involving webview navigation. */
export function qrDataUrlToPngDataUrl(
  svgDataUrl: string,
  dependencies: QrRasterDependencies = {},
): Promise<string> {
  const createImage = dependencies.createImage ?? (() => new Image())
  const createCanvas = dependencies.createCanvas ?? (() => document.createElement('canvas'))

  return new Promise((resolve, reject) => {
    const image = createImage()
    image.onload = () => {
      const canvas = createCanvas()
      canvas.width = QR_EXPORT_SIZE
      canvas.height = QR_EXPORT_SIZE
      const context = canvas.getContext('2d')
      if (!context) {
        reject(new Error('Could not create QR image'))
        return
      }
      context.drawImage(image, 0, 0, QR_EXPORT_SIZE, QR_EXPORT_SIZE)
      resolve(canvas.toDataURL('image/png'))
    }
    image.onerror = () => reject(new Error('Could not render QR image'))
    image.src = svgDataUrl
  })
}
