// SPDX-License-Identifier: GPL-3.0-only

//! Lookup on hover (POP-003) without Qt: with Shift held (or always, with
//! `lookup.hover_without_modifier`), a movement of at least 8 px over a new
//! character looks it up; a popup opened this way closes, and playback
//! resumes, when the pointer stays out of the subtitles and the popups for
//! 320 ms (`lookup.resume_on_leave`).

use std::time::Duration;

/// Movement that starts a hover lookup, in pixels.
pub const THRESHOLD_PX: f64 = 8.0;

/// Time outside the subtitles and the popups after which a hover popup closes.
pub const LEAVE: Duration = Duration::from_millis(320);

/// The hover state.
#[derive(Debug, Default)]
pub struct Hover {
    /// Where the pointer was at the last lookup (or when the modifier was
    /// first seen).
    anchor: Option<(f64, f64)>,
    /// The character looked up last: cue and byte offset.
    last: Option<(usize, usize)>,
    /// The open popup came from a hover.
    opened: bool,
    outside_since: Option<Duration>,
}

impl Hover {
    /// The pointer moved to `(x, y)` over character `target`; `armed` is
    /// Shift held or hover without modifier. True: look `target` up.
    pub fn moved(&mut self, x: f64, y: f64, armed: bool, target: (usize, usize)) -> bool {
        self.outside_since = None;
        if !armed {
            self.anchor = None;
            return false;
        }
        let far = self
            .anchor
            .is_none_or(|(ax, ay)| (x - ax).hypot(y - ay) >= THRESHOLD_PX);
        if !far || self.last == Some(target) {
            return false;
        }
        self.anchor = Some((x, y));
        self.last = Some(target);
        self.opened = true;
        true
    }

    /// The pointer is over the subtitles or a popup (`true`) or elsewhere.
    pub fn inside(&mut self, inside: bool, now: Duration) {
        if inside {
            self.outside_since = None;
        } else if self.opened && self.outside_since.is_none() {
            self.outside_since = Some(now);
        }
    }

    /// True once: the hover popup must close (the pointer left long enough).
    pub fn expired(&mut self, now: Duration, resume_on_leave: bool) -> bool {
        let gone = self.outside_since.is_some_and(|t| now >= t + LEAVE);
        if resume_on_leave && self.opened && gone {
            self.reset();
            return true;
        }
        false
    }

    /// The popups were closed or opened by a click: hover forgets them.
    pub fn reset(&mut self) {
        *self = Hover::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    // T-POP-003
    #[test]
    fn shift_movement_looks_up_once_per_character() {
        let mut h = Hover::default();
        assert!(!h.moved(10.0, 10.0, false, (0, 0)), "no Shift, no lookup");
        assert!(
            h.moved(10.0, 10.0, true, (0, 0)),
            "Shift: the first character"
        );
        assert!(!h.moved(14.0, 10.0, true, (0, 3)), "less than 8 px");
        assert!(
            h.moved(19.0, 10.0, true, (0, 3)),
            "8 px or more, another character"
        );
        assert!(
            !h.moved(40.0, 10.0, true, (0, 3)),
            "the same character again"
        );
        assert!(h.moved(60.0, 10.0, true, (0, 6)));
    }

    // T-POP-003
    #[test]
    fn leaving_for_320_ms_closes_a_hover_popup() {
        let mut h = Hover::default();
        h.moved(0.0, 0.0, true, (0, 0));
        h.inside(false, ms(1000));
        assert!(!h.expired(ms(1200), true));
        h.inside(true, ms(1250));
        h.inside(false, ms(1300));
        assert!(
            !h.expired(ms(1500), true),
            "the clock restarts after coming back"
        );
        assert!(h.expired(ms(1620), true));
        assert!(!h.expired(ms(5000), true), "only once");

        // resume_on_leave off: it stays
        let mut h = Hover::default();
        h.moved(0.0, 0.0, true, (0, 0));
        h.inside(false, ms(0));
        assert!(!h.expired(ms(10_000), false));

        // a popup opened by a click is not a hover popup
        let mut h = Hover::default();
        h.inside(false, ms(0));
        assert!(!h.expired(ms(10_000), true));
    }
}
