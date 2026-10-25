[CmdletBinding()]
param(
    [string]$ExecutablePath = (Join-Path $PSScriptRoot 'claude-code-proxy.exe'),
    [string]$IconPath = (Join-Path $PSScriptRoot 'ClaudeGPT.ico'),
    [string]$ClaudePath,
    [string]$ShortcutDirectory = [Environment]::GetFolderPath('Desktop'),
    [string]$WorkingDirectory = $env:USERPROFILE
)

$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path -LiteralPath $ExecutablePath).ProviderPath
$icon = (Resolve-Path -LiteralPath $IconPath).ProviderPath
if (-not $ClaudePath) {
    $nativeClaude = Join-Path $env:USERPROFILE '.local\bin\claude.exe'
    if (Test-Path -LiteralPath $nativeClaude -PathType Leaf) {
        $ClaudePath = $nativeClaude
    } else {
        $ClaudePath = (Get-Command claude.exe -CommandType Application -ErrorAction Stop).Source
    }
}
$claude = (Resolve-Path -LiteralPath $ClaudePath).ProviderPath
if (-not (Test-Path -LiteralPath $ShortcutDirectory -PathType Container)) {
    throw 'Shortcut directory must already exist.'
}
if (-not (Test-Path -LiteralPath $WorkingDirectory -PathType Container)) {
    throw 'Working directory must already exist.'
}
$shell = New-Object -ComObject WScript.Shell
$shortcutPath = Join-Path $ShortcutDirectory 'ClaudeGPT.lnk'
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $exe
$shortcut.Arguments = 'claude --claude-bin "' + $claude + '"'
$shortcut.WorkingDirectory = (Resolve-Path -LiteralPath $WorkingDirectory).ProviderPath
$shortcut.IconLocation = "$icon,0"
$shortcut.Description = 'Claude Code with your local Codex proxy (unofficial)'
$shortcut.WindowStyle = 1
$shortcut.Save()
Write-Output $shortcutPath
