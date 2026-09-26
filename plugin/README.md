# The Glass plugin

This directory is the Volumio plugin that ships the `glass` display. It is packaged by `scripts/package.sh` into the zip Volumio installs.

What is here:

- `index.js`: the plugin. It keeps the audio path up (the ALSA chain with the tap, the MPD side output, Spotify, AirPlay and DSP handling), starts the display after the screensaver timeout while music plays, keeps it up through a pause for the persist time, stops it when the run flag goes, serves the display the player's state over a local socket and takes its commands, and serves the settings page and the artist fanart cascade. The audio path, the fanart cascade and the settings backups are carried over from PeppyMeter Screensaver (MIT).
- `run_glass.sh`: the launcher. It sets up the X display and starts the `glass` binary for the machine's architecture with `GLASS_HOME` pointing at the plugin and `GLASS_CHANNEL` naming the socket.
- `config/meter.txt.tmpl` and `config/spectrum.txt.tmpl`: the configurations the display reads, copied to `config/*.txt` on the first install and kept across upgrades.
- `Glass.postGlass.5.conf.tmpl` and the x64 variant: the ALSA contribution, written into `asound/` at runtime. The tap heads it as a PCM plugin every source's stream passes through byte for byte, Spotify's and AirPlay's own PCMs are the same tap, and the side outputs of earlier releases remain as names that play nowhere.
- `mpd_custom.conf`: the MPD side output of earlier releases, kept so an existing player configuration still loads; it stays disabled.
- `templates` and `templates_spectrum`: the bundled default themes from PeppyMeter and PeppySpectrum, moved into `/data/INTERNAL/glass` on install.
- `fonts` and `format-icons`: the clock font, the fallback face for scripts the theme fonts lack, and the player's icon set.
- `install.sh` and `uninstall.sh`: the install steps Volumio runs as root.
- `UIConfig.json`, `i18n`, `config.json`: the settings page, its strings, and the defaults.

The packaging step adds `bin/<arch>/glass`, `bin/<arch>/tapdump` and `lib/<arch>/libglasstap.so` from the ship script's output, and `node_modules`.

Glass replaces PeppyMeter Screensaver. The installer refuses to run while that plugin is enabled; the plugin refuses to start while it is enabled, disables itself so the two are never both enabled, and offers to disable that plugin and start instead; at boot Glass starts after it, so when both are found enabled PeppyMeter Screensaver's transition release steps aside and Glass runs. Themes and settings are taken over: copied from an installed PeppyMeter Screensaver, or adopted in place when it was uninstalled with its themes preserved.
