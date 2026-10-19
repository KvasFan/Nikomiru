// SPDX-License-Identifier: GPL-3.0-only

//! When the screensaver is inhibited (PLY-013): during playback, and for one
//! more minute after a pause, so that a short pause does not blank the screen.

use std::time::Duration;

/// How long a pause keeps the inhibition.
pub const RELEASE_AFTER: Duration = Duration::from_secs(60);

/// The inhibition policy.
#[derive(Debug, Clone, Default)]
pub struct ScreensaverPolicy {
    playing: bool,
    stopped_at: Option<Duration>,
}

impl ScreensaverPolicy {
    /// Whether a video is playing now (loaded and not paused).
    pub fn set_playing(&mut self, playing: bool, now: Duration) {
        if playing {
            self.stopped_at = None;
        } else if self.playing {
            self.stopped_at = Some(now);
        }
        self.playing = playing;
    }

    /// Whether the screensaver must be inhibited at `now`.
    pub fn inhibit(&self, now: Duration) -> bool {
        self.playing
            || self
                .stopped_at
                .is_some_and(|t| now.saturating_sub(t) < RELEASE_AFTER)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-PLY-013
    #[test]
    fn inhibited_while_playing_and_released_after_a_minute_of_pause() {
        let s = Duration::from_secs;
        let mut p = ScreensaverPolicy::default();
        assert!(!p.inhibit(s(0)), "nothing loaded yet");
        p.set_playing(true, s(1));
        assert!(p.inhibit(s(500)));
        p.set_playing(false, s(600));
        assert!(p.inhibit(s(659)), "a short pause keeps the screen on");
        assert!(!p.inhibit(s(660)));
        // repeated "paused" updates do not restart the minute
        p.set_playing(false, s(650));
        assert!(!p.inhibit(s(661)));
        p.set_playing(true, s(700));
        assert!(p.inhibit(s(10_000)));
    }
}
