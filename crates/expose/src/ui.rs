//! Primitives for a face drawn over the theme: a rectangle filled with an
//! alpha, a frame blitted with an alpha, a line of text set in fonts. Thin
//! wrappers over the renderer's own painters, for what a face draws on
//! top of the picture after the theme has been rastered.

use crate::{blit_op, render_text, Band, Fonts, Frame, Op, TextStyle};

/// Fill a rectangle, `rgba` blended over what is there by its alpha. Parts
/// outside the frame are clipped away.
pub fn fill(frame: &mut Frame, x: i32, y: i32, w: u32, h: u32, rgba: [u8; 4]) {
    let (fw, fh) = (frame.width as i32, frame.height as i32);
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w as i32).min(fw);
    let y1 = (y + h as i32).min(fh);
    if x1 <= x0 || y1 <= y0 || rgba[3] == 0 {
        return;
    }
    let a = rgba[3] as u32;
    for row in y0..y1 {
        let start = (row * fw + x0) as usize * 4;
        let end = (row * fw + x1) as usize * 4;
        for px in frame.rgba[start..end].as_chunks_mut::<4>().0 {
            for c in 0..3 {
                px[c] = ((rgba[c] as u32 * a + px[c] as u32 * (255 - a)) / 255) as u8;
            }
            px[3] = px[3].max(rgba[3]);
        }
    }
}

/// Blit a frame at a place, its own alpha scaled by `alpha`.
pub fn blit(frame: &mut Frame, src: &Frame, x: i32, y: i32, alpha: u8) {
    if alpha == 0 {
        return;
    }
    let mut op = blit_op(src, (x, y));
    if let Op::Blit { alpha: a, .. } = &mut op {
        *a = alpha;
    }
    op.paint(&mut Band::whole(frame));
}

/// Set a line of text on its own transparent frame, to measure or to
/// place: `None` when the fonts have no face for the style or the text is
/// empty.
pub fn line(
    fonts: &Fonts,
    style: TextStyle,
    size: u32,
    color: [u8; 3],
    text: &str,
) -> Option<Frame> {
    render_text(Some(fonts), style, size, color, text, 0)
}

/// Set a line of text and blit it at a place; the width and height of the
/// line as set, or zero by zero when the fonts have no face for the style
/// or the text is empty.
#[allow(clippy::too_many_arguments)]
pub fn text(
    frame: &mut Frame,
    fonts: &Fonts,
    style: TextStyle,
    size: u32,
    color: [u8; 3],
    x: i32,
    y: i32,
    text: &str,
    max_width: u32,
) -> (u32, u32) {
    match render_text(Some(fonts), style, size, color, text, max_width) {
        Some(line) => {
            blit(frame, &line, x, y, 255);
            (line.width, line.height)
        }
        None => (0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Blend;

    fn frame(w: u32, h: u32, rgba: [u8; 4]) -> Frame {
        Frame {
            blend: Blend::Normal,
            width: w,
            height: h,
            rgba: rgba.repeat((w * h) as usize),
        }
    }

    #[test]
    fn a_fill_blends_by_its_alpha_and_clips_to_the_frame() {
        fn at(f: &Frame, x: usize, y: usize) -> [u8; 4] {
            let i = (y * f.width as usize + x) * 4;
            [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2], f.rgba[i + 3]]
        }
        let mut f = frame(4, 4, [0, 0, 0, 255]);
        fill(&mut f, 2, 2, 10, 10, [255, 255, 255, 128]);
        assert_eq!(
            at(&f, 0, 0),
            [0, 0, 0, 255],
            "outside the rectangle untouched"
        );
        assert_eq!(at(&f, 3, 3)[0], 128, "half white over black");
        fill(&mut f, -3, -3, 4, 4, [0, 255, 0, 255]);
        assert_eq!(
            at(&f, 0, 0),
            [0, 255, 0, 255],
            "clipped at the frame's edge"
        );
        fill(&mut f, 0, 0, 4, 4, [255, 0, 0, 0]);
        assert_eq!(
            at(&f, 0, 0),
            [0, 255, 0, 255],
            "an alpha of zero paints nothing"
        );
    }

    #[test]
    fn a_blit_lands_where_asked_and_text_without_fonts_sets_nothing() {
        let mut f = frame(4, 2, [0, 0, 0, 255]);
        let dot = frame(1, 1, [255, 255, 255, 255]);
        blit(&mut f, &dot, 3, 1, 255);
        assert_eq!(&f.rgba[7 * 4..7 * 4 + 3], [255, 255, 255]);
        let fonts = Fonts::default();
        assert_eq!(
            text(
                &mut f,
                &fonts,
                TextStyle::Regular,
                12,
                [255, 255, 255],
                0,
                0,
                "x",
                0
            ),
            (0, 0)
        );
    }
}
