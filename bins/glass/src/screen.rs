//! The panel's power as the display holds it: who switched it off, and
//! what a word from the player, a word from the face or a finger does;
//! and a backlight on its way from one brightness to another.

use intake::ScreenRequest;

/// A backlight on its way from one brightness to another over a time, as
/// the player asks with a fade: stepped by the display's clock, one level
/// a frame at most, a level never written twice. Done once the brightness
/// asked is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BacklightFade {
    from: u32,
    to: u32,
    started_ms: u64,
    duration_ms: u64,
    written: Option<u32>,
}

impl BacklightFade {
    pub fn new(from: u32, to: u32, now_ms: u64, duration_ms: u64) -> Self {
        Self {
            from,
            to,
            started_ms: now_ms,
            duration_ms: duration_ms.max(1),
            written: None,
        }
    }

    /// Where the backlight should stand at `now_ms`, on the straight line
    /// from `from` to `to`, `to` from the end of the time on.
    fn level_at(&self, now_ms: u64) -> u32 {
        let elapsed = now_ms.saturating_sub(self.started_ms);
        if elapsed >= self.duration_ms {
            return self.to;
        }
        let from = self.from as i64;
        let to = self.to as i64;
        let level = from + (to - from) * elapsed as i64 / self.duration_ms as i64;
        level.max(0) as u32
    }

    /// The level to write this frame, when it differs from the last one
    /// written; none when the backlight already stands where the line says.
    pub fn step(&mut self, now_ms: u64) -> Option<u32> {
        let level = self.level_at(now_ms);
        if self.written == Some(level) {
            return None;
        }
        self.written = Some(level);
        Some(level)
    }

    /// Whether the brightness asked has been written.
    pub fn done(&self) -> bool {
        self.written == Some(self.to)
    }
}

/// Who switched the panel off. The player's word (the API, the Manager, a
/// remote) holds until the player's word or a touch; the face's idle black
/// lets the face's own waking switch the panel back on as well.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holder {
    Player,
    Face,
}

/// The panel's power as the display holds it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScreenPower {
    off_by: Option<Holder>,
}

impl ScreenPower {
    pub fn off(&self) -> bool {
        self.off_by.is_some()
    }

    /// The player's word: on, off, or a toggle. The new power when it
    /// changed; a word that changes nothing says nothing. A player's off
    /// over the face's takes the hold, so the face's waking no longer
    /// switches the panel on.
    pub fn by_player(&mut self, request: &ScreenRequest) -> Option<bool> {
        let on = match (request.power, request.toggle) {
            (Some(on), _) => on,
            (None, true) => self.off(),
            (None, false) => return None,
        };
        self.set(on, Holder::Player)
    }

    /// The face's word: `false` with its idle black come whole, `true`
    /// when it wakes. The face's waking undoes the face's own off alone.
    pub fn by_face(&mut self, on: bool) -> Option<bool> {
        if on && self.off_by != Some(Holder::Face) {
            return None;
        }
        self.set(on, Holder::Face)
    }

    /// A finger or a button on the panel: on, whoever switched it off.
    pub fn touched(&mut self) -> Option<bool> {
        self.off().then(|| {
            self.off_by = None;
            true
        })
    }

    fn set(&mut self, on: bool, by: Holder) -> Option<bool> {
        if on {
            self.off_by?;
            self.off_by = None;
            return Some(true);
        }
        match self.off_by {
            None => {
                self.off_by = Some(by);
                Some(false)
            }
            Some(Holder::Face) if by == Holder::Player => {
                self.off_by = Some(Holder::Player);
                None
            }
            Some(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(power: Option<bool>, toggle: bool) -> ScreenRequest {
        ScreenRequest {
            power,
            toggle,
            brightness: None,
            fade_ms: 0,
        }
    }

    /// The player's off holds through the face's waking; a touch or the
    /// player's word switches the panel back on.
    #[test]
    fn the_players_off_holds_until_the_player_or_a_touch() {
        let mut screen = ScreenPower::default();
        assert_eq!(screen.by_player(&ask(Some(false), false)), Some(false));
        assert!(screen.off());
        assert_eq!(
            screen.by_face(true),
            None,
            "the face's waking does not undo it"
        );
        assert!(screen.off());
        assert_eq!(
            screen.by_player(&ask(Some(false), false)),
            None,
            "off again says nothing"
        );
        assert_eq!(screen.touched(), Some(true));
        assert!(!screen.off());
        assert_eq!(
            screen.touched(),
            None,
            "a touch on a panel that is on says nothing"
        );
        assert_eq!(screen.by_player(&ask(Some(false), false)), Some(false));
        assert_eq!(screen.by_player(&ask(Some(true), false)), Some(true));
    }

    /// The face's idle black switches the panel off, and its waking on;
    /// a player's off in between takes the hold from the face.
    #[test]
    fn the_faces_off_is_undone_by_its_own_waking_unless_the_player_took_the_hold() {
        let mut screen = ScreenPower::default();
        assert_eq!(screen.by_face(false), Some(false));
        assert_eq!(screen.by_face(false), None);
        assert_eq!(screen.by_face(true), Some(true));
        assert!(!screen.off());
        assert_eq!(
            screen.by_face(true),
            None,
            "waking a panel that is on says nothing"
        );
        assert_eq!(screen.by_face(false), Some(false));
        assert_eq!(
            screen.by_player(&ask(Some(false), false)),
            None,
            "off already, the hold taken"
        );
        assert_eq!(screen.by_face(true), None, "the face can no longer wake it");
        assert!(screen.off());
        assert_eq!(screen.by_player(&ask(Some(true), false)), Some(true));
    }

    /// A toggle flips whatever stands; a bare ask changes nothing.
    #[test]
    fn a_toggle_flips_and_a_bare_ask_changes_nothing() {
        let mut screen = ScreenPower::default();
        assert_eq!(screen.by_player(&ask(None, false)), None);
        assert_eq!(screen.by_player(&ask(None, true)), Some(false));
        assert!(screen.off());
        assert_eq!(screen.by_player(&ask(None, true)), Some(true));
        assert!(!screen.off());
        assert_eq!(
            screen.by_player(&ask(Some(true), true)),
            None,
            "a power named beats the toggle"
        );
    }

    /// A fade walks the backlight from where it stands to the level asked
    /// over the time, a level never written twice, and is done once the
    /// level asked is written; downwards as upwards.
    #[test]
    fn a_fade_steps_the_backlight_a_level_at_a_time_and_arrives() {
        let mut fade = BacklightFade::new(31, 5, 1000, 1000);
        assert_eq!(
            fade.step(1000),
            Some(31),
            "the first frame writes where it stands"
        );
        assert_eq!(
            fade.step(1010),
            None,
            "a level stands until the line moves on"
        );
        assert_eq!(fade.step(1500), Some(18), "half way, half the distance");
        assert_eq!(fade.step(1500), None);
        assert!(!fade.done());
        assert_eq!(
            fade.step(2000),
            Some(5),
            "the end of the time is the level asked"
        );
        assert!(fade.done());
        assert_eq!(fade.step(2500), None, "and nothing more is written");
        // Upwards, with few levels: every level on the way, none twice.
        let mut up = BacklightFade::new(2, 6, 0, 400);
        let mut written = Vec::new();
        for now in (0..=400).step_by(10) {
            if let Some(level) = up.step(now) {
                written.push(level);
            }
        }
        assert_eq!(written, vec![2, 3, 4, 5, 6]);
        assert!(up.done());
        // A fade of no time is the level at once; a clock that ran past the end too.
        let mut none = BacklightFade::new(10, 20, 0, 0);
        assert_eq!(none.step(0), Some(10));
        assert_eq!(none.step(1), Some(20));
        assert!(none.done());
        let mut late = BacklightFade::new(10, 20, 0, 100);
        assert_eq!(late.step(5000), Some(20));
        assert!(late.done());
    }
}
