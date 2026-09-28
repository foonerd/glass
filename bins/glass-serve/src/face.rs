//! The frames for a browser page: a local socket the manager proxies,
//! serving each datagram the daemon sends as one server-sent event. A
//! browser takes no UDP; it takes an event stream over HTTP, and the
//! manager carries it same-origin. The responder is written here, small
//! and unbuffered: an event is on the wire the moment it is sent, which a
//! general HTTP server's buffered body would not give. One thread
//! accepts, one thread per page streams; a page that stops reading is
//! dropped at its next event or at the keepalive after a quiet stretch.

use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;

/// Events a page may fall behind by before it is dropped.
const BACKLOG: usize = 64;
/// A comment goes out after this long without an event, so a page that
/// went away is found while the player is quiet.
const KEEPALIVE: Duration = Duration::from_secs(10);
/// A request head arrives within this, or the connection is dropped.
const HEAD_WITHIN: Duration = Duration::from_secs(5);
const HEAD_MAX: usize = 4096;

/// The pages connected, each reached through its channel.
#[derive(Clone, Default)]
pub struct Pages {
    senders: Arc<Mutex<Vec<(u64, SyncSender<Arc<[u8]>>)>>>,
    next_id: Arc<Mutex<u64>>,
}

impl Pages {
    /// Serve on the socket at `path`, a stale file removed first; the
    /// accept loop runs on its own thread from here on.
    pub fn serve(path: &Path) -> Result<Self, String> {
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let pages = Self::default();
        let accept = pages.clone();
        std::thread::Builder::new()
            .name("face-accept".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    accept.attach(stream);
                }
            })
            .map_err(|e| format!("face thread: {e}"))?;
        Ok(pages)
    }

    /// A datagram to every page; the count of pages it went to. A page
    /// whose channel is full has fallen behind and is dropped; one whose
    /// channel is closed has gone.
    pub fn send(&self, datagram: &[u8]) -> usize {
        let bytes: Arc<[u8]> = Arc::from(datagram);
        let Ok(mut senders) = self.senders.lock() else {
            return 0;
        };
        senders.retain(|(_, tx)| match tx.try_send(Arc::clone(&bytes)) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
        });
        senders.len()
    }

    /// How many pages are connected.
    pub fn count(&self) -> usize {
        self.senders.lock().map(|s| s.len()).unwrap_or(0)
    }

    fn attach(&self, stream: UnixStream) {
        let pages = self.clone();
        let _ = std::thread::Builder::new()
            .name("face-page".into())
            .spawn(move || pages.page(stream));
    }

    /// One page's connection, from its request to its going.
    fn page(&self, mut stream: UnixStream) {
        let Some(path) = request_path(&mut stream) else {
            return;
        };
        if path != "/events" {
            let _ = stream.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            return;
        }
        let (tx, rx) = sync_channel(BACKLOG);
        let id = {
            let mut next = self.next_id.lock().unwrap_or_else(|e| e.into_inner());
            *next += 1;
            *next
        };
        if let Ok(mut senders) = self.senders.lock() {
            senders.push((id, tx));
        }
        let _ = stream.set_read_timeout(None);
        let served = (|| -> io::Result<()> {
            stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\n\
                  Connection: close\r\nTransfer-Encoding: chunked\r\n\r\n",
            )?;
            chunk(&mut stream, b"retry: 1000\n\n")?;
            loop {
                match rx.recv_timeout(KEEPALIVE) {
                    Ok(bytes) => chunk(&mut stream, event_line(&bytes).as_bytes())?,
                    Err(RecvTimeoutError::Timeout) => chunk(&mut stream, b": keepalive\n\n")?,
                    Err(RecvTimeoutError::Disconnected) => return Ok(()),
                }
            }
        })();
        let _ = served;
        if let Ok(mut senders) = self.senders.lock() {
            senders.retain(|(other, _)| *other != id);
        }
    }
}

/// The request head's path, or none when it does not arrive in time or
/// is not a GET.
fn request_path(stream: &mut UnixStream) -> Option<String> {
    let _ = stream.set_read_timeout(Some(HEAD_WITHIN));
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while head.len() < HEAD_MAX {
        match stream.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            _ => return None,
        }
        if head.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let line = head.split(|b| *b == b'\n').next()?;
    let line = String::from_utf8_lossy(line);
    let mut words = line.split_whitespace();
    if words.next()? != "GET" {
        return None;
    }
    let target = words.next()?;
    Some(target.split('?').next().unwrap_or(target).to_string())
}

/// One chunk of the response, on the wire at once.
fn chunk(stream: &mut UnixStream, text: &[u8]) -> io::Result<()> {
    write!(stream, "{:x}\r\n", text.len())?;
    stream.write_all(text)?;
    stream.write_all(b"\r\n")?;
    stream.flush()
}

/// The event line a datagram becomes, for the page's side to decode.
pub fn event_line(datagram: &[u8]) -> String {
    format!(
        "data: {}\n\n",
        base64::engine::general_purpose::STANDARD.encode(datagram)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};

    fn socket_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("glass-face-{}-{tag}.sock", std::process::id()))
    }

    fn connect(path: &Path, request: &[u8]) -> BufReader<UnixStream> {
        let mut stream = UnixStream::connect(path).expect("connect");
        stream.write_all(request).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        BufReader::new(stream)
    }

    /// The status line and the headers, lowercased names.
    fn head(reader: &mut BufReader<UnixStream>) -> (String, Vec<String>) {
        let mut status = String::new();
        reader.read_line(&mut status).unwrap();
        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
            headers.push(line.trim().to_ascii_lowercase());
        }
        (status.trim().to_string(), headers)
    }

    /// The next lines of text in the chunked body, chunk sizes skipped.
    fn text_lines(reader: &mut BufReader<UnixStream>, count: usize) -> Vec<String> {
        let mut kept = Vec::new();
        while kept.len() < count {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let t = line.trim();
            if t.is_empty() || t.chars().all(|c| c.is_ascii_hexdigit()) {
                continue;
            }
            kept.push(t.to_string());
        }
        kept
    }

    #[test]
    fn a_page_gets_the_retry_hint_then_every_datagram_as_base64() {
        let path = socket_path("events");
        let pages = Pages::serve(&path).expect("a socket");
        let mut reader = connect(&path, b"GET /events?x=1 HTTP/1.1\r\nHost: face\r\n\r\n");
        let (status, headers) = head(&mut reader);
        assert!(status.starts_with("HTTP/1.1 200"), "{status}");
        assert!(
            headers
                .iter()
                .any(|h| h == "content-type: text/event-stream"),
            "{headers:?}"
        );
        assert!(
            headers.iter().any(|h| h == "transfer-encoding: chunked"),
            "{headers:?}"
        );
        assert_eq!(text_lines(&mut reader, 1), ["retry: 1000"]);
        let started = std::time::Instant::now();
        while pages.count() == 0 && started.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(pages.count(), 1);
        assert_eq!(pages.send(b"GLSFdatagram"), 1);
        assert_eq!(pages.send(b"second"), 1);
        assert_eq!(
            text_lines(&mut reader, 2),
            ["data: R0xTRmRhdGFncmFt", "data: c2Vjb25k"]
        );
        drop(reader);
        let started = std::time::Instant::now();
        while pages.count() == 1 && started.elapsed() < Duration::from_secs(2) {
            pages.send(b"after");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(pages.count(), 0, "a page that went is dropped");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn another_path_is_not_found_and_a_bad_request_is_dropped() {
        let path = socket_path("other");
        let pages = Pages::serve(&path).expect("a socket");
        let mut reader = connect(&path, b"GET /nothing HTTP/1.1\r\nHost: face\r\n\r\n");
        let (status, _) = head(&mut reader);
        assert!(status.starts_with("HTTP/1.1 404"), "{status}");
        assert_eq!(pages.count(), 0);
        let mut reader = connect(&path, b"POST /events HTTP/1.1\r\nHost: face\r\n\r\n");
        let mut line = String::new();
        assert_eq!(
            reader.read_line(&mut line).unwrap_or(0),
            0,
            "closed without an answer"
        );
        assert_eq!(pages.send(b"x"), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_event_line_is_the_datagram_in_base64() {
        assert_eq!(event_line(b"abc"), "data: YWJj\n\n");
    }
}
