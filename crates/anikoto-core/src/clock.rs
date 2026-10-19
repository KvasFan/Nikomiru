// SPDX-License-Identifier: GPL-3.0-only

//! Injectable clock (TEST §1): code that depends on time takes a [`Clock`], so
//! tests drive it with [`ManualClock`] instead of sleeping.

use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

/// A source of time.
pub trait Clock: Send + Sync {
    /// Monotonic time since an arbitrary, fixed origin.
    fn monotonic(&self) -> Duration;
    /// Wall-clock time (for timestamps stored on disk).
    fn now(&self) -> SystemTime;
}

/// The real clock.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    /// A clock whose monotonic origin is now.
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn monotonic(&self) -> Duration {
        self.origin.elapsed()
    }

    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// A clock that only moves when told to, for tests.
#[derive(Debug)]
pub struct ManualClock {
    state: Mutex<(Duration, SystemTime)>,
}

impl ManualClock {
    /// Starts at monotonic zero and at the given wall-clock time.
    pub fn new(wall: SystemTime) -> Self {
        Self {
            state: Mutex::new((Duration::ZERO, wall)),
        }
    }

    /// Moves both clocks forward.
    pub fn advance(&self, by: Duration) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.0 += by;
        s.1 += by;
    }
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new(SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000))
    }
}

impl Clock for ManualClock {
    fn monotonic(&self) -> Duration {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).0
    }

    fn now(&self) -> SystemTime {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_clock_moves_only_when_advanced() {
        let c = ManualClock::default();
        let (m0, w0) = (c.monotonic(), c.now());
        assert_eq!(c.monotonic(), m0);
        c.advance(Duration::from_millis(1500));
        assert_eq!(c.monotonic() - m0, Duration::from_millis(1500));
        assert_eq!(
            c.now().duration_since(w0).ok(),
            Some(Duration::from_millis(1500))
        );
    }

    #[test]
    fn system_clock_is_monotonic() {
        let c = SystemClock::new();
        let a = c.monotonic();
        assert!(c.monotonic() >= a);
    }
}
