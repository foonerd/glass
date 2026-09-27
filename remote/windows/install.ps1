# Install Glass as a remote display for this user, from an unpacked release
# archive: glass.exe and SDL2.dll into the user's programs folder, and two
# Start menu entries, the display and its settings page. With -Startup the
# display also starts with the session, for a screen on the wall.
#
#   powershell -ExecutionPolicy Bypass -File remote\windows\install.ps1 [-Startup]
#
# Nothing is written outside the user's own folders; no administrator
# rights are needed. -Check only says where the files would come from.
param([switch]$Startup, [switch]$Check)
$ErrorActionPreference = 'Stop'

# The release archive holds the display under bin; a checkout that ran
# scripts/ship-windows.sh holds it under bin/windows-x64. Paths are written
# with forward slashes, which Windows takes as well, so the script also
# runs under PowerShell on Linux for the project's checks.
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = [IO.Path]::GetFullPath((Join-Path $here '../..'))
$source = $null
foreach ($dir in (Join-Path $root 'bin'), (Join-Path $root 'bin/windows-x64')) {
    if ((Test-Path (Join-Path $dir 'glass.exe')) -and (Test-Path (Join-Path $dir 'SDL2.dll'))) { $source = $dir; break }
}
if (-not $source) {
    Write-Error "install.ps1: glass.exe and SDL2.dll were not found under $root/bin (is this the Windows archive, unpacked whole?)"
}
if ($Check) { Write-Output "install.ps1: would install from $source"; exit 0 }

$programs = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Programs/Glass Remote'))
New-Item -ItemType Directory -Force -Path $programs | Out-Null
Copy-Item (Join-Path $source 'glass.exe') $programs -Force
Copy-Item (Join-Path $source 'SDL2.dll') $programs -Force
$exe = Join-Path $programs 'glass.exe'
Write-Output "installed: $exe"

$shell = New-Object -ComObject WScript.Shell
function Shortcut([string]$path, [string]$arguments, [string]$description) {
    $path = [IO.Path]::GetFullPath($path)
    $link = $shell.CreateShortcut($path)
    $link.TargetPath = $exe
    $link.Arguments = $arguments
    $link.WorkingDirectory = $programs
    $link.Description = $description
    $link.WindowStyle = 7
    $link.Save()
    Write-Output "installed: $path"
}
$menu = Join-Path $env:APPDATA 'Microsoft/Windows/Start Menu/Programs'
Shortcut (Join-Path $menu 'Glass Remote.lnk') '--remote' 'A Volumio player''s meters on this screen'
Shortcut (Join-Path $menu 'Glass Remote Settings.lnk') '--remote --settings' 'The remote display''s settings page'
if ($Startup) {
    Shortcut (Join-Path $menu 'Startup/Glass Remote.lnk') '--remote' 'A Volumio player''s meters on this screen'
}
Write-Output "The first start shows the settings page address on the screen; Glass Remote Settings opens it."
