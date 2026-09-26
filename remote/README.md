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

The first start shows a page address on the screen; the page adds a player (found on the network or typed), chooses the theme (the player's, or one of the player's themes kept as this remote's own with its own meter rotation), the window (full screen, a window, or a frameless one at a fixed place), the frame rate and the gain. Changes apply while the display runs. The configuration is `~/.config/glass-remote/config.json`; what is brought from players is under `~/.cache/glass-remote`.
