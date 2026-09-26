//! The two FIFO records the previous tap wrote, made from this tap's
//! measurements so both can run side by side: a meter record of two 16-bit
//! levels with the old decay, and a spectrum record of `size` 32-bit values
//! on the old logarithmic bins with the old smoothing.

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

/// The spectrum as it was: `size` bins spaced logarithmically over the
/// first 256 of a 512-point FFT's outputs, each the summed magnitude of its
/// FFT bins on the 16-bit scale, smoothed against the last value, and
/// squeezed with a logarithm.
pub struct Spectrum {
    size: usize,
    max: u32,
    log_f: bool,
    log_y: bool,
    smooth: f64,
    edges: Vec<f64>,
    held: Vec<f64>,
}

impl Spectrum {
    pub fn new(size: usize, max: u32, log_f: bool, log_y: bool, smoothing_factor: u32) -> Self {
        let size = size.clamp(1, 256);
        // Bin edges on the old 257-output scale, 1 through 257.
        let mut edges = vec![1.0f64; size + 1];
        if log_f {
            let log_gs = (256f64).log10() / size as f64;
            for m in 1..=size {
                let mut width = 10f64.powf(log_gs * m as f64) - edges[m - 1];
                if width < 1.0 {
                    width = 1.0;
                }
                edges[m] = edges[m - 1] + width;
            }
        } else {
            let group = (257 / size) as f64;
            for m in 1..=size {
                edges[m] = edges[m - 1] + group;
            }
        }
        Self {
            size,
            max,
            log_f,
            log_y,
            smooth: smoothing_factor.min(100) as f64,
            edges,
            held: vec![0.0; size],
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// One record from a spectrum of `magnitudes` (a full-scale sine reading
    /// 1.0 at its bin) with `bins` values up to half the sample rate.
    pub fn update(&mut self, magnitudes: &[f32]) -> Vec<u32> {
        let bins = magnitudes.len().max(1) as f64;
        // The old scale had 256 outputs below Nyquist; ours has `bins`.
        let per_old = bins / 256.0;
        let mut out = Vec::with_capacity(self.size);
        for m in 0..self.size {
            let from = (self.edges[m] * per_old) as usize;
            let to = ((self.edges[m + 1] * per_old) as usize)
                .max(from + 1)
                .min(magnitudes.len());
            let mut y = 0.0f64;
            if from < magnitudes.len() {
                for v in &magnitudes[from..to] {
                    // The old value summed |X| / N over the bin, |X| / N being the amplitude over two.
                    y += *v as f64 * 16383.5;
                }
                y /= per_old.max(1.0);
            }
            y = y.clamp(0.0, 65535.0);
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
        let _ = self.log_f;
        out
    }
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
    fn a_single_tone_lands_in_one_bin_and_smooths_in() {
        let mut spectrum = Spectrum::new(20, 100, true, true, 60);
        assert_eq!(spectrum.size(), 20);
        let mut magnitudes = vec![0.0f32; 1024];
        magnitudes[600] = 1.0; // high up: the last few log bins
        let first = spectrum.update(&magnitudes);
        assert_eq!(first.len(), 20);
        let lit: Vec<usize> = first
            .iter()
            .enumerate()
            .filter(|(_, v)| **v > 0)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(lit.len(), 1, "one bin lit: {first:?}");
        assert!(lit[0] >= 15, "a high tone lands high: {}", lit[0]);
        let second = spectrum.update(&magnitudes);
        assert!(
            second[lit[0]] >= first[lit[0]],
            "smoothing rises towards the value"
        );
        let quiet = spectrum.update(&vec![0.0; 1024]);
        assert!(
            quiet[lit[0]] < second[lit[0]],
            "and falls when the tone stops"
        );
        assert_eq!(spectrum_record(&[1, 258]).len(), 8);
    }
}
