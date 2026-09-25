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
    let get = |key: &str| values.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    let number = |key: &str| get(key).and_then(|v| v.trim().parse::<u32>().ok());
    let signed = |key: &str| get(key).and_then(|v| v.trim().parse::<i32>().ok());
    let angle = |key: &str| get(key).and_then(|v| v.trim().parse::<f32>().ok());
    let flag = |key: &str| truthy(get(key));
    let kind = match get("meter.type").map(|v| v.trim().to_ascii_lowercase()).as_deref() {
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
        direction: match get("direction").map(|v| v.trim().to_ascii_lowercase()).as_deref() {
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpectrumSpec {
    /// `spectrum.x/y`: the box on screen. Everything is clipped to it.
    pub x: i32,
    pub y: i32,
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
        let n = if v % step == 0.0 { (v / step) as u32 } else { (v / step) as u32 + 1 };
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

/// `spectrum.name` and `spectrum.size` of a meter that shows a spectrum:
/// `config.extend` and `spectrum.visible` both true.
pub fn meter_spectrum(meters_txt: &str, meter: &str) -> Option<(String, u32, u32)> {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| values.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    if !truthy(get("config.extend")) || !truthy(get("spectrum.visible")) {
        return None;
    }
    let name = get("spectrum.name")?.trim().to_string();
    let mut size = get("spectrum.size")?.split(',');
    let w = size.next()?.trim().parse().ok()?;
    let h = size.next()?.trim().parse().ok()?;
    if name.is_empty() {
        return None;
    }
    Some((name, w, h))
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
    let get = |key: &str| values.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).filter(|v| !v.is_empty());
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
        reflection: fill("reflection.type", "reflection.color", "reflection.gradient", "reflection.filename"),
        reflection_gap: int("reflection.gap").unwrap_or(0),
        topping: match (uint("topping.height"), uint("topping.step")) {
            (Some(height), Some(step)) if height > 0 => Some((height, step)),
            _ => None,
        },
        foreground: get("fgr.filename").map(path).unwrap_or_default(),
    })
}

/// How a picture is placed in a box: kept in proportion and centred, or
/// stretched to fill it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Scale {
    #[default]
    Fit,
    Stretch,
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
pub const FOLDER_LAYER_FILES: [&str; 6] = ["back.png", "Back.png", "back.jpg", "Back.jpg", "logo.png", "Logo.png"];

/// The folder layers a meter declares: the legacy `folderlayer.*` when
/// `folderlayer.enabled` is true, then `folderlayer.1.*` to `folderlayer.5.*`,
/// each needing a position and a dimension.
pub fn meter_folder_layers(meters_txt: &str, meter: &str) -> Vec<FolderLayerSpec> {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| values.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    let font_color = get("font.color").and_then(color_triplet).unwrap_or([255, 255, 255]);
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
                .map(|list| list.split(',').map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).collect())
                .filter(|files: &Vec<String>| !files.is_empty())
                .unwrap_or_else(|| FOLDER_LAYER_FILES.iter().map(|f| f.to_string()).collect());
            Some(FolderLayerSpec {
                files,
                x,
                y,
                w,
                h,
                scale: match get(&format!("{prefix}.scale")).map(|v| v.trim().to_ascii_lowercase()).as_deref() {
                    Some("stretch") => Scale::Stretch,
                    _ => Scale::Fit,
                },
                zorder: match get(&format!("{prefix}.zorder")).map(|v| v.trim().to_ascii_lowercase()).as_deref() {
                    Some("background") => ZOrder::Background,
                    _ => ZOrder::Overlay,
                },
                border: get(&format!("{prefix}.border")).and_then(|v| v.trim().parse().ok()).unwrap_or(0),
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
            [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|ext| lower.ends_with(ext))
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
    let get = |key: &str| values.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
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
        scale: match get("fanart.scale").map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("stretch") => Scale::Stretch,
            _ => Scale::Fit,
        },
        zorder: match get("fanart.zorder").map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("overlay") => ZOrder::Overlay,
            _ => ZOrder::Background,
        },
    })
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
    #[serde(default)]
    pub spectrum: Option<SpectrumSpec>,
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
            meter: MeterSpec::default(),
            spectrum: None,
            folder_layers: Vec::new(),
            fanart: None,
            vinyl: None,
            tonearm: None,
            rotation: RotationSettings::default(),
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
            if let (true, Some(a), Some(b)) = (take, found_start.or(left_start), found_stop.or(left_stop)) {
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
        rotation: truthy(get("albumart.rotation")),
        rpm: get("albumart.rotation.speed").and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.0),
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
}

impl Default for RotationSettings {
    fn default() -> Self {
        Self { fps: 8, step: 6, speed: 1.0, direction: "cw".into() }
    }
}

pub fn rotation_settings(text: &str) -> RotationSettings {
    let quality = current_value(text, "rotation.quality").map(|v| v.to_ascii_lowercase()).unwrap_or_else(|| "medium".into());
    let custom_fps = current_value(text, "rotation.fps").and_then(|v| v.parse::<u32>().ok()).unwrap_or(8).max(1);
    let (fps, step, speed) = match quality.as_str() {
        "low" => (4, 12, 1.0),
        "high" => (15, 3, 1.0),
        "custom" => (
            custom_fps,
            (45 / custom_fps).clamp(1, 12),
            current_value(text, "rotation.speed").and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0),
        ),
        _ => (8, 6, 1.0),
    };
    RotationSettings {
        fps,
        step,
        speed,
        direction: current_value(text, "reel.direction").map(|v| v.to_ascii_lowercase()).filter(|v| v == "ccw").unwrap_or_else(|| "cw".into()),
    }
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
pub fn meter_vinyl(meters_txt: &str, meter: &str, theme_dir: &str, settings: &RotationSettings) -> Option<VinylSpec> {
    let values = section_values(meters_txt, meter);
    let get = |key: &str| values.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).map(str::trim).filter(|v| !v.is_empty());
    let ipair = |key: &str| -> Option<(i32, i32)> {
        let mut parts = get(key)?.split(',');
        Some((parts.next()?.trim().parse().ok()?, parts.next()?.trim().parse().ok()?))
    };
    let upair = |key: &str| -> Option<(u32, u32)> {
        let mut parts = get(key)?.split(',');
        Some((parts.next()?.trim().parse().ok()?, parts.next()?.trim().parse().ok()?))
    };
    let path = |file: &str| if theme_dir.is_empty() { file.to_string() } else { format!("{}/{}", theme_dir.trim_end_matches('/'), file) };
    let has_tonearm = get("tonearm.filename").is_some() && get("tonearm.pivot.screen").is_some() && get("tonearm.pivot.image").is_some();
    let mut file = get("vinyl.filename").map(str::to_string);
    let mut pos = ipair("vinyl.pos");
    let mut center = ipair("vinyl.center");
    let mut rpm = get("albumart.rotation.speed").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
    if file.is_none() && has_tonearm {
        if let (Some(reel), Some(c)) = (get("reel.left.filename"), ipair("reel.left.center")) {
            file = Some(reel.to_string());
            pos = ipair("reel.left.pos");
            center = Some(c);
        } else if let (Some(reel), Some(c)) = (get("reel.right.filename"), ipair("reel.right.center")) {
            file = Some(reel.to_string());
            pos = ipair("reel.right.pos");
            center = Some(c);
        }
        if rpm <= 0.0 {
            rpm = get("reel.rotation.speed").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        }
    }
    let file = file?;
    let center = center?;
    // `a,b` prefers `a` from the track's folder with `b` from the theme;
    // `,b` and `b` are the theme's picture alone.
    let (album_file, theme_file) = match file.split_once(',') {
        Some((album, theme)) => (album.trim().to_string(), if theme.trim().is_empty() { file.clone() } else { theme.trim().to_string() }),
        None => (String::new(), file.clone()),
    };
    let (x, y) = pos.unwrap_or((0, 0));
    let direction = get("vinyl.direction").map(|v| v.to_ascii_lowercase()).unwrap_or_else(|| settings.direction.clone());
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
    let get = |key: &str| values.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).map(str::trim).filter(|v| !v.is_empty());
    let ipair = |key: &str| -> Option<(i32, i32)> {
        let mut parts = get(key)?.split(',');
        Some((parts.next()?.trim().parse().ok()?, parts.next()?.trim().parse().ok()?))
    };
    let number = |key: &str, default: f32| get(key).and_then(|v| v.parse::<f32>().ok()).unwrap_or(default);
    let file = get("tonearm.filename")?;
    let pivot_screen = ipair("tonearm.pivot.screen")?;
    let pivot_image = ipair("tonearm.pivot.image")?;
    Some(TonearmSpec {
        file: if theme_dir.is_empty() { file.to_string() } else { format!("{}/{}", theme_dir.trim_end_matches('/'), file) },
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
    fn a_meter_spec_reads_kind_channels_angles_flips_and_bar_steps() {
        let m = "[bar]\nmeter.type = linear\nchannels = 2\nposition.regular = 9\nposition.overload = 4\nstep.width.regular = 34\n\
            step.width.overload = 20\ndirection = bottom-top\nindicator.type = single\nflip.right.x = True\nmeter.x = 770\nmeter.y = 569\n\
            [mono]\nmeter.type = circular\nchannels = 1\nmono.origin.x = 397\nmono.origin.y = 605\nmeter.x = 10\nmeter.y = 5\nleft.needle.flip = true\n\
            [pair]\nmeter.type = circular\nchannels = 2\nleft.start.angle = 40\nleft.stop.angle = -40\nright.start.angle = -40\nright.stop.angle = 40\nright.needle.flip = True\n";
        let bar = meter_spec(m, "bar");
        assert_eq!((bar.kind, bar.channels, bar.mono_at), (MeterKind::Linear, 2, None));
        let linear = bar.linear.unwrap();
        assert_eq!((linear.direction, linear.single, linear.flip_left, linear.flip_right), (Direction::BottomTop, true, false, true));
        assert_eq!(linear.masks(), [0, 34, 68, 102, 136, 170, 204, 238, 272, 306, 326, 346, 366, 386]);
        assert_eq!((linear.bar_width(0.0), linear.bar_width(0.5), linear.bar_width(1.0)), (1, 238, 386), "14 steps; 0.5 reaches step 7; full is the last mask");
        let mono = meter_spec(m, "mono");
        assert_eq!((mono.kind, mono.channels, mono.mono_at, mono.flip_left), (MeterKind::Circular, 1, Some((407, 610)), true));
        assert_eq!(mono.left_angles, None);
        let pair = meter_spec(m, "pair");
        assert_eq!((pair.left_angles, pair.right_angles, pair.flip_right), (Some((40.0, -40.0)), Some((-40.0, 40.0)), true));
        assert!(pair.visible);
        assert_eq!(meter_needle(m, "pair"), Some((40.0, -40.0, 0.0)), "the left pair stands in for missing shared angles");
    }

    #[test]
    fn a_spectrum_is_read_from_the_meter_the_settings_and_the_theme() {
        let meters = "[m]\nconfig.extend = True\nspectrum.visible = True\nspectrum.name = s.2\nspectrum.size = 1260,307\n[n]\nspectrum.visible = True\nspectrum.name = s.2\nspectrum.size = 1,1\n";
        assert_eq!(meter_spectrum(meters, "m"), Some(("s.2".into(), 1260, 307)));
        assert_eq!(meter_spectrum(meters, "n"), None, "needs config.extend");
        let settings = spectrum_settings("[current]\nspectrum = s.7\nbase.folder = /t\nspectrum.folder = 1280x720\nmax.value = 100\nsize = 20\n");
        assert_eq!(settings, SpectrumSettings { base_folder: "/t".into(), folder: "1280x720".into(), bins: 20, max_value: 100.0 });
        let theme = "[s.2]\norigin.x = 123\norigin.y = 196\nspectrum.x = 10\nspectrum.y = 224\nbgr.type = image\nbgr.filename = bgr-2.png\n\
            bar.type = image\nbar.filename = bar-2.png\nbar.width = 27\nbar.height = 210\nbar.gap = 25\nreflection.type = gradient\n\
            reflection.gradient = (0, 0, 0, 0), (0, 0, 0, 80)\nreflection.gap = 0\ntopping.height = 3\ntopping.step = 2\nfgr.filename =\nsteps = 30\n";
        let spec = spectrum_from_theme(theme, "s.2", (1260, 307), &settings, "/t/1280x720").unwrap();
        assert_eq!((spec.x, spec.y, spec.w, spec.h, spec.origin_x, spec.origin_y), (10, 224, 1260, 307, 123, 196));
        assert_eq!(spec.background, Some(Fill::Image("/t/1280x720/bgr-2.png".into())));
        assert_eq!(spec.bar, Some(Fill::Image("/t/1280x720/bar-2.png".into())));
        assert_eq!(spec.reflection, Some(Fill::Gradient(vec![[0, 0, 0, 0], [0, 0, 0, 80]])));
        assert_eq!((spec.topping, spec.foreground.as_str(), spec.step()), (Some((3, 2)), "", 7));
        // 210 / 30 = 7 px steps; a raw 50 is 105 px, exactly 15 steps; 51 rounds up to 16.
        assert_eq!((spec.bar_height(0.0), spec.bar_height(50.0), spec.bar_height(51.0), spec.bar_height(100.0)), (0, 105, 112, 210));
        assert_eq!(spectrum_from_theme(theme, "s.9", (1, 1), &settings, ""), None);
    }

    #[test]
    fn folder_layers_need_a_box_and_look_in_the_track_folder() {
        let m = "[m]\nfont.color = 1,2,3\nfolderlayer.enabled = True\nfolderlayer.pos = 40,40\nfolderlayer.dimension = 300,300\nfolderlayer.zorder = background\n\
            folderlayer.2.files = logo.png, Logo.png\nfolderlayer.2.pos = 980,40\nfolderlayer.2.dimension = 240,120\nfolderlayer.2.scale = stretch\nfolderlayer.2.border = 2\n\
            folderlayer.3.pos = 1,1\n";
        let layers = meter_folder_layers(m, "m");
        assert_eq!(layers.len(), 2, "the third has no dimension");
        assert_eq!((layers[0].x, layers[0].y, layers[0].w, layers[0].h, layers[0].zorder, layers[0].scale, layers[0].border), (40, 40, 300, 300, ZOrder::Background, Scale::Fit, 0));
        assert_eq!(layers[0].files, FOLDER_LAYER_FILES.map(String::from).to_vec());
        assert_eq!((layers[1].files.clone(), layers[1].scale, layers[1].zorder, layers[1].border, layers[1].border_color), (vec!["logo.png".to_string(), "Logo.png".to_string()], Scale::Stretch, ZOrder::Overlay, 2, [1, 2, 3]));
        let files = ["back.png".to_string(), "../x.png".to_string(), "a/b.png".to_string(), "logo.txt".to_string(), "Logo.JPG".to_string()];
        assert_eq!(folder_candidates("mnt/INTERNAL/U2/War (1983)/02. Seconds.flac", &files), ["/mnt/INTERNAL/U2/War (1983)/back.png", "/mnt/INTERNAL/U2/War (1983)/Logo.JPG"]);
        assert_eq!(folder_candidates("music-library/NAS/a/b.flac", &files)[0], "/mnt/NAS/a/back.png");
        assert!(folder_candidates("", &files).is_empty());
        assert_eq!(folder_candidates("rp2/channel@id=0", &files), ["/mnt/rp2/back.png", "/mnt/rp2/Logo.JPG"], "a stream maps under /mnt too and simply is not found");
    }

    #[test]
    fn a_fanart_slot_needs_position_and_dimension() {
        let m = "[m]\nfanart.pos = 0,0\nfanart.dimension = 1280,720\nfanart.scale = stretch\n[n]\nfanart.pos = 1,1\n";
        let slot = meter_fanart(m, "m").unwrap();
        assert_eq!((slot.x, slot.y, slot.w, slot.h, slot.scale, slot.zorder), (0, 0, 1280, 720, Scale::Stretch, ZOrder::Background));
        assert_eq!(meter_fanart(m, "n"), None);
    }

    #[test]
    fn a_turntable_meter_has_a_record_and_a_tonearm() {
        let settings = rotation_settings("[current]\nrotation.quality = custom\nrotation.fps = 25\nrotation.speed = 2\nreel.direction = ccw\n");
        assert_eq!(settings, RotationSettings { fps: 25, step: 1, speed: 2.0, direction: "ccw".into() });
        assert_eq!(rotation_settings("[current]\nrotation.quality = high\nrotation.speed = 3\n"), RotationSettings { fps: 15, step: 3, speed: 1.0, direction: "cw".into() });
        let m = "[t]\nalbumart.pos = 195,235\nalbumart.dimension = 198,198\nalbumart.rotation = True\nalbumart.rotation.speed = 30\n\
            tonearm.filename = arm.png\ntonearm.pivot.screen = 629,166\ntonearm.pivot.image = 57,129\ntonearm.angle.rest = 0\ntonearm.angle.start = -27\n\
            tonearm.angle.end = -47\ntonearm.drop.duration = 1.8\nvinyl.filename = vinyl.jpg,disc.png\nvinyl.pos = 50,92\nvinyl.center = 293,334\nvinyl.direction = cw\n\
            [r]\ntonearm.filename = arm.png\ntonearm.pivot.screen = 1,1\ntonearm.pivot.image = 1,1\nreel.left.filename = reel.png\nreel.left.pos = 5,5\nreel.left.center = 40,40\nreel.rotation.speed = 3\n";
        let vinyl = meter_vinyl(m, "t", "/th", &settings).unwrap();
        assert_eq!((vinyl.theme_file.as_str(), vinyl.album_file.as_str(), vinyl.x, vinyl.y, vinyl.center, vinyl.clockwise, vinyl.rpm), ("/th/disc.png", "vinyl.jpg", 50, 92, (293, 334), true, 60.0));
        let arm = meter_tonearm(m, "t", "/th").unwrap();
        assert_eq!((arm.file.as_str(), arm.pivot_screen, arm.pivot_image, arm.rest, arm.start, arm.end, arm.drop_s, arm.lift_s), ("/th/arm.png", (629, 166), (57, 129), 0.0, -27.0, -47.0, 1.8, 1.0));
        let art = meter_art(m, "t", "/th").unwrap();
        assert!((art.rotation, art.rpm) == (true, 30.0));
        let reel = meter_vinyl(m, "r", "", &settings).unwrap();
        assert_eq!((reel.theme_file.as_str(), reel.center, reel.rpm, reel.clockwise), ("reel.png", (40, 40), 6.0, false), "a single reel stands in, turning the default way");
        assert_eq!(meter_vinyl("[x]\nvinyl.filename = a.png\n", "x", "", &settings), None, "no centre, no record");
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
