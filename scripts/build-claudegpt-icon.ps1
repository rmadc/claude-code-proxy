# Rebuild the checked-in ICO using the vendored SVG paths and Microsoft Edge.
[CmdletBinding()]
param([string]$EdgePath = "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe")
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$root = Split-Path $PSScriptRoot
$assets = Join-Path $root 'assets\windows'
$scratch = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
    $claude = [IO.File]::ReadAllText((Join-Path $assets 'claude.svg'))
    $openai = [IO.File]::ReadAllText((Join-Path $assets 'openai.svg'))
    $svg = '<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512"><rect x="8" y="8" width="496" height="496" rx="108" fill="#202522"/><circle cx="190" cy="190" r="164" fill="#f5eee5"/><circle cx="334" cy="334" r="164" fill="#202522"/><circle cx="334" cy="334" r="148" fill="#10a37f"/><g transform="translate(66 66) scale(10.3)" fill="#d97757">' +
        ($claude -replace '^.*?<path', '<path' -replace '</svg>\s*$', '') +
        '</g><g transform="translate(224 224) scale(9.2)" fill="white">' +
        ($openai -replace '^.*?<path', '<path' -replace '</svg>\s*$', '') + '</g></svg>'
    [IO.File]::WriteAllText((Join-Path $assets 'ClaudeGPT.svg'), $svg)
    $html = Join-Path $scratch 'icon.html'
    [IO.File]::WriteAllText($html, '<html><body style="margin:0;background:#202522;width:512px;height:512px">' + $svg + '</body></html>')
    $png = Join-Path $scratch 'icon.png'
    $edgeArgs = @('--headless', '--disable-gpu', '--no-first-run', '--hide-scrollbars', "--user-data-dir=`"$scratch\edge`"", '--window-size=512,512', "--screenshot=`"$png`"", "`"$([Uri]::new($html).AbsoluteUri)`"")
    $process = Start-Process -FilePath $EdgePath -ArgumentList $edgeArgs -WindowStyle Hidden -PassThru
    if (-not $process.WaitForExit(30000)) { $process.Kill(); throw 'Icon render timed out.' }
    if (-not (Test-Path -LiteralPath $png)) { throw 'Edge did not render the icon.' }
    $source = [Drawing.Image]::FromFile($png)
    try {
        $sizes = @(16, 24, 32, 48, 64, 128, 256)
        $layers = @()
        foreach ($size in $sizes) {
            $bitmap = New-Object Drawing.Bitmap $size, $size
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $graphics.DrawImage($source, 0, 0, $size, $size)
            $graphics.Dispose()
            try {
                $stream = New-Object IO.MemoryStream
                # Uncompressed 32-bit ICO images, including the legacy AND mask.
                $bitmap.Save($stream, [Drawing.Imaging.ImageFormat]::Bmp)
                $bmp = $stream.ToArray()
                $payload = New-Object byte[] ($bmp.Length - 14 + ([int][Math]::Ceiling($size / 32.0) * 4 * $size))
                [Array]::Copy($bmp, 14, $payload, 0, $bmp.Length - 14)
                [BitConverter]::GetBytes([int]($size * 2)).CopyTo($payload, 8)
                $layers += ,$payload
                $stream.Dispose()
            } finally { $bitmap.Dispose() }
        }
        $output = [IO.File]::Create((Join-Path $assets 'ClaudeGPT.ico'))
        $writer = New-Object IO.BinaryWriter $output
        try {
            $writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$sizes.Count)
            $offset = 6 + 16 * $sizes.Count
            for ($index = 0; $index -lt $sizes.Count; $index++) {
                $dimension = $sizes[$index] % 256
                $writer.Write([byte]$dimension); $writer.Write([byte]$dimension)
                $writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]32)
                $writer.Write([uint32]$layers[$index].Length)
                $writer.Write([uint32]$offset)
                $offset += $layers[$index].Length
            }
            foreach ($layer in $layers) { $writer.Write([byte[]]$layer) }
        } finally { $writer.Dispose() }
    } finally { $source.Dispose() }
} finally {
    # Only this randomly generated temporary directory is removed.
    $resolvedScratch = [IO.Path]::GetFullPath($scratch)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $resolvedScratch.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($resolvedScratch) -notmatch '^[0-9a-f-]{36}$') { throw 'Unsafe temporary cleanup path' }
    Remove-Item -LiteralPath $resolvedScratch -Recurse -Force
}
