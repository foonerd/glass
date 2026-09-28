# Glass as a remote display

A remote display is the same `glass` binary run on another machine with `--remote`: it shows a player's meters with the player's theme, fonts and icons, fed with the player's measurements over the network. The wiki's Remotes page describes the wire and the settings; this directory holds what installs a remote.

## Linux

Unpack the release archive for the machine's architecture (`x64` for a PC, `armv8` for a 64-bit Raspberry Pi OS, `armv7` for a 32-bit one) and run the installer as the user who will run the display:

```sh
tar xzf glass-<version>-<arch>.tar.gz
glass-<version>-<arch>/remote/linux/install.sh
```

It puts `glass` in `~/.local/bin` and two entries in the applications menu: **Glass Remote**, the display, and **Glass Remote Settings**, which opens the display's settings page in a browser (starting the display when it is not running). SDL2 is needed: `sudo apt install libsdl2-2.0-0`.

`install.sh --service` also installs a user service that starts the display with the session and keeps it running, for a screen on the wall. `sudo loginctl enable-linger $USER` starts it before anyone logs in. `uninstall.sh` takes everything out again; `--purge` also removes the configuration and the cache.

## Windows

Unpack `glass-<version>-windows-x64.zip` and run the installer in a PowerShell window, as the user who will run the display:

```powershell
powershell -ExecutionPolicy Bypass -File glass-<version>-windows-x64\remote\windows\install.ps1
```

It puts `glass.exe` and `SDL2.dll` under the user's programs folder (`%LOCALAPPDATA%\Programs\Glass Remote`) and two entries in the Start menu: **Glass Remote**, the display, and **Glass Remote Settings**, which opens the display's settings page in a browser. `install.ps1 -Startup` also starts the display with the session. `uninstall.ps1` takes everything out again; `-Purge` also removes the configuration and the cache. Nothing needs administrator rights, and nothing else needs installing: SDL2 comes in the archive. The binary runs on Windows 10 or later, 64 bit. It opens no console window; started from a terminal, its lines go to that terminal. Allow `glass` when Windows Defender Firewall asks, so players can announce themselves. The binary and the installer scripts in a release are signed, so SmartScreen and Smart App Control let them run; a build made by hand is not, and SmartScreen's "Run anyway" then applies. The binary can also be run straight from the unpacked folder: `glass.exe --remote`.

The configuration is `%APPDATA%\glass-remote\config.json`; what is brought from players is under `%LOCALAPPDATA%\glass-remote`.

## Android

Download `glass-<version>-android.apk` from the release to the phone or tablet, open it, and allow the install when Android asks about apps from this source (the app is not in the Play Store). Open **Glass Remote**: the first start shows the settings page's address on the screen. Open that page in the phone's own browser at `http://127.0.0.1:5583/`, or from any machine on the network at the address shown, add the player, and the meters appear. The app runs landscape, keeps the screen on, and asks for no permission beyond the network. Its configuration and what it brings from players live in its own storage; its lines are in the system log under the tag `glass` (`adb logcat -s glass`). Android 7 or later, 64 bit arm, 32 bit arm or x86_64.

## The settings page

The first start shows a page address on the screen; the page adds a player (found on the network or typed), chooses the theme (the player's; one of the player's themes kept as this remote's own with its own meter rotation; or a theme from a folder on this machine or a share mounted here), the window (full screen, a window, or a frameless one at a fixed place, and which screen), the frame rate, the gain and a spectrum decay, the log level, and downloads or uploads the configuration as a file. Changes apply while the display runs. On the keyboard, Escape leaves full screen so the page can be used on the same machine, F takes the screen back, Q quits. A tap on one of the theme's controls acts on the player when the player has its interactive controls on. On Linux the configuration is `~/.config/glass-remote/config.json`; what is brought from players is under `~/.cache/glass-remote`. For a device with nothing installed, a phone or a tablet, Anymote is the same face in the browser, served by the player's manager at `/anymote`; the wiki's Anymote page has it.
