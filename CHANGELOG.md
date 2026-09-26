# Changelog

All notable changes to Glass are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.6.4] - 2026-09-26

A settings backup downloads from the manager as a zip, and a zip uploads as a backup: its manifest and files are checked the way a restore checks them before it lands, under its own name or a numbered one when taken. Settings move from one player to another this way.

## [0.6.3] - 2026-09-26

The restart after an upgrade is handed to systemd as one restart job, which it carries out on its own. The stop-then-start of 0.6.1 and 0.6.2 was issued from inside the player's service, and the stop took the process waiting to start it again with it, so the backend stayed down until started by hand. A player that upgraded to 0.6.2 that way needs one `volumio vstart`; from 0.6.3 on the backend comes back by itself. The kept-plugin zip writer no longer leaves an error listener behind per drained write.

## [0.6.2] - 2026-09-26

The manager looks for a new release on its own, a minute after it starts and once a day after, and says so in its header on every tab; the header's notice opens the status tab where the upgrade is. A job left in `restarting` for minutes no longer blocks another upgrade.

## [0.6.1] - 2026-09-26

Glass upgrades itself from the manager's status tab. The latest release is read from GitHub, at most once a day unless asked; when it is newer, one press downloads its plugin zip, checks it against the digest the release carries, writes a settings backup, keeps the installed plugin as a zip for going back, snapshots the meter and spectrum configurations, hands the zip to the player's own plugin manager (which replaces the plugin and runs its install script, keeping the settings), puts the configurations back, and restarts the player's backend so the new code loads. The page follows the job, waits for the new version to answer, and reloads. The kept version goes back the same way. `node --test` covers the version order, the release parsing and the zip writer.

## [0.6.0] - 2026-09-26

The Glass Manager: a web application the plugin serves on its own port (5582, `managerPort`) for everything that manages the display rather than operates it, opened from the settings page in the same window or a tab of its own. Installed themes are shown with previews the display draws itself, every meter of a theme rendered headless and kept until the theme's meters file or the plugin changes; a theme is put on show, its meter rotation set (all at random, a list, or one, on a timer or with the title), or removed. The theme catalog from peppy_templates is fetched with its ETag and browsed by size, kind and state; an install downloads the zip, checks its size and SHA-256 against the catalog, unpacks the folders the catalog names into the meter and spectrum trees through a staging directory that replaces a folder whole, and remembers the checksum so a changed zip shows as an update. A theme zip uploaded from the browser installs the same way, in any of the layouts the catalog holds. The artist fanart settings, the settings backups and a status page (display, player, channel, audio rings, catalog, storage) are the manager's too. The settings page keeps what operates the display and no longer holds the fanart controls, the remove control or the backup sections. The plugin reads zips itself, with no unzip in the path. Restoring a settings backup works again: since the share-access option left, the restore called a helper that no longer existed and every restore failed. `glass --thumb WIDTH` writes a thumbnail beside each snapshot.

## [0.5.13] - 2026-09-26

The tap asks its PCM how many frames stand between the last one written and the sound, from its measuring thread, and places every hop from that; the buffer-and-period estimate remains the fallback when the PCM cannot say. A player that keeps its buffer anywhere between a period and full, as one reading from a pipe does, now meters in step with the sound rather than within a period of it.

## [0.5.12] - 2026-09-26

Hops keep their cadence. When a player writes ahead of the sound, as one does while it fills its buffer or reads from a pipe, each hop now falls due no sooner than one hop after the last, so the ring fills evenly instead of in pairs; and a stream's first frames are timed from an empty buffer, so the meters start with the sound instead of a buffer's length behind it.

## [0.5.11] - 2026-09-26

The meters move as the audio does again. A player that writes a whole period at a time (MPD for local files, NAS and DLNA, the radio's aplay) handed the tap five or six hops in a burst every eighth of a second, and the display, reading the latest hop each frame, saw only one of them; with a half-second buffer the hops also ran ahead of what was heard. The tap's measuring thread now places every hop in time, from when its transfer arrived and the buffer and period the player asked for, and puts it into the ring when its audio plays. One-bit audio is measured a hop at a time too. The relay holds two thirds of a second of stereo at 384 kHz.

## [0.5.10] - 2026-09-26

The plugin sheds what the tap made redundant. The audio selection (modular alsa or DSD native) and the Spotify, Soloist, USB DAC, AirPlay and DSP switches are gone from the settings page and the configuration: every source meters through the tap on the main path, bit-perfect. The MPD side output and its include, the copies mounted over MPD's and AirPlay's configuration templates, Spotify's own PCM and Soloist's metering device are taken back once on the first start after the upgrade, so Spotify, AirPlay and Soloist play to `volumio` again, and on uninstall. The Dummy and Loopback cards are no longer loaded, and the display starts for every service that plays. The ALSA template no longer varies with the settings.

## [0.5.9] - 2026-09-26

Meters for every source, bit-perfect. The tap is an ALSA PCM plugin now (`type glasstap`), at the head of the Glass section for every source: the stream passes through it byte for byte in the format the player and the output agreed, so PCM, DoP and native DSD stay bit-perfect, and it is measured on the way, linear audio for peak, RMS and spectrum, one-bit audio by density. The audio thread only copies and hands the samples across a lock-free relay; a thread of the tap's own measures and publishes. FM and DAB, Tidal Connect, Bluetooth and anything else that plays through `volumio` meter as MPD, Spotify and AirPlay did, in both audio selections and on x64 as on the Pi; the MPD side output is off and the loopback card unused. `scripts/tap_exact.sh` plays raw frames of each format through the tap into a file and proves the bytes; CI runs it. Ring files carry a writer number after the process id.

## [0.5.8] - 2026-09-26

The guard holds. When Glass refuses to start because PeppyMeter Screensaver is enabled, it disables itself, so the two are never both enabled at the next start and the ALSA chain is built without Glass until the switch is made. Glass starts after PeppyMeter Screensaver at boot, so when both are found enabled the transition release steps aside and Glass runs. The guard's messages name PeppyMeter Screensaver where they said Glass, and the German and French files carry their own words where they carried English. PeppyMeter Screensaver 3.5.0, its transition release, mirrors the guard.

## [0.5.7] - 2026-09-26

The channel. The plugin serves a local socket with the player's state and the infinity playback flag, pushed as they change, and takes commands for the player back. The display reads its state from the channel when the plugin runs it, so a track change shows at once and the player is no longer asked once a second; without a channel it asks the player as before. Infinity playback shows on the repeat indicator's fourth state, or on the shuffle indicator's third. The wiki's Contracts page has the lines.

## [0.5.6] - 2026-09-26

The cross builds survive a restored CI cache, which keeps the links of the unpacked sysroot and drops the files behind them: the ship script looks for each target's SDL and ALSA library at its own path and unpacks it again when the file is gone, and the tap's build script runs again when the library it links appears, changes or goes.

## [0.5.5] - 2026-09-26

The chain names the tap. The ALSA contribution puts `glasstap` on the meter PCMs, the display reads the tap's ring instead of the two pipes, and nothing of peppyalsa is left: no library in the package, no pipes made or held by the plugin, no peppy names in the PCMs or the MPD side output, which is `mpd_glass` now and is rewritten on the first start after this release. The meter keeps its fall and the spectrum its bins and smoothing, made in the display from the raw measurements. `tapdump` stays as the way to look at the tap.

## [0.5.4] - 2026-09-26

The release and CI workflows check out the peppyalsa builds the plugin still carries, run the scripts as executables, and use the actions' Node 24 releases; the packaging refuses to make a zip without the ALSA scope for an architecture.

## [0.5.3] - 2026-09-26

The cross builds in CI get the C library headers of each target, which the TLS dependency's C sources need.

## [0.5.2] - 2026-09-26

The workshop. The toolchain is pinned in `rust-toolchain.toml`, the workspace shares one set of lints, the code is formatted by rustfmt and clean under clippy with warnings denied, and `scripts/check.sh` runs formatting, lints, tests and documentation the way CI does. GitHub Actions runs that check and the cross builds on every push, and publishes the plugin zip and per-architecture archives of the display and the tap on every version tag, with the changelog section as the notes. `scripts/package.sh` uses npm when the machine has it and a container otherwise.

## [0.5.1] - 2026-09-26

The audio tap. `glasstap` is an ALSA scope of Glass's own: loaded by the audio player's ALSA chain in place of peppyalsa, it measures the stream a hop at a time, the peak and RMS of each channel and the magnitude spectrum of each channel through a 2048-point window by default, and publishes them into a shared ring under `/dev/shm`, one file per writing process, which the display reads at its own frame rate without anything blocking the player. DSD over PCM is measured by bit density as before. When the configuration names them, the tap also writes the two FIFOs peppyalsa wrote, so both can run side by side; in this release the chain still names peppyalsa and the tap ships beside it. `tapdump` prints what the live ring says. The `tap` crate holds the ring contract, the measurements and their tests.

The display binaries, the tap library and `tapdump` are no longer committed; `scripts/ship.sh` builds them and the plugin zip carries them.

## [0.5.0] - 2026-09-26

Glass is a Volumio plugin. The `plugin` directory holds it and `scripts/package.sh` builds the zip Volumio installs, with the display binaries, the ALSA scope and the node modules. The plugin keeps the audio path up, starts the display after the screensaver timeout while music plays, keeps it up through a pause for the persist time, and serves the settings page, the artist fanart cascade and the settings backups. Glass replaces PeppyMeter Screensaver: the installer refuses while that plugin is enabled, the plugin refuses to start while it is enabled and offers to disable it, and themes and settings are taken over from an installed or a preserved PeppyMeter Screensaver. The bundled themes are the PeppyMeter and PeppySpectrum defaults.

The display reads its configuration from the plugin's home, `GLASS_HOME`, by default `/data/plugins/user_interface/glass`: `config/meter.txt` and `config/spectrum.txt`, with `fonts` and `format-icons` beside them. The run contract's files are `/tmp/glass_running`, `/tmp/glass_dismiss` and `/tmp/glass_persist`, the dismiss marker's variable `GLASS_DISMISS_FILE`, the fade lock `glass_fade_lock`, and the fanart endpoint `glass_artistfanart`.

## [0.4.28] - 2026-09-25

Less memory. Font files are mapped rather than read, so a face costs the pages its glyphs touch and a file named by several styles is mapped once; a turned picture not shown for ten seconds is let go and the turned pictures share 16 MB; the meter foreground keeps only the pixels of its opaque spans. On a Pi 5 the Thorens turntable's resident set went from 125 MB to 87 MB. With `GLASS_PROFILE=1` Glass says what each store holds.

## [0.4.27] - 2026-09-25

Only what changed is painted. A frame's drawing steps are keyed, the boxes of the steps that differ from the last frame's are the only part of the canvas repainted, and only those boxes are uploaded to the window's texture; a frame with nothing changed is not uploaded at all. The result is pixel for pixel the frame a whole repaint gives. A turning picture's box is measured from where it has pixels, so a round record or reel in a square picture claims a box its own size. The profile line says how much of the frame was painted and in how many boxes.

## [0.4.26] - 2026-09-25

A frame is planned, then painted. The drawing steps of a frame are laid out first as read-only operations, and the canvas is painted in row bands that several threads share. By default a frame takes from one thread up to one a core as it needs: painting that overruns its share of the period gets another thread, painting that would fit in half the period with one fewer gives it back. `--threads N` fixes the count. Lines set in type for the type area and the gauges are kept between frames. The turned pictures kept for needles and the tonearm share a byte budget. The screen and face pictures are dropped once the base is composed. Glass says which SDL renderer draws the window at start.

Fixed: the glow of a picture indicator was composed onto black and darkened its surroundings.

## [0.4.25] - 2026-09-25

Fixed: a picture kept turned (a needle or the tonearm) keeps the alpha of its soft edges. The kept copy was composed onto black, so a tonearm with a translucent glow or shadow drew a black surface around the arm since 0.4.23, and needles with soft edges carried a dark fringe since 0.4.20.

## [0.4.24] - 2026-09-25

Fixed: the spectrum theme is taken from the folder that carries the meter theme's own name, falling back to the spectrum configuration's `spectrum.folder`. A meter shown through `--theme`, or before the player has mirrored a theme change into the spectrum configuration, drew no spectrum.

## [0.4.23] - 2026-09-25

Turning pictures cost less. A turned picture visits only the span each frame row covers; records, reels, turning art and knobs take the nearest texel in fixed point, as the engine's plain rotation does, while needles and the tonearm keep bilinear sampling; the tonearm is kept turned per half degree like the needles. On the Pi 5 the Pioneer Gold turntable went from 55 to 25 percent of one core at 30 frames a second and holds 60 frames a second at 41 percent. `--fps N` stands in for `frame.rate` for one run.

## [0.4.22] - 2026-09-25

Theme review. `--list` prints every installed theme with its meters. `--theme FOLDER`, `--meter NAME|random|a,b,c` and `--interval SECONDS` stand in for the installed configuration's theme, meter and rotation without changing the player's files. `--snapshot DIR` shows each meter of the theme, or of the `--meter` list, for `--settle` seconds (default 4) and writes `DIR/<theme>/<meter>.png` before moving on, with or without a window. `--output` writes PNG when the name ends in `.png`.

## [0.4.21] - 2026-09-25

The plugin's run contract. When a window is open the player stands up the run flag `/tmp/peppyrunning` and leaves, fading out when it faded in, within half a second of the plugin removing it, taking the flag down as it goes. With `exit.on.touch` or `stop.display.on.touch`, a lifted finger or mouse button ends the player, writing the marker named in `PEPPY_USER_DISMISS_FILE` first when the run flag still stands, so the plugin re-arms its timeout instead of restarting. `position.type` other than `center` puts the frame's top left at `position.x`, `position.y`; the pointer is hidden. `plugin/run_glass.sh` names the dismiss marker as the engine's launcher does.

## [0.4.20] - 2026-09-25

Performance. The screen picture and meter face are composed once per meter and copied into a frame buffer that is kept between frames; rendered text lines are kept per position until the text, style, size, colour or font changes; needle sprites are kept turned per half degree, so a holding or slowly moving needle costs one plain blit; the meter foreground blends only the opaque span of each row; the window keeps one streaming texture instead of making one per frame. On a Raspberry Pi 5 with a 32-bit system at 1280 by 720 and 30 frames a second, the player's CPU share fell from about 36 percent to about 10 percent of one core. `GLASS_PROFILE=1` prints, every 60 frames, how long the poll, the step, each stage of the raster and the upload take.

## [0.4.19] - 2026-09-25

Start and stop. With `start.animation` the first frame fades in from `transition.color` over `transition.duration` seconds at `transition.opacity`, unless `transition.type` is `none`; every later meter of a rotation fades in the same way; a fade lock file shared with the player's engine keeps two starts within the fade time from fading twice. When the window closes after a fade in, the frame fades out before the player exits. After every start the levels and spectrum bars rise to their values over 0.7 seconds in ten steps, as the engine raises its full scale.

## [0.4.18] - 2026-09-25

Indicators, under `config.extend`. `mute.*`, `shuffle.*`, `repeat.*` and `playstate.*` show a state as an LED (`*.led` size, `*.led.shape` circle or rect, `*.led.color` one triple per state) or as one picture per state (`*.icon`), with an optional glow (`*.led.glow` or `*.icon.glow` radius, `.intensity`, `.color` per state) blurred behind them; a state past the look's last takes the last. Mute is off, muted, or zero volume; shuffle follows random; repeat is off, all, single, with a fourth state for infinity when the look has one; play state is stop, pause, play. `volume.*` and `progress.*` are gauges in the styles `numeric`, `slider` (a rounded bar, or a picture tip on a track with an optional fill tail), `knob` (a picture turned between two angles) and `arc` (a solid ring sector), with markers (`progress.marker.N.*`, picture or label at a percentage) and a head picture that moves with the value. The progress bar draws `progress.border` in `progress.border.color`. The player's controls (volume, mute, random, repeat, repeat single) come with the state.

## [0.4.17] - 2026-09-25

Cassettes. `reel.left.*` and `reel.right.*` (a theme picture or `album,theme` to prefer a file from the track's folder, scaled to the theme reel's size, with `center`) turn at `reel.rotation.speed` while the player plays or a stop is only a transition, in `reel.direction` (the meter's, else the player's, default counter-clockwise). Under `rotation.quality = custom` the player's `spool.left.speed` and `spool.right.speed` scale each reel. With `spool.adaptive` (the meter's or the player's) the supply reel speeds up and the take-up reel slows down as the tape runs, from half to one and a half times the speed. `queue.mode = queue` makes the progress that reels and the tonearm follow run over the whole queue, from the player's queue read every ten seconds, unless the source is a stream.

## [0.4.16] - 2026-09-25

Turntables. `vinyl.filename` (a theme picture, or `album,theme` to prefer a file from the track's folder), `vinyl.pos`, `vinyl.center`, `vinyl.dimension` and `vinyl.direction` put a record under the art that turns at `albumart.rotation.speed` while the player plays, keeps turning through a transitional stop and while the tonearm moves, and slows to a halt over the tonearm's lift after playback stops. `albumart.rotation` turns the art with the record on the record's centre, cut to a circle when it has no mask, with a spindle and ring in `font.color` and a round border. `tonearm.*` places an arm that drops onto the record over `tonearm.drop.duration`, follows the track from `tonearm.angle.start` to `tonearm.angle.end`, lifts over `tonearm.lift.duration` when playback stops, a second and a half before the end, or on a jump, and drops again where the track now is. `rotation.quality`, `rotation.fps` and `rotation.speed` from the player pace and scale the turning; a tonearm with a single reel and no record turns the reel as the record. Needles, records and arms all turn through one routine that maps a pivot in the picture onto a point on screen.

## [0.4.15] - 2026-09-25

Artist fanart. A meter with `fanart.pos` and `fanart.dimension` (plus `fanart.scale` and `fanart.zorder`, default `fit` and `background`) shows the pictures the player resolves for the playing artist through its `peppy_screensaver_artistfanart` endpoint, read from the player's own cache when it is there. The set is asked for when the artist changes and again on every track and every timed interval; the show moves one picture on per track and per interval, in order or at random as the player is set, remembering its place per artist and set. `fade` and `merge` transitions run over the player's duration; `none` cuts. Pictures for fanart and folder layers are now decoded off the frame loop, so a large picture never stalls a frame.

## [0.4.14] - 2026-09-25

Folder layers. `folderlayer.1.*` to `folderlayer.5.*`, and the legacy `folderlayer.*` under `folderlayer.enabled`, each with `pos` and `dimension`, show the first of their `files` found in the playing track's folder under `/mnt`, kept in proportion and centred (`scale = fit`) or stretched, with an optional `border` in `font.color`. `zorder = background` draws the picture over the screen picture and meter face and under the needles; `overlay` draws it over the texts and under the meter foreground. The layers follow the track folder, looked up once per folder. The meter face is now composed before the album art, and the meter foreground is drawn last of all, as the meter engine orders them. GIF and WebP pictures decode.

## [0.4.13] - 2026-09-25

Spectrum analyser. A meter with `config.extend` and `spectrum.visible` shows the spectrum `spectrum.name` names in the theme's `spectrum.txt`, in a `spectrum.size` box placed by the section's `spectrum.x/y` and clipped to it. Backgrounds, bars and reflections come from `color`, `gradient` (first colour at the bottom), `image` or `image.extended` fills; the background and `fgr.filename` are centred in the box. Bars rise from `origin.x/y` in `bar.height / steps` pixel steps, a raw bin rounded up to a whole step against `max.value`; reflections hang `reflection.gap` below the baseline; toppings stay a step above a dropping bar and fall `topping.step` pixels a frame. The pipe record length follows the spectrum configuration's `size`.

## [0.4.12] - 2026-09-25

Meter types. A circular meter may have one channel (`channels = 1`, needle at `mono.origin.x/y` for the mono level), per-channel angles (`left.start.angle`, `left.stop.angle`, `right.start.angle`, `right.stop.angle`, the left pair standing in when the shared `start.angle` and `stop.angle` are absent) and a mirrored needle per channel (`left.needle.flip`, `right.needle.flip`). Origins may lie off screen. A linear meter (`meter.type = linear`) shows the indicator picture up to the width its steps reach: `position.regular` steps of `step.width.regular` pixels, then `position.overload` steps of `step.width.overload`; `direction` is `left-right`, `right-left`, `bottom-top`, `top-bottom`, `edges-center` or `center-edges`; `indicator.type = single` moves the whole picture instead; `flip.left.x` and `flip.right.x` mirror a channel's picture; `mono.x/y` places a one-channel bar. `meter.visible = False` now hides bars as well as needles. A frame written with `--output` is renamed into place, never seen half written.

## [0.4.11] - 2026-09-25

Time fields and the data source. `time.remaining`, `time.elapsed` and `time.total` each take a position with an optional style word, a colour (elapsed and total default to the remaining colour), and for the clock style a font of their own through `time.*.font`, found as an absolute path, in the theme folder or under `font.path`, and `time.*.fontsize`. Elapsed and total show `mm:ss` of the position and the length. While the display persists after a pause, the plugin's persist file drives the remaining field: `countdown` counts the persist period down in orange, `freeze` keeps the track time. The pipe levels are conditioned as the meter engine conditions them, from `[data.source]`: full scales, `volume.gain.db` and the live `volume.gain.db.source` file, the stereo algorithm, the smoothing buffer and the mono algorithm.

Meter selection. `meter = random` walks every section of the theme's meters file, each once before starting over; a comma list cycles in order. The next meter comes after `random.meter.interval` seconds, or with the next title when `random.change.title` is set. `meter.visible = False` under `config.extend` hides the needles.

The remaining time counts whole seconds of position before subtracting, as the player does.

## [0.4.10] - 2026-09-25

Text moves as the player moves it. Every title, artist, album and next line has a box: its own `maxwidth`, else `playinfo.maxwidth`, else the width left on screen minus a margin, or six tenths of the screen when `playinfo.align` (or the legacy `playinfo.center`) centres. A text that fits is placed by the alignment; a wider one bounces between its ends with a pause, at the speed the player's `scrolling.mode` selects: 40 everywhere, the player's custom values, or the meter's `playinfo.scrolling.speed.*`. The ticker (`playinfo.ticker.*`) composes one looping line from artist, title, album and the next track, in its direction, with its separator, spacing and end gap, capped to the visible width, and can replace the separate lines. `playinfo.next.title/artist/album.*` show the track after the current one, read from the player's queue. The `italic` style uses `font.italic` with `font.size.italic`, falling back to regular.

## [0.4.9] - 2026-09-25

The type area: `playinfo.type.pos`, `dimension`, `mode` (`icon`, `text`, `both`, the meter's value before the player's `[current]` setting), `align`, `color` and `fontsize`, with the icon resolved from the theme's `format-icons`, then the player's own set, then Volumio's stock set, case-insensitive. SVG icons are rendered with resvg and tinted with the type colour; PNG icons keep their colours. Album art honours `albumart.mask` (white cut away) and `albumart.border` (drawn in `font.color`). The samplerate line falls back to the bitrate when samplerate and bit depth are empty, and takes `playinfo.type.color` before `font.color`.

## [0.4.8] - 2026-09-25

Album art. `intake` reads the art location from the player and fetches the picture in the background, three seconds at most, into a cache under the temp directory; a location that starts with a slash is served by the player itself. `plot` places the picture from `albumart.pos` and `albumart.dimension` once the file exists. `expose` decodes it by content and stretches it to the box, drawn over the screen picture and under the meter face.

## [0.4.7] - 2026-09-25

Theme text is set in the theme's fonts. `lead` reads `font.path` and the `font.*` files from the player configuration, and each meter's `playinfo.title.pos`, `playinfo.artist.pos`, `playinfo.album.pos`, `playinfo.samplerate.pos` and `time.remaining.pos` with their style word, colour, `font.size.*` and `playinfo.maxwidth`. `intake` carries samplerate, bitdepth, status, duration and a seek position that moves on between the once-a-second reads. `plot` composes the title, the artist with the album when the meter has no album line, the samplerate line and the remaining time as `Scene.texts`; the clock turns red in the last ten seconds. `expose` rasterises them with `ab_glyph`, clipping at `playinfo.maxwidth`.

## [0.4.6] - 2026-09-25

The needle is drawn by sampling its sprite bilinearly at every frame pixel inside its turned bounds, and its alpha channel blends over the face. The meter foreground blends the same way. Edges that were stepped are smooth.

## [0.4.5] - 2026-09-25

Now-playing text is read once a second inside `intake` and carried in `Input.metadata`; the player no longer contacts Volumio on every frame. `--record step.json` writes the skin, input and scene of each step, and `plot` replays every recorded frame under `testdata/frames/` as a test.

## [0.4.4] - 2026-09-24

The theme is drawn at its own pixel size on a fullscreen window. Layer order is the screen picture, the meter face, a needle rotated around the theme origin at `distance`, then the meter foreground. Title and artist stay at `playinfo.title.pos` and `playinfo.artist.pos`.

## [0.4.3] - 2026-09-24

The window covers the display. Needles rotate with the meter level. Title and artist come from Volumio while a track is playing.

## [0.4.2] - 2026-09-24

The window is borderless. The unrotated needle image is not drawn. That image was the thick black bar on each meter.

## [0.4.1] - 2026-09-24

A theme frame no longer paints the placeholder meters or the spectrum staircase. The theme JPEG is shown, and a needle image is placed at `left.origin` and `right.origin`.

## [0.4.0] - 2026-09-24

The selected meter's indicator image is drawn at `left.x`/`left.y` and `right.x`/`right.y`. The visible width follows the channel level. `testdata/show-theme.sh` leaves that frame on the display until Ctrl-C.

## [0.3.0] - 2026-09-24

The frame size follows the selected theme folder. `screen.width` and `screen.height` override that pair when both are set. The theme's meter background is copied at its own resolution.

## [0.2.0] - 2026-09-24

`pane` opens a window when `DISPLAY` is set and uploads one RGBA frame. `plugin/run_glass.sh` sets the X11 driver, and on x64 turns MIT-SHM off, before the process starts. The loop sleeps for `[current] frame.rate` from the plugin `config.txt` (10–60, default 30). `--headless` skips that window. `--output` still writes a PPM.

## [0.1.0] - 2026-09-24

The player reads `/tmp/myfifo` and `/tmp/myfifosa`, plots levels and bars, and rasters one RGBA frame. `--output` writes a PPM. No display library is linked.

## [0.0.1] - 2026-09-24

Initial scaffold. The station line compiles. It does not yet read a FIFO or draw a frame.
