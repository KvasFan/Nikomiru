// SPDX-License-Identifier: GPL-3.0-only

//! Visibility of the control bar (PLY-004): it hides after 2 s without mouse
//! movement during playback, together with the cursor, and comes back on
//! movement. It stays visible while paused, with the mouse over the subtitles
//! and with a popup open. Only the mouse moving onto the subtitles counts:
//! the bar lifts the bottom subtitles, and subtitles that come back under a
//! resting pointer when it hides must not bring it back, every 2 s.

use std::time::Duration;

/// Time without movement after which the bar hides.
pub const HIDE_AFTER: Duration = Duration::from_secs(2);
/// The pointer entered the subtitles by moving if it moved this recently.
const MOVED_ONTO: Duration = Duration::from_millis(250);

/// State of the control bar.
#[derive(Debug, Clone, Default)]
pub struct ControlsVisibility {
    last_motion: Duration,
    playing: bool,
    over_subtitles: bool,
    popup_open: bool,
}

impl ControlsVisibility {
    /// The mouse moved at monotonic time `now`.
    pub fn mouse_moved(&mut self, now: Duration) {
        self.last_motion = now;
    }

    /// Playback started or paused.
    pub fn set_playing(&mut self, playing: bool, now: Duration) {
        if playing && !self.playing {
            // the 2 s count from the moment playback starts
            self.last_motion = now;
        }
        self.playing = playing;
    }

    /// The mouse is over the subtitles, or left them, at `now`.
    pub fn set_over_subtitles(&mut self, over: bool, now: Duration) {
        self.over_subtitles = over && now.saturating_sub(self.last_motion) < MOVED_ONTO;
    }

    /// A popup opened or the last one closed.
    pub fn set_popup_open(&mut self, open: bool) {
        self.popup_open = open;
    }

    /// Whether the bar (and the cursor) is visible at `now`.
    pub fn visible(&self, now: Duration) -> bool {
        !self.playing
            || self.over_subtitles
            || self.popup_open
            || now.saturating_sub(self.last_motion) < HIDE_AFTER
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-PLY-004
    #[test]
    fn hides_after_two_seconds_of_playback_without_movement() {
        let s = Duration::from_secs;
        let ms = Duration::from_millis;
        let mut c = ControlsVisibility::default();
        assert!(c.visible(s(100)), "paused: always visible");
        c.set_playing(true, s(0));
        assert!(c.visible(ms(1999)));
        assert!(!c.visible(ms(2000)));
        c.mouse_moved(s(3));
        assert!(c.visible(ms(4999)));
        assert!(!c.visible(s(5)));
        c.mouse_moved(ms(59_900));
        c.set_over_subtitles(true, s(60));
        assert!(c.visible(s(90)), "mouse over the subtitles");
        c.set_over_subtitles(false, s(90));
        assert!(!c.visible(s(90)));
        // subtitles that come under a resting pointer do not count
        c.set_over_subtitles(true, s(95));
        assert!(!c.visible(s(95)));
        c.set_popup_open(true);
        assert!(c.visible(s(60)), "popup open");
        c.set_popup_open(false);
        c.set_playing(false, s(60));
        assert!(c.visible(s(120)), "paused again");
    }
}
