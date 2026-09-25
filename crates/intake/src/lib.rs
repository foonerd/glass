//! Poll the outside world and return the latest [`lead::Input`].
//! This station does not parse skin geometry or draw.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use lead::{
    decode_meter, decode_spectrum, fonts_from_config, frame_rate_from_config, meter_at,
    meter_background, meter_indicator, meter_layers, meter_needle, meter_text_at, meter_texts,
    mono_average, scale_level, screen_from_config, Bins, Input, Levels, SkinDesc, CONFIG_TXT,
    DEFAULT_FRAME_RATE, DEFAULT_METER_MAX, DEFAULT_SPECTRUM_BINS, METER_FIFO, SPECTRUM_FIFO,
    current_value,
};

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
    meter_max: f32,
    spectrum_held: Vec<f32>,
    levels_held: Levels,
    metadata_held: lead::Metadata,
    metadata_at: Option<Instant>,
    /// How often the player is asked for now-playing text. `None` never asks.
    metadata_every: Option<Duration>,
    /// Position the player reported at `metadata_at`, in seconds.
    seek_polled: f32,
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
            meter_max,
            spectrum_held: Vec::new(),
            levels_held: Levels::default(),
            metadata_held: lead::Metadata::default(),
            metadata_at: None,
            metadata_every: Some(Duration::from_secs(1)),
            seek_polled: 0.0,
        }
    }

    /// Never ask the player for now-playing text. For tests and recordings on
    /// a host without Volumio.
    pub fn without_player(mut self) -> Self {
        self.metadata_every = None;
        self
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
                let left = scale_level(left, self.meter_max, self.meter_max);
                let right = scale_level(right, self.meter_max, self.meter_max);
                self.levels_held = Levels {
                    mono: mono_average(left, right),
                    left,
                    right,
                };
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
                self.metadata_held = lead::Metadata {
                    title: playing.title,
                    artist: playing.artist,
                    album: playing.album,
                    samplerate: playing.samplerate,
                    bitdepth: playing.bitdepth,
                    status: playing.status,
                    duration: playing.duration,
                    seek: playing.seek,
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
    let path = std::env::var("GLASS_CONFIG").unwrap_or_else(|_| CONFIG_TXT.to_string());
    let mut skin = SkinDesc::basic();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return skin;
    };
    let (width, height) = screen_from_config(&text);
    skin.width = width;
    skin.height = height;
    let folder = current_value(&text, "meter.folder").unwrap_or_default();
    let base = current_value(&text, "base.folder").unwrap_or_default();
    let meter = current_value(&text, "meter").unwrap_or_default();
    if !meter.is_empty() {
        skin.name = meter;
    }
    if folder.is_empty() {
        return skin;
    }
    let root = if base.is_empty() {
        Path::new(&path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(base)
    };
    let theme = root.join(&folder);
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
        let (title_at, artist_at) = meter_text_at(&meters, &skin.name);
        skin.title_at = title_at;
        skin.artist_at = artist_at;
        let texts = meter_texts(&meters, &skin.name);
        skin.title = texts.title;
        skin.artist = texts.artist;
        skin.album = texts.album;
        skin.sample = texts.sample;
        skin.time = texts.time;
    }
    // The clock font ships next to the player's handlers: <plugin>/screensaver/fonts.
    let digi_default = Path::new(&path)
        .parent()
        .and_then(Path::parent)
        .map(|dir| dir.join("fonts").join("DSEG7Classic-Italic.ttf"))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    skin.fonts = fonts_from_config(&text, &digi_default);
    skin
}

#[derive(Debug, Default)]
pub struct NowPlaying {
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
            let left = scale_level(left, meter_max, meter_max);
            let right = scale_level(right, meter_max, meter_max);
            Levels {
                mono: mono_average(left, right),
                left,
                right,
            }
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
    fn partial_chunks_keep_the_latest_record() {
        let mut buf = RecordBuf::new(4);
        buf.push(&[1, 0]);
        buf.push(&[0, 0, 9, 0, 0, 0]);
        assert_eq!(buf.latest.unwrap(), vec![9, 0, 0, 0]);
    }
}
