//! Poll the outside world and return the latest [`lead::Input`].
//! This station does not parse skin geometry or draw.

use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use lead::{
    decode_meter, decode_spectrum, mono_average, scale_level, Bins, Input, Levels,
    DEFAULT_METER_MAX, DEFAULT_SPECTRUM_BINS, METER_FIFO, SPECTRUM_FIFO,
};

/// Linux `O_NONBLOCK`. A blocking open on a FIFO waits for the writer.
const O_NONBLOCK: i32 = 0x800;

/// A source of snapshots. The player polls FIFOs. The remote will poll UDP.
pub trait Source {
    fn poll(&mut self) -> Input;
}

/// Placeholder source used when no pipe is open.
#[derive(Debug, Default)]
pub struct IdleSource;

impl Source for IdleSource {
    fn poll(&mut self) -> Input {
        Input::default()
    }
}

/// Bytes already pulled off a pipe. Keeps only the latest complete record.
#[derive(Debug, Default)]
struct RecordBuf {
    pending: Vec<u8>,
    record: usize,
    latest: Option<Vec<u8>>,
}

impl RecordBuf {
    fn new(record: usize) -> Self {
        Self {
            pending: Vec::new(),
            record,
            latest: None,
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        if self.record == 0 {
            return;
        }
        self.pending.extend_from_slice(chunk);
        while self.pending.len() >= self.record {
            let rec: Vec<u8> = self.pending.drain(..self.record).collect();
            self.latest = Some(rec);
        }
    }
}

/// Installed meter and spectrum FIFOs.
pub struct PipeSource {
    meter_path: String,
    spectrum_path: String,
    meter: Option<File>,
    spectrum: Option<File>,
    meter_buf: RecordBuf,
    spectrum_buf: RecordBuf,
    spectrum_bins: usize,
    meter_max: f32,
    spectrum_held: Vec<f32>,
    levels_held: Levels,
}

impl PipeSource {
    /// Open the Volumio pipes. Missing files stay closed and are retried on poll.
    pub fn installed() -> Self {
        Self::new(
            METER_FIFO,
            SPECTRUM_FIFO,
            DEFAULT_SPECTRUM_BINS,
            DEFAULT_METER_MAX,
        )
    }

    pub fn new(
        meter_path: impl Into<String>,
        spectrum_path: impl Into<String>,
        spectrum_bins: usize,
        meter_max: f32,
    ) -> Self {
        let bins = spectrum_bins.max(1);
        Self {
            meter_path: meter_path.into(),
            spectrum_path: spectrum_path.into(),
            meter: None,
            spectrum: None,
            meter_buf: RecordBuf::new(4),
            spectrum_buf: RecordBuf::new(bins * 4),
            spectrum_bins: bins,
            meter_max,
            spectrum_held: Vec::new(),
            levels_held: Levels::default(),
        }
    }

    fn try_open(slot: &mut Option<File>, path: &str) {
        if slot.is_some() || !Path::new(path).exists() {
            return;
        }
        if let Ok(file) = OpenOptions::new()
            .read(true)
            .custom_flags(O_NONBLOCK)
            .open(path)
        {
            *slot = Some(file);
        }
    }

    fn drain(file: &mut File, buf: &mut RecordBuf) {
        let mut chunk = [0u8; 4096];
        loop {
            match file.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => buf.push(&chunk[..n]),
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    }
}

impl Source for PipeSource {
    fn poll(&mut self) -> Input {
        Self::try_open(&mut self.meter, &self.meter_path);
        Self::try_open(&mut self.spectrum, &self.spectrum_path);

        if let Some(file) = self.meter.as_mut() {
            Self::drain(file, &mut self.meter_buf);
        }
        if let Some(file) = self.spectrum.as_mut() {
            Self::drain(file, &mut self.spectrum_buf);
        }

        if let Some(record) = self.meter_buf.latest.take() {
            if let Some((left, right)) = decode_meter(&record) {
                let left = scale_level(left, self.meter_max, self.meter_max);
                let right = scale_level(right, self.meter_max, self.meter_max);
                self.levels_held = Levels {
                    mono: mono_average(left, right),
                    left,
                    right,
                };
            }
        }
        if let Some(record) = self.spectrum_buf.latest.take() {
            if let Some(values) = decode_spectrum(&record, self.spectrum_bins) {
                self.spectrum_held = values;
            }
        }

        Input {
            levels: self.levels_held,
            bins: Bins {
                values: self.spectrum_held.clone(),
            },
            metadata: lead::Metadata::default(),
        }
    }
}

/// Build an input from one meter record and one spectrum record.
pub fn input_from_records(
    meter: &[u8],
    spectrum: &[u8],
    spectrum_bins: usize,
    meter_max: f32,
) -> Input {
    let levels = decode_meter(meter)
        .map(|(left, right)| {
            let left = scale_level(left, meter_max, meter_max);
            let right = scale_level(right, meter_max, meter_max);
            Levels {
                mono: mono_average(left, right),
                left,
                right,
            }
        })
        .unwrap_or_default();
    let bins = decode_spectrum(spectrum, spectrum_bins).unwrap_or_default();
    Input {
        levels,
        bins: Bins { values: bins },
        metadata: lead::Metadata::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_become_ui_levels_and_bins() {
        let input = input_from_records(
            &[50, 0, 25, 0],
            &[100, 0, 0, 0, 0, 0, 0, 0],
            2,
            100.0,
        );
        assert_eq!(input.levels.left, 50.0);
        assert_eq!(input.levels.right, 25.0);
        assert_eq!(input.levels.mono, 37.5);
        assert_eq!(input.bins.values, vec![100.0, 0.0]);
    }

    #[test]
    fn partial_chunks_keep_the_latest_record() {
        let mut buf = RecordBuf::new(4);
        buf.push(&[1, 0]);
        buf.push(&[0, 0, 9, 0, 0, 0]);
        assert_eq!(buf.latest.unwrap(), vec![9, 0, 0, 0]);
    }
}
