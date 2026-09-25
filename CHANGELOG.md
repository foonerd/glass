# Changelog

All notable changes to Glass are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
