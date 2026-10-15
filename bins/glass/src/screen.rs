//! The panel's power as the display holds it: who switched it off, and
//! what a word from the player, a word from the face or a finger does.

use intake::ScreenRequest;

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
}
