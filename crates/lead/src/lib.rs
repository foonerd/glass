//! The tap's measurements and the player's words become these types. This
//! station does not open devices or draw.

/// Bin count in the Volumio ALSA template.
use std::path::Path;

pub const DEFAULT_SPECTRUM_BINS: usize = 20;

/// UI full-scale that matches `meter_max`.
pub const DEFAULT_METER_MAX: f32 = 100.0;

/// `spectrum_max` written by the scope.
pub const DEFAULT_SPECTRUM_MAX: f32 = 100.0;

/// `[current] frame.rate` when the file or the key is missing.
pub const DEFAULT_FRAME_RATE: u32 = 30;

/// Inclusive range of the Volumio frame-rate control.
pub const MIN_FRAME_RATE: u32 = 10;
pub const MAX_FRAME_RATE: u32 = 60;

/// The environment variable that names the plugin's home directory.
pub const HOME_VAR: &str = "GLASS_HOME";
/// Where the plugin lives when `GLASS_HOME` is not set.
pub const DEFAULT_HOME: &str = "/data/plugins/user_interface/glass";
/// The meter configuration under the home: `[current]` names the theme,
/// the meter, the rotation and the frame rate.
pub const METER_CONFIG: &str = "config/meter.txt";
/// The spectrum configuration beside it.
pub const SPECTRUM_CONFIG: &str = "config/spectrum.txt";

/// The plugin's home directory: what `set_home` named, else `GLASS_HOME`,
/// else the default.
pub fn home() -> std::path::PathBuf {
    if let Some(home) = HOME.lock().ok().and_then(|slot| slot.clone()) {
        return home;
    }
    std::env::var_os(HOME_VAR)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_HOME))
}

static HOME: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

/// Name the home for this process, over the environment: a host without
/// an environment, such as a browser, says where it put the files.
pub fn set_home(path: impl Into<std::path::PathBuf>) {
    if let Ok(mut slot) = HOME.lock() {
        *slot = Some(path.into());
    }
}

/// Microseconds on a clock that only moves forward: the process's own on a
/// machine; on a target without one, what the host last set.
pub fn clock_us() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_micros() as u64
    }
    #[cfg(target_arch = "wasm32")]
    {
        HOST_CLOCK_US.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(target_arch = "wasm32")]
static HOST_CLOCK_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The host's clock, in microseconds, on a target without one of its own.
#[cfg(target_arch = "wasm32")]
pub fn set_clock_us(us: u64) {
    HOST_CLOCK_US.store(us, std::sync::atomic::Ordering::Relaxed);
}

/// A moment on the clock every target has, [`clock_us`] as a stand-in
/// for `std::time::Instant`: timing code written against it runs the same
/// on a player, a remote and in a browser, where the standard clock has
/// no source and stops the program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Moment(u64);

impl Moment {
    pub fn now() -> Self {
        Self(clock_us())
    }

    /// The time since this moment.
    pub fn elapsed(&self) -> std::time::Duration {
        std::time::Duration::from_micros(clock_us().saturating_sub(self.0))
    }

    /// The time from `earlier` to this moment, zero when it is not earlier.
    pub fn duration_since(&self, earlier: Self) -> std::time::Duration {
        std::time::Duration::from_micros(self.0.saturating_sub(earlier.0))
    }

    /// Microseconds on the clock.
    pub fn as_micros(&self) -> u64 {
        self.0
    }
}

impl std::ops::Sub<std::time::Duration> for Moment {
    type Output = Self;
    fn sub(self, earlier_by: std::time::Duration) -> Self {
        Self(self.0.saturating_sub(earlier_by.as_micros() as u64))
    }
}

impl std::ops::Add<std::time::Duration> for Moment {
    type Output = Self;
    fn add(self, later_by: std::time::Duration) -> Self {
        Self(self.0.saturating_add(later_by.as_micros() as u64))
    }
}

/// Nanoseconds since the epoch on the wall clock, for seeds and stamps;
/// on a target without a wall clock, the monotonic clock stands in.
pub fn epoch_nanos() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        clock_us().saturating_mul(1000)
    }
}

/// Files the host hands over in place of a file system. A browser has no
/// disk: the page puts the configuration, the theme's pictures and the
/// fonts here under the paths the configuration names, and every reader
/// below looks here when the file system has nothing. On a machine it
/// stays empty and costs one lookup.
pub mod tailor;

pub mod vfs {
    use std::collections::{HashMap, HashSet};
    use std::sync::{Arc, Mutex, OnceLock};

    fn table() -> &'static Mutex<HashMap<String, Arc<[u8]>>> {
        static TABLE: OnceLock<Mutex<HashMap<String, Arc<[u8]>>>> = OnceLock::new();
        TABLE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn missing() -> &'static Mutex<HashSet<String>> {
        static MISSING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
        MISSING.get_or_init(|| Mutex::new(HashSet::new()))
    }

    /// Keep `bytes` as the file at `path`; a later put replaces it, and a
    /// file put is no longer missing.
    pub fn put(path: &str, bytes: Vec<u8>) {
        if let Ok(mut table) = table().lock() {
            table.insert(path.to_string(), Arc::from(bytes));
        }
        if let Ok(mut missing) = missing().lock() {
            missing.remove(path);
        }
    }

    /// The host has no file for `path`: whoever wanted it stops asking.
    pub fn mark_missing(path: &str) {
        if let Ok(mut missing) = missing().lock() {
            missing.insert(path.to_string());
        }
    }

    /// Whether the host said there is no file at `path`.
    pub fn is_missing(path: &str) -> bool {
        missing().lock().is_ok_and(|missing| missing.contains(path))
    }

    /// The bytes put under `path`, shared with whoever else holds them.
    pub fn get(path: &str) -> Option<Arc<[u8]>> {
        table().lock().ok()?.get(path).cloned()
    }

    /// Whether a file was put under `path`.
    pub fn has(path: &str) -> bool {
        table().lock().is_ok_and(|table| table.contains_key(path))
    }

    /// The names directly under `dir` among the files put: a file's own
    /// name, or the folder's for a file further down, each once, sorted.
    pub fn entries(dir: &str) -> Vec<String> {
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        let mut names: Vec<String> = table()
            .lock()
            .map(|table| {
                table
                    .keys()
                    .filter_map(|key| key.strip_prefix(&prefix))
                    .map(|rest| rest.split('/').next().unwrap_or(rest).to_string())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names.dedup();
        names
    }

    /// Forget every file, and every file said to be missing.
    pub fn clear() {
        if let Ok(mut table) = table().lock() {
            table.clear();
        }
        if let Ok(mut missing) = missing().lock() {
            missing.clear();
        }
    }
}

/// The bytes of a file: read from the file system, else what the host put
/// in [`vfs`] under the same path.
pub fn read_file(path: &Path) -> Option<Vec<u8>> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(bytes) = std::fs::read(path) {
        return Some(bytes);
    }
    vfs::get(path.to_str()?).map(|bytes| bytes.to_vec())
}

/// The text of a file, the same way.
pub fn read_to_string(path: &Path) -> Option<String> {
    String::from_utf8(read_file(path)?).ok()
}

/// Whether a file is there, on the file system or in [`vfs`].
pub fn is_file(path: &Path) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    if path.is_file() {
        return true;
    }
    path.to_str().is_some_and(vfs::has)
}

/// What sits directly under a directory: the file system's entries when it
/// has the directory, else the names [`vfs`] holds under it, as paths.
pub fn dir_entries(path: &Path) -> Vec<std::path::PathBuf> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(entries) = std::fs::read_dir(path) {
        return entries.flatten().map(|entry| entry.path()).collect();
    }
    path.to_str()
        .map(vfs::entries)
        .unwrap_or_default()
        .into_iter()
        .map(|name| path.join(name))
        .collect()
}

use serde::{Deserialize, Serialize};

/// Left and right in UI units, plus mono derived from them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Levels {
    pub left: f32,
    pub right: f32,
    pub mono: f32,
}

/// Latest spectrum frame, raw scope units. Older frames are discarded upstream.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Bins {
    /// The theme's bars on the pipe scale, as the previous engine's
    /// spectrum read them, regrouped from the bank.
    pub values: Vec<f32>,
    /// The bank per channel, a full-scale sine reading 1.0 in its band;
    /// a one-channel bank fills both.
    #[serde(default)]
    pub bank: [Vec<f32>; 2],
    /// The bank's peak hold per channel, the same layout.
    #[serde(default)]
    pub hold: [Vec<f32>; 2],
    #[serde(default)]
    pub scale: bank::Scale,
    /// The onsets of the four band groups, sub-bass lowest.
    #[serde(default)]
    pub onsets: u8,
}

/// Now-playing text and position for the surface.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// As the player reports it, for example `44.1 kHz`.
    #[serde(default)]
    pub samplerate: String,
    /// As the player reports it, for example `16-bit`.
    #[serde(default)]
    pub bitdepth: String,
    /// Player status word: `play`, `pause`, `stop`, or empty.
    #[serde(default)]
    pub status: String,
    /// Track length in seconds. Zero when the source has none.
    #[serde(default)]
    pub duration: f32,
    /// Position in seconds at the time of this snapshot.
    #[serde(default)]
    pub seek: f32,
    /// Album art location as the player reports it: a URL, or a path on the player.
    #[serde(default)]
    pub albumart: String,
    /// Local file holding the fetched picture for `albumart`. Empty until fetched.
    #[serde(default)]
    pub art_file: String,
    /// Track type as the player reports it, for example `flac` or `webradio`.
    #[serde(default)]
    pub track_type: String,
    /// Bitrate as the player reports it, for example `192 Kbps`.
    #[serde(default)]
    pub bitrate: String,
    /// Icon file resolved for `track_type`, or empty when none exists.
    #[serde(default)]
    pub type_icon: String,
    /// The track after the current one in the queue, or empty.
    #[serde(default)]
    pub next_title: String,
    #[serde(default)]
    pub next_artist: String,
    #[serde(default)]
    pub next_album: String,
    /// The plugin's persist file while the display is kept after a pause:
    /// `freeze` or `countdown`, empty when there is none.
    #[serde(default)]
    pub persist_mode: String,
    /// Seconds left of the persist period.
    #[serde(default)]
    pub persist_left: u32,
    /// The track's location as the player reports it.
    #[serde(default)]
    pub uri: String,
    /// One entry per folder layer of the skin: the file found in the track's
    /// folder, or empty.
    #[serde(default)]
    pub folder_files: Vec<String>,
    /// The fanart picture on show, the one it replaces during a transition,
    /// the transition (`none`, `fade`, `merge`), its length, and how far it is.
    #[serde(default)]
    pub fanart_file: String,
    #[serde(default)]
    pub fanart_prev_file: String,
    #[serde(default)]
    pub fanart_transition: String,
    #[serde(default)]
    pub fanart_transition_ms: u32,
    #[serde(default)]
    pub fanart_elapsed_ms: u32,
    /// `volatile` as the player reports it: a stop or pause that is only a
    /// transition when true or unknown. `None` is unknown.
    #[serde(default)]
    pub volatile: Option<bool>,
    /// The record picture for this track: the album's own file or the theme's.
    #[serde(default)]
    pub vinyl_file: String,
    /// The reel pictures for this track from the album folder, or empty for the theme's.
    #[serde(default)]
    pub reel_files: (String, String),
    /// In queue mode: seconds of queue played before this track, and the
    /// queue's whole length. Zero length means track progress applies.
    #[serde(default)]
    pub queue_before_s: f32,
    #[serde(default)]
    pub queue_total_s: f32,
    /// The player's controls: volume 0 to 100, mute, random, repeat all, repeat single.
    #[serde(default)]
    pub volume: u32,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub random: bool,
    #[serde(default)]
    pub repeat: bool,
    #[serde(default)]
    pub repeat_single: bool,
    /// Infinity playback as the plugin's channel reports it; false without a channel.
    #[serde(default)]
    pub infinity: bool,
}

/// Where a text sits inside its box when it fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// How a text that does not fit its box moves. `Bounce` runs to the end,
/// pauses and comes back. `Ltr` and `Rtl` are the ticker's continuous loops,
/// named as the player names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ScrollDirection {
    #[default]
    Bounce,
    Ltr,
    Rtl,
}

/// The single looping line: `playinfo.ticker.*`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TickerSpec {
    pub text: TextSpec,
    pub direction: ScrollDirection,
    pub separator: String,
    pub space_between: u32,
    pub end_spaces: u32,
    pub append_next: bool,
    /// Hide the separate title, artist, album and next lines.
    pub replace: bool,
}

/// Scrolling speeds from the player configuration `[current]`:
/// `scrolling.mode` is `default` (40 everywhere), `custom` (these values)
/// or `skin` (the meter's own keys).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScrollSpeeds {
    pub mode: String,
    pub title: f32,
    pub artist: f32,
    pub album: f32,
}

pub fn scroll_speeds_from_config(text: &str) -> ScrollSpeeds {
    let number = |key: &str| {
        current_value(text, key)
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|n| *n > 0.0)
            .unwrap_or(40.0)
    };
    ScrollSpeeds {
        mode: current_value(text, "scrolling.mode")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase(),
        title: number("scrolling.speed.title"),
        artist: number("scrolling.speed.artist"),
        album: number("scrolling.speed.album"),
    }
}

/// Where the album art is drawn. The picture is stretched to `w` by `h`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtSpec {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    /// Mask picture path, or empty. Its white is cut away, its black kept.
    #[serde(default)]
    pub mask: String,
    /// Border width in pixels drawn inside the box. Zero is none.
    #[serde(default)]
    pub border: u32,
    /// Border colour: the theme's `font.color`.
    #[serde(default = "white")]
    pub border_color: [u8; 3],
    /// `albumart.rotation`: the art turns like a record label, cut to a
    /// circle when it has no mask, with a spindle and ring drawn over it.
    #[serde(default)]
    pub rotation: bool,
    /// Turns per minute, `albumart.rotation.speed` times the player's multiplier.
    #[serde(default)]
    pub rpm: f32,
}

fn white() -> [u8; 3] {
    [255, 255, 255]
}

/// How the type area shows the track type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeMode {
    Icon,
    Text,
    Both,
}

/// Horizontal placement inside the type box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeAlign {
    Left,
    Center,
    Right,
}

/// The type area: `playinfo.type.*`. `box_size` is `None` when the meter
/// gives no real dimension, which only the text mode accepts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeSpec {
    pub x: u32,
    pub y: u32,
    pub box_size: Option<(u32, u32)>,
    pub mode: TypeMode,
    pub align: TypeAlign,
    pub color: [u8; 3],
    pub font_size: u32,
    pub font_style: TextStyle,
    /// `playinfo.type.label`: what the label says beside or in place of
    /// the icon, the format's name or the sample rate line.
    #[serde(default)]
    pub label: TypeLabel,
}

/// What the type area's label says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TypeLabel {
    #[default]
    Format,
    SampleRate,
}

/// Volumio's own icon set.
pub const STOCK_ICONS: &str = "/volumio/http/www3/app/assets-common/format-icons";

/// The icon and label key for a reported track type: lower case, spaces to
/// underscores, `dsf` is `dsd`, cut at the first character outside
/// `[a-z0-9_]`, then the known aliases.
pub fn format_key(track_type: &str) -> String {
    let mut key: String = track_type
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c == ' ' { '_' } else { c })
        .collect();
    if key == "dsf" {
        key = "dsd".into();
    }
    let clean: String = key
        .chars()
        .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
        .collect();
    if !clean.is_empty() {
        key = clean;
    }
    let alias = match key.as_str() {
        "dab_radio" | "dab_" | "dab" | "rtlsdr" | "rtlsdr_radio" => "dab",
        "fm_radio" | "fm_" | "fm" => "fm",
        "webradio" | "web_radio" | "internet_radio" => "radio",
        "tidal_connect" => "tidal",
        "qobuz_connect" => "qobuz",
        "spotify_connect" => "spotify",
        "dlna" => "upnp",
        other => other,
    };
    alias.to_string()
}

/// The text shown for a key: proper case for known services, upper case
/// for codecs.
pub fn format_label(key: &str) -> String {
    match key {
        "" => String::new(),
        "tidal" => "Tidal".into(),
        "qobuz" => "Qobuz".into(),
        "spotify" => "Spotify".into(),
        "radio" => "Webradio".into(),
        "airplay" => "AirPlay".into(),
        "bluetooth" => "Bluetooth".into(),
        "upnp" => "UPnP".into(),
        "dab" => "DAB".into(),
        "fm" => "FM".into(),
        "cd" => "CD".into(),
        other => other.to_ascii_uppercase(),
    }
}

/// Font size for the type text when the meter sets none: inside a real box,
/// the smaller of the samplerate size and `0.45 × height`, at least 10;
/// without a box, the samplerate size.
pub fn type_font_size(sample_size: u32, box_height: Option<u32>) -> u32 {
    let sample = sample_size.max(1);
    match box_height {
        Some(h) if h > 1 => sample
            .min(((h as f32) * 0.45) as u32)
            .max(10)
            .min(sample.max(10)),
        _ => sample,
    }
}

/// Which theme font a text is set in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextStyle {
    Light,
    Regular,
    Bold,
    Italic,
    Digi,
}

/// Where and how one theme text is drawn. The top of the text sits at `y`.
/// `size` is the em height in pixels, as the theme files count it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextSpec {
    pub x: u32,
    pub y: u32,
    pub style: TextStyle,
    pub size: u32,
    pub color: [u8; 3],
    /// The box the text draws in, in pixels. Zero is no box: the text is
    /// drawn whole at its position.
    pub max_width: u32,
    /// Placement inside the box when the text fits.
    #[serde(default)]
    pub align: TextAlign,
    /// Pixels per second when the text does not fit. Zero clips instead.
    #[serde(default)]
    pub speed: f32,
    /// A font file of its own, or empty for the style's font. Time fields
    /// name one with `time.*.font`.
    #[serde(default)]
    pub font_file: String,
}

/// The meter engine's `[data.source]` conditioning of the pipe levels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataSourceSpec {
    /// `volume.max`: full scale in UI units.
    pub max_ui: f32,
    /// `volume.max.in.pipe`: full scale as the pipe writes it.
    pub max_pipe: f32,
    /// `volume.gain.db`: gain applied to every level, 0 is unity.
    pub gain_db: f32,
    /// `volume.gain.db.source`: a file holding a live gain in dB, or empty.
    pub gain_source: String,
    /// `smooth.buffer.size`: levels are the mean of this many snapshots. Zero is none.
    pub smooth: usize,
    /// `stereo.algorithm`: `new`, `average` or `logarithm`.
    pub stereo: String,
    /// `mono.algorithm`: `average` or `maximum`.
    pub mono: String,
}

impl Default for DataSourceSpec {
    fn default() -> Self {
        Self {
            max_ui: DEFAULT_METER_MAX,
            max_pipe: DEFAULT_METER_MAX,
            gain_db: 0.0,
            gain_source: String::new(),
            smooth: 0,
            stereo: "new".into(),
            mono: "average".into(),
        }
    }
}

/// Value of one key in a named section of the player configuration.
pub fn section_value(text: &str, section: &str, wanted: &str) -> Option<String> {
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line.eq_ignore_ascii_case(&format!("[{section}]"));
            continue;
        }
        if !inside || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == wanted {
            return Some(value.trim().to_string());
        }
    }
    None
}

/// Names of every `[section]` in a theme's meters file, in file order.
pub fn meter_sections(meters_txt: &str) -> Vec<String> {
    meters_txt
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            line.strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .map(|s| s.trim().to_string())
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Which meter of the theme to show: one by name, a random walk over all of
/// them, or a list to cycle in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    Named(String),
    Random,
    List(Vec<String>),
}

/// `meter` from `[current]`: `random`, a comma list, or one name.
pub fn selection_from_config(text: &str) -> Selection {
    let meter = current_value(text, "meter").unwrap_or_default();
    if meter.eq_ignore_ascii_case("random") {
        Selection::Random
    } else if meter.contains(',') {
        Selection::List(
            meter
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        )
    } else {
        Selection::Named(meter)
    }
}

/// Seconds a meter stays up in random and list mode, `random.meter.interval`.
pub fn random_interval_from_config(text: &str) -> u32 {
    current_value(text, "random.meter.interval")
        .and_then(|v| v.parse().ok())
        .unwrap_or(60)
}

/// `random.change.title`: the next meter comes with the next title, not the timer.
pub fn random_change_title_from_config(text: &str) -> bool {
    truthy(current_value(text, "random.change.title").as_deref())
}

/// `meter.visible` of a meter under `config.extend`. False hides the needles.
pub fn meter_visible(meters_txt: &str, meter: &str) -> bool {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    if !truthy(get("config.extend")) {
        return true;
    }
    get("meter.visible")
        .map(|v| truthy(Some(v)))
        .unwrap_or(true)
}

/// How a meter shows its level: a needle turning about an origin, or a bar
/// growing along a direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MeterKind {
    #[default]
    Circular,
    Linear,
}

/// The way a linear meter's bar grows, `direction` in the meter section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Direction {
    #[default]
    LeftRight,
    RightLeft,
    BottomTop,
    TopBottom,
    EdgesCenter,
    CenterEdges,
}

/// A linear meter: the indicator picture is shown up to a width that grows
/// in steps, or, as a single indicator, moved by that width.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LinearSpec {
    pub regular: u32,
    pub overload: u32,
    pub step_regular: u32,
    pub step_overload: u32,
    pub direction: Direction,
    /// `indicator.type = single`: the whole picture moves instead of growing.
    pub single: bool,
    /// `flip.left.x` and `flip.right.x`: that channel's picture is mirrored.
    pub flip_left: bool,
    pub flip_right: bool,
}

impl LinearSpec {
    /// Width per step, as the meter engine builds its masks: zero, then each
    /// regular step, then each overload step on top of the regular run.
    pub fn masks(&self) -> Vec<u32> {
        let mut masks = vec![0];
        masks.extend((1..=self.regular).map(|n| n * self.step_regular));
        let regular_run = self.regular * self.step_regular;
        masks.extend((1..=self.overload).map(|n| regular_run + n * self.step_overload));
        masks
    }

    /// Pixels of bar for a level from 0 to 1: the step the level reaches,
    /// never less than one pixel.
    pub fn bar_width(&self, level: f32) -> u32 {
        let masks = self.masks();
        let total = masks.len();
        let n = ((level.clamp(0.0, 1.0) * total as f32) as usize).min(total - 1);
        masks[n].max(1)
    }
}

/// Everything about how one meter draws its level, beyond the origins and
/// the default needle angles the skin already carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeterSpec {
    pub kind: MeterKind,
    /// 1 shows one needle or bar for the mono level at `mono_at`, 2 shows left and right.
    pub channels: u8,
    /// `mono.origin.x/y` (circular) or `mono.x/y` (linear), plus `meter.x/y`.
    pub mono_at: Option<(i32, i32)>,
    /// `left.start.angle` and `left.stop.angle` when the channel has its own.
    pub left_angles: Option<(f32, f32)>,
    pub right_angles: Option<(f32, f32)>,
    /// `left.needle.flip` and `right.needle.flip`: the needle picture is mirrored.
    pub flip_left: bool,
    pub flip_right: bool,
    pub linear: Option<LinearSpec>,
    /// `meter.visible` under `config.extend`; false draws no level at all.
    pub visible: bool,
}

impl Default for MeterSpec {
    fn default() -> Self {
        Self {
            kind: MeterKind::Circular,
            channels: 2,
            mono_at: None,
            left_angles: None,
            right_angles: None,
            flip_left: false,
            flip_right: false,
            linear: None,
            visible: true,
        }
    }
}

/// The meter's kind, channels, per-channel angles, flips and bar steps.
pub fn meter_spec(meters_txt: &str, meter: &str) -> MeterSpec {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let number = |key: &str| get(key).and_then(|v| v.trim().parse::<u32>().ok());
    let signed = |key: &str| get(key).and_then(|v| v.trim().parse::<i32>().ok());
    let angle = |key: &str| get(key).and_then(|v| v.trim().parse::<f32>().ok());
    let flag = |key: &str| truthy(get(key));
    let kind = match get("meter.type")
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("linear") => MeterKind::Linear,
        _ => MeterKind::Circular,
    };
    let channels = if number("channels") == Some(1) { 1 } else { 2 };
    let meter_x = signed("meter.x").unwrap_or(0);
    let meter_y = signed("meter.y").unwrap_or(0);
    let mono_at = match kind {
        MeterKind::Circular => (signed("mono.origin.x"), signed("mono.origin.y")),
        MeterKind::Linear => (signed("mono.x"), signed("mono.y")),
    };
    let mono_at = match mono_at {
        (Some(x), Some(y)) => Some((x + meter_x, y + meter_y)),
        _ => None,
    };
    let pair = |start: &str, stop: &str| match (angle(start), angle(stop)) {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    };
    let linear = (kind == MeterKind::Linear).then(|| LinearSpec {
        regular: number("position.regular").unwrap_or(0),
        overload: number("position.overload").unwrap_or(0),
        step_regular: number("step.width.regular").unwrap_or(0),
        step_overload: number("step.width.overload").unwrap_or(0),
        direction: match get("direction")
            .map(|v| v.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("right-left") => Direction::RightLeft,
            Some("bottom-top") => Direction::BottomTop,
            Some("top-bottom") => Direction::TopBottom,
            Some("edges-center") => Direction::EdgesCenter,
            Some("center-edges") => Direction::CenterEdges,
            _ => Direction::LeftRight,
        },
        single: get("indicator.type").is_some_and(|v| v.trim().eq_ignore_ascii_case("single")),
        flip_left: flag("flip.left.x"),
        flip_right: flag("flip.right.x"),
    });
    MeterSpec {
        kind,
        channels,
        mono_at,
        left_angles: pair("left.start.angle", "left.stop.angle"),
        right_angles: pair("right.start.angle", "right.stop.angle"),
        flip_left: flag("left.needle.flip"),
        flip_right: flag("right.needle.flip"),
        linear,
        visible: meter_visible(meters_txt, meter),
    }
}

/// How a spectrum layer is filled: one colour, a vertical gradient (first
/// colour at the bottom), a picture, or a picture stretched to the layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Fill {
    Color([u8; 4]),
    Gradient(Vec<[u8; 4]>),
    Image(String),
    ImageExtended(String),
}

/// A spectrum analyser as the spectrum engine draws it: a box on screen,
/// bars rising from an origin inside it, an optional reflection below the
/// origin, a topping that falls after the bar, and pictures around them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SpectrumSpec {
    /// `spectrum.x/y`: the box on screen. Everything is clipped to it.
    pub x: i32,
    pub y: i32,
    /// `channel`: which channel of the bank this box draws in the analyser's
    /// `single` layout: the two channels' mean, the left or the right.
    #[serde(default)]
    pub channel: SpectrumChannel,
    /// `spectrum.size` from the meter: the box.
    pub w: u32,
    pub h: u32,
    /// `origin.x/y` inside the box: the first bar's left edge and the bars' baseline.
    pub origin_x: i32,
    pub origin_y: i32,
    pub bar_w: u32,
    pub bar_h: u32,
    pub gap: u32,
    /// `steps`: a bar rises in `bar_h / steps` pixel steps.
    pub steps: u32,
    /// Bins the pipe carries, `size` in the spectrum configuration.
    pub bins: usize,
    /// `max.value`: a raw bin of this value is a full bar.
    pub max_value: f32,
    /// `bgr.*`; `None` for `player.bgr` or nothing.
    pub background: Option<Fill>,
    pub bar: Option<Fill>,
    pub reflection: Option<Fill>,
    pub reflection_gap: i32,
    /// `topping.height` and `topping.step`.
    pub topping: Option<(u32, u32)>,
    /// `fgr.filename` as a path, or empty.
    pub foreground: String,
    /// What the section asks of the bank: `bins`, `channels`, `scale` and
    /// `window`; `None` for a section that says nothing, which is measured
    /// as its `size` rounded up.
    #[serde(default)]
    pub demand: Option<bank::Demand>,
    /// The analyser look, for a section with a `style`; `None` draws the
    /// previous engine's bars from the keys above.
    #[serde(default)]
    pub look: Option<Look>,
}

/// The family an analyser section draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LookStyle {
    #[default]
    Bars,
    /// The band levels joined into a line, the area under it filled.
    Graph,
    /// A disc per band at its level.
    Dots,
    /// A spectrogram: each frame's levels a row of colour, moving away from the base.
    Waterfall,
}

/// How the bands' colours are taken from the palette.
/// `onset`: what an onset in a band group does to the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnsetLook {
    #[default]
    Off,
    /// The bars of the group brighten and fade back.
    Flash,
    /// Every bar grows a little and settles back.
    Pulse,
    /// A line, or a ring in the radial look, runs from the base to the tip and fades.
    Ring,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    /// The palette runs along the bar, from the base to its top.
    #[default]
    Gradient,
    /// Each bar takes one colour by its place across the bands.
    Index,
    /// Each bar takes one colour by its level.
    Level,
}

/// How two channels share the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    /// The two channels' average, one set of bars.
    #[default]
    Single,
    /// Both channels over each other, the right one translucent.
    DualCombined,
    /// The left channel on the left half, the right on the right.
    DualHorizontal,
    /// The left channel on the top half rising, the right below it hanging.
    DualVertical,
}

/// A weighting curve applied to the bands by their frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Weighting {
    #[default]
    None,
    A,
    B,
    C,
    D,
    #[serde(rename = "468")]
    Itu468,
}

impl Weighting {
    pub fn parse(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            "" | "none" | "off" => Some(Self::None),
            "a" => Some(Self::A),
            "b" => Some(Self::B),
            "c" => Some(Self::C),
            "d" => Some(Self::D),
            "468" | "itu-r 468" | "itu468" => Some(Self::Itu468),
            _ => None,
        }
    }

    /// The curve's gain at `hz`, in decibels, zero at 1 kHz.
    pub fn gain_db(self, hz: f32) -> f32 {
        let f = hz.max(1.0);
        let f2 = f * f;
        match self {
            Self::None => 0.0,
            Self::A => {
                let n = 12194.0f32.powi(2) * f2 * f2;
                let d = (f2 + 20.6f32.powi(2))
                    * ((f2 + 107.7f32.powi(2)) * (f2 + 737.9f32.powi(2))).sqrt()
                    * (f2 + 12194.0f32.powi(2));
                20.0 * (n / d).log10() + 2.0
            }
            Self::B => {
                let n = 12194.0f32.powi(2) * f2 * f;
                let d = (f2 + 20.6f32.powi(2))
                    * (f2 + 158.5f32.powi(2)).sqrt()
                    * (f2 + 12194.0f32.powi(2));
                20.0 * (n / d).log10() + 0.17
            }
            Self::C => {
                let n = 12194.0f32.powi(2) * f2;
                let d = (f2 + 20.6f32.powi(2)) * (f2 + 12194.0f32.powi(2));
                20.0 * (n / d).log10() + 0.06
            }
            Self::D => {
                let h = ((1_037_918.5 - f2).powi(2) + 1_080_768.1 * f2)
                    / ((9_837_328.0 - f2).powi(2) + 11_723_776.0 * f2);
                let n = f / 6.896_689e-5 * (h / ((f2 + 79_919.29) * (f2 + 1_345_600.0))).sqrt();
                20.0 * n.log10()
            }
            Self::Itu468 => {
                // The curve's samples at the standard's frequencies, in
                // decibels, joined by straight lines on the log axis.
                const POINTS: [(f32, f32); 16] = [
                    (31.5, -29.9),
                    (63.0, -23.9),
                    (100.0, -19.8),
                    (200.0, -13.8),
                    (400.0, -7.8),
                    (800.0, -1.9),
                    (1000.0, 0.0),
                    (2000.0, 5.6),
                    (3150.0, 9.0),
                    (4000.0, 10.5),
                    (5000.0, 11.7),
                    (6300.0, 12.2),
                    (7100.0, 12.0),
                    (8000.0, 11.4),
                    (10000.0, 7.8),
                    (20000.0, -22.2),
                ];
                if f <= POINTS[0].0 {
                    return POINTS[0].1;
                }
                for pair in POINTS.windows(2) {
                    let ((f0, g0), (f1, g1)) = (pair[0], pair[1]);
                    if f <= f1 {
                        let t = (f.log10() - f0.log10()) / (f1.log10() - f0.log10());
                        return g0 + (g1 - g0) * t;
                    }
                }
                POINTS[POINTS.len() - 1].1
            }
        }
    }
}

/// One colour of a palette: at a position along the bar (0 the base, 1
/// the top), at a level (the bar's level from which it takes over), or
/// evenly spaced when neither is said.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stop {
    pub color: [u8; 4],
    #[serde(default)]
    pub pos: Option<f32>,
    #[serde(default)]
    pub level: Option<f32>,
}

/// A palette: the colours along a bar, or across the bands, or by level.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Palette {
    pub stops: Vec<Stop>,
    /// The name it was taken from; empty for stops of the theme's own.
    #[serde(default)]
    pub name: String,
}

impl Palette {
    /// The shipped palettes, by name.
    pub fn named(name: &str) -> Option<Self> {
        let hex = |s: &str| -> [u8; 4] {
            let n = u32::from_str_radix(s, 16).unwrap_or(0);
            [(n >> 16) as u8, (n >> 8) as u8, n as u8, 255]
        };
        let stops: Vec<&str> = match name.trim().to_ascii_lowercase().as_str() {
            "classic" => vec!["2ecc40", "ffdc00", "ff4136"],
            "orangered" => vec!["ff7f00", "ff2a00"],
            "prism" => vec!["d94a4a", "e8a33d", "e6d34f", "4fc46a", "3fa9dd", "7a5fd0"],
            "rainbow" => vec!["ff0000", "ffa500", "ffff00", "00ff00", "00bfff", "8a2be2"],
            "steelblue" => vec!["0b3d66", "2f7fbf", "9fd3ff"],
            "aurora" => vec!["0b6e63", "2fd3a6", "57e0d0", "7a8bf0", "a06ee0"],
            "ember" => vec!["5a1010", "c23a1a", "f07f26", "ffc24d", "ffe9a6"],
            "ice" => vec!["0d3f66", "1f6fa8", "3fa3d6", "8fd0ea", "e8f7ff"],
            "violet" => vec!["2a1454", "5a2fa0", "8a4fd0", "c07fe8", "efc9ff"],
            "mono" => vec!["2a2f36", "6a737d", "c8d2dc", "ffffff"],
            _ => return None,
        };
        Some(Self {
            stops: stops
                .into_iter()
                .map(|h| Stop {
                    color: hex(h),
                    pos: None,
                    level: None,
                })
                .collect(),
            name: name.trim().to_ascii_lowercase(),
        })
    }

    /// A palette from a configuration value: a shipped name, or a list of
    /// stops `color[@pos][/level]` separated by commas, a colour being
    /// `#rrggbb`, `#rrggbbaa` or `(r,g,b[,a])`.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if let Some(named) = Self::named(value) {
            return Some(named);
        }
        let stops: Vec<Stop> = split_stops(value)
            .into_iter()
            .filter_map(|item| parse_stop(&item))
            .collect();
        (!stops.is_empty()).then_some(Self {
            stops,
            name: String::new(),
        })
    }

    /// The colour a fraction `t` of the way along the palette: stops at
    /// their positions, or evenly spaced.
    pub fn at(&self, t: f32) -> [u8; 4] {
        let n = self.stops.len();
        if n == 0 {
            return [255, 255, 255, 255];
        }
        if n == 1 {
            return self.stops[0].color;
        }
        let t = t.clamp(0.0, 1.0);
        let positions: Vec<f32> = (0..n)
            .map(|i| {
                self.stops[i]
                    .pos
                    .unwrap_or(i as f32 / (n - 1) as f32)
                    .clamp(0.0, 1.0)
            })
            .collect();
        let mut i = 0;
        while i + 1 < n && t > positions[i + 1] {
            i += 1;
        }
        if i + 1 >= n {
            return self.stops[n - 1].color;
        }
        let span = (positions[i + 1] - positions[i]).max(1e-6);
        let f = ((t - positions[i]) / span).clamp(0.0, 1.0);
        let (a, b) = (self.stops[i].color, self.stops[i + 1].color);
        let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f).round() as u8;
        [
            mix(a[0], b[0]),
            mix(a[1], b[1]),
            mix(a[2], b[2]),
            mix(a[3], b[3]),
        ]
    }

    /// The colour for a bar at `level`: the last stop whose level is at
    /// or below it, else the colour along the palette at that level.
    pub fn for_level(&self, level: f32) -> [u8; 4] {
        let mut chosen = None;
        for stop in &self.stops {
            if let Some(from) = stop.level {
                if level >= from {
                    chosen = Some(stop.color);
                }
            }
        }
        chosen.unwrap_or_else(|| self.at(level))
    }
}

/// The stops of a palette value, split at the commas outside parentheses.
fn split_stops(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut current = String::new();
    for c in value.chars() {
        match c {
            '(' => {
                depth += 1;
                current.push(c);
            }
            ')' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

/// One stop: `#rrggbb`, `#rrggbbaa` or `(r,g,b[,a])`, then `@pos` and
/// `/level` as fractions.
fn parse_stop(item: &str) -> Option<Stop> {
    let item = item.trim();
    let (color_part, rest) = match item.find(['@', '/']) {
        Some(i) => (&item[..i], &item[i..]),
        None => (item, ""),
    };
    let color = if let Some(hex) = color_part.trim().strip_prefix('#') {
        let n = u32::from_str_radix(hex, 16).ok()?;
        match hex.len() {
            6 => [(n >> 16) as u8, (n >> 8) as u8, n as u8, 255],
            8 => [(n >> 24) as u8, (n >> 16) as u8, (n >> 8) as u8, n as u8],
            _ => return None,
        }
    } else {
        let inner = color_part
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')');
        color_quad(inner)?
    };
    let mut pos = None;
    let mut level = None;
    let mut rest = rest;
    while !rest.is_empty() {
        let (mark, tail) = rest.split_at(1);
        let end = tail.find(['@', '/']).unwrap_or(tail.len());
        let number: f32 = tail[..end].trim().parse().ok()?;
        match mark {
            "@" => pos = Some(number.clamp(0.0, 1.0)),
            _ => level = Some(number.clamp(0.0, 1.0)),
        }
        rest = &tail[end..];
    }
    Some(Stop { color, pos, level })
}

/// An analyser section's look: audioMotion's controls as theme keys, with
/// its defaults, so a configuration ports across.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Look {
    pub style: LookStyle,
    /// `range`: the bands drawn, in hertz.
    pub range: (f32, f32),
    /// `level.range`: the decibels of an empty and a full bar. The bank's
    /// bands read hotter than a browser's single bins, a full-scale sine
    /// reading 0 dB in its band, so the default is -60 to 0 rather than
    /// audioMotion's -85 to -25.
    pub level_range: (f32, f32),
    /// `level.scale = linear`: the bar by amplitude, raised to `1 / level.boost`.
    pub level_linear: bool,
    pub level_boost: f32,
    pub weighting: Weighting,
    /// `smoothing`: 0 follows every hop, 1 never moves.
    pub smoothing: f32,
    pub peaks: bool,
    pub peak_hold_ms: u32,
    /// `peaks.fade`: milliseconds a peak fades over instead of falling.
    pub peak_fade_ms: Option<u32>,
    /// `gravity`: thousands of pixels a second squared a peak falls with.
    pub gravity: f32,
    /// `bar.space`: below 1 a share of the bar's pitch, else pixels.
    pub bar_space: f32,
    pub round: bool,
    pub outline: bool,
    pub line_width: f32,
    pub fill_alpha: f32,
    pub led: bool,
    pub led_max: u32,
    /// `led.space`: the gaps between LEDs vertically and horizontally.
    pub led_space: (f32, f32),
    pub led_true: bool,
    pub lumi: bool,
    pub alpha_bars: bool,
    /// `mirror`: -1 the bands mirrored to the left, 1 to the right.
    pub mirror: i8,
    pub layout: Layout,
    pub palette: Palette,
    pub palette_left: Option<Palette>,
    pub palette_right: Option<Palette>,
    pub palette_split: bool,
    /// `palette.dir = h`: the palette runs across the bands, not along a bar.
    pub palette_horizontal: bool,
    pub color_mode: ColorMode,
    /// `reflex`: the share of the box the reflection takes, 0 for none.
    pub reflex: f32,
    pub reflex_alpha: f32,
    pub reflex_bright: f32,
    pub reflex_fit: bool,
    /// `bgr.alpha`: how solid the box's own background is.
    pub bgr_alpha: f32,
    /// `radial`: the bars radiate from a circle instead of rising from a
    /// baseline; LEDs, luminance, outlines, rounding and the reflection
    /// do not apply.
    #[serde(default)]
    pub radial: bool,
    /// `radial.invert`: the bars grow from the rim towards the centre.
    #[serde(default)]
    pub radial_invert: bool,
    /// `radius`: the base circle as a share of the box's radius; inverted,
    /// how close to the centre the bars reach.
    #[serde(default = "default_radius")]
    pub radius: f32,
    /// `spin`: revolutions a minute, clockwise positive.
    #[serde(default)]
    pub spin_rpm: f32,
    /// `peaks.line`: in the graph style, the peaks joined into a line of
    /// their own instead of a mark per band.
    #[serde(default)]
    pub peak_line: bool,
    /// `scale.x`: a strip of frequency labels under the bars, with
    /// `note.labels` the notes' names instead of hertz.
    #[serde(default)]
    pub scale_x: bool,
    #[serde(default)]
    pub note_labels: bool,
    /// `scale.y`: decibel labels at the left of the bars with faint lines across.
    #[serde(default)]
    pub scale_y: bool,
    /// `scale.size` and `scale.color`: the labels' size and colour.
    #[serde(default = "default_scale_size")]
    pub scale_size: u32,
    #[serde(default = "default_scale_color")]
    pub scale_color: [u8; 3],
    /// `onset`, `onset.decay`, `onset.strength`, `onset.groups`,
    /// `onset.color`: what an onset does, how long it lasts in
    /// milliseconds, how strong it is (0 to 1), which band groups fire it
    /// (bits: sub-bass, bass, mid, high), and the colour it flashes in.
    #[serde(default)]
    pub onset: OnsetLook,
    #[serde(default = "default_onset_decay")]
    pub onset_decay_ms: u32,
    #[serde(default = "default_onset_strength")]
    pub onset_strength: f32,
    #[serde(default = "default_onset_groups")]
    pub onset_groups: u8,
    #[serde(default = "default_onset_color")]
    pub onset_color: [u8; 3],
    /// `dot.size`: a dot's diameter in pixels; 0 takes seven tenths of a band's width.
    #[serde(default)]
    pub dot_size: f32,
    /// `dot.hold`: the dot sits at the band's held peak and falls with it,
    /// instead of at the level with a smaller peak mark above.
    #[serde(default)]
    pub dot_hold: bool,
    /// `blend = add`: the box's own drawing adds its colour to what is
    /// under it within the box instead of covering it, so overlaps bloom.
    #[serde(default)]
    pub blend_add: bool,
    /// `waterfall.speed`: rows a frame the spectrogram moves; `waterfall.reverse`
    /// starts the rows at the far edge instead of the base.
    #[serde(default = "default_waterfall_speed")]
    pub waterfall_speed: u32,
    #[serde(default)]
    pub waterfall_reverse: bool,
    /// `trail`: the share of the last frame kept under this one, a fading
    /// wake; 0 for none.
    #[serde(default)]
    pub trail: f32,
    /// `bar.glow`: a soft halo behind each bar, wider by this share of the
    /// pitch; 0 for none. `line.glow`: a soft band this many pixels wide
    /// around a graph's line.
    #[serde(default)]
    pub bar_glow: f32,
    #[serde(default)]
    pub line_glow: f32,
    /// `bar.fade`: each bar dim at its base and bright at its tip.
    #[serde(default)]
    pub bar_fade: bool,
    /// `sparkle`: specks above loud bars, twinkling.
    #[serde(default)]
    pub sparkle: bool,
    /// `line.width.max`: a graph line this thick at a full level, thinning
    /// to `line.width` at nothing; 0 for a fixed width.
    #[serde(default)]
    pub line_width_max: f32,
    /// `echo`: a ghost of the levels that follows them by this share a
    /// frame, drawn as a line of its own; 0 for none.
    #[serde(default)]
    pub echo: f32,
}

fn default_waterfall_speed() -> u32 {
    1
}

fn default_onset_decay() -> u32 {
    150
}

fn default_onset_strength() -> f32 {
    0.6
}

fn default_onset_groups() -> u8 {
    0b1111
}

fn default_onset_color() -> [u8; 3] {
    [255, 255, 255]
}

fn default_scale_size() -> u32 {
    11
}

fn default_scale_color() -> [u8; 3] {
    [180, 180, 180]
}

fn default_radius() -> f32 {
    0.3
}

impl Default for Look {
    fn default() -> Self {
        Self {
            style: LookStyle::Bars,
            range: (20.0, 22_000.0),
            level_range: (-60.0, 0.0),
            level_linear: false,
            level_boost: 1.0,
            weighting: Weighting::None,
            smoothing: 0.5,
            peaks: true,
            peak_hold_ms: 500,
            peak_fade_ms: None,
            gravity: 3.8,
            bar_space: 0.1,
            round: false,
            outline: false,
            line_width: 0.0,
            fill_alpha: 1.0,
            led: false,
            led_max: 0,
            led_space: (0.25, 0.3),
            led_true: false,
            lumi: false,
            alpha_bars: false,
            mirror: 0,
            layout: Layout::Single,
            palette: Palette::named("classic").unwrap_or_default(),
            palette_left: None,
            palette_right: None,
            palette_split: false,
            palette_horizontal: false,
            color_mode: ColorMode::Gradient,
            reflex: 0.0,
            reflex_alpha: 0.15,
            reflex_bright: 1.0,
            reflex_fit: true,
            bgr_alpha: 0.7,
            radial: false,
            radial_invert: false,
            radius: default_radius(),
            spin_rpm: 0.0,
            peak_line: false,
            scale_x: false,
            note_labels: false,
            scale_y: false,
            scale_size: default_scale_size(),
            scale_color: default_scale_color(),
            onset: OnsetLook::Off,
            onset_decay_ms: default_onset_decay(),
            onset_strength: default_onset_strength(),
            onset_groups: default_onset_groups(),
            onset_color: default_onset_color(),
            dot_size: 0.0,
            dot_hold: false,
            blend_add: false,
            waterfall_speed: default_waterfall_speed(),
            waterfall_reverse: false,
            trail: 0.0,
            bar_glow: 0.0,
            line_glow: 0.0,
            bar_fade: false,
            sparkle: false,
            line_width_max: 0.0,
            echo: 0.0,
        }
    }
}

/// The look of a section with a `style`; `None` for a section without
/// one, or with `style = legacy`.
fn look_from_section(get: &dyn Fn(&str) -> Option<String>) -> Option<Look> {
    let style = get("style")?;
    let style = match style.trim().to_ascii_lowercase().as_str() {
        "bars" => LookStyle::Bars,
        "graph" | "area" | "line" => LookStyle::Graph,
        "dots" => LookStyle::Dots,
        "waterfall" | "spectrogram" => LookStyle::Waterfall,
        _ => return None,
    };
    let mut look = Look {
        style,
        ..Look::default()
    };
    let number = |key: &str| get(key).and_then(|v| v.trim().parse::<f32>().ok());
    let pair = |key: &str| -> Option<(f32, f32)> {
        let v = get(key)?;
        let mut it = v.split(',').map(|p| p.trim().parse::<f32>().ok());
        Some((it.next()??, it.next()??))
    };
    let flag = |key: &str| get(key).map(|v| truthy(Some(v.as_str())));
    if let Some(r) = pair("range") {
        look.range = (r.0.max(1.0), r.1.max(r.0 + 1.0));
    }
    if let Some(r) = pair("level.range") {
        look.level_range = (r.0.min(r.1 - 1.0), r.1);
    }
    if let Some(v) = get("level.scale") {
        look.level_linear = v.trim().eq_ignore_ascii_case("linear");
    }
    if let Some(v) = number("level.boost") {
        look.level_boost = v.max(1.0);
    }
    if let Some(v) = get("weighting") {
        look.weighting = Weighting::parse(&v).unwrap_or_default();
    }
    if let Some(v) = number("smoothing") {
        look.smoothing = v.clamp(0.0, 1.0);
    }
    if let Some(v) = flag("peaks") {
        look.peaks = v;
    }
    if let Some(v) = number("peaks.hold") {
        look.peak_hold_ms = v.max(0.0) as u32;
    }
    if let Some(v) = get("peaks.fade") {
        look.peak_fade_ms = match v.trim().to_ascii_lowercase().as_str() {
            "off" | "false" | "0" | "" => None,
            "on" | "true" => Some(750),
            other => other.parse::<f32>().ok().map(|ms| ms.max(0.0) as u32),
        };
    }
    if let Some(v) = number("gravity") {
        look.gravity = v.max(0.01);
    }
    if let Some(v) = number("bar.space") {
        look.bar_space = v.max(0.0);
    }
    if let Some(v) = flag("bar.round") {
        look.round = v;
    }
    if let Some(v) = flag("bar.outline") {
        look.outline = v;
    }
    if let Some(v) = number("line.width") {
        look.line_width = v.max(0.0);
    }
    if let Some(v) = number("fill.alpha") {
        look.fill_alpha = v.clamp(0.0, 1.0);
    }
    if let Some(v) = flag("led") {
        look.led = v;
    }
    if let Some(v) = number("led.max") {
        look.led_max = v.max(0.0) as u32;
    }
    if let Some(r) = pair("led.space") {
        look.led_space = (r.0.max(0.0), r.1.max(0.0));
    }
    if let Some(v) = flag("led.true") {
        look.led_true = v;
    }
    if let Some(v) = flag("lumi") {
        look.lumi = v;
    }
    if let Some(v) = flag("alpha") {
        look.alpha_bars = v;
    }
    if let Some(v) = number("mirror") {
        look.mirror = v.round().clamp(-1.0, 1.0) as i8;
    }
    if let Some(v) = get("layout") {
        look.layout = match v.trim().to_ascii_lowercase().as_str() {
            "dual-combined" | "combined" => Layout::DualCombined,
            "dual-horizontal" | "horizontal" => Layout::DualHorizontal,
            "dual-vertical" | "vertical" => Layout::DualVertical,
            _ => Layout::Single,
        };
    }
    if let Some(p) = get("palette").and_then(|v| Palette::parse(&v)) {
        look.palette = p;
    }
    look.palette_left = get("palette.left").and_then(|v| Palette::parse(&v));
    look.palette_right = get("palette.right").and_then(|v| Palette::parse(&v));
    if let Some(v) = flag("palette.split") {
        look.palette_split = v;
    }
    if let Some(v) = get("palette.dir") {
        look.palette_horizontal = v.trim().to_ascii_lowercase().starts_with('h');
    }
    if let Some(v) = get("color.mode") {
        look.color_mode = match v.trim().to_ascii_lowercase().as_str() {
            "index" | "bar-index" => ColorMode::Index,
            "level" | "bar-level" => ColorMode::Level,
            _ => ColorMode::Gradient,
        };
    }
    if let Some(v) = number("reflex") {
        look.reflex = v.clamp(0.0, 0.99);
    }
    if let Some(v) = number("reflex.alpha") {
        look.reflex_alpha = v.clamp(0.0, 1.0);
    }
    if let Some(v) = number("reflex.bright") {
        look.reflex_bright = v.max(0.0);
    }
    if let Some(v) = flag("reflex.fit") {
        look.reflex_fit = v;
    }
    if let Some(v) = number("bgr.alpha") {
        look.bgr_alpha = v.clamp(0.0, 1.0);
    }
    if let Some(v) = flag("radial") {
        look.radial = v;
    }
    if let Some(v) = flag("radial.invert") {
        look.radial_invert = v;
    }
    if let Some(v) = number("radius") {
        look.radius = v.clamp(0.0, 0.95);
    }
    if let Some(v) = number("spin").or_else(|| number("spin.speed")) {
        look.spin_rpm = v.clamp(-600.0, 600.0);
    }
    if let Some(v) = flag("peaks.line") {
        look.peak_line = v;
    }
    if let Some(v) = flag("scale.x") {
        look.scale_x = v;
    }
    if let Some(v) = flag("scale.y") {
        look.scale_y = v;
    }
    if let Some(v) = flag("note.labels") {
        look.note_labels = v;
    }
    if let Some(v) = number("scale.size") {
        look.scale_size = v.clamp(6.0, 48.0) as u32;
    }
    if let Some(c) = get("scale.color").and_then(|v| color_triplet(&v)) {
        look.scale_color = c;
    }
    if let Some(v) = get("onset") {
        look.onset = match v.trim().to_ascii_lowercase().as_str() {
            "flash" => OnsetLook::Flash,
            "pulse" => OnsetLook::Pulse,
            "ring" => OnsetLook::Ring,
            _ => OnsetLook::Off,
        };
    }
    if let Some(v) = number("onset.decay") {
        look.onset_decay_ms = v.clamp(30.0, 3000.0) as u32;
    }
    if let Some(v) = number("onset.strength") {
        look.onset_strength = v.clamp(0.0, 1.0);
    }
    if let Some(v) = get("onset.groups") {
        let v = v.trim().to_ascii_lowercase();
        look.onset_groups = if v == "all" || v.is_empty() {
            0b1111
        } else {
            v.split(',').map(str::trim).fold(0u8, |bits, g| match g {
                "sub" | "sub-bass" | "subbass" => bits | 1,
                "bass" => bits | 2,
                "mid" => bits | 4,
                "high" => bits | 8,
                _ => bits,
            })
        };
    }
    if let Some(c) = get("onset.color").and_then(|v| color_triplet(&v)) {
        look.onset_color = c;
    }
    if let Some(v) = number("dot.size") {
        look.dot_size = v.clamp(0.0, 200.0);
    }
    if let Some(v) = flag("dot.hold") {
        look.dot_hold = v;
    }
    if let Some(v) = get("blend") {
        look.blend_add = v.trim().eq_ignore_ascii_case("add");
    }
    if let Some(v) = number("waterfall.speed") {
        look.waterfall_speed = v.clamp(1.0, 16.0) as u32;
    }
    if let Some(v) = flag("waterfall.reverse") {
        look.waterfall_reverse = v;
    }
    if let Some(v) = number("trail") {
        look.trail = v.clamp(0.0, 0.98);
    }
    if let Some(v) = get("bar.glow") {
        look.bar_glow = match v.trim().parse::<f32>() {
            Ok(n) => n.clamp(0.0, 8.0),
            Err(_) if truthy(Some(v.as_str())) => 1.2,
            Err(_) => 0.0,
        };
    }
    if let Some(v) = number("line.glow") {
        look.line_glow = v.clamp(0.0, 200.0);
    }
    if let Some(v) = flag("bar.fade") {
        look.bar_fade = v;
    }
    if let Some(v) = flag("sparkle") {
        look.sparkle = v;
    }
    if let Some(v) = number("line.width.max") {
        look.line_width_max = v.clamp(0.0, 200.0);
    }
    if let Some(v) = get("echo") {
        look.echo = match v.trim().parse::<f32>() {
            Ok(n) => n.clamp(0.0, 1.0),
            Err(_) if truthy(Some(v.as_str())) => 0.05,
            Err(_) => 0.0,
        };
    }
    Some(look)
}

impl SpectrumSpec {
    /// Pixels per step, `int(bar.height / steps)`, at least one.
    pub fn step(&self) -> u32 {
        (self.bar_h / self.steps.max(1)).max(1)
    }

    /// The bar height for one raw bin: the value scaled to the bar, rounded
    /// up to a whole step.
    pub fn bar_height(&self, raw: f32) -> u32 {
        let unit = self.bar_h as f32 / self.max_value.max(1.0);
        let v = raw * unit;
        if v <= 0.0 {
            return 0;
        }
        let step = self.step() as f32;
        let n = if v % step == 0.0 {
            (v / step) as u32
        } else {
            (v / step) as u32 + 1
        };
        n * self.step()
    }
}

/// The spectrum settings of the spectrum engine's `config.txt`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpectrumSettings {
    pub base_folder: String,
    pub folder: String,
    pub bins: usize,
    pub max_value: f32,
}

pub fn spectrum_settings(text: &str) -> SpectrumSettings {
    SpectrumSettings {
        base_folder: section_value(text, "current", "base.folder").unwrap_or_default(),
        folder: section_value(text, "current", "spectrum.folder").unwrap_or_default(),
        bins: section_value(text, "current", "size")
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(DEFAULT_SPECTRUM_BINS),
        max_value: section_value(text, "current", "max.value")
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|&m| m > 0.0)
            .unwrap_or(DEFAULT_SPECTRUM_MAX),
    }
}

/// Which channel of the bank a spectrum box draws in the `single` layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpectrumChannel {
    #[default]
    Mean,
    Left,
    Right,
}

/// The first spectrum a meter shows: its name and box. See `meter_spectra`.
pub fn meter_spectrum(meters_txt: &str, meter: &str) -> Option<(String, u32, u32)> {
    meter_spectra(meters_txt, meter).into_iter().next()
}

/// The spectrum boxes of a meter that shows a spectrum, `config.extend`
/// and `spectrum.visible` both true: `spectrum.name` names one section, or
/// a comma list of them, each a box of `spectrum.size`, the second and
/// later with their own `spectrum.<n>.size` when given (n from 2).
pub fn meter_spectra(meters_txt: &str, meter: &str) -> Vec<(String, u32, u32)> {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    if !truthy(get("config.extend")) || !truthy(get("spectrum.visible")) {
        return Vec::new();
    }
    let size = |key: &str| -> Option<(u32, u32)> {
        let mut parts = get(key)?.split(',');
        let w = parts.next()?.trim().parse().ok()?;
        let h = parts.next()?.trim().parse().ok()?;
        Some((w, h))
    };
    let Some(names) = get("spectrum.name") else {
        return Vec::new();
    };
    let Some(shared) = size("spectrum.size") else {
        return Vec::new();
    };
    names
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .enumerate()
        .map(|(i, name)| {
            let (w, h) = if i > 0 {
                size(&format!("spectrum.{}.size", i + 1)).unwrap_or(shared)
            } else {
                shared
            };
            (name.to_string(), w, h)
        })
        .collect()
}

fn color_quad(value: &str) -> Option<[u8; 4]> {
    let parts: Vec<u8> = value
        .split(',')
        .map(|p| p.trim().parse::<u8>())
        .collect::<Result<_, _>>()
        .ok()?;
    match parts.as_slice() {
        [r, g, b] => Some([*r, *g, *b, 255]),
        [r, g, b, a] => Some([*r, *g, *b, *a]),
        _ => None,
    }
}

fn gradient_list(value: &str) -> Option<Vec<[u8; 4]>> {
    let colors: Vec<[u8; 4]> = value
        .split('(')
        .skip(1)
        .filter_map(|part| part.split(')').next())
        .filter_map(color_quad)
        .collect();
    (!colors.is_empty()).then_some(colors)
}

/// One `[name]` of a theme's `spectrum.txt`, sized by the meter's box, with
/// picture names turned into paths under `dir`.
pub fn spectrum_from_theme(
    spectrum_txt: &str,
    name: &str,
    (w, h): (u32, u32),
    settings: &SpectrumSettings,
    dir: &str,
) -> Option<SpectrumSpec> {
    let values = section_values(spectrum_txt, name);
    if values.is_empty() {
        return None;
    }
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .filter(|v| !v.is_empty())
    };
    let int = |key: &str| get(key).and_then(|v| v.parse::<i32>().ok());
    let uint = |key: &str| get(key).and_then(|v| v.parse::<u32>().ok());
    let path = |file: &str| {
        if dir.is_empty() {
            file.to_string()
        } else {
            format!("{}/{}", dir.trim_end_matches('/'), file)
        }
    };
    let fill = |kind: &str, color: &str, gradient: &str, file: &str| -> Option<Fill> {
        match get(kind).map(|v| v.to_ascii_lowercase()).as_deref() {
            Some("color") => get(color).and_then(color_quad).map(Fill::Color),
            Some("gradient") => get(gradient).and_then(gradient_list).map(Fill::Gradient),
            Some("image") => get(file).map(|f| Fill::Image(path(f))),
            Some("image.extended") => get(file).map(|f| Fill::ImageExtended(path(f))),
            _ => None,
        }
    };
    Some(SpectrumSpec {
        x: int("spectrum.x").unwrap_or(0),
        y: int("spectrum.y").unwrap_or(0),
        channel: match get("channel")
            .map(|v| v.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("left") | Some("l") => SpectrumChannel::Left,
            Some("right") | Some("r") => SpectrumChannel::Right,
            _ => SpectrumChannel::Mean,
        },
        w,
        h,
        origin_x: int("origin.x").unwrap_or(0),
        origin_y: int("origin.y").unwrap_or(0),
        bar_w: uint("bar.width").unwrap_or(1).max(1),
        bar_h: uint("bar.height").unwrap_or(1).max(1),
        gap: uint("bar.gap").unwrap_or(0),
        steps: uint("steps").unwrap_or(1).max(1),
        bins: settings.bins,
        max_value: settings.max_value,
        background: fill("bgr.type", "bgr.color", "bgr.gradient", "bgr.filename"),
        bar: fill("bar.type", "bar.color", "bar.gradient", "bar.filename"),
        reflection: fill(
            "reflection.type",
            "reflection.color",
            "reflection.gradient",
            "reflection.filename",
        ),
        reflection_gap: int("reflection.gap").unwrap_or(0),
        topping: match (uint("topping.height"), uint("topping.step")) {
            (Some(height), Some(step)) if height > 0 => Some((height, step)),
            _ => None,
        },
        foreground: get("fgr.filename").map(path).unwrap_or_default(),
        demand: section_demand(get("bins"), get("channels"), get("scale"), get("window")),
        look: look_from_section(&|key: &str| get(key).map(str::to_string)),
    })
}

/// The bank a spectrum section asks for, from its `bins`, `channels`,
/// `scale` and `window` values; `None` when it names none of them.
fn section_demand(
    bins: Option<&str>,
    channels: Option<&str>,
    scale: Option<&str>,
    window: Option<&str>,
) -> Option<bank::Demand> {
    let bins = bins.and_then(|v| v.trim().parse::<usize>().ok());
    let channels = channels.and_then(|v| v.trim().parse::<usize>().ok());
    let scale = scale.and_then(bank::Scale::parse);
    let window = window.and_then(|v| v.trim().parse::<usize>().ok());
    if bins.is_none() && channels.is_none() && scale.is_none() && window.is_none() {
        return None;
    }
    Some(
        bank::Demand {
            bins: bins.unwrap_or(bank::MAX_BINS),
            channels: channels.unwrap_or(2),
            scale: scale.unwrap_or_default(),
            window: window.unwrap_or(0),
        }
        .clean(),
    )
}

/// The bank count a theme of the previous engine is measured at: its bar
/// count rounded up to the next of 32, 64, 128 and 256.
pub fn bins_for_legacy(size: usize) -> usize {
    [32, 64, 128, 256]
        .into_iter()
        .find(|b| *b >= size)
        .unwrap_or(bank::MAX_BINS)
}

/// How a picture is placed in a box: kept in proportion and centred, or
/// stretched to fill it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Scale {
    #[default]
    Fit,
    Stretch,
    /// Fill the box keeping the picture's proportion, cropped centred.
    Cover,
}

/// Where a decorative layer sits: under the meters, or over everything but
/// the foreground.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ZOrder {
    Background,
    #[default]
    Overlay,
}

/// A decorative picture from the playing track's folder, `folderlayer.N.*`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FolderLayerSpec {
    /// File names tried in order inside the track's folder.
    pub files: Vec<String>,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub scale: Scale,
    pub zorder: ZOrder,
    /// Border width in pixels around the box, in `font.color`. Zero is none.
    pub border: u32,
    pub border_color: [u8; 3],
}

/// The default candidates when a layer names no files.
pub const FOLDER_LAYER_FILES: [&str; 6] = [
    "back.png", "Back.png", "back.jpg", "Back.jpg", "logo.png", "Logo.png",
];

/// The folder layers a meter declares: the legacy `folderlayer.*` when
/// `folderlayer.enabled` is true, then `folderlayer.1.*` to `folderlayer.5.*`,
/// each needing a position and a dimension.
pub fn meter_folder_layers(meters_txt: &str, meter: &str) -> Vec<FolderLayerSpec> {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let font_color = get("font.color")
        .and_then(color_triplet)
        .unwrap_or([255, 255, 255]);
    let pair = |key: &str| -> Option<(u32, u32)> {
        let mut parts = get(key)?.split(',');
        let a = parts.next()?.trim().parse().ok()?;
        let b = parts.next()?.trim().parse().ok()?;
        Some((a, b))
    };
    let mut prefixes = Vec::new();
    if truthy(get("folderlayer.enabled")) {
        prefixes.push("folderlayer".to_string());
    }
    prefixes.extend((1..=5).map(|n| format!("folderlayer.{n}")));
    prefixes
        .iter()
        .filter_map(|prefix| {
            let (x, y) = pair(&format!("{prefix}.pos"))?;
            let (w, h) = pair(&format!("{prefix}.dimension"))?;
            let files: Vec<String> = get(&format!("{prefix}.files"))
                .map(|list| {
                    list.split(',')
                        .map(|f| f.trim().to_string())
                        .filter(|f| !f.is_empty())
                        .collect()
                })
                .filter(|files: &Vec<String>| !files.is_empty())
                .unwrap_or_else(|| FOLDER_LAYER_FILES.iter().map(|f| f.to_string()).collect());
            Some(FolderLayerSpec {
                files,
                x,
                y,
                w,
                h,
                scale: match get(&format!("{prefix}.scale"))
                    .map(|v| v.trim().to_ascii_lowercase())
                    .as_deref()
                {
                    Some("stretch") => Scale::Stretch,
                    Some("cover") => Scale::Cover,
                    _ => Scale::Fit,
                },
                zorder: match get(&format!("{prefix}.zorder"))
                    .map(|v| v.trim().to_ascii_lowercase())
                    .as_deref()
                {
                    Some("background") => ZOrder::Background,
                    _ => ZOrder::Overlay,
                },
                border: get(&format!("{prefix}.border"))
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0),
                border_color: font_color,
            })
        })
        .collect()
}

/// The files a folder layer may show for a track, as paths on the player,
/// in the order to try them. The track's location is its folder under
/// `/mnt`, after the player's `music-library/` or `mnt/` prefix; a name with
/// a slash, a `..`, or an extension other than png, jpg, jpeg, gif or webp is
/// skipped. Empty for a source that is not a file.
pub fn folder_candidates(uri: &str, files: &[String]) -> Vec<String> {
    let uri = uri.trim();
    if uri.is_empty() {
        return Vec::new();
    }
    // A track inside a cue sheet is named `cue://<path>@<track>`: the
    // sheet's folder is the track's.
    let uri = match uri.strip_prefix("cue://") {
        Some(rest) => rest.rsplit_once('@').map(|(path, _)| path).unwrap_or(rest),
        None => uri,
    };
    let stripped = uri
        .strip_prefix("music-library/")
        .or_else(|| uri.strip_prefix("music-library"))
        .unwrap_or(uri);
    let stripped = stripped
        .strip_prefix("mnt/")
        .or_else(|| stripped.strip_prefix("mnt"))
        .unwrap_or(stripped);
    let base = if stripped.starts_with('/') {
        format!("/mnt{stripped}")
    } else {
        format!("/mnt/{stripped}")
    };
    let Some(slash) = base.rfind('/') else {
        return Vec::new();
    };
    let folder = &base[..slash];
    if !folder.starts_with("/mnt") {
        return Vec::new();
    }
    files
        .iter()
        .map(|f| f.trim())
        .filter(|f| !f.is_empty() && !f.contains('/') && !f.contains(".."))
        .filter(|f| {
            let lower = f.to_ascii_lowercase();
            [".png", ".jpg", ".jpeg", ".gif", ".webp"]
                .iter()
                .any(|ext| lower.ends_with(ext))
        })
        .map(|f| format!("{folder}/{f}"))
        .collect()
}

/// The artist fanart slot a meter offers, `fanart.pos` and `fanart.dimension`.
/// The player decides whether anything is shown in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FanartSpec {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub scale: Scale,
    pub zorder: ZOrder,
}

pub fn meter_fanart(meters_txt: &str, meter: &str) -> Option<FanartSpec> {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let pair = |key: &str| -> Option<(u32, u32)> {
        let mut parts = get(key)?.split(',');
        let a = parts.next()?.trim().parse().ok()?;
        let b = parts.next()?.trim().parse().ok()?;
        Some((a, b))
    };
    let (x, y) = pair("fanart.pos")?;
    let (w, h) = pair("fanart.dimension")?;
    Some(FanartSpec {
        x,
        y,
        w,
        h,
        scale: match get("fanart.scale")
            .map(|v| v.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("stretch") => Scale::Stretch,
            Some("cover") => Scale::Cover,
            _ => Scale::Fit,
        },
        zorder: match get("fanart.zorder")
            .map(|v| v.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("overlay") => ZOrder::Overlay,
            _ => ZOrder::Background,
        },
    })
}

/// A state indicator's look: a coloured LED shape, or one picture per state,
/// either with an optional glow behind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StateLook {
    Led {
        w: u32,
        h: u32,
        circle: bool,
        /// One colour per state, in state order; a state past the end takes the last.
        colors: Vec<[u8; 3]>,
    },
    Icons {
        /// One picture path per state; an empty path is a state with no picture.
        files: Vec<String>,
    },
}

/// A mute, shuffle, repeat or play-state indicator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateIndicator {
    pub x: i32,
    pub y: i32,
    pub look: StateLook,
    /// Glow blur radius in pixels, its opacity, and its colour per state
    /// (empty: the LED colour, or white behind a picture).
    pub glow: u32,
    pub glow_intensity: f32,
    pub glow_colors: Vec<[u8; 3]>,
}

impl StateIndicator {
    /// How many states the look provides.
    pub fn states(&self) -> usize {
        match &self.look {
            StateLook::Led { colors, .. } => colors.len(),
            StateLook::Icons { files } => files.len(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GaugeStyle {
    #[default]
    Numeric,
    Slider,
    Knob,
    Arc,
}

/// A marker along a progress gauge: a picture or a label at a percentage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    pub pos: f32,
    pub image: String,
    pub label: String,
    pub font_size: Option<u32>,
}

/// A value from 0 to 100 shown as a number, a bar, a knob or an arc: the
/// volume and the progress indicators.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GaugeSpec {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub style: GaugeStyle,
    pub color: [u8; 3],
    pub bg_color: Option<[u8; 3]>,
    pub font_size: u32,
    pub knob_image: String,
    pub knob_start: f32,
    pub knob_end: f32,
    pub arc_width: u32,
    pub arc_start: f32,
    pub arc_end: f32,
    pub track: String,
    pub tip: String,
    /// `vertical`, `horizontal`, or another word for detection from the box.
    pub orientation: String,
    pub travel: Option<(i32, i32)>,
    pub tip_offset: (i32, i32),
    pub fill_color: Option<[u8; 3]>,
    pub fill_width: Option<u32>,
    pub fill_offset: (i32, i32),
    pub fill_radius: u32,
    /// Progress only: a border of this width in `border_color` around the bar.
    pub border: u32,
    pub border_color: [u8; 3],
    pub markers: Vec<Marker>,
    pub head_image: String,
    pub head_offset: (i32, i32),
}

impl GaugeSpec {
    /// Whether the bar fills bottom to top.
    pub fn vertical(&self) -> bool {
        self.orientation == "vertical" || (self.orientation != "horizontal" && self.h > self.w)
    }
}

/// The indicators a meter declares under `config.extend`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct IndicatorsSpec {
    pub volume: Option<GaugeSpec>,
    pub mute: Option<StateIndicator>,
    pub shuffle: Option<StateIndicator>,
    pub repeat: Option<StateIndicator>,
    pub playstate: Option<StateIndicator>,
    pub progress: Option<GaugeSpec>,
    /// Buttons the theme draws and wires to actions: `button.<name>.*`.
    #[serde(default)]
    pub buttons: Vec<ButtonSpec>,
    /// `interactive = True`: the meter is meant for fingers, its indicators
    /// and buttons act. The player's `touch.interactive` setting may
    /// override it either way.
    #[serde(default)]
    pub interactive: bool,
    /// `touch.margin`: half the least size of a control's touch region,
    /// in pixels; a control smaller than twice this grows to it, centred.
    /// 24 unless the theme says; 0 keeps every control to its drawn box.
    #[serde(default = "default_touch_margin")]
    pub touch_margin: u32,
}

fn default_touch_margin() -> u32 {
    24
}

/// A button a theme draws: a picture at a position, or a bare region with
/// a size, wired to one action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ButtonSpec {
    pub name: String,
    pub x: i32,
    pub y: i32,
    /// The region's size; zero takes the picture's size.
    pub w: u32,
    pub h: u32,
    /// The picture drawn at the position, or empty for a bare region.
    pub image: String,
    /// The picture drawn while the button is active, the second of
    /// `button.<name>.image`'s list; empty draws `image` throughout.
    #[serde(default)]
    pub image_active: String,
    /// The whole list of `button.<name>.image`: with three or more, one
    /// picture per state of the action's indicator (repeat: off, all,
    /// single, infinity; mute: off, muted, zero; random: off, on; play,
    /// pause, stop and toggle: stop, pause, play) instead of rest and active.
    #[serde(default)]
    pub images: Vec<String>,
    pub action: ButtonAction,
}

/// What a button does when tapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ButtonAction {
    /// Play or pause.
    Toggle,
    Play,
    Pause,
    Stop,
    /// The next or the previous track.
    Next,
    Previous,
    /// The next or the previous meter of the theme's rotation.
    MeterNext,
    MeterPrevious,
    Mute,
    Random,
    /// Off, all, single, round again.
    Repeat,
    /// What a touch does with `exit.on.touch`: the display leaves.
    Dismiss,
}

impl ButtonAction {
    /// The action a theme names: `toggle` (also `playpause`), `play`,
    /// `pause`, `stop`, `next`, `previous` (also `prev`), `meter.next`,
    /// `meter.previous`, `mute`, `random` (also `shuffle`), `repeat`, `dismiss`.
    pub fn parse(word: &str) -> Option<ButtonAction> {
        Some(match word.trim().to_ascii_lowercase().as_str() {
            "toggle" | "playpause" | "play.pause" => ButtonAction::Toggle,
            "play" => ButtonAction::Play,
            "pause" => ButtonAction::Pause,
            "stop" => ButtonAction::Stop,
            "next" => ButtonAction::Next,
            "previous" | "prev" => ButtonAction::Previous,
            "meter.next" => ButtonAction::MeterNext,
            "meter.previous" | "meter.prev" => ButtonAction::MeterPrevious,
            "mute" => ButtonAction::Mute,
            "random" | "shuffle" => ButtonAction::Random,
            "repeat" => ButtonAction::Repeat,
            "dismiss" | "exit" => ButtonAction::Dismiss,
            _ => return None,
        })
    }
}

/// Whether the controls of a theme act: as the theme says of each meter,
/// on for every theme, or off for every theme; `touch.interactive` in
/// the display's configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum InteractiveMode {
    #[default]
    Theme,
    On,
    Off,
}

impl InteractiveMode {
    pub fn parse(word: Option<&str>) -> InteractiveMode {
        match word.map(|w| w.trim().to_ascii_lowercase()).as_deref() {
            Some("on") | Some("true") | Some("yes") | Some("all") => InteractiveMode::On,
            Some("off") | Some("false") | Some("no") | Some("none") => InteractiveMode::Off,
            _ => InteractiveMode::Theme,
        }
    }
}

impl IndicatorsSpec {
    pub fn is_empty(&self) -> bool {
        self.volume.is_none()
            && self.mute.is_none()
            && self.shuffle.is_none()
            && self.repeat.is_none()
            && self.playstate.is_none()
            && self.progress.is_none()
            && self.buttons.is_empty()
    }
}

fn color_list(value: &str) -> Vec<[u8; 3]> {
    let numbers: Vec<u8> = value
        .split(',')
        .filter_map(|p| p.trim().parse::<u8>().ok())
        .collect();
    numbers
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| [c[0], c[1], c[2]])
        .collect()
}

/// The indicators of a meter: `volume.*`, `mute.*`, `shuffle.*`, `repeat.*`,
/// `playstate.*` and `progress.*`, as the player's parser reads them.
pub fn meter_indicators(meters_txt: &str, meter: &str, theme_dir: &str) -> Option<IndicatorsSpec> {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    if !truthy(get("config.extend")) {
        return None;
    }
    let ipair = |key: &str| -> Option<(i32, i32)> {
        let mut parts = get(key)?.split(',');
        Some((
            parts.next()?.trim().parse().ok()?,
            parts.next()?.trim().parse().ok()?,
        ))
    };
    let upair = |key: &str| -> Option<(u32, u32)> {
        let mut parts = get(key)?.split(',');
        Some((
            parts.next()?.trim().parse().ok()?,
            parts.next()?.trim().parse().ok()?,
        ))
    };
    let number = |key: &str, default: f32| {
        get(key)
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(default)
    };
    let path = |file: &str| {
        if theme_dir.is_empty() {
            file.to_string()
        } else {
            format!("{}/{}", theme_dir.trim_end_matches('/'), file)
        }
    };
    // LED colour lists as the player reads them: mute and play state take
    // the listed triples; shuffle takes nine values as off, shuffle, infinity
    // and six legacy values as on, off (so off comes second); repeat takes
    // three or four triples.
    let state =
        |name: &str, default_colors: Vec<[u8; 3]>, legacy_swap: bool| -> Option<StateIndicator> {
            let (x, y) = ipair(&format!("{name}.pos"))?;
            let (look, glow, glow_intensity, glow_colors) =
                if let Some((w, h)) = upair(&format!("{name}.led")) {
                    let mut colors = get(&format!("{name}.led.color"))
                        .map(color_list)
                        .unwrap_or_default();
                    if legacy_swap && colors.len() == 2 {
                        colors = vec![colors[1], colors[0], colors[0]];
                    }
                    if colors.len() < 2 {
                        colors = default_colors.clone();
                    }
                    let mut glow_colors = get(&format!("{name}.led.glow.color"))
                        .map(color_list)
                        .unwrap_or_default();
                    if legacy_swap && glow_colors.len() == 2 {
                        glow_colors = vec![glow_colors[1], glow_colors[0], glow_colors[0]];
                    }
                    (
                        StateLook::Led {
                            w,
                            h,
                            circle: get(&format!("{name}.led.shape"))
                                .is_none_or(|s| !s.eq_ignore_ascii_case("rect")),
                            colors,
                        },
                        number(&format!("{name}.led.glow"), 0.0).max(0.0) as u32,
                        number(&format!("{name}.led.glow.intensity"), 0.5).clamp(0.0, 1.0),
                        glow_colors,
                    )
                } else {
                    let files: Vec<String> = get(&format!("{name}.icon"))?
                        .split(',')
                        .map(|f| f.trim())
                        .map(|f| if f.is_empty() { String::new() } else { path(f) })
                        .collect();
                    let mut glow_colors = get(&format!("{name}.icon.glow.color"))
                        .map(color_list)
                        .unwrap_or_default();
                    if legacy_swap && glow_colors.len() == 2 {
                        glow_colors = vec![glow_colors[1], glow_colors[0], glow_colors[0]];
                    }
                    (
                        StateLook::Icons { files },
                        number(&format!("{name}.icon.glow"), 0.0).max(0.0) as u32,
                        number(&format!("{name}.icon.glow.intensity"), 0.5).clamp(0.0, 1.0),
                        glow_colors,
                    )
                };
            Some(StateIndicator {
                x,
                y,
                look,
                glow,
                glow_intensity,
                glow_colors,
            })
        };
    let gauge = |name: &str,
                 default_style: GaugeStyle,
                 default_orientation: &str,
                 default_bg: Option<[u8; 3]>,
                 default_color: [u8; 3]|
     -> Option<GaugeSpec> {
        let (x, y) = ipair(&format!("{name}.pos"))?;
        let (w, h) = upair(&format!("{name}.dim")).or(if name == "volume" {
            Some((100, 20))
        } else {
            None
        })?;
        let style = match get(&format!("{name}.style"))
            .map(|s| s.to_ascii_lowercase())
            .as_deref()
        {
            Some("slider") => GaugeStyle::Slider,
            Some("knob") => GaugeStyle::Knob,
            Some("arc") => GaugeStyle::Arc,
            Some("numeric") => GaugeStyle::Numeric,
            _ => default_style,
        };
        let mut markers = Vec::new();
        for n in 1..=10 {
            let Some(pos) =
                get(&format!("{name}.marker.{n}.pos")).and_then(|v| v.parse::<f32>().ok())
            else {
                break;
            };
            let image = get(&format!("{name}.marker.{n}.image"))
                .map(path)
                .unwrap_or_default();
            let label = get(&format!("{name}.marker.{n}.label"))
                .unwrap_or("")
                .to_string();
            if image.is_empty() && label.is_empty() {
                continue;
            }
            markers.push(Marker {
                pos: pos.clamp(0.0, 100.0),
                image,
                label,
                font_size: get(&format!("{name}.marker.{n}.fontsize")).and_then(|v| v.parse().ok()),
            });
        }
        Some(GaugeSpec {
            x,
            y,
            w,
            h,
            style,
            color: get(&format!("{name}.color"))
                .and_then(color_triplet)
                .unwrap_or(default_color),
            bg_color: get(&format!("{name}.bg.color"))
                .and_then(color_triplet)
                .or(default_bg),
            font_size: number(&format!("{name}.font.size"), 24.0).max(1.0) as u32,
            knob_image: get(&format!("{name}.knob.image"))
                .map(path)
                .unwrap_or_else(|| path("volume_knob.png")),
            knob_start: number(&format!("{name}.knob.angle.start"), 225.0),
            knob_end: number(&format!("{name}.knob.angle.end"), -45.0),
            arc_width: number(&format!("{name}.arc.width"), 6.0).max(1.0) as u32,
            arc_start: number(&format!("{name}.arc.angle.start"), 225.0),
            arc_end: number(&format!("{name}.arc.angle.end"), -45.0),
            track: get(&format!("{name}.slider.track"))
                .map(path)
                .unwrap_or_default(),
            tip: get(&format!("{name}.slider.tip"))
                .map(path)
                .unwrap_or_default(),
            orientation: get(&format!("{name}.slider.orientation"))
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_else(|| default_orientation.to_string()),
            travel: ipair(&format!("{name}.slider.travel")),
            tip_offset: ipair(&format!("{name}.slider.tip.offset")).unwrap_or((0, 0)),
            fill_color: get(&format!("{name}.fill.color")).and_then(color_triplet),
            fill_width: get(&format!("{name}.fill.width")).and_then(|v| v.parse().ok()),
            fill_offset: ipair(&format!("{name}.fill.offset")).unwrap_or((0, 0)),
            fill_radius: number(&format!("{name}.fill.radius"), 0.0).max(0.0) as u32,
            border: if name == "progress" {
                number("progress.border", 0.0).max(0.0) as u32
            } else {
                0
            },
            border_color: get("progress.border.color")
                .and_then(color_triplet)
                .unwrap_or([100, 100, 100]),
            markers,
            head_image: get(&format!("{name}.head.image"))
                .map(path)
                .unwrap_or_default(),
            head_offset: ipair(&format!("{name}.head.offset")).unwrap_or((0, 0)),
        })
    };
    // Buttons: `button.<name>.pos`, an `action`, and a `size` or an `image`.
    let mut names: Vec<String> = values
        .iter()
        .filter_map(|(k, _)| k.strip_prefix("button."))
        .filter_map(|rest| rest.split('.').next())
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
    let mut buttons = Vec::new();
    for name in names {
        let Some((x, y)) = ipair(&format!("button.{name}.pos")) else {
            continue;
        };
        let Some(action) = get(&format!("button.{name}.action")).and_then(ButtonAction::parse)
        else {
            continue;
        };
        let (w, h) = upair(&format!("button.{name}.size")).unwrap_or((0, 0));
        // `image = rest.png, active.png`: the second picture shows while
        // the button is active; three or more are one per state.
        let images: Vec<String> = get(&format!("button.{name}.image"))
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(path)
            .collect();
        let image = images.first().cloned().unwrap_or_default();
        let image_active = images.get(1).cloned().unwrap_or_default();
        buttons.push(ButtonSpec {
            name: name.clone(),
            x,
            y,
            w,
            h,
            image,
            image_active,
            images,
            action,
        });
    }
    let spec = IndicatorsSpec {
        buttons,
        interactive: truthy(get("interactive")),
        touch_margin: get("touch.margin")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(24),
        volume: gauge(
            "volume",
            GaugeStyle::Numeric,
            "vertical",
            None,
            [255, 255, 255],
        ),
        mute: state("mute", vec![[255, 0, 0], [64, 64, 64]], false),
        shuffle: state(
            "shuffle",
            vec![[64, 64, 64], [0, 200, 255], [200, 0, 200]],
            true,
        ),
        repeat: state(
            "repeat",
            vec![[64, 64, 64], [0, 255, 0], [255, 200, 0]],
            false,
        ),
        playstate: state(
            "playstate",
            vec![[64, 64, 64], [255, 200, 0], [0, 255, 0]],
            false,
        ),
        progress: gauge(
            "progress",
            GaugeStyle::Slider,
            "horizontal",
            Some([40, 40, 40]),
            [0, 200, 255],
        ),
    };
    (!spec.is_empty()).then_some(spec)
}

/// The player's start and stop animation: `start.animation` turns the
/// fade at the first frame on; `transition.type` (`fade` or `none`),
/// `transition.duration` in seconds, `transition.color` (`black` or `white`)
/// and `transition.opacity` (0 to 100) shape every fade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionSettings {
    pub at_start: bool,
    pub fade: bool,
    pub duration_s: f32,
    pub white: bool,
    pub opacity: f32,
}

impl Default for TransitionSettings {
    fn default() -> Self {
        Self {
            at_start: false,
            fade: true,
            duration_s: 0.5,
            white: false,
            opacity: 1.0,
        }
    }
}

pub fn transition_settings(text: &str) -> TransitionSettings {
    TransitionSettings {
        at_start: truthy(current_value(text, "start.animation").as_deref()),
        fade: current_value(text, "transition.type")
            .is_none_or(|v| !v.eq_ignore_ascii_case("none")),
        duration_s: current_value(text, "transition.duration")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.5)
            .max(0.0),
        white: current_value(text, "transition.color")
            .is_some_and(|v| v.eq_ignore_ascii_case("white")),
        opacity: current_value(text, "transition.opacity")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(100.0)
            .clamp(0.0, 100.0)
            / 100.0,
    }
}

/// What draws the player's window: chosen by the machine, or named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScreenDriver {
    /// X when a display is set, Wayland when one is set, else KMS/DRM: what
    /// SDL would pick by itself, said out loud.
    #[default]
    Auto,
    X11,
    Wayland,
    /// The screen itself, with no X server: SDL's KMS/DRM driver.
    KmsDrm,
}

impl ScreenDriver {
    /// `auto`, `x11`, `wayland`, `kmsdrm` (also `kms`, `drm`); anything else is auto.
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("x11") => Self::X11,
            Some("wayland") => Self::Wayland,
            Some("kmsdrm") | Some("kms") | Some("drm") => Self::KmsDrm,
            _ => Self::Auto,
        }
    }

    /// The name SDL knows the driver by; none for auto.
    pub fn sdl_name(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::X11 => Some("x11"),
            Self::Wayland => Some("wayland"),
            Self::KmsDrm => Some("kmsdrm"),
        }
    }

    pub fn as_str(self) -> &'static str {
        self.sdl_name().unwrap_or("auto")
    }
}

/// How far the picture is turned on the screen, clockwise, in quarter
/// turns; touch is turned the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Rotation {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    /// `0`, `90`, `180` or `270`; anything else is 0.
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(|v| v.trim()) {
            Some("90") => Self::R90,
            Some("180") => Self::R180,
            Some("270") => Self::R270,
            _ => Self::R0,
        }
    }

    pub fn degrees(self) -> u32 {
        match self {
            Self::R0 => 0,
            Self::R90 => 90,
            Self::R180 => 180,
            Self::R270 => 270,
        }
    }

    /// A quarter turn swaps the picture's width and height on the screen.
    pub fn quarter(self) -> bool {
        matches!(self, Self::R90 | Self::R270)
    }
}

/// How the player runs on the glass: whether a touch ends it, and where
/// the frame sits in the window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSettings {
    /// `exit.on.touch` or `stop.display.on.touch`: a touch or click ends the player.
    pub exit_on_touch: bool,
    /// `touch.interactive`: whether a theme's controls act.
    #[serde(default)]
    pub interactive: InteractiveMode,
    /// `position.type`: `center` (default) centres the frame, fitted or
    /// not; anything else puts its top left at `position.x`, `position.y`.
    /// `fit` is read as centred, and fits.
    pub centered: bool,
    pub x: i32,
    pub y: i32,
    /// `position.fit`: the frame scaled to the screen, its shape kept, put
    /// where the position says; `position.type = fit` says the same.
    #[serde(default)]
    pub fit: bool,
    /// `screen.driver`: what draws the window, `auto` unless said.
    #[serde(default)]
    pub driver: ScreenDriver,
    /// `screen.rotation`: the picture and touch turned by 0, 90, 180 or 270
    /// degrees clockwise. Read where no X server turns the screen, the
    /// KMS/DRM driver; 0 unless said.
    #[serde(default)]
    pub rotation: Rotation,
    /// `screen.pointer.shown`: the pointer drawn on the player's window,
    /// as the plugin resolved it from the screen's choice and what the
    /// player has; hidden unless said.
    #[serde(default)]
    pub pointer: bool,
    /// `touch.matrix`: six numbers `a,b,c,d,e,f` mapping a finger's share
    /// of the panel `(x, y)` to `(ax + by + c, dx + ey + f)` before it
    /// becomes a pixel, for a panel whose touch frame is not the
    /// picture's: swapped, flipped, offset or scaled. Identity unless said.
    #[serde(default = "identity_matrix")]
    pub touch_matrix: [f32; 6],
    /// How large a face draws its controls and its clock: 1 as designed,
    /// more for a hand at arm's length; `face.size` normal, large or car.
    pub face_scale: f32,
    /// A face's own settings: every `face.<name>` key, by its name without
    /// the prefix, as written. The display hands them to a face unread; the
    /// face owns their meaning.
    #[serde(default)]
    pub face: std::collections::BTreeMap<String, String>,
}

/// The matrix that changes nothing.
pub const IDENTITY_MATRIX: [f32; 6] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

fn identity_matrix() -> [f32; 6] {
    IDENTITY_MATRIX
}

/// `touch.matrix` as six numbers separated by commas or spaces; anything
/// else, or a matrix that maps everything to one point, is the identity.
pub fn parse_matrix(value: Option<&str>) -> [f32; 6] {
    let Some(text) = value else {
        return IDENTITY_MATRIX;
    };
    let numbers: Vec<f32> = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<f32>().ok())
        .collect();
    if numbers.len() != 6 || numbers.iter().any(|n| !n.is_finite()) {
        return IDENTITY_MATRIX;
    }
    let m = [
        numbers[0], numbers[1], numbers[2], numbers[3], numbers[4], numbers[5],
    ];
    if (m[0] * m[4] - m[1] * m[3]).abs() < 1e-6 {
        return IDENTITY_MATRIX;
    }
    m
}

/// How large a face draws, from `face.size`: normal as designed, large
/// for a hand at arm's length, car for a glance while driving; anything
/// else, or nothing, is normal.
/// A face's settings in a configuration: the `face.<name>` keys of its
/// current section, under their names without the prefix, the last of a
/// name winning as elsewhere. The plugin's writer puts a backslash before
/// a `#` or a `;` in a value, which would otherwise begin a comment; the
/// value is handed on as it was meant, a colour as `#rrggbb`.
pub fn face_settings(text: &str) -> std::collections::BTreeMap<String, String> {
    let mut found = std::collections::BTreeMap::new();
    let mut in_current = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_current = line.eq_ignore_ascii_case("[current]");
            continue;
        }
        if !in_current || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if let Some(name) = key.trim().strip_prefix("face.") {
            if !name.is_empty() {
                let value = value.trim().replace("\\#", "#").replace("\\;", ";");
                found.insert(name.to_string(), value);
            }
        }
    }
    found
}

pub fn face_scale(size: Option<&str>) -> f32 {
    match size.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("large") => 1.4,
        Some("car") => 2.0,
        _ => 1.0,
    }
}

/// A point through the matrix.
pub fn apply_matrix(m: [f32; 6], point: (f32, f32)) -> (f32, f32) {
    (
        m[0] * point.0 + m[1] * point.1 + m[2],
        m[3] * point.0 + m[4] * point.1 + m[5],
    )
}

/// How far a calibration's worst touch may miss its target, in window
/// pixels, for the map to be kept: a twentieth of the window's diagonal.
/// A finger's jitter sits well inside it; a touch that missed its target,
/// or a lift the calibration did not ask for, sits far outside.
pub fn calibration_tolerance(window: (u32, u32)) -> f32 {
    let (w, h) = (window.0 as f32, window.1 as f32);
    (w * w + h * h).sqrt() / 20.0
}

/// The worst miss of one touch against the map fitted from the others, in
/// share units. A touch that missed its target stands out in full here,
/// where the map fitted from all of them spreads the miss over every
/// sample and can hide it; none with fewer than four pairs, or when
/// leaving one out leaves the rest on one line.
pub fn calibration_miss(pairs: &[((f32, f32), (f32, f32))]) -> Option<f32> {
    if pairs.len() < 4 {
        return None;
    }
    let mut worst = 0f32;
    for (i, (raw, expected)) in pairs.iter().enumerate() {
        let others: Vec<_> = pairs
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, pair)| *pair)
            .collect();
        let (matrix, _) = fit_affine(&others)?;
        let got = apply_matrix(matrix, *raw);
        let miss = ((got.0 - expected.0).powi(2) + (got.1 - expected.1).powi(2)).sqrt();
        worst = worst.max(miss);
    }
    Some(worst)
}

/// The affine map that takes each raw share to its expected share with
/// the least squared error, from three or more pairs that do not lie on
/// one line, and the worst distance left over; none when the pairs do not
/// pin the map down.
pub fn fit_affine(pairs: &[((f32, f32), (f32, f32))]) -> Option<([f32; 6], f32)> {
    if pairs.len() < 3 {
        return None;
    }
    // Normal equations for x' = ax + by + c and y' = dx + ey + f, with the
    // same 3x3 on the left for both, in f64 so the sums do not lose bits.
    let mut ata = [[0f64; 3]; 3];
    let mut atx = [0f64; 3];
    let mut aty = [0f64; 3];
    for ((x, y), (ex, ey)) in pairs {
        let row = [*x as f64, *y as f64, 1.0];
        for i in 0..3 {
            for j in 0..3 {
                ata[i][j] += row[i] * row[j];
            }
            atx[i] += row[i] * *ex as f64;
            aty[i] += row[i] * *ey as f64;
        }
    }
    let abc = solve3(ata, atx)?;
    let def = solve3(ata, aty)?;
    let m = [
        abc[0] as f32,
        abc[1] as f32,
        abc[2] as f32,
        def[0] as f32,
        def[1] as f32,
        def[2] as f32,
    ];
    let worst = pairs
        .iter()
        .map(|(raw, expected)| {
            let got = apply_matrix(m, *raw);
            ((got.0 - expected.0).powi(2) + (got.1 - expected.1).powi(2)).sqrt()
        })
        .fold(0f32, f32::max);
    Some((m, worst))
}

/// A 3x3 system by Cramer's rule; none when its determinant is nothing,
/// the points all on one line.
fn solve3(a: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: [[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(a);
    if d.abs() < 1e-9 {
        return None;
    }
    let mut out = [0f64; 3];
    for (k, slot) in out.iter_mut().enumerate() {
        let mut m = a;
        for (i, row) in m.iter_mut().enumerate() {
            row[k] = b[i];
        }
        *slot = det(m) / d;
    }
    Some(out)
}

impl Default for RunSettings {
    fn default() -> Self {
        Self {
            exit_on_touch: false,
            interactive: InteractiveMode::Theme,
            centered: true,
            x: 0,
            y: 0,
            fit: false,
            driver: ScreenDriver::Auto,
            rotation: Rotation::R0,
            pointer: false,
            touch_matrix: IDENTITY_MATRIX,
            face_scale: 1.0,
            face: Default::default(),
        }
    }
}

pub fn run_settings(text: &str) -> RunSettings {
    let kind = current_value(text, "position.type").map(|v| v.trim().to_ascii_lowercase());
    let fit =
        truthy(current_value(text, "position.fit").as_deref()) || kind.as_deref() == Some("fit");
    RunSettings {
        exit_on_touch: truthy(current_value(text, "exit.on.touch").as_deref())
            || truthy(current_value(text, "stop.display.on.touch").as_deref()),
        interactive: InteractiveMode::parse(current_value(text, "touch.interactive").as_deref()),
        centered: kind.as_deref().is_none_or(|v| v == "center" || v == "fit"),
        fit,
        driver: ScreenDriver::parse(current_value(text, "screen.driver").as_deref()),
        rotation: Rotation::parse(current_value(text, "screen.rotation").as_deref()),
        pointer: truthy(current_value(text, "screen.pointer.shown").as_deref()),
        touch_matrix: parse_matrix(current_value(text, "touch.matrix").as_deref()),
        face_scale: face_scale(current_value(text, "face.size").as_deref()),
        face: face_settings(text),
        x: current_value(text, "position.x")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        y: current_value(text, "position.y")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    }
}

/// The player's run flag: the plugin removes it to stop the player, and the
/// player creates it while it runs.
pub const RUN_FLAG: &str = "/tmp/glass_running";

/// The marker the launcher names in this variable; written on a real touch
/// so the plugin re-arms its timeout instead of restarting at once.
pub const DISMISS_FILE_VAR: &str = "GLASS_DISMISS_FILE";

/// The plugin's channel: a local socket it serves with the player's state,
/// named in this variable by the launcher, else at the default path.
pub const CHANNEL_VAR: &str = "GLASS_CHANNEL";
pub const CHANNEL_PATH: &str = "/tmp/glass_channel";

/// Whether a touch should leave the dismiss marker: only when the launcher
/// asked for one, the stop is not the plugin's, and the run flag still stands.
pub fn should_mark_dismiss(
    marker_path: Option<&str>,
    external_stop: bool,
    run_flag_exists: bool,
) -> bool {
    if external_stop || !run_flag_exists {
        return false;
    }
    marker_path.is_some_and(|p| !p.is_empty())
}

/// `[data.source]` from the player configuration, with the engine's defaults.
pub fn data_source_from_config(text: &str) -> DataSourceSpec {
    let number = |key: &str, default: f32| {
        section_value(text, "data.source", key)
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(default)
    };
    let word = |key: &str, default: &str| {
        section_value(text, "data.source", key)
            .filter(|v| !v.is_empty())
            .map(|v| v.to_ascii_lowercase())
            .unwrap_or_else(|| default.to_string())
    };
    DataSourceSpec {
        max_ui: number("volume.max", DEFAULT_METER_MAX).max(1.0),
        max_pipe: number("volume.max.in.pipe", DEFAULT_METER_MAX).max(1.0),
        gain_db: number("volume.gain.db", 0.0),
        gain_source: section_value(text, "data.source", "volume.gain.db.source")
            .unwrap_or_default(),
        smooth: number("smooth.buffer.size", 0.0).max(0.0) as usize,
        stereo: word("stereo.algorithm", "new"),
        mono: word("mono.algorithm", "average"),
    }
}

/// Font files the theme text is set in. An empty string is no file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FontFiles {
    pub light: String,
    pub regular: String,
    pub bold: String,
    pub digi: String,
    #[serde(default)]
    pub italic: String,
    /// A multi-script face consulted glyph by glyph when a text's own face
    /// lacks a character; empty when the plugin ships none.
    #[serde(default)]
    pub fallback: String,
}

/// The text placements one meter declares.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeterTexts {
    pub title: Option<TextSpec>,
    pub artist: Option<TextSpec>,
    pub album: Option<TextSpec>,
    pub sample: Option<TextSpec>,
    pub time: Option<TextSpec>,
    pub time_elapsed: Option<TextSpec>,
    pub time_total: Option<TextSpec>,
    pub next_title: Option<TextSpec>,
    pub next_artist: Option<TextSpec>,
    pub next_album: Option<TextSpec>,
    pub ticker: Option<TickerSpec>,
    /// `volume.value.pos`: the volume as a number, beside its gauge.
    pub volume_value: Option<TextSpec>,
}

/// One snapshot of the outside world. `plot` turns it into a scene.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Input {
    pub levels: Levels,
    pub bins: Bins,
    pub metadata: Metadata,
}

/// Geometry the scene is plotted into. Pixels stay in `expose`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinDesc {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub meter_max: f32,
    pub spectrum_max: f32,
    /// Directory of the selected theme, empty when no config is loaded.
    pub theme_dir: String,
    /// Background file name inside `theme_dir`, from the theme's `meters.txt`.
    pub background: String,
    /// `left.x` and `left.y` from the selected meter, plus `meter.x/y`. Absent on a
    /// theme-less frame. A theme may put an origin off screen, so they are signed.
    pub left_at: Option<(i32, i32)>,
    /// `right.x` and `right.y` from the selected meter.
    pub right_at: Option<(i32, i32)>,
    /// `indicator.filename` from the selected meter.
    pub indicator: String,
    pub face: String,
    pub front: String,
    pub face_at: (u32, u32),
    /// Circular needle: start angle, stop angle, distance from origin to sprite center.
    pub needle: Option<(f32, f32, f32)>,
    pub title_at: Option<(u32, u32)>,
    pub artist_at: Option<(u32, u32)>,
    /// Theme fonts from the player configuration.
    #[serde(default)]
    pub fonts: FontFiles,
    #[serde(default)]
    pub title: Option<TextSpec>,
    #[serde(default)]
    pub artist: Option<TextSpec>,
    /// Present only when the meter places the album on its own line.
    #[serde(default)]
    pub album: Option<TextSpec>,
    #[serde(default)]
    pub sample: Option<TextSpec>,
    /// The volume as a number, beside its gauge.
    #[serde(default)]
    pub volume_value: Option<TextSpec>,
    #[serde(default)]
    pub time: Option<TextSpec>,
    #[serde(default)]
    pub art: Option<ArtSpec>,
    #[serde(default)]
    pub type_area: Option<TypeSpec>,
    /// `format-icons` inside the theme folder, searched first. Empty when none.
    #[serde(default)]
    pub skin_icons: String,
    /// `format-icons` beside the player's handlers, searched second.
    #[serde(default)]
    pub plugin_icons: String,
    #[serde(default)]
    pub next_title: Option<TextSpec>,
    #[serde(default)]
    pub next_artist: Option<TextSpec>,
    #[serde(default)]
    pub next_album: Option<TextSpec>,
    #[serde(default)]
    pub ticker: Option<TickerSpec>,
    #[serde(default)]
    pub time_elapsed: Option<TextSpec>,
    #[serde(default)]
    pub time_total: Option<TextSpec>,
    #[serde(default)]
    pub data_source: DataSourceSpec,
    #[serde(default)]
    pub meter: MeterSpec,
    /// The spectrum boxes the meter shows, in the order `spectrum.name`
    /// lists them: one, or one per channel.
    #[serde(default)]
    pub spectra: Vec<SpectrumSpec>,
    #[serde(default)]
    pub folder_layers: Vec<FolderLayerSpec>,
    #[serde(default)]
    pub fanart: Option<FanartSpec>,
    #[serde(default)]
    pub vinyl: Option<VinylSpec>,
    #[serde(default)]
    pub tonearm: Option<TonearmSpec>,
    #[serde(default)]
    pub rotation: RotationSettings,
    #[serde(default)]
    pub reels: Option<ReelsSpec>,
    #[serde(default)]
    pub indicators: Option<IndicatorsSpec>,
    #[serde(default)]
    pub transition: TransitionSettings,
    #[serde(default)]
    pub run: RunSettings,
}

impl Default for SkinDesc {
    fn default() -> Self {
        Self::basic()
    }
}

impl SkinDesc {
    pub fn basic() -> Self {
        Self {
            name: "basic".into(),
            width: 800,
            height: 480,
            meter_max: DEFAULT_METER_MAX,
            spectrum_max: DEFAULT_SPECTRUM_MAX,
            theme_dir: String::new(),
            background: String::new(),
            left_at: None,
            right_at: None,
            indicator: String::new(),
            face: String::new(),
            front: String::new(),
            face_at: (0, 0),
            needle: None,
            title_at: None,
            artist_at: None,
            fonts: FontFiles::default(),
            title: None,
            artist: None,
            album: None,
            sample: None,
            volume_value: None,
            time: None,
            art: None,
            type_area: None,
            skin_icons: String::new(),
            plugin_icons: String::new(),
            next_title: None,
            next_artist: None,
            next_album: None,
            ticker: None,
            time_elapsed: None,
            time_total: None,
            data_source: DataSourceSpec::default(),
            meter: MeterSpec::default(),
            spectra: Vec::new(),
            folder_layers: Vec::new(),
            fanart: None,
            vinyl: None,
            tonearm: None,
            rotation: RotationSettings::default(),
            reels: None,
            indicators: None,
            transition: TransitionSettings::default(),
            run: RunSettings::default(),
        }
    }
}

/// Last complete meter record: little-endian `u16` left, then right.
///
/// The scope packs the same bits as `left + (right << 16)`.
pub fn decode_meter(record: &[u8]) -> Option<(u16, u16)> {
    if record.len() != 4 {
        return None;
    }
    let left = u16::from_le_bytes([record[0], record[1]]);
    let right = u16::from_le_bytes([record[2], record[3]]);
    Some((left, right))
}

/// One spectrum frame: `bin_count` little-endian `i32` values.
pub fn decode_spectrum(record: &[u8], bin_count: usize) -> Option<Vec<f32>> {
    if bin_count == 0 || record.len() != bin_count * 4 {
        return None;
    }
    let mut values = Vec::with_capacity(bin_count);
    for chunk in record.as_chunks::<4>().0 {
        let raw = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        values.push(raw.max(0) as f32);
    }
    Some(values)
}

/// Map a scope reading onto the UI scale. Values above the pipe max clamp.
pub fn scale_level(raw: u16, max_pipe: f32, max_ui: f32) -> f32 {
    if max_pipe <= 0.0 || max_ui <= 0.0 {
        return 0.0;
    }
    (raw as f32 / max_pipe * max_ui).clamp(0.0, max_ui)
}

/// Mono is the average of the two channels.
pub fn mono_average(left: f32, right: f32) -> f32 {
    (left + right) / 2.0
}

/// Read `frame.rate` from `[current]`. Missing or invalid text is 30.
/// Values are clamped to the UI range, 10 through 60.
pub fn frame_rate_from_config(text: &str) -> u32 {
    let mut in_current = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_current = line.eq_ignore_ascii_case("[current]");
            continue;
        }
        if !in_current || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "frame.rate" {
            continue;
        }
        return value
            .trim()
            .parse::<u32>()
            .unwrap_or(DEFAULT_FRAME_RATE)
            .clamp(MIN_FRAME_RATE, MAX_FRAME_RATE);
    }
    DEFAULT_FRAME_RATE
}

/// Screen size from `[current]`. `meter.folder` names it as `480x320` or
/// `480x320-text`. `screen.width` and `screen.height` override that pair
/// when both are set. Missing text is 800×480.
pub fn screen_from_config(text: &str) -> (u32, u32) {
    let mut folder = String::new();
    let mut width = None;
    let mut height = None;
    let mut in_current = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_current = line.eq_ignore_ascii_case("[current]");
            continue;
        }
        if !in_current || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "meter.folder" => folder = value.trim().to_string(),
            "screen.width" => width = value.trim().parse::<u32>().ok().filter(|n| *n > 0),
            "screen.height" => height = value.trim().parse::<u32>().ok().filter(|n| *n > 0),
            _ => {}
        }
    }
    // The folder may be a path, as an override gives one: its last part is the name.
    let folder_name = Path::new(&folder)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&folder);
    let (folder_w, folder_h) = size_from_folder(folder_name).unwrap_or((800, 480));
    match (width, height) {
        (Some(w), Some(h)) => (w, h),
        _ => (folder_w, folder_h),
    }
}

fn size_from_folder(name: &str) -> Option<(u32, u32)> {
    let (width, rest) = name.split_once('x')?;
    let width: u32 = width.parse().ok()?;
    let height: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let height: u32 = height.parse().ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    Some((width, height))
}

/// Value of one key in `[current]`.
pub fn current_value(text: &str, wanted: &str) -> Option<String> {
    let mut in_current = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_current = line.eq_ignore_ascii_case("[current]");
            continue;
        }
        if !in_current || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == wanted {
            return Some(value.trim().to_string());
        }
    }
    None
}

/// Background file for the meter named in config. `random` and `list` use the
/// first meter in the file. `screen.bgr` wins when that meter sets it.
pub fn meter_background(meters_txt: &str, meter: &str) -> Option<String> {
    let mut sections: Vec<(String, String, String)> = Vec::new();
    let mut name = String::new();
    let mut bgr = String::new();
    let mut screen = String::new();
    let mut in_section = false;
    let flush = |sections: &mut Vec<(String, String, String)>,
                 name: &mut String,
                 bgr: &mut String,
                 screen: &mut String,
                 in_section: &mut bool| {
        if *in_section && !name.is_empty() {
            sections.push((
                std::mem::take(name),
                std::mem::take(bgr),
                std::mem::take(screen),
            ));
        }
        *in_section = false;
    };
    for line in meters_txt.lines() {
        let line = line.trim();
        if let Some(title) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            flush(
                &mut sections,
                &mut name,
                &mut bgr,
                &mut screen,
                &mut in_section,
            );
            name = title.trim().to_string();
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "bgr.filename" => bgr = value.trim().to_string(),
            "screen.bgr" => screen = value.trim().to_string(),
            _ => {}
        }
    }
    flush(
        &mut sections,
        &mut name,
        &mut bgr,
        &mut screen,
        &mut in_section,
    );
    let named = meter != "random" && meter != "list" && !meter.is_empty();
    let section = if named {
        sections.iter().find(|(n, _, _)| n == meter)
    } else {
        None
    };
    let (_, bgr, screen) = section.or_else(|| sections.first())?;
    if !screen.is_empty() {
        return Some(screen.clone());
    }
    if !bgr.is_empty() {
        return Some(bgr.clone());
    }
    None
}

/// Channel origins from the selected meter. `random` and `list` use the first meter.
pub fn meter_at(meters_txt: &str, meter: &str) -> (Option<(i32, i32)>, Option<(i32, i32)>) {
    let mut sections: Vec<(String, Option<(i32, i32)>, Option<(i32, i32)>)> = Vec::new();
    let mut name = String::new();
    let mut left_x = None;
    let mut left_y = None;
    let mut right_x = None;
    let mut right_y = None;
    let mut meter_x = 0i32;
    let mut meter_y = 0i32;
    let mut in_section = false;
    let flush = |sections: &mut Vec<(String, Option<(i32, i32)>, Option<(i32, i32)>)>,
                 name: &mut String,
                 left_x: &mut Option<i32>,
                 left_y: &mut Option<i32>,
                 right_x: &mut Option<i32>,
                 right_y: &mut Option<i32>,
                 meter_x: &mut i32,
                 meter_y: &mut i32,
                 in_section: &mut bool| {
        if *in_section && !name.is_empty() {
            let left = match (*left_x, *left_y) {
                (Some(x), Some(y)) => Some((x + *meter_x, y + *meter_y)),
                _ => None,
            };
            let right = match (*right_x, *right_y) {
                (Some(x), Some(y)) => Some((x + *meter_x, y + *meter_y)),
                _ => None,
            };
            sections.push((std::mem::take(name), left, right));
        }
        *left_x = None;
        *left_y = None;
        *right_x = None;
        *right_y = None;
        *meter_x = 0;
        *meter_y = 0;
        *in_section = false;
    };
    for line in meters_txt.lines() {
        let line = line.trim();
        if let Some(title) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            flush(
                &mut sections,
                &mut name,
                &mut left_x,
                &mut left_y,
                &mut right_x,
                &mut right_y,
                &mut meter_x,
                &mut meter_y,
                &mut in_section,
            );
            name = title.trim().to_string();
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let parsed = value.trim().parse::<i32>().ok();
        match key.trim() {
            "left.x" | "left.origin.x" => left_x = parsed,
            "left.y" | "left.origin.y" => left_y = parsed,
            "right.x" | "right.origin.x" => right_x = parsed,
            "right.y" | "right.origin.y" => right_y = parsed,
            "meter.x" => meter_x = parsed.unwrap_or(0),
            "meter.y" => meter_y = parsed.unwrap_or(0),
            _ => {}
        }
    }
    flush(
        &mut sections,
        &mut name,
        &mut left_x,
        &mut left_y,
        &mut right_x,
        &mut right_y,
        &mut meter_x,
        &mut meter_y,
        &mut in_section,
    );
    let named = meter != "random" && meter != "list" && !meter.is_empty();
    let section = if named {
        sections.iter().find(|(n, _, _)| n == meter)
    } else {
        None
    };
    section
        .or_else(|| sections.first())
        .map(|(_, left, right)| (*left, *right))
        .unwrap_or((None, None))
}

/// `indicator.filename` for the selected meter. `random` and `list` use the first meter.
pub fn meter_indicator(meters_txt: &str, meter: &str) -> Option<String> {
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut name = String::new();
    let mut indicator = String::new();
    let mut in_section = false;
    for line in meters_txt.lines() {
        let line = line.trim();
        if let Some(title) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if in_section && !name.is_empty() {
                sections.push((std::mem::take(&mut name), std::mem::take(&mut indicator)));
            }
            name = title.trim().to_string();
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "indicator.filename" {
            indicator = value.trim().to_string();
        }
    }
    if in_section && !name.is_empty() {
        sections.push((name, indicator));
    }
    let named = meter != "random" && meter != "list" && !meter.is_empty();
    let section = if named {
        sections.iter().find(|(n, _)| n == meter)
    } else {
        None
    };
    section
        .or_else(|| sections.first())
        .map(|(_, file)| file.clone())
        .filter(|file| !file.is_empty())
}

/// Screen picture, meter face, and meter foreground for the selected meter.
pub fn meter_layers(meters_txt: &str, meter: &str) -> (String, String, String, (u32, u32)) {
    let mut found_screen = String::new();
    let mut found_face = String::new();
    let mut found_front = String::new();
    let mut found_at = (0u32, 0u32);
    let named = meter != "random" && meter != "list" && !meter.is_empty();
    let mut take = !named;
    for line in meters_txt.lines() {
        let line = line.trim();
        if let Some(title) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if take && (!found_face.is_empty() || !found_screen.is_empty()) {
                return (found_screen, found_face, found_front, found_at);
            }
            take = !named || title.trim() == meter;
            if take {
                found_screen.clear();
                found_face.clear();
                found_front.clear();
                found_at = (0, 0);
            }
            continue;
        }
        if !take {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "screen.bgr" => found_screen = value.to_string(),
            "bgr.filename" => found_face = value.to_string(),
            "fgr.filename" => found_front = value.to_string(),
            "meter.x" => found_at.0 = value.parse().unwrap_or(0),
            "meter.y" => found_at.1 = value.parse().unwrap_or(0),
            _ => {}
        }
    }
    (found_screen, found_face, found_front, found_at)
}

/// `start.angle` and `stop.angle` for the selected meter.
pub fn meter_needle(meters_txt: &str, meter: &str) -> Option<(f32, f32, f32)> {
    let mut found_start = None;
    let mut found_stop = None;
    let mut left_start = None;
    let mut left_stop = None;
    let mut found_distance = 0.0;
    let named = meter != "random" && meter != "list" && !meter.is_empty();
    let mut take = !named;
    for line in meters_txt.lines() {
        let line = line.trim();
        if let Some(title) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if let (true, Some(a), Some(b)) =
                (take, found_start.or(left_start), found_stop.or(left_stop))
            {
                return Some((a, b, found_distance));
            }
            take = !named || title.trim() == meter;
            if take {
                found_start = None;
                found_stop = None;
                left_start = None;
                left_stop = None;
                found_distance = 0.0;
            }
            continue;
        }
        if !take {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let parsed = value.trim().parse::<f32>().ok();
        match key.trim() {
            "start.angle" => found_start = parsed,
            "stop.angle" => found_stop = parsed,
            // A meter with per-channel angles only: the left pair stands in
            // for the shared one, as the meter engine reads it.
            "left.start.angle" => left_start = parsed,
            "left.stop.angle" => left_stop = parsed,
            "distance" => found_distance = parsed.unwrap_or(0.0),
            _ => {}
        }
    }
    match (found_start.or(left_start), found_stop.or(left_stop)) {
        (Some(a), Some(b)) => Some((a, b, found_distance)),
        _ => None,
    }
}

fn pair_pos(value: &str) -> Option<(u32, u32)> {
    let mut parts = value.split(',');
    let x = parts.next()?.trim().parse().ok()?;
    let y = parts.next()?.trim().parse().ok()?;
    Some((x, y))
}

/// `playinfo.title.pos` and `playinfo.artist.pos` as x,y. The style word is ignored.
pub fn meter_text_at(meters_txt: &str, meter: &str) -> (Option<(u32, u32)>, Option<(u32, u32)>) {
    let mut title = None;
    let mut artist = None;
    let named = meter != "random" && meter != "list" && !meter.is_empty();
    let mut take = !named;
    for line in meters_txt.lines() {
        let line = line.trim();
        if let Some(title_name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if take && (title.is_some() || artist.is_some()) && named {
                break;
            }
            take = !named || title_name.trim() == meter;
            if take && named {
                title = None;
                artist = None;
            }
            continue;
        }
        if !take {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "playinfo.title.pos" => title = pair_pos(value),
            "playinfo.artist.pos" => artist = pair_pos(value),
            _ => {}
        }
    }
    (title, artist)
}

/// Font files from `[current]`. Each style's `font.<style>` value is one
/// of: `builtin` (or nothing), the plugin's multi-script PeppyFont face of
/// that style; an absolute path to a file, a font the listener uploaded;
/// or a file name under `font.path`, the player's own fonts. An older
/// configuration's `use.system.fonts = False` sets every style to the
/// built-in face, as it always meant. `font.digi` is the clock font,
/// DSEG7 unless a file is named. The fallback is the built-in regular face.
pub fn fonts_from_config(text: &str, plugin_fonts: &Path) -> FontFiles {
    let base = current_value(text, "font.path").unwrap_or_default();
    let base = base.trim().trim_end_matches('/').to_string();
    let shipped = |name: &str| -> String {
        let path = plugin_fonts.join(name);
        if is_file(&path) {
            path.to_string_lossy().into_owned()
        } else {
            String::new()
        }
    };
    let force_builtin = current_value(text, "use.system.fonts")
        .is_some_and(|v| !v.trim().eq_ignore_ascii_case("true"));
    let style = |key: &str, face: &str| -> String {
        let value = current_value(text, key).unwrap_or_default();
        let value = value.trim();
        let builtin = value.is_empty() || value.eq_ignore_ascii_case("builtin");
        if builtin || force_builtin && !is_file(Path::new(value)) {
            return shipped(face);
        }
        if value.starts_with('/') && is_file(Path::new(value)) {
            return value.to_string();
        }
        if base.is_empty() {
            value.to_string()
        } else {
            format!("{base}/{}", value.trim_start_matches('/'))
        }
    };
    let digi = current_value(text, "font.digi").unwrap_or_default();
    let digi = digi.trim();
    let digi = if digi.is_empty() || digi.eq_ignore_ascii_case("builtin") {
        plugin_fonts
            .join("DSEG7Classic-Italic.ttf")
            .to_string_lossy()
            .into_owned()
    } else {
        digi.to_string()
    };
    FontFiles {
        light: style("font.light", "PeppyFont-Light.ttf"),
        regular: style("font.regular", "PeppyFont-Regular.ttf"),
        bold: style("font.bold", "PeppyFont-Bold.ttf"),
        digi,
        italic: style("font.italic", "PeppyFont-Italic.ttf"),
        fallback: shipped("PeppyFont-Regular.ttf"),
    }
}

/// Key and value pairs of the selected meter section. `random` and `list`
/// use the first section in the file.
fn section_values(meters_txt: &str, meter: &str) -> Vec<(String, String)> {
    let named = meter != "random" && meter != "list" && !meter.is_empty();
    let mut take = false;
    let mut seen_any = false;
    let mut values = Vec::new();
    for line in meters_txt.lines() {
        let line = line.trim();
        if let Some(title) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if take {
                break;
            }
            take = if named {
                title.trim() == meter
            } else {
                !seen_any
            };
            seen_any = true;
            continue;
        }
        if !take || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            values.push((key.trim().to_string(), value.trim().to_string()));
        }
    }
    values
}

fn color_triplet(value: &str) -> Option<[u8; 3]> {
    let mut parts = value.split(',').map(|p| p.trim().parse::<u8>().ok());
    Some([parts.next()??, parts.next()??, parts.next()??])
}

fn style_word(word: &str) -> Option<TextStyle> {
    match word.trim().to_ascii_lowercase().as_str() {
        "light" => Some(TextStyle::Light),
        "regular" => Some(TextStyle::Regular),
        "bold" => Some(TextStyle::Bold),
        "italic" => Some(TextStyle::Italic),
        "digi" => Some(TextStyle::Digi),
        _ => None,
    }
}

fn truthy(value: Option<&str>) -> bool {
    matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("true") | Some("1") | Some("yes") | Some("on")
    )
}

/// Text placements for the selected meter. A position is `x,y` or
/// `x,y,style`. Missing sizes and colours fall back to `font.size.*` and
/// `font.color`, with the player's defaults when those are absent too.
///
/// Every title, artist, album and next line has a box: its own `maxwidth`,
/// else `playinfo.maxwidth`, else the width left of it on a `screen_w` wide
/// screen minus a 20 pixel margin, or six tenths of the screen when the
/// lines are centred. `playinfo.align` places a fitting text; the legacy
/// `playinfo.center = True` means centred. Scrolling speed follows
/// `speeds.mode`: `default` is 40 everywhere, `custom` takes the player's
/// values, anything else the meter's `playinfo.scrolling.speed.*`, then its
/// `playinfo.scrolling.speed`, then 40.
pub fn meter_texts(
    meters_txt: &str,
    meter: &str,
    screen_w: u32,
    speeds: &ScrollSpeeds,
) -> MeterTexts {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let number = |key: &str, default: u32| get(key).and_then(|v| v.parse().ok()).unwrap_or(default);
    let font_color = get("font.color")
        .and_then(color_triplet)
        .unwrap_or([255, 255, 255]);
    let global_max = number("playinfo.maxwidth", 0);
    let align = match get("playinfo.align")
        .map(|w| w.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("center") => TextAlign::Center,
        Some("right") => TextAlign::Right,
        Some("left") => TextAlign::Left,
        _ => {
            if truthy(get("playinfo.center")) || truthy(get("playinfo.text.center")) {
                TextAlign::Center
            } else {
                TextAlign::Left
            }
        }
    };
    let size_of = |style: TextStyle| match style {
        TextStyle::Light => number("font.size.light", 30),
        TextStyle::Regular => number("font.size.regular", 35),
        TextStyle::Bold => number("font.size.bold", 40),
        TextStyle::Italic => number("font.size.italic", number("font.size.regular", 35)),
        TextStyle::Digi => number("font.size.digi", 40),
    };
    let skin_speed = |field: &str| -> f32 {
        get(&format!("playinfo.scrolling.speed.{field}"))
            .or_else(|| get("playinfo.scrolling.speed"))
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|n| *n > 0.0)
            .unwrap_or(40.0)
    };
    let speed_for = |field: &str| -> f32 {
        match speeds.mode.as_str() {
            "default" => 40.0,
            "custom" => match field {
                "title" => speeds.title,
                "artist" => speeds.artist,
                _ => speeds.album,
            },
            _ => skin_speed(field),
        }
    };
    let box_for = |x: u32, own: u32| -> u32 {
        if own > 0 {
            own
        } else if global_max > 0 {
            global_max
        } else if align == TextAlign::Center {
            (screen_w as f32 * 0.6) as u32
        } else {
            screen_w.saturating_sub(x).saturating_sub(20)
        }
    };
    let spec =
        |pos_key: &str, color_key: &str, default_style: TextStyle, boxed: Option<(&str, &str)>| {
            let pos = get(pos_key)?;
            let mut parts = pos.split(',');
            let x = parts.next()?.trim().parse().ok()?;
            let y = parts.next()?.trim().parse().ok()?;
            let style = parts.next().and_then(style_word).unwrap_or(default_style);
            let (max_width, speed, align) = match boxed {
                Some((max_key, field)) => (box_for(x, number(max_key, 0)), speed_for(field), align),
                None => (0, 0.0, TextAlign::Left),
            };
            Some(TextSpec {
                x,
                y,
                style,
                size: size_of(style),
                color: get(color_key).and_then(color_triplet).unwrap_or(font_color),
                max_width,
                align,
                speed,
                font_file: String::new(),
            })
        };
    // Time fields: none or `digi` picks the clock font, which `time.*.font`
    // and `time.*.fontsize` may replace per field; `light` and `bold` pick
    // those text fonts, any other word the regular one.
    let time_field = |field: &str, fallback_color: [u8; 3]| -> Option<TextSpec> {
        let pos = get(&format!("time.{field}.pos"))?;
        let mut parts = pos.split(',');
        let x = parts.next()?.trim().parse().ok()?;
        let y = parts.next()?.trim().parse().ok()?;
        let style = match parts.next().map(|w| w.trim().to_ascii_lowercase()) {
            None => TextStyle::Digi,
            Some(word) if word.is_empty() || word == "digi" => TextStyle::Digi,
            Some(word) if word == "light" => TextStyle::Light,
            Some(word) if word == "bold" => TextStyle::Bold,
            Some(_) => TextStyle::Regular,
        };
        let (size, font_file) = if style == TextStyle::Digi {
            (
                number(&format!("time.{field}.fontsize"), size_of(TextStyle::Digi)),
                get(&format!("time.{field}.font"))
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            )
        } else {
            (size_of(style), String::new())
        };
        Some(TextSpec {
            x,
            y,
            style,
            size,
            color: get(&format!("time.{field}.color"))
                .and_then(color_triplet)
                .unwrap_or(fallback_color),
            max_width: 0,
            align: TextAlign::Left,
            speed: 0.0,
            font_file,
        })
    };
    // A number placed by the theme: the volume beside its gauge. `x,y[,style]`,
    // its own size, colour and width, centred as the meter's texts are.
    let value_field = |prefix: &str| -> Option<TextSpec> {
        let pos = get(&format!("{prefix}.pos"))?;
        let mut parts = pos.split(',');
        let x = parts.next()?.trim().parse().ok()?;
        let y = parts.next()?.trim().parse().ok()?;
        let style = match parts.next().map(|w| w.trim().to_ascii_lowercase()) {
            Some(word) if word == "bold" => TextStyle::Bold,
            Some(word) if word == "regular" => TextStyle::Regular,
            Some(word) if word == "digi" => TextStyle::Digi,
            _ => TextStyle::Light,
        };
        Some(TextSpec {
            x,
            y,
            style,
            size: number(&format!("{prefix}.fontsize"), size_of(style)),
            color: get(&format!("{prefix}.color"))
                .and_then(color_triplet)
                .unwrap_or(font_color),
            max_width: number(&format!("{prefix}.maxwidth"), 0),
            align,
            speed: 0.0,
            font_file: get(&format!("{prefix}.font"))
                .unwrap_or("")
                .trim()
                .to_string(),
        })
    };
    let volume_value = value_field("volume.value");
    let time = time_field("remaining", font_color);
    let time_color = time.as_ref().map(|t| t.color).unwrap_or(font_color);
    let time_elapsed = time_field("elapsed", time_color);
    let time_total = time_field("total", time_color);
    // The samplerate line takes the type colour before the font colour.
    let type_color = get("playinfo.type.color")
        .and_then(color_triplet)
        .unwrap_or(font_color);
    let mut sample = spec(
        "playinfo.samplerate.pos",
        "playinfo.samplerate.color",
        TextStyle::Light,
        None,
    );
    if let Some(s) = sample.as_mut() {
        if get("playinfo.samplerate.color")
            .and_then(color_triplet)
            .is_none()
        {
            s.color = type_color;
        }
        s.max_width = number("playinfo.samplerate.maxwidth", 0);
    }
    let title = spec(
        "playinfo.title.pos",
        "playinfo.title.color",
        TextStyle::Bold,
        Some(("playinfo.title.maxwidth", "title")),
    );
    let ticker = if truthy(get("playinfo.ticker")) {
        spec(
            "playinfo.ticker.pos",
            "playinfo.ticker.color",
            TextStyle::Regular,
            Some(("playinfo.ticker.maxwidth", "ticker")),
        )
        .map(|mut text| {
            if get("playinfo.ticker.color")
                .and_then(color_triplet)
                .is_none()
            {
                text.color = title.as_ref().map(|t| t.color).unwrap_or(font_color);
            }
            // Always inside the visible width, whatever the box says.
            let visible = screen_w.saturating_sub(text.x);
            text.max_width = if number("playinfo.ticker.maxwidth", 0) > 0 {
                text.max_width.min(visible)
            } else {
                visible
            };
            text.speed = get("playinfo.ticker.speed")
                .and_then(|v| v.trim().parse::<f32>().ok())
                .filter(|n| *n > 0.0)
                .unwrap_or_else(|| skin_speed("ticker"));
            text.align = TextAlign::Left;
            TickerSpec {
                text,
                direction: match get("playinfo.ticker.direction")
                    .map(|w| w.trim().to_ascii_lowercase())
                    .as_deref()
                {
                    Some("ltr") => ScrollDirection::Ltr,
                    _ => ScrollDirection::Rtl,
                },
                separator: get("playinfo.ticker.separator")
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| " · ".to_string()),
                space_between: number("playinfo.ticker.space_between", 0),
                end_spaces: number("playinfo.ticker.end_spaces", 8),
                append_next: truthy(get("playinfo.ticker.append_next")),
                replace: truthy(get("playinfo.ticker.replace")),
            }
        })
    } else {
        None
    };
    MeterTexts {
        title,
        artist: spec(
            "playinfo.artist.pos",
            "playinfo.artist.color",
            TextStyle::Light,
            Some(("playinfo.artist.maxwidth", "artist")),
        ),
        album: spec(
            "playinfo.album.pos",
            "playinfo.album.color",
            TextStyle::Light,
            Some(("playinfo.album.maxwidth", "album")),
        ),
        sample,
        volume_value,
        time,
        time_elapsed,
        time_total,
        next_title: spec(
            "playinfo.next.title.pos",
            "playinfo.next.title.color",
            TextStyle::Regular,
            Some(("playinfo.next.title.maxwidth", "title")),
        ),
        next_artist: spec(
            "playinfo.next.artist.pos",
            "playinfo.next.artist.color",
            TextStyle::Regular,
            Some(("playinfo.next.artist.maxwidth", "artist")),
        ),
        next_album: spec(
            "playinfo.next.album.pos",
            "playinfo.next.album.color",
            TextStyle::Regular,
            Some(("playinfo.next.album.maxwidth", "album")),
        ),
        ticker,
    }
}

/// The type area for the selected meter. `default_mode` is the player's
/// `playinfo.type.mode` from `[current]`, used when the meter sets none;
/// `theme_dir` resolves `albumart.mask` and the skin icons.
pub fn meter_type(meters_txt: &str, meter: &str, default_mode: Option<&str>) -> Option<TypeSpec> {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let (x, y) = pair_pos(get("playinfo.type.pos")?)?;
    let mode_word =
        |word: Option<&str>| match word.map(|w| w.trim().to_ascii_lowercase()).as_deref() {
            Some("icon") => Some(TypeMode::Icon),
            Some("text") => Some(TypeMode::Text),
            Some("both") => Some(TypeMode::Both),
            _ => None,
        };
    let mode = mode_word(get("playinfo.type.mode"))
        .or_else(|| mode_word(default_mode))
        .unwrap_or(TypeMode::Icon);
    let box_size = get("playinfo.type.dimension")
        .and_then(pair_pos)
        .filter(|&(w, h)| w > 0 && h > 0 && (w, h) != (1, 1));
    if box_size.is_none() && mode != TypeMode::Text {
        return None;
    }
    let align = match get("playinfo.type.align")
        .map(|w| w.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("left") => TypeAlign::Left,
        Some("right") => TypeAlign::Right,
        _ => TypeAlign::Center,
    };
    let font_color = get("font.color")
        .and_then(color_triplet)
        .unwrap_or([255, 255, 255]);
    let color = get("playinfo.type.color")
        .and_then(color_triplet)
        .unwrap_or(font_color);
    let sample_style = get("playinfo.samplerate.pos")
        .and_then(|pos| pos.split(',').nth(2))
        .and_then(style_word)
        .unwrap_or(TextStyle::Light);
    let number = |key: &str, default: u32| get(key).and_then(|v| v.parse().ok()).unwrap_or(default);
    let sample_size = match sample_style {
        TextStyle::Light => number("font.size.light", 30),
        TextStyle::Regular => number("font.size.regular", 35),
        TextStyle::Bold => number("font.size.bold", 40),
        TextStyle::Italic => number("font.size.italic", number("font.size.regular", 35)),
        TextStyle::Digi => number("font.size.digi", 40),
    };
    let font_size = get("playinfo.type.fontsize")
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|n| *n > 0)
        .unwrap_or_else(|| type_font_size(sample_size, box_size.map(|(_, h)| h)));
    let label = match get("playinfo.type.label")
        .map(|w| w.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("samplerate") | Some("sample") | Some("rate") => TypeLabel::SampleRate,
        _ => TypeLabel::Format,
    };
    Some(TypeSpec {
        x,
        y,
        box_size,
        mode,
        align,
        color,
        font_size,
        font_style: sample_style,
        label,
    })
}

/// Album art box for the selected meter: `albumart.pos` as `x,y` and
/// `albumart.dimension` as `w,h`. Both must be present. `albumart.mask` is
/// a file in `theme_dir`; `albumart.border` is a width in pixels drawn in
/// `font.color`.
pub fn meter_art(meters_txt: &str, meter: &str, theme_dir: &str) -> Option<ArtSpec> {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let (x, y) = pair_pos(get("albumart.pos")?)?;
    let (w, h) = pair_pos(get("albumart.dimension")?)?;
    if w == 0 || h == 0 {
        return None;
    }
    let mask = match get("albumart.mask").map(str::trim) {
        Some(file) if !file.is_empty() && !theme_dir.is_empty() => {
            format!("{}/{}", theme_dir.trim_end_matches('/'), file)
        }
        Some(file) if !file.is_empty() => file.to_string(),
        _ => String::new(),
    };
    let border = get("albumart.border")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    let border_color = get("font.color")
        .and_then(color_triplet)
        .unwrap_or([255, 255, 255]);
    Some(ArtSpec {
        x,
        y,
        w,
        h,
        mask,
        border,
        border_color,
        rotation: truthy(get("albumart.rotation")),
        rpm: get("albumart.rotation.speed")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(0.0),
    })
}

/// How turning pictures are paced: `rotation.quality` picks frames per
/// second and the degree step of the engine's prepared frames (`low` 4 and 12,
/// `medium` 8 and 6, `high` 15 and 3, `custom` takes `rotation.fps` with a
/// step of `45 / fps` from 1 to 12), and only `custom` applies `rotation.speed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RotationSettings {
    pub fps: u32,
    pub step: u32,
    pub speed: f32,
    /// `reel.direction`, the default turning direction: `cw` or `ccw`.
    pub direction: String,
    /// `spool.left.speed` and `spool.right.speed`, applied under `custom` only.
    #[serde(default = "one")]
    pub spool_left: f32,
    #[serde(default = "one")]
    pub spool_right: f32,
    /// `spool.adaptive`: reels speed up and slow down with the tape's progress.
    #[serde(default)]
    pub spool_adaptive: bool,
    /// `queue.mode = queue`: progress runs over the whole queue.
    #[serde(default)]
    pub queue_mode: bool,
}

fn one() -> f32 {
    1.0
}

impl Default for RotationSettings {
    fn default() -> Self {
        Self {
            fps: 8,
            step: 6,
            speed: 1.0,
            direction: "ccw".into(),
            spool_left: 1.0,
            spool_right: 1.0,
            spool_adaptive: false,
            queue_mode: false,
        }
    }
}

pub fn rotation_settings(text: &str) -> RotationSettings {
    let quality = current_value(text, "rotation.quality")
        .map(|v| v.to_ascii_lowercase())
        .unwrap_or_else(|| "medium".into());
    let custom_fps = current_value(text, "rotation.fps")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(8)
        .max(1);
    let (fps, step, speed) = match quality.as_str() {
        "low" => (4, 12, 1.0),
        "high" => (15, 3, 1.0),
        "custom" => (
            custom_fps,
            (45 / custom_fps).clamp(1, 12),
            current_value(text, "rotation.speed")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(1.0),
        ),
        _ => (8, 6, 1.0),
    };
    let custom = quality == "custom";
    let spool = |key: &str| {
        if custom {
            current_value(text, key)
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(1.0)
        } else {
            1.0
        }
    };
    RotationSettings {
        fps,
        step,
        speed,
        direction: current_value(text, "reel.direction")
            .map(|v| v.to_ascii_lowercase())
            .filter(|v| v == "cw")
            .unwrap_or_else(|| "ccw".into()),
        spool_left: spool("spool.left.speed"),
        spool_right: spool("spool.right.speed"),
        spool_adaptive: truthy(current_value(text, "spool.adaptive").as_deref()),
        queue_mode: current_value(text, "queue.mode")
            .map(|v| v.eq_ignore_ascii_case("queue"))
            .unwrap_or(false),
    }
}

/// One tape reel: a theme picture, or a file from the track's folder scaled
/// to the theme picture's size, turning about `center`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReelSpec {
    pub theme_file: String,
    pub album_file: String,
    pub center: (i32, i32),
    /// Turns a minute before the spool multiplier: `reel.rotation.speed`.
    pub rpm: f32,
}

/// The two reels of a cassette meter and how they run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReelsSpec {
    pub left: Option<ReelSpec>,
    pub right: Option<ReelSpec>,
    pub clockwise: bool,
    /// The meter's own `spool.adaptive`, or the player's.
    pub adaptive: bool,
    pub spool_left: f32,
    pub spool_right: f32,
}

/// `reel.*` of a meter, when it has a reel with a centre. A meter with a
/// tonearm keeps its reels for the record instead.
pub fn meter_reels(
    meters_txt: &str,
    meter: &str,
    theme_dir: &str,
    settings: &RotationSettings,
) -> Option<ReelsSpec> {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    let ipair = |key: &str| -> Option<(i32, i32)> {
        let mut parts = get(key)?.split(',');
        Some((
            parts.next()?.trim().parse().ok()?,
            parts.next()?.trim().parse().ok()?,
        ))
    };
    let has_tonearm = get("tonearm.filename").is_some()
        && get("tonearm.pivot.screen").is_some()
        && get("tonearm.pivot.image").is_some();
    if has_tonearm || get("vinyl.center").is_some() {
        return None;
    }
    let rpm = get("reel.rotation.speed")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0)
        .abs();
    let path = |file: &str| {
        if theme_dir.is_empty() {
            file.to_string()
        } else {
            format!("{}/{}", theme_dir.trim_end_matches('/'), file)
        }
    };
    let reel = |side: &str| -> Option<ReelSpec> {
        let file = get(&format!("reel.{side}.filename"))?;
        let center = ipair(&format!("reel.{side}.center"))?;
        let (album_file, theme_file) = match file.split_once(',') {
            Some((album, theme)) => (
                album.trim().to_string(),
                if theme.trim().is_empty() {
                    album.trim().to_string()
                } else {
                    theme.trim().to_string()
                },
            ),
            None => (String::new(), file.to_string()),
        };
        Some(ReelSpec {
            theme_file: path(&theme_file),
            album_file,
            center,
            rpm,
        })
    };
    let (left, right) = (reel("left"), reel("right"));
    if left.is_none() && right.is_none() {
        return None;
    }
    let direction = get("reel.direction")
        .map(|v| v.to_ascii_lowercase())
        .unwrap_or_else(|| settings.direction.clone());
    Some(ReelsSpec {
        left,
        right,
        clockwise: direction == "cw",
        adaptive: get("spool.adaptive")
            .map(|v| truthy(Some(v)))
            .unwrap_or(settings.spool_adaptive),
        spool_left: settings.spool_left,
        spool_right: settings.spool_right,
    })
}

/// A turning record under the album art.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VinylSpec {
    /// The theme's picture, as a path.
    pub theme_file: String,
    /// A file name to prefer from the track's folder, or empty.
    pub album_file: String,
    pub x: i32,
    pub y: i32,
    /// `vinyl.center`: the point it turns about.
    pub center: (i32, i32),
    /// `vinyl.dimension`: the picture is stretched to this before it turns.
    pub dimension: Option<(u32, u32)>,
    pub clockwise: bool,
    /// Turns per minute: `albumart.rotation.speed` times the multiplier, or a
    /// stand-in reel's `reel.rotation.speed`.
    pub rpm: f32,
}

/// `vinyl.*` of a meter. With a tonearm but no vinyl, a single reel stands
/// in for the record, as the player's handler does.
pub fn meter_vinyl(
    meters_txt: &str,
    meter: &str,
    theme_dir: &str,
    settings: &RotationSettings,
) -> Option<VinylSpec> {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    let ipair = |key: &str| -> Option<(i32, i32)> {
        let mut parts = get(key)?.split(',');
        Some((
            parts.next()?.trim().parse().ok()?,
            parts.next()?.trim().parse().ok()?,
        ))
    };
    let upair = |key: &str| -> Option<(u32, u32)> {
        let mut parts = get(key)?.split(',');
        Some((
            parts.next()?.trim().parse().ok()?,
            parts.next()?.trim().parse().ok()?,
        ))
    };
    let path = |file: &str| {
        if theme_dir.is_empty() {
            file.to_string()
        } else {
            format!("{}/{}", theme_dir.trim_end_matches('/'), file)
        }
    };
    let has_tonearm = get("tonearm.filename").is_some()
        && get("tonearm.pivot.screen").is_some()
        && get("tonearm.pivot.image").is_some();
    let mut file = get("vinyl.filename").map(str::to_string);
    let mut pos = ipair("vinyl.pos");
    let mut center = ipair("vinyl.center");
    let mut rpm = get("albumart.rotation.speed")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0);
    if file.is_none() && has_tonearm {
        if let (Some(reel), Some(c)) = (get("reel.left.filename"), ipair("reel.left.center")) {
            file = Some(reel.to_string());
            pos = ipair("reel.left.pos");
            center = Some(c);
        } else if let (Some(reel), Some(c)) =
            (get("reel.right.filename"), ipair("reel.right.center"))
        {
            file = Some(reel.to_string());
            pos = ipair("reel.right.pos");
            center = Some(c);
        }
        if rpm <= 0.0 {
            rpm = get("reel.rotation.speed")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.0);
        }
    }
    let file = file?;
    let center = center?;
    // `a,b` prefers `a` from the track's folder with `b` from the theme;
    // `,b` and `b` are the theme's picture alone.
    let (album_file, theme_file) = match file.split_once(',') {
        Some((album, theme)) => (
            album.trim().to_string(),
            if theme.trim().is_empty() {
                file.clone()
            } else {
                theme.trim().to_string()
            },
        ),
        None => (String::new(), file.clone()),
    };
    let (x, y) = pos.unwrap_or((0, 0));
    let direction = get("vinyl.direction")
        .map(|v| v.to_ascii_lowercase())
        .unwrap_or_else(|| settings.direction.clone());
    Some(VinylSpec {
        theme_file: path(&theme_file),
        album_file,
        x,
        y,
        center,
        dimension: upair("vinyl.dimension").filter(|&(w, h)| w > 0 && h > 0),
        clockwise: direction != "ccw",
        rpm: (rpm * settings.speed).abs(),
    })
}

/// A tonearm that follows the track: parked at `rest`, dropped onto the
/// record over `drop_s` seconds, sweeping from `start` to `end` with the
/// track, lifted back over `lift_s`. Angles in degrees, 0 pointing right,
/// negative clockwise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TonearmSpec {
    pub file: String,
    pub pivot_screen: (i32, i32),
    pub pivot_image: (i32, i32),
    pub rest: f32,
    pub start: f32,
    pub end: f32,
    pub drop_s: f32,
    pub lift_s: f32,
}

pub fn meter_tonearm(meters_txt: &str, meter: &str, theme_dir: &str) -> Option<TonearmSpec> {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    let ipair = |key: &str| -> Option<(i32, i32)> {
        let mut parts = get(key)?.split(',');
        Some((
            parts.next()?.trim().parse().ok()?,
            parts.next()?.trim().parse().ok()?,
        ))
    };
    let number = |key: &str, default: f32| {
        get(key)
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(default)
    };
    let file = get("tonearm.filename")?;
    let pivot_screen = ipair("tonearm.pivot.screen")?;
    let pivot_image = ipair("tonearm.pivot.image")?;
    Some(TonearmSpec {
        file: if theme_dir.is_empty() {
            file.to_string()
        } else {
            format!("{}/{}", theme_dir.trim_end_matches('/'), file)
        },
        pivot_screen,
        pivot_image,
        rest: number("tonearm.angle.rest", -30.0),
        start: number("tonearm.angle.start", 0.0),
        end: number("tonearm.angle.end", 25.0),
        drop_s: number("tonearm.drop.duration", 1.5),
        lift_s: number("tonearm.lift.duration", 1.0),
    })
}

/// Sleep between steps for a frame rate in frames per second.
pub fn frame_period(rate: u32) -> std::time::Duration {
    let rate = rate.clamp(MIN_FRAME_RATE, MAX_FRAME_RATE);
    std::time::Duration::from_nanos(1_000_000_000 / u64::from(rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_record_is_left_then_right() {
        let (left, right) = decode_meter(&[50, 0, 25, 0]).unwrap();
        assert_eq!((left, right), (50, 25));
    }

    #[test]
    fn spectrum_record_is_little_endian_bins() {
        let bins = decode_spectrum(&[100, 0, 0, 0, 0, 0, 0, 0], 2).unwrap();
        assert_eq!(bins, vec![100.0, 0.0]);
    }

    #[test]
    fn scale_clamps_at_full() {
        assert_eq!(scale_level(150, 100.0, 100.0), 100.0);
        assert_eq!(scale_level(50, 100.0, 100.0), 50.0);
    }

    #[test]
    fn frame_rate_comes_from_current_and_clamps() {
        let text = "[sdl.env]\nframe.rate = 10\n\n[current]\nframe.rate = 15\n";
        assert_eq!(frame_rate_from_config(text), 15);
        assert_eq!(frame_rate_from_config("[current]\nframe.rate = 99\n"), 60);
        assert_eq!(frame_rate_from_config(""), DEFAULT_FRAME_RATE);
    }

    #[test]
    fn screen_size_follows_the_folder_then_the_override() {
        let folder = "[current]\nmeter.folder = 480x320-wide\nscreen.width =\nscreen.height =\n";
        assert_eq!(screen_from_config(folder), (480, 320));
        let override_size =
            "[current]\nmeter.folder = 480x320\nscreen.width = 1920\nscreen.height = 1080\n";
        assert_eq!(screen_from_config(override_size), (1920, 1080));
        assert_eq!(screen_from_config(""), (800, 480));
    }

    #[test]
    fn a_named_meter_uses_its_background() {
        let text = "[bar]\nbgr.filename = bar-bgr.png\nscreen.bgr =\n\n[blue]\nbgr.filename = blue-bgr.png\nscreen.bgr = blue-screen.png\n";
        assert_eq!(
            meter_background(text, "bar").as_deref(),
            Some("bar-bgr.png")
        );
        assert_eq!(
            meter_background(text, "blue").as_deref(),
            Some("blue-screen.png")
        );
        assert_eq!(
            meter_background(text, "random").as_deref(),
            Some("bar-bgr.png")
        );
    }

    #[test]
    fn meter_positions_come_from_the_named_section() {
        let text = "[bar]\nleft.x = 130\nleft.y = 113\nright.x = 130\nright.y = 178\n";
        assert_eq!(meter_at(text, "bar"), (Some((130, 113)), Some((130, 178))));
        let needle =
            "[gold]\nleft.origin.x = 333\nleft.origin.y = 305\nmeter.x = 0\nmeter.y = 124\n";
        assert_eq!(meter_at(needle, "gold").0, Some((333, 429)));
        assert_eq!(
            meter_indicator("[bar]\nindicator.filename = bar-indicator.png\n", "bar").as_deref(),
            Some("bar-indicator.png")
        );
    }

    #[test]
    fn theme_texts_come_with_style_size_and_colour() {
        let text = "[gold]\nplayinfo.title.pos = 10,20\n\n[black-white]\n\
            playinfo.title.pos = 283,138,bold\nplayinfo.title.color = 255,237,76\n\
            playinfo.artist.pos = 283,172,light\nplayinfo.samplerate.pos = 902,160,regular\n\
            time.remaining.pos = 1100,160\ntime.remaining.color = 180,180,180\n\
            playinfo.maxwidth = 535\nfont.size.light = 25\nfont.size.regular = 20\n\
            font.size.bold = 28\nfont.color = 255,255,255\n";
        let speeds = ScrollSpeeds {
            mode: "custom".into(),
            title: 8.0,
            artist: 10.0,
            album: 8.0,
        };
        let texts = meter_texts(text, "black-white", 1280, &speeds);
        let title = texts.title.unwrap();
        assert_eq!(
            (title.x, title.y, title.style, title.size),
            (283, 138, TextStyle::Bold, 28)
        );
        assert_eq!(
            (title.color, title.max_width, title.speed, title.align),
            ([255, 237, 76], 535, 8.0, TextAlign::Left)
        );
        let artist = texts.artist.unwrap();
        assert_eq!(
            (artist.style, artist.size, artist.color, artist.speed),
            (TextStyle::Light, 25, [255, 255, 255], 10.0)
        );
        assert_eq!(texts.sample.unwrap().style, TextStyle::Regular);
        let time = texts.time.unwrap();
        assert_eq!(
            (time.style, time.size, time.color, time.max_width),
            (TextStyle::Digi, 40, [180, 180, 180], 0)
        );
        assert!(texts.album.is_none() && texts.ticker.is_none());
        assert_eq!(
            meter_texts(text, "random", 1280, &speeds).title.unwrap().x,
            10
        );
    }

    #[test]
    fn time_fields_pick_their_font_and_inherit_the_remaining_colour() {
        let text = "[m]\ntime.remaining.pos = 585,605\ntime.remaining.color = 180,180,180\ntime.remaining.font = fonts/MyDigi.ttf\n\
            time.remaining.fontsize = 32\ntime.elapsed.pos = 400,605,italic\ntime.total.pos = 520,605,digi\n\
            time.total.color = 1,2,3\nfont.size.digi = 40\nfont.size.regular = 22\n";
        let texts = meter_texts(text, "m", 1280, &ScrollSpeeds::default());
        let remaining = texts.time.unwrap();
        assert_eq!(
            (
                remaining.style,
                remaining.size,
                remaining.font_file.as_str()
            ),
            (TextStyle::Digi, 32, "fonts/MyDigi.ttf")
        );
        let elapsed = texts.time_elapsed.unwrap();
        assert_eq!(
            (
                elapsed.style,
                elapsed.size,
                elapsed.color,
                elapsed.font_file.as_str()
            ),
            (TextStyle::Regular, 22, [180, 180, 180], ""),
            "an unknown style word is the regular font"
        );
        let total = texts.time_total.unwrap();
        assert_eq!(
            (total.style, total.size, total.color),
            (TextStyle::Digi, 40, [1, 2, 3])
        );
    }

    #[test]
    fn the_selection_names_random_or_a_list_and_visibility_needs_the_extension() {
        assert_eq!(
            selection_from_config("[current]\nmeter = gold\n"),
            Selection::Named("gold".into())
        );
        assert_eq!(
            selection_from_config("[current]\nmeter = Random\n"),
            Selection::Random
        );
        assert_eq!(
            selection_from_config("[current]\nmeter = gold, red ,dash\n"),
            Selection::List(vec!["gold".into(), "red".into(), "dash".into()])
        );
        assert_eq!(
            random_interval_from_config("[current]\nrandom.meter.interval = 15\n"),
            15
        );
        assert_eq!(random_interval_from_config(""), 60);
        assert!(random_change_title_from_config(
            "[current]\nrandom.change.title = True\n"
        ));
        assert!(!random_change_title_from_config(
            "[current]\nrandom.change.title = False\n"
        ));
        assert_eq!(
            meter_sections("[gold]\nx=1\n[ red ]\n\n[dash]\n"),
            ["gold", "red", "dash"]
        );
        let m = "[a]\nconfig.extend = True\nmeter.visible = False\n[b]\nmeter.visible = False\n[c]\nconfig.extend = True\n";
        assert!(!meter_visible(m, "a"));
        assert!(meter_visible(m, "b"), "meter.visible needs config.extend");
        assert!(meter_visible(m, "c"));
    }

    #[test]
    fn a_meter_spec_reads_kind_channels_angles_flips_and_bar_steps() {
        let m = "[bar]\nmeter.type = linear\nchannels = 2\nposition.regular = 9\nposition.overload = 4\nstep.width.regular = 34\n\
            step.width.overload = 20\ndirection = bottom-top\nindicator.type = single\nflip.right.x = True\nmeter.x = 770\nmeter.y = 569\n\
            [mono]\nmeter.type = circular\nchannels = 1\nmono.origin.x = 397\nmono.origin.y = 605\nmeter.x = 10\nmeter.y = 5\nleft.needle.flip = true\n\
            [pair]\nmeter.type = circular\nchannels = 2\nleft.start.angle = 40\nleft.stop.angle = -40\nright.start.angle = -40\nright.stop.angle = 40\nright.needle.flip = True\n";
        let bar = meter_spec(m, "bar");
        assert_eq!(
            (bar.kind, bar.channels, bar.mono_at),
            (MeterKind::Linear, 2, None)
        );
        let linear = bar.linear.unwrap();
        assert_eq!(
            (
                linear.direction,
                linear.single,
                linear.flip_left,
                linear.flip_right
            ),
            (Direction::BottomTop, true, false, true)
        );
        assert_eq!(
            linear.masks(),
            [0, 34, 68, 102, 136, 170, 204, 238, 272, 306, 326, 346, 366, 386]
        );
        assert_eq!(
            (
                linear.bar_width(0.0),
                linear.bar_width(0.5),
                linear.bar_width(1.0)
            ),
            (1, 238, 386),
            "14 steps; 0.5 reaches step 7; full is the last mask"
        );
        let mono = meter_spec(m, "mono");
        assert_eq!(
            (mono.kind, mono.channels, mono.mono_at, mono.flip_left),
            (MeterKind::Circular, 1, Some((407, 610)), true)
        );
        assert_eq!(mono.left_angles, None);
        let pair = meter_spec(m, "pair");
        assert_eq!(
            (pair.left_angles, pair.right_angles, pair.flip_right),
            (Some((40.0, -40.0)), Some((-40.0, 40.0)), true)
        );
        assert!(pair.visible);
        assert_eq!(
            meter_needle(m, "pair"),
            Some((40.0, -40.0, 0.0)),
            "the left pair stands in for missing shared angles"
        );
    }

    #[test]
    fn a_spectrum_is_read_from_the_meter_the_settings_and_the_theme() {
        let meters = "[m]\nconfig.extend = True\nspectrum.visible = True\nspectrum.name = s.2\nspectrum.size = 1260,307\n[n]\nspectrum.visible = True\nspectrum.name = s.2\nspectrum.size = 1,1\n";
        assert_eq!(meter_spectrum(meters, "m"), Some(("s.2".into(), 1260, 307)));
        assert_eq!(meter_spectrum(meters, "n"), None, "needs config.extend");
        let pair = "[p]\nconfig.extend = True\nspectrum.visible = True\nspectrum.name = left, right, mid\nspectrum.size = 400,200\nspectrum.2.size = 300,200\n";
        assert_eq!(
            meter_spectra(pair, "p"),
            vec![
                ("left".to_string(), 400, 200),
                ("right".to_string(), 300, 200),
                ("mid".to_string(), 400, 200)
            ],
            "a list of boxes, the second with its own size, the third the shared one"
        );
        let settings = spectrum_settings("[current]\nspectrum = s.7\nbase.folder = /t\nspectrum.folder = 1280x720\nmax.value = 100\nsize = 20\n");
        assert_eq!(
            settings,
            SpectrumSettings {
                base_folder: "/t".into(),
                folder: "1280x720".into(),
                bins: 20,
                max_value: 100.0
            }
        );
        let theme = "[s.2]\norigin.x = 123\norigin.y = 196\nspectrum.x = 10\nspectrum.y = 224\nbgr.type = image\nbgr.filename = bgr-2.png\n\
            bar.type = image\nbar.filename = bar-2.png\nbar.width = 27\nbar.height = 210\nbar.gap = 25\nreflection.type = gradient\n\
            reflection.gradient = (0, 0, 0, 0), (0, 0, 0, 80)\nreflection.gap = 0\ntopping.height = 3\ntopping.step = 2\nfgr.filename =\nsteps = 30\n";
        let spec =
            spectrum_from_theme(theme, "s.2", (1260, 307), &settings, "/t/1280x720").unwrap();
        assert_eq!(
            (spec.x, spec.y, spec.w, spec.h, spec.origin_x, spec.origin_y),
            (10, 224, 1260, 307, 123, 196)
        );
        assert_eq!(
            spec.background,
            Some(Fill::Image("/t/1280x720/bgr-2.png".into()))
        );
        assert_eq!(spec.bar, Some(Fill::Image("/t/1280x720/bar-2.png".into())));
        assert_eq!(
            spec.reflection,
            Some(Fill::Gradient(vec![[0, 0, 0, 0], [0, 0, 0, 80]]))
        );
        assert_eq!(
            (spec.topping, spec.foreground.as_str(), spec.step()),
            (Some((3, 2)), "", 7)
        );
        // 210 / 30 = 7 px steps; a raw 50 is 105 px, exactly 15 steps; 51 rounds up to 16.
        assert_eq!(
            (
                spec.bar_height(0.0),
                spec.bar_height(50.0),
                spec.bar_height(51.0),
                spec.bar_height(100.0)
            ),
            (0, 105, 112, 210)
        );
        assert_eq!(
            spectrum_from_theme(theme, "s.9", (1, 1), &settings, ""),
            None
        );
    }

    #[test]
    fn folder_layers_need_a_box_and_look_in_the_track_folder() {
        let m = "[m]\nfont.color = 1,2,3\nfolderlayer.enabled = True\nfolderlayer.pos = 40,40\nfolderlayer.dimension = 300,300\nfolderlayer.zorder = background\n\
            folderlayer.2.files = logo.png, Logo.png\nfolderlayer.2.pos = 980,40\nfolderlayer.2.dimension = 240,120\nfolderlayer.2.scale = stretch\nfolderlayer.2.border = 2\n\
            folderlayer.3.pos = 1,1\n";
        let layers = meter_folder_layers(m, "m");
        assert_eq!(layers.len(), 2, "the third has no dimension");
        assert_eq!(
            (
                layers[0].x,
                layers[0].y,
                layers[0].w,
                layers[0].h,
                layers[0].zorder,
                layers[0].scale,
                layers[0].border
            ),
            (40, 40, 300, 300, ZOrder::Background, Scale::Fit, 0)
        );
        assert_eq!(
            layers[0].files,
            FOLDER_LAYER_FILES.map(String::from).to_vec()
        );
        assert_eq!(
            (
                layers[1].files.clone(),
                layers[1].scale,
                layers[1].zorder,
                layers[1].border,
                layers[1].border_color
            ),
            (
                vec!["logo.png".to_string(), "Logo.png".to_string()],
                Scale::Stretch,
                ZOrder::Overlay,
                2,
                [1, 2, 3]
            )
        );
        let files = [
            "back.png".to_string(),
            "../x.png".to_string(),
            "a/b.png".to_string(),
            "logo.txt".to_string(),
            "Logo.JPG".to_string(),
        ];
        assert_eq!(
            folder_candidates("mnt/INTERNAL/U2/War (1983)/02. Seconds.flac", &files),
            [
                "/mnt/INTERNAL/U2/War (1983)/back.png",
                "/mnt/INTERNAL/U2/War (1983)/Logo.JPG"
            ]
        );
        assert_eq!(
            folder_candidates("music-library/NAS/a/b.flac", &files)[0],
            "/mnt/NAS/a/back.png"
        );
        assert!(folder_candidates("", &files).is_empty());
        assert_eq!(
            folder_candidates("cue://NAS/Music/Songs Without Words/CD1.cue@5", &files)[0],
            "/mnt/NAS/Music/Songs Without Words/back.png",
            "a track inside a cue sheet is in the sheet's folder"
        );
        assert_eq!(
            folder_candidates("rp2/channel@id=0", &files),
            ["/mnt/rp2/back.png", "/mnt/rp2/Logo.JPG"],
            "a stream maps under /mnt too and simply is not found"
        );
    }

    #[test]
    fn a_fanart_slot_needs_position_and_dimension() {
        let m = "[m]\nfanart.pos = 0,0\nfanart.dimension = 1280,720\nfanart.scale = stretch\n[n]\nfanart.pos = 1,1\n";
        let slot = meter_fanart(m, "m").unwrap();
        assert_eq!(
            (slot.x, slot.y, slot.w, slot.h, slot.scale, slot.zorder),
            (0, 0, 1280, 720, Scale::Stretch, ZOrder::Background)
        );
        assert_eq!(meter_fanart(m, "n"), None);
    }

    #[test]
    fn a_turntable_meter_has_a_record_and_a_tonearm() {
        let settings = rotation_settings("[current]\nrotation.quality = custom\nrotation.fps = 25\nrotation.speed = 2\nreel.direction = ccw\nspool.left.speed = 1.5\nspool.adaptive = true\nqueue.mode = queue\n");
        assert_eq!(
            settings,
            RotationSettings {
                fps: 25,
                step: 1,
                speed: 2.0,
                direction: "ccw".into(),
                spool_left: 1.5,
                spool_right: 1.0,
                spool_adaptive: true,
                queue_mode: true
            }
        );
        assert_eq!(rotation_settings("[current]\nrotation.quality = high\nrotation.speed = 3\nspool.left.speed = 4\nreel.direction = cw\n"), RotationSettings { fps: 15, step: 3, speed: 1.0, direction: "cw".into(), ..RotationSettings::default() });
        let m = "[t]\nalbumart.pos = 195,235\nalbumart.dimension = 198,198\nalbumart.rotation = True\nalbumart.rotation.speed = 30\n\
            tonearm.filename = arm.png\ntonearm.pivot.screen = 629,166\ntonearm.pivot.image = 57,129\ntonearm.angle.rest = 0\ntonearm.angle.start = -27\n\
            tonearm.angle.end = -47\ntonearm.drop.duration = 1.8\nvinyl.filename = vinyl.jpg,disc.png\nvinyl.pos = 50,92\nvinyl.center = 293,334\nvinyl.direction = cw\n\
            [r]\ntonearm.filename = arm.png\ntonearm.pivot.screen = 1,1\ntonearm.pivot.image = 1,1\nreel.left.filename = reel.png\nreel.left.pos = 5,5\nreel.left.center = 40,40\nreel.rotation.speed = 3\n";
        let vinyl = meter_vinyl(m, "t", "/th", &settings).unwrap();
        assert_eq!(
            (
                vinyl.theme_file.as_str(),
                vinyl.album_file.as_str(),
                vinyl.x,
                vinyl.y,
                vinyl.center,
                vinyl.clockwise,
                vinyl.rpm
            ),
            ("/th/disc.png", "vinyl.jpg", 50, 92, (293, 334), true, 60.0)
        );
        let arm = meter_tonearm(m, "t", "/th").unwrap();
        assert_eq!(
            (
                arm.file.as_str(),
                arm.pivot_screen,
                arm.pivot_image,
                arm.rest,
                arm.start,
                arm.end,
                arm.drop_s,
                arm.lift_s
            ),
            (
                "/th/arm.png",
                (629, 166),
                (57, 129),
                0.0,
                -27.0,
                -47.0,
                1.8,
                1.0
            )
        );
        let art = meter_art(m, "t", "/th").unwrap();
        assert!((art.rotation, art.rpm) == (true, 30.0));
        let reel = meter_vinyl(m, "r", "", &settings).unwrap();
        assert_eq!(
            (
                reel.theme_file.as_str(),
                reel.center,
                reel.rpm,
                reel.clockwise
            ),
            ("reel.png", (40, 40), 6.0, false),
            "a single reel stands in, turning the default way"
        );
        let c = "[c]\nreel.left.filename = cdart.png,left.png\nreel.left.center = 360,321\nreel.right.filename = right.png\nreel.right.center = 957,321\nreel.rotation.speed = 25\nspool.adaptive = false\n";
        let reels = meter_reels(c, "c", "/th", &settings).unwrap();
        let left = reels.left.unwrap();
        assert_eq!(
            (
                left.theme_file.as_str(),
                left.album_file.as_str(),
                left.center,
                left.rpm
            ),
            ("/th/left.png", "cdart.png", (360, 321), 25.0)
        );
        assert_eq!(
            (
                reels.right.unwrap().theme_file.as_str(),
                reels.clockwise,
                reels.adaptive,
                reels.spool_left
            ),
            ("/th/right.png", false, false, 1.5),
            "the meter's spool.adaptive overrides the player's"
        );
        assert_eq!(
            meter_reels(m, "r", "", &settings),
            None,
            "a reel with a tonearm is the record, not a reel"
        );
        assert_eq!(
            meter_vinyl("[x]\nvinyl.filename = a.png\n", "x", "", &settings),
            None,
            "no centre, no record"
        );
    }

    #[test]
    fn indicators_read_leds_icons_gauges_and_markers() {
        let m = "[i]\nconfig.extend = True\nmute.pos = 50,680\nmute.led = 16,16\nmute.led.shape = rect\nmute.led.color = 64,64,64,255,0,0,255,128,0\nmute.led.glow = 8\nmute.led.glow.intensity = 0.7\n\
            shuffle.pos = 1,2\nshuffle.led = 10,10\nshuffle.led.color = 0,200,255,64,64,64\nrepeat.pos = 3,4\nrepeat.icon = r_off.png,r_all.png,r_single.png,r_inf.png\nrepeat.icon.glow = 6\n\
            playstate.pos = 5,6\nplaystate.icon = stop.png,,play.png\nvolume.pos = 843,425\nvolume.dim = 55,41\nvolume.style = slider\nvolume.slider.tip = tip.png\nvolume.slider.travel = 3,202\nvolume.slider.tip.offset = -7,0\n\
            progress.pos = 947,466\nprogress.dim = 315,17\nprogress.color = 22,22,22\nprogress.bg.color = 173,143,99\nprogress.border = 1\nprogress.border.color = 173,143,99\n\
            progress.marker.1.pos = 0\nprogress.marker.1.label = |\nprogress.marker.2.pos = 50\nprogress.marker.2.image = m.png\nprogress.marker.2.fontsize = 12\nprogress.marker.3.pos = 100\nprogress.marker.4.pos = 75\nprogress.marker.4.label = x\nprogress.head.image = head.png\n\
            [n]\nmute.pos = 1,1\nmute.icon = a.png\n";
        let spec = meter_indicators(m, "i", "/th").unwrap();
        let mute = spec.mute.unwrap();
        assert_eq!(
            (
                mute.x,
                mute.y,
                mute.glow,
                mute.glow_intensity,
                mute.states()
            ),
            (50, 680, 8, 0.7, 3)
        );
        assert_eq!(
            mute.look,
            StateLook::Led {
                w: 16,
                h: 16,
                circle: false,
                colors: vec![[64, 64, 64], [255, 0, 0], [255, 128, 0]]
            }
        );
        let shuffle = spec.shuffle.unwrap();
        assert_eq!(
            shuffle.look,
            StateLook::Led {
                w: 10,
                h: 10,
                circle: true,
                colors: vec![[64, 64, 64], [0, 200, 255], [0, 200, 255]]
            },
            "six legacy values are on then off"
        );
        let repeat = spec.repeat.unwrap();
        assert_eq!(
            (repeat.states(), repeat.glow, repeat.glow_intensity),
            (4, 6, 0.5)
        );
        assert_eq!(
            repeat.look,
            StateLook::Icons {
                files: [
                    "/th/r_off.png",
                    "/th/r_all.png",
                    "/th/r_single.png",
                    "/th/r_inf.png"
                ]
                .map(String::from)
                .to_vec()
            }
        );
        assert_eq!(
            spec.playstate.unwrap().look,
            StateLook::Icons {
                files: vec!["/th/stop.png".into(), String::new(), "/th/play.png".into()]
            }
        );
        let volume = spec.volume.unwrap();
        assert_eq!(
            (
                volume.style,
                volume.tip.as_str(),
                volume.travel,
                volume.tip_offset,
                volume.vertical(),
                volume.bg_color
            ),
            (
                GaugeStyle::Slider,
                "/th/tip.png",
                Some((3, 202)),
                (-7, 0),
                true,
                None
            )
        );
        let progress = spec.progress.unwrap();
        assert_eq!(
            (
                progress.style,
                progress.color,
                progress.bg_color,
                progress.border,
                progress.border_color,
                progress.vertical()
            ),
            (
                GaugeStyle::Slider,
                [22, 22, 22],
                Some([173, 143, 99]),
                1,
                [173, 143, 99],
                false
            )
        );
        assert_eq!(progress.markers.len(), 3, "the third marker has neither picture nor label and is skipped; the fourth still counts");
        assert_eq!(
            (
                progress.markers[1].pos,
                progress.markers[1].image.as_str(),
                progress.markers[1].font_size
            ),
            (50.0, "/th/m.png", Some(12))
        );
        assert_eq!(progress.head_image.as_str(), "/th/head.png");
        assert_eq!(
            meter_indicators(m, "n", ""),
            None,
            "indicators need config.extend"
        );
    }

    #[test]
    fn the_transition_settings_follow_the_player() {
        let s = transition_settings("[current]\nstart.animation = True\ntransition.type = fade\ntransition.duration = 1.5\ntransition.color = white\ntransition.opacity = 60\n");
        assert_eq!(
            s,
            TransitionSettings {
                at_start: true,
                fade: true,
                duration_s: 1.5,
                white: true,
                opacity: 0.6
            }
        );
        assert_eq!(
            transition_settings("[current]\ntransition.type = none\n"),
            TransitionSettings {
                fade: false,
                ..TransitionSettings::default()
            }
        );
        assert_eq!(transition_settings(""), TransitionSettings::default());
    }

    /// The theme's size comes from the folder's name, also when the
    /// configuration names the folder by its whole path.
    #[test]
    fn the_screen_size_reads_the_folder_name_out_of_a_path() {
        assert_eq!(
            screen_from_config("[current]\nmeter.folder = 1280x720_x\n"),
            (1280, 720)
        );
        assert_eq!(
            screen_from_config(
                "[current]\nmeter.folder = /data/INTERNAL/glass/templates/1280x720_x\n"
            ),
            (1280, 720)
        );
        assert_eq!(screen_from_config("[current]\nmeter.folder = /tmp/cut/templates/3840x2160_y\nscreen.width = 100\nscreen.height = 50\n"), (100, 50));
        assert_eq!(
            screen_from_config("[current]\nmeter.folder = /x/none\n"),
            (800, 480)
        );
    }

    /// Four corners and the centre, read through a swapped and flipped
    /// panel: the fit finds the map that undoes it, to within rounding,
    /// and points on one line pin nothing.
    #[test]
    fn the_affine_fit_undoes_a_swapped_and_flipped_panel() {
        let panel = |x: f32, y: f32| (1.0 - y, x);
        let targets = [(0.1, 0.1), (0.9, 0.1), (0.9, 0.9), (0.1, 0.9), (0.5, 0.5)];
        let pairs: Vec<_> = targets
            .iter()
            .map(|&(x, y)| (panel(x, y), (x, y)))
            .collect();
        let (m, worst) = fit_affine(&pairs).expect("five points pin the map");
        assert!(worst < 1e-5, "worst {worst}");
        let back = apply_matrix(m, panel(0.3, 0.7));
        assert!((back.0 - 0.3).abs() < 1e-5 && (back.1 - 0.7).abs() < 1e-5);
        assert_eq!(apply_matrix(IDENTITY_MATRIX, (0.25, 0.75)), (0.25, 0.75));
        let flat: Vec<_> = [(0.1, 0.5), (0.5, 0.5), (0.9, 0.5)]
            .iter()
            .map(|&p| (p, p))
            .collect();
        assert!(
            fit_affine(&flat).is_none(),
            "points on one line pin nothing"
        );
        assert!(fit_affine(&pairs[..2]).is_none(), "two points are too few");
        // A lift the calibration did not ask for, ahead of the five, puts
        // every sample one target off: the fit is far past the tolerance.
        let shifted: Vec<_> = std::iter::once((0.5, 0.5))
            .chain(pairs.iter().map(|(raw, _)| *raw))
            .zip(pairs.iter().map(|(_, expected)| *expected))
            .collect();
        let (_, off) = fit_affine(&shifted).expect("five points still pin a map");
        let panel = (720u32, 1280u32);
        assert!(
            off * 1468.0 > calibration_tolerance(panel),
            "a shifted set is refused: {off}"
        );
        assert!(worst * 1468.0 < calibration_tolerance(panel));
        assert!(calibration_miss(&pairs).expect("five pairs") * 1468.0 < 0.01);
        // One target missed by a tenth of the panel: the fit from all five
        // spreads it under the tolerance, held against the other four it
        // shows in full and is refused.
        let mut missed = pairs.clone();
        missed[2].0 .1 -= 0.1;
        let (_, spread) = fit_affine(&missed).expect("five points pin a map");
        let miss = calibration_miss(&missed).expect("five pairs");
        assert!(
            spread * 1468.0 < calibration_tolerance(panel),
            "spread {spread}"
        );
        assert!(miss * 1468.0 > calibration_tolerance(panel), "miss {miss}");
        assert!(miss > 0.09, "the miss shows in full: {miss}");
        assert!(
            calibration_miss(&pairs[..3]).is_none(),
            "three pairs leave nothing to hold one against"
        );
        assert!((calibration_tolerance(panel) - 73.4).abs() < 0.1);
        assert!((calibration_tolerance((320, 240)) - 20.0).abs() < 1e-3);
        assert_eq!(
            parse_matrix(Some("0, 1, 0, 1, 0, 0")),
            [0.0, 1.0, 0.0, 1.0, 0.0, 0.0]
        );
        assert_eq!(parse_matrix(Some("nonsense")), IDENTITY_MATRIX);
        assert_eq!(face_scale(None), 1.0);
        assert_eq!(face_scale(Some("Large")), 1.4);
        assert_eq!(face_scale(Some(" car ")), 2.0);
        assert_eq!(face_scale(Some("huge")), 1.0, "anything else is normal");
        let run = run_settings(
            "[current]\nface.size = car\nface.theme = Midnight\nface.colours.accent = \\#ff8800\nface. = x\nscreen.rotation = 90\n[other]\nface.lost = 1\n",
        );
        assert_eq!(run.face_scale, 2.0);
        let pairs: Vec<String> = run.face.iter().map(|(k, v)| format!("{k}={v}")).collect();
        assert_eq!(
            pairs.join(" "),
            "colours.accent=#ff8800 size=car theme=Midnight",
            "the face's keys of the current section, by name, the writer's backslash before a # taken off"
        );
    }

    #[test]
    fn the_run_settings_and_the_dismiss_rule_follow_the_player() {
        let s = run_settings("[current]\nexit.on.touch = False\nstop.display.on.touch = True\nposition.type = custom\nposition.x = 10\nposition.y = 20\n");
        assert_eq!(
            s,
            RunSettings {
                interactive: InteractiveMode::Theme,
                exit_on_touch: true,
                centered: false,
                x: 10,
                y: 20,
                fit: false,
                driver: ScreenDriver::Auto,
                rotation: Rotation::R0,
                pointer: false,
                touch_matrix: IDENTITY_MATRIX,
                face_scale: 1.0,
                face: Default::default(),
            }
        );
        assert_eq!(run_settings(""), RunSettings::default());
        assert_eq!(
            run_settings("[current]\ntouch.matrix = 0,1,0,1,0,0\n").touch_matrix,
            [0.0, 1.0, 0.0, 1.0, 0.0, 0.0],
            "a swap"
        );
        assert_eq!(
            run_settings("[current]\ntouch.matrix = 1,2,3\n").touch_matrix,
            IDENTITY_MATRIX,
            "six numbers or nothing"
        );
        assert_eq!(
            run_settings("[current]\ntouch.matrix = 0,0,0,0,0,0\n").touch_matrix,
            IDENTITY_MATRIX,
            "a matrix that flattens everything is nothing"
        );
        assert!(run_settings("[current]\nscreen.pointer.shown = True\n").pointer);
        assert!(
            !run_settings("[current]\nscreen.pointer = show\n").pointer,
            "the choice alone shows nothing; the plugin resolves it"
        );
        let screen = run_settings("[current]\nscreen.driver = KMS\nscreen.rotation = 270\n");
        assert_eq!(
            (screen.driver, screen.rotation),
            (ScreenDriver::KmsDrm, Rotation::R270)
        );
        assert_eq!(screen.driver.sdl_name(), Some("kmsdrm"));
        assert!(screen.rotation.quarter() && screen.rotation.degrees() == 270);
        let odd = run_settings("[current]\nscreen.driver = fbdev\nscreen.rotation = 45\n");
        assert_eq!(
            (odd.driver, odd.rotation),
            (ScreenDriver::Auto, Rotation::R0)
        );
        let fitted = run_settings("[current]\nposition.type = Fit\nposition.x = 10\n");
        assert!(
            fitted.fit && fitted.centered,
            "fit scales and centres: {fitted:?}"
        );
        assert!(!s.fit, "a manual position does not fit");
        let placed = run_settings("[current]\nposition.fit = True\nposition.type = manual\nposition.x = 0\nposition.y = 0\n");
        assert!(
            placed.fit && !placed.centered,
            "fitted and placed: {placed:?}"
        );
        // A position may start left of the screen's corner, or above it.
        let before = run_settings(
            "[current]\nposition.type = manual\nposition.x = -40\nposition.y = -1080\n",
        );
        assert_eq!((before.x, before.y, before.centered), (-40, -1080, false));
        assert!(should_mark_dismiss(Some("/tmp/glass_dismiss"), false, true));
        assert!(
            !should_mark_dismiss(Some("/tmp/glass_dismiss"), true, true),
            "the plugin's own stop is not a dismiss"
        );
        assert!(
            !should_mark_dismiss(Some("/tmp/glass_dismiss"), false, false),
            "no run flag, no plugin to re-arm"
        );
        assert!(
            !should_mark_dismiss(None, false, true),
            "a remote launcher sets no marker"
        );
    }

    #[test]
    fn the_data_source_section_has_the_engine_defaults() {
        let spec = data_source_from_config("[current]\nmeter = x\n\n[data.source]\nvolume.max = 100.0\nvolume.gain.db = -6\nsmooth.buffer.size = 2\nstereo.algorithm = average\n");
        assert_eq!(
            (spec.max_ui, spec.max_pipe, spec.gain_db, spec.smooth),
            (100.0, 100.0, -6.0, 2)
        );
        assert_eq!(
            (
                spec.stereo.as_str(),
                spec.mono.as_str(),
                spec.gain_source.as_str()
            ),
            ("average", "average", "")
        );
        assert_eq!(data_source_from_config(""), DataSourceSpec::default());
    }

    #[test]
    fn boxes_alignment_speeds_and_the_ticker_follow_the_player() {
        let text = "[m]\nplayinfo.title.pos = 830,205,italic\nplayinfo.artist.pos = 830,40\nplayinfo.artist.maxwidth = 441\n\
            playinfo.center = True\nplayinfo.scrolling.speed = 25\nplayinfo.scrolling.speed.title = 15\nfont.size.regular = 22\n\
            playinfo.ticker = True\nplayinfo.ticker.pos = 40,420,regular\nplayinfo.ticker.maxwidth = 9000\n\
            playinfo.ticker.direction = ltr\nplayinfo.ticker.separator =  - \nplayinfo.ticker.space_between = 1\n\
            playinfo.ticker.end_spaces = 10\nplayinfo.ticker.append_next = True\nplayinfo.ticker.replace = True\n\
            playinfo.next.title.pos = 830,300\n";
        let skin = ScrollSpeeds {
            mode: "skin".into(),
            ..ScrollSpeeds::default()
        };
        let texts = meter_texts(text, "m", 1280, &skin);
        let title = texts.title.unwrap();
        assert_eq!(
            (title.style, title.size, title.align),
            (TextStyle::Italic, 22, TextAlign::Center)
        );
        assert_eq!(
            (title.max_width, title.speed),
            (768, 15.0),
            "centred: six tenths of the screen; per-field speed"
        );
        let artist = texts.artist.unwrap();
        assert_eq!(
            (artist.max_width, artist.speed),
            (441, 25.0),
            "own maxwidth; meter's global speed"
        );
        assert_eq!(texts.next_title.unwrap().max_width, 768);
        let ticker = texts.ticker.unwrap();
        assert_eq!(
            (ticker.text.x, ticker.text.max_width, ticker.text.speed),
            (40, 1240, 25.0),
            "capped to the visible width"
        );
        assert_eq!((ticker.direction, ticker.separator.as_str(), ticker.space_between, ticker.end_spaces), (ScrollDirection::Ltr, "-", 1, 10), "values are trimmed as the player's parser trims them; spacing comes from space_between");
        assert!(ticker.append_next && ticker.replace);
        let default_mode = ScrollSpeeds {
            mode: "default".into(),
            ..ScrollSpeeds::default()
        };
        assert_eq!(
            meter_texts(text, "m", 1280, &default_mode)
                .title
                .unwrap()
                .speed,
            40.0
        );
        let plain = meter_texts("[m]\nplayinfo.title.pos = 100,5\n", "m", 800, &default_mode);
        assert_eq!(
            plain.title.unwrap().max_width,
            680,
            "auto box: screen minus x minus margin"
        );
    }

    #[test]
    fn album_art_box_needs_position_and_dimension() {
        let text = "[black-white]\nalbumart.pos = 36,25\nalbumart.dimension = 201,201\n";
        let art = meter_art(text, "black-white", "/themes/t").unwrap();
        assert_eq!((art.x, art.y, art.w, art.h), (36, 25, 201, 201));
        assert_eq!(
            (art.mask.as_str(), art.border, art.border_color),
            ("", 0, [255, 255, 255])
        );
        assert_eq!(meter_art("[bar]\nalbumart.pos = 1,2\n", "bar", ""), None);
        let masked = "[v]\nalbumart.pos = 27,28\nalbumart.dimension = 432,432\nalbumart.mask = mask.png\nalbumart.border = 2\nfont.color = 10,20,30\n";
        let art = meter_art(masked, "v", "/themes/v").unwrap();
        assert_eq!(
            (art.mask.as_str(), art.border, art.border_color),
            ("/themes/v/mask.png", 2, [10, 20, 30])
        );
    }

    #[test]
    fn track_types_become_keys_and_labels() {
        assert_eq!(format_key("FLAC"), "flac");
        assert_eq!(format_key("dsf"), "dsd");
        assert_eq!(format_key("dab_●◦◦◦◦"), "dab");
        assert_eq!(format_key("Tidal Connect"), "tidal");
        assert_eq!(format_key("The Main Mix - "), "the_main_mix_");
        assert_eq!(format_key("WebRadio"), "radio");
        assert_eq!(format_label("radio"), "Webradio");
        assert_eq!(format_label("flac"), "FLAC");
        assert_eq!(format_label(""), "");
    }

    #[test]
    fn type_area_follows_mode_box_and_size_rules() {
        let text = "[m]\nplayinfo.type.pos = 847,149\nplayinfo.type.dimension = 45,45\n\
            playinfo.type.color = 204,176,97\nplayinfo.samplerate.pos = 902,160,regular\nfont.size.regular = 20\n";
        let spec = meter_type(text, "m", Some("icon")).unwrap();
        assert_eq!((spec.x, spec.y, spec.box_size), (847, 149, Some((45, 45))));
        assert_eq!(
            (spec.mode, spec.align, spec.color),
            (TypeMode::Icon, TypeAlign::Center, [204, 176, 97])
        );
        assert_eq!((spec.font_size, spec.font_style), (20, TextStyle::Regular));
        let meter_wins = "[m]\nplayinfo.type.pos = 1,1\nplayinfo.type.dimension = 53,53\nplayinfo.type.mode = both\nplayinfo.type.align = right\nplayinfo.type.fontsize = 18\n";
        let spec = meter_type(meter_wins, "m", Some("text")).unwrap();
        assert_eq!(
            (spec.mode, spec.align, spec.font_size),
            (TypeMode::Both, TypeAlign::Right, 18)
        );
        assert_eq!(
            meter_type(
                "[m]\nplayinfo.type.pos = 1,1\nplayinfo.type.dimension = 1,1\n",
                "m",
                None
            ),
            None
        );
        let text_only = meter_type(
            "[m]\nplayinfo.type.pos = 5,6\nplayinfo.type.mode = text\n",
            "m",
            None,
        )
        .unwrap();
        assert_eq!((text_only.box_size, text_only.font_size), (None, 30));
        assert_eq!(type_font_size(20, Some(45)), 20);
        assert_eq!(type_font_size(40, Some(45)), 20);
        assert_eq!(type_font_size(40, Some(12)), 10);
    }

    #[test]
    fn font_files_join_the_path_and_default_the_clock_font() {
        let none = Path::new("/nowhere/fonts");
        // The player's fonts, named under font.path (the old form with a leading slash too).
        let text = "[current]\nuse.system.fonts = True\nfont.path = /fonts\nfont.light = /Lato-Light.ttf\nfont.bold = Lato-Bold.ttf\n";
        let fonts = fonts_from_config(text, none);
        assert_eq!(fonts.light, "/fonts/Lato-Light.ttf");
        assert_eq!(fonts.bold, "/fonts/Lato-Bold.ttf");
        assert_eq!(fonts.regular, "", "nothing named and nothing shipped");
        assert_eq!(fonts.digi, "/nowhere/fonts/DSEG7Classic-Italic.ttf");
        assert_eq!(fonts.italic, "", "no italic shipped here");
        assert_eq!(fonts.fallback, "");
        let own = fonts_from_config("[current]\nfont.path = /f\nfont.italic = /I.ttf\n", none);
        assert_eq!(own.italic, "/f/I.ttf");
        let speeds = scroll_speeds_from_config("[current]\nscrolling.mode = custom\nscrolling.speed.title = 8\nscrolling.speed.artist = 10\n");
        assert_eq!(
            (
                speeds.mode.as_str(),
                speeds.title,
                speeds.artist,
                speeds.album
            ),
            ("custom", 8.0, 10.0, 40.0)
        );
    }

    /// With the PeppyFont set shipped, the styles are set in it unless the
    /// configuration asks for the system fonts; the regular face is the
    /// fallback either way.
    #[test]
    fn the_shipped_multi_script_faces_are_the_default_and_the_fallback() {
        let dir = std::env::temp_dir().join(format!("glass-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            "PeppyFont-Light.ttf",
            "PeppyFont-Regular.ttf",
            "PeppyFont-Bold.ttf",
            "PeppyFont-Italic.ttf",
            "Mine.ttf",
        ] {
            std::fs::write(dir.join(name), b"").unwrap();
        }
        let at = |name: &str| dir.join(name).to_string_lossy().into_owned();
        // Nothing named, or builtin: the shipped faces.
        let plain = fonts_from_config(
            "[current]\nfont.path = /fonts\nfont.regular = builtin\n",
            &dir,
        );
        assert_eq!(plain.light, at("PeppyFont-Light.ttf"));
        assert_eq!(plain.regular, at("PeppyFont-Regular.ttf"));
        assert_eq!(plain.bold, at("PeppyFont-Bold.ttf"));
        assert_eq!(plain.italic, at("PeppyFont-Italic.ttf"));
        assert_eq!(plain.fallback, at("PeppyFont-Regular.ttf"));
        // An uploaded font, named by its path, for one style; the others stay built in.
        let custom = fonts_from_config(
            &format!(
                "[current]\nfont.path = /fonts\nfont.bold = {}\n",
                at("Mine.ttf")
            ),
            &dir,
        );
        assert_eq!(custom.bold, at("Mine.ttf"));
        assert_eq!(custom.light, at("PeppyFont-Light.ttf"));
        // The player's fonts by name under font.path.
        let lato = "[current]\nfont.path = /fonts\nfont.light = /Lato-Light.ttf\nfont.regular = Lato-Regular.ttf\n";
        let players = fonts_from_config(lato, &dir);
        assert_eq!(players.light, "/fonts/Lato-Light.ttf");
        assert_eq!(players.regular, "/fonts/Lato-Regular.ttf");
        assert_eq!(
            players.bold,
            at("PeppyFont-Bold.ttf"),
            "unnamed styles stay built in"
        );
        // An older configuration: use.system.fonts False meant the built-in set, whatever the names.
        let older = fonts_from_config(&format!("{lato}use.system.fonts = False\n"), &dir);
        assert_eq!(older.light, at("PeppyFont-Light.ttf"));
        assert_eq!(older.regular, at("PeppyFont-Regular.ttf"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod interactive_tests {
    use super::*;

    #[test]
    fn a_meter_names_its_buttons_and_says_whether_it_is_for_fingers() {
        let text = "[dash]\nmeter.type = linear\nconfig.extend = True\ninteractive = True\nbutton.play.pos = 10,20\nbutton.play.size = 40,40\nbutton.play.action = toggle\nbutton.nextmeter.pos = 60,20\nbutton.nextmeter.image = next.png\nbutton.nextmeter.action = meter.next\nbutton.bad.pos = 1,1\nbutton.zmute.pos = 1,2\nbutton.zmute.image = m_off.png, m_on.png\nbutton.zmute.action = mute\nbutton.zrepeat.pos = 1,3\nbutton.zrepeat.image = r_off.png, r_all.png, r_single.png\nbutton.zrepeat.action = repeat\n";
        let spec = meter_indicators(text, "dash", "/t").expect("extended");
        assert!(spec.interactive);
        assert_eq!(
            spec.buttons.len(),
            4,
            "a button without an action is no button"
        );
        let mute = &spec.buttons[2];
        assert_eq!(
            (mute.image.as_str(), mute.image_active.as_str()),
            ("/t/m_off.png", "/t/m_on.png"),
            "the second picture of the list is the active one"
        );
        assert_eq!(mute.images.len(), 2);
        let repeat = &spec.buttons[3];
        assert_eq!(
            repeat.images,
            vec!["/t/r_off.png", "/t/r_all.png", "/t/r_single.png"],
            "three pictures or more are one per state, kept in order"
        );
        let next = &spec.buttons[0];
        assert_eq!(
            (next.name.as_str(), next.x, next.y, next.w, next.h),
            ("nextmeter", 60, 20, 0, 0)
        );
        assert_eq!(next.image, "/t/next.png");
        assert_eq!(next.action, ButtonAction::MeterNext);
        let play = &spec.buttons[1];
        assert_eq!(
            (play.w, play.h, play.action),
            (40, 40, ButtonAction::Toggle)
        );
        assert!(play.image.is_empty());
        let plain = meter_indicators(
            "[m]\nconfig.extend = True\ntouch.margin = 0\nvolume.pos = 1,1\nvolume.size = 10,10\n",
            "m",
            "/t",
        )
        .expect("extended");
        assert_eq!(spec.touch_margin, 24, "the usual finger");
        assert!(!plain.interactive);
        assert_eq!(plain.touch_margin, 0);
        assert!(plain.buttons.is_empty());
        assert_eq!(ButtonAction::parse("Prev"), Some(ButtonAction::Previous));
        assert_eq!(ButtonAction::parse("shuffle"), Some(ButtonAction::Random));
        assert_eq!(ButtonAction::parse("fly"), None);
    }

    #[test]
    fn the_display_setting_says_theme_on_or_off() {
        assert_eq!(
            run_settings("[current]\n").interactive,
            InteractiveMode::Theme
        );
        assert_eq!(
            run_settings("[current]\ntouch.interactive = on\n").interactive,
            InteractiveMode::On
        );
        assert_eq!(
            run_settings("[current]\ntouch.interactive = Off\n").interactive,
            InteractiveMode::Off
        );
        assert_eq!(
            run_settings("[current]\ntouch.interactive = theme\n").interactive,
            InteractiveMode::Theme
        );
    }
}

#[cfg(test)]
mod look_tests {
    use super::*;

    const SECTION: &str = "[studio]\nstyle = bars\nbins = 128\nchannels = 2\nscale = log\nlayout = dual-vertical\npalette = ember\npalette.split = True\ncolor.mode = index\nbar.space = 0.2\nbar.round = True\nbar.outline = True\nline.width = 1.5\nfill.alpha = 0.3\nled = True\nled.max = 24\nled.space = 0.3, 2\nled.true = True\nlumi = False\nalpha = True\nmirror = -1\npeaks = True\npeaks.hold = 400\npeaks.fade = 600\ngravity = 4\nreflex = 0.25\nreflex.alpha = 0.2\nreflex.bright = 0.7\nreflex.fit = False\nrange = 30, 16000\nlevel.range = -70, -20\nlevel.scale = linear\nlevel.boost = 2\nweighting = A\nsmoothing = 0.6\nbgr.alpha = 0\nbar.width = 4\nbar.height = 100\n\n[plain]\nbar.width = 4\nbar.height = 100\n\n[old]\nstyle = legacy\nbar.width = 4\nbar.height = 100\n";

    fn settings() -> SpectrumSettings {
        SpectrumSettings {
            base_folder: String::new(),
            folder: String::new(),
            bins: 20,
            max_value: 100.0,
        }
    }

    #[test]
    fn an_analyser_section_reads_its_keys_and_a_plain_one_has_no_look() {
        let spec = spectrum_from_theme(SECTION, "studio", (1200, 560), &settings(), "").unwrap();
        let look = spec.look.expect("a look");
        assert_eq!(look.style, LookStyle::Bars);
        assert_eq!(look.layout, Layout::DualVertical);
        assert_eq!(look.palette.name, "ember");
        assert_eq!(look.palette.stops.len(), 5);
        assert!(look.palette_split);
        assert_eq!(look.color_mode, ColorMode::Index);
        assert_eq!(look.bar_space, 0.2);
        assert!(look.round && look.outline && look.led && look.led_true && look.alpha_bars);
        assert!(!look.lumi);
        assert_eq!(look.line_width, 1.5);
        assert_eq!(look.fill_alpha, 0.3);
        assert_eq!(look.led_max, 24);
        assert_eq!(look.led_space, (0.3, 2.0));
        assert_eq!(look.mirror, -1);
        assert!(look.peaks);
        assert_eq!(look.peak_hold_ms, 400);
        assert_eq!(look.peak_fade_ms, Some(600));
        assert_eq!(look.gravity, 4.0);
        assert_eq!(look.reflex, 0.25);
        assert_eq!(look.reflex_alpha, 0.2);
        assert_eq!(look.reflex_bright, 0.7);
        assert!(!look.reflex_fit);
        assert_eq!(look.range, (30.0, 16000.0));
        assert_eq!(look.level_range, (-70.0, -20.0));
        assert!(look.level_linear);
        assert_eq!(look.level_boost, 2.0);
        assert_eq!(look.weighting, Weighting::A);
        assert_eq!(look.smoothing, 0.6);
        assert_eq!(look.bgr_alpha, 0.0);
        assert_eq!(
            spec.demand,
            Some(bank::Demand::new(128, 2, bank::Scale::Log))
        );
        let plain = spectrum_from_theme(SECTION, "plain", (100, 50), &settings(), "").unwrap();
        assert!(plain.look.is_none());
        let old = spectrum_from_theme(SECTION, "old", (100, 50), &settings(), "").unwrap();
        assert!(old.look.is_none(), "legacy is the engine's bars");
        let defaults = Look::default();
        assert_eq!(defaults.palette.name, "classic");
        assert_eq!(defaults.peak_hold_ms, 500);
        assert_eq!(defaults.gravity, 3.8);
        assert_eq!(defaults.level_range, (-60.0, 0.0));
    }

    #[test]
    fn a_palette_parses_names_and_stops_and_gives_colours_by_place_and_level() {
        let own = Palette::parse("#ff0000, (0,255,0)@0.5, #0000ff80/0.8").expect("stops");
        assert_eq!(own.stops.len(), 3);
        assert_eq!(own.stops[0].color, [255, 0, 0, 255]);
        assert_eq!(own.stops[1].pos, Some(0.5));
        assert_eq!(own.stops[2].color, [0, 0, 255, 128]);
        assert_eq!(own.stops[2].level, Some(0.8));
        assert_eq!(own.at(0.0), [255, 0, 0, 255]);
        assert_eq!(own.at(0.5), [0, 255, 0, 255]);
        assert_eq!(own.at(1.0), [0, 0, 255, 128]);
        let mid = own.at(0.25);
        assert!(
            mid[0] > 100 && mid[1] > 100,
            "between red and green: {mid:?}"
        );
        assert_eq!(
            own.for_level(0.9),
            [0, 0, 255, 128],
            "the stop at that level"
        );
        assert_eq!(
            own.for_level(0.2),
            own.at(0.2),
            "no stop yet: along the palette"
        );
        assert_eq!(Palette::named("prism").unwrap().stops.len(), 6);
        assert_eq!(Palette::parse(" Rainbow ").unwrap().name, "rainbow");
        assert!(Palette::parse("nonsense").is_none());
        assert!(Palette::parse("").is_none());
    }

    #[test]
    fn the_weighting_curves_are_flat_at_one_kilohertz_and_shaped_elsewhere() {
        for w in [
            Weighting::A,
            Weighting::B,
            Weighting::C,
            Weighting::D,
            Weighting::Itu468,
        ] {
            assert!(
                w.gain_db(1_000.0).abs() < 0.25,
                "{w:?} at 1 kHz: {}",
                w.gain_db(1_000.0)
            );
        }
        assert!((Weighting::A.gain_db(100.0) + 19.1).abs() < 0.5);
        assert!((Weighting::C.gain_db(100.0) + 0.3).abs() < 0.3);
        assert!((Weighting::Itu468.gain_db(6_300.0) - 12.2).abs() < 0.1);
        assert!(Weighting::Itu468.gain_db(20.0) < -29.0);
        assert_eq!(Weighting::None.gain_db(50.0), 0.0);
        assert_eq!(Weighting::parse("468"), Some(Weighting::Itu468));
        assert_eq!(Weighting::parse("off"), Some(Weighting::None));
        assert_eq!(Weighting::parse("x"), None);
    }
}
