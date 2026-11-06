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
/// The clock moving by more than its own time plus this, or back at all,
/// between two frames is a jump (a seek): the decoder is started again
/// where the clock will be rather than caught up by dropping, which the
/// decoder's own pace (several times real time) does for anything less. A
/// resume after a pause moves mpd's position by about a second, which is
/// caught up, not started again.
const JUMP_S: f64 = 2.0;
/// The decoder's first frame after a start this far behind the clock, when
/// the start was placed by an estimate of the decoder's own start-up that
/// proved far too short (a long group of pictures to decode through), has
/// the decoder started once more with the start-up now learnt.
const RESTART_BEHIND_S: f64 = 3.0;
/// The decoder's start-up as first assumed, in seconds: from its spawn to
/// its first frame, the seek to the key frame before the place asked and
/// the decode from there to it. Learnt from each start afterwards.
const STARTUP_S: f64 = 1.0;
/// Over the start-up learnt, so the first frame lands a little early and
/// is held rather than late and dropped.
const STARTUP_MARGIN_S: f64 = 0.25;

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
    /// Taken before the thread is joined: a thread blocked handing a frame
    /// into the full queue leaves the moment its receiver is gone.
    frames: Option<Receiver<Decoded>>,
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
            frames: Some(frames),
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

    /// The next frame decoded, if one waits.
    fn next(&self) -> Option<Decoded> {
        self.frames.as_ref()?.try_recv().ok()
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.frames = None;
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

/// The movie's own loudness, decoded ahead of the picture by a second
/// child (the sound track alone, mono at 8 kHz) into bins of ten
/// milliseconds on the file's clock: what the auto-calibration correlates
/// with the loudness the tap measures, to find the sound output's delay.
struct Envelope {
    child: Child,
    bins: Arc<Mutex<VecDeque<(f64, f32)>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// Bins kept: thirty seconds.
const ENVELOPE_BINS: usize = 3000;
/// One bin: ten milliseconds of sound at 8 kHz.
const ENVELOPE_BIN: usize = 80;

impl Envelope {
    fn start(file: &str, from: f64) -> Result<Self, String> {
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
                "-vn",
                "-sn",
                "-ac",
                "1",
                "-ar",
                "8000",
                "-f",
                "f32le",
                "-",
            ],
        )?;
        let bins = Arc::new(Mutex::new(VecDeque::with_capacity(ENVELOPE_BINS)));
        let stop = Arc::new(AtomicBool::new(false));
        let mut out = child.out.try_clone().map_err(|e| e.to_string())?;
        let thread = {
            let (bins, stop) = (bins.clone(), stop.clone());
            thread::Builder::new()
                .name("video-envelope".into())
                .spawn(move || {
                    let mut raw = [0u8; ENVELOPE_BIN * 4];
                    let mut index = 0u64;
                    while !stop.load(Ordering::Acquire) {
                        // The ring full: the child waits on the pipe meanwhile.
                        if bins.lock().unwrap_or_else(|e| e.into_inner()).len() >= ENVELOPE_BINS {
                            thread::sleep(Duration::from_millis(50));
                            continue;
                        }
                        if out.read_exact(&mut raw).is_err() {
                            break;
                        }
                        let mut sum = 0f32;
                        for chunk in raw.as_chunks::<4>().0 {
                            let v = f32::from_le_bytes(*chunk);
                            sum += v * v;
                        }
                        let rms = (sum / ENVELOPE_BIN as f32).sqrt();
                        let t = from + index as f64 * ENVELOPE_BIN as f64 / 8000.0;
                        index += 1;
                        bins.lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .push_back((t, rms));
                    }
                })
                .map_err(|e| e.to_string())?
        };
        Ok(Self {
            child,
            bins,
            stop,
            thread: Some(thread),
        })
    }

    /// The envelope between two times in the file, as bins of ten
    /// milliseconds, where the ring holds it; the oldest bins are let go
    /// once a window has passed.
    fn window(&self, from: f64, to: f64) -> Vec<(f64, f32)> {
        let mut bins = self.bins.lock().unwrap_or_else(|e| e.into_inner());
        while bins.len() > ENVELOPE_BINS / 2 && bins.front().is_some_and(|b| b.0 < from - 2.0) {
            bins.pop_front();
        }
        bins.iter()
            .filter(|b| b.0 >= from && b.0 <= to)
            .copied()
            .collect()
    }
}

impl Drop for Envelope {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.child.end();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The sound output's delay found by correlation: the loudness the tap
/// measured, placed by mpd's position at each measure, against the file's
/// own loudness at every lag from two hundred milliseconds to a second,
/// the band mpd's own buffering spans (what is measured is the way from
/// mpd's position to the tap, before the output; a film's own rhythm
/// offers peaks further out that mean nothing); the lag whose correlation
/// stands highest, when it stands clear of the rest and high enough to
/// mean anything. What is correlated is each
/// side's onsets, the rises of loudness, since a rise is where a sound
/// begins on both sides alike, while the tap's meter falls slowly and
/// would drag the lag late if its whole shape were matched.
/// The lags tried, in bins of ten milliseconds.
const LAG_LEAST: i64 = 20;
const LAG_MOST: i64 = 100;

pub fn correlate_delay(heard: &[(f64, f32)], file: &[(f64, f32)]) -> Option<(u32, f32)> {
    if heard.len() < 300 || file.len() < 300 {
        return None;
    }
    // Both as ten-millisecond bins on the file's clock, smoothed over fifty.
    let bin = |t: f64| (t * 100.0).round() as i64;
    let mut heard_bins: std::collections::BTreeMap<i64, (f32, u32)> = Default::default();
    for &(t, v) in heard {
        let e = heard_bins.entry(bin(t)).or_insert((0.0, 0));
        e.0 += v;
        e.1 += 1;
    }
    let first = *heard_bins.keys().next()?;
    let last = *heard_bins.keys().next_back()?;
    let span = (last - first + 1) as usize;
    if span < 300 {
        return None;
    }
    let mut h = vec![f32::NAN; span];
    for (k, (sum, n)) in &heard_bins {
        h[(k - first) as usize] = sum / *n as f32;
    }
    // Gaps between frames filled from the neighbour before.
    for i in 1..span {
        if h[i].is_nan() {
            h[i] = h[i - 1];
        }
    }
    if h[0].is_nan() {
        return None;
    }
    let file_first = bin(file[0].0);
    let mut f = vec![0f32; (bin(file[file.len() - 1].0) - file_first + 1) as usize];
    for &(t, v) in file {
        let i = (bin(t) - file_first) as usize;
        if i < f.len() {
            f[i] = v;
        }
    }
    let smooth = |x: &[f32]| -> Vec<f32> {
        let w = 5usize;
        (0..x.len())
            .map(|i| {
                let lo = i.saturating_sub(w / 2);
                let hi = (i + w / 2 + 1).min(x.len());
                x[lo..hi].iter().sum::<f32>() / (hi - lo) as f32
            })
            .collect()
    };
    let onsets = |x: &[f32]| -> Vec<f32> {
        let mut out = vec![0f32; x.len()];
        for i in 1..x.len() {
            out[i] = (x[i] - x[i - 1]).max(0.0);
        }
        out
    };
    let h = onsets(&smooth(&h));
    let f = onsets(&smooth(&f));
    let pearson = |lag: i64| -> Option<f32> {
        let mut xs = Vec::with_capacity(span);
        let mut ys = Vec::with_capacity(span);
        for (i, hv) in h.iter().enumerate() {
            let fi = first + i as i64 - lag - file_first;
            if fi < 0 || fi as usize >= f.len() {
                continue;
            }
            xs.push(*hv);
            ys.push(f[fi as usize]);
        }
        if xs.len() < 200 {
            return None;
        }
        let n = xs.len() as f32;
        let (mx, my) = (xs.iter().sum::<f32>() / n, ys.iter().sum::<f32>() / n);
        let (mut sxy, mut sxx, mut syy) = (0f32, 0f32, 0f32);
        for (x, y) in xs.iter().zip(ys.iter()) {
            let (dx, dy) = (x - mx, y - my);
            sxy += dx * dy;
            sxx += dx * dx;
            syy += dy * dy;
        }
        if sxx <= 0.0 || syy <= 0.0 {
            return None;
        }
        Some(sxy / (sxx * syy).sqrt())
    };
    let mut scores: Vec<(i64, f32)> = (LAG_LEAST..=LAG_MOST)
        .filter_map(|lag| pearson(lag).map(|c| (lag, c)))
        .collect();
    if scores.is_empty() {
        return None;
    }
    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let (best_lag, best) = scores[0];
    // Clear of the rest: the best outside a hundred milliseconds of the
    // peak stands well below it.
    let rival = scores
        .iter()
        .filter(|(lag, _)| (lag - best_lag).abs() > 10)
        .map(|(_, c)| *c)
        .fold(f32::MIN, f32::max);
    if best < 0.45 || best - rival < 0.08 {
        return None;
    }
    Some((best_lag as u32 * 10, best))
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
                                // A read behind the last one's projection, or
                                // of another file or state, starts the fit afresh:
                                // the earlier reads would hold the position up.
                                let afresh = reads.back().is_some_and(|last: &Status| {
                                    last.file != status.file
                                        || last.state != status.state
                                        || matches!(
                                            (last.elapsed, status.elapsed),
                                            (Some(a), Some(b)) if b + 0.1 < a
                                        )
                                });
                                if afresh {
                                    reads.clear();
                                }
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

impl Clock {
    /// The last read, in a few words, for the log.
    pub fn last_read(&self) -> String {
        let reads = self.reads.lock().unwrap_or_else(|e| e.into_inner());
        match reads.back() {
            Some(r) => format!(
                "{} reads, the last state {:?} elapsed {:?} file {:?} {} ms ago",
                reads.len(),
                r.state,
                r.elapsed,
                r.file.chars().rev().take(24).collect::<String>().chars().rev().collect::<String>(),
                r.at.map(|at| at.elapsed().as_millis()).unwrap_or(0)
            ),
            None => "no read yet".to_string(),
        }
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
    envelope: Option<Envelope>,
    clock: Clock,
    /// The frame shown, kept until the next.
    shown: Option<Decoded>,
    /// Frames taken from the decoder and not yet due.
    waiting: VecDeque<Decoded>,
    /// The screen's lead, in seconds: the picture that far ahead of the
    /// sound, as the meters are, for a screen whose own processing runs
    /// behind the sound.
    lead: f64,
    /// The sound output's delay, in seconds: mpd's position runs ahead of
    /// the sound at the DAC by its buffers, about half a second on a
    /// player. The measured default until the correlation finds it.
    delay: f64,
    /// Whether the delay was found by correlation, and how well it stood.
    measured: Option<f32>,
    /// One window's reading, waiting for the next window to agree.
    candidate: Option<f64>,
    /// The loudness the tap measured, placed by mpd's position at each
    /// measure, for the correlation.
    heard: VecDeque<(f64, f32)>,
    last_position: Option<f64>,
    calibrated_at: Option<Instant>,
    /// The last tick's moment, for telling a jump of the clock from its run.
    last_tick: Option<Instant>,
    /// The decoder's start-up as learnt, in seconds, and the last start:
    /// its moment and the place asked, until its first frame arrives.
    startup: f64,
    starting: Option<(Instant, f64)>,
    /// What was last reported to the player, so the line goes once per change.
    reported: String,
    /// The sync harness: a line per frame when `GLASS_SYNC_LOG` names a file.
    log: Option<File>,
    started: Instant,
}

/// The sound output's delay on a player as measured on a Raspberry Pi 5
/// with mpd's buffers as the image sets them: the start until the
/// correlation finds the player's own.
pub const DEFAULT_DELAY_MS: u32 = 530;
/// How often the delay is looked for again.
const CALIBRATE_EVERY: Duration = Duration::from_secs(4);
/// The window the correlation looks over, in seconds of the file.
const CALIBRATE_WINDOW: f64 = 12.0;
/// Two windows' readings this close agree, and the delay is taken.
const CANDIDATE_AGREE_S: f64 = 0.06;

impl Video {
    /// For `file`, which the prober says carries video; `lead_ms` as the
    /// meters' lead, `delay_ms` the sound output's delay to start from
    /// (the measured default, or the last one found on this player).
    pub fn open(file: &str, probe: Probe, lead_ms: u32, delay_ms: u32) -> Self {
        let log = std::env::var_os("GLASS_SYNC_LOG").and_then(|p| File::create(p).ok());
        Self {
            file: file.to_string(),
            probe,
            decoder: None,
            envelope: None,
            clock: Clock::start(),
            shown: None,
            waiting: VecDeque::new(),
            lead: lead_ms as f64 / 1000.0,
            delay: delay_ms as f64 / 1000.0,
            measured: None,
            candidate: None,
            heard: VecDeque::with_capacity(4000),
            last_position: None,
            calibrated_at: None,
            last_tick: None,
            startup: STARTUP_S,
            starting: None,
            reported: String::new(),
            log,
            started: Instant::now(),
        }
    }

    /// The sound output's delay in force, in milliseconds, and whether it
    /// was measured on this file (with the correlation it stood at).
    pub fn delay(&self) -> (u32, Option<f32>) {
        ((self.delay * 1000.0).round() as u32, self.measured)
    }

    /// What the display does with the picture, for the player: the file,
    /// its shape, the delay and whether it was measured; sent once per
    /// change through `report`.
    pub fn report(&mut self, state: &str) -> Option<serde_json::Value> {
        let (delay, measured) = self.delay();
        let value = serde_json::json!({
            "file": self.file,
            "width": self.probe.width,
            "height": self.probe.height,
            "fps": (self.probe.fps * 1000.0).round() / 1000.0,
            "state": state,
            "delay": delay,
            "measured": measured.map(|c| (c * 100.0).round() / 100.0),
        });
        let text = value.to_string();
        if text == self.reported {
            return None;
        }
        self.reported = text;
        Some(value)
    }

    /// The correlation run, when enough has been heard since the last.
    fn calibrate(&mut self, now: Instant, position: f64) {
        if self
            .calibrated_at
            .is_some_and(|at| now.duration_since(at) < CALIBRATE_EVERY)
        {
            return;
        }
        self.calibrated_at = Some(now);
        let Some(envelope) = self.envelope.as_ref() else {
            return;
        };
        let from = position - CALIBRATE_WINDOW;
        let heard: Vec<(f64, f32)> = self
            .heard
            .iter()
            .filter(|(p, _)| *p >= from)
            .copied()
            .collect();
        let file = envelope.window(from - 1.6, position);
        logline::say!(
            Verbose,
            "video",
            "calibration: {} measures heard, {} file bins",
            heard.len(),
            file.len()
        );
        if let Some((ms, corr)) = correlate_delay(&heard, &file) {
            let found = ms as f64 / 1000.0;
            // A film's sound has onsets of its own at every spacing, so one
            // window's reading is held until the next agrees with it; then
            // eased in, so no window's reading jolts the picture.
            let next = match (self.measured, self.candidate) {
                (Some(_), _) => self.delay * 0.5 + found * 0.5,
                (None, Some(held)) if (held - found).abs() <= CANDIDATE_AGREE_S => found,
                (None, _) => {
                    logline::say!(
                        Verbose,
                        "video",
                        "the delay read at {ms} ms (correlation {corr:.2}), held for the next window"
                    );
                    self.candidate = Some(found);
                    return;
                }
            };
            if (next - self.delay).abs() >= 0.015 {
                logline::say!(
                    Info,
                    "video",
                    "the sound output's delay measured at {} ms (correlation {corr:.2})",
                    (next * 1000.0).round() as u32
                );
            }
            self.delay = next;
            self.measured = Some(corr);
        }
    }

    pub fn file(&self) -> &str {
        &self.file
    }

    pub fn size(&self) -> (u32, u32) {
        (self.probe.width, self.probe.height)
    }

    /// The decoder started where the clock will be when its first frame
    /// comes: `target` plus the start-up learnt.
    fn restart(&mut self, now: Instant, target: f64) {
        let at = target + self.startup + STARTUP_MARGIN_S;
        self.waiting.clear();
        self.heard.clear();
        self.decoder = None;
        self.envelope = None;
        self.starting = Some((now, at));
        match Decoder::start(&self.file, &self.probe, at) {
            Ok(decoder) => self.decoder = Some(decoder),
            Err(e) => logline::say!(Warn, "video", "the decoder did not start: {e}"),
        }
        // The sound's envelope from a second and a half before, so the
        // correlation has the file's side at every lag it tries.
        match Envelope::start(&self.file, at - self.delay - 1.6) {
            Ok(envelope) => self.envelope = Some(envelope),
            Err(e) => logline::say!(Verbose, "video", "no envelope, the delay stays as set: {e}"),
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
            if self.decoder.is_some() || self.shown.is_some() {
                logline::say!(
                    Verbose,
                    "video",
                    "the clock gives no position ({}): the picture let go",
                    self.clock.last_read()
                );
            }
            self.decoder = None;
            self.envelope = None;
            self.starting = None;
            self.waiting.clear();
            self.heard.clear();
            self.shown = None;
            self.last_position = None;
            self.last_tick = None;
            return false;
        };
        // A jump of the clock: further than its own time since the last
        // tick allows, or back.
        let jumped = match (self.last_position, self.last_tick) {
            (Some(p), Some(t)) => {
                let wall = now.duration_since(t).as_secs_f64();
                position < p - 0.1 || position > p + wall + JUMP_S
            }
            _ => false,
        };
        // The sound heard now, placed by the position, while it moves.
        if !jumped && self.last_position.is_none_or(|p| position > p) {
            self.heard.push_back((position, level));
            if self.heard.len() > 4000 {
                self.heard.pop_front();
            }
            self.calibrate(now, position);
        }
        self.last_position = Some(position);
        self.last_tick = Some(now);
        let target = position + self.lead - self.delay;
        if self.decoder.is_none() || jumped {
            logline::say!(
                Verbose,
                "video",
                "the decoder started at {target:.2} ({})",
                if jumped { "the clock jumped" } else { "none ran" }
            );
            self.restart(now, target);
        }
        // Frames from the decoder into the waiting line; the first after a
        // start teaches the start-up, and one far behind the clock all the
        // same has the decoder started once more with it.
        let mut first = None;
        if let Some(decoder) = &self.decoder {
            while self.waiting.len() < AHEAD_FRAMES {
                match decoder.next() {
                    Some(frame) => {
                        if self.starting.is_some() && self.waiting.is_empty() {
                            first = Some(frame.pts);
                        }
                        self.waiting.push_back(frame);
                    }
                    None => break,
                }
            }
            if decoder.ended.load(Ordering::Acquire) && self.waiting.is_empty() {
                // The file's end, or the child gone: nothing more to show.
                self.decoder = None;
                self.starting = None;
            }
        }
        if let Some(pts) = first {
            let (since, asked) = self.starting.take().unwrap_or((now, pts));
            let took = now.duration_since(since).as_secs_f64();
            self.startup = (self.startup + took) / 2.0;
            logline::say!(
                Verbose,
                "video",
                "the decoder's first frame {:.2} s after its start at {asked:.2}, {:.2} s from the clock; start-up now {:.2} s",
                took,
                pts - target,
                self.startup
            );
            if target - pts > RESTART_BEHIND_S {
                self.restart(now, target);
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

    /// The file's loudness: bursts at every whole second; the tap heard
    /// the same bursts 530 ms later by mpd's position, through a meter's
    /// slow fall: the correlation names the lag within a bin or two.
    #[test]
    fn the_correlation_finds_the_output_delay() {
        let file: Vec<(f64, f32)> = (0..2500)
            .map(|i| {
                let t = 10.0 + i as f64 * 0.01;
                let in_burst = (t.fract() * 100.0) < 5.0;
                (t, if in_burst { 0.8 } else { 0.02 })
            })
            .collect();
        let mut level = 0.0f32;
        let heard: Vec<(f64, f32)> = (0..900)
            .map(|i| {
                let position = 14.0 + i as f64 * 0.0333;
                let t = position - 0.53;
                let in_burst = (t.fract() * 100.0) < 5.0;
                level = if in_burst { 80.0 } else { level * 0.9 };
                (position, level)
            })
            .collect();
        let (ms, corr) = correlate_delay(&heard, &file).expect("a delay found");
        assert!((500..=560).contains(&ms), "{ms} ms at {corr}");
        assert!(corr > 0.45);
        // A flat sound says nothing.
        let flat: Vec<(f64, f32)> = heard.iter().map(|(p, _)| (*p, 10.0)).collect();
        assert_eq!(correlate_delay(&flat, &file), None);
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
