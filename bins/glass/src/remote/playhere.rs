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

/// The queue between the socket and the output: samples as floats,
/// interleaved by channel, at one shape at a time.
pub struct Ring {
    samples: VecDeque<f32>,
    rate: u32,
    channels: u32,
    gain: f32,
    started: bool,
    /// Frames the trim dropped since the ring was shaped.
    dropped: u64,
}

impl Ring {
    pub fn new(rate: u32, channels: u32) -> Self {
        Self {
            samples: VecDeque::new(),
            rate: rate.max(1),
            channels: channels.max(1),
            gain: 1.0,
            started: false,
            dropped: 0,
        }
    }

    /// Shaped for a segment: emptied and at its rate and channels.
    pub fn reshape(&mut self, rate: u32, channels: u32) {
        self.rate = rate.max(1);
        self.channels = channels.max(1);
        self.clear();
        self.dropped = 0;
    }

    pub fn shape(&self) -> (u32, u32) {
        (self.rate, self.channels)
    }

    fn samples_in(&self, ms: u32) -> usize {
        (self.rate as usize * self.channels as usize * ms as usize) / 1000
    }

    /// How much is queued, in milliseconds of sound.
    pub fn queued_ms(&self) -> u32 {
        let per_second = (self.rate as usize * self.channels as usize).max(1);
        (self.samples.len() * 1000 / per_second) as u32
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

    /// Frames in, as decoded; past `MOST_MS` the oldest go, back to the lead.
    pub fn push(&mut self, samples: &[f32]) {
        self.samples.extend(samples.iter().copied());
        let most = self.samples_in(MOST_MS);
        if self.samples.len() > most {
            let keep = self.samples_in(LEAD_MS);
            let drop = self.samples.len() - keep;
            self.samples.drain(..drop);
            self.dropped += (drop / self.channels as usize) as u64;
        }
    }

    /// Emptied, the lead to gather again: a gap, the stream's end, a new segment.
    pub fn clear(&mut self) {
        self.samples.clear();
        self.started = false;
    }

    /// The output's call: `out` filled with the next samples at the gain,
    /// or with silence until the lead is queued; a ring that runs dry
    /// mid-way fills the rest with silence and waits for the lead again.
    /// `true` while sound flowed.
    pub fn pull(&mut self, out: &mut [f32]) -> bool {
        if !self.started {
            if self.queued_ms() < LEAD_MS {
                out.fill(0.0);
                return false;
            }
            self.started = true;
        }
        let have = self.samples.len().min(out.len());
        for (o, s) in out.iter_mut().zip(self.samples.drain(..have)) {
            *o = s * self.gain;
        }
        if have < out.len() {
            out[have..].fill(0.0);
            self.started = false;
        }
        have > 0
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
        let mut ring = Ring::new(44_100, 2);
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
        let mut decoded = Vec::new();
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
                                .reshape(segment.rate, segment.channels);
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
                    if let Some(s) = shape.as_ref().filter(|s| s.known() && !s.one_bit) {
                        decoded.clear();
                        s.to_f32(&pcm, &mut decoded);
                        ring.lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .push(&decoded);
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
}

impl Default for Run {
    fn default() -> Self {
        Self::new()
    }
}

impl Run {
    pub fn new() -> Self {
        Self {
            listener: None,
            output: None,
            shape: None,
            state: State::Off,
            told: None,
            said: String::new(),
        }
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
                            self.output = None;
                            self.state = State::PlayerOnly;
                        } else {
                            let tap = Tap {
                                ring: ring.clone(),
                                rate: shape.rate,
                                channels: shape.channels,
                            };
                            self.output = None;
                            match pane::audio::Output::open(
                                shape.rate,
                                shape.channels.min(255) as u8,
                                tap,
                            ) {
                                Ok(output) => {
                                    logline::say!(
                                        Info,
                                        "remotes",
                                        "play here: {} through {}",
                                        shape_words(&shape),
                                        output.driver()
                                    );
                                    self.output = Some(output);
                                    self.state = State::Playing;
                                }
                                Err(e) => {
                                    logline::say!(
                                        Warn,
                                        "remotes",
                                        "play here: no audio output: {e}"
                                    );
                                    self.state = State::NoOutput(e);
                                }
                            }
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
                        self.output = None;
                        self.state = State::Reaching;
                    }
                }
            }
            // A stream on and the output open: playing while the ring feeds
            // it, waiting while the lead gathers.
            if self.output.is_some() && matches!(self.state, State::Playing | State::Waiting) {
                let playing = ring.lock().unwrap_or_else(|e| e.into_inner()).playing();
                self.state = if playing {
                    State::Playing
                } else {
                    State::Waiting
                };
            }
        } else {
            if let Some(mut listener) = self.listener.take() {
                self.output = None;
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
        app.set_status(|s| {
            s.play_here = PlayHereStatus {
                on,
                gain,
                state: words,
                shape,
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(ring: &Ring, ms: u32) -> Vec<f32> {
        vec![0.5; ring.samples_in(ms)]
    }

    #[test]
    fn the_ring_gathers_the_lead_before_any_sound_and_pauses_when_dry() {
        let mut ring = Ring::new(48_000, 2);
        let mut out = vec![1.0; 96 * 10]; // 10 ms
        assert!(!ring.pull(&mut out), "nothing queued: silence");
        assert!(out.iter().all(|s| *s == 0.0));
        let a_bit = ms(&ring, LEAD_MS - 10);
        ring.push(&a_bit);
        assert!(!ring.pull(&mut out), "short of the lead: still silence");
        assert_eq!(ring.queued_ms(), LEAD_MS - 10, "silence takes nothing");
        ring.push(&ms(&ring, 20));
        assert!(ring.pull(&mut out), "the lead is there: sound");
        assert!(out.iter().all(|s| *s == 0.5));
        assert!(ring.playing());
        // Everything out, and more asked: the rest is silence and the lead
        // gathers again before the next sound.
        let mut much = vec![1.0; ring.samples_in(MOST_MS)];
        assert!(ring.pull(&mut much));
        assert!(!ring.playing(), "ran dry: paused");
        assert_eq!(much[ring.samples_in(LEAD_MS + 10)], 0.0);
        ring.push(&ms(&ring, 100));
        assert!(!ring.pull(&mut out), "the lead gathers again");
    }

    #[test]
    fn the_ring_trims_back_to_the_lead_when_the_clocks_drift_it_long() {
        let mut ring = Ring::new(44_100, 2);
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
        let mut ring = Ring::new(44_100, 2);
        ring.set_gain(50);
        ring.push(&ms(&ring, LEAD_MS));
        let mut out = vec![0.0; 88];
        assert!(ring.pull(&mut out));
        assert!(out.iter().all(|s| (*s - 0.25).abs() < 1e-6));
        ring.clear();
        assert_eq!(ring.queued_ms(), 0);
        assert!(!ring.playing());
        ring.push(&ms(&ring, LEAD_MS));
        ring.reshape(96_000, 2);
        assert_eq!(ring.shape(), (96_000, 2));
        assert_eq!(ring.queued_ms(), 0);
        assert!(
            (ring.gain() - 0.5).abs() < 1e-6,
            "the gain outlives a reshape"
        );
    }

    #[test]
    fn the_tap_gives_silence_while_the_ring_is_at_another_shape() {
        use pane::audio::Pull;
        let ring = Arc::new(Mutex::new(Ring::new(44_100, 2)));
        ring.lock().unwrap().push(&vec![0.5; 44_100]);
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
        assert!(out.iter().all(|s| *s == 0.5));
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
