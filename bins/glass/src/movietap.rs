//! The tap in the movie's box: a finger down in the box and up within a
//! moment and a few pixels brings the picture to the whole screen, and the
//! next tap brings it back. While the movie shows, every touch that begins
//! in its box is the movie's: neither the face's controls, nor the theme's
//! touch rules, nor exit-on-touch see it. Pure: the box is given, the
//! answer says what was taken and whether the tap landed.

use std::time::{Duration, Instant};

use pane::PointerKind;

/// A tap lifts within this long, and this close to where it went down.
const TAP_WITHIN: Duration = Duration::from_millis(400);
const TAP_SLACK: i32 = 24;

#[derive(Default)]
pub struct MovieTap {
    /// The finger down in the box: where and when.
    down: Option<(i32, i32, Instant)>,
}

impl MovieTap {
    /// A pointer event in frame pixels against the movie's box `boxed`
    /// (x, y, w, h in frame pixels; none while no movie shows). Answers
    /// (taken, toggled): `taken` when the event is the movie's and goes no
    /// further, `toggled` when a tap landed.
    pub fn pointer(
        &mut self,
        kind: PointerKind,
        x: i32,
        y: i32,
        now: Instant,
        boxed: Option<(u32, u32, u32, u32)>,
    ) -> (bool, bool) {
        let Some(boxed) = boxed else {
            self.down = None;
            return (false, false);
        };
        let inside = |px: i32, py: i32| {
            px >= boxed.0 as i32
                && py >= boxed.1 as i32
                && px < boxed.0 as i32 + boxed.2 as i32
                && py < boxed.1 as i32 + boxed.3 as i32
        };
        match kind {
            PointerKind::Down => {
                if inside(x, y) {
                    self.down = Some((x, y, now));
                    (true, false)
                } else {
                    self.down = None;
                    (false, false)
                }
            }
            PointerKind::Move => (self.down.is_some(), false),
            PointerKind::Up => {
                let Some((dx, dy, at)) = self.down.take() else {
                    return (false, false);
                };
                let quick = now.saturating_duration_since(at) <= TAP_WITHIN;
                let still = (x - dx).abs() <= TAP_SLACK && (y - dy).abs() <= TAP_SLACK;
                (true, quick && still && inside(x, y))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: Option<(u32, u32, u32, u32)> = Some((52, 52, 704, 396));

    #[test]
    fn a_tap_in_the_box_is_the_movies_and_toggles() {
        let mut tap = MovieTap::default();
        let t0 = Instant::now();
        assert_eq!(
            tap.pointer(PointerKind::Down, 400, 250, t0, BOX),
            (true, false)
        );
        assert_eq!(
            tap.pointer(PointerKind::Move, 404, 252, t0, BOX),
            (true, false)
        );
        assert_eq!(
            tap.pointer(
                PointerKind::Up,
                406,
                254,
                t0 + Duration::from_millis(120),
                BOX
            ),
            (true, true)
        );
    }

    #[test]
    fn a_slow_or_a_far_lift_is_still_the_movies_but_no_tap() {
        let mut tap = MovieTap::default();
        let t0 = Instant::now();
        tap.pointer(PointerKind::Down, 400, 250, t0, BOX);
        assert_eq!(
            tap.pointer(
                PointerKind::Up,
                400,
                250,
                t0 + Duration::from_millis(900),
                BOX
            ),
            (true, false),
            "held too long"
        );
        tap.pointer(PointerKind::Down, 400, 250, t0, BOX);
        assert_eq!(
            tap.pointer(
                PointerKind::Up,
                480,
                250,
                t0 + Duration::from_millis(100),
                BOX
            ),
            (true, false),
            "dragged away"
        );
    }

    #[test]
    fn outside_the_box_or_without_one_nothing_is_taken() {
        let mut tap = MovieTap::default();
        let t0 = Instant::now();
        assert_eq!(
            tap.pointer(PointerKind::Down, 10, 10, t0, BOX),
            (false, false)
        );
        assert_eq!(
            tap.pointer(PointerKind::Move, 12, 12, t0, BOX),
            (false, false)
        );
        assert_eq!(
            tap.pointer(PointerKind::Up, 12, 12, t0, BOX),
            (false, false)
        );
        assert_eq!(
            tap.pointer(PointerKind::Down, 400, 250, t0, None),
            (false, false)
        );
        assert_eq!(
            tap.pointer(PointerKind::Up, 400, 250, t0, None),
            (false, false)
        );
        // A finger down in the box, the movie gone before it lifts: the lift is nobody's.
        tap.pointer(PointerKind::Down, 400, 250, t0, BOX);
        assert_eq!(
            tap.pointer(PointerKind::Up, 400, 250, t0, None),
            (false, false)
        );
    }

    #[test]
    fn the_whole_screen_is_the_box_when_full() {
        let mut tap = MovieTap::default();
        let t0 = Instant::now();
        let full = Some((0, 0, 1280, 720));
        assert_eq!(
            tap.pointer(PointerKind::Down, 10, 700, t0, full),
            (true, false)
        );
        assert_eq!(
            tap.pointer(
                PointerKind::Up,
                12,
                702,
                t0 + Duration::from_millis(50),
                full
            ),
            (true, true)
        );
    }
}
