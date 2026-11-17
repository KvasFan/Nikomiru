// SPDX-License-Identifier: GPL-3.0-only

//! Touch gestures (PORT-003): a single-finger horizontal swipe, read from
//! egui's touch events. Taps need nothing here: egui already turns them
//! into clicks.

use egui::{Event, Pos2, TouchId, TouchPhase};

/// The shortest swipe, in points.
pub const MIN_DISTANCE: f32 = 80.0;
/// The slowest swipe, in seconds.
pub const MAX_SECONDS: f64 = 0.8;

/// A gesture recognized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    /// The finger moved to the left.
    SwipeLeft,
    /// The finger moved to the right.
    SwipeRight,
}

/// Follows one touch at a time.
#[derive(Debug, Default)]
pub struct Tracker {
    /// The finger followed: its id, where and when it touched.
    down: Option<(TouchId, Pos2, f64)>,
    /// Where it is now.
    last: Pos2,
    /// Another finger touched: nothing is recognized until all are up.
    spoiled: bool,
    /// Fingers down.
    fingers: usize,
}

impl Tracker {
    /// Reads a frame's events at time `now` (seconds); a swipe when one ends.
    pub fn feed(&mut self, events: &[Event], now: f64) -> Option<Gesture> {
        let mut found = None;
        for event in events {
            let Event::Touch { id, phase, pos, .. } = event else {
                continue;
            };
            match phase {
                TouchPhase::Start => {
                    self.fingers += 1;
                    if self.fingers == 1 {
                        self.down = Some((*id, *pos, now));
                        self.last = *pos;
                        self.spoiled = false;
                    } else {
                        self.spoiled = true;
                    }
                }
                TouchPhase::Move => {
                    if self.down.is_some_and(|(d, _, _)| d == *id) {
                        self.last = *pos;
                    }
                }
                TouchPhase::End | TouchPhase::Cancel => {
                    self.fingers = self.fingers.saturating_sub(1);
                    let Some((d, start, at)) = self.down else {
                        continue;
                    };
                    if d != *id {
                        continue;
                    }
                    self.down = None;
                    let end = if *phase == TouchPhase::End {
                        *pos
                    } else {
                        continue;
                    };
                    let (dx, dy) = (end.x - start.x, end.y - start.y);
                    if !self.spoiled
                        && dx.abs() >= MIN_DISTANCE
                        && dx.abs() > 2.0 * dy.abs()
                        && now - at <= MAX_SECONDS
                    {
                        found = Some(if dx > 0.0 {
                            Gesture::SwipeRight
                        } else {
                            Gesture::SwipeLeft
                        });
                    }
                }
            }
        }
        if self.fingers == 0 {
            self.spoiled = false;
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(id: u64, phase: TouchPhase, x: f32, y: f32) -> Event {
        Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: TouchId(id),
            phase,
            pos: egui::pos2(x, y),
            force: None,
        }
    }

    // T-PORT-004: a quick horizontal swipe, in either direction.
    #[test]
    fn swipes_are_recognized() {
        let mut t = Tracker::default();
        assert_eq!(
            t.feed(&[touch(1, TouchPhase::Start, 300.0, 200.0)], 0.0),
            None
        );
        assert_eq!(
            t.feed(&[touch(1, TouchPhase::Move, 200.0, 210.0)], 0.1),
            None
        );
        assert_eq!(
            t.feed(&[touch(1, TouchPhase::End, 150.0, 215.0)], 0.2),
            Some(Gesture::SwipeLeft)
        );
        t.feed(&[touch(2, TouchPhase::Start, 100.0, 200.0)], 1.0);
        assert_eq!(
            t.feed(&[touch(2, TouchPhase::End, 250.0, 200.0)], 1.3),
            Some(Gesture::SwipeRight)
        );
    }

    // T-PORT-004: what is not a swipe.
    #[test]
    fn other_touches_are_not_swipes() {
        let mut t = Tracker::default();
        // too short
        t.feed(&[touch(1, TouchPhase::Start, 100.0, 100.0)], 0.0);
        assert_eq!(
            t.feed(&[touch(1, TouchPhase::End, 150.0, 100.0)], 0.1),
            None
        );
        // mostly vertical: a scroll
        t.feed(&[touch(1, TouchPhase::Start, 100.0, 100.0)], 1.0);
        assert_eq!(
            t.feed(&[touch(1, TouchPhase::End, 200.0, 300.0)], 1.1),
            None
        );
        // too slow
        t.feed(&[touch(1, TouchPhase::Start, 100.0, 100.0)], 2.0);
        assert_eq!(
            t.feed(&[touch(1, TouchPhase::End, 300.0, 100.0)], 3.0),
            None
        );
        // cancelled
        t.feed(&[touch(1, TouchPhase::Start, 100.0, 100.0)], 4.0);
        assert_eq!(
            t.feed(&[touch(1, TouchPhase::Cancel, 300.0, 100.0)], 4.1),
            None
        );
        // a second finger: a pinch, not a swipe
        t.feed(
            &[
                touch(1, TouchPhase::Start, 100.0, 100.0),
                touch(2, TouchPhase::Start, 120.0, 100.0),
            ],
            5.0,
        );
        assert_eq!(
            t.feed(
                &[
                    touch(1, TouchPhase::End, 300.0, 100.0),
                    touch(2, TouchPhase::End, 320.0, 100.0)
                ],
                5.2
            ),
            None
        );
        // and afterwards a swipe works again
        t.feed(&[touch(3, TouchPhase::Start, 300.0, 100.0)], 6.0);
        assert_eq!(
            t.feed(&[touch(3, TouchPhase::End, 100.0, 100.0)], 6.2),
            Some(Gesture::SwipeLeft)
        );
    }
}
