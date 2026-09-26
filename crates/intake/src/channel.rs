//! The plugin's channel: a local socket the plugin serves, one JSON object
//! per line. A display that connects gets a greeting, then the player's
//! state and the infinity flag as the plugin last saw them, then every
//! change as it comes; it sends commands for the player back. Nothing here
//! blocks the frame loop: the socket is non-blocking and a poll reads what
//! has arrived.

use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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

/// How long a gone socket rests before it is tried again.
const RETRY: Duration = Duration::from_secs(2);
/// A line longer than this is not the plugin's; what was buffered is dropped.
const LINE_MAX: usize = 1 << 20;

/// A connection to the plugin's socket, made again when it goes.
pub struct Channel {
    path: PathBuf,
    stream: Option<UnixStream>,
    pending: Vec<u8>,
    queued: Vec<Event>,
    tried_at: Option<Instant>,
}

impl Channel {
    /// Connects now when the socket is there; otherwise the first poll
    /// after two seconds tries again.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        let mut channel = Self {
            path: path.into(),
            stream: None,
            pending: Vec::new(),
            queued: Vec::new(),
            tried_at: None,
        };
        channel.connect();
        channel
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn connected(&self) -> bool {
        self.stream.is_some()
    }

    fn connect(&mut self) -> bool {
        self.tried_at = Some(Instant::now());
        let Ok(stream) = UnixStream::connect(&self.path) else {
            return false;
        };
        if stream.set_nonblocking(true).is_err() {
            return false;
        }
        self.stream = Some(stream);
        self.pending.clear();
        true
    }

    fn drop_stream(&mut self) {
        self.stream = None;
        self.pending.clear();
        self.tried_at = Some(Instant::now());
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
        let started = Instant::now();
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
        let Some(stream) = self.stream.as_mut() else {
            return false;
        };
        match stream.write_all(command.line().as_bytes()) {
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
fn decode(line: &[u8]) -> Option<Event> {
    let value: Value = serde_json::from_slice(line).ok()?;
    match value.get("kind")?.as_str()? {
        "hello" => Some(Event::Hello {
            protocol: value.get("protocol").and_then(Value::as_u64).unwrap_or(0) as u32,
            plugin: value
                .get("plugin")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        "state" => value
            .get("state")
            .filter(|state| state.is_object())
            .cloned()
            .map(Event::State),
        "infinity" => Some(Event::Infinity(
            value.get("on").and_then(Value::as_bool).unwrap_or(false),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::os::unix::net::UnixListener;
    use std::thread;

    fn socket_path(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("glass-channel-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn pump_until(channel: &mut Channel, count: usize) -> Vec<Event> {
        let started = Instant::now();
        let mut events = Vec::new();
        while events.len() < count && started.elapsed() < Duration::from_secs(5) {
            events.extend(channel.pump());
            thread::sleep(Duration::from_millis(5));
        }
        events
    }

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

    #[test]
    fn a_missing_socket_is_tried_again_after_the_rest() {
        let path = socket_path("missing");
        let mut channel = Channel::at(&path);
        assert!(!channel.connected());
        assert!(channel.pump().is_empty());
        let listener = UnixListener::bind(&path).unwrap();
        assert!(channel.pump().is_empty(), "no try before the rest is over");
        assert!(!channel.connected());
        channel.tried_at = Some(Instant::now() - RETRY);
        assert!(channel.pump().is_empty());
        assert!(channel.connected());
        drop(listener);
        let _ = std::fs::remove_file(&path);
    }

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
        let started = Instant::now();
        while channel.connected() && started.elapsed() < Duration::from_secs(5) {
            channel.pump();
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!channel.connected());
        assert!(!channel.send(&Command::new("toggle")));
        let _ = std::fs::remove_file(&path);
    }
}
