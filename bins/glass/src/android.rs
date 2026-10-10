//! The display as an Android app's native side. SDL's activity loads
//! `libSDL2.so` and the app's `libmain.so` and calls `SDL_main` there with
//! the arguments the activity gives; that library, Glass's own or one that
//! brings a face, hands them on to [`enter`]. Nothing here draws: it points
//! the display at the app's own storage and runs it as a remote.

use std::ffi::{c_char, c_int, CStr, CString};

use crate::{run_with, Overlay};

extern "C" {
    fn SDL_AndroidGetInternalStoragePath() -> *const c_char;
    fn SDL_SetHint(name: *const c_char, value: *const c_char) -> c_int;
    fn SDL_AndroidSendMessage(command: u32, param: c_int) -> c_int;
}

/// The message to the app's activity that opens the system's screen where
/// the app is given access to the device's files: `COMMAND_USER + 1` on
/// the Java side (GlassActivity).
const ASK_STORAGE_ACCESS: u32 = 0x8001;
static ASKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Ask the activity, once per start of the app, to open the system's
/// screen that grants access to the device's files: Android lets an app
/// read what other apps put under Download only with it, and a theme
/// from a folder on the device lies there. The activity does nothing
/// when the access is there already.
pub fn ask_storage_access() {
    if ASKED.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }
    // SAFETY: SDL's own call, made from any thread; it posts a message to
    // the activity's handler and reads nothing of ours.
    unsafe {
        SDL_AndroidSendMessage(ASK_STORAGE_ACCESS, 0);
    }
}

#[allow(unused_unsafe)]
fn set_env(name: &str, value: &str) {
    // SAFETY: called on the first thread before any other thread exists,
    // which is what setting the environment asks for.
    unsafe { std::env::set_var(name, value) };
}

/// What an app's `SDL_main` does: the display as a remote, with the face
/// the app was built with over it, or none. The return value is only
/// logged by SDL.
///
/// # Safety
///
/// `argv` holds `argc` C strings, as SDL passes them.
pub unsafe fn enter(
    argc: c_int,
    argv: *const *const c_char,
    face: Option<Box<dyn Overlay>>,
) -> c_int {
    // The app's files directory holds the configuration and what is brought
    // from players: the display reads XDG_CONFIG_HOME and XDG_CACHE_HOME first.
    let storage = SDL_AndroidGetInternalStoragePath();
    if !storage.is_null() {
        let dir = CStr::from_ptr(storage).to_string_lossy().into_owned();
        set_env("XDG_CONFIG_HOME", &format!("{dir}/config"));
        set_env("XDG_CACHE_HOME", &format!("{dir}/cache"));
        // The system's temp folder on Android, /data/local/tmp, is the
        // shell's and closed to an app: what the display keeps for a while,
        // the album art it fetches among it, goes under the app's cache.
        let tmp = format!("{dir}/cache/tmp");
        let _ = std::fs::create_dir_all(&tmp);
        set_env("TMPDIR", &tmp);
    }
    // A meter theme is landscape; the window turns with the device within that.
    if let (Ok(name), Ok(value)) = (
        CString::new("SDL_ORIENTATIONS"),
        CString::new("LandscapeLeft LandscapeRight"),
    ) {
        SDL_SetHint(name.as_ptr(), value.as_ptr());
    }
    let count = usize::try_from(argc).unwrap_or(0);
    let mut args: Vec<String> = (0..count)
        .filter_map(|i| {
            let item = *argv.add(i);
            (!item.is_null()).then(|| CStr::from_ptr(item).to_string_lossy().into_owned())
        })
        .collect();
    if args.is_empty() {
        args.push("glass".to_string());
    }
    if !args.iter().any(|a| a == "--remote") {
        args.insert(1, "--remote".to_string());
    }
    let _ = run_with(args, face);
    0
}
