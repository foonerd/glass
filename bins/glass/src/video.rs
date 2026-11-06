//! The picture of a video file the player plays: decoded by a child
//! process behind a pipe (never linked, so a decoder's failure is never
//! the display's), paced to the player's own clock and handed to the pane
//! as the backdrop under the theme, the meters and the face.
//!
//! The sound is the player's as ever: mpd to ALSA to the tap to the DAC.
//! The clock is the sound: mpd's position over its own protocol, read a
//! few times a second and fitted, since it advances in steps; the picture
//! is shown the lead ahead that the meters are shown ahead, for the same
//! reason (the sound goes to the DAC, the picture through the screen's own
//! processing), plus a correction for the sound's own output delay that the
//! sync harness measures. Frames behind the clock are dropped, a frame
//! ahead is held, a jump restarts the decoder at the new place.
//!
//! Nothing here runs unless a video file plays and the picture is wanted:
//! a player that never plays a movie pays nothing. While one plays, the
//! decode is the child's on its own cores; the display's added work per
//! frame is one texture upload and no conversion or copy of its own.

#![cfg(all(unix, not(target_os = "android")))]

use std::collections::VecDeque;
use std::ffi::CString;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::os::fd::FromRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// The decoder and the prober: evovpeg, the static build of our own with
/// the video decoders, once it ships as a component beside the display;
/// until then the image's own ffmpeg, by name on the path.
const DECODER: &str = "ffmpeg";
const PROBER: &str = "ffprobe";
/// mpd on the player.
const MPD: &str = "127.0.0.1:6600";
/// How often the clock reads mpd.
const CLOCK_EVERY: Duration = Duration::from_millis(250);
/// Frames decoded ahead of the clock and held: the pipe's backpressure
/// paces the decoder past this.
const AHEAD_FRAMES: usize = 4;
/// Past this far behind or ahead the decoder is started again at the
/// clock's place rather than dropped or held into step.
const RESTART_S: f64 = 0.75;

/// A child started with its standard output on a pipe we read; the other
/// two standard streams go nowhere.
struct Child {
    pid: libc::pid_t,
    out: File,
}

impl Child {
    fn spawn(program: &str, args: &[&str]) -> Result<Self, String> {
        extern "C" {
            static environ: *const *mut libc::c_char;
        }
        let c = |s: &str| CString::new(s).map_err(|e| e.to_string());
        let program_c = c(program)?;
        let args_c: Vec<CString> = std::iter::once(c(program))
            .chain(args.iter().map(|a| c(a)))
            .collect::<Result<_, _>>()?;
        let mut argv: Vec<*mut libc::c_char> = args_c
            .iter()
            .map(|a| a.as_ptr() as *mut libc::c_char)
            .collect();
        argv.push(std::ptr::null_mut());
        let null = c("/dev/null")?;
        let mut fds = [0 as libc::c_int; 2];
        // SAFETY: a pipe whose ends close on exec; the child's dup of the
        // write end onto its standard output clears the flag on that one.
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
            return Err(format!("pipe: {}", std::io::Error::last_os_error()));
        }
        let (read_end, write_end) = (fds[0], fds[1]);
        let mut pid: libc::pid_t = 0;
        // SAFETY: posix_spawn with file actions built and destroyed here;
        // argv is NUL-terminated and outlives the call.
        let rc = unsafe {
            let mut actions: libc::posix_spawn_file_actions_t = std::mem::zeroed();
            libc::posix_spawn_file_actions_init(&mut actions);
            libc::posix_spawn_file_actions_addopen(
                &mut actions,
                0,
                null.as_ptr(),
                libc::O_RDONLY,
                0,
            );
            libc::posix_spawn_file_actions_adddup2(&mut actions, write_end, 1);
            libc::posix_spawn_file_actions_addopen(
                &mut actions,
                2,
                null.as_ptr(),
                libc::O_WRONLY,
                0,
            );
            let rc = libc::posix_spawnp(
                &mut pid,
                program_c.as_ptr(),
                &actions,
                std::ptr::null(),
                argv.as_ptr(),
                environ,
            );
            libc::posix_spawn_file_actions_destroy(&mut actions);
            libc::close(write_end);
            rc
        };
        if rc != 0 {
            // SAFETY: our own read end, closed once.
            unsafe { libc::close(read_end) };
            return Err(format!(
                "{program}: {}",
                std::io::Error::from_raw_os_error(rc)
            ));
        }
        // SAFETY: the read end is ours alone from here.
        let out = unsafe { File::from_raw_fd(read_end) };
        Ok(Self { pid, out })
    }

    /// Reaped if it has ended; `true` then.
    fn reap(&self) -> bool {
        let mut status = 0;
        // SAFETY: our own child, waited without blocking.
        unsafe { libc::waitpid(self.pid, &mut status, libc::WNOHANG) == self.pid }
    }

    /// Ended: told to leave, given a moment, then killed, and reaped.
    fn end(&self) {
        // SAFETY: our own child.
        unsafe { libc::kill(self.pid, libc::SIGTERM) };
        for _ in 0..20 {
            if self.reap() {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        // SAFETY: our own child, still there.
        unsafe { libc::kill(self.pid, libc::SIGKILL) };
        let mut status = 0;
        // SAFETY: a blocking wait on our own child, which SIGKILL ends.
        unsafe { libc::waitpid(self.pid, &mut status, 0) };
    }
}

/// What a file's video stream is, from the prober: none where the file
/// carries no video.
#[derive(Clone, Debug, PartialEq)]
pub struct Probe {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
}

/// The file's first video stream, or none; the prober's own absence or
/// refusal is an error in words.
pub fn probe(file: &str) -> Result<Option<Probe>, String> {
    let mut child = Child::spawn(
        PROBER,
        &[
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate",
            "-of",
            "csv=p=0",
            file,
        ],
    )?;
    let mut text = String::new();
    let _ = child.out.read_to_string(&mut text);
    child.end();
    Ok(parse_probe(&text))
}

/// `1920,1080,30000/1001` as the prober prints it.
pub fn parse_probe(text: &str) -> Option<Probe> {
    let line = text.lines().find(|l| !l.trim().is_empty())?;
    let mut parts = line.trim().split(',');
    let width: u32 = parts.next()?.trim().parse().ok()?;
    let height: u32 = parts.next()?.trim().parse().ok()?;
    let rate = parts.next()?.trim();
    let fps = match rate.split_once('/') {
        Some((n, d)) => {
            let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
            if d <= 0.0 {
                return None;
            }
            n / d
        }
        None => rate.parse().ok()?,
    };
    if width < 2 || height < 2 || !(1.0..=240.0).contains(&fps) {
        return None;
    }
    Some(Probe { width, height, fps })
}

/// One decoded frame: its time in the file and its planes.
struct Decoded {
    pts: f64,
    planes: Vec<u8>,
}

/// The decoder child and the thread draining its pipe into a short queue;
/// the queue's bound is the backpressure that paces the child.
struct Decoder {
    child: Child,
    frames: Receiver<Decoded>,
    spare: SyncSender<Vec<u8>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    ended: Arc<AtomicBool>,
}

impl Decoder {
    fn start(file: &str, probe: &Probe, from: f64) -> Result<Self, String> {
        let from = from.max(0.0);
        let at = format!("{from:.3}");
        let child = Child::spawn(
            DECODER,
            &[
                "-nostdin",
                "-loglevel",
                "error",
                "-ss",
                &at,
                "-i",
                file,
                "-an",
                "-sn",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "yuv420p",
                "-",
            ],
        )?;
        let frame_len = (probe.width * probe.height) as usize * 3 / 2;
        let (tx, frames) = mpsc::sync_channel::<Decoded>(AHEAD_FRAMES);
        let (spare, spares) = mpsc::sync_channel::<Vec<u8>>(AHEAD_FRAMES + 2);
        let stop = Arc::new(AtomicBool::new(false));
        let ended = Arc::new(AtomicBool::new(false));
        let mut out = child.out.try_clone().map_err(|e| e.to_string())?;
        let fps = probe.fps;
        let thread = {
            let (stop, ended) = (stop.clone(), ended.clone());
            thread::Builder::new()
                .name("video-decode".into())
                .spawn(move || {
                    let mut index = 0u64;
                    while !stop.load(Ordering::Acquire) {
                        let mut planes = spares
                            .try_recv()
                            .unwrap_or_else(|_| Vec::with_capacity(frame_len));
                        planes.resize(frame_len, 0);
                        if out.read_exact(&mut planes).is_err() {
                            break;
                        }
                        let pts = from + index as f64 / fps;
                        index += 1;
                        if tx.send(Decoded { pts, planes }).is_err() {
                            break;
                        }
                    }
                    ended.store(true, Ordering::Release);
                })
                .map_err(|e| e.to_string())?
        };
        Ok(Self {
            child,
            frames,
            spare,
            stop,
            thread: Some(thread),
            ended,
        })
    }

    /// A frame's planes handed back for the next decode.
    fn give_back(&self, planes: Vec<u8>) {
        let _ = self.spare.try_send(planes);
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.child.end();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        self.stop();
    }
}

/// What mpd said of itself at one read.
#[derive(Clone, Debug, Default)]
struct Status {
    state: String,
    elapsed: Option<f64>,
    file: String,
    at: Option<Instant>,
}

/// mpd's position, read over its protocol a few times a second on a
/// thread and fitted: `elapsed` advances in steps of about a third of a
/// second, so the position now is the largest projection of the recent
/// reads, which rides the steps' leading edges.
pub struct Clock {
    reads: Arc<Mutex<VecDeque<Status>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Clock {
    pub fn start() -> Self {
        let reads = Arc::new(Mutex::new(VecDeque::with_capacity(8)));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (reads, stop) = (reads.clone(), stop.clone());
            thread::Builder::new()
                .name("video-clock".into())
                .spawn(move || {
                    let mut link: Option<(BufReader<TcpStream>, TcpStream)> = None;
                    while !stop.load(Ordering::Acquire) {
                        if link.is_none() {
                            link = connect_mpd();
                        }
                        let status = link.as_mut().and_then(|(r, w)| read_status(r, w));
                        match status {
                            Some(status) => {
                                let mut reads = reads.lock().unwrap_or_else(|e| e.into_inner());
                                if reads.len() == 8 {
                                    reads.pop_front();
                                }
                                reads.push_back(status);
                            }
                            None => link = None,
                        }
                        thread::sleep(CLOCK_EVERY);
                    }
                })
                .ok()
        };
        Self {
            reads,
            stop,
            thread,
        }
    }

    /// The position in the file now, while playing; none otherwise or
    /// before any read.
    pub fn position(&self, now: Instant) -> Option<f64> {
        let reads = self.reads.lock().unwrap_or_else(|e| e.into_inner());
        let last = reads.back()?;
        if last.state != "play" {
            return last.elapsed.filter(|_| last.state == "pause");
        }
        reads
            .iter()
            .filter(|r| r.state == "play" && r.file == last.file)
            .filter_map(|r| Some(r.elapsed? + now.duration_since(r.at?).as_secs_f64()))
            .fold(None, |best: Option<f64>, p| {
                Some(best.map_or(p, |b| b.max(p)))
            })
    }
}

impl Drop for Clock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn connect_mpd() -> Option<(BufReader<TcpStream>, TcpStream)> {
    let stream = TcpStream::connect_timeout(&MPD.parse().ok()?, Duration::from_secs(2)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    let writer = stream.try_clone().ok()?;
    let mut reader = BufReader::new(stream);
    let mut hello = String::new();
    reader.read_line(&mut hello).ok()?;
    hello.starts_with("OK MPD").then_some((reader, writer))
}

/// One `status` and `currentsong` exchange; none when the link is gone.
fn read_status(reader: &mut BufReader<TcpStream>, writer: &mut TcpStream) -> Option<Status> {
    writer
        .write_all(b"command_list_begin\nstatus\ncurrentsong\ncommand_list_end\n")
        .ok()?;
    let at = Instant::now();
    let mut status = Status {
        at: Some(at),
        ..Default::default()
    };
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let l = line.trim_end();
        if l == "OK" {
            break;
        }
        if l.starts_with("ACK") {
            return None;
        }
        if let Some((key, value)) = l.split_once(": ") {
            match key {
                "state" => status.state = value.to_string(),
                "elapsed" => status.elapsed = value.parse().ok(),
                "file" => status.file = value.to_string(),
                _ => {}
            }
        }
    }
    Some(status)
}

/// The picture of the file the player plays: the decoder kept in step with
/// the clock, the frame to show now, and the harness's log.
pub struct Video {
    file: String,
    probe: Probe,
    decoder: Option<Decoder>,
    clock: Clock,
    /// The frame shown, kept until the next.
    shown: Option<Decoded>,
    /// Frames taken from the decoder and not yet due.
    waiting: VecDeque<Decoded>,
    /// Where the picture stands against mpd's position, in seconds: the
    /// screen's lead ahead, less the sound output's delay behind (mpd's
    /// position runs ahead of the sound at the DAC by its buffers, about
    /// half a second on a player; the sync harness measures it).
    ahead: f64,
    /// The sync harness: a line per frame when `GLASS_SYNC_LOG` names a file.
    log: Option<File>,
    started: Instant,
}

impl Video {
    /// For `file`, which the prober says carries video; `lead_ms` as the
    /// meters' lead, `delay_ms` the sound output's delay the harness found.
    pub fn open(file: &str, probe: Probe, lead_ms: u32, delay_ms: u32) -> Self {
        let log = std::env::var_os("GLASS_SYNC_LOG").and_then(|p| File::create(p).ok());
        Self {
            file: file.to_string(),
            probe,
            decoder: None,
            clock: Clock::start(),
            shown: None,
            waiting: VecDeque::new(),
            ahead: (lead_ms as f64 - delay_ms as f64) / 1000.0,
            log,
            started: Instant::now(),
        }
    }

    pub fn file(&self) -> &str {
        &self.file
    }

    pub fn size(&self) -> (u32, u32) {
        (self.probe.width, self.probe.height)
    }

    fn restart(&mut self, at: f64) {
        self.waiting.clear();
        self.decoder = None;
        match Decoder::start(&self.file, &self.probe, at) {
            Ok(decoder) => self.decoder = Some(decoder),
            Err(e) => logline::say!(Warn, "video", "the decoder did not start: {e}"),
        }
    }

    /// The planes of the frame shown, while one is.
    pub fn shown(&self) -> Option<&[u8]> {
        self.shown.as_ref().map(|f| f.planes.as_slice())
    }

    /// Once a frame: whether a new frame is due now and is the one shown
    /// (`shown` has its planes); `false` while the one shown stands.
    /// `level` is the sound's measured level this frame, for the harness
    /// alone.
    pub fn tick(&mut self, now: Instant, level: f32) -> bool {
        let Some(position) = self.clock.position(now) else {
            // Stopped, or the clock silent: the picture is let go.
            self.decoder = None;
            self.waiting.clear();
            self.shown = None;
            return false;
        };
        let target = position + self.ahead;
        if self.decoder.is_none() {
            self.restart(target);
        }
        // Frames from the decoder into the waiting line.
        if let Some(decoder) = &self.decoder {
            while self.waiting.len() < AHEAD_FRAMES {
                match decoder.frames.try_recv() {
                    Ok(frame) => self.waiting.push_back(frame),
                    Err(_) => break,
                }
            }
            if decoder.ended.load(Ordering::Acquire) && self.waiting.is_empty() {
                // The file's end, or the child gone: nothing more to show.
                self.decoder = None;
            }
        }
        // A jump: the clock far from what the decoder gives.
        let edge = self
            .waiting
            .front()
            .map(|f| f.pts)
            .or(self.shown.as_ref().map(|f| f.pts));
        if let Some(edge) = edge {
            if target - edge > RESTART_S || edge - target > RESTART_S {
                self.restart(target);
                return false;
            }
        }
        // Due frames: the last of them is shown, the earlier dropped.
        let mut due: Option<Decoded> = None;
        while self.waiting.front().is_some_and(|f| f.pts <= target) {
            if let Some(dropped) = due.take() {
                if let Some(decoder) = &self.decoder {
                    decoder.give_back(dropped.planes);
                }
            }
            due = self.waiting.pop_front();
        }
        let new = due.is_some();
        if let Some(frame) = due {
            if let Some(old) = self.shown.take() {
                if let Some(decoder) = &self.decoder {
                    decoder.give_back(old.planes);
                }
            }
            self.shown = Some(frame);
        }
        if let (Some(log), Some(shown)) = (self.log.as_mut(), self.shown.as_ref()) {
            let luma = luma_of(&shown.planes, self.probe.width, self.probe.height);
            let _ = writeln!(
                log,
                "{} {:.3} {:.3} {} {:.3}",
                self.started.elapsed().as_millis(),
                position,
                shown.pts,
                luma,
                level
            );
        }
        new && self.shown.is_some()
    }
}

/// The mean of the Y plane over a coarse grid: the harness's flash detector.
fn luma_of(planes: &[u8], width: u32, height: u32) -> u8 {
    let (w, h) = (width as usize, height as usize);
    let mut sum = 0u64;
    let mut n = 0u64;
    let mut y = 0;
    while y < h {
        let row = &planes[y * w..(y + 1) * w];
        let mut x = 0;
        while x < w {
            sum += row[x] as u64;
            n += 1;
            x += 16;
        }
        y += 16;
    }
    (sum / n.max(1)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probers_line_is_read() {
        assert_eq!(
            parse_probe("1920,1080,30000/1001\n"),
            Some(Probe {
                width: 1920,
                height: 1080,
                fps: 30000.0 / 1001.0
            })
        );
        assert_eq!(parse_probe("1280,720,25\n").map(|p| p.fps), Some(25.0));
        assert_eq!(parse_probe(""), None, "no video stream");
        assert_eq!(parse_probe("1920,1080,0/0\n"), None, "a rate of nothing");
    }

    #[test]
    fn the_clock_rides_the_steps_leading_edges() {
        let clock = Clock {
            reads: Arc::new(Mutex::new(VecDeque::new())),
            stop: Arc::new(AtomicBool::new(true)),
            thread: None,
        };
        let t0 = Instant::now();
        let read = |elapsed: f64, ago_ms: u64| Status {
            state: "play".into(),
            elapsed: Some(elapsed),
            file: "a.mp4".into(),
            at: Some(t0 - Duration::from_millis(ago_ms)),
        };
        {
            let mut reads = clock.reads.lock().unwrap();
            // Three reads a quarter second apart, the middle one just after
            // a step: its projection leads.
            reads.push_back(read(10.000, 500));
            reads.push_back(read(10.340, 250));
            reads.push_back(read(10.340, 0));
        }
        let p = clock.position(t0).unwrap();
        assert!((p - 10.590).abs() < 0.001, "{p}");
        clock.reads.lock().unwrap().push_back(Status {
            state: "pause".into(),
            elapsed: Some(11.0),
            file: "a.mp4".into(),
            at: Some(t0),
        });
        assert_eq!(
            clock.position(t0 + Duration::from_secs(5)),
            Some(11.0),
            "paused: held"
        );
        clock.reads.lock().unwrap().push_back(Status {
            state: "stop".into(),
            ..Default::default()
        });
        assert_eq!(clock.position(t0), None);
    }

    #[test]
    fn the_flash_detector_reads_a_white_frame() {
        let (w, h) = (64u32, 32u32);
        let mut planes = vec![16u8; (w * h) as usize * 3 / 2];
        assert_eq!(luma_of(&planes, w, h), 16);
        planes[..(w * h) as usize].fill(235);
        assert_eq!(luma_of(&planes, w, h), 235);
    }
}
