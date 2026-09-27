//! The lines Glass's programs write to their standard output, which the
//! plugin relays to the player's journal. A level from `GLASS_LOG`
//! (`error`, `warn`, `info`, `verbose`, `trace`; `info` when unset) says
//! how much, and at the two finest levels `GLASS_LOG_TARGETS`, a comma
//! list, narrows it to the targets named. Errors and warnings go to the
//! standard error stream, the rest to the standard output.

use std::fmt::Arguments;
use std::sync::OnceLock;

/// How much is said, from the least to the most.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error,
    Warn,
    Info,
    Verbose,
    Trace,
}

impl Level {
    /// The level a word names, or none.
    pub fn parse(word: &str) -> Option<Level> {
        match word.trim().to_ascii_lowercase().as_str() {
            "error" | "errors" | "off" => Some(Level::Error),
            "warn" | "warning" | "warnings" => Some(Level::Warn),
            "info" => Some(Level::Info),
            "verbose" | "debug" => Some(Level::Verbose),
            "trace" => Some(Level::Trace),
            _ => None,
        }
    }
}

struct Settings {
    level: Level,
    targets: Vec<String>,
    prefix: &'static str,
}

static SETTINGS: OnceLock<Settings> = OnceLock::new();

/// Read the environment once, and name the program in every line. A
/// second call changes nothing.
pub fn init(prefix: &'static str) {
    let _ = SETTINGS.get_or_init(|| Settings {
        level: std::env::var("GLASS_LOG")
            .ok()
            .and_then(|v| Level::parse(&v))
            .unwrap_or(Level::Info),
        targets: std::env::var("GLASS_LOG_TARGETS")
            .unwrap_or_default()
            .split(',')
            .map(|t| t.trim().to_ascii_lowercase())
            .filter(|t| !t.is_empty())
            .collect(),
        prefix,
    });
}

fn settings() -> &'static Settings {
    init("glass");
    SETTINGS.get().expect("initialised above")
}

/// The level in force.
pub fn level() -> Level {
    settings().level
}

/// Whether a line at `level` about `target` is written: within the level,
/// and, at verbose and trace, among the targets when any are named.
pub fn on(level: Level, target: &str) -> bool {
    let s = settings();
    if level > s.level {
        return false;
    }
    if level >= Level::Verbose && !s.targets.is_empty() && !s.targets.iter().any(|t| t == target) {
        return false;
    }
    true
}

/// Write one line, when the level and target allow it.
pub fn say(level: Level, target: &str, args: Arguments) {
    if !on(level, target) {
        return;
    }
    let prefix = settings().prefix;
    #[cfg(target_os = "android")]
    {
        android::write(level, prefix, &format!("{args}"));
    }
    #[cfg(not(target_os = "android"))]
    if level <= Level::Warn {
        eprintln!("{prefix}: {args}");
    } else {
        println!("{prefix}: {args}");
    }
}

/// On Android the lines go to the system log, read with `adb logcat -s
/// glass`, since a program there has no terminal.
#[cfg(target_os = "android")]
mod android {
    use super::Level;
    use std::ffi::{c_char, c_int, CString};

    #[link(name = "log")]
    extern "C" {
        fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }

    pub fn write(level: Level, tag: &str, text: &str) {
        let priority = match level {
            Level::Error => 6,
            Level::Warn => 5,
            Level::Info => 4,
            _ => 3,
        };
        let (Ok(tag), Ok(text)) = (CString::new(tag), CString::new(text)) else {
            return;
        };
        // SAFETY: two C strings that live for the call, which copies them.
        unsafe {
            __android_log_write(priority, tag.as_ptr(), text.as_ptr());
        }
    }
}

/// `say!(Info, "display", "meter={name}")`: a line at a level about a target.
#[macro_export]
macro_rules! say {
    ($level:ident, $target:expr, $($arg:tt)*) => {
        $crate::say($crate::Level::$level, $target, format_args!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_parse_and_order() {
        assert_eq!(Level::parse("Verbose"), Some(Level::Verbose));
        assert_eq!(Level::parse("off"), Some(Level::Error));
        assert_eq!(Level::parse("loud"), None);
        assert!(
            Level::Trace > Level::Verbose
                && Level::Verbose > Level::Info
                && Level::Info > Level::Warn
        );
    }

    #[test]
    fn the_gate_reads_the_environment_once() {
        // The environment is read at first use; the tests run in one process,
        // so the level is whatever the first reader saw: info unless set.
        assert!(on(Level::Error, "display"));
        assert!(on(Level::Warn, "display"));
        assert_eq!(on(Level::Trace, "display"), level() >= Level::Trace);
    }
}
