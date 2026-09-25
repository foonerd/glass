# Changelog

All notable changes to Glass are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
