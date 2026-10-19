// SPDX-License-Identifier: GPL-3.0-only

//! Pure logic of the player controls: limits of PLY-001, "hold to speed up"
//! (PLY-002), load tokens (ARC-002) and the `time-pos` rate limit (ARC-004).

use std::time::Duration;

/// Slowest speed (PLY-001).
pub const SPEED_MIN: f64 = 0.25;
/// Fastest speed (PLY-001).
pub const SPEED_MAX: f64 = 4.0;
/// Speed step (PLY-001).
pub const SPEED_STEP: f64 = 0.1;
/// Maximum volume, in percent (PLY-001).
pub const VOLUME_MAX: f64 = 150.0;
/// Volume step (PLY-001).
pub const VOLUME_STEP: f64 = 5.0;
/// Speed while the "hold to speed up" key is held (PLY-002).
pub const HOLD_SPEED: f64 = 2.0;

/// `speed` clamped to 0.25–4.0 and rounded to hundredths.
pub fn clamp_speed(speed: f64) -> f64 {
    let s = if speed.is_finite() { speed } else { 1.0 };
    (s.clamp(SPEED_MIN, SPEED_MAX) * 100.0).round() / 100.0
}

/// The speed after `steps` presses of speed up (positive) or down (negative).
pub fn step_speed(current: f64, steps: i32) -> f64 {
    clamp_speed(current + SPEED_STEP * f64::from(steps))
}

/// `volume` clamped to 0–150.
pub fn clamp_volume(volume: f64) -> f64 {
    if volume.is_finite() {
        volume.clamp(0.0, VOLUME_MAX)
    } else {
        100.0
    }
}

/// The volume after `steps` presses of volume up or down.
pub fn step_volume(current: f64, steps: i32) -> f64 {
    clamp_volume(current + VOLUME_STEP * f64::from(steps))
}

/// Seek step in seconds, 1–30 (PLY-001).
pub fn seek_step(seconds: u8) -> f64 {
    f64::from(seconds.clamp(1, 30))
}

/// "Hold to speed up" (PLY-002): 2× while the key is held, then back to the
/// speed in use before. Auto-repeated key presses are ignored.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct HoldToSpeed {
    saved: Option<f64>,
}

impl HoldToSpeed {
    /// Key pressed: the speed to set, if any.
    pub fn key_down(&mut self, current_speed: f64, auto_repeat: bool) -> Option<f64> {
        if auto_repeat || self.saved.is_some() {
            return None;
        }
        self.saved = Some(current_speed);
        Some(HOLD_SPEED)
    }

    /// Key released: the speed to restore, if the key was held.
    pub fn key_up(&mut self) -> Option<f64> {
        self.saved.take()
    }

    /// Whether the key is currently held.
    pub fn active(&self) -> bool {
        self.saved.is_some()
    }
}

/// Identifies one `loadfile` (ARC-002). Events and asynchronous results carry
/// the token of the load they belong to; those of an older load are discarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct LoadToken(pub u64);

impl LoadToken {
    /// The token of the next load.
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// A value tagged with the load it belongs to.
#[derive(Debug, Clone, PartialEq)]
pub struct Stamped<T> {
    /// The load.
    pub token: LoadToken,
    /// The value.
    pub value: T,
}

impl<T> Stamped<T> {
    /// The value, if it belongs to the `current` load.
    pub fn current(self, current: LoadToken) -> Option<T> {
        (self.token == current).then_some(self.value)
    }
}

/// Rate limit for `time-pos` towards the UI (ARC-004): at most `max_per_second`
/// updates, and the latest value is always delivered.
#[derive(Debug, Clone)]
pub struct Throttle {
    interval: Duration,
    last_emit: Option<Duration>,
    pending: Option<f64>,
}

impl Throttle {
    /// A limit of `max_per_second` updates per second.
    pub fn new(max_per_second: u32) -> Self {
        Self {
            interval: Duration::from_secs(1) / max_per_second.max(1),
            last_emit: None,
            pending: None,
        }
    }

    /// A new value at monotonic time `now`: returns it if it may go out now,
    /// otherwise keeps it for [`Throttle::flush`].
    pub fn push(&mut self, value: f64, now: Duration) -> Option<f64> {
        self.pending = Some(value);
        self.flush(now)
    }

    /// Delivers the pending value if the interval has passed.
    pub fn flush(&mut self, now: Duration) -> Option<f64> {
        let due = self.last_emit.is_none_or(|t| now >= t + self.interval);
        if due && let Some(v) = self.pending.take() {
            self.last_emit = Some(now);
            return Some(v);
        }
        None
    }

    /// When the pending value becomes due, if there is one.
    pub fn deadline(&self) -> Option<Duration> {
        self.pending?;
        Some(self.last_emit.map_or(Duration::ZERO, |t| t + self.interval))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-PLY-001
    #[test]
    fn limits_of_speed_volume_and_seek() {
        assert_eq!(step_speed(1.0, 1), 1.1);
        assert_eq!(step_speed(3.95, 1), 4.0);
        assert_eq!(step_speed(0.3, -1), 0.25);
        assert_eq!(clamp_speed(f64::NAN), 1.0);
        assert_eq!(step_volume(100.0, 1), 105.0);
        assert_eq!(step_volume(148.0, 1), 150.0);
        assert_eq!(step_volume(2.0, -1), 0.0);
        assert_eq!(seek_step(0), 1.0);
        assert_eq!(seek_step(3), 3.0);
        assert_eq!(seek_step(90), 30.0);
    }

    // T-PLY-002
    #[test]
    fn hold_to_speed_up() {
        let mut h = HoldToSpeed::default();
        assert_eq!(h.key_down(1.25, false), Some(2.0));
        assert!(h.active());
        assert_eq!(h.key_down(2.0, true), None, "auto-repeat is ignored");
        assert_eq!(h.key_down(2.0, false), None, "already held");
        assert_eq!(h.key_up(), Some(1.25));
        assert_eq!(h.key_up(), None);
        assert!(!h.active());
    }

    // T-ARC-002
    #[test]
    fn events_of_an_older_load_are_discarded() {
        let first = LoadToken::default().next();
        let second = first.next();
        let stale = Stamped {
            token: first,
            value: 12.5,
        };
        let fresh = Stamped {
            token: second,
            value: 0.5,
        };
        assert_eq!(stale.current(second), None);
        assert_eq!(fresh.current(second), Some(0.5));
    }

    // T-ARC-004
    #[test]
    fn time_pos_is_limited_to_30_per_second_and_the_last_value_arrives() {
        let mut t = Throttle::new(30);
        let mut delivered = Vec::new();
        // 240 updates per second for one second
        for i in 0..240u32 {
            let now = Duration::from_micros(u64::from(i) * 1_000_000 / 240);
            if let Some(v) = t.push(f64::from(i), now) {
                delivered.push(v);
            }
        }
        assert!((20..=30).contains(&delivered.len()), "{}", delivered.len());
        // the last value is still pending and comes out at its deadline
        let deadline = t.deadline().unwrap();
        assert_eq!(t.flush(deadline), Some(239.0));
        assert_eq!(t.deadline(), None);
    }
}
