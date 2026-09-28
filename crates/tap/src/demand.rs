//! What the tap measures to: the demand of the theme on show, as the
//! plugin writes it into a small JSON file beside the rings, read by the
//! tap when it changes. Without the file the bank is the default one.
//!
//! The file is tiny and read whole once a second at most, so a change
//! is seen by its content rather than by a stamp.

use std::path::{Path, PathBuf};

pub use bank::{Demand, Scale};

/// The file's name under the ring directory.
pub const FILE: &str = "glasstap.demand";
/// How often the file is looked at, in nanoseconds.
const LOOK_EVERY_NS: u64 = 1_000_000_000;

/// The demand file's path under `dir`.
pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// The demand the file holds, cleaned; `None` without a file or with one
/// that does not parse.
pub fn read(path: &Path) -> Option<Demand> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice::<Demand>(&bytes)
        .ok()
        .map(Demand::clean)
}

/// Write a demand for the tap to find, whole or not at all.
pub fn write(path: &Path, demand: &Demand) -> std::io::Result<()> {
    let text = serde_json::to_string(&demand.clean())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let part = path.with_extension("part");
    std::fs::write(&part, text)?;
    std::fs::rename(&part, path)
}

/// The demand file watched: the demand in force, and whether it changed.
pub struct Watch {
    path: PathBuf,
    current: Demand,
    looked_ns: Option<u64>,
}

impl Watch {
    /// Watch the file under `dir`, starting from what it holds now, or
    /// the default demand without it.
    pub fn new(dir: &Path) -> Self {
        let path = path(dir);
        let current = read(&path).unwrap_or_default();
        Self {
            path,
            current,
            looked_ns: None,
        }
    }

    /// The demand in force.
    pub fn current(&self) -> Demand {
        self.current
    }

    /// Look at the file if a second has passed since the last look: the
    /// new demand when it differs from the one in force, which it then
    /// becomes. A file gone means the default.
    pub fn changed(&mut self, now_ns: u64) -> Option<Demand> {
        if self
            .looked_ns
            .is_some_and(|at| now_ns.saturating_sub(at) < LOOK_EVERY_NS)
        {
            return None;
        }
        self.looked_ns = Some(now_ns);
        let found = read(&self.path).unwrap_or_default();
        if found == self.current {
            return None;
        }
        self.current = found;
        Some(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_demand_written_is_the_demand_read_and_a_change_is_seen_once() {
        let dir = std::env::temp_dir().join(format!("glass-demand-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut watch = Watch::new(&dir);
        assert_eq!(watch.current(), Demand::default(), "no file: the default");
        assert_eq!(watch.changed(0), None);
        let wanted = Demand::new(256, 2, Scale::Mel);
        write(&path(&dir), &wanted).unwrap();
        assert_eq!(read(&path(&dir)), Some(wanted));
        assert_eq!(
            watch.changed(500_000_000),
            None,
            "not looked at again within a second"
        );
        assert_eq!(watch.changed(2_000_000_000), Some(wanted));
        assert_eq!(watch.changed(4_000_000_000), None, "seen once");
        assert_eq!(watch.current(), wanted);
        std::fs::write(path(&dir), b"{not json").unwrap();
        assert_eq!(
            watch.changed(6_000_000_000),
            Some(Demand::default()),
            "a file that does not parse means the default"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
