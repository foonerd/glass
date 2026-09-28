//! The frame as it travels to a remote display: one datagram per hop,
//! small enough for one Ethernet frame. The peaks and RMS, which the
//! needles follow, travel exact; the bank travels as one byte a band in
//! quarter-decibel steps, which is finer than a pixel on any bar under 240
//! pixels per 60 dB, for each channel the bank has and then the peak hold
//! the same way. A remote turns a packet back into a [`Frame`] that is
//! what the player's own display reads.
//!
//! Layout, little-endian: magic `GLSF`, protocol, flags, the band count,
//! the sequence, the player's monotonic time, the rate, the channel count,
//! the bank's channel count, the frames in the hop, two peaks, two RMS,
//! the scale, the window as a power of two, then the bands per bank
//! channel, then the hold per bank channel.

use bank::Scale;

use crate::ring::{Frame, MAX_CHANNELS};

pub const MAGIC: [u8; 4] = *b"GLSF";
/// Protocol 2 carries the bank per channel with its hold; 1 carried the
/// raw spectrum of the channels' average.
pub const PROTOCOL: u8 = 2;
/// The bytes before the bands.
pub const HEADER: usize = 46;
/// More bands than a bank has is not a frame of ours.
pub const MAX_BINS: usize = bank::MAX_BINS;
/// One-bit audio: the bands are density levels rather than a spectrum.
pub const FLAG_ONE_BIT: u8 = 1;
/// The onsets ride in the flags' high nibble.
const FLAG_ONSETS_SHIFT: u8 = 4;
/// One step of a band's byte, in decibels.
pub const STEP_DB: f32 = 0.25;
/// The quietest a band can say before it says nothing.
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
    /// How many channels the bank has, one or two.
    pub layout: u8,
    pub scale: Scale,
    pub window: u32,
    /// The bands, channel-major, one byte each: 255 is full scale, each
    /// step a quarter decibel down, 0 is quieter than the floor.
    pub bins: Vec<u8>,
    /// The peak hold in the same layout.
    pub hold: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    /// Fewer bytes than a header.
    Short,
    /// Not our magic.
    Magic,
    /// A protocol this build does not read.
    Protocol(u8),
    /// The band count does not fit the datagram.
    Bins,
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::Short => write!(f, "datagram shorter than a header"),
            WireError::Magic => write!(f, "not a glass frame"),
            WireError::Protocol(p) => write!(f, "protocol {p} is not read by this build"),
            WireError::Bins => write!(f, "the band count does not fit the datagram"),
        }
    }
}

impl std::error::Error for WireError {}

/// An amplitude (1.0 full scale) as a band byte.
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

/// A band byte as an amplitude.
pub fn dequantize(byte: u8) -> f32 {
    if byte == 0 {
        return 0.0;
    }
    let db = (byte as f32 - 255.0) * STEP_DB;
    10f32.powf(db / 20.0)
}

/// How many channels a frame's bank needs on the wire: one when the two
/// read alike, as a one-channel demand or a mono source leaves them.
fn layout_of(frame: &Frame) -> u8 {
    if frame.spectrum[1].is_empty() || frame.spectrum[0] == frame.spectrum[1] {
        1
    } else {
        2
    }
}

/// A frame as a datagram.
pub fn encode(frame: &Frame, rate: u32, channels: u32, flags: u8) -> Vec<u8> {
    let layout = layout_of(frame);
    let bins = frame.spectrum[0].len().min(MAX_BINS);
    let flags = (flags & 0x0f) | (frame.onsets & 0x0f) << FLAG_ONSETS_SHIFT;
    let mut out = Vec::with_capacity(HEADER + bins * layout as usize * 2);
    out.extend_from_slice(&MAGIC);
    out.push(PROTOCOL);
    out.push(flags);
    out.extend_from_slice(&(bins as u16).to_le_bytes());
    out.extend_from_slice(&(frame.seq as u32).to_le_bytes());
    out.extend_from_slice(&frame.time_ns.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.push(channels.min(MAX_CHANNELS as u32) as u8);
    out.push(layout);
    out.extend_from_slice(&(frame.frames.min(u16::MAX as u64) as u16).to_le_bytes());
    for ch in 0..MAX_CHANNELS {
        out.extend_from_slice(&frame.peak[ch].to_le_bytes());
    }
    for ch in 0..MAX_CHANNELS {
        out.extend_from_slice(&frame.rms[ch].to_le_bytes());
    }
    out.push(frame.scale.byte());
    out.push(frame.window.max(1).ilog2() as u8);
    for ch in 0..layout as usize {
        out.extend(frame.spectrum[ch].iter().take(bins).map(|a| quantize(*a)));
    }
    for ch in 0..layout as usize {
        let hold = &frame.hold[ch];
        out.extend((0..bins).map(|k| quantize(hold.get(k).copied().unwrap_or(0.0))));
    }
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
    let layout = bytes[25].clamp(1, 2);
    let body = count * layout as usize;
    if count > MAX_BINS || bytes.len() < HEADER + body * 2 {
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
        layout,
        scale: Scale::from_byte(bytes[44]).unwrap_or_default(),
        window: 1u32 << bytes[45].min(20),
        bins: bytes[HEADER..HEADER + body].to_vec(),
        hold: bytes[HEADER + body..HEADER + body * 2].to_vec(),
    })
}

impl Packet {
    /// The bands per channel, as the display reads them: a one-channel
    /// bank stands for both.
    pub fn frame(&self) -> Frame {
        let bins = self.bins.len() / self.layout.max(1) as usize;
        let channel = |bytes: &[u8], ch: usize| -> Vec<f32> {
            let ch = ch.min(self.layout as usize - 1);
            bytes[ch * bins..(ch + 1) * bins]
                .iter()
                .map(|b| dequantize(*b))
                .collect()
        };
        Frame {
            seq: self.seq as u64,
            time_ns: self.time_ns,
            frames: self.frames as u64,
            peak: self.peak,
            rms: self.rms,
            spectrum: [channel(&self.bins, 0), channel(&self.bins, 1)],
            hold: [channel(&self.hold, 0), channel(&self.hold, 1)],
            onsets: self.flags >> FLAG_ONSETS_SHIFT,
            scale: self.scale,
            window: self.window,
            raw: false,
            one_bit: self.flags & FLAG_ONE_BIT != 0,
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
            spectrum: [left.clone(), right.clone()],
            hold: [left.iter().map(|v| (v + 0.1).min(1.0)).collect(), right],
            onsets: 0b0101,
            scale: Scale::Mel,
            window: 8192,
            raw: false,
            one_bit: false,
        }
    }

    fn within_a_step(got: &[f32], want: &[f32]) {
        assert_eq!(got.len(), want.len());
        for (got, want) in got.iter().zip(want.iter()) {
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
    }

    #[test]
    fn a_stereo_bank_travels_and_comes_back_within_a_quarter_decibel() {
        let frame = a_frame(256);
        let bytes = encode(&frame, 44100, 2, 0);
        assert_eq!(
            bytes.len(),
            HEADER + 256 * 2 * 2,
            "one datagram inside an Ethernet frame"
        );
        let packet = decode(&bytes).unwrap();
        assert_eq!(packet.seq, 7, "the sequence wraps to 32 bits");
        assert_eq!(packet.time_ns, 123_456_789_012);
        assert_eq!(packet.rate, 44100);
        assert_eq!(packet.channels, 2);
        assert_eq!(packet.layout, 2);
        assert_eq!(packet.frames, 512);
        assert_eq!(packet.peak, frame.peak, "peaks travel exact");
        assert_eq!(packet.rms, frame.rms, "RMS travels exact");
        assert_eq!((packet.scale, packet.window), (Scale::Mel, 8192));
        let back = packet.frame();
        within_a_step(&back.spectrum[0], &frame.spectrum[0]);
        within_a_step(&back.spectrum[1], &frame.spectrum[1]);
        within_a_step(&back.hold[0], &frame.hold[0]);
        within_a_step(&back.hold[1], &frame.hold[1]);
        assert_eq!(back.onsets, 0b0101, "the onsets ride in the flags");
        assert_ne!(back.spectrum[0], back.spectrum[1]);
    }

    #[test]
    fn a_bank_alike_on_both_channels_travels_once() {
        let mut frame = a_frame(64);
        frame.spectrum[1] = frame.spectrum[0].clone();
        frame.hold[1] = frame.hold[0].clone();
        let bytes = encode(&frame, 48000, 2, 0);
        assert_eq!(bytes.len(), HEADER + 64 * 2);
        let packet = decode(&bytes).unwrap();
        assert_eq!(packet.layout, 1);
        let back = packet.frame();
        assert_eq!(back.spectrum[0], back.spectrum[1]);
        assert_eq!(back.hold[0], back.hold[1]);
        within_a_step(&back.spectrum[1], &frame.spectrum[0]);
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
        let mut older = bytes.clone();
        older[4] = 1;
        assert_eq!(decode(&older), Err(WireError::Protocol(1)));
        let mut lying = bytes.clone();
        lying[6] = 0xff;
        lying[7] = 0x7f;
        assert_eq!(decode(&lying), Err(WireError::Bins));
        assert_eq!(decode(&bytes[..bytes.len() - 1]), Err(WireError::Bins));
        assert_eq!(decode(&bytes).unwrap().flags & 0x0f, FLAG_ONE_BIT);
        assert!(
            decode(&bytes).unwrap().frame().one_bit,
            "the one-bit flag comes back as the frame's"
        );
    }

    #[test]
    fn a_frame_with_no_bands_and_bad_floats_still_decodes() {
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
        assert!(packet.frame().spectrum[0].is_empty());
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
