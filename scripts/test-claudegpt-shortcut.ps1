$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$scratch = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
    # Apostrophes, spaces and shell metacharacters must remain literal paths.
    $exe = Join-Path $scratch "proxy's `$test.exe"
    [IO.File]::WriteAllText($exe, 'synthetic placeholder; never executed')
    $icon = Join-Path $root 'assets/windows/ClaudeGPT.ico'
    $profile = Join-Path $scratch "profile's `$test"
    $installer = Join-Path $PSScriptRoot 'create-claudegpt-shortcut.ps1'
    & $installer -ExecutablePath $exe -ClaudePath $exe -IconPath $icon -ShortcutDirectory $scratch -WorkingDirectory $scratch | Out-Null
    & $installer -ExecutablePath $exe -ClaudePath $exe -IconPath $icon -ShortcutDirectory $scratch -WorkingDirectory $scratch | Out-Null
    if (@(Get-ChildItem -LiteralPath $scratch -Filter '*.lnk').Count -ne 1) { throw 'Duplicate shortcuts' }
    $link = (New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path $scratch 'ClaudeGPT.lnk'))
    if ($link.IconLocation -ne "$icon,0") { throw 'Wrong icon path' }
    if ($link.WorkingDirectory -ne $scratch) { throw 'Wrong working directory' }
    if ($link.TargetPath -ne $exe -or $link.Arguments -ne ('claude --claude-bin "' + $exe + '"')) { throw 'Shortcut must target the executable directly with an explicit Claude path' }
    if (Test-Path -LiteralPath $profile) { throw 'Installer touched the credential profile' }
    Add-Type -AssemblyName System.Drawing
    $image = [Drawing.Icon]::new($icon, 48, 48)
    try { if ($image.Width -ne 48) { throw 'Invalid Windows icon' } } finally { $image.Dispose() }
    $bytes = [IO.File]::ReadAllBytes($icon)
    if ([BitConverter]::ToUInt16($bytes, 4) -ne 7) { throw 'Missing icon sizes' }
    Write-Output 'ClaudeGPT shortcut checks passed (no provider or Claude processes started).'
} finally {
    $resolvedScratch = [IO.Path]::GetFullPath($scratch)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $resolvedScratch.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($resolvedScratch) -notmatch '^[0-9a-f-]{36}$') { throw 'Unsafe temporary cleanup path' }
    Remove-Item -LiteralPath $resolvedScratch -Recurse -Force
}
