[CmdletBinding()]
param(
  [ValidateRange(0.10, 0.40)]
  [double]$CornerRadiusRatio = 0.40,

  [ValidateRange(3, 8)]
  [int]$SuperellipseExponent = 4
)

$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$sourcePath = Join-Path $repositoryRoot 'docs/logo.png'
$iconDirectory = Join-Path $repositoryRoot 'apps/shell/src-tauri/icons'
$roundedPngPath = Join-Path $iconDirectory 'windows-rounded.png'
$windowsIconPath = Join-Path $iconDirectory 'icon.ico'
$generatedIconDirectory = Join-Path $repositoryRoot 'target/windows-rounded-icons'
$tauriCli = Join-Path $repositoryRoot 'node_modules/.bin/tauri.cmd'

if (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) {
  throw "Source logo not found: $sourcePath"
}
if (-not (Test-Path -LiteralPath $tauriCli -PathType Leaf)) {
  throw "Tauri CLI not found: $tauriCli. Run npm install first."
}

Add-Type -AssemblyName System.Drawing
$source = $null
$destination = $null
$graphics = $null
$path = $null

try {
  $source = [System.Drawing.Bitmap]::new($sourcePath)
  $width = $source.Width
  $height = $source.Height
  if ($width -ne $height) {
    throw 'The source logo must be square.'
  }

  $radius = [double]($width * $CornerRadiusRatio)
  $power = 2.0 / $SuperellipseExponent
  $stepsPerCorner = 192
  $quarterTurn = [Math]::PI / 2
  $points = [System.Collections.Generic.List[System.Drawing.PointF]]::new()

  function Add-IconPoint([double]$x, [double]$y) {
    [void]$points.Add([System.Drawing.PointF]::new([single]$x, [single]$y))
  }

  # Trace a superellipse (squircle) around the original square artwork.
  Add-IconPoint $radius 0
  Add-IconPoint ($width - $radius) 0
  for ($i = 1; $i -le $stepsPerCorner; $i++) {
    $angle = $quarterTurn * $i / $stepsPerCorner
    Add-IconPoint (($width - $radius) + $radius * [Math]::Pow([Math]::Sin($angle), $power)) ($radius - $radius * [Math]::Pow([Math]::Cos($angle), $power))
  }

  Add-IconPoint $width ($height - $radius)
  for ($i = 1; $i -le $stepsPerCorner; $i++) {
    $angle = $quarterTurn * $i / $stepsPerCorner
    Add-IconPoint (($width - $radius) + $radius * [Math]::Pow([Math]::Cos($angle), $power)) (($height - $radius) + $radius * [Math]::Pow([Math]::Sin($angle), $power))
  }

  Add-IconPoint $radius $height
  for ($i = 1; $i -le $stepsPerCorner; $i++) {
    $angle = $quarterTurn * $i / $stepsPerCorner
    Add-IconPoint ($radius - $radius * [Math]::Pow([Math]::Sin($angle), $power)) (($height - $radius) + $radius * [Math]::Pow([Math]::Cos($angle), $power))
  }

  Add-IconPoint 0 $radius
  for ($i = 1; $i -le $stepsPerCorner; $i++) {
    $angle = $quarterTurn * $i / $stepsPerCorner
    Add-IconPoint ($radius - $radius * [Math]::Pow([Math]::Cos($angle), $power)) ($radius - $radius * [Math]::Pow([Math]::Sin($angle), $power))
  }

  $path = [System.Drawing.Drawing2D.GraphicsPath]::new()
  $path.AddLines($points.ToArray())
  $path.CloseFigure()

  $destination = [System.Drawing.Bitmap]::new(
    $width,
    $height,
    [System.Drawing.Imaging.PixelFormat]::Format32bppArgb
  )
  $graphics = [System.Drawing.Graphics]::FromImage($destination)
  $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
  $graphics.Clear([System.Drawing.Color]::Transparent)
  $graphics.SetClip($path)
  $graphics.DrawImage($source, 0, 0, $width, $height)
  $graphics.ResetClip()
  $destination.Save($roundedPngPath, [System.Drawing.Imaging.ImageFormat]::Png)
}
finally {
  if ($graphics) { $graphics.Dispose() }
  if ($path) { $path.Dispose() }
  if ($destination) { $destination.Dispose() }
  if ($source) { $source.Dispose() }
}

& $tauriCli icon $roundedPngPath --output $generatedIconDirectory
if ($LASTEXITCODE -ne 0) {
  throw "Tauri icon generation failed with exit code $LASTEXITCODE."
}

$generatedWindowsIcon = Join-Path $generatedIconDirectory 'icon.ico'
if (-not (Test-Path -LiteralPath $generatedWindowsIcon -PathType Leaf)) {
  throw "Tauri did not generate the Windows ICO: $generatedWindowsIcon"
}
Copy-Item -LiteralPath $generatedWindowsIcon -Destination $windowsIconPath -Force

Write-Host "Generated $roundedPngPath"
Write-Host "Generated $windowsIconPath with $([int]($CornerRadiusRatio * 100))% superellipse corners."
