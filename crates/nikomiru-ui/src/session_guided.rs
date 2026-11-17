// SPDX-License-Identifier: GPL-3.0-only

//! Pause-transition lookup ownership and transient secondary-track holds.
use super::*;
use crate::guided_lookup::{Pending, first_kanji, first_unknown};
impl Session {
    fn guided_allowed(&self) -> bool {
        self.view.loaded
            && self.view.paused
            && !self.popups_covered
            && !self.popups.is_open()
            && !self.subs.disabled
            && !self.subs.incomplete
            && self.recall.is_none()
            && !self.clozes.open
            && !self.batch.open
            && !self.bank.open
            && !self.reader.open
            && !self.podcasts.open
    }
    pub(super) fn cancel_guided(&mut self) {
        self.guided_pending = None;
        if self.guided_target {
            self.keyboard_lookup = None;
            self.keyboard_kanji = false;
        }
        self.guided_target = false;
    }
    pub(super) fn arm_guided(&mut self) {
        self.cancel_guided();
        let rule = &self.services.config.lookup.pause_lookup;
        if !self.guided_allowed() || rule == "off" || self.keyboard_lookup.is_some() {
            return;
        }
        let Some(&cue) = self.subs.active().first() else {
            return;
        };
        self.guided_pending = Some(Pending {
            token: self.token,
            version: self.sub_list_version,
            dictionaries: self.words.generation(),
            cue,
            rule: rule.clone(),
        });
        self.resolve_guided();
    }
    pub(super) fn resolve_guided(&mut self) {
        let Some(pending) = self.guided_pending.clone() else {
            return;
        };
        if !self.guided_allowed()
            || pending.token != self.token
            || pending.version != self.sub_list_version
            || pending.dictionaries != self.words.generation()
            || pending.rule != self.services.config.lookup.pause_lookup
            || self.subs.active().first() != Some(&pending.cue)
        {
            self.cancel_guided();
            return;
        }
        let Some(cue) = self
            .subs
            .cue(pending.cue)
            .filter(|c| c.plain_text.len() <= 16384)
        else {
            self.cancel_guided();
            return;
        };
        let text = &cue.plain_text;
        let offset = match pending.rule.as_str() {
            "unknown" => {
                let Some(words) = self.words.get(text) else {
                    let text = text.clone();
                    self.words.ask(&text);
                    return;
                };
                first_unknown(text, words, |e, r| self.words.status(e, r))
            }
            "kanji" => first_kanji(text),
            _ => None,
        };
        self.guided_pending = None;
        if let Some(offset) = offset {
            self.keyboard_lookup = Some((pending.cue, offset));
            self.keyboard_kanji = pending.rule == "kanji";
            self.guided_target = true;
        }
    }
    pub(super) fn first_kanji_lookup(&mut self) {
        self.cancel_guided();
        if self.popups_covered
            || self.popups.is_open()
            || self.subs.disabled
            || self.subs.incomplete
        {
            return;
        }
        let target = self
            .subs
            .active()
            .first()
            .and_then(|&cue| Some((cue, first_kanji(&self.subs.cue(cue)?.plain_text)?)));
        self.keyboard_lookup = target;
        self.keyboard_kanji = target.is_some();
    }
    pub(super) fn look_up_kanji(&mut self, cue: usize, offset: usize, anchor: Rect) {
        let Some(c) = self
            .subs
            .cue(cue)
            .and_then(|c| c.plain_text.get(offset..))
            .and_then(|s| s.chars().next())
            .filter(|c| nikomiru_text::kana::is_kanji(*c))
        else {
            return;
        };
        self.hover.reset();
        self.subs.selection = Some((cue, offset));
        self.subs.highlight = None;
        let at = self.moment();
        self.study.lookup(at);
        let playing = self.view.loaded && !self.view.paused;
        let commands = self.popups.open_from_subtitle(
            playing,
            c.to_string(),
            Origin::Subtitle { cue, start: offset },
            anchor,
        );
        self.run_popup_commands(commands);
    }
    /// Clear transient translation visibility; never changes a saved obscuring mode.
    pub fn release_translation_peek(&mut self) {
        self.translation_hold = None;
    }
    /// Whether an existing secondary track is currently revealed by its held key.
    pub fn translation_peeking(&self) -> bool {
        self.translation_hold.is_some()
            && self.secondary.source.is_some()
            && !self.secondary.disabled
            && !self.popups_covered
            && !self.popups.is_open()
            && self.recall.is_none()
            && !self.batch.open
            && !self.bank.open
            && !self.reader.open
            && !self.podcasts.open
            && !self.clozes.open
    }
    /// Refresh priority barriers when a page or text input gains ownership.
    pub(crate) fn guided_input_priority(&mut self, text_focus: bool) {
        if self.popups_covered
            || self.popups.is_open()
            || text_focus
            || self.recall.is_some()
            || self.clozes.open
            || self.batch.open
            || self.bank.open
            || self.reader.open
            || self.podcasts.open
        {
            self.release_translation_peek();
            self.cancel_guided();
        }
    }
}
