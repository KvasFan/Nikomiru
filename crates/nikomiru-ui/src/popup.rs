// SPDX-License-Identifier: GPL-3.0-only

//! The dictionary popups, apart from their drawing: the stack of nested
//! popups (POP-012), the pause and resume of playback around them (POP-001,
//! POP-004, POP-005) and where a popup goes on screen (POP-006).

use std::sync::Arc;

use crate::popup_view::Content;

/// Nested popups at most (POP-012).
pub const MAX_DEPTH: usize = 8;

/// What the session must do after a popup operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Pause playback (the lookup paused it).
    Pause,
    /// Resume playback: the lookup had paused it and every popup is closed.
    Resume,
    /// Look `text` up; the reply carries `generation`.
    Lookup {
        /// Matches the reply to its popup.
        generation: u64,
        /// The query.
        text: String,
    },
}

/// Where a popup's query comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A subtitle: the cue and the byte where the query starts.
    Subtitle {
        /// Cue index.
        cue: usize,
        /// Byte offset of the query in the cue's text.
        start: usize,
    },
    /// Text inside the popup below (a link or a selection).
    Glossary,
}

/// A rectangle in window coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// Left.
    pub x: f64,
    /// Top.
    pub y: f64,
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
}

impl Rect {
    fn bottom(&self) -> f64 {
        self.y + self.h
    }

    fn right(&self) -> f64 {
        self.x + self.w
    }

    /// Whether the two rectangles overlap (touching edges do not).
    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
}

/// The reply to a lookup.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reply {
    /// What the popup shows, built on the lookup thread.
    pub content: Arc<Content>,
    /// Characters of the query that matched (POP-002).
    pub best_length: usize,
    /// No entry was found.
    pub empty: bool,
    /// Entries shown, for the keyboard (POP-014).
    pub entries: usize,
}

/// One popup.
#[derive(Debug, Clone, PartialEq)]
pub struct Popup {
    /// The query.
    pub query: String,
    /// Where it comes from.
    pub origin: Origin,
    /// The rectangle of the looked-up text.
    pub anchor: Rect,
    /// The lookup that fills it.
    pub generation: u64,
    /// Its content, once the lookup answers.
    pub reply: Option<Reply>,
    /// The entry the keyboard is on (POP-014).
    pub entry: usize,
}

/// The open popups, root first.
#[derive(Debug, Default)]
pub struct Popups {
    stack: Vec<Popup>,
    paused_for_lookup: bool,
    generation: u64,
}

impl Popups {
    /// The open popups, root first.
    pub fn stack(&self) -> &[Popup] {
        &self.stack
    }

    /// Whether any popup is open.
    pub fn is_open(&self) -> bool {
        !self.stack.is_empty()
    }

    /// Whether the lookup paused playback (and will resume it).
    pub fn paused_for_lookup(&self) -> bool {
        self.paused_for_lookup
    }

    fn lookup(&mut self, query: String, origin: Origin, anchor: Rect) -> Command {
        self.generation += 1;
        let text = query.clone();
        self.stack.push(Popup {
            query,
            origin,
            anchor,
            generation: self.generation,
            reply: None,
            entry: 0,
        });
        Command::Lookup {
            generation: self.generation,
            text,
        }
    }

    /// A click on a subtitle character (POP-001, POP-005): the root popup
    /// opens, or replaces the open ones, which keeps playback paused. If the
    /// video was playing it is paused without waiting for mpv to confirm.
    pub fn open_from_subtitle(
        &mut self,
        playing: bool,
        query: String,
        origin: Origin,
        anchor: Rect,
    ) -> Vec<Command> {
        let mut out = Vec::new();
        if self.stack.is_empty() && playing {
            self.paused_for_lookup = true;
            out.push(Command::Pause);
        }
        self.stack.clear();
        out.push(self.lookup(query, origin, anchor));
        out
    }

    /// A link or a selection in popup `level` (POP-011, POP-012): its
    /// children close and a child opens. At the maximum depth the deepest
    /// popup is replaced.
    pub fn open_nested(&mut self, level: usize, query: String, anchor: Rect) -> Vec<Command> {
        if level >= self.stack.len() {
            return Vec::new();
        }
        self.stack.truncate((level + 1).min(MAX_DEPTH - 1));
        vec![self.lookup(query, Origin::Glossary, anchor)]
    }

    /// A click inside popup `level`: the popups above it close (POP-012).
    pub fn focus(&mut self, level: usize) {
        self.stack.truncate(level + 1);
    }

    fn closed(&mut self) -> Vec<Command> {
        if self.stack.is_empty() && std::mem::take(&mut self.paused_for_lookup) {
            vec![Command::Resume]
        } else {
            Vec::new()
        }
    }

    /// Esc or Back: the last popup closes; with the last one, playback
    /// resumes if the lookup paused it (POP-004).
    pub fn close_last(&mut self) -> Vec<Command> {
        if self.stack.pop().is_none() {
            return Vec::new();
        }
        self.closed()
    }

    /// A click outside every popup: all close.
    pub fn close_all(&mut self) -> Vec<Command> {
        if self.stack.is_empty() {
            return Vec::new();
        }
        self.stack.clear();
        self.closed()
    }

    /// The user played or paused by hand: the lookup no longer owns the pause
    /// (BUG-072).
    pub fn playback_changed_by_user(&mut self) {
        self.paused_for_lookup = false;
    }

    /// Moves the keyboard to the next (`+1`) or previous (`-1`) entry of the
    /// last popup (POP-014); true if it moved.
    pub fn move_entry(&mut self, delta: i64) -> bool {
        let Some(p) = self.stack.last_mut() else {
            return false;
        };
        let count = p.reply.as_ref().map_or(0, |r| r.entries);
        if count == 0 {
            return false;
        }
        let to = (p.entry as i64 + delta).clamp(0, count as i64 - 1) as usize;
        let moved = to != p.entry;
        p.entry = to;
        moved
    }

    /// The cue the root popup was opened from.
    pub fn root_cue(&self) -> Option<usize> {
        match self.stack.first()?.origin {
            Origin::Subtitle { cue, .. } => Some(cue),
            Origin::Glossary => None,
        }
    }

    /// A lookup answered: true if its popup is still open.
    pub fn deliver(&mut self, generation: u64, reply: Reply) -> bool {
        match self.stack.iter_mut().find(|p| p.generation == generation) {
            Some(p) => {
                p.reply = Some(reply);
                true
            }
            None => false,
        }
    }

    /// The query range to highlight in a cue (POP-002): the cue, the byte of
    /// the start and the number of characters.
    pub fn highlight(&self) -> Option<(usize, usize, usize)> {
        let root = self.stack.first()?;
        let Origin::Subtitle { cue, start } = root.origin else {
            return None;
        };
        let reply = root.reply.as_ref()?;
        (reply.best_length > 0).then_some((cue, start, reply.best_length))
    }
}

/// Where a popup of `size` goes (POP-006): above the word if it fits, else
/// below, else on the side with more room, shrunk to fit; never over the word
/// and always inside the window (`margin` from its edges). `size` is the
/// popup's preferred size.
pub fn place(word: Rect, size: (f64, f64), window: (f64, f64), margin: f64) -> Rect {
    let (ww, wh) = window;
    let gap = 4.0;
    let above_room = (word.y - gap - margin).max(0.0);
    let below_room = (wh - margin - word.bottom() - gap).max(0.0);
    let w = size.0.min(ww - 2.0 * margin).max(0.0);
    let (y, h) = if size.1 <= above_room {
        (word.y - gap - size.1, size.1)
    } else if size.1 <= below_room {
        (word.bottom() + gap, size.1)
    } else if above_room >= below_room {
        (word.y - gap - above_room, above_room)
    } else {
        (word.bottom() + gap, below_room)
    };
    // centred on the word, pushed inside the window
    let x = (word.x + word.w / 2.0 - w / 2.0).clamp(margin, (ww - margin - w).max(margin));
    Rect { x, y, w, h }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word() -> Rect {
        Rect {
            x: 100.0,
            y: 600.0,
            w: 40.0,
            h: 30.0,
        }
    }

    fn sub(start: usize) -> Origin {
        Origin::Subtitle { cue: 0, start }
    }

    // T-POP-001, T-POP-004, T-POP-005 state machine
    #[test]
    fn pause_and_resume_around_popups() {
        let mut p = Popups::default();
        // playing: a click pauses and the close resumes
        let c = p.open_from_subtitle(true, "食べる".into(), sub(0), word());
        assert_eq!(c[0], Command::Pause);
        assert!(matches!(&c[1], Command::Lookup { text, .. } if text == "食べる"));
        // another word: replaced, still paused, no second pause
        let c = p.open_from_subtitle(false, "猫".into(), sub(9), word());
        assert_eq!(c.len(), 1);
        assert_eq!(p.stack().len(), 1);
        assert_eq!(p.stack()[0].query, "猫");
        assert_eq!(p.close_last(), [Command::Resume]);
        assert!(!p.is_open());

        // paused by the user: the close leaves it paused
        let mut p = Popups::default();
        let c = p.open_from_subtitle(false, "x".into(), sub(0), word());
        assert!(!c.contains(&Command::Pause));
        assert_eq!(p.close_last(), []);

        // the user resumed by hand while the popup was open: no resume later
        let mut p = Popups::default();
        p.open_from_subtitle(true, "x".into(), sub(0), word());
        p.playback_changed_by_user();
        assert_eq!(p.close_all(), []);
    }

    // T-POP-014
    #[test]
    fn the_keyboard_moves_between_entries() {
        let mut p = Popups::default();
        assert!(!p.move_entry(1));
        p.open_from_subtitle(true, "x".into(), sub(0), word());
        let g = p.stack()[0].generation;
        assert!(!p.move_entry(1), "nothing to move before the reply");
        p.deliver(
            g,
            Reply {
                entries: 3,
                ..Reply::default()
            },
        );
        assert!(p.move_entry(1) && p.move_entry(1));
        assert!(!p.move_entry(1), "stops at the last");
        assert_eq!(p.stack()[0].entry, 2);
        assert!(p.move_entry(-5));
        assert_eq!(p.stack()[0].entry, 0);
        assert_eq!(p.root_cue(), Some(0));
    }

    // T-POP-012
    #[test]
    fn nesting() {
        let mut p = Popups::default();
        p.open_from_subtitle(true, "root".into(), sub(0), word());
        for i in 0..20 {
            let level = p.stack().len() - 1;
            p.open_nested(level, format!("child {i}"), word());
        }
        assert_eq!(p.stack().len(), MAX_DEPTH);
        // Esc closes only the last one; no resume while others are open
        assert_eq!(p.close_last(), []);
        assert_eq!(p.stack().len(), MAX_DEPTH - 1);
        // a click on the parent closes its children
        p.focus(1);
        assert_eq!(p.stack().len(), 2);
        // a link in the root replaces the child
        p.open_nested(0, "other".into(), word());
        assert_eq!(p.stack().len(), 2);
        assert_eq!(p.stack()[1].query, "other");
        // closing everything resumes once
        assert_eq!(p.close_all(), [Command::Resume]);
        assert_eq!(p.close_all(), []);
    }

    #[test]
    fn replies_reach_only_open_popups() {
        let mut p = Popups::default();
        let c = p.open_from_subtitle(true, "a".into(), sub(3), word());
        let Command::Lookup {
            generation: first, ..
        } = c[1].clone()
        else {
            panic!()
        };
        p.open_from_subtitle(false, "b".into(), sub(5), word());
        assert!(
            !p.deliver(first, Reply::default()),
            "the replaced popup's reply is dropped"
        );
        let generation = p.stack()[0].generation;
        assert!(p.deliver(
            generation,
            Reply {
                content: Arc::default(),
                best_length: 2,
                empty: false,
                entries: 1,
            }
        ));
        assert_eq!(p.highlight(), Some((0, 5, 2)));
    }

    // T-POP-006
    #[test]
    fn placement_above_below_and_shrunk() {
        let window = (1280.0, 720.0);
        let above = place(word(), (400.0, 300.0), window, 8.0);
        assert!(above.bottom() <= word().y && above.h == 300.0);
        let top_word = Rect { y: 20.0, ..word() };
        let below = place(top_word, (400.0, 300.0), window, 8.0);
        assert!(below.y >= top_word.bottom());
        let middle = Rect { y: 300.0, ..word() };
        let shrunk = place(middle, (400.0, 600.0), window, 8.0);
        assert!(shrunk.h < 600.0 && !shrunk.overlaps(&middle));
    }

    proptest::proptest! {
        // T-POP-006
        #[test]
        fn the_popup_never_covers_the_word_and_stays_inside(
            ww in 320.0f64..3840.0, wh in 240.0f64..2160.0,
            fx in 0.0f64..1.0, fy in 0.0f64..1.0, fw in 0.0f64..0.5, fh in 0.0f64..0.2,
            pw in 50.0f64..2000.0, ph in 50.0f64..2000.0,
        ) {
            let word = Rect { x: fx * ww * 0.9, y: fy * wh * 0.9, w: (fw * ww).max(1.0), h: (fh * wh).max(1.0) };
            let word = Rect { w: word.w.min(ww - word.x), h: word.h.min(wh - word.y), ..word };
            let r = place(word, (pw, ph), (ww, wh), 8.0);
            proptest::prop_assert!(!r.overlaps(&word), "{r:?} covers {word:?}");
            proptest::prop_assert!(r.x >= 0.0 && r.y >= 0.0 && r.right() <= ww + 1e-9 && r.bottom() <= wh + 1e-9, "{r:?} outside {ww}x{wh}");
        }
    }
}
