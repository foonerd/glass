//! Sample formats as ALSA numbers them, and how one sample of each reads
//! on the 16-bit scale: the top sixteen bits of a linear sample, a float
//! scaled to them. One-bit audio has no amplitude per sample; its bytes go
//! to the density measure instead.

/// How the bytes of one sample are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// A linear sample `bytes` wide with `valid` significant bits at the
    /// bottom, two's complement or offset binary, little or big endian.
    Linear {
        bytes: u8,
        valid: u8,
        signed: bool,
        little: bool,
    },
    /// A 32-bit IEEE float between minus one and one.
    Float { little: bool },
    /// One-bit audio, `bytes` per sample word.
    Dsd { bytes: u8 },
}

impl Layout {
    /// From ALSA's `snd_pcm_format_t`. `None` for a format the tap does
    /// not read (compressed, 64-bit float, 18-bit words in four bytes);
    /// such a stream passes unmeasured.
    pub fn from_alsa(format: i32) -> Option<Layout> {
        let linear = |bytes, valid, signed, little| Layout::Linear {
            bytes,
            valid,
            signed,
            little,
        };
        Some(match format {
            0 => linear(1, 8, true, true),
            1 => linear(1, 8, false, true),
            2 => linear(2, 16, true, true),
            3 => linear(2, 16, true, false),
            4 => linear(2, 16, false, true),
            5 => linear(2, 16, false, false),
            6 => linear(4, 24, true, true),
            7 => linear(4, 24, true, false),
            8 => linear(4, 24, false, true),
            9 => linear(4, 24, false, false),
            10 => linear(4, 32, true, true),
            11 => linear(4, 32, true, false),
            12 => linear(4, 32, false, true),
            13 => linear(4, 32, false, false),
            14 => Layout::Float { little: true },
            15 => Layout::Float { little: false },
            25 => linear(4, 20, true, true),
            26 => linear(4, 20, true, false),
            27 => linear(4, 20, false, true),
            28 => linear(4, 20, false, false),
            32 => linear(3, 24, true, true),
            33 => linear(3, 24, true, false),
            34 => linear(3, 24, false, true),
            35 => linear(3, 24, false, false),
            36 => linear(3, 20, true, true),
            37 => linear(3, 20, true, false),
            38 => linear(3, 20, false, true),
            39 => linear(3, 20, false, false),
            40 => linear(3, 18, true, true),
            41 => linear(3, 18, true, false),
            42 => linear(3, 18, false, true),
            43 => linear(3, 18, false, false),
            48 => Layout::Dsd { bytes: 1 },
            49 | 51 => Layout::Dsd { bytes: 2 },
            50 | 52 => Layout::Dsd { bytes: 4 },
            _ => return None,
        })
    }

    /// Bytes one sample occupies.
    pub fn bytes(&self) -> usize {
        match *self {
            Layout::Linear { bytes, .. } | Layout::Dsd { bytes } => bytes as usize,
            Layout::Float { .. } => 4,
        }
    }

    /// Whether the samples are one-bit audio.
    pub fn is_dsd(&self) -> bool {
        matches!(self, Layout::Dsd { .. })
    }

    /// The sample at the start of `bytes` on the 32-bit scale, a linear
    /// sample's valid bits left-justified (so every width is carried
    /// without loss), a float scaled. DSD reads as zero here.
    pub fn to_i32(&self, bytes: &[u8]) -> i32 {
        match *self {
            Layout::Linear {
                bytes: n,
                valid,
                signed,
                little,
            } => {
                let n = n as usize;
                if bytes.len() < n {
                    return 0;
                }
                let mut v: u32 = 0;
                for i in 0..n {
                    let b = if little { bytes[i] } else { bytes[n - 1 - i] };
                    v |= (b as u32) << (8 * i);
                }
                let mut s = v.wrapping_shl(32 - valid as u32) as i32;
                if !signed {
                    s ^= i32::MIN;
                }
                s
            }
            Layout::Float { little } => {
                if bytes.len() < 4 {
                    return 0;
                }
                let raw = [bytes[0], bytes[1], bytes[2], bytes[3]];
                let f = if little {
                    f32::from_le_bytes(raw)
                } else {
                    f32::from_be_bytes(raw)
                };
                let f = if f.is_finite() {
                    f.clamp(-1.0, 1.0)
                } else {
                    0.0
                };
                (f as f64 * 2_147_483_647.0) as i32
            }
            Layout::Dsd { .. } => 0,
        }
    }

    /// The sample at the start of `bytes` on the 16-bit scale: the top
    /// sixteen of a linear sample's valid bits, a float scaled. DSD reads
    /// as zero here; its bytes are measured by density.
    pub fn to_i16(&self, bytes: &[u8]) -> i16 {
        match *self {
            Layout::Linear {
                bytes: n,
                valid,
                signed,
                little,
            } => {
                let n = n as usize;
                if bytes.len() < n {
                    return 0;
                }
                let mut v: u32 = 0;
                for i in 0..n {
                    let b = if little { bytes[i] } else { bytes[n - 1 - i] };
                    v |= (b as u32) << (8 * i);
                }
                // Left-justify the valid bits, turn offset binary into
                // two's complement, and keep the top sixteen.
                let mut s = v.wrapping_shl(32 - valid as u32) as i32;
                if !signed {
                    s ^= i32::MIN;
                }
                (s >> 16) as i16
            }
            Layout::Float { little } => {
                if bytes.len() < 4 {
                    return 0;
                }
                let raw = [bytes[0], bytes[1], bytes[2], bytes[3]];
                let f = if little {
                    f32::from_le_bytes(raw)
                } else {
                    f32::from_be_bytes(raw)
                };
                let f = if f.is_finite() {
                    f.clamp(-1.0, 1.0)
                } else {
                    0.0
                };
                (f * 32767.0) as i16
            }
            Layout::Dsd { .. } => 0,
        }
    }
}

/// ALSA's name for a format number, as `aplay -f` takes it; `unknown`
/// for a number libasound has no format for.
pub fn alsa_name(format: i32) -> &'static str {
    match format {
        0 => "S8",
        1 => "U8",
        2 => "S16_LE",
        3 => "S16_BE",
        4 => "U16_LE",
        5 => "U16_BE",
        6 => "S24_LE",
        7 => "S24_BE",
        8 => "U24_LE",
        9 => "U24_BE",
        10 => "S32_LE",
        11 => "S32_BE",
        12 => "U32_LE",
        13 => "U32_BE",
        14 => "FLOAT_LE",
        15 => "FLOAT_BE",
        16 => "FLOAT64_LE",
        17 => "FLOAT64_BE",
        18 => "IEC958_SUBFRAME_LE",
        19 => "IEC958_SUBFRAME_BE",
        20 => "MU_LAW",
        21 => "A_LAW",
        22 => "IMA_ADPCM",
        23 => "MPEG",
        24 => "GSM",
        25 => "S20_LE",
        26 => "S20_BE",
        27 => "U20_LE",
        28 => "U20_BE",
        31 => "SPECIAL",
        32 => "S24_3LE",
        33 => "S24_3BE",
        34 => "U24_3LE",
        35 => "U24_3BE",
        36 => "S20_3LE",
        37 => "S20_3BE",
        38 => "U20_3LE",
        39 => "U20_3BE",
        40 => "S18_3LE",
        41 => "S18_3BE",
        42 => "U18_3LE",
        43 => "U18_3BE",
        44 => "G723_24",
        45 => "G723_24_1B",
        46 => "G723_40",
        47 => "G723_40_1B",
        48 => "DSD_U8",
        49 => "DSD_U16_LE",
        50 => "DSD_U32_LE",
        51 => "DSD_U16_BE",
        52 => "DSD_U32_BE",
        _ => "unknown",
    }
}

/// ffmpeg's name for the raw bytes of a format (its `-f`), where it has
/// one: the whole-byte linear widths and the floats. A 24- or 20-bit
/// sample in a four-byte word has none; `evotap-cat --s32` carries those.
pub fn ffmpeg_name(format: i32) -> Option<&'static str> {
    Some(match format {
        0 => "s8",
        1 => "u8",
        2 => "s16le",
        3 => "s16be",
        4 => "u16le",
        5 => "u16be",
        10 => "s32le",
        11 => "s32be",
        12 => "u32le",
        13 => "u32be",
        14 => "f32le",
        15 => "f32be",
        16 => "f64le",
        17 => "f64be",
        32 => "s24le",
        33 => "s24be",
        34 => "u24le",
        35 => "u24be",
        _ => return None,
    })
}

/// Every sample of `bytes`, of `layout`, onto `out` as S32_LE: the valid
/// bits left-justified, so nothing is lost and an encoder need know one
/// width. A trailing part of a sample is left out.
pub fn widen_to_s32(layout: &Layout, bytes: &[u8], out: &mut Vec<u8>) {
    let n = layout.bytes().max(1);
    out.reserve(bytes.len() / n * 4);
    for s in bytes.chunks_exact(n) {
        out.extend_from_slice(&layout.to_i32(s).to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_of_every_width_are_widened_to_s32_left_justified() {
        let mut out = Vec::new();
        widen_to_s32(
            &Layout::from_alsa(2).unwrap(),
            &[0x34, 0x12, 0x00, 0x80, 0xAA],
            &mut out,
        );
        assert_eq!(
            out,
            [0x00, 0x00, 0x34, 0x12, 0x00, 0x00, 0x00, 0x80],
            "16-bit samples in the top half, a trailing byte left out"
        );
        out.clear();
        widen_to_s32(
            &Layout::from_alsa(32).unwrap(),
            &[0x56, 0x34, 0x12],
            &mut out,
        );
        assert_eq!(out, [0x00, 0x56, 0x34, 0x12]);
        out.clear();
        widen_to_s32(
            &Layout::from_alsa(6).unwrap(),
            &[0x56, 0x34, 0x12, 0xEE],
            &mut out,
        );
        assert_eq!(
            out,
            [0x00, 0x56, 0x34, 0x12],
            "24 in 32: the top byte ignored"
        );
        out.clear();
        widen_to_s32(&Layout::from_alsa(10).unwrap(), &[1, 2, 3, 4], &mut out);
        assert_eq!(out, [1, 2, 3, 4], "S32_LE is itself");
    }

    #[test]
    fn every_width_reads_its_valid_bits_left_justified_on_the_32_bit_scale() {
        assert_eq!(
            Layout::from_alsa(2).unwrap().to_i32(&[0x34, 0x12]),
            0x1234_0000
        );
        assert_eq!(
            Layout::from_alsa(6)
                .unwrap()
                .to_i32(&[0x56, 0x34, 0x12, 0xEE]),
            0x1234_5600,
            "24 bits in four bytes, left-justified"
        );
        assert_eq!(
            Layout::from_alsa(32).unwrap().to_i32(&[0x56, 0x34, 0x12]),
            0x1234_5600
        );
        assert_eq!(
            Layout::from_alsa(10)
                .unwrap()
                .to_i32(&[0x78, 0x56, 0x34, 0x12]),
            0x1234_5678
        );
        assert_eq!(
            Layout::from_alsa(1).unwrap().to_i32(&[0x80u8]),
            0,
            "unsigned midpoint is silence"
        );
        assert_eq!(
            Layout::from_alsa(14).unwrap().to_i32(&1.0f32.to_le_bytes()),
            i32::MAX
        );
        assert_eq!(
            Layout::from_alsa(14)
                .unwrap()
                .to_i32(&(-1.0f32).to_le_bytes()),
            -i32::MAX
        );
        assert_eq!(Layout::from_alsa(50).unwrap().to_i32(&[0xFF; 4]), 0);
        assert_eq!(
            Layout::from_alsa(2).unwrap().to_i32(&[0x34]),
            0,
            "short input"
        );
        assert_eq!(alsa_name(10), "S32_LE");
        assert_eq!(alsa_name(50), "DSD_U32_LE");
        assert_eq!(alsa_name(99), "unknown");
        assert_eq!(ffmpeg_name(10), Some("s32le"));
        assert_eq!(ffmpeg_name(32), Some("s24le"));
        assert_eq!(ffmpeg_name(6), None, "24 in 32 has no raw name in ffmpeg");
        assert_eq!(ffmpeg_name(50), None);
    }

    #[test]
    fn every_linear_width_reads_its_top_sixteen_bits() {
        let s16 = Layout::from_alsa(2).unwrap();
        assert_eq!(s16.to_i16(&[0x34, 0x12]), 0x1234);
        assert_eq!(Layout::from_alsa(3).unwrap().to_i16(&[0x12, 0x34]), 0x1234);
        assert_eq!(
            Layout::from_alsa(4).unwrap().to_i16(&[0x00, 0x80]),
            0,
            "unsigned midpoint is silence"
        );
        assert_eq!(Layout::from_alsa(0).unwrap().to_i16(&[0x80u8]), i16::MIN);
        assert_eq!(Layout::from_alsa(1).unwrap().to_i16(&[0xFFu8]), 0x7F00);
        // 24 bits in four bytes, right-justified: the top byte is ignored.
        let s24 = Layout::from_alsa(6).unwrap();
        assert_eq!(s24.to_i16(&[0x56, 0x34, 0x12, 0xEE]), 0x1234);
        let s24_be = Layout::from_alsa(7).unwrap();
        assert_eq!(s24_be.to_i16(&[0xEE, 0x12, 0x34, 0x56]), 0x1234);
        // 24 bits in three bytes.
        let s24_3 = Layout::from_alsa(32).unwrap();
        assert_eq!(s24_3.to_i16(&[0x56, 0x34, 0x12]), 0x1234);
        assert_eq!(s24_3.to_i16(&[0x00, 0x00, 0x80]), i16::MIN);
        assert_eq!(
            Layout::from_alsa(33).unwrap().to_i16(&[0x12, 0x34, 0x56]),
            0x1234
        );
        // 20 bits in three bytes and in four.
        assert_eq!(
            Layout::from_alsa(36).unwrap().to_i16(&[0x45, 0x23, 0x01]),
            0x1234
        );
        assert_eq!(
            Layout::from_alsa(25)
                .unwrap()
                .to_i16(&[0x45, 0x23, 0x01, 0x00]),
            0x1234
        );
        // 32 bits, signed and unsigned.
        assert_eq!(
            Layout::from_alsa(10)
                .unwrap()
                .to_i16(&[0x78, 0x56, 0x34, 0x12]),
            0x1234
        );
        assert_eq!(
            Layout::from_alsa(11)
                .unwrap()
                .to_i16(&[0x12, 0x34, 0x56, 0x78]),
            0x1234
        );
        assert_eq!(
            Layout::from_alsa(12)
                .unwrap()
                .to_i16(&[0x78, 0x56, 0x34, 0x92]),
            0x1234
        );
        // Floats, both endians, clamped.
        assert_eq!(
            Layout::from_alsa(14).unwrap().to_i16(&0.5f32.to_le_bytes()),
            16383
        );
        assert_eq!(
            Layout::from_alsa(15)
                .unwrap()
                .to_i16(&(-2.0f32).to_be_bytes()),
            -32767
        );
        assert_eq!(
            Layout::from_alsa(14)
                .unwrap()
                .to_i16(&f32::NAN.to_le_bytes()),
            0
        );
        // Short input is silence, not a panic.
        assert_eq!(s24_3.to_i16(&[0x12]), 0);
    }

    #[test]
    fn dsd_and_unknown_formats_are_told_apart() {
        for f in [48, 49, 50, 51, 52] {
            let l = Layout::from_alsa(f).unwrap();
            assert!(l.is_dsd());
            assert_eq!(l.to_i16(&[0xFF; 4]), 0);
        }
        assert_eq!(Layout::from_alsa(50).unwrap().bytes(), 4);
        assert_eq!(Layout::from_alsa(48).unwrap().bytes(), 1);
        assert_eq!(Layout::from_alsa(16), None, "64-bit float is not read");
        assert_eq!(Layout::from_alsa(20), None, "mu-law is not read");
        assert_eq!(Layout::from_alsa(-1), None);
        assert_eq!(Layout::from_alsa(53), None);
    }
}
