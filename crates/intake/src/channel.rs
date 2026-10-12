//! The plugin's channel: a local socket the plugin serves, one JSON object
//! per line, or the same over TCP for a remote display. A display that
//! connects gets a greeting, then the player's state, the queue and the
//! infinity flag as the plugin last saw them, then every change as it
//! comes, and word when the configuration changes; it sends commands for the player back,
//! and a remote says who it is. Nothing here blocks the frame loop: the
//! socket is non-blocking and a poll reads what has arrived.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
use std::path::PathBuf;
use std::time::Duration;

use lead::Moment;

use serde::Serialize;
use serde_json::Value;

/// What the plugin sent.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The plugin's greeting: the protocol it speaks and its version.
    Hello { protocol: u32, plugin: String },
    /// The player's state as the player pushes it.
    State(Value),
    /// Whether infinity playback is on.
    Infinity(bool),
    /// The configuration changed: its version, the theme on show and the meter.
    Config {
        version: String,
        theme: String,
        meter: String,
    },
    /// The meter the player's own display shows, for remotes that follow it.
    Showing { theme: String, meter: String },
    /// A meter the player's own display is asked to show: a button pressed
    /// in a browser view, carried to the screen.
    Show { meter: String },
    /// The forecast for the place the user chose, or that there is none to
    /// show: for a face to draw.
    Weather(Option<lead::Weather>),
    /// The player's queue as the plugin pushes it, on connect and on every
    /// change: the tracks in order, for the next line and the queue's length.
    Queue(Vec<QueueItem>),
    /// The persist period after a stop or a pause, as the plugin times it:
    /// the mode (`countdown` or `freeze`, empty when cleared), its length
    /// in seconds and when it began (epoch milliseconds), for a display
    /// that cannot read the persist file, a browser face.
    Persist {
        mode: String,
        seconds: u32,
        started_ms: u64,
    },
    /// The plugin asks the display to calibrate its touch panel: the
    /// display shows targets, reads the fingers and reports the map.
    Calibrate { points: u32 },
    /// The plugin's answer to a remote's probe request: how many probe
    /// datagrams it sent to the remote's frames port, and where to.
    Probed { sent: u32, to: String },
}

/// One track of the player's queue, as the plugin's `queue` line carries it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct QueueItem {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Seconds; 0 when the player does not say.
    pub duration: f32,
}

fn queue_item(item: &Value) -> QueueItem {
    let text = |key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let title = text("title");
    QueueItem {
        title: if title.is_empty() {
            text("name")
        } else {
            title
        },
        artist: text("artist"),
        album: text("album"),
        duration: item
            .get("duration")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .max(0.0) as f32,
    }
}

/// A command for the player, sent to the plugin.
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub name: String,
    pub value: Option<Value>,
}

#[derive(Serialize)]
struct Wire<'a> {
    kind: &'static str,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<&'a Value>,
}

impl Command {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            value: None,
        }
    }

    pub fn with(name: &str, value: Value) -> Self {
        Self {
            name: name.to_string(),
            value: Some(value),
        }
    }

    /// The line the plugin reads, newline included.
    pub fn line(&self) -> String {
        let mut line = serde_json::to_string(&Wire {
            kind: "command",
            name: &self.name,
            value: self.value.as_ref(),
        })
        .unwrap_or_default();
        line.push('\n');
        line
    }
}

/// Who a remote display is, said once after connecting.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RemoteHello {
    pub id: String,
    pub name: String,
    pub release: String,
    pub screen: [u32; 2],
    /// The remote's own settings page, when it serves one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub page: String,
    /// The face the display was built with, by its name and version; left
    /// out by a display without one (the standalone remote).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub face: String,
    /// The UDP port this remote hears frames on, for the plugin's probe;
    /// left out where there is none (a display on the player itself).
    #[serde(default, skip_serializing_if = "port_is_none")]
    pub frames_port: u16,
}

fn port_is_none(port: &u16) -> bool {
    *port == 0
}

impl RemoteHello {
    /// The line the plugin reads, newline included.
    pub fn line(&self) -> String {
        let mut line = serde_json::json!({ "kind": "hello", "remote": self }).to_string();
        line.push('\n');
        line
    }
}

/// How long a gone socket rests before it is tried again.
const RETRY: Duration = Duration::from_secs(2);
/// A line longer than this is not the plugin's; what was buffered is dropped.
const LINE_MAX: usize = 1 << 20;
/// How long a TCP connect may take.
const TCP_CONNECT: Duration = Duration::from_secs(2);

enum Target {
    #[cfg(unix)]
    Path(PathBuf),
    Tcp(String),
}

enum Link {
    #[cfg(unix)]
    Unix(UnixStream),
    Tcp(TcpStream),
}

impl Read for Link {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            #[cfg(unix)]
            Link::Unix(s) => s.read(buf),
            Link::Tcp(s) => s.read(buf),
        }
    }
}

impl Write for Link {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            #[cfg(unix)]
            Link::Unix(s) => s.write(buf),
            Link::Tcp(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            #[cfg(unix)]
            Link::Unix(s) => s.flush(),
            Link::Tcp(s) => s.flush(),
        }
    }
}

/// A connection to the plugin's channel, made again when it goes.
pub struct Channel {
    target: Target,
    stream: Option<Link>,
    pending: Vec<u8>,
    queued: Vec<Event>,
    tried_at: Option<Moment>,
    /// Said again after every connect, so the plugin knows the remote across reconnects.
    hello: Option<RemoteHello>,
}

impl Channel {
    /// Connects now when the socket is there; otherwise the first poll
    /// after two seconds tries again.
    #[cfg(unix)]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self::to(Target::Path(path.into()))
    }

    /// Elsewhere there is no local channel: one that never connects, until
    /// `with_channel` puts a player's TCP channel in its place.
    #[cfg(not(unix))]
    pub fn at(_path: impl AsRef<std::path::Path>) -> Self {
        Self::to(Target::Tcp(String::new()))
    }

    /// The same over TCP, `host:port`, for a remote display.
    pub fn tcp(address: impl Into<String>) -> Self {
        Self::to(Target::Tcp(address.into()))
    }

    fn to(target: Target) -> Self {
        let mut channel = Self {
            target,
            stream: None,
            pending: Vec::new(),
            queued: Vec::new(),
            tried_at: None,
            hello: None,
        };
        channel.connect();
        channel
    }

    /// Where the channel connects, for a log line.
    pub fn name(&self) -> String {
        match &self.target {
            #[cfg(unix)]
            Target::Path(path) => path.display().to_string(),
            Target::Tcp(address) => address.clone(),
        }
    }

    /// Say who this remote is on every connect, starting with the one made
    /// already when there was one.
    pub fn with_hello(mut self, hello: RemoteHello) -> Self {
        self.hello = Some(hello);
        if self.stream.is_some() {
            self.say_hello();
        }
        self
    }

    pub fn connected(&self) -> bool {
        self.stream.is_some()
    }

    fn connect(&mut self) -> bool {
        self.tried_at = Some(Moment::now());
        let link = match &self.target {
            #[cfg(unix)]
            Target::Path(path) => match UnixStream::connect(path) {
                Ok(stream) if stream.set_nonblocking(true).is_ok() => Link::Unix(stream),
                _ => return false,
            },
            Target::Tcp(address) => {
                let Ok(addrs) = address.to_socket_addrs() else {
                    return false;
                };
                let mut found = None;
                for addr in addrs {
                    if let Ok(stream) = TcpStream::connect_timeout(&addr, TCP_CONNECT) {
                        found = Some(stream);
                        break;
                    }
                }
                match found {
                    Some(stream) if stream.set_nonblocking(true).is_ok() => {
                        let _ = stream.set_nodelay(true);
                        Link::Tcp(stream)
                    }
                    _ => return false,
                }
            }
        };
        self.stream = Some(link);
        self.pending.clear();
        self.say_hello();
        true
    }

    fn say_hello(&mut self) {
        if let Some(hello) = self.hello.clone() {
            self.send_line(&hello.line());
        }
    }

    fn drop_stream(&mut self) {
        self.stream = None;
        self.pending.clear();
        self.tried_at = Some(Moment::now());
    }

    /// The events that arrived since the last poll. While the socket is
    /// gone, a connect is tried every two seconds.
    pub fn pump(&mut self) -> Vec<Event> {
        let mut events = std::mem::take(&mut self.queued);
        if self.stream.is_none() {
            let due = self.tried_at.is_none_or(|at| at.elapsed() >= RETRY);
            if !due || !self.connect() {
                return events;
            }
        }
        let mut chunk = [0u8; 8192];
        // A dropped stream ends the loop through its condition.
        while let Some(stream) = self.stream.as_mut() {
            match stream.read(&mut chunk) {
                Ok(0) => self.drop_stream(),
                Ok(n) => {
                    self.pending.extend_from_slice(&chunk[..n]);
                    self.take_lines(&mut events);
                    if self.pending.len() > LINE_MAX {
                        self.pending.clear();
                    }
                }
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) if err.kind() == ErrorKind::Interrupted => {}
                Err(_) => self.drop_stream(),
            }
        }
        events
    }

    fn take_lines(&mut self, events: &mut Vec<Event>) {
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            if let Some(event) = decode(&line[..end]) {
                events.push(event);
            }
        }
    }

    /// Waits up to `limit` for the state that follows a connect, so the
    /// first frame is not painted from nothing. What arrives is kept for
    /// the next [`Channel::pump`].
    pub fn await_state(&mut self, limit: Duration) {
        let started = Moment::now();
        while self.connected() && started.elapsed() < limit {
            let events = self.pump();
            let got_state = events.iter().any(|e| matches!(e, Event::State(_)));
            self.queued.extend(events);
            if got_state {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Sends a command. False when there is no connection or the write
    /// could not be made.
    pub fn send(&mut self, command: &Command) -> bool {
        self.send_line(&command.line())
    }

    /// Ask the plugin to send probe datagrams to this remote's frames port,
    /// so the remote can tell an open UDP path from a blocked one. The
    /// answer comes as [`Event::Probed`].
    pub fn ask_probe(&mut self) -> bool {
        self.send_line("{\"kind\":\"probe\"}\n")
    }

    /// Tell the plugin which meter this display shows, so remotes that
    /// follow the player can show the same one.
    pub fn report_showing(&mut self, theme: &str, meter: &str, rate: u32) -> bool {
        let mut line =
            serde_json::json!({ "kind": "showing", "theme": theme, "meter": meter, "rate": rate })
                .to_string();
        line.push('\n');
        self.send_line(&line)
    }

    fn send_line(&mut self, line: &str) -> bool {
        let Some(stream) = self.stream.as_mut() else {
            return false;
        };
        match stream.write_all(line.as_bytes()) {
            Ok(()) => true,
            Err(err) if err.kind() == ErrorKind::WouldBlock => false,
            Err(_) => {
                self.drop_stream();
                false
            }
        }
    }
}

/// One line from the plugin. Lines that are not JSON objects with a known
/// `kind` are ignored, so a newer plugin may say more than this reads.
/// One line of the plugin's, a JSON object with a `kind`, as an event;
/// `None` for a line of another kind or none at all.
pub fn decode(line: &[u8]) -> Option<Event> {
    let value: Value = serde_json::from_slice(line).ok()?;
    let text = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    match value.get("kind")?.as_str()? {
        "hello" => Some(Event::Hello {
            protocol: value.get("protocol").and_then(Value::as_u64).unwrap_or(0) as u32,
            plugin: text("plugin"),
        }),
        "state" => value
            .get("state")
            .filter(|state| state.is_object())
            .cloned()
            .map(Event::State),
        "infinity" => Some(Event::Infinity(
            value.get("on").and_then(Value::as_bool).unwrap_or(false),
        )),
        "config" => Some(Event::Config {
            version: text("version"),
            theme: text("theme"),
            meter: text("meter"),
        }),
        "showing" => Some(Event::Showing {
            theme: text("theme"),
            meter: text("meter"),
        }),
        "show" => Some(Event::Show {
            meter: text("meter"),
        }),
        // A line that says `off`, or is no forecast, clears the one held.
        "weather" => Some(Event::Weather(
            if value.get("off").and_then(Value::as_bool).unwrap_or(false) {
                None
            } else {
                serde_json::from_value(value.clone()).ok()
            },
        )),
        "queue" => Some(Event::Queue(
            value
                .get("items")
                .and_then(Value::as_array)
                .map(|items| items.iter().map(queue_item).collect())
                .unwrap_or_default(),
        )),
        "persist" => Some(Event::Persist {
            mode: text("mode"),
            seconds: value.get("seconds").and_then(Value::as_u64).unwrap_or(0) as u32,
            started_ms: value.get("startedAt").and_then(Value::as_u64).unwrap_or(0),
        }),
        "calibrate" => Some(Event::Calibrate {
            points: value
                .get("points")
                .and_then(Value::as_u64)
                .unwrap_or(5)
                .clamp(3, 9) as u32,
        }),
        "probed" => Some(Event::Probed {
            sent: value
                .get("sent")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .min(u32::MAX as u64) as u32,
            to: text("to"),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod decode_tests {
    use super::*;

    /// The plugin's answer to a probe request: how many datagrams went
    /// where; a count that is no number is none sent.
    #[test]
    fn the_probed_line_is_decoded() {
        assert_eq!(
            decode(br#"{"kind":"probed","sent":3,"to":"10.0.0.7:5585"}"#),
            Some(Event::Probed {
                sent: 3,
                to: "10.0.0.7:5585".to_string()
            })
        );
        assert_eq!(
            decode(br#"{"kind":"probed","sent":"x"}"#),
            Some(Event::Probed {
                sent: 0,
                to: String::new()
            })
        );
    }

    /// The persist line carries the mode, the seconds and when it began;
    /// a cleared one has an empty mode; an unknown kind is nothing.
    #[test]
    fn the_persist_line_is_decoded_and_an_unknown_kind_is_not() {
        assert_eq!(
            decode(
                br#"{"kind":"persist","mode":"countdown","seconds":15,"startedAt":1700000000000}"#
            ),
            Some(Event::Persist {
                mode: "countdown".into(),
                seconds: 15,
                started_ms: 1_700_000_000_000
            })
        );
        assert_eq!(
            decode(br#"{"kind":"persist","mode":"","seconds":0,"startedAt":0}"#),
            Some(Event::Persist {
                mode: String::new(),
                seconds: 0,
                started_ms: 0
            })
        );
        assert_eq!(decode(br#"{"kind":"whatever"}"#), None);
        assert_eq!(
            decode(br#"{"kind":"calibrate","points":5}"#),
            Some(Event::Calibrate { points: 5 })
        );
        assert_eq!(
            decode(br#"{"kind":"calibrate"}"#),
            Some(Event::Calibrate { points: 5 }),
            "five targets unless said"
        );
        assert_eq!(
            decode(br#"{"kind":"calibrate","points":99}"#),
            Some(Event::Calibrate { points: 9 }),
            "nine at most"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    #[cfg(unix)]
    use std::os::unix::net::UnixListener;
    use std::thread;

    #[cfg(unix)]
    fn socket_path(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("glass-channel-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn pump_until(channel: &mut Channel, count: usize) -> Vec<Event> {
        let started = Moment::now();
        let mut events = Vec::new();
        while events.len() < count && started.elapsed() < Duration::from_secs(5) {
            events.extend(channel.pump());
            thread::sleep(Duration::from_millis(5));
        }
        events
    }

    #[cfg(unix)]
    #[test]
    fn lines_arrive_whole_or_in_pieces_and_a_command_goes_back() {
        let path = socket_path("lines");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            conn.write_all(
                b"{\"kind\":\"hello\",\"protocol\":1,\"plugin\":\"0.5.7\"}\n{\"kind\":\"sta",
            )
            .unwrap();
            thread::sleep(Duration::from_millis(20));
            conn.write_all(b"te\",\"state\":{\"status\":\"play\",\"title\":\"Wonder\"}}\n")
                .unwrap();
            conn.write_all(
                b"not json\n{\"kind\":\"later\",\"x\":1}\n{\"kind\":\"infinity\",\"on\":true}\n",
            )
            .unwrap();
            let mut line = String::new();
            BufReader::new(conn).read_line(&mut line).unwrap();
            line
        });
        let mut channel = Channel::at(&path);
        assert!(channel.connected());
        channel.await_state(Duration::from_secs(5));
        let events = pump_until(&mut channel, 3);
        assert_eq!(
            events[0],
            Event::Hello {
                protocol: 1,
                plugin: "0.5.7".into()
            }
        );
        assert!(matches!(&events[1], Event::State(state) if state["title"] == "Wonder"));
        assert_eq!(events[2], Event::Infinity(true));
        assert_eq!(
            events.len(),
            3,
            "the line that is not JSON and the unknown kind are ignored"
        );
        assert!(channel.send(&Command::with("volume", Value::from(30))));
        assert_eq!(
            server.join().unwrap(),
            "{\"kind\":\"command\",\"name\":\"volume\",\"value\":30}\n"
        );
        assert_eq!(
            Command::new("toggle").line(),
            "{\"kind\":\"command\",\"name\":\"toggle\"}\n"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(unix)]
    #[test]
    fn a_missing_socket_is_tried_again_after_the_rest() {
        let path = socket_path("missing");
        let mut channel = Channel::at(&path);
        assert!(!channel.connected());
        assert!(channel.pump().is_empty());
        let listener = UnixListener::bind(&path).unwrap();
        assert!(channel.pump().is_empty(), "no try before the rest is over");
        assert!(!channel.connected());
        // The rest is over: a moment never tried is due at once.
        channel.tried_at = None;
        assert!(channel.pump().is_empty());
        assert!(channel.connected());
        drop(listener);
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(unix)]
    #[test]
    fn the_peer_leaving_ends_the_connection() {
        let path = socket_path("leaving");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (conn, _) = listener.accept().unwrap();
            drop(conn);
        });
        let mut channel = Channel::at(&path);
        assert!(channel.connected());
        server.join().unwrap();
        let started = Moment::now();
        while channel.connected() && started.elapsed() < Duration::from_secs(5) {
            channel.pump();
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!channel.connected());
        assert!(!channel.send(&Command::new("toggle")));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn over_tcp_a_remote_says_hello_first_and_hears_of_the_configuration() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(conn.try_clone().unwrap());
            let mut hello = String::new();
            reader.read_line(&mut hello).unwrap();
            conn.write_all(
                b"{\"kind\":\"hello\",\"protocol\":1,\"plugin\":\"0.7.0\"}\n{\"kind\":\"config\",\"version\":\"abcd1234\",\"theme\":\"1280x720_x\",\"meter\":\"gold\"}\n",
            )
            .unwrap();
            let mut command = String::new();
            reader.read_line(&mut command).unwrap();
            (hello, command)
        });
        let mut channel = Channel::tcp(address.to_string()).with_hello(RemoteHello {
            id: "kitchen".into(),
            name: "Kitchen".into(),
            release: "0.7.0".into(),
            screen: [1280, 720],
            page: "http://10.0.0.7:5583/".into(),
            face: String::new(),
            frames_port: 5585,
        });
        assert!(channel.connected());
        assert_eq!(channel.name(), address.to_string());
        let events = pump_until(&mut channel, 2);
        assert_eq!(
            events[1],
            Event::Config {
                version: "abcd1234".into(),
                theme: "1280x720_x".into(),
                meter: "gold".into()
            }
        );
        assert!(channel.send(&Command::new("toggle")));
        let (hello, command) = server.join().unwrap();
        assert_eq!(
            hello,
            "{\"kind\":\"hello\",\"remote\":{\"frames_port\":5585,\"id\":\"kitchen\",\"name\":\"Kitchen\",\"page\":\"http://10.0.0.7:5583/\",\"release\":\"0.7.0\",\"screen\":[1280,720]}}\n"
        );
        assert_eq!(command, "{\"kind\":\"command\",\"name\":\"toggle\"}\n");
    }

    #[test]
    fn the_meter_on_show_is_parsed_and_reported() {
        let event = decode(br#"{"kind":"showing","theme":"1280x720_x","meter":"gold"}"#)
            .expect("a showing line");
        assert_eq!(
            event,
            Event::Showing {
                theme: "1280x720_x".into(),
                meter: "gold".into()
            }
        );
        assert_eq!(
            decode(br#"{"kind":"show","meter":"gold"}"#),
            Some(Event::Show {
                meter: "gold".into()
            }),
            "a meter asked for by name"
        );
        assert_eq!(decode(br#"{"kind":"unknown"}"#), None);
    }

    #[test]
    fn the_forecast_is_parsed_and_off_or_a_broken_line_clears_it() {
        let line = br#"{"kind":"weather","place":"Krakow","unit":"C","now":13.6,"code":3,"day":true,"today":61,"low":8.9,"high":17.2,"rain":64,"at":1760000000}"#;
        let Some(Event::Weather(Some(weather))) = decode(line) else {
            panic!("a forecast line");
        };
        assert_eq!(
            (
                weather.place.as_str(),
                weather.unit.as_str(),
                weather.now,
                weather.code,
                weather.day
            ),
            ("Krakow", "C", Some(13.6), 3, true)
        );
        assert_eq!(
            (
                weather.today,
                weather.low,
                weather.high,
                weather.rain,
                weather.at
            ),
            (61, 8.9, 17.2, Some(64), 1_760_000_000)
        );
        let bare = br#"{"kind":"weather","place":"x","unit":"F","now":null,"code":0,"day":false,"today":0,"low":-4,"high":1,"rain":null,"at":1}"#;
        let Some(Event::Weather(Some(weather))) = decode(bare) else {
            panic!("a forecast with no current values");
        };
        assert_eq!((weather.now, weather.rain, weather.low), (None, None, -4.0));
        assert_eq!(
            decode(br#"{"kind":"weather","off":true}"#),
            Some(Event::Weather(None))
        );
        assert_eq!(
            decode(br#"{"kind":"weather","high":"warm"}"#),
            Some(Event::Weather(None)),
            "no forecast in the line: none is shown"
        );
    }

    #[test]
    fn the_queue_is_parsed_with_the_name_for_a_title_and_no_duration_as_zero() {
        let event = decode(
            br#"{"kind":"queue","items":[{"name":"One","artist":"A","album":"X","duration":12.5,"uri":"mnt/x"},{"title":"Two","name":"not this"}]}"#,
        )
        .expect("a queue line");
        assert_eq!(
            event,
            Event::Queue(vec![
                QueueItem {
                    title: "One".into(),
                    artist: "A".into(),
                    album: "X".into(),
                    duration: 12.5
                },
                QueueItem {
                    title: "Two".into(),
                    ..Default::default()
                },
            ])
        );
        assert_eq!(
            decode(br#"{"kind":"queue"}"#),
            Some(Event::Queue(Vec::new()))
        );
    }

    #[test]
    fn a_player_that_is_not_there_is_tried_again_later() {
        let mut channel = Channel::tcp("127.0.0.1:1");
        assert!(!channel.connected());
        assert!(channel.pump().is_empty());
        assert!(!channel.send(&Command::new("toggle")));
    }
}
