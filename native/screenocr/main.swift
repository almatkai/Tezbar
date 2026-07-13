import Cocoa
import PDFKit
import Vision

enum HelperError: LocalizedError {
  case invalidArguments
  case captureFailed
  case screenRecordingPermissionDenied
  case documentOpenFailed

  var errorDescription: String? {
    switch self {
    case .invalidArguments: return "Invalid ScreenOCR helper arguments"
    case .captureFailed: return "Failed to capture an image"
    case .documentOpenFailed: return "Failed to open the document"
    case .screenRecordingPermissionDenied:
      return "Screen Recording permission is required. Enable Raymes in System Settings > Privacy & Security > Screen & System Audio Recording."
    }
  }
}

func bool(_ values: [String: Any], _ key: String, default fallback: Bool = false) -> Bool {
  values[key] as? Bool ?? fallback
}

func strings(_ values: [String: Any], _ key: String) -> [String] {
  values[key] as? [String] ?? []
}

func copyToPasteboard(_ image: CGImage) {
  let pasteboardImage = NSImage(cgImage: image, size: NSSize(width: image.width, height: image.height))
  let pasteboard = NSPasteboard.general
  pasteboard.clearContents()
  pasteboard.writeObjects([pasteboardImage])
}

func captureFullscreen(keepImage: Bool) -> CGImage? {
  let output = FileManager.default.temporaryDirectory
    .appendingPathComponent("tezbar-screenocr-\(UUID().uuidString).png")
  defer { try? FileManager.default.removeItem(at: output) }
  let process = Process()
  process.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
  process.arguments = ["-x", output.path]
  do {
    try process.run()
    process.waitUntilExit()
  } catch {
    return nil
  }
  guard process.terminationStatus == 0,
        let screenImage = NSImage(contentsOf: output),
        let data = screenImage.tiffRepresentation,
        let bitmap = NSBitmapImageRep(data: data),
        let image = bitmap.cgImage else { return nil }
  if keepImage { copyToPasteboard(image) }
  return image
}

func captureSelectedArea(keepImage: Bool, playSound: Bool) -> CGImage? {
  let output = FileManager.default.temporaryDirectory
    .appendingPathComponent("tezbar-screenocr-\(UUID().uuidString).png")
  defer { try? FileManager.default.removeItem(at: output) }

  let process = Process()
  process.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
  var arguments = ["-i"]
  if !playSound { arguments.append("-x") }
  arguments.append(output.path)
  process.arguments = arguments
  do {
    try process.run()
    process.waitUntilExit()
  } catch {
    return nil
  }
  guard process.terminationStatus == 0,
        let image = NSImage(contentsOf: output),
        let data = image.tiffRepresentation,
        let bitmap = NSBitmapImageRep(data: data),
        let cgImage = bitmap.cgImage else { return nil }
  if keepImage { copyToPasteboard(cgImage) }
  return cgImage
}

func capturedImage(_ values: [String: Any], fullscreen: Bool) throws -> CGImage {
  if let imagePath = values["imagePath"] as? String,
     let source = NSImage(contentsOfFile: imagePath),
     let data = source.tiffRepresentation,
     let bitmap = NSBitmapImageRep(data: data),
     let image = bitmap.cgImage {
    return image
  }
  if !CGPreflightScreenCaptureAccess() {
    throw HelperError.screenRecordingPermissionDenied
  }
  let image = fullscreen
    ? captureFullscreen(keepImage: bool(values, "keepImage"))
    : captureSelectedArea(
        keepImage: bool(values, "keepImage"),
        playSound: bool(values, "playSound")
      )
  guard let image else { throw HelperError.captureFailed }
  return image
}

func recognizeTextDetails(_ image: CGImage, values: [String: Any]) throws -> [String: Any] {
  var result = ""
  var blocks: [[String: Any]] = []
  var requestError: Error?
  let ignoreLineBreaks = bool(values, "ignoreLineBreaks")
  let request = VNRecognizeTextRequest { request, error in
    requestError = error
    guard let observations = request.results as? [VNRecognizedTextObservation] else { return }
    blocks = observations.compactMap { observation in
      guard let candidate = observation.topCandidates(1).first else { return nil }
      let box = observation.boundingBox
      return [
        "text": candidate.string,
        "confidence": Double(candidate.confidence),
        "bounds": [
          "x": Double(box.origin.x),
          "y": Double(box.origin.y),
          "width": Double(box.size.width),
          "height": Double(box.size.height),
        ],
      ]
    }
    result = blocks.compactMap { $0["text"] as? String }
      .joined(separator: ignoreLineBreaks ? " " : "\n")
  }
  request.recognitionLevel = bool(values, "fast") ? .fast : .accurate
  request.usesLanguageCorrection = bool(values, "languageCorrection")
  request.recognitionLanguages = strings(values, "languages").isEmpty
    ? ["en-US"]
    : strings(values, "languages")
  request.customWords = strings(values, "customWordsList")
  try VNImageRequestHandler(cgImage: image, options: [:]).perform([request])
  if let requestError { throw requestError }
  let confidences = blocks.compactMap { $0["confidence"] as? Double }
  let averageConfidence = confidences.isEmpty
    ? nil
    : confidences.reduce(0, +) / Double(confidences.count)
  var output: [String: Any] = ["text": result, "blocks": blocks]
  if let averageConfidence { output["averageConfidence"] = averageConfidence }
  return output
}

func recognizeText(_ values: [String: Any]) throws -> String {
  let image = try capturedImage(values, fullscreen: bool(values, "fullscreen"))
  return try recognizeTextDetails(image, values: values)["text"] as? String ?? ""
}

func recognizeFile(_ values: [String: Any]) throws -> [String: Any] {
  let image = try capturedImage(values, fullscreen: false)
  var result = try recognizeTextDetails(image, values: values)
  result["width"] = image.width
  result["height"] = image.height
  return result
}

func pdfPageImage(_ page: PDFPage) -> CGImage? {
  let bounds = page.bounds(for: .mediaBox)
  guard bounds.width > 0, bounds.height > 0 else { return nil }
  let maxDimension: CGFloat = 1800
  let scale = min(maxDimension / max(bounds.width, bounds.height), 2.5)
  let size = NSSize(width: max(1, bounds.width * scale), height: max(1, bounds.height * scale))
  let thumbnail = page.thumbnail(of: size, for: .mediaBox)
  guard let data = thumbnail.tiffRepresentation,
        let bitmap = NSBitmapImageRep(data: data) else { return nil }
  return bitmap.cgImage
}

func extractPdf(_ values: [String: Any]) throws -> [String: Any] {
  guard let documentPath = values["documentPath"] as? String,
        let document = PDFDocument(url: URL(fileURLWithPath: documentPath)) else {
    throw HelperError.documentOpenFailed
  }
  let requestedMax = values["maxPages"] as? Int ?? document.pageCount
  let pageLimit = min(document.pageCount, max(0, requestedMax))
  let shouldOcr = bool(values, "ocrScannedPages", default: true)
  let requestedOcrMax = values["maxOcrPages"] as? Int ?? pageLimit
  let ocrPageLimit = max(0, requestedOcrMax)
  let ocrEveryPage = bool(values, "ocrEveryPage")
  var ocrPages = 0
  var pages: [[String: Any]] = []
  for index in 0..<pageLimit {
    guard let page = document.page(at: index) else { continue }
    let extracted = page.string?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    var row: [String: Any] = ["pageNumber": index + 1, "extractedText": extracted]
    if shouldOcr && ocrPages < ocrPageLimit && (ocrEveryPage || extracted.count < 32),
       let image = pdfPageImage(page) {
      row["ocr"] = try recognizeTextDetails(image, values: values)
      ocrPages += 1
    }
    pages.append(row)
  }
  return ["pages": pages, "totalPages": document.pageCount]
}

func detectBarcode(_ values: [String: Any]) throws -> String {
  let image = try capturedImage(values, fullscreen: false)
  var result = ""
  var requestError: Error?
  let request = VNDetectBarcodesRequest { request, error in
    requestError = error
    let observations = request.results as? [VNBarcodeObservation] ?? []
    result = observations.compactMap(\.payloadStringValue).joined(separator: "\n")
  }
  try VNImageRequestHandler(cgImage: image, options: [:]).perform([request])
  if let requestError { throw requestError }
  return result.isEmpty ? "No barcodes or QR codes detected" : result
}

func emit(_ value: [String: Any]) {
  guard let data = try? JSONSerialization.data(withJSONObject: value),
        let output = String(data: data, encoding: .utf8) else { return }
  print(output)
}

do {
  guard CommandLine.arguments.count == 3,
        let data = CommandLine.arguments[2].data(using: .utf8),
        let values = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
    throw HelperError.invalidArguments
  }
  let value: Any
  switch CommandLine.arguments[1] {
  case "recognize-text": value = try recognizeText(values)
  case "recognize-file": value = try recognizeFile(values)
  case "extract-pdf": value = try extractPdf(values)
  case "detect-barcode": value = try detectBarcode(values)
  default: throw HelperError.invalidArguments
  }
  emit(["ok": true, "value": value])
} catch {
  emit(["ok": false, "error": error.localizedDescription])
  exit(1)
}
