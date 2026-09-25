//! Poll the outside world and return the latest [`lead::Input`].
//! This station does not parse skin geometry or draw.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use std::collections::VecDeque;

use lead::{
    data_source_from_config, decode_meter, decode_spectrum, fonts_from_config, format_key,
    frame_rate_from_config, meter_art, meter_at, meter_background, meter_indicator,
    folder_candidates, meter_folder_layers, meter_layers, meter_needle, meter_sections, meter_spec, meter_spectrum, meter_text_at,
    meter_texts, meter_type, random_change_title_from_config, random_interval_from_config,
    screen_from_config, scroll_speeds_from_config, selection_from_config, spectrum_from_theme,
    spectrum_settings, Bins, DataSourceSpec, Input, Levels, Selection, SkinDesc, TextSpec, CONFIG_TXT,
    DEFAULT_FRAME_RATE, DEFAULT_METER_MAX, DEFAULT_SPECTRUM_BINS, METER_FIFO, SPECTRUM_FIFO,
    STOCK_ICONS, current_value,
};

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

fn config_path() -> String {
    std::env::var("GLASS_CONFIG").unwrap_or_else(|_| CONFIG_TXT.to_string())
}

/// The meter rotation from the installed configuration. `random` walks every
/// section of the theme's meters file.
pub fn installed_rotation() -> Rotation {
    let path = config_path();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
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

/// The plugin's persist file: `duration:start_ms:mode`.
pub const PERSIST_FILE: &str = "/tmp/peppy_persist";

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
    let (Ok(duration), Ok(start)) = (duration.trim().parse::<u64>(), start.trim().parse::<u64>()) else {
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
        let window = std::iter::repeat(Levels::default()).take(spec.smooth).collect();
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
        if self.source_at.is_some_and(|at| at.elapsed() < Duration::from_secs(1)) {
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
        let scale = |raw: u16| (self.spec.max_ui * (f32::from(raw) / self.spec.max_pipe) * gain).floor();
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
            candidates.push(Path::new(base.trim_end_matches('/')).join(value.trim_start_matches('/')));
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
        .write_all(format!("GET {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").as_bytes())
        .ok()?;
    let mut buf = String::new();
    stream.read_to_string(&mut buf).ok()?;
    Some(buf.split_once("\r\n\r\n").map(|(_, body)| body.to_string()).unwrap_or(buf))
}

/// The track after `position` in the player's queue: title (or name),
/// artist, album. Empty strings when there is none or the player does not answer.
pub fn queue_next(position: i64) -> (String, String, String) {
    let Some(body) = player_get("/api/v1/getQueue") else {
        return Default::default();
    };
    let value: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return Default::default(),
    };
    let items = value
        .as_array()
        .or_else(|| value.get("queue").and_then(|q| q.as_array()));
    let Some(items) = items else {
        return Default::default();
    };
    let next = usize::try_from(position + 1).ok().and_then(|i| items.get(i));
    let Some(next) = next else {
        return Default::default();
    };
    let text = |key: &str| next.get(key).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let title = {
        let t = text("title");
        if t.is_empty() { text("name") } else { t }
    };
    (title, text("artist"), text("album"))
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

/// Linux `O_NONBLOCK`. A blocking open on a FIFO waits for the writer.
const O_NONBLOCK: i32 = 0x800;

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

/// Bytes already pulled off a pipe. Keeps only the latest complete record.
#[derive(Debug, Default)]
struct RecordBuf {
    pending: Vec<u8>,
    record: usize,
    latest: Option<Vec<u8>>,
}

impl RecordBuf {
    fn new(record: usize) -> Self {
        Self {
            pending: Vec::new(),
            record,
            latest: None,
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        if self.record == 0 {
            return;
        }
        self.pending.extend_from_slice(chunk);
        while self.pending.len() >= self.record {
            let rec: Vec<u8> = self.pending.drain(..self.record).collect();
            self.latest = Some(rec);
        }
    }
}

/// Installed meter and spectrum FIFOs.
pub struct PipeSource {
    meter_path: String,
    spectrum_path: String,
    meter: Option<File>,
    spectrum: Option<File>,
    meter_buf: RecordBuf,
    spectrum_buf: RecordBuf,
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
}

impl PipeSource {
    /// Open the Volumio pipes. Missing files stay closed and are retried on poll.
    /// Now-playing text is asked of the player once a second, not once a frame.
    pub fn installed() -> Self {
        Self::new(
            METER_FIFO,
            SPECTRUM_FIFO,
            DEFAULT_SPECTRUM_BINS,
            DEFAULT_METER_MAX,
        )
    }

    pub fn new(
        meter_path: impl Into<String>,
        spectrum_path: impl Into<String>,
        spectrum_bins: usize,
        meter_max: f32,
    ) -> Self {
        let bins = spectrum_bins.max(1);
        Self {
            meter_path: meter_path.into(),
            spectrum_path: spectrum_path.into(),
            meter: None,
            spectrum: None,
            meter_buf: RecordBuf::new(4),
            spectrum_buf: RecordBuf::new(bins * 4),
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
            conditioner: Conditioner::new(DataSourceSpec {
                max_ui: meter_max,
                max_pipe: meter_max,
                ..DataSourceSpec::default()
            }),
        }
    }

    /// Never ask the player for now-playing text. For tests and recordings on
    /// a host without Volumio.
    pub fn without_player(mut self) -> Self {
        self.metadata_every = None;
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
        self.metadata_at = None;
        self.folder_layers = skin.folder_layers.iter().map(|l| l.files.clone()).collect();
        self.folder_key = String::new();
        self.folder_files = Vec::new();
        if let Some(bins) = skin.spectrum.as_ref().map(|s| s.bins.max(1)) {
            if bins != self.spectrum_bins {
                self.spectrum_bins = bins;
                self.spectrum_buf = RecordBuf::new(bins * 4);
                self.spectrum_held.clear();
            }
        }
    }

    fn try_open(slot: &mut Option<File>, path: &str) {
        if slot.is_some() || !Path::new(path).exists() {
            return;
        }
        if let Ok(file) = OpenOptions::new()
            .read(true)
            .custom_flags(O_NONBLOCK)
            .open(path)
        {
            *slot = Some(file);
        }
    }

    fn drain(file: &mut File, buf: &mut RecordBuf) {
        let mut chunk = [0u8; 4096];
        loop {
            match file.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => buf.push(&chunk[..n]),
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    }
}

impl Source for PipeSource {
    fn poll(&mut self) -> Input {
        Self::try_open(&mut self.meter, &self.meter_path);
        Self::try_open(&mut self.spectrum, &self.spectrum_path);

        if let Some(file) = self.meter.as_mut() {
            Self::drain(file, &mut self.meter_buf);
        }
        if let Some(file) = self.spectrum.as_mut() {
            Self::drain(file, &mut self.spectrum_buf);
        }

        if let Some(record) = self.meter_buf.latest.take() {
            if let Some((left, right)) = decode_meter(&record) {
                self.levels_held = self.conditioner.condition(left, right);
            }
        }
        if let Some(record) = self.spectrum_buf.latest.take() {
            if let Some(values) = decode_spectrum(&record, self.spectrum_bins) {
                self.spectrum_held = values;
            }
        }

        if let Some(every) = self.metadata_every {
            let due = self.metadata_at.map_or(true, |at| at.elapsed() >= every);
            if due {
                let playing = now_playing();
                self.seek_polled = playing.seek;
                self.art.want(&playing.albumart);
                let key = format_key(&playing.track_type);
                if key != self.icon_cache.0 {
                    let icon = resolve_icon(&key, &self.skin_icons, &self.plugin_icons);
                    self.icon_cache = (key, icon);
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
                    uri: playing.uri,
                };
                self.metadata_at = Some(Instant::now());
            }
        }

        // The player reports its position once a second; while it plays,
        // the snapshot moves on from that report by the time since.
        let mut metadata = self.metadata_held.clone();
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
    let path = std::env::var("GLASS_CONFIG").unwrap_or_else(|_| CONFIG_TXT.to_string());
    match std::fs::read_to_string(&path) {
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
    let Ok(text) = std::fs::read_to_string(&path) else {
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
        for field in [&mut time, &mut time_elapsed, &mut time_total].into_iter().flatten() {
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
    // The clock font and the player's icon set ship next to the player's
    // handlers: <plugin>/screensaver/fonts and <plugin>/screensaver/format-icons.
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
    // The spectrum engine's own configuration sits beside the handlers; the
    // meter names which of the theme's spectra it shows and how big.
    let placed = std::fs::read_to_string(theme.join("meters.txt"))
        .ok()
        .and_then(|meters| meter_spectrum(&meters, &skin.name));
    if let (Some((name, w, h)), Some(dir)) = (placed, handlers_dir) {
        if let Ok(config) = std::fs::read_to_string(dir.join("spectrum").join("config.txt")) {
            let settings = spectrum_settings(&config);
            let folder = Path::new(&settings.base_folder).join(&settings.folder);
            if let Ok(spectra) = std::fs::read_to_string(folder.join("spectrum.txt")) {
                skin.spectrum = spectrum_from_theme(&spectra, &name, (w, h), &settings, &folder.to_string_lossy());
                skin.spectrum_max = settings.max_value;
            }
        }
    }
    skin
}

#[derive(Debug, Default)]
pub struct NowPlaying {
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

/// Current track from Volumio. Empty strings when the player does not answer.
/// One HTTP request; [`PipeSource`] calls this once a second.
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
    let body = buf.split_once("\r\n\r\n").map(|(_, body)| body).unwrap_or(&buf);
    playing.title = json_string(body, "title");
    playing.artist = json_string(body, "artist");
    playing.album = json_string(body, "album");
    playing.samplerate = json_string(body, "samplerate");
    playing.bitdepth = json_string(body, "bitdepth");
    playing.status = json_string(body, "status");
    playing.duration = json_number(body, "duration").unwrap_or(0.0);
    playing.seek = json_number(body, "seek").unwrap_or(0.0) / 1000.0;
    playing.albumart = json_string(body, "albumart");
    playing.uri = json_string(body, "uri");
    playing.track_type = json_string(body, "trackType");
    playing.bitrate = json_string(body, "bitrate");
    playing.position = json_number(body, "position").map(|p| p as i64).unwrap_or(0);
    playing
}

/// A bare JSON number after `"key":`. `None` when the key is absent or the
/// value is not a number.
fn json_number(body: &str, key: &str) -> Option<f32> {
    let pattern = format!("\"{key}\"");
    let start = body.find(&pattern)?;
    let rest = body[start + pattern.len()..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn json_string(body: &str, key: &str) -> String {
    let pattern = format!("\"{key}\"");
    let Some(start) = body.find(&pattern) else {
        return String::new();
    };
    let rest = body[start + pattern.len()..].trim_start();
    let rest = rest.trim_start_matches(':').trim_start();
    let Some(rest) = rest.strip_prefix('"') else {
        return String::new();
    };
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
            continue;
        }
        if ch == '"' {
            break;
        }
        out.push(ch);
    }
    out
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
        let input = input_from_records(
            &[50, 0, 25, 0],
            &[100, 0, 0, 0, 0, 0, 0, 0],
            2,
            100.0,
        );
        assert_eq!(input.levels.left, 50.0);
        assert_eq!(input.levels.right, 25.0);
        assert_eq!(input.levels.mono, 37.5);
        assert_eq!(input.bins.values, vec![100.0, 0.0]);
    }

    #[test]
    fn player_state_numbers_and_words_are_read() {
        let body = r#"{"status":"play","title":"Wonder","duration":218.051,"seek":1994.96,"samplerate":"44.1 kHz","bitdepth":"16-bit"}"#;
        assert_eq!(json_number(body, "duration"), Some(218.051));
        assert_eq!(json_number(body, "seek"), Some(1994.96));
        assert_eq!(json_number(body, "missing"), None);
        assert_eq!(json_string(body, "status"), "play");
        assert_eq!(json_string(body, "samplerate"), "44.1 kHz");
    }

    #[test]
    fn levels_are_conditioned_as_the_engine_conditions_them() {
        let mut plain = Conditioner::new(DataSourceSpec::default());
        assert_eq!(plain.condition(80, 40), Levels { left: 80.0, right: 40.0, mono: 60.0 });
        let mut quiet = Conditioner::new(DataSourceSpec { gain_db: -6.0, ..DataSourceSpec::default() });
        let l = quiet.condition(80, 40);
        assert_eq!((l.left, l.right), (40.0, 20.0), "-6 dB halves, floored as the engine floors");
        let mut smooth = Conditioner::new(DataSourceSpec { smooth: 2, mono: "maximum".into(), ..DataSourceSpec::default() });
        smooth.condition(0, 0);
        let l = smooth.condition(80, 40);
        assert_eq!((l.left, l.right, l.mono), (40.0, 20.0, 40.0), "mean of the last two; mono is the maximum");
        let mut averaged = Conditioner::new(DataSourceSpec { stereo: "average".into(), ..DataSourceSpec::default() });
        averaged.condition(80, 80);
        assert_eq!(averaged.condition(0, 0).left, 20.0, "average with the previous averaged value");
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
        assert_eq!(persist_state(&dir.join("none").to_string_lossy(), 1), (String::new(), 0));
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
        assert_eq!(art_url("/albumart?web=a/b/large"), "http://127.0.0.1:3000/albumart?web=a/b/large");
        assert_eq!(art_url("https://img.example/cover.jpg"), "https://img.example/cover.jpg");
        assert_ne!(fnv1a("a"), fnv1a("b"));
        assert_eq!(fnv1a("cover"), fnv1a("cover"));
    }

    #[test]
    fn partial_chunks_keep_the_latest_record() {
        let mut buf = RecordBuf::new(4);
        buf.push(&[1, 0]);
        buf.push(&[0, 0, 9, 0, 0, 0]);
        assert_eq!(buf.latest.unwrap(), vec![9, 0, 0, 0]);
    }
}
