param(
  [Parameter(Mandatory = $true, Position = 0)]
  [string]$ImagePath,

  [Parameter(Position = 1)]
  [int]$RequestedCount = 40,

  [Parameter(Position = 2)]
  [string]$DominantOnly = 'false'
)

$ErrorActionPreference = 'Stop'

function Fail([string]$Message) {
  [Console]::Error.WriteLine($Message)
  exit 1
}

function Get-Hsl([double]$Red, [double]$Green, [double]$Blue) {
  $maximum = [math]::Max($Red, [math]::Max($Green, $Blue))
  $minimum = [math]::Min($Red, [math]::Min($Green, $Blue))
  $lightness = ($maximum + $minimum) / 2.0
  $delta = $maximum - $minimum

  if ($delta -le 0) {
    return [pscustomobject]@{
      Hue = 0.0
      Saturation = 0.0
      Lightness = $lightness * 100.0
    }
  }

  $denominator = 1.0 - [math]::Abs(2.0 * $lightness - 1.0)
  $saturation = if ($denominator -le 0) { 0.0 } else { $delta / $denominator }
  if ($maximum -eq $Red) {
    $hue = 60.0 * ((($Green - $Blue) / $delta) % 6.0)
  } elseif ($maximum -eq $Green) {
    $hue = 60.0 * ((($Blue - $Red) / $delta) + 2.0)
  } else {
    $hue = 60.0 * ((($Red - $Green) / $delta) + 4.0)
  }
  if ($hue -lt 0) { $hue += 360.0 }

  return [pscustomobject]@{
    Hue = $hue
    Saturation = $saturation * 100.0
    Lightness = $lightness * 100.0
  }
}

if (-not (Test-Path -LiteralPath $ImagePath -PathType Leaf)) {
  Fail 'The selected image no longer exists'
}

$RequestedCount = [math]::Max(1, [math]::Min(80, $RequestedCount))
$bitmap = $null
try {
  Add-Type -AssemblyName System.Drawing
  $bitmap = [System.Drawing.Bitmap]::new($ImagePath)
  if ($bitmap.Width -le 0 -or $bitmap.Height -le 0) { Fail 'Could not decode image' }

  $totalPixels = [int64]$bitmap.Width * [int64]$bitmap.Height
  $stride = [math]::Max(1, [int][math]::Ceiling([math]::Sqrt($totalPixels / 40000.0)))
  $buckets = @{}
  $sampledPixels = 0

  for ($y = 0; $y -lt $bitmap.Height; $y += $stride) {
    for ($x = 0; $x -lt $bitmap.Width; $x += $stride) {
      $pixel = $bitmap.GetPixel($x, $y)
      if ($pixel.A -lt 21) { continue }

      $red = [math]::Max(0.0, [math]::Min(1.0, $pixel.R / 255.0))
      $green = [math]::Max(0.0, [math]::Min(1.0, $pixel.G / 255.0))
      $blue = [math]::Max(0.0, [math]::Min(1.0, $pixel.B / 255.0))
      $redBin = [math]::Min(15, [int]($red * 15.999))
      $greenBin = [math]::Min(15, [int]($green * 15.999))
      $blueBin = [math]::Min(15, [int]($blue * 15.999))
      $key = "$redBin|$greenBin|$blueBin"

      if (-not $buckets.ContainsKey($key)) {
        $buckets[$key] = [pscustomobject]@{
          Count = 0
          Red = 0.0
          Green = 0.0
          Blue = 0.0
        }
      }
      $bucket = $buckets[$key]
      $bucket.Count = [int]$bucket.Count + 1
      $bucket.Red = [double]$bucket.Red + $red
      $bucket.Green = [double]$bucket.Green + $green
      $bucket.Blue = [double]$bucket.Blue + $blue
      $sampledPixels++
    }
  }

  if ($sampledPixels -le 0) { Fail 'Image contains no visible pixels' }
  $resultCount = if ($DominantOnly -eq 'true') { 1 } else { $RequestedCount }
  $colors = @(
    $buckets.Values |
      Sort-Object -Property Count -Descending |
      Select-Object -First $resultCount |
      ForEach-Object {
        $redUnit = $_.Red / [double]$_.Count
        $greenUnit = $_.Green / [double]$_.Count
        $blueUnit = $_.Blue / [double]$_.Count
        $redValue = [int][math]::Round($redUnit * 255.0)
        $greenValue = [int][math]::Round($greenUnit * 255.0)
        $blueValue = [int][math]::Round($blueUnit * 255.0)
        $hsl = Get-Hsl $redUnit $greenUnit $blueUnit
        [pscustomobject]@{
          hex = ('#{0}{1}{2}' -f $redValue.ToString('X2'), $greenValue.ToString('X2'), $blueValue.ToString('X2'))
          red = $redValue
          green = $greenValue
          blue = $blueValue
          area = [double]$_.Count / [double]$sampledPixels
          hue = $hsl.Hue
          saturation = $hsl.Saturation
          lightness = $hsl.Lightness
          intensity = ($redValue + $greenValue + $blueValue) / 3.0
        }
      }
  )

  $json = $colors | ConvertTo-Json -Compress
  [Console]::Out.Write($json)
} catch {
  if ($_.Exception.Message) { Fail $_.Exception.Message }
  Fail 'Could not decode image'
} finally {
  if ($bitmap) { $bitmap.Dispose() }
}
