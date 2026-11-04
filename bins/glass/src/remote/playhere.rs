//! Play here on a remote display: the player's sound on this machine,
//! taken over the player's Manager port as a page takes it (the wire
//! outcast-lite speaks, read with its client crate) and played through
//! this machine's audio output a quarter second behind the room.
//!
//! Three parts. A socket thread takes the frames and decodes them into a
//! ring. The ring keeps the queue's rules: a lead gathered before the sound
//! starts, silence when it runs dry until the lead is there again, and a
//! trim back to the lead when the two clocks drift it long. The session's
//! side, on the thread that owns the window as SDL asks, opens the audio
//! output at each segment's shape, feeds the face the host's word, and
//! says in the Status panel what the sound is doing.

use std::collections::VecDeque;
use std::io::ErrorKind;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use intake::remote::Beacon;
use intake::TapSource;
use lite_client::{parse_text, Event, Shape};
use serde::Serialize;
use tungstenite::{HandshakeError, Message};

#[cfg(target_os = "linux")]
use super::alsa;
use super::RemoteApp;

/// The sound starts once this much is queued: the jitter buffer, a
/// quarter second being right for a LAN.
pub const LEAD_MS: u32 = 250;
/// Past this much queued the ring drops back to the lead: the player's
/// clock and this machine's drift by a few milliseconds a minute.
pub const MOST_MS: u32 = 500;
/// The waits between tries at the player's sound, each longer, the last kept.
const BACKOFF: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];
/// How long a connect may take.
const CONNECT: Duration = Duration::from_secs(3);
/// A read waits this long for a frame before the thread looks at its stop
/// flag and pings the player; a player that stands still sends nothing.
const READ: Duration = Duration::from_secs(5);
/// The route the player's Manager proxies to its sound server.
pub const ROUTE: &str = "/anymote/listen";

/// The queue between the socket and the output: the stream's own PCM
/// frames as they came, at one shape at a time. The output takes them as
/// floats (the system's output, SDL converting) or as they are (a device
/// opened at the stream's shape, bit-perfect at a volume of 100).
pub struct Ring {
    bytes: VecDeque<u8>,
    shape: Option<Shape>,
    gain: f32,
    started: bool,
    /// Frames the trim dropped since the ring was shaped.
    dropped: u64,
    /// The frames last taken, contiguous, and their floats.
    raw: Vec<u8>,
    floats: Vec<f32>,
}

impl Default for Ring {
    fn default() -> Self {
        Self::new()
    }
}

impl Ring {
    pub fn new() -> Self {
        Self {
            bytes: VecDeque::new(),
            shape: None,
            gain: 1.0,
            started: false,
            dropped: 0,
            raw: Vec::new(),
            floats: Vec::new(),
        }
    }

    /// Shaped for a segment: emptied and at its rate, channels and format.
    pub fn reshape(&mut self, shape: &Shape) {
        self.shape = Some(shape.clone());
        self.clear();
        self.dropped = 0;
    }

    /// The rate and channels in force; zeros before any segment.
    pub fn shape(&self) -> (u32, u32) {
        self.shape
            .as_ref()
            .map(|s| (s.rate, s.channels))
            .unwrap_or((0, 0))
    }

    fn frame_bytes(&self) -> usize {
        self.shape
            .as_ref()
            .map(|s| s.frame_bytes as usize)
            .unwrap_or(1)
            .max(1)
    }

    fn rate(&self) -> usize {
        self.shape.as_ref().map(|s| s.rate as usize).unwrap_or(0)
    }

    fn channels(&self) -> usize {
        self.shape
            .as_ref()
            .map(|s| s.channels as usize)
            .unwrap_or(1)
            .max(1)
    }

    fn bytes_in(&self, ms: u32) -> usize {
        self.rate() * self.frame_bytes() * ms as usize / 1000
    }

    /// How much is queued, in milliseconds of sound.
    pub fn queued_ms(&self) -> u32 {
        let per_second = (self.rate() * self.frame_bytes()).max(1);
        (self.bytes.len() * 1000 / per_second) as u32
    }

    /// The volume on this device, 0 to 100, as the page's slider: a share
    /// of the level, the player's own stream being at full level.
    pub fn set_gain(&mut self, percent: u8) {
        self.gain = percent.min(100) as f32 / 100.0;
    }

    #[cfg(test)]
    pub fn gain(&self) -> f32 {
        self.gain
    }

    /// Frames in, as they came; past `MOST_MS` the oldest go, back to the
    /// lead, whole frames at a time. Nothing is taken before a segment
    /// shaped the ring.
    pub fn push(&mut self, pcm: &[u8]) {
        if self.shape.is_none() {
            return;
        }
        let frame = self.frame_bytes();
        let whole = pcm.len() / frame * frame;
        self.bytes.extend(pcm[..whole].iter().copied());
        let most = self.bytes_in(MOST_MS);
        if self.bytes.len() > most {
            let keep = self.bytes_in(LEAD_MS);
            let drop = (self.bytes.len() - keep) / frame * frame;
            self.bytes.drain(..drop);
            self.dropped += (drop / frame) as u64;
        }
    }

    /// Emptied, the lead to gather again: a gap, the stream's end, a new segment.
    pub fn clear(&mut self) {
        self.bytes.clear();
        self.started = false;
    }

    /// Up to `frames` frames taken into `raw`: none until the lead is
    /// queued, and a ring that runs dry mid-way gives what it has and
    /// waits for the lead again. How many were taken.
    fn take(&mut self, frames: usize) -> usize {
        self.raw.clear();
        if !self.started {
            if self.queued_ms() < LEAD_MS {
                return 0;
            }
            self.started = true;
        }
        let frame = self.frame_bytes();
        let have = (self.bytes.len() / frame).min(frames);
        self.raw.extend(self.bytes.drain(..have * frame));
        if have < frames {
            self.started = false;
        }
        have
    }

    /// The system output's call: `out` filled with the next frames as
    /// floats at the gain, silence where the ring gives nothing. `true`
    /// while sound flowed.
    pub fn pull(&mut self, out: &mut [f32]) -> bool {
        let channels = self.channels();
        let taken = self.take(out.len() / channels);
        let Self {
            raw,
            floats,
            shape,
            gain,
            ..
        } = self;
        floats.clear();
        if let Some(shape) = shape {
            shape.to_f32(raw, floats);
        }
        let n = floats.len().min(out.len());
        for (o, s) in out[..n].iter_mut().zip(floats.iter()) {
            *o = s * *gain;
        }
        out[n..].fill(0.0);
        taken > 0
    }

    /// A device's call: `out` filled with the next frames as they came,
    /// the bits untouched at a volume of 100 and scaled below it, silence
    /// where the ring gives nothing. `true` while sound flowed.
    pub fn pull_raw(&mut self, out: &mut [u8]) -> bool {
        let frame = self.frame_bytes();
        let taken = self.take(out.len() / frame);
        let n = self.raw.len().min(out.len());
        out[..n].copy_from_slice(&self.raw[..n]);
        out[n..].fill(0);
        if self.gain < 1.0 {
            if let Some(shape) = &self.shape {
                scale_in_place(&shape.format, &mut out[..n], self.gain);
            }
        }
        taken > 0
    }

    /// Whether the output is being fed, the lead gathered.
    pub fn playing(&self) -> bool {
        self.started
    }

    #[cfg(test)]
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

/// Samples in the stream's own format scaled by `gain`, in place: the
/// volume below 100 on a device opened at the stream's shape.
pub fn scale_in_place(format: &str, bytes: &mut [u8], gain: f32) {
    match format {
        "s16le" => {
            for s in bytes.as_chunks_mut::<2>().0 {
                let v = (i16::from_le_bytes(*s) as f32 * gain).round() as i16;
                *s = v.to_le_bytes();
            }
        }
        "s24le" => {
            for s in bytes.as_chunks_mut::<3>().0 {
                let v = (i32::from_le_bytes([0, s[0], s[1], s[2]]) >> 8) as f32 * gain;
                let v = (v.round() as i32)
                    .clamp(-(1 << 23), (1 << 23) - 1)
                    .to_le_bytes();
                *s = [v[0], v[1], v[2]];
            }
        }
        "s32le" => {
            for s in bytes.as_chunks_mut::<4>().0 {
                let v = (i32::from_le_bytes(*s) as f64 * gain as f64).round() as i32;
                *s = v.to_le_bytes();
            }
        }
        "f32le" => {
            for s in bytes.as_chunks_mut::<4>().0 {
                let v = f32::from_le_bytes(*s) * gain;
                *s = v.to_le_bytes();
            }
        }
        _ => {}
    }
}

/// The output's pull: the ring's samples while the ring is at the shape
/// the output was opened at, silence otherwise (a new segment is on its
/// way to a new output).
pub struct Tap {
    ring: Arc<Mutex<Ring>>,
    rate: u32,
    channels: u32,
}

impl pane::audio::Pull for Tap {
    fn pull(&mut self, out: &mut [f32]) {
        let mut ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        if ring.shape() == (self.rate, self.channels) {
            ring.pull(out);
        } else {
            out.fill(0.0);
        }
    }
}

/// What the socket thread says to the session, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Word {
    /// Reaching the player's Manager.
    Reaching,
    /// Connected; the sound server's hello (its version).
    Hello(String),
    /// A stream begins, or is on.
    Segment(Shape),
    /// The stream ended; the player stands still.
    End,
    /// This listener fell behind and frames were lost for it.
    Gap(u64),
    /// Too many listeners on the player.
    Full,
    /// The Manager refused the route with this HTTP status: 403 is the
    /// player's switch off, 503 a player without the sound server.
    Refused(u16),
    /// The player could not be reached, with the reason.
    Unreachable(String),
    /// The connection ended; another is tried.
    Closed,
}

/// The socket thread and its ring; stopped and joined on drop.
pub struct Listener {
    stop: Arc<AtomicBool>,
    socket: Arc<Mutex<Option<TcpStream>>>,
    words: Receiver<Word>,
    thread: Option<JoinHandle<()>>,
    pub ring: Arc<Mutex<Ring>>,
}

impl Listener {
    /// Started towards `host:port`, the Manager's; the thread tries again
    /// with a backoff whenever the connection ends, until stopped.
    pub fn start(host: &str, port: u16, gain: u8) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let socket = Arc::new(Mutex::new(None));
        let mut ring = Ring::new();
        ring.set_gain(gain);
        let ring = Arc::new(Mutex::new(ring));
        let (sender, words) = mpsc::channel();
        let thread = {
            let (host, stop, socket, ring) =
                (host.to_string(), stop.clone(), socket.clone(), ring.clone());
            thread::Builder::new()
                .name("play-here".into())
                .spawn(move || listen(&host, port, &stop, &socket, &ring, &sender))
                .ok()
        };
        Self {
            stop,
            socket,
            words,
            thread,
            ring,
        }
    }

    /// The thread's words since the last look.
    pub fn words(&self) -> Vec<Word> {
        let mut said = Vec::new();
        while let Ok(word) = self.words.try_recv() {
            said.push(word);
        }
        said
    }

    /// Stopped: the socket shut under the thread, which leaves at once.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(socket) = self.socket.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = socket.shutdown(Shutdown::Both);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A wait that ends early when stopped.
fn wait(how_long: Duration, stop: &AtomicBool) {
    let until = Instant::now() + how_long;
    while Instant::now() < until && !stop.load(Ordering::Acquire) {
        thread::sleep(Duration::from_millis(100));
    }
}

fn connect(host: &str, port: u16) -> Result<TcpStream, String> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("{host}: {e}"))?;
    let mut last = format!("{host}: no address");
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                let _ = stream.set_read_timeout(Some(READ));
                return Ok(stream);
            }
            Err(e) => last = format!("{address}: {e}"),
        }
    }
    Err(last)
}

/// The thread: connect, take the frames into the ring, say what happens,
/// try again after a wait; until the stop flag, which the shut socket
/// makes immediate.
fn listen(
    host: &str,
    port: u16,
    stop: &AtomicBool,
    socket: &Mutex<Option<TcpStream>>,
    ring: &Mutex<Ring>,
    words: &Sender<Word>,
) {
    let url = format!("ws://{host}:{port}{ROUTE}");
    let mut tries = 0usize;
    let say = |word: Word| {
        let _ = words.send(word);
    };
    while !stop.load(Ordering::Acquire) {
        say(Word::Reaching);
        let stream = match connect(host, port) {
            Ok(stream) => stream,
            Err(e) => {
                say(Word::Unreachable(e));
                wait(BACKOFF[tries.min(BACKOFF.len() - 1)], stop);
                tries += 1;
                continue;
            }
        };
        *socket.lock().unwrap_or_else(|e| e.into_inner()) = stream.try_clone().ok();
        let mut ws = match tungstenite::client(url.as_str(), stream) {
            Ok((ws, _)) => ws,
            Err(HandshakeError::Failure(tungstenite::Error::Http(response))) => {
                say(Word::Refused(response.status().as_u16()));
                *socket.lock().unwrap_or_else(|e| e.into_inner()) = None;
                wait(BACKOFF[BACKOFF.len() - 1], stop);
                continue;
            }
            Err(e) => {
                say(Word::Unreachable(e.to_string()));
                *socket.lock().unwrap_or_else(|e| e.into_inner()) = None;
                wait(BACKOFF[tries.min(BACKOFF.len() - 1)], stop);
                tries += 1;
                continue;
            }
        };
        tries = 0;
        let mut shape: Option<Shape> = None;
        loop {
            let message = match ws.read() {
                Ok(message) => message,
                // Nothing for a while: the player stands still, or is gone;
                // a ping tells the two apart in time, and the stop flag is
                // looked at.
                Err(tungstenite::Error::Io(e))
                    if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    if stop.load(Ordering::Acquire)
                        || ws.send(Message::Ping(Vec::new().into())).is_err()
                    {
                        break;
                    }
                    continue;
                }
                Err(_) => break,
            };
            match message {
                Message::Text(text) => {
                    match parse_text(&text).unwrap_or_else(|_| Event::Other(text.to_string())) {
                        Event::Hello { version, .. } => say(Word::Hello(version)),
                        Event::Segment(segment) => {
                            ring.lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .reshape(&segment);
                            shape = Some(segment.clone());
                            say(Word::Segment(segment));
                        }
                        Event::End => {
                            ring.lock().unwrap_or_else(|e| e.into_inner()).clear();
                            say(Word::End);
                        }
                        Event::Gap { dropped } => {
                            ring.lock().unwrap_or_else(|e| e.into_inner()).clear();
                            say(Word::Gap(dropped));
                        }
                        Event::Full => {
                            say(Word::Full);
                            break;
                        }
                        Event::Pcm(_) | Event::Other(_) | Event::Closed => {}
                    }
                }
                Message::Binary(pcm) => {
                    if shape.as_ref().is_some_and(|s| s.known() && !s.one_bit) {
                        ring.lock().unwrap_or_else(|e| e.into_inner()).push(&pcm);
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        ring.lock().unwrap_or_else(|e| e.into_inner()).clear();
        *socket.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if stop.load(Ordering::Acquire) {
            break;
        }
        say(Word::Closed);
        wait(BACKOFF[tries.min(BACKOFF.len() - 1)], stop);
        tries += 1;
    }
}

/// What the sound is doing here, for the Status panel and the journal.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Off,
    OffOnPlayer,
    Reaching,
    Waiting,
    Playing,
    Still,
    PlayerOnly,
    Full,
    NoSoundServer,
    Unreachable(String),
    NoOutput(String),
}

impl State {
    /// The Status panel's words.
    pub fn words(&self) -> String {
        match self {
            State::Off => "off".to_string(),
            State::OffOnPlayer => "off on the player (its System tab, Play here)".to_string(),
            State::Reaching => "reaching the player's sound".to_string(),
            State::Waiting => "waiting for the stream".to_string(),
            State::Playing => "playing".to_string(),
            State::Still => "the player stands still".to_string(),
            State::PlayerOnly => "plays on the player only: a DSD stream".to_string(),
            State::Full => "too many listeners on the player (eight at once)".to_string(),
            State::NoSoundServer => "the player has no sound server in its build".to_string(),
            State::Unreachable(why) => format!("not reachable: {why}"),
            State::NoOutput(why) => format!("no audio output here: {why}"),
        }
    }
}

/// A segment's shape in words: `44.1 kHz 16-bit stereo`.
pub fn shape_words(shape: &Shape) -> String {
    let khz = shape.rate as f32 / 1000.0;
    let khz = if (khz - khz.round()).abs() < 0.05 {
        format!("{}", khz.round() as u32)
    } else {
        format!("{khz:.1}")
    };
    let bits = if shape.format == "f32le" {
        "32-bit float".to_string()
    } else {
        format!("{}-bit", shape.bits)
    };
    let channels = match shape.channels {
        1 => "mono".to_string(),
        2 => "stereo".to_string(),
        n => format!("{n} channels"),
    };
    format!("{khz} kHz {bits} {channels}")
}

/// The Status panel's Play here block.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct PlayHereStatus {
    pub on: bool,
    pub gain: u8,
    pub state: String,
    pub shape: String,
    /// The output in force while a stream is on: the system's output
    /// through its driver, or bit-perfect through a device, with the
    /// reason where the device named could not be used.
    pub output: String,
}

/// A device opened at the stream's shape (Linux, ALSA), fed by a thread
/// of its own from the ring in whole periods, paced by the device's own
/// clock; a write that fails is kept for the session, which falls back to
/// the system's output.
#[cfg(target_os = "linux")]
struct Direct {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    failed: Arc<Mutex<Option<String>>>,
}

#[cfg(target_os = "linux")]
impl Direct {
    fn start(mut device: alsa::Device, ring: Arc<Mutex<Ring>>, shape: &Shape) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(Mutex::new(None));
        // A period of 20 ms a write, as the stream's chunks are.
        let frame = (shape.frame_bytes as usize).max(1);
        let period = (shape.rate as usize * 20 / 1000).max(1) * frame;
        let (rate, channels) = (shape.rate, shape.channels);
        let thread = {
            let (stop, failed) = (stop.clone(), failed.clone());
            thread::Builder::new()
                .name("play-here-alsa".into())
                .spawn(move || {
                    let mut buf = vec![0u8; period];
                    while !stop.load(Ordering::Acquire) {
                        let flowed = {
                            let mut ring = ring.lock().unwrap_or_else(|e| e.into_inner());
                            if ring.shape() == (rate, channels) {
                                ring.pull_raw(&mut buf)
                            } else {
                                buf.fill(0);
                                false
                            }
                        };
                        if let Err(why) = device.write(&buf) {
                            *failed.lock().unwrap_or_else(|e| e.into_inner()) = Some(why);
                            break;
                        }
                        // The device paces the writes by its own clock; a
                        // device that takes silence at once (ALSA's null,
                        // a plugin without a clock) is not spun on.
                        if !flowed {
                            thread::sleep(Duration::from_millis(10));
                        }
                    }
                })
                .ok()
        };
        Self {
            stop,
            thread,
            failed,
        }
    }

    fn failed(&self) -> Option<String> {
        self.failed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

#[cfg(target_os = "linux")]
impl Drop for Direct {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The device named opened at the stream's shape and its writer started;
/// the error says what stood in the way (no library, a format ALSA does
/// not take, a device held or short of the shape).
#[cfg(target_os = "linux")]
fn open_direct(
    device: &str,
    shape: &Shape,
    ring: &Arc<Mutex<Ring>>,
) -> Result<(Direct, &'static str), String> {
    let lib = alsa::Lib::load()?;
    let (format, name) = alsa::format_of(&shape.format)
        .ok_or_else(|| format!("{} is not a format ALSA takes", shape.format))?;
    let opened = alsa::Device::open(
        lib,
        device,
        format,
        shape.channels,
        shape.rate,
        shape.frame_bytes as usize,
        LEAD_MS * 1000,
    )?;
    Ok((Direct::start(opened, ring.clone(), shape), name))
}

/// The session's side: the listener while the sound is wanted and served,
/// the output at the stream's shape, the face told the host's word, the
/// status kept.
pub struct Run {
    listener: Option<Listener>,
    output: Option<pane::audio::Output<Tap>>,
    shape: Option<Shape>,
    state: State,
    /// What the face was last told: on and the gain.
    told: Option<(bool, u8)>,
    /// The last words written to the journal, written once per change.
    said: String,
    /// The ALSA device named for a bit-perfect output on Linux; empty is
    /// the system's output.
    device: String,
    #[cfg(target_os = "linux")]
    direct: Option<Direct>,
    /// The output in force, in words, for the Status panel.
    through: String,
}

impl Run {
    pub fn new(device: String) -> Self {
        Self {
            listener: None,
            output: None,
            shape: None,
            state: State::Off,
            told: None,
            said: String::new(),
            device,
            #[cfg(target_os = "linux")]
            direct: None,
            through: String::new(),
        }
    }

    /// Both outputs shut; the next segment opens one again.
    fn close_outputs(&mut self) {
        self.output = None;
        #[cfg(target_os = "linux")]
        {
            self.direct = None;
        }
        self.through.clear();
    }

    fn any_output(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.output.is_some() || self.direct.is_some()
        }
        #[cfg(not(target_os = "linux"))]
        {
            self.output.is_some()
        }
    }

    /// The system's output opened at the shape, SDL converting; a note
    /// (why the device named was not used) is kept in the words.
    fn open_system(&mut self, shape: &Shape, ring: &Arc<Mutex<Ring>>, note: Option<String>) {
        let tap = Tap {
            ring: ring.clone(),
            rate: shape.rate,
            channels: shape.channels,
        };
        match pane::audio::Output::open(shape.rate, shape.channels.min(255) as u8, tap) {
            Ok(output) => {
                logline::say!(
                    Info,
                    "remotes",
                    "play here: {} through {}",
                    shape_words(shape),
                    output.driver()
                );
                self.through = match note {
                    Some(why) => format!("the system's output through {} ({why})", output.driver()),
                    None => format!("the system's output through {}", output.driver()),
                };
                self.output = Some(output);
                self.state = State::Playing;
            }
            Err(e) => {
                logline::say!(Warn, "remotes", "play here: no audio output: {e}");
                self.state = State::NoOutput(e);
            }
        }
    }

    /// An output opened at a new segment's shape: the device named first,
    /// bit-perfect, else the system's output with the reason noted.
    fn open_output(&mut self, shape: &Shape, ring: &Arc<Mutex<Ring>>) {
        self.close_outputs();
        #[cfg(target_os = "linux")]
        if !self.device.is_empty() {
            match open_direct(&self.device, shape, ring) {
                Ok((direct, format)) => {
                    logline::say!(
                        Info,
                        "remotes",
                        "play here: {} bit-perfect through {} ({format})",
                        shape_words(shape),
                        self.device
                    );
                    self.through = format!("bit-perfect through {} ({format})", self.device);
                    self.direct = Some(direct);
                    self.state = State::Playing;
                    return;
                }
                Err(why) => {
                    logline::say!(
                        Warn,
                        "remotes",
                        "play here: {why}; the system's output instead"
                    );
                    self.open_system(shape, ring, Some(why));
                    return;
                }
            }
        }
        self.open_system(shape, ring, None);
    }

    /// A command of the face's: `play-here` with `on` and `gain` is this
    /// remote's own affair and never the player's; `true` when taken.
    pub fn command(&mut self, app: &RemoteApp, command: &intake::Command) -> bool {
        if command.name != "play-here" {
            return false;
        }
        let value = command.value.as_ref();
        let on = value.and_then(|v| v.get("on")).and_then(|v| v.as_bool());
        let gain = value
            .and_then(|v| v.get("gain"))
            .and_then(|v| v.as_u64())
            .map(|g| g.min(100) as u8);
        app.set_play_here(on, gain);
        true
    }

    /// Once a frame: the listener started or stopped as the switch and the
    /// player's word say, the thread's words taken, the output opened at a
    /// new segment, the face told, the status kept.
    pub fn tick(&mut self, app: &RemoteApp, beacon: &Beacon, source: &mut TapSource, served: bool) {
        let (on, gain) = app.play_here();
        if on && served {
            if self.listener.is_none() {
                logline::say!(
                    Info,
                    "remotes",
                    "play here: reaching ws://{}:{}{ROUTE}",
                    beacon.address(),
                    beacon.manager_port
                );
                self.listener = Some(Listener::start(
                    &beacon.address(),
                    beacon.manager_port,
                    gain,
                ));
                self.state = State::Reaching;
            }
            let listener = self.listener.as_ref().expect("started above");
            listener
                .ring
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .set_gain(gain);
            let ring = listener.ring.clone();
            for word in listener.words() {
                match word {
                    Word::Reaching => self.state = State::Reaching,
                    Word::Hello(version) => {
                        logline::say!(
                            Info,
                            "remotes",
                            "play here: the player's sound server {version}"
                        );
                        self.state = State::Waiting;
                    }
                    Word::Segment(shape) => {
                        if shape.one_bit || !shape.known() {
                            self.close_outputs();
                            self.state = State::PlayerOnly;
                        } else {
                            self.open_output(&shape, &ring);
                        }
                        self.shape = Some(shape);
                    }
                    Word::End => self.state = State::Still,
                    Word::Gap(dropped) => {
                        logline::say!(
                            Verbose,
                            "remotes",
                            "play here: fell behind, {dropped} frames lost"
                        );
                    }
                    Word::Full => self.state = State::Full,
                    Word::Refused(403) => self.state = State::OffOnPlayer,
                    Word::Refused(503) => self.state = State::NoSoundServer,
                    Word::Refused(code) => self.state = State::Unreachable(format!("HTTP {code}")),
                    Word::Unreachable(why) => self.state = State::Unreachable(why),
                    Word::Closed => {
                        self.close_outputs();
                        self.state = State::Reaching;
                    }
                }
            }
            // A device that failed mid-stream: said, and the stream goes on
            // through the system's output.
            #[cfg(target_os = "linux")]
            if let Some(why) = self.direct.as_ref().and_then(|d| d.failed()) {
                logline::say!(
                    Warn,
                    "remotes",
                    "play here: {why}; the system's output instead"
                );
                self.direct = None;
                if let Some(shape) = self.shape.clone() {
                    self.open_system(&shape, &ring, Some(why));
                }
            }
            // A stream on and the output open: playing while the ring feeds
            // it, waiting while the lead gathers.
            if self.any_output() && matches!(self.state, State::Playing | State::Waiting) {
                let playing = ring.lock().unwrap_or_else(|e| e.into_inner()).playing();
                self.state = if playing {
                    State::Playing
                } else {
                    State::Waiting
                };
            }
        } else {
            if let Some(mut listener) = self.listener.take() {
                self.close_outputs();
                self.shape = None;
                listener.stop();
                logline::say!(
                    Info,
                    "remotes",
                    "play here: stopped ({})",
                    if on {
                        "the player stopped serving the sound"
                    } else {
                        "the switch went off"
                    }
                );
            }
            self.state = if on { State::OffOnPlayer } else { State::Off };
        }
        // The face's tile wears the host's word, and its page the gain.
        let word = (on && served, gain);
        if self.told != Some(word) {
            source.push_event(intake::Event::PlayHere {
                served: None,
                on: word.0,
                gain,
            });
            self.told = Some(word);
        }
        let words = self.state.words();
        let shape = match (&self.state, &self.shape) {
            (State::Playing | State::Waiting | State::Still, Some(shape)) => shape_words(shape),
            _ => String::new(),
        };
        if words != self.said {
            logline::say!(Info, "remotes", "play here: {words}");
            self.said = words.clone();
        }
        let output = if self.any_output() {
            self.through.clone()
        } else {
            String::new()
        };
        app.set_status(|s| {
            s.play_here = PlayHereStatus {
                on,
                gain,
                state: words,
                shape,
                output,
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16-bit stereo shape at `rate`.
    fn s16(rate: u32) -> Shape {
        Shape {
            rate,
            channels: 2,
            format: "s16le".into(),
            bits: 16,
            frame_bytes: 4,
            one_bit: false,
            capped: false,
            chunk_ms: 20,
        }
    }

    /// A ring shaped 16-bit stereo at `rate`.
    fn ring16(rate: u32) -> Ring {
        let mut ring = Ring::new();
        ring.reshape(&s16(rate));
        ring
    }

    /// `ms` of 16-bit stereo frames at half scale (0x4000), as bytes.
    fn ms(ring: &Ring, ms: u32) -> Vec<u8> {
        let frames = ring.bytes_in(ms) / 4;
        (0..frames * 2)
            .flat_map(|_| 0x4000i16.to_le_bytes())
            .collect()
    }

    #[test]
    fn the_ring_gathers_the_lead_before_any_sound_and_pauses_when_dry() {
        let mut ring = ring16(48_000);
        let mut out = vec![1.0; 96 * 10]; // 10 ms of floats
        assert!(!ring.pull(&mut out), "nothing queued: silence");
        assert!(out.iter().all(|s| *s == 0.0));
        let a_bit = ms(&ring, LEAD_MS - 10);
        ring.push(&a_bit);
        assert!(!ring.pull(&mut out), "short of the lead: still silence");
        assert_eq!(ring.queued_ms(), LEAD_MS - 10, "silence takes nothing");
        ring.push(&ms(&ring, 20));
        assert!(ring.pull(&mut out), "the lead is there: sound");
        assert!(
            out.iter().all(|s| (*s - 0.5).abs() < 1e-3),
            "{:?}",
            &out[..4]
        );
        assert!(ring.playing());
        // Everything out, and more asked: the rest is silence and the lead
        // gathers again before the next sound.
        let mut much = vec![1.0; ring.bytes_in(MOST_MS) / 2];
        assert!(ring.pull(&mut much));
        assert!(!ring.playing(), "ran dry: paused");
        assert_eq!(much[ring.bytes_in(LEAD_MS + 10) / 2], 0.0);
        ring.push(&ms(&ring, 100));
        assert!(!ring.pull(&mut out), "the lead gathers again");
    }

    #[test]
    fn the_ring_trims_back_to_the_lead_when_the_clocks_drift_it_long() {
        let mut ring = ring16(44_100);
        ring.push(&ms(&ring, MOST_MS));
        assert_eq!(ring.dropped(), 0);
        ring.push(&ms(&ring, 40));
        assert_eq!(ring.queued_ms(), LEAD_MS);
        assert_eq!(
            ring.dropped() as u32,
            (MOST_MS + 40 - LEAD_MS) * 44_100 / 1000
        );
    }

    #[test]
    fn the_gain_scales_the_sound_and_a_clear_or_reshape_empties_it() {
        let mut ring = ring16(44_100);
        ring.set_gain(50);
        ring.push(&ms(&ring, LEAD_MS));
        let mut out = vec![0.0; 88];
        assert!(ring.pull(&mut out));
        assert!(out.iter().all(|s| (*s - 0.25).abs() < 1e-3));
        ring.clear();
        assert_eq!(ring.queued_ms(), 0);
        assert!(!ring.playing());
        ring.push(&ms(&ring, LEAD_MS));
        ring.reshape(&s16(96_000));
        assert_eq!(ring.shape(), (96_000, 2));
        assert_eq!(ring.queued_ms(), 0);
        assert!(
            (ring.gain() - 0.5).abs() < 1e-6,
            "the gain outlives a reshape"
        );
        let mut unshaped = Ring::new();
        unshaped.push(&[1, 2, 3, 4]);
        assert_eq!(unshaped.queued_ms(), 0, "nothing taken before a segment");
        assert_eq!(unshaped.shape(), (0, 0));
    }

    #[test]
    fn the_raw_pull_passes_the_bits_at_a_hundred_and_scales_below() {
        let mut ring = ring16(44_100);
        let frames: Vec<u8> = (0..ring.bytes_in(LEAD_MS) / 4)
            .flat_map(|i| {
                let v = (i as i16).wrapping_mul(37);
                [v.to_le_bytes(), (-v).to_le_bytes()].concat()
            })
            .collect();
        ring.push(&frames);
        let mut out = vec![0xffu8; 4 * 100];
        assert!(ring.pull_raw(&mut out));
        assert_eq!(&out[..], &frames[..400], "the bytes as they came");
        ring.clear();
        ring.set_gain(50);
        ring.push(&frames);
        assert!(ring.pull_raw(&mut out));
        let first = i16::from_le_bytes([out[8], out[9]]);
        assert_eq!(first, 37, "the third frame's left sample, 74 at half");
        // A part frame at the end is left out, silence filling the rest.
        let mut odd = vec![0xffu8; 4 * 3 + 2];
        assert!(ring.pull_raw(&mut odd));
        assert_eq!(&odd[12..], &[0, 0]);
    }

    #[test]
    fn samples_scale_in_every_format() {
        let mut s16 = 1000i16.to_le_bytes().to_vec();
        scale_in_place("s16le", &mut s16, 0.5);
        assert_eq!(i16::from_le_bytes([s16[0], s16[1]]), 500);
        let mut s24 = (-0x010000i32).to_le_bytes()[..3].to_vec();
        scale_in_place("s24le", &mut s24, 0.5);
        assert_eq!(
            i32::from_le_bytes([0, s24[0], s24[1], s24[2]]) >> 8,
            -0x008000
        );
        let mut s32 = 2_000_000_000i32.to_le_bytes().to_vec();
        scale_in_place("s32le", &mut s32, 0.25);
        assert_eq!(
            i32::from_le_bytes([s32[0], s32[1], s32[2], s32[3]]),
            500_000_000
        );
        let mut f32s = 0.8f32.to_le_bytes().to_vec();
        scale_in_place("f32le", &mut f32s, 0.5);
        assert!((f32::from_le_bytes([f32s[0], f32s[1], f32s[2], f32s[3]]) - 0.4).abs() < 1e-6);
    }

    #[test]
    fn the_tap_gives_silence_while_the_ring_is_at_another_shape() {
        use pane::audio::Pull;
        let ring = Arc::new(Mutex::new(ring16(44_100)));
        let half_second = ms(&ring.lock().unwrap(), 500);
        ring.lock().unwrap().push(&half_second);
        let mut tap = Tap {
            ring: ring.clone(),
            rate: 48_000,
            channels: 2,
        };
        let mut out = vec![1.0; 64];
        tap.pull(&mut out);
        assert!(out.iter().all(|s| *s == 0.0), "another shape: silence");
        assert_eq!(ring.lock().unwrap().queued_ms(), 500, "and nothing taken");
        tap.rate = 44_100;
        tap.pull(&mut out);
        assert!(out.iter().all(|s| (*s - 0.5).abs() < 1e-3));
    }

    #[test]
    fn shapes_and_states_in_words() {
        let shape = Shape {
            rate: 44_100,
            channels: 2,
            format: "s16le".into(),
            bits: 16,
            frame_bytes: 4,
            one_bit: false,
            capped: false,
            chunk_ms: 20,
        };
        assert_eq!(shape_words(&shape), "44.1 kHz 16-bit stereo");
        let hi = Shape {
            rate: 192_000,
            channels: 1,
            format: "f32le".into(),
            bits: 32,
            frame_bytes: 4,
            ..shape.clone()
        };
        assert_eq!(shape_words(&hi), "192 kHz 32-bit float mono");
        assert_eq!(State::Off.words(), "off");
        assert_eq!(
            State::Unreachable("x: refused".into()).words(),
            "not reachable: x: refused"
        );
        assert!(State::OffOnPlayer.words().starts_with("off on the player"));
    }

    #[test]
    fn the_listener_says_the_player_is_unreachable_and_stops_at_once() {
        // Nothing listens on this port: the thread says so and waits for
        // its backoff; a stop ends the wait and the thread.
        let port = {
            let bound = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            bound.local_addr().unwrap().port()
        };
        let mut listener = Listener::start("127.0.0.1", port, 80);
        let started = Instant::now();
        let mut said = Vec::new();
        while started.elapsed() < Duration::from_secs(5) {
            said.extend(listener.words());
            if said.iter().any(|w| matches!(w, Word::Unreachable(_))) {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(said.first(), Some(&Word::Reaching));
        assert!(
            said.iter().any(|w| matches!(w, Word::Unreachable(_))),
            "{said:?}"
        );
        let stopping = Instant::now();
        listener.stop();
        assert!(
            stopping.elapsed() < Duration::from_secs(2),
            "a stop does not wait the backoff out"
        );
    }

    #[test]
    fn the_listener_takes_a_refusal_as_the_players_word() {
        // A stand-in Manager that answers the upgrade with 403, as the real
        // one does while the switch is off.
        let bound = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = bound.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            if let Ok((mut stream, _)) = bound.accept() {
                use std::io::{Read, Write};
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        });
        let mut listener = Listener::start("127.0.0.1", port, 80);
        let started = Instant::now();
        let mut said = Vec::new();
        while started.elapsed() < Duration::from_secs(5) {
            said.extend(listener.words());
            if said.contains(&Word::Refused(403)) {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(said.contains(&Word::Refused(403)), "{said:?}");
        listener.stop();
        let _ = server.join();
    }
}
