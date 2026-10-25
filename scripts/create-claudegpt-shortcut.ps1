[CmdletBinding()]
param(
    [string]$ExecutablePath = (Join-Path $PSScriptRoot 'claude-code-proxy.exe'),
    [string]$IconPath = (Join-Path $PSScriptRoot 'ClaudeGPT.ico'),
    [string]$ConfigDirectory = (Join-Path $env:LOCALAPPDATA 'claude-code-proxy'),
    [string]$ShortcutDirectory = [Environment]::GetFolderPath('Desktop'),
    [string]$WorkingDirectory = $env:USERPROFILE
)

$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path -LiteralPath $ExecutablePath).ProviderPath
$icon = (Resolve-Path -LiteralPath $IconPath).ProviderPath
if (-not (Test-Path -LiteralPath $ShortcutDirectory -PathType Container)) {
    throw 'Shortcut directory must already exist.'
}
if (-not (Test-Path -LiteralPath $WorkingDirectory -PathType Container)) {
    throw 'Working directory must already exist.'
}
# Encode PowerShell source as UTF-16, avoiding shell interpolation of any paths.
# This contains paths only, never credentials or a client token.
function Quote-Literal([string]$Value) { "'" + $Value.Replace("'", "''") + "'" }
$command = '$env:CCP_CONFIG_DIR = ' + (Quote-Literal $ConfigDirectory) +
    '; & ' + (Quote-Literal $exe) + ' claude; if ($LASTEXITCODE -ne 0) { Read-Host ''ClaudeGPT exited with an error. Press Enter to close'' }'
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
$shell = New-Object -ComObject WScript.Shell
$shortcutPath = Join-Path $ShortcutDirectory 'ClaudeGPT.lnk'
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$shortcut.Arguments = '-NoLogo -NoProfile -EncodedCommand ' + $encoded
$shortcut.WorkingDirectory = (Resolve-Path -LiteralPath $WorkingDirectory).ProviderPath
$shortcut.IconLocation = "$icon,0"
$shortcut.Description = 'Claude Code with your local Codex proxy (unofficial)'
$shortcut.WindowStyle = 1
$shortcut.Save()
Write-Output $shortcutPath
