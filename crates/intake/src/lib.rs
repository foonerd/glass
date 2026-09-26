//! Poll the outside world and return the latest [`lead::Input`].
//! This station does not parse skin geometry or draw.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use std::collections::{HashMap, VecDeque};

use serde_json::Value;

use lead::{
    current_value, data_source_from_config, decode_meter, decode_spectrum, folder_candidates,
    fonts_from_config, format_key, frame_rate_from_config, meter_art, meter_at, meter_background,
    meter_fanart, meter_folder_layers, meter_indicator, meter_indicators, meter_layers,
    meter_needle, meter_reels, meter_sections, meter_spec, meter_spectrum, meter_text_at,
    meter_texts, meter_tonearm, meter_type, meter_vinyl, random_change_title_from_config,
    random_interval_from_config, rotation_settings, run_settings, screen_from_config,
    scroll_speeds_from_config, selection_from_config, spectrum_from_theme, spectrum_settings,
    transition_settings, Bins, DataSourceSpec, Input, Levels, Selection, SkinDesc, TextSpec,
    DEFAULT_FRAME_RATE, DEFAULT_METER_MAX, DEFAULT_SPECTRUM_BINS, METER_CONFIG, SPECTRUM_CONFIG,
    STOCK_ICONS,
};

mod channel;
pub use channel::{Channel, Command, Event};

/// The theme's meter rotation as the player configures it: which names, in
/// what order, and what moves it on. No names means one fixed meter.
#[derive(Debug, Clone, PartialEq)]
pub struct Rotation {
    pub names: Vec<String>,
    pub random: bool,
    pub interval: Duration,
    pub on_title: bool,
}

/// Walk the rotation the way the meter engine does: random draws every name
/// once before starting over, a list cycles in order.
pub struct Selector {
    rotation: Rotation,
    remaining: Vec<String>,
    index: usize,
    seed: u64,
}

impl Selector {
    pub fn new(rotation: Rotation) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self::seeded(rotation, seed)
    }

    pub fn seeded(rotation: Rotation, seed: u64) -> Self {
        Self {
            rotation,
            remaining: Vec::new(),
            index: 0,
            seed: seed | 1,
        }
    }

    /// Whether there is anything to move on to.
    pub fn rotates(&self) -> bool {
        !self.rotation.names.is_empty()
    }

    pub fn interval(&self) -> Duration {
        self.rotation.interval
    }

    pub fn on_title(&self) -> bool {
        self.rotation.on_title
    }

    fn rand(&mut self) -> u64 {
        // xorshift64*: enough to pick a meter, no crate needed.
        let mut x = self.seed;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.seed = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// The next meter name, or `None` for a fixed meter.
    // The name is the player's word for it; the selector is no iterator.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<String> {
        if self.rotation.names.is_empty() {
            return None;
        }
        if self.rotation.random {
            if self.remaining.is_empty() {
                self.remaining = self.rotation.names.clone();
            }
            let i = (self.rand() % self.remaining.len() as u64) as usize;
            Some(self.remaining.remove(i))
        } else {
            if self.index >= self.rotation.names.len() {
                self.index = 0;
            }
            let name = self.rotation.names[self.index].clone();
            self.index += 1;
            Some(name)
        }
    }
}

/// The theme folder the configuration names, or `None` without one.
fn theme_dir_from(text: &str, config_path: &str) -> Option<PathBuf> {
    let folder = current_value(text, "meter.folder").unwrap_or_default();
    if folder.is_empty() {
        return None;
    }
    let base = current_value(text, "base.folder").unwrap_or_default();
    let root = if base.is_empty() {
        Path::new(config_path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(base)
    };
    Some(root.join(folder))
}

/// The meter configuration file: `GLASS_CONFIG`, or the one under the
/// plugin's home. The spectrum configuration, the fonts and the icon set
/// are found from it: `spectrum.txt` beside it, `fonts` and `format-icons`
/// one level up.
fn config_path() -> String {
    std::env::var("GLASS_CONFIG").unwrap_or_else(|_| {
        lead::home()
            .join(METER_CONFIG)
            .to_string_lossy()
            .into_owned()
    })
}

/// The spectrum configuration beside the meter configuration.
/// The plugin's channel socket: `GLASS_CHANNEL`, or the default path.
fn channel_path() -> PathBuf {
    std::env::var_os(lead::CHANNEL_VAR)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(lead::CHANNEL_PATH))
}

fn spectrum_config_path() -> PathBuf {
    let config = PathBuf::from(config_path());
    match config.parent() {
        Some(dir) => dir.join(Path::new(SPECTRUM_CONFIG).file_name().unwrap_or_default()),
        None => lead::home().join(SPECTRUM_CONFIG),
    }
}

/// Values that stand in for the installed configuration's `[current]`
/// theme, meter and rotation interval, for reviewing themes without
/// touching the player's files.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overrides {
    pub theme: Option<String>,
    pub meter: Option<String>,
    /// Seconds between meters; also turns title-driven rotation off.
    pub interval: Option<u32>,
    /// Frames a second in place of `frame.rate`.
    pub fps: Option<u32>,
}

static OVERRIDES: std::sync::Mutex<Option<Overrides>> = std::sync::Mutex::new(None);

/// Apply overrides to every later read of the installed configuration.
pub fn set_overrides(overrides: Overrides) {
    if let Ok(mut slot) = OVERRIDES.lock() {
        *slot = Some(overrides);
    }
}

/// The `[current]` section with one key set to a value, added when absent.
fn with_current(text: &str, key: &str, value: &str) -> String {
    let mut out = String::new();
    let mut in_current = false;
    let mut written = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if in_current && !written {
                out.push_str(&format!("{key} = {value}\n"));
                written = true;
            }
            in_current = trimmed.eq_ignore_ascii_case("[current]");
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_current
            && trimmed
                .split_once('=')
                .is_some_and(|(k, _)| k.trim() == key)
        {
            if !written {
                out.push_str(&format!("{key} = {value}\n"));
                written = true;
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !written {
        if in_current {
            out.push_str(&format!("{key} = {value}\n"));
        } else {
            out.push_str(&format!("[current]\n{key} = {value}\n"));
        }
    }
    out
}

/// The installed configuration's text with the overrides applied.
fn config_text() -> Option<String> {
    let mut text = std::fs::read_to_string(config_path()).ok()?;
    let overrides = OVERRIDES.lock().ok().and_then(|slot| slot.clone());
    if let Some(overrides) = overrides {
        if let Some(theme) = &overrides.theme {
            text = with_current(&text, "meter.folder", theme);
        }
        if let Some(meter) = &overrides.meter {
            text = with_current(&text, "meter", meter);
        }
        if let Some(interval) = overrides.interval {
            text = with_current(&text, "random.meter.interval", &interval.to_string());
            text = with_current(&text, "random.change.title", "False");
        }
        if let Some(fps) = overrides.fps {
            text = with_current(&text, "frame.rate", &fps.to_string());
        }
    }
    Some(text)
}

/// Every theme folder under the installed `base.folder`, each with the
/// names of its meters, in folder order.
pub fn installed_themes() -> Vec<(String, Vec<String>)> {
    let Some(text) = config_text() else {
        return Vec::new();
    };
    let path = config_path();
    let base = current_value(&text, "base.folder")
        .filter(|b| !b.is_empty())
        .map(PathBuf::from)
        .or_else(|| Path::new(&path).parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let Ok(entries) = std::fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut themes: Vec<(String, Vec<String>)> = entries
        .flatten()
        .filter(|e| e.path().join("meters.txt").is_file())
        .map(|e| {
            let sections = std::fs::read_to_string(e.path().join("meters.txt"))
                .map(|m| meter_sections(&m))
                .unwrap_or_default();
            (e.file_name().to_string_lossy().into_owned(), sections)
        })
        .collect();
    themes.sort();
    themes
}

/// The meters of the installed theme, after overrides.
pub fn installed_meter_names() -> Vec<String> {
    let Some(text) = config_text() else {
        return Vec::new();
    };
    theme_dir_from(&text, &config_path())
        .and_then(|dir| std::fs::read_to_string(dir.join("meters.txt")).ok())
        .map(|meters| meter_sections(&meters))
        .unwrap_or_default()
}

/// The meter rotation from the installed configuration. `random` walks every
/// section of the theme's meters file.
pub fn installed_rotation() -> Rotation {
    let path = config_path();
    let text = config_text().unwrap_or_default();
    let (names, random) = match selection_from_config(&text) {
        Selection::Named(_) => (Vec::new(), false),
        Selection::List(names) => (names, false),
        Selection::Random => {
            let sections = theme_dir_from(&text, &path)
                .and_then(|dir| std::fs::read_to_string(dir.join("meters.txt")).ok())
                .map(|meters| meter_sections(&meters))
                .unwrap_or_default();
            (sections, true)
        }
    };
    Rotation {
        names,
        random,
        interval: Duration::from_secs(u64::from(random_interval_from_config(&text))),
        on_title: random_change_title_from_config(&text),
    }
}

/// Where the player keeps the fanart it resolves; a reference the player
/// answers is a path under here.
pub const PLUGINS_DIR: &str = "/data/plugins";

/// What the player answers for an artist's fanart.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
pub struct FanartAnswer {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub images: Vec<String>,
    #[serde(default)]
    pub interval_ms: u64,
    #[serde(default)]
    pub transition: String,
    #[serde(default)]
    pub transition_ms: u64,
    #[serde(default)]
    pub order: String,
}

/// Ask the player for an artist's fanart set and the slideshow settings.
fn fanart_list(artist: &str, uri: &str) -> FanartAnswer {
    #[derive(serde::Deserialize)]
    struct Outer {
        #[serde(default)]
        success: bool,
        #[serde(default)]
        data: Option<FanartAnswer>,
    }
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .new_agent();
    let body = serde_json::json!({
        "endpoint": "glass_artistfanart",
        "data": { "artist": artist, "uri": uri },
    });
    let Ok(mut response) = agent
        .post("http://localhost:3000/api/v1/pluginEndpoint")
        .header("Content-Type", "application/json")
        .send(body.to_string().as_bytes())
    else {
        return FanartAnswer::default();
    };
    let Ok(text) = response.body_mut().read_to_string() else {
        return FanartAnswer::default();
    };
    match serde_json::from_str::<Outer>(&text) {
        Ok(Outer {
            success: true,
            data: Some(answer),
        }) if answer.success => answer,
        Ok(Outer {
            data: Some(answer), ..
        }) => FanartAnswer {
            images: Vec::new(),
            ..answer
        },
        _ => FanartAnswer::default(),
    }
}

/// The file for a fanart reference: the player's own copy when it is there,
/// else fetched through the player and kept beside the album art.
fn fanart_file(reference: &str) -> String {
    let local = Path::new(PLUGINS_DIR).join(reference);
    if local.is_file() {
        return local.to_string_lossy().into_owned();
    }
    fetch_art(&format!(
        "http://localhost:3000/albumart?sectionimage={reference}"
    ))
    .map(|p| p.to_string_lossy().into_owned())
    .unwrap_or_default()
}

fn xorshift(seed: &mut u64) -> u64 {
    let mut x = *seed | 1;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *seed = x;
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Why {
    Artist,
    Track,
    Interval,
}

/// The artist fanart slideshow as the player's own handler runs it: the set
/// is asked of the player when the artist changes, asked again on every track
/// and every timed interval, one picture forward per track and per interval,
/// in order or at random, with the position remembered per artist and set.
#[derive(Default)]
pub struct Slideshow {
    artist_key: String,
    track_key: String,
    refs: Vec<String>,
    index: usize,
    interval_ms: u64,
    transition: String,
    transition_ms: u64,
    order: String,
    last_advance: Option<Instant>,
    file: String,
    prev_file: String,
    transition_started: Option<Instant>,
    pending: Option<(Why, mpsc::Receiver<FanartAnswer>)>,
    /// Position and last advance per artist and picture set.
    memory: HashMap<String, (usize, Option<Instant>)>,
    seed: u64,
}

impl Slideshow {
    fn ask(&mut self, why: Why, artist: &str, uri: &str) {
        if self.pending.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let (artist, uri) = (artist.to_string(), uri.to_string());
        thread::spawn(move || {
            let _ = tx.send(fanart_list(&artist, &uri));
        });
        self.pending = Some((why, rx));
    }

    fn memory_key(&self) -> String {
        format!("{}\0{}", self.artist_key, self.refs.join("\0"))
    }

    fn begin_transition(&mut self) {
        if matches!(self.transition.as_str(), "fade" | "merge") {
            self.prev_file = std::mem::take(&mut self.file);
            self.transition_started = Some(Instant::now());
        } else {
            self.prev_file.clear();
            self.transition_started = None;
        }
    }

    fn show(&mut self, index: usize) {
        if index >= self.refs.len() {
            self.file.clear();
            self.index = 0;
            return;
        }
        self.index = index;
        self.file = fanart_file(&self.refs[index]);
        let key = self.memory_key();
        self.memory.insert(key, (index, self.last_advance));
        while self.memory.len() > 20 {
            let first = self.memory.keys().next().cloned().unwrap();
            self.memory.remove(&first);
        }
    }

    fn start_index(&mut self) -> usize {
        let n = self.refs.len();
        if n == 0 {
            return 0;
        }
        if let Some((index, _)) = self.memory.get(&self.memory_key()) {
            return (*index).min(n - 1);
        }
        if self.order == "random" && n > 1 {
            (xorshift(&mut self.seed) % n as u64) as usize
        } else {
            0
        }
    }

    fn advance(&mut self) {
        let n = self.refs.len();
        if n <= 1 {
            return;
        }
        self.begin_transition();
        let next = if self.order == "random" {
            if n == 2 {
                1 - self.index
            } else {
                let mut pick = (xorshift(&mut self.seed) % (n as u64 - 1)) as usize;
                if pick >= self.index {
                    pick += 1;
                }
                pick
            }
        } else {
            (self.index + 1) % n
        };
        self.last_advance = Some(Instant::now());
        self.show(next);
    }

    fn interval_elapsed(&self) -> bool {
        self.interval_ms > 0
            && self
                .last_advance
                .is_none_or(|at| at.elapsed().as_millis() as u64 >= self.interval_ms)
    }

    fn take_answer(&mut self) {
        let Some((why, rx)) = &self.pending else {
            return;
        };
        let answer = match rx.try_recv() {
            Ok(answer) => answer,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => FanartAnswer::default(),
        };
        let why = *why;
        self.pending = None;
        self.interval_ms = answer.interval_ms;
        self.transition = if matches!(answer.transition.as_str(), "fade" | "merge") {
            answer.transition.clone()
        } else {
            "none".into()
        };
        self.transition_ms = answer.transition_ms.max(50);
        self.order = if answer.order == "random" {
            "random".into()
        } else {
            "sequential".into()
        };
        match why {
            Why::Artist => {
                self.refs = answer.images;
                if self.refs.is_empty() {
                    self.file.clear();
                    return;
                }
                let remembered = self.memory.get(&self.memory_key()).map(|(_, at)| *at);
                self.last_advance = remembered.unwrap_or(Some(Instant::now()));
                self.prev_file.clear();
                self.transition_started =
                    matches!(self.transition.as_str(), "fade" | "merge").then(Instant::now);
                let start = self.start_index();
                self.show(start);
                if self.refs.len() > 1 && self.interval_elapsed() {
                    self.advance();
                }
            }
            Why::Track | Why::Interval => {
                if answer.images != self.refs {
                    self.refs = answer.images;
                    self.transition_started = None;
                    self.prev_file.clear();
                    if self.refs.is_empty() {
                        self.file.clear();
                        self.index = 0;
                        return;
                    }
                    self.last_advance = Some(Instant::now());
                    if !self.file.is_empty() {
                        self.begin_transition();
                    }
                    let start = self.start_index();
                    self.show(start);
                } else if self.refs.len() > 1 {
                    self.advance();
                }
            }
        }
    }

    /// Once a second, with the player's artist and track location.
    pub fn update(&mut self, artist: &str, uri: &str) {
        self.take_answer();
        let key = artist.trim().to_ascii_lowercase();
        if key != self.artist_key {
            self.artist_key = key;
            self.track_key = uri.to_string();
            self.refs.clear();
            self.index = 0;
            self.file.clear();
            self.prev_file.clear();
            self.transition_started = None;
            self.pending = None;
            if !self.artist_key.is_empty() {
                self.ask(Why::Artist, artist, uri);
            }
            return;
        }
        if uri != self.track_key {
            self.track_key = uri.to_string();
            self.ask(Why::Track, artist, uri);
            return;
        }
        if self.refs.len() > 1 && self.interval_elapsed() {
            self.ask(Why::Interval, artist, uri);
        }
    }

    /// The picture on show, the one it replaces, and the transition's mode,
    /// length and progress in milliseconds. Every frame.
    pub fn snapshot(&mut self) -> (String, String, String, u32, u32) {
        self.take_answer();
        let duration = self.transition_ms.max(50) as u32;
        let elapsed = match self.transition_started {
            Some(at) => at.elapsed().as_millis().min(u32::MAX as u128) as u32,
            None => duration,
        };
        if elapsed >= duration {
            self.transition_started = None;
            self.prev_file.clear();
        }
        (
            self.file.clone(),
            self.prev_file.clone(),
            self.transition.clone(),
            duration,
            elapsed.min(duration),
        )
    }
}

/// The plugin's persist file: `duration:start_ms:mode`.
pub const PERSIST_FILE: &str = "/tmp/glass_persist";

/// Persist mode and seconds left, from the plugin's file and the wall clock.
/// Empty mode and zero when the file is absent or malformed.
pub fn persist_state(path: &str, now_epoch_ms: u64) -> (String, u32) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return (String::new(), 0);
    };
    let mut parts = text.trim().split(':');
    let (Some(duration), Some(start)) = (parts.next(), parts.next()) else {
        return (String::new(), 0);
    };
    let (Ok(duration), Ok(start)) = (duration.trim().parse::<u64>(), start.trim().parse::<u64>())
    else {
        return (String::new(), 0);
    };
    let mode = parts.next().unwrap_or("freeze").trim().to_ascii_lowercase();
    let elapsed = now_epoch_ms.saturating_sub(start) / 1000;
    (mode, duration.saturating_sub(elapsed) as u32)
}

/// Turn the pipe's raw levels into UI levels the way the meter engine does:
/// scale from the pipe's full scale to the UI's, apply the gain in dB and the
/// live gain file, run the stereo algorithm against the previous values,
/// average the last `smooth` snapshots, and derive mono.
pub struct Conditioner {
    spec: DataSourceSpec,
    gain_mult: f32,
    source_mult: f32,
    source_at: Option<Instant>,
    previous: Levels,
    window: VecDeque<Levels>,
}

impl Conditioner {
    pub fn new(spec: DataSourceSpec) -> Self {
        let gain_mult = 10f32.powf(spec.gain_db / 20.0);
        // The engine's buffer starts full of silence, so the first readings
        // rise over `smooth` snapshots.
        let window = std::iter::repeat_n(Levels::default(), spec.smooth).collect();
        Self {
            spec,
            gain_mult,
            source_mult: 1.0,
            source_at: None,
            previous: Levels::default(),
            window,
        }
    }

    fn refresh_source(&mut self) {
        if self.spec.gain_source.is_empty() {
            return;
        }
        if self
            .source_at
            .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.source_at = Some(Instant::now());
        self.source_mult = std::fs::read_to_string(&self.spec.gain_source)
            .ok()
            .and_then(|t| t.trim().parse::<f32>().ok())
            .map(|db| 10f32.powf(db / 20.0))
            .unwrap_or(1.0);
    }

    fn channel(&self, previous: f32, new: f32) -> f32 {
        match self.spec.stereo.as_str() {
            "average" => (previous + new) / 2.0,
            "logarithm" => {
                if previous == 0.0 {
                    0.0
                } else {
                    let db = (20.0 * (new / previous).log10()).clamp(-20.0, 3.0);
                    (db + 20.0) * (100.0 / 23.0)
                }
            }
            _ => new,
        }
    }

    /// One raw stereo record from the pipe becomes the levels to plot.
    pub fn condition(&mut self, raw_left: u16, raw_right: u16) -> Levels {
        self.refresh_source();
        let gain = self.gain_mult * self.source_mult;
        let scale =
            |raw: u16| (self.spec.max_ui * (f32::from(raw) / self.spec.max_pipe) * gain).floor();
        let new_left = scale(raw_left);
        let new_right = scale(raw_right);
        let new_mono = match self.spec.mono.as_str() {
            "maximum" => new_left.max(new_right),
            _ => (new_left + new_right) / 2.0,
        };
        let mut levels = Levels {
            left: self.channel(self.previous.left, new_left),
            right: self.channel(self.previous.right, new_right),
            mono: self.channel(self.previous.mono, new_mono),
        };
        if self.spec.smooth > 0 {
            self.window.push_back(levels);
            self.window.pop_front();
            let n = self.spec.smooth as f32;
            levels = Levels {
                left: self.window.iter().map(|l| l.left).sum::<f32>() / n,
                right: self.window.iter().map(|l| l.right).sum::<f32>() / n,
                mono: self.window.iter().map(|l| l.mono).sum::<f32>() / n,
            };
        }
        self.previous = levels;
        levels
    }
}

/// A time field's font file as the player finds it: an absolute path that
/// exists, else the file inside the theme folder, else under `font.path`.
/// Not found means the clock font, so the field is left empty.
pub fn resolve_time_font(spec: &mut TextSpec, theme_dir: &str, font_path: &str) {
    if spec.font_file.is_empty() {
        return;
    }
    let value = spec.font_file.clone();
    let mut candidates = Vec::new();
    if value.starts_with('/') {
        candidates.push(PathBuf::from(&value));
    }
    for base in [theme_dir, font_path] {
        if !base.is_empty() {
            candidates
                .push(Path::new(base.trim_end_matches('/')).join(value.trim_start_matches('/')));
        }
    }
    spec.font_file = candidates
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
}

/// One HTTP GET against the player on localhost. `None` when it does not answer.
fn player_get(path: &str) -> Option<String> {
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(200)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    stream
        .write_all(
            format!("GET {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .ok()?;
    let mut buf = String::new();
    stream.read_to_string(&mut buf).ok()?;
    Some(
        buf.split_once("\r\n\r\n")
            .map(|(_, body)| body.to_string())
            .unwrap_or(buf),
    )
}

/// The track after `position` in the player's queue: title (or name),
/// artist, album. Empty strings when there is none or the player does not answer.
pub fn queue_next(position: i64) -> (String, String, String) {
    let Some(body) = player_get("/api/v1/getQueue") else {
        return Default::default();
    };
    let value: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return Default::default(),
    };
    let items = value
        .as_array()
        .or_else(|| value.get("queue").and_then(|q| q.as_array()));
    let Some(items) = items else {
        return Default::default();
    };
    let next = usize::try_from(position + 1)
        .ok()
        .and_then(|i| items.get(i));
    let Some(next) = next else {
        return Default::default();
    };
    let text = |key: &str| {
        next.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let title = {
        let t = text("title");
        if t.is_empty() {
            text("name")
        } else {
            t
        }
    };
    (title, text("artist"), text("album"))
}

/// The length in seconds of every track in the player's queue, in order.
pub fn queue_lengths() -> Vec<f32> {
    let Some(body) = player_get("/api/v1/getQueue") else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&body) else {
        return Vec::new();
    };
    let items = value
        .as_array()
        .or_else(|| value.get("queue").and_then(|q| q.as_array()));
    items
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item.get("duration")
                        .and_then(|d| d.as_f64())
                        .unwrap_or(0.0)
                        .max(0.0) as f32
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A file named `basename` in `dir`, matched exactly first, then ignoring
/// case. Volumio ships `YouTube.svg`, which a case-sensitive open misses.
pub fn existing_icon_file(dir: &str, basename: &str) -> Option<PathBuf> {
    if dir.is_empty() || basename.is_empty() {
        return None;
    }
    let exact = Path::new(dir).join(basename);
    if exact.is_file() {
        return Some(exact);
    }
    let wanted = basename.to_ascii_lowercase();
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.to_ascii_lowercase() == wanted)
        })
}

/// The icon for a key: the skin's `format-icons` (`.png` then `.svg`), the
/// player's own set (`.svg`), then Volumio's stock set. Empty when none.
pub fn resolve_icon(key: &str, skin_icons: &str, plugin_icons: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    for ext in [".png", ".svg"] {
        if let Some(found) = existing_icon_file(skin_icons, &format!("{key}{ext}")) {
            return found.to_string_lossy().into_owned();
        }
    }
    if let Some(found) = existing_icon_file(plugin_icons, &format!("{key}.svg")) {
        return found.to_string_lossy().into_owned();
    }
    existing_icon_file(STOCK_ICONS, &format!("{key}.svg"))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Album art location as the player reports it, made fetchable. A leading
/// slash is a path served by the player itself; anything else is used as is.
pub fn art_url(reported: &str) -> String {
    if reported.starts_with('/') {
        format!("http://127.0.0.1:3000{reported}")
    } else {
        reported.to_string()
    }
}

fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Fetch one picture into the art cache under the temp dir. `None` when the
/// player does not answer, the answer is not an image, or the file cannot be
/// written. A picture fetched earlier for the same location is reused.
fn fetch_art(reported: &str) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join("glass-art");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("{:016x}.img", fnv1a(reported)));
    if path.is_file() {
        return Some(path);
    }
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .build()
        .new_agent();
    let mut response = agent.get(art_url(reported)).call().ok()?;
    let kind = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !kind.contains("image") {
        return None;
    }
    let bytes = response.body_mut().read_to_vec().ok()?;
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// Fetches the picture for the current album art location in the background
/// and remembers the file it landed in. One fetch runs at a time; a location
/// that changes meanwhile is fetched once the running one returns.
#[derive(Default)]
struct ArtFetcher {
    wanted: String,
    have_url: String,
    have_file: String,
    pending: Option<(String, mpsc::Receiver<Option<PathBuf>>)>,
}

impl ArtFetcher {
    fn want(&mut self, reported: &str) {
        self.wanted = reported.to_string();
    }

    /// The file for the wanted location, or empty while it is not there yet.
    fn file(&mut self) -> String {
        if let Some((url, rx)) = &self.pending {
            match rx.try_recv() {
                Ok(result) => {
                    self.have_url = url.clone();
                    self.have_file = result
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.pending = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.pending = None,
            }
        }
        if self.pending.is_none() && self.wanted != self.have_url {
            if self.wanted.is_empty() {
                self.have_url.clear();
                self.have_file.clear();
            } else {
                let (tx, rx) = mpsc::channel();
                let reported = self.wanted.clone();
                thread::spawn(move || {
                    let _ = tx.send(fetch_art(&reported));
                });
                self.pending = Some((self.wanted.clone(), rx));
            }
        }
        if self.wanted == self.have_url {
            self.have_file.clone()
        } else {
            String::new()
        }
    }
}

/// A source of snapshots. The player polls FIFOs. The remote will poll UDP.
pub trait Source {
    fn poll(&mut self) -> Input;
}

/// Placeholder source used when no pipe is open.
#[derive(Debug, Default)]
pub struct IdleSource;

impl Source for IdleSource {
    fn poll(&mut self) -> Input {
        Input::default()
    }
}

/// The tap's ring and the player's state: what the display shows.
pub struct TapSource {
    /// The live ring, found under `/dev/shm` and looked for again when it goes.
    ring: Option<tap::Reader>,
    ring_looked_at: Option<Instant>,
    last_seq: u64,
    last_frames: u64,
    /// The meter's fall, as the old scope shaped it, on the pipe scale.
    decay: tap::legacy::Meter,
    /// The theme's bins from the raw spectrum, on the old logarithmic mapping.
    bins_mapper: tap::legacy::Spectrum,
    spectrum_max: u32,
    spectrum_bins: usize,
    spectrum_held: Vec<f32>,
    levels_held: Levels,
    metadata_held: lead::Metadata,
    metadata_at: Option<Instant>,
    /// How often the player is asked for now-playing text. `None` never asks.
    metadata_every: Option<Duration>,
    /// Position the player reported at `metadata_at`, in seconds.
    seek_polled: f32,
    art: ArtFetcher,
    /// Icon directories from the skin, for resolving the type icon.
    skin_icons: String,
    plugin_icons: String,
    /// Last resolved (key, icon path), so the directories are read once per key.
    icon_cache: (String, String),
    /// Whether the skin shows the next track, which costs a queue read per refresh.
    wants_next: bool,
    conditioner: Conditioner,
    /// The skin's folder layers' file lists, and the files found for the
    /// current track folder, resolved once per folder.
    folder_layers: Vec<Vec<String>>,
    folder_key: String,
    folder_files: Vec<String>,
    /// The fanart slideshow, run only when the skin has a slot.
    fanart: Option<Slideshow>,
    /// The record's album file name to look for in the track folder, and the file found.
    vinyl_album_file: String,
    vinyl_key: String,
    vinyl_file: String,
    /// The reels' album file names and the files found.
    reel_album_files: (String, String),
    reel_key: String,
    reel_files: (String, String),
    /// Queue mode: the queue's track lengths, read every ten seconds.
    queue_mode: bool,
    queue_lengths: Vec<f32>,
    queue_read_at: Option<Instant>,
    /// The plugin's channel, when the player runs under it: the state
    /// arrives as it changes and the player is not asked.
    channel: Option<Channel>,
    channel_was_live: bool,
    /// Infinity playback, which only the channel reports.
    infinity_held: bool,
    /// The last state, and whether the skin's needs are to be derived from
    /// it again.
    playing_held: NowPlaying,
    rederive: bool,
}

/// The hops that arrived between two looks at the ring, as one: the peaks,
/// RMS and bins take the highest of them, the count and sequence the
/// latest, so a transient inside one frame of the display still shows.
fn merge_hops(hops: impl Iterator<Item = tap::Frame>) -> Option<tap::Frame> {
    let mut merged: Option<tap::Frame> = None;
    for frame in hops {
        match merged.as_mut() {
            None => merged = Some(frame),
            Some(m) => {
                for ch in 0..tap::MAX_CHANNELS {
                    m.peak[ch] = m.peak[ch].max(frame.peak[ch]);
                    m.rms[ch] = m.rms[ch].max(frame.rms[ch]);
                    for (a, b) in m.spectrum[ch].iter_mut().zip(frame.spectrum[ch].iter()) {
                        *a = a.max(*b);
                    }
                }
                m.frames = frame.frames;
                m.seq = frame.seq;
                m.time_ns = frame.time_ns;
            }
        }
    }
    merged
}

/// The meter falls no faster than this from full scale, as the old scope had it.
const METER_DECAY_MS: u32 = 500;
/// The old scope's smoothing of the spectrum bins.
const SPECTRUM_SMOOTHING: u32 = 60;
/// A ring not written for this long is silence: the player has stopped.
const RING_QUIET_NS: u64 = 500_000_000;

impl TapSource {
    /// Read the tap's ring. Without a live ring the levels sit at zero and
    /// the ring is looked for again once a second. Now-playing text is
    /// asked of the player once a second, not once a frame.
    pub fn installed() -> Self {
        Self::new(DEFAULT_SPECTRUM_BINS, DEFAULT_METER_MAX)
    }
    pub fn new(spectrum_bins: usize, meter_max: f32) -> Self {
        let bins = spectrum_bins.max(1);
        // The plugin's channel, when it serves one: the first state is
        // waited for briefly so the first frame is not painted from nothing.
        let mut channel = Channel::at(channel_path());
        if channel.connected() {
            channel.await_state(Duration::from_millis(300));
        }
        Self {
            ring: None,
            ring_looked_at: None,
            last_seq: 0,
            last_frames: 0,
            decay: tap::legacy::Meter::new(METER_DECAY_MS, meter_max.max(1.0) as u32),
            bins_mapper: tap::legacy::Spectrum::new(
                bins,
                lead::DEFAULT_SPECTRUM_MAX as u32,
                true,
                true,
                SPECTRUM_SMOOTHING,
            ),
            spectrum_max: lead::DEFAULT_SPECTRUM_MAX as u32,
            spectrum_bins: bins,
            spectrum_held: Vec::new(),
            levels_held: Levels::default(),
            metadata_held: lead::Metadata::default(),
            metadata_at: None,
            metadata_every: Some(Duration::from_secs(1)),
            seek_polled: 0.0,
            art: ArtFetcher::default(),
            skin_icons: String::new(),
            plugin_icons: String::new(),
            icon_cache: (String::new(), String::new()),
            wants_next: false,
            folder_layers: Vec::new(),
            folder_key: String::new(),
            folder_files: Vec::new(),
            fanart: None,
            vinyl_album_file: String::new(),
            vinyl_key: String::new(),
            vinyl_file: String::new(),
            reel_album_files: (String::new(), String::new()),
            reel_key: String::new(),
            reel_files: (String::new(), String::new()),
            queue_mode: false,
            queue_lengths: Vec::new(),
            queue_read_at: None,
            channel: Some(channel),
            channel_was_live: false,
            infinity_held: false,
            playing_held: NowPlaying::default(),
            rederive: false,
            conditioner: Conditioner::new(DataSourceSpec {
                max_ui: meter_max,
                max_pipe: meter_max,
                ..DataSourceSpec::default()
            }),
        }
    }

    /// Never ask the player for now-playing text, nor listen for it. For
    /// tests and recordings on a host without Volumio.
    pub fn without_player(mut self) -> Self {
        self.metadata_every = None;
        self.channel = None;
        self
    }

    /// What the skin needs from the player beyond the state: icon
    /// directories, and the queue when a next line or the ticker shows it.
    pub fn with_skin(mut self, skin: &SkinDesc) -> Self {
        self.set_skin(skin);
        self
    }

    /// The folder layer files for a track, looked up once per track folder:
    /// for each layer, the first of its candidates that exists, or empty.
    fn folder_files_for(&mut self, uri: &str) -> Vec<String> {
        if self.folder_layers.is_empty() {
            return Vec::new();
        }
        let key = uri.rfind('/').map(|i| &uri[..i]).unwrap_or(uri).to_string();
        if key != self.folder_key || self.folder_files.len() != self.folder_layers.len() {
            self.folder_key = key;
            self.folder_files = self
                .folder_layers
                .iter()
                .map(|files| {
                    folder_candidates(uri, files)
                        .into_iter()
                        .find(|candidate| Path::new(candidate).is_file())
                        .unwrap_or_default()
                })
                .collect();
        }
        self.folder_files.clone()
    }

    /// The reel pictures for a track, as for the record.
    fn reel_files_for(&mut self, uri: &str) -> (String, String) {
        if self.reel_album_files.0.is_empty() && self.reel_album_files.1.is_empty() {
            return (String::new(), String::new());
        }
        let key = uri.rfind('/').map(|i| &uri[..i]).unwrap_or(uri).to_string();
        if key != self.reel_key {
            self.reel_key = key;
            let find = |name: &String| -> String {
                if name.is_empty() {
                    return String::new();
                }
                folder_candidates(uri, std::slice::from_ref(name))
                    .into_iter()
                    .find(|candidate| Path::new(candidate).is_file())
                    .unwrap_or_default()
            };
            self.reel_files = (
                find(&self.reel_album_files.0),
                find(&self.reel_album_files.1),
            );
        }
        self.reel_files.clone()
    }

    /// In queue mode, the seconds of queue before `position` and the whole
    /// queue's length, from the player's queue read every ten seconds.
    fn queue_progress_for(&mut self, position: i64) -> (f32, f32) {
        if !self.queue_mode {
            return (0.0, 0.0);
        }
        if self
            .queue_read_at
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(10))
        {
            self.queue_lengths = queue_lengths();
            self.queue_read_at = Some(Instant::now());
        }
        let total: f32 = self.queue_lengths.iter().sum();
        if self.queue_lengths.is_empty() || total <= 0.0 {
            return (0.0, 0.0);
        }
        let before: f32 = self
            .queue_lengths
            .iter()
            .take(position.max(0) as usize)
            .sum();
        (before, total)
    }

    /// The record picture for a track: the album file named by the skin when
    /// the track's folder has it, else empty for the theme's own.
    fn vinyl_file_for(&mut self, uri: &str) -> String {
        if self.vinyl_album_file.is_empty() {
            return String::new();
        }
        let key = uri.rfind('/').map(|i| &uri[..i]).unwrap_or(uri).to_string();
        if key != self.vinyl_key {
            self.vinyl_key = key;
            self.vinyl_file = folder_candidates(uri, std::slice::from_ref(&self.vinyl_album_file))
                .into_iter()
                .find(|candidate| Path::new(candidate).is_file())
                .unwrap_or_default();
        }
        self.vinyl_file.clone()
    }

    /// Follow another skin: its icon folders, whether it wants the next
    /// track, and its level conditioning. The type icon is resolved afresh.
    pub fn set_skin(&mut self, skin: &SkinDesc) {
        self.skin_icons = skin.skin_icons.clone();
        self.plugin_icons = skin.plugin_icons.clone();
        self.wants_next = skin.next_title.is_some()
            || skin.next_artist.is_some()
            || skin.next_album.is_some()
            || skin.ticker.as_ref().is_some_and(|t| t.append_next);
        self.conditioner = Conditioner::new(skin.data_source.clone());
        self.icon_cache = (String::new(), String::new());
        self.rederive = true;
        self.folder_layers = skin.folder_layers.iter().map(|l| l.files.clone()).collect();
        self.folder_key = String::new();
        self.folder_files = Vec::new();
        self.vinyl_album_file = skin
            .vinyl
            .as_ref()
            .map(|v| v.album_file.clone())
            .unwrap_or_default();
        self.vinyl_key = String::new();
        self.vinyl_file = String::new();
        self.reel_album_files = (
            skin.reels
                .as_ref()
                .and_then(|r| r.left.as_ref())
                .map(|r| r.album_file.clone())
                .unwrap_or_default(),
            skin.reels
                .as_ref()
                .and_then(|r| r.right.as_ref())
                .map(|r| r.album_file.clone())
                .unwrap_or_default(),
        );
        self.reel_key = String::new();
        self.reel_files = (String::new(), String::new());
        self.queue_mode = skin.rotation.queue_mode;
        self.queue_lengths.clear();
        self.queue_read_at = None;
        self.fanart = skin.fanart.as_ref().map(|_| Slideshow {
            seed: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(1),
            ..Slideshow::default()
        });
        let spectrum_max = skin.spectrum_max.max(1.0) as u32;
        if let Some(bins) = skin.spectrum.as_ref().map(|s| s.bins.max(1)) {
            if bins != self.spectrum_bins || spectrum_max != self.spectrum_max {
                self.spectrum_bins = bins;
                self.spectrum_max = spectrum_max;
                self.bins_mapper =
                    tap::legacy::Spectrum::new(bins, spectrum_max, true, true, SPECTRUM_SMOOTHING);
                self.spectrum_held.clear();
            }
        }
    }

    /// The live ring, looked for at most once a second while there is none.
    fn ring(&mut self) -> Option<&tap::Reader> {
        if self.ring.as_ref().is_some_and(|r| r.is_live()) {
            return self.ring.as_ref();
        }
        let due = self
            .ring_looked_at
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(1));
        if due {
            self.ring_looked_at = Some(Instant::now());
            self.ring = tap::Reader::open_live(Path::new(tap::ring::DIR));
            self.last_seq = 0;
            self.last_frames = 0;
        }
        self.ring.as_ref().filter(|r| r.is_live())
    }
}

impl Source for TapSource {
    fn poll(&mut self) -> Input {
        // The latest hop, or silence when the ring is gone or has gone quiet.
        let mut hop: Option<(tap::Frame, u32, u64)> = None;
        let mut quiet = true;
        let last_seq = self.last_seq;
        if let Some(reader) = self.ring() {
            let info = reader.info();
            let stale = tap::ring::now_ns().saturating_sub(info.written_ns) > RING_QUIET_NS;
            if !stale {
                quiet = false;
                if info.seq != last_seq {
                    // Every hop since the last look, so no peak between two
                    // frames of the display is missed.
                    let first = if last_seq == 0
                        || info.seq.saturating_sub(last_seq) >= info.slots as u64
                    {
                        info.seq
                    } else {
                        last_seq + 1
                    };
                    let merged = merge_hops((first..=info.seq).filter_map(|seq| reader.slot(seq)));
                    if let Some(frame) = merged {
                        let elapsed = frame.frames.saturating_sub(self.last_frames).max(1);
                        hop = Some((frame, info.rate.max(1), elapsed));
                    }
                }
            }
        }
        if let Some((frame, rate, elapsed)) = hop {
            self.last_seq = frame.seq;
            self.last_frames = frame.frames;
            let raw = [
                (frame.peak[0] * 32767.0) as i32,
                (frame.peak[1] * 32767.0) as i32,
            ];
            let (left, right) = self.decay.update(raw, elapsed, rate);
            self.levels_held = self.conditioner.condition(left, right);
            // The old scope measured the two channels' average; so do the bins.
            let mixed: Vec<f32> = frame.spectrum[0]
                .iter()
                .zip(frame.spectrum[1].iter())
                .map(|(l, r)| (l + r) / 2.0)
                .collect();
            self.spectrum_held = self
                .bins_mapper
                .update(&mixed)
                .into_iter()
                .map(|v| v as f32)
                .collect();
        } else if quiet {
            let (left, right) = self.decay.update([0, 0], 1024, 48_000);
            self.levels_held = self.conditioner.condition(left, right);
            if self.spectrum_held.iter().any(|v| *v > 0.0) {
                let zeros = vec![0.0f32; 1024];
                self.spectrum_held = self
                    .bins_mapper
                    .update(&zeros)
                    .into_iter()
                    .map(|v| v as f32)
                    .collect();
            }
        }

        // The player's state: pushed by the plugin's channel as it changes,
        // or asked of the player once a second while there is no channel.
        let mut arrived: Option<NowPlaying> = None;
        if let Some(channel) = self.channel.as_mut() {
            for event in channel.pump() {
                match event {
                    Event::State(state) => arrived = Some(NowPlaying::from_value(&state)),
                    Event::Infinity(on) => self.infinity_held = on,
                    Event::Hello { .. } => {}
                }
            }
            let live = channel.connected();
            if live != self.channel_was_live {
                self.channel_was_live = live;
                if live {
                    println!("glass: channel {}", channel.path().display());
                } else {
                    println!("glass: channel gone, asking the player");
                }
            }
        }
        if arrived.is_none() && !self.channel_was_live {
            if let Some(every) = self.metadata_every {
                let due = self.metadata_at.is_none_or(|at| at.elapsed() >= every);
                if due {
                    arrived = Some(now_playing());
                }
            }
        }
        if let Some(playing) = arrived {
            self.seek_polled = playing.seek;
            self.metadata_at = Some(Instant::now());
            self.playing_held = playing;
            self.rederive = true;
        }
        // What the skin needs from the state is derived when a state arrives
        // and again when the skin changes.
        {
            if self.rederive {
                self.rederive = false;
                let playing = self.playing_held.clone();
                self.art.want(&playing.albumart);
                let key = format_key(&playing.track_type);
                if key != self.icon_cache.0 {
                    let icon = resolve_icon(&key, &self.skin_icons, &self.plugin_icons);
                    self.icon_cache = (key, icon);
                }
                if let Some(show) = self.fanart.as_mut() {
                    show.update(&playing.artist, &playing.uri);
                }
                let (next_title, next_artist, next_album) = if self.wants_next {
                    queue_next(playing.position)
                } else {
                    Default::default()
                };
                self.metadata_held = lead::Metadata {
                    title: playing.title,
                    artist: playing.artist,
                    album: playing.album,
                    samplerate: playing.samplerate,
                    bitdepth: playing.bitdepth,
                    status: playing.status,
                    duration: playing.duration,
                    seek: playing.seek,
                    albumart: playing.albumart,
                    art_file: String::new(),
                    track_type: playing.track_type,
                    bitrate: playing.bitrate,
                    type_icon: self.icon_cache.1.clone(),
                    next_title,
                    next_artist,
                    next_album,
                    persist_mode: String::new(),
                    persist_left: 0,
                    folder_files: self.folder_files_for(&playing.uri),
                    vinyl_file: self.vinyl_file_for(&playing.uri),
                    reel_files: self.reel_files_for(&playing.uri),
                    queue_before_s: self.queue_progress_for(playing.position).0,
                    queue_total_s: self.queue_progress_for(playing.position).1,
                    volatile: playing.volatile,
                    volume: playing.volume,
                    mute: playing.mute,
                    random: playing.random,
                    repeat: playing.repeat,
                    repeat_single: playing.repeat_single,
                    infinity: self.infinity_held,
                    uri: playing.uri,
                    fanart_file: String::new(),
                    fanart_prev_file: String::new(),
                    fanart_transition: String::new(),
                    fanart_transition_ms: 0,
                    fanart_elapsed_ms: 0,
                };
            }
        }

        // The player reports its position with its state; while it plays,
        // the snapshot moves on from that report by the time since.
        let mut metadata = self.metadata_held.clone();
        metadata.infinity = self.infinity_held;
        if metadata.status == "play" {
            if let Some(at) = self.metadata_at {
                metadata.seek = self.seek_polled + at.elapsed().as_secs_f32();
            }
        }
        if self.metadata_every.is_some() {
            metadata.art_file = self.art.file();
            let now_epoch_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let (mode, left) = persist_state(PERSIST_FILE, now_epoch_ms);
            metadata.persist_mode = mode;
            metadata.persist_left = left;
        }
        if let Some(show) = self.fanart.as_mut() {
            let (file, prev, mode, duration, elapsed) = show.snapshot();
            metadata.fanart_file = file;
            metadata.fanart_prev_file = prev;
            metadata.fanart_transition = mode;
            metadata.fanart_transition_ms = duration;
            metadata.fanart_elapsed_ms = elapsed;
        }

        Input {
            levels: self.levels_held,
            bins: Bins {
                values: self.spectrum_held.clone(),
            },
            metadata,
        }
    }
}

/// `frame.rate` from the installed `config.txt`, or 30 when that file is absent.
pub fn installed_frame_rate() -> u32 {
    match config_text().ok_or(()) {
        Ok(text) => frame_rate_from_config(&text),
        Err(_) => DEFAULT_FRAME_RATE,
    }
}

/// Skin from the installed `config.txt`. Size and background come from the
/// selected theme. With no config file the size stays 800×480 and there is
/// no theme background.
pub fn installed_skin() -> SkinDesc {
    installed_skin_named(None)
}

/// The installed skin for one meter of the theme, `None` for the meter the
/// configuration names. Rotation calls this once per switch.
pub fn installed_skin_named(meter: Option<&str>) -> SkinDesc {
    let path = config_path();
    let mut skin = SkinDesc::basic();
    let Some(text) = config_text() else {
        return skin;
    };
    let (width, height) = screen_from_config(&text);
    skin.width = width;
    skin.height = height;
    let meter = meter
        .map(str::to_string)
        .unwrap_or_else(|| current_value(&text, "meter").unwrap_or_default());
    if !meter.is_empty() {
        skin.name = meter;
    }
    let Some(theme) = theme_dir_from(&text, &path) else {
        return skin;
    };
    skin.theme_dir = theme.to_string_lossy().into_owned();
    if let Ok(meters) = std::fs::read_to_string(theme.join("meters.txt")) {
        if let Some(file) = meter_background(&meters, &skin.name) {
            skin.background = file;
        }
        let (left_at, right_at) = meter_at(&meters, &skin.name);
        skin.left_at = left_at;
        skin.right_at = right_at;
        if let Some(file) = meter_indicator(&meters, &skin.name) {
            skin.indicator = file;
        }
        let (screen, face, front, face_at) = meter_layers(&meters, &skin.name);
        if !screen.is_empty() {
            skin.background = screen;
        }
        skin.face = face;
        skin.front = front;
        skin.face_at = face_at;
        skin.needle = meter_needle(&meters, &skin.name);
        skin.meter = meter_spec(&meters, &skin.name);
        skin.folder_layers = meter_folder_layers(&meters, &skin.name);
        skin.fanart = meter_fanart(&meters, &skin.name);
        skin.rotation = rotation_settings(&text);
        skin.vinyl = meter_vinyl(&meters, &skin.name, &skin.theme_dir, &skin.rotation);
        skin.tonearm = meter_tonearm(&meters, &skin.name, &skin.theme_dir);
        skin.reels = meter_reels(&meters, &skin.name, &skin.theme_dir, &skin.rotation);
        skin.indicators = meter_indicators(&meters, &skin.name, &skin.theme_dir);
        skin.transition = transition_settings(&text);
        skin.run = run_settings(&text);
        let (title_at, artist_at) = meter_text_at(&meters, &skin.name);
        skin.title_at = title_at;
        skin.artist_at = artist_at;
        let speeds = scroll_speeds_from_config(&text);
        let texts = meter_texts(&meters, &skin.name, skin.width, &speeds);
        skin.title = texts.title;
        skin.artist = texts.artist;
        skin.album = texts.album;
        skin.sample = texts.sample;
        let font_path = current_value(&text, "font.path").unwrap_or_default();
        let mut time = texts.time;
        let mut time_elapsed = texts.time_elapsed;
        let mut time_total = texts.time_total;
        for field in [&mut time, &mut time_elapsed, &mut time_total]
            .into_iter()
            .flatten()
        {
            resolve_time_font(field, &skin.theme_dir, &font_path);
        }
        skin.time = time;
        skin.time_elapsed = time_elapsed;
        skin.time_total = time_total;
        skin.next_title = texts.next_title;
        skin.next_artist = texts.next_artist;
        skin.next_album = texts.next_album;
        skin.ticker = texts.ticker;
        skin.art = meter_art(&meters, &skin.name, &skin.theme_dir);
        let default_mode = current_value(&text, "playinfo.type.mode");
        skin.type_area = meter_type(&meters, &skin.name, default_mode.as_deref());
        skin.skin_icons = theme.join("format-icons").to_string_lossy().into_owned();
    }
    // The clock font and the player's icon set ship in the plugin's home:
    // <home>/fonts and <home>/format-icons, one level above the configuration.
    let handlers_dir = Path::new(&path).parent().and_then(Path::parent);
    let digi_default = handlers_dir
        .map(|dir| dir.join("fonts").join("DSEG7Classic-Italic.ttf"))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let italic_default = handlers_dir
        .map(|dir| dir.join("fonts").join("PeppyFont-Italic.ttf"))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    skin.plugin_icons = handlers_dir
        .map(|dir| dir.join("format-icons").to_string_lossy().into_owned())
        .unwrap_or_default();
    skin.fonts = fonts_from_config(&text, &digi_default, &italic_default);
    skin.data_source = data_source_from_config(&text);
    skin.meter_max = skin.data_source.max_ui;
    // The spectrum configuration sits beside the meter configuration; the
    // meter names which of the theme's spectra it shows and how big.
    let placed = std::fs::read_to_string(theme.join("meters.txt"))
        .ok()
        .and_then(|meters| meter_spectrum(&meters, &skin.name));
    if let Some((name, w, h)) = placed {
        if let Ok(config) = std::fs::read_to_string(spectrum_config_path()) {
            let settings = spectrum_settings(&config);
            // The spectrum theme carries the meter theme's folder name; the
            // player keeps `spectrum.folder` in step with `meter.folder`, and
            // the theme's own name wins when the two disagree, as after a
            // theme override or before the player has caught up.
            let base = Path::new(&settings.base_folder);
            let by_theme = theme
                .file_name()
                .map(|name| base.join(name))
                .filter(|dir| dir.join("spectrum.txt").is_file());
            let folder = by_theme.unwrap_or_else(|| base.join(&settings.folder));
            if let Ok(spectra) = std::fs::read_to_string(folder.join("spectrum.txt")) {
                skin.spectrum = spectrum_from_theme(
                    &spectra,
                    &name,
                    (w, h),
                    &settings,
                    &folder.to_string_lossy(),
                );
                skin.spectrum_max = settings.max_value;
            }
        }
    }
    skin
}

#[derive(Debug, Default, Clone)]
pub struct NowPlaying {
    pub volume: u32,
    pub mute: bool,
    pub random: bool,
    pub repeat: bool,
    pub repeat_single: bool,
    pub volatile: Option<bool>,
    pub uri: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub samplerate: String,
    pub bitdepth: String,
    pub status: String,
    /// Seconds. Zero when the player reports none.
    pub duration: f32,
    /// Seconds. The player reports milliseconds.
    pub seek: f32,
    /// Album art location as reported: a URL, or a path on the player.
    pub albumart: String,
    pub track_type: String,
    pub bitrate: String,
    /// Index of the playing item in the queue.
    pub position: i64,
}

impl NowPlaying {
    /// The fields Glass shows, read leniently from the player's state as
    /// the player pushes it or answers with it: a number may come as a
    /// string, and a missing or null field is its default.
    pub fn from_value(state: &Value) -> Self {
        let text = |key: &str| match state.get(key) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        };
        let number = |key: &str| -> Option<f32> {
            match state.get(key)? {
                Value::Number(n) => n.as_f64().map(|n| n as f32),
                Value::String(s) => s.trim().parse().ok(),
                _ => None,
            }
        };
        let flag = |key: &str| state.get(key).and_then(Value::as_bool);
        NowPlaying {
            volume: number("volume").unwrap_or(0.0).clamp(0.0, 100.0) as u32,
            mute: flag("mute").unwrap_or(false),
            random: flag("random").unwrap_or(false),
            repeat: flag("repeat").unwrap_or(false),
            repeat_single: flag("repeatSingle").unwrap_or(false),
            volatile: flag("volatile"),
            uri: text("uri"),
            title: text("title"),
            artist: text("artist"),
            album: text("album"),
            samplerate: text("samplerate"),
            bitdepth: text("bitdepth"),
            status: text("status"),
            duration: number("duration").unwrap_or(0.0),
            seek: number("seek").unwrap_or(0.0) / 1000.0,
            albumart: text("albumart"),
            track_type: text("trackType"),
            bitrate: text("bitrate"),
            position: number("position").map(|p| p as i64).unwrap_or(0),
        }
    }
}

/// Current track from Volumio, asked over HTTP. Empty strings when the
/// player does not answer. [`TapSource`] asks once a second while the
/// plugin's channel is not there.
pub fn now_playing() -> NowPlaying {
    let mut playing = NowPlaying::default();
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(200)) else {
        return playing;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
    let _ = stream.write_all(
        b"GET /api/v1/getState HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    let mut buf = String::new();
    if stream.read_to_string(&mut buf).is_err() {
        return playing;
    }
    let body = buf
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or(&buf);
    if let Ok(state) = serde_json::from_str::<Value>(body) {
        playing = NowPlaying::from_value(&state);
    }
    playing
}

pub fn input_from_records(
    meter: &[u8],
    spectrum: &[u8],
    spectrum_bins: usize,
    meter_max: f32,
) -> Input {
    let levels = decode_meter(meter)
        .map(|(left, right)| {
            Conditioner::new(DataSourceSpec {
                max_ui: meter_max,
                max_pipe: meter_max,
                ..DataSourceSpec::default()
            })
            .condition(left, right)
        })
        .unwrap_or_default();
    let bins = decode_spectrum(spectrum, spectrum_bins).unwrap_or_default();
    Input {
        levels,
        bins: Bins { values: bins },
        metadata: lead::Metadata::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_become_ui_levels_and_bins() {
        let input = input_from_records(&[50, 0, 25, 0], &[100, 0, 0, 0, 0, 0, 0, 0], 2, 100.0);
        assert_eq!(input.levels.left, 50.0);
        assert_eq!(input.levels.right, 25.0);
        assert_eq!(input.levels.mono, 37.5);
        assert_eq!(input.bins.values, vec![100.0, 0.0]);
    }

    #[test]
    fn hops_between_two_looks_merge_into_their_loudest() {
        let hop = |seq: u64, peak: f32, bin: f32| tap::Frame {
            seq,
            time_ns: seq * 10,
            frames: seq * 1024,
            peak: [peak, peak / 2.0],
            rms: [peak / 2.0, peak / 4.0],
            spectrum: [vec![bin, 0.1], vec![0.0, bin]],
        };
        let merged =
            merge_hops([hop(5, 0.2, 0.5), hop(6, 0.9, 0.1), hop(7, 0.4, 0.3)].into_iter()).unwrap();
        assert_eq!(merged.peak, [0.9, 0.45]);
        assert_eq!(merged.rms, [0.45, 0.225]);
        assert_eq!(merged.spectrum[0], [0.5, 0.1]);
        assert_eq!(merged.spectrum[1], [0.0, 0.5]);
        assert_eq!(
            (merged.seq, merged.frames, merged.time_ns),
            (7, 7 * 1024, 70)
        );
        assert!(merge_hops(std::iter::empty()).is_none());
    }

    #[test]
    fn player_state_is_read_leniently() {
        let state: Value = serde_json::from_str(
            r#"{"status":"play","title":"Wonder","artist":null,"duration":218.051,"seek":1994.96,"volume":"46","mute":false,"repeatSingle":true,"position":3,"samplerate":"44.1 kHz","bitdepth":"16-bit","bitrate":320,"trackType":"flac"}"#,
        )
        .unwrap();
        let playing = NowPlaying::from_value(&state);
        assert_eq!(playing.status, "play");
        assert_eq!(playing.title, "Wonder");
        assert_eq!(playing.artist, "", "null reads as empty");
        assert_eq!(playing.duration, 218.051);
        assert!(
            (playing.seek - 1.99496).abs() < 1e-5,
            "seek comes in milliseconds"
        );
        assert_eq!(playing.volume, 46, "a number in a string still counts");
        assert!(playing.repeat_single);
        assert!(!playing.repeat);
        assert_eq!(playing.position, 3);
        assert_eq!(playing.samplerate, "44.1 kHz");
        assert_eq!(playing.bitrate, "320", "a number reads as its text");
        assert_eq!(playing.track_type, "flac");
        assert_eq!(playing.volatile, None);
    }

    #[test]
    fn levels_are_conditioned_as_the_engine_conditions_them() {
        let mut plain = Conditioner::new(DataSourceSpec::default());
        assert_eq!(
            plain.condition(80, 40),
            Levels {
                left: 80.0,
                right: 40.0,
                mono: 60.0
            }
        );
        let mut quiet = Conditioner::new(DataSourceSpec {
            gain_db: -6.0,
            ..DataSourceSpec::default()
        });
        let l = quiet.condition(80, 40);
        assert_eq!(
            (l.left, l.right),
            (40.0, 20.0),
            "-6 dB halves, floored as the engine floors"
        );
        let mut smooth = Conditioner::new(DataSourceSpec {
            smooth: 2,
            mono: "maximum".into(),
            ..DataSourceSpec::default()
        });
        smooth.condition(0, 0);
        let l = smooth.condition(80, 40);
        assert_eq!(
            (l.left, l.right, l.mono),
            (40.0, 20.0, 40.0),
            "mean of the last two; mono is the maximum"
        );
        let mut averaged = Conditioner::new(DataSourceSpec {
            stereo: "average".into(),
            ..DataSourceSpec::default()
        });
        averaged.condition(80, 80);
        assert_eq!(
            averaged.condition(0, 0).left,
            20.0,
            "average with the previous averaged value"
        );
    }

    #[test]
    fn an_override_replaces_or_adds_a_current_value() {
        let text = "[current]\nmeter = gold\nmeter.folder = a\n[data.source]\nmeter = x\n";
        let out = with_current(text, "meter", "random");
        assert!(
            out.contains("[current]\nmeter = random\nmeter.folder = a\n"),
            "{out}"
        );
        assert!(
            out.contains("[data.source]\nmeter = x\n"),
            "other sections keep theirs: {out}"
        );
        let added = with_current(
            "[current]\nmeter = gold\n[x]\n",
            "random.meter.interval",
            "5",
        );
        assert!(
            added.contains("meter = gold\nrandom.meter.interval = 5\n[x]"),
            "{added}"
        );
        assert_eq!(with_current("", "meter", "a"), "[current]\nmeter = a\n");
    }

    #[test]
    fn the_selector_draws_every_name_before_repeating_and_cycles_a_list() {
        let names: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
        let rotation = |random: bool, names: Vec<String>| Rotation {
            names,
            random,
            interval: Duration::from_secs(1),
            on_title: false,
        };
        let mut random = Selector::seeded(rotation(true, names.clone()), 7);
        for round in 0..3 {
            let mut drawn: Vec<String> = (0..3).map(|_| random.next().unwrap()).collect();
            drawn.sort();
            assert_eq!(drawn, names, "round {round} draws each name once");
        }
        let mut list = Selector::seeded(rotation(false, names.clone()), 1);
        let walk: Vec<String> = (0..4).map(|_| list.next().unwrap()).collect();
        assert_eq!(walk, ["a", "b", "c", "a"]);
        let mut fixed = Selector::seeded(rotation(true, Vec::new()), 1);
        assert!(!fixed.rotates());
        assert_eq!(fixed.next(), None);
    }

    #[test]
    fn the_persist_file_gives_mode_and_seconds_left() {
        let dir = std::env::temp_dir().join(format!("glass-persist-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("persist");
        std::fs::write(&file, "15:1000000:countdown").unwrap();
        let path = file.to_string_lossy().into_owned();
        assert_eq!(persist_state(&path, 1_004_000), ("countdown".into(), 11));
        assert_eq!(persist_state(&path, 1_020_000), ("countdown".into(), 0));
        std::fs::write(&file, "30:1000000").unwrap();
        assert_eq!(persist_state(&path, 1_000_000), ("freeze".into(), 30));
        assert_eq!(
            persist_state(&dir.join("none").to_string_lossy(), 1),
            (String::new(), 0)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn icons_are_found_ignoring_case_and_in_order() {
        let dir = std::env::temp_dir().join(format!("glass-icons-{}", std::process::id()));
        let skin = dir.join("skin");
        let plugin = dir.join("plugin");
        std::fs::create_dir_all(&skin).unwrap();
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(skin.join("flac.png"), b"x").unwrap();
        std::fs::write(plugin.join("YouTube.svg"), b"x").unwrap();
        std::fs::write(plugin.join("flac.svg"), b"x").unwrap();
        let skin_s = skin.to_string_lossy().into_owned();
        let plugin_s = plugin.to_string_lossy().into_owned();
        assert!(resolve_icon("flac", &skin_s, &plugin_s).ends_with("skin/flac.png"));
        assert!(resolve_icon("youtube", &skin_s, &plugin_s).ends_with("plugin/YouTube.svg"));
        assert_eq!(resolve_icon("", &skin_s, &plugin_s), "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn art_on_the_player_is_fetched_from_the_player() {
        assert_eq!(
            art_url("/albumart?web=a/b/large"),
            "http://127.0.0.1:3000/albumart?web=a/b/large"
        );
        assert_eq!(
            art_url("https://img.example/cover.jpg"),
            "https://img.example/cover.jpg"
        );
        assert_ne!(fnv1a("a"), fnv1a("b"));
        assert_eq!(fnv1a("cover"), fnv1a("cover"));
    }
}
