// SPDX-License-Identifier: GPL-3.0-only

//! Process setup that needs `unsafe` (ARC-011): the environment Qt reads at
//! startup, the numeric locale libmpv requires and the termination signals.

use std::fs::File;
use std::io::Read;
use std::os::fd::FromRawFd;
use std::sync::atomic::{AtomicI32, Ordering};

/// Sets the variables Qt reads when it starts. MUST run before any other thread
/// exists, because changing the environment is not thread-safe.
pub fn prepare_environment() {
    // X11 with EGL, so that the video item gets the same GL path as on Wayland
    // (ADR 0003). The xcb platform plugin is the only reader of this variable.
    if std::env::var_os("QT_XCB_GL_INTEGRATION").is_none() {
        // SAFETY: called at the start of `main`, before any thread is spawned, so
        // nothing reads the environment concurrently.
        unsafe { std::env::set_var("QT_XCB_GL_INTEGRATION", "xcb_egl") };
    }
}

/// libmpv refuses to create a core unless `LC_NUMERIC` is `C`, and
/// `QGuiApplication` sets every category from the environment. Call it after
/// `QGuiApplication` exists and before the player is created.
pub fn force_c_numeric_locale() {
    // SAFETY: `setlocale` is called on the main thread before the threads that
    // format numbers (mpv, the Qt render thread) start; the argument is a static
    // NUL-terminated string.
    unsafe {
        libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr());
    }
}

/// The signals that ask Nikomiru to quit: a session logout, `kill`, Ctrl+C.
const TERMINATION: [libc::c_int; 3] = [libc::SIGTERM, libc::SIGINT, libc::SIGHUP];

/// Write end of the self-pipe, for the signal handler.
static SIGNAL_PIPE: AtomicI32 = AtomicI32::new(-1);

extern "C" fn on_termination_signal(signal: libc::c_int) {
    let byte = signal as u8;
    let fd = SIGNAL_PIPE.load(Ordering::Relaxed);
    // SAFETY: `write` is async-signal-safe; `byte` lives for the call. A full
    // pipe drops the byte, which is fine: one pending signal is enough.
    unsafe {
        libc::write(fd, (&raw const byte).cast(), 1);
    }
}

/// Termination signals arrive here, one byte each, written by the handler.
#[derive(Debug)]
pub struct Signals(File);

/// Routes the termination signals to a self-pipe, so that a thread can read
/// them with [`Signals::wait`]. Handlers, unlike a blocked signal mask, are
/// reset by `exec`: child processes (ffmpeg) keep the default behaviour.
pub fn catch_termination_signals() -> std::io::Result<Signals> {
    let mut fds = [0; 2];
    // SAFETY: `fds` has room for the two descriptors `pipe2` writes.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `pipe2` just returned this descriptor and nothing else owns it.
    let read = unsafe { File::from_raw_fd(fds[0]) };
    // the read end blocks, the write end must not (a signal handler cannot wait)
    // SAFETY: `fds[0]` is a valid descriptor owned by `read`.
    unsafe { libc::fcntl(fds[0], libc::F_SETFL, 0) };
    SIGNAL_PIPE.store(fds[1], Ordering::Relaxed);
    for signal in TERMINATION {
        // SAFETY: `action` is zeroed and then filled before use; the handler only
        // calls async-signal-safe functions.
        let rc = unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_termination_signal as *const () as usize;
            action.sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(signal, &action, std::ptr::null_mut())
        };
        if rc != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(Signals(read))
}

impl Signals {
    /// Waits for the next termination signal and returns its number.
    pub fn wait(&mut self) -> Option<i32> {
        let mut byte = [0u8; 1];
        loop {
            match self.0.read(&mut byte) {
                Ok(1) => return Some(i32::from(byte[0])),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                _ => return None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signals_reach_the_pipe_and_children_keep_the_defaults() {
        let mut signals = catch_termination_signals().unwrap();
        // SAFETY: sends SIGHUP to this process, whose handler was just installed.
        unsafe { libc::raise(libc::SIGHUP) };
        assert_eq!(signals.wait(), Some(libc::SIGHUP));
        // a child sees neither a blocked mask nor our handler: TERM kills it
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let mask = std::fs::read_to_string(format!("/proc/{}/status", child.id())).unwrap();
        let blocked = mask
            .lines()
            .find_map(|l| l.strip_prefix("SigBlk:"))
            .map(|v| u64::from_str_radix(v.trim(), 16).unwrap());
        assert_eq!(blocked, Some(0));
        // SAFETY: sends SIGTERM to our own child.
        unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
        let status = child.wait().unwrap();
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(status.signal(), Some(libc::SIGTERM));
    }
}
