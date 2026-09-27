// On Windows the display is a windowed program: no console opens behind
// it when a shortcut starts it. Started from a terminal, it attaches to
// that terminal's console first, so its lines still arrive there.
#![cfg_attr(windows, windows_subsystem = "windows")]

/// Attach to the console of the terminal that started this process, when
/// there is one and nothing was redirected, so println! reaches it although
/// the program is built without a console of its own.
#[cfg(windows)]
fn attach_parent_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn GetStdHandle(handle: u32) -> *mut std::ffi::c_void;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    const STD_OUTPUT_HANDLE: u32 = 4_294_967_285; // (DWORD)-11
                                                  // SAFETY: plain Win32 calls with constant arguments; a failure to
                                                  // attach only means there is no parent console, and is ignored.
    unsafe {
        if GetStdHandle(STD_OUTPUT_HANDLE).is_null() {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

fn main() -> std::process::ExitCode {
    #[cfg(windows)]
    attach_parent_console();
    glass::run(std::env::args().collect())
}
