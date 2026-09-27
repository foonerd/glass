//! The Android shell of the display. SDL's activity loads `libSDL2.so`
//! and this library, `libmain.so`, and calls [`SDL_main`] with the
//! arguments the activity gives. Nothing here draws: it points the display
//! at the app's own storage and runs it as a remote.

use std::ffi::{c_char, c_int, CStr, CString};

extern "C" {
    fn SDL_AndroidGetInternalStoragePath() -> *const c_char;
    fn SDL_SetHint(name: *const c_char, value: *const c_char) -> c_int;
}

#[allow(unused_unsafe)]
fn set_env(name: &str, value: &str) {
    // SAFETY: called on the first thread before any other thread exists,
    // which is what setting the environment asks for.
    unsafe { std::env::set_var(name, value) };
}

/// The entry point SDL's Java side calls once the activity is up. The
/// return value is only logged by SDL.
///
/// # Safety
///
/// `argv` holds `argc` C strings, as SDL passes them.
#[no_mangle]
pub unsafe extern "C" fn SDL_main(argc: c_int, argv: *const *const c_char) -> c_int {
    // The app's files directory holds the configuration and what is brought
    // from players: the display reads XDG_CONFIG_HOME and XDG_CACHE_HOME first.
    let storage = SDL_AndroidGetInternalStoragePath();
    if !storage.is_null() {
        let dir = CStr::from_ptr(storage).to_string_lossy().into_owned();
        set_env("XDG_CONFIG_HOME", &format!("{dir}/config"));
        set_env("XDG_CACHE_HOME", &format!("{dir}/cache"));
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
    let _ = glass::run(args);
    0
}
