// SPDX-License-Identifier: GPL-3.0-only

//! What needs `unsafe` in the application for the system (ARC-011): the
//! environment at startup, the numeric locale libmpv requires, the termination
//! signals, glibc's `iconv` for subtitle encodings (SUB-005), and drag and drop
//! on Wayland ([`dnd`]).

pub mod dnd;

use std::fs::File;
use std::io::Read;
use std::os::fd::FromRawFd;
use std::sync::atomic::{AtomicI32, Ordering};

/// Takes the launcher's startup-notification token out of the environment
/// (`XDG_ACTIVATION_TOKEN` on Wayland, `DESKTOP_STARTUP_ID` on X11): the window
/// uses it to come forward, and the processes Nikomiru starts (ffmpeg) must not
/// inherit it. MUST run before any other thread exists, because changing the
/// environment is not thread-safe.
pub fn take_activation_token() -> Option<String> {
    let token = std::env::var("XDG_ACTIVATION_TOKEN")
        .or_else(|_| std::env::var("DESKTOP_STARTUP_ID"))
        .ok()
        .filter(|t| !t.is_empty());
    // SAFETY: called at the start of `main`, before any thread is spawned, so
    // nothing reads the environment concurrently.
    unsafe {
        std::env::remove_var("XDG_ACTIVATION_TOKEN");
        std::env::remove_var("DESKTOP_STARTUP_ID");
    }
    token
}

/// libmpv refuses to create a core unless `LC_NUMERIC` is `C`; on X11 winit sets
/// `LC_CTYPE` when its event loop starts. Call it after the event loop exists and
/// before the player is created.
pub fn force_c_numeric_locale() {
    // SAFETY: `setlocale` is called on the main thread before the threads that
    // format numbers (mpv) start; the argument is a static NUL-terminated string.
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

/// glibc `iconv` as the subtitle decoder of SUB-005.
#[derive(Debug, Clone, Copy, Default)]
pub struct Iconv;

impl nikomiru_subs::encoding::Decoder for Iconv {
    fn decode(&self, bytes: &[u8], encoding: &str) -> Option<String> {
        let from = std::ffi::CString::new(encoding).ok()?;
        // UTF-8 needs at most 4 bytes for any input character
        let mut out = vec![0u8; bytes.len() * 4 + 16];
        let mut in_ptr = bytes.as_ptr().cast_mut().cast::<libc::c_char>();
        let mut in_left = bytes.len();
        let mut out_ptr = out.as_mut_ptr().cast::<libc::c_char>();
        let mut out_left = out.len();
        // SAFETY: the conversion descriptor is checked before use and closed on
        // every path; iconv reads at most `in_left` bytes from `bytes` (it does
        // not write through the input pointer) and writes at most `out_left`
        // bytes into `out`, updating the pointers and counts it is given.
        let ok = unsafe {
            let cd = libc::iconv_open(c"UTF-8".as_ptr(), from.as_ptr());
            if cd as isize == -1 {
                return None;
            }
            let rc = libc::iconv(cd, &mut in_ptr, &mut in_left, &mut out_ptr, &mut out_left);
            let flushed = libc::iconv(
                cd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut out_ptr,
                &mut out_left,
            );
            libc::iconv_close(cd);
            rc != usize::MAX && flushed != usize::MAX && in_left == 0
        };
        if !ok {
            return None;
        }
        out.truncate(out.len() - out_left);
        String::from_utf8(out).ok()
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

/// How far the local time is east of UTC at `at`, in seconds (STA-004),
/// from the C library's time zone (`TZ`, `/etc/localtime`); 0 when unknown.
pub fn utc_offset(at: std::time::SystemTime) -> i32 {
    let secs = at
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as libc::time_t;
    let mut tm = std::mem::MaybeUninit::<libc::tm>::zeroed();
    // SAFETY: `localtime_r` writes only the `tm` given, from a `time_t` we
    // own; it is the reentrant variant, safe with other threads.
    let done = unsafe { libc::localtime_r(&secs, tm.as_mut_ptr()) };
    if done.is_null() {
        return 0;
    }
    // SAFETY: `localtime_r` succeeded, so it filled `tm`.
    let tm = unsafe { tm.assume_init() };
    i32::try_from(tm.tm_gmtoff).unwrap_or(0)
}

/// The system clock, with the local time zone (STA-004).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalClock(nikomiru_core::SystemClock);

impl nikomiru_core::Clock for LocalClock {
    fn monotonic(&self) -> std::time::Duration {
        self.0.monotonic()
    }

    fn now(&self) -> std::time::SystemTime {
        self.0.now()
    }

    fn utc_offset(&self, at: std::time::SystemTime) -> i32 {
        utc_offset(at)
    }
}

#[cfg(test)]
mod time_tests {
    use super::*;

    // T-STA-004: the C library's zone, here forced to one without daylight saving
    #[test]
    fn the_local_offset_is_the_time_zone_s() {
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_792_970_000);
        let offset = utc_offset(at);
        assert!((-14 * 3600..=14 * 3600).contains(&offset), "{offset}");
        assert_eq!(offset % 900, 0, "zones move by quarters of an hour");
    }
}
