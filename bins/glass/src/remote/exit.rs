//! The way out of a remote by touch alone. A finger held still for two
//! seconds anywhere on the picture opens a page of the display's own, over
//! everything, with Leave full screen, Quit the remote and Cancel: for a
//! panel with only its touch screen, whatever the theme shows, whether or
//! not the Glass interface is on, and with no other device at hand for the
//! settings page. Nothing of it on the player's own screen.

use std::time::{Duration, Instant};

use expose::{bitmap_line, fit_art, render_text, ui, Fonts, Frame};
use lead::TextStyle;
use pane::PointerKind;

/// How long a finger must stay before the page opens.
pub const HOLD: Duration = Duration::from_secs(2);
/// How far a held finger may wander, in picture pixels, and still be held.
const SLIP: i32 = 12;

/// What the page asks of the display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    /// The window leaves full screen, the remote still showing.
    LeaveFullscreen,
    /// The remote leaves this screen.
    Quit,
}

/// A row of the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Window,
    Quit,
    Cancel,
}

#[derive(Debug, Default)]
struct Page {
    /// The second screen: Quit asked once more.
    asking: bool,
    pressed: Option<Row>,
}

/// How long the hint at the bottom of the picture stays after a start.
pub const HINT_FOR: Duration = Duration::from_secs(10);
/// The hint's words, on the picture and on the status screen.
pub const HINT: &str = "Hold a finger on the screen for two seconds to leave full screen or quit";

/// The hold and the page, between frames.
#[derive(Debug, Default)]
pub struct ExitTouch {
    /// A finger down and still: where and since when.
    hold: Option<(i32, i32, Instant)>,
    page: Option<Page>,
    /// The lift of the finger that opened the page is the page's, not the
    /// theme's or the face's, which saw the finger go down.
    swallow_up: bool,
    /// The hint at the bottom of the picture, until: for the first seconds
    /// after the remote starts, and gone at the first touch.
    hint_until: Option<Instant>,
}

/// The page's rows, from the top, and the header above them.
fn rows(asking: bool) -> &'static [(Row, &'static str)] {
    if asking {
        &[(Row::Quit, "Quit"), (Row::Cancel, "Cancel")]
    } else {
        &[
            (Row::Window, "Leave full screen"),
            (Row::Quit, "Quit the remote"),
            (Row::Cancel, "Cancel"),
        ]
    }
}

/// The page's box and row height for a picture of `w` by `h`.
fn layout(w: u32, h: u32, asking: bool) -> (i32, i32, u32, u32, u32, u32) {
    let row_h = (h / 9).clamp(36, 96);
    let header = row_h * 3 / 4;
    let pad = row_h / 4;
    let n = rows(asking).len() as u32;
    let box_w = (w * 3 / 5).clamp(220, 640).min(w);
    let box_h = header + n * row_h + (n + 2) * pad;
    let x = (w as i32 - box_w as i32) / 2;
    let y = (h as i32 - box_h as i32) / 2;
    (x, y, box_w, box_h, row_h, header)
}

/// The row under a point, if any.
fn row_at(x: i32, y: i32, w: u32, h: u32, asking: bool) -> Option<Row> {
    let (bx, by, bw, _, row_h, header) = layout(w, h, asking);
    let pad = (row_h / 4) as i32;
    if x < bx + pad || x >= bx + bw as i32 - pad {
        return None;
    }
    let mut top = by + header as i32 + pad;
    for (row, _) in rows(asking) {
        if y >= top && y < top + row_h as i32 {
            return Some(*row);
        }
        top += row_h as i32 + pad;
    }
    None
}

impl ExitTouch {
    pub fn open(&self) -> bool {
        self.page.is_some()
    }

    /// The hint shown until a moment set for the whole run, so a session
    /// started anew within the first seconds keeps showing it.
    pub fn hint_until_at(&mut self, until: Instant) {
        self.hint_until = Some(until);
    }

    pub fn hint_showing(&self, now: Instant) -> bool {
        self.hint_until.is_some_and(|until| now < until)
    }

    /// Whether the picture has anything of this on it: the page, or the hint.
    pub fn drawing(&self, now: Instant) -> bool {
        self.open() || self.hint_showing(now)
    }

    /// A pointer event in picture pixels: whether it is the page's, so the
    /// theme's controls and the face are not to see it, and what the page
    /// asks of the display on it.
    pub fn pointer(
        &mut self,
        kind: PointerKind,
        x: i32,
        y: i32,
        now: Instant,
        w: u32,
        h: u32,
    ) -> (bool, Option<Ask>) {
        // The first touch takes the hint away: the user has found the screen.
        self.hint_until = None;
        // The lift of the finger that opened the page: the page's, and no
        // tap on it, beside it, or on the theme under it.
        if self.swallow_up {
            match kind {
                PointerKind::Up => {
                    self.swallow_up = false;
                    return (true, None);
                }
                PointerKind::Move => return (true, None),
                PointerKind::Down => self.swallow_up = false,
            }
        }
        if let Some(page) = self.page.as_mut() {
            let row = row_at(x, y, w, h, page.asking);
            match kind {
                PointerKind::Down => page.pressed = row,
                PointerKind::Move => {}
                PointerKind::Up => {
                    let was = page.pressed.take();
                    if was.is_none() && row.is_none() {
                        // A tap beside the page shuts it.
                        self.page = None;
                        return (true, None);
                    }
                    if was != row {
                        return (true, None);
                    }
                    match row {
                        Some(Row::Window) => {
                            self.page = None;
                            return (true, Some(Ask::LeaveFullscreen));
                        }
                        Some(Row::Quit) if !page.asking => page.asking = true,
                        Some(Row::Quit) => {
                            self.page = None;
                            return (true, Some(Ask::Quit));
                        }
                        Some(Row::Cancel) | None => self.page = None,
                    }
                }
            }
            return (true, None);
        }
        match kind {
            PointerKind::Down => {
                self.hold = Some((x, y, now));
                (false, None)
            }
            PointerKind::Move => {
                if let Some((hx, hy, _)) = self.hold {
                    if (x - hx).abs() > SLIP || (y - hy).abs() > SLIP {
                        self.hold = None;
                    }
                }
                (false, None)
            }
            PointerKind::Up => {
                self.hold = None;
                (false, None)
            }
        }
    }

    /// Once a frame: a finger held still long enough opens the page, and
    /// says so once. The lift that follows is the page's.
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.page.is_some() {
            return false;
        }
        if let Some((_, _, since)) = self.hold {
            if now.duration_since(since) >= HOLD {
                self.hold = None;
                self.page = Some(Page::default());
                self.swallow_up = true;
                return true;
            }
        }
        false
    }

    /// The page over the picture: the picture dimmed, a box in the middle,
    /// a header and the rows, the pressed one lit. The theme's fonts where
    /// they have a face for the style, the display's own bitmap font else.
    pub fn draw(&self, frame: &mut Frame, fonts: &Fonts, now: Instant) {
        let Some(page) = self.page.as_ref() else {
            if self.hint_showing(now) {
                draw_hint(frame, fonts);
            }
            return;
        };
        let (w, h) = (frame.width, frame.height);
        let (bx, by, bw, bh, row_h, header) = layout(w, h, page.asking);
        let pad = row_h / 4;
        ui::fill(frame, 0, 0, w, h, [0, 0, 0, 150]);
        ui::fill(frame, bx, by, bw, bh, [24, 24, 26, 245]);
        let size = (row_h * 2 / 5).max(14);
        let ink = [230, 230, 230];
        // The theme's font for the style, else another of its faces, else
        // the display's own bitmap font scaled to the size: the page is
        // never without its words.
        let set = |style: TextStyle, size: u32, text: &str| -> Option<Frame> {
            words(fonts, style, size, ink, text)
        };
        let title = if page.asking {
            "Quit the remote?"
        } else {
            "This remote"
        };
        if let Some(line) = set(TextStyle::Bold, size * 9 / 10, title) {
            let x = bx + (bw as i32 - line.width as i32) / 2;
            let y = by + pad as i32 + (header as i32 - line.height as i32) / 2;
            ui::blit(frame, &line, x, y, 255);
        }
        let mut top = by + header as i32 + pad as i32;
        for (row, label) in rows(page.asking) {
            let lit = page.pressed == Some(*row);
            let colour = match (row, lit) {
                (Row::Quit, true) if page.asking => [170, 40, 40, 255],
                (Row::Quit, false) if page.asking => [130, 30, 30, 255],
                (_, true) => [72, 72, 78, 255],
                (_, false) => [42, 42, 46, 255],
            };
            ui::fill(frame, bx + pad as i32, top, bw - 2 * pad, row_h, colour);
            if let Some(line) = set(TextStyle::Regular, size, label) {
                let x = bx + (bw as i32 - line.width as i32) / 2;
                let y = top + (row_h as i32 - line.height as i32) / 2;
                ui::blit(frame, &line, x, y, 255);
            }
            top += row_h as i32 + pad as i32;
        }
    }
}

/// A line of words in the theme's font for the style, else another of its
/// faces, else the display's own bitmap font scaled to the size.
fn words(fonts: &Fonts, style: TextStyle, size: u32, ink: [u8; 3], text: &str) -> Option<Frame> {
    [style, TextStyle::Regular, TextStyle::Bold, TextStyle::Light]
        .into_iter()
        .find_map(|s| render_text(Some(fonts), s, size, ink, text, 0))
        .or_else(|| {
            let line = bitmap_line(text);
            let h = size.max(8);
            let w = (line.width * h / line.height.max(1)).max(1);
            Some(fit_art(&line, w, h))
        })
}

/// The hint: a dark strip along the bottom of the picture with the words.
fn draw_hint(frame: &mut Frame, fonts: &Fonts) {
    let (w, h) = (frame.width, frame.height);
    let strip = (h / 12).clamp(28, 72);
    let size = (strip * 2 / 5).max(12);
    ui::fill(frame, 0, h as i32 - strip as i32, w, strip, [0, 0, 0, 170]);
    if let Some(line) = words(fonts, TextStyle::Regular, size, [235, 235, 235], HINT) {
        let x = (w as i32 - line.width as i32) / 2;
        let y = h as i32 - strip as i32 + (strip as i32 - line.height as i32) / 2;
        ui::blit(frame, &line, x, y, 255);
    }
}

/// The movie's box on a remote that cannot show the picture: a dark panel
/// over the box with the words, line under line, sized to the box, so the
/// screen itself says what is missing rather than standing empty.
pub fn draw_missing(frame: &mut Frame, fonts: &Fonts, boxed: (u32, u32, u32, u32), lines: &[&str]) {
    let (bx, by, bw, bh) = boxed;
    if bw < 8 || bh < 8 || lines.is_empty() {
        return;
    }
    ui::fill(frame, bx as i32, by as i32, bw, bh, [16, 16, 18, 235]);
    let size = (bh / 10).clamp(14, 40).min(bw / 20);
    let rendered: Vec<Frame> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, text)| {
            let ink = if i == 0 { [240, 240, 240] } else { [200, 200, 204] };
            let style = if i == 0 { TextStyle::Bold } else { TextStyle::Regular };
            words(fonts, style, if i == 0 { size } else { size * 4 / 5 }, ink, text)
        })
        .collect();
    let gap = size / 2;
    let total: u32 = rendered.iter().map(|l| l.height).sum::<u32>() + gap * rendered.len().saturating_sub(1) as u32;
    let mut y = by as i32 + (bh as i32 - total as i32) / 2;
    for line in &rendered {
        let line = if line.width > bw.saturating_sub(16) {
            fit_art(line, bw.saturating_sub(16).max(1), (line.height * bw.saturating_sub(16).max(1) / line.width.max(1)).max(1))
        } else {
            line.clone()
        };
        let x = bx as i32 + (bw as i32 - line.width as i32) / 2;
        ui::blit(frame, &line, x, y, 255);
        y += line.height as i32 + gap as i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: u32 = 1280;
    const H: u32 = 720;

    fn centre_of(row: Row, asking: bool) -> (i32, i32) {
        let (bx, by, bw, _, row_h, header) = layout(W, H, asking);
        let pad = (row_h / 4) as i32;
        let index = rows(asking).iter().position(|(r, _)| *r == row).unwrap() as i32;
        (
            bx + bw as i32 / 2,
            by + header as i32 + pad + index * (row_h as i32 + pad) + row_h as i32 / 2,
        )
    }

    fn tap(exit: &mut ExitTouch, at: (i32, i32), now: Instant) -> (bool, Option<Ask>) {
        exit.pointer(PointerKind::Down, at.0, at.1, now, W, H);
        exit.pointer(PointerKind::Up, at.0, at.1, now, W, H)
    }

    /// A finger held still for two seconds opens the page; one that moves
    /// or lifts before does nothing, and the theme sees those touches.
    #[test]
    fn a_held_finger_opens_the_page_and_its_lift_is_the_pages() {
        let t0 = Instant::now();
        let mut exit = ExitTouch::default();
        assert_eq!(
            exit.pointer(PointerKind::Down, 100, 100, t0, W, H),
            (false, None)
        );
        assert!(!exit.tick(t0 + Duration::from_millis(1500)));
        assert!(!exit.open());
        assert!(exit.tick(t0 + HOLD), "held long enough");
        assert!(exit.open());
        assert!(!exit.tick(t0 + HOLD), "said once");
        assert_eq!(
            exit.pointer(PointerKind::Up, 100, 100, t0 + HOLD, W, H),
            (true, None),
            "the lift is the page's"
        );
        assert!(exit.open());
        // A finger that wanders is not held.
        let mut moved = ExitTouch::default();
        moved.pointer(PointerKind::Down, 100, 100, t0, W, H);
        moved.pointer(PointerKind::Move, 100 + SLIP + 1, 100, t0, W, H);
        assert!(!moved.tick(t0 + HOLD));
        // A finger lifted early is a tap for the theme, and nothing here.
        let mut tapped = ExitTouch::default();
        tapped.pointer(PointerKind::Down, 100, 100, t0, W, H);
        assert_eq!(
            tapped.pointer(PointerKind::Up, 100, 100, t0, W, H),
            (false, None)
        );
        assert!(!tapped.tick(t0 + HOLD));
    }

    /// Leave full screen acts on the tap; Quit asks once more; Cancel and
    /// a tap beside the page shut it.
    #[test]
    fn the_rows_leave_full_screen_at_once_and_quit_after_a_question() {
        let t0 = Instant::now();
        let mut exit = ExitTouch::default();
        exit.pointer(PointerKind::Down, 10, 10, t0, W, H);
        exit.tick(t0 + HOLD);
        exit.pointer(PointerKind::Up, 10, 10, t0 + HOLD, W, H);
        assert_eq!(
            tap(&mut exit, centre_of(Row::Window, false), t0),
            (true, Some(Ask::LeaveFullscreen))
        );
        assert!(!exit.open(), "the page shuts with the ask");

        exit.pointer(PointerKind::Down, 10, 10, t0, W, H);
        exit.tick(t0 + HOLD);
        exit.pointer(PointerKind::Up, 10, 10, t0 + HOLD, W, H);
        assert_eq!(
            tap(&mut exit, centre_of(Row::Quit, false), t0),
            (true, None)
        );
        assert!(
            exit.open() && exit.page.as_ref().unwrap().asking,
            "quit asks first"
        );
        assert_eq!(
            tap(&mut exit, centre_of(Row::Cancel, true), t0),
            (true, None)
        );
        assert!(!exit.open(), "cancel shuts the question");

        exit.pointer(PointerKind::Down, 10, 10, t0, W, H);
        exit.tick(t0 + HOLD);
        exit.pointer(PointerKind::Up, 10, 10, t0 + HOLD, W, H);
        tap(&mut exit, centre_of(Row::Quit, false), t0);
        assert_eq!(
            tap(&mut exit, centre_of(Row::Quit, true), t0),
            (true, Some(Ask::Quit))
        );
        assert!(!exit.open());

        exit.pointer(PointerKind::Down, 10, 10, t0, W, H);
        exit.tick(t0 + HOLD);
        exit.pointer(PointerKind::Up, 10, 10, t0 + HOLD, W, H);
        assert_eq!(tap(&mut exit, (5, 5), t0), (true, None), "beside the page");
        assert!(!exit.open());
    }

    /// The hint stands at the bottom for its seconds after a start, and goes
    /// at the first touch or when its time is up.
    #[test]
    fn the_hint_shows_after_a_start_until_a_touch_or_its_time() {
        let t0 = Instant::now();
        let mut exit = ExitTouch::default();
        assert!(!exit.hint_showing(t0));
        exit.hint_until_at(t0 + HINT_FOR);
        assert!(exit.hint_showing(t0 + Duration::from_secs(3)) && exit.drawing(t0));
        assert!(!exit.hint_showing(t0 + HINT_FOR), "its time is up");
        exit.hint_until_at(t0 + HINT_FOR);
        exit.pointer(PointerKind::Down, 5, 5, t0, W, H);
        assert!(
            !exit.hint_showing(t0 + Duration::from_secs(1)),
            "a touch takes it away"
        );
        let mut frame = Frame {
            width: W,
            height: H,
            rgba: vec![200; (W * H * 4) as usize],
            ..Frame::default()
        };
        let mut shown = ExitTouch::default();
        shown.hint_until_at(t0 + HINT_FOR);
        shown.draw(&mut frame, &Fonts::default(), t0);
        let i = ((H - 4) * W + W / 2) as usize * 4;
        assert!(frame.rgba[i] < 200, "the strip darkens the bottom");
        let j = ((4 * W) + W / 2) as usize * 4;
        assert_eq!(frame.rgba[j], 200, "the rest of the picture stays");
    }

    /// The page is drawn over the picture: the middle of the box is the
    /// page's, a corner stays the dimmed picture.
    #[test]
    fn the_page_is_drawn_over_the_picture() {
        let t0 = Instant::now();
        let mut exit = ExitTouch::default();
        exit.pointer(PointerKind::Down, 10, 10, t0, W, H);
        exit.tick(t0 + HOLD);
        let mut frame = Frame {
            width: W,
            height: H,
            rgba: vec![200; (W * H * 4) as usize],
            ..Frame::default()
        };
        exit.draw(&mut frame, &Fonts::default(), t0 + HOLD);
        let px = |x: u32, y: u32| {
            let i = ((y * W + x) * 4) as usize;
            [frame.rgba[i], frame.rgba[i + 1], frame.rgba[i + 2]]
        };
        assert!(px(2, 2)[0] < 200, "the corner is dimmed");
        let (bx, by, bw, _, row_h, header) = layout(W, H, false);
        let pad = row_h / 4;
        let first = px(
            (bx + bw as i32 / 4) as u32,
            (by + header as i32 + pad as i32 + row_h as i32 / 2) as u32,
        );
        assert_ne!(
            first,
            px(2, 2),
            "the first row stands out from the dimmed picture"
        );
        // The row carries its words: some pixel across its middle is the ink, not the fill.
        let y = (by + header as i32 + pad as i32 + row_h as i32 / 2) as u32;
        let inked = (bx as u32..bx as u32 + bw).any(|x| px(x, y)[0] > 150);
        assert!(
            inked,
            "the first row has its words, in the theme's font or the bitmap one"
        );
        if let Some(path) = std::env::var_os("GLASS_EXIT_PAGE_PNG") {
            expose::write_png(std::path::Path::new(&path), &frame).unwrap();
        }
    }
}
