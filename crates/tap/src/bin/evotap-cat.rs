//! Stream a live audio ring to stdout, for an encoder, a recorder or a
//! second output:
//!
//! `evotap-cat [--tag TAG] [--wait S] [--follow] [--gap silence|skip] [--s32] [--shape]`
//!
//! The ring is the newest live one under `/dev/shm`, of `TAG` when one is
//! given (the tap's `ring` key). One segment is one ring: the stream of
//! one shape from the moment evotap-cat opens it to the moment the ring goes,
//! when the player closes the stream or changes its shape. A line on
//! stderr says the shape as the segment begins (`rate=… channels=…
//! format=… frame=… one_bit=… ffmpeg=…`), the bytes go to stdout as they
//! are, zeros stand in for bytes lost when the reader fell behind (`--gap
//! skip` leaves them out), and evotap-cat exits 0 when the segment ends, so a
//! supervisor starts it again with its encoder for the next shape;
//! `--follow` goes on to the next segment instead. `--s32` turns every
//! linear format into S32_LE, the valid bits left-justified, for an
//! encoder that has no name for a 24-bit sample in a four-byte word.
//! `--shape` prints the line and exits. `--wait S` is how long to wait for
//! a ring, 5 seconds unless given, 0 for ever; none in time exits 2.

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use tap::audio::Reader;
use tap::sample::{alsa_name, ffmpeg_name, widen_to_s32, Layout};

/// Bytes taken from the ring in one go.
const TAKE: usize = 1 << 18;
/// How long to sleep when the ring has nothing new.
const NAP: Duration = Duration::from_millis(4);
/// How often to look for a ring that is not there yet.
const LOOK: Duration = Duration::from_millis(50);

struct Args {
    tag: Option<String>,
    wait: f64,
    follow: bool,
    silence: bool,
    s32: bool,
    shape_only: bool,
}

fn usage() -> ! {
    eprintln!(
        "evotap-cat [--tag TAG] [--wait S] [--follow] [--gap silence|skip] [--s32] [--shape] [--version]"
    );
    std::process::exit(64);
}

fn args() -> Args {
    let mut out = Args {
        tag: None,
        wait: 5.0,
        follow: false,
        silence: true,
        s32: false,
        shape_only: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--tag" => out.tag = it.next().filter(|t| !t.is_empty()),
            "--wait" => {
                out.wait = it
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| usage())
            }
            "--follow" => out.follow = true,
            "--gap" => match it.next().as_deref() {
                Some("silence") => out.silence = true,
                Some("skip") => out.silence = false,
                _ => usage(),
            },
            "--s32" => out.s32 = true,
            "--shape" => out.shape_only = true,
            "--version" => {
                println!("evotap {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-h" | "--help" => usage(),
            _ => usage(),
        }
    }
    out
}

/// Wait for a live ring, up to `wait` seconds (0: for ever).
fn find(dir: &Path, tag: Option<&str>, wait: f64) -> Option<Reader> {
    let started = Instant::now();
    loop {
        if let Some(r) = Reader::open_live(dir, tag) {
            return Some(r);
        }
        if wait > 0.0 && started.elapsed().as_secs_f64() >= wait {
            return None;
        }
        std::thread::sleep(LOOK);
    }
}

/// Zeros for `bytes` bytes of the stream, in pieces.
fn silence(out: &mut impl Write, bytes: u64) -> std::io::Result<()> {
    let zeros = [0u8; 1 << 16];
    let mut left = bytes;
    while left > 0 {
        let n = (left as usize).min(zeros.len());
        out.write_all(&zeros[..n])?;
        left -= n as u64;
    }
    Ok(())
}

fn main() {
    let a = args();
    let dir = Path::new(tap::ring::DIR);
    let mut stdout = std::io::stdout().lock();
    let mut buf: Vec<u8> = Vec::with_capacity(TAKE);
    let mut conv: Vec<u8> = Vec::new();
    loop {
        let Some(mut reader) = find(dir, a.tag.as_deref(), a.wait) else {
            eprintln!(
                "evotap-cat: no live audio ring{} under {}",
                a.tag
                    .as_deref()
                    .map(|t| format!(" of tag {t}"))
                    .unwrap_or_default(),
                dir.display()
            );
            std::process::exit(2);
        };
        let shape = reader.shape();
        let layout = Layout::from_alsa(shape.format);
        // Linear audio to S32_LE when asked; one-bit audio and formats the
        // tap does not read pass as they are.
        let s32 = a.s32 && layout.is_some_and(|l| !l.is_dsd());
        let out_format = if s32 { 10 } else { shape.format };
        let out_frame = if s32 {
            4 * shape.channels as usize
        } else {
            shape.frame_bytes as usize
        };
        eprintln!(
            "evotap-cat: segment ring={} pid={} rate={} channels={} format={} frame={} one_bit={} ffmpeg={}",
            reader.path().display(),
            reader.info().pid,
            shape.rate,
            shape.channels,
            alsa_name(out_format),
            out_frame,
            u8::from(shape.one_bit),
            ffmpeg_name(out_format).unwrap_or("none")
        );
        if a.shape_only {
            std::process::exit(0);
        }
        let in_frame = shape.frame_bytes.max(1) as u64;
        let mut written: u64 = 0;
        let mut lost_total: u64 = 0;
        loop {
            buf.clear();
            let took = reader.take(&mut buf, TAKE);
            if took.lost > 0 {
                lost_total += took.lost;
                if a.silence {
                    let frames = took.lost / in_frame;
                    if silence(&mut stdout, frames * out_frame as u64).is_err() {
                        std::process::exit(0);
                    }
                }
            }
            if took.bytes > 0 {
                let bytes: &[u8] = if s32 {
                    let layout = layout.unwrap_or(Layout::Float { little: true });
                    conv.clear();
                    widen_to_s32(&layout, &buf, &mut conv);
                    &conv
                } else {
                    &buf
                };
                if stdout.write_all(bytes).is_err() {
                    // The consumer went: nothing more to say.
                    std::process::exit(0);
                }
                written += bytes.len() as u64;
                continue;
            }
            if !reader.is_live() {
                break;
            }
            std::thread::sleep(NAP);
        }
        let _ = stdout.flush();
        eprintln!(
            "evotap-cat: segment ended after {written} bytes out, {lost_total} bytes of the stream lost, {} dropped by the tap",
            reader.info().dropped
        );
        if !a.follow {
            std::process::exit(0);
        }
    }
}
