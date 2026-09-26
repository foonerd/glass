//! `glass-serve` sends the tap's frames to remote displays. It reads the
//! live ring under `/dev/shm` the way the display does, merges the hops
//! that arrived since its last look, and sends one datagram per look, at
//! most `--rate` a second, to every remote that subscribed. A remote
//! subscribes with a JSON datagram to the same port every few seconds and
//! is forgotten fifteen seconds after its last; no remote, no traffic.
//! The subscribers and the ring's state go to a status file for the
//! manager every two seconds.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tap::ring::{merge_hops, now_ns, Frame, LIVE_NS};
use tap::wire;

const DEFAULT_PORT: u16 = 5580;
const DEFAULT_RATE: u32 = 60;
const DEFAULT_STATUS: &str = "/tmp/glass_serve.json";
/// A remote that has not subscribed again within this is gone.
const SUBSCRIBER_TTL: Duration = Duration::from_secs(15);
/// The ring is looked for again this often while there is none.
const RING_LOOK: Duration = Duration::from_secs(1);
/// A ring not written for this long is silence.
const QUIET_NS: u64 = 500_000_000;
/// How often the status file is written.
const STATUS_EVERY: Duration = Duration::from_secs(2);
/// The loop's rest between looks at the ring and the socket.
const REST: Duration = Duration::from_millis(3);
const SUBSCRIBE_MAX: usize = 2048;

struct Subscriber {
    id: String,
    name: String,
    release: String,
    seen: Instant,
    since: Instant,
    sent: u64,
}

struct Args {
    port: u16,
    rate: u32,
    status: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        port: DEFAULT_PORT,
        rate: DEFAULT_RATE,
        status: PathBuf::from(DEFAULT_STATUS),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--port" => {
                args.port = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--port needs a number from 1 to 65535")?;
            }
            "--rate" => {
                args.rate = it
                    .next()
                    .and_then(|v| v.parse::<u32>().ok())
                    .map(|r| r.clamp(1, 200))
                    .ok_or("--rate needs frames a second")?;
            }
            "--status" => {
                args.status = it
                    .next()
                    .map(PathBuf::from)
                    .ok_or("--status needs a path")?;
            }
            "--help" => {
                println!(
                    "glass-serve [--port {DEFAULT_PORT}] [--rate {DEFAULT_RATE}] [--status {DEFAULT_STATUS}]\n\
                     Sends the tap's frames to remote displays that subscribe on the port, at most --rate a second.\n\
                     Writes the subscribers and the ring's state to the status file every two seconds."
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(args)
}

/// A subscribe datagram: `{"glass":"subscribe","protocol":1,"id":...,"name":...,"release":...}`.
fn parse_subscribe(bytes: &[u8]) -> Option<(String, String, String)> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    if value.get("glass")?.as_str()? != "subscribe" {
        return None;
    }
    let protocol = value.get("protocol").and_then(Value::as_u64).unwrap_or(0);
    if protocol != wire::PROTOCOL as u64 {
        return None;
    }
    let text = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .chars()
            .take(64)
            .collect::<String>()
    };
    Some((text("id"), text("name"), text("release")))
}

fn write_status(
    path: &Path,
    subscribers: &HashMap<SocketAddr, Subscriber>,
    ring: Option<&tap::Reader>,
    sent: u64,
    port: u16,
    rate: u32,
) {
    let now = Instant::now();
    let subs: Vec<Value> = subscribers
        .iter()
        .map(|(addr, s)| {
            json!({
                "address": addr.to_string(),
                "id": s.id,
                "name": s.name,
                "release": s.release,
                "seenMsAgo": now.duration_since(s.seen).as_millis() as u64,
                "sinceS": now.duration_since(s.since).as_secs(),
                "sent": s.sent,
            })
        })
        .collect();
    let ring_value = match ring {
        Some(reader) => {
            let info = reader.info();
            json!({
                "path": reader.path().to_string_lossy(),
                "rate": info.rate,
                "channels": info.channels,
                "bins": info.bins,
                "seq": info.seq,
                "live": reader.is_live(),
            })
        }
        None => Value::Null,
    };
    let status = json!({
        "port": port,
        "rate": rate,
        "protocol": wire::PROTOCOL,
        "subscribers": subs,
        "ring": ring_value,
        "sent": sent,
        "at": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
    });
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, status.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("glass-serve: {err}");
            return ExitCode::from(2);
        }
    };
    let socket = match UdpSocket::bind(("0.0.0.0", args.port)) {
        Ok(socket) => socket,
        Err(err) => {
            eprintln!("glass-serve: port {}: {err}", args.port);
            return ExitCode::from(1);
        }
    };
    if let Err(err) = socket.set_nonblocking(true) {
        eprintln!("glass-serve: socket: {err}");
        return ExitCode::from(1);
    }
    println!(
        "glass-serve: port {} rate {} status {}",
        args.port,
        args.rate,
        args.status.display()
    );

    let interval = Duration::from_micros(1_000_000 / args.rate as u64);
    let dir = Path::new(tap::ring::DIR);
    let mut subscribers: HashMap<SocketAddr, Subscriber> = HashMap::new();
    let mut ring: Option<tap::Reader> = None;
    let mut ring_looked_at: Option<Instant> = None;
    let mut last_seq: u64 = 0;
    let mut pending: Option<Frame> = None;
    let mut pending_hops: Vec<Frame> = Vec::new();
    let mut last_sent_at: Option<Instant> = None;
    let mut silence_sent = false;
    let mut sent: u64 = 0;
    let mut status_at = Instant::now() - STATUS_EVERY;
    let mut buffer = [0u8; SUBSCRIBE_MAX];

    loop {
        let now = Instant::now();

        // Subscriptions, as they come.
        loop {
            match socket.recv_from(&mut buffer) {
                Ok((n, from)) => {
                    if let Some((id, name, release)) = parse_subscribe(&buffer[..n]) {
                        let entry = subscribers.entry(from).or_insert_with(|| {
                            println!("glass-serve: {from} subscribed ({name}, {release})");
                            Subscriber {
                                id: id.clone(),
                                name: name.clone(),
                                release: release.clone(),
                                seen: now,
                                since: now,
                                sent: 0,
                            }
                        });
                        entry.seen = now;
                        entry.id = id;
                        entry.name = name;
                        entry.release = release;
                    }
                }
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) if err.kind() == ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        subscribers.retain(|addr, s| {
            let kept = now.duration_since(s.seen) < SUBSCRIBER_TTL;
            if !kept {
                println!("glass-serve: {addr} gone");
            }
            kept
        });

        // The ring: the live one, looked for again once a second when there is none.
        let live_now = ring.as_ref().is_some_and(tap::Reader::is_live);
        if !live_now && ring_looked_at.is_none_or(|at| now.duration_since(at) >= RING_LOOK) {
            ring_looked_at = Some(now);
            if let Some(found) = tap::Reader::open_live(dir) {
                if ring
                    .as_ref()
                    .map(|r| r.path() != found.path())
                    .unwrap_or(true)
                {
                    println!("glass-serve: ring {}", found.path().display());
                    last_seq = 0;
                }
                ring = Some(found);
            } else if ring.is_some() && !live_now {
                // Kept as it is: a ring gone quiet may write again; a file
                // removed is noticed by is_live.
            }
        }
        let mut quiet = true;
        if let Some(reader) = ring.as_ref() {
            let info = reader.info();
            if now_ns().saturating_sub(info.written_ns) <= QUIET_NS {
                quiet = false;
                if info.seq != last_seq {
                    let first = if last_seq == 0
                        || info.seq.saturating_sub(last_seq) >= info.slots as u64
                    {
                        info.seq
                    } else {
                        last_seq + 1
                    };
                    pending_hops.extend((first..=info.seq).filter_map(|seq| reader.slot(seq)));
                    last_seq = info.seq;
                    if let Some(merged) = merge_hops(pending_hops.drain(..)) {
                        pending = Some(match pending.take() {
                            Some(earlier) => {
                                merge_hops([earlier, merged].into_iter()).unwrap_or_default()
                            }
                            None => merged,
                        });
                    }
                    silence_sent = false;
                }
            }
        }

        // One datagram per look, at the rate asked; silence once when the ring goes quiet.
        let due = last_sent_at.is_none_or(|at| now.duration_since(at) >= interval);
        let mut to_send: Option<Vec<u8>> = None;
        if let (Some(frame), true) = (pending.as_ref(), due) {
            let info = ring.as_ref().map(|r| r.info()).unwrap_or_default();
            to_send = Some(wire::encode(frame, info.rate, info.channels, 0));
        } else if quiet && !silence_sent && ring.is_some() {
            let info = ring.as_ref().map(|r| r.info()).unwrap_or_default();
            let silence = Frame {
                seq: last_seq.wrapping_add(1),
                time_ns: now_ns(),
                spectrum: [vec![0.0; info.bins as usize], vec![0.0; info.bins as usize]],
                ..Default::default()
            };
            to_send = Some(wire::encode(&silence, info.rate, info.channels, 0));
            silence_sent = true;
        }
        if let Some(bytes) = to_send {
            pending = None;
            last_sent_at = Some(now);
            for (addr, s) in subscribers.iter_mut() {
                if socket.send_to(&bytes, addr).is_ok() {
                    s.sent += 1;
                }
            }
            sent += 1;
        }

        if now.duration_since(status_at) >= STATUS_EVERY {
            status_at = now;
            write_status(
                &args.status,
                &subscribers,
                ring.as_ref(),
                sent,
                args.port,
                args.rate,
            );
            // A ring whose file is gone and stays gone is let go of, so the status says so.
            if ring.as_ref().is_some_and(|r| !r.path().exists())
                && now_ns().saturating_sub(ring.as_ref().map(|r| r.info().written_ns).unwrap_or(0))
                    > LIVE_NS
            {
                ring = None;
                last_seq = 0;
            }
        }
        std::thread::sleep(REST);
    }
}
