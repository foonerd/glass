//! `glasstap`: the tap inside the audio player, in two forms. As an ALSA
//! PCM (`type glasstap`, the `pcm` module) the stream passes through it
//! byte for byte and is measured on the way. As an ALSA scope (this file)
//! a `meter` PCM hands it the stream a period at a time. Both publish the
//! peak and RMS of each channel and the spectrum into the shared ring, and
//! the scope, when the configuration names them, writes the two FIFOs of
//! the previous tap.
//!
//! ALSA loads this library into the audio player's process and calls it
//! from the thread that writes audio. Every entry point catches a panic,
//! nothing allocates once the stream is set up, and nothing blocks.
//!
//! Configuration keys, in the `pcm_scope` or the `pcm` node: `fft_size`
//! (default 2048), `hop` (default half the FFT), `ring` (a tag in the
//! ring's file name) and `slots` (hops kept in the ring, default 64); for
//! the scope also `meter` and `spectrum` (the FIFO paths, optional),
//! `meter_max`, `spectrum_max`, `spectrum_size`, `decay_ms`,
//! `logarithmic_frequency`, `logarithmic_amplitude` and `smoothing_factor`
//! as before.

use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use tap::analysis::Analyser;
use tap::fifo::{ignore_sigpipe, Fifo};
use tap::legacy;
use tap::ring::{Frame, Writer};

mod pcm;

#[allow(non_camel_case_types)]
pub(crate) mod ffi {
    use super::*;

    #[repr(C)]
    pub struct snd_pcm_t {
        _private: [u8; 0],
    }
    #[repr(C)]
    pub struct snd_pcm_scope_t {
        _private: [u8; 0],
    }
    #[repr(C)]
    pub struct snd_config_t {
        _private: [u8; 0],
    }
    pub type snd_config_iterator_t = *mut c_void;

    /// The scope callbacks, in the order libasound declares them.
    #[repr(C)]
    pub struct snd_pcm_scope_ops_t {
        pub enable: Option<unsafe extern "C" fn(*mut snd_pcm_scope_t) -> c_int>,
        pub disable: Option<unsafe extern "C" fn(*mut snd_pcm_scope_t)>,
        pub start: Option<unsafe extern "C" fn(*mut snd_pcm_scope_t)>,
        pub stop: Option<unsafe extern "C" fn(*mut snd_pcm_scope_t)>,
        pub update: Option<unsafe extern "C" fn(*mut snd_pcm_scope_t)>,
        pub reset: Option<unsafe extern "C" fn(*mut snd_pcm_scope_t)>,
        pub close: Option<unsafe extern "C" fn(*mut snd_pcm_scope_t)>,
    }

    extern "C" {
        pub fn snd_pcm_meter_get_bufsize(pcm: *mut snd_pcm_t) -> c_ulong;
        pub fn snd_pcm_meter_get_channels(pcm: *mut snd_pcm_t) -> c_uint;
        pub fn snd_pcm_meter_get_rate(pcm: *mut snd_pcm_t) -> c_uint;
        pub fn snd_pcm_meter_get_now(pcm: *mut snd_pcm_t) -> c_ulong;
        pub fn snd_pcm_meter_get_boundary(pcm: *mut snd_pcm_t) -> c_ulong;
        pub fn snd_pcm_meter_add_scope(pcm: *mut snd_pcm_t, scope: *mut snd_pcm_scope_t) -> c_int;
        pub fn snd_pcm_meter_search_scope(
            pcm: *mut snd_pcm_t,
            name: *const c_char,
        ) -> *mut snd_pcm_scope_t;
        pub fn snd_pcm_scope_malloc(ptr: *mut *mut snd_pcm_scope_t) -> c_int;
        pub fn snd_pcm_scope_set_ops(scope: *mut snd_pcm_scope_t, ops: *const snd_pcm_scope_ops_t);
        pub fn snd_pcm_scope_set_name(scope: *mut snd_pcm_scope_t, name: *const c_char);
        pub fn snd_pcm_scope_get_callback_private(scope: *mut snd_pcm_scope_t) -> *mut c_void;
        pub fn snd_pcm_scope_set_callback_private(
            scope: *mut snd_pcm_scope_t,
            private: *mut c_void,
        );
        pub fn snd_pcm_scope_s16_open(
            pcm: *mut snd_pcm_t,
            name: *const c_char,
            scope: *mut *mut snd_pcm_scope_t,
        ) -> c_int;
        pub fn snd_pcm_scope_s16_get_channel_buffer(
            scope: *mut snd_pcm_scope_t,
            channel: c_uint,
        ) -> *mut i16;
        pub fn snd_config_iterator_first(node: *const snd_config_t) -> snd_config_iterator_t;
        pub fn snd_config_iterator_next(it: snd_config_iterator_t) -> snd_config_iterator_t;
        pub fn snd_config_iterator_end(node: *const snd_config_t) -> snd_config_iterator_t;
        pub fn snd_config_iterator_entry(it: snd_config_iterator_t) -> *mut snd_config_t;
        pub fn snd_config_get_id(node: *const snd_config_t, id: *mut *const c_char) -> c_int;
        pub fn snd_config_get_integer(node: *const snd_config_t, value: *mut c_long) -> c_int;
        pub fn snd_config_get_string(node: *const snd_config_t, value: *mut *const c_char)
            -> c_int;
    }
}

use ffi::*;

/// What the configuration asked for.
pub(crate) struct Settings {
    meter_fifo: Option<String>,
    spectrum_fifo: Option<String>,
    meter_max: u32,
    spectrum_max: u32,
    spectrum_size: usize,
    decay_ms: u32,
    log_f: bool,
    log_y: bool,
    smoothing: u32,
    fft_size: usize,
    hop: usize,
    ring: String,
    slots: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            meter_fifo: None,
            spectrum_fifo: None,
            meter_max: 100,
            spectrum_max: 100,
            spectrum_size: 20,
            decay_ms: 400,
            log_f: true,
            log_y: true,
            smoothing: 60,
            fft_size: 2048,
            hop: 0,
            ring: "glasstap".to_string(),
            slots: 64,
        }
    }
}

/// Everything the scope keeps between calls.
struct State {
    pcm: *mut snd_pcm_t,
    s16: *mut snd_pcm_scope_t,
    settings: Settings,
    old: c_ulong,
    channels: usize,
    rate: u32,
    bufsize: usize,
    analyser: Option<Analyser>,
    writer: Option<Writer>,
    meter: Option<Fifo>,
    spectrum: Option<Fifo>,
    legacy_meter: legacy::Meter,
    legacy_spectrum: legacy::Spectrum,
    staging: [Vec<i16>; 2],
    dop_group: usize,
    /// Set after a panic: the scope does nothing more, the player plays on.
    failed: bool,
}

// The scope runs on the player's audio thread only.
unsafe impl Send for State {}

fn state<'a>(scope: *mut snd_pcm_scope_t) -> Option<&'a mut State> {
    let p = unsafe { snd_pcm_scope_get_callback_private(scope) } as *mut State;
    if p.is_null() {
        None
    } else {
        Some(unsafe { &mut *p })
    }
}

pub(crate) fn note(msg: &str) {
    eprintln!("glasstap: {msg}");
}

/// Read the tap's configuration node, the scope's or the PCM's.
pub(crate) unsafe fn settings_from(conf: *const snd_config_t) -> Settings {
    let mut s = Settings::default();
    let end = snd_config_iterator_end(conf);
    let mut it = snd_config_iterator_first(conf);
    while it != end {
        let node = snd_config_iterator_entry(it);
        it = snd_config_iterator_next(it);
        let mut id: *const c_char = std::ptr::null();
        if snd_config_get_id(node, &mut id) < 0 || id.is_null() {
            continue;
        }
        let key = CStr::from_ptr(id).to_string_lossy().into_owned();
        let integer = |node: *const snd_config_t| -> Option<i64> {
            let mut v: c_long = 0;
            if snd_config_get_integer(node, &mut v) < 0 {
                None
            } else {
                Some(v as i64)
            }
        };
        let string = |node: *const snd_config_t| -> Option<String> {
            let mut v: *const c_char = std::ptr::null();
            if snd_config_get_string(node, &mut v) < 0 || v.is_null() {
                None
            } else {
                Some(CStr::from_ptr(v).to_string_lossy().into_owned())
            }
        };
        match key.as_str() {
            "comment" | "type" | "slave" | "hint" | "meter_show" | "window" => {}
            "meter" => s.meter_fifo = string(node).filter(|p| !p.is_empty()),
            "spectrum" => s.spectrum_fifo = string(node).filter(|p| !p.is_empty()),
            "meter_max" => s.meter_max = integer(node).unwrap_or(100).clamp(1, 65535) as u32,
            "spectrum_max" => s.spectrum_max = integer(node).unwrap_or(100).clamp(1, 65535) as u32,
            "spectrum_size" => s.spectrum_size = integer(node).unwrap_or(20).clamp(1, 256) as usize,
            "decay_ms" => s.decay_ms = integer(node).unwrap_or(400).clamp(1, 60_000) as u32,
            "logarithmic_frequency" => s.log_f = integer(node).unwrap_or(1) != 0,
            "logarithmic_amplitude" => s.log_y = integer(node).unwrap_or(1) != 0,
            "smoothing_factor" => s.smoothing = integer(node).unwrap_or(60).clamp(0, 100) as u32,
            "fft_size" => s.fft_size = integer(node).unwrap_or(2048).clamp(64, 32_768) as usize,
            "hop" => s.hop = integer(node).unwrap_or(0).clamp(0, 32_768) as usize,
            "slots" => s.slots = integer(node).unwrap_or(64).clamp(2, 4096) as usize,
            "ring" => {
                s.ring = string(node)
                    .filter(|t| {
                        !t.is_empty()
                            && t.chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                    })
                    .unwrap_or_else(|| "glasstap".to_string())
            }
            other => note(&format!("unknown key {other} ignored")),
        }
    }
    if s.hop == 0 {
        s.hop = s.fft_size / 2;
    }
    s
}

unsafe extern "C" fn enable(scope: *mut snd_pcm_scope_t) -> c_int {
    let Some(st) = state(scope) else {
        return -libc::EINVAL;
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        st.channels = (snd_pcm_meter_get_channels(st.pcm) as usize).clamp(1, tap::MAX_CHANNELS);
        st.rate = snd_pcm_meter_get_rate(st.pcm);
        st.bufsize = snd_pcm_meter_get_bufsize(st.pcm) as usize;
        st.dop_group = tap::dop::group_frames(st.rate);
        st.staging = [vec![0; st.bufsize], vec![0; st.bufsize]];
        st.analyser = Some(Analyser::new(
            st.rate,
            st.channels as u32,
            st.settings.fft_size,
            st.settings.hop,
        ));
        match Writer::create(
            Path::new(tap::ring::DIR),
            &st.settings.ring,
            st.rate,
            st.channels as u32,
            st.settings.fft_size as u32,
            st.settings.hop as u32,
            st.settings.slots,
        ) {
            Ok(w) => st.writer = Some(w),
            Err(e) => note(&format!("no ring: {e}")),
        }
        0
    }));
    match outcome {
        Ok(code) => code,
        Err(_) => {
            st.failed = true;
            note("enable failed; the tap stays quiet");
            0
        }
    }
}

unsafe extern "C" fn disable(scope: *mut snd_pcm_scope_t) {
    if let Some(st) = state(scope) {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            st.writer = None;
            st.analyser = None;
            st.staging = [Vec::new(), Vec::new()];
        }));
    }
}

unsafe extern "C" fn start(scope: *mut snd_pcm_scope_t) {
    if let Some(st) = state(scope) {
        st.old = snd_pcm_meter_get_now(st.pcm);
    }
}

unsafe extern "C" fn stop(_scope: *mut snd_pcm_scope_t) {}

unsafe extern "C" fn reset(scope: *mut snd_pcm_scope_t) {
    if let Some(st) = state(scope) {
        st.old = snd_pcm_meter_get_now(st.pcm);
        if let Some(a) = st.analyser.as_mut() {
            a.reset();
        }
    }
}

unsafe extern "C" fn update(scope: *mut snd_pcm_scope_t) {
    let Some(st) = state(scope) else { return };
    if st.failed {
        return;
    }
    let outcome = catch_unwind(AssertUnwindSafe(|| measure(st)));
    if outcome.is_err() {
        st.failed = true;
        note("measurement failed; the tap stays quiet");
    }
}

/// One period: copy the new frames out of the meter's buffer, measure them.
unsafe fn measure(st: &mut State) {
    let pcm = st.pcm;
    let now = snd_pcm_meter_get_now(pcm);
    let boundary = snd_pcm_meter_get_boundary(pcm);
    let mut size = now.wrapping_sub(st.old);
    if now < st.old {
        size = size.wrapping_add(boundary);
    }
    st.old = now;
    let bufsize = st.bufsize;
    if bufsize == 0 || size == 0 {
        return;
    }
    // More than a buffer since the last look means frames were lost; the
    // last buffer's worth is what there is.
    let size = (size as usize).min(bufsize);
    let offset = (now as usize + bufsize - size) % bufsize;
    let first = size.min(bufsize - offset);
    let second = size - first;
    for ch in 0..st.channels {
        let buffer = snd_pcm_scope_s16_get_channel_buffer(st.s16, ch as c_uint);
        if buffer.is_null() {
            return;
        }
        let ring = std::slice::from_raw_parts(buffer, bufsize);
        let staging = &mut st.staging[ch];
        staging[..first].copy_from_slice(&ring[offset..offset + first]);
        staging[first..size].copy_from_slice(&ring[..second]);
    }
    let channels = st.channels;
    let (left, right) = st.staging.split_at_mut(1);
    let left = &left[0][..size];
    let right = if channels > 1 {
        &right[0][..size]
    } else {
        left
    };

    // Raw peaks on the 16-bit scale for the old meter record; DoP by density.
    let dop = tap::dop::is_stream(left);
    let raw = if dop {
        [
            tap::dop::level(left, st.dop_group),
            tap::dop::level(right, st.dop_group),
        ]
    } else {
        let peak = |s: &[i16]| {
            s.iter()
                .map(|v| (*v as i32).abs())
                .max()
                .unwrap_or(0)
                .min(32767)
        };
        [peak(left), peak(right)]
    };
    if let Some(fifo) = st.meter.as_mut() {
        let (l, r) = st.legacy_meter.update(raw, size as u64, st.rate);
        fifo.write(&legacy::meter_record(l, r));
    }

    let Some(analyser) = st.analyser.as_mut() else {
        return;
    };
    if dop {
        // The bit stream has no spectrum to show; the level stands in.
        let mut frame = Frame {
            frames: 0,
            ..Frame::default()
        };
        frame.peak = [raw[0] as f32 / 32767.0, raw[1] as f32 / 32767.0];
        frame.rms = frame.peak;
        frame.spectrum = [vec![0.0; analyser.bins()], vec![0.0; analyser.bins()]];
        if let Some(w) = st.writer.as_mut() {
            w.publish(&frame);
        }
        return;
    }
    let writer = st.writer.as_mut();
    let spectrum_fifo = st.spectrum.as_mut();
    let legacy_spectrum = &mut st.legacy_spectrum;
    let mut writer = writer;
    let mut spectrum_fifo = spectrum_fifo;
    let chans: [&[i16]; 2] = [left, right];
    analyser.feed(&chans[..channels.max(1)], size, &mut |frame, _| {
        if let Some(w) = writer.as_mut() {
            w.publish(frame);
        }
        if let Some(fifo) = spectrum_fifo.as_mut() {
            let values = legacy_spectrum.update(&frame.spectrum[0]);
            fifo.write(&legacy::spectrum_record(&values));
        }
    });
}

unsafe extern "C" fn close(scope: *mut snd_pcm_scope_t) {
    let p = snd_pcm_scope_get_callback_private(scope) as *mut State;
    if !p.is_null() {
        snd_pcm_scope_set_callback_private(scope, std::ptr::null_mut());
        // Made by Box::into_raw in the open function.
        drop(Box::from_raw(p));
    }
}

static OPS: snd_pcm_scope_ops_t = snd_pcm_scope_ops_t {
    enable: Some(enable),
    disable: Some(disable),
    start: Some(start),
    stop: Some(stop),
    update: Some(update),
    reset: Some(reset),
    close: Some(close),
};

/// The entry point libasound looks for when the configuration names the
/// scope type `glasstap`.
///
/// # Safety
/// Called by libasound with a live PCM and configuration nodes.
#[no_mangle]
pub unsafe extern "C" fn _snd_pcm_scope_glasstap_open(
    pcm: *mut snd_pcm_t,
    name: *const c_char,
    _root: *mut snd_config_t,
    conf: *mut snd_config_t,
) -> c_int {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let settings = settings_from(conf);
        ignore_sigpipe();
        let mut scope: *mut snd_pcm_scope_t = std::ptr::null_mut();
        let err = snd_pcm_scope_malloc(&mut scope);
        if err < 0 {
            return err;
        }
        let s16_name = CString::new("s16").unwrap();
        let mut s16 = snd_pcm_meter_search_scope(pcm, s16_name.as_ptr());
        if s16.is_null() {
            let err = snd_pcm_scope_s16_open(pcm, s16_name.as_ptr(), &mut s16);
            if err < 0 {
                libc::free(scope as *mut c_void);
                return err;
            }
        }
        let meter = settings.meter_fifo.as_deref().and_then(Fifo::new);
        let spectrum = settings.spectrum_fifo.as_deref().and_then(Fifo::new);
        let state = Box::new(State {
            pcm,
            s16,
            legacy_meter: legacy::Meter::new(settings.decay_ms, settings.meter_max),
            legacy_spectrum: legacy::Spectrum::new(
                settings.spectrum_size,
                settings.spectrum_max,
                settings.log_f,
                settings.log_y,
                settings.smoothing,
            ),
            settings,
            old: 0,
            channels: 2,
            rate: 44_100,
            bufsize: 0,
            analyser: None,
            writer: None,
            meter,
            spectrum,
            staging: [Vec::new(), Vec::new()],
            dop_group: 8,
            failed: false,
        });
        snd_pcm_scope_set_ops(scope, &OPS);
        snd_pcm_scope_set_callback_private(scope, Box::into_raw(state) as *mut c_void);
        if !name.is_null() {
            // libasound keeps the pointer; the copy lives as long as the process.
            snd_pcm_scope_set_name(scope, libc::strdup(name));
        }
        snd_pcm_meter_add_scope(pcm, scope);
        0
    }));
    match outcome {
        Ok(code) => code,
        Err(_) => {
            note("open failed");
            -libc::EINVAL
        }
    }
}
