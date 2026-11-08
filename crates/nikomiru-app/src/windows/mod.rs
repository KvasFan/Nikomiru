// SPDX-License-Identifier: GPL-3.0-only

//! What the `linux` module does, on Windows (ADR 0009): the code pages of
//! Windows for the subtitle encodings, the local time zone, the light or
//! dark setting of the system. `unsafe` is allowed here (ARC-011): the Win32
//! calls, each with its `SAFETY` comment.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::{ERROR_SUCCESS, FILETIME, SYSTEMTIME};
use windows_sys::Win32::Globalization::{MB_ERR_INVALID_CHARS, MultiByteToWideChar};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::System::Time::{
    FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTime,
};

/// `text` as a NUL-terminated UTF-16 string, for the `W` calls of Win32.
pub(crate) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A UTF-16 buffer filled by Win32, up to its first NUL.
pub(crate) fn from_wide(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&u| u == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// The space free for this user on the volume of `path`, in bytes
/// (INS-004); `None` when it cannot be read.
pub fn free_space(path: &std::path::Path) -> Option<u64> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let name = wide(&path.to_string_lossy());
    let mut free = 0u64;
    // SAFETY: `name` is a NUL-terminated UTF-16 path we own; the call writes
    // only `free`, and the other outputs may be null.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            name.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(free)
}

/// No launcher token on Windows.
pub fn take_activation_token() -> Option<String> {
    None
}

/// Rust and mpv format numbers themselves: nothing to set.
pub fn force_c_numeric_locale() {}

/// The allocator of Windows gives memory back by itself.
pub fn release_free_memory() {}

/// A GUI program on Windows gets no termination signals: closing the window
/// is the way out, and it saves the position as on Linux (LIB-006).
#[derive(Debug)]
pub struct Signals;

/// Always unsupported: see [`Signals`].
pub fn catch_termination_signals() -> std::io::Result<Signals> {
    Err(std::io::ErrorKind::Unsupported.into())
}

impl Signals {
    /// Never a signal.
    pub fn wait(&mut self) -> Option<i32> {
        None
    }
}

/// The Windows code page of an encoding named as iconv names it (SUB-005,
/// and the encodings web pages declare).
fn code_page(encoding: &str) -> Option<u32> {
    Some(
        match encoding.to_ascii_uppercase().replace('_', "-").as_str() {
            "CP932" | "SHIFT-JIS" | "SJIS" | "WINDOWS-31J" | "MS932" => 932,
            "EUC-JP" => 20932,
            "GB18030" => 54936,
            "GBK" | "GB2312" | "CP936" => 936,
            "BIG5" | "CP950" => 950,
            "EUC-KR" | "CP949" => 949,
            "CP1252" | "WINDOWS-1252" => 1252,
            "ISO-8859-1" | "LATIN1" => 28591,
            _ => return None,
        },
    )
}

/// The subtitle decoder of SUB-005: Windows's own code pages, as iconv is on
/// Linux.
#[derive(Debug, Clone, Copy, Default)]
pub struct Iconv;

impl nikomiru_subs::encoding::Decoder for Iconv {
    fn decode(&self, bytes: &[u8], encoding: &str) -> Option<String> {
        let page = code_page(encoding)?;
        if bytes.is_empty() {
            return Some(String::new());
        }
        let len = i32::try_from(bytes.len()).ok()?;
        // SAFETY: `bytes` is valid for `len` bytes; a null output with size 0
        // asks only for the length needed.
        let needed = unsafe {
            MultiByteToWideChar(
                page,
                MB_ERR_INVALID_CHARS,
                bytes.as_ptr(),
                len,
                std::ptr::null_mut(),
                0,
            )
        };
        if needed <= 0 {
            return None;
        }
        let mut out = vec![0u16; usize::try_from(needed).ok()?];
        // SAFETY: `out` has room for the `needed` units just asked for.
        let written = unsafe {
            MultiByteToWideChar(
                page,
                MB_ERR_INVALID_CHARS,
                bytes.as_ptr(),
                len,
                out.as_mut_ptr(),
                needed,
            )
        };
        if written != needed {
            return None;
        }
        String::from_utf16(&out).ok()
    }
}

/// Seconds between 1601-01-01, where `FILETIME` counts from, and 1970.
const FILETIME_EPOCH: u64 = 11_644_473_600;

fn filetime(ticks: u64) -> FILETIME {
    FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    }
}

fn ticks(time: &FILETIME) -> u64 {
    u64::from(time.dwHighDateTime) << 32 | u64::from(time.dwLowDateTime)
}

/// How far the local time is east of UTC at `at`, in seconds (STA-004), from
/// the time zone of Windows with its daylight saving; 0 when unknown.
pub fn utc_offset(at: SystemTime) -> i32 {
    let secs = at.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let utc = filetime((secs + FILETIME_EPOCH) * 10_000_000);
    let mut utc_parts = SYSTEMTIME::default();
    let mut local_parts = SYSTEMTIME::default();
    let mut local = FILETIME::default();
    // SAFETY: each call reads and writes only the structures given, owned
    // here; a null time zone means the current one.
    let done = unsafe {
        FileTimeToSystemTime(&utc, &mut utc_parts) != 0
            && SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc_parts, &mut local_parts) != 0
            && SystemTimeToFileTime(&local_parts, &mut local) != 0
    };
    if !done {
        return 0;
    }
    let difference = ticks(&local) as i128 - ticks(&utc) as i128;
    i32::try_from(difference / 10_000_000).unwrap_or(0)
}

/// The system clock, with the local time zone (STA-004).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalClock(nikomiru_core::SystemClock);

impl nikomiru_core::Clock for LocalClock {
    fn monotonic(&self) -> Duration {
        self.0.monotonic()
    }

    fn now(&self) -> SystemTime {
        self.0.now()
    }

    fn utc_offset(&self, at: SystemTime) -> i32 {
        utc_offset(at)
    }
}

/// Whether the apps of this user are dark: `AppsUseLightTheme` is 0;
/// `None` when the setting is not there (Windows before 10).
fn system_dark() -> Option<bool> {
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let value = wide("AppsUseLightTheme");
    let mut light: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: the strings are NUL-terminated and live through the call; the
    // value is a DWORD, written into `light` of `size` bytes.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut light).cast(),
            &mut size,
        )
    };
    (status == ERROR_SUCCESS).then_some(light == 0)
}

/// SET-003 on Windows: tells `on_change` whether the system is dark, now
/// and whenever the setting changes, read every 2 s; nothing if Windows has
/// no such setting.
pub fn follow_theme(on_change: impl Fn(bool) + Send + 'static) {
    let spawned = std::thread::Builder::new()
        .name("theme".into())
        .spawn(move || {
            let mut last = None;
            loop {
                let now = system_dark();
                if now.is_some() && now != last {
                    on_change(now == Some(true));
                }
                if now.is_none() && last.is_none() {
                    return;
                }
                last = now;
                std::thread::sleep(Duration::from_secs(2));
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("theme thread: {e}");
    }
}

/// Drag and drop: winit reports dropped files on Windows itself
/// (`WindowEvent::DroppedFile`).
pub mod dnd {
    use super::{Arc, PathBuf};
    use winit::window::Window;

    /// What a drop calls with its files.
    pub type OnDrop = Arc<dyn Fn(Vec<PathBuf>) + Send + Sync>;

    /// Nothing to listen to beside winit.
    #[derive(Debug)]
    pub struct Drops;

    impl Drops {
        /// Always `None`.
        pub fn new(_window: &Window, _on_drop: OnDrop) -> Option<Self> {
            None
        }

        /// Nothing to dispatch.
        pub fn dispatch(&mut self) {}
    }

    /// Nothing to release; the shell drops it before the event loop, as on
    /// Linux.
    impl Drop for Drops {
        fn drop(&mut self) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nikomiru_subs::encoding::Decoder;

    #[test]
    fn the_legacy_encodings_decode() {
        // あ in Shift_JIS, EUC-JP; é in CP1252
        assert_eq!(Iconv.decode(b"\x82\xa0", "CP932").as_deref(), Some("あ"));
        assert_eq!(Iconv.decode(b"\xa4\xa2", "EUC-JP").as_deref(), Some("あ"));
        assert_eq!(Iconv.decode(b"\xe9", "CP1252").as_deref(), Some("é"));
        assert_eq!(Iconv.decode(b"\x82", "CP932"), None, "half a character");
        assert_eq!(Iconv.decode(b"a", "KLINGON"), None);
    }

    #[test]
    fn the_offset_is_whole_quarter_hours() {
        let offset = utc_offset(SystemTime::now());
        assert_eq!(offset % 900, 0, "{offset}");
        assert!(offset.abs() <= 14 * 3600);
    }
}
