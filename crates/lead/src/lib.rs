//! FIFO bytes become these types. This station does not open devices or draw.

/// Installed meter pipe. The ALSA scope writes it.
pub const METER_FIFO: &str = "/tmp/myfifo";

/// Installed spectrum pipe. The ALSA scope writes it.
pub const SPECTRUM_FIFO: &str = "/tmp/myfifosa";

/// Bin count in the Volumio ALSA template.
pub const DEFAULT_SPECTRUM_BINS: usize = 20;

/// `meter_max` written by the scope, and the UI full-scale that matches it.
pub const DEFAULT_METER_MAX: f32 = 100.0;

/// `spectrum_max` written by the scope.
pub const DEFAULT_SPECTRUM_MAX: f32 = 100.0;

/// Left and right in UI units, plus mono derived from them.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Levels {
    pub left: f32,
    pub right: f32,
    pub mono: f32,
}

/// Latest spectrum frame, raw scope units. Older frames are discarded upstream.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bins {
    pub values: Vec<f32>,
}

/// Now-playing text for the surface.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    pub title: String,
    pub artist: String,
    pub album: String,
}

/// One snapshot of the outside world. `plot` turns it into a scene.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Input {
    pub levels: Levels,
    pub bins: Bins,
    pub metadata: Metadata,
}

/// Geometry the scene is plotted into. Pixels stay in `expose`.
#[derive(Debug, Clone, PartialEq)]
pub struct SkinDesc {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub meter_max: f32,
    pub spectrum_max: f32,
}

impl Default for SkinDesc {
    fn default() -> Self {
        Self::basic()
    }
}

impl SkinDesc {
    pub fn basic() -> Self {
        Self {
            name: "basic".into(),
            width: 800,
            height: 480,
            meter_max: DEFAULT_METER_MAX,
            spectrum_max: DEFAULT_SPECTRUM_MAX,
        }
    }
}

/// Last complete meter record: little-endian `u16` left, then right.
///
/// The scope packs the same bits as `left + (right << 16)`.
pub fn decode_meter(record: &[u8]) -> Option<(u16, u16)> {
    if record.len() != 4 {
        return None;
    }
    let left = u16::from_le_bytes([record[0], record[1]]);
    let right = u16::from_le_bytes([record[2], record[3]]);
    Some((left, right))
}

/// One spectrum frame: `bin_count` little-endian `i32` values.
pub fn decode_spectrum(record: &[u8], bin_count: usize) -> Option<Vec<f32>> {
    if bin_count == 0 || record.len() != bin_count * 4 {
        return None;
    }
    let mut values = Vec::with_capacity(bin_count);
    for chunk in record.chunks_exact(4) {
        let raw = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        values.push(raw.max(0) as f32);
    }
    Some(values)
}

/// Map a scope reading onto the UI scale. Values above the pipe max clamp.
pub fn scale_level(raw: u16, max_pipe: f32, max_ui: f32) -> f32 {
    if max_pipe <= 0.0 || max_ui <= 0.0 {
        return 0.0;
    }
    (raw as f32 / max_pipe * max_ui).clamp(0.0, max_ui)
}

/// Mono is the average of the two channels.
pub fn mono_average(left: f32, right: f32) -> f32 {
    (left + right) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_record_is_left_then_right() {
        let (left, right) = decode_meter(&[50, 0, 25, 0]).unwrap();
        assert_eq!((left, right), (50, 25));
    }

    #[test]
    fn spectrum_record_is_little_endian_bins() {
        let bins = decode_spectrum(&[100, 0, 0, 0, 0, 0, 0, 0], 2).unwrap();
        assert_eq!(bins, vec![100.0, 0.0]);
    }

    #[test]
    fn scale_clamps_at_full() {
        assert_eq!(scale_level(150, 100.0, 100.0), 100.0);
        assert_eq!(scale_level(50, 100.0, 100.0), 50.0);
    }
}
