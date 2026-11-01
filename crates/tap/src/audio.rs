//! The audio ring: a second ring beside the measurements, carrying the
//! stream's own bytes for any process that wants the audio (an encoder,
//! a recorder, a second output). A file under `/dev/shm`, one per writer
//! and per shape of stream: a new shape makes a new file and the old one
//! goes, so a reader that finds its file gone has reached the end of a
//! segment and looks for the next. Readers are many and anonymous, each
//! with a cursor of its own; the writer, the audio thread, never knows
//! of them and never waits: it copies and moves the head.
//!
//! Layout, little-endian:
//!
//! - header, 256 bytes: magic `EVOAUDIO`, version, header size, sample
//!   rate, channels, the ALSA format number, bytes per frame, capacity (a
//!   power of two), writer pid, flags (bit 0: one-bit audio), then,
//!   written last with release ordering, the head (bytes written since
//!   the ring was made), the time of the last write, bytes dropped and
//!   the time the ring was made.
//! - data: `capacity` bytes, the frames interleaved as ALSA delivers
//!   them, at the head modulo the capacity.
//!
//! A reader loads the head, copies from its cursor to the head, loads the
//! head again: if the writer has moved more than a capacity past the
//! cursor meanwhile, the copy may hold bytes written over and is
//! discarded, the cursor set to the head and the gap counted.

#[cfg(unix)]
use std::ffi::CString;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::io;
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[cfg(unix)]
use crate::ring::now_ns;

pub const MAGIC: &[u8; 8] = b"EVOAUDIO";
pub const VERSION: u32 = 1;
pub const PREFIX: &str = "evoaudio.";
pub const HEADER_BYTES: usize = 256;
/// The least a ring holds, whatever the seconds asked.
pub const MIN_CAPACITY: usize = 1 << 16;
/// The most seconds a ring holds.
pub const MAX_SECONDS: u32 = 10;
/// A ring written longer ago than this, by a process still alive, is a
/// stream that stands still: paused, or the player waiting. It is live.
/// A ring whose writer is gone is a leftover; the sweep removes it.
pub const FLAG_ONE_BIT: u32 = 1;

/// The shape of the stream a ring carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shape {
    pub rate: u32,
    pub channels: u32,
    /// ALSA's `snd_pcm_format_t` number.
    pub format: i32,
    /// Bytes of one frame, every channel.
    pub frame_bytes: u32,
    /// One-bit audio (DSD): the bytes are bit streams, not samples.
    pub one_bit: bool,
}

/// What the header says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Info {
    pub shape: Shape,
    pub capacity: u64,
    pub pid: u32,
    pub head: u64,
    pub written_ns: u64,
    pub dropped: u64,
    pub started_ns: u64,
    pub version: u32,
}

#[cfg(unix)]
const H_MAGIC: usize = 0;
#[cfg(unix)]
const H_VERSION: usize = 8;
#[cfg(unix)]
const H_HEADER_BYTES: usize = 12;
#[cfg(unix)]
const H_RATE: usize = 16;
#[cfg(unix)]
const H_CHANNELS: usize = 20;
#[cfg(unix)]
const H_FORMAT: usize = 24;
#[cfg(unix)]
const H_FRAME_BYTES: usize = 28;
#[cfg(unix)]
const H_CAPACITY: usize = 32;
#[cfg(unix)]
const H_PID: usize = 40;
#[cfg(unix)]
const H_FLAGS: usize = 44;
#[cfg(unix)]
const H_HEAD: usize = 48;
#[cfg(unix)]
const H_WRITTEN: usize = 56;
#[cfg(unix)]
const H_DROPPED: usize = 64;
#[cfg(unix)]
const H_STARTED: usize = 72;

/// Writers made by this process so far, for their file names.
#[cfg(unix)]
static INSTANCE: AtomicU32 = AtomicU32::new(0);

/// The bytes a ring of `seconds` of this shape holds: a power of two, at
/// least [`MIN_CAPACITY`].
pub fn capacity_for(shape: &Shape, seconds: u32) -> usize {
    let seconds = seconds.clamp(1, MAX_SECONDS) as usize;
    let wanted = shape.rate as usize * shape.frame_bytes.max(1) as usize * seconds;
    wanted.max(MIN_CAPACITY).next_power_of_two()
}

/// Remove the audio rings under `dir` whose writer is gone.
#[cfg(unix)]
pub fn sweep(dir: &Path) -> usize {
    crate::ring::sweep_prefix(dir, PREFIX)
}

/// A mapped file, unmapped when dropped.
#[cfg(unix)]
struct Map {
    ptr: *mut u8,
    len: usize,
}

#[cfg(unix)]
impl Map {
    fn open(fd: i32, len: usize, write: bool) -> io::Result<Self> {
        let prot = if write {
            libc::PROT_READ | libc::PROT_WRITE
        } else {
            libc::PROT_READ
        };
        // A shared mapping of `len` bytes of an open file.
        let ptr = unsafe { libc::mmap(std::ptr::null_mut(), len, prot, libc::MAP_SHARED, fd, 0) };
        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            ptr: ptr as *mut u8,
            len,
        })
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

#[cfg(unix)]
impl Drop for Map {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.ptr as *mut libc::c_void, self.len) };
    }
}

// The ring is shared between processes by design; the mapping itself is
// owned by one writer or one reader at a time.
#[cfg(unix)]
unsafe impl Send for Map {}

#[cfg(unix)]
fn put_u32(bytes: &mut [u8], at: usize, v: u32) {
    bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
#[cfg(unix)]
fn get_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}
#[cfg(unix)]
fn put_u64(bytes: &mut [u8], at: usize, v: u64) {
    bytes[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

/// The writing side: the audio thread's. Creating one creates the file;
/// dropping it removes the file.
#[cfg(unix)]
pub struct Writer {
    map: Map,
    path: PathBuf,
    capacity: usize,
    head: u64,
    dropped: u64,
}

#[cfg(unix)]
impl Writer {
    /// Create the ring for a stream of `shape`, holding `seconds` of it.
    /// Rings of writers that are gone are swept first. Every page is
    /// touched here, so the audio thread's copies fault nothing in.
    pub fn create(dir: &Path, tag: &str, shape: Shape, seconds: u32) -> io::Result<Self> {
        sweep(dir);
        let capacity = capacity_for(&shape, seconds);
        let len = HEADER_BYTES + capacity;
        let instance = INSTANCE.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!("{PREFIX}{tag}.{}.{instance}", std::process::id()));
        let cpath = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        let fd = unsafe {
            libc::open(
                cpath.as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_CLOEXEC,
                0o644,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let sized = unsafe { libc::ftruncate(fd, len as libc::off_t) };
        if sized != 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            let _ = fs::remove_file(&path);
            return Err(err);
        }
        let map = Map::open(fd, len, true);
        unsafe { libc::close(fd) };
        let mut map = match map {
            Ok(m) => m,
            Err(e) => {
                let _ = fs::remove_file(&path);
                return Err(e);
            }
        };
        {
            let bytes = map.bytes_mut();
            bytes[..len].fill(0);
            bytes[H_MAGIC..H_MAGIC + 8].copy_from_slice(MAGIC);
            put_u32(bytes, H_VERSION, VERSION);
            put_u32(bytes, H_HEADER_BYTES, HEADER_BYTES as u32);
            put_u32(bytes, H_RATE, shape.rate);
            put_u32(bytes, H_CHANNELS, shape.channels);
            put_u32(bytes, H_FORMAT, shape.format as u32);
            put_u32(bytes, H_FRAME_BYTES, shape.frame_bytes);
            put_u64(bytes, H_CAPACITY, capacity as u64);
            put_u32(bytes, H_PID, std::process::id());
            put_u32(bytes, H_FLAGS, if shape.one_bit { FLAG_ONE_BIT } else { 0 });
            put_u64(bytes, H_STARTED, now_ns());
        }
        Ok(Self {
            map,
            path,
            capacity,
            head: 0,
            dropped: 0,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Bytes written so far.
    pub fn head(&self) -> u64 {
        self.head
    }

    /// Bytes dropped so far: those of a transfer longer than the ring.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Append one transfer's bytes: a copy, two at a wrap, then the head.
    /// Never waits, never allocates. A transfer longer than the ring
    /// leaves its last `capacity` bytes and counts the rest as dropped.
    pub fn push(&mut self, bytes: &[u8]) {
        let capacity = self.capacity;
        let (bytes, lost) = if bytes.len() > capacity {
            (
                &bytes[bytes.len() - capacity..],
                (bytes.len() - capacity) as u64,
            )
        } else {
            (bytes, 0)
        };
        if lost > 0 {
            self.dropped += lost;
            self.head += lost;
        }
        let at = (self.head as usize) & (capacity - 1);
        let first = bytes.len().min(capacity - at);
        {
            let data = &mut self.map.bytes_mut()[HEADER_BYTES..];
            data[at..at + first].copy_from_slice(&bytes[..first]);
            if first < bytes.len() {
                data[..bytes.len() - first].copy_from_slice(&bytes[first..]);
            }
        }
        self.head += bytes.len() as u64;
        if lost > 0 {
            self.map
                .atomic_u64(H_DROPPED)
                .store(self.dropped, Ordering::Relaxed);
        }
        self.map
            .atomic_u64(H_WRITTEN)
            .store(now_ns(), Ordering::Relaxed);
        self.map
            .atomic_u64(H_HEAD)
            .store(self.head, Ordering::Release);
    }
}

#[cfg(unix)]
impl Drop for Writer {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// What one read brought: bytes appended to the caller's buffer, and
/// bytes lost before them because the reader fell behind the writer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Took {
    pub bytes: usize,
    pub lost: u64,
}

/// The reading side of one ring, with a cursor of its own that starts at
/// the head: a reader takes the stream from the moment it opens.
#[cfg(unix)]
pub struct Reader {
    map: Map,
    path: PathBuf,
    info: Info,
    cursor: u64,
    lost: u64,
}

#[cfg(unix)]
impl Reader {
    /// Open a ring by path, the cursor at its head.
    pub fn open(path: &Path) -> io::Result<Self> {
        let meta = fs::metadata(path)?;
        let len = meta.len() as usize;
        if len < HEADER_BYTES {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "ring too short"));
        }
        let cpath = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        let fd = unsafe { libc::open(cpath.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let map = Map::open(fd, len, false);
        unsafe { libc::close(fd) };
        let map = map?;
        let bytes = map.bytes();
        let version = get_u32(bytes, H_VERSION);
        if &bytes[H_MAGIC..H_MAGIC + 8] != MAGIC || version != VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not a glass audio ring this build reads",
            ));
        }
        let capacity = u64::from_le_bytes(bytes[H_CAPACITY..H_CAPACITY + 8].try_into().unwrap());
        let started = u64::from_le_bytes(bytes[H_STARTED..H_STARTED + 8].try_into().unwrap());
        if capacity == 0
            || !capacity.is_power_of_two()
            || len < HEADER_BYTES + capacity as usize
            || get_u32(bytes, H_HEADER_BYTES) as usize != HEADER_BYTES
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ring header disagrees with its size",
            ));
        }
        let info = Info {
            shape: Shape {
                rate: get_u32(bytes, H_RATE),
                channels: get_u32(bytes, H_CHANNELS),
                format: get_u32(bytes, H_FORMAT) as i32,
                frame_bytes: get_u32(bytes, H_FRAME_BYTES),
                one_bit: get_u32(bytes, H_FLAGS) & FLAG_ONE_BIT != 0,
            },
            capacity,
            pid: get_u32(bytes, H_PID),
            head: 0,
            written_ns: 0,
            dropped: 0,
            started_ns: started,
            version,
        };
        let cursor = map.atomic_u64(H_HEAD).load(Ordering::Acquire);
        Ok(Self {
            map,
            path: path.to_path_buf(),
            info,
            cursor,
            lost: 0,
        })
    }

    /// Every audio ring under `dir`, of `tag` when one is given, the most
    /// recently made first.
    pub fn find(dir: &Path, tag: Option<&str>) -> Vec<PathBuf> {
        let mut found: Vec<(u64, PathBuf)> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                match name.strip_prefix(PREFIX) {
                    Some(rest) => tag.is_none_or(|t| rest.starts_with(&format!("{t}."))),
                    None => false,
                }
            })
            .filter_map(|entry| {
                let path = entry.path();
                let reader = Reader::open(&path).ok()?;
                Some((reader.info().started_ns, path))
            })
            .collect();
        found.sort_by_key(|a| std::cmp::Reverse(a.0));
        found.into_iter().map(|(_, p)| p).collect()
    }

    /// The ring being written by a live process, if any: the newest of
    /// `tag`, or of any tag.
    pub fn open_live(dir: &Path, tag: Option<&str>) -> Option<Self> {
        Self::find(dir, tag)
            .into_iter()
            .find_map(|path| Reader::open(&path).ok().filter(Reader::is_live))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The header, with the head, the last write and the drops as they stand.
    pub fn info(&self) -> Info {
        Info {
            head: self.map.atomic_u64(H_HEAD).load(Ordering::Acquire),
            written_ns: self.map.atomic_u64(H_WRITTEN).load(Ordering::Relaxed),
            dropped: self.map.atomic_u64(H_DROPPED).load(Ordering::Relaxed),
            ..self.info
        }
    }

    pub fn shape(&self) -> Shape {
        self.info.shape
    }

    /// Bytes lost to this reader so far: skipped when it fell behind.
    pub fn lost(&self) -> u64 {
        self.lost
    }

    /// The writer's process is alive and its file still there: a stream
    /// that stands still is live too.
    pub fn is_live(&self) -> bool {
        let pid = self.info.pid as i32;
        // A process of another user answers with EPERM: alive.
        let alive = pid > 0
            && (unsafe { libc::kill(pid, 0) } == 0
                || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM));
        alive && self.path.exists()
    }

    /// Bytes waiting between the cursor and the head.
    pub fn available(&self) -> u64 {
        self.map
            .atomic_u64(H_HEAD)
            .load(Ordering::Acquire)
            .saturating_sub(self.cursor)
    }

    /// Move the cursor to the head: what came before is not wanted.
    pub fn skip_to_live(&mut self) {
        self.cursor = self.map.atomic_u64(H_HEAD).load(Ordering::Acquire);
    }

    /// Append what the writer has written since the cursor to `out`, at
    /// most `most` bytes, whole frames only. A reader that fell behind by
    /// more than the ring holds takes nothing of what it lost: the copy
    /// is discarded, the cursor moves to the head, and the loss is
    /// counted and returned.
    pub fn take(&mut self, out: &mut Vec<u8>, most: usize) -> Took {
        let capacity = self.info.capacity;
        let frame = self.info.shape.frame_bytes.max(1) as u64;
        let head = self.map.atomic_u64(H_HEAD).load(Ordering::Acquire);
        if head < self.cursor {
            // A head behind the cursor is another ring's: start afresh.
            self.cursor = head;
            return Took::default();
        }
        if head - self.cursor > capacity {
            let lost = head - self.cursor;
            self.cursor = head;
            self.lost += lost;
            return Took { bytes: 0, lost };
        }
        let want = ((head - self.cursor).min(most as u64) / frame * frame) as usize;
        if want == 0 {
            return Took::default();
        }
        let at = (self.cursor & (capacity - 1)) as usize;
        let first = want.min(capacity as usize - at);
        let before = out.len();
        {
            let data = &self.map.bytes()[HEADER_BYTES..];
            out.extend_from_slice(&data[at..at + first]);
            if first < want {
                out.extend_from_slice(&data[..want - first]);
            }
        }
        // The writer may have gone round meanwhile and written over what
        // was copied: then the copy is not the stream, and goes.
        let head_now = self.map.atomic_u64(H_HEAD).load(Ordering::Acquire);
        if head_now - self.cursor > capacity {
            out.truncate(before);
            let lost = head_now - self.cursor;
            self.cursor = head_now;
            self.lost += lost;
            return Took { bytes: 0, lost };
        }
        self.cursor += want as u64;
        Took {
            bytes: want,
            lost: 0,
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("glass-audio-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn shape() -> Shape {
        Shape {
            rate: 48_000,
            channels: 2,
            format: 10,
            frame_bytes: 8,
            one_bit: false,
        }
    }

    #[test]
    fn the_capacity_is_a_power_of_two_of_at_least_the_seconds_asked() {
        assert_eq!(
            capacity_for(&shape(), 2),
            1 << 20,
            "768 000 rounds up to a megabyte"
        );
        assert_eq!(capacity_for(&shape(), 0), capacity_for(&shape(), 1));
        assert_eq!(
            capacity_for(&shape(), 99),
            capacity_for(&shape(), MAX_SECONDS)
        );
        let small = Shape {
            rate: 8_000,
            frame_bytes: 2,
            ..shape()
        };
        assert_eq!(capacity_for(&small, 1), MIN_CAPACITY);
    }

    #[test]
    fn the_bytes_pushed_are_the_bytes_taken_across_the_wrap() {
        let d = dir("exact");
        let tiny = Shape {
            rate: 1_000,
            frame_bytes: 4,
            ..shape()
        };
        let mut w = Writer::create(&d, "t", tiny, 1).unwrap();
        assert_eq!(w.capacity(), MIN_CAPACITY);
        let mut r = Reader::open(w.path()).unwrap();
        assert_eq!(r.shape(), tiny);
        assert_eq!(r.info().pid, std::process::id());
        assert!(r.is_live());
        assert_eq!(r.available(), 0);
        // Many pushes of odd sizes, all whole frames, read back in order.
        let mut sent = Vec::new();
        let mut got = Vec::new();
        let mut k = 0u32;
        for round in 0..300usize {
            let frames = 1 + (round * 37) % 700;
            let chunk: Vec<u8> = (0..frames * 4)
                .map(|_| {
                    k = k.wrapping_mul(1664525).wrapping_add(1013904223);
                    (k >> 24) as u8
                })
                .collect();
            w.push(&chunk);
            sent.extend_from_slice(&chunk);
            if round % 3 == 0 {
                let took = r.take(&mut got, 1 << 20);
                assert_eq!(took.lost, 0);
            }
        }
        let took = r.take(&mut got, 1 << 20);
        assert_eq!(took.lost, 0);
        assert_eq!(got, sent, "every byte, in order, through several wraps");
        assert_eq!(w.head(), sent.len() as u64);
        assert_eq!(r.info().head, w.head());
        assert_eq!(r.available(), 0);
        // `most` bounds a take to whole frames.
        w.push(&[7u8; 40]);
        let before = got.len();
        assert_eq!(r.take(&mut got, 10).bytes, 8);
        assert_eq!(got.len() - before, 8);
        assert_eq!(r.take(&mut got, 1000).bytes, 32);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_reader_that_falls_behind_takes_nothing_of_what_it_lost_and_counts_it() {
        let d = dir("behind");
        let tiny = Shape {
            rate: 1_000,
            frame_bytes: 4,
            ..shape()
        };
        let mut w = Writer::create(&d, "t", tiny, 1).unwrap();
        let cap = w.capacity();
        let mut r = Reader::open(w.path()).unwrap();
        w.push(&[1u8; 100]);
        // More than a ring's worth goes by before the reader looks.
        w.push(&vec![2u8; cap]);
        let mut got = Vec::new();
        let took = r.take(&mut got, 1 << 24);
        assert_eq!(took.bytes, 0);
        assert_eq!(took.lost, 100 + cap as u64);
        assert_eq!(r.lost(), 100 + cap as u64);
        assert!(got.is_empty());
        // From the head on, the stream is whole again.
        w.push(&[3u8; 16]);
        assert_eq!(r.take(&mut got, 1 << 24).bytes, 16);
        assert!(got.iter().all(|b| *b == 3));
        // A transfer longer than the ring keeps its tail and counts the rest.
        let long: Vec<u8> = (0..(cap + 64)).map(|i| (i % 251) as u8).collect();
        w.push(&long);
        assert_eq!(w.dropped(), 64);
        assert_eq!(r.info().dropped, 64);
        got.clear();
        let took = r.take(&mut got, 1 << 24);
        assert_eq!(took.lost, cap as u64 + 64, "behind by the whole transfer");
        assert_eq!(took.bytes, 0);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_new_shape_is_a_new_file_found_by_its_tag_and_the_old_one_goes() {
        let d = dir("shape");
        let w1 = Writer::create(&d, "pub", shape(), 1).unwrap();
        let p1 = w1.path().to_path_buf();
        assert_eq!(Reader::find(&d, Some("pub")).len(), 1);
        assert!(Reader::find(&d, Some("other")).is_empty());
        assert!(Reader::open_live(&d, Some("pub")).is_some());
        assert!(Reader::open_live(&d, None).is_some());
        std::thread::sleep(std::time::Duration::from_millis(2));
        let w2 = Writer::create(
            &d,
            "pub",
            Shape {
                rate: 96_000,
                ..shape()
            },
            1,
        )
        .unwrap();
        drop(w1);
        assert!(!p1.exists(), "the old ring goes with its writer");
        let live = Reader::open_live(&d, Some("pub")).expect("the new ring");
        assert_eq!(live.shape().rate, 96_000);
        assert_eq!(live.path(), w2.path());
        // A one-bit shape is flagged.
        let dsd = Writer::create(
            &d,
            "dsd",
            Shape {
                format: 50,
                one_bit: true,
                ..shape()
            },
            1,
        )
        .unwrap();
        assert!(Reader::open(dsd.path()).unwrap().shape().one_bit);
        // The sweep removes a ring whose writer is gone, keeps a live one.
        let dead = d.join(format!("{PREFIX}pub.{}.0", i32::MAX));
        fs::write(&dead, b"x").unwrap();
        assert_eq!(sweep(&d), 1);
        assert!(!dead.exists() && w2.path().exists());
        let _ = fs::remove_dir_all(&d);
    }
}
