# Generates the 1024x1024 source icon: a minimalist midnight-blue quill on a
# TRANSPARENT background. The shaft groove is cut to transparency so the mark
# works on any surface. Output: icon-source.png; feed it to `pnpm tauri icon`.
Add-Type -AssemblyName System.Drawing

$size = 1024
$bmp = New-Object System.Drawing.Bitmap($size, $size)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.Clear([System.Drawing.Color]::Transparent)

# --- minimalist quill ---------------------------------------------------------
# NOTE: PowerShell variables are case-insensitive, so the control points must
# not collide with function parameters like $t.
$nibPt = @(238, 900)    # nib, lower left
$ctrlPt = @(430, 400)   # control point (bows the shaft left)
$topPt = @(810, 108)    # plume top, upper right

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
    return 152 * [Math]::Pow($t, 0.62) * [Math]::Pow((1 - $t), 0.24)
}

$left = New-Object System.Collections.Generic.List[System.Drawing.PointF]
$right = New-Object System.Collections.Generic.List[System.Drawing.PointF]
for ($i = 0; $i -le 56; $i++) {
    $t = $i / 56
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

# Midnight blue: deep at the nib, lighter royal tone at the plume top.
$featherBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    (New-Object System.Drawing.Point(220, 930)),
    (New-Object System.Drawing.Point(840, 90)),
    [System.Drawing.Color]::FromArgb(255, 20, 27, 94),
    [System.Drawing.Color]::FromArgb(255, 62, 82, 190))
$g.FillPolygon($featherBrush, $outline.ToArray())

# Shaft groove: cut to TRANSPARENCY so the mark works on any background.
$g.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
$shaftPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(0, 0, 0, 0), 16)
$shaftPen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
$shaftPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
$shaftPts = New-Object System.Collections.Generic.List[System.Drawing.PointF]
for ($i = 3; $i -le 53; $i++) {
    $b = Bez ($i / 56)
    $shaftPts.Add((New-Object System.Drawing.PointF($b[0], $b[1])))
}
$g.DrawLines($shaftPen, $shaftPts.ToArray())
$g.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceOver

$out = Join-Path $PSScriptRoot "..\icon-source.png"
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output "wrote $out"
