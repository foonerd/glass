# Install Glass as a remote display for this user, from an unpacked release
# archive: glass.exe and SDL2.dll into the user's programs folder, and two
# Start menu entries, the display and its settings page. With -Startup the
# display also starts with the session, for a screen on the wall.
#
# A remote comes in two flavours, and this installer installs whichever its
# archive holds, in the same place under the same name, so one takes the
# other's place and the settings and the Start menu entries stay: the
# standalone (Glass's own archive, the player's theme and nothing over it)
# and the bundle (glass-evo's archive, the same display with the Glass
# interface in it: a clock when the player stands still and a bar of
# controls, which need a mouse or a touch screen).
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
# The display this archive holds: glass-evo's where it is the bundle, else
# Glass's own.
$source = $null
$display = $null
foreach ($name in 'glass-evo.exe', 'glass.exe') {
    foreach ($dir in (Join-Path $root 'bin'), (Join-Path $root 'bin/windows-x64')) {
        if ((Test-Path (Join-Path $dir $name)) -and (Test-Path (Join-Path $dir 'SDL2.dll'))) { $source = $dir; $display = $name; break }
    }
    if ($source) { break }
}
if (-not $source) {
    Write-Error "install.ps1: glass.exe and SDL2.dll were not found under $root/bin (is this the Windows archive, unpacked whole?)"
}
$bundle = $display -eq 'glass-evo.exe'
if ($Check) { Write-Output "install.ps1: would install from $source ($(if ($bundle) { 'the bundle' } else { 'the standalone remote' }))"; exit 0 }

$programs = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Programs/Glass Remote'))
New-Item -ItemType Directory -Force -Path $programs | Out-Null
Copy-Item (Join-Path $source $display) (Join-Path $programs 'glass.exe') -Force
Copy-Item (Join-Path $source 'SDL2.dll') $programs -Force
# Files unpacked from a download carry the mark of the web; without it
# SmartScreen does not ask about a program it has not seen before.
Get-ChildItem $programs | Unblock-File -ErrorAction SilentlyContinue
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
    $link.IconLocation = "$exe,0"
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
if ($bundle) {
    Write-Output "This is the bundle: the display with the Glass interface in it. It shows the clock and the bar of controls where the player's own screen shows them; the settings page has it otherwise (always, or never). The controls need a mouse or a touch screen."
    Write-Output "For the theme alone, install Glass's own archive in its place: https://github.com/foonerd/glass/releases"
} else {
    Write-Output "This is the standalone remote: the player's theme and nothing over it."
    Write-Output "For the Glass interface on this remote (a clock when the player stands still, a bar of controls), install the bundle in its place, from glass-evo's releases: https://github.com/foonerd/glass-evo/releases"
}
Write-Output "When Windows Defender Firewall asks whether glass may accept connections on private networks, allow it: that is how players announce themselves."
