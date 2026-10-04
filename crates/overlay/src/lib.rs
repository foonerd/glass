//! The contract between the display and a face drawn over it: what a face
//! is asked, what it is shown of the display, and the types it is written
//! against. Nothing here knows a window or a file system, so a face
//! written against it draws the same on the player's screen, where the
//! display's loop calls it, and in a browser, where the page's pipeline
//! does.

use std::collections::BTreeMap;

use expose::Frame;
use lead::Input;

pub use controls::PointerKind;

mod laid;
pub use laid::{Laid, Lay};

/// What a face sees of the display each frame: the player's state as the
/// source has it (the cover's file among it, once fetched), the theme's
/// fonts, the picture's size, the display's clock, the time of day,
/// whether the screen is the display's own, the face's own settings as
/// the configuration has them, and the folder of the theme on show.
pub struct View<'a> {
    pub input: &'a Input,
    pub fonts: &'a expose::Fonts,
    pub width: u32,
    pub height: u32,
    /// The display's clock in milliseconds: it only ever goes forward, and
    /// only differences of it mean anything.
    pub now_ms: u64,
    /// The time of day where the player is.
    pub wall: &'a Wall,
    pub ours: bool,
    /// How large the face draws: 1 as designed, more for a hand at arm's length.
    pub scale: f32,
    /// The configuration's `face.<name>` keys by name, as written: the
    /// display reads none of them but the size; their meaning is the face's.
    pub settings: &'a BTreeMap<String, String>,
    /// The folder of the theme on show, empty where there is none: what a
    /// theme brings for a face lies there, beside its `meters.txt`.
    pub theme_dir: &'a str,
}

/// What a face has to draw over a frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cover {
    /// Nothing at all.
    Nothing,
    /// What it drew on the frame before, were the picture under it the same.
    Same,
    /// Something it has not drawn before.
    New,
}

/// A face drawn over the display: what glass-evo adds on top of the
/// theme. The display calls it every frame, offers it every touch before
/// the theme's controls, and sends the commands it hands back through the
/// same path the theme's own buttons use. With no face nothing changes.
pub trait Overlay {
    /// What the face has to draw over the frame about to be shown. Asked
    /// before every `draw`. A face that says `Nothing` is not handed the
    /// frame, and the display spares the copy it would have drawn on; one
    /// that says `Same` is not asked to draw again while the picture under
    /// it stands still, and the window is left as it is. A face that does
    /// not say draws every frame.
    fn covers(&mut self, _view: &View) -> Cover {
        Cover::New
    }
    /// Draw over the frame as shown, after the theme; `true` when anything
    /// was drawn, and the whole picture is then shown again.
    fn draw(&mut self, frame: &mut Frame, view: &View) -> bool;
    /// A pointer event in picture pixels; `true` when taken, and the
    /// theme's controls and the touch rules then do not see it.
    fn pointer(&mut self, kind: PointerKind, x: i32, y: i32, view: &View) -> bool;
    /// The commands for the player the face wants sent, taken every frame.
    fn commands(&mut self) -> Vec<intake::Command>;
    /// What the face is called and its version, as a remote display says
    /// which flavour it is: on its settings page and to the player.
    fn name(&self) -> Option<String> {
        None
    }
    /// Where the display built with this face is released, for a remote
    /// display to bring itself up to date as what it is. A face that says
    /// nothing is offered no upgrade: the display's own release would take
    /// the face away.
    fn origin(&self) -> Option<Origin> {
        None
    }
}

/// Where a display built with a face is released: the repository on
/// GitHub (`owner/name`), what its archives are called before the version
/// (`glass-evo-` for `glass-evo-0.1.27-x64.tar.gz`), the name of its
/// binary in an archive, and the version that runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    pub repository: String,
    pub asset: String,
    pub binary: String,
    pub version: String,
}

/// The types a face is written against, in one place.
pub mod face {
    pub use controls::PointerKind;
    pub use expose::{blur, fit_art, read_art, read_covering, render_text, ui, Fonts, Frame};
    pub use intake::Command;
    /// A text file and whether one is there, read where the display reads
    /// its own: the file system on a player, the page's file table in a browser.
    pub use lead::{is_file, read_to_string};
    pub use lead::{Input, Metadata, TextStyle};
}

/// The time of day where the player is, broken down: what a clock and a
/// date are written from. The display reads it from the system; a browser's
/// pipeline reckons it from the page's clock and zone (`Wall::at`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wall {
    /// Seconds since 1970 in universal time.
    pub epoch_s: i64,
    pub year: i32,
    /// 1 to 12.
    pub month: u32,
    /// 1 to 31.
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    /// 0 for Sunday to 6 for Saturday.
    pub weekday: u32,
    /// The day of the year, 0 for the first of January.
    pub yearday: u32,
    /// Minutes east of universal time.
    pub offset_minutes: i32,
    /// The zone's short name where it is known (`BST`), else empty.
    pub zone: String,
}

impl Default for Wall {
    /// The first moment of 1970 in universal time.
    fn default() -> Self {
        Wall::at(0, 0, "")
    }
}

/// Days since 1970-01-01 of a date in the Gregorian calendar.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (i64::from(month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The Gregorian date of a day counted from 1970-01-01: year, month, day.
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    (year, month, day)
}

impl Wall {
    /// The time of day at `epoch_ms` (milliseconds since 1970, universal
    /// time) in a zone `offset_minutes` east of it, named `zone`.
    pub fn at(epoch_ms: i64, offset_minutes: i32, zone: &str) -> Self {
        let epoch_s = epoch_ms.div_euclid(1000);
        let local = epoch_s + i64::from(offset_minutes) * 60;
        let days = local.div_euclid(86_400);
        let of_day = local.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        Wall {
            epoch_s,
            year,
            month,
            day,
            hour: (of_day / 3600) as u32,
            minute: (of_day % 3600 / 60) as u32,
            second: (of_day % 60) as u32,
            // 1970-01-01 was a Thursday.
            weekday: (days + 4).rem_euclid(7) as u32,
            yearday: (days - days_from_civil(year, 1, 1)) as u32,
            offset_minutes,
            zone: zone.to_string(),
        }
    }

    /// Now, as the system has it: its local time and zone on a player, its
    /// local time on Windows (the zone's name is not read there); universal
    /// time where the system's zone is not read.
    pub fn now() -> Self {
        let epoch_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        let (offset_minutes, zone) = system_zone(epoch_ms.div_euclid(1000));
        Wall::at(epoch_ms, offset_minutes, &zone)
    }
}

/// The system's zone at a moment: minutes east of universal time and the
/// zone's short name.
#[cfg(unix)]
fn system_zone(epoch_s: i64) -> (i32, String) {
    // SAFETY: localtime_r writes the struct it is handed and nothing else;
    // a zeroed tm is a valid one to hand it; the zone's name it points at
    // is a static string of the C library's.
    unsafe {
        let moment = epoch_s as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&moment, &mut tm).is_null() {
            return (0, String::new());
        }
        let zone = if tm.tm_zone.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(tm.tm_zone)
                .to_string_lossy()
                .into_owned()
        };
        ((tm.tm_gmtoff / 60) as i32, zone)
    }
}

/// A moment as Windows keeps one: the calendar's fields, sixteen bits each.
#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct SystemTime {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}

#[cfg(windows)]
extern "system" {
    /// A moment of universal time as local time in the system's zone (a
    /// null zone), by the zone's own rules for that date.
    fn SystemTimeToTzSpecificLocalTime(
        zone: *const core::ffi::c_void,
        universal: *const SystemTime,
        local: *mut SystemTime,
    ) -> i32;
}

/// The system's zone at a moment, on Windows: the system turns the moment
/// into its local time, and the difference is how far east the zone
/// stands. The system is asked, not the C runtime, which would read a `TZ`
/// variable of another system's making in its own way. The zone's name is
/// not read.
#[cfg(windows)]
fn system_zone(epoch_s: i64) -> (i32, String) {
    let at = Wall::at(epoch_s * 1000, 0, "");
    let universal = SystemTime {
        year: at.year as u16,
        month: at.month as u16,
        day: at.day as u16,
        hour: at.hour as u16,
        minute: at.minute as u16,
        second: at.second as u16,
        ..SystemTime::default()
    };
    let mut local = SystemTime::default();
    // SAFETY: the call reads the moment it is handed and writes the one it
    // is handed, both whole structs of this module's own.
    let turned =
        unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &universal, &mut local) };
    if turned == 0 {
        return (0, String::new());
    }
    let seconds = |t: &SystemTime| {
        days_from_civil(i32::from(t.year), u32::from(t.month), u32::from(t.day)) * 86_400
            + i64::from(t.hour) * 3600
            + i64::from(t.minute) * 60
            + i64::from(t.second)
    };
    (
        ((seconds(&local) - seconds(&universal)) / 60) as i32,
        String::new(),
    )
}

#[cfg(not(any(unix, windows)))]
fn system_zone(_epoch_s: i64) -> (i32, String) {
    (0, String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_moment_is_broken_down_in_its_zone() {
        // 2026-10-02 03:14:07 UTC, a Friday, the 275th day of the year.
        let epoch_ms = 1_790_910_847_000;
        let utc = Wall::at(epoch_ms, 0, "UTC");
        assert_eq!(
            (utc.year, utc.month, utc.day, utc.hour, utc.minute, utc.second),
            (2026, 10, 2, 3, 14, 7)
        );
        assert_eq!((utc.weekday, utc.yearday), (5, 274));
        assert_eq!(utc.epoch_s, 1_790_910_847);
        // An hour east the clock is an hour on; the moment is the same.
        let bst = Wall::at(epoch_ms, 60, "BST");
        assert_eq!((bst.hour, bst.day, bst.zone.as_str()), (4, 2, "BST"));
        assert_eq!(bst.epoch_s, utc.epoch_s);
        // Far enough west it is still the day before.
        let west = Wall::at(epoch_ms, -8 * 60, "PST");
        assert_eq!(
            (west.day, west.hour, west.weekday, west.yearday),
            (1, 19, 4, 273)
        );
    }

    #[test]
    fn the_calendar_holds_at_its_edges() {
        // The last second of a leap year, and the first of the next.
        let eve = Wall::at(1_735_689_599_000, 0, "");
        assert_eq!(
            (eve.year, eve.month, eve.day, eve.yearday),
            (2024, 12, 31, 365)
        );
        assert_eq!((eve.hour, eve.minute, eve.second), (23, 59, 59));
        let new_year = Wall::at(1_735_689_600_000, 0, "");
        assert_eq!(
            (
                new_year.year,
                new_year.month,
                new_year.day,
                new_year.yearday
            ),
            (2025, 1, 1, 0)
        );
        assert_eq!(new_year.weekday, 3, "a Wednesday");
        // A leap day, and a moment before 1970.
        let leap = Wall::at(1_709_208_000_000, 0, "");
        assert_eq!(
            (leap.year, leap.month, leap.day, leap.weekday),
            (2024, 2, 29, 4)
        );
        let before = Wall::at(-1000, 0, "");
        assert_eq!(
            (
                before.year,
                before.month,
                before.day,
                before.hour,
                before.second
            ),
            (1969, 12, 31, 23, 59)
        );
        assert_eq!(before.weekday, 3);
        assert_eq!(Wall::default().year, 1970);
        // Every day of four centuries comes back as the date it was counted from.
        let mut days = days_from_civil(1900, 1, 1);
        for year in 1900..2300 {
            for month in 1..=12u32 {
                let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
                let length = match month {
                    2 if leap => 29,
                    2 => 28,
                    4 | 6 | 9 | 11 => 30,
                    _ => 31,
                };
                for day in 1..=length {
                    assert_eq!(civil_from_days(days), (year, month, day));
                    assert_eq!(days_from_civil(year, month, day), days);
                    days += 1;
                }
            }
        }
    }

    #[test]
    fn the_systems_zone_is_a_zone() {
        // Whatever the system's zone, it stands within fourteen hours of
        // universal time and at a whole number of quarter hours, and the
        // local time of day is the universal one moved by it.
        let now = Wall::now();
        println!(
            "zone: {} minutes east, named {:?}",
            now.offset_minutes, now.zone
        );
        assert!(
            now.offset_minutes.abs() <= 14 * 60,
            "{}",
            now.offset_minutes
        );
        assert_eq!(now.offset_minutes % 15, 0, "{}", now.offset_minutes);
        let universal = Wall::at(now.epoch_s * 1000, 0, "");
        let moved = (i64::from(universal.hour) * 60
            + i64::from(universal.minute)
            + i64::from(now.offset_minutes))
        .rem_euclid(24 * 60);
        assert_eq!(i64::from(now.hour) * 60 + i64::from(now.minute), moved);
    }

    #[test]
    fn now_is_a_moment_of_this_age() {
        let now = Wall::now();
        assert!(now.year >= 2026, "the system's clock is set");
        assert!((1..=12).contains(&now.month) && (1..=31).contains(&now.day));
        assert!(now.offset_minutes.abs() <= 14 * 60);
    }
}
