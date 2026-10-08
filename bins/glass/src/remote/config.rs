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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum ThemeChoice {
    Follow {
        /// Show the meter the player shows, rather than rolling one of
        /// this remote's own under a random or list selection.
        #[serde(default = "default_true")]
        same_meter: bool,
    },
    Own {
        folder: String,
        #[serde(default)]
        meter: MeterChoice,
    },
    /// A theme from the folder on this machine the configuration names
    /// (`themes_dir`): a disk of this machine's, or a share mounted here.
    Local {
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

impl Default for ThemeChoice {
    fn default() -> Self {
        ThemeChoice::Follow { same_meter: true }
    }
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

/// The folders a themes folder on this machine resolves to: the theme
/// folders, and their spectrum twins. The folder may hold theme folders
/// itself, or a `templates` folder with `templates_spectrum` beside it,
/// as Glass's own data folder is laid out; a `templates` folder named
/// directly finds its twins beside it.
pub fn local_roots(dir: &str) -> (PathBuf, PathBuf) {
    let dir = PathBuf::from(dir.trim());
    if dir.join("templates").is_dir() {
        (dir.join("templates"), dir.join("templates_spectrum"))
    } else if dir.file_name().is_some_and(|n| n == "templates") {
        let beside = dir.parent().map(Path::to_path_buf).unwrap_or_default();
        (dir.clone(), beside.join("templates_spectrum"))
    } else {
        let twins = dir.join("templates_spectrum");
        (dir, twins)
    }
}

/// A theme folder found on this machine.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LocalTheme {
    pub folder: String,
    /// The meters, by section name of `meters.txt`.
    pub meters: Vec<String>,
    /// Whether a spectrum twin exists.
    pub spectrum: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// The themes under a folder on this machine: every folder with a
/// `meters.txt`, sorted by name, with the size a `WxH` at the front of the
/// name says.
pub fn local_themes(dir: &str) -> Result<Vec<LocalTheme>, String> {
    let (templates, spectrum) = local_roots(dir);
    let entries =
        std::fs::read_dir(&templates).map_err(|e| format!("{}: {e}", templates.display()))?;
    let mut themes = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') || !path.join("meters.txt").is_file() {
            continue;
        }
        let text = std::fs::read_to_string(path.join("meters.txt")).unwrap_or_default();
        let (width, height) = size_in_name(name);
        themes.push(LocalTheme {
            folder: name.to_string(),
            meters: section_names(&text),
            spectrum: spectrum.join(name).join("spectrum.txt").is_file(),
            width,
            height,
        });
    }
    themes.sort_by(|a, b| a.folder.cmp(&b.folder));
    Ok(themes)
}

/// The `[section]` names of a configuration text, in order, without the
/// `[current]` bookmark.
fn section_names(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix('[').and_then(|r| r.strip_suffix(']')))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("current"))
        .collect()
}

/// The `WxH` a theme folder's name starts with, when it does.
fn size_in_name(name: &str) -> (Option<u32>, Option<u32>) {
    let head: String = name
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == 'x')
        .collect();
    match head.split_once('x') {
        Some((w, h)) => (w.parse().ok(), h.parse().ok()),
        None => (None, None),
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
    /// The screen the window opens on, by the system's count from 0.
    #[serde(default)]
    pub monitor: u32,
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
            monitor: 0,
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
    /// A folder of themes on this machine, for a player's `Local` theme
    /// choice: theme folders, or a `templates` folder with
    /// `templates_spectrum` beside it, as Glass's own data folder is laid out.
    #[serde(default)]
    pub themes_dir: Option<String>,
    /// How much this remote logs: `error`, `warn`, `info`, `verbose` or
    /// `trace`; as started (the environment, else info) when absent.
    #[serde(default)]
    pub log_level: Option<String>,
    /// The share of its height a spectrum bar may fall per frame at most,
    /// 0.5 to 0.99, on this remote; 0 shows the bars as the player sends them.
    #[serde(default)]
    pub spectrum_decay: f32,
    /// When a display that carries a face draws it over the theme. A
    /// display without one has nothing to draw, whatever this says.
    #[serde(default)]
    pub face: FaceShown,
    /// Whether this remote is offered test releases, as the player's
    /// "Offer test releases": the newest of the repository's last ten
    /// releases, a pre-release among them; off, the latest release only.
    #[serde(default)]
    pub test_releases: bool,
}

/// When a remote that carries a face shows it: where the player's own
/// screen shows it, always, or never.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FaceShown {
    #[default]
    Follow,
    Always,
    Off,
}

impl FaceShown {
    /// Whether the face is drawn, the player's screen being `owner`'s.
    pub fn shows(self, owner: &str) -> bool {
        match self {
            FaceShown::Follow => owner == "glass-evo",
            FaceShown::Always => true,
            FaceShown::Off => false,
        }
    }
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
            themes_dir: None,
            log_level: None,
            spectrum_decay: 0.0,
            face: FaceShown::default(),
            test_releases: false,
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
        self.themes_dir = self
            .themes_dir
            .take()
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty());
        self.log_level = self
            .log_level
            .take()
            .filter(|l| logline::Level::parse(l).is_some())
            .map(|l| l.trim().to_ascii_lowercase());
        self.spectrum_decay = if self.spectrum_decay.is_finite() && self.spectrum_decay >= 0.5 {
            self.spectrum_decay.min(0.99)
        } else {
            0.0
        };
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
            if let ThemeChoice::Local { folder, .. } = &player.theme {
                if self.themes_dir.is_none() {
                    return Err(format!(
                        "player {id} takes a theme from this machine, but no themes folder is set"
                    ));
                }
                if folder.trim().is_empty() || folder.contains(['/', '\\']) {
                    return Err(format!(
                        "player {id}: the theme is a folder name under the themes folder"
                    ));
                }
            }
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
    fn following_shows_the_same_meter_unless_told_otherwise() {
        let follow: ThemeChoice = serde_json::from_str(r#"{"mode":"follow"}"#).unwrap();
        assert_eq!(follow, ThemeChoice::Follow { same_meter: true });
        let own_meter: ThemeChoice =
            serde_json::from_str(r#"{"mode":"follow","same_meter":false}"#).unwrap();
        assert_eq!(own_meter, ThemeChoice::Follow { same_meter: false });
        assert_eq!(
            ThemeChoice::default(),
            ThemeChoice::Follow { same_meter: true }
        );
    }

    #[test]
    fn the_defaults_hold_no_player_and_the_file_round_trips() {
        let dir = std::env::temp_dir().join(format!("glass-remote-config-{}", std::process::id()));
        let path = dir.join("config.json");
        let (config, note) = RemoteConfig::load(&path);
        assert!(note.is_none());
        assert!(config.player().is_none());
        let mut config = config;
        let id = config.put_player(Player {
            host: "kitchen.local".into(),
            manager_port: 5582,
            name: "kitchen".into(),
            theme: ThemeChoice::Own {
                folder: "1280x720_x".into(),
                meter: MeterChoice::List {
                    names: vec!["gold".into(), "blue".into()],
                    interval_s: 30,
                    on_title: true,
                },
            },
        });
        assert_eq!(id, "kitchen");
        config.gain_db = 3.5;
        config.save(&path).unwrap();
        let (back, note) = RemoteConfig::load(&path);
        assert!(note.is_none());
        assert_eq!(back, config);
        assert_eq!(back.player().unwrap().0, "kitchen");
        if let ThemeChoice::Own { meter, .. } = &back.players["kitchen"].theme {
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
    fn a_face_is_shown_where_the_players_screen_shows_it_unless_said_otherwise() {
        assert!(FaceShown::Follow.shows("glass-evo"));
        assert!(!FaceShown::Follow.shows("kiosk"));
        assert!(!FaceShown::Follow.shows(""), "a player that does not say");
        assert!(FaceShown::Always.shows("kiosk") && FaceShown::Always.shows(""));
        assert!(!FaceShown::Off.shows("glass-evo"));
        // A configuration from before the setting follows the player.
        let old: RemoteConfig = serde_json::from_str(r#"{"version":1,"name":"k"}"#).unwrap();
        assert_eq!(old.face, FaceShown::Follow);
        let set: RemoteConfig = serde_json::from_str(r#"{"face":"always"}"#).unwrap();
        assert_eq!(set.face, FaceShown::Always);
        assert!(serde_json::to_string(&set)
            .unwrap()
            .contains(r#""face":"always""#));
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
                theme: ThemeChoice::Follow { same_meter: true },
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
            RemoteConfig::id_for("Kitchen (Ground Floor)"),
            "kitchen-ground-floor"
        );
        assert_eq!(RemoteConfig::id_for("  "), "player");
        let mut config = RemoteConfig::default();
        let a = config.put_player(Player {
            host: "kitchen.local".into(),
            manager_port: 5582,
            name: String::new(),
            theme: ThemeChoice::Follow { same_meter: true },
        });
        assert_eq!(a, "kitchen-local");
        let b = config.put_player(Player {
            host: "KITCHEN.local".into(),
            manager_port: 5590,
            name: "Kitchen".into(),
            theme: ThemeChoice::Follow { same_meter: true },
        });
        assert_eq!(b, a, "the same host keeps its id");
        assert_eq!(config.players.len(), 1);
        assert_eq!(config.players[&a].manager_port, 5590);
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"mode\":\"follow\""));
    }
}

#[cfg(test)]
mod local_tests {
    use super::*;

    #[test]
    fn a_themes_folder_on_this_machine_lists_its_themes_and_their_twins() {
        let dir = std::env::temp_dir().join(format!("glass-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("templates/1280x720_mine")).unwrap();
        std::fs::create_dir_all(dir.join("templates/plain/sub")).unwrap();
        std::fs::create_dir_all(dir.join("templates/.hidden")).unwrap();
        std::fs::create_dir_all(dir.join("templates_spectrum/1280x720_mine")).unwrap();
        std::fs::write(
            dir.join("templates/1280x720_mine/meters.txt"),
            "[first]\nmeter.type = circular\n\n[second]\nmeter.type = linear\n",
        )
        .unwrap();
        std::fs::write(dir.join("templates/plain/meters.txt"), "[only]\n").unwrap();
        std::fs::write(dir.join("templates/.hidden/meters.txt"), "[x]\n").unwrap();
        std::fs::write(
            dir.join("templates_spectrum/1280x720_mine/spectrum.txt"),
            "[s]\n",
        )
        .unwrap();
        // The data folder layout, the templates folder named directly, and a bare folder of themes.
        let text = dir.to_string_lossy().into_owned();
        let (templates, twins) = local_roots(&text);
        assert_eq!(templates, dir.join("templates"));
        assert_eq!(twins, dir.join("templates_spectrum"));
        let (templates, twins) = local_roots(&dir.join("templates").to_string_lossy());
        assert_eq!(templates, dir.join("templates"));
        assert_eq!(twins, dir.join("templates_spectrum"));
        let (bare, bare_twins) = local_roots("/nowhere/skins");
        assert_eq!(bare, PathBuf::from("/nowhere/skins"));
        assert_eq!(
            bare_twins,
            PathBuf::from("/nowhere/skins/templates_spectrum")
        );
        let themes = local_themes(&text).unwrap();
        assert_eq!(
            themes,
            vec![
                LocalTheme {
                    folder: "1280x720_mine".to_string(),
                    meters: vec!["first".to_string(), "second".to_string()],
                    spectrum: true,
                    width: Some(1280),
                    height: Some(720),
                },
                LocalTheme {
                    folder: "plain".to_string(),
                    meters: vec!["only".to_string()],
                    spectrum: false,
                    width: None,
                    height: None,
                },
            ]
        );
        assert!(local_themes("/nowhere/at/all").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_local_theme_needs_a_themes_folder_and_a_plain_name() {
        let mut config = RemoteConfig::default();
        config.players.insert(
            "one".to_string(),
            Player {
                host: "player".to_string(),
                manager_port: 5582,
                name: String::new(),
                theme: ThemeChoice::Local {
                    folder: "1280x720_mine".to_string(),
                    meter: MeterChoice::default(),
                },
            },
        );
        assert!(config.check().is_err(), "no folder set");
        config.themes_dir = Some(" /media/skins ".to_string());
        config.tidy();
        assert_eq!(config.themes_dir.as_deref(), Some("/media/skins"));
        assert!(config.check().is_ok());
        config.players.get_mut("one").unwrap().theme = ThemeChoice::Local {
            folder: "../etc".to_string(),
            meter: MeterChoice::default(),
        };
        assert!(config.check().is_err(), "a path is not a folder name");
    }

    #[test]
    fn the_log_level_and_the_decay_are_kept_within_what_they_mean() {
        let mut config = RemoteConfig {
            log_level: Some(" Verbose ".to_string()),
            spectrum_decay: 1.5,
            ..Default::default()
        };
        config.tidy();
        assert_eq!(config.log_level.as_deref(), Some("verbose"));
        assert_eq!(config.spectrum_decay, 0.99);
        config.log_level = Some("loud".to_string());
        config.spectrum_decay = 0.2;
        config.tidy();
        assert_eq!(config.log_level, None);
        assert_eq!(config.spectrum_decay, 0.0, "below the range means off");
        config.spectrum_decay = 0.9;
        config.tidy();
        assert_eq!(config.spectrum_decay, 0.9);
    }
}
