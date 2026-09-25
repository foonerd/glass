//! Raster a [`plot::Scene`] into one RGBA frame.
//! This station does not poll a FIFO or talk to the player.

use std::path::Path;

use std::collections::HashMap;

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use lead::{Direction, Fill, FolderLayerSpec, FontFiles, LinearSpec, MeterKind, Scale, ScrollDirection, SpectrumSpec, TextAlign, TextStyle, TypeAlign, TypeMode, ZOrder};
use lead::TonearmSpec;
use plot::{Fanart, Scene, Text, TypeArea};

const BG: [u8; 4] = [12, 12, 16, 255];

/// Theme fonts, loaded once per run. A style whose file is missing or
/// unreadable stays `None`, and its texts fall back to the built-in bitmap
/// font. Italic without its own file falls back to regular, as the player does.
#[derive(Default)]
pub struct Fonts {
    light: Option<FontVec>,
    regular: Option<FontVec>,
    bold: Option<FontVec>,
    italic: Option<FontVec>,
    digi: Option<FontVec>,
    /// Fonts a field names by file, loaded once each.
    files: HashMap<String, FontVec>,
}

fn read_font(path: &str) -> Option<FontVec> {
    if path.is_empty() {
        return None;
    }
    FontVec::try_from_vec(std::fs::read(path).ok()?).ok()
}

impl Fonts {
    pub fn load(files: &FontFiles) -> Self {
        Self {
            light: read_font(&files.light),
            regular: read_font(&files.regular),
            bold: read_font(&files.bold),
            italic: read_font(&files.italic),
            digi: read_font(&files.digi),
            files: HashMap::new(),
        }
    }

    /// Load a font a field names by file, so `get_for` can serve it.
    pub fn add_file(&mut self, path: &str) {
        if path.is_empty() || self.files.contains_key(path) {
            return;
        }
        if let Some(font) = read_font(path) {
            self.files.insert(path.to_string(), font);
        }
    }

    fn get(&self, style: TextStyle) -> Option<&FontVec> {
        match style {
            TextStyle::Light => self.light.as_ref(),
            TextStyle::Regular => self.regular.as_ref(),
            TextStyle::Bold => self.bold.as_ref(),
            TextStyle::Italic => self.italic.as_ref().or(self.regular.as_ref()),
            TextStyle::Digi => self.digi.as_ref(),
        }
    }

    /// The font for a text: its own file when it names one and that file
    /// loaded, else its style's font.
    fn get_for(&self, style: TextStyle, font_file: &str) -> Option<&FontVec> {
        if !font_file.is_empty() {
            if let Some(font) = self.files.get(font_file) {
                return Some(font);
            }
        }
        self.get(style)
    }

    /// How many of the five styles have a font file of their own.
    pub fn loaded(&self) -> usize {
        [&self.light, &self.regular, &self.bold, &self.italic, &self.digi]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            rgba: Vec::new(),
        }
    }
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
        },
        &mut Motion::default(),
        0,
    )
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
    let Some(image) = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone()) else {
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
    pub front: Option<&'a Frame>,
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
}

/// Theme order: full-screen picture, meter face, album art, folder layers, needles, meter
/// spectrum, texts, overlay folder layers, and the meter foreground last. `now_ms` drives the moving texts through `motion`.
pub fn raster_over(scene: &Scene, stack: Stack<'_>, motion: &mut Motion, now_ms: u64) -> Frame {
    let width = scene.width.max(1);
    let height = scene.height.max(1);
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for px in rgba.chunks_exact_mut(4) {
        px.copy_from_slice(&BG);
    }
    if let Some(screen) = stack.screen {
        blit(&mut rgba, width, height, screen);
    }
    // The meter engine composes the screen picture and the meter face into
    // one static background; album art and background folder layers go over
    // that, under the needles.
    if let Some(face) = stack.face {
        blit_at(&mut rgba, width, height, face, stack.face_at);
    }
    // The tonearm's state decides whether the record keeps turning while it lifts.
    let tonearm_animating = scene.tonearm.as_ref().map_or(false, |spec| {
        motion.tonearm.update(spec, scene.playing, scene.progress_pct, scene.time_remaining, now_ms);
        motion.tonearm.is_animating()
    });
    let lift_s = scene.tonearm.as_ref().map_or(1.5, |spec| spec.lift_s);
    let vinyl_rpm = scene.vinyl.as_ref().map_or(0.0, |v| v.spec.rpm);
    let art_rpm = scene.art.as_ref().filter(|a| a.rotation).map_or(0.0, |a| a.rpm);
    let turn_rpm = if scene.vinyl.is_some() { vinyl_rpm } else { art_rpm };
    let angle = motion.vinyl.advance(turn_rpm, scene.vinyl.as_ref().map_or(true, |v| v.spec.clockwise), scene.playing, scene.transitional, tonearm_animating, lift_s, now_ms);
    if let Some(reels) = &scene.reels {
        let spin = scene.playing || scene.transitional;
        let p = scene.progress_pct / 100.0;
        let (left_mult, right_mult) = if reels.spec.adaptive {
            if reels.spec.clockwise {
                (reels.spec.spool_left * (1.5 - p), reels.spec.spool_right * (0.5 + p))
            } else {
                (reels.spec.spool_left * (0.5 + p), reels.spec.spool_right * (1.5 - p))
            }
        } else {
            (reels.spec.spool_left, reels.spec.spool_right)
        };
        let sides = [(&reels.spec.left, stack.reels.0, left_mult, &mut motion.reels.0), (&reels.spec.right, stack.reels.1, right_mult, &mut motion.reels.1)];
        for (spec, picture, mult, turn) in sides {
            let Some(spec) = spec else { continue };
            let angle = turn.advance(spec.rpm * mult, reels.spec.clockwise, spin, now_ms);
            if let Some(picture) = picture {
                let pivot = (picture.width as f32 / 2.0, picture.height as f32 / 2.0);
                blit_pivot(&mut rgba, width, height, picture, pivot, (spec.center.0 as f32, spec.center.1 as f32), -angle);
            }
        }
    }
    if let (Some(vinyl), Some(picture)) = (&scene.vinyl, stack.vinyl) {
        let pivot = (picture.width as f32 / 2.0, picture.height as f32 / 2.0);
        blit_pivot(&mut rgba, width, height, picture, pivot, (vinyl.spec.center.0 as f32, vinyl.spec.center.1 as f32), -angle);
    }
    if let (Some(art), Some(place)) = (stack.art, &scene.art) {
        let color = [place.border_color[0], place.border_color[1], place.border_color[2], 255];
        if place.rotation && place.rpm > 0.0 {
            // Turning art sits on the record's centre when there is one.
            let centre = scene
                .vinyl
                .as_ref()
                .map(|v| (v.spec.center.0 as f32, v.spec.center.1 as f32))
                .unwrap_or((place.x as f32 + place.w as f32 / 2.0, place.y as f32 + place.h as f32 / 2.0));
            blit_pivot(&mut rgba, width, height, art, (art.width as f32 / 2.0, art.height as f32 / 2.0), centre, -angle);
            let (cx, cy) = (centre.0.round() as i32, centre.1.round() as i32);
            if place.border > 0 {
                draw_circle(&mut rgba, width, height, cx, cy, (place.w.min(place.h) / 2) as i32, place.border as i32, color);
            }
            fill_circle(&mut rgba, width, height, cx, cy, 5, color);
            draw_circle(&mut rgba, width, height, cx, cy, (place.w.min(place.h) / 10).max(3) as i32, 1, color);
        } else {
            blit_at(&mut rgba, width, height, art, (place.x, place.y));
            if place.border > 0 {
                draw_border(&mut rgba, width, height, (place.x, place.y, place.w, place.h), place.border, color);
            }
        }
    }
    draw_folder_layers(&mut rgba, width, height, scene, stack.folder_pictures, ZOrder::Background);
    draw_fanart(&mut rgba, width, height, scene.fanart.as_ref(), stack.fanart, ZOrder::Background);
    if scene.meter.visible {
        let right_sprite = stack.needle_right.or(stack.needle);
        match (scene.meter.kind, &scene.meter.linear) {
            (MeterKind::Linear, Some(linear)) => {
                if let Some(sprite) = stack.needle {
                    if scene.meter.channels == 1 {
                        if let Some(at) = scene.meter.mono_at {
                            draw_bar(&mut rgba, width, height, sprite, at, linear.bar_width(scene.mono), linear, true);
                        }
                    } else {
                        if let Some(at) = scene.left_at {
                            draw_bar(&mut rgba, width, height, sprite, at, linear.bar_width(scene.left), linear, true);
                        }
                        if let (Some(at), Some(sprite)) = (scene.right_at, right_sprite) {
                            draw_bar(&mut rgba, width, height, sprite, at, linear.bar_width(scene.right), linear, false);
                        }
                    }
                }
            }
            _ => {
                if let (Some(sprite), Some((start, stop, distance))) = (stack.needle, scene.needle) {
                    let (left_start, left_stop) = scene.meter.left_angles.unwrap_or((start, stop));
                    if scene.meter.channels == 1 {
                        if let Some(at) = scene.meter.mono_at {
                            let angle = left_start + (left_stop - left_start) * scene.mono;
                            blit_rotated(&mut rgba, width, height, sprite, at, angle, distance);
                        }
                    } else {
                        if let Some(at) = scene.left_at {
                            let angle = left_start + (left_stop - left_start) * scene.left;
                            blit_rotated(&mut rgba, width, height, sprite, at, angle, distance);
                        }
                        if let (Some(at), Some(sprite)) = (scene.right_at, right_sprite) {
                            let (right_start, right_stop) = scene.meter.right_angles.unwrap_or((start, stop));
                            let angle = right_start + (right_stop - right_start) * scene.right;
                            blit_rotated(&mut rgba, width, height, sprite, at, angle, distance);
                        }
                    }
                }
            }
        }
    }
    if let (Some(spec), Some(assets)) = (&scene.spectrum, stack.spectrum) {
        draw_spectrum(&mut rgba, width, height, spec, &scene.bar_heights, assets, &mut motion.spectrum);
    }
    if stack.screen.is_none() && stack.face.is_none() {
        let layout = layout(width, height);
        let left_meter = place(layout.left_meter, scene.left_at, width, height);
        let right_meter = place(layout.right_meter, scene.right_at, width, height);
        fill_column(&mut rgba, width, &left_meter, scene.left, METER);
        fill_column(&mut rgba, width, &right_meter, scene.right, METER);
        fill_bars(&mut rgba, width, &layout.spectrum, &scene.bars, BAR);
    }
    let mut frame = Frame {
        width,
        height,
        rgba,
    };
    for text in &scene.texts {
        draw_text_moving(&mut frame, text, stack.fonts, &mut motion.text, now_ms);
    }
    if let (Some(spec), Some(picture)) = (&scene.tonearm, stack.tonearm) {
        blit_pivot(
            &mut frame.rgba,
            width,
            height,
            picture,
            (spec.pivot_image.0 as f32, spec.pivot_image.1 as f32),
            (spec.pivot_screen.0 as f32, spec.pivot_screen.1 as f32),
            motion.tonearm.angle(),
        );
    }
    if let Some(area) = &scene.type_area {
        draw_type_area(&mut frame, area, stack.icon, stack.fonts);
    }
    // Overlay folder layers sit above everything but the meter foreground,
    // which the meter engine draws last of all.
    draw_folder_layers(&mut frame.rgba, width, height, scene, stack.folder_pictures, ZOrder::Overlay);
    draw_fanart(&mut frame.rgba, width, height, scene.fanart.as_ref(), stack.fanart, ZOrder::Overlay);
    if let Some(front) = stack.front {
        blit_at(&mut frame.rgba, width, height, front, stack.face_at);
    }
    frame
}

fn align_text_x(box_x: u32, box_w: u32, item_w: u32, align: TextAlign) -> u32 {
    match align {
        TextAlign::Left => box_x,
        TextAlign::Right => box_x + box_w.saturating_sub(item_w),
        TextAlign::Center => box_x + box_w.saturating_sub(item_w) / 2,
    }
}

/// Copy the part of `src` that falls inside the window `x..x+box_w` on the
/// destination when `src` is placed at `draw_x`.
fn blit_window(dst: &mut [u8], dst_w: u32, dst_h: u32, src: &Frame, draw_x: i32, y: u32, x: u32, box_w: u32) {
    let win_from = x as i32;
    let win_to = (x + box_w).min(dst_w) as i32;
    for row in 0..src.height {
        let dy = y + row;
        if dy >= dst_h {
            break;
        }
        for col in 0..src.width {
            let dx = draw_x + col as i32;
            if dx < win_from {
                continue;
            }
            if dx >= win_to {
                break;
            }
            let s = (row as usize * src.width as usize + col as usize) * 4;
            let d = (dy as usize * dst_w as usize + dx as usize) * 4;
            blend(dst, d, [src.rgba[s], src.rgba[s + 1], src.rgba[s + 2], src.rgba[s + 3]]);
        }
    }
}

/// Draw one text in its box. A text that fits is placed by its alignment.
/// A wider text moves as the player moves it: it bounces between its ends
/// with a pause, or, as a ticker, loops by one segment. The drawn position
/// is the box left edge minus the offset, and the box clips.
pub fn draw_text_moving(frame: &mut Frame, text: &Text, fonts: Option<&Fonts>, motion: &mut TextMotion, now_ms: u64) {
    let font = fonts.and_then(|f| f.get_for(text.style, &text.font_file));
    let Some(line) = render_line(font, text.size, text.color, &text.text, 0) else {
        draw_text(frame, text.x, text.y, &text.text);
        return;
    };
    let key = (text.x, text.y);
    let box_w = text.max_width;
    if box_w == 0 || line.width <= box_w || text.speed <= 0.0 {
        motion.states.remove(&key);
        let x = if box_w == 0 {
            text.x
        } else {
            align_text_x(text.x, box_w, line.width, text.align)
        };
        let shown = if box_w == 0 { line } else { clip_frame(&line, box_w, line.height) };
        blit_at(&mut frame.rgba, frame.width, frame.height, &shown, (x, text.y));
        return;
    }
    let limit = (line.width - box_w) as f32;
    let segment = if text.loop_thirds { (line.width / 3) as f32 } else { 0.0 };
    let state = motion.states.entry(key).or_default();
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
    blit_window(&mut frame.rgba, frame.width, frame.height, &line, draw_x, text.y, text.x, box_w);
}

/// A rectangle outline `thickness` pixels wide, inside the box.
fn draw_border(dst: &mut [u8], dst_w: u32, dst_h: u32, rect: (u32, u32, u32, u32), thickness: u32, color: [u8; 4]) {
    let (x, y, w, h) = rect;
    let t = thickness.min(w / 2).min(h / 2).max(1);
    for row in 0..h {
        let edge_row = row < t || row + t >= h;
        for col in 0..w {
            if !(edge_row || col < t || col + t >= w) {
                continue;
            }
            let (px, py) = (x + col, y + row);
            if px < dst_w && py < dst_h {
                put(dst, dst_w, px, py, color);
            }
        }
    }
}

/// Copy the top-left `w` by `h` of a frame. A frame already inside is returned as is.
fn clip_frame(frame: &Frame, w: u32, h: u32) -> Frame {
    if frame.width <= w && frame.height <= h {
        return frame.clone();
    }
    let cw = frame.width.min(w).max(1);
    let ch = frame.height.min(h).max(1);
    let mut rgba = Vec::with_capacity((cw * ch * 4) as usize);
    for y in 0..ch {
        let from = (y * frame.width * 4) as usize;
        rgba.extend_from_slice(&frame.rgba[from..from + (cw * 4) as usize]);
    }
    Frame {
        width: cw,
        height: ch,
        rgba,
    }
}

fn align_x(box_x: u32, box_w: u32, item_w: u32, align: TypeAlign) -> u32 {
    match align {
        TypeAlign::Left => box_x,
        TypeAlign::Right => box_x + box_w.saturating_sub(item_w),
        TypeAlign::Center => box_x + box_w.saturating_sub(item_w) / 2,
    }
}

/// Draw the type area. Inside a real box the icon or label is clipped to the
/// box, placed by `align`, and centred vertically. `both` puts the icon on
/// the left and the label three pixels to its right. Text mode without a
/// box draws the label at the position.
pub fn draw_type_area(frame: &mut Frame, area: &TypeArea, icon: Option<&Frame>, fonts: Option<&Fonts>) {
    const GAP: u32 = 3;
    let label = render_text(fonts, area.font_style, area.font_size, area.color, &area.label, 0);
    let Some((w, h)) = area.box_size else {
        if area.mode == TypeMode::Text {
            if let Some(label) = label {
                blit_at(&mut frame.rgba, frame.width, frame.height, &label, (area.x, area.y));
            }
        }
        return;
    };
    let place = |frame: &mut Frame, item: &Frame| {
        let item = clip_frame(item, w, h);
        let x = align_x(area.x, w, item.width, area.align);
        let y = area.y + h.saturating_sub(item.height) / 2;
        blit_at(&mut frame.rgba, frame.width, frame.height, &item, (x, y));
    };
    match area.mode {
        TypeMode::Text => {
            if let Some(label) = &label {
                place(frame, label);
            }
        }
        TypeMode::Icon => match (icon, &label) {
            (Some(icon), _) => place(frame, icon),
            (None, Some(label)) => place(frame, label),
            (None, None) => {}
        },
        TypeMode::Both => match (icon, label) {
            (None, None) => {}
            (None, Some(label)) => place(frame, &label),
            (Some(icon), None) => {
                let y = area.y + h.saturating_sub(icon.height) / 2;
                blit_at(&mut frame.rgba, frame.width, frame.height, &clip_frame(icon, w, h), (area.x, y));
            }
            (Some(icon), Some(label)) => {
                let icon = clip_frame(icon, w, h);
                let iy = area.y + h.saturating_sub(icon.height) / 2;
                blit_at(&mut frame.rgba, frame.width, frame.height, &icon, (area.x, iy));
                let text_x = icon.width + GAP;
                if text_x < w {
                    let label = clip_frame(&label, w - text_x, h);
                    let ty = area.y + h.saturating_sub(label.height) / 2;
                    blit_at(&mut frame.rgba, frame.width, frame.height, &label, (area.x + text_x, ty));
                }
            }
        },
    }
}

/// Decode a type icon fitted inside `w` by `h`, keeping its aspect. An SVG
/// is rendered at that size and every visible pixel takes `tint`; a PNG keeps
/// its own colours. A picture smaller than the box is enlarged to it.
pub fn read_icon(path: &Path, w: u32, h: u32, tint: Option<[u8; 3]>) -> Option<Frame> {
    let (w, h) = (w.max(1), h.max(1));
    let is_svg = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"));
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
        let scale = (w as f32 / picture.width.max(1) as f32).min(h as f32 / picture.height.max(1) as f32);
        let fw = ((picture.width as f32 * scale) as u32).clamp(1, w);
        let fh = ((picture.height as f32 * scale) as u32).clamp(1, h);
        fit_art(&picture, fw, fh)
    };
    if let (true, Some(tint)) = (is_svg, tint) {
        for px in frame.rgba.chunks_exact_mut(4) {
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
    for (px, mpx) in art.rgba.chunks_exact_mut(4).zip(mask.rgba.chunks_exact(4)) {
        let luminance = (u32::from(mpx[0]) * 299 + u32::from(mpx[1]) * 587 + u32::from(mpx[2]) * 114) / 1000;
        let keep = 255 - luminance.min(255) as u8;
        px[3] = ((u32::from(px[3]) * u32::from(keep)) / 255) as u8;
    }
}

/// Draw one theme text. With its font, the top of the em box sits at `y`, as
/// the player's renderer places it, and `max_width` clips the line. Without
/// a font for its style the bitmap font stands in.
pub fn draw_text_styled(frame: &mut Frame, text: &Text, fonts: Option<&Fonts>) {
    match render_text(fonts, text.style, text.size, text.color, &text.text, text.max_width) {
        Some(line) => blit_at(&mut frame.rgba, frame.width, frame.height, &line, (text.x, text.y)),
        None => draw_text(frame, text.x, text.y, &text.text),
    }
}

/// Set a line of text in a font: a transparent frame one line high whose
/// width is the text's advance, or `max_width` when that is smaller and not
/// zero. `None` when the style has no font or the text is empty.
pub fn render_text(fonts: Option<&Fonts>, style: TextStyle, size: u32, color: [u8; 3], text: &str, max_width: u32) -> Option<Frame> {
    render_line(fonts.and_then(|f| f.get(style)), size, color, text, max_width)
}

fn render_line(font: Option<&FontVec>, size: u32, color: [u8; 3], text: &str, max_width: u32) -> Option<Frame> {
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

fn fill_column(rgba: &mut [u8], stride: u32, rect: &Rect, level: f32, color: [u8; 4]) {
    let lit = (level.clamp(0.0, 1.0) * rect.h as f32).round() as u32;
    let lit = lit.min(rect.h);
    for row in 0..lit {
        let y = rect.y + rect.h - 1 - row;
        for x in rect.x..rect.x + rect.w {
            put(rgba, stride, x, y, color);
        }
    }
}

fn fill_bars(rgba: &mut [u8], stride: u32, rect: &Rect, bars: &[f32], color: [u8; 4]) {
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
        fill_column(
            rgba,
            stride,
            &Rect {
                x,
                y: rect.y,
                w,
                h: rect.h,
            },
            *level,
            color,
        );
    }
}

/// Copy a layer at a position. Its alpha channel blends over what is there.
fn blit_at(dst: &mut [u8], dst_w: u32, dst_h: u32, src: &Frame, at: (u32, u32)) {
    for y in 0..src.height {
        let dy = at.1 + y;
        if dy >= dst_h {
            break;
        }
        for x in 0..src.width {
            let dx = at.0 + x;
            if dx >= dst_w {
                break;
            }
            let s = (y as usize * src.width as usize + x as usize) * 4;
            let d = (dy as usize * dst_w as usize + dx as usize) * 4;
            blend(
                dst,
                d,
                [src.rgba[s], src.rgba[s + 1], src.rgba[s + 2], src.rgba[s + 3]],
            );
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
    [src.rgba[s], src.rgba[s + 1], src.rgba[s + 2], src.rgba[s + 3]]
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
/// Every frame pixel inside the turned bounds samples the sprite bilinearly,
/// so the edge stays smooth and the alpha channel blends over the face.
fn blit_rotated(dst: &mut [u8], dst_w: u32, dst_h: u32, src: &Frame, at: (i32, i32), degrees: f32, distance: f32) {
    // The needle turns about a point `distance` below its picture's centre,
    // which the theme places on the origin.
    let pivot = (src.width as f32 / 2.0, src.height as f32 / 2.0 + distance);
    blit_pivot(dst, dst_w, dst_h, src, pivot, (at.0 as f32, at.1 as f32), degrees);
}

/// Blend a picture turned `degrees` counter-clockwise about `pivot_image`,
/// that point landing on `pivot_screen`. Bilinear, premultiplied alpha,
/// inverse mapping from every frame pixel the turned picture can reach.
fn blit_pivot(dst: &mut [u8], dst_w: u32, dst_h: u32, src: &Frame, pivot_image: (f32, f32), pivot_screen: (f32, f32), degrees: f32) {
    if src.width == 0 || src.height == 0 || dst_w == 0 || dst_h == 0 {
        return;
    }
    let rad = degrees.to_radians();
    let (sin, cos) = rad.sin_cos();
    let (px, py) = pivot_image;
    let reach = [(0.0, 0.0), (src.width as f32, 0.0), (0.0, src.height as f32), (src.width as f32, src.height as f32)]
        .iter()
        .map(|(x, y)| ((x - px).powi(2) + (y - py).powi(2)).sqrt())
        .fold(0.0f32, f32::max)
        + 1.0;
    let x_from = ((pivot_screen.0 - reach).floor() as i32).max(0);
    let x_to = ((pivot_screen.0 + reach).ceil() as i32).min(dst_w as i32 - 1);
    let y_from = ((pivot_screen.1 - reach).floor() as i32).max(0);
    let y_to = ((pivot_screen.1 + reach).ceil() as i32).min(dst_h as i32 - 1);
    for dy in y_from..=y_to {
        for dx in x_from..=x_to {
            let vx = dx as f32 + 0.5 - pivot_screen.0;
            let vy = dy as f32 + 0.5 - pivot_screen.1;
            let sx = px + vx * cos - vy * sin;
            let sy = py + vx * sin + vy * cos;
            if sx < 0.0 || sy < 0.0 || sx > src.width as f32 || sy > src.height as f32 {
                continue;
            }
            let color = sample_bilinear(src, sx, sy);
            if color[3] == 0 {
                continue;
            }
            let d = (dy as usize * dst_w as usize + dx as usize) * 4;
            blend(dst, d, color);
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

/// A filled disc.
fn fill_circle(dst: &mut [u8], dst_w: u32, dst_h: u32, cx: i32, cy: i32, r: i32, color: [u8; 4]) {
    draw_circle(dst, dst_w, dst_h, cx, cy, r, r + 1, color);
}

/// A ring of the given thickness inside radius `r`, as the player's draw call
/// makes it.
fn draw_circle(dst: &mut [u8], dst_w: u32, dst_h: u32, cx: i32, cy: i32, r: i32, thickness: i32, color: [u8; 4]) {
    if r <= 0 || thickness <= 0 {
        return;
    }
    let inner = (r - thickness).max(0) as f32;
    let outer = r as f32;
    for y in (cy - r).max(0)..=(cy + r).min(dst_h as i32 - 1) {
        for x in (cx - r).max(0)..=(cx + r).min(dst_w as i32 - 1) {
            let d = (((x - cx) as f32).powi(2) + ((y - cy) as f32).powi(2)).sqrt();
            if d <= outer && d >= inner {
                blend(dst, (y as usize * dst_w as usize + x as usize) * 4, color);
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
    pub fn advance(&mut self, rpm: f32, clockwise: bool, playing: bool, transitional: bool, tonearm_animating: bool, lift_s: f32, now_ms: u64) -> f32 {
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
        let dt = self.last_ms.map_or(0.0, |last| (now_ms.saturating_sub(last) as f32 / 1000.0).min(0.5));
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
        let dt = self.last_ms.map_or(0.0, |last| (now_ms.saturating_sub(last) as f32 / 1000.0).min(0.5));
        self.last_ms = Some(now_ms);
        if rpm > 0.0 && spinning {
            let direction = if clockwise { 1.0 } else { -1.0 };
            self.angle = (self.angle + rpm * 6.0 * dt * direction).rem_euclid(360.0);
        }
        self.angle
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

    pub fn update(&mut self, spec: &TonearmSpec, playing: bool, progress_pct: f32, time_remaining: Option<f32>, now_ms: u64) {
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

/// Blend the part `(src_x, src_y, src_w, src_h)` of a picture at a frame
/// position that may lie partly outside the frame.
fn blit_part(dst: &mut [u8], dst_w: u32, dst_h: u32, src: &Frame, at: (i32, i32), part: (u32, u32, u32, u32)) {
    let (src_x, src_y, src_w, src_h) = part;
    for row in 0..src_h {
        let sy = src_y + row;
        let dy = at.1 + row as i32;
        if sy >= src.height || dy < 0 || dy >= dst_h as i32 {
            continue;
        }
        for col in 0..src_w {
            let sx = src_x + col;
            let dx = at.0 + col as i32;
            if sx >= src.width || dx < 0 || dx >= dst_w as i32 {
                continue;
            }
            let s = (sy as usize * src.width as usize + sx as usize) * 4;
            let d = (dy as usize * dst_w as usize + dx as usize) * 4;
            blend(dst, d, [src.rgba[s], src.rgba[s + 1], src.rgba[s + 2], src.rgba[s + 3]]);
        }
    }
}

/// One channel of a linear meter, as the meter engine draws it. A bar shows
/// `w` pixels of the indicator picture from the end the direction names,
/// anchored at the channel origin; `edges-center` and `center-edges` anchor
/// the two channels at opposite ends. A single indicator moves by `w`.
#[allow(clippy::too_many_arguments)]
fn draw_bar(dst: &mut [u8], dst_w: u32, dst_h: u32, sprite: &Frame, at: (i32, i32), w: u32, linear: &LinearSpec, left: bool) {
    let (cw, ch) = (sprite.width, sprite.height);
    if cw == 0 || ch == 0 {
        return;
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
        blit_part(dst, dst_w, dst_h, sprite, at, (0, 0, cw, ch));
        return;
    }
    let across = w.min(cw);
    let down = w.min(ch);
    let (at, part) = match linear.direction {
        Direction::LeftRight => ((ox, oy), (0, 0, across, ch)),
        Direction::RightLeft => ((ox + (cw - across) as i32, oy), (cw - across, 0, across, ch)),
        Direction::BottomTop => ((ox, oy + (ch - down) as i32), (0, ch - down, cw, down)),
        Direction::TopBottom => ((ox, oy), (0, 0, cw, down)),
        Direction::EdgesCenter => {
            if left {
                ((ox, oy), (0, 0, across, ch))
            } else {
                let x = if linear.flip_right { ox - across as i32 } else { ox };
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
    };
    blit_part(dst, dst_w, dst_h, sprite, at, part);
}

/// Everything that moves between frames: text offsets and spectrum toppings.
#[derive(Default)]
pub struct Motion {
    pub text: TextMotion,
    pub spectrum: SpectrumMotion,
    pub vinyl: VinylMotion,
    pub tonearm: TonearmMotion,
    pub reels: (ReelMotion, ReelMotion),
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
    pub fn load(spec: &SpectrumSpec) -> Self {
        let bar_box = (spec.bar_w, spec.bar_h);
        Self {
            background: spec.background.as_ref().and_then(|f| fill_frame(f, (spec.w, spec.h), false)),
            bar: spec.bar.as_ref().and_then(|f| fill_frame(f, bar_box, true)),
            reflection: spec.reflection.as_ref().and_then(|f| fill_frame(f, bar_box, true)),
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
            Some(if stretch { fit_art(&picture, w, h) } else { picture })
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
        let down = if h > 1 { y as f32 / (h - 1) as f32 } else { 0.0 };
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

/// Blend the part of a picture at a position, showing only what falls inside
/// the clip box `(x, y, w, h)`.
fn blit_clipped(dst: &mut [u8], dst_w: u32, dst_h: u32, src: &Frame, at: (i32, i32), part: (u32, u32, u32, u32), clip: (i32, i32, i32, i32)) {
    let (src_x, src_y, src_w, src_h) = part;
    let x_from = clip.0.max(0);
    let y_from = clip.1.max(0);
    let x_to = (clip.0 + clip.2).min(dst_w as i32);
    let y_to = (clip.1 + clip.3).min(dst_h as i32);
    for row in 0..src_h {
        let sy = src_y + row;
        let dy = at.1 + row as i32;
        if sy >= src.height || dy < y_from || dy >= y_to {
            continue;
        }
        for col in 0..src_w {
            let sx = src_x + col;
            let dx = at.0 + col as i32;
            if sx >= src.width || dx < x_from || dx >= x_to {
                continue;
            }
            let s = (sy as usize * src.width as usize + sx as usize) * 4;
            let d = (dy as usize * dst_w as usize + dx as usize) * 4;
            blend(dst, d, [src.rgba[s], src.rgba[s + 1], src.rgba[s + 2], src.rgba[s + 3]]);
        }
    }
}

/// The spectrum as its engine draws it, clipped to its box: background
/// centred in the box, each bar's bottom `height` rows rising from the
/// origin, the reflection's top rows hanging below it, the topping falling
/// `step` pixels a frame once the bar has dropped away from it, and the
/// foreground over everything.
pub fn draw_spectrum(dst: &mut [u8], dst_w: u32, dst_h: u32, spec: &SpectrumSpec, heights: &[u32], assets: &SpectrumAssets, motion: &mut SpectrumMotion) {
    let clip = (spec.x, spec.y, spec.w as i32, spec.h as i32);
    let centred = |picture: &Frame| {
        (
            spec.x + (spec.w as i32 - picture.width as i32) / 2,
            spec.y + (spec.h as i32 - picture.height as i32) / 2,
        )
    };
    if let Some(background) = &assets.background {
        blit_clipped(dst, dst_w, dst_h, background, centred(background), (0, 0, background.width, background.height), clip);
    }
    let baseline = spec.y + spec.origin_y;
    if motion.toppings.len() != heights.len() {
        motion.toppings = vec![None; heights.len()];
    }
    for (r, &height) in heights.iter().enumerate() {
        let height = height.min(spec.bar_h);
        let bx = spec.x + spec.origin_x + r as i32 * (spec.bar_w + spec.gap) as i32;
        if height > 0 {
            if let Some(bar) = &assets.bar {
                blit_clipped(dst, dst_w, dst_h, bar, (bx, baseline - height as i32), (0, spec.bar_h - height, spec.bar_w, height), clip);
            }
            if let Some(reflection) = &assets.reflection {
                blit_clipped(dst, dst_w, dst_h, reflection, (bx, baseline + spec.reflection_gap), (0, 0, spec.bar_w, height), clip);
            }
        }
        if let (Some((topping_h, topping_step)), Some(bar)) = (spec.topping, &assets.bar) {
            let top = (baseline - height as i32).min(baseline);
            match motion.toppings[r] {
                None => motion.toppings[r] = Some(top),
                Some(sitting) => {
                    if top > sitting + topping_step as i32 + topping_h as i32 {
                        let fallen = sitting + topping_step as i32;
                        motion.toppings[r] = Some(fallen);
                        let src_y = spec.bar_h as i32 - (baseline - fallen) + 1;
                        if src_y >= 0 {
                            blit_clipped(dst, dst_w, dst_h, bar, (bx, fallen), (0, src_y as u32, spec.bar_w, topping_h), clip);
                        }
                    } else {
                        motion.toppings[r] = Some(top - topping_h as i32 - topping_step as i32);
                    }
                }
            }
        }
    }
    if let Some(foreground) = &assets.foreground {
        blit_clipped(dst, dst_w, dst_h, foreground, centred(foreground), (0, 0, foreground.width, foreground.height), clip);
    }
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
                let ratio = (bw as f32 / picture.width as f32).min(bh as f32 / picture.height as f32);
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
fn draw_folder_layers(dst: &mut [u8], dst_w: u32, dst_h: u32, scene: &Scene, pictures: &[Option<FolderPicture>], zorder: ZOrder) {
    for (layer, picture) in scene.folder_layers.iter().zip(pictures.iter()) {
        if layer.spec.zorder != zorder {
            continue;
        }
        let Some(picture) = picture else {
            continue;
        };
        blit_at(dst, dst_w, dst_h, &picture.frame, picture.at);
        if layer.spec.border > 0 {
            let c = layer.spec.border_color;
            draw_border(dst, dst_w, dst_h, (layer.spec.x, layer.spec.y, layer.spec.w, layer.spec.h), layer.spec.border, [c[0], c[1], c[2], 255]);
        }
    }
}

/// Blend a picture at a position with its alpha scaled by `alpha` (255 is as is).
fn blit_alpha(dst: &mut [u8], dst_w: u32, dst_h: u32, src: &Frame, at: (u32, u32), alpha: u8) {
    if alpha == 0 {
        return;
    }
    if alpha == 255 {
        blit_at(dst, dst_w, dst_h, src, at);
        return;
    }
    for y in 0..src.height {
        let dy = at.1 + y;
        if dy >= dst_h {
            break;
        }
        for x in 0..src.width {
            let dx = at.0 + x;
            if dx >= dst_w {
                break;
            }
            let s = (y as usize * src.width as usize + x as usize) * 4;
            let d = (dy as usize * dst_w as usize + dx as usize) * 4;
            let a = (src.rgba[s + 3] as u32 * alpha as u32 / 255) as u8;
            blend(dst, d, [src.rgba[s], src.rgba[s + 1], src.rgba[s + 2], a]);
        }
    }
}

/// The fanart slot of one z-order. A `merge` crossfades the old picture into
/// the new over the transition; a `fade` takes the old one out in the first
/// half and the new one in over the second, or the new one in alone when
/// there is no old picture; `none` shows the picture as it is.
fn draw_fanart(dst: &mut [u8], dst_w: u32, dst_h: u32, fanart: Option<&Fanart>, pictures: (Option<&FolderPicture>, Option<&FolderPicture>), zorder: ZOrder) {
    let Some(fanart) = fanart else {
        return;
    };
    if fanart.spec.zorder != zorder {
        return;
    }
    let (current, previous) = pictures;
    let running = fanart.transition_ms > 0 && fanart.elapsed_ms < fanart.transition_ms && fanart.transition != "none";
    if !running {
        if let Some(picture) = current {
            blit_at(dst, dst_w, dst_h, &picture.frame, picture.at);
        }
        return;
    }
    let p = fanart.elapsed_ms as f32 / fanart.transition_ms as f32;
    let level = |f: f32| (255.0 * f.clamp(0.0, 1.0)) as u8;
    match fanart.transition.as_str() {
        "merge" => {
            if let Some(old) = previous {
                blit_alpha(dst, dst_w, dst_h, &old.frame, old.at, level(1.0 - p));
            }
            if let Some(new) = current {
                blit_alpha(dst, dst_w, dst_h, &new.frame, new.at, level(p));
            }
        }
        _ => match (previous, current) {
            (Some(old), _) if p < 0.5 => blit_alpha(dst, dst_w, dst_h, &old.frame, old.at, level(1.0 - 2.0 * p)),
            (Some(_), Some(new)) => blit_alpha(dst, dst_w, dst_h, &new.frame, new.at, level(2.0 * p - 1.0)),
            (None, Some(new)) => blit_alpha(dst, dst_w, dst_h, &new.frame, new.at, level(p)),
            _ => {}
        },
    }
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

fn blit(dst: &mut [u8], dst_w: u32, dst_h: u32, src: &Frame) {
    let copy_w = dst_w.min(src.width) as usize;
    let copy_h = dst_h.min(src.height) as usize;
    for y in 0..copy_h {
        let from = y * src.width as usize * 4;
        let to = y * dst_w as usize * 4;
        dst[to..to + copy_w * 4].copy_from_slice(&src.rgba[from..from + copy_w * 4]);
    }
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

pub fn draw_text(frame: &mut Frame, x: u32, y: u32, text: &str) {
    use font8x8::UnicodeFonts;
    let scale = 2u32;
    for (index, ch) in text.chars().enumerate() {
        let Some(glyph) = font8x8::BASIC_FONTS.get(ch) else {
            continue;
        };
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..8u32 {
                if bits & (1 << col) == 0 {
                    continue;
                }
                let px = x + index as u32 * 8 * scale + col * scale;
                let py = y + row as u32 * scale;
                for sy in 0..scale {
                    for sx in 0..scale {
                        put(
                            &mut frame.rgba,
                            frame.width,
                            px + sx,
                            py + sy,
                            [240, 240, 240, 255],
                        );
                    }
                }
            }
        }
    }
}

fn put(rgba: &mut [u8], stride: u32, x: u32, y: u32, color: [u8; 4]) {
    let i = ((y * stride + x) * 4) as usize;
    if i + 3 < rgba.len() {
        rgba[i..i + 4].copy_from_slice(&color);
    }
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
        let mut pic = Frame { width: 4, height: 2, rgba: vec![0; 4 * 2 * 4] };
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
        draw_bar(&mut dst, 10, 6, &pic, (2, 1), 2, &spec(Direction::LeftRight, false, false), true);
        assert_eq!((lit(&dst, 10, 2, 1), lit(&dst, 10, 3, 1), lit(&dst, 10, 4, 1)), (1, 2, 0));
        // right-left: the last two columns, at the picture's right end.
        let mut dst = vec![0u8; 10 * 6 * 4];
        draw_bar(&mut dst, 10, 6, &pic, (2, 1), 2, &spec(Direction::RightLeft, false, false), true);
        assert_eq!((lit(&dst, 10, 3, 1), lit(&dst, 10, 4, 1), lit(&dst, 10, 5, 1)), (0, 3, 4));
        // center-edges, left channel: the last two columns end at the origin.
        let mut dst = vec![0u8; 10 * 6 * 4];
        draw_bar(&mut dst, 10, 6, &pic, (5, 1), 2, &spec(Direction::CenterEdges, false, false), true);
        assert_eq!((lit(&dst, 10, 3, 1), lit(&dst, 10, 4, 1), lit(&dst, 10, 5, 1)), (3, 4, 0));
        // edges-center, flipped right channel: the last two columns end at the origin.
        let mut dst = vec![0u8; 10 * 6 * 4];
        draw_bar(&mut dst, 10, 6, &pic, (5, 1), 2, &spec(Direction::EdgesCenter, false, true), false);
        assert_eq!((lit(&dst, 10, 3, 1), lit(&dst, 10, 4, 1), lit(&dst, 10, 5, 1)), (3, 4, 0));
        // bottom-top: the bottom row only.
        let mut dst = vec![0u8; 10 * 6 * 4];
        draw_bar(&mut dst, 10, 6, &pic, (2, 1), 1, &spec(Direction::BottomTop, false, false), true);
        assert_eq!((lit(&dst, 10, 2, 1), lit(&dst, 10, 2, 2)), (0, 1));
        // single: the whole picture moved by w.
        let mut dst = vec![0u8; 10 * 6 * 4];
        draw_bar(&mut dst, 10, 6, &pic, (2, 1), 3, &spec(Direction::LeftRight, true, false), true);
        assert_eq!((lit(&dst, 10, 4, 1), lit(&dst, 10, 5, 1), lit(&dst, 10, 8, 1)), (0, 1, 4));
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
            foreground: None.unwrap_or_default(),
        };
        let assets = SpectrumAssets::load(&spec);
        let mut motion = SpectrumMotion::default();
        let (w, h) = (120u32, 80u32);
        let red = |dst: &[u8], x: i32, y: i32| dst[((y as u32 * w + x as u32) * 4) as usize];
        let blue = |dst: &[u8], x: i32, y: i32| dst[((y as u32 * w + x as u32) * 4 + 2) as usize];
        // Half height on the first bar: 10 rows up from the baseline at y 60, at x 15.
        let mut dst = vec![0u8; (w * h * 4) as usize];
        draw_spectrum(&mut dst, w, h, &spec, &[10, 0], &assets, &mut motion);
        assert_eq!((red(&dst, 15, 59), red(&dst, 15, 50), red(&dst, 15, 49)), (255, 255, 0), "bar covers y 50..59");
        assert_eq!((blue(&dst, 15, 61), blue(&dst, 15, 70), blue(&dst, 15, 71)), (255, 255, 0), "reflection hangs from the gap");
        assert_eq!(red(&dst, 18, 59), 0, "the second bar is silent");
        // The bar drops: the topping stays one step above where it was and falls one step a frame.
        let mut dst = vec![0u8; (w * h * 4) as usize];
        draw_spectrum(&mut dst, w, h, &spec, &[10, 0], &assets, &mut motion);
        let mut dst = vec![0u8; (w * h * 4) as usize];
        draw_spectrum(&mut dst, w, h, &spec, &[2, 0], &assets, &mut motion);
        assert_eq!((red(&dst, 15, 49), red(&dst, 15, 55)), (255, 0), "topping drawn one step above the old top, bar gone there");
        let mut dst = vec![0u8; (w * h * 4) as usize];
        draw_spectrum(&mut dst, w, h, &spec, &[2, 0], &assets, &mut motion);
        assert_eq!((red(&dst, 15, 49), red(&dst, 15, 50)), (0, 255), "a frame later it sits one step lower");
        // Nothing outside the box: a bar past the right edge is clipped.
        let wide = SpectrumSpec { origin_x: 95, ..spec.clone() };
        let mut dst = vec![0u8; (w * h * 4) as usize];
        draw_spectrum(&mut dst, w, h, &wide, &[10, 10], &assets, &mut SpectrumMotion::default());
        assert_eq!((red(&dst, 105, 59), red(&dst, 108, 59), red(&dst, 110, 59)), (255, 255, 0), "second bar starts at 108, clipped at 110");
        // Gradient: first colour at the bottom.
        let g = gradient_frame(1, 3, &[[0, 0, 0, 255], [200, 0, 0, 255]]);
        assert_eq!((g.rgba[0], g.rgba[4], g.rgba[8]), (200, 100, 0));
    }

    #[test]
    fn a_tonearm_drops_tracks_and_lifts_and_a_record_slows_to_a_halt() {
        let spec = TonearmSpec { file: String::new(), pivot_screen: (0, 0), pivot_image: (0, 0), rest: 0.0, start: -20.0, end: -40.0, drop_s: 1.0, lift_s: 1.0 };
        let mut arm = TonearmMotion::default();
        arm.update(&spec, false, 0.0, None, 0);
        assert_eq!((arm.angle(), arm.is_animating()), (0.0, false), "parked");
        arm.update(&spec, true, 50.0, Some(100.0), 0);
        assert!(arm.is_animating(), "dropping");
        arm.update(&spec, true, 50.0, Some(100.0), 500);
        assert!(arm.angle() < -20.0 && arm.angle() > -30.0, "eased past the midpoint: {}", arm.angle());
        arm.update(&spec, true, 50.0, Some(100.0), 1000);
        arm.update(&spec, true, 50.0, Some(100.0), 1001);
        assert_eq!((arm.angle(), arm.is_animating()), (-30.0, false), "tracking at half the track");
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
        assert!((a - 36.0).abs() < 0.01, "60 rpm turns 36 degrees in 100 ms: {a}");
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
    fn a_turned_needle_lands_where_the_theme_origin_sends_it() {
        // Pivot at (50, 80), sprite centre 30 px out. Straight up, the sprite
        // covers y 40..60 at x 50. Turned 90° left, it covers x 10..30 at y 80.
        let needle = sprite(2, 20, [255, 0, 0, 255]);
        let mut rgba = vec![0u8; 100 * 100 * 4];
        blit_rotated(&mut rgba, 100, 100, &needle, (50, 80), 0.0, 30.0);
        assert_eq!(at(&rgba, 100, 50, 45), [255, 0, 0]);
        assert_eq!(at(&rgba, 100, 25, 80), [0, 0, 0]);
        let mut rgba = vec![0u8; 100 * 100 * 4];
        blit_rotated(&mut rgba, 100, 100, &needle, (50, 80), 90.0, 30.0);
        assert_eq!(at(&rgba, 100, 25, 80), [255, 0, 0]);
        assert_eq!(at(&rgba, 100, 50, 45), [0, 0, 0]);
    }

    /// Any TrueType file on the host will do; the test is about placement and
    /// clipping, not the face. Without one the bitmap fallback is exercised.
    fn any_font() -> Option<String> {
        let dirs = ["/usr/share/fonts/truetype", "/usr/share/fonts/TTF", "/usr/share/fonts"];
        fn walk(dir: &Path, depth: u32) -> Option<String> {
            for entry in std::fs::read_dir(dir).ok()?.flatten() {
                let path = entry.path();
                if path.is_dir() && depth > 0 {
                    if let Some(found) = walk(&path, depth - 1) {
                        return Some(found);
                    }
                } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ttf")) {
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
        assert_eq!((0..120).filter(|&x| sample(&frame, x, 0)[0] > 0).count(), 0, "row 0 above the em box is empty");

        let mut clipped = Frame {
            width: 120,
            height: 40,
            rgba: vec![0u8; 120 * 40 * 4],
        };
        let narrow = Text { max_width: 10, ..text };
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
        assert_eq!(motion.offset(10, 2), Some(0.0), "pauses at the end it starts from, as the player does");
        draw_text_moving(&mut frame, &text, Some(&fonts), &mut motion, 500);
        let after = motion.offset(10, 2).unwrap();
        assert!(after > 39.0 && after < 41.0, "100 px/s over the 0.4 s since the last frame: {after}");
        for step in 2..200u64 {
            draw_text_moving(&mut frame, &text, Some(&fonts), &mut motion, step * 100);
        }
        let line_w = render_text(Some(&fonts), TextStyle::Regular, 20, [255, 255, 255], &text.text, 0).unwrap().width;
        let limit = (line_w - 30) as f32;
        let at_end = motion.offset(10, 2).unwrap();
        assert!(at_end >= 0.0 && at_end <= limit, "bounces inside 0..limit: {at_end} of {limit}");
        assert_eq!((0..10).filter(|&x| sample(&frame, x, 10)[3] > 0).count(), 0, "nothing left of the box");
        assert_eq!((41..200).filter(|&x| sample(&frame, x, 10)[3] > 0).count(), 0, "nothing right of the box");

        text.text = "loop  ".repeat(3);
        text.direction = ScrollDirection::Ltr;
        text.loop_thirds = true;
        text.max_width = 20;
        draw_text_moving(&mut frame, &text, Some(&fonts), &mut motion, 20_000);
        let segment = render_text(Some(&fonts), TextStyle::Regular, 20, [255, 255, 255], &text.text, 0).unwrap().width / 3;
        for step in 1..400u64 {
            draw_text_moving(&mut frame, &text, Some(&fonts), &mut motion, 20_000 + step * 50);
        }
        let looped = motion.offset(10, 2).unwrap();
        assert!(looped >= 0.0 && looped < segment as f32, "wraps by one segment: {looped} of {segment}");
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
        assert_eq!((icon.width, icon.height), (50, 25), "fits the box, keeps the aspect");
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
        assert_eq!(sample(&art, 0, 0)[3], 255, "black in the mask keeps the picture");
        assert_eq!(sample(&art, 3, 3)[3], 0, "white in the mask cuts it");

        let mut rgba = vec![0u8; 8 * 8 * 4];
        draw_border(&mut rgba, 8, 8, (2, 2, 4, 4), 1, [9, 9, 9, 255]);
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
        assert_eq!(sample(&frame, 59, 17)[..3], [1, 2, 3], "right-aligned, vertically centred");
        assert_eq!(sample(&frame, 20, 17)[3], 0, "nothing on the left of the box");
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
        for px in rgba.chunks_exact_mut(4) {
            px.copy_from_slice(&[0, 0, 0, 255]);
        }
        let layer = sprite(1, 1, [200, 100, 0, 128]);
        blit_at(&mut rgba, 4, 4, &layer, (1, 1));
        assert_eq!(at(&rgba, 4, 1, 1), [100, 50, 0]);
    }
}
