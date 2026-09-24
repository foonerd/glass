# Changelog

All notable changes to Glass are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-09-24

The frame size follows the selected theme folder. `screen.width` and `screen.height` override that pair when both are set. The theme's meter background is copied at its own resolution.

## [0.2.0] - 2026-09-24

`pane` opens a window when `DISPLAY` is set and uploads one RGBA frame. `plugin/run_glass.sh` sets the X11 driver, and on x64 turns MIT-SHM off, before the process starts. The loop sleeps for `[current] frame.rate` from the plugin `config.txt` (10–60, default 30). `--headless` skips that window. `--output` still writes a PPM.

## [0.1.0] - 2026-09-24

The player reads `/tmp/myfifo` and `/tmp/myfifosa`, plots levels and bars, and rasters one RGBA frame. `--output` writes a PPM. No display library is linked.

## [0.0.1] - 2026-09-24

Initial scaffold. The station line compiles. It does not yet read a FIFO or draw a frame.
