//! Bit-perfect output on a Linux remote: ALSA opened directly at the
//! stream's own rate and width, the one place on the way where the bits
//! reach a DAC untouched. The library is opened at run time, never linked:
//! a machine without it, or a device that cannot take the stream's shape,
//! says so and the sound goes through the system's output as before.

use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};

/// `SND_PCM_STREAM_PLAYBACK`.
const STREAM_PLAYBACK: c_int = 0;
/// `SND_PCM_ACCESS_RW_INTERLEAVED`: frames written interleaved by `writei`.
const ACCESS_RW_INTERLEAVED: c_int = 3;
/// The library as Debian names it, the version every player and remote carries.
const LIBRARY: &str = "libasound.so.2";

type OpenFn = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
type CloseFn = unsafe extern "C" fn(*mut c_void) -> c_int;
type SetParamsFn =
    unsafe extern "C" fn(*mut c_void, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int;
type WriteiFn = unsafe extern "C" fn(*mut c_void, *const c_void, libc::c_ulong) -> libc::c_long;
type RecoverFn = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
type DropFn = unsafe extern "C" fn(*mut c_void) -> c_int;
type StrerrorFn = unsafe extern "C" fn(c_int) -> *const c_char;

/// ALSA's library, opened at run time, with the seven calls a writer needs.
pub struct Lib {
    handle: *mut c_void,
    open: OpenFn,
    close: CloseFn,
    set_params: SetParamsFn,
    writei: WriteiFn,
    recover: RecoverFn,
    drop_: DropFn,
    strerror: StrerrorFn,
}

// The handle and the function pointers are the library's, valid on any
// thread; the device opened from them is used by one thread at a time.
unsafe impl Send for Lib {}

impl Lib {
    /// The library by its name; the error says what is missing.
    pub fn load() -> Result<Self, String> {
        Self::load_named(LIBRARY)
    }

    fn load_named(name: &str) -> Result<Self, String> {
        let c_name = CString::new(name).map_err(|e| e.to_string())?;
        // SAFETY: dlopen with a valid C string; a null handle is the failure.
        let handle = unsafe { libc::dlopen(c_name.as_ptr(), libc::RTLD_NOW) };
        if handle.is_null() {
            return Err(format!("{name} is not on this machine"));
        }
        let sym = |symbol: &str| -> Result<*mut c_void, String> {
            let c_symbol = CString::new(symbol).map_err(|e| e.to_string())?;
            // SAFETY: dlsym on an open handle with a valid C string.
            let p = unsafe { libc::dlsym(handle, c_symbol.as_ptr()) };
            if p.is_null() {
                Err(format!("{name} has no {symbol}"))
            } else {
                Ok(p)
            }
        };
        // SAFETY: each symbol is transmuted to the prototype alsa-lib 1.2
        // declares for it (include/pcm.h); the ABI of these calls has not
        // changed within the library's major version.
        let lib = unsafe {
            Self {
                handle,
                open: std::mem::transmute::<*mut c_void, OpenFn>(sym("snd_pcm_open")?),
                close: std::mem::transmute::<*mut c_void, CloseFn>(sym("snd_pcm_close")?),
                set_params: std::mem::transmute::<*mut c_void, SetParamsFn>(sym(
                    "snd_pcm_set_params",
                )?),
                writei: std::mem::transmute::<*mut c_void, WriteiFn>(sym("snd_pcm_writei")?),
                recover: std::mem::transmute::<*mut c_void, RecoverFn>(sym("snd_pcm_recover")?),
                drop_: std::mem::transmute::<*mut c_void, DropFn>(sym("snd_pcm_drop")?),
                strerror: std::mem::transmute::<*mut c_void, StrerrorFn>(sym("snd_strerror")?),
            }
        };
        Ok(lib)
    }

    fn words(&self, err: c_int) -> String {
        // SAFETY: strerror returns a static string for any code.
        let p = unsafe { (self.strerror)(err) };
        if p.is_null() {
            return format!("error {err}");
        }
        // SAFETY: a NUL-terminated static string.
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

impl Drop for Lib {
    fn drop(&mut self) {
        // The library stays mapped for the process's life: a dlclose while
        // another device still uses it would pull the floor away, and the
        // handle is reference counted by the loader anyway.
        let _ = self.handle;
    }
}

/// ALSA's format number for a segment's sample format, and its name.
pub fn format_of(format: &str) -> Option<(c_int, &'static str)> {
    match format {
        "s16le" => Some((2, "S16_LE")),
        "s24le" => Some((32, "S24_3LE")),
        "s32le" => Some((10, "S32_LE")),
        "f32le" => Some((14, "FLOAT_LE")),
        _ => None,
    }
}

/// A device open at one shape, written to in whole frames.
pub struct Device {
    lib: Lib,
    pcm: *mut c_void,
    name: String,
    frame_bytes: usize,
}

// The device is driven by one thread; the pointer is ALSA's own.
unsafe impl Send for Device {}

impl Device {
    /// Opened at exactly `rate` and `format` with `channels`: no
    /// resampling and no conversion in the library (soft_resample off),
    /// so a shape the device cannot take is refused and said. The latency
    /// is the device's own ring, in microseconds.
    pub fn open(
        lib: Lib,
        name: &str,
        format: c_int,
        channels: u32,
        rate: u32,
        frame_bytes: usize,
        latency_us: u32,
    ) -> Result<Self, String> {
        let c_name = CString::new(name).map_err(|e| e.to_string())?;
        let mut pcm: *mut c_void = std::ptr::null_mut();
        // SAFETY: the call writes the handle into `pcm` on success.
        let err = unsafe { (lib.open)(&mut pcm, c_name.as_ptr(), STREAM_PLAYBACK, 0) };
        if err < 0 {
            return Err(format!("{name} cannot be opened: {}", lib.words(err)));
        }
        // SAFETY: a handle just opened.
        let err = unsafe {
            (lib.set_params)(
                pcm,
                format,
                ACCESS_RW_INTERLEAVED,
                channels,
                rate,
                0,
                latency_us,
            )
        };
        if err < 0 {
            let why = lib.words(err);
            // SAFETY: the handle is closed once and not used again.
            unsafe { (lib.close)(pcm) };
            return Err(format!("{name} cannot take this shape: {why}"));
        }
        Ok(Self {
            lib,
            pcm,
            name: name.to_string(),
            frame_bytes: frame_bytes.max(1),
        })
    }

    /// Whole frames written, waiting for room; an underrun or a suspend
    /// is recovered from and the frames written after; any other error
    /// ends the device.
    pub fn write(&mut self, frames: &[u8]) -> Result<(), String> {
        let mut left = frames;
        while left.len() >= self.frame_bytes {
            let count = (left.len() / self.frame_bytes) as libc::c_ulong;
            // SAFETY: `left` holds `count` whole frames at the device's shape.
            let wrote =
                unsafe { (self.lib.writei)(self.pcm, left.as_ptr() as *const c_void, count) };
            if wrote < 0 {
                // SAFETY: the handle is open; recover prepares it again after
                // an underrun (EPIPE) or a suspend (ESTRPIPE).
                let again = unsafe { (self.lib.recover)(self.pcm, wrote as c_int, 1) };
                if again < 0 {
                    return Err(format!("{}: {}", self.name, self.lib.words(again)));
                }
                continue;
            }
            left = &left[(wrote as usize) * self.frame_bytes..];
        }
        Ok(())
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: the handle is open; dropped (the queue discarded) and closed once.
        unsafe {
            (self.lib.drop_)(self.pcm);
            (self.lib.close)(self.pcm);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_map_to_alsas_numbers() {
        assert_eq!(format_of("s16le"), Some((2, "S16_LE")));
        assert_eq!(format_of("s24le"), Some((32, "S24_3LE")));
        assert_eq!(format_of("s32le"), Some((10, "S32_LE")));
        assert_eq!(format_of("f32le"), Some((14, "FLOAT_LE")));
        assert_eq!(format_of("dsd"), None);
    }

    #[test]
    fn a_missing_library_is_said_not_crashed() {
        let Err(err) = Lib::load_named("libnothing-of-the-kind.so.9") else {
            panic!("a library that is not there was loaded");
        };
        assert!(err.contains("not on this machine"), "{err}");
    }

    /// ALSA's own `null` device takes any shape and discards the frames:
    /// the whole path from the library through set_params and writei,
    /// with no hardware. Skipped where the library is absent.
    #[test]
    fn the_null_device_takes_frames_at_the_streams_shape() {
        let Ok(lib) = Lib::load() else {
            eprintln!("no libasound here: the device test is skipped");
            return;
        };
        let (format, _) = format_of("s24le").unwrap();
        let mut device = match Device::open(lib, "null", format, 2, 96_000, 6, 250_000) {
            Ok(device) => device,
            Err(why) => panic!("the null device refused: {why}"),
        };
        let frames = vec![0u8; 6 * 960]; // 10 ms at 96 kHz, 24-bit stereo
        device.write(&frames).unwrap();
        device.write(&frames[..6 * 7 + 3]).unwrap(); // a part frame at the end is left
    }

    #[test]
    fn a_device_that_is_not_there_is_said() {
        let Ok(lib) = Lib::load() else {
            return;
        };
        let Err(err) = Device::open(lib, "hw:99,99", 2, 2, 44_100, 4, 250_000) else {
            panic!("a device that is not there was opened");
        };
        assert!(err.starts_with("hw:99,99 cannot be opened"), "{err}");
    }
}
