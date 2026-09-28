//! The two FIFO records the previous tap wrote, made from this tap's
//! measurements so both can run side by side: a meter record of two 16-bit
//! levels with the old decay, and a spectrum record of `size` 32-bit values
//! on the old logarithmic bins with the old smoothing. The same regrouping
//! gives a theme of the previous engine its bars from the bank.

use bank::Scale;

/// The meter as it was: per update, the peak of the update's samples on the
/// 16-bit scale, falling no faster than `decay_ms` allows from full scale.
pub struct Meter {
    decay_ms: u32,
    max: u32,
    held: [i32; 2],
}

impl Meter {
    pub fn new(decay_ms: u32, max: u32) -> Self {
        Self {
            decay_ms: decay_ms.max(1),
            max,
            held: [0; 2],
        }
    }

    /// One update: the raw peaks, 0 through 32767, over `frames` at `rate`.
    /// The two levels on the 0 through `max` scale, as the old record held them.
    pub fn update(&mut self, raw: [i32; 2], frames: u64, rate: u32) -> (u16, u16) {
        let ms = (frames * 1000 / rate.max(1) as u64) as i64;
        let max_decay = (32768 * ms / self.decay_ms as i64) as i32;
        let mut out = [0u16; 2];
        for ch in 0..2 {
            let mut lev = raw[ch].clamp(0, 32767);
            let held = self.held[ch];
            if lev < held && held > 0 {
                let step = max_decay / (32767 / held).max(1);
                lev = (held - step).max(0);
            }
            self.held[ch] = lev;
            out[ch] = ((lev as f32 / 32767.0) * self.max as f32) as u16;
        }
        (out[0], out[1])
    }
}

/// The old spectrum's outputs below half the sample rate: a 512-point FFT
/// gave 256 of them, index `k` standing for `k` times the rate over 512.
const OLD_OUTPUTS: f64 = 256.0;
const OLD_FFT: f64 = 512.0;

/// The spectrum as it was, from the bank: `size` bars spaced
/// logarithmically over the old 256 outputs, each fed by the bands whose
/// centre falls in its range, smoothed against the last value, and squeezed
/// with a logarithm. A full-scale sine in one band reads as the old tap
/// read it at one output.
pub struct Regroup {
    size: usize,
    max: u32,
    log_y: bool,
    smooth: f64,
    /// Bar edges on the old 257-output scale, 1 through 257.
    edges: Vec<f64>,
    held: Vec<f64>,
    /// The bands of each bar, for the bank shape and rate last seen.
    members: Vec<Vec<usize>>,
    shape: Option<(usize, Scale, u32)>,
}

impl Regroup {
    pub fn new(size: usize, max: u32, log_f: bool, log_y: bool, smoothing_factor: u32) -> Self {
        let size = size.clamp(1, 256);
        let mut edges = vec![1.0f64; size + 1];
        if log_f {
            let log_gs = OLD_OUTPUTS.log10() / size as f64;
            for m in 1..=size {
                let mut width = 10f64.powf(log_gs * m as f64) - edges[m - 1];
                if width < 1.0 {
                    width = 1.0;
                }
                edges[m] = edges[m - 1] + width;
            }
        } else {
            let group = ((OLD_OUTPUTS as usize + 1) / size) as f64;
            for m in 1..=size {
                edges[m] = edges[m - 1] + group;
            }
        }
        Self {
            size,
            max,
            log_y,
            smooth: smoothing_factor.min(100) as f64,
            edges,
            held: vec![0.0; size],
            members: Vec::new(),
            shape: None,
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// Each bar's range in hertz at `rate`, from the old output scale.
    pub fn bar_ranges_hz(&self, rate: u32) -> Vec<(f32, f32)> {
        let per_output = rate as f64 / OLD_FFT;
        (0..self.size)
            .map(|m| {
                (
                    (self.edges[m] * per_output) as f32,
                    (self.edges[m + 1] * per_output) as f32,
                )
            })
            .collect()
    }

    /// Which bands feed which bar, worked out once per bank shape: the
    /// bands whose centre falls in the bar's range, or the band that holds
    /// the bar's middle when none does.
    fn place(&mut self, bins: usize, scale: Scale, rate: u32) {
        if self.shape == Some((bins, scale, rate)) {
            return;
        }
        let edges = bank::edges(bins, scale);
        let centres = bank::centres(bins, scale);
        let ranges = self.bar_ranges_hz(rate);
        self.members = ranges
            .iter()
            .map(|&(lo, hi)| {
                let inside: Vec<usize> = centres
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| **c >= lo && **c < hi)
                    .map(|(i, _)| i)
                    .collect();
                if !inside.is_empty() {
                    return inside;
                }
                let middle = (lo + hi) / 2.0;
                edges
                    .iter()
                    .position(|&(a, b)| middle >= a && middle < b)
                    .map(|i| vec![i])
                    .unwrap_or_default()
            })
            .collect();
        self.shape = Some((bins, scale, rate));
    }

    /// One record from one channel's bands on `scale`, at `rate`.
    pub fn update(&mut self, bands: &[f32], scale: Scale, rate: u32) -> Vec<u32> {
        self.place(bands.len().max(1), scale, rate);
        let mut out = Vec::with_capacity(self.size);
        for m in 0..self.size {
            // The bar's energy over its bands, on the old 16-bit scale: a
            // full-scale sine in one band read 16383.5 at one output.
            let power: f64 = self.members[m]
                .iter()
                .filter_map(|&i| bands.get(i))
                .map(|v| (*v as f64) * (*v as f64))
                .sum();
            let mut y = (power.sqrt() * 16383.5).clamp(0.0, 65535.0);
            y = (self.smooth * self.held[m] + (100.0 - self.smooth) * y) / 100.0;
            self.held[m] = y;
            let mut v = if self.log_y {
                y.log10() / 4.82
            } else {
                y / 65535.0
            };
            if !v.is_finite() || v < 0.0 {
                v = 0.0;
            }
            out.push((v.min(1.0) * self.max as f64) as u32);
        }
        out
    }
}

/// A frame of a tap that measured before the bank made to read as the
/// rest: its raw spectrum projected onto the projector's bands, with the
/// hold the projector keeps. A frame that is not raw is left alone.
pub fn project_raw(frame: &mut crate::ring::Frame, projector: &mut bank::Projector) {
    if !frame.raw {
        return;
    }
    let bank = projector.project([&frame.spectrum[0], &frame.spectrum[1]]);
    frame.spectrum = [bank.channel(0).to_vec(), bank.channel(1).to_vec()];
    frame.hold = [bank.channel_hold(0).to_vec(), bank.channel_hold(1).to_vec()];
    frame.onsets = 0;
    frame.scale = projector.demand().scale;
    frame.raw = false;
}

/// The meter record: the two levels, little-endian 16 bits each.
pub fn meter_record(left: u16, right: u16) -> [u8; 4] {
    let mut r = [0u8; 4];
    r[..2].copy_from_slice(&left.to_le_bytes());
    r[2..].copy_from_slice(&right.to_le_bytes());
    r
}

/// The spectrum record: each bin a little-endian 32-bit value.
pub fn spectrum_record(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_meter_falls_no_faster_than_the_decay() {
        let mut meter = Meter::new(400, 100);
        assert_eq!(meter.update([32767, 16383], 441, 44_100), (100, 49));
        // A silent 10 ms update drops a full-scale level by 10/400 of full scale, not to zero.
        let (l, _) = meter.update([0, 0], 441, 44_100);
        assert!(l > 90 && l < 100, "decayed a little: {l}");
        let record = meter_record(l, 7);
        assert_eq!(u16::from_le_bytes([record[0], record[1]]), l);
        assert_eq!(u16::from_le_bytes([record[2], record[3]]), 7);
    }

    #[test]
    fn a_single_tone_lands_in_the_bar_that_holds_it_and_smooths_in() {
        let mut regroup = Regroup::new(20, 100, true, true, 60);
        assert_eq!(regroup.size(), 20);
        // A 256-band log bank with one band lit: the one holding 1 kHz.
        let centres = bank::centres(256, Scale::Log);
        let lit_band = centres.iter().position(|c| *c >= 1_000.0).unwrap();
        let mut bands = vec![0.0f32; 256];
        bands[lit_band] = 1.0;
        let first = regroup.update(&bands, Scale::Log, 48_000);
        assert_eq!(first.len(), 20);
        let lit: Vec<usize> = first
            .iter()
            .enumerate()
            .filter(|(_, v)| **v > 0)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(lit.len(), 1, "one bar lit: {first:?}");
        let (lo, hi) = regroup.bar_ranges_hz(48_000)[lit[0]];
        assert!(
            lo <= centres[lit_band] && centres[lit_band] < hi,
            "the bar {} spans {lo} to {hi} Hz",
            lit[0]
        );
        let second = regroup.update(&bands, Scale::Log, 48_000);
        assert!(
            second[lit[0]] >= first[lit[0]],
            "smoothing rises towards the value"
        );
        let quiet = regroup.update(&vec![0.0; 256], Scale::Log, 48_000);
        assert!(
            quiet[lit[0]] < second[lit[0]],
            "and falls when the tone stops"
        );
        assert_eq!(spectrum_record(&[1, 258]).len(), 8);
    }

    #[test]
    fn a_raw_frame_is_projected_once_and_a_bank_frame_left_alone() {
        let mut raw = crate::ring::Frame {
            spectrum: [vec![0.0; 1024], vec![0.0; 1024]],
            window: 2048,
            raw: true,
            ..Default::default()
        };
        raw.spectrum[0][43] = 1.0;
        let mut projector =
            bank::Projector::new(48_000, 2048, 1024, bank::Demand::new(64, 2, Scale::Log));
        project_raw(&mut raw, &mut projector);
        assert!(!raw.raw);
        assert_eq!(raw.spectrum[0].len(), 64);
        assert_eq!(raw.hold[1].len(), 64);
        assert!(raw.spectrum[0].iter().any(|m| *m > 0.5));
        assert!(raw.spectrum[1].iter().all(|m| *m == 0.0));
        let before = raw.clone();
        project_raw(&mut raw, &mut projector);
        assert_eq!(raw, before, "a bank frame is not projected again");
    }

    #[test]
    fn every_bar_within_the_bank_has_a_band_to_read_whatever_the_bank() {
        for (bins, scale) in [(32, Scale::Log), (64, Scale::Mel), (256, Scale::Linear)] {
            let mut regroup = Regroup::new(128, 100, true, false, 0);
            let bands = vec![1.0f32; bins];
            let out = regroup.update(&bands, scale, 44_100);
            let ranges = regroup.bar_ranges_hz(44_100);
            for (m, value) in out.iter().enumerate() {
                let (lo, _) = ranges[m];
                if lo < bank::HIGH_HZ {
                    assert!(
                        *value > 0,
                        "{bins} {scale:?}: bar {m} reads nothing: {out:?}"
                    );
                } else {
                    assert_eq!(*value, 0, "above the bank there is nothing to read");
                }
            }
        }
    }
}
