# Take out what install.ps1 put in: the Start menu entries, the startup
# entry, and the program folder. With -Purge also the configuration
# (%APPDATA%\glass-remote) and what was brought from players
# (%LOCALAPPDATA%\glass-remote).
#
#   powershell -ExecutionPolicy Bypass -File remote\windows\uninstall.ps1 [-Purge]
param([switch]$Purge)
$ErrorActionPreference = 'Continue'

Get-Process glass -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "*Glass Remote*" } | Stop-Process -Force
$menu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
foreach ($link in (Join-Path $menu 'Glass Remote.lnk'), (Join-Path $menu 'Glass Remote Settings.lnk'), (Join-Path $menu 'Startup\Glass Remote.lnk')) {
    if (Test-Path $link) { Remove-Item $link -Force; Write-Output "removed: $link" }
}
$programs = Join-Path $env:LOCALAPPDATA 'Programs\Glass Remote'
if (Test-Path $programs) { Remove-Item $programs -Recurse -Force; Write-Output "removed: $programs" }
if ($Purge) {
    foreach ($dir in (Join-Path $env:APPDATA 'glass-remote'), (Join-Path $env:LOCALAPPDATA 'glass-remote')) {
        if (Test-Path $dir) { Remove-Item $dir -Recurse -Force; Write-Output "removed: $dir" }
    }
}
