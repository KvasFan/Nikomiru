// SPDX-License-Identifier: GPL-3.0-only

//! What the `linux` module does, on Windows (spike S6, §5.2.4): stand-ins
//! that let the program start, each saying what a port still has to do.

use std::path::PathBuf;
use std::sync::Arc;

/// No launcher token on Windows.
pub fn take_activation_token() -> Option<String> {
    None
}

/// Rust and mpv format numbers themselves: nothing to set.
pub fn force_c_numeric_locale() {}

/// The allocator of Windows gives memory back by itself.
pub fn release_free_memory() {}

/// Console control events would take the place of the signals; not yet.
#[derive(Debug)]
pub struct Signals;

/// Not on Windows yet: closing the window is the way out.
pub fn catch_termination_signals() -> std::io::Result<Signals> {
    Err(std::io::ErrorKind::Unsupported.into())
}

impl Signals {
    /// Never a signal.
    pub fn wait(&mut self) -> Option<i32> {
        None
    }
}

/// The subtitle decoder of SUB-005: only UTF-8 until a port brings a
/// decoder for the legacy encodings (Shift_JIS, EUC-JP, …).
#[derive(Debug, Clone, Copy, Default)]
pub struct Iconv;

impl nikomiru_subs::encoding::Decoder for Iconv {
    fn decode(&self, bytes: &[u8], encoding: &str) -> Option<String> {
        encoding
            .eq_ignore_ascii_case("UTF-8")
            .then(|| String::from_utf8(bytes.to_vec()).ok())
            .flatten()
    }
}

/// The system clock; the local time zone is not read yet, so the
/// statistics count days in UTC (STA-004).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalClock(nikomiru_core::SystemClock);

impl nikomiru_core::Clock for LocalClock {
    fn monotonic(&self) -> std::time::Duration {
        self.0.monotonic()
    }

    fn now(&self) -> std::time::SystemTime {
        self.0.now()
    }

    fn utc_offset(&self, _at: std::time::SystemTime) -> i32 {
        0
    }
}

/// Drag and drop: winit reports dropped files on Windows itself.
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
}
