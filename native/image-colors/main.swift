import AppKit
import Foundation

struct ColorBucket {
    var count = 0
    var red = 0.0
    var green = 0.0
    var blue = 0.0
}

struct ExtractedColor: Codable {
    let hex: String
    let red: Int
    let green: Int
    let blue: Int
    let area: Double
    let hue: Double
    let saturation: Double
    let lightness: Double
    let intensity: Double
}

func hsl(red: Double, green: Double, blue: Double) -> (Double, Double, Double) {
    let maximum = max(red, green, blue)
    let minimum = min(red, green, blue)
    let lightness = (maximum + minimum) / 2
    let delta = maximum - minimum
    guard delta > 0 else { return (0, 0, lightness * 100) }

    let saturation = delta / (1 - abs(2 * lightness - 1))
    let hue: Double
    if maximum == red {
        hue = 60 * (((green - blue) / delta).truncatingRemainder(dividingBy: 6))
    } else if maximum == green {
        hue = 60 * (((blue - red) / delta) + 2)
    } else {
        hue = 60 * (((red - green) / delta) + 4)
    }
    return (hue < 0 ? hue + 360 : hue, saturation * 100, lightness * 100)
}

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data(message.utf8))
    exit(1)
}

guard CommandLine.arguments.count >= 2 else { fail("Missing image path") }
let imagePath = CommandLine.arguments[1]
let requestedCount = max(1, min(80, Int(CommandLine.arguments.dropFirst(2).first ?? "40") ?? 40))
let dominantOnly = CommandLine.arguments.dropFirst(3).first == "true"

guard
    let image = NSImage(contentsOfFile: imagePath),
    let tiff = image.tiffRepresentation,
    let bitmap = NSBitmapImageRep(data: tiff),
    bitmap.pixelsWide > 0,
    bitmap.pixelsHigh > 0
else {
    fail("Could not decode image")
}

let totalPixels = bitmap.pixelsWide * bitmap.pixelsHigh
let stride = max(1, Int(ceil(sqrt(Double(totalPixels) / 40_000.0))))
var buckets: [Int: ColorBucket] = [:]
var sampledPixels = 0

for y in Swift.stride(from: 0, to: bitmap.pixelsHigh, by: stride) {
    for x in Swift.stride(from: 0, to: bitmap.pixelsWide, by: stride) {
        guard
            let sourceColor = bitmap.colorAt(x: x, y: y),
            let color = sourceColor.usingColorSpace(.sRGB),
            color.alphaComponent >= 0.08
        else { continue }

        let red = max(0, min(1, color.redComponent))
        let green = max(0, min(1, color.greenComponent))
        let blue = max(0, min(1, color.blueComponent))
        let redBin = min(15, Int(red * 15.999))
        let greenBin = min(15, Int(green * 15.999))
        let blueBin = min(15, Int(blue * 15.999))
        let key = (redBin << 8) | (greenBin << 4) | blueBin

        var bucket = buckets[key] ?? ColorBucket()
        bucket.count += 1
        bucket.red += red
        bucket.green += green
        bucket.blue += blue
        buckets[key] = bucket
        sampledPixels += 1
    }
}

guard sampledPixels > 0 else { fail("Image contains no visible pixels") }
let resultCount = dominantOnly ? 1 : requestedCount
let colors = buckets.values
    .sorted { $0.count > $1.count }
    .prefix(resultCount)
    .map { bucket -> ExtractedColor in
        let redUnit = bucket.red / Double(bucket.count)
        let greenUnit = bucket.green / Double(bucket.count)
        let blueUnit = bucket.blue / Double(bucket.count)
        let red = Int(round(redUnit * 255))
        let green = Int(round(greenUnit * 255))
        let blue = Int(round(blueUnit * 255))
        let components = hsl(red: redUnit, green: greenUnit, blue: blueUnit)
        return ExtractedColor(
            hex: String(format: "#%02X%02X%02X", red, green, blue),
            red: red,
            green: green,
            blue: blue,
            area: Double(bucket.count) / Double(sampledPixels),
            hue: components.0,
            saturation: components.1,
            lightness: components.2,
            intensity: (Double(red) + Double(green) + Double(blue)) / 3
        )
    }

do {
    FileHandle.standardOutput.write(try JSONEncoder().encode(colors))
} catch {
    fail("Could not encode extracted colors")
}
