//! The shared ring: a file under `/dev/shm` with a header and a ring of
//! slots, written by the tap and read by the display. One file per writing
//! process, so two players loaded with the tap never write over each other;
//! the reader takes the one written most recently.
//!
//! Layout, little-endian, all offsets fixed by the header:
//!
//! - header, 256 bytes: magic `GLASSTAP`, version, header size, sample rate,
//!   channels, FFT size, bins per channel, slot count, slot size, writer
//!   pid, hop, then the sequence of the last slot published and the time it
//!   was published, both written last with release ordering.
//! - slots: sequence, time in nanoseconds since the monotonic clock's
//!   start, frames seen so far, peak per channel, RMS per channel, spectrum
//!   per channel, and the sequence again. A reader whose two sequences
//!   disagree with the header's caught a write in flight and tries again.

use std::ffi::CString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const MAGIC: &[u8; 8] = b"GLASSTAP";
pub const VERSION: u32 = 1;
pub const DIR: &str = "/dev/shm";
pub const PREFIX: &str = "glasstap.";
pub const MAX_CHANNELS: usize = 2;
pub const HEADER_BYTES: usize = 256;
/// A ring written longer ago than this is a leftover of a player that stopped.
pub const LIVE_NS: u64 = 3_000_000_000;

/// One hop's measurements. Peak and RMS are linear, 1.0 being full scale;
/// the spectrum is the amplitude at each FFT bin, a full-scale sine reading
/// 1.0 at its bin, from 0 Hz up to just under half the sample rate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    pub seq: u64,
    pub time_ns: u64,
    pub frames: u64,
    pub peak: [f32; MAX_CHANNELS],
    pub rms: [f32; MAX_CHANNELS],
    pub spectrum: [Vec<f32>; MAX_CHANNELS],
}

/// What the header says about the stream and the ring.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Info {
    pub rate: u32,
    pub channels: u32,
    pub fft_size: u32,
    pub bins: u32,
    pub slots: u32,
    pub slot_bytes: u32,
    pub pid: u32,
    pub hop: u32,
    pub seq: u64,
    pub written_ns: u64,
}

const H_MAGIC: usize = 0;
const H_VERSION: usize = 8;
const H_HEADER_BYTES: usize = 12;
const H_RATE: usize = 16;
const H_CHANNELS: usize = 20;
const H_FFT: usize = 24;
const H_BINS: usize = 28;
const H_SLOTS: usize = 32;
const H_SLOT_BYTES: usize = 36;
const H_PID: usize = 40;
const H_HOP: usize = 44;
const H_SEQ: usize = 48;
const H_WRITTEN: usize = 56;

const S_SEQ: usize = 0;
const S_TIME: usize = 8;
const S_FRAMES: usize = 16;
const S_PEAK: usize = 24;
const S_RMS: usize = 32;
const S_SPECTRUM: usize = 40;

fn slot_bytes(bins: usize) -> usize {
    let raw = S_SPECTRUM + bins * MAX_CHANNELS * 4 + 8;
    (raw + 63) / 64 * 64
}

/// Nanoseconds of the monotonic clock.
pub fn now_ns() -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // A valid clock id and a valid pointer: this cannot fail.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

/// A mapped file. Unmapped when dropped.
struct Map {
    ptr: *mut u8,
    len: usize,
}

impl Map {
    fn map(fd: i32, len: usize, write: bool) -> io::Result<Self> {
        let prot = if write { libc::PROT_READ | libc::PROT_WRITE } else { libc::PROT_READ };
        // A shared mapping of `len` bytes of an open file.
        let ptr = unsafe { libc::mmap(std::ptr::null_mut(), len, prot, libc::MAP_SHARED, fd, 0) };
        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { ptr: ptr as *mut u8, len })
    }

    fn bytes(&self) -> &[u8] {
        // The mapping is `len` bytes long for as long as `self` lives.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }

    fn atomic_u64(&self, at: usize) -> &AtomicU64 {
        // Every atomic offset is 8-aligned within a page-aligned mapping.
        unsafe { &*(self.ptr.add(at) as *const AtomicU64) }
    }
}

impl Drop for Map {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.ptr as *mut libc::c_void, self.len) };
    }
}

// The ring is shared between processes by design; the mapping itself is
// owned by one writer or one reader at a time.
unsafe impl Send for Map {}

fn put_u32(bytes: &mut [u8], at: usize, v: u32) {
    bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn get_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}
fn put_u64(bytes: &mut [u8], at: usize, v: u64) {
    bytes[at..at + 8].copy_from_slice(&v.to_le_bytes());
}
fn get_u64(bytes: &[u8], at: usize) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(b)
}
fn put_f32(bytes: &mut [u8], at: usize, v: f32) {
    bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn get_f32(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// The writing side. Creating one creates the file; dropping it removes it.
pub struct Writer {
    map: Map,
    path: PathBuf,
    bins: usize,
    slots: usize,
    slot_bytes: usize,
    seq: u64,
}

impl Writer {
    /// Create the ring for a stream. `slots` is how many hops are kept; the
    /// reader takes the latest, a history view can take more.
    pub fn create(dir: &Path, tag: &str, rate: u32, channels: u32, fft_size: u32, hop: u32, slots: usize) -> io::Result<Self> {
        let bins = (fft_size / 2) as usize;
        let slots = slots.max(2);
        let slot_bytes = slot_bytes(bins);
        let len = HEADER_BYTES + slots * slot_bytes;
        let path = dir.join(format!("{PREFIX}{tag}.{}", std::process::id()));
        let cpath = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        let fd = unsafe { libc::open(cpath.as_ptr(), libc::O_RDWR | libc::O_CREAT | libc::O_CLOEXEC, 0o644) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let sized = unsafe { libc::ftruncate(fd, len as libc::off_t) };
        if sized != 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(err);
        }
        let map = Map::map(fd, len, true);
        unsafe { libc::close(fd) };
        let mut map = map?;
        {
            let bytes = map.bytes_mut();
            bytes[..len].fill(0);
            bytes[H_MAGIC..H_MAGIC + 8].copy_from_slice(MAGIC);
            put_u32(bytes, H_VERSION, VERSION);
            put_u32(bytes, H_HEADER_BYTES, HEADER_BYTES as u32);
            put_u32(bytes, H_RATE, rate);
            put_u32(bytes, H_CHANNELS, channels.min(MAX_CHANNELS as u32));
            put_u32(bytes, H_FFT, fft_size);
            put_u32(bytes, H_BINS, bins as u32);
            put_u32(bytes, H_SLOTS, slots as u32);
            put_u32(bytes, H_SLOT_BYTES, slot_bytes as u32);
            put_u32(bytes, H_PID, std::process::id());
            put_u32(bytes, H_HOP, hop);
        }
        Ok(Self { map, path, bins, slots, slot_bytes, seq: 0 })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Publish one hop. The frame's `seq` and `time_ns` are set here.
    pub fn publish(&mut self, frame: &Frame) {
        self.seq += 1;
        let seq = self.seq;
        let time = now_ns();
        let at = HEADER_BYTES + ((seq as usize) % self.slots) * self.slot_bytes;
        let bins = self.bins;
        {
            let bytes = self.map.bytes_mut();
            let slot = &mut bytes[at..at + self.slot_bytes];
            // The sequence at the head marks the slot as being written.
            put_u64(slot, S_SEQ, seq | (1 << 63));
            put_u64(slot, S_TIME, time);
            put_u64(slot, S_FRAMES, frame.frames);
            for ch in 0..MAX_CHANNELS {
                put_f32(slot, S_PEAK + ch * 4, frame.peak[ch]);
                put_f32(slot, S_RMS + ch * 4, frame.rms[ch]);
                let base = S_SPECTRUM + ch * bins * 4;
                for (k, v) in frame.spectrum[ch].iter().take(bins).enumerate() {
                    put_f32(slot, base + k * 4, *v);
                }
            }
            let tail = S_SPECTRUM + bins * MAX_CHANNELS * 4;
            put_u64(slot, tail, seq);
            put_u64(slot, S_SEQ, seq);
        }
        self.map.atomic_u64(H_WRITTEN).store(time, Ordering::Release);
        self.map.atomic_u64(H_SEQ).store(seq, Ordering::Release);
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// The reading side of one ring.
pub struct Reader {
    map: Map,
    path: PathBuf,
    info: Info,
}

impl Reader {
    /// Open a ring by path.
    pub fn open(path: &Path) -> io::Result<Self> {
        let meta = fs::metadata(path)?;
        let len = meta.len() as usize;
        if len < HEADER_BYTES {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "ring too short"));
        }
        let cpath = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        let fd = unsafe { libc::open(cpath.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let map = Map::map(fd, len, false);
        unsafe { libc::close(fd) };
        let map = map?;
        let bytes = map.bytes();
        if &bytes[H_MAGIC..H_MAGIC + 8] != MAGIC || get_u32(bytes, H_VERSION) != VERSION {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not a glasstap ring"));
        }
        let info = Info {
            rate: get_u32(bytes, H_RATE),
            channels: get_u32(bytes, H_CHANNELS),
            fft_size: get_u32(bytes, H_FFT),
            bins: get_u32(bytes, H_BINS),
            slots: get_u32(bytes, H_SLOTS),
            slot_bytes: get_u32(bytes, H_SLOT_BYTES),
            pid: get_u32(bytes, H_PID),
            hop: get_u32(bytes, H_HOP),
            seq: 0,
            written_ns: 0,
        };
        let needed = HEADER_BYTES + info.slots as usize * info.slot_bytes as usize;
        if info.slots == 0 || info.slot_bytes as usize != slot_bytes(info.bins as usize) || len < needed {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "ring header disagrees with its size"));
        }
        Ok(Self { map, path: path.to_path_buf(), info })
    }

    /// Every ring under `dir`, the most recently written first.
    pub fn find(dir: &Path) -> Vec<PathBuf> {
        let mut found: Vec<(u64, PathBuf)> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(PREFIX))
            .filter_map(|entry| {
                let path = entry.path();
                let reader = Reader::open(&path).ok()?;
                Some((reader.info().written_ns, path))
            })
            .collect();
        found.sort_by(|a, b| b.0.cmp(&a.0));
        found.into_iter().map(|(_, p)| p).collect()
    }

    /// The ring being written right now, if any.
    pub fn open_live(dir: &Path) -> Option<Self> {
        Self::find(dir).into_iter().find_map(|path| Reader::open(&path).ok().filter(Reader::is_live))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The header, with the sequence and time of the last publish.
    pub fn info(&self) -> Info {
        Info {
            seq: self.map.atomic_u64(H_SEQ).load(Ordering::Acquire),
            written_ns: self.map.atomic_u64(H_WRITTEN).load(Ordering::Acquire),
            ..self.info
        }
    }

    /// Written within the last few seconds and still on disk.
    pub fn is_live(&self) -> bool {
        let written = self.map.atomic_u64(H_WRITTEN).load(Ordering::Acquire);
        written != 0 && now_ns().saturating_sub(written) < LIVE_NS && self.path.exists()
    }

    /// The last published hop, or `None` before the first, or when the
    /// writer was mid-write three times in a row.
    pub fn latest(&self) -> Option<Frame> {
        let seq = self.map.atomic_u64(H_SEQ).load(Ordering::Acquire);
        if seq == 0 {
            return None;
        }
        self.slot(seq)
    }

    /// The hop with a given sequence, while the ring still holds it.
    pub fn slot(&self, seq: u64) -> Option<Frame> {
        let latest = self.map.atomic_u64(H_SEQ).load(Ordering::Acquire);
        if seq == 0 || seq > latest || latest - seq >= self.info.slots as u64 {
            return None;
        }
        let bins = self.info.bins as usize;
        let at = HEADER_BYTES + ((seq as usize) % self.info.slots as usize) * self.info.slot_bytes as usize;
        for _ in 0..3 {
            let bytes = self.map.bytes();
            let slot = &bytes[at..at + self.info.slot_bytes as usize];
            let head = get_u64(slot, S_SEQ);
            let tail = get_u64(slot, S_SPECTRUM + bins * MAX_CHANNELS * 4);
            if head != seq || tail != seq {
                std::hint::spin_loop();
                continue;
            }
            let mut frame = Frame { seq, time_ns: get_u64(slot, S_TIME), frames: get_u64(slot, S_FRAMES), ..Frame::default() };
            for ch in 0..MAX_CHANNELS {
                frame.peak[ch] = get_f32(slot, S_PEAK + ch * 4);
                frame.rms[ch] = get_f32(slot, S_RMS + ch * 4);
                let base = S_SPECTRUM + ch * bins * 4;
                frame.spectrum[ch] = (0..bins).map(|k| get_f32(slot, base + k * 4)).collect();
            }
            // The head is written last; if it still matches, the copy is whole.
            if get_u64(slot, S_SEQ) == seq {
                return Some(frame);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glasstap-ring-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_frame_published_is_the_frame_read() {
        let dir = temp_dir();
        let mut writer = Writer::create(&dir, "test", 48_000, 2, 64, 32, 4).unwrap();
        let reader = Reader::open(writer.path()).unwrap();
        assert_eq!(reader.latest(), None, "nothing before the first publish");
        let frame = Frame {
            frames: 4096,
            peak: [0.5, 0.25],
            rms: [0.3, 0.2],
            spectrum: [(0..32).map(|k| k as f32 / 32.0).collect(), (0..32).map(|k| 1.0 - k as f32 / 32.0).collect()],
            ..Frame::default()
        };
        writer.publish(&frame);
        let read = reader.latest().expect("a frame");
        assert_eq!(read.seq, 1);
        assert_eq!(read.frames, 4096);
        assert_eq!(read.peak, frame.peak);
        assert_eq!(read.rms, frame.rms);
        assert_eq!(read.spectrum, frame.spectrum);
        assert!(read.time_ns > 0);
        let info = reader.info();
        assert_eq!((info.rate, info.channels, info.fft_size, info.bins, info.hop, info.slots), (48_000, 2, 64, 32, 32, 4));
        assert!(reader.is_live());
        // The ring keeps the last `slots` hops and no more.
        for _ in 0..5 {
            writer.publish(&frame);
        }
        assert_eq!(reader.info().seq, 6);
        assert!(reader.slot(6).is_some());
        assert!(reader.slot(3).is_some());
        assert!(reader.slot(2).is_none(), "overwritten");
        assert_eq!(Reader::find(&dir).len(), 1);
        assert!(Reader::open_live(&dir).is_some());
        let path = writer.path().to_path_buf();
        drop(writer);
        assert!(!path.exists(), "the file goes with the writer");
        let _ = fs::remove_dir_all(&dir);
    }
}
