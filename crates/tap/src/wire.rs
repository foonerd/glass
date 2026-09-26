//! The frame as it travels to a remote display: one datagram per hop,
//! small enough for one Ethernet frame. The peaks and RMS, which the
//! needles follow, travel exact; the spectrum travels as the two channels'
//! average, one byte a bin in quarter-decibel steps, which is finer than a
//! pixel on any bar under 240 pixels per 60 dB. A remote turns a packet
//! back into a [`Frame`] whose two spectra are that average, which is
//! what the display mixes them to anyway.
//!
//! Layout, little-endian: magic `GLSF`, protocol, flags, the bin count,
//! the sequence, the player's monotonic time, the rate, the channel
//! count, a reserved byte, the frames in the hop, two peaks, two RMS, then
//! the bins.

use crate::ring::{Frame, MAX_CHANNELS};

pub const MAGIC: [u8; 4] = *b"GLSF";
pub const PROTOCOL: u8 = 1;
/// The bytes before the bins.
pub const HEADER: usize = 44;
/// More bins than a 8192-point FFT gives is not a frame of ours.
pub const MAX_BINS: usize = 4096;
/// One-bit audio: the bins are density levels rather than a spectrum.
pub const FLAG_ONE_BIT: u8 = 1;
/// One step of a bin's byte, in decibels.
pub const STEP_DB: f32 = 0.25;
/// The quietest a bin can say before it says nothing.
pub const FLOOR_DB: f32 = -(255.0 * STEP_DB);

/// What a packet carries.
#[derive(Clone, Debug, PartialEq)]
pub struct Packet {
    pub seq: u32,
    pub time_ns: u64,
    pub rate: u32,
    pub channels: u8,
    pub flags: u8,
    pub frames: u16,
    pub peak: [f32; MAX_CHANNELS],
    pub rms: [f32; MAX_CHANNELS],
    /// The channels' average spectrum, one byte a bin: 255 is full scale,
    /// each step a quarter decibel down, 0 is quieter than the floor.
    pub bins: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    /// Fewer bytes than a header.
    Short,
    /// Not our magic.
    Magic,
    /// A protocol this build does not read.
    Protocol(u8),
    /// The bin count does not fit the datagram.
    Bins,
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::Short => write!(f, "datagram shorter than a header"),
            WireError::Magic => write!(f, "not a glass frame"),
            WireError::Protocol(p) => write!(f, "protocol {p} is not read by this build"),
            WireError::Bins => write!(f, "the bin count does not fit the datagram"),
        }
    }
}

impl std::error::Error for WireError {}

/// An amplitude (1.0 full scale) as a bin byte.
pub fn quantize(amplitude: f32) -> u8 {
    if amplitude.is_nan() || amplitude <= 0.0 {
        return 0;
    }
    let db = 20.0 * amplitude.log10();
    if db <= FLOOR_DB {
        return 0;
    }
    let steps = 255.0 + db / STEP_DB;
    steps.round().clamp(0.0, 255.0) as u8
}

/// A bin byte as an amplitude.
pub fn dequantize(byte: u8) -> f32 {
    if byte == 0 {
        return 0.0;
    }
    let db = (byte as f32 - 255.0) * STEP_DB;
    10f32.powf(db / 20.0)
}

/// The channels' average of a frame's spectra, as the display mixes them.
pub fn mixed(frame: &Frame) -> Vec<f32> {
    let left = &frame.spectrum[0];
    let right = &frame.spectrum[1];
    if right.len() == left.len() && !right.is_empty() {
        left.iter()
            .zip(right.iter())
            .map(|(l, r)| (l + r) / 2.0)
            .collect()
    } else if left.is_empty() {
        right.clone()
    } else {
        left.clone()
    }
}

/// A frame as a datagram.
pub fn encode(frame: &Frame, rate: u32, channels: u32, flags: u8) -> Vec<u8> {
    let bins: Vec<u8> = mixed(frame)
        .iter()
        .take(MAX_BINS)
        .map(|a| quantize(*a))
        .collect();
    let mut out = Vec::with_capacity(HEADER + bins.len());
    out.extend_from_slice(&MAGIC);
    out.push(PROTOCOL);
    out.push(flags);
    out.extend_from_slice(&(bins.len() as u16).to_le_bytes());
    out.extend_from_slice(&(frame.seq as u32).to_le_bytes());
    out.extend_from_slice(&frame.time_ns.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.push(channels.min(MAX_CHANNELS as u32) as u8);
    out.push(0);
    out.extend_from_slice(&(frame.frames.min(u16::MAX as u64) as u16).to_le_bytes());
    for ch in 0..MAX_CHANNELS {
        out.extend_from_slice(&frame.peak[ch].to_le_bytes());
    }
    for ch in 0..MAX_CHANNELS {
        out.extend_from_slice(&frame.rms[ch].to_le_bytes());
    }
    out.extend_from_slice(&bins);
    out
}

/// A datagram as a packet, or why it is not one.
pub fn decode(bytes: &[u8]) -> Result<Packet, WireError> {
    if bytes.len() < HEADER {
        return Err(WireError::Short);
    }
    if bytes[0..4] != MAGIC {
        return Err(WireError::Magic);
    }
    if bytes[4] != PROTOCOL {
        return Err(WireError::Protocol(bytes[4]));
    }
    let flags = bytes[5];
    let count = u16::from_le_bytes([bytes[6], bytes[7]]) as usize;
    if count > MAX_BINS || bytes.len() < HEADER + count {
        return Err(WireError::Bins);
    }
    let u32_at =
        |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let f32_at =
        |at: usize| f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let mut time = [0u8; 8];
    time.copy_from_slice(&bytes[12..20]);
    let clean = |v: f32| {
        if v.is_finite() {
            v.clamp(0.0, 4.0)
        } else {
            0.0
        }
    };
    Ok(Packet {
        seq: u32_at(8),
        time_ns: u64::from_le_bytes(time),
        rate: u32_at(20),
        channels: bytes[24],
        flags,
        frames: u16::from_le_bytes([bytes[26], bytes[27]]),
        peak: [clean(f32_at(28)), clean(f32_at(32))],
        rms: [clean(f32_at(36)), clean(f32_at(40))],
        bins: bytes[HEADER..HEADER + count].to_vec(),
    })
}

impl Packet {
    /// The frame a remote's display reads: the average spectrum stands in
    /// for both channels, so mixing them again changes nothing.
    pub fn frame(&self) -> Frame {
        let spectrum: Vec<f32> = self.bins.iter().map(|b| dequantize(*b)).collect();
        Frame {
            seq: self.seq as u64,
            time_ns: self.time_ns,
            frames: self.frames as u64,
            peak: self.peak,
            rms: self.rms,
            spectrum: [spectrum.clone(), spectrum],
        }
    }

    /// Whether `seq` comes after `last`, with the wrap of the counter in mind.
    pub fn is_after(seq: u32, last: u32) -> bool {
        seq != last && seq.wrapping_sub(last) < (1 << 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_frame(bins: usize) -> Frame {
        let left: Vec<f32> = (0..bins)
            .map(|i| (i as f32 / bins as f32).powi(2))
            .collect();
        let right: Vec<f32> = (0..bins).map(|i| 1.0 - i as f32 / bins as f32).collect();
        Frame {
            seq: 0x1_0000_0007,
            time_ns: 123_456_789_012,
            frames: 512,
            peak: [0.71, 0.33],
            rms: [0.25, 0.125],
            spectrum: [left, right],
        }
    }

    #[test]
    fn a_frame_travels_and_comes_back_within_a_quarter_decibel() {
        let frame = a_frame(1024);
        let bytes = encode(&frame, 44100, 2, 0);
        assert_eq!(
            bytes.len(),
            HEADER + 1024,
            "one datagram inside an Ethernet frame"
        );
        let packet = decode(&bytes).unwrap();
        assert_eq!(packet.seq, 7, "the sequence wraps to 32 bits");
        assert_eq!(packet.time_ns, 123_456_789_012);
        assert_eq!(packet.rate, 44100);
        assert_eq!(packet.channels, 2);
        assert_eq!(packet.frames, 512);
        assert_eq!(packet.peak, frame.peak, "peaks travel exact");
        assert_eq!(packet.rms, frame.rms, "RMS travels exact");
        let back = packet.frame();
        let expected = mixed(&frame);
        for (got, want) in back.spectrum[0].iter().zip(expected.iter()) {
            if *want > dequantize(1) {
                let db = 20.0 * (got / want).log10();
                assert!(
                    db.abs() <= STEP_DB / 2.0 + 1e-3,
                    "got {got} for {want}: {db} dB off"
                );
            } else {
                assert!(*got <= dequantize(1));
            }
        }
        assert_eq!(back.spectrum[0], back.spectrum[1]);
    }

    #[test]
    fn the_quantizer_covers_full_scale_and_the_floor() {
        assert_eq!(quantize(1.0), 255);
        assert_eq!(quantize(2.0), 255, "over full scale is clamped");
        assert_eq!(quantize(0.0), 0);
        assert_eq!(quantize(-1.0), 0);
        assert_eq!(quantize(f32::NAN), 0);
        assert_eq!(quantize(1e-9), 0, "below the floor says nothing");
        assert_eq!(dequantize(255), 1.0);
        assert_eq!(dequantize(0), 0.0);
        // Half scale is 6.02 dB down: 24 steps of a quarter.
        assert_eq!(quantize(0.5), 231);
        assert!((dequantize(231) - 0.5).abs() < 0.01);
    }

    #[test]
    fn what_is_not_a_frame_is_refused() {
        let frame = a_frame(8);
        let bytes = encode(&frame, 48000, 2, FLAG_ONE_BIT);
        assert_eq!(decode(&bytes[..HEADER - 1]), Err(WireError::Short));
        let mut wrong = bytes.clone();
        wrong[0] = b'X';
        assert_eq!(decode(&wrong), Err(WireError::Magic));
        let mut newer = bytes.clone();
        newer[4] = 9;
        assert_eq!(decode(&newer), Err(WireError::Protocol(9)));
        let mut lying = bytes.clone();
        lying[6] = 0xff;
        lying[7] = 0x7f;
        assert_eq!(decode(&lying), Err(WireError::Bins));
        assert_eq!(decode(&bytes[..bytes.len() - 1]), Err(WireError::Bins));
        assert_eq!(decode(&bytes).unwrap().flags, FLAG_ONE_BIT);
    }

    #[test]
    fn a_frame_with_no_bins_and_bad_floats_still_decodes() {
        let mut frame = a_frame(0);
        frame.peak = [f32::NAN, 9.0];
        let bytes = encode(&frame, 96000, 1, 0);
        let packet = decode(&bytes).unwrap();
        assert!(packet.bins.is_empty());
        assert_eq!(
            packet.peak,
            [0.0, 4.0],
            "not a number reads as silence, a wild value is clamped"
        );
        assert_eq!(packet.channels, 1);
    }

    #[test]
    fn sequence_order_survives_the_wrap() {
        assert!(Packet::is_after(1, 0));
        assert!(!Packet::is_after(0, 1));
        assert!(Packet::is_after(0, u32::MAX));
        assert!(!Packet::is_after(u32::MAX, 0));
        assert!(!Packet::is_after(5, 5));
    }
}
