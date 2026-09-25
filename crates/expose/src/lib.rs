//! Raster a [`plot::Scene`] into one RGBA frame.
//! This station does not poll a FIFO or talk to the player.

use std::path::Path;

use plot::Scene;

const BG: [u8; 4] = [12, 12, 16, 255];
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
        },
    )
}

/// `background` is the theme picture. It is copied in place. It is not scaled.
/// A theme is authored at its own resolution, so a mismatch leaves the dark fill.
pub struct Stack<'a> {
    pub screen: Option<&'a Frame>,
    pub face: Option<&'a Frame>,
    pub front: Option<&'a Frame>,
    pub needle: Option<&'a Frame>,
    pub face_at: (u32, u32),
}

/// Theme order: full-screen picture, meter face, needles, meter foreground.
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
    Frame {
        width,
        height,
        rgba,
    }
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
