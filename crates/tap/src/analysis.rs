//! The measurements: a hop at a time, the peak and RMS of each channel and
//! the magnitude spectrum of the last `fft_size` samples of each channel
//! through a Hann window.

use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

use crate::ring::{Frame, MAX_CHANNELS};

pub struct Analyser {
    rate: u32,
    channels: usize,
    fft_size: usize,
    hop: usize,
    window: Vec<f32>,
    /// What a full-scale sine reads at its bin before scaling: half the
    /// window's sum.
    scale: f32,
    fft: Arc<dyn Fft<f32>>,
    work: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    history: [Vec<f32>; MAX_CHANNELS],
    pos: usize,
    hop_fill: usize,
    frames: u64,
    peak: [f32; MAX_CHANNELS],
    square_sum: [f64; MAX_CHANNELS],
    out: Frame,
}

impl Analyser {
    /// For a stream of `channels` at `rate`; `fft_size` a power of two;
    /// `hop` how many frames between measurements.
    pub fn new(rate: u32, channels: u32, fft_size: usize, hop: usize) -> Self {
        let fft_size = fft_size.clamp(64, 1 << 15).next_power_of_two();
        let hop = hop.clamp(1, fft_size);
        let channels = (channels as usize).clamp(1, MAX_CHANNELS);
        let window: Vec<f32> = (0..fft_size)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / fft_size as f32).cos())
            .collect();
        let scale = window.iter().sum::<f32>() / 2.0;
        let fft = FftPlanner::<f32>::new().plan_fft_forward(fft_size);
        let scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
        let bins = fft_size / 2;
        Self {
            rate,
            channels,
            fft_size,
            hop,
            window,
            scale,
            fft,
            work: vec![Complex::default(); fft_size],
            scratch,
            history: [vec![0.0; fft_size], vec![0.0; fft_size]],
            pos: 0,
            hop_fill: 0,
            frames: 0,
            peak: [0.0; MAX_CHANNELS],
            square_sum: [0.0; MAX_CHANNELS],
            out: Frame {
                spectrum: [vec![0.0; bins], vec![0.0; bins]],
                ..Frame::default()
            },
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }
    pub fn channels(&self) -> usize {
        self.channels
    }
    pub fn fft_size(&self) -> usize {
        self.fft_size
    }
    pub fn hop(&self) -> usize {
        self.hop
    }
    pub fn bins(&self) -> usize {
        self.fft_size / 2
    }

    /// Forget the stream so far; the next hop starts from silence.
    pub fn reset(&mut self) {
        for h in self.history.iter_mut() {
            h.fill(0.0);
        }
        self.pos = 0;
        self.hop_fill = 0;
        self.peak = [0.0; MAX_CHANNELS];
        self.square_sum = [0.0; MAX_CHANNELS];
    }

    /// Feed `n` frames, `chans[ch][i]` being frame `i` of channel `ch`; a
    /// mono stream gives one channel and is measured as both. `sink` is
    /// called once per completed hop.
    pub fn feed(&mut self, chans: &[&[i16]], n: usize, sink: &mut dyn FnMut(&Frame)) {
        let used = chans.len().min(self.channels).max(1);
        for i in 0..n {
            for ch in 0..MAX_CHANNELS {
                let source = if ch < used { ch } else { 0 };
                let sample = chans
                    .get(source)
                    .and_then(|c| c.get(i))
                    .copied()
                    .unwrap_or(0);
                let s = sample as f32 / 32768.0;
                self.history[ch][self.pos] = s;
                let a = s.abs();
                if a > self.peak[ch] {
                    self.peak[ch] = a;
                }
                self.square_sum[ch] += (s as f64) * (s as f64);
            }
            self.pos = (self.pos + 1) % self.fft_size;
            self.hop_fill += 1;
            self.frames += 1;
            if self.hop_fill >= self.hop {
                self.measure();
                sink(&self.out);
                self.hop_fill = 0;
                self.peak = [0.0; MAX_CHANNELS];
                self.square_sum = [0.0; MAX_CHANNELS];
            }
        }
    }

    fn measure(&mut self) {
        let n = self.fft_size;
        let hop = self.hop as f64;
        self.out.frames = self.frames;
        for ch in 0..MAX_CHANNELS {
            self.out.peak[ch] = self.peak[ch].min(1.0);
            self.out.rms[ch] = (self.square_sum[ch] / hop).sqrt().min(1.0) as f32;
            // The last `n` samples, oldest first, through the window.
            for k in 0..n {
                let s = self.history[ch][(self.pos + k) % n];
                self.work[k] = Complex {
                    re: s * self.window[k],
                    im: 0.0,
                };
            }
            self.fft
                .process_with_scratch(&mut self.work, &mut self.scratch);
            let out = &mut self.out.spectrum[ch];
            for (k, v) in out.iter_mut().enumerate() {
                *v = (self.work[k].norm() / self.scale).min(1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, hz: f32, amplitude: f32, n: usize) -> Vec<i16> {
        (0..n)
            .map(|i| {
                (amplitude
                    * (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin()
                    * 32767.0) as i16
            })
            .collect()
    }

    #[test]
    fn a_full_scale_sine_reads_one_at_its_bin_and_its_peak_and_rms() {
        let rate = 48_000;
        let fft = 1024;
        let hz = 48_000.0 / 1024.0 * 40.0; // bin 40 exactly
        let left = sine(rate, hz, 1.0, 4096);
        let right = sine(rate, hz, 0.5, 4096);
        let mut analyser = Analyser::new(rate, 2, fft, 512);
        let mut frames = Vec::new();
        analyser.feed(&[&left, &right], 4096, &mut |f| frames.push(f.clone()));
        assert_eq!(frames.len(), 8, "one hop every 512 frames");
        let last = frames.last().unwrap();
        assert!(
            last.peak[0] > 0.99 && last.peak[0] <= 1.0,
            "left peak {}",
            last.peak[0]
        );
        assert!(
            (last.peak[1] - 0.5).abs() < 0.01,
            "right peak {}",
            last.peak[1]
        );
        assert!(
            (last.rms[0] - 0.707).abs() < 0.01,
            "left rms {}",
            last.rms[0]
        );
        assert!(
            (last.spectrum[0][40] - 1.0).abs() < 0.02,
            "bin 40 reads {}",
            last.spectrum[0][40]
        );
        assert!(
            (last.spectrum[1][40] - 0.5).abs() < 0.02,
            "right bin 40 reads {}",
            last.spectrum[1][40]
        );
        assert!(
            last.spectrum[0][200] < 0.001,
            "far bins are quiet: {}",
            last.spectrum[0][200]
        );
        assert_eq!(last.frames, 4096);
        // Mono is measured as both channels.
        let mut mono = Analyser::new(rate, 1, fft, 512);
        let mut got = Vec::new();
        mono.feed(&[&left], 1024, &mut |f| got.push(f.clone()));
        assert_eq!(got.last().unwrap().peak[0], got.last().unwrap().peak[1]);
    }

    #[test]
    fn silence_reads_zero_and_a_reset_forgets() {
        let mut analyser = Analyser::new(44_100, 2, 256, 128);
        let loud = sine(44_100, 1000.0, 1.0, 256);
        let mut frames = Vec::new();
        analyser.feed(&[&loud, &loud], 256, &mut |f| frames.push(f.clone()));
        analyser.reset();
        let quiet = vec![0i16; 256];
        analyser.feed(&[&quiet, &quiet], 256, &mut |f| frames.push(f.clone()));
        let last = frames.last().unwrap();
        assert_eq!(last.peak, [0.0, 0.0]);
        assert_eq!(last.rms, [0.0, 0.0]);
        assert!(last.spectrum[0].iter().all(|v| *v == 0.0));
    }
}
