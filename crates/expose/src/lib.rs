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
    raster_over(scene, None, None)
}

/// `background` is the theme picture. It is copied in place. It is not scaled.
/// A theme is authored at its own resolution, so a mismatch leaves the dark fill.
pub fn raster_over(scene: &Scene, background: Option<&Frame>, indicator: Option<&Frame>) -> Frame {
    let width = scene.width.max(1);
    let height = scene.height.max(1);
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for px in rgba.chunks_exact_mut(4) {
        px.copy_from_slice(&BG);
    }
    if let Some(background) = background {
        blit(&mut rgba, width, height, background);
        if let (Some(sprite), Some((start, stop))) = (indicator, scene.needle) {
            if let Some(at) = scene.left_at {
                let angle = start + (stop - start) * scene.left;
                blit_rotated(&mut rgba, width, height, sprite, at, angle);
            }
            if let Some(at) = scene.right_at {
                let angle = start + (stop - start) * scene.right;
                blit_rotated(&mut rgba, width, height, sprite, at, angle);
            }
        }
    } else {
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

fn blit_rotated(dst: &mut [u8], dst_w: u32, dst_h: u32, src: &Frame, at: (u32, u32), degrees: f32) {
    let rad = degrees.to_radians();
    let (sin, cos) = rad.sin_cos();
    let pivot_x = src.width as f32 / 2.0;
    let pivot_y = src.height as f32;
    let span = (src.width.max(src.height) as i32) * 2;
    for dy in -span..span {
        for dx in -span..span {
            let sx = pivot_x + dx as f32 * cos - dy as f32 * sin;
            let sy = pivot_y + dx as f32 * sin + dy as f32 * cos;
            let ix = sx.floor() as i32;
            let iy = sy.floor() as i32;
            if ix < 0 || iy < 0 || ix >= src.width as i32 || iy >= src.height as i32 {
                continue;
            }
            let s = (iy as usize * src.width as usize + ix as usize) * 4;
            if src.rgba[s + 3] == 0 {
                continue;
            }
            let ox = at.0 as i32 + dx;
            let oy = at.1 as i32 + dy;
            if ox < 0 || oy < 0 || ox >= dst_w as i32 || oy >= dst_h as i32 {
                continue;
            }
            let d = (oy as usize * dst_w as usize + ox as usize) * 4;
            dst[d..d + 4].copy_from_slice(&src.rgba[s..s + 4]);
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
}
