# Install Glass as a remote display for this user, from an unpacked release
# archive: glass.exe and SDL2.dll into the user's programs folder, and two
# Start menu entries, the display and its settings page. With -Startup the
# display also starts with the session, for a screen on the wall.
#
#   powershell -ExecutionPolicy Bypass -File remote\windows\install.ps1 [-Startup]
#
# Nothing is written outside the user's own folders; no administrator
# rights are needed.
param([switch]$Startup)
$ErrorActionPreference = 'Stop'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Resolve-Path (Join-Path $here '..\..')
$source = Join-Path $root 'bin\windows-x64'
foreach ($file in 'glass.exe', 'SDL2.dll') {
    if (-not (Test-Path (Join-Path $source $file))) {
        Write-Error "install.ps1: $file is missing under $source (this archive is not the Windows one)"
    }
}

$programs = Join-Path $env:LOCALAPPDATA 'Programs\Glass Remote'
New-Item -ItemType Directory -Force -Path $programs | Out-Null
Copy-Item (Join-Path $source 'glass.exe') $programs -Force
Copy-Item (Join-Path $source 'SDL2.dll') $programs -Force
$exe = Join-Path $programs 'glass.exe'
Write-Output "installed: $exe"

$shell = New-Object -ComObject WScript.Shell
function Shortcut([string]$path, [string]$arguments, [string]$description) {
    $link = $shell.CreateShortcut($path)
    $link.TargetPath = $exe
    $link.Arguments = $arguments
    $link.WorkingDirectory = $programs
    $link.Description = $description
    $link.WindowStyle = 7
    $link.Save()
    Write-Output "installed: $path"
}
$menu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
Shortcut (Join-Path $menu 'Glass Remote.lnk') '--remote' 'A Volumio player''s meters on this screen'
Shortcut (Join-Path $menu 'Glass Remote Settings.lnk') '--remote --settings' 'The remote display''s settings page'
if ($Startup) {
    $startup = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Startup'
    Shortcut (Join-Path $startup 'Glass Remote.lnk') '--remote' 'A Volumio player''s meters on this screen'
}
Write-Output "The first start shows the settings page address on the screen; Glass Remote Settings opens it."
