//! What a remote display is set to: the players it knows and the one it
//! shows, how it follows the player's theme or keeps its own, how its
//! window sits, and its gain. Kept as one JSON file, changed from the
//! remote's own page while it runs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u32 = 1;
pub const DEFAULT_PAGE_PORT: u16 = 5583;
pub const MAX_GAIN_DB: f32 = 12.0;

/// A player this remote knows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Player {
    /// Host name or address, as typed or as discovered.
    pub host: String,
    #[serde(default = "default_manager_port")]
    pub manager_port: u16,
    /// A name for the list, the player's own when discovered.
    #[serde(default)]
    pub name: String,
    /// The theme this remote shows of this player.
    #[serde(default)]
    pub theme: ThemeChoice,
}

fn default_manager_port() -> u16 {
    intake::remote::DEFAULT_MANAGER_PORT
}

/// Follow the player's theme and meter, or keep one of the player's
/// themes as this remote's own, with its own meter choice.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    Follow,
    Own {
        folder: String,
        #[serde(default)]
        meter: MeterChoice,
    },
}

/// Which meters of an own theme show, and how they move on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "select", rename_all = "lowercase")]
pub enum MeterChoice {
    Random {
        #[serde(default = "default_interval")]
        interval_s: u32,
        #[serde(default)]
        on_title: bool,
    },
    List {
        names: Vec<String>,
        #[serde(default = "default_interval")]
        interval_s: u32,
        #[serde(default)]
        on_title: bool,
    },
    Single {
        name: String,
    },
}

fn default_interval() -> u32 {
    60
}

impl Default for MeterChoice {
    fn default() -> Self {
        MeterChoice::Random {
            interval_s: default_interval(),
            on_title: false,
        }
    }
}

impl MeterChoice {
    /// The configuration's `meter` value: a name, a list, or `random`.
    pub fn meter_value(&self) -> String {
        match self {
            MeterChoice::Random { .. } => "random".to_string(),
            MeterChoice::List { names, .. } => names.join(","),
            MeterChoice::Single { name } => name.clone(),
        }
    }

    pub fn interval_s(&self) -> u32 {
        match self {
            MeterChoice::Random { interval_s, .. } | MeterChoice::List { interval_s, .. } => {
                (*interval_s).clamp(15, 1000)
            }
            MeterChoice::Single { .. } => default_interval(),
        }
    }

    pub fn on_title(&self) -> bool {
        match self {
            MeterChoice::Random { on_title, .. } | MeterChoice::List { on_title, .. } => *on_title,
            MeterChoice::Single { .. } => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WindowMode {
    #[default]
    Fullscreen,
    Windowed,
    Frameless,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Display {
    #[serde(default)]
    pub mode: WindowMode,
    /// Where a windowed or frameless window goes; centred when absent.
    #[serde(default)]
    pub position: Option<(i32, i32)>,
    /// Scale the theme to the window, keeping its shape.
    #[serde(default = "default_true")]
    pub fit: bool,
    /// A frame rate of this remote's own; the player's when absent.
    #[serde(default)]
    pub fps: Option<u32>,
}

fn default_true() -> bool {
    true
}

impl Default for Display {
    fn default() -> Self {
        Self {
            mode: WindowMode::Fullscreen,
            position: None,
            fit: true,
            fps: None,
        }
    }
}

/// The remote's configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RemoteConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    /// How this remote shows on the player's Remotes tab.
    #[serde(default)]
    pub name: String,
    /// The players known, by an id of their own.
    #[serde(default)]
    pub players: BTreeMap<String, Player>,
    /// The one shown.
    #[serde(default)]
    pub active: Option<String>,
    #[serde(default)]
    pub display: Display,
    /// Levels scaled on this remote, in decibels, within plus or minus twelve.
    #[serde(default)]
    pub gain_db: f32,
    /// The port of this remote's own page.
    #[serde(default = "default_page_port")]
    pub page_port: u16,
}

fn default_version() -> u32 {
    CONFIG_VERSION
}
fn default_page_port() -> u16 {
    DEFAULT_PAGE_PORT
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            name: String::new(),
            players: BTreeMap::new(),
            active: None,
            display: Display::default(),
            gain_db: 0.0,
            page_port: DEFAULT_PAGE_PORT,
        }
    }
}

impl RemoteConfig {
    /// The file, or the default when there is none or it cannot be read.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice::<RemoteConfig>(&bytes) {
                Ok(mut config) => {
                    config.tidy();
                    (config, None)
                }
                Err(err) => (
                    Self::default(),
                    Some(format!(
                        "{}: {err}; starting from the defaults",
                        path.display()
                    )),
                ),
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (Self::default(), None),
            Err(err) => (Self::default(), Some(format!("{}: {err}", path.display()))),
        }
    }

    /// Written whole, beside the file and renamed over it.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let part = path.with_extension("json.part");
        std::fs::write(&part, text).map_err(|e| format!("{}: {e}", part.display()))?;
        std::fs::rename(&part, path).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Bring what was read within bounds.
    pub fn tidy(&mut self) {
        self.version = CONFIG_VERSION;
        self.gain_db = if self.gain_db.is_finite() {
            self.gain_db.clamp(-MAX_GAIN_DB, MAX_GAIN_DB)
        } else {
            0.0
        };
        if self.page_port == 0 {
            self.page_port = DEFAULT_PAGE_PORT;
        }
        if let Some(fps) = self.display.fps {
            self.display.fps = Some(fps.clamp(lead::MIN_FRAME_RATE, lead::MAX_FRAME_RATE));
        }
        self.players
            .retain(|id, p| !id.is_empty() && !p.host.trim().is_empty());
        for player in self.players.values_mut() {
            player.host = player.host.trim().to_string();
            if player.manager_port == 0 {
                player.manager_port = default_manager_port();
            }
        }
        if self
            .active
            .as_ref()
            .is_some_and(|id| !self.players.contains_key(id))
        {
            self.active = None;
        }
        if self.active.is_none() && self.players.len() == 1 {
            self.active = self.players.keys().next().cloned();
        }
    }

    /// What is wrong with this configuration, if anything.
    pub fn check(&self) -> Result<(), String> {
        if self.page_port < 1024 {
            return Err("the page port must be 1024 or above".to_string());
        }
        for (id, player) in &self.players {
            if !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(format!(
                    "player id {id:?} may hold letters, digits, - and _ only"
                ));
            }
            if player.host.trim().is_empty() {
                return Err(format!("player {id} has no host"));
            }
            if player.host.contains(['/', ' ']) || player.host.contains("://") {
                return Err(format!(
                    "player {id}: the host is a name or an address, without a scheme or a path"
                ));
            }
            if let ThemeChoice::Own { folder, meter } = &player.theme {
                if folder.trim().is_empty() {
                    return Err(format!("player {id}: an own theme needs a folder"));
                }
                match meter {
                    MeterChoice::List { names, .. } if names.is_empty() => {
                        return Err(format!("player {id}: a list needs at least one meter"));
                    }
                    MeterChoice::Single { name } if name.trim().is_empty() => {
                        return Err(format!("player {id}: one meter needs its name"));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// The active player.
    pub fn player(&self) -> Option<(&str, &Player)> {
        let id = self.active.as_deref()?;
        self.players.get(id).map(|p| (id, p))
    }

    /// An id for a player from its host or name: lowercase, letters, digits and hyphens.
    pub fn id_for(text: &str) -> String {
        let mut id: String = text
            .trim()
            .to_ascii_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        while id.contains("--") {
            id = id.replace("--", "-");
        }
        let id = id.trim_matches('-').to_string();
        if id.is_empty() {
            "player".to_string()
        } else {
            id
        }
    }

    /// Add or replace a player and make it the active one.
    pub fn put_player(&mut self, player: Player) -> String {
        let base = Self::id_for(if player.name.is_empty() {
            &player.host
        } else {
            &player.name
        });
        let id = self
            .players
            .iter()
            .find(|(_, p)| p.host.eq_ignore_ascii_case(&player.host))
            .map(|(id, _)| id.clone())
            .unwrap_or(base);
        self.players.insert(id.clone(), player);
        self.active = Some(id.clone());
        id
    }
}

/// Where the configuration and the cache live: `--config` and `--cache`,
/// else the user's configuration and cache directories.
pub fn config_path(given: Option<&str>) -> PathBuf {
    if let Some(path) = given {
        return PathBuf::from(path);
    }
    let base = if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        PathBuf::from(xdg)
    } else if let Some(app) = std::env::var_os("APPDATA").filter(|v| !v.is_empty()) {
        PathBuf::from(app)
    } else if let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
        PathBuf::from(home).join(".config")
    } else {
        std::env::temp_dir()
    };
    base.join("glass-remote").join("config.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_hold_no_player_and_the_file_round_trips() {
        let dir = std::env::temp_dir().join(format!("glass-remote-config-{}", std::process::id()));
        let path = dir.join("config.json");
        let (config, note) = RemoteConfig::load(&path);
        assert!(note.is_none());
        assert!(config.player().is_none());
        let mut config = config;
        let id = config.put_player(Player {
            host: "hanger.local".into(),
            manager_port: 5582,
            name: "hanger".into(),
            theme: ThemeChoice::Own {
                folder: "1280x720_x".into(),
                meter: MeterChoice::List {
                    names: vec!["gold".into(), "blue".into()],
                    interval_s: 30,
                    on_title: true,
                },
            },
        });
        assert_eq!(id, "hanger");
        config.gain_db = 3.5;
        config.save(&path).unwrap();
        let (back, note) = RemoteConfig::load(&path);
        assert!(note.is_none());
        assert_eq!(back, config);
        assert_eq!(back.player().unwrap().0, "hanger");
        if let ThemeChoice::Own { meter, .. } = &back.players["hanger"].theme {
            assert_eq!(meter.meter_value(), "gold,blue");
            assert_eq!(meter.interval_s(), 30);
            assert!(meter.on_title());
        } else {
            panic!("own theme expected");
        }
        std::fs::write(&path, b"{ not json").unwrap();
        let (fallback, note) = RemoteConfig::load(&path);
        assert!(note.is_some());
        assert_eq!(fallback, RemoteConfig::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tidy_and_check_keep_it_within_bounds() {
        let mut config = RemoteConfig {
            gain_db: 40.0,
            page_port: 0,
            display: Display {
                fps: Some(500),
                ..Display::default()
            },
            ..Default::default()
        };
        config.players.insert(
            "one".into(),
            Player {
                host: "  10.0.0.5 ".into(),
                manager_port: 0,
                name: String::new(),
                theme: ThemeChoice::Follow,
            },
        );
        config.active = Some("gone".into());
        config.tidy();
        assert_eq!(config.gain_db, MAX_GAIN_DB);
        assert_eq!(config.page_port, DEFAULT_PAGE_PORT);
        assert_eq!(config.display.fps, Some(lead::MAX_FRAME_RATE));
        assert_eq!(config.players["one"].host, "10.0.0.5");
        assert_eq!(config.players["one"].manager_port, 5582);
        assert_eq!(
            config.active.as_deref(),
            Some("one"),
            "the only player becomes active"
        );
        assert!(config.check().is_ok());
        config.players.get_mut("one").unwrap().theme = ThemeChoice::Own {
            folder: String::new(),
            meter: MeterChoice::default(),
        };
        assert!(config.check().is_err());
        config.players.get_mut("one").unwrap().theme = ThemeChoice::Own {
            folder: "x".into(),
            meter: MeterChoice::List {
                names: vec![],
                interval_s: 60,
                on_title: false,
            },
        };
        assert!(config.check().is_err());
        config.players.get_mut("one").unwrap().host = "http://x".into();
        assert!(config.check().is_err());
    }

    #[test]
    fn ids_come_from_names_and_a_known_host_is_replaced() {
        assert_eq!(
            RemoteConfig::id_for("Hanger (Living Room)"),
            "hanger-living-room"
        );
        assert_eq!(RemoteConfig::id_for("  "), "player");
        let mut config = RemoteConfig::default();
        let a = config.put_player(Player {
            host: "hanger.local".into(),
            manager_port: 5582,
            name: String::new(),
            theme: ThemeChoice::Follow,
        });
        assert_eq!(a, "hanger-local");
        let b = config.put_player(Player {
            host: "HANGER.local".into(),
            manager_port: 5590,
            name: "Hanger".into(),
            theme: ThemeChoice::Follow,
        });
        assert_eq!(b, a, "the same host keeps its id");
        assert_eq!(config.players.len(), 1);
        assert_eq!(config.players[&a].manager_port, 5590);
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"mode\":\"follow\""));
    }
}
