//! Raster a [`plot::Scene`] into one RGBA frame.
//! This station does not poll a FIFO or talk to the player.

use std::path::Path;

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use lead::{FontFiles, TextStyle, TypeAlign, TypeMode};
use plot::{Scene, Text, TypeArea};

const BG: [u8; 4] = [12, 12, 16, 255];

/// Theme fonts, loaded once per run. A style whose file is missing or
/// unreadable stays `None`, and its texts fall back to the built-in bitmap font.
#[derive(Default)]
pub struct Fonts {
    light: Option<FontVec>,
    regular: Option<FontVec>,
    bold: Option<FontVec>,
    digi: Option<FontVec>,
}

impl Fonts {
    pub fn load(files: &FontFiles) -> Self {
        let read = |path: &str| -> Option<FontVec> {
            if path.is_empty() {
                return None;
            }
            FontVec::try_from_vec(std::fs::read(path).ok()?).ok()
        };
        Self {
            light: read(&files.light),
            regular: read(&files.regular),
            bold: read(&files.bold),
            digi: read(&files.digi),
        }
    }

    fn get(&self, style: TextStyle) -> Option<&FontVec> {
        match style {
            TextStyle::Light => self.light.as_ref(),
            TextStyle::Regular => self.regular.as_ref(),
            TextStyle::Bold => self.bold.as_ref(),
            TextStyle::Digi => self.digi.as_ref(),
        }
    }

    /// How many of the four styles have a font.
    pub fn loaded(&self) -> usize {
        [&self.light, &self.regular, &self.bold, &self.digi]
            .iter()
            .filter(|f| f.is_some())
            .count()
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
            face_at: (0, 0),
            fonts: None,
            art: None,
            icon: None,
        },
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
    pub needle: Option<&'a Frame>,
    pub face_at: (u32, u32),
    /// Theme fonts for `Scene::texts`. `None` draws every text in the bitmap font.
    pub fonts: Option<&'a Fonts>,
    /// Album art already stretched to the box in `Scene::art`.
    pub art: Option<&'a Frame>,
    /// Type icon already fitted for `Scene::type_area`, tinted when it was an SVG.
    pub icon: Option<&'a Frame>,
}

/// Theme order: full-screen picture, album art, meter face, needles, meter
/// foreground, then the texts.
pub fn raster_over(scene: &Scene, stack: Stack<'_>) -> Frame {
    let width = scene.width.max(1);
    let height = scene.height.max(1);
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for px in rgba.chunks_exact_mut(4) {
        px.copy_from_slice(&BG);
    }
    if let Some(screen) = stack.screen {
        blit(&mut rgba, width, height, screen);
    }
    if let (Some(art), Some(place)) = (stack.art, &scene.art) {
        blit_at(&mut rgba, width, height, art, (place.x, place.y));
        if place.border > 0 {
            let color = [place.border_color[0], place.border_color[1], place.border_color[2], 255];
            draw_border(&mut rgba, width, height, (place.x, place.y, place.w, place.h), place.border, color);
        }
    }
    if let Some(face) = stack.face {
        blit_at(&mut rgba, width, height, face, stack.face_at);
    }
    if let (Some(sprite), Some((start, stop, distance))) = (stack.needle, scene.needle) {
        if let Some(at) = scene.left_at {
            let angle = start + (stop - start) * scene.left;
            blit_rotated(&mut rgba, width, height, sprite, at, angle, distance);
        }
        if let Some(at) = scene.right_at {
            let angle = start + (stop - start) * scene.right;
            blit_rotated(&mut rgba, width, height, sprite, at, angle, distance);
        }
    }
    if let Some(front) = stack.front {
        blit_at(&mut rgba, width, height, front, stack.face_at);
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
        draw_text_styled(&mut frame, text, stack.fonts);
    }
    if let Some(area) = &scene.type_area {
        draw_type_area(&mut frame, area, stack.icon, stack.fonts);
    }
    frame
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
    let font = fonts.and_then(|f| f.get(style))?;
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
fn blit_rotated(
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    src: &Frame,
    at: (u32, u32),
    degrees: f32,
    distance: f32,
) {
    if src.width == 0 || src.height == 0 || dst_w == 0 || dst_h == 0 {
        return;
    }
    let rad = degrees.to_radians();
    let (sin, cos) = rad.sin_cos();
    let half_w = src.width as f32 / 2.0;
    let half_h = src.height as f32 / 2.0;
    let center_x = at.0 as f32 - distance * sin;
    let center_y = at.1 as f32 - distance * cos;
    let extent_x = (half_w * cos).abs() + (half_h * sin).abs() + 1.0;
    let extent_y = (half_w * sin).abs() + (half_h * cos).abs() + 1.0;
    let x_from = ((center_x - extent_x).floor() as i32).max(0);
    let x_to = ((center_x + extent_x).ceil() as i32).min(dst_w as i32 - 1);
    let y_from = ((center_y - extent_y).floor() as i32).max(0);
    let y_to = ((center_y + extent_y).ceil() as i32).min(dst_h as i32 - 1);
    for dy in y_from..=y_to {
        for dx in x_from..=x_to {
            let vx = dx as f32 + 0.5 - center_x;
            let vy = dy as f32 + 0.5 - center_y;
            // Inverse of the rotation that carries the sprite onto the frame.
            let sx = half_w + vx * cos - vy * sin;
            let sy = half_h + vx * sin + vy * cos;
            if sx < 0.0 || sy < 0.0 || sx > src.width as f32 || sy > src.height as f32 {
                continue;
            }
            let px = sample_bilinear(src, sx, sy);
            if px[3] == 0 {
                continue;
            }
            let d = (dy as usize * dst_w as usize + dx as usize) * 4;
            blend(dst, d, px);
        }
    }
}

fn place(fallback: Rect, at: Option<(u32, u32)>, width: u32, height: u32) -> Rect {
    let Some((x, y)) = at else {
        return fallback;
    };
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
