param([Parameter(Mandatory=$true)][string]$Source)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$destination = Join-Path $PSScriptRoot '..\src-tauri\icons\desk-audio-cast'
New-Item -ItemType Directory -Path $destination -Force | Out-Null
# Deterministic format/size packaging only; artwork is generated separately.
Copy-Item -LiteralPath $Source -Destination (Join-Path $destination 'master.png')
$sourceImage = [System.Drawing.Image]::FromFile((Resolve-Path -LiteralPath $Source).Path)
$sizes = @(16, 20, 24, 32, 48, 64, 128, 256)
$payloads = [System.Collections.Generic.List[byte[]]]::new()
try {
    foreach ($size in $sizes) {
        $bitmap = [System.Drawing.Bitmap]::new($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.Clear([System.Drawing.Color]::Transparent)
            $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $graphics.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
            $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
            $graphics.DrawImage($sourceImage, 0, 0, $size, $size)
            $stream = [System.IO.MemoryStream]::new()
            try { $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png); $payloads.Add($stream.ToArray()) } finally { $stream.Dispose() }
            $bitmap.Save((Join-Path $destination "$($size)x$($size).png"), [System.Drawing.Imaging.ImageFormat]::Png)
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
    }
    $icon = [System.IO.MemoryStream]::new()
    $writer = [System.IO.BinaryWriter]::new($icon)
    try {
        $writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$sizes.Count)
        $offset = 6 + 16 * $sizes.Count
        for ($index = 0; $index -lt $sizes.Count; $index++) {
            $edge = if ($sizes[$index] -eq 256) { 0 } else { $sizes[$index] }
            $writer.Write([byte]$edge); $writer.Write([byte]$edge)
            $writer.Write([byte]0); $writer.Write([byte]0); $writer.Write([uint16]1); $writer.Write([uint16]32)
            $writer.Write([uint32]$payloads[$index].Length); $writer.Write([uint32]$offset)
            $offset += $payloads[$index].Length
        }
        foreach ($payload in $payloads) { $writer.Write($payload) }
        $writer.Flush()
        [System.IO.File]::WriteAllBytes((Join-Path $destination 'icon.ico'), $icon.ToArray())
    } finally { $writer.Dispose(); $icon.Dispose() }
    Copy-Item -LiteralPath (Join-Path $destination '32x32.png') -Destination (Join-Path $destination 'tray.png')
} finally { $sourceImage.Dispose() }
Get-ChildItem -LiteralPath $destination | Select-Object Name, Length
