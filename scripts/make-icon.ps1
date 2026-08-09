# Generates the 1024x1024 source icon (dark rounded square + accent bolt).
# Output: icon-source.png in the repo root; feed it to `pnpm tauri icon`.
Add-Type -AssemblyName System.Drawing

$size = 1024
$bmp = New-Object System.Drawing.Bitmap($size, $size)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.Clear([System.Drawing.Color]::Transparent)

# Rounded-square background with a vertical gradient.
$radius = 190
$rect = New-Object System.Drawing.Rectangle(32, 32, ($size - 64), ($size - 64))
$path = New-Object System.Drawing.Drawing2D.GraphicsPath
$d = $radius * 2
$path.AddArc($rect.X, $rect.Y, $d, $d, 180, 90)
$path.AddArc($rect.Right - $d, $rect.Y, $d, $d, 270, 90)
$path.AddArc($rect.Right - $d, $rect.Bottom - $d, $d, $d, 0, 90)
$path.AddArc($rect.X, $rect.Bottom - $d, $d, $d, 90, 90)
$path.CloseFigure()

$gradTop = [System.Drawing.Color]::FromArgb(255, 32, 35, 48)
$gradBottom = [System.Drawing.Color]::FromArgb(255, 11, 12, 14)
$brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    (New-Object System.Drawing.Point(0, 0)),
    (New-Object System.Drawing.Point(0, $size)),
    $gradTop, $gradBottom)
$g.FillPath($brush, $path)

# Subtle border.
$pen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(60, 124, 140, 248), 10)
$g.DrawPath($pen, $path)

# Lightning bolt (accent #7c8cf8), centered.
$bolt = @(
    (New-Object System.Drawing.PointF(566, 128)),
    (New-Object System.Drawing.PointF(288, 574)),
    (New-Object System.Drawing.PointF(470, 574)),
    (New-Object System.Drawing.PointF(420, 896)),
    (New-Object System.Drawing.PointF(738, 428)),
    (New-Object System.Drawing.PointF(542, 428))
)
$boltBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 124, 140, 248))
$g.FillPolygon($boltBrush, $bolt)

$out = Join-Path $PSScriptRoot "..\icon-source.png"
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output "wrote $out"
