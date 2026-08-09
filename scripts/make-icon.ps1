# Generates the 1024x1024 source icon: a terminal window with an accent
# prompt chevron and a quill writing at the cursor — terminal + feather.
# Output: icon-source.png in the repo root; feed it to `pnpm tauri icon`.
Add-Type -AssemblyName System.Drawing

$size = 1024
$bmp = New-Object System.Drawing.Bitmap($size, $size)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.Clear([System.Drawing.Color]::Transparent)

# --- rounded-square terminal window with a vertical gradient ---------------
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

# --- title-bar dots (terminal window) ---------------------------------------
$dotBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 58, 61, 72))
foreach ($x in 150, 218, 286) {
    $g.FillEllipse($dotBrush, ($x - 22), 128, 44, 44)
}

# --- prompt chevron ❯ --------------------------------------------------------
$accent = [System.Drawing.Color]::FromArgb(255, 124, 140, 248)
$chev = New-Object System.Drawing.Pen($accent, 74)
$chev.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
$chev.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
$chev.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
$chevPts = @(
    (New-Object System.Drawing.PointF(232, 396)),
    (New-Object System.Drawing.PointF(392, 540)),
    (New-Object System.Drawing.PointF(232, 684))
)
$g.DrawLines($chev, $chevPts)

# --- cursor block being "written" -------------------------------------------
$cursorBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 61, 68, 140))
$cursorPath = New-Object System.Drawing.Drawing2D.GraphicsPath
$cursorPath.AddArc(478, 646, 24, 24, 90, 180)
$cursorPath.AddArc(596, 646, 24, 24, 270, 180)
$cursorPath.CloseFigure()
$g.FillPath($cursorBrush, $cursorPath)

# --- quill feather ------------------------------------------------------------
# NOTE: PowerShell variables are case-insensitive, so the control points must
# not collide with function parameters like $t.
$nibPt = @(452, 668)    # nib — right above the cursor
$ctrlPt = @(560, 380)   # control point (bows the shaft left)
$topPt = @(842, 172)    # plume top

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
    return 96 * [Math]::Pow($t, 0.65) * [Math]::Pow((1 - $t), 0.22)
}

$left = New-Object System.Collections.Generic.List[System.Drawing.PointF]
$right = New-Object System.Collections.Generic.List[System.Drawing.PointF]
for ($i = 0; $i -le 40; $i++) {
    $t = $i / 40
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

# Keep the plume inside the window.
$g.SetClip($path)

$featherBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    (New-Object System.Drawing.Point(400, 700)),
    (New-Object System.Drawing.Point(880, 150)),
    [System.Drawing.Color]::FromArgb(255, 214, 220, 240),
    [System.Drawing.Color]::FromArgb(255, 245, 246, 252))
$g.FillPolygon($featherBrush, $outline.ToArray())

# Shaft groove: dark line through the middle of the plume.
$shaftPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 22, 24, 32), 13)
$shaftPen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
$shaftPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
$shaftPts = New-Object System.Collections.Generic.List[System.Drawing.PointF]
for ($i = 2; $i -le 38; $i++) {
    $b = Bez ($i / 40)
    $shaftPts.Add((New-Object System.Drawing.PointF($b[0], $b[1])))
}
$g.DrawLines($shaftPen, $shaftPts.ToArray())

# Barb cuts: three notches on the outer edge for feather character.
$cutPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 22, 24, 32), 11)
$cutPen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
$cutPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
foreach ($t in 0.42, 0.60, 0.78) {
    $b = Bez $t
    $tan = BezTangent $t
    $nx = -$tan[1]; $ny = $tan[0]
    $w = HalfWidth $t
    # From just outside the left edge, angled back toward the shaft.
    $ex = $b[0] + $nx * ($w + 6) - $tan[0] * 6
    $ey = $b[1] + $ny * ($w + 6) - $tan[1] * 6
    $ix = $b[0] + $nx * ($w * 0.30) - $tan[0] * ($w * 0.72)
    $iy = $b[1] + $ny * ($w * 0.30) - $tan[1] * ($w * 0.72)
    $g.DrawLine($cutPen, [single]$ex, [single]$ey, [single]$ix, [single]$iy)
}

$g.ResetClip()

$out = Join-Path $PSScriptRoot "..\icon-source.png"
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output "wrote $out"
