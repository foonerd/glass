//! A FIFO written without ever blocking the audio thread: opened for
//! writing only while a reader has it open, written non-blocking, dropped
//! when the reader goes and opened again when one is back.

use std::ffi::CString;

pub struct Fifo {
    path: CString,
    fd: i32,
}

impl Fifo {
    /// Make the FIFO if it is not there, and be ready to open it.
    pub fn new(path: &str) -> Option<Self> {
        let cpath = CString::new(path).ok()?;
        // EEXIST is fine: the FIFO is there already.
        unsafe { libc::mkfifo(cpath.as_ptr(), 0o666) };
        let mut fifo = Self { path: cpath, fd: -1 };
        fifo.open();
        Some(fifo)
    }

    fn open(&mut self) {
        if self.fd >= 0 {
            return;
        }
        // Without a reader the open fails at once instead of waiting.
        self.fd = unsafe { libc::open(self.path.as_ptr(), libc::O_WRONLY | libc::O_NONBLOCK | libc::O_CLOEXEC) };
    }

    fn close(&mut self) {
        if self.fd >= 0 {
            unsafe { libc::close(self.fd) };
            self.fd = -1;
        }
    }

    /// Write one record. A full pipe drops it; a gone reader closes the
    /// descriptor until the next write finds a reader again.
    pub fn write(&mut self, bytes: &[u8]) {
        self.open();
        if self.fd < 0 {
            return;
        }
        let n = unsafe { libc::write(self.fd, bytes.as_ptr() as *const libc::c_void, bytes.len()) };
        if n < 0 {
            let err = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            if err == libc::EPIPE {
                self.close();
            }
        }
    }
}

impl Drop for Fifo {
    fn drop(&mut self) {
        self.close();
    }
}

/// Leave SIGPIPE ignored when the player has not set a handler of its own,
/// so a reader that goes away costs a failed write, not the player.
pub fn ignore_sigpipe() {
    unsafe {
        let mut current: libc::sigaction = std::mem::zeroed();
        if libc::sigaction(libc::SIGPIPE, std::ptr::null(), &mut current) != 0 {
            return;
        }
        if current.sa_sigaction != libc::SIG_DFL {
            return;
        }
        let mut ignore: libc::sigaction = std::mem::zeroed();
        ignore.sa_sigaction = libc::SIG_IGN;
        libc::sigaction(libc::SIGPIPE, &ignore, std::ptr::null_mut());
    }
}
