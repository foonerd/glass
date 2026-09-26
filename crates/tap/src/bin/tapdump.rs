//! Print what the live ring says, a line a hop, for a look at the tap
//! without the display: `tapdump [seconds]`.

use std::path::Path;
use std::time::{Duration, Instant};

fn main() {
    let seconds: f64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(5.0);
    let dir = Path::new(tap::ring::DIR);
    let Some(reader) = tap::Reader::open_live(dir) else {
        eprintln!("tapdump: no live ring under {}", dir.display());
        std::process::exit(1);
    };
    let info = reader.info();
    println!(
        "ring {} pid {} rate {} channels {} fft {} bins {} hop {} slots {}",
        reader.path().display(),
        info.pid,
        info.rate,
        info.channels,
        info.fft_size,
        info.bins,
        info.hop,
        info.slots
    );
    let started = Instant::now();
    let mut last_seq = 0;
    while started.elapsed() < Duration::from_secs_f64(seconds) {
        if let Some(frame) = reader.latest() {
            if frame.seq != last_seq {
                last_seq = frame.seq;
                let bins = frame.spectrum[0].len();
                let loud = |from: usize, to: usize| -> f32 {
                    frame.spectrum[0][from.min(bins)..to.min(bins)].iter().cloned().fold(0.0, f32::max)
                };
                println!(
                    "seq {:6} frames {:9} peak {:.3} {:.3} rms {:.3} {:.3} low {:.3} mid {:.3} high {:.3}",
                    frame.seq,
                    frame.frames,
                    frame.peak[0],
                    frame.peak[1],
                    frame.rms[0],
                    frame.rms[1],
                    loud(1, bins / 32),
                    loud(bins / 32, bins / 4),
                    loud(bins / 4, bins)
                );
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
