import Cocoa
import Vision

enum HelperError: LocalizedError {
  case invalidArguments
  case captureFailed
  case screenRecordingPermissionDenied

  var errorDescription: String? {
    switch self {
    case .invalidArguments: return "Invalid ScreenOCR helper arguments"
    case .captureFailed: return "Failed to capture an image"
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

func recognizeText(_ values: [String: Any]) throws -> String {
  let image = try capturedImage(values, fullscreen: bool(values, "fullscreen"))
  var result = ""
  var requestError: Error?
  let ignoreLineBreaks = bool(values, "ignoreLineBreaks")
  let request = VNRecognizeTextRequest { request, error in
    requestError = error
    guard let observations = request.results as? [VNRecognizedTextObservation] else { return }
    result = observations.compactMap { $0.topCandidates(1).first?.string }
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
  return result
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
  let value: String
  switch CommandLine.arguments[1] {
  case "recognize-text": value = try recognizeText(values)
  case "detect-barcode": value = try detectBarcode(values)
  default: throw HelperError.invalidArguments
  }
  emit(["ok": true, "value": value])
} catch {
  emit(["ok": false, "error": error.localizedDescription])
  exit(1)
}
