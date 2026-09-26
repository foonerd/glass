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
#[cfg(test)]
use plot::Art;
use plot::{Fanart, Indicators, Scene, Text, TypeArea};

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
    /// Fonts a field names by file.
    files: HashMap<String, Arc<Face>>,
    /// Every face by its path.
    by_path: HashMap<String, Arc<Face>>,
}

/// A font file mapped into memory rather than read: the kernel brings in
/// only the pages the glyphs touch, and shares them with any other process
/// that maps the file. A 16 MB face costs the tables it uses.
pub struct Face {
    font: FontRef<'static>,
    /// Holds the bytes `font` reads; dropped after it.
    _map: memmap2::Mmap,
}

fn open_face(path: &str) -> Option<Arc<Face>> {
    if path.is_empty() {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    // The theme's font files are not written while a meter shows them.
    let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
    // The font borrows the map for as long as the face lives, and the face
    // keeps the map; nothing hands the font out past the face.
    let bytes: &'static [u8] = unsafe { std::slice::from_raw_parts(map.as_ptr(), map.len()) };
    let font = FontRef::try_from_slice(bytes).ok()?;
    Some(Arc::new(Face { font, _map: map }))
}

impl Fonts {
    pub fn load(files: &FontFiles) -> Self {
        let mut fonts = Self::default();
        fonts.light = fonts.face(&files.light);
        fonts.regular = fonts.face(&files.regular);
        fonts.bold = fonts.face(&files.bold);
        fonts.italic = fonts.face(&files.italic);
        fonts.digi = fonts.face(&files.digi);
        fonts
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

    /// The bytes the mapped font files span; only the pages touched are resident.
    pub fn bytes(&self) -> usize {
        self.by_path.values().map(|f| f._map.len()).sum()
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

/// One finished picture. `pane` uploads it once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frame {
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
            spectrum: None,
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
    let image = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?
        .into_rgba8();
    let frame = Frame {
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
        width: w,
        height: h,
        rgba: scaled.into_raw(),
    }
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
    /// The spectrum's pictures, built from `Scene::spectrum` once per meter.
    pub spectrum: Option<&'a SpectrumAssets>,
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
                .map(|&h| (h as f32 * ramp) as u32)
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
        vinyl,
        tonearm,
        reels,
        fade,
        needles,
        labels,
        canvas,
        painters,
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
    let spectrum_plan = match (&scene.spectrum, stack.spectrum) {
        (Some(spec), Some(_)) => Some(spectrum.advance(spec, &scene.bar_heights)),
        _ => None,
    };
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
    if let (Some(spec), Some(assets), Some(plan)) =
        (&scene.spectrum, stack.spectrum, &spectrum_plan)
    {
        plan_spectrum(spec, assets, plan, &mut ops);
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
    let painting = std::time::Instant::now();
    paint(canvas, base, &ops, &rects, painters.active);
    painters.settle(painting.elapsed().as_micros() as u64, now_ms);
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
    loop {
        let mut merged = false;
        'pairs: for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                if boxes_meet(boxes[i], boxes[j])
                    && box_area(box_union(boxes[i], boxes[j]))
                        <= box_area(boxes[i]) + box_area(boxes[j])
                {
                    let union = box_union(boxes[i], boxes[j]);
                    boxes.swap_remove(j);
                    boxes[i] = union;
                    merged = true;
                    break 'pairs;
                }
            }
        }
        if !merged {
            break;
        }
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
            let line = render_line(font, text.size, text.color, &text.text, 0)
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
/// the box, placed by `align`, and centred vertically. `both` puts the icon
/// on the left and the label three pixels to its right. Text mode without a
/// box draws the label at the position.
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
            (Some(icon), None) => {
                let (iw, ih) = fitted(icon);
                let y = area.y + h.saturating_sub(ih) / 2;
                ops.push(Op::Blit {
                    src: icon,
                    at: (area.x as i32, y as i32),
                    part: (0, 0, iw, ih),
                    clip: None,
                    alpha: 255,
                });
            }
            (Some(icon), Some(label)) => {
                let (iw, ih) = fitted(icon);
                let iy = area.y + h.saturating_sub(ih) / 2;
                ops.push(Op::Blit {
                    src: icon,
                    at: (area.x as i32, iy as i32),
                    part: (0, 0, iw, ih),
                    clip: None,
                    alpha: 255,
                });
                let text_x = iw + GAP;
                if text_x < w {
                    let (lw, lh) = (label.width.min(w - text_x), label.height.min(h));
                    let ty = area.y + h.saturating_sub(lh) / 2;
                    ops.push(Op::Blit {
                        src: label,
                        at: ((area.x + text_x) as i32, ty as i32),
                        part: (0, 0, lw, lh),
                        clip: None,
                        alpha: 255,
                    });
                }
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

/// Decode a type icon fitted inside `w` by `h`, keeping its aspect. An SVG
/// is rendered at that size and every visible pixel takes `tint`; a PNG keeps
/// its own colours. A picture smaller than the box is enlarged to it.
pub fn read_icon(path: &Path, w: u32, h: u32, tint: Option<[u8; 3]>) -> Option<Frame> {
    let (w, h) = (w.max(1), h.max(1));
    let is_svg = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
    let mut frame = if is_svg {
        let bytes = std::fs::read(path).ok()?;
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
        size,
        color,
        text,
        max_width,
    )
}

fn render_line(
    font: Option<&FontRef<'static>>,
    size: u32,
    color: [u8; 3],
    text: &str,
    max_width: u32,
) -> Option<Frame> {
    let font = font?;
    if text.is_empty() {
        return None;
    }
    let scale = PxScale::from(size.max(1) as f32);
    let scaled = font.as_scaled(scale);
    let ascent = scaled.ascent();
    let line_height = (ascent - scaled.descent()).ceil().max(1.0) as u32;
    let mut advance = 0.0f32;
    let mut last: Option<ab_glyph::GlyphId> = None;
    for ch in text.chars() {
        let id = font.glyph_id(ch);
        if let Some(prev) = last {
            advance += scaled.kern(prev, id);
        }
        advance += scaled.h_advance(id);
        last = Some(id);
    }
    let mut width = advance.ceil().max(1.0) as u32;
    if max_width > 0 {
        width = width.min(max_width);
    }
    let height = line_height;
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    let mut pen = 0.0f32;
    let mut last: Option<ab_glyph::GlyphId> = None;
    for ch in text.chars() {
        let id = font.glyph_id(ch);
        if let Some(prev) = last {
            pen += scaled.kern(prev, id);
        }
        let glyph = id.with_scale_and_position(scale, ab_glyph::point(pen, ascent));
        if let Some(outline) = font.outline_glyph(glyph) {
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
        pen += scaled.h_advance(id);
        last = Some(id);
        if pen >= width as f32 {
            break;
        }
    }
    Some(Frame {
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
    pub spectrum: SpectrumMotion,
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
        let most = most.max(1);
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
    last: std::time::Instant,
}

impl<'a> Stages<'a> {
    fn new(sink: Option<&'a mut Vec<(&'static str, u64)>>) -> Self {
        Self {
            sink,
            last: std::time::Instant::now(),
        }
    }

    fn mark(&mut self, name: &'static str) {
        if let Some(sink) = self.sink.as_deref_mut() {
            let now = std::time::Instant::now();
            sink.push((name, now.duration_since(self.last).as_micros() as u64));
            self.last = now;
        }
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
    let image = image::open(path).ok()?.into_rgba8();
    let width = image.width();
    let height = image.height();
    Some(Frame {
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
            width: 97,
            height: 61,
            rgba: vec![0; 97 * 61 * 4],
        };
        paint(&mut one, &base, &ops, &all, 1);
        assert_ne!(one.rgba, base.rgba, "something was painted");
        for threads in [2usize, 3, 7] {
            let mut many = Frame {
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
                    spectrum: None,
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
                    spectrum: None,
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
                spectrum: None,
                folder_pictures: &[],
                fanart: (None, None),
                vinyl: None,
                tonearm: None,
                reels: (None, None),
                indicators: None,
                base: None,
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
