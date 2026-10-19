// SPDX-License-Identifier: GPL-3.0-only

//! When to save the playback position (LIB-006): at most once per second, only
//! after the file is open and the resume seek has happened; immediately on exit
//! and on episode change.

use std::time::Duration;

/// Minimum time between two saves during playback.
pub const SAVE_INTERVAL: Duration = Duration::from_secs(1);

/// Decides when the position of the current file must be written.
#[derive(Debug, Default, Clone)]
pub struct ProgressSaver {
    ready: bool,
    last_save: Option<Duration>,
    pending: Option<u64>,
}

impl ProgressSaver {
    /// A new file starts loading: nothing is saved until [`Self::ready`].
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The file is open and the resume position has been applied.
    pub fn ready(&mut self) {
        self.ready = true;
    }

    /// A new position (ms) at monotonic time `now`: the position to save now, if any.
    pub fn position(&mut self, position_ms: u64, now: Duration) -> Option<u64> {
        if !self.ready {
            return None;
        }
        self.pending = Some(position_ms);
        if self.last_save.is_some_and(|t| now < t + SAVE_INTERVAL) {
            return None;
        }
        self.last_save = Some(now);
        self.pending.take()
    }

    /// Exit or episode change: the last unsaved position, to write right away.
    pub fn flush(&mut self) -> Option<u64> {
        if !self.ready {
            return None;
        }
        self.pending.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-LIB-006
    #[test]
    fn at_most_one_save_per_second_and_only_after_resume() {
        let mut s = ProgressSaver::default();
        let ms = Duration::from_millis;
        assert_eq!(s.position(5_000, ms(0)), None, "before the resume seek");
        assert_eq!(s.flush(), None);
        s.ready();
        let mut saved = Vec::new();
        // 30 position updates per second for 3 seconds
        for i in 0..90u64 {
            if let Some(p) = s.position(5_000 + i * 33, ms(i * 33)) {
                saved.push(p);
            }
        }
        assert_eq!(saved.len(), 3, "{saved:?}");
        // the last position is written on exit even if a second has not passed
        assert_eq!(s.flush(), Some(5_000 + 89 * 33));
        assert_eq!(s.flush(), None);
        s.reset();
        assert_eq!(
            s.position(0, ms(10_000)),
            None,
            "new file: wait for the resume"
        );
    }
}
