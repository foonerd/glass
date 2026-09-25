//! Player end of the line.
//!
//! `--once` reads the pipes, plots, and rasters a single frame.
//! `--output frame.ppm` writes that frame. Without it, the process keeps the
//! latest scene in memory at about 30 steps a second.

use std::env;
use std::process::ExitCode;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use expose::{apply_circle, fit_art, flip_x, raster_over, read_art, read_icon, read_png, FolderPicture, Fonts, IndicatorAssets, Motion, SpectrumAssets, Stack};
use intake::{PipeSource, Selector, Source};
use lead::{frame_period, FolderLayerSpec, Input, MeterKind, SkinDesc, TypeMode};
use pane::{publish, write_ppm, Surface};
use plot::{step, Scene};

fn load_theme(dir: &str, file: &str) -> Option<expose::Frame> {
    if dir.is_empty() || file.is_empty() {
        return None;
    }
    read_png(std::path::Path::new(dir).join(file).as_path())
}

/// Everything decoded once per meter: its pictures, fonts and art mask.
struct Assets {
    background: Option<expose::Frame>,
    face: Option<expose::Frame>,
    front: Option<expose::Frame>,
    /// The indicator for the left or mono channel, mirrored when the meter flips it.
    indicator: Option<expose::Frame>,
    /// The right channel's indicator when it differs from the left one.
    indicator_right: Option<expose::Frame>,
    fonts: Fonts,
    art_mask: Option<expose::Frame>,
    /// The spectrum's pictures when the meter shows one.
    spectrum: Option<SpectrumAssets>,
    /// The tonearm picture when the meter has one.
    tonearm: Option<expose::Frame>,
    /// The theme's reel pictures; an album's reel is scaled to their size.
    reels: (Option<expose::Frame>, Option<expose::Frame>),
    /// The indicators' prepared states and pictures.
    indicators: Option<IndicatorAssets>,
}

impl Assets {
    fn load(skin: &SkinDesc) -> Self {
        let mut fonts = Fonts::load(&skin.fonts);
        for field in [&skin.time, &skin.time_elapsed, &skin.time_total].into_iter().flatten() {
            fonts.add_file(&field.font_file);
        }
        let picture = load_theme(&skin.theme_dir, &skin.indicator);
        let (flip_left, flip_right) = match (skin.meter.kind, &skin.meter.linear) {
            (MeterKind::Linear, Some(linear)) => (linear.flip_left, linear.flip_right),
            _ => (skin.meter.flip_left, skin.meter.flip_right),
        };
        let indicator_right = match (&picture, flip_left == flip_right) {
            (Some(p), false) if flip_right => Some(flip_x(p)),
            (Some(p), false) => Some(p.clone()),
            _ => None,
        };
        let indicator = match picture {
            Some(p) if flip_left => Some(flip_x(&p)),
            other => other,
        };
        Self {
            background: load_theme(&skin.theme_dir, &skin.background),
            face: load_theme(&skin.theme_dir, &skin.face),
            front: load_theme(&skin.theme_dir, &skin.front),
            indicator,
            indicator_right,
            fonts,
            art_mask: skin
                .art
                .as_ref()
                .filter(|art| !art.mask.is_empty())
                .and_then(|art| read_png(std::path::Path::new(&art.mask))),
            spectrum: skin.spectrum.as_ref().map(SpectrumAssets::load),
            tonearm: skin.tonearm.as_ref().and_then(|arm| read_png(std::path::Path::new(&arm.file))),
            reels: (
                skin.reels.as_ref().and_then(|r| r.left.as_ref()).and_then(|r| read_png(std::path::Path::new(&r.theme_file))),
                skin.reels.as_ref().and_then(|r| r.right.as_ref()).and_then(|r| read_png(std::path::Path::new(&r.theme_file))),
            ),
            indicators: skin.indicators.as_ref().map(IndicatorAssets::load),
        }
    }
}

/// A picture decoded and fitted off the frame loop: the slot keeps showing
/// what it has until the next file is ready.
#[derive(Default)]
struct PictureSlot {
    file: String,
    picture: Option<FolderPicture>,
    pending: Option<(String, mpsc::Receiver<Option<FolderPicture>>)>,
}

impl PictureSlot {
    /// Ask for `file` in this box; `ready` is a picture of that file already
    /// decoded elsewhere, taken over without decoding again.
    fn want(&mut self, file: &str, spec: &FolderLayerSpec, ready: Option<&FolderPicture>) {
        if let Some((wanted, rx)) = &self.pending {
            match rx.try_recv() {
                Ok(picture) => {
                    self.file = wanted.clone();
                    self.picture = picture;
                    self.pending = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.pending = None,
            }
        }
        if file == self.file || self.pending.as_ref().is_some_and(|(wanted, _)| wanted == file) {
            return;
        }
        if file.is_empty() {
            self.file.clear();
            self.picture = None;
            self.pending = None;
            return;
        }
        if let Some(picture) = ready {
            self.file = file.to_string();
            self.picture = Some(picture.clone());
            self.pending = None;
            return;
        }
        let (tx, rx) = mpsc::channel();
        let (path, spec) = (file.to_string(), spec.clone());
        thread::spawn(move || {
            let _ = tx.send(FolderPicture::load(std::path::Path::new(&path), &spec));
        });
        self.pending = Some((file.to_string(), rx));
    }
}

/// A picture decoded off the frame loop and stretched to a size, or kept
/// as it is: the record.
#[derive(Default)]
struct PlainSlot {
    key: (String, Option<(u32, u32)>),
    frame: Option<expose::Frame>,
    pending: Option<((String, Option<(u32, u32)>), mpsc::Receiver<Option<expose::Frame>>)>,
}

impl PlainSlot {
    fn want(&mut self, file: &str, size: Option<(u32, u32)>) {
        if let Some((wanted, rx)) = &self.pending {
            match rx.try_recv() {
                Ok(frame) => {
                    self.key = wanted.clone();
                    self.frame = frame;
                    self.pending = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.pending = None,
            }
        }
        let key = (file.to_string(), size);
        if key == self.key || self.pending.as_ref().is_some_and(|(wanted, _)| *wanted == key) {
            return;
        }
        if file.is_empty() {
            self.key = key;
            self.frame = None;
            self.pending = None;
            return;
        }
        let (tx, rx) = mpsc::channel();
        let path = file.to_string();
        thread::spawn(move || {
            let frame = read_png(std::path::Path::new(&path)).map(|f| match size {
                Some((w, h)) => fit_art(&f, w, h),
                None => f,
            });
            let _ = tx.send(frame);
        });
        self.pending = Some((key, rx));
    }
}

/// Whether a fade may start now: the engine's lock file is older than the
/// fade plus a second, or absent. Touching it claims the fade.
fn fade_lock_free(duration_s: f32) -> bool {
    let lock = std::env::temp_dir().join("peppy_fade_lock");
    let cooldown = std::time::Duration::from_secs_f32(duration_s.max(0.0) + 1.0);
    let free = match std::fs::metadata(&lock).and_then(|m| m.modified()) {
        Ok(modified) => modified.elapsed().map_or(true, |age| age > cooldown),
        Err(_) => true,
    };
    if free {
        let _ = std::fs::write(&lock, b"");
    }
    free
}

/// One recorded step, the form `plot` replays from `testdata/frames/`.
#[derive(serde::Serialize)]
struct Recorded<'a> {
    skin: &'a SkinDesc,
    input: &'a Input,
    scene: &'a Scene,
}

fn main() -> ExitCode {
    let mut once = false;
    let mut headless = false;
    let mut print_scene = false;
    let mut output: Option<String> = None;
    let mut record: Option<String> = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--once" => once = true,
            "--headless" => headless = true,
            "--print" => print_scene = true,
            "--output" => match args.next() {
                Some(path) => output = Some(path),
                None => {
                    eprintln!("glass: --output needs a path");
                    return ExitCode::from(2);
                }
            },
            "--record" => match args.next() {
                Some(path) => record = Some(path),
                None => {
                    eprintln!("glass: --record needs a path");
                    return ExitCode::from(2);
                }
            },
            "--help" => {
                println!(
                    "glass [--once] [--headless] [--print] [--output frame.ppm] [--record step.json]\n\
                     Reads {meter} and {spectrum}.\n\
                     A window opens when DISPLAY is set. --headless skips it.\n\
                     --output writes a PPM and still rasters.\n\
                     --record writes the skin, input and scene of each step as JSON.",
                    meter = lead::METER_FIFO,
                    spectrum = lead::SPECTRUM_FIFO
                );
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("glass: unknown argument {other}");
                return ExitCode::from(2);
            }
        }
    }

    // A theme in random or list mode moves to its next meter on the timer,
    // or with the next title when the player says so.
    let mut selector = Selector::new(intake::installed_rotation());
    let mut skin = match selector.next() {
        Some(name) => intake::installed_skin_named(Some(&name)),
        None => intake::installed_skin(),
    };
    let mut source = PipeSource::installed().with_skin(&skin);
    let frame_rate = intake::installed_frame_rate();
    let started = Instant::now();
    let mut motion = Motion::default();
    let period = frame_period(frame_rate);
    println!(
        "glass: frame.rate={frame_rate} size={}x{} theme={} meter={}",
        skin.width,
        skin.height,
        if skin.theme_dir.is_empty() {
            "none"
        } else {
            &skin.theme_dir
        },
        skin.name
    );
    let mut assets = Assets::load(&skin);
    println!("glass: fonts loaded {} of 5", assets.fonts.loaded());
    // The engine fades the first frame in when the player asks for a start
    // animation, and every later meter in; a lock file shared with the
    // player's own engine keeps two starts within the fade's time from fading twice.
    let mut did_fade_in = false;
    let loaded_ms = started.elapsed().as_millis() as u64;
    if skin.transition.at_start && skin.transition.fade && fade_lock_free(skin.transition.duration_s) {
        motion.fade.begin_in(loaded_ms, skin.transition.duration_s, skin.transition.white, skin.transition.opacity);
        did_fade_in = true;
    }
    motion.ramp.begin(loaded_ms);
    let mut switched_at = Instant::now();
    let mut last_title: Option<String> = None;
    // The art picture, decoded and stretched once per file and box, cut with
    // the theme's mask when it has one.
    let mut art_cache: Option<(plot::Art, expose::Frame)> = None;
    // The type icon, decoded once per file, box and tint.
    let mut icon_cache: Option<((String, u32, u32, Option<[u8; 3]>), expose::Frame)> = None;
    // Folder layer pictures, decoded once per file and box, one slot per layer.
    let mut folder_slots: Vec<PictureSlot> = Vec::new();
    // The fanart on show and the one it replaces during a transition.
    let mut fanart_slots: (PictureSlot, PictureSlot) = Default::default();
    // The record picture for the track, stretched to the theme's dimension.
    let mut vinyl_slot = PlainSlot::default();
    // Album reel pictures for the track, scaled to the theme reels.
    let mut reel_slots: (PlainSlot, PlainSlot) = Default::default();
    let show_window = env::var_os("DISPLAY").is_some() && !headless;
    let write_file = output.is_some();
    let serving_remote = false;
    let mut surface = if show_window {
        match Surface::open(skin.width, skin.height) {
            Ok(surface) => Some(surface),
            Err(err) => {
                eprintln!("glass: {err}");
                return ExitCode::from(1);
            }
        }
    } else {
        None
    };

    loop {
        let input = source.poll();
        if selector.rotates() {
            let due = if selector.on_title() {
                let title = &input.metadata.title;
                let changed = last_title.as_ref().is_some_and(|known| known != title);
                if last_title.as_ref() != Some(title) {
                    last_title = Some(title.clone());
                }
                changed
            } else {
                switched_at.elapsed() >= selector.interval()
            };
            if due {
                if let Some(name) = selector.next() {
                    skin = intake::installed_skin_named(Some(&name));
                    source.set_skin(&skin);
                    assets = Assets::load(&skin);
                    art_cache = None;
                    icon_cache = None;
                    folder_slots.clear();
                    fanart_slots = Default::default();
                    vinyl_slot = PlainSlot::default();
                    reel_slots = Default::default();
                    motion = Motion::default();
                    let now = started.elapsed().as_millis() as u64;
                    if skin.transition.fade && fade_lock_free(skin.transition.duration_s) {
                        motion.fade.begin_in(now, skin.transition.duration_s, skin.transition.white, skin.transition.opacity);
                        did_fade_in = true;
                    }
                    motion.ramp.begin(now);
                    switched_at = Instant::now();
                    println!("glass: meter={name}");
                }
            }
        }
        let scene = step(&skin, &input);
        if let Some(path) = &record {
            let recorded = Recorded {
                skin: &skin,
                input: &input,
                scene: &scene,
            };
            let written = serde_json::to_string_pretty(&recorded)
                .map_err(|err| err.to_string())
                .and_then(|text| std::fs::write(path, text).map_err(|err| err.to_string()));
            if let Err(err) = written {
                eprintln!("glass: --record {path}: {err}");
                return ExitCode::from(1);
            }
        }
        if print_scene {
            println!(
                "left {:.2} right {:.2} bars {}",
                scene.left,
                scene.right,
                scene.bars.len()
            );
        }
        if surface.is_some() || write_file {
            match &scene.art {
                Some(art) => {
                    let stale = art_cache.as_ref().map_or(true, |(known, _)| known != art);
                    if stale {
                        art_cache = read_art(std::path::Path::new(&art.file), art.w, art.h, assets.art_mask.as_ref())
                            .map(|frame| if art.rotation && art.mask.is_empty() { apply_circle(&frame) } else { frame })
                            .map(|frame| (art.clone(), frame));
                    }
                }
                None => art_cache = None,
            }
            match scene.type_area.as_ref().filter(|a| !a.icon.is_empty()) {
                Some(area) => {
                    let (w, h) = area.box_size.unwrap_or((1, 1));
                    let (fw, fh) = if area.mode == TypeMode::Both {
                        let side = w.min(h).max(1);
                        (side, side)
                    } else {
                        (w, h)
                    };
                    let tint = if area.icon.to_ascii_lowercase().ends_with(".svg") {
                        Some(area.color)
                    } else {
                        None
                    };
                    let key = (area.icon.clone(), fw, fh, tint);
                    let stale = icon_cache.as_ref().map_or(true, |(known, _)| *known != key);
                    if stale {
                        icon_cache = read_icon(std::path::Path::new(&area.icon), fw, fh, tint)
                            .map(|frame| (key, frame));
                    }
                }
                None => icon_cache = None,
            }
            if folder_slots.len() != scene.folder_layers.len() {
                folder_slots = scene.folder_layers.iter().map(|_| PictureSlot::default()).collect();
            }
            for (slot, layer) in folder_slots.iter_mut().zip(scene.folder_layers.iter()) {
                slot.want(&layer.file, &layer.spec, None);
            }
            let folder_pictures: Vec<Option<FolderPicture>> = folder_slots.iter().map(|s| s.picture.clone()).collect();
            if let Some(fanart) = &scene.fanart {
                let spec = FolderLayerSpec {
                    files: Vec::new(),
                    x: fanart.spec.x,
                    y: fanart.spec.y,
                    w: fanart.spec.w,
                    h: fanart.spec.h,
                    scale: fanart.spec.scale,
                    zorder: fanart.spec.zorder,
                    border: 0,
                    border_color: [0, 0, 0],
                };
                // The picture being replaced is the one the current slot held.
                let handed_over = if fanart_slots.0.file == fanart.prev_file { fanart_slots.0.picture.clone() } else { None };
                fanart_slots.1.want(&fanart.prev_file, &spec, handed_over.as_ref());
                fanart_slots.0.want(&fanart.file, &spec, None);
            } else {
                fanart_slots = Default::default();
            }
            match &scene.vinyl {
                Some(vinyl) => vinyl_slot.want(&vinyl.file, vinyl.spec.dimension),
                None => vinyl_slot = PlainSlot::default(),
            }
            // A reel from the album is scaled to the theme reel's size; the theme reel itself needs no slot.
            let reel_pictures = match &scene.reels {
                Some(reels) => {
                    let side = |slot: &mut PlainSlot, file: &str, spec: Option<&lead::ReelSpec>, theme: Option<&expose::Frame>| -> Option<expose::Frame> {
                        let spec = spec?;
                        if file.is_empty() || file == spec.theme_file {
                            slot.want("", None);
                            return theme.cloned();
                        }
                        slot.want(file, theme.map(|t| (t.width, t.height)));
                        slot.frame.clone().or_else(|| theme.cloned())
                    };
                    (
                        side(&mut reel_slots.0, &reels.left_file, reels.spec.left.as_ref(), assets.reels.0.as_ref()),
                        side(&mut reel_slots.1, &reels.right_file, reels.spec.right.as_ref(), assets.reels.1.as_ref()),
                    )
                }
                None => (None, None),
            };
            let frame = raster_over(
                &scene,
                Stack {
                    screen: assets.background.as_ref(),
                    face: assets.face.as_ref(),
                    front: assets.front.as_ref(),
                    needle: assets.indicator.as_ref(),
                    needle_right: assets.indicator_right.as_ref(),
                    face_at: skin.face_at,
                    fonts: Some(&assets.fonts),
                    art: art_cache.as_ref().map(|(_, frame)| frame),
                    icon: icon_cache.as_ref().map(|(_, frame)| frame),
                    spectrum: assets.spectrum.as_ref(),
                    folder_pictures: &folder_pictures,
                    fanart: (fanart_slots.0.picture.as_ref(), fanart_slots.1.picture.as_ref()),
                    vinyl: vinyl_slot.frame.as_ref(),
                    tonearm: assets.tonearm.as_ref(),
                    reels: (reel_pictures.0.as_ref(), reel_pictures.1.as_ref()),
                    indicators: assets.indicators.as_ref(),
                },
                &mut motion,
                started.elapsed().as_millis() as u64,
            );
            if let Some(window) = surface.as_mut() {
                match window.show(&frame) {
                    Ok(true) => {}
                    Ok(false) => {
                        // Leave the way the engine leaves: fade out when a fade in was shown.
                        if did_fade_in && skin.transition.fade {
                            let now = started.elapsed().as_millis() as u64;
                            motion.fade.begin_out(now, skin.transition.duration_s, skin.transition.white, skin.transition.opacity);
                            while motion.fade.running(started.elapsed().as_millis() as u64) {
                                let frame = raster_over(&scene, Stack { screen: assets.background.as_ref(), face: assets.face.as_ref(), front: assets.front.as_ref(), needle: assets.indicator.as_ref(), needle_right: assets.indicator_right.as_ref(), face_at: skin.face_at, fonts: Some(&assets.fonts), art: art_cache.as_ref().map(|(_, frame)| frame), icon: icon_cache.as_ref().map(|(_, frame)| frame), spectrum: assets.spectrum.as_ref(), folder_pictures: &folder_pictures, fanart: (fanart_slots.0.picture.as_ref(), fanart_slots.1.picture.as_ref()), vinyl: vinyl_slot.frame.as_ref(), tonearm: assets.tonearm.as_ref(), reels: (reel_pictures.0.as_ref(), reel_pictures.1.as_ref()), indicators: assets.indicators.as_ref() }, &mut motion, started.elapsed().as_millis() as u64);
                                if window.show(&frame).is_err() {
                                    break;
                                }
                                thread::sleep(period);
                            }
                        }
                        break;
                    }
                    Err(err) => {
                        eprintln!("glass: {err}");
                        return ExitCode::from(1);
                    }
                }
            }
            if let Some(path) = &output {
                if let Err(err) = write_ppm(path, &frame) {
                    eprintln!("glass: {err}");
                    return ExitCode::from(1);
                }
            }
        }
        if serving_remote {
            publish(&scene);
        }
        if once {
            break;
        }
        thread::sleep(period);
    }
    ExitCode::SUCCESS
}
