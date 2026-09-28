//! The frames as a host hands them over, one datagram at a time, merged
//! between two looks: the wire's decoding, the ordering by the player's
//! stamps, the silence, the gain and the spectrum decay, in one place for
//! the remote display's socket and for a page that receives the datagrams
//! through the manager.

use tap::ring::{merge_hops, Frame};
use tap::wire;

use crate::{Hops, Taken};

/// No frame for this long is silence, as a ring gone quiet is.
const QUIET_US: u64 = 500_000;
/// Stamps further apart than this are not in order, or a stream started
/// again; the packet's own count stands in then.
const MAX_GAP_NS: u64 = 2_000_000_000;

/// Frames pushed as datagrams, taken merged.
pub struct WireHops {
    last_seq: Option<u32>,
    /// When the last frame with sound in it came, on `lead::clock_us`.
    last_packet_us: Option<u64>,
    /// The player's stamp on the last packet: the next one's elapsed
    /// frames are the time between them at the stream's rate.
    last_time_ns: Option<u64>,
    rate: u32,
    frames: Vec<Frame>,
    elapsed: u64,
    received: u64,
    refused: u64,
    /// The factor levels are scaled by, from a gain in decibels.
    gain: f32,
    /// The share of its height a spectrum bar may fall per frame at most;
    /// 0 leaves the bars as the player sends them.
    decay: f32,
    /// The bars as last shown, for the decay.
    last_spectrum: [Vec<f32>; 2],
}

impl Default for WireHops {
    fn default() -> Self {
        Self {
            last_seq: None,
            last_packet_us: None,
            last_time_ns: None,
            rate: 0,
            frames: Vec::new(),
            elapsed: 0,
            received: 0,
            refused: 0,
            gain: 1.0,
            decay: 0.0,
            last_spectrum: [Vec::new(), Vec::new()],
        }
    }
}

impl WireHops {
    pub fn new() -> Self {
        Self::default()
    }

    /// Scale the levels by a gain in decibels, within plus or minus twelve.
    pub fn with_gain_db(mut self, db: f32) -> Self {
        let db = if db.is_finite() {
            db.clamp(-12.0, 12.0)
        } else {
            0.0
        };
        self.gain = 10f32.powf(db / 20.0);
        self
    }

    /// Let a spectrum bar fall by at most this share of its height per
    /// frame, 0.5 to 0.99 (Peppy Remote's decay rate); 0 turns it off.
    pub fn with_spectrum_decay(mut self, per_frame: f32) -> Self {
        self.decay = if per_frame.is_finite() && per_frame >= 0.5 {
            per_frame.min(0.99)
        } else {
            0.0
        };
        self
    }

    /// A datagram as it came. One that is not a frame is refused; one no
    /// newer than the last is dropped; a frame of silence, the daemon's
    /// word that the ring went quiet, lets the levels fall as at a stop.
    pub fn push(&mut self, bytes: &[u8]) {
        self.received += 1;
        let packet = match wire::decode(bytes) {
            Ok(packet) => packet,
            Err(_) => {
                self.refused += 1;
                return;
            }
        };
        // Newer by the player's stamp, the count breaking a tie: a daemon
        // of an earlier release numbered its datagrams by the ring, whose
        // count starts again with every stream, while the stamps go on.
        let by_seq = self
            .last_seq
            .is_none_or(|last| wire::Packet::is_after(packet.seq, last));
        let new = match self.last_time_ns {
            None => by_seq,
            Some(last) => packet.time_ns > last || (packet.time_ns == last && by_seq),
        };
        if !new {
            return;
        }
        self.last_seq = Some(packet.seq);
        // The frames elapsed since the last packet: the time between the
        // player's stamps at the stream's rate, which spans dropped
        // datagrams too; the packet's own count for the first one, or
        // when the stamps are not in order.
        let by_stamp = self
            .last_time_ns
            .map(|last| packet.time_ns.saturating_sub(last))
            .filter(|ns| (1..=MAX_GAP_NS).contains(ns))
            .map(|ns| ns.saturating_mul(u64::from(packet.rate)) / 1_000_000_000);
        let in_hop = by_stamp.unwrap_or(u64::from(packet.frames)).max(1);
        self.last_time_ns = Some(packet.time_ns);
        let silent = packet.peak.iter().all(|p| *p <= 0.0)
            && packet.rms.iter().all(|r| *r <= 0.0)
            && packet.bins.iter().all(|b| *b == 0);
        if silent {
            self.last_packet_us = None;
            self.last_spectrum = [Vec::new(), Vec::new()];
            return;
        }
        self.last_packet_us = Some(lead::clock_us());
        self.rate = packet.rate;
        self.elapsed += in_hop;
        let mut frame = packet.frame();
        if self.gain != 1.0 {
            scale_frame(&mut frame, self.gain);
        }
        if self.decay > 0.0 {
            decay_spectrum(&mut frame, &mut self.last_spectrum, self.decay);
        }
        self.frames.push(frame);
    }
}

impl Hops for WireHops {
    fn take(&mut self) -> Taken {
        let quiet = self
            .last_packet_us
            .is_none_or(|at| lead::clock_us().saturating_sub(at) > QUIET_US);
        let elapsed = std::mem::take(&mut self.elapsed);
        let hop = merge_hops(self.frames.drain(..))
            .map(|frame| (frame, self.rate.max(1), elapsed.max(1)));
        Taken { hop, quiet }
    }

    fn stats(&self) -> (u64, u64) {
        (self.received, self.refused)
    }
}

/// Every level of a frame multiplied by `gain`, kept within full scale.
fn scale_frame(frame: &mut Frame, gain: f32) {
    for level in frame.peak.iter_mut().chain(frame.rms.iter_mut()) {
        *level = (*level * gain).min(1.0);
    }
    for bin in frame
        .spectrum
        .iter_mut()
        .chain(frame.hold.iter_mut())
        .flat_map(|channel| channel.iter_mut())
    {
        *bin = (*bin * gain).min(1.0);
    }
}

/// Each spectrum bar no lower than the last one's height times `decay`:
/// bars rise at once and fall by at most that share per frame. `last`
/// keeps what was shown; a channel with a different number of bins
/// starts afresh.
fn decay_spectrum(frame: &mut Frame, last: &mut [Vec<f32>; 2], decay: f32) {
    for (channel, kept) in frame.spectrum.iter_mut().zip(last.iter_mut()) {
        if kept.len() != channel.len() {
            kept.clear();
            kept.resize(channel.len(), 0.0);
        }
        for (bin, held) in channel.iter_mut().zip(kept.iter_mut()) {
            *bin = bin.max(*held * decay);
            *held = *bin;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn datagram(seq: u64, time_ns: u64, peak: f32, bin: f32) -> Vec<u8> {
        let frame = Frame {
            seq,
            time_ns,
            frames: 1024,
            peak: [peak, peak],
            rms: [peak / 2.0, peak / 2.0],
            spectrum: [vec![bin; 4], vec![bin; 4]],
            ..Frame::default()
        };
        wire::encode(&frame, 48_000, 2, 0)
    }

    #[test]
    fn datagrams_merge_between_looks_and_older_ones_are_dropped() {
        let mut hops = WireHops::new();
        hops.push(&datagram(1, 1_000_000, 0.5, 0.2));
        hops.push(&datagram(2, 2_000_000, 0.8, 0.4));
        hops.push(&datagram(1, 1_000_000, 0.9, 0.9));
        let taken = hops.take();
        let (frame, rate, elapsed) = taken.hop.expect("a hop");
        assert_eq!(rate, 48_000);
        assert!((frame.peak[0] - 0.8).abs() < 0.01);
        assert!(!taken.quiet);
        // 1024 frames from the first packet's own count, 48 from the stamps.
        assert_eq!(elapsed, 1024 + 48);
        assert_eq!(hops.stats(), (3, 0));
        assert!(hops.take().hop.is_none());
    }

    #[test]
    fn what_is_not_a_frame_is_refused_and_silence_is_quiet() {
        let mut hops = WireHops::new();
        hops.push(b"not a frame");
        assert_eq!(hops.stats(), (1, 1));
        hops.push(&datagram(1, 1_000_000, 0.5, 0.2));
        hops.push(&datagram(2, 2_000_000, 0.0, 0.0));
        let taken = hops.take();
        assert!(taken.hop.is_some());
        assert!(taken.quiet, "silence ends the sound");
    }

    #[test]
    fn gain_scales_and_decay_holds_the_bars_up() {
        let mut hops = WireHops::new().with_gain_db(6.0).with_spectrum_decay(0.9);
        hops.push(&datagram(1, 1_000_000, 0.25, 0.5));
        let (first, _, _) = hops.take().hop.expect("a hop");
        assert!((first.peak[0] - 0.25 * 10f32.powf(0.3)).abs() < 0.01);
        hops.push(&datagram(2, 2_000_000, 0.25, 0.0));
        let (second, _, _) = hops.take().hop.expect("a hop");
        let held = first.spectrum[0][0] * 0.9;
        assert!(
            (second.spectrum[0][0] - held).abs() < 0.001,
            "{}",
            second.spectrum[0][0]
        );
    }
}
