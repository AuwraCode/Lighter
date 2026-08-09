# Generates the 1024x1024 source icon: a single minimalist quill on the dark
# rounded tile. Output: icon-source.png in the repo root; feed it to
# `pnpm tauri icon`.
Add-Type -AssemblyName System.Drawing

$size = 1024
$bmp = New-Object System.Drawing.Bitmap($size, $size)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.Clear([System.Drawing.Color]::Transparent)

# --- rounded tile with a vertical gradient ----------------------------------
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

$pen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(60, 124, 140, 248), 10)
$g.DrawPath($pen, $path)

# --- minimalist quill ---------------------------------------------------------
# NOTE: PowerShell variables are case-insensitive, so the control points must
# not collide with function parameters like $t.
$nibPt = @(300, 830)    # nib, lower left
$ctrlPt = @(452, 420)   # control point (bows the shaft left)
$topPt = @(772, 168)    # plume top, upper right

function Bez($t) {
    $mt = 1 - $t
    $x = $mt * $mt * $script:nibPt[0] + 2 * $mt * $t * $script:ctrlPt[0] + $t * $t * $script:topPt[0]
    $y = $mt * $mt * $script:nibPt[1] + 2 * $mt * $t * $script:ctrlPt[1] + $t * $t * $script:topPt[1]
    return @($x, $y)
}
function BezTangent($t) {
    $x = 2 * (1 - $t) * ($script:ctrlPt[0] - $script:nibPt[0]) + 2 * $t * ($script:topPt[0] - $script:ctrlPt[0])
    $y = 2 * (1 - $t) * ($script:ctrlPt[1] - $script:nibPt[1]) + 2 * $t * ($script:topPt[1] - $script:ctrlPt[1])
    $len = [Math]::Sqrt($x * $x + $y * $y)
    return @(($x / $len), ($y / $len))
}
# Half-width profile: 0 at the nib, broad in the upper third, soft point on top.
function HalfWidth($t) {
    return 128 * [Math]::Pow($t, 0.62) * [Math]::Pow((1 - $t), 0.24)
}

$left = New-Object System.Collections.Generic.List[System.Drawing.PointF]
$right = New-Object System.Collections.Generic.List[System.Drawing.PointF]
for ($i = 0; $i -le 48; $i++) {
    $t = $i / 48
    $b = Bez $t
    $tan = BezTangent $t
    $nx = -$tan[1]; $ny = $tan[0]   # normal
    $w = HalfWidth $t
    $left.Add((New-Object System.Drawing.PointF(($b[0] + $nx * $w), ($b[1] + $ny * $w))))
    $right.Add((New-Object System.Drawing.PointF(($b[0] - $nx * $w), ($b[1] - $ny * $w))))
}
$outline = New-Object System.Collections.Generic.List[System.Drawing.PointF]
$outline.AddRange($left)
$right.Reverse()
$outline.AddRange($right)

$g.SetClip($path)

$featherBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    (New-Object System.Drawing.Point(280, 860)),
    (New-Object System.Drawing.Point(800, 140)),
    [System.Drawing.Color]::FromArgb(255, 206, 214, 240),
    [System.Drawing.Color]::FromArgb(255, 248, 249, 253))
$g.FillPolygon($featherBrush, $outline.ToArray())

# Shaft groove: one clean negative line through the plume.
$shaftPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 20, 22, 30), 14)
$shaftPen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
$shaftPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
$shaftPts = New-Object System.Collections.Generic.List[System.Drawing.PointF]
for ($i = 3; $i -le 45; $i++) {
    $b = Bez ($i / 48)
    $shaftPts.Add((New-Object System.Drawing.PointF($b[0], $b[1])))
}
$g.DrawLines($shaftPen, $shaftPts.ToArray())

$g.ResetClip()

$out = Join-Path $PSScriptRoot "..\icon-source.png"
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output "wrote $out"
