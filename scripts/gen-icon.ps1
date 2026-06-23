# gen-icon.ps1 — Genera TODOS los iconos del widget desde una sola fuente:
#   -Icono de app -> PNGs + icon.ico (Windows/Linux) + icon.icns (macOS).
#   -Iconos de bandeja (la "zona de minimizada") -> tray-normal/warning/critical.png.
#
# LOGO: un "anillo-gauge" radial;un aro de 300° con un hueco abajo,
# extremos redondeados y un punto/indicador en la punta del relleno. Evoca un
# medidor de uso, se reconoce a 16 px y funciona en monocromo (silueta por
# forma, no por color), siguiendo lo que hacen las grandes con los app icons.
#
# Variantes por plataforma (como Apple/MS recomiendan, NO el mismo PNG escalado):
#   -App icon Win/Linux -> cuadrado redondeado a casi-sangre.
#   -App icon macOS     -> "squircle" con el MARGEN del grid de Apple (+-9.8%) y
#                          una sombra suave, para que encaje en el Dock como las
#                          demás apps (un cuadrado a sangre se ve descolocado).
#   -Bandeja            -> SIN fondo (transparente): solo el aro en el color de
#                          severidad, para que se vea en cualquier barra (clara u
#                          oscura). Un fondo oscuro se perdería en taskbars oscuras.

Add-Type -AssemblyName System.Drawing

$OutDir = Join-Path $PSScriptRoot "..\src-tauri\icons"
$OutDir = (Resolve-Path $OutDir).Path

# Paleta (coincide con styles.css del widget).
$BG_TOP = [System.Drawing.Color]::FromArgb(255, 0x1b, 0x1b, 0x20)
$BG_BOT = [System.Drawing.Color]::FromArgb(255, 0x0d, 0x0d, 0x10)
$GREEN  = [System.Drawing.Color]::FromArgb(255, 0x59, 0xd4, 0x99)  # normal
$AMBER  = [System.Drawing.Color]::FromArgb(255, 0xff, 0xc5, 0x33)  # warning
$RED    = [System.Drawing.Color]::FromArgb(255, 0xff, 0x61, 0x61)  # critical
$TRACK  = [System.Drawing.Color]::FromArgb(46, 255, 255, 255)
$BORDER = [System.Drawing.Color]::FromArgb(40, 255, 255, 255)

# Geometria compartida del gauge
$GAUGE_START = 120.0   # arranque del aro (lower-left); 0°=3 en punto, horario.
$GAUGE_SWEEP = 300.0   # barrido total → hueco de 60° centrado abajo.
$GAUGE_FILL  = 0.72    # fracción rellena en el app icon (muestra "uso").

function New-RoundedPath([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
  $p = New-Object System.Drawing.Drawing2D.GraphicsPath
  $d = $r * 2
  $p.AddArc($x, $y, $d, $d, 180, 90)
  $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
  $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
  $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
  $p.CloseFigure()
  return $p
}

# Dibuja el anillo-gauge: pista opcional + relleno + punto en la punta.
function Draw-Gauge {
  param(
    [System.Drawing.Graphics]$g,
    [float]$cx, [float]$cy, [float]$dia, [float]$stroke,
    [System.Drawing.Color]$fill,
    [System.Drawing.Color]$track = ([System.Drawing.Color]::Empty),
    [float]$frac = 1.0,
    [System.Drawing.Color]$dot = ([System.Drawing.Color]::Empty)
  )
  $rect = New-Object System.Drawing.RectangleF(($cx - $dia / 2), ($cy - $dia / 2), $dia, $dia)
  $round = [System.Drawing.Drawing2D.LineCap]::Round

  if (-not $track.IsEmpty) {
    $tp = New-Object System.Drawing.Pen($track, $stroke)
    $tp.StartCap = $round; $tp.EndCap = $round
    $g.DrawArc($tp, $rect, $GAUGE_START, $GAUGE_SWEEP)
    $tp.Dispose()
  }

  $fillSweep = [float]($GAUGE_SWEEP * $frac)
  $fp = New-Object System.Drawing.Pen($fill, $stroke)
  $fp.StartCap = $round; $fp.EndCap = $round
  $g.DrawArc($fp, $rect, $GAUGE_START, $fillSweep)
  $fp.Dispose()

  if (-not $dot.IsEmpty) {
    $ang = ($GAUGE_START + $fillSweep) * [Math]::PI / 180.0
    $r = $dia / 2
    $dx = $cx + $r * [Math]::Cos($ang)
    $dy = $cy + $r * [Math]::Sin($ang)
    $dr = $stroke * 0.74
    $db = New-Object System.Drawing.SolidBrush($dot)
    $g.FillEllipse($db, [float]($dx - $dr), [float]($dy - $dr), [float]($dr * 2), [float]($dr * 2))
    $db.Dispose()
  }
}

# Icono de app. $mac=$true → squircle con margen del grid de Apple + sombra.
function New-AppIcon([int]$s, [bool]$mac) {
  $bmp = New-Object System.Drawing.Bitmap($s, $s, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $g.Clear([System.Drawing.Color]::Transparent)

  if ($mac) {
    # Grid de Apple: cuerpo ~80.5% del lienzo (margen ~9.8%), esquina ~22.5%.
    $pad = $s * 0.098
    $radius = ($s - 2 * $pad) * 0.225
  }
  else {
    # Win/Linux: casi a sangre, esquina estilo Win11.
    $pad = $s * 0.06
    $radius = ($s - 2 * $pad) * 0.22
  }
  $side = $s - 2 * $pad

  # Sombra suave bajo el squircle (solo macOS): varias capas con poco alfa para
  # falsear un desenfoque sin filtros.
  if ($mac) {
    for ($i = 6; $i -ge 1; $i--) {
      $off = $side * 0.012 * $i
      $exp = $side * 0.004 * $i
      $sp = New-RoundedPath ($pad - $exp) ($pad - $exp + $off) ($side + 2 * $exp) ($side + 2 * $exp) ($radius + $exp)
      $sb = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(10, 0, 0, 0))
      $g.FillPath($sb, $sp)
      $sb.Dispose(); $sp.Dispose()
    }
  }

  # Cuerpo: cuadrado/squircle con gradiente vertical + borde sutil.
  $path = New-RoundedPath $pad $pad $side $side $radius
  $rect = New-Object System.Drawing.RectangleF($pad, $pad, $side, $side)
  $lg = New-Object System.Drawing.Drawing2D.LinearGradientBrush($rect, $BG_TOP, $BG_BOT, 90)
  $g.FillPath($lg, $path)
  $bp = New-Object System.Drawing.Pen($BORDER, [Math]::Max(1, $s * 0.008))
  $g.DrawPath($bp, $path)
  $lg.Dispose(); $bp.Dispose(); $path.Dispose()

  # La marca: anillo-gauge centrado, con pista + relleno verde + punto.
  $dia = $side * 0.62
  $stroke = [float]($side * 0.10)
  Draw-Gauge -g $g -cx ($s / 2) -cy ($s / 2) -dia $dia -stroke $stroke `
    -fill $GREEN -track $TRACK -frac $GAUGE_FILL -dot $GREEN

  $g.Dispose()
  return $bmp
}

# Icono de bandeja: SIN fondo, aro completo en el color de severidad. Glifo
# crisp y visible en cualquier barra (clara u oscura).
function New-TrayIcon([int]$s, [System.Drawing.Color]$color) {
  $bmp = New-Object System.Drawing.Bitmap($s, $s, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $g.Clear([System.Drawing.Color]::Transparent)

  $dia = $s * 0.72
  $stroke = [float]($s * 0.15)
  Draw-Gauge -g $g -cx ($s / 2) -cy ($s / 2) -dia $dia -stroke $stroke -fill $color -frac 1.0
  $g.Dispose()
  return $bmp
}

# --- PNGs de app que usa el bundler (cuadrado redondeado, Win/Linux) ---
function Save-Png([int]$s, [string]$name) {
  $b = New-AppIcon $s $false
  $b.Save((Join-Path $OutDir $name), [System.Drawing.Imaging.ImageFormat]::Png)
  $b.Dispose()
}
Save-Png 32  "32x32.png"
Save-Png 128 "128x128.png"
Save-Png 256 "128x128@2x.png"
Save-Png 1024 "icon.png"

# --- Iconos de bandeja (zona de minimizada) ---
function Save-Tray([System.Drawing.Color]$color, [string]$name) {
  $b = New-TrayIcon 32 $color
  $b.Save((Join-Path $OutDir $name), [System.Drawing.Imaging.ImageFormat]::Png)
  $b.Dispose()
}
Save-Tray $GREEN "tray-normal.png"
Save-Tray $AMBER "tray-warning.png"
Save-Tray $RED   "tray-critical.png"

# --- icon.icns (macOS) — usa la variante SQUIRCLE con margen del grid de Apple.
# Formato Apple ICNS: 'icns' + longitud total (BE) + serie de bloques
# [OSType(4) + longitud(BE,4) + datos PNG]. Big-endian a mano (BinaryWriter es LE).
function Get-PngBytes([int]$s) {
  $b = New-AppIcon $s $true
  $ms = New-Object System.IO.MemoryStream
  $b.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
  $b.Dispose()
  return $ms.ToArray()
}
function Write-UInt32BE([System.IO.Stream]$st, [uint32]$v) {
  $bytes = [byte[]]@((($v -shr 24) -band 0xFF), (($v -shr 16) -band 0xFF), (($v -shr 8) -band 0xFF), ($v -band 0xFF))
  $st.Write($bytes, 0, 4)
}
# Tipos OSType estándar (datos PNG, soportados por macOS 10.7+).
$icnsTypes = [ordered]@{ icp4 = 16; icp5 = 32; icp6 = 64; ic07 = 128; ic08 = 256; ic09 = 512; ic10 = 1024 }
$body = New-Object System.IO.MemoryStream
foreach ($t in $icnsTypes.Keys) {
  $png = Get-PngBytes $icnsTypes[$t]
  $body.Write([System.Text.Encoding]::ASCII.GetBytes($t), 0, 4)
  Write-UInt32BE $body ([uint32]($png.Length + 8))
  $body.Write($png, 0, $png.Length)
}
$bodyBytes = $body.ToArray()
$icns = New-Object System.IO.MemoryStream
$icns.Write([System.Text.Encoding]::ASCII.GetBytes('icns'), 0, 4)
Write-UInt32BE $icns ([uint32]($bodyBytes.Length + 8))
$icns.Write($bodyBytes, 0, $bodyBytes.Length)
[System.IO.File]::WriteAllBytes((Join-Path $OutDir "icon.icns"), $icns.ToArray())

# --- icon.ico (DIB BGRA 32-bit, multi-tamaño) — variante Win/Linux ---
function Get-DibBlob([System.Drawing.Bitmap]$bmp) {
  $s = $bmp.Width
  $rect = New-Object System.Drawing.Rectangle(0, 0, $s, $s)
  $ld = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $top = New-Object byte[] ($s * $s * 4)
  [System.Runtime.InteropServices.Marshal]::Copy($ld.Scan0, $top, 0, $top.Length)
  $bmp.UnlockBits($ld)

  $ms = New-Object System.IO.MemoryStream
  $bw = New-Object System.IO.BinaryWriter($ms)
  # BITMAPINFOHEADER
  $bw.Write([uint32]40)
  $bw.Write([int32]$s)
  $bw.Write([int32]($s * 2))   # alto = XOR + AND
  $bw.Write([uint16]1)
  $bw.Write([uint16]32)
  $bw.Write([uint32]0)
  $bw.Write([uint32]0)
  $bw.Write([int32]0); $bw.Write([int32]0)
  $bw.Write([uint32]0); $bw.Write([uint32]0)
  # XOR: filas de abajo a arriba (BGRA tal cual sale de LockBits)
  $stride = $s * 4
  for ($y = $s - 1; $y -ge 0; $y--) {
    $bw.Write($top, $y * $stride, $stride)
  }
  # Máscara AND: 1bpp, filas alineadas a 4 bytes, todo 0 (usa el alfa del XOR)
  $andRow = [int]([Math]::Floor(($s + 31) / 32) * 4)
  $zeros = New-Object byte[] $andRow
  for ($y = 0; $y -lt $s; $y++) { $bw.Write($zeros, 0, $andRow) }
  $bw.Flush()
  return $ms.ToArray()
}

$sizes = 16, 24, 32, 48, 64, 128, 256
$blobs = @()
foreach ($sz in $sizes) {
  $b = New-AppIcon $sz $false
  $blobs += , (Get-DibBlob $b)
  $b.Dispose()
}

$ico = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter($ico)
$w.Write([uint16]0)               # reservado
$w.Write([uint16]1)               # tipo = icono
$w.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
  $sz = $sizes[$i]
  $len = $blobs[$i].Length
  $w.Write([byte]($(if ($sz -ge 256) { 0 } else { $sz })))
  $w.Write([byte]($(if ($sz -ge 256) { 0 } else { $sz })))
  $w.Write([byte]0)               # colores en paleta
  $w.Write([byte]0)               # reservado
  $w.Write([uint16]1)             # planos
  $w.Write([uint16]32)            # bits por píxel
  $w.Write([uint32]$len)
  $w.Write([uint32]$offset)
  $offset += $len
}
foreach ($blob in $blobs) { $w.Write($blob, 0, $blob.Length) }
$w.Flush()
[System.IO.File]::WriteAllBytes((Join-Path $OutDir "icon.ico"), $ico.ToArray())

"OK; iconos generados en $OutDir"
Get-ChildItem $OutDir -Filter "*.png" | Select-Object Name, Length
Get-ChildItem $OutDir -Filter "*.ic*" | Select-Object Name, Length
