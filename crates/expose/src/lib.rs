//! Raster a [`plot::Scene`] into one RGBA frame.
//! This station does not poll a FIFO or talk to the player.

use std::path::Path;

use std::collections::HashMap;

use std::sync::Arc;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
#[cfg(test)]
use lead::MeterSpec;
use lead::{
    Direction, Fill, FolderLayerSpec, FontFiles, LinearSpec, MeterKind, Scale, ScrollDirection,
    SpectrumSpec, TextAlign, TextStyle, TypeAlign, TypeMode, ZOrder,
};
use lead::{GaugeSpec, GaugeStyle, StateIndicator, StateLook, TonearmSpec};
use plot::{Art, Fanart, Indicators, Scene, Text, TypeArea};

const BG: [u8; 4] = [12, 12, 16, 255];

/// Theme fonts, loaded once per meter. A style whose file is missing or
/// unreadable stays `None`, and its texts fall back to the built-in bitmap
/// font. Italic without its own file falls back to regular, as the player does.
/// A file named by several styles is mapped once.
#[derive(Default)]
pub struct Fonts {
    light: Option<Arc<Face>>,
    regular: Option<Arc<Face>>,
    bold: Option<Arc<Face>>,
    italic: Option<Arc<Face>>,
    digi: Option<Arc<Face>>,
    /// The face a glyph comes from when a text's own face lacks it.
    fallback: Option<Arc<Face>>,
    /// Fonts a field names by file.
    files: HashMap<String, Arc<Face>>,
    /// Every face by its path.
    by_path: HashMap<String, Arc<Face>>,
}

/// The files a host hands over in place of a file system, and the reader
/// every picture and font here goes through: `lead`'s, shared with what
/// reads the configuration and the theme.
pub use lead::{read_file, vfs};

/// The clock the stages and the painters are timed by: `lead`'s, the
/// process's own or the host's where the target has none.
pub use lead::clock_us;
#[cfg(target_arch = "wasm32")]
pub use lead::set_clock_us;

/// A font file mapped into memory rather than read: the kernel brings in
/// only the pages the glyphs touch, and shares them with any other process
/// that maps the file. A 16 MB face costs the tables it uses. Where the
/// host hands the file over instead (`vfs`), the bytes are held as they came.
pub struct Face {
    font: FontRef<'static>,
    /// Holds the bytes `font` reads; dropped after it.
    _keep: Keep,
}

enum Keep {
    #[cfg(not(target_arch = "wasm32"))]
    Map(memmap2::Mmap),
    Shared(Arc<[u8]>),
}

impl Keep {
    fn len(&self) -> usize {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Keep::Map(map) => map.len(),
            Keep::Shared(bytes) => bytes.len(),
        }
    }
}

fn open_face(path: &str) -> Option<Arc<Face>> {
    if path.is_empty() {
        return None;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(file) = std::fs::File::open(path) {
        // The theme's font files are not written while a meter shows them.
        let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
        // The font borrows the map for as long as the face lives, and the face
        // keeps the map; nothing hands the font out past the face.
        let bytes: &'static [u8] = unsafe { std::slice::from_raw_parts(map.as_ptr(), map.len()) };
        let font = FontRef::try_from_slice(bytes).ok()?;
        return Some(Arc::new(Face {
            font,
            _keep: Keep::Map(map),
        }));
    }
    let held = vfs::get(path)?;
    // As with the map: the shared buffer stays where it is for as long as
    // the face holds it, and the face is the last to let go.
    let bytes: &'static [u8] = unsafe { std::slice::from_raw_parts(held.as_ptr(), held.len()) };
    let font = FontRef::try_from_slice(bytes).ok()?;
    Some(Arc::new(Face {
        font,
        _keep: Keep::Shared(held),
    }))
}

impl Fonts {
    pub fn load(files: &FontFiles) -> Self {
        let mut fonts = Self::default();
        fonts.light = fonts.face(&files.light);
        fonts.regular = fonts.face(&files.regular);
        fonts.bold = fonts.face(&files.bold);
        fonts.italic = fonts.face(&files.italic);
        fonts.digi = fonts.face(&files.digi);
        fonts.fallback = fonts.face(&files.fallback);
        fonts
    }

    /// The face consulted for a glyph the text's own face lacks.
    fn fallback(&self) -> Option<&FontRef<'static>> {
        self.fallback.as_ref().map(|f| &f.font)
    }

    /// The face of a file, mapped on the first ask.
    fn face(&mut self, path: &str) -> Option<Arc<Face>> {
        if path.is_empty() {
            return None;
        }
        if let Some(face) = self.by_path.get(path) {
            return Some(Arc::clone(face));
        }
        let face = open_face(path)?;
        self.by_path.insert(path.to_string(), Arc::clone(&face));
        Some(face)
    }

    /// Load a font a field names by file, so `get_for` can serve it.
    pub fn add_file(&mut self, path: &str) {
        if path.is_empty() || self.files.contains_key(path) {
            return;
        }
        if let Some(face) = self.face(path) {
            self.files.insert(path.to_string(), face);
        }
    }

    fn get(&self, style: TextStyle) -> Option<&FontRef<'static>> {
        let face = match style {
            TextStyle::Light => self.light.as_ref(),
            TextStyle::Regular => self.regular.as_ref(),
            TextStyle::Bold => self.bold.as_ref(),
            TextStyle::Italic => self.italic.as_ref().or(self.regular.as_ref()),
            TextStyle::Digi => self.digi.as_ref(),
        };
        face.map(|f| &f.font)
    }

    /// The font for a text: its own file when it names one and that file
    /// loaded, else its style's font.
    fn get_for(&self, style: TextStyle, font_file: &str) -> Option<&FontRef<'static>> {
        if !font_file.is_empty() {
            if let Some(face) = self.files.get(font_file) {
                return Some(&face.font);
            }
        }
        self.get(style)
    }

    /// The bytes the font files span; of a mapped file only the pages touched are resident.
    pub fn bytes(&self) -> usize {
        self.by_path.values().map(|f| f._keep.len()).sum()
    }

    /// How many of the five styles have a font file of their own.
    pub fn loaded(&self) -> usize {
        [
            &self.light,
            &self.regular,
            &self.bold,
            &self.italic,
            &self.digi,
        ]
        .iter()
        .filter(|f| f.is_some())
        .count()
    }
}

/// Pause at each end of a bouncing text, as the player pauses.
const SCROLL_PAUSE_MS: u64 = 400;

#[derive(Debug, Clone, Default)]
struct ScrollState {
    text: String,
    box_w: u32,
    offset: f32,
    dir: f32,
    pause_until: u64,
    last_ms: u64,
}

/// Where each moving text is on its way, keyed by its position. Owned by the
/// player binary across frames; `raster_over` advances it.
#[derive(Debug, Default)]
pub struct TextMotion {
    states: HashMap<(u32, u32), ScrollState>,
    /// The rendered line of each text position, kept while the text, its
    /// style, size, colour and font stay the same; glyphs are set once, not
    /// every frame.
    lines: HashMap<(u32, u32), (String, Frame)>,
}

impl TextMotion {
    /// The current offset of a text, for tests and diagnostics.
    pub fn offset(&self, x: u32, y: u32) -> Option<f32> {
        self.states.get(&(x, y)).map(|s| s.offset)
    }
}
const METER: [u8; 4] = [80, 220, 120, 255];
const BAR: [u8; 4] = [90, 170, 255, 255];

/// How drawing lands on a frame: over what is there, the alpha deciding,
/// or added to it, so overlaps bloom and colours saturate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Blend {
    #[default]
    Normal,
    Add,
}

/// One finished picture. `pane` uploads it once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frame {
    /// How the primitives land on this frame; normal unless a look asks.
    pub blend: Blend,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Frame {
    /// The bytes the pixels take.
    pub fn bytes(&self) -> usize {
        self.rgba.len()
    }
}

/// The bytes a set of optional pictures take.
pub fn bytes_of<'a>(pictures: impl IntoIterator<Item = &'a Option<Frame>>) -> usize {
    pictures.into_iter().flatten().map(Frame::bytes).sum()
}

/// Rectangles the raster fills. Tests sample these instead of guessing pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub left_meter: Rect,
    pub right_meter: Rect,
    pub spectrum: Rect,
}

/// Meters take the left quarter. Spectrum bars share the rest.
pub fn layout(width: u32, height: u32) -> Layout {
    let margin = (width / 40).max(1);
    let meter_w = (width / 16).max(1);
    let meter_h = height.saturating_sub(margin * 2).max(1);
    let left_meter = Rect {
        x: margin,
        y: margin,
        w: meter_w,
        h: meter_h,
    };
    let right_meter = Rect {
        x: margin + meter_w + margin,
        y: margin,
        w: meter_w,
        h: meter_h,
    };
    let spectrum_x = right_meter.x + right_meter.w + margin;
    let spectrum = Rect {
        x: spectrum_x.min(width.saturating_sub(1)),
        y: margin,
        w: width.saturating_sub(spectrum_x).max(1),
        h: meter_h,
    };
    Layout {
        left_meter,
        right_meter,
        spectrum,
    }
}

/// Build the frame from the scene fractions, over an optional theme image.
pub fn raster(scene: &Scene) -> Frame {
    let mut motion = Motion::default();
    raster_over(
        scene,
        Stack {
            screen: None,
            face: None,
            front: None,
            needle: None,
            needle_right: None,
            face_at: (0, 0),
            fonts: None,
            art: None,
            icon: None,
            spectra: &[],
            folder_pictures: &[],
            fanart: (None, None),
            vinyl: None,
            tonearm: None,
            reels: (None, None),
            indicators: None,
            base: None,
        },
        &mut motion,
        0,
    )
    .frame
    .clone()
}

/// Decode a picture by its content and stretch it to `w` by `h`, as the
/// player's engine does with album art, then cut it with `mask` if given.
pub fn read_art(path: &Path, w: u32, h: u32, mask: Option<&Frame>) -> Option<Frame> {
    let image = image::ImageReader::new(std::io::Cursor::new(read_file(path)?))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?
        .into_rgba8();
    let frame = Frame {
        blend: Blend::Normal,
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    };
    let mut fitted = fit_art(&frame, w, h);
    if let Some(mask) = mask {
        apply_mask(&mut fitted, mask);
    }
    Some(fitted)
}

/// Stretch a frame to `w` by `h` with bilinear filtering.
pub fn fit_art(frame: &Frame, w: u32, h: u32) -> Frame {
    let (w, h) = (w.max(1), h.max(1));
    if frame.width == w && frame.height == h {
        return frame.clone();
    }
    let Some(image) = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone())
    else {
        return frame.clone();
    };
    let scaled = image::imageops::resize(&image, w, h, image::imageops::FilterType::Triangle);
    Frame {
        blend: Blend::Normal,
        width: w,
        height: h,
        rgba: scaled.into_raw(),
    }
}

/// A picture file's size, from its header.
pub fn picture_size(path: &Path) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(read_file(path)?))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// A picture file scaled by `sx` and `sy` into another file of the same
/// format (by its name), for the cutter: decoded, its colour premultiplied
/// by its alpha so edges do not fringe, resampled with Lanczos, and
/// written back. The new size comes back, or why it could not be done.
pub fn resample_picture(src: &Path, dst: &Path, sx: f32, sy: f32) -> Result<(u32, u32), String> {
    let bytes = read_file(src).ok_or_else(|| format!("{}: cannot be read", src.display()))?;
    let image = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| format!("{}: {e}", src.display()))?
        .decode()
        .map_err(|e| format!("{}: {e}", src.display()))?
        .into_rgba8();
    let (w, h) = (
        ((image.width() as f32 * sx).round() as u32).max(1),
        ((image.height() as f32 * sy).round() as u32).max(1),
    );
    let mut premultiplied = image;
    for px in premultiplied.pixels_mut() {
        let a = px[3] as u32;
        for k in 0..3 {
            px[k] = ((px[k] as u32 * a + 127) / 255) as u8;
        }
    }
    let mut scaled =
        image::imageops::resize(&premultiplied, w, h, image::imageops::FilterType::Lanczos3);
    for px in scaled.pixels_mut() {
        let a = px[3] as u32;
        if a > 0 && a < 255 {
            for k in 0..3 {
                px[k] = ((px[k] as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    let ext = dst
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => {
            // JPEG has no alpha: on an opaque background.
            let rgb = image::DynamicImage::ImageRgba8(scaled).into_rgb8();
            rgb.save(dst)
                .map_err(|e| format!("{}: {e}", dst.display()))?;
        }
        _ => scaled
            .save(dst)
            .map_err(|e| format!("{}: {e}", dst.display()))?,
    }
    Ok((w, h))
}

/// `background` is the theme picture. It is copied in place. It is not scaled.
/// A theme is authored at its own resolution, so a mismatch leaves the dark fill.
pub struct Stack<'a> {
    pub screen: Option<&'a Frame>,
    pub face: Option<&'a Frame>,
    /// The meter foreground with its opaque spans.
    pub front: Option<&'a Spans>,
    /// The indicator picture for the left or mono channel: a needle sprite,
    /// or the bar picture of a linear meter, mirrored when the meter says so.
    pub needle: Option<&'a Frame>,
    /// The right channel's picture; `None` shares `needle`.
    pub needle_right: Option<&'a Frame>,
    pub face_at: (u32, u32),
    /// Theme fonts for `Scene::texts`. `None` draws every text in the bitmap font.
    pub fonts: Option<&'a Fonts>,
    /// Album art already stretched to the box in `Scene::art`.
    pub art: Option<&'a Frame>,
    /// Type icon already fitted for `Scene::type_area`, tinted when it was an SVG.
    pub icon: Option<&'a Frame>,
    /// The spectrum boxes' pictures, one per `Scene::spectra`, built once per meter.
    pub spectra: &'a [SpectrumAssets],
    /// One entry per `Scene::folder_layers`, the picture fitted to its box.
    pub folder_pictures: &'a [Option<FolderPicture>],
    /// The fanart on show and the one it replaces, fitted to the slot.
    pub fanart: (Option<&'a FolderPicture>, Option<&'a FolderPicture>),
    /// The record picture, already stretched to `vinyl.dimension`, and the tonearm picture.
    pub vinyl: Option<&'a Frame>,
    pub tonearm: Option<&'a Frame>,
    /// The reel pictures, the album's scaled to the theme's.
    pub reels: (Option<&'a Frame>, Option<&'a Frame>),
    /// The indicators' prepared pictures, when the meter has indicators.
    pub indicators: Option<&'a IndicatorAssets>,
    /// The screen picture and meter face composed once; the frame starts as a copy of it.
    pub base: Option<&'a Frame>,
}

/// Theme order: full-screen picture, meter face, album art, folder layers, needles, meter
/// spectrum, texts, overlay folder layers, and the meter foreground last. `now_ms` drives the moving texts through `motion`.
///
/// A frame is made in two steps. First everything that moves is advanced and
/// every line of text is set in type, which is where `motion` changes. Then
/// the drawing steps, which only read, are painted over the base in row
/// bands shared among the threads `motion` allows.
pub fn raster_over<'m>(
    scene: &Scene,
    stack: Stack<'_>,
    motion: &'m mut Motion,
    now_ms: u64,
) -> Painted<'m> {
    // A start ramps the levels up over the engine's first frames.
    let ramp = motion.ramp.factor(now_ms);
    let ramped;
    let scene = if ramp < 1.0 {
        ramped = Scene {
            left: scene.left * ramp,
            right: scene.right * ramp,
            mono: scene.mono * ramp,
            bar_heights: scene
                .bar_heights
                .iter()
                .map(|heights| heights.iter().map(|&h| (h as f32 * ramp) as u32).collect())
                .collect(),
            ..scene.clone()
        };
        &ramped
    } else {
        scene
    };
    let width = scene.width.max(1);
    let height = scene.height.max(1);
    let mut taken = motion.profile.take();
    let mut stages = Stages::new(taken.as_mut());
    let Motion {
        text,
        spectrum,
        analyser,
        vinyl,
        tonearm,
        reels,
        fade,
        needles,
        labels,
        canvas,
        painters,
        bench_delay,
        profile,
        last_steps,
        last_base,
        damage,
        paint_all,
        reaches,
        ..
    } = motion;
    // The meter engine composes the screen picture and the meter face into
    // one static background; the frame starts as a copy of it. A prepared
    // base is used as it is, otherwise it is composed here.
    let composed;
    let base: &Frame = match stack.base {
        Some(base) if base.width == width && base.height == height => base,
        _ => {
            composed = compose_base(width, height, stack.screen, stack.face, stack.face_at);
            &composed
        }
    };

    // Everything that moves, advanced once for this frame. The tonearm's
    // state decides whether the record keeps turning while it lifts.
    let tonearm_animating = scene.tonearm.as_ref().is_some_and(|spec| {
        tonearm.update(
            spec,
            scene.playing,
            scene.progress_pct,
            scene.time_remaining,
            now_ms,
        );
        tonearm.is_animating()
    });
    let lift_s = scene.tonearm.as_ref().map_or(1.5, |spec| spec.lift_s);
    let vinyl_rpm = scene.vinyl.as_ref().map_or(0.0, |v| v.spec.rpm);
    let art_rpm = scene
        .art
        .as_ref()
        .filter(|a| a.rotation)
        .map_or(0.0, |a| a.rpm);
    let turn_rpm = if scene.vinyl.is_some() {
        vinyl_rpm
    } else {
        art_rpm
    };
    let angle = vinyl.advance(
        turn_rpm,
        scene.vinyl.as_ref().is_none_or(|v| v.spec.clockwise),
        scene.playing,
        scene.transitional,
        tonearm_animating,
        lift_s,
        now_ms,
    );
    let mut reel_angles = [0.0f32; 2];
    if let Some(spec) = &scene.reels {
        let spin = scene.playing || scene.transitional;
        let p = scene.progress_pct / 100.0;
        let (left_mult, right_mult) = if spec.spec.adaptive {
            if spec.spec.clockwise {
                (
                    spec.spec.spool_left * (1.5 - p),
                    spec.spec.spool_right * (0.5 + p),
                )
            } else {
                (
                    spec.spec.spool_left * (0.5 + p),
                    spec.spec.spool_right * (1.5 - p),
                )
            }
        } else {
            (spec.spec.spool_left, spec.spec.spool_right)
        };
        let sides = [
            (&spec.spec.left, left_mult, &mut reels.0),
            (&spec.spec.right, right_mult, &mut reels.1),
        ];
        for (i, (side, mult, turn)) in sides.into_iter().enumerate() {
            if let Some(side) = side {
                reel_angles[i] = turn.advance(side.rpm * mult, spec.spec.clockwise, spin, now_ms);
            }
        }
    }
    let linear = matches!(
        (scene.meter.kind, &scene.meter.linear),
        (MeterKind::Linear, Some(_))
    );
    let needle_turns = if scene.meter.visible && !linear {
        needle_turns(scene, &stack, needles, now_ms)
    } else {
        Vec::new()
    };
    let tonearm_turn = match (&scene.tonearm, stack.tonearm) {
        (Some(spec), Some(picture)) => {
            let key = needles.ensure(
                2,
                picture,
                (spec.pivot_image.0 as f32, spec.pivot_image.1 as f32),
                tonearm.angle(),
                now_ms,
            );
            Some((
                key,
                (spec.pivot_screen.0 as f32, spec.pivot_screen.1 as f32),
            ))
        }
        _ => None,
    };
    let text_plans: Vec<TextPlan> = scene
        .texts
        .iter()
        .map(|t| text.advance(t, stack.fonts, now_ms))
        .collect();
    // One motion per box, grown as the scene asks; the boxes keep their order.
    while analyser.len() < scene.analysers.len() {
        analyser.push(AnalyserMotion::default());
    }
    let analyser_pictures: Vec<(i32, i32, &Frame)> = scene
        .analysers
        .iter()
        .zip(analyser.iter_mut())
        .map(|(a, motion)| (a.x, a.y, motion.advance_with_fonts(a, now_ms, stack.fonts)))
        .collect();
    while spectrum.len() < scene.spectra.len() {
        spectrum.push(SpectrumMotion::default());
    }
    let spectrum_plans: Vec<Option<SpectrumPlan>> = scene
        .spectra
        .iter()
        .enumerate()
        .zip(spectrum.iter_mut())
        .map(|((i, spec), motion)| {
            let heights = scene.bar_heights.get(i).map(Vec::as_slice).unwrap_or(&[]);
            stack.spectra.get(i).map(|_| motion.advance(spec, heights))
        })
        .collect();
    labels.sweep(now_ms);
    let type_label = scene.type_area.as_ref().and_then(|area| {
        labels.ensure(
            stack.fonts,
            area.font_style,
            area.font_size,
            area.color,
            &area.label,
            now_ms,
        )
    });
    let indicator_labels = scene
        .indicators
        .as_ref()
        .map(|i| indicator_labels(i, stack.fonts, labels, now_ms));
    let overlay = fade.overlay(now_ms);

    // The drawing steps, in the theme's order.
    let mut ops: Vec<Op> = Vec::with_capacity(48);
    if let Some(spec) = &scene.reels {
        for (i, (side, picture)) in [
            (&spec.spec.left, stack.reels.0),
            (&spec.spec.right, stack.reels.1),
        ]
        .into_iter()
        .enumerate()
        {
            if let (Some(side), Some(picture)) = (side, picture) {
                let reach = reaches.reach(picture, centre_of(picture));
                ops.push(Op::Turn {
                    src: picture,
                    pivot_image: centre_of(picture),
                    pivot_screen: (side.center.0 as f32, side.center.1 as f32),
                    degrees: -reel_angles[i],
                    smooth: false,
                    reach,
                });
            }
        }
    }
    if let (Some(v), Some(picture)) = (&scene.vinyl, stack.vinyl) {
        let reach = reaches.reach(picture, centre_of(picture));
        ops.push(Op::Turn {
            src: picture,
            pivot_image: centre_of(picture),
            pivot_screen: (v.spec.center.0 as f32, v.spec.center.1 as f32),
            degrees: -angle,
            smooth: false,
            reach,
        });
    }
    if let (Some(art), Some(place)) = (stack.art, &scene.art) {
        let color = [
            place.border_color[0],
            place.border_color[1],
            place.border_color[2],
            255,
        ];
        if place.rotation && place.rpm > 0.0 {
            // Turning art sits on the record's centre when there is one.
            let centre = scene
                .vinyl
                .as_ref()
                .map(|v| (v.spec.center.0 as f32, v.spec.center.1 as f32))
                .unwrap_or((
                    place.x as f32 + place.w as f32 / 2.0,
                    place.y as f32 + place.h as f32 / 2.0,
                ));
            let reach = reaches.reach(art, centre_of(art));
            ops.push(Op::Turn {
                src: art,
                pivot_image: centre_of(art),
                pivot_screen: centre,
                degrees: -angle,
                smooth: false,
                reach,
            });
            let (cx, cy) = (centre.0.round() as i32, centre.1.round() as i32);
            if place.border > 0 {
                ops.push(Op::Ring {
                    cx,
                    cy,
                    r: (place.w.min(place.h) / 2) as i32,
                    thickness: place.border as i32,
                    color,
                });
            }
            ops.push(Op::Ring {
                cx,
                cy,
                r: 5,
                thickness: 6,
                color,
            });
            ops.push(Op::Ring {
                cx,
                cy,
                r: (place.w.min(place.h) / 10).max(3) as i32,
                thickness: 1,
                color,
            });
        } else {
            ops.push(blit_op(art, (place.x as i32, place.y as i32)));
            if place.border > 0 {
                ops.push(Op::Border {
                    rect: (place.x, place.y, place.w, place.h),
                    thickness: place.border,
                    color,
                });
            }
        }
    }
    plan_folder_layers(scene, stack.folder_pictures, ZOrder::Background, &mut ops);
    plan_fanart(
        scene.fanart.as_ref(),
        stack.fanart,
        ZOrder::Background,
        &mut ops,
    );
    if scene.meter.visible {
        if let (true, Some(linear), Some(sprite)) = (linear, &scene.meter.linear, stack.needle) {
            let right_sprite = stack.needle_right.or(stack.needle);
            if scene.meter.channels == 1 {
                if let Some(at) = scene.meter.mono_at {
                    ops.extend(bar_op(
                        sprite,
                        at,
                        linear.bar_width(scene.mono),
                        linear,
                        true,
                    ));
                }
            } else {
                if let Some(at) = scene.left_at {
                    ops.extend(bar_op(
                        sprite,
                        at,
                        linear.bar_width(scene.left),
                        linear,
                        true,
                    ));
                }
                if let (Some(at), Some(sprite)) = (scene.right_at, right_sprite) {
                    ops.extend(bar_op(
                        sprite,
                        at,
                        linear.bar_width(scene.right),
                        linear,
                        false,
                    ));
                }
            }
        }
        for (key, at) in &needle_turns {
            ops.extend(turned_op(needles, *key, *at));
        }
    }
    for (i, spec) in scene.spectra.iter().enumerate() {
        if let (Some(assets), Some(Some(plan))) = (stack.spectra.get(i), spectrum_plans.get(i)) {
            plan_spectrum(spec, assets, plan, &mut ops);
        }
    }
    for (x, y, picture) in analyser_pictures {
        ops.push(blit_op(picture, (x, y)));
    }
    // A scene without a theme shows plain meter columns and spectrum bars.
    if stack.base.is_none() && stack.screen.is_none() && stack.face.is_none() {
        let layout = layout(width, height);
        let left_meter = place(layout.left_meter, scene.left_at, width, height);
        let right_meter = place(layout.right_meter, scene.right_at, width, height);
        ops.push(Op::Column {
            rect: left_meter,
            level: scene.left,
            color: METER,
        });
        ops.push(Op::Column {
            rect: right_meter,
            level: scene.right,
            color: METER,
        });
        plan_bars(&layout.spectrum, &scene.bars, BAR, &mut ops);
    }
    for plan in &text_plans {
        if let Some(line) = text.line(plan.key) {
            ops.push(text_op(plan, line));
        }
    }
    if let Some((key, at)) = tonearm_turn {
        ops.extend(turned_op(needles, key, at));
    }
    if let (Some(indicators), Some(assets), Some(names)) =
        (&scene.indicators, stack.indicators, &indicator_labels)
    {
        plan_indicators(indicators, assets, labels, names, &mut ops);
    }
    if let Some(area) = &scene.type_area {
        plan_type_area(
            area,
            stack.icon,
            type_label.as_deref().and_then(|k| labels.get(k)),
            &mut ops,
        );
    }
    // Overlay folder layers sit above everything but the meter foreground,
    // which the meter engine draws last of all.
    plan_folder_layers(scene, stack.folder_pictures, ZOrder::Overlay, &mut ops);
    plan_fanart(
        scene.fanart.as_ref(),
        stack.fanart,
        ZOrder::Overlay,
        &mut ops,
    );
    if let Some(front) = stack.front {
        ops.push(Op::Front {
            spans: front,
            at: stack.face_at,
        });
    }
    if let Some((color, alpha)) = overlay {
        ops.push(Op::Fade { color, alpha });
    }
    // Only the boxes of steps that differ from the last frame's are
    // painted; the canvas keeps the rest. A new base or size repaints all.
    let steps: Vec<(u64, Option<Box4>)> = ops
        .iter()
        .map(|op| (op.key(), op.bounds(width, height)))
        .collect();
    let base_id = (base.rgba.as_ptr() as usize, base.rgba.len());
    let whole = *paint_all
        || last_steps.is_empty()
        || *last_base != base_id
        || canvas.width != width
        || canvas.height != height;
    let rects: Vec<Rect> = if whole {
        vec![Rect {
            x: 0,
            y: 0,
            w: width,
            h: height,
        }]
    } else {
        merge_boxes(changed_boxes(last_steps, &steps))
            .into_iter()
            .map(box_rect)
            .collect()
    };
    *last_steps = steps;
    *last_base = base_id;
    stages.mark("prep");
    canvas.width = width;
    canvas.height = height;
    canvas.rgba.resize((width * height * 4) as usize, 0);
    let painting = clock_us();
    paint(canvas, base, &ops, &rects, painters.active);
    if let Some(delay) = bench_delay {
        std::thread::sleep(*delay);
    }
    painters.settle(clock_us().saturating_sub(painting), now_ms);
    *damage = rects;
    stages.mark("paint");
    *profile = taken;
    Painted {
        frame: canvas,
        damage,
    }
}

/// A finished frame and the boxes of it that this raster painted; the rest
/// is as the frame before. Empty boxes mean the frame is the one before.
pub struct Painted<'m> {
    pub frame: &'m Frame,
    pub damage: &'m [Rect],
}

/// The needle turns of this frame, each turned once and kept: the cache key
/// and where the pivot sits on screen, in drawing order.
fn needle_turns(
    scene: &Scene,
    stack: &Stack<'_>,
    needles: &mut Turned,
    tick: u64,
) -> Vec<((usize, i32), (f32, f32))> {
    let mut turns = Vec::with_capacity(2);
    let (Some(sprite), Some((start, stop, distance))) = (stack.needle, scene.needle) else {
        return turns;
    };
    let right_sprite = stack.needle_right.or(stack.needle);
    let (left_start, left_stop) = scene.meter.left_angles.unwrap_or((start, stop));
    let pivot = |s: &Frame| (s.width as f32 / 2.0, s.height as f32 / 2.0 + distance);
    let mut turn = |slot: usize, sprite: &Frame, at: (i32, i32), angle: f32| {
        let key = needles.ensure(slot, sprite, pivot(sprite), angle, tick);
        turns.push((key, (at.0 as f32, at.1 as f32)));
    };
    if scene.meter.channels == 1 {
        if let Some(at) = scene.meter.mono_at {
            turn(
                0,
                sprite,
                at,
                left_start + (left_stop - left_start) * scene.mono,
            );
        }
    } else {
        if let Some(at) = scene.left_at {
            turn(
                0,
                sprite,
                at,
                left_start + (left_stop - left_start) * scene.left,
            );
        }
        if let (Some(at), Some(sprite)) = (scene.right_at, right_sprite) {
            let (right_start, right_stop) = scene.meter.right_angles.unwrap_or((start, stop));
            let slot = if stack.needle_right.is_some() { 1 } else { 0 };
            turn(
                slot,
                sprite,
                at,
                right_start + (right_stop - right_start) * scene.right,
            );
        }
    }
    turns
}

/// A strip of a frame's rows with the box being painted in it: columns
/// `x0..x1` of rows `y0..y1`. Every primitive draws through a band and
/// clips to its box, so the bands of one frame can be painted at the same
/// time, and only the boxes that changed need painting at all.
pub struct Band<'a> {
    rgba: &'a mut [u8],
    width: u32,
    /// The frame row the slice starts at.
    top: u32,
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
}

impl<'a> Band<'a> {
    /// A whole buffer of `width` by `height` pixels.
    pub fn over(rgba: &'a mut [u8], width: u32, height: u32) -> Self {
        Self {
            rgba,
            width,
            top: 0,
            x0: 0,
            x1: width,
            y0: 0,
            y1: height,
        }
    }

    /// A whole frame.
    pub fn whole(frame: &'a mut Frame) -> Self {
        let (width, height) = (frame.width, frame.height);
        Self::over(&mut frame.rgba, width, height)
    }

    /// The rows in `from..to` that fall in this band's box.
    fn rows(&self, from: i32, to: i32) -> std::ops::Range<u32> {
        let from = from.max(self.y0 as i32);
        let to = to.min(self.y1 as i32);
        if to <= from {
            0..0
        } else {
            from as u32..to as u32
        }
    }

    /// The columns in `from..to` that fall in this band's box.
    fn cols(&self, from: i32, to: i32) -> std::ops::Range<u32> {
        let from = from.max(self.x0 as i32);
        let to = to.min(self.x1 as i32);
        if to <= from {
            0..0
        } else {
            from as u32..to as u32
        }
    }

    /// One row, `width` pixels. `y` comes from `rows`.
    fn row(&mut self, y: u32) -> &mut [u8] {
        let stride = self.width as usize * 4;
        let start = (y - self.top) as usize * stride;
        &mut self.rgba[start..start + stride]
    }
}

/// A box on the frame: x0, y0, x1, y1, the last two exclusive.
type Box4 = (i32, i32, i32, i32);

fn clip_box(b: Box4, width: u32, height: u32) -> Option<Box4> {
    let clipped = (
        b.0.max(0),
        b.1.max(0),
        b.2.min(width as i32),
        b.3.min(height as i32),
    );
    (clipped.2 > clipped.0 && clipped.3 > clipped.1).then_some(clipped)
}

fn boxes_meet(a: Box4, b: Box4) -> bool {
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}

fn box_union(a: Box4, b: Box4) -> Box4 {
    (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
}

fn box_area(b: Box4) -> i64 {
    (b.2 - b.0) as i64 * (b.3 - b.1) as i64
}

fn box_rect(b: Box4) -> Rect {
    Rect {
        x: b.0 as u32,
        y: b.1 as u32,
        w: (b.2 - b.0) as u32,
        h: (b.3 - b.1) as u32,
    }
}

/// How far a picture reaches from its pivot when turned, plus a pixel.
fn reach_of(src: &Frame, pivot: (f32, f32)) -> f32 {
    let (px, py) = pivot;
    let (sw, sh) = (src.width as f32, src.height as f32);
    [(0.0, 0.0), (sw, 0.0), (0.0, sh), (sw, sh)]
        .iter()
        .map(|(x, y)| ((x - px).powi(2) + (y - py).powi(2)).sqrt())
        .fold(0.0f32, f32::max)
        + 1.0
}

/// How far from its pivot each turning picture has pixels, measured once
/// per picture, so the box a turn paints is the picture's, not its
/// diagonal's: a round record in a square picture claims a box the size
/// of the record.
#[derive(Default)]
pub struct Reaches {
    known: Vec<((usize, usize, u32, u32), f32)>,
}

impl Reaches {
    fn reach(&mut self, src: &Frame, pivot: (f32, f32)) -> f32 {
        let key = (
            src.rgba.as_ptr() as usize,
            src.rgba.len(),
            pivot.0.to_bits(),
            pivot.1.to_bits(),
        );
        if let Some((_, reach)) = self.known.iter().find(|(k, _)| *k == key) {
            return *reach;
        }
        let mut furthest = 0.0f32;
        for y in 0..src.height {
            let row = &src.rgba[(y * src.width * 4) as usize..((y + 1) * src.width * 4) as usize];
            let dy = y as f32 + 0.5 - pivot.1;
            // The first and last pixel of the row with any alpha are its furthest.
            let first = (0..src.width).find(|&x| row[(x * 4 + 3) as usize] != 0);
            let last = (0..src.width)
                .rev()
                .find(|&x| row[(x * 4 + 3) as usize] != 0);
            for x in [first, last].into_iter().flatten() {
                for edge in [x as f32, x as f32 + 1.0] {
                    furthest =
                        furthest.max(((edge - pivot.0).powi(2) + (dy.abs() + 0.5).powi(2)).sqrt());
                }
            }
        }
        let reach = (furthest + 1.0).min(reach_of(src, pivot));
        if self.known.len() >= 16 {
            self.known.remove(0);
        }
        self.known.push((key, reach));
        reach
    }
}

/// FNV-1a over a step's fields, the pictures by where their pixels live.
struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
    fn tag(&mut self, tag: u8) {
        self.bytes(&[tag]);
    }
    fn i32s(&mut self, values: &[i32]) {
        for v in values {
            self.bytes(&v.to_le_bytes());
        }
    }
    fn u32s(&mut self, values: &[u32]) {
        for v in values {
            self.bytes(&v.to_le_bytes());
        }
    }
    fn f32s(&mut self, values: &[f32]) {
        for v in values {
            self.bytes(&v.to_bits().to_le_bytes());
        }
    }
    /// A picture is the same picture while its pixels live at the same
    /// place with the same length; a replacement is made before the old
    /// one goes, so it never takes the old place.
    fn pic(&mut self, picture: &Frame) {
        self.buf(picture.rgba.as_ptr() as usize, picture.rgba.len());
    }
    fn buf(&mut self, at: usize, len: usize) {
        self.bytes(&at.to_le_bytes());
        self.bytes(&len.to_le_bytes());
    }
}

/// One drawing step of a frame, planned before painting. A step only reads
/// its pictures, so the bands of a frame can paint it at the same time.
enum Op<'a> {
    /// Blend `part` (x, y, w, h) of a picture at `at`, its alpha scaled by
    /// `alpha`, showing only what falls inside `clip` (x, y, w, h).
    Blit {
        src: &'a Frame,
        at: (i32, i32),
        part: (u32, u32, u32, u32),
        clip: Option<(i32, i32, i32, i32)>,
        alpha: u8,
    },
    /// A picture turned about `pivot_image`, that point on `pivot_screen`.
    /// `reach` is how far from the pivot the picture has pixels.
    Turn {
        src: &'a Frame,
        pivot_image: (f32, f32),
        pivot_screen: (f32, f32),
        degrees: f32,
        smooth: bool,
        reach: f32,
    },
    /// A rectangle outline inside the box.
    Border {
        rect: (u32, u32, u32, u32),
        thickness: u32,
        color: [u8; 4],
    },
    /// A ring of `thickness` inside radius `r`; a thickness past `r` fills the disc.
    Ring {
        cx: i32,
        cy: i32,
        r: i32,
        thickness: i32,
        color: [u8; 4],
    },
    RoundRect {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        radius: i32,
        color: [u8; 4],
    },
    Arc {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        start: f32,
        stop: f32,
        ring: u32,
        color: [u8; 3],
    },
    /// A meter column lit from the bottom, for a scene without a theme.
    Column {
        rect: Rect,
        level: f32,
        color: [u8; 4],
    },
    /// The meter foreground, blended only where it has pixels.
    Front { spans: &'a Spans, at: (u32, u32) },
    /// A colour over the whole frame at `alpha`.
    Fade { color: [u8; 3], alpha: u8 },
}

impl Op<'_> {
    /// The box the step may paint, clipped to the frame.
    fn bounds(&self, width: u32, height: u32) -> Option<Box4> {
        let b = match *self {
            Op::Blit {
                src,
                at,
                part,
                clip,
                ..
            } => {
                let mut b = (
                    at.0,
                    at.1,
                    at.0.saturating_add(part.2.min(src.width) as i32),
                    at.1.saturating_add(part.3.min(src.height) as i32),
                );
                if let Some((cx, cy, cw, ch)) = clip {
                    b = (
                        b.0.max(cx),
                        b.1.max(cy),
                        b.2.min(cx.saturating_add(cw)),
                        b.3.min(cy.saturating_add(ch)),
                    );
                }
                b
            }
            Op::Turn {
                pivot_screen,
                reach,
                ..
            } => (
                (pivot_screen.0 - reach).floor() as i32,
                (pivot_screen.1 - reach).floor() as i32,
                (pivot_screen.0 + reach).ceil() as i32 + 1,
                (pivot_screen.1 + reach).ceil() as i32 + 1,
            ),
            Op::Border { rect, .. } => (
                rect.0 as i32,
                rect.1 as i32,
                rect.0.saturating_add(rect.2) as i32,
                rect.1.saturating_add(rect.3) as i32,
            ),
            Op::Ring { cx, cy, r, .. } => (cx - r, cy - r, cx + r + 1, cy + r + 1),
            Op::RoundRect { x, y, w, h, .. } => (x, y, x.saturating_add(w), y.saturating_add(h)),
            Op::Arc { x, y, w, h, .. } => {
                (x, y, x.saturating_add(w as i32), y.saturating_add(h as i32))
            }
            Op::Column { rect, .. } => (
                rect.x as i32,
                rect.y as i32,
                (rect.x + rect.w) as i32,
                (rect.y + rect.h) as i32,
            ),
            Op::Front { spans, at } => (
                at.0 as i32,
                at.1 as i32,
                (at.0 + spans.width) as i32,
                (at.1 + spans.height) as i32,
            ),
            Op::Fade { .. } => (0, 0, width as i32, height as i32),
        };
        clip_box(b, width, height)
    }

    /// A key that tells one step from another: the same step keyed twice
    /// paints the same pixels.
    fn key(&self) -> u64 {
        let mut h = Fnv::default();
        match *self {
            Op::Blit {
                src,
                at,
                part,
                clip,
                alpha,
            } => {
                h.tag(1);
                h.pic(src);
                h.i32s(&[at.0, at.1]);
                h.u32s(&[part.0, part.1, part.2, part.3]);
                match clip {
                    Some(c) => h.i32s(&[c.0, c.1, c.2, c.3]),
                    None => h.tag(0),
                }
                h.tag(alpha);
            }
            Op::Turn {
                src,
                pivot_image,
                pivot_screen,
                degrees,
                smooth,
                ..
            } => {
                h.tag(2);
                h.pic(src);
                h.f32s(&[
                    pivot_image.0,
                    pivot_image.1,
                    pivot_screen.0,
                    pivot_screen.1,
                    degrees,
                ]);
                h.tag(smooth as u8);
            }
            Op::Border {
                rect,
                thickness,
                color,
            } => {
                h.tag(3);
                h.u32s(&[rect.0, rect.1, rect.2, rect.3, thickness]);
                h.bytes(&color);
            }
            Op::Ring {
                cx,
                cy,
                r,
                thickness,
                color,
            } => {
                h.tag(4);
                h.i32s(&[cx, cy, r, thickness]);
                h.bytes(&color);
            }
            Op::RoundRect {
                x,
                y,
                w,
                h: hh,
                radius,
                color,
            } => {
                h.tag(5);
                h.i32s(&[x, y, w, hh, radius]);
                h.bytes(&color);
            }
            Op::Arc {
                x,
                y,
                w,
                h: hh,
                start,
                stop,
                ring,
                color,
            } => {
                h.tag(6);
                h.i32s(&[x, y]);
                h.u32s(&[w, hh, ring]);
                h.f32s(&[start, stop]);
                h.bytes(&color);
            }
            Op::Column { rect, level, color } => {
                h.tag(7);
                h.u32s(&[rect.x, rect.y, rect.w, rect.h]);
                h.f32s(&[level]);
                h.bytes(&color);
            }
            Op::Front { spans, at } => {
                h.tag(8);
                h.buf(spans.pixels.as_ptr() as usize, spans.pixels.len());
                h.u32s(&[at.0, at.1]);
            }
            Op::Fade { color, alpha } => {
                h.tag(9);
                h.bytes(&color);
                h.tag(alpha);
            }
        }
        h.0
    }

    fn paint(&self, band: &mut Band) {
        match *self {
            Op::Blit {
                src,
                at,
                part,
                clip,
                alpha,
            } => blit_into(band, src, at, part, clip, alpha),
            Op::Turn {
                src,
                pivot_image,
                pivot_screen,
                degrees,
                smooth,
                ..
            } => turn_onto(band, src, pivot_image, pivot_screen, degrees, smooth, false),
            Op::Border {
                rect,
                thickness,
                color,
            } => draw_border(band, rect, thickness, color),
            Op::Ring {
                cx,
                cy,
                r,
                thickness,
                color,
            } => draw_ring(band, cx, cy, r, thickness, color),
            Op::RoundRect {
                x,
                y,
                w,
                h,
                radius,
                color,
            } => fill_round_rect(band, x, y, w, h, radius, color),
            Op::Arc {
                x,
                y,
                w,
                h,
                start,
                stop,
                ring,
                color,
            } => fill_arc(band, x, y, w, h, start, stop, ring, color),
            Op::Column { rect, level, color } => fill_column(band, &rect, level, color),
            Op::Front { spans, at } => spans.blit(band, at),
            Op::Fade { color, alpha } => fade_band(band, color, alpha),
        }
    }
}

/// Rows in one unit of painting; a frame is many units, so the threads
/// share the heavy strips as well as the light ones.
const BAND_ROWS: usize = 16;

/// Copy the base into the canvas inside `rects` and paint the steps that
/// touch them, in row bands shared among `threads` threads. One thread
/// paints in place. Nothing outside the rectangles is touched.
fn paint(canvas: &mut Frame, base: &Frame, ops: &[Op], rects: &[Rect], threads: usize) {
    if rects.is_empty() {
        return;
    }
    let width = canvas.width;
    let stride = width as usize * 4;
    if stride == 0 {
        return;
    }
    let boxes: Vec<Option<Box4>> = ops
        .iter()
        .map(|op| op.bounds(width, canvas.height))
        .collect();
    // The row strips some rectangle touches, each with its pieces of them.
    let units: Vec<(Band, Vec<Box4>)> = canvas
        .rgba
        .chunks_mut(BAND_ROWS * stride)
        .enumerate()
        .filter_map(|(i, chunk)| {
            let top = (i * BAND_ROWS) as u32;
            let bottom = top + (chunk.len() / stride) as u32;
            let pieces: Vec<Box4> = rects
                .iter()
                .filter_map(|r| {
                    let (y0, y1) = (r.y.max(top), (r.y + r.h).min(bottom));
                    (y1 > y0 && r.w > 0).then_some((
                        r.x as i32,
                        y0 as i32,
                        (r.x + r.w).min(width) as i32,
                        y1 as i32,
                    ))
                })
                .collect();
            (!pieces.is_empty()).then_some((
                Band {
                    rgba: chunk,
                    width,
                    top,
                    x0: 0,
                    x1: width,
                    y0: top,
                    y1: bottom,
                },
                pieces,
            ))
        })
        .collect();
    let work = |band: &mut Band, pieces: &[Box4]| {
        for &piece in pieces {
            let (x0, y0, x1, y1) = piece;
            if x1 <= x0 {
                continue;
            }
            band.x0 = x0 as u32;
            band.x1 = x1 as u32;
            band.y0 = y0 as u32;
            band.y1 = y1 as u32;
            let (from_x, bytes) = (x0 as usize * 4, (x1 - x0) as usize * 4);
            for y in y0 as u32..y1 as u32 {
                let from = y as usize * stride + from_x;
                band.row(y)[from_x..from_x + bytes].copy_from_slice(&base.rgba[from..from + bytes]);
            }
            for (op, bounds) in ops.iter().zip(&boxes) {
                if bounds.is_some_and(|b| boxes_meet(b, piece)) {
                    op.paint(band);
                }
            }
        }
    };
    let threads = threads.clamp(1, units.len().max(1));
    if threads == 1 {
        for (band, pieces) in units.into_iter() {
            let mut band = band;
            work(&mut band, &pieces);
        }
        return;
    }
    let queue = std::sync::Mutex::new(units);
    let pull = || loop {
        let next = queue.lock().map(|mut q| q.pop()).unwrap_or(None);
        let Some((mut band, pieces)) = next else {
            break;
        };
        work(&mut band, &pieces);
    };
    std::thread::scope(|scope| {
        for _ in 1..threads {
            scope.spawn(pull);
        }
        pull();
    });
}

/// The boxes that differ between the last frame's steps and this frame's:
/// steps that appeared, went, or changed, told apart by their keys.
fn changed_boxes(prev: &[(u64, Option<Box4>)], cur: &[(u64, Option<Box4>)]) -> Vec<Box4> {
    let mut a = prev.to_vec();
    a.sort_by_key(|e| e.0);
    let mut b = cur.to_vec();
    b.sort_by_key(|e| e.0);
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    loop {
        match (a.get(i), b.get(j)) {
            (Some(x), Some(y)) if x.0 == y.0 => {
                i += 1;
                j += 1;
            }
            (Some(x), Some(y)) if x.0 < y.0 => {
                out.extend(x.1);
                i += 1;
            }
            (Some(_), Some(y)) => {
                out.extend(y.1);
                j += 1;
            }
            (Some(x), None) => {
                out.extend(x.1);
                i += 1;
            }
            (None, Some(y)) => {
                out.extend(y.1);
                j += 1;
            }
            (None, None) => break,
        }
    }
    out
}

/// Boxes that meet become one when their union is no more to paint than
/// both, since an overlap painted twice comes out the same; past eight,
/// the pair whose union wastes the least is merged until eight remain.
fn merge_boxes(mut boxes: Vec<Box4>) -> Vec<Box4> {
    // The boxes come in the order of their steps' keys, which carry
    // picture identities and differ from run to run; the merge goes by
    // geometry so the same scene always paints the same boxes.
    boxes.sort_unstable();
    loop {
        // Of the pairs that meet, the one whose union wastes the least
        // (an overlap counts as a saving) is merged first.
        let mut best: Option<(i64, usize, usize)> = None;
        for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                if !boxes_meet(boxes[i], boxes[j]) {
                    continue;
                }
                let waste = box_area(box_union(boxes[i], boxes[j]))
                    - box_area(boxes[i])
                    - box_area(boxes[j]);
                if waste <= 0 && best.is_none_or(|b| waste < b.0) {
                    best = Some((waste, i, j));
                }
            }
        }
        let Some((_, i, j)) = best else { break };
        let union = box_union(boxes[i], boxes[j]);
        boxes.swap_remove(j);
        boxes[i] = union;
    }
    while boxes.len() > 8 {
        let mut best = (i64::MAX, 0, 1);
        for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                let waste = box_area(box_union(boxes[i], boxes[j]))
                    - box_area(boxes[i])
                    - box_area(boxes[j]);
                if waste < best.0 {
                    best = (waste, i, j);
                }
            }
        }
        let union = box_union(boxes[best.1], boxes[best.2]);
        boxes.swap_remove(best.2);
        boxes[best.1] = union;
    }
    boxes
}

fn paint_onto(band: &mut Band, ops: &[Op]) {
    for op in ops {
        op.paint(band);
    }
}

fn fade_band(band: &mut Band, color: [u8; 3], alpha: u8) {
    let (x0, x1) = (band.x0 as usize * 4, band.x1 as usize * 4);
    for y in band.y0..band.y1 {
        for px in band.row(y)[x0..x1].as_chunks_mut::<4>().0 {
            blend(px, 0, [color[0], color[1], color[2], alpha]);
        }
    }
}

fn whole(picture: &Frame) -> (u32, u32, u32, u32) {
    (0, 0, picture.width, picture.height)
}

fn blit_op(src: &Frame, at: (i32, i32)) -> Op<'_> {
    Op::Blit {
        src,
        at,
        part: whole(src),
        clip: None,
        alpha: 255,
    }
}

fn centre_of(picture: &Frame) -> (f32, f32) {
    (picture.width as f32 / 2.0, picture.height as f32 / 2.0)
}

/// A kept turn blitted with its top left offset from the pivot on screen.
fn turned_op(needles: &Turned, key: (usize, i32), at: (f32, f32)) -> Option<Op<'_>> {
    let (picture, offset) = needles.get(key)?;
    Some(blit_op(
        picture,
        (
            at.0.round() as i32 + offset.0,
            at.1.round() as i32 + offset.1,
        ),
    ))
}

fn text_op<'a>(plan: &TextPlan, line: &'a Frame) -> Op<'a> {
    Op::Blit {
        src: line,
        at: plan.at,
        part: (0, 0, plan.width.unwrap_or(line.width), line.height),
        clip: plan.clip,
        alpha: 255,
    }
}

fn align_text_x(box_x: u32, box_w: u32, item_w: u32, align: TextAlign) -> u32 {
    match align {
        TextAlign::Left => box_x,
        TextAlign::Right => box_x + box_w.saturating_sub(item_w),
        TextAlign::Center => box_x + box_w.saturating_sub(item_w) / 2,
    }
}

/// Where one text's line goes this frame: the line's key, its top left,
/// how much of its width shows, and the box that clips a moving one.
pub struct TextPlan {
    key: (u32, u32),
    at: (i32, i32),
    width: Option<u32>,
    clip: Option<(i32, i32, i32, i32)>,
}

impl TextMotion {
    /// Advance one text and say where its line goes. A text that fits is
    /// placed by its alignment. A wider text moves as the player moves it:
    /// it bounces between its ends with a pause, or, as a ticker, loops by
    /// one segment. The drawn position is the box left edge minus the
    /// offset, and the box clips. The line is set in type once and kept.
    pub fn advance(&mut self, text: &Text, fonts: Option<&Fonts>, now_ms: u64) -> TextPlan {
        let key = (text.x, text.y);
        let line_key = format!(
            "{}\0{:?}\0{}\0{:?}\0{}",
            text.text, text.style, text.size, text.color, text.font_file
        );
        if self.lines.get(&key).is_none_or(|(k, _)| *k != line_key) {
            let font = fonts.and_then(|f| f.get_for(text.style, &text.font_file));
            let fallback = fonts.and_then(Fonts::fallback);
            let line = render_line(font, fallback, text.size, text.color, &text.text, 0)
                .unwrap_or_else(|| bitmap_line(&text.text));
            self.lines.insert(key, (line_key, line));
        }
        let line_w = self.lines[&key].1.width;
        let box_w = text.max_width;
        if box_w == 0 || line_w <= box_w || text.speed <= 0.0 {
            self.states.remove(&key);
            let x = if box_w == 0 {
                text.x
            } else {
                align_text_x(text.x, box_w, line_w, text.align)
            };
            return TextPlan {
                key,
                at: (x as i32, text.y as i32),
                width: (box_w > 0).then_some(box_w.min(line_w)),
                clip: None,
            };
        }
        let limit = (line_w - box_w) as f32;
        let segment = if text.loop_thirds {
            (line_w / 3) as f32
        } else {
            0.0
        };
        let state = self.states.entry(key).or_default();
        if state.text != text.text || state.box_w != box_w {
            state.text = text.text.clone();
            state.box_w = box_w;
            if text.direction == ScrollDirection::Rtl {
                state.offset = limit;
                state.dir = -1.0;
            } else {
                state.offset = 0.0;
                state.dir = 1.0;
            }
            state.pause_until = 0;
            state.last_ms = now_ms;
        }
        let dt = now_ms.saturating_sub(state.last_ms) as f32 / 1000.0;
        state.last_ms = now_ms;
        if now_ms >= state.pause_until {
            state.offset += state.dir * text.speed * dt;
            match text.direction {
                ScrollDirection::Bounce => {
                    if state.offset <= 0.0 {
                        state.offset = 0.0;
                        state.dir = 1.0;
                        state.pause_until = now_ms + SCROLL_PAUSE_MS;
                    } else if state.offset >= limit {
                        state.offset = limit;
                        state.dir = -1.0;
                        state.pause_until = now_ms + SCROLL_PAUSE_MS;
                    }
                }
                ScrollDirection::Ltr => {
                    if segment > 0.0 {
                        while state.offset >= segment {
                            state.offset -= segment;
                        }
                    } else if state.offset >= limit {
                        state.offset = limit;
                        state.pause_until = now_ms + SCROLL_PAUSE_MS;
                    }
                }
                ScrollDirection::Rtl => {
                    if segment > 0.0 {
                        while state.offset <= limit - segment {
                            state.offset += segment;
                        }
                    } else if state.offset <= 0.0 {
                        state.offset = 0.0;
                        state.pause_until = now_ms + SCROLL_PAUSE_MS;
                    }
                }
            }
        }
        let draw_x = text.x as i32 - state.offset.floor() as i32;
        TextPlan {
            key,
            at: (draw_x, text.y as i32),
            width: None,
            clip: Some((text.x as i32, 0, box_w as i32, i32::MAX / 2)),
        }
    }

    /// The line set for a text position.
    pub fn line(&self, key: (u32, u32)) -> Option<&Frame> {
        self.lines.get(&key).map(|(_, line)| line)
    }
}

/// Draw one text in its box straight onto a frame.
pub fn draw_text_moving(
    frame: &mut Frame,
    text: &Text,
    fonts: Option<&Fonts>,
    motion: &mut TextMotion,
    now_ms: u64,
) {
    let plan = motion.advance(text, fonts, now_ms);
    if let Some(line) = motion.line(plan.key) {
        text_op(&plan, line).paint(&mut Band::whole(frame));
    }
}

/// A rectangle outline `thickness` pixels wide, inside the box.
fn draw_border(band: &mut Band, rect: (u32, u32, u32, u32), thickness: u32, color: [u8; 4]) {
    let (x, y, w, h) = rect;
    if w == 0 || h == 0 {
        return;
    }
    let t = thickness.min(w / 2).min(h / 2).max(1);
    let cols = band.cols(x as i32, x.saturating_add(w) as i32);
    for py in band.rows(y as i32, y.saturating_add(h) as i32) {
        let row_index = py - y;
        let edge_row = row_index < t || row_index + t >= h;
        let row = band.row(py);
        for px in cols.clone() {
            let col = px - x;
            if edge_row || col < t || col + t >= w {
                let i = (px * 4) as usize;
                row[i..i + 4].copy_from_slice(&color);
            }
        }
    }
}

fn align_x(box_x: u32, box_w: u32, item_w: u32, align: TypeAlign) -> u32 {
    match align {
        TypeAlign::Left => box_x,
        TypeAlign::Right => box_x + box_w.saturating_sub(item_w),
        TypeAlign::Center => box_x + box_w.saturating_sub(item_w) / 2,
    }
}

/// The type area's steps. Inside a real box the icon or label is clipped to
/// the box, placed by `align`, and centred vertically. `both` puts the label
/// three pixels to the icon's right and places the two as one by `align`.
/// Text mode without a box draws the label at the position.
fn plan_type_area<'a>(
    area: &TypeArea,
    icon: Option<&'a Frame>,
    label: Option<&'a Frame>,
    ops: &mut Vec<Op<'a>>,
) {
    const GAP: u32 = 3;
    let Some((w, h)) = area.box_size else {
        if let (TypeMode::Text, Some(label)) = (area.mode, label) {
            ops.push(blit_op(label, (area.x as i32, area.y as i32)));
        }
        return;
    };
    let fitted = |item: &Frame| (item.width.min(w), item.height.min(h));
    let place = |ops: &mut Vec<Op<'a>>, item: &'a Frame| {
        let (iw, ih) = fitted(item);
        let x = align_x(area.x, w, iw, area.align);
        let y = area.y + h.saturating_sub(ih) / 2;
        ops.push(Op::Blit {
            src: item,
            at: (x as i32, y as i32),
            part: (0, 0, iw, ih),
            clip: None,
            alpha: 255,
        });
    };
    match area.mode {
        TypeMode::Text => {
            if let Some(label) = label {
                place(ops, label);
            }
        }
        TypeMode::Icon => match (icon, label) {
            (Some(icon), _) => place(ops, icon),
            (None, Some(label)) => place(ops, label),
            (None, None) => {}
        },
        TypeMode::Both => match (icon, label) {
            (None, None) => {}
            (None, Some(label)) => place(ops, label),
            (Some(icon), None) => place(ops, icon),
            (Some(icon), Some(label)) => {
                let (iw, ih) = fitted(icon);
                let text_x = iw + GAP;
                if text_x >= w {
                    place(ops, icon);
                    return;
                }
                let (lw, lh) = (label.width.min(w - text_x), label.height.min(h));
                // The icon and the label as one, placed by the alignment.
                let x0 = align_x(area.x, w, text_x + lw, area.align);
                let iy = area.y + h.saturating_sub(ih) / 2;
                ops.push(Op::Blit {
                    src: icon,
                    at: (x0 as i32, iy as i32),
                    part: (0, 0, iw, ih),
                    clip: None,
                    alpha: 255,
                });
                let ty = area.y + h.saturating_sub(lh) / 2;
                ops.push(Op::Blit {
                    src: label,
                    at: ((x0 + text_x) as i32, ty as i32),
                    part: (0, 0, lw, lh),
                    clip: None,
                    alpha: 255,
                });
            }
        },
    }
}

/// Draw the type area straight onto a frame.
pub fn draw_type_area(
    frame: &mut Frame,
    area: &TypeArea,
    icon: Option<&Frame>,
    fonts: Option<&Fonts>,
) {
    let mut labels = Labels::default();
    let key = labels.ensure(
        fonts,
        area.font_style,
        area.font_size,
        area.color,
        &area.label,
        0,
    );
    let mut ops = Vec::new();
    plan_type_area(
        area,
        icon,
        key.as_deref().and_then(|k| labels.get(k)),
        &mut ops,
    );
    paint_onto(&mut Band::whole(frame), &ops);
}

/// The type icon between frames: decoded once per file, box and tint, and
/// kept until one of them changes. The display and the browser module each
/// keep one and hand its frame to `Stack::icon`.
#[derive(Default)]
pub struct TypeIcon {
    kept: Option<((String, u32, u32, Option<[u8; 3]>), Frame)>,
}

impl TypeIcon {
    /// Follow the scene's type area: decode the icon when the file, the box
    /// or the tint changed, drop it when the area names none. In `Both`
    /// mode the icon takes a square of the box's shorter side, the label the
    /// rest; an SVG takes the area's colour, a PNG keeps its own.
    pub fn follow(&mut self, area: Option<&TypeArea>) {
        let Some(area) = area.filter(|a| !a.icon.is_empty()) else {
            self.kept = None;
            return;
        };
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
        let stale = self.kept.as_ref().is_none_or(|(known, _)| *known != key);
        if stale {
            self.kept = read_icon(Path::new(&area.icon), fw, fh, tint).map(|frame| (key, frame));
        }
    }

    /// The icon as decoded, `None` without one or when the file could not
    /// be read.
    pub fn frame(&self) -> Option<&Frame> {
        self.kept.as_ref().map(|(_, frame)| frame)
    }

    /// Forget the icon, for a change of meter.
    pub fn clear(&mut self) {
        self.kept = None;
    }

    /// Bytes the kept icon holds.
    pub fn bytes(&self) -> usize {
        self.frame().map_or(0, Frame::bytes)
    }
}

/// A picture decoded and fitted for a box, kept until the file changes: on
/// a machine decoded off the frame loop, the slot showing what it has
/// until the next file is ready; on a target without threads decoded in
/// the frame that first asks for it.
#[derive(Default)]
struct PictureSlot {
    file: String,
    picture: Option<FolderPicture>,
    #[cfg(not(target_arch = "wasm32"))]
    pending: Option<(String, std::sync::mpsc::Receiver<Option<FolderPicture>>)>,
}

impl PictureSlot {
    /// Ask for `file` in this box; `ready` is a picture of that file already
    /// decoded elsewhere, taken over without decoding again.
    fn want(&mut self, file: &str, spec: &FolderLayerSpec, ready: Option<&FolderPicture>) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some((wanted, rx)) = &self.pending {
                match rx.try_recv() {
                    Ok(picture) => {
                        self.file = wanted.clone();
                        self.picture = picture;
                        self.pending = None;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => self.pending = None,
                }
            }
            if self
                .pending
                .as_ref()
                .is_some_and(|(wanted, _)| wanted == file)
            {
                return;
            }
        }
        if file == self.file {
            return;
        }
        if file.is_empty() {
            self.file.clear();
            self.picture = None;
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.pending = None;
            }
            return;
        }
        if let Some(picture) = ready {
            self.file = file.to_string();
            self.picture = Some(picture.clone());
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.pending = None;
            }
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            let (path, spec) = (file.to_string(), spec.clone());
            std::thread::spawn(move || {
                let _ = tx.send(FolderPicture::load(Path::new(&path), &spec));
            });
            self.pending = Some((file.to_string(), rx));
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.file = file.to_string();
            self.picture = FolderPicture::load(Path::new(file), spec);
        }
    }
}

/// A picture decoded and stretched to a size, or kept as it is: the record
/// and the album's reels. Off the frame loop on a machine, as the slot above.
#[derive(Default)]
struct PlainSlot {
    key: (String, Option<(u32, u32)>),
    frame: Option<Frame>,
    #[cfg(not(target_arch = "wasm32"))]
    pending: Option<(
        (String, Option<(u32, u32)>),
        std::sync::mpsc::Receiver<Option<Frame>>,
    )>,
}

impl PlainSlot {
    fn want(&mut self, file: &str, size: Option<(u32, u32)>) {
        let key = (file.to_string(), size);
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some((wanted, rx)) = &self.pending {
                match rx.try_recv() {
                    Ok(frame) => {
                        self.key = wanted.clone();
                        self.frame = frame;
                        self.pending = None;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => self.pending = None,
                }
            }
            if self
                .pending
                .as_ref()
                .is_some_and(|(wanted, _)| *wanted == key)
            {
                return;
            }
        }
        if key == self.key {
            return;
        }
        if file.is_empty() {
            self.key = key;
            self.frame = None;
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.pending = None;
            }
            return;
        }
        let decode = move |path: &str| {
            read_png(Path::new(path)).map(|f| match size {
                Some((w, h)) => fit_art(&f, w, h),
                None => f,
            })
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            let path = file.to_string();
            std::thread::spawn(move || {
                let _ = tx.send(decode(&path));
            });
            self.pending = Some((key, rx));
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.frame = decode(file);
            self.key = key;
        }
    }
}

/// Every picture a scene names, decoded once per file and box and kept
/// between frames: the album art, the type icon, the folder layers, the
/// fanart on show and the one it replaces, the record and the album's
/// reels. The display and the browser module each keep one and hand what
/// it holds to `Stack`.
#[derive(Default)]
pub struct Pictures {
    art: Option<(Art, Frame)>,
    icon: TypeIcon,
    folder: Vec<PictureSlot>,
    /// The folder slots' pictures as `Stack` takes them, rebuilt when a
    /// slot's file changes.
    folder_pictures: Vec<Option<FolderPicture>>,
    folder_files: Vec<String>,
    fanart: (PictureSlot, PictureSlot),
    vinyl: PlainSlot,
    reels: (PlainSlot, PlainSlot),
}

impl Pictures {
    /// Follow the scene: decode what changed, drop what it no longer names.
    /// The art is decoded here and now; the rest as the slots do it.
    pub fn follow(&mut self, scene: &Scene, assets: &MeterAssets) {
        match &scene.art {
            Some(art) => {
                let stale = self.art.as_ref().is_none_or(|(known, _)| known != art);
                if stale {
                    self.art =
                        read_art(Path::new(&art.file), art.w, art.h, assets.art_mask.as_ref())
                            .map(|frame| {
                                if art.rotation && art.mask.is_empty() {
                                    apply_circle(&frame)
                                } else {
                                    frame
                                }
                            })
                            .map(|frame| (art.clone(), frame));
                }
            }
            None => self.art = None,
        }
        self.icon.follow(scene.type_area.as_ref());
        if self.folder.len() != scene.folder_layers.len() {
            self.folder = scene
                .folder_layers
                .iter()
                .map(|_| PictureSlot::default())
                .collect();
        }
        for (slot, layer) in self.folder.iter_mut().zip(scene.folder_layers.iter()) {
            slot.want(&layer.file, &layer.spec, None);
        }
        if self
            .folder
            .iter()
            .map(|slot| slot.file.as_str())
            .ne(self.folder_files.iter().map(String::as_str))
        {
            self.folder_files = self.folder.iter().map(|slot| slot.file.clone()).collect();
            self.folder_pictures = self
                .folder
                .iter()
                .map(|slot| slot.picture.clone())
                .collect();
        }
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
            let handed_over = if self.fanart.0.file == fanart.prev_file {
                self.fanart.0.picture.clone()
            } else {
                None
            };
            self.fanart
                .1
                .want(&fanart.prev_file, &spec, handed_over.as_ref());
            self.fanart.0.want(&fanart.file, &spec, None);
        } else {
            self.fanart = Default::default();
        }
        match &scene.vinyl {
            Some(vinyl) => self.vinyl.want(&vinyl.file, vinyl.spec.dimension),
            None => self.vinyl = PlainSlot::default(),
        }
        // A reel from the album is scaled to the theme reel's size; the
        // theme reel itself needs no slot.
        match &scene.reels {
            Some(reels) => {
                let side = |slot: &mut PlainSlot,
                            file: &str,
                            spec: Option<&lead::ReelSpec>,
                            theme: Option<&Frame>| {
                    let Some(spec) = spec else {
                        return;
                    };
                    if file.is_empty() || file == spec.theme_file {
                        slot.want("", None);
                    } else {
                        slot.want(file, theme.map(|t| (t.width, t.height)));
                    }
                };
                side(
                    &mut self.reels.0,
                    &reels.left_file,
                    reels.spec.left.as_ref(),
                    assets.reels.0.as_ref(),
                );
                side(
                    &mut self.reels.1,
                    &reels.right_file,
                    reels.spec.right.as_ref(),
                    assets.reels.1.as_ref(),
                );
            }
            None => self.reels = Default::default(),
        }
    }

    /// Forget everything, for a change of meter.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The album art stretched to its box.
    pub fn art(&self) -> Option<&Frame> {
        self.art.as_ref().map(|(_, frame)| frame)
    }

    /// The type icon fitted to its area.
    pub fn icon(&self) -> Option<&Frame> {
        self.icon.frame()
    }

    /// One entry per folder layer of the scene.
    pub fn folder_pictures(&self) -> &[Option<FolderPicture>] {
        &self.folder_pictures
    }

    /// The fanart on show and the one it replaces.
    pub fn fanart(&self) -> (Option<&FolderPicture>, Option<&FolderPicture>) {
        (
            self.fanart.0.picture.as_ref(),
            self.fanart.1.picture.as_ref(),
        )
    }

    /// The record picture from the album, stretched to the theme's.
    pub fn vinyl(&self) -> Option<&Frame> {
        self.vinyl.frame.as_ref()
    }

    /// The reels as `Stack` takes them: the album's where the scene names
    /// one and it is decoded, else the theme's own.
    pub fn reels<'a>(
        &'a self,
        scene: &Scene,
        assets: &'a MeterAssets,
    ) -> (Option<&'a Frame>, Option<&'a Frame>) {
        let Some(reels) = &scene.reels else {
            return (None, None);
        };
        let side = |slot: &'a PlainSlot,
                    file: &str,
                    spec: Option<&lead::ReelSpec>,
                    theme: Option<&'a Frame>| {
            let spec = spec?;
            if file.is_empty() || file == spec.theme_file {
                return theme;
            }
            slot.frame.as_ref().or(theme)
        };
        (
            side(
                &self.reels.0,
                &reels.left_file,
                reels.spec.left.as_ref(),
                assets.reels.0.as_ref(),
            ),
            side(
                &self.reels.1,
                &reels.right_file,
                reels.spec.right.as_ref(),
                assets.reels.1.as_ref(),
            ),
        )
    }

    /// The bytes each kind of picture holds, for the memory line.
    pub fn memory(&self) -> Vec<(&'static str, usize)> {
        vec![
            ("art", self.art().map_or(0, Frame::bytes)),
            ("icon", self.icon.bytes()),
            (
                "layers",
                self.folder_pictures
                    .iter()
                    .flatten()
                    .map(|p| p.frame.bytes())
                    .sum(),
            ),
            (
                "fanart",
                [&self.fanart.0, &self.fanart.1]
                    .into_iter()
                    .filter_map(|s| s.picture.as_ref())
                    .map(|p| p.frame.bytes())
                    .sum(),
            ),
            ("vinyl", self.vinyl().map_or(0, Frame::bytes)),
            (
                "reels",
                [&self.reels.0, &self.reels.1]
                    .into_iter()
                    .filter_map(|s| s.frame.as_ref())
                    .map(Frame::bytes)
                    .sum(),
            ),
        ]
    }
}

/// Decode a type icon fitted inside `w` by `h`, keeping its aspect. An SVG
/// is rendered at that size and every visible pixel takes `tint`; a PNG keeps
/// its own colours. A picture smaller than the box is enlarged to it.
pub fn read_icon(path: &Path, w: u32, h: u32, tint: Option<[u8; 3]>) -> Option<Frame> {
    let (w, h) = (w.max(1), h.max(1));
    let is_svg = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
    let mut frame = if is_svg {
        let bytes = read_file(path)?;
        let tree = resvg::usvg::Tree::from_data(&bytes, &resvg::usvg::Options::default()).ok()?;
        let size = tree.size();
        let (sw, sh) = (size.width(), size.height());
        if sw <= 0.0 || sh <= 0.0 {
            return None;
        }
        let scale = (w as f32 / sw).min(h as f32 / sh);
        let pw = ((sw * scale).round() as u32).clamp(1, w);
        let ph = ((sh * scale).round() as u32).clamp(1, h);
        let mut pixmap = resvg::tiny_skia::Pixmap::new(pw, ph)?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        let mut rgba = Vec::with_capacity((pw * ph * 4) as usize);
        for px in pixmap.pixels() {
            let c = px.demultiply();
            rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        Frame {
            blend: Blend::Normal,
            width: pw,
            height: ph,
            rgba,
        }
    } else {
        let picture = read_png(path)?;
        let scale =
            (w as f32 / picture.width.max(1) as f32).min(h as f32 / picture.height.max(1) as f32);
        let fw = ((picture.width as f32 * scale) as u32).clamp(1, w);
        let fh = ((picture.height as f32 * scale) as u32).clamp(1, h);
        fit_art(&picture, fw, fh)
    };
    if let (true, Some(tint)) = (is_svg, tint) {
        for px in frame.rgba.as_chunks_mut::<4>().0 {
            if px[3] > 0 {
                px[..3].copy_from_slice(&tint);
            }
        }
    }
    Some(frame)
}

/// Cut a picture with a mask of the same box: the mask's white is removed,
/// its black kept, as the player's engine does with `albumart.mask`.
pub fn apply_mask(art: &mut Frame, mask: &Frame) {
    let mask = fit_art(mask, art.width, art.height);
    for (px, mpx) in art
        .rgba
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(mask.rgba.as_chunks::<4>().0)
    {
        let luminance =
            (u32::from(mpx[0]) * 299 + u32::from(mpx[1]) * 587 + u32::from(mpx[2]) * 114) / 1000;
        let keep = 255 - luminance.min(255) as u8;
        px[3] = ((u32::from(px[3]) * u32::from(keep)) / 255) as u8;
    }
}

/// Draw one theme text. With its font, the top of the em box sits at `y`, as
/// the player's renderer places it, and `max_width` clips the line. Without
/// a font for its style the bitmap font stands in.
pub fn draw_text_styled(frame: &mut Frame, text: &Text, fonts: Option<&Fonts>) {
    let line = render_text(
        fonts,
        text.style,
        text.size,
        text.color,
        &text.text,
        text.max_width,
    )
    .unwrap_or_else(|| bitmap_line(&text.text));
    blit_op(&line, (text.x as i32, text.y as i32)).paint(&mut Band::whole(frame));
}

/// Set a line of text in a font: a transparent frame one line high whose
/// width is the text's advance, or `max_width` when that is smaller and not
/// zero. `None` when the style has no font or the text is empty.
pub fn render_text(
    fonts: Option<&Fonts>,
    style: TextStyle,
    size: u32,
    color: [u8; 3],
    text: &str,
    max_width: u32,
) -> Option<Frame> {
    render_line(
        fonts.and_then(|f| f.get(style)),
        fonts.and_then(Fonts::fallback),
        size,
        color,
        text,
        max_width,
    )
}

/// The face and glyph for a character: the text's own face when it has the
/// character, else the fallback face when that has it, else the own face's
/// missing-glyph mark.
fn pick_glyph<'f>(
    font: &'f FontRef<'static>,
    fallback: Option<&'f FontRef<'static>>,
    ch: char,
) -> (&'f FontRef<'static>, ab_glyph::GlyphId) {
    let id = font.glyph_id(ch);
    if id.0 != 0 {
        return (font, id);
    }
    if let Some(other) = fallback {
        let alt = other.glyph_id(ch);
        if alt.0 != 0 {
            return (other, alt);
        }
    }
    (font, id)
}

/// The scale that sets a face's em at `size` pixels, as FreeType sizes a
/// face for pygame. An ab_glyph scale is the face's ascent-to-descent
/// height, so the size is scaled by that height over the units per em: a
/// face whose height is its em (Lato, DSEG7) is unchanged by this, while
/// PeppyFont's height is 1.227 em.
fn em_scale(face: &FontRef<'static>, size: u32) -> PxScale {
    let size = size.max(1) as f32;
    let upem = face.units_per_em().unwrap_or(1000.0);
    PxScale::from(size * face.height_unscaled() / upem)
}

/// FreeType's FT_CEIL of a pixel value: rounded to 26.6 fixed point, then
/// up to a whole pixel.
fn ft_ceil(px: f32) -> i32 {
    ((px * 64.0).round() as i32 + 63).div_euclid(64)
}

fn render_line(
    font: Option<&FontRef<'static>>,
    fallback: Option<&FontRef<'static>>,
    size: u32,
    color: [u8; 3],
    text: &str,
    max_width: u32,
) -> Option<Frame> {
    let font = font?;
    if text.is_empty() {
        return None;
    }
    let fallback = fallback.filter(|f| !std::ptr::eq(*f, font));
    let scaled = font.as_scaled(em_scale(font, size));
    // The baseline sits the face's ascent below the line's top and the
    // line is ascent to descent high, both whole pixels rounded up, as
    // FreeType sets them for pygame.
    let ascent = ft_ceil(scaled.ascent()) as f32;
    let line_height = ft_ceil(scaled.ascent() - scaled.descent()).max(1) as u32;
    let mut advance = 0.0f32;
    // The last glyph and the face it came from; kerning is only between
    // glyphs of one face.
    let mut last: Option<(&FontRef<'static>, ab_glyph::GlyphId)> = None;
    for ch in text.chars() {
        let (face, id) = pick_glyph(font, fallback, ch);
        let face_scaled = face.as_scaled(em_scale(face, size));
        if let Some((prev_face, prev)) = last {
            if std::ptr::eq(prev_face, face) {
                advance += face_scaled.kern(prev, id);
            }
        }
        advance += face_scaled.h_advance(id);
        last = Some((face, id));
    }
    let mut width = advance.ceil().max(1.0) as u32;
    if max_width > 0 {
        width = width.min(max_width);
    }
    let height = line_height;
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    let mut pen = 0.0f32;
    let mut last: Option<(&FontRef<'static>, ab_glyph::GlyphId)> = None;
    for ch in text.chars() {
        let (face, id) = pick_glyph(font, fallback, ch);
        let face_scaled = face.as_scaled(em_scale(face, size));
        if let Some((prev_face, prev)) = last {
            if std::ptr::eq(prev_face, face) {
                pen += face_scaled.kern(prev, id);
            }
        }
        let glyph = id.with_scale_and_position(face_scaled.scale(), ab_glyph::point(pen, ascent));
        if let Some(outline) = face.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, coverage| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px < 0 || py < 0 || px as u32 >= width || py as u32 >= height {
                    return;
                }
                let a = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
                if a == 0 {
                    return;
                }
                let d = (py as usize * width as usize + px as usize) * 4;
                // Text is drawn onto a clear frame: keep the strongest coverage.
                if a >= rgba[d + 3] {
                    rgba[d..d + 4].copy_from_slice(&[color[0], color[1], color[2], a]);
                }
            });
        }
        pen += face_scaled.h_advance(id);
        last = Some((face, id));
        if pen >= width as f32 {
            break;
        }
    }
    Some(Frame {
        blend: Blend::Normal,
        width,
        height,
        rgba,
    })
}

pub fn sample(frame: &Frame, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * frame.width + x) * 4) as usize;
    [
        frame.rgba[i],
        frame.rgba[i + 1],
        frame.rgba[i + 2],
        frame.rgba[i + 3],
    ]
}

fn fill_column(band: &mut Band, rect: &Rect, level: f32, color: [u8; 4]) {
    let lit = ((level.clamp(0.0, 1.0) * rect.h as f32).round() as u32).min(rect.h);
    let bottom = rect.y + rect.h;
    let cols = band.cols(rect.x as i32, (rect.x + rect.w) as i32);
    for y in band.rows((bottom - lit) as i32, bottom as i32) {
        let row = band.row(y);
        for x in cols.clone() {
            let i = (x * 4) as usize;
            row[i..i + 4].copy_from_slice(&color);
        }
    }
}

/// The spectrum bars of a scene without a theme: one column a bin with a gap.
fn plan_bars(rect: &Rect, bars: &[f32], color: [u8; 4], ops: &mut Vec<Op<'_>>) {
    if bars.is_empty() || rect.w == 0 {
        return;
    }
    let gap = 1u32;
    let slot = rect.w / bars.len() as u32;
    let bar_w = slot.saturating_sub(gap).max(1);
    for (i, level) in bars.iter().enumerate() {
        let x = rect.x + i as u32 * slot;
        if x >= rect.x + rect.w {
            break;
        }
        let w = bar_w.min(rect.x + rect.w - x);
        ops.push(Op::Column {
            rect: Rect {
                x,
                y: rect.y,
                w,
                h: rect.h,
            },
            level: *level,
            color,
        });
    }
}

/// Blend `part` (x, y, w, h) of a picture at `at`, which may lie partly
/// outside the frame, its alpha scaled by `alpha` (255 is as is), and only
/// inside `clip` (x, y, w, h) when one is given.
fn blit_into(
    band: &mut Band,
    src: &Frame,
    at: (i32, i32),
    part: (u32, u32, u32, u32),
    clip: Option<(i32, i32, i32, i32)>,
    alpha: u8,
) {
    if alpha == 0 {
        return;
    }
    let (src_x, src_y, src_w, src_h) = part;
    let src_w = src_w.min(src.width.saturating_sub(src_x));
    let src_h = src_h.min(src.height.saturating_sub(src_y));
    if src_w == 0 || src_h == 0 {
        return;
    }
    let (mut x_lo, mut x_hi) = (band.x0 as i32, band.x1 as i32);
    let (mut y_lo, mut y_hi) = (i32::MIN / 2, i32::MAX / 2);
    if let Some((cx, cy, cw, ch)) = clip {
        x_lo = x_lo.max(cx);
        x_hi = x_hi.min(cx.saturating_add(cw));
        y_lo = cy;
        y_hi = cy.saturating_add(ch);
    }
    let dx_from = at.0.max(x_lo);
    let dx_to = at.0.saturating_add(src_w as i32).min(x_hi);
    if dx_to <= dx_from {
        return;
    }
    let col_from = (dx_from - at.0) as usize;
    let cols = (dx_to - dx_from) as usize;
    let dy_from = at.1.max(y_lo);
    let dy_to = at.1.saturating_add(src_h as i32).min(y_hi);
    let stride = src.width as usize * 4;
    for dy in band.rows(dy_from, dy_to) {
        let sy = src_y as usize + (dy as i32 - at.1) as usize;
        let s = sy * stride + (src_x as usize + col_from) * 4;
        let src_row = &src.rgba[s..s + cols * 4];
        let d = dx_from as usize * 4;
        let dst_row = &mut band.row(dy)[d..d + cols * 4];
        if alpha == 255 {
            for (dp, sp) in dst_row
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(src_row.as_chunks::<4>().0)
            {
                blend(dp, 0, [sp[0], sp[1], sp[2], sp[3]]);
            }
        } else {
            for (dp, sp) in dst_row
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(src_row.as_chunks::<4>().0)
            {
                let a = (sp[3] as u32 * alpha as u32 / 255) as u8;
                blend(dp, 0, [sp[0], sp[1], sp[2], a]);
            }
        }
    }
}

/// Source-over one straight-alpha pixel onto the opaque frame.
fn blend(dst: &mut [u8], d: usize, color: [u8; 4]) {
    let a = color[3] as u32;
    if a == 0 {
        return;
    }
    if a == 255 {
        dst[d..d + 3].copy_from_slice(&color[..3]);
        dst[d + 3] = 255;
        return;
    }
    let inv = 255 - a;
    for c in 0..3 {
        dst[d + c] = ((color[c] as u32 * a + dst[d + c] as u32 * inv + 127) / 255) as u8;
    }
    dst[d + 3] = 255;
}

/// Texel at integer coordinates. Outside the sprite is transparent.
fn texel(src: &Frame, x: i32, y: i32) -> [u8; 4] {
    if x < 0 || y < 0 || x >= src.width as i32 || y >= src.height as i32 {
        return [0, 0, 0, 0];
    }
    let s = (y as usize * src.width as usize + x as usize) * 4;
    [
        src.rgba[s],
        src.rgba[s + 1],
        src.rgba[s + 2],
        src.rgba[s + 3],
    ]
}

/// Bilinear sample at a continuous position, where texel centres sit at
/// half coordinates. Alpha is premultiplied while mixing so a transparent
/// neighbour lends no colour to the edge.
fn sample_bilinear(src: &Frame, sx: f32, sy: f32) -> [u8; 4] {
    let fx = sx - 0.5;
    let fy = sy - 0.5;
    let x0 = fx.floor();
    let y0 = fy.floor();
    let tx = fx - x0;
    let ty = fy - y0;
    let (x0, y0) = (x0 as i32, y0 as i32);
    let corners = [
        (texel(src, x0, y0), (1.0 - tx) * (1.0 - ty)),
        (texel(src, x0 + 1, y0), tx * (1.0 - ty)),
        (texel(src, x0, y0 + 1), (1.0 - tx) * ty),
        (texel(src, x0 + 1, y0 + 1), tx * ty),
    ];
    let mut acc = [0.0f32; 4];
    for (px, w) in corners {
        let a = px[3] as f32 * w;
        acc[0] += px[0] as f32 * a;
        acc[1] += px[1] as f32 * a;
        acc[2] += px[2] as f32 * a;
        acc[3] += a;
    }
    if acc[3] <= 0.0 {
        return [0, 0, 0, 0];
    }
    [
        (acc[0] / acc[3]).round() as u8,
        (acc[1] / acc[3]).round() as u8,
        (acc[2] / acc[3]).round() as u8,
        acc[3].round().clamp(0.0, 255.0) as u8,
    ]
}

/// Draw `src` turned by `degrees` about the theme origin `at`, with the sprite
/// centre `distance` pixels from that origin along the needle. Positive degrees
/// turn the needle to the left of vertical, as the theme files count them.
/// The direct form of the needle turn, which the kept turn reproduces; the
/// tests pin the geometry through it.
#[cfg(test)]
fn blit_rotated(band: &mut Band, src: &Frame, at: (i32, i32), degrees: f32, distance: f32) {
    // The needle turns about a point `distance` below its picture's centre,
    // which the theme places on the origin.
    let pivot = (src.width as f32 / 2.0, src.height as f32 / 2.0 + distance);
    turn_onto(
        band,
        src,
        pivot,
        (at.0 as f32, at.1 as f32),
        degrees,
        true,
        false,
    );
}

/// Draw a picture turned `degrees` counter-clockwise about `pivot_image`,
/// that point landing on `pivot_screen`. Each frame row visits only the
/// span the turned picture covers; `smooth` samples bilinearly with
/// premultiplied alpha, otherwise the nearest texel, as the engine's plain
/// rotation does for records, reels, art and knobs. `store` writes each
/// turned pixel with its own alpha into a transparent canvas, for a picture
/// kept turned; otherwise pixels blend onto the opaque frame.
fn turn_onto(
    band: &mut Band,
    src: &Frame,
    pivot_image: (f32, f32),
    pivot_screen: (f32, f32),
    degrees: f32,
    smooth: bool,
    store: bool,
) {
    if src.width == 0 || src.height == 0 || band.x1 <= band.x0 {
        return;
    }
    let rad = degrees.to_radians();
    let (sin, cos) = rad.sin_cos();
    let (px, py) = pivot_image;
    let (sw, sh) = (src.width as f32, src.height as f32);
    let reach = reach_of(src, pivot_image);
    let y_from = (pivot_screen.1 - reach).floor() as i32;
    let y_to = (pivot_screen.1 + reach).ceil() as i32;
    // A source coordinate is linear in the frame column: sx = a_x + vx cos,
    // sy = a_y + vx sin. Each bound gives an interval of vx.
    let bound = |coef: f32, base: f32, limit: f32| -> Option<(f32, f32)> {
        if coef.abs() < 1e-6 {
            return if base >= 0.0 && base <= limit {
                Some((f32::MIN, f32::MAX))
            } else {
                None
            };
        }
        let (a, b) = ((0.0 - base) / coef, (limit - base) / coef);
        Some((a.min(b), a.max(b)))
    };
    for dy in band.rows(y_from, y_to.saturating_add(1)) {
        let vy = dy as f32 + 0.5 - pivot_screen.1;
        let a_x = px - vy * sin;
        let a_y = py + vy * cos;
        let (Some(bx), Some(by)) = (bound(cos, a_x, sw), bound(sin, a_y, sh)) else {
            continue;
        };
        let lo = bx.0.max(by.0);
        let hi = bx.1.min(by.1);
        if hi < lo {
            continue;
        }
        let x_from = ((lo + pivot_screen.0 - 0.5).ceil() as i32).max(band.x0 as i32);
        let x_to = ((hi + pivot_screen.0 - 0.5).floor() as i32).min(band.x1 as i32 - 1);
        if x_to < x_from {
            continue;
        }
        let vx0 = x_from as f32 + 0.5 - pivot_screen.0;
        let row = band.row(dy);
        if smooth {
            let mut sx = a_x + vx0 * cos;
            let mut sy = a_y + vx0 * sin;
            for dx in x_from..=x_to {
                let color = sample_bilinear(src, sx, sy);
                if color[3] != 0 {
                    let d = dx as usize * 4;
                    if store {
                        row[d..d + 4].copy_from_slice(&color);
                    } else {
                        blend(row, d, color);
                    }
                }
                sx += cos;
                sy += sin;
            }
        } else {
            // Nearest texel in 16.16 fixed point: two adds and two shifts a pixel.
            let scale = 65536.0;
            let mut fx = ((a_x + vx0 * cos) * scale) as i64;
            let mut fy = ((a_y + vx0 * sin) * scale) as i64;
            let (dfx, dfy) = ((cos * scale) as i64, (sin * scale) as i64);
            let (max_x, max_y) = (src.width as i64 - 1, src.height as i64 - 1);
            let stride = src.width as usize * 4;
            for dx in x_from..=x_to {
                let tx = (fx >> 16).clamp(0, max_x) as usize;
                let ty = (fy >> 16).clamp(0, max_y) as usize;
                let i = ty * stride + tx * 4;
                let a = src.rgba[i + 3];
                if a != 0 {
                    let d = dx as usize * 4;
                    if store {
                        row[d..d + 4].copy_from_slice(&src.rgba[i..i + 4]);
                    } else if a == 255 {
                        row[d..d + 3].copy_from_slice(&src.rgba[i..i + 3]);
                        row[d + 3] = 255;
                    } else {
                        blend(row, d, [src.rgba[i], src.rgba[i + 1], src.rgba[i + 2], a]);
                    }
                }
                fx += dfx;
                fy += dfy;
            }
        }
    }
}

/// Cut a picture to the ellipse inscribed in its box, as turning art is cut
/// when it has no mask of its own.
pub fn apply_circle(frame: &Frame) -> Frame {
    let mut out = frame.clone();
    let (w, h) = (frame.width as f32, frame.height as f32);
    let (rx, ry) = (w / 2.0, h / 2.0);
    for y in 0..frame.height {
        for x in 0..frame.width {
            let nx = (x as f32 + 0.5 - rx) / rx;
            let ny = (y as f32 + 0.5 - ry) / ry;
            if nx * nx + ny * ny > 1.0 {
                out.rgba[((y * frame.width + x) * 4 + 3) as usize] = 0;
            }
        }
    }
    out
}

/// A ring of the given thickness inside radius `r`, as the player's draw call
/// makes it; a thickness past `r` fills the disc.
fn draw_ring(band: &mut Band, cx: i32, cy: i32, r: i32, thickness: i32, color: [u8; 4]) {
    if r <= 0 || thickness <= 0 {
        return;
    }
    let inner = (r - thickness).max(0) as f32;
    let outer = r as f32;
    let cols = band.cols(cx - r, cx.saturating_add(r).saturating_add(1));
    for y in band.rows(cy - r, cy.saturating_add(r).saturating_add(1)) {
        let row = band.row(y);
        for x in cols.clone() {
            let d = (((x as i32 - cx) as f32).powi(2) + ((y as i32 - cy) as f32).powi(2)).sqrt();
            if d <= outer && d >= inner {
                blend(row, x as usize * 4, color);
            }
        }
    }
}

/// The record's turn: it spins while the player plays, while a stop is only
/// a transition, while the tonearm moves, and while it slows to a halt over
/// the tonearm's lift after playback stops.
#[derive(Default)]
pub struct VinylMotion {
    angle: f32,
    last_ms: Option<u64>,
    was_playing: bool,
    decel_start_ms: Option<u64>,
    decel_ms: u64,
}

impl VinylMotion {
    /// Move the record on and give its angle in degrees, growing clockwise.
    #[allow(clippy::too_many_arguments)]
    pub fn advance(
        &mut self,
        rpm: f32,
        clockwise: bool,
        playing: bool,
        transitional: bool,
        tonearm_animating: bool,
        lift_s: f32,
        now_ms: u64,
    ) -> f32 {
        if self.was_playing && !playing {
            self.decel_start_ms = Some(now_ms);
            self.decel_ms = (lift_s.max(0.0) * 1000.0) as u64;
        }
        if !self.was_playing && playing {
            self.decel_start_ms = None;
        }
        self.was_playing = playing;
        let mut factor = 1.0f32;
        let mut decelerating = false;
        if let Some(start) = self.decel_start_ms {
            let elapsed = now_ms.saturating_sub(start);
            if elapsed < self.decel_ms {
                let p = elapsed as f32 / self.decel_ms as f32;
                factor = (1.0 - p * p).max(0.0);
                decelerating = true;
            } else {
                factor = 0.0;
                if !tonearm_animating {
                    self.decel_start_ms = None;
                }
            }
        }
        let spinning = playing || transitional || decelerating || tonearm_animating;
        let dt = self.last_ms.map_or(0.0, |last| {
            (now_ms.saturating_sub(last) as f32 / 1000.0).min(0.5)
        });
        self.last_ms = Some(now_ms);
        if rpm > 0.0 && spinning && (playing || transitional || factor > 0.0) {
            let direction = if clockwise { 1.0 } else { -1.0 };
            self.angle = (self.angle + rpm * factor * 6.0 * dt * direction).rem_euclid(360.0);
        }
        self.angle
    }

    pub fn angle(&self) -> f32 {
        self.angle
    }
}

/// A tape reel's turn: it spins while the player plays or a stop is only a
/// transition, at the reel's speed times its spool multiplier.
#[derive(Default)]
pub struct ReelMotion {
    angle: f32,
    last_ms: Option<u64>,
}

impl ReelMotion {
    pub fn advance(&mut self, rpm: f32, clockwise: bool, spinning: bool, now_ms: u64) -> f32 {
        let dt = self.last_ms.map_or(0.0, |last| {
            (now_ms.saturating_sub(last) as f32 / 1000.0).min(0.5)
        });
        self.last_ms = Some(now_ms);
        if rpm > 0.0 && spinning {
            let direction = if clockwise { 1.0 } else { -1.0 };
            self.angle = (self.angle + rpm * 6.0 * dt * direction).rem_euclid(360.0);
        }
        self.angle
    }
}

/// A fade of the whole frame from or to a colour: the overlay's alpha runs
/// from the opacity down to nothing on the way in, and up on the way out,
/// over the duration.
#[derive(Default)]
pub struct Fade {
    started_ms: Option<u64>,
    duration_ms: u64,
    color: [u8; 3],
    max_alpha: u8,
    out: bool,
}

impl Fade {
    fn begin(&mut self, now_ms: u64, duration_s: f32, white: bool, opacity: f32, out: bool) {
        self.started_ms = Some(now_ms);
        self.duration_ms = (duration_s.max(0.0) * 1000.0) as u64;
        self.color = if white { [255, 255, 255] } else { [0, 0, 0] };
        self.max_alpha = (255.0 * opacity.clamp(0.0, 1.0)) as u8;
        self.out = out;
    }

    /// Start showing the frame from the colour.
    pub fn begin_in(&mut self, now_ms: u64, duration_s: f32, white: bool, opacity: f32) {
        self.begin(now_ms, duration_s, white, opacity, false);
    }

    /// Start hiding the frame under the colour.
    pub fn begin_out(&mut self, now_ms: u64, duration_s: f32, white: bool, opacity: f32) {
        self.begin(now_ms, duration_s, white, opacity, true);
    }

    /// The overlay for this moment, or `None` once a fade in has finished.
    /// A finished fade out keeps the frame covered.
    pub fn overlay(&mut self, now_ms: u64) -> Option<([u8; 3], u8)> {
        let started = self.started_ms?;
        let p = if self.duration_ms == 0 {
            1.0
        } else {
            (now_ms.saturating_sub(started) as f32 / self.duration_ms as f32).min(1.0)
        };
        if self.out {
            return Some((self.color, (self.max_alpha as f32 * p) as u8));
        }
        if p >= 1.0 {
            self.started_ms = None;
            return None;
        }
        Some((self.color, (self.max_alpha as f32 * (1.0 - p)) as u8))
    }

    pub fn running(&self, now_ms: u64) -> bool {
        self.started_ms
            .is_some_and(|s| now_ms.saturating_sub(s) < self.duration_ms)
    }
}

/// The level ramp after a start: the engine raises its full scale in ten
/// steps of 70 ms, so the needles rise to the level over 0.7 s.
#[derive(Default)]
pub struct Ramp {
    started_ms: Option<u64>,
}

impl Ramp {
    pub fn begin(&mut self, now_ms: u64) {
        self.started_ms = Some(now_ms);
    }

    /// The share of the level to show, 0.0 to 1.0.
    pub fn factor(&mut self, now_ms: u64) -> f32 {
        let Some(started) = self.started_ms else {
            return 1.0;
        };
        let steps = now_ms.saturating_sub(started) / 70;
        if steps >= 10 {
            self.started_ms = None;
            return 1.0;
        }
        steps as f32 / 10.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ArmState {
    #[default]
    Rest,
    Drop,
    Tracking,
    Lift,
}

/// The tonearm's state: parked, dropping onto the record, following the
/// track, or lifting back, with eased moves over the theme's durations.
#[derive(Default)]
pub struct TonearmMotion {
    state: ArmState,
    angle: f32,
    started: bool,
    anim_start_ms: u64,
    anim_from: f32,
    anim_to: f32,
    anim_ms: u64,
    early_lift: bool,
    pending_target: Option<f32>,
}

impl TonearmMotion {
    fn start_move(&mut self, to: f32, seconds: f32, now_ms: u64) {
        self.anim_start_ms = now_ms;
        self.anim_from = self.angle;
        self.anim_to = to;
        self.anim_ms = (seconds.max(0.0) * 1000.0) as u64;
    }

    /// Advance the current move; true when it has arrived.
    fn step_move(&mut self, now_ms: u64) -> bool {
        if self.anim_ms == 0 {
            self.angle = self.anim_to;
            return true;
        }
        let p = (now_ms.saturating_sub(self.anim_start_ms) as f32 / self.anim_ms as f32).min(1.0);
        let eased = 1.0 - (1.0 - p) * (1.0 - p);
        self.angle = self.anim_from + (self.anim_to - self.anim_from) * eased;
        p >= 1.0
    }

    pub fn update(
        &mut self,
        spec: &TonearmSpec,
        playing: bool,
        progress_pct: f32,
        time_remaining: Option<f32>,
        now_ms: u64,
    ) {
        if !self.started {
            self.started = true;
            self.angle = spec.rest;
        }
        let progress = progress_pct.clamp(0.0, 100.0);
        let target = spec.start + (spec.end - spec.start) * (progress / 100.0);
        match self.state {
            ArmState::Rest => {
                if playing {
                    if self.early_lift && progress > 10.0 {
                        return;
                    }
                    self.early_lift = false;
                    self.state = ArmState::Drop;
                    self.start_move(target, spec.drop_s, now_ms);
                }
            }
            ArmState::Drop => {
                if !playing {
                    self.state = ArmState::Lift;
                    self.early_lift = false;
                    self.start_move(spec.rest, spec.lift_s, now_ms);
                } else if self.step_move(now_ms) {
                    self.angle = target;
                    self.state = ArmState::Tracking;
                }
            }
            ArmState::Tracking => {
                if time_remaining.is_some_and(|left| left < 1.5 && left > 0.0) {
                    self.state = ArmState::Lift;
                    self.pending_target = None;
                    self.early_lift = true;
                    self.start_move(spec.rest, spec.lift_s, now_ms);
                } else if !playing {
                    self.state = ArmState::Lift;
                    self.pending_target = None;
                    self.early_lift = false;
                    self.start_move(spec.rest, spec.lift_s, now_ms);
                } else if (target - self.angle).abs() > 2.0 {
                    self.state = ArmState::Lift;
                    self.pending_target = Some(target);
                    self.early_lift = false;
                    self.start_move(spec.rest, spec.lift_s, now_ms);
                } else if (target - self.angle).abs() > 0.2 {
                    self.angle = target;
                }
            }
            ArmState::Lift => {
                if self.step_move(now_ms) {
                    if self.early_lift {
                        self.state = ArmState::Rest;
                        self.pending_target = None;
                    } else if let Some(pending) = self.pending_target.take() {
                        self.state = ArmState::Drop;
                        self.start_move(pending, spec.drop_s, now_ms);
                    } else if playing {
                        self.state = ArmState::Drop;
                        self.start_move(target, spec.drop_s, now_ms);
                    } else {
                        self.state = ArmState::Rest;
                    }
                }
            }
        }
    }

    pub fn is_animating(&self) -> bool {
        matches!(self.state, ArmState::Drop | ArmState::Lift)
    }

    pub fn angle(&self) -> f32 {
        self.angle
    }
}

/// The picture mirrored left to right.
pub fn flip_x(src: &Frame) -> Frame {
    let mut rgba = vec![0u8; src.rgba.len()];
    let w = src.width as usize;
    for y in 0..src.height as usize {
        for x in 0..w {
            let s = (y * w + x) * 4;
            let d = (y * w + (w - 1 - x)) * 4;
            rgba[d..d + 4].copy_from_slice(&src.rgba[s..s + 4]);
        }
    }
    Frame {
        blend: Blend::Normal,
        width: src.width,
        height: src.height,
        rgba,
    }
}

/// Where one channel of a linear meter shows its picture, as the meter
/// engine draws it. A bar shows `w` pixels of the indicator picture from the
/// end the direction names, anchored at the channel origin; `edges-center`
/// and `center-edges` anchor the two channels at opposite ends. A single
/// indicator moves by `w`. The place and the part of the picture.
fn bar_part(
    sprite: &Frame,
    at: (i32, i32),
    w: u32,
    linear: &LinearSpec,
    left: bool,
) -> Option<((i32, i32), (u32, u32, u32, u32))> {
    let (cw, ch) = (sprite.width, sprite.height);
    if cw == 0 || ch == 0 {
        return None;
    }
    let (ox, oy) = at;
    let w_i = w as i32;
    if linear.single {
        let at = match linear.direction {
            Direction::BottomTop => (ox, oy - w_i),
            Direction::TopBottom => (ox, oy + w_i),
            Direction::CenterEdges => (if left { ox - w_i } else { ox + w_i }, oy),
            Direction::EdgesCenter => (if left { ox + w_i } else { ox - w_i }, oy),
            Direction::LeftRight => (ox + w_i, oy),
            Direction::RightLeft => (ox - w_i, oy),
        };
        return Some((at, (0, 0, cw, ch)));
    }
    let across = w.min(cw);
    let down = w.min(ch);
    Some(match linear.direction {
        Direction::LeftRight => ((ox, oy), (0, 0, across, ch)),
        Direction::RightLeft => (
            (ox + (cw - across) as i32, oy),
            (cw - across, 0, across, ch),
        ),
        Direction::BottomTop => ((ox, oy + (ch - down) as i32), (0, ch - down, cw, down)),
        Direction::TopBottom => ((ox, oy), (0, 0, cw, down)),
        Direction::EdgesCenter => {
            if left {
                ((ox, oy), (0, 0, across, ch))
            } else {
                let x = if linear.flip_right {
                    ox - across as i32
                } else {
                    ox
                };
                ((x, oy), (cw - across, 0, across, ch))
            }
        }
        Direction::CenterEdges => {
            if left {
                ((ox - across as i32, oy), (cw - across, 0, across, ch))
            } else {
                ((ox, oy), (0, 0, across, ch))
            }
        }
    })
}

fn bar_op<'a>(
    sprite: &'a Frame,
    at: (i32, i32),
    w: u32,
    linear: &LinearSpec,
    left: bool,
) -> Option<Op<'a>> {
    bar_part(sprite, at, w, linear, left).map(|(at, part)| Op::Blit {
        src: sprite,
        at,
        part,
        clip: None,
        alpha: 255,
    })
}

/// One channel of a linear meter straight onto a band.
#[cfg(test)]
fn draw_bar(
    band: &mut Band,
    sprite: &Frame,
    at: (i32, i32),
    w: u32,
    linear: &LinearSpec,
    left: bool,
) {
    if let Some(op) = bar_op(sprite, at, w, linear, left) {
        op.paint(band);
    }
}

/// Everything that moves between frames, and what is kept from one frame
/// to the next: text offsets, spectrum toppings, the turn of records and
/// reels, the tonearm, pictures turned, lines set in type, and the canvas.
#[derive(Default)]
pub struct Motion {
    pub text: TextMotion,
    /// One per spectrum box: the toppings of the previous engine's bars.
    pub spectrum: Vec<SpectrumMotion>,
    /// One per analyser box: its bars and peaks between frames, and its picture.
    pub analyser: Vec<AnalyserMotion>,
    pub vinyl: VinylMotion,
    pub tonearm: TonearmMotion,
    pub reels: (ReelMotion, ReelMotion),
    /// The fade over the whole frame and the level ramp after a start.
    pub fade: Fade,
    pub ramp: Ramp,
    /// When set, `raster_over` appends how long each stage took, in microseconds.
    pub profile: Option<Vec<(&'static str, u64)>>,
    /// Needle sprites and the tonearm turned to recent angles, so a needle
    /// that holds or moves slowly costs one plain blit a frame.
    pub needles: Turned,
    /// Lines set in type for the type area and the gauges.
    labels: Labels,
    /// How many threads paint a frame.
    painters: Painters,
    /// Added to every frame's painting, for a bench: the painters grow to
    /// their most and the governor steps down on a machine that would
    /// never overrun by itself.
    pub bench_delay: Option<std::time::Duration>,
    /// The frame buffer, kept between frames so no frame allocates one.
    canvas: Frame,
    /// The last frame's steps by key and box, and the base they went over,
    /// so the next frame paints only what differs.
    last_steps: Vec<(u64, Option<Box4>)>,
    last_base: (usize, usize),
    /// The boxes the last frame painted; empty when nothing changed.
    damage: Vec<Rect>,
    /// How far each turning picture reaches from its pivot.
    reaches: Reaches,
    /// Paint every frame whole, for tests that check what changed-box
    /// painting must equal.
    paint_all: bool,
}

impl Motion {
    /// Motion for a player painting on up to `threads` threads. With a
    /// frame rate, only as many threads as a frame needs are used, from one
    /// up; without one, all of them, always.
    pub fn new(threads: usize, frame_rate: Option<u32>) -> Self {
        Self {
            painters: Painters::new(threads, frame_rate),
            ..Self::default()
        }
    }

    /// How many threads paint the next frame.
    pub fn painters(&self) -> usize {
        self.painters.active
    }

    /// The boxes the last frame painted, in frame pixels; nothing else in
    /// the frame changed. Empty when the frame is the one before.
    pub fn damage(&self) -> &[Rect] {
        &self.damage
    }

    /// What the motion holds, by store, in bytes.
    pub fn memory(&self) -> [(&'static str, usize); 4] {
        [
            ("canvas", self.canvas.bytes()),
            ("turned", self.needles.bytes()),
            (
                "labels",
                self.labels.lines.values().map(|(f, _)| f.bytes()).sum(),
            ),
            (
                "lines",
                self.text.lines.values().map(|(_, f)| f.bytes()).sum(),
            ),
        ]
    }
}

/// How many threads paint. Frames whose painting overruns its budget get
/// one more thread, up to the most allowed; frames that would paint in
/// half the period with one fewer give one back. Only the painting counts,
/// since it is the part more threads shorten. Frames are judged a second
/// at a time; the first seconds after a start or a change are not counted,
/// since caches fill, pictures are turned afresh and the clock climbs; and
/// giving a thread back waits five seconds, so the count settles rather
/// than flaps. A board that scales its clock with the load is left to do
/// so: only painting that does not fit at all asks for another thread.
struct Painters {
    most: usize,
    active: usize,
    /// What a frame's painting may take, or zero for a fixed count: most
    /// of the period, the rest being the planning, the upload and the loop.
    budget_us: f64,
    period_us: f64,
    /// A running average of the last frames' painting time.
    mean_us: f64,
    /// The average seen at each count when it was last left, to judge a
    /// return to it by what it cost rather than by a guess.
    seen_us: Vec<Option<f64>>,
    /// Frames and overruns in the current window, and when it began.
    frames: u32,
    overruns: u32,
    window_ms: u64,
    /// Frames before this are not counted: after a start or a change.
    quiet_until_ms: u64,
    changed_ms: u64,
}

impl Default for Painters {
    fn default() -> Self {
        Self::new(1, None)
    }
}

impl Painters {
    fn new(most: usize, frame_rate: Option<u32>) -> Self {
        // A browser paints on the one thread it has.
        let most = if cfg!(target_arch = "wasm32") {
            1
        } else {
            most.max(1)
        };
        let period_us = frame_rate
            .filter(|&fps| fps > 0)
            .map_or(0.0, |fps| 1_000_000.0 / fps as f64);
        let active = if period_us > 0.0 { 1 } else { most };
        Self {
            most,
            active,
            budget_us: period_us * 0.8,
            period_us,
            mean_us: 0.0,
            seen_us: vec![None; most + 1],
            frames: 0,
            overruns: 0,
            window_ms: 0,
            quiet_until_ms: 0,
            changed_ms: 0,
        }
    }

    /// Note how long a frame's painting took and settle the count for the next.
    fn settle(&mut self, paint_us: u64, now_ms: u64) {
        if self.period_us <= 0.0 || self.most <= 1 {
            return;
        }
        if self.quiet_until_ms == 0 {
            self.quiet_until_ms = now_ms + 3000;
        }
        if now_ms < self.quiet_until_ms {
            return;
        }
        if self.frames == 0 {
            self.window_ms = now_ms;
        }
        self.frames += 1;
        if paint_us as f64 > self.budget_us {
            self.overruns += 1;
        }
        self.mean_us = if self.mean_us <= 0.0 {
            paint_us as f64
        } else {
            self.mean_us * 0.9 + paint_us as f64 * 0.1
        };
        if now_ms.saturating_sub(self.window_ms) < 1000 || self.frames < 20 {
            return;
        }
        let since_change = now_ms.saturating_sub(self.changed_ms);
        // What one fewer thread would cost: as it was measured, or, without
        // a measurement or when the load has eased since, as if painting
        // shared out perfectly.
        let projected = self.mean_us * self.active as f64 / (self.active as f64 - 1.0).max(1.0);
        let one_fewer = self
            .seen_us
            .get(self.active - 1)
            .copied()
            .flatten()
            .map_or(projected, |seen| seen.min(projected));
        if self.overruns * 20 > self.frames && self.active < self.most {
            self.seen_us[self.active] = Some(self.mean_us);
            self.active += 1;
        } else if self.active > 1 && since_change >= 5000 && one_fewer < self.period_us * 0.5 {
            self.seen_us[self.active] = Some(self.mean_us);
            self.active -= 1;
        } else {
            self.frames = 0;
            self.overruns = 0;
            return;
        }
        self.changed_ms = now_ms;
        self.quiet_until_ms = now_ms + 1000;
        self.mean_us = 0.0;
        self.frames = 0;
        self.overruns = 0;
    }
}

/// Lines set in type for the type area and the gauges, kept while they are
/// shown, so a label costs one blit a frame.
#[derive(Default)]
pub struct Labels {
    lines: HashMap<String, (Frame, u64)>,
}

impl Labels {
    /// Set a label in type unless it is kept already; the key to find it by.
    /// `None` for an empty label or a style without a font.
    fn ensure(
        &mut self,
        fonts: Option<&Fonts>,
        style: TextStyle,
        size: u32,
        color: [u8; 3],
        text: &str,
        tick: u64,
    ) -> Option<String> {
        if text.is_empty() {
            return None;
        }
        let key = format!("{style:?}\0{size}\0{color:?}\0{text}");
        if let Some(entry) = self.lines.get_mut(&key) {
            entry.1 = tick;
            return Some(key);
        }
        let line = render_text(fonts, style, size, color, text, 0)?;
        self.lines.insert(key.clone(), (line, tick));
        Some(key)
    }

    fn get(&self, key: &str) -> Option<&Frame> {
        self.lines.get(key).map(|(line, _)| line)
    }

    /// Once a few dozen lines are kept, drop those not shown for ten seconds.
    fn sweep(&mut self, tick: u64) {
        if self.lines.len() > 48 {
            self.lines
                .retain(|_, (_, used)| tick.saturating_sub(*used) < 10_000);
        }
    }
}

/// Pictures turned to a quantised angle about a pivot, kept for the angles
/// seen lately within a byte budget. Half a degree is the engine's high
/// quality step.
#[derive(Default)]
pub struct Turned {
    entries: Vec<((usize, i32), TurnedPicture)>,
    bytes: usize,
}

struct TurnedPicture {
    frame: Frame,
    /// Where the frame's top left sits relative to the pivot on screen.
    offset: (i32, i32),
    used: u64,
}

const TURNED_KEEP: usize = 400;
/// What the kept turned pictures may hold together; the least recently
/// shown go first.
const TURNED_BUDGET: usize = 16 << 20;
/// A turn not shown for this long is let go: a tonearm's drop leaves a
/// trail of angles that are never shown again.
const TURNED_KEEP_MS: u64 = 10_000;

impl Turned {
    /// Turn `src` by `degrees` about `pivot_image` unless that turn is kept
    /// already. `slot` tells pictures apart. The key to find the turn by.
    fn ensure(
        &mut self,
        slot: usize,
        src: &Frame,
        pivot_image: (f32, f32),
        degrees: f32,
        tick: u64,
    ) -> (usize, i32) {
        let key = (slot, (degrees * 2.0).round() as i32);
        if let Some((_, entry)) = self.entries.iter_mut().find(|(k, _)| *k == key) {
            entry.used = tick;
            return key;
        }
        let mut picture = turn_picture(src, pivot_image, key.1 as f32 / 2.0);
        picture.used = tick;
        let size = picture.frame.rgba.len();
        let bytes = &mut self.bytes;
        self.entries.retain(|(_, p)| {
            let kept = tick.saturating_sub(p.used) < TURNED_KEEP_MS;
            if !kept {
                *bytes -= p.frame.rgba.len();
            }
            kept
        });
        while !self.entries.is_empty()
            && (self.entries.len() >= TURNED_KEEP || self.bytes + size > TURNED_BUDGET)
        {
            let oldest = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, (_, p))| p.used)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let (_, gone) = self.entries.swap_remove(oldest);
            self.bytes -= gone.frame.rgba.len();
        }
        self.bytes += size;
        self.entries.push((key, picture));
        key
    }

    /// A kept turn: the picture and where its top left sits relative to the
    /// pivot on screen.
    fn get(&self, key: (usize, i32)) -> Option<(&Frame, (i32, i32))> {
        self.entries
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, entry)| (&entry.frame, entry.offset))
    }

    /// What the kept pictures hold, in bytes.
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

/// A picture turned about a pivot into its own frame, with the frame's
/// offset from the pivot, sampled as a turn onto the frame samples.
fn turn_picture(src: &Frame, pivot_image: (f32, f32), degrees: f32) -> TurnedPicture {
    let (px, py) = pivot_image;
    let reach = [
        (0.0, 0.0),
        (src.width as f32, 0.0),
        (0.0, src.height as f32),
        (src.width as f32, src.height as f32),
    ]
    .iter()
    .map(|(x, y)| ((x - px).powi(2) + (y - py).powi(2)).sqrt())
    .fold(0.0f32, f32::max)
    .ceil() as i32
        + 1;
    let side = (reach * 2 + 1) as u32;
    let mut frame = empty_frame(side, side);
    // The pivot sits at the centre of the square frame.
    turn_onto(
        &mut Band::whole(&mut frame),
        src,
        pivot_image,
        (reach as f32 + 0.5, reach as f32 + 0.5),
        degrees,
        true,
        true,
    );
    // Trim to the rows and columns that hold anything.
    let (mut x0, mut y0, mut x1, mut y1) = (side, side, 0u32, 0u32);
    for y in 0..side {
        for x in 0..side {
            if frame.rgba[((y * side + x) * 4 + 3) as usize] != 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return TurnedPicture {
            frame: empty_frame(1, 1),
            offset: (0, 0),
            used: 0,
        };
    }
    let mut trimmed = empty_frame(x1 - x0, y1 - y0);
    for y in y0..y1 {
        let from = ((y * side + x0) * 4) as usize;
        let to = (((y - y0) * (x1 - x0)) * 4) as usize;
        trimmed.rgba[to..to + ((x1 - x0) * 4) as usize]
            .copy_from_slice(&frame.rgba[from..from + ((x1 - x0) * 4) as usize]);
    }
    TurnedPicture {
        frame: trimmed,
        offset: (x0 as i32 - reach, y0 as i32 - reach),
        used: 0,
    }
}

/// A picture kept as the opaque span of every row, so a mostly transparent
/// layer such as a meter foreground blends only where it has pixels and
/// holds only those pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Spans {
    width: u32,
    height: u32,
    /// Each row's first and last column with alpha, and where its pixels start.
    rows: Vec<(u32, u32, usize)>,
    pixels: Vec<u8>,
}

impl Spans {
    pub fn new(frame: Frame) -> Self {
        let mut rows = Vec::with_capacity(frame.height as usize);
        let mut pixels = Vec::new();
        for y in 0..frame.height {
            let row =
                &frame.rgba[(y * frame.width * 4) as usize..((y + 1) * frame.width * 4) as usize];
            let first = (0..frame.width).find(|&x| row[(x * 4 + 3) as usize] != 0);
            let last = (0..frame.width)
                .rev()
                .find(|&x| row[(x * 4 + 3) as usize] != 0);
            match (first, last) {
                (Some(a), Some(b)) => {
                    rows.push((a, b + 1, pixels.len()));
                    pixels.extend_from_slice(&row[(a * 4) as usize..((b + 1) * 4) as usize]);
                }
                _ => rows.push((0, 0, pixels.len())),
            }
        }
        Self {
            width: frame.width,
            height: frame.height,
            rows,
            pixels,
        }
    }

    pub fn bytes(&self) -> usize {
        self.pixels.len() + self.rows.len() * size_of::<(u32, u32, usize)>()
    }

    fn blit(&self, band: &mut Band, at: (u32, u32)) {
        for dy in band.rows(
            at.1 as i32,
            at.1.saturating_add(self.rows.len() as u32) as i32,
        ) {
            let y = dy - at.1;
            let (x0, x1, start) = self.rows[y as usize];
            let cols = band.cols((at.0 + x0) as i32, (at.0 + x1) as i32);
            if cols.is_empty() {
                continue;
            }
            let s = start + ((cols.start - at.0 - x0) * 4) as usize;
            let d = (cols.start * 4) as usize;
            let n = ((cols.end - cols.start) * 4) as usize;
            let row = band.row(dy);
            for (dp, sp) in row[d..d + n]
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(self.pixels[s..s + n].as_chunks::<4>().0)
            {
                blend(dp, 0, [sp[0], sp[1], sp[2], sp[3]]);
            }
        }
    }
}

/// A stopwatch for the raster's stages, silent unless profiling is on.
struct Stages<'a> {
    sink: Option<&'a mut Vec<(&'static str, u64)>>,
    last: u64,
}

impl<'a> Stages<'a> {
    fn new(sink: Option<&'a mut Vec<(&'static str, u64)>>) -> Self {
        Self {
            sink,
            last: clock_us(),
        }
    }

    fn mark(&mut self, name: &'static str) {
        if let Some(sink) = self.sink.as_deref_mut() {
            let now = clock_us();
            sink.push((name, now.saturating_sub(self.last)));
            self.last = now;
        }
    }
}

/// A peak marker: where it stands on the bar scale, since when it has
/// been held there, how fast it falls, and how far it has faded.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Peak {
    level: f32,
    held_since_ms: u64,
    velocity: f32,
    fade: f32,
}

/// The analyser's motion between frames: the bars' smoothed levels, the
/// peaks with their hold, fall and fade, and the picture drawn last. The
/// picture alternates between two buffers so its key differs every frame
/// and the box is painted anew.
#[derive(Default)]
pub struct AnalyserMotion {
    smoothed: [Vec<f32>; 2],
    peaks: [Vec<Peak>; 2],
    last_ms: Option<u64>,
    pictures: [Frame; 2],
    which: usize,
    /// The box's polar map for the radial look, built once per box size.
    radial: Option<RadialMap>,
    /// The scale labels, set once per look and box.
    labels: LabelCache,
    /// When each band group last had an onset: sub-bass, bass, mid, high.
    onset_since: [Option<u64>; 4],
    /// The waterfall's rows so far, the box's size, kept between frames.
    waterfall: Frame,
    /// The echo's ghost levels per channel, following the bars slowly.
    ghost: [Vec<f32>; 2],
    /// The sparkles' random state, carried from frame to frame.
    rng: u32,
}

/// The effects of this frame: what the onsets do, decayed to now (a
/// brightening per band for a flash, a growth of every bar as a share for
/// a pulse, rings on their way from the base to the tip with their alpha),
/// in a colour; the echo's ghost levels per channel; and the random
/// state the sparkles draw from.
#[derive(Default)]
struct FrameFx {
    boost: Vec<f32>,
    pulse: f32,
    rings: Vec<(f32, f32)>,
    color: [u8; 4],
    ghost: [Vec<f32>; 2],
    rng: std::cell::Cell<u32>,
}

impl FrameFx {
    /// A random number from 0 to 1, xorshift.
    fn random(&self) -> f32 {
        let mut s = self.rng.get();
        if s == 0 {
            s = 0x9e37_79b9;
        }
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        self.rng.set(s);
        (s >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// The box's picture at the start of a frame: the background alone, or,
/// with a trail, the last frame with what its bars added over the
/// background faded by the trail's share, so a wake follows the bars.
fn begin_frame(out: &mut Frame, w: u32, h: u32, bg: u8, previous: Option<(&Frame, f32)>) {
    out.width = w;
    out.height = h;
    match previous {
        Some((last, trail)) if last.width == w && last.height == h && trail > 0.0 => {
            // The colour dims towards the black background and what the
            // bars added to the alpha thins, both by the trail's share.
            out.rgba.clear();
            out.rgba.extend_from_slice(&last.rgba);
            let keep = (trail * 256.0) as u32;
            for px in out.rgba.as_chunks_mut::<4>().0 {
                px[0] = ((px[0] as u32 * keep) >> 8) as u8;
                px[1] = ((px[1] as u32 * keep) >> 8) as u8;
                px[2] = ((px[2] as u32 * keep) >> 8) as u8;
                let a = px[3] as u32;
                if a > bg as u32 {
                    px[3] = (bg as u32 + (((a - bg as u32) * keep) >> 8)) as u8;
                }
            }
        }
        _ => {
            out.rgba.clear();
            out.rgba.resize((w * h * 4) as usize, 0);
            if bg > 0 {
                for px in out.rgba.as_chunks_mut::<4>().0 {
                    px[3] = bg;
                }
            }
        }
    }
}

/// A band's group by its centre frequency: sub-bass below 60 Hz, bass
/// below 250, mid below 2000, high above, as the bank groups them.
fn band_group(edges: (f32, f32)) -> usize {
    let centre = (edges.0.max(1.0) * edges.1.max(1.0)).sqrt();
    if centre < 60.0 {
        0
    } else if centre < 250.0 {
        1
    } else if centre < 2000.0 {
        2
    } else {
        3
    }
}

/// The scale labels of an analyser box, set once per look and box and
/// blitted every frame: each line with its top left in the box, and the
/// ticks and faint lines as rectangles with their alpha.
#[derive(Default)]
struct LabelCache {
    key: Option<LabelKey>,
    lines: Vec<(i32, i32, Frame)>,
    marks: Vec<(f32, f32, f32, f32, f32)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct LabelKey {
    w: u32,
    h: u32,
    n: usize,
    lo: u32,
    hi: u32,
    level: (u32, u32),
    flags: u8,
    size: u32,
    color: [u8; 3],
    layout: u8,
    mirror: i8,
    reflex: u32,
}

impl AnalyserMotion {
    /// Advance the bars and the peaks to `now_ms` and draw the box, the
    /// scale labels in the bitmap font.
    pub fn advance(&mut self, a: &plot::Analyser, now_ms: u64) -> &Frame {
        self.advance_with_fonts(a, now_ms, None)
    }

    /// Advance the bars and the peaks to `now_ms` and draw the box, the
    /// scale labels set in the theme's regular font.
    pub fn advance_with_fonts(
        &mut self,
        a: &plot::Analyser,
        now_ms: u64,
        fonts: Option<&Fonts>,
    ) -> &Frame {
        let dt = self
            .last_ms
            .map(|last| (now_ms.saturating_sub(last) as f32 / 1000.0).clamp(0.0, 0.1))
            .unwrap_or(0.0);
        self.last_ms = Some(now_ms);
        let look = &a.look;
        let height = a.h.max(1) as f32;
        for ch in 0..2 {
            let levels = &a.levels[ch];
            if self.smoothed[ch].len() != levels.len() {
                self.smoothed[ch] = levels.clone();
                self.peaks[ch] = vec![Peak::default(); levels.len()];
            }
            for (i, &level) in levels.iter().enumerate() {
                let s = &mut self.smoothed[ch][i];
                *s = look.smoothing * *s + (1.0 - look.smoothing) * level;
                let bar = *s;
                let peak = &mut self.peaks[ch][i];
                if bar >= peak.level {
                    *peak = Peak {
                        level: bar,
                        held_since_ms: now_ms,
                        velocity: 0.0,
                        fade: 0.0,
                    };
                } else if now_ms.saturating_sub(peak.held_since_ms) >= u64::from(look.peak_hold_ms)
                {
                    match look.peak_fade_ms {
                        Some(fade_ms) => {
                            peak.fade += dt * 1000.0 / fade_ms.max(1) as f32;
                            if peak.fade >= 1.0 {
                                peak.level = bar;
                                peak.fade = 0.0;
                                peak.held_since_ms = now_ms;
                            }
                        }
                        None => {
                            // Thousands of pixels a second squared, on the box.
                            peak.velocity += look.gravity * 1000.0 * dt;
                            peak.level -= peak.velocity * dt / height;
                            if peak.level <= bar {
                                peak.level = bar;
                                peak.velocity = 0.0;
                            }
                        }
                    }
                }
            }
        }
        // The onsets: a group that fired starts its effect now; each
        // effect fades over the decay.
        let mut onset = FrameFx {
            color: [
                look.onset_color[0],
                look.onset_color[1],
                look.onset_color[2],
                255,
            ],
            ..FrameFx::default()
        };
        if look.onset != lead::OnsetLook::Off {
            for g in 0..4 {
                if a.onsets & look.onset_groups & (1 << g) != 0 {
                    self.onset_since[g] = Some(now_ms);
                }
            }
            let decay = look.onset_decay_ms.max(1) as f32;
            let glow: [f32; 4] = std::array::from_fn(|g| {
                self.onset_since[g].map_or(0.0, |since| {
                    (1.0 - now_ms.saturating_sub(since) as f32 / decay).clamp(0.0, 1.0)
                        * look.onset_strength
                })
            });
            match look.onset {
                lead::OnsetLook::Flash => {
                    onset.boost = a.edges.iter().map(|&e| glow[band_group(e)]).collect();
                }
                lead::OnsetLook::Pulse => {
                    onset.pulse = glow.iter().cloned().fold(0.0, f32::max) * 0.3;
                }
                lead::OnsetLook::Ring => {
                    for g in 0..4 {
                        if let Some(since) = self.onset_since[g] {
                            let t = now_ms.saturating_sub(since) as f32 / decay;
                            if t < 1.0 && look.onset_groups & (1 << g) != 0 {
                                onset.rings.push((t, (1.0 - t) * look.onset_strength));
                            }
                        }
                    }
                }
                lead::OnsetLook::Off => {}
            }
        }
        let grown: [Vec<f32>; 2];
        let levels: &[Vec<f32>; 2] = if onset.pulse > 0.0 {
            grown = std::array::from_fn(|ch| {
                self.smoothed[ch]
                    .iter()
                    .map(|l| (l * (1.0 + onset.pulse)).min(1.0))
                    .collect()
            });
            &grown
        } else {
            &self.smoothed
        };
        // The echo's ghost follows the bars by its share a frame.
        if look.echo > 0.0 {
            for (ghost, channel) in self.ghost.iter_mut().zip(levels.iter()) {
                if ghost.len() != channel.len() {
                    *ghost = channel.clone();
                }
                for (g, l) in ghost.iter_mut().zip(channel.iter()) {
                    *g += (l - *g) * look.echo;
                }
            }
            onset.ghost = self.ghost.clone();
        }
        onset.rng.set(self.rng);
        self.which = 1 - self.which;
        let (first, second) = self.pictures.split_at_mut(1);
        let (picture, last) = if self.which == 0 {
            (&mut first[0], &second[0])
        } else {
            (&mut second[0], &first[0])
        };
        let previous = if look.trail > 0.0 && look.style != lead::LookStyle::Waterfall {
            Some((last, look.trail))
        } else {
            None
        };
        if look.radial {
            render_radial(
                a,
                levels,
                &self.peaks,
                now_ms,
                &mut self.radial,
                picture,
                &onset,
                previous,
            );
        } else {
            render_analyser(
                a,
                levels,
                &self.peaks,
                picture,
                fonts,
                &mut self.labels,
                &onset,
                &mut self.waterfall,
                previous,
            );
        }
        self.rng = onset.rng.get();
        picture
    }

    /// The bytes the two pictures hold, the polar map when there is one,
    /// and the waterfall's rows.
    pub fn bytes(&self) -> usize {
        self.pictures.iter().map(Frame::bytes).sum::<usize>()
            + self.radial.as_ref().map_or(0, |m| m.entries.len() * 4)
            + self.waterfall.bytes()
    }
}

/// The polar map of a box for the radial look: each pixel's distance from
/// the centre in sixty-fourths of a pixel (the high half) and its angle as
/// a turn in 65536 steps, 0 at the top and clockwise (the low half); a
/// pixel outside the box's circle is marked so.
struct RadialMap {
    w: u32,
    h: u32,
    entries: Vec<u32>,
    /// Per channel, each angle step's band (the high half) and how far
    /// across it the step lies (the low half), or outside for an angle
    /// that is the other channel's side; built for `bands_key`.
    bands: [Vec<u32>; 2],
    /// The band count, mirror and layout kind the tables were built for.
    bands_key: (usize, i8, u8),
}

const RADIAL_OUTSIDE: u32 = u32::MAX;

impl RadialMap {
    /// The angle tables for `n` bands, a mirror and a layout kind (1 for a
    /// half of the circle per channel, else the whole circle for each),
    /// kept until one of them changes.
    fn bands_for(&mut self, n: usize, mirror: i8, layout: u8) {
        let key = (n, mirror, layout);
        if self.bands_key == key && !self.bands[0].is_empty() {
            return;
        }
        self.bands_key = key;
        for ch in 0..2 {
            self.bands[ch] = (0..65536u32)
                .map(|u| {
                    let (band, frac) = if layout == 1 {
                        let (side, v) = if u >= 32768 {
                            (0usize, (65535 - u) * 2)
                        } else {
                            (1usize, u * 2)
                        };
                        if side != ch {
                            return RADIAL_OUTSIDE;
                        }
                        radial_band(v, n, mirror)
                    } else {
                        radial_band(u, n, mirror)
                    };
                    ((band as u32) << 16) | ((frac * 65536.0) as u32).min(0xffff)
                })
                .collect();
        }
    }

    fn build(w: u32, h: u32) -> Self {
        let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
        let rmax = cx.min(cy);
        let mut entries = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let r = (dx * dx + dy * dy).sqrt();
                if r >= rmax {
                    entries.push(RADIAL_OUTSIDE);
                    continue;
                }
                let mut angle = dx.atan2(-dy);
                if angle < 0.0 {
                    angle += std::f32::consts::TAU;
                }
                let a = ((angle / std::f32::consts::TAU) * 65536.0) as u32 & 0xffff;
                let rq = ((r * 64.0).round() as u32).min(0xfffe);
                entries.push((rq << 16) | a);
            }
        }
        Self {
            w,
            h,
            entries,
            bands: [Vec::new(), Vec::new()],
            bands_key: (0, 0, 0),
        }
    }
}

/// Every pixel of a `w` by `h` box whose centre lies within the ring
/// between `rin` and `rout` around the box's centre, row by row, a pixel
/// to spare on either edge so the caller's own test decides the rim.
fn annulus_rows(w: u32, h: u32, rin: f32, rout: f32, mut each: impl FnMut(usize, usize)) {
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let rout = rout + 1.0;
    let rin = (rin - 1.0).max(0.0);
    let y0 = (cy - rout).floor().max(0.0) as usize;
    let y1 = ((cy + rout).ceil().max(0.0) as usize).min(h as usize);
    for y in y0..y1 {
        let dy = y as f32 + 0.5 - cy;
        let d2 = dy * dy;
        if d2 >= rout * rout {
            continue;
        }
        let half = (rout * rout - d2).sqrt();
        let xa = (cx - half).floor().max(0.0) as usize;
        let xb = ((cx + half).ceil().max(0.0) as usize).min(w as usize);
        if d2 < rin * rin {
            let inner = (rin * rin - d2).sqrt();
            let ia = ((cx - inner).ceil().max(0.0) as usize).clamp(xa, xb);
            let ib = ((cx + inner).floor().max(0.0) as usize).clamp(ia, xb);
            for x in xa..ia {
                each(x, y);
            }
            for x in ib..xb {
                each(x, y);
            }
        } else {
            for x in xa..xb {
                each(x, y);
            }
        }
    }
}

/// Which band a point of the circle falls in, from its angle around the
/// turn (65536 steps), for `n` bands over the whole turn or, mirrored,
/// over half of it and back: the band and how far across it the point is.
fn radial_band(u: u32, n: usize, mirror: i8) -> (usize, f32) {
    let v = match mirror {
        0 => u,
        1 => {
            if u < 32768 {
                u * 2
            } else {
                (65535 - u) * 2
            }
        }
        _ => {
            if u < 32768 {
                65535 - u * 2
            } else {
                65535 - (65535 - u) * 2
            }
        }
    };
    let pos = v * n as u32;
    let band = ((pos >> 16) as usize).min(n - 1);
    (band, (pos & 0xffff) as f32 / 65536.0)
}

/// The radial look: each channel's ring between its base circle and the
/// furthest tip of the frame is walked row by row, and every pixel of it
/// asks the polar map where it is, finds its band by the angle table, and
/// takes the bar's colour when it lies before the bar's tip, or the
/// peak's when it lies on the peak's ring. The whole picture turns with
/// `spin`.
fn render_radial(
    a: &plot::Analyser,
    smoothed: &[Vec<f32>; 2],
    peaks: &[Vec<Peak>; 2],
    now_ms: u64,
    map: &mut Option<RadialMap>,
    out: &mut Frame,
    onset: &FrameFx,
    previous: Option<(&Frame, f32)>,
) {
    let (w, h) = (a.w.max(1), a.h.max(1));
    let look = &a.look;
    let bg = (look.bgr_alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    begin_frame(out, w, h, bg, previous);
    out.blend = if look.blend_add {
        Blend::Add
    } else {
        Blend::Normal
    };
    let n = smoothed[0].len();
    if n == 0 {
        return;
    }
    if map.as_ref().is_none_or(|m| m.w != w || m.h != h) {
        *map = Some(RadialMap::build(w, h));
    }
    let Some(map) = map.as_mut() else {
        return;
    };
    let stereo = a.stereo && smoothed[1].len() == n;
    let layout = if stereo {
        look.layout
    } else {
        lead::Layout::Single
    };
    let rmax = (w.min(h) as f32 / 2.0 - 1.0).max(1.0);
    let r0 = rmax * look.radius.clamp(0.0, 0.95);
    // Each channel's base circle and reach, negative for inward.
    let (base, reach): ([f32; 2], [f32; 2]) = match layout {
        lead::Layout::DualVertical => ([r0, r0], [rmax - r0, -r0]),
        _ if look.radial_invert => ([rmax, rmax], [-(rmax - r0), -(rmax - r0)]),
        _ => ([r0, r0], [rmax - r0, rmax - r0]),
    };
    let spin = (((now_ms as f64 / 60_000.0) * f64::from(look.spin_rpm)).rem_euclid(1.0) * 65536.0)
        as u32
        & 0xffff;
    // The gap between bars as a share of a sector: a share below 1 as
    // given, else pixels measured on the base circle.
    let space = if look.bar_space < 1.0 {
        look.bar_space
    } else {
        look.bar_space * n as f32 / (std::f32::consts::TAU * r0.max(1.0))
    }
    .clamp(0.0, 0.9);
    let (gap_lo, gap_hi) = (space / 2.0, 1.0 - space / 2.0);
    let left = look.palette_left.as_ref().unwrap_or(&look.palette);
    let right = look.palette_right.as_ref().unwrap_or(&look.palette);
    let palettes = [left, right];
    // The colours: along the reach for the gradient mode, else one per band.
    let gradient: [Vec<[u8; 4]>; 2] = [
        (0..256).map(|k| left.at(k as f32 / 255.0)).collect(),
        (0..256).map(|k| right.at(k as f32 / 255.0)).collect(),
    ];
    let inks: [Vec<[u8; 4]>; 2] = std::array::from_fn(|ch| {
        (0..n)
            .map(|i| match look.color_mode {
                lead::ColorMode::Index => palettes[ch].at(if n > 1 {
                    i as f32 / (n - 1) as f32
                } else {
                    0.0
                }),
                lead::ColorMode::Level => {
                    palettes[ch].for_level(smoothed[ch].get(i).copied().unwrap_or(0.0))
                }
                lead::ColorMode::Gradient => [0, 0, 0, 0],
            })
            .collect()
    });
    let channel_alpha = [
        1.0f32,
        if matches!(layout, lead::Layout::DualCombined) {
            0.5
        } else {
            1.0
        },
    ];
    let channels: &[usize] = match layout {
        lead::Layout::Single => &[0],
        _ => &[0, 1],
    };
    let layout_kind: u8 = match layout {
        lead::Layout::DualHorizontal => 1,
        _ => 0,
    };
    let mirror = match layout {
        lead::Layout::DualHorizontal => look.mirror,
        _ if look.mirror == 0 => 0,
        _ => 1,
    };
    map.bands_for(n, mirror, layout_kind);
    let map: &RadialMap = map;
    let gap_lo = (gap_lo * 65536.0) as u32;
    let gap_hi = (gap_hi * 65536.0) as u32;
    let width = w as usize;
    for &ch in channels {
        let (b, reach) = (base[ch], reach[ch]);
        let span = reach.abs().max(1.0);
        // The ring this frame: from the base circle to the loudest bar's
        // tip, or its peak, and the peak ring's width beyond.
        let mut top = 0.0f32;
        for (i, &level) in smoothed[ch].iter().enumerate() {
            top = top.max(level);
            if look.peaks {
                if let Some(peak) = peaks[ch].get(i) {
                    top = top.max(peak.level);
                }
            }
        }
        let far = (top.clamp(0.0, 1.0) * span + 1.5).min(span);
        if far <= 0.0 {
            continue;
        }
        let (rin, rout) = if reach > 0.0 {
            (b, b + far)
        } else {
            ((b - far).max(0.0), b)
        };
        let table = &map.bands[ch];
        annulus_rows(w, h, rin, rout, |x, y| {
            let e = map.entries[y * width + x];
            if e == RADIAL_OUTSIDE {
                return;
            }
            let r = (e >> 16) as f32 / 64.0;
            let along = (r - b) * reach.signum();
            if along < 0.0 || along >= span {
                return;
            }
            let u = ((e & 0xffff) + 65536 - spin) & 0xffff;
            let entry = table[u as usize];
            if entry == RADIAL_OUTSIDE {
                return;
            }
            let band = (entry >> 16) as usize;
            let frac = entry & 0xffff;
            if frac < gap_lo || frac > gap_hi {
                return;
            }
            let at = (y * width + x) * 4;
            let level = smoothed[ch][band].clamp(0.0, 1.0);
            let bar_alpha =
                channel_alpha[ch] * look.fill_alpha * if look.alpha_bars { level } else { 1.0 };
            if along < level * span {
                let c = match look.color_mode {
                    lead::ColorMode::Gradient => {
                        gradient[ch][((along / span) * 255.0).clamp(0.0, 255.0) as usize]
                    }
                    _ => inks[ch][band],
                };
                let alpha = ((c[3] as f32 * bar_alpha).round().clamp(0.0, 255.0)) as u32;
                put_px(out, at, c, alpha);
                if let Some(&boost) = onset.boost.get(band) {
                    if boost > 0.0 {
                        let a = (boost * 0.7 * 255.0).round() as u32;
                        put_px(out, at, onset.color, a);
                    }
                }
            }
            if look.peaks {
                let peak = peaks[ch].get(band).copied().unwrap_or_default();
                if peak.level > level + 0.002 {
                    let pr = peak.level.clamp(0.0, 1.0) * span;
                    if (along - pr).abs() < 1.0 {
                        let c = match look.color_mode {
                            lead::ColorMode::Gradient => {
                                palettes[ch].at(peak.level.clamp(0.0, 1.0))
                            }
                            lead::ColorMode::Index => inks[ch][band],
                            lead::ColorMode::Level => palettes[ch].for_level(peak.level),
                        };
                        let mark_alpha = channel_alpha[ch] * (1.0 - peak.fade).clamp(0.0, 1.0);
                        let alpha = ((c[3] as f32 * mark_alpha).round().clamp(0.0, 255.0)) as u32;
                        put_px(out, at, c, alpha);
                    }
                }
            }
        });
    }
    // The onset rings, on the first channel's reach, each on its own thin ring.
    if !onset.rings.is_empty() {
        let span = reach[0].abs().max(1.0);
        for &(t, ring_alpha) in &onset.rings {
            let ring = base[0] + t * span * reach[0].signum();
            let a = (ring_alpha * 255.0).round().clamp(0.0, 255.0) as u32;
            annulus_rows(w, h, ring - 1.0, ring + 1.0, |x, y| {
                let e = map.entries[y * width + x];
                if e == RADIAL_OUTSIDE {
                    return;
                }
                let r = (e >> 16) as f32 / 64.0;
                let along = (r - base[0]) * reach[0].signum();
                if along >= 0.0 && along < span && (along - t * span).abs() < 1.0 {
                    let at = (y * width + x) * 4;
                    put_px(out, at, onset.color, a);
                }
            });
        }
    }
}

/// The bars' area of one channel inside the box, and which way they grow.
struct Area {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    /// Bars grow downwards from the top of the area.
    hanging: bool,
    /// The bands run right to left.
    reversed: bool,
}

/// What a fill is coloured with: one colour, a colour per row of the box
/// (the palette along the bars), or a colour per column (across them).
enum Ink<'a> {
    Solid([u8; 4]),
    Rows(&'a [[u8; 4]]),
    Cols(&'a [[u8; 4]]),
}

impl Ink<'_> {
    fn at(&self, x: usize, y: usize) -> [u8; 4] {
        match self {
            Ink::Solid(c) => *c,
            Ink::Rows(rows) => rows[y.min(rows.len().saturating_sub(1))],
            Ink::Cols(cols) => cols[x.min(cols.len().saturating_sub(1))],
        }
    }
}

/// `c` at `a` (0 to 255) over one pixel, straight alpha: opaque copies.
#[inline]
/// One pixel added: the colour times its alpha on top of what is there,
/// saturating, the alpha joined as a cover would join it.
fn add_px(d: &mut [u8], c: [u8; 4], a: u32) {
    if a == 0 {
        return;
    }
    let a = a.min(255);
    for k in 0..3 {
        d[k] = (d[k] as u32 + (c[k] as u32 * a + 127) / 255).min(255) as u8;
    }
    d[3] = (a + (d[3] as u32 * (255 - a) + 127) / 255).min(255) as u8;
}

/// One pixel of the frame at byte offset `at`, landed as the frame blends.
#[inline]
fn put_px(out: &mut Frame, at: usize, c: [u8; 4], a: u32) {
    match out.blend {
        Blend::Add => add_px(&mut out.rgba[at..at + 4], c, a),
        Blend::Normal => blend_px(&mut out.rgba[at..at + 4], c, a),
    }
}

fn blend_px(d: &mut [u8], c: [u8; 4], a: u32) {
    if a == 0 {
        return;
    }
    if a >= 255 {
        d[0] = c[0];
        d[1] = c[1];
        d[2] = c[2];
        d[3] = 255;
        return;
    }
    let da = d[3] as u32;
    let oa = a + (da * (255 - a) + 127) / 255;
    if oa == 0 {
        return;
    }
    let keep = da * (255 - a);
    for k in 0..3 {
        let num = c[k] as u32 * a * 255 + d[k] as u32 * keep;
        d[k] = (num / (oa * 255)) as u8;
    }
    d[3] = oa as u8;
}

/// A run of one colour at one alpha: the blend's factors worked out once,
/// so a pixel on the background it was worked out for costs multiplies
/// and shifts, and only a pixel already painted takes the general path.
fn blend_run(px: &mut [u8], c: [u8; 4], a: u32, under: u32) {
    if a == 0 {
        return;
    }
    if a >= 255 {
        for d in px.as_chunks_mut::<4>().0 {
            d[0] = c[0];
            d[1] = c[1];
            d[2] = c[2];
            d[3] = 255;
        }
        return;
    }
    let oa = a + (under * (255 - a) + 127) / 255;
    let keep = under * (255 - a);
    let inv = (1u64 << 24) / u64::from(oa.max(1) * 255);
    let oa8 = oa.min(255) as u8;
    for d in px.as_chunks_mut::<4>().0 {
        if d[3] as u32 == under {
            for k in 0..3 {
                let num = u64::from(c[k] as u32 * a * 255 + d[k] as u32 * keep);
                d[k] = ((num * inv) >> 24).min(255) as u8;
            }
            d[3] = oa8;
        } else {
            blend_px(d, c, a);
        }
    }
}

/// `ink` at `alpha` over the pixels of row `y` from `xa` to `xb` (box
/// pixels, fractional ends covering their pixel in part), inside `clip`.
/// The run between the ends is filled without per-pixel arithmetic on the
/// coverage; a colour per row is looked up once for the span.
fn fill_span(
    out: &mut Frame,
    clip: (i32, i32, i32, i32),
    y: i32,
    xa: f32,
    xb: f32,
    ink: &Ink,
    alpha: f32,
) {
    if y < clip.1 || y >= clip.3 || alpha <= 0.0 || xb <= xa {
        return;
    }
    let (cx0, cx1) = (clip.0.max(0) as f32, clip.2.min(out.width as i32) as f32);
    let (xa, xb) = (xa.max(cx0), xb.min(cx1));
    if xb <= xa {
        return;
    }
    let width = out.width as usize;
    let row = y as usize * width;
    let span_alpha = (alpha * 255.0).round().clamp(0.0, 255.0) as u32;
    if out.blend == Blend::Add {
        let (x0, x1) = (
            (xa.round().max(0.0) as usize).min(width),
            (xb.round().max(0.0) as usize).min(width),
        );
        for x in x0..x1 {
            let c = ink.at(x, y as usize);
            let a = ((c[3] as u32 * span_alpha) + 127) / 255;
            add_px(&mut out.rgba[(row + x) * 4..(row + x) * 4 + 4], c, a);
        }
        return;
    }
    let x0 = xa.ceil() as usize;
    let x1 = (xb.floor() as usize).min(width);
    // The partial pixel at the left end.
    if x0 > 0 && (x0 as f32) > xa {
        edge_px(
            out,
            ink,
            row,
            y as usize,
            x0 - 1,
            span_alpha,
            x0 as f32 - xa,
        );
    }
    if x0 < x1 {
        match ink {
            Ink::Solid(_) | Ink::Rows(_) => {
                let c = ink.at(x0, y as usize);
                let a = ((c[3] as u32 * span_alpha) + 127) / 255;
                let under = out.rgba[(row + x0) * 4 + 3] as u32;
                blend_run(&mut out.rgba[(row + x0) * 4..(row + x1) * 4], c, a, under);
            }
            Ink::Cols(_) => {
                for x in x0..x1 {
                    let c = ink.at(x, y as usize);
                    let a = ((c[3] as u32 * span_alpha) + 127) / 255;
                    let at = (row + x) * 4;
                    blend_px(&mut out.rgba[at..at + 4], c, a);
                }
            }
        }
    }
    // The partial pixel at the right end.
    if xb > x1 as f32 && x1 < width {
        edge_px(out, ink, row, y as usize, x1, span_alpha, xb - x1 as f32);
    }
}

/// One pixel at an end of a span, covered in part.
fn edge_px(
    out: &mut Frame,
    ink: &Ink,
    row: usize,
    y: usize,
    x: usize,
    span_alpha: u32,
    cover: f32,
) {
    let c = ink.at(x, y);
    let cover = (cover.clamp(0.0, 1.0) * 255.0).round() as u32;
    let a = (c[3] as u32 * span_alpha * cover + 127 * 255) / (255 * 255);
    let at = (row + x) * 4;
    blend_px(&mut out.rgba[at..at + 4], c, a.min(255));
}

/// The blend of one alpha over one destination alpha, worked out once.
struct Factors {
    a: u32,
    under: u32,
    keep: u32,
    inv: u64,
    oa8: u8,
}

fn factors(a: u32, under: u32) -> Factors {
    let oa = a + (under * (255 - a) + 127) / 255;
    Factors {
        a,
        under,
        keep: under * (255 - a),
        inv: (1u64 << 24) / u64::from(oa.max(1) * 255),
        oa8: oa.min(255) as u8,
    }
}

/// One pixel with the factors: multiplies and a shift on the destination
/// alpha they were made for, the general path on any other.
#[inline]
fn blend_fast(d: &mut [u8], c: [u8; 4], f: &Factors) {
    if f.a >= 255 {
        d[0] = c[0];
        d[1] = c[1];
        d[2] = c[2];
        d[3] = 255;
        return;
    }
    if d[3] as u32 != f.under {
        blend_px(d, c, f.a);
        return;
    }
    for k in 0..3 {
        let num = u64::from(c[k] as u32 * f.a * 255 + d[k] as u32 * f.keep);
        d[k] = ((num * f.inv) >> 24).min(255) as u8;
    }
    d[3] = f.oa8;
}

/// A rectangle of `ink`, its edges snapped to whole pixels, blended over
/// what is under it: the factors are worked out once per rectangle (per
/// row for a palette whose stops carry their own alpha), so a bar costs
/// its pixels and little else.
fn fill_rect(
    out: &mut Frame,
    clip: (i32, i32, i32, i32),
    xa: f32,
    ya: f32,
    xb: f32,
    yb: f32,
    ink: &Ink,
    alpha: f32,
) {
    if alpha <= 0.0 || xb <= xa || yb <= ya {
        return;
    }
    let width = out.width as usize;
    let x0 = (xa.round() as i32).max(clip.0).max(0) as usize;
    let mut x1 = (xb.round() as i32).min(clip.2).min(width as i32) as usize;
    if x1 <= x0 {
        x1 = (x0 + 1).min(width);
        if x1 <= x0 || (x0 as i32) >= clip.2 {
            return;
        }
    }
    let y0 = (ya.round() as i32).max(clip.1).max(0) as usize;
    let y1 = (yb.round() as i32).min(clip.3).min(out.height as i32) as usize;
    if y1 <= y0 {
        return;
    }
    let span_alpha = (alpha * 255.0).round().clamp(0.0, 255.0) as u32;
    if out.blend == Blend::Add {
        for y in y0..y1 {
            let row = y * width;
            for x in x0..x1 {
                let c = ink.at(x, y);
                let a = ((c[3] as u32 * span_alpha) + 127) / 255;
                add_px(&mut out.rgba[(row + x) * 4..(row + x) * 4 + 4], c, a);
            }
        }
        return;
    }
    let under = out.rgba[(y0 * width + x0) * 4 + 3] as u32;
    match ink {
        Ink::Solid(c) => {
            let f = factors(((c[3] as u32 * span_alpha) + 127) / 255, under);
            if f.a == 0 {
                return;
            }
            for y in y0..y1 {
                let row = y * width;
                for d in out.rgba[(row + x0) * 4..(row + x1) * 4]
                    .as_chunks_mut::<4>()
                    .0
                {
                    blend_fast(d, *c, &f);
                }
            }
        }
        Ink::Rows(table) => {
            let mut f = factors(0, under);
            let mut last_a = u32::MAX;
            for y in y0..y1 {
                let c = table[y.min(table.len().saturating_sub(1))];
                let a = ((c[3] as u32 * span_alpha) + 127) / 255;
                if a != last_a {
                    f = factors(a, under);
                    last_a = a;
                }
                if a == 0 {
                    continue;
                }
                let row = y * width;
                for d in out.rgba[(row + x0) * 4..(row + x1) * 4]
                    .as_chunks_mut::<4>()
                    .0
                {
                    blend_fast(d, c, &f);
                }
            }
        }
        Ink::Cols(table) => {
            for y in y0..y1 {
                let row = y * width;
                for x in x0..x1 {
                    let c = table[x.min(table.len().saturating_sub(1))];
                    let a = ((c[3] as u32 * span_alpha) + 127) / 255;
                    let at = (row + x) * 4;
                    blend_px(&mut out.rgba[at..at + 4], c, a);
                }
            }
        }
    }
}

/// The palette as a colour per row of the box for the bars in `area`,
/// their base the palette's start; or per column when it runs across.
fn palette_lookup(
    palette: &lead::Palette,
    span: &Area,
    horizontal: bool,
    box_w: u32,
    box_h: u32,
) -> Vec<[u8; 4]> {
    if horizontal {
        (0..box_w as usize)
            .map(|x| {
                let t = ((x as f32 + 0.5 - span.x) / span.w.max(1.0)).clamp(0.0, 1.0);
                palette.at(if span.reversed { 1.0 - t } else { t })
            })
            .collect()
    } else {
        (0..box_h as usize)
            .map(|y| {
                let t = if span.hanging {
                    (y as f32 + 0.5 - span.y) / span.h.max(1.0)
                } else {
                    (span.y + span.h - y as f32 - 0.5) / span.h.max(1.0)
                };
                palette.at(t.clamp(0.0, 1.0))
            })
            .collect()
    }
}

/// One channel's bars, peaks and LEDs into the frame. `span` is the area
/// the palette runs over, the bars' own area unless a split palette spans
/// both channels.
#[allow(clippy::too_many_arguments)]
fn draw_channel(
    out: &mut Frame,
    look: &lead::Look,
    palette: &lead::Palette,
    area: &Area,
    span: &Area,
    levels: &[f32],
    peaks: &[Peak],
    alpha: f32,
    onset: &FrameFx,
    ghost: &[f32],
) {
    let n = levels.len();
    // An onset's flash: the bar brightened towards the onset colour.
    let flash = |out: &mut Frame, i: usize, level: f32, xa: f32, xb: f32| {
        if let Some(&boost) = onset.boost.get(i) {
            if boost > 0.0 && level > 0.0 {
                let (top, bottom) = if area.hanging {
                    (area.y, area.y + level * area.h)
                } else {
                    (area.y + area.h - level * area.h, area.y + area.h)
                };
                let clip = (
                    area.x.floor() as i32,
                    area.y.floor() as i32,
                    (area.x + area.w).ceil() as i32,
                    (area.y + area.h).ceil() as i32,
                );
                fill_rect(
                    out,
                    clip,
                    xa,
                    top,
                    xb,
                    bottom,
                    &Ink::Solid(onset.color),
                    alpha * boost * 0.7,
                );
            }
        }
    };
    let bar_clip = (
        area.x.floor() as i32,
        area.y.floor() as i32,
        (area.x + area.w).ceil() as i32,
        (area.y + area.h).ceil() as i32,
    );
    // A sparkle: a speck above a loud bar, some frames, in the tip's colour.
    let sparkle = |out: &mut Frame, i: usize, level: f32, xa: f32, xb: f32| {
        if !look.sparkle || level <= 0.55 {
            return;
        }
        if onset.random() >= (level - 0.55) * 1.6 {
            return;
        }
        let x = (xa + xb) / 2.0 + (onset.random() - 0.5) * (xb - xa) * 1.4;
        let lift = 2.0 + onset.random() * 6.0;
        let y = if area.hanging {
            area.y + level * area.h + lift
        } else {
            area.y + area.h - level * area.h - lift
        };
        let radius = 1.0 + onset.random() * 1.2;
        let tip = Ink::Solid(match look.color_mode {
            lead::ColorMode::Index => palette.at(if levels.len() > 1 {
                i as f32 / (levels.len() - 1) as f32
            } else {
                0.0
            }),
            _ => palette.at(level.clamp(0.0, 1.0)),
        });
        let sparkle_clip = (bar_clip.0, 0, bar_clip.2, out.height as i32);
        fill_disc(
            out,
            sparkle_clip,
            x,
            y,
            radius,
            &tip,
            alpha * (0.4 + onset.random() * 0.55),
        );
    };
    // The echo's mark: a thin line at the ghost's level in the palette's top colour.
    let echo_mark = |out: &mut Frame, i: usize, xa: f32, xb: f32| {
        let Some(&g) = ghost.get(i) else {
            return;
        };
        let g = g.clamp(0.0, 1.0);
        let y = if area.hanging {
            area.y + g * area.h
        } else {
            area.y + area.h - g * area.h
        };
        fill_rect(
            out,
            bar_clip,
            xa,
            y - 0.5,
            xb,
            y + 0.5,
            &Ink::Solid(palette.at(1.0)),
            alpha * 0.6,
        );
    };
    if n == 0 || area.w < 1.0 || area.h < 1.0 {
        return;
    }
    let clip = (
        area.x.floor() as i32,
        area.y.floor() as i32,
        (area.x + area.w).ceil() as i32,
        (area.y + area.h).ceil() as i32,
    );
    let pitch = area.w / n as f32;
    let space = if look.bar_space < 1.0 {
        pitch * look.bar_space
    } else {
        look.bar_space
    }
    .clamp(0.0, (pitch - 1.0).max(0.0));
    let bar_w = (pitch - space).max(1.0);
    let lookup = match look.color_mode {
        lead::ColorMode::Gradient => Some(palette_lookup(
            palette,
            span,
            look.palette_horizontal,
            out.width,
            out.height,
        )),
        _ => None,
    };
    let ink_for = |i: usize, level: f32| -> Ink<'_> {
        match (look.color_mode, lookup.as_deref()) {
            (lead::ColorMode::Gradient, Some(table)) => {
                if look.palette_horizontal {
                    Ink::Cols(table)
                } else {
                    Ink::Rows(table)
                }
            }
            (lead::ColorMode::Index, _) => Ink::Solid(palette.at(if n > 1 {
                i as f32 / (n - 1) as f32
            } else {
                0.0
            })),
            (lead::ColorMode::Level, _) => Ink::Solid(palette.for_level(level)),
            _ => Ink::Solid([255, 255, 255, 255]),
        }
    };
    let led_cells = if look.led {
        let wanted = if look.led_max > 0 {
            look.led_max as f32
        } else {
            (area.h / (bar_w * (1.0 + look.led_space.0)).max(1.0)).round()
        };
        wanted.clamp(1.0, area.h.max(1.0)) as u32
    } else {
        0
    };
    for (i, raw) in levels.iter().enumerate() {
        let level = raw.clamp(0.0, 1.0);
        let slot = if area.reversed { n - 1 - i } else { i };
        let x = area.x + slot as f32 * pitch + space / 2.0;
        let (xa, xb) = (x, x + bar_w);
        let bar_alpha = if look.lumi {
            alpha * level
        } else if look.alpha_bars {
            alpha * look.fill_alpha * level
        } else {
            alpha * look.fill_alpha
        };
        let ink = ink_for(i, level);
        let bar_h = if look.lumi { area.h } else { level * area.h };
        if look.led && led_cells > 0 {
            let cell = area.h / led_cells as f32;
            let gap = (cell * look.led_space.0).min(cell * 0.9);
            let lit = ((level * led_cells as f32).round() as u32).min(led_cells);
            let inset = if look.led_space.1 < 1.0 {
                bar_w * look.led_space.1 / 2.0
            } else {
                look.led_space.1 / 2.0
            }
            .clamp(0.0, (bar_w / 2.0 - 0.5).max(0.0));
            for k in 0..lit {
                let (cy0, cy1) = if area.hanging {
                    (
                        area.y + k as f32 * cell,
                        area.y + (k + 1) as f32 * cell - gap,
                    )
                } else {
                    (
                        area.y + area.h - (k + 1) as f32 * cell + gap,
                        area.y + area.h - k as f32 * cell,
                    )
                };
                if look.led_true {
                    // Each cell in the colour of its own height, a column of lamps.
                    let t = (k as f32 + 0.5) / led_cells as f32;
                    let lamp = Ink::Solid(palette.at(t));
                    fill_rect(
                        out,
                        clip,
                        xa + inset,
                        cy0,
                        xb - inset,
                        cy1,
                        &lamp,
                        bar_alpha,
                    );
                } else {
                    let cell_alpha = if look.bar_fade {
                        bar_alpha * (0.3 + 0.7 * (k as f32 + 0.5) / led_cells as f32)
                    } else {
                        bar_alpha
                    };
                    fill_rect(
                        out,
                        clip,
                        xa + inset,
                        cy0,
                        xb - inset,
                        cy1,
                        &ink,
                        cell_alpha,
                    );
                }
            }
            flash(out, i, level, xa, xb);
            sparkle(out, i, level, xa, xb);
            echo_mark(out, i, xa, xb);
        } else if bar_h >= 0.5 {
            let (top, bottom) = if area.hanging {
                (area.y, area.y + bar_h)
            } else {
                (area.y + area.h - bar_h, area.y + area.h)
            };
            let (y0, y1) = (top.round() as i32, bottom.round() as i32);
            if look.bar_glow > 0.0 {
                // A soft halo behind the bar, wider by the glow's share of the pitch.
                let extra = pitch * look.bar_glow / 2.0;
                fill_rect(
                    out,
                    clip,
                    xa - extra,
                    top,
                    xb + extra,
                    bottom,
                    &ink,
                    bar_alpha * 0.25,
                );
            }
            if !look.round && !look.outline {
                if look.bar_fade {
                    // Dim at the base, bright at the tip: three slices.
                    for (k, share) in [0.3f32, 0.6, 1.0].iter().enumerate() {
                        let (s0, s1) = (k as f32 / 3.0, (k as f32 + 1.0) / 3.0);
                        let (ya, yb) = if area.hanging {
                            (
                                top + (bottom - top) * (1.0 - s1),
                                top + (bottom - top) * (1.0 - s0),
                            )
                        } else {
                            (bottom - (bottom - top) * s1, bottom - (bottom - top) * s0)
                        };
                        fill_rect(out, clip, xa, ya, xb, yb, &ink, bar_alpha * share);
                    }
                } else {
                    fill_rect(out, clip, xa, top, xb, bottom, &ink, bar_alpha);
                }
                flash(out, i, level, xa, xb);
                sparkle(out, i, level, xa, xb);
                echo_mark(out, i, xa, xb);
                continue;
            }
            // A rounded or outlined bar: the straight body as rectangles,
            // and only the rows the corners or the end lines touch one by
            // one, all on the same whole-pixel edges so the two meet
            // without a seam. The corners round over `radius` rows at the
            // cap; an outline's lines are `lw` rows at both ends.
            let radius = if look.round {
                (bar_w / 2.0).min(bar_h).max(0.5)
            } else {
                0.0
            };
            let lw = if look.outline {
                look.line_width.max(1.0).round()
            } else {
                0.0
            };
            let sxa = xa.round();
            let sxb = xb.round().max(sxa + 1.0);
            let rows = y1 - y0;
            let cap_rows = (radius.ceil() as i32).max(lw as i32);
            let base_rows = lw as i32;
            let (body_y0, body_y1) = if area.hanging {
                (y0 + base_rows, y1 - cap_rows)
            } else {
                (y0 + cap_rows, y1 - base_rows)
            };
            if body_y1 > body_y0 {
                let (bt, bb) = (body_y0 as f32, body_y1 as f32);
                if look.outline {
                    if look.fill_alpha > 0.0 && sxb - sxa > 2.0 * lw {
                        fill_rect(out, clip, sxa + lw, bt, sxb - lw, bb, &ink, bar_alpha);
                    }
                    fill_rect(out, clip, sxa, bt, (sxa + lw).min(sxb), bb, &ink, alpha);
                    fill_rect(out, clip, (sxb - lw).max(sxa), bt, sxb, bb, &ink, alpha);
                } else {
                    fill_rect(out, clip, sxa, bt, sxb, bb, &ink, bar_alpha);
                }
            }
            for y in y0.max(clip.1)..y1.min(clip.3) {
                if y >= body_y0 && y < body_y1 {
                    continue;
                }
                // The outer corners rounded: the cap rows narrow by the circle.
                let from_cap = if area.hanging {
                    (y1 - 1 - y) as f32 + 0.5
                } else {
                    (y - y0) as f32 + 0.5
                };
                let inset = if radius > 0.0 && from_cap < radius {
                    let d = radius - from_cap;
                    radius - (radius * radius - d * d).max(0.0).sqrt()
                } else {
                    0.0
                };
                if look.outline {
                    let from_base = rows as f32 - from_cap;
                    if from_cap < lw || from_base < lw {
                        fill_span(out, clip, y, sxa + inset, sxb - inset, &ink, alpha);
                    } else {
                        if look.fill_alpha > 0.0 {
                            fill_span(
                                out,
                                clip,
                                y,
                                sxa + inset + lw,
                                sxb - inset - lw,
                                &ink,
                                bar_alpha,
                            );
                        }
                        fill_span(out, clip, y, sxa + inset, sxa + inset + lw, &ink, alpha);
                        fill_span(out, clip, y, sxb - inset - lw, sxb - inset, &ink, alpha);
                    }
                } else {
                    fill_span(out, clip, y, sxa + inset, sxb - inset, &ink, bar_alpha);
                }
            }
            flash(out, i, level, sxa, sxb);
            sparkle(out, i, level, sxa, sxb);
            echo_mark(out, i, sxa, sxb);
        }
        // The peak: a thin mark at the peak's height, fading if it fades.
        if look.peaks && !look.lumi {
            let peak = peaks.get(i).copied().unwrap_or_default();
            if peak.level > level + 0.002 {
                let ph = (peak.level.clamp(0.0, 1.0) * area.h).max(2.0);
                let py = if area.hanging {
                    area.y + ph - 2.0
                } else {
                    area.y + area.h - ph
                };
                let mark_alpha = alpha * (1.0 - peak.fade).clamp(0.0, 1.0);
                let mark = match look.color_mode {
                    lead::ColorMode::Gradient => Ink::Solid(palette.at(peak.level.clamp(0.0, 1.0))),
                    lead::ColorMode::Index => Ink::Solid(palette.at(if n > 1 {
                        i as f32 / (n - 1) as f32
                    } else {
                        0.0
                    })),
                    lead::ColorMode::Level => Ink::Solid(palette.for_level(peak.level)),
                };
                fill_rect(out, clip, xa, py, xb, py + 2.0, &mark, mark_alpha);
            }
        }
    }
}

/// One channel as a graph: the band levels, read at each band's centre and
/// joined straight between neighbours, give every column of the area a
/// height; the area under that line is filled at `fill.alpha` in the
/// palette, the line itself is `line.width` pixels thick at the ink of its
/// height, and the peaks are marks per band or, with `peaks.line`, a line
/// of their own.
#[allow(clippy::too_many_arguments)]
fn draw_graph(
    out: &mut Frame,
    look: &lead::Look,
    palette: &lead::Palette,
    area: &Area,
    span: &Area,
    levels: &[f32],
    peaks: &[Peak],
    alpha: f32,
    onset: &FrameFx,
    ghost: &[f32],
) {
    let n = levels.len();
    if n == 0 || area.w < 1.0 || area.h < 1.0 {
        return;
    }
    let clip = (
        area.x.floor() as i32,
        area.y.floor() as i32,
        (area.x + area.w).ceil() as i32,
        (area.y + area.h).ceil() as i32,
    );
    let pitch = area.w / n as f32;
    let band_at = |i: usize| -> usize {
        if area.reversed {
            n - 1 - i
        } else {
            i
        }
    };
    // A value across the area, between the band centres, straight between neighbours.
    let across = |values: &dyn Fn(usize) -> f32, xf: f32| -> f32 {
        let t = ((xf - area.x) / pitch - 0.5).clamp(0.0, (n - 1) as f32);
        let i = t.floor() as usize;
        let f = t - i as f32;
        let a = values(band_at(i));
        let b = values(band_at((i + 1).min(n - 1)));
        (a + (b - a) * f).clamp(0.0, 1.0)
    };
    let level_of = |i: usize| levels[i];
    let peak_of = |i: usize| peaks.get(i).map_or(0.0, |p| p.level);
    let fade_of = |i: usize| peaks.get(i).map_or(0.0, |p| p.fade);
    let lookup = match look.color_mode {
        lead::ColorMode::Gradient => Some(palette_lookup(
            palette,
            span,
            look.palette_horizontal,
            out.width,
            out.height,
        )),
        _ => None,
    };
    let lw = look.line_width.max(0.0);
    let y_of = |level: f32| -> f32 {
        if area.hanging {
            area.y + level * area.h
        } else {
            area.y + area.h - level * area.h
        }
    };
    let x0 = area.x.floor().max(clip.0 as f32) as i32;
    let x1 = (area.x + area.w).ceil().min(clip.2 as f32) as i32;
    for x in x0..x1 {
        let xf = x as f32;
        let level = across(&level_of, xf + 0.5);
        let t_across = ((xf + 0.5 - area.x) / area.w).clamp(0.0, 1.0);
        let ink_for = |lvl: f32| -> Ink<'_> {
            match (look.color_mode, lookup.as_deref()) {
                (lead::ColorMode::Gradient, Some(table)) => {
                    if look.palette_horizontal {
                        Ink::Cols(table)
                    } else {
                        Ink::Rows(table)
                    }
                }
                (lead::ColorMode::Index, _) => Ink::Solid(palette.at(if area.reversed {
                    1.0 - t_across
                } else {
                    t_across
                })),
                (lead::ColorMode::Level, _) => Ink::Solid(palette.for_level(lvl)),
                _ => Ink::Solid([255, 255, 255, 255]),
            }
        };
        let ink = ink_for(level);
        // The area under the line.
        if look.fill_alpha > 0.0 && level > 0.0 {
            let (top, bottom) = if area.hanging {
                (area.y, y_of(level))
            } else {
                (y_of(level), area.y + area.h)
            };
            let fill_alpha = alpha
                * look.fill_alpha
                * if look.alpha_bars || look.lumi {
                    level
                } else {
                    1.0
                };
            fill_rect(out, clip, xf, top, xf + 1.0, bottom, &ink, fill_alpha);
        }
        // An onset's flash on this column, the band's boost read across.
        if !onset.boost.is_empty() && level > 0.0 {
            let boost_of = |i: usize| onset.boost.get(i).copied().unwrap_or(0.0);
            let boost = across(&boost_of, xf + 0.5);
            if boost > 0.0 {
                let (top, bottom) = if area.hanging {
                    (area.y, y_of(level))
                } else {
                    (y_of(level), area.y + area.h)
                };
                fill_rect(
                    out,
                    clip,
                    xf,
                    top,
                    xf + 1.0,
                    bottom,
                    &Ink::Solid(onset.color),
                    alpha * boost * 0.7,
                );
            }
        }
        // The line: from this column's height to the next one's, so a
        // steep slope stays joined, `line.width` thick.
        // A ribbon's width follows the level up to `line.width.max`.
        let lw_here = if look.line_width_max > lw {
            lw + (look.line_width_max - lw) * level
        } else {
            lw
        };
        if lw_here > 0.0 || look.line_glow > 0.0 {
            let next = across(&level_of, xf + 1.5);
            let (ya, yb) = (y_of(level), y_of(next));
            // A stroke keeps its width across the line: on a slope the
            // column's span grows by the slope's secant, so a steep run is
            // as wide as a flat one.
            let secant = (1.0 + (yb - ya) * (yb - ya)).sqrt().min(8.0);
            let line_ink = match look.color_mode {
                lead::ColorMode::Gradient => Ink::Solid(palette.at(level)),
                _ => ink_for(level),
            };
            if look.line_glow > 0.0 {
                // A soft band around the line: two passes, wide and faint, narrower and less faint.
                for (share, band_alpha) in [(1.0f32, 0.18f32), (0.5, 0.3)] {
                    let half = look.line_glow * share * secant / 2.0;
                    fill_rect(
                        out,
                        clip,
                        xf,
                        ya.min(yb) - half,
                        xf + 1.0,
                        ya.max(yb) + half,
                        &line_ink,
                        alpha * band_alpha,
                    );
                }
            }
            if lw_here > 0.0 {
                let half = lw_here * secant / 2.0;
                fill_rect(
                    out,
                    clip,
                    xf,
                    ya.min(yb) - half,
                    xf + 1.0,
                    ya.max(yb) + half,
                    &line_ink,
                    alpha,
                );
            }
        }
        // The echo: the ghost's line, thin, in the palette's top colour.
        if !ghost.is_empty() {
            let ghost_of = |i: usize| ghost.get(i).copied().unwrap_or(0.0);
            let g = across(&ghost_of, xf + 0.5);
            let next = across(&ghost_of, xf + 1.5);
            let (ya, yb) = (y_of(g), y_of(next));
            fill_rect(
                out,
                clip,
                xf,
                ya.min(yb) - 0.5,
                xf + 1.0,
                ya.max(yb) + 0.5,
                &Ink::Solid(palette.at(1.0)),
                alpha * 0.6,
            );
        }
        // The peaks as a line of their own.
        if look.peaks && look.peak_line {
            let peak = across(&peak_of, xf + 0.5);
            if peak > level + 0.002 {
                let next = across(&peak_of, xf + 1.5);
                let (ya, yb) = (y_of(peak), y_of(next));
                let fade = across(&fade_of, xf + 0.5);
                let mark = match look.color_mode {
                    lead::ColorMode::Gradient => Ink::Solid(palette.at(peak)),
                    lead::ColorMode::Level => Ink::Solid(palette.for_level(peak)),
                    lead::ColorMode::Index => ink_for(peak),
                };
                fill_rect(
                    out,
                    clip,
                    xf,
                    ya.min(yb) - 1.0,
                    xf + 1.0,
                    ya.max(yb) + 1.0,
                    &mark,
                    alpha * (1.0 - fade).clamp(0.0, 1.0),
                );
            }
        }
    }
    // The peaks as a mark per band.
    if look.peaks && !look.peak_line {
        for (i, raw) in levels.iter().enumerate() {
            let peak = peaks.get(i).copied().unwrap_or_default();
            let level = raw.clamp(0.0, 1.0);
            if peak.level <= level + 0.002 {
                continue;
            }
            let slot = band_at(i);
            let xa = area.x + slot as f32 * pitch;
            let py = y_of(peak.level.clamp(0.0, 1.0));
            let mark = match look.color_mode {
                lead::ColorMode::Gradient => Ink::Solid(palette.at(peak.level.clamp(0.0, 1.0))),
                lead::ColorMode::Index => Ink::Solid(palette.at(if n > 1 {
                    i as f32 / (n - 1) as f32
                } else {
                    0.0
                })),
                lead::ColorMode::Level => Ink::Solid(palette.for_level(peak.level)),
            };
            fill_rect(
                out,
                clip,
                xa,
                py - 1.0,
                xa + pitch,
                py + 1.0,
                &mark,
                alpha * (1.0 - peak.fade).clamp(0.0, 1.0),
            );
        }
    }
}

/// A disc of `radius` about a centre, row by row with fractional ends.
fn fill_disc(
    out: &mut Frame,
    clip: (i32, i32, i32, i32),
    cx: f32,
    cy: f32,
    radius: f32,
    ink: &Ink,
    alpha: f32,
) {
    let r = radius.max(0.5);
    let y0 = (cy - r).floor() as i32;
    let y1 = (cy + r).ceil() as i32;
    for y in y0..y1 {
        let dy = y as f32 + 0.5 - cy;
        let half = (r * r - dy * dy).max(0.0).sqrt();
        if half > 0.0 {
            fill_span(out, clip, y, cx - half, cx + half, ink, alpha);
        }
    }
}

/// One channel as dots: a disc per band at its level, `dot.size` across,
/// the peaks as discs half that size, an onset's flash as a disc of the
/// onset colour over the dot; with `dot.hold` the disc sits at the band's
/// held peak instead and falls with it, and no peak mark is drawn.
#[allow(clippy::too_many_arguments)]
fn draw_dots(
    out: &mut Frame,
    look: &lead::Look,
    palette: &lead::Palette,
    area: &Area,
    levels: &[f32],
    peaks: &[Peak],
    alpha: f32,
    onset: &FrameFx,
) {
    let n = levels.len();
    if n == 0 || area.w < 1.0 || area.h < 1.0 {
        return;
    }
    let clip = (
        area.x.floor() as i32,
        area.y.floor() as i32,
        (area.x + area.w).ceil() as i32,
        (area.y + area.h).ceil() as i32,
    );
    let pitch = area.w / n as f32;
    let size = if look.dot_size > 0.0 {
        look.dot_size
    } else {
        (pitch * 0.7).max(2.0)
    };
    let radius = size / 2.0;
    let y_of = |level: f32| -> f32 {
        let level = level.clamp(0.0, 1.0);
        if area.hanging {
            area.y + level * area.h
        } else {
            area.y + area.h - level * area.h
        }
    };
    let ink_for = |i: usize, level: f32| -> Ink<'static> {
        Ink::Solid(match look.color_mode {
            lead::ColorMode::Gradient => palette.at(level.clamp(0.0, 1.0)),
            lead::ColorMode::Index => palette.at(if n > 1 {
                i as f32 / (n - 1) as f32
            } else {
                0.0
            }),
            lead::ColorMode::Level => palette.for_level(level),
        })
    };
    for (i, raw) in levels.iter().enumerate() {
        let level = raw.clamp(0.0, 1.0);
        let held = peaks.get(i).map_or(0.0, |p| p.level.clamp(0.0, 1.0));
        let shown = if look.dot_hold {
            level.max(held)
        } else {
            level
        };
        let slot = if area.reversed { n - 1 - i } else { i };
        let cx = area.x + (slot as f32 + 0.5) * pitch;
        let dot_alpha = alpha * look.fill_alpha * if look.alpha_bars { shown } else { 1.0 };
        fill_disc(
            out,
            clip,
            cx,
            y_of(shown),
            radius,
            &ink_for(i, shown),
            dot_alpha,
        );
        if let Some(&boost) = onset.boost.get(i) {
            if boost > 0.0 {
                fill_disc(
                    out,
                    clip,
                    cx,
                    y_of(shown),
                    radius,
                    &Ink::Solid(onset.color),
                    alpha * boost * 0.7,
                );
            }
        }
        if look.peaks && !look.dot_hold {
            let peak = peaks.get(i).copied().unwrap_or_default();
            if peak.level > level + 0.002 {
                fill_disc(
                    out,
                    clip,
                    cx,
                    y_of(peak.level),
                    (radius * 0.5).max(1.0),
                    &ink_for(i, peak.level),
                    alpha * (1.0 - peak.fade).clamp(0.0, 1.0),
                );
            }
        }
    }
}

/// One channel's frame into the waterfall: the rows of its area move
/// `waterfall.speed` rows away from the base (the far edge with
/// `waterfall.reverse`), and the levels are written as that many new rows
/// at the base, a colour per column from its band: the palette by level in
/// the gradient and level modes, the band's own colour at the level's
/// opacity by index. An onset's flash tints the new rows.
fn draw_waterfall(
    history: &mut Frame,
    look: &lead::Look,
    palette: &lead::Palette,
    area: &Area,
    levels: &[f32],
    alpha: f32,
    onset: &FrameFx,
) {
    let n = levels.len();
    let width = history.width as usize;
    if n == 0 || area.w < 1.0 || area.h < 1.0 || width == 0 {
        return;
    }
    let x0 = (area.x.round().max(0.0) as usize).min(width);
    let x1 = ((area.x + area.w).round().max(0.0) as usize).min(width);
    let y0 = (area.y.round().max(0.0) as usize).min(history.height as usize);
    let y1 = ((area.y + area.h).round().max(0.0) as usize).min(history.height as usize);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let rows = y1 - y0;
    let speed = (look.waterfall_speed.max(1) as usize).min(rows);
    // New rows at the base: the bottom of a rising area, the top of a
    // hanging one; reversed, the other way.
    let new_at_bottom = !area.hanging != look.waterfall_reverse;
    let row_bytes = (x1 - x0) * 4;
    if new_at_bottom {
        // Everything moves up by `speed` rows.
        for y in y0..y1 - speed {
            let (dst, src) = ((y * width + x0) * 4, ((y + speed) * width + x0) * 4);
            history.rgba.copy_within(src..src + row_bytes, dst);
        }
    } else {
        for y in (y0 + speed..y1).rev() {
            let (dst, src) = ((y * width + x0) * 4, ((y - speed) * width + x0) * 4);
            history.rgba.copy_within(src..src + row_bytes, dst);
        }
    }
    // The new rows: a colour per column from its band.
    let pitch = area.w / n as f32;
    let mut row: Vec<[u8; 4]> = Vec::with_capacity(x1 - x0);
    for x in x0..x1 {
        let mut slot = (((x as f32 + 0.5 - area.x) / pitch).floor().max(0.0) as usize).min(n - 1);
        if area.reversed {
            slot = n - 1 - slot;
        }
        let level = levels[slot].clamp(0.0, 1.0);
        let (mut c, a) = match look.color_mode {
            lead::ColorMode::Gradient => (palette.at(level), 1.0),
            lead::ColorMode::Level => (palette.for_level(level), 1.0),
            lead::ColorMode::Index => (
                palette.at(if n > 1 {
                    slot as f32 / (n - 1) as f32
                } else {
                    0.0
                }),
                level,
            ),
        };
        if let Some(&boost) = onset.boost.get(slot) {
            if boost > 0.0 {
                for (channel, target) in c.iter_mut().zip(onset.color.iter()).take(3) {
                    *channel = (*channel as f32 + (*target as f32 - *channel as f32) * boost * 0.7)
                        .round() as u8;
                }
            }
        }
        c[3] = (c[3] as f32 * a * alpha * look.fill_alpha)
            .round()
            .clamp(0.0, 255.0) as u8;
        row.push(c);
    }
    let new_rows = if new_at_bottom {
        y1 - speed..y1
    } else {
        y0..y0 + speed
    };
    for y in new_rows {
        let at = (y * width + x0) * 4;
        for (k, c) in row.iter().enumerate() {
            history.rgba[at + k * 4..at + k * 4 + 4].copy_from_slice(c);
        }
    }
}

/// A channel's area into `areas`, whole or as two halves mirrored: the
/// bands one way and their mirror image, meeting in the middle.
#[allow(clippy::too_many_arguments)]
fn push_areas<'p>(
    areas: &mut Vec<(usize, Area, &'p lead::Palette, f32)>,
    mirror: i8,
    ch: usize,
    x: f32,
    y: f32,
    aw: f32,
    ah: f32,
    hanging: bool,
    palette: &'p lead::Palette,
    alpha: f32,
) {
    if mirror == 0 {
        areas.push((
            ch,
            Area {
                x,
                y,
                w: aw,
                h: ah,
                hanging,
                reversed: false,
            },
            palette,
            alpha,
        ));
        return;
    }
    let first = mirror == 1;
    areas.push((
        ch,
        Area {
            x,
            y,
            w: aw / 2.0,
            h: ah,
            hanging,
            reversed: !first,
        },
        palette,
        alpha,
    ));
    areas.push((
        ch,
        Area {
            x: x + aw / 2.0,
            y,
            w: aw / 2.0,
            h: ah,
            hanging,
            reversed: first,
        },
        palette,
        alpha,
    ));
}

/// The analyser's box drawn into `out`: the background at its alpha, the
/// channels laid out as the look says, mirrored and reflected as asked.
/// Straight into the frame's pixels: a bar is a run of rows filled from a
/// palette lookup, so a frame costs the pixels it covers and no more.
fn render_analyser(
    a: &plot::Analyser,
    smoothed: &[Vec<f32>; 2],
    peaks: &[Vec<Peak>; 2],
    out: &mut Frame,
    fonts: Option<&Fonts>,
    labels: &mut LabelCache,
    onset: &FrameFx,
    history: &mut Frame,
    previous: Option<(&Frame, f32)>,
) {
    let (w, h) = (a.w.max(1), a.h.max(1));
    let look = &a.look;
    // The waterfall keeps its rows between frames, in a frame of the box's size.
    if look.style == lead::LookStyle::Waterfall && (history.width != w || history.height != h) {
        history.width = w;
        history.height = h;
        history.rgba.clear();
        history.rgba.resize((w * h * 4) as usize, 0);
    }
    let bg = (look.bgr_alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    begin_frame(out, w, h, bg, previous);
    out.blend = if look.blend_add {
        Blend::Add
    } else {
        Blend::Normal
    };
    let (fw, fh) = (w as f32, h as f32);
    // The frequency scale takes a strip at the bottom of the box.
    let strip = if look.scale_x {
        (look.scale_size + 6) as f32
    } else {
        0.0
    };
    let body_h = (fh - strip).max(1.0);
    let bars_h = if look.reflex > 0.0 {
        (body_h * (1.0 - look.reflex)).max(1.0)
    } else {
        body_h
    };
    let stereo = a.stereo && !a.levels[1].is_empty();
    let left = look.palette_left.as_ref().unwrap_or(&look.palette);
    let right = look.palette_right.as_ref().unwrap_or(&look.palette);
    let mut areas: Vec<(usize, Area, &lead::Palette, f32)> = Vec::new();
    let half_w = fw / 2.0;
    let mirror = look.mirror;
    match (look.layout, stereo) {
        (lead::Layout::DualVertical, true) => {
            push_areas(
                &mut areas,
                mirror,
                0,
                0.0,
                0.0,
                fw,
                bars_h / 2.0,
                false,
                left,
                1.0,
            );
            push_areas(
                &mut areas,
                mirror,
                1,
                0.0,
                bars_h / 2.0,
                fw,
                bars_h / 2.0,
                true,
                right,
                1.0,
            );
        }
        (lead::Layout::DualHorizontal, true) => {
            push_areas(
                &mut areas, mirror, 0, 0.0, 0.0, half_w, bars_h, false, left, 1.0,
            );
            push_areas(
                &mut areas, mirror, 1, half_w, 0.0, half_w, bars_h, false, right, 1.0,
            );
        }
        (lead::Layout::DualCombined, true) => {
            push_areas(
                &mut areas, mirror, 0, 0.0, 0.0, fw, bars_h, false, left, 1.0,
            );
            push_areas(
                &mut areas, mirror, 1, 0.0, 0.0, fw, bars_h, false, right, 0.5,
            );
        }
        _ => push_areas(
            &mut areas,
            mirror,
            0,
            0.0,
            0.0,
            fw,
            bars_h,
            false,
            &look.palette,
            1.0,
        ),
    }
    // With a split palette in a vertical pair, the palette runs over both
    // halves: the top channel shows its upper colours, the hanging one its
    // lower.
    let split = look.palette_split && matches!(look.layout, lead::Layout::DualVertical) && stereo;
    for (ch, area, palette, alpha) in &areas {
        let span = if split {
            Area {
                x: area.x,
                y: 0.0,
                w: area.w,
                h: bars_h,
                hanging: false,
                reversed: area.reversed,
            }
        } else {
            Area {
                x: area.x,
                y: area.y,
                w: area.w,
                h: area.h,
                hanging: area.hanging,
                reversed: area.reversed,
            }
        };
        match look.style {
            lead::LookStyle::Graph => draw_graph(
                out,
                look,
                palette,
                area,
                &span,
                &smoothed[*ch],
                &peaks[*ch],
                *alpha,
                onset,
                &onset.ghost[*ch],
            ),
            lead::LookStyle::Bars => draw_channel(
                out,
                look,
                palette,
                area,
                &span,
                &smoothed[*ch],
                &peaks[*ch],
                *alpha,
                onset,
                &onset.ghost[*ch],
            ),
            lead::LookStyle::Dots => draw_dots(
                out,
                look,
                palette,
                area,
                &smoothed[*ch],
                &peaks[*ch],
                *alpha,
                onset,
            ),
            lead::LookStyle::Waterfall => {
                // Two channels over each other would move the same rows
                // twice: the first channel alone draws there.
                if *ch == 0 || !matches!(look.layout, lead::Layout::DualCombined) {
                    draw_waterfall(history, look, palette, area, &smoothed[*ch], *alpha, onset);
                }
            }
        }
    }
    if look.style == lead::LookStyle::Waterfall {
        // The rows so far over the background: copied where they are solid,
        // blended where a level left them thin.
        for (dst, src) in out
            .rgba
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(history.rgba.as_chunks::<4>().0.iter())
        {
            match src[3] {
                0 => {}
                255 => *dst = *src,
                a => blend_px(dst, *src, a as u32),
            }
        }
    }
    // The onset rings: a line across each area on its way from the base to the top.
    if !onset.rings.is_empty() {
        let clip = (0, 0, w as i32, h as i32);
        let ink = Ink::Solid(onset.color);
        for (_, area, _, _) in &areas {
            for &(t, ring_alpha) in &onset.rings {
                let y = if area.hanging {
                    area.y + t * area.h
                } else {
                    area.y + area.h - t * area.h
                };
                fill_rect(
                    out,
                    clip,
                    area.x,
                    y - 1.0,
                    area.x + area.w,
                    y + 1.0,
                    &ink,
                    ring_alpha,
                );
            }
        }
    }
    if look.reflex > 0.0 {
        // The bars upside down under them, dimmed and faded: each row of
        // the reflection reads one row of the bars, squeezed to fit or not.
        let reflex_h = (body_h - bars_h).max(0.0) as usize;
        let bars_rows = bars_h.round().max(1.0) as usize;
        let alpha_255 = (look.reflex_alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
        let bright_255 = (look.reflex_bright.clamp(0.0, 4.0) * 255.0).round() as u32;
        let width = w as usize;
        for ry in 0..reflex_h {
            let source = if look.reflex_fit {
                ((ry as f32 + 0.5) / reflex_h.max(1) as f32 * bars_rows as f32) as usize
            } else {
                ry
            };
            if source >= bars_rows {
                break;
            }
            let src_row = (bars_rows - 1 - source) * width * 4;
            let dst_row = (bars_rows + ry) * width * 4;
            if dst_row + width * 4 > out.rgba.len() {
                break;
            }
            for x in 0..width {
                let sa = out.rgba[src_row + x * 4 + 3] as u32;
                // The background is under every pixel already: only what the
                // bars added above the background goes into the reflection.
                let added = sa.saturating_sub(bg as u32);
                let a = (added * alpha_255 + 127) / 255;
                if a == 0 {
                    continue;
                }
                let c = [
                    ((out.rgba[src_row + x * 4] as u32 * bright_255) / 255).min(255) as u8,
                    ((out.rgba[src_row + x * 4 + 1] as u32 * bright_255) / 255).min(255) as u8,
                    ((out.rgba[src_row + x * 4 + 2] as u32 * bright_255) / 255).min(255) as u8,
                    255,
                ];
                let at = dst_row + x * 4;
                put_px(out, at, c, a.min(255));
            }
        }
    }
    if look.scale_x || look.scale_y {
        draw_scales(out, a, &areas, body_h, fonts, labels);
    }
}

/// The frequencies labelled along the bottom, and their texts.
const SCALE_HZ: [(f32, &str); 10] = [
    (20.0, "20"),
    (50.0, "50"),
    (100.0, "100"),
    (200.0, "200"),
    (500.0, "500"),
    (1000.0, "1k"),
    (2000.0, "2k"),
    (5000.0, "5k"),
    (10_000.0, "10k"),
    (20_000.0, "20k"),
];

/// Where a frequency falls across the drawn bands, 0 at the first band's
/// low edge and 1 at the last band's high edge, straight inside each band.
fn across_bands(edges: &[(f32, f32)], hz: f32) -> Option<f32> {
    let n = edges.len();
    let (lo, hi) = (edges.first()?.0, edges.last()?.1);
    if hz < lo || hz > hi {
        return None;
    }
    let i = edges
        .iter()
        .position(|&(a, b)| hz >= a && hz < b)
        .unwrap_or(n - 1);
    let (a, b) = edges[i];
    let inside = if b > a { (hz - a) / (b - a) } else { 0.0 };
    Some((i as f32 + inside.clamp(0.0, 1.0)) / n as f32)
}

/// The scales of a box: frequency labels in a strip under the bars, one set
/// per distinct run of bands (each half of a mirror, each channel's side),
/// with a tick above each; and decibel labels at the left of each bars area
/// with a faint line across at every step. Set once per look and box, then
/// blitted every frame.
fn draw_scales(
    out: &mut Frame,
    a: &plot::Analyser,
    areas: &[(usize, Area, &lead::Palette, f32)],
    body_h: f32,
    fonts: Option<&Fonts>,
    cache: &mut LabelCache,
) {
    let look = &a.look;
    let n = a.edges.len();
    if n == 0 {
        return;
    }
    let key = LabelKey {
        w: out.width,
        h: out.height,
        n,
        lo: a.edges[0].0.to_bits(),
        hi: a.edges[n - 1].1.to_bits(),
        level: (look.level_range.0.to_bits(), look.level_range.1.to_bits()),
        flags: u8::from(look.scale_x)
            | u8::from(look.scale_y) << 1
            | u8::from(look.note_labels) << 2
            | u8::from(look.level_linear) << 3
            | u8::from(look.lumi) << 4,
        size: look.scale_size,
        color: look.scale_color,
        layout: look.layout as u8,
        mirror: look.mirror,
        reflex: look.reflex.to_bits(),
    };
    if cache.key != Some(key) {
        cache.key = Some(key);
        cache.lines.clear();
        cache.marks.clear();
        let color = look.scale_color;
        let size = look.scale_size;
        let set = |text: &str| -> Frame {
            render_text(fonts, TextStyle::Regular, size, color, text, 0)
                .unwrap_or_else(|| bitmap_line(text))
        };
        if look.scale_x {
            let notes: Vec<(f32, String)> = (1..=8)
                .map(|k| (32.703 * 2f32.powi(k - 1), format!("C{k}")))
                .collect();
            let hz: Vec<(f32, String)> = SCALE_HZ
                .iter()
                .map(|(f, t)| (*f, (*t).to_string()))
                .collect();
            let labels = if look.note_labels { &notes } else { &hz };
            let mut runs: Vec<(f32, f32, bool)> = Vec::new();
            for (_, area, _, _) in areas {
                let run = (area.x, area.w, area.reversed);
                if !runs
                    .iter()
                    .any(|r| r.0 == run.0 && r.1 == run.1 && r.2 == run.2)
                {
                    runs.push(run);
                }
            }
            for (x, w, reversed) in runs {
                for (f, text) in labels {
                    let Some(t) = across_bands(&a.edges, *f) else {
                        continue;
                    };
                    let px = if reversed { x + w - t * w } else { x + t * w };
                    let line = set(text);
                    let left = px - line.width as f32 / 2.0;
                    if left < 0.0 || left + line.width as f32 > out.width as f32 {
                        continue;
                    }
                    cache
                        .lines
                        .push((left.round() as i32, (body_h + 3.0) as i32, line));
                    cache
                        .marks
                        .push((px - 0.5, body_h, px + 0.5, body_h + 3.0, 1.0));
                }
            }
        }
        if look.scale_y && !look.level_linear && !look.lumi {
            let (lo, hi) = look.level_range;
            let step = if hi - lo >= 90.0 { 20.0 } else { 10.0 };
            let mut seen: Vec<(u32, u32, u32, u32, bool)> = Vec::new();
            for (_, area, _, _) in areas {
                let sig = (
                    area.x.to_bits(),
                    area.y.to_bits(),
                    area.w.to_bits(),
                    area.h.to_bits(),
                    area.hanging,
                );
                if seen.contains(&sig) {
                    continue;
                }
                seen.push(sig);
                let mut db = (lo / step).ceil() * step;
                while db <= hi + 0.01 {
                    let level = ((db - lo) / (hi - lo).max(1.0)).clamp(0.0, 1.0);
                    let y = if area.hanging {
                        area.y + level * area.h
                    } else {
                        area.y + area.h - level * area.h
                    };
                    let line = set(&format!("{}", db.round() as i32));
                    let top = (y - line.height as f32 / 2.0)
                        .clamp(area.y, (area.y + area.h - line.height as f32).max(area.y));
                    let left = area.x + 30.0 - line.width as f32;
                    cache
                        .lines
                        .push((left.round().max(0.0) as i32, top.round() as i32, line));
                    cache
                        .marks
                        .push((area.x + 33.0, y - 0.5, area.x + area.w, y + 0.5, 0.2));
                    db += step;
                }
            }
        }
    }
    let clip = (0, 0, out.width as i32, out.height as i32);
    let ink = Ink::Solid([
        look.scale_color[0],
        look.scale_color[1],
        look.scale_color[2],
        255,
    ]);
    for &(x0, y0, x1, y1, alpha) in &cache.marks {
        fill_rect(out, clip, x0, y0, x1, y1, &ink, alpha);
    }
    for (x, y, line) in &cache.lines {
        blit_op(line, (*x, *y)).paint(&mut Band::whole(out));
    }
}

/// The topping of each spectrum bar: where it sits, or `None` before the
/// first frame placed it.
#[derive(Default)]
pub struct SpectrumMotion {
    toppings: Vec<Option<i32>>,
}

/// Pictures of a spectrum, built once per meter from its spec.
pub struct SpectrumAssets {
    pub background: Option<Frame>,
    pub bar: Option<Frame>,
    pub reflection: Option<Frame>,
    pub foreground: Option<Frame>,
}

impl SpectrumAssets {
    pub fn bytes(&self) -> usize {
        bytes_of([
            &self.background,
            &self.bar,
            &self.reflection,
            &self.foreground,
        ])
    }

    pub fn load(spec: &SpectrumSpec) -> Self {
        let bar_box = (spec.bar_w, spec.bar_h);
        Self {
            background: spec
                .background
                .as_ref()
                .and_then(|f| fill_frame(f, (spec.w, spec.h), false)),
            bar: spec.bar.as_ref().and_then(|f| fill_frame(f, bar_box, true)),
            reflection: spec
                .reflection
                .as_ref()
                .and_then(|f| fill_frame(f, bar_box, true)),
            foreground: if spec.foreground.is_empty() {
                None
            } else {
                read_png(Path::new(&spec.foreground))
            },
        }
    }
}

/// A layer from its fill. A plain picture keeps its own size unless `stretch`;
/// an extended one is always stretched to the box.
fn fill_frame(fill: &Fill, (w, h): (u32, u32), stretch: bool) -> Option<Frame> {
    match fill {
        Fill::Color(color) => Some(solid_frame(w, h, *color)),
        Fill::Gradient(colors) => Some(gradient_frame(w, h, colors)),
        Fill::Image(path) => {
            let picture = read_png(Path::new(path))?;
            Some(if stretch {
                fit_art(&picture, w, h)
            } else {
                picture
            })
        }
        Fill::ImageExtended(path) => Some(fit_art(&read_png(Path::new(path))?, w, h)),
    }
}

fn solid_frame(w: u32, h: u32, color: [u8; 4]) -> Frame {
    let (w, h) = (w.max(1), h.max(1));
    Frame {
        blend: Blend::Normal,
        width: w,
        height: h,
        rgba: color.repeat((w * h) as usize),
    }
}

/// A vertical gradient: the first colour at the bottom, the last at the top,
/// blended linearly between, as the spectrum engine lays out its colour rows
/// before stretching them.
pub fn gradient_frame(w: u32, h: u32, colors: &[[u8; 4]]) -> Frame {
    let (w, h) = (w.max(1), h.max(1));
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let last = colors.len().saturating_sub(1);
    for y in 0..h {
        let down = if h > 1 {
            y as f32 / (h - 1) as f32
        } else {
            0.0
        };
        let pos = (1.0 - down) * last as f32;
        let i = (pos.floor() as usize).min(last);
        let f = pos - i as f32;
        let a = colors[i];
        let b = colors[(i + 1).min(last)];
        let mut px = [0u8; 4];
        for k in 0..4 {
            px[k] = (a[k] as f32 * (1.0 - f) + b[k] as f32 * f).round() as u8;
        }
        let row = (y * w * 4) as usize;
        for x in 0..w as usize {
            rgba[row + x * 4..row + x * 4 + 4].copy_from_slice(&px);
        }
    }
    Frame {
        blend: Blend::Normal,
        width: w,
        height: h,
        rgba,
    }
}

/// Where each bar of the spectrum stands this frame: its left edge, its
/// height, and a falling topping's top and source row when one shows.
pub struct SpectrumPlan {
    bars: Vec<(i32, u32, Option<(i32, u32)>)>,
}

impl SpectrumMotion {
    /// Advance the toppings and say where the bars stand. A topping falls
    /// `step` pixels a frame once its bar has dropped away from it.
    pub fn advance(&mut self, spec: &SpectrumSpec, heights: &[u32]) -> SpectrumPlan {
        let baseline = spec.y + spec.origin_y;
        if self.toppings.len() != heights.len() {
            self.toppings = vec![None; heights.len()];
        }
        let bars = heights
            .iter()
            .enumerate()
            .map(|(r, &height)| {
                let height = height.min(spec.bar_h);
                let bx = spec.x + spec.origin_x + r as i32 * (spec.bar_w + spec.gap) as i32;
                let mut topping = None;
                if let Some((topping_h, topping_step)) = spec.topping {
                    let top = (baseline - height as i32).min(baseline);
                    match self.toppings[r] {
                        None => self.toppings[r] = Some(top),
                        Some(sitting) => {
                            if top > sitting + topping_step as i32 + topping_h as i32 {
                                let fallen = sitting + topping_step as i32;
                                self.toppings[r] = Some(fallen);
                                let src_y = spec.bar_h as i32 - (baseline - fallen) + 1;
                                if src_y >= 0 {
                                    topping = Some((fallen, src_y as u32));
                                }
                            } else {
                                self.toppings[r] =
                                    Some(top - topping_h as i32 - topping_step as i32);
                            }
                        }
                    }
                }
                (bx, height, topping)
            })
            .collect();
        SpectrumPlan { bars }
    }
}

/// The spectrum as its engine draws it, clipped to its box: background
/// centred in the box, each bar's bottom `height` rows rising from the
/// origin, the reflection's top rows hanging below it, a falling topping,
/// and the foreground over everything.
fn plan_spectrum<'a>(
    spec: &SpectrumSpec,
    assets: &'a SpectrumAssets,
    plan: &SpectrumPlan,
    ops: &mut Vec<Op<'a>>,
) {
    let clip = Some((spec.x, spec.y, spec.w as i32, spec.h as i32));
    let centred = |picture: &Frame| {
        (
            spec.x + (spec.w as i32 - picture.width as i32) / 2,
            spec.y + (spec.h as i32 - picture.height as i32) / 2,
        )
    };
    if let Some(background) = &assets.background {
        ops.push(Op::Blit {
            src: background,
            at: centred(background),
            part: whole(background),
            clip,
            alpha: 255,
        });
    }
    let baseline = spec.y + spec.origin_y;
    for &(bx, height, topping) in &plan.bars {
        if height > 0 {
            if let Some(bar) = &assets.bar {
                ops.push(Op::Blit {
                    src: bar,
                    at: (bx, baseline - height as i32),
                    part: (0, spec.bar_h - height, spec.bar_w, height),
                    clip,
                    alpha: 255,
                });
            }
            if let Some(reflection) = &assets.reflection {
                ops.push(Op::Blit {
                    src: reflection,
                    at: (bx, baseline + spec.reflection_gap),
                    part: (0, 0, spec.bar_w, height),
                    clip,
                    alpha: 255,
                });
            }
        }
        if let (Some((topping_h, _)), Some(bar), Some((top, src_y))) =
            (spec.topping, &assets.bar, topping)
        {
            ops.push(Op::Blit {
                src: bar,
                at: (bx, top),
                part: (0, src_y, spec.bar_w, topping_h),
                clip,
                alpha: 255,
            });
        }
    }
    if let Some(foreground) = &assets.foreground {
        ops.push(Op::Blit {
            src: foreground,
            at: centred(foreground),
            part: whole(foreground),
            clip,
            alpha: 255,
        });
    }
}

/// The spectrum straight onto a band.
pub fn draw_spectrum(
    band: &mut Band,
    spec: &SpectrumSpec,
    heights: &[u32],
    assets: &SpectrumAssets,
    motion: &mut SpectrumMotion,
) {
    let plan = motion.advance(spec, heights);
    let mut ops = Vec::new();
    plan_spectrum(spec, assets, &plan, &mut ops);
    paint_onto(band, &ops);
}

/// A folder layer's picture, scaled for its box, and where it is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderPicture {
    pub frame: Frame,
    pub at: (u32, u32),
}

impl FolderPicture {
    /// Decode a picture and place it in the layer's box: stretched to fill it,
    /// or kept in proportion and centred.
    pub fn load(path: &Path, layer: &FolderLayerSpec) -> Option<Self> {
        let picture = read_png(path)?;
        if picture.width == 0 || picture.height == 0 {
            return None;
        }
        let (bw, bh) = (layer.w.max(1), layer.h.max(1));
        Some(match layer.scale {
            Scale::Stretch => Self {
                frame: fit_art(&picture, bw, bh),
                at: (layer.x, layer.y),
            },
            Scale::Fit => {
                let ratio =
                    (bw as f32 / picture.width as f32).min(bh as f32 / picture.height as f32);
                let nw = ((picture.width as f32 * ratio) as u32).max(1);
                let nh = ((picture.height as f32 * ratio) as u32).max(1);
                Self {
                    frame: fit_art(&picture, nw, nh),
                    at: (layer.x + (bw - nw) / 2, layer.y + (bh - nh) / 2),
                }
            }
            Scale::Cover => {
                // Scaled to cover the box, then the middle of it cut out.
                let ratio =
                    (bw as f32 / picture.width as f32).max(bh as f32 / picture.height as f32);
                let nw = ((picture.width as f32 * ratio).ceil() as u32).max(bw);
                let nh = ((picture.height as f32 * ratio).ceil() as u32).max(bh);
                let scaled = fit_art(&picture, nw, nh);
                let (ox, oy) = ((nw - bw) / 2, (nh - bh) / 2);
                let mut rgba = Vec::with_capacity((bw * bh * 4) as usize);
                for y in 0..bh {
                    let from = (((oy + y) * nw + ox) * 4) as usize;
                    rgba.extend_from_slice(&scaled.rgba[from..from + (bw * 4) as usize]);
                }
                Self {
                    frame: Frame {
                        blend: Blend::Normal,
                        width: bw,
                        height: bh,
                        rgba,
                    },
                    at: (layer.x, layer.y),
                }
            }
        })
    }
}

/// The folder layers of one z-order, each with its border when it has a picture.
fn plan_folder_layers<'a>(
    scene: &Scene,
    pictures: &'a [Option<FolderPicture>],
    zorder: ZOrder,
    ops: &mut Vec<Op<'a>>,
) {
    for (layer, picture) in scene.folder_layers.iter().zip(pictures.iter()) {
        if layer.spec.zorder != zorder {
            continue;
        }
        let Some(picture) = picture else {
            continue;
        };
        ops.push(blit_op(
            &picture.frame,
            (picture.at.0 as i32, picture.at.1 as i32),
        ));
        if layer.spec.border > 0 {
            let c = layer.spec.border_color;
            ops.push(Op::Border {
                rect: (layer.spec.x, layer.spec.y, layer.spec.w, layer.spec.h),
                thickness: layer.spec.border,
                color: [c[0], c[1], c[2], 255],
            });
        }
    }
}

/// A picture at its place with its alpha scaled by `alpha` (255 is as is).
fn alpha_op(picture: &FolderPicture, alpha: u8) -> Op<'_> {
    Op::Blit {
        src: &picture.frame,
        at: (picture.at.0 as i32, picture.at.1 as i32),
        part: whole(&picture.frame),
        clip: None,
        alpha,
    }
}

/// The fanart slot of one z-order. A `merge` crossfades the old picture into
/// the new over the transition; a `fade` takes the old one out in the first
/// half and the new one in over the second, or the new one in alone when
/// there is no old picture; `none` shows the picture as it is.
fn plan_fanart<'a>(
    fanart: Option<&Fanart>,
    pictures: (Option<&'a FolderPicture>, Option<&'a FolderPicture>),
    zorder: ZOrder,
    ops: &mut Vec<Op<'a>>,
) {
    let Some(fanart) = fanart else {
        return;
    };
    if fanart.spec.zorder != zorder {
        return;
    }
    let (current, previous) = pictures;
    let running = fanart.transition_ms > 0
        && fanart.elapsed_ms < fanart.transition_ms
        && fanart.transition != "none";
    if !running {
        if let Some(picture) = current {
            ops.push(alpha_op(picture, 255));
        }
        return;
    }
    let p = fanart.elapsed_ms as f32 / fanart.transition_ms as f32;
    let level = |f: f32| (255.0 * f.clamp(0.0, 1.0)) as u8;
    match fanart.transition.as_str() {
        "merge" => {
            if let Some(old) = previous {
                ops.push(alpha_op(old, level(1.0 - p)));
            }
            if let Some(new) = current {
                ops.push(alpha_op(new, level(p)));
            }
        }
        _ => match (previous, current) {
            (Some(old), _) if p < 0.5 => ops.push(alpha_op(old, level(1.0 - 2.0 * p))),
            (Some(_), Some(new)) => ops.push(alpha_op(new, level(2.0 * p - 1.0))),
            (None, Some(new)) => ops.push(alpha_op(new, level(p))),
            _ => {}
        },
    }
}

/// A blurred copy of a picture, as a Gaussian blur of the given radius
/// would leave it: three box blurs of that radius, close enough for a glow.
pub fn blur(src: &Frame, radius: u32) -> Frame {
    if radius == 0 {
        return src.clone();
    }
    let (w, h) = (src.width as usize, src.height as usize);
    let r = radius as usize;
    let mut a: Vec<f32> = src.rgba.iter().map(|&v| v as f32).collect();
    let mut b = vec![0f32; a.len()];
    for _ in 0..3 {
        // Horizontal pass.
        for y in 0..h {
            for c in 0..4 {
                let mut sum = 0.0;
                let row = y * w;
                for x in 0..w.min(r + 1) {
                    sum += a[(row + x) * 4 + c];
                }
                for x in 0..w {
                    let count = (x.min(r) + (w - 1 - x).min(r) + 1) as f32;
                    b[(row + x) * 4 + c] = sum / count;
                    if x + r + 1 < w {
                        sum += a[(row + x + r + 1) * 4 + c];
                    }
                    if x >= r {
                        sum -= a[(row + x - r) * 4 + c];
                    }
                }
            }
        }
        // Vertical pass.
        for x in 0..w {
            for c in 0..4 {
                let mut sum = 0.0;
                for y in 0..h.min(r + 1) {
                    sum += b[(y * w + x) * 4 + c];
                }
                for y in 0..h {
                    let count = (y.min(r) + (h - 1 - y).min(r) + 1) as f32;
                    a[(y * w + x) * 4 + c] = sum / count;
                    if y + r + 1 < h {
                        sum += b[((y + r + 1) * w + x) * 4 + c];
                    }
                    if y >= r {
                        sum -= b[((y - r) * w + x) * 4 + c];
                    }
                }
            }
        }
    }
    Frame {
        blend: Blend::Normal,
        width: src.width,
        height: src.height,
        rgba: a
            .iter()
            .map(|v| v.round().clamp(0.0, 255.0) as u8)
            .collect(),
    }
}

fn empty_frame(w: u32, h: u32) -> Frame {
    Frame {
        blend: Blend::Normal,
        width: w.max(1),
        height: h.max(1),
        rgba: vec![0u8; (w.max(1) * h.max(1) * 4) as usize],
    }
}

/// Paint a filled ellipse or rectangle of the box into a frame.
fn paint_shape(frame: &mut Frame, x: u32, y: u32, w: u32, h: u32, circle: bool, color: [u8; 4]) {
    for py in y..(y + h).min(frame.height) {
        for px in x..(x + w).min(frame.width) {
            let inside = if circle {
                let nx = (px as f32 + 0.5 - x as f32) / (w as f32 / 2.0) - 1.0;
                let ny = (py as f32 + 0.5 - y as f32) / (h as f32 / 2.0) - 1.0;
                nx * nx + ny * ny <= 1.0
            } else {
                true
            };
            if inside {
                let i = ((py * frame.width + px) * 4) as usize;
                frame.rgba[i..i + 4].copy_from_slice(&color);
            }
        }
    }
}

/// One state of an LED indicator: the glow behind, the shape on top, in a
/// canvas padded by twice the glow radius as the player pads it.
pub fn led_state_frame(
    w: u32,
    h: u32,
    circle: bool,
    color: [u8; 3],
    glow: u32,
    intensity: f32,
    glow_color: [u8; 3],
) -> Frame {
    let pad = if glow > 0 { glow * 2 } else { 0 };
    let mut frame = empty_frame(w + pad * 2, h + pad * 2);
    if glow > 0 && intensity > 0.0 {
        let mut halo = empty_frame(w + pad * 2, h + pad * 2);
        let alpha = (255.0 * intensity.clamp(0.0, 1.0)) as u8;
        paint_shape(
            &mut halo,
            pad,
            pad,
            w,
            h,
            circle,
            [glow_color[0], glow_color[1], glow_color[2], alpha],
        );
        frame = blur(&halo, glow);
    }
    if circle {
        let r = (w.min(h) / 2) as i32;
        let (cx, cy) = ((pad + w / 2) as i32, (pad + h / 2) as i32);
        draw_ring(
            &mut Band::whole(&mut frame),
            cx,
            cy,
            r,
            r + 1,
            [color[0], color[1], color[2], 255],
        );
    } else {
        paint_shape(
            &mut frame,
            pad,
            pad,
            w,
            h,
            false,
            [color[0], color[1], color[2], 255],
        );
    }
    frame
}

/// One state of a picture indicator: the picture's own alpha in the glow
/// colour, blurred, behind the picture, in a canvas of the largest picture
/// padded by twice the glow radius.
pub fn icon_state_frame(
    icon: &Frame,
    canvas: (u32, u32),
    glow: u32,
    intensity: f32,
    glow_color: Option<[u8; 3]>,
) -> Frame {
    let pad = if glow > 0 { glow * 2 } else { 0 };
    let (cw, ch) = (canvas.0 + pad * 2, canvas.1 + pad * 2);
    let mut frame = empty_frame(cw, ch);
    if glow > 0 && intensity > 0.0 {
        let mut halo = empty_frame(icon.width + pad * 2, icon.height + pad * 2);
        let color = glow_color.unwrap_or([255, 255, 255]);
        let level = 255.0 * intensity.clamp(0.0, 1.0);
        for y in 0..icon.height {
            for x in 0..icon.width {
                let a = icon.rgba[((y * icon.width + x) * 4 + 3) as usize];
                if a > 0 {
                    let i = (((y + pad) * halo.width + x + pad) * 4) as usize;
                    halo.rgba[i..i + 4].copy_from_slice(&[
                        color[0],
                        color[1],
                        color[2],
                        (a as f32 / 255.0 * level) as u8,
                    ]);
                }
            }
        }
        let halo = blur(&halo, glow);
        let gx = (cw as i32 - halo.width as i32) / 2;
        let gy = (ch as i32 - halo.height as i32) / 2;
        over_onto(&mut frame, &halo, (gx, gy));
    }
    let ix = (cw as i32 - icon.width as i32) / 2;
    let iy = (ch as i32 - icon.height as i32) / 2;
    over_onto(&mut frame, icon, (ix, iy));
    frame
}

/// Source-over a picture onto a canvas that may be transparent, keeping
/// straight alpha: what a state picture is built on before it goes over the frame.
fn over_onto(dst: &mut Frame, src: &Frame, at: (i32, i32)) {
    for y in 0..src.height {
        let dy = at.1 + y as i32;
        if dy < 0 || dy >= dst.height as i32 {
            continue;
        }
        for x in 0..src.width {
            let dx = at.0 + x as i32;
            if dx < 0 || dx >= dst.width as i32 {
                continue;
            }
            let s = ((y * src.width + x) * 4) as usize;
            let d = ((dy as u32 * dst.width + dx as u32) * 4) as usize;
            let sa = src.rgba[s + 3] as u32;
            if sa == 0 {
                continue;
            }
            let da = dst.rgba[d + 3] as u32;
            let out_a = sa * 255 + da * (255 - sa);
            for c in 0..3 {
                let sc = src.rgba[s + c] as u32;
                let dc = dst.rgba[d + c] as u32;
                dst.rgba[d + c] =
                    ((sc * sa * 255 + dc * da * (255 - sa) + out_a / 2) / out_a.max(1)) as u8;
            }
            dst.rgba[d + 3] = ((out_a + 127) / 255) as u8;
        }
    }
}

/// The pictures an indicator set needs, prepared once per meter.
#[derive(Default)]
pub struct IndicatorAssets {
    /// One frame per state for mute, shuffle, repeat and play state.
    pub mute: Vec<Option<Frame>>,
    pub shuffle: Vec<Option<Frame>>,
    pub repeat: Vec<Option<Frame>>,
    pub playstate: Vec<Option<Frame>>,
    pub volume: GaugeAssets,
    pub progress: GaugeAssets,
    /// One picture per button, when it has one, and the whole list the
    /// theme gives it: rest and active, or one per state with three or more.
    pub buttons: Vec<Option<Frame>>,
    pub buttons_pictures: Vec<Vec<Option<Frame>>>,
}

#[derive(Default)]
pub struct GaugeAssets {
    pub knob: Option<Frame>,
    pub track: Option<Frame>,
    pub tip: Option<Frame>,
    pub head: Option<Frame>,
    /// One per marker, a picture when the marker has one.
    pub markers: Vec<Option<Frame>>,
}

impl IndicatorAssets {
    pub fn bytes(&self) -> usize {
        let gauge =
            |g: &GaugeAssets| bytes_of([&g.knob, &g.track, &g.tip, &g.head]) + bytes_of(&g.markers);
        bytes_of(&self.mute)
            + bytes_of(&self.shuffle)
            + bytes_of(&self.repeat)
            + bytes_of(&self.playstate)
            + gauge(&self.volume)
            + gauge(&self.progress)
            + bytes_of(&self.buttons)
            + self.buttons_pictures.iter().map(bytes_of).sum::<usize>()
    }

    pub fn load(spec: &lead::IndicatorsSpec) -> Self {
        let states = |indicator: &Option<StateIndicator>| -> Vec<Option<Frame>> {
            let Some(indicator) = indicator else {
                return Vec::new();
            };
            match &indicator.look {
                StateLook::Led {
                    w,
                    h,
                    circle,
                    colors,
                } => colors
                    .iter()
                    .enumerate()
                    .map(|(i, &color)| {
                        let glow_color = indicator
                            .glow_colors
                            .get(i)
                            .or(indicator.glow_colors.last())
                            .copied()
                            .unwrap_or(color);
                        Some(led_state_frame(
                            *w,
                            *h,
                            *circle,
                            color,
                            indicator.glow,
                            indicator.glow_intensity,
                            glow_color,
                        ))
                    })
                    .collect(),
                StateLook::Icons { files } => {
                    let icons: Vec<Option<Frame>> = files
                        .iter()
                        .map(|f| {
                            if f.is_empty() {
                                None
                            } else {
                                read_png(Path::new(f))
                            }
                        })
                        .collect();
                    let canvas = icons
                        .iter()
                        .flatten()
                        .fold((0, 0), |(w, h), f| (w.max(f.width), h.max(f.height)));
                    icons
                        .iter()
                        .enumerate()
                        .map(|(i, icon)| {
                            icon.as_ref().map(|icon| {
                                icon_state_frame(
                                    icon,
                                    canvas,
                                    indicator.glow,
                                    indicator.glow_intensity,
                                    indicator.glow_colors.get(i).copied(),
                                )
                            })
                        })
                        .collect()
                }
            }
        };
        let gauge = |gauge: &Option<GaugeSpec>| -> GaugeAssets {
            let Some(g) = gauge else {
                return GaugeAssets::default();
            };
            let picture = |file: &str| {
                if file.is_empty() {
                    None
                } else {
                    read_png(Path::new(file))
                }
            };
            GaugeAssets {
                knob: if g.style == GaugeStyle::Knob {
                    picture(&g.knob_image)
                } else {
                    None
                },
                track: picture(&g.track),
                tip: picture(&g.tip),
                head: picture(&g.head_image),
                markers: g.markers.iter().map(|m| picture(&m.image)).collect(),
            }
        };
        Self {
            mute: states(&spec.mute),
            shuffle: states(&spec.shuffle),
            repeat: states(&spec.repeat),
            playstate: states(&spec.playstate),
            volume: gauge(&spec.volume),
            progress: gauge(&spec.progress),
            buttons: spec
                .buttons
                .iter()
                .map(|b| {
                    if b.image.is_empty() {
                        None
                    } else {
                        read_png(Path::new(&b.image))
                    }
                })
                .collect(),
            buttons_pictures: spec
                .buttons
                .iter()
                .map(|b| {
                    b.images
                        .iter()
                        .map(|p| {
                            if p.is_empty() {
                                None
                            } else {
                                read_png(Path::new(p))
                            }
                        })
                        .collect()
                })
                .collect(),
        }
    }
}

/// A filled rectangle with rounded corners, the radius clamped to half the
/// shorter side as the player's draw call clamps it.
fn fill_round_rect(band: &mut Band, x: i32, y: i32, w: i32, h: i32, radius: i32, color: [u8; 4]) {
    if w <= 0 || h <= 0 {
        return;
    }
    let r = radius.clamp(0, w.min(h) / 2) as f32;
    let cols = band.cols(x, x.saturating_add(w));
    for py in band.rows(y, y.saturating_add(h)) {
        let row = band.row(py);
        for px in cols.clone() {
            if r > 0.0 {
                let fx = px as f32 + 0.5;
                let fy = py as f32 + 0.5;
                let cx = fx.clamp(x as f32 + r, (x + w) as f32 - r);
                let cy = fy.clamp(y as f32 + r, (y + h) as f32 - r);
                if (fx - cx).powi(2) + (fy - cy).powi(2) > r * r {
                    continue;
                }
            }
            blend(row, px as usize * 4, color);
        }
    }
}

/// A solid arc: the ring between the box's ellipse and the same ellipse
/// `ring` inside it, from `start` counter-clockwise to `stop` degrees with 0
/// pointing right, sampled four times a pixel for a soft rim.
#[allow(clippy::too_many_arguments)]
fn fill_arc(
    band: &mut Band,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    start: f32,
    stop: f32,
    ring: u32,
    color: [u8; 3],
) {
    if w == 0 || h == 0 {
        return;
    }
    let mut span = (stop - start).rem_euclid(360.0);
    if span == 0.0 && (stop - start).abs() > 0.0 {
        span = 360.0;
    }
    if span <= 0.0 {
        return;
    }
    let (cx, cy) = (x as f32 + w as f32 / 2.0, y as f32 + h as f32 / 2.0);
    let (rx, ry) = (w as f32 / 2.0, h as f32 / 2.0);
    let (rx_in, ry_in) = ((rx - ring as f32).max(0.0), (ry - ring as f32).max(0.0));
    let cols = band.cols(x, x.saturating_add(w as i32));
    for py in band.rows(y, y.saturating_add(h as i32)) {
        let row = band.row(py);
        for px in cols.clone() {
            let mut hits = 0;
            for (sx, sy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                let dx = px as f32 + sx - cx;
                let dy = py as f32 + sy - cy;
                let outer = (dx / rx).powi(2) + (dy / ry).powi(2);
                if outer > 1.0 {
                    continue;
                }
                if rx_in > 0.5 && ry_in > 0.5 && (dx / rx_in).powi(2) + (dy / ry_in).powi(2) < 1.0 {
                    continue;
                }
                let angle = (-dy).atan2(dx).to_degrees();
                if (angle - start).rem_euclid(360.0) <= span {
                    hits += 1;
                }
            }
            if hits > 0 {
                blend(
                    row,
                    px as usize * 4,
                    [color[0], color[1], color[2], (255 * hits / 4) as u8],
                );
            }
        }
    }
}

/// Where a percentage sits along a gauge: on the bar, or on the arc.
fn gauge_point(g: &GaugeSpec, pct: f32) -> (i32, i32) {
    let p = pct.clamp(0.0, 100.0) / 100.0;
    match g.style {
        GaugeStyle::Slider => {
            if g.vertical() {
                (
                    g.x + g.w as i32 / 2,
                    g.y + g.h as i32 - (p * g.h as f32) as i32,
                )
            } else {
                (g.x + (p * g.w as f32) as i32, g.y + g.h as i32 / 2)
            }
        }
        GaugeStyle::Arc | GaugeStyle::Knob => {
            let cx = g.x + g.w as i32 / 2;
            let cy = g.y + g.h as i32 / 2;
            let r = ((g.w.min(g.h) / 2) as i32 - g.arc_width as i32).max(4) as f32;
            let angle = (g.arc_start - p * (g.arc_start - g.arc_end)).to_radians();
            (cx + (r * angle.cos()) as i32, cy - (r * angle.sin()) as i32)
        }
        GaugeStyle::Numeric => (g.x + (p * g.w as f32) as i32, g.y + g.h as i32 / 2),
    }
}

/// The lines a gauge shows, set in type and kept in `Labels`: the value of
/// a numeric gauge and one entry per marker.
struct GaugeLabels {
    value: Option<String>,
    markers: Vec<Option<String>>,
}

fn gauge_labels(
    g: &GaugeSpec,
    value: u32,
    fonts: Option<&Fonts>,
    labels: &mut Labels,
    tick: u64,
) -> GaugeLabels {
    let value = value.min(100);
    let numeric = match g.style {
        GaugeStyle::Numeric => labels.ensure(
            fonts,
            TextStyle::Regular,
            g.font_size,
            g.color,
            &format!("{value}%"),
            tick,
        ),
        _ => None,
    };
    let markers = g
        .markers
        .iter()
        .map(|m| {
            labels.ensure(
                fonts,
                TextStyle::Regular,
                m.font_size.unwrap_or(g.font_size),
                g.color,
                &m.label,
                tick,
            )
        })
        .collect();
    GaugeLabels {
        value: numeric,
        markers,
    }
}

/// One gauge at a value from 0 to 100, as the player's slider indicator draws it.
fn plan_gauge<'a>(
    g: &GaugeSpec,
    value: u32,
    assets: &'a GaugeAssets,
    labels: &'a Labels,
    names: &GaugeLabels,
    ops: &mut Vec<Op<'a>>,
) {
    let value = value.min(100);
    let rgb = |c: [u8; 3]| [c[0], c[1], c[2], 255];
    match g.style {
        GaugeStyle::Numeric => {
            if let Some(line) = names.value.as_deref().and_then(|k| labels.get(k)) {
                ops.push(blit_op(line, (g.x.max(0), g.y.max(0))));
            }
        }
        GaugeStyle::Slider => {
            if let Some(tip) = &assets.tip {
                if let Some(track) = &assets.track {
                    ops.push(blit_op(track, (g.x, g.y)));
                }
                let (tw, th) = (tip.width as i32, tip.height as i32);
                let (w, h) = (g.w as i32, g.h as i32);
                let travel = g.travel.unwrap_or(if g.vertical() {
                    (0, h - th)
                } else {
                    (0, w - tw)
                });
                let range = travel.1 - travel.0;
                let (tip_x, tip_y) = if g.vertical() {
                    (
                        g.x + g.tip_offset.0 + (w - tw) / 2,
                        g.y + travel.1 - (value as f32 / 100.0 * range as f32) as i32
                            + g.tip_offset.1,
                    )
                } else {
                    (
                        g.x + travel.0
                            + (value as f32 / 100.0 * range as f32) as i32
                            + g.tip_offset.0,
                        g.y + g.tip_offset.1 + (h - th) / 2,
                    )
                };
                if let Some(fill) = g.fill_color {
                    let (dx, dy) = g.fill_offset;
                    if g.vertical() {
                        let thickness = g.fill_width.map_or(tw, |f| f as i32).max(1);
                        let cx = tip_x + tw / 2 + dx;
                        let current = tip_y + th / 2;
                        let zero = g.y + travel.1 + g.tip_offset.1 + th / 2;
                        let top = current.min(zero) + dy;
                        let fill_h = (zero - current).abs();
                        ops.push(Op::RoundRect {
                            x: cx - thickness / 2,
                            y: top,
                            w: thickness,
                            h: fill_h,
                            radius: g.fill_radius as i32,
                            color: rgb(fill),
                        });
                    } else {
                        let thickness = g.fill_width.map_or(th, |f| f as i32).max(1);
                        let cy = tip_y + th / 2 + dy;
                        let current = tip_x + tw / 2;
                        let zero = g.x + travel.0 + g.tip_offset.0 + tw / 2;
                        let left = current.min(zero) + dx;
                        let fill_w = (current - zero).abs();
                        ops.push(Op::RoundRect {
                            x: left,
                            y: cy - thickness / 2,
                            w: fill_w,
                            h: thickness,
                            radius: g.fill_radius as i32,
                            color: rgb(fill),
                        });
                    }
                }
                ops.push(blit_op(tip, (tip_x, tip_y)));
            } else {
                let (w, h) = (g.w as i32, g.h as i32);
                if let Some(bg) = g.bg_color {
                    ops.push(Op::RoundRect {
                        x: g.x,
                        y: g.y,
                        w,
                        h,
                        radius: g.fill_radius as i32,
                        color: rgb(bg),
                    });
                }
                if g.vertical() {
                    let fill_h = (value as f32 / 100.0 * h as f32) as i32;
                    if fill_h > 0 {
                        ops.push(Op::RoundRect {
                            x: g.x,
                            y: g.y + h - fill_h,
                            w,
                            h: fill_h,
                            radius: g.fill_radius as i32,
                            color: rgb(g.color),
                        });
                    }
                } else {
                    let fill_w = (value as f32 / 100.0 * w as f32) as i32;
                    if fill_w > 0 {
                        ops.push(Op::RoundRect {
                            x: g.x,
                            y: g.y,
                            w: fill_w,
                            h,
                            radius: g.fill_radius as i32,
                            color: rgb(g.color),
                        });
                    }
                }
                if g.border > 0 && g.x >= 0 && g.y >= 0 {
                    ops.push(Op::Border {
                        rect: (g.x as u32, g.y as u32, g.w, g.h),
                        thickness: g.border,
                        color: rgb(g.border_color),
                    });
                }
            }
        }
        GaugeStyle::Knob => {
            if let Some(knob) = &assets.knob {
                let angle = g.knob_start - value as f32 / 100.0 * (g.knob_start - g.knob_end);
                let centre = (g.x as f32 + g.w as f32 / 2.0, g.y as f32 + g.h as f32 / 2.0);
                ops.push(Op::Turn {
                    src: knob,
                    pivot_image: centre_of(knob),
                    pivot_screen: centre,
                    degrees: angle,
                    smooth: false,
                    reach: reach_of(knob, centre_of(knob)),
                });
            } else {
                plan_arc_gauge(g, value, ops);
            }
        }
        GaugeStyle::Arc => plan_arc_gauge(g, value, ops),
    }
    for ((marker, picture), name) in g
        .markers
        .iter()
        .zip(assets.markers.iter())
        .zip(names.markers.iter())
    {
        let (px, py) = gauge_point(g, marker.pos);
        if let Some(picture) = picture {
            ops.push(blit_op(
                picture,
                (
                    px - picture.width as i32 / 2,
                    py - picture.height as i32 / 2,
                ),
            ));
        } else if let Some(line) = name.as_deref().and_then(|k| labels.get(k)) {
            ops.push(blit_op(
                line,
                (px - line.width as i32 / 2, py - line.height as i32 / 2),
            ));
        }
    }
    if let Some(head) = &assets.head {
        let (px, py) = gauge_point(g, value as f32);
        ops.push(blit_op(
            head,
            (
                px - head.width as i32 / 2 + g.head_offset.0,
                py - head.height as i32 / 2 + g.head_offset.1,
            ),
        ));
    }
}

fn plan_arc_gauge(g: &GaugeSpec, value: u32, ops: &mut Vec<Op<'_>>) {
    if let Some(bg) = g.bg_color {
        ops.push(Op::Arc {
            x: g.x,
            y: g.y,
            w: g.w,
            h: g.h,
            start: g.arc_end,
            stop: g.arc_start,
            ring: g.arc_width,
            color: bg,
        });
    }
    if value > 0 {
        let current = g.arc_start - value as f32 / 100.0 * (g.arc_start - g.arc_end);
        ops.push(Op::Arc {
            x: g.x,
            y: g.y,
            w: g.w,
            h: g.h,
            start: current,
            stop: g.arc_start,
            ring: g.arc_width,
            color: g.color,
        });
    }
}

/// The lines the indicators show this frame.
struct IndicatorLabels {
    volume: Option<GaugeLabels>,
    progress: Option<GaugeLabels>,
}

fn indicator_labels(
    indicators: &Indicators,
    fonts: Option<&Fonts>,
    labels: &mut Labels,
    tick: u64,
) -> IndicatorLabels {
    IndicatorLabels {
        volume: indicators
            .spec
            .volume
            .as_ref()
            .map(|g| gauge_labels(g, indicators.volume, fonts, labels, tick)),
        progress: indicators
            .spec
            .progress
            .as_ref()
            .map(|g| gauge_labels(g, indicators.progress, fonts, labels, tick)),
    }
}

/// The indicators in their states: a state past a look's last state takes
/// the last, as the player clamps it.
fn plan_indicators<'a>(
    indicators: &Indicators,
    assets: &'a IndicatorAssets,
    labels: &'a Labels,
    names: &IndicatorLabels,
    ops: &mut Vec<Op<'a>>,
) {
    let mut state = |spec: &Option<StateIndicator>, frames: &'a [Option<Frame>], index: usize| {
        let (Some(spec), false) = (spec, frames.is_empty()) else {
            return;
        };
        let index = index.min(frames.len() - 1);
        if let Some(picture) = &frames[index] {
            ops.push(blit_op(picture, (spec.x, spec.y)));
        }
    };
    state(&indicators.spec.mute, &assets.mute, indicators.mute_state);
    state(
        &indicators.spec.shuffle,
        &assets.shuffle,
        indicators.shuffle_state,
    );
    state(
        &indicators.spec.repeat,
        &assets.repeat,
        indicators.repeat_state,
    );
    state(
        &indicators.spec.playstate,
        &assets.playstate,
        indicators.play_state,
    );
    // The theme's buttons, drawn where they are tapped: the active
    // picture while the button is active and the theme gives one, or,
    // with three pictures or more, the one for the state of the action's
    // indicator.
    for (i, (button, picture)) in indicators
        .spec
        .buttons
        .iter()
        .zip(assets.buttons.iter())
        .enumerate()
    {
        let active = indicators.buttons_active.get(i).copied().unwrap_or(false);
        let pictures = assets
            .buttons_pictures
            .get(i)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let index = if pictures.len() >= 3 {
            let state = match button.action {
                lead::ButtonAction::Repeat => indicators.repeat_state,
                lead::ButtonAction::Random => indicators.shuffle_state,
                lead::ButtonAction::Mute => indicators.mute_state,
                lead::ButtonAction::Play
                | lead::ButtonAction::Pause
                | lead::ButtonAction::Stop
                | lead::ButtonAction::Toggle => indicators.play_state,
                _ => usize::from(active),
            };
            state.min(pictures.len() - 1)
        } else {
            usize::from(active)
        };
        let shown = pictures
            .get(index)
            .and_then(|p| p.as_ref())
            .or(picture.as_ref());
        if let Some(picture) = shown {
            ops.push(blit_op(picture, (button.x, button.y)));
        }
    }
    if let (Some(volume), Some(names)) = (&indicators.spec.volume, &names.volume) {
        plan_gauge(
            volume,
            indicators.volume,
            &assets.volume,
            labels,
            names,
            ops,
        );
    }
    if let (Some(progress), Some(names)) = (&indicators.spec.progress, &names.progress) {
        plan_gauge(
            progress,
            indicators.progress,
            &assets.progress,
            labels,
            names,
            ops,
        );
    }
}

/// A theme picture by file name under the theme's folder; none for an
/// empty name or a file that is not there.
pub fn load_theme(dir: &str, file: &str) -> Option<Frame> {
    if dir.is_empty() || file.is_empty() {
        return None;
    }
    read_png(Path::new(dir).join(file).as_path())
}

/// Everything decoded once per meter: its pictures, fonts and art mask,
/// as the display and the browser module both prepare them.
pub struct MeterAssets {
    pub front: Option<Spans>,
    /// The indicator for the left or mono channel, mirrored when the meter flips it.
    pub indicator: Option<Frame>,
    /// The right channel's indicator when it differs from the left one.
    pub indicator_right: Option<Frame>,
    pub fonts: Fonts,
    pub art_mask: Option<Frame>,
    /// The spectrum boxes' pictures, one per box the meter shows.
    pub spectra: Vec<SpectrumAssets>,
    /// The tonearm picture when the meter has one.
    pub tonearm: Option<Frame>,
    /// The theme's reel pictures; an album's reel is scaled to their size.
    pub reels: (Option<Frame>, Option<Frame>),
    /// The indicators' prepared states and pictures.
    pub indicators: Option<IndicatorAssets>,
    /// The screen picture and face composed once per meter; the pictures
    /// themselves are not kept.
    pub base: Frame,
}

impl MeterAssets {
    /// What the meter's pictures and fonts take, by store, in bytes.
    pub fn memory(&self) -> Vec<(&'static str, usize)> {
        vec![
            ("base", self.base.bytes()),
            ("front", self.front.as_ref().map_or(0, Spans::bytes)),
            (
                "needles",
                bytes_of([&self.indicator, &self.indicator_right]),
            ),
            ("tonearm", bytes_of([&self.tonearm])),
            ("reels", bytes_of([&self.reels.0, &self.reels.1])),
            ("mask", bytes_of([&self.art_mask])),
            (
                "spectrum",
                self.spectra
                    .iter()
                    .map(SpectrumAssets::bytes)
                    .sum::<usize>(),
            ),
            (
                "indicators",
                self.indicators.as_ref().map_or(0, IndicatorAssets::bytes),
            ),
            ("fonts", self.fonts.bytes()),
        ]
    }

    /// The meter's assets from its skin: the fonts, the indicator flipped
    /// as the meter says, the base composed from the screen picture and
    /// the face, and every other picture the meter names.
    pub fn load(skin: &lead::SkinDesc) -> Self {
        let mut fonts = Fonts::load(&skin.fonts);
        for field in [
            &skin.time,
            &skin.time_elapsed,
            &skin.time_total,
            &skin.volume_value,
        ]
        .into_iter()
        .flatten()
        {
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
        let background = load_theme(&skin.theme_dir, &skin.background);
        let face = load_theme(&skin.theme_dir, &skin.face);
        let base = compose_base(
            skin.width.max(1),
            skin.height.max(1),
            background.as_ref(),
            face.as_ref(),
            skin.face_at,
        );
        Self {
            base,
            front: load_theme(&skin.theme_dir, &skin.front).map(Spans::new),
            indicator,
            indicator_right,
            fonts,
            art_mask: skin
                .art
                .as_ref()
                .filter(|art| !art.mask.is_empty())
                .and_then(|art| read_png(Path::new(&art.mask))),
            spectra: skin.spectra.iter().map(SpectrumAssets::load).collect(),
            tonearm: skin
                .tonearm
                .as_ref()
                .and_then(|arm| read_png(Path::new(&arm.file))),
            reels: (
                skin.reels
                    .as_ref()
                    .and_then(|r| r.left.as_ref())
                    .and_then(|r| read_png(Path::new(&r.theme_file))),
                skin.reels
                    .as_ref()
                    .and_then(|r| r.right.as_ref())
                    .and_then(|r| read_png(Path::new(&r.theme_file))),
            ),
            indicators: skin.indicators.as_ref().map(IndicatorAssets::load),
        }
    }
}

/// The static background: the dark fill, the screen picture over it, and
/// the meter face at its position.
pub fn compose_base(
    width: u32,
    height: u32,
    screen: Option<&Frame>,
    face: Option<&Frame>,
    face_at: (u32, u32),
) -> Frame {
    let (width, height) = (width.max(1), height.max(1));
    let mut frame = Frame {
        blend: Blend::Normal,
        width,
        height,
        rgba: vec![0u8; (width * height * 4) as usize],
    };
    for px in frame.rgba.as_chunks_mut::<4>().0 {
        px.copy_from_slice(&BG);
    }
    if let Some(screen) = screen {
        copy_top_left(&mut frame, screen);
    }
    if let Some(face) = face {
        blit_op(face, (face_at.0 as i32, face_at.1 as i32)).paint(&mut Band::whole(&mut frame));
    }
    frame
}

fn place(fallback: Rect, at: Option<(i32, i32)>, width: u32, height: u32) -> Rect {
    let Some((x, y)) = at else {
        return fallback;
    };
    if x < 0 || y < 0 {
        return fallback;
    }
    let (x, y) = (x as u32, y as u32);
    if x >= width || y >= height {
        return fallback;
    }
    Rect {
        x,
        y,
        w: fallback.w.min(width - x).max(1),
        h: fallback.h.min(height - y).max(1),
    }
}

/// Copy a picture's top left over the frame's, as far as both reach. It is
/// not scaled: a theme is authored at its own resolution.
fn copy_top_left(dst: &mut Frame, src: &Frame) {
    let copy_w = dst.width.min(src.width) as usize;
    let copy_h = dst.height.min(src.height) as usize;
    for y in 0..copy_h {
        let from = y * src.width as usize * 4;
        let to = y * dst.width as usize * 4;
        dst.rgba[to..to + copy_w * 4].copy_from_slice(&src.rgba[from..from + copy_w * 4]);
    }
}

/// Write a frame as a PNG, through a part file renamed into place.
pub fn write_png(path: &Path, frame: &Frame) -> Result<(), String> {
    let part = path.with_extension("png.part");
    image::save_buffer_with_format(
        &part,
        &frame.rgba,
        frame.width,
        frame.height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(&part, path).map_err(|e| e.to_string())
}

/// Load a theme PNG. The alpha channel is kept.
pub fn read_png(path: &Path) -> Option<Frame> {
    // The format is read from the bytes, not the name: a remote keeps the
    // pictures it fetches under hashed names with no telling extension.
    let image = image::ImageReader::new(std::io::Cursor::new(read_file(path)?))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?
        .into_rgba8();
    let width = image.width();
    let height = image.height();
    Some(Frame {
        blend: Blend::Normal,
        width,
        height,
        rgba: image.into_raw(),
    })
}

/// A line in the built-in 8 by 8 font at twice its size, for a text whose
/// style has no font.
fn bitmap_line(text: &str) -> Frame {
    use font8x8::UnicodeFonts;
    let scale = 2u32;
    let count = text.chars().count().max(1) as u32;
    let mut frame = empty_frame(count * 8 * scale, 8 * scale);
    for (index, ch) in text.chars().enumerate() {
        let Some(glyph) = font8x8::BASIC_FONTS.get(ch) else {
            continue;
        };
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..8u32 {
                if bits & (1 << col) == 0 {
                    continue;
                }
                for sy in 0..scale {
                    for sx in 0..scale {
                        let px = index as u32 * 8 * scale + col * scale + sx;
                        let py = row as u32 * scale + sy;
                        let i = ((py * frame.width + px) * 4) as usize;
                        frame.rgba[i..i + 4].copy_from_slice(&[240, 240, 240, 255]);
                    }
                }
            }
        }
    }
    frame
}

/// A line of bitmap text at a position.
pub fn draw_text(frame: &mut Frame, x: u32, y: u32, text: &str) {
    let line = bitmap_line(text);
    blit_op(&line, (x as i32, y as i32)).paint(&mut Band::whole(frame));
}

#[cfg(test)]
mod tests {
    /// The rows a rendered line inks (alpha above 60), first and last.
    fn ink_rows(frame: &Frame) -> (u32, u32) {
        let inked = |y: u32| {
            (0..frame.width).any(|x| frame.rgba[((y * frame.width + x) * 4 + 3) as usize] > 60)
        };
        let first = (0..frame.height).find(|&y| inked(y)).unwrap();
        let last = (0..frame.height).rev().find(|&y| inked(y)).unwrap();
        (first, last)
    }

    /// A line lands on the rows pygame's SDL_ttf sets it on, measured with
    /// pygame 2.5.2 (SDL_ttf 2.22, FreeType 2.12): the size is the em in
    /// pixels, the baseline the ascent rounded up. PeppyFont's height is
    /// 1.227 em, so its glyphs would come out small and high were the size
    /// taken as the height; DSEG7's height is its em.
    #[test]
    fn a_line_sits_on_the_rows_pygame_sets_it_on() {
        let mut fonts = Fonts::default();
        let white = [255, 255, 255];
        let clock = fonts
            .face(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../plugin/fonts/DSEG7Classic-Italic.ttf"
            ))
            .expect("the clock font ships with the plugin");
        let italic = fonts
            .face(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../plugin/fonts/PeppyFont-Italic.ttf"
            ))
            .expect("the italic face ships with the plugin");
        // pygame: DSEG7 26 'H' surface height 26, ink rows 1..24; 40: 40, 2..37.
        let h = render_line(Some(&clock.font), None, 26, white, "H", 0).unwrap();
        assert_eq!((h.height, ink_rows(&h)), (26, (1, 24)));
        let h = render_line(Some(&clock.font), None, 40, white, "H", 0).unwrap();
        assert_eq!((h.height, ink_rows(&h)), (40, (2, 37)));
        // pygame: PeppyFont-Italic 26 'H' height 32, rows 8..25; 'Hg' 8..31; 40 'H' 50, 11..39.
        // FreeType hints the outlines onto the pixel grid and this raster
        // does not, so a glyph's top may start one row of anti-aliasing
        // above pygame's; the baseline row and the line's height are exact.
        let as_pygame = |frame: &Frame, height: u32, top: u32, bottom: u32| {
            let rows = ink_rows(frame);
            assert_eq!((frame.height, rows.1), (height, bottom));
            assert!(
                rows.0 == top || rows.0 + 1 == top,
                "top row {} for {top}",
                rows.0
            );
        };
        let h = render_line(Some(&italic.font), None, 26, white, "H", 0).unwrap();
        as_pygame(&h, 32, 8, 25);
        let hg = render_line(Some(&italic.font), None, 26, white, "Hg", 0).unwrap();
        as_pygame(&hg, 32, 8, 31);
        let h = render_line(Some(&italic.font), None, 40, white, "H", 0).unwrap();
        as_pygame(&h, 50, 11, 39);
        // A letter from the fallback face is set at the text's em, on the
        // text's baseline: lambda in a clock line at 26 sits where the
        // italic face sets it alone (pygame: rows 6..25, both baselines 26).
        let lambda = "\u{3bb}";
        let helped =
            render_line(Some(&clock.font), Some(&italic.font), 26, white, lambda, 0).unwrap();
        let alone = render_line(Some(&italic.font), None, 26, white, lambda, 0).unwrap();
        assert_eq!(ink_rows(&helped), ink_rows(&alone));
        as_pygame(&alone, 32, 6, 25);
    }

    /// A face that lacks a character hands it to the fallback face: the
    /// clock font sets digits and the hex letters, so a Greek letter comes
    /// from the host's font and takes the width it has there.
    #[test]
    fn a_glyph_the_face_lacks_comes_from_the_fallback_face() {
        let Some(host_font) = any_font() else {
            println!("no TrueType font on this host; fallback path only");
            return;
        };
        let digits = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../plugin/fonts/DSEG7Classic-Regular.ttf"
        );
        let mut fonts = Fonts::default();
        let clock = fonts
            .face(digits)
            .expect("the clock font ships with the plugin");
        let other = fonts.face(&host_font).expect("a host font");
        let lambda = "\u{3bb}";
        if other.font.glyph_id('\u{3bb}').0 == 0 {
            println!("the host's font has no Greek; fallback path only");
            return;
        }
        assert_eq!(
            clock.font.glyph_id('\u{3bb}').0,
            0,
            "the clock font lacks lambda"
        );
        let white = [255, 255, 255];
        let helped =
            render_line(Some(&clock.font), Some(&other.font), 24, white, lambda, 0).unwrap();
        let from_other = render_line(Some(&other.font), None, 24, white, lambda, 0).unwrap();
        assert_eq!(
            helped.width, from_other.width,
            "the letter is set in the fallback face"
        );
        assert!(
            helped.rgba.iter().skip(3).step_by(4).any(|&a| a > 0),
            "the letter is drawn"
        );
        // A digit stays with the clock font, fallback or not.
        let one = render_line(Some(&clock.font), Some(&other.font), 24, white, "1", 0).unwrap();
        let one_alone = render_line(Some(&clock.font), None, 24, white, "1", 0).unwrap();
        assert_eq!(one.width, one_alone.width);
        // With the shipped multi-script face at hand, a Japanese title sets whole.
        let peppy = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../plugin/fonts/PeppyFont-Regular.ttf"
        );
        if let Some(multi) = fonts.face(peppy) {
            let japan = render_line(
                Some(&other.font),
                Some(&multi.font),
                24,
                white,
                "Ellie \u{65e5}\u{672c}",
                0,
            )
            .unwrap();
            let latin_only = render_line(Some(&other.font), None, 24, white, "Ellie ", 0).unwrap();
            assert!(
                japan.width > latin_only.width + 20,
                "two kanji were set after the Latin word"
            );
        }
    }

    #[test]
    fn a_picture_is_read_by_its_bytes_whatever_its_name() {
        // A remote saves what it fetches as <hash>.img; the loader must not
        // trust the name.
        let dir = std::env::temp_dir().join(format!("glass-read-png-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("0123456789abcdef.img");
        let picture = image::RgbImage::from_pixel(6, 4, image::Rgb([10, 20, 30]));
        let mut bytes = Vec::new();
        picture
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let frame = read_png(&path).expect("a jpeg under an .img name is read");
        assert_eq!((frame.width, frame.height), (6, 4));
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;
    use plot::Scene;

    #[test]
    fn a_full_left_meter_lights_the_top_of_its_column() {
        let scene = Scene {
            skin: "test".into(),
            width: 64,
            height: 32,
            left: 1.0,
            right: 0.0,
            bars: vec![1.0, 0.0],
            left_at: None,
            right_at: None,
            needle: None,
            texts: Vec::new(),
            art: None,
            type_area: None,
            ..Scene::default()
        };
        let frame = raster(&scene);
        let layout = layout(frame.width, frame.height);
        let top = sample(&frame, layout.left_meter.x, layout.left_meter.y);
        let right_top = sample(&frame, layout.right_meter.x, layout.right_meter.y);
        assert_eq!(top, METER);
        assert_eq!(right_top, BG);
    }

    fn sprite(width: u32, height: u32, color: [u8; 4]) -> Frame {
        Frame {
            blend: Blend::Normal,
            width,
            height,
            rgba: (0..width * height).flat_map(|_| color).collect(),
        }
    }

    fn at(rgba: &[u8], stride: u32, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * stride + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    }

    #[test]
    fn a_bar_shows_the_picture_from_the_end_its_direction_names() {
        // A 4 wide, 2 tall picture whose columns are 1, 2, 3, 4 in red.
        let mut pic = Frame {
            blend: Blend::Normal,
            width: 4,
            height: 2,
            rgba: vec![0; 4 * 2 * 4],
        };
        for y in 0..2 {
            for x in 0..4u32 {
                let i = ((y * 4 + x) * 4) as usize;
                pic.rgba[i..i + 4].copy_from_slice(&[(x + 1) as u8, 0, 0, 255]);
            }
        }
        let lit = |dst: &[u8], w: u32, x: u32, y: u32| dst[((y * w + x) * 4) as usize];
        let spec = |direction: Direction, single: bool, flip_right: bool| LinearSpec {
            regular: 4,
            step_regular: 1,
            direction,
            single,
            flip_right,
            ..LinearSpec::default()
        };
        // left-right: the first two columns at the origin.
        let mut dst = vec![0u8; 10 * 6 * 4];
        draw_bar(
            &mut Band::over(&mut dst, 10, 6),
            &pic,
            (2, 1),
            2,
            &spec(Direction::LeftRight, false, false),
            true,
        );
        assert_eq!(
            (
                lit(&dst, 10, 2, 1),
                lit(&dst, 10, 3, 1),
                lit(&dst, 10, 4, 1)
            ),
            (1, 2, 0)
        );
        // right-left: the last two columns, at the picture's right end.
        let mut dst = vec![0u8; 10 * 6 * 4];
        draw_bar(
            &mut Band::over(&mut dst, 10, 6),
            &pic,
            (2, 1),
            2,
            &spec(Direction::RightLeft, false, false),
            true,
        );
        assert_eq!(
            (
                lit(&dst, 10, 3, 1),
                lit(&dst, 10, 4, 1),
                lit(&dst, 10, 5, 1)
            ),
            (0, 3, 4)
        );
        // center-edges, left channel: the last two columns end at the origin.
        let mut dst = vec![0u8; 10 * 6 * 4];
        draw_bar(
            &mut Band::over(&mut dst, 10, 6),
            &pic,
            (5, 1),
            2,
            &spec(Direction::CenterEdges, false, false),
            true,
        );
        assert_eq!(
            (
                lit(&dst, 10, 3, 1),
                lit(&dst, 10, 4, 1),
                lit(&dst, 10, 5, 1)
            ),
            (3, 4, 0)
        );
        // edges-center, flipped right channel: the last two columns end at the origin.
        let mut dst = vec![0u8; 10 * 6 * 4];
        draw_bar(
            &mut Band::over(&mut dst, 10, 6),
            &pic,
            (5, 1),
            2,
            &spec(Direction::EdgesCenter, false, true),
            false,
        );
        assert_eq!(
            (
                lit(&dst, 10, 3, 1),
                lit(&dst, 10, 4, 1),
                lit(&dst, 10, 5, 1)
            ),
            (3, 4, 0)
        );
        // bottom-top: the bottom row only.
        let mut dst = vec![0u8; 10 * 6 * 4];
        draw_bar(
            &mut Band::over(&mut dst, 10, 6),
            &pic,
            (2, 1),
            1,
            &spec(Direction::BottomTop, false, false),
            true,
        );
        assert_eq!((lit(&dst, 10, 2, 1), lit(&dst, 10, 2, 2)), (0, 1));
        // single: the whole picture moved by w.
        let mut dst = vec![0u8; 10 * 6 * 4];
        draw_bar(
            &mut Band::over(&mut dst, 10, 6),
            &pic,
            (2, 1),
            3,
            &spec(Direction::LeftRight, true, false),
            true,
        );
        assert_eq!(
            (
                lit(&dst, 10, 4, 1),
                lit(&dst, 10, 5, 1),
                lit(&dst, 10, 8, 1)
            ),
            (0, 1, 4)
        );
        // mirrored picture.
        let flipped = flip_x(&pic);
        assert_eq!((flipped.rgba[0], flipped.rgba[12]), (4, 1));
    }

    #[test]
    fn spectrum_bars_rise_from_the_origin_reflect_below_it_and_toppings_fall() {
        let spec = SpectrumSpec {
            x: 10,
            y: 20,
            channel: lead::SpectrumChannel::Mean,
            w: 100,
            h: 60,
            origin_x: 5,
            origin_y: 40,
            bar_w: 2,
            bar_h: 20,
            gap: 1,
            steps: 10,
            bins: 2,
            max_value: 100.0,
            background: None,
            bar: Some(Fill::Color([255, 0, 0, 255])),
            reflection: Some(Fill::Color([0, 0, 255, 255])),
            reflection_gap: 1,
            topping: Some((1, 1)),
            foreground: Default::default(),
            demand: None,
            look: None,
        };
        let assets = SpectrumAssets::load(&spec);
        let mut motion = SpectrumMotion::default();
        let (w, h) = (120u32, 80u32);
        let red = |dst: &[u8], x: i32, y: i32| dst[((y as u32 * w + x as u32) * 4) as usize];
        let blue = |dst: &[u8], x: i32, y: i32| dst[((y as u32 * w + x as u32) * 4 + 2) as usize];
        // Half height on the first bar: 10 rows up from the baseline at y 60, at x 15.
        let mut dst = vec![0u8; (w * h * 4) as usize];
        draw_spectrum(
            &mut Band::over(&mut dst, w, h),
            &spec,
            &[10, 0],
            &assets,
            &mut motion,
        );
        assert_eq!(
            (red(&dst, 15, 59), red(&dst, 15, 50), red(&dst, 15, 49)),
            (255, 255, 0),
            "bar covers y 50..59"
        );
        assert_eq!(
            (blue(&dst, 15, 61), blue(&dst, 15, 70), blue(&dst, 15, 71)),
            (255, 255, 0),
            "reflection hangs from the gap"
        );
        assert_eq!(red(&dst, 18, 59), 0, "the second bar is silent");
        // The bar drops: the topping stays one step above where it was and falls one step a frame.
        let mut dst = vec![0u8; (w * h * 4) as usize];
        draw_spectrum(
            &mut Band::over(&mut dst, w, h),
            &spec,
            &[10, 0],
            &assets,
            &mut motion,
        );
        let mut dst = vec![0u8; (w * h * 4) as usize];
        draw_spectrum(
            &mut Band::over(&mut dst, w, h),
            &spec,
            &[2, 0],
            &assets,
            &mut motion,
        );
        assert_eq!(
            (red(&dst, 15, 49), red(&dst, 15, 55)),
            (255, 0),
            "topping drawn one step above the old top, bar gone there"
        );
        let mut dst = vec![0u8; (w * h * 4) as usize];
        draw_spectrum(
            &mut Band::over(&mut dst, w, h),
            &spec,
            &[2, 0],
            &assets,
            &mut motion,
        );
        assert_eq!(
            (red(&dst, 15, 49), red(&dst, 15, 50)),
            (0, 255),
            "a frame later it sits one step lower"
        );
        // Nothing outside the box: a bar past the right edge is clipped.
        let wide = SpectrumSpec {
            origin_x: 95,
            ..spec.clone()
        };
        let mut dst = vec![0u8; (w * h * 4) as usize];
        draw_spectrum(
            &mut Band::over(&mut dst, w, h),
            &wide,
            &[10, 10],
            &assets,
            &mut SpectrumMotion::default(),
        );
        assert_eq!(
            (red(&dst, 105, 59), red(&dst, 108, 59), red(&dst, 110, 59)),
            (255, 255, 0),
            "second bar starts at 108, clipped at 110"
        );
        // Gradient: first colour at the bottom.
        let g = gradient_frame(1, 3, &[[0, 0, 0, 255], [200, 0, 0, 255]]);
        assert_eq!((g.rgba[0], g.rgba[4], g.rgba[8]), (200, 100, 0));
    }

    #[test]
    fn a_tonearm_drops_tracks_and_lifts_and_a_record_slows_to_a_halt() {
        let spec = TonearmSpec {
            file: String::new(),
            pivot_screen: (0, 0),
            pivot_image: (0, 0),
            rest: 0.0,
            start: -20.0,
            end: -40.0,
            drop_s: 1.0,
            lift_s: 1.0,
        };
        let mut arm = TonearmMotion::default();
        arm.update(&spec, false, 0.0, None, 0);
        assert_eq!((arm.angle(), arm.is_animating()), (0.0, false), "parked");
        arm.update(&spec, true, 50.0, Some(100.0), 0);
        assert!(arm.is_animating(), "dropping");
        arm.update(&spec, true, 50.0, Some(100.0), 500);
        assert!(
            arm.angle() < -20.0 && arm.angle() > -30.0,
            "eased past the midpoint: {}",
            arm.angle()
        );
        arm.update(&spec, true, 50.0, Some(100.0), 1000);
        arm.update(&spec, true, 50.0, Some(100.0), 1001);
        assert_eq!(
            (arm.angle(), arm.is_animating()),
            (-30.0, false),
            "tracking at half the track"
        );
        arm.update(&spec, true, 55.0, Some(90.0), 1002);
        assert_eq!(arm.angle(), -31.0, "follows a small move");
        arm.update(&spec, true, 5.0, Some(190.0), 1003);
        assert!(arm.is_animating(), "a jump lifts the arm first");
        arm.update(&spec, true, 5.0, Some(190.0), 2100);
        arm.update(&spec, true, 5.0, Some(190.0), 3200);
        arm.update(&spec, true, 5.0, Some(190.0), 3201);
        assert_eq!(arm.angle(), -21.0, "dropped back to the new position");
        arm.update(&spec, true, 99.0, Some(1.0), 3202);
        assert!(arm.is_animating(), "lifts early before the end");

        let mut record = VinylMotion::default();
        record.advance(60.0, true, true, false, false, 1.0, 0);
        let a = record.advance(60.0, true, true, false, false, 1.0, 100);
        assert!(
            (a - 36.0).abs() < 0.01,
            "60 rpm turns 36 degrees in 100 ms: {a}"
        );
        record.advance(60.0, true, false, false, false, 1.0, 100);
        let b = record.advance(60.0, true, false, false, false, 1.0, 600);
        assert!(b > 36.0 && b < 36.0 + 180.0, "slowing: {b}");
        let c = record.advance(60.0, true, false, false, false, 1.0, 1200);
        let d = record.advance(60.0, true, false, false, false, 1.0, 1300);
        assert_eq!(c, d, "stopped after the lift");
        let mut ccw = VinylMotion::default();
        ccw.advance(60.0, false, true, false, false, 1.0, 0);
        assert!((ccw.advance(60.0, false, true, false, false, 1.0, 100) - 324.0).abs() < 0.01);
    }

    #[test]
    fn a_picture_kept_turned_keeps_its_soft_alpha() {
        let mut soft = Frame {
            blend: Blend::Normal,
            width: 4,
            height: 4,
            rgba: vec![0; 64],
        };
        for px in soft.rgba.as_chunks_mut::<4>().0 {
            px.copy_from_slice(&[200, 100, 50, 128]);
        }
        let turned = turn_picture(&soft, (2.0, 2.0), 0.0);
        let inside: Vec<[u8; 4]> = turned
            .frame
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] != 0)
            .map(|p| [p[0], p[1], p[2], p[3]])
            .collect();
        assert!(!inside.is_empty());
        assert!(
            inside.iter().all(|p| p[3] < 255 && p[0] >= 190),
            "kept half transparent and undarkened: {:?}",
            inside[0]
        );
        // Blended onto a white frame, a half-transparent pixel lightens, never blackens.
        let mut white = vec![255u8; 8 * 8 * 4];
        let mut cache = Turned::default();
        let key = cache.ensure(9, &soft, (2.0, 2.0), 0.0, 1);
        turned_op(&cache, key, (4.0, 4.0))
            .unwrap()
            .paint(&mut Band::over(&mut white, 8, 8));
        assert!(cache.bytes() > 0);
        let centre = &white[(4 * 8 + 4) * 4..(4 * 8 + 4) * 4 + 3];
        assert!(
            centre[0] > 200 && centre[2] > 120,
            "blended over white: {centre:?}"
        );
    }

    #[test]
    fn painters_grow_with_the_load_and_shrink_when_it_eases() {
        let mut painters = Painters::new(4, Some(30));
        assert_eq!(painters.active, 1);
        let mut clock = 0u64;
        let mut run = |painters: &mut Painters, frames: u64, paint_us: u64| {
            for _ in 0..frames {
                clock += 50;
                painters.settle(paint_us, clock);
            }
        };
        // 30 ms of painting in a 33 ms period overruns the budget: one thread is not enough.
        run(&mut painters, 100, 30_000);
        assert_eq!(
            painters.active, 2,
            "one more once the start is over and a second of overruns is in"
        );
        run(&mut painters, 40, 30_000);
        assert_eq!(painters.active, 3, "and another a second later");
        run(&mut painters, 40, 30_000);
        assert_eq!(painters.active, 4, "up to the most allowed");
        // 3 ms frames would fit one thread; a thread goes back every five seconds.
        run(&mut painters, 130, 3_000);
        assert_eq!(
            painters.active, 3,
            "one back after five seconds of light frames"
        );
        run(&mut painters, 250, 3_000);
        assert_eq!(painters.active, 1, "and the rest in turn");
        // Two thirds of the period on one thread is left alone, however long it lasts.
        run(&mut painters, 400, 22_000);
        assert_eq!(
            painters.active, 1,
            "two thirds of a period fits on one thread"
        );
        // A count that was measured too slow is not returned to on a guess.
        run(&mut painters, 40, 30_000);
        assert_eq!(painters.active, 2);
        run(&mut painters, 200, 10_000);
        assert_eq!(
            painters.active, 2,
            "one thread was seen at 30 ms; ten on two does not argue it down"
        );
        let mut fixed = Painters::new(4, None);
        for i in 0..40u64 {
            fixed.settle(3_000, i * 100);
        }
        assert_eq!(fixed.active, 4, "a fixed count stays");
    }

    #[test]
    fn bands_paint_what_one_thread_paints() {
        // A frame with every kind of step, painted on one thread and on several.
        let base = sprite(97, 61, [30, 40, 50, 255]);
        let needle = sprite(3, 30, [255, 0, 0, 200]);
        let layer = sprite(50, 20, [0, 200, 0, 128]);
        // A disc in a square picture: its measured reach is the disc's, not the square's diagonal.
        let disc = apply_circle(&sprite(30, 30, [120, 0, 200, 255]));
        let measured = Reaches::default().reach(&disc, (15.0, 15.0));
        assert!(
            measured < 17.5 && measured > 15.0,
            "reach of a 30 px disc is its radius and a little: {measured}"
        );
        assert!(
            reach_of(&disc, (15.0, 15.0)) > 22.0,
            "the diagonal reach is larger"
        );
        let mut stripe = empty_frame(97, 61);
        for y in 20..25u32 {
            for x in 0..97u32 {
                let i = ((y * 97 + x) * 4) as usize;
                stripe.rgba[i..i + 4].copy_from_slice(&[250, 250, 250, 90]);
            }
        }
        let spans = Spans::new(stripe);
        let ops = vec![
            Op::Blit {
                src: &layer,
                at: (10, 5),
                part: (0, 0, 50, 20),
                clip: Some((15, 0, 30, 100)),
                alpha: 180,
            },
            Op::Turn {
                src: &needle,
                pivot_image: (1.5, 25.0),
                pivot_screen: (48.0, 40.0),
                degrees: 33.0,
                smooth: true,
                reach: reach_of(&needle, (1.5, 25.0)),
            },
            Op::Turn {
                src: &layer,
                pivot_image: (25.0, 10.0),
                pivot_screen: (30.0, 30.0),
                degrees: -70.0,
                smooth: false,
                reach: reach_of(&layer, (25.0, 10.0)),
            },
            Op::Turn {
                src: &disc,
                pivot_image: (15.0, 15.0),
                pivot_screen: (75.0, 45.0),
                degrees: 45.0,
                smooth: false,
                reach: Reaches::default().reach(&disc, (15.0, 15.0)),
            },
            Op::Ring {
                cx: 60,
                cy: 30,
                r: 12,
                thickness: 3,
                color: [9, 9, 200, 255],
            },
            Op::Border {
                rect: (2, 2, 20, 20),
                thickness: 2,
                color: [1, 2, 3, 255],
            },
            Op::RoundRect {
                x: 70,
                y: 40,
                w: 20,
                h: 15,
                radius: 4,
                color: [200, 200, 0, 128],
            },
            Op::Arc {
                x: 5,
                y: 35,
                w: 24,
                h: 24,
                start: 30.0,
                stop: 300.0,
                ring: 5,
                color: [0, 255, 255],
            },
            Op::Column {
                rect: Rect {
                    x: 90,
                    y: 5,
                    w: 4,
                    h: 50,
                },
                level: 0.6,
                color: METER,
            },
            Op::Front {
                spans: &spans,
                at: (0, 0),
            },
            Op::Fade {
                color: [0, 0, 0],
                alpha: 60,
            },
        ];
        let all = [Rect {
            x: 0,
            y: 0,
            w: 97,
            h: 61,
        }];
        let mut one = Frame {
            blend: Blend::Normal,
            width: 97,
            height: 61,
            rgba: vec![0; 97 * 61 * 4],
        };
        paint(&mut one, &base, &ops, &all, 1);
        assert_ne!(one.rgba, base.rgba, "something was painted");
        for threads in [2usize, 3, 7] {
            let mut many = Frame {
                blend: Blend::Normal,
                width: 97,
                height: 61,
                rgba: vec![0; 97 * 61 * 4],
            };
            paint(&mut many, &base, &ops, &all, threads);
            assert!(many == one, "{threads} threads paint the same frame");
        }
        // Every step stays inside the box it declares.
        for (i, op) in ops.iter().enumerate() {
            let mut alone = Frame {
                blend: Blend::Normal,
                width: 97,
                height: 61,
                rgba: vec![0; 97 * 61 * 4],
            };
            paint(&mut alone, &base, std::slice::from_ref(op), &all, 1);
            let bounds = op.bounds(97, 61).expect("the step paints something");
            for y in 0..61i32 {
                for x in 0..97i32 {
                    let i4 = ((y * 97 + x) * 4) as usize;
                    if alone.rgba[i4..i4 + 4] != base.rgba[i4..i4 + 4] {
                        assert!(
                            x >= bounds.0 && x < bounds.2 && y >= bounds.1 && y < bounds.3,
                            "step {i} painted ({x}, {y}) outside {bounds:?}"
                        );
                    }
                }
            }
        }
        // Painting two boxes touches nothing outside them.
        let mut boxed = Frame {
            blend: Blend::Normal,
            width: 97,
            height: 61,
            rgba: vec![7; 97 * 61 * 4],
        };
        let rects = [
            Rect {
                x: 10,
                y: 5,
                w: 30,
                h: 20,
            },
            Rect {
                x: 50,
                y: 30,
                w: 40,
                h: 25,
            },
        ];
        paint(&mut boxed, &base, &ops, &rects, 3);
        for y in 0..61u32 {
            for x in 0..97u32 {
                let i4 = ((y * 97 + x) * 4) as usize;
                let inside = rects
                    .iter()
                    .any(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h);
                if inside {
                    assert_eq!(
                        boxed.rgba[i4..i4 + 4],
                        one.rgba[i4..i4 + 4],
                        "({x}, {y}) inside a box is painted as the whole frame is"
                    );
                } else {
                    assert_eq!(
                        boxed.rgba[i4..i4 + 4],
                        [7, 7, 7, 7],
                        "({x}, {y}) outside the boxes is untouched"
                    );
                }
            }
        }
        // Through the raster too: texts in the bitmap font, meters, and a fade.
        let scene = Scene {
            skin: "test".into(),
            width: 200,
            height: 90,
            left: 0.7,
            right: 0.3,
            bars: vec![0.2, 0.9, 0.5],
            texts: vec![Text {
                x: 60,
                y: 10,
                style: TextStyle::Regular,
                size: 16,
                color: [255, 255, 255],
                max_width: 40,
                text: "A long bitmap line".into(),
                align: TextAlign::Left,
                speed: 50.0,
                direction: ScrollDirection::Bounce,
                loop_thirds: false,
                font_file: String::new(),
            }],
            ..Scene::default()
        };
        let mut single = Motion::new(1, None);
        let mut several = Motion::new(3, None);
        for now in [0u64, 250, 700] {
            single.fade.begin_in(0, 2.0, false, 1.0);
            several.fade.begin_in(0, 2.0, false, 1.0);
            let a = raster_over(
                &scene,
                Stack {
                    screen: None,
                    face: None,
                    front: None,
                    needle: None,
                    needle_right: None,
                    face_at: (0, 0),
                    fonts: None,
                    art: None,
                    icon: None,
                    spectra: &[],
                    folder_pictures: &[],
                    fanart: (None, None),
                    vinyl: None,
                    tonearm: None,
                    reels: (None, None),
                    indicators: None,
                    base: None,
                },
                &mut single,
                now,
            )
            .frame
            .clone();
            let b = raster_over(
                &scene,
                Stack {
                    screen: None,
                    face: None,
                    front: None,
                    needle: None,
                    needle_right: None,
                    face_at: (0, 0),
                    fonts: None,
                    art: None,
                    icon: None,
                    spectra: &[],
                    folder_pictures: &[],
                    fanart: (None, None),
                    vinyl: None,
                    tonearm: None,
                    reels: (None, None),
                    indicators: None,
                    base: None,
                },
                &mut several,
                now,
            )
            .frame
            .clone();
            assert!(
                a == b,
                "frame at {now} ms is the same on one and three threads"
            );
        }
    }

    #[test]
    fn painting_what_changed_matches_painting_everything() {
        // A moving scene rastered twice: one motion paints only the boxes that
        // changed since its last frame, the other paints every frame whole.
        let needle = sprite(3, 40, [255, 0, 0, 255]);
        let mut art_a = sprite(60, 60, [0, 120, 255, 255]);
        art_a.rgba[0] = 1;
        let art_b = sprite(60, 60, [255, 120, 0, 255]);
        let mut scene = Scene {
            skin: "test".into(),
            width: 240,
            height: 120,
            left: 0.2,
            right: 0.6,
            left_at: Some((60, 90)),
            right_at: Some((180, 90)),
            needle: Some((-40.0, 40.0, 20.0)),
            meter: MeterSpec {
                visible: true,
                channels: 2,
                ..MeterSpec::default()
            },
            art: Some(Art {
                x: 90,
                y: 30,
                w: 60,
                h: 60,
                file: String::new(),
                mask: String::new(),
                border: 2,
                border_color: [255, 255, 255],
                rotation: true,
                rpm: 45.0,
            }),
            playing: true,
            texts: vec![Text {
                x: 10,
                y: 5,
                style: TextStyle::Regular,
                size: 16,
                color: [255, 255, 255],
                max_width: 60,
                text: "A long line of bitmap text".into(),
                align: TextAlign::Left,
                speed: 80.0,
                direction: ScrollDirection::Bounce,
                loop_thirds: false,
                font_file: String::new(),
            }],
            ..Scene::default()
        };
        let mut changed = Motion::new(2, None);
        let mut everything = Motion::new(1, None);
        everything.paint_all = true;
        changed.fade.begin_in(0, 0.5, false, 1.0);
        everything.fade.begin_in(0, 0.5, false, 1.0);
        // One base for every frame, as the display keeps one per meter: a
        // base composed per call would have a new address whenever the
        // allocator felt like it, and a new base repaints the whole frame.
        let base = compose_base(240, 120, None, None, (0, 0));
        let mut art: &Frame = &art_a;
        let mut painted_boxes = 0usize;
        for step in 0..30u64 {
            let now = step * 100;
            if step == 12 {
                scene.left = 0.9;
                scene.texts[0].text = "Another".into();
            }
            if step == 20 {
                art = &art_b;
            }
            let stack = || Stack {
                screen: None,
                face: None,
                front: None,
                needle: Some(&needle),
                needle_right: None,
                face_at: (0, 0),
                fonts: None,
                art: Some(art),
                icon: None,
                spectra: &[],
                folder_pictures: &[],
                fanart: (None, None),
                vinyl: None,
                tonearm: None,
                reels: (None, None),
                indicators: None,
                base: Some(&base),
            };
            let a = raster_over(&scene, stack(), &mut changed, now)
                .frame
                .clone();
            let b = raster_over(&scene, stack(), &mut everything, now)
                .frame
                .clone();
            assert!(
                a == b,
                "frame at {now} ms painted by boxes equals the whole repaint"
            );
            painted_boxes += changed
                .damage()
                .iter()
                .map(|r| (r.w * r.h) as usize)
                .sum::<usize>();
        }
        let whole = 240 * 120 * 30;
        assert!(
            painted_boxes < whole / 2,
            "boxes painted {painted_boxes} of {whole} pixels"
        );
    }

    #[test]
    fn a_turned_needle_lands_where_the_theme_origin_sends_it() {
        // Pivot at (50, 80), sprite centre 30 px out. Straight up, the sprite
        // covers y 40..60 at x 50. Turned 90° left, it covers x 10..30 at y 80.
        let needle = sprite(2, 20, [255, 0, 0, 255]);
        let mut rgba = vec![0u8; 100 * 100 * 4];
        blit_rotated(
            &mut Band::over(&mut rgba, 100, 100),
            &needle,
            (50, 80),
            0.0,
            30.0,
        );
        assert_eq!(at(&rgba, 100, 50, 45), [255, 0, 0]);
        assert_eq!(at(&rgba, 100, 25, 80), [0, 0, 0]);
        let mut rgba = vec![0u8; 100 * 100 * 4];
        blit_rotated(
            &mut Band::over(&mut rgba, 100, 100),
            &needle,
            (50, 80),
            90.0,
            30.0,
        );
        assert_eq!(at(&rgba, 100, 25, 80), [255, 0, 0]);
        assert_eq!(at(&rgba, 100, 50, 45), [0, 0, 0]);
    }

    /// Any TrueType file on the host will do; the test is about placement and
    /// clipping, not the face. Without one the bitmap fallback is exercised.
    fn any_font() -> Option<String> {
        let dirs = [
            "/usr/share/fonts/truetype",
            "/usr/share/fonts/TTF",
            "/usr/share/fonts",
        ];
        fn walk(dir: &Path, depth: u32) -> Option<String> {
            for entry in std::fs::read_dir(dir).ok()?.flatten() {
                let path = entry.path();
                if path.is_dir() && depth > 0 {
                    if let Some(found) = walk(&path, depth - 1) {
                        return Some(found);
                    }
                } else if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("ttf"))
                {
                    return Some(path.to_string_lossy().into_owned());
                }
            }
            None
        }
        dirs.iter().find_map(|d| walk(Path::new(d), 3))
    }

    #[test]
    fn theme_text_draws_below_its_top_and_clips_at_max_width() {
        let Some(file) = any_font() else {
            println!("no TrueType font on this host; fallback path only");
            return;
        };
        let fonts = Fonts::load(&FontFiles {
            regular: file,
            ..FontFiles::default()
        });
        assert_eq!(fonts.loaded(), 1);
        let mut frame = Frame {
            blend: Blend::Normal,
            width: 120,
            height: 40,
            rgba: vec![0u8; 120 * 40 * 4],
        };
        let text = Text {
            x: 4,
            y: 2,
            style: TextStyle::Regular,
            size: 24,
            color: [255, 200, 0],
            max_width: 0,
            text: "HHHH".into(),
            align: TextAlign::Left,
            speed: 0.0,
            direction: ScrollDirection::Bounce,
            loop_thirds: false,
            font_file: String::new(),
        };
        draw_text_styled(&mut frame, &text, Some(&fonts));
        let lit = |frame: &Frame, x0: u32, x1: u32| -> usize {
            (0..frame.height)
                .flat_map(|y| (x0..x1).map(move |x| (x, y)))
                .filter(|&(x, y)| sample(frame, x, y)[0] > 0)
                .count()
        };
        assert!(lit(&frame, 4, 120) > 0, "glyphs were drawn");
        assert_eq!(lit(&frame, 0, 4), 0, "nothing left of x");
        assert_eq!(
            (0..120).filter(|&x| sample(&frame, x, 0)[0] > 0).count(),
            0,
            "row 0 above the em box is empty"
        );

        let mut clipped = Frame {
            blend: Blend::Normal,
            width: 120,
            height: 40,
            rgba: vec![0u8; 120 * 40 * 4],
        };
        let narrow = Text {
            max_width: 10,
            ..text
        };
        draw_text_styled(&mut clipped, &narrow, Some(&fonts));
        assert!(lit(&clipped, 4, 14) > 0);
        assert_eq!(lit(&clipped, 14, 120), 0, "nothing past max_width");
    }

    #[test]
    fn a_wide_text_bounces_and_a_ticker_loops() {
        let Some(file) = any_font() else {
            println!("no TrueType font on this host; fallback path only");
            return;
        };
        let fonts = Fonts::load(&FontFiles {
            regular: file,
            ..FontFiles::default()
        });
        let mut motion = TextMotion::default();
        let mut frame = Frame {
            blend: Blend::Normal,
            width: 200,
            height: 40,
            rgba: vec![0u8; 200 * 40 * 4],
        };
        let mut text = Text {
            x: 10,
            y: 2,
            style: TextStyle::Regular,
            size: 20,
            color: [255, 255, 255],
            max_width: 30,
            text: "A very long line indeed".into(),
            align: TextAlign::Left,
            speed: 100.0,
            direction: ScrollDirection::Bounce,
            loop_thirds: false,
            font_file: String::new(),
        };
        draw_text_moving(&mut frame, &text, Some(&fonts), &mut motion, 0);
        assert_eq!(motion.offset(10, 2), Some(0.0), "starts at the left end");
        draw_text_moving(&mut frame, &text, Some(&fonts), &mut motion, 100);
        assert_eq!(
            motion.offset(10, 2),
            Some(0.0),
            "pauses at the end it starts from, as the player does"
        );
        draw_text_moving(&mut frame, &text, Some(&fonts), &mut motion, 500);
        let after = motion.offset(10, 2).unwrap();
        assert!(
            after > 39.0 && after < 41.0,
            "100 px/s over the 0.4 s since the last frame: {after}"
        );
        for step in 2..200u64 {
            draw_text_moving(&mut frame, &text, Some(&fonts), &mut motion, step * 100);
        }
        let line_w = render_text(
            Some(&fonts),
            TextStyle::Regular,
            20,
            [255, 255, 255],
            &text.text,
            0,
        )
        .unwrap()
        .width;
        let limit = (line_w - 30) as f32;
        let at_end = motion.offset(10, 2).unwrap();
        assert!(
            at_end >= 0.0 && at_end <= limit,
            "bounces inside 0..limit: {at_end} of {limit}"
        );
        assert_eq!(
            (0..10).filter(|&x| sample(&frame, x, 10)[3] > 0).count(),
            0,
            "nothing left of the box"
        );
        assert_eq!(
            (41..200).filter(|&x| sample(&frame, x, 10)[3] > 0).count(),
            0,
            "nothing right of the box"
        );

        text.text = "loop  ".repeat(3);
        text.direction = ScrollDirection::Ltr;
        text.loop_thirds = true;
        text.max_width = 20;
        draw_text_moving(&mut frame, &text, Some(&fonts), &mut motion, 20_000);
        let segment = render_text(
            Some(&fonts),
            TextStyle::Regular,
            20,
            [255, 255, 255],
            &text.text,
            0,
        )
        .unwrap()
        .width
            / 3;
        for step in 1..400u64 {
            draw_text_moving(
                &mut frame,
                &text,
                Some(&fonts),
                &mut motion,
                20_000 + step * 50,
            );
        }
        let looped = motion.offset(10, 2).unwrap();
        assert!(
            looped >= 0.0 && looped < segment as f32,
            "wraps by one segment: {looped} of {segment}"
        );
    }

    #[test]
    fn an_svg_icon_is_fitted_and_tinted() {
        let dir = std::env::temp_dir().join(format!("glass-svg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wide.svg");
        std::fs::write(
            &path,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><rect x="0" y="0" width="200" height="100" fill="#000"/></svg>"##,
        )
        .unwrap();
        let icon = read_icon(&path, 50, 50, Some([204, 176, 97])).unwrap();
        assert_eq!(
            (icon.width, icon.height),
            (50, 25),
            "fits the box, keeps the aspect"
        );
        assert_eq!(sample(&icon, 25, 12), [204, 176, 97, 255], "tinted, opaque");
        let untinted = read_icon(&path, 50, 50, None).unwrap();
        assert_eq!(sample(&untinted, 25, 12), [0, 0, 0, 255]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_mask_cuts_the_white_and_a_border_frames_the_box() {
        let mut art = sprite(4, 4, [10, 20, 30, 255]);
        let mut mask = sprite(4, 4, [0, 0, 0, 255]);
        for x in 2..4u32 {
            for y in 0..4u32 {
                let i = ((y * 4 + x) * 4) as usize;
                mask.rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        apply_mask(&mut art, &mask);
        assert_eq!(
            sample(&art, 0, 0)[3],
            255,
            "black in the mask keeps the picture"
        );
        assert_eq!(sample(&art, 3, 3)[3], 0, "white in the mask cuts it");

        let mut rgba = vec![0u8; 8 * 8 * 4];
        draw_border(
            &mut Band::over(&mut rgba, 8, 8),
            (2, 2, 4, 4),
            1,
            [9, 9, 9, 255],
        );
        assert_eq!(at(&rgba, 8, 2, 2), [9, 9, 9], "corner is border");
        assert_eq!(at(&rgba, 8, 5, 3), [9, 9, 9], "right edge is border");
        assert_eq!(at(&rgba, 8, 3, 3), [0, 0, 0], "inside stays");
        assert_eq!(at(&rgba, 8, 1, 1), [0, 0, 0], "outside stays");
    }

    #[test]
    fn the_type_area_aligns_inside_its_box() {
        let icon = sprite(10, 6, [1, 2, 3, 255]);
        let area = TypeArea {
            x: 20,
            y: 10,
            box_size: Some((40, 20)),
            mode: TypeMode::Icon,
            align: TypeAlign::Right,
            color: [1, 2, 3],
            font_size: 12,
            font_style: TextStyle::Regular,
            label: "FLAC".into(),
            icon: "x.svg".into(),
        };
        let mut frame = Frame {
            blend: Blend::Normal,
            width: 80,
            height: 40,
            rgba: vec![0u8; 80 * 40 * 4],
        };
        draw_type_area(&mut frame, &area, Some(&icon), None);
        assert_eq!(
            sample(&frame, 59, 17)[..3],
            [1, 2, 3],
            "right-aligned, vertically centred"
        );
        assert_eq!(
            sample(&frame, 20, 17)[3],
            0,
            "nothing on the left of the box"
        );
    }

    #[test]
    fn art_is_stretched_to_its_box() {
        let small = sprite(2, 2, [10, 20, 30, 255]);
        let fitted = fit_art(&small, 6, 4);
        assert_eq!((fitted.width, fitted.height), (6, 4));
        assert_eq!(sample(&fitted, 3, 2), [10, 20, 30, 255]);
        assert_eq!(fit_art(&small, 2, 2), small);
    }

    #[test]
    fn a_half_transparent_layer_blends_over_the_frame() {
        let mut rgba = vec![0u8; 4 * 4 * 4];
        for px in rgba.as_chunks_mut::<4>().0 {
            px.copy_from_slice(&[0, 0, 0, 255]);
        }
        let layer = sprite(1, 1, [200, 100, 0, 128]);
        blit_op(&layer, (1, 1)).paint(&mut Band::over(&mut rgba, 4, 4));
        assert_eq!(at(&rgba, 4, 1, 1), [100, 50, 0]);
    }
}

#[cfg(test)]
mod analyser_tests {
    use super::*;
    use lead::{Layout, Look};

    fn analyser(look: Look, w: u32, h: u32, left: Vec<f32>, right: Vec<f32>) -> plot::Analyser {
        let n = left.len();
        let stereo = left != right;
        plot::Analyser {
            look,
            x: 0,
            y: 0,
            w,
            h,
            edges: vec![(20.0, 20.0); n],
            levels: [left, right],
            hold: [vec![1.0; n], vec![1.0; n]],
            stereo,
            onsets: 0,
        }
    }

    fn pixel(frame: &Frame, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * frame.width + x) * 4) as usize;
        [
            frame.rgba[at],
            frame.rgba[at + 1],
            frame.rgba[at + 2],
            frame.rgba[at + 3],
        ]
    }

    #[test]
    fn bars_rise_from_the_base_in_the_palette_over_the_background_at_its_alpha() {
        let look = Look {
            smoothing: 0.0,
            peaks: false,
            bar_space: 0.0,
            ..Look::default()
        };
        let a = analyser(
            look,
            100,
            50,
            vec![1.0, 0.5, 0.0, 0.25],
            vec![1.0, 0.5, 0.0, 0.25],
        );
        let mut motion = AnalyserMotion::default();
        let picture = motion.advance(&a, 0);
        assert_eq!((picture.width, picture.height), (100, 50));
        // The first bar is full: coloured at the top and the bottom.
        let top = pixel(picture, 12, 2);
        let bottom = pixel(picture, 12, 47);
        assert!(
            top[3] == 255 && top[0] > 150,
            "a full bar's top is the palette's end: {top:?}"
        );
        assert!(
            bottom[3] == 255 && bottom[1] > 150,
            "its base the palette's start: {bottom:?}"
        );
        // The third bar is empty: the background at 70 percent black.
        let empty = pixel(picture, 62, 47);
        assert_eq!(empty[..3], [0, 0, 0]);
        assert!(
            (empty[3] as i32 - 179).abs() <= 2,
            "the background's alpha: {}",
            empty[3]
        );
        // The second bar reaches half way.
        let half = pixel(picture, 37, 40);
        assert!(
            half[3] == 255 && half[..3] != [0, 0, 0],
            "half way up: {half:?}"
        );
        assert_eq!(pixel(picture, 37, 10)[..3], [0, 0, 0]);
        assert_eq!(motion.bytes(), 100 * 50 * 4, "one picture after one frame");
        motion.advance(&a, 16);
        assert_eq!(
            motion.bytes(),
            100 * 50 * 4 * 2,
            "two pictures kept, one per frame in turn"
        );
    }

    #[test]
    fn a_peak_holds_then_falls_with_gravity() {
        let look = Look {
            smoothing: 0.0,
            bar_space: 0.0,
            peak_hold_ms: 200,
            gravity: 1.0,
            ..Look::default()
        };
        let loud = analyser(look.clone(), 10, 100, vec![1.0], vec![1.0]);
        let quiet = analyser(look, 10, 100, vec![0.0], vec![0.0]);
        let mut motion = AnalyserMotion::default();
        motion.advance(&loud, 0);
        // Held: the mark stays at the top while the bar is gone.
        let picture = motion.advance(&quiet, 100);
        assert!(
            pixel(picture, 5, 1)[3] == 255,
            "the peak mark at the top while held"
        );
        assert_eq!(pixel(picture, 5, 50)[..3], [0, 0, 0], "no bar under it");
        // After the hold it falls: a tenth of a second on, a few pixels
        // down and still there; a second on, it has reached the base and
        // rests on the empty bar, out of sight.
        let mut t = 200;
        while t <= 300 {
            motion.advance(&quiet, t);
            t += 20;
        }
        let picture = motion.advance(&quiet, 320);
        assert_eq!(
            pixel(picture, 5, 1)[..3],
            [0, 0, 0],
            "the mark has left the top"
        );
        let mut found = None;
        for y in 2..100 {
            if pixel(picture, 5, y)[3] == 255 && pixel(picture, 5, y)[..3] != [0, 0, 0] {
                found = Some(y);
                break;
            }
        }
        let y = found.expect("the mark on its way down");
        assert!((3..60).contains(&y), "some pixels down: {y}");
        while t <= 1500 {
            motion.advance(&quiet, t);
            t += 20;
        }
        let picture = motion.advance(&quiet, 1520);
        assert!(
            (0..100).all(|y| pixel(picture, 5, y)[..3] == [0, 0, 0]),
            "the mark rests on the empty bar"
        );
    }

    /// The worst of the reference theme's looks, 256 bands of two channels
    /// as full-height luminance bars in a 1200 by 420 box: a frame must
    /// cost a few milliseconds, not tens, or a Pi cannot keep 60 a second.
    #[test]
    fn a_dense_luminance_frame_costs_milliseconds() {
        let look = Look {
            lumi: true,
            bar_space: 0.0,
            layout: Layout::DualHorizontal,
            smoothing: 0.6,
            peaks: false,
            ..Look::default()
        };
        let left: Vec<f32> = (0..256).map(|i| ((i * 37) % 100) as f32 / 100.0).collect();
        let right: Vec<f32> = (0..256).map(|i| ((i * 53) % 100) as f32 / 100.0).collect();
        let a = analyser(look, 1200, 420, left, right);
        let mut motion = AnalyserMotion::default();
        motion.advance(&a, 0);
        let started = std::time::Instant::now();
        let frames = 60;
        for i in 1..=frames {
            motion.advance(&a, i * 16);
        }
        let per_frame = started.elapsed().as_secs_f64() * 1000.0 / frames as f64;
        eprintln!("a dense luminance frame: {per_frame:.2} ms");
        let bound = if cfg!(debug_assertions) { 100.0 } else { 8.0 };
        assert!(per_frame < bound, "a frame took {per_frame:.1} ms");
    }

    /// Outlined and rounded bars are drawn row by row only where the
    /// corners and the end lines are; the body is rectangles. A frame of
    /// outlined bars, the dearest look, must cost what the luminance one does.
    #[test]
    fn a_dense_outlined_frame_costs_milliseconds() {
        let look = Look {
            outline: true,
            round: true,
            line_width: 1.5,
            fill_alpha: 0.15,
            bar_space: 0.35,
            layout: Layout::DualCombined,
            color_mode: lead::ColorMode::Index,
            smoothing: 0.6,
            peaks: true,
            ..Look::default()
        };
        let left: Vec<f32> = (0..96).map(|i| ((i * 37) % 100) as f32 / 100.0).collect();
        let right: Vec<f32> = (0..96).map(|i| ((i * 53) % 100) as f32 / 100.0).collect();
        let a = analyser(look, 1200, 420, left, right);
        let mut motion = AnalyserMotion::default();
        motion.advance(&a, 0);
        let started = std::time::Instant::now();
        let frames = 60;
        for i in 1..=frames {
            motion.advance(&a, i * 16);
        }
        let per_frame = started.elapsed().as_secs_f64() * 1000.0 / frames as f64;
        eprintln!("a dense outlined frame: {per_frame:.2} ms");
        let bound = if cfg!(debug_assertions) { 100.0 } else { 8.0 };
        assert!(per_frame < bound, "a frame took {per_frame:.1} ms");
    }

    /// Four bands round a circle: the first band's sector, clockwise from
    /// the top, is filled from the base circle out; an empty band's is not;
    /// the centre and the corners stay background. A quarter turn a second
    /// of spin moves the filled sector on; inverted, the bars grow inward.
    #[test]
    fn a_radial_look_draws_bars_out_from_the_base_circle_and_spins() {
        let look = Look {
            radial: true,
            radius: 0.3,
            spin_rpm: 15.0,
            smoothing: 0.0,
            peaks: false,
            bar_space: 0.0,
            bgr_alpha: 1.0,
            color_mode: lead::ColorMode::Index,
            ..Look::default()
        };
        let a = analyser(
            look.clone(),
            100,
            100,
            vec![1.0, 0.0, 1.0, 0.0],
            vec![1.0, 0.0, 1.0, 0.0],
        );
        let mut motion = AnalyserMotion::default();
        let picture = motion.advance(&a, 0);
        let lit = |p: [u8; 4]| p[0] as u32 + p[1] as u32 + p[2] as u32 > 100;
        // Band 0 spans the top right quadrant; its middle at 45 degrees.
        assert!(lit(pixel(picture, 74, 25)), "band 0 filled at r 35");
        assert!(!lit(pixel(picture, 74, 74)), "band 1 empty");
        assert!(
            !lit(pixel(picture, 50, 50)),
            "the centre inside the base circle"
        );
        assert!(
            !lit(pixel(picture, 57, 43)),
            "r 10, inside the base circle of 14.7"
        );
        assert!(!lit(pixel(picture, 2, 2)), "the corner outside the circle");
        // A second on at 15 rpm: a quarter turn clockwise, band 0 now at the bottom right.
        let picture = motion.advance(&a, 1000);
        assert!(lit(pixel(picture, 74, 74)), "band 0 turned on by a quarter");
        assert!(
            !lit(pixel(picture, 74, 25)),
            "band 3, empty, took its place"
        );
        let inverted = Look {
            radial_invert: true,
            spin_rpm: 0.0,
            ..look
        };
        let a = analyser(
            inverted,
            100,
            100,
            vec![0.5, 0.0, 0.5, 0.0],
            vec![0.5, 0.0, 0.5, 0.0],
        );
        let mut motion = AnalyserMotion::default();
        let picture = motion.advance(&a, 0);
        // Half a bar from the rim (49) inward over 34 pixels reaches r 32.
        assert!(lit(pixel(picture, 79, 20)), "r 42, filled from the rim");
        assert!(!lit(pixel(picture, 67, 32)), "r 25, short of the bar's tip");
    }

    /// Two bands as a graph: full at the first band's centre, empty at the
    /// second's, half way between, the area filled under the line; with a
    /// line and no fill, only the line's pixels are lit.
    #[test]
    fn a_graph_joins_the_band_tips_and_fills_under_them() {
        let look = Look {
            style: lead::LookStyle::Graph,
            smoothing: 0.0,
            peaks: false,
            bgr_alpha: 1.0,
            color_mode: lead::ColorMode::Index,
            ..Look::default()
        };
        let a = analyser(look.clone(), 100, 50, vec![1.0, 0.0], vec![1.0, 0.0]);
        let mut motion = AnalyserMotion::default();
        let picture = motion.advance(&a, 0);
        let lit = |p: [u8; 4]| p[0] as u32 + p[1] as u32 + p[2] as u32 > 100;
        assert!(
            lit(pixel(picture, 25, 2)) && lit(pixel(picture, 25, 47)),
            "full at band 0's centre"
        );
        assert!(!lit(pixel(picture, 75, 47)), "empty at band 1's centre");
        assert!(
            lit(pixel(picture, 50, 40)),
            "half way between: the lower half filled"
        );
        assert!(!lit(pixel(picture, 50, 10)), "and the upper half not");
        let lined = Look {
            line_width: 3.0,
            fill_alpha: 0.0,
            ..look
        };
        let a = analyser(lined, 100, 50, vec![1.0, 0.0], vec![1.0, 0.0]);
        let mut motion = AnalyserMotion::default();
        let picture = motion.advance(&a, 0);
        assert!(
            lit(pixel(picture, 50, 25)),
            "the line half way up at the middle"
        );
        assert!(
            !lit(pixel(picture, 50, 40)),
            "nothing under it without a fill"
        );
    }

    /// With the frequency scale on, the bars end above a strip that holds
    /// the labels, a label under the band edge at 100 Hz; with the level
    /// scale on, a label sits at the left near the top for 0 dB.
    #[test]
    fn the_scales_sit_under_and_beside_the_bars() {
        let look = Look {
            scale_x: true,
            scale_y: true,
            smoothing: 0.0,
            peaks: false,
            bar_space: 0.0,
            bgr_alpha: 1.0,
            color_mode: lead::ColorMode::Index,
            ..Look::default()
        };
        let mut a = analyser(look, 300, 80, vec![1.0, 1.0, 1.0], vec![1.0, 1.0, 1.0]);
        a.edges = vec![(20.0, 100.0), (100.0, 1000.0), (1000.0, 10_000.0)];
        let mut motion = AnalyserMotion::default();
        let picture = motion.advance(&a, 0);
        let lit = |p: [u8; 4]| p[0] as u32 + p[1] as u32 + p[2] as u32 > 100;
        // The strip is 17 rows: the bars stop at row 62.
        assert!(
            lit(pixel(picture, 150, 60)),
            "a full bar just above the strip"
        );
        assert!(
            !lit(pixel(picture, 60, 75)),
            "the strip between labels is background"
        );
        // The 100 Hz label is centred on x 100: some ink near it in the strip.
        let inked = (90..111).any(|x| (64..80).any(|y| lit(pixel(picture, x, y))));
        assert!(inked, "the 100 Hz label in the strip");
        // The 0 dB label at the left near the top of the bars.
        let db0 = (0..30).any(|x| {
            (0..12).any(|y| {
                let p = pixel(picture, x, y);
                // The label's grey over a bar's colour reads as a change from the bar.
                p != pixel(picture, 60, y)
            })
        });
        assert!(db0, "the 0 dB label at the top left");
    }

    /// An onset in the bass group: with `flash` the bass band brightens
    /// and the high band does not, and the brightness is gone once the
    /// decay has passed; with `pulse` every bar grows for the moment;
    /// with `ring` a line crosses the box half way through the decay.
    #[test]
    fn an_onset_flashes_its_group_pulses_every_bar_or_sends_a_ring() {
        let base = Look {
            onset: lead::OnsetLook::Flash,
            onset_strength: 1.0,
            onset_decay_ms: 200,
            smoothing: 0.0,
            peaks: false,
            bar_space: 0.0,
            bgr_alpha: 1.0,
            color_mode: lead::ColorMode::Index,
            palette: lead::Palette::parse("#204080").expect("one stop"),
            ..Look::default()
        };
        let sum = |p: [u8; 4]| p[0] as u32 + p[1] as u32 + p[2] as u32;
        let make = |look: Look, onsets: u8| {
            let mut a = analyser(look, 100, 50, vec![0.5, 0.5], vec![0.5, 0.5]);
            a.edges = vec![(60.0, 250.0), (2000.0, 8000.0)];
            a.onsets = onsets;
            a
        };
        // Flash: the bass bar (left) brighter than the high bar (right).
        let mut motion = AnalyserMotion::default();
        let picture = motion.advance(&make(base.clone(), 0b0010), 0);
        let (bass, high) = (pixel(picture, 25, 40), pixel(picture, 75, 40));
        assert!(
            sum(bass) > sum(high) + 60,
            "bass flashed: {bass:?} against {high:?}"
        );
        let picture = motion.advance(&make(base.clone(), 0), 400);
        assert_eq!(pixel(picture, 25, 40), pixel(picture, 75, 40), "decayed");
        // Pulse: the bars grow by three tenths of the strength: 0.5 becomes 0.65.
        let pulse = Look {
            onset: lead::OnsetLook::Pulse,
            ..base.clone()
        };
        let mut motion = AnalyserMotion::default();
        let picture = motion.advance(&make(pulse, 0b0010), 0);
        assert!(
            sum(pixel(picture, 75, 20)) > 60,
            "a bar at 0.65 covers row 20 of 50"
        );
        let picture = motion.advance(&make(base.clone(), 0), 0);
        assert_eq!(sum(pixel(picture, 75, 20)), 0, "at 0.5 it does not");
        // Ring: half way through the decay a line crosses the box at half height.
        let ring = Look {
            onset: lead::OnsetLook::Ring,
            ..base
        };
        let mut motion = AnalyserMotion::default();
        motion.advance(&make(ring.clone(), 0b0010), 0);
        let picture = motion.advance(&make(ring, 0), 100);
        assert!(
            sum(pixel(picture, 75, 25)) > 200,
            "the ring at half height: {:?}",
            pixel(picture, 75, 25)
        );
        assert!(sum(pixel(picture, 75, 10)) < 60, "and not above it");
    }

    /// Dots: a disc at the band's level and nothing above or below it;
    /// waterfall: a frame's levels as a row at the base, moving up a row
    /// a frame as new rows come.
    #[test]
    fn an_additive_box_blooms_where_its_channels_overlap_and_a_held_dot_sits_at_the_peak() {
        // Two channels over each other (the right a shade lower, so the
        // analyser is stereo), the second at half alpha: covering, the
        // overlap stays at the colour's level; adding, it goes past it.
        let combined = |add: bool| Look {
            layout: Layout::DualCombined,
            smoothing: 0.0,
            peaks: false,
            bar_space: 0.0,
            bgr_alpha: 1.0,
            color_mode: lead::ColorMode::Index,
            blend_add: add,
            ..Look::default()
        };
        let sum = |p: [u8; 4]| p[0] as u32 + p[1] as u32 + p[2] as u32;
        let a = analyser(combined(false), 20, 40, vec![1.0], vec![0.99]);
        let mut motion = AnalyserMotion::default();
        let covered = sum(pixel(motion.advance(&a, 0), 10, 20));
        let a = analyser(combined(true), 20, 40, vec![1.0], vec![0.99]);
        let mut motion = AnalyserMotion::default();
        let added = sum(pixel(motion.advance(&a, 0), 10, 20));
        assert!(
            added > covered + 60,
            "added {added} against covered {covered}"
        );
        // A dot held at the peak: the disc at the peak's height, none at the level's.
        let look = Look {
            style: lead::LookStyle::Dots,
            smoothing: 0.0,
            peaks: true,
            peak_hold_ms: 10_000,
            dot_size: 6.0,
            dot_hold: true,
            bgr_alpha: 1.0,
            color_mode: lead::ColorMode::Index,
            ..Look::default()
        };
        let a = analyser(look, 20, 100, vec![0.9], vec![0.9]);
        let mut motion = AnalyserMotion::default();
        motion.advance(&a, 0);
        // The level drops; the peak holds where it was.
        let a = analyser(a.look.clone(), 20, 100, vec![0.2], vec![0.2]);
        let picture = motion.advance(&a, 100);
        let lit = |p: [u8; 4]| sum(p) > 100;
        assert!(
            lit(pixel(picture, 10, 10)),
            "the dot at the held peak, y of 0.9"
        );
        assert!(
            !lit(pixel(picture, 10, 80)),
            "nothing at the level, y of 0.2"
        );
    }

    #[test]
    fn dots_sit_at_their_levels_and_a_waterfall_moves_its_rows_away() {
        let dots = Look {
            style: lead::LookStyle::Dots,
            dot_size: 10.0,
            smoothing: 0.0,
            peaks: false,
            bgr_alpha: 1.0,
            color_mode: lead::ColorMode::Index,
            ..Look::default()
        };
        let a = analyser(dots, 100, 50, vec![0.5, 0.0], vec![0.5, 0.0]);
        let mut motion = AnalyserMotion::default();
        let picture = motion.advance(&a, 0);
        let lit = |p: [u8; 4]| p[0] as u32 + p[1] as u32 + p[2] as u32 > 100;
        assert!(lit(pixel(picture, 25, 25)), "a dot at band 0's level");
        assert!(
            !lit(pixel(picture, 25, 12)) && !lit(pixel(picture, 25, 38)),
            "and nothing above or below"
        );
        assert!(
            lit(pixel(picture, 75, 47)),
            "band 1's dot at the base, half of it in the box"
        );
        let waterfall = Look {
            style: lead::LookStyle::Waterfall,
            smoothing: 0.0,
            peaks: false,
            bgr_alpha: 1.0,
            color_mode: lead::ColorMode::Index,
            palette: lead::Palette::parse("#ff0000").expect("one stop"),
            ..Look::default()
        };
        let loud = analyser(waterfall.clone(), 100, 50, vec![1.0, 0.0], vec![1.0, 0.0]);
        let quiet = analyser(waterfall, 100, 50, vec![0.0, 0.0], vec![0.0, 0.0]);
        let mut motion = AnalyserMotion::default();
        let picture = motion.advance(&loud, 0);
        assert!(
            lit(pixel(picture, 25, 49)),
            "the loud band's row at the base"
        );
        assert!(
            !lit(pixel(picture, 75, 49)),
            "the quiet band's row is clear"
        );
        motion.advance(&quiet, 16);
        let picture = motion.advance(&quiet, 32);
        assert!(
            !lit(pixel(picture, 25, 49)),
            "two quiet frames on, the base is clear"
        );
        assert!(lit(pixel(picture, 25, 47)), "the loud row has moved up two");
        assert_eq!(
            motion.bytes(),
            100 * 50 * 4 * 3,
            "two pictures and the waterfall's rows"
        );
    }

    /// The effects across styles: a trail keeps a fading wake of the last
    /// frame; a halo widens a bar faintly; a fade dims a bar's base; the
    /// ribbon thickens a graph's line with the level and the glow bands it;
    /// an echo's ghost lags behind the bars; sparkles appear above loud
    /// bars over a few frames.
    #[test]
    fn the_effects_trail_halo_fade_ribbon_glow_echo_and_sparkle() {
        let base = Look {
            smoothing: 0.0,
            peaks: false,
            bar_space: 0.5,
            bgr_alpha: 1.0,
            color_mode: lead::ColorMode::Index,
            palette: lead::Palette::parse("#ffffff").expect("one stop"),
            ..Look::default()
        };
        let sum = |p: [u8; 4]| p[0] as u32 + p[1] as u32 + p[2] as u32;
        // Trail: a bar drawn, then silence; the wake stays, fainter.
        let trail = Look {
            trail: 0.8,
            ..base.clone()
        };
        let loud = analyser(trail.clone(), 100, 50, vec![1.0, 0.0], vec![1.0, 0.0]);
        let quiet = analyser(trail, 100, 50, vec![0.0, 0.0], vec![0.0, 0.0]);
        let mut motion = AnalyserMotion::default();
        motion.advance(&loud, 0);
        let picture = motion.advance(&quiet, 16);
        let wake = pixel(picture, 25, 25);
        assert!(
            sum(wake) > 300 && sum(wake) < 700,
            "the wake, dimmed to eight tenths: {wake:?}"
        );
        // Halo: a pixel beside the bar, in the gap, has some ink.
        let halo = Look {
            bar_glow: 1.5,
            ..base.clone()
        };
        let a = analyser(halo, 100, 50, vec![1.0, 0.0], vec![1.0, 0.0]);
        let picture = AnalyserMotion::default().advance(&a, 0).clone();
        assert!(
            sum(pixel(picture_ref(&picture), 45, 25)) > 60,
            "the halo beside the bar"
        );
        // Fade: the base dimmer than the tip.
        let fade = Look {
            bar_fade: true,
            ..base.clone()
        };
        let a = analyser(fade, 100, 50, vec![1.0, 0.0], vec![1.0, 0.0]);
        let picture = AnalyserMotion::default().advance(&a, 0).clone();
        let (tip, foot) = (pixel(&picture, 25, 3), pixel(&picture, 25, 47));
        assert!(
            sum(foot) < sum(tip) / 2,
            "dim at the base: {foot:?} against {tip:?}"
        );
        // Ribbon and glow on a graph: the line thick at the full band, thin at the empty one.
        let ribbon = Look {
            style: lead::LookStyle::Graph,
            fill_alpha: 0.0,
            line_width: 1.0,
            line_width_max: 12.0,
            line_glow: 20.0,
            ..base.clone()
        };
        let a = analyser(ribbon, 100, 50, vec![0.5, 0.5], vec![0.5, 0.5]);
        let picture = AnalyserMotion::default().advance(&a, 0).clone();
        assert!(
            sum(pixel(&picture, 50, 25)) > 600,
            "the ribbon, six wide, at half height"
        );
        assert!(
            sum(pixel(&picture, 50, 22)) > 600 && sum(pixel(&picture, 50, 27)) > 600,
            "six wide: three rows above and below the middle are the line's"
        );
        assert!(
            sum(pixel(&picture, 50, 18)) < 400,
            "seven rows above it is the glow only: {}",
            sum(pixel(&picture, 50, 18))
        );
        let band = sum(pixel(&picture, 50, 32));
        assert!(band > 60 && band < 400, "the glow band below it: {band}");
        // The reference theme's ribbon: 1.5 to 40 wide with a 30 glow, by
        // level, in a 600 tall box: at half level the line is 21 rows.
        let hanger = Look {
            style: lead::LookStyle::Graph,
            fill_alpha: 0.0,
            line_width: 1.5,
            line_width_max: 40.0,
            line_glow: 30.0,
            color_mode: lead::ColorMode::Level,
            palette: lead::Palette::named("violet").expect("violet"),
            smoothing: 0.0,
            peaks: false,
            bgr_alpha: 1.0,
            ..Look::default()
        };
        let a = analyser(hanger, 100, 600, vec![0.5, 0.5], vec![0.5, 0.5]);
        let picture = AnalyserMotion::default().advance(&a, 0).clone();
        let core: Vec<u32> = (270..330)
            .filter(|&y| {
                pixel(&picture, 50, y)[3] == 255
                    && sum(pixel(&picture, 50, y)) == sum(pixel(&picture, 50, 300))
            })
            .collect();
        assert!(
            core.len() >= 19 && core.len() <= 22,
            "the ribbon's core rows at half level: {} ({:?})",
            core.len(),
            pixel(&picture, 50, 300)
        );
        // Echo: after one frame at rate 0.5 the ghost sits half way.
        let echo = Look {
            echo: 0.5,
            ..base.clone()
        };
        let quiet = analyser(echo.clone(), 100, 50, vec![0.0, 0.0], vec![0.0, 0.0]);
        let loud = analyser(echo, 100, 50, vec![1.0, 0.0], vec![1.0, 0.0]);
        let mut motion = AnalyserMotion::default();
        motion.advance(&quiet, 0);
        let picture = motion.advance(&loud, 16).clone();
        assert!(
            sum(pixel(&picture, 25, 25)) > 600,
            "the ghost's mark half way up the full bar"
        );
        // Sparkle: over twenty frames a loud bar throws at least one speck above its tip.
        let sparkle = Look {
            sparkle: true,
            ..base
        };
        let a = analyser(sparkle, 100, 50, vec![1.0, 0.0], vec![1.0, 0.0]);
        let mut motion = AnalyserMotion::default();
        let mut seen = false;
        for f in 0..20 {
            let picture = motion.advance(&a, f * 16);
            seen |= (10..40).any(|x| {
                (0..6).any(|y| sum(pixel(picture, x, y)) > 60 && pixel(picture, x, y)[3] > 0)
            });
        }
        assert!(seen, "a sparkle above the bar within twenty frames");
    }

    fn picture_ref(f: &Frame) -> &Frame {
        f
    }

    /// A dense graph frame, two channels of 256 bands with a line and a
    /// peak line, must cost what the other looks do.
    #[test]
    fn a_dense_graph_frame_costs_milliseconds() {
        let look = Look {
            style: lead::LookStyle::Graph,
            layout: Layout::DualVertical,
            line_width: 2.0,
            fill_alpha: 0.4,
            peak_line: true,
            smoothing: 0.6,
            ..Look::default()
        };
        let left: Vec<f32> = (0..256).map(|i| ((i * 37) % 100) as f32 / 100.0).collect();
        let right: Vec<f32> = (0..256).map(|i| ((i * 53) % 100) as f32 / 100.0).collect();
        let a = analyser(look, 1200, 420, left, right);
        let mut motion = AnalyserMotion::default();
        motion.advance(&a, 0);
        let started = std::time::Instant::now();
        let frames = 60;
        for i in 1..=frames {
            motion.advance(&a, i * 16);
        }
        let per_frame = started.elapsed().as_secs_f64() * 1000.0 / frames as f64;
        eprintln!("a dense graph frame: {per_frame:.2} ms");
        let bound = if cfg!(debug_assertions) { 100.0 } else { 8.0 };
        assert!(per_frame < bound, "a frame took {per_frame:.1} ms");
    }

    /// A dense radial frame, two channels of 256 bands, must cost what
    /// the other looks do.
    #[test]
    fn a_dense_radial_frame_costs_milliseconds() {
        let look = Look {
            radial: true,
            layout: Layout::DualVertical,
            smoothing: 0.6,
            peaks: true,
            spin_rpm: 3.0,
            ..Look::default()
        };
        let left: Vec<f32> = (0..256).map(|i| ((i * 37) % 100) as f32 / 100.0).collect();
        let right: Vec<f32> = (0..256).map(|i| ((i * 53) % 100) as f32 / 100.0).collect();
        let a = analyser(look, 420, 420, left, right);
        let mut motion = AnalyserMotion::default();
        motion.advance(&a, 0);
        let started = std::time::Instant::now();
        let frames = 60;
        for i in 1..=frames {
            motion.advance(&a, i * 16);
        }
        let per_frame = started.elapsed().as_secs_f64() * 1000.0 / frames as f64;
        eprintln!("a dense radial frame: {per_frame:.2} ms");
        let bound = if cfg!(debug_assertions) { 100.0 } else { 8.0 };
        assert!(per_frame < bound, "a frame took {per_frame:.1} ms");
    }

    /// One outlined bar filling its box: full ink on its lines, the
    /// fill's alpha inside, and every body row alike; one rounded bar:
    /// the corner left to the background, the body straight.
    #[test]
    fn a_rounded_or_outlined_bar_keeps_a_straight_body_between_its_ends() {
        let outlined = Look {
            smoothing: 0.0,
            peaks: false,
            bar_space: 0.0,
            outline: true,
            line_width: 2.0,
            fill_alpha: 0.2,
            bgr_alpha: 1.0,
            color_mode: lead::ColorMode::Index,
            ..Look::default()
        };
        let a = analyser(outlined, 20, 40, vec![1.0], vec![1.0]);
        let mut motion = AnalyserMotion::default();
        let picture = motion.advance(&a, 0);
        let sum = |p: [u8; 4]| p[0] as u32 + p[1] as u32 + p[2] as u32;
        let line = pixel(picture, 0, 20);
        let inside = pixel(picture, 10, 20);
        let top = pixel(picture, 10, 0);
        assert!(sum(line) > 100, "the side line is the ink: {line:?}");
        assert_eq!(sum(top), sum(line), "the end line is the ink too");
        assert!(
            sum(inside) * 3 < sum(line) && sum(inside) > 0,
            "inside, the fill's fifth: {inside:?} against {line:?}"
        );
        for y in 2..38 {
            assert_eq!(pixel(picture, 10, y), inside, "body row {y} inside");
            assert_eq!(pixel(picture, 1, y), line, "body row {y} on the line");
        }
        let rounded = Look {
            smoothing: 0.0,
            peaks: false,
            bar_space: 0.0,
            round: true,
            bgr_alpha: 1.0,
            color_mode: lead::ColorMode::Index,
            ..Look::default()
        };
        let a = analyser(rounded, 20, 40, vec![1.0], vec![1.0]);
        let mut motion = AnalyserMotion::default();
        let picture = motion.advance(&a, 0);
        assert_eq!(pixel(picture, 0, 0)[..3], [0, 0, 0], "the corner is cut");
        let body = pixel(picture, 0, 20);
        assert!(sum(body) > 100, "the body reaches the edge: {body:?}");
        for y in 10..40 {
            assert_eq!(pixel(picture, 0, y), body, "body row {y}");
        }
    }

    #[test]
    fn a_mirror_is_symmetric_and_a_vertical_pair_hangs_the_right_channel() {
        let look = Look {
            smoothing: 0.0,
            peaks: false,
            bar_space: 0.0,
            mirror: 1,
            ..Look::default()
        };
        let a = analyser(look, 80, 40, vec![1.0, 0.5], vec![1.0, 0.5]);
        let mut motion = AnalyserMotion::default();
        let picture = motion.advance(&a, 0);
        for y in [5u32, 20, 38] {
            for x in 0..40u32 {
                assert_eq!(
                    pixel(picture, x, y),
                    pixel(picture, 79 - x, y),
                    "mirror at {x},{y}"
                );
            }
        }
        let look = Look {
            smoothing: 0.0,
            peaks: false,
            bar_space: 0.0,
            layout: Layout::DualVertical,
            ..Look::default()
        };
        let a = analyser(look, 40, 100, vec![1.0], vec![0.5]);
        let mut motion = AnalyserMotion::default();
        let picture = motion.advance(&a, 0);
        assert_eq!(
            pixel(picture, 20, 2)[3],
            255,
            "the left channel reaches the top"
        );
        assert_eq!(
            pixel(picture, 20, 60)[3],
            255,
            "the right channel hangs from the middle"
        );
        assert_eq!(
            pixel(picture, 20, 90)[..3],
            [0, 0, 0],
            "and stops half way down"
        );
    }
}
