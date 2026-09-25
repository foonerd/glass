//! FIFO bytes become these types. This station does not open devices or draw.

/// Installed meter pipe. The ALSA scope writes it.
pub const METER_FIFO: &str = "/tmp/myfifo";

/// Installed spectrum pipe. The ALSA scope writes it.
pub const SPECTRUM_FIFO: &str = "/tmp/myfifosa";

/// Bin count in the Volumio ALSA template.
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

/// Plugin `config.txt`. The UI writes `frame.rate` into `[current]`.
pub const CONFIG_TXT: &str =
    "/data/plugins/user_interface/peppy_screensaver/screensaver/peppymeter/config.txt";

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
    pub values: Vec<f32>,
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
        mode: current_value(text, "scrolling.mode").unwrap_or_default().trim().to_ascii_lowercase(),
        title: number("scrolling.speed.title"),
        artist: number("scrolling.speed.artist"),
        album: number("scrolling.speed.album"),
    }
}

/// Where the album art is drawn. The picture is stretched to `w` by `h`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        Some(h) if h > 1 => sample.min(((h as f32) * 0.45) as u32).max(10).min(sample.max(10)),
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
    let get = |key: &str| values.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    if !truthy(get("config.extend")) {
        return true;
    }
    get("meter.visible").map(|v| truthy(Some(v))).unwrap_or(true)
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
        gain_source: section_value(text, "data.source", "volume.gain.db.source").unwrap_or_default(),
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
    /// `left.x` and `left.y` from the selected meter. Absent on a theme-less frame.
    pub left_at: Option<(u32, u32)>,
    /// `right.x` and `right.y` from the selected meter.
    pub right_at: Option<(u32, u32)>,
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
    for chunk in record.chunks_exact(4) {
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
    let (folder_w, folder_h) = size_from_folder(&folder).unwrap_or((800, 480));
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
    let flush = |sections: &mut Vec<(String, String, String)>, name: &mut String, bgr: &mut String, screen: &mut String, in_section: &mut bool| {
        if *in_section && !name.is_empty() {
            sections.push((std::mem::take(name), std::mem::take(bgr), std::mem::take(screen)));
        }
        *in_section = false;
    };
    for line in meters_txt.lines() {
        let line = line.trim();
        if let Some(title) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            flush(&mut sections, &mut name, &mut bgr, &mut screen, &mut in_section);
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
    flush(&mut sections, &mut name, &mut bgr, &mut screen, &mut in_section);
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
pub fn meter_at(meters_txt: &str, meter: &str) -> (Option<(u32, u32)>, Option<(u32, u32)>) {
    let mut sections: Vec<(String, Option<(u32, u32)>, Option<(u32, u32)>)> = Vec::new();
    let mut name = String::new();
    let mut left_x = None;
    let mut left_y = None;
    let mut right_x = None;
    let mut right_y = None;
    let mut meter_x = 0u32;
    let mut meter_y = 0u32;
    let mut in_section = false;
    let flush = |sections: &mut Vec<(String, Option<(u32, u32)>, Option<(u32, u32)>)>,
                 name: &mut String,
                 left_x: &mut Option<u32>,
                 left_y: &mut Option<u32>,
                 right_x: &mut Option<u32>,
                 right_y: &mut Option<u32>,
                 meter_x: &mut u32,
                 meter_y: &mut u32,
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
        let parsed = value.trim().parse::<u32>().ok();
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
    let mut found_distance = 0.0;
    let named = meter != "random" && meter != "list" && !meter.is_empty();
    let mut take = !named;
    for line in meters_txt.lines() {
        let line = line.trim();
        if let Some(title) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if take && found_start.is_some() && found_stop.is_some() {
                return Some((found_start.unwrap(), found_stop.unwrap(), found_distance));
            }
            take = !named || title.trim() == meter;
            if take {
                found_start = None;
                found_stop = None;
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
            "distance" => found_distance = parsed.unwrap_or(0.0),
            _ => {}
        }
    }
    match (found_start, found_stop) {
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

/// Font files from `[current]`: `font.path` joined with `font.light`,
/// `font.regular`, `font.bold` and `font.italic`. `font.digi` is optional;
/// `digi_default` stands in when it is absent, and `italic_default` when
/// `font.italic` is.
pub fn fonts_from_config(text: &str, digi_default: &str, italic_default: &str) -> FontFiles {
    let base = current_value(text, "font.path").unwrap_or_default();
    let join = |file: Option<String>| -> String {
        let file = file.unwrap_or_default();
        let file = file.trim();
        if file.is_empty() {
            return String::new();
        }
        if file.starts_with('/') && !base.is_empty() {
            format!("{base}{file}")
        } else if base.is_empty() {
            file.to_string()
        } else {
            format!("{base}/{file}")
        }
    };
    let digi = current_value(text, "font.digi").unwrap_or_default();
    let italic = join(current_value(text, "font.italic"));
    FontFiles {
        light: join(current_value(text, "font.light")),
        regular: join(current_value(text, "font.regular")),
        bold: join(current_value(text, "font.bold")),
        digi: if digi.trim().is_empty() {
            digi_default.to_string()
        } else {
            digi.trim().to_string()
        },
        italic: if italic.is_empty() {
            italic_default.to_string()
        } else {
            italic
        },
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
pub fn meter_texts(meters_txt: &str, meter: &str, screen_w: u32, speeds: &ScrollSpeeds) -> MeterTexts {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| values.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    let number = |key: &str, default: u32| get(key).and_then(|v| v.parse().ok()).unwrap_or(default);
    let font_color = get("font.color").and_then(color_triplet).unwrap_or([255, 255, 255]);
    let global_max = number("playinfo.maxwidth", 0);
    let align = match get("playinfo.align").map(|w| w.trim().to_ascii_lowercase()).as_deref() {
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
    let spec = |pos_key: &str, color_key: &str, default_style: TextStyle, boxed: Option<(&str, &str)>| {
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
                get(&format!("time.{field}.font")).unwrap_or("").trim().to_string(),
            )
        } else {
            (size_of(style), String::new())
        };
        Some(TextSpec {
            x,
            y,
            style,
            size,
            color: get(&format!("time.{field}.color")).and_then(color_triplet).unwrap_or(fallback_color),
            max_width: 0,
            align: TextAlign::Left,
            speed: 0.0,
            font_file,
        })
    };
    let time = time_field("remaining", font_color);
    let time_color = time.as_ref().map(|t| t.color).unwrap_or(font_color);
    let time_elapsed = time_field("elapsed", time_color);
    let time_total = time_field("total", time_color);
    // The samplerate line takes the type colour before the font colour.
    let type_color = get("playinfo.type.color").and_then(color_triplet).unwrap_or(font_color);
    let mut sample = spec("playinfo.samplerate.pos", "playinfo.samplerate.color", TextStyle::Light, None);
    if let Some(s) = sample.as_mut() {
        if get("playinfo.samplerate.color").and_then(color_triplet).is_none() {
            s.color = type_color;
        }
        s.max_width = number("playinfo.samplerate.maxwidth", 0);
    }
    let title = spec("playinfo.title.pos", "playinfo.title.color", TextStyle::Bold, Some(("playinfo.title.maxwidth", "title")));
    let ticker = if truthy(get("playinfo.ticker")) {
        spec("playinfo.ticker.pos", "playinfo.ticker.color", TextStyle::Regular, Some(("playinfo.ticker.maxwidth", "ticker"))).map(|mut text| {
            if get("playinfo.ticker.color").and_then(color_triplet).is_none() {
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
                direction: match get("playinfo.ticker.direction").map(|w| w.trim().to_ascii_lowercase()).as_deref() {
                    Some("ltr") => ScrollDirection::Ltr,
                    _ => ScrollDirection::Rtl,
                },
                separator: get("playinfo.ticker.separator").map(|s| s.to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| " · ".to_string()),
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
        artist: spec("playinfo.artist.pos", "playinfo.artist.color", TextStyle::Light, Some(("playinfo.artist.maxwidth", "artist"))),
        album: spec("playinfo.album.pos", "playinfo.album.color", TextStyle::Light, Some(("playinfo.album.maxwidth", "album"))),
        sample,
        time,
        time_elapsed,
        time_total,
        next_title: spec("playinfo.next.title.pos", "playinfo.next.title.color", TextStyle::Regular, Some(("playinfo.next.title.maxwidth", "title"))),
        next_artist: spec("playinfo.next.artist.pos", "playinfo.next.artist.color", TextStyle::Regular, Some(("playinfo.next.artist.maxwidth", "artist"))),
        next_album: spec("playinfo.next.album.pos", "playinfo.next.album.color", TextStyle::Regular, Some(("playinfo.next.album.maxwidth", "album"))),
        ticker,
    }
}

/// The type area for the selected meter. `default_mode` is the player's
/// `playinfo.type.mode` from `[current]`, used when the meter sets none;
/// `theme_dir` resolves `albumart.mask` and the skin icons.
pub fn meter_type(meters_txt: &str, meter: &str, default_mode: Option<&str>) -> Option<TypeSpec> {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| values.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    let (x, y) = pair_pos(get("playinfo.type.pos")?)?;
    let mode_word = |word: Option<&str>| match word.map(|w| w.trim().to_ascii_lowercase()).as_deref() {
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
    let align = match get("playinfo.type.align").map(|w| w.trim().to_ascii_lowercase()).as_deref() {
        Some("left") => TypeAlign::Left,
        Some("right") => TypeAlign::Right,
        _ => TypeAlign::Center,
    };
    let font_color = get("font.color").and_then(color_triplet).unwrap_or([255, 255, 255]);
    let color = get("playinfo.type.color").and_then(color_triplet).unwrap_or(font_color);
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
    Some(TypeSpec {
        x,
        y,
        box_size,
        mode,
        align,
        color,
        font_size,
        font_style: sample_style,
    })
}

/// Album art box for the selected meter: `albumart.pos` as `x,y` and
/// `albumart.dimension` as `w,h`. Both must be present. `albumart.mask` is
/// a file in `theme_dir`; `albumart.border` is a width in pixels drawn in
/// `font.color`.
pub fn meter_art(meters_txt: &str, meter: &str, theme_dir: &str) -> Option<ArtSpec> {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| values.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
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
    let border = get("albumart.border").and_then(|v| v.trim().parse().ok()).unwrap_or(0);
    let border_color = get("font.color").and_then(color_triplet).unwrap_or([255, 255, 255]);
    Some(ArtSpec {
        x,
        y,
        w,
        h,
        mask,
        border,
        border_color,
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
        let override_size = "[current]\nmeter.folder = 480x320\nscreen.width = 1920\nscreen.height = 1080\n";
        assert_eq!(screen_from_config(override_size), (1920, 1080));
        assert_eq!(screen_from_config(""), (800, 480));
    }

    #[test]
    fn a_named_meter_uses_its_background() {
        let text = "[bar]\nbgr.filename = bar-bgr.png\nscreen.bgr =\n\n[blue]\nbgr.filename = blue-bgr.png\nscreen.bgr = blue-screen.png\n";
        assert_eq!(meter_background(text, "bar").as_deref(), Some("bar-bgr.png"));
        assert_eq!(meter_background(text, "blue").as_deref(), Some("blue-screen.png"));
        assert_eq!(meter_background(text, "random").as_deref(), Some("bar-bgr.png"));
    }

    #[test]
    fn meter_positions_come_from_the_named_section() {
        let text = "[bar]\nleft.x = 130\nleft.y = 113\nright.x = 130\nright.y = 178\n";
        assert_eq!(meter_at(text, "bar"), (Some((130, 113)), Some((130, 178))));
        let needle = "[gold]\nleft.origin.x = 333\nleft.origin.y = 305\nmeter.x = 0\nmeter.y = 124\n";
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
        let speeds = ScrollSpeeds { mode: "custom".into(), title: 8.0, artist: 10.0, album: 8.0 };
        let texts = meter_texts(text, "black-white", 1280, &speeds);
        let title = texts.title.unwrap();
        assert_eq!((title.x, title.y, title.style, title.size), (283, 138, TextStyle::Bold, 28));
        assert_eq!((title.color, title.max_width, title.speed, title.align), ([255, 237, 76], 535, 8.0, TextAlign::Left));
        let artist = texts.artist.unwrap();
        assert_eq!((artist.style, artist.size, artist.color, artist.speed), (TextStyle::Light, 25, [255, 255, 255], 10.0));
        assert_eq!(texts.sample.unwrap().style, TextStyle::Regular);
        let time = texts.time.unwrap();
        assert_eq!((time.style, time.size, time.color, time.max_width), (TextStyle::Digi, 40, [180, 180, 180], 0));
        assert!(texts.album.is_none() && texts.ticker.is_none());
        assert_eq!(meter_texts(text, "random", 1280, &speeds).title.unwrap().x, 10);
    }

    #[test]
    fn time_fields_pick_their_font_and_inherit_the_remaining_colour() {
        let text = "[m]\ntime.remaining.pos = 585,605\ntime.remaining.color = 180,180,180\ntime.remaining.font = fonts/MyDigi.ttf\n\
            time.remaining.fontsize = 32\ntime.elapsed.pos = 400,605,italic\ntime.total.pos = 520,605,digi\n\
            time.total.color = 1,2,3\nfont.size.digi = 40\nfont.size.regular = 22\n";
        let texts = meter_texts(text, "m", 1280, &ScrollSpeeds::default());
        let remaining = texts.time.unwrap();
        assert_eq!((remaining.style, remaining.size, remaining.font_file.as_str()), (TextStyle::Digi, 32, "fonts/MyDigi.ttf"));
        let elapsed = texts.time_elapsed.unwrap();
        assert_eq!((elapsed.style, elapsed.size, elapsed.color, elapsed.font_file.as_str()), (TextStyle::Regular, 22, [180, 180, 180], ""), "an unknown style word is the regular font");
        let total = texts.time_total.unwrap();
        assert_eq!((total.style, total.size, total.color), (TextStyle::Digi, 40, [1, 2, 3]));
    }

    #[test]
    fn the_selection_names_random_or_a_list_and_visibility_needs_the_extension() {
        assert_eq!(selection_from_config("[current]\nmeter = gold\n"), Selection::Named("gold".into()));
        assert_eq!(selection_from_config("[current]\nmeter = Random\n"), Selection::Random);
        assert_eq!(
            selection_from_config("[current]\nmeter = gold, red ,dash\n"),
            Selection::List(vec!["gold".into(), "red".into(), "dash".into()])
        );
        assert_eq!(random_interval_from_config("[current]\nrandom.meter.interval = 15\n"), 15);
        assert_eq!(random_interval_from_config(""), 60);
        assert!(random_change_title_from_config("[current]\nrandom.change.title = True\n"));
        assert!(!random_change_title_from_config("[current]\nrandom.change.title = False\n"));
        assert_eq!(meter_sections("[gold]\nx=1\n[ red ]\n\n[dash]\n"), ["gold", "red", "dash"]);
        let m = "[a]\nconfig.extend = True\nmeter.visible = False\n[b]\nmeter.visible = False\n[c]\nconfig.extend = True\n";
        assert!(!meter_visible(m, "a"));
        assert!(meter_visible(m, "b"), "meter.visible needs config.extend");
        assert!(meter_visible(m, "c"));
    }

    #[test]
    fn the_data_source_section_has_the_engine_defaults() {
        let spec = data_source_from_config("[current]\nmeter = x\n\n[data.source]\nvolume.max = 100.0\nvolume.gain.db = -6\nsmooth.buffer.size = 2\nstereo.algorithm = average\n");
        assert_eq!((spec.max_ui, spec.max_pipe, spec.gain_db, spec.smooth), (100.0, 100.0, -6.0, 2));
        assert_eq!((spec.stereo.as_str(), spec.mono.as_str(), spec.gain_source.as_str()), ("average", "average", ""));
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
        let skin = ScrollSpeeds { mode: "skin".into(), ..ScrollSpeeds::default() };
        let texts = meter_texts(text, "m", 1280, &skin);
        let title = texts.title.unwrap();
        assert_eq!((title.style, title.size, title.align), (TextStyle::Italic, 22, TextAlign::Center));
        assert_eq!((title.max_width, title.speed), (768, 15.0), "centred: six tenths of the screen; per-field speed");
        let artist = texts.artist.unwrap();
        assert_eq!((artist.max_width, artist.speed), (441, 25.0), "own maxwidth; meter's global speed");
        assert_eq!(texts.next_title.unwrap().max_width, 768);
        let ticker = texts.ticker.unwrap();
        assert_eq!((ticker.text.x, ticker.text.max_width, ticker.text.speed), (40, 1240, 25.0), "capped to the visible width");
        assert_eq!((ticker.direction, ticker.separator.as_str(), ticker.space_between, ticker.end_spaces), (ScrollDirection::Ltr, "-", 1, 10), "values are trimmed as the player's parser trims them; spacing comes from space_between");
        assert!(ticker.append_next && ticker.replace);
        let default_mode = ScrollSpeeds { mode: "default".into(), ..ScrollSpeeds::default() };
        assert_eq!(meter_texts(text, "m", 1280, &default_mode).title.unwrap().speed, 40.0);
        let plain = meter_texts("[m]\nplayinfo.title.pos = 100,5\n", "m", 800, &default_mode);
        assert_eq!(plain.title.unwrap().max_width, 680, "auto box: screen minus x minus margin");
    }

    #[test]
    fn album_art_box_needs_position_and_dimension() {
        let text = "[black-white]\nalbumart.pos = 36,25\nalbumart.dimension = 201,201\n";
        let art = meter_art(text, "black-white", "/themes/t").unwrap();
        assert_eq!((art.x, art.y, art.w, art.h), (36, 25, 201, 201));
        assert_eq!((art.mask.as_str(), art.border, art.border_color), ("", 0, [255, 255, 255]));
        assert_eq!(meter_art("[bar]\nalbumart.pos = 1,2\n", "bar", ""), None);
        let masked = "[v]\nalbumart.pos = 27,28\nalbumart.dimension = 432,432\nalbumart.mask = mask.png\nalbumart.border = 2\nfont.color = 10,20,30\n";
        let art = meter_art(masked, "v", "/themes/v").unwrap();
        assert_eq!((art.mask.as_str(), art.border, art.border_color), ("/themes/v/mask.png", 2, [10, 20, 30]));
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
        assert_eq!((spec.mode, spec.align, spec.color), (TypeMode::Icon, TypeAlign::Center, [204, 176, 97]));
        assert_eq!((spec.font_size, spec.font_style), (20, TextStyle::Regular));
        let meter_wins = "[m]\nplayinfo.type.pos = 1,1\nplayinfo.type.dimension = 53,53\nplayinfo.type.mode = both\nplayinfo.type.align = right\nplayinfo.type.fontsize = 18\n";
        let spec = meter_type(meter_wins, "m", Some("text")).unwrap();
        assert_eq!((spec.mode, spec.align, spec.font_size), (TypeMode::Both, TypeAlign::Right, 18));
        assert_eq!(meter_type("[m]\nplayinfo.type.pos = 1,1\nplayinfo.type.dimension = 1,1\n", "m", None), None);
        let text_only = meter_type("[m]\nplayinfo.type.pos = 5,6\nplayinfo.type.mode = text\n", "m", None).unwrap();
        assert_eq!((text_only.box_size, text_only.font_size), (None, 30));
        assert_eq!(type_font_size(20, Some(45)), 20);
        assert_eq!(type_font_size(40, Some(45)), 20);
        assert_eq!(type_font_size(40, Some(12)), 10);
    }

    #[test]
    fn font_files_join_the_path_and_default_the_clock_font() {
        let text = "[current]\nfont.path = /fonts\nfont.light = /Lato-Light.ttf\nfont.bold = Lato-Bold.ttf\n";
        let fonts = fonts_from_config(text, "/plugin/fonts/DSEG7.ttf", "/plugin/fonts/PeppyFont-Italic.ttf");
        assert_eq!(fonts.light, "/fonts/Lato-Light.ttf");
        assert_eq!(fonts.bold, "/fonts/Lato-Bold.ttf");
        assert_eq!(fonts.regular, "");
        assert_eq!(fonts.digi, "/plugin/fonts/DSEG7.ttf");
        assert_eq!(fonts.italic, "/plugin/fonts/PeppyFont-Italic.ttf");
        let own = fonts_from_config("[current]\nfont.path = /f\nfont.italic = /I.ttf\n", "", "/d/i.ttf");
        assert_eq!(own.italic, "/f/I.ttf");
        let speeds = scroll_speeds_from_config("[current]\nscrolling.mode = custom\nscrolling.speed.title = 8\nscrolling.speed.artist = 10\n");
        assert_eq!((speeds.mode.as_str(), speeds.title, speeds.artist, speeds.album), ("custom", 8.0, 10.0, 40.0));
    }
}
