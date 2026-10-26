// SPDX-License-Identifier: GPL-3.0-only

//! Mining to Anki from the popup (ANK-030…ANK-039, Ctrl+Enter of POP-014): the
//! state of each entry's card button, the requests the `anki` worker of the
//! app runs, and the last card made since Nikomiru started ([ADR 0007]).
//!
//! [ADR 0007]: ../../../docs/adr/0007-mining-to-anki.md

use std::path::{Path, PathBuf};
use std::time::Duration;

use nikomiru_core::config::AnkiConfig;

/// The card button of an entry (ANK-030).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardState {
    /// Not known yet, or Anki cannot be asked: `+`.
    #[default]
    Unknown,
    /// No card yet: `+`.
    CanAdd,
    /// Anki has one already: `✓`, with a menu.
    Exists,
    /// The last card made in this run, with its note id: `✓↩`, with a menu
    /// that can overwrite it (ANK-037).
    Last(u64),
    /// A card is being made.
    Busy,
}

impl CardState {
    /// What the button shows.
    pub fn glyph(self) -> &'static str {
        match self {
            CardState::Unknown | CardState::CanAdd => "+",
            CardState::Exists => "✓",
            CardState::Last(_) => "✓↩",
            CardState::Busy => "…",
        }
    }

    /// Its name for the GUI test harness.
    pub fn name(self) -> &'static str {
        match self {
            CardState::Unknown => "unknown",
            CardState::CanAdd => "add",
            CardState::Exists => "exists",
            CardState::Last(_) => "last",
            CardState::Busy => "busy",
        }
    }

    /// Whether a click opens the menu instead of making a card.
    pub fn has_menu(self) -> bool {
        matches!(self, CardState::Exists | CardState::Last(_))
    }
}

/// An item of the menu of `✓` and `✓↩` (ANK-030).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    /// Replaces the fields of the last card (only on `✓↩`, ANK-037).
    Overwrite,
    /// Opens Anki's browser on the card (ANK-038).
    OpenInAnki,
    /// Makes another card although one exists.
    AddAnyway,
}

impl MenuItem {
    /// The items of the menu of a button in `state`.
    pub fn of(state: CardState) -> &'static [MenuItem] {
        match state {
            CardState::Last(_) => &[
                MenuItem::Overwrite,
                MenuItem::OpenInAnki,
                MenuItem::AddAnyway,
            ],
            CardState::Exists => &[MenuItem::OpenInAnki, MenuItem::AddAnyway],
            _ => &[],
        }
    }

    /// The message id of its label.
    pub fn label(self) -> &'static str {
        match self {
            MenuItem::Overwrite => "card-overwrite",
            MenuItem::OpenInAnki => "card-open-in-anki",
            MenuItem::AddAnyway => "card-add-anyway",
        }
    }

    /// Its name for the GUI test harness.
    pub fn name(self) -> &'static str {
        match self {
            MenuItem::Overwrite => "overwrite",
            MenuItem::OpenInAnki => "open-in-anki",
            MenuItem::AddAnyway => "add-anyway",
        }
    }

    /// The item with this harness name.
    pub fn from_name(name: &str) -> Option<Self> {
        [
            MenuItem::Overwrite,
            MenuItem::OpenInAnki,
            MenuItem::AddAnyway,
        ]
        .into_iter()
        .find(|m| m.name() == name)
    }
}

/// What to do with the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardAction {
    /// A new card; Anki refuses a duplicate.
    Add,
    /// A new card even if one exists.
    AddAnyway,
    /// The fields of this note replaced (ANK-037).
    Overwrite(u64),
}

/// The picture of a card (ANK-034, ANK-040).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrameChoice {
    /// The frame of the click, saved by the player.
    #[default]
    Click,
    /// The frame at this position of the video, in ms.
    At(i64),
    /// No picture.
    None,
}

/// The picture chosen in the "Advanced mining" dialog (ANK-040).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrameKind {
    /// The frame of the click.
    #[default]
    Click,
    /// The frame at the start of the sentence.
    SentenceStart,
    /// No picture.
    None,
}

/// The "Advanced mining" dialog (ANK-040, ANK-041): the card of an entry
/// with its sentence, clip, picture and audio chosen by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancedCard {
    /// The popup.
    pub level: usize,
    /// The entry.
    pub entry: usize,
    /// The word, for the title.
    pub expression: String,
    /// The reference cue (ANK-031), and the cues merged around it.
    pub reference: Option<usize>,
    /// The first cue of the sentence.
    pub first: usize,
    /// The last one.
    pub last: usize,
    /// The sentence: the merged cues, a line each.
    pub sentence: String,
    /// The word's place in it (ANK-024).
    pub offset: Option<usize>,
    /// The clip in player ms; `None` without a cue.
    pub clip: Option<(i64, i64)>,
    /// The picture.
    pub frame: FrameKind,
    /// The sentence audio goes with the card.
    pub audio: bool,
    /// A cue before the first, or after the last, can be added.
    pub can_previous: bool,
    /// See `can_previous`.
    pub can_next: bool,
}

/// A change in the "Advanced mining" dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedEdit {
    /// The cue before the sentence added (`true`) or its first cue taken
    /// away (ANK-041).
    Previous(bool),
    /// The cue after it added, or its last cue taken away.
    Next(bool),
    /// The clip's start, in player ms.
    Start(i64),
    /// The clip's end.
    End(i64),
    /// The picture.
    Frame(FrameKind),
    /// With the sentence audio or without.
    Audio(bool),
    /// Plays the clip.
    Preview,
    /// Makes the card.
    Create,
    /// Closes the dialog.
    Cancel,
}

/// The sentence of `lines` (the cues merged, ANK-041), a line each, and the
/// word's `offset` in line `reference` moved to its place in it.
pub fn merge_sentence(
    lines: &[&str],
    reference: usize,
    offset: Option<usize>,
) -> (String, Option<usize>) {
    let before: usize = lines.iter().take(reference).map(|l| l.len() + 1).sum();
    (lines.join("\n"), offset.map(|o| o + before))
}

/// The entries of a popup to check for existing cards (ANK-030).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckRequest {
    /// Expression and reading of each entry.
    pub entries: Vec<(String, String)>,
}

/// A card to make from a popup entry and its scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MineRequest {
    /// The popup's query: the worker looks it up again.
    pub query: String,
    /// The entry's index in the reply.
    pub entry: usize,
    /// Its headword, to check the entry is the same.
    pub expression: String,
    /// Its reading.
    pub reading: String,
    /// The text of the query it matched.
    pub matched: String,
    /// The reference cue's text (ANK-031), empty without one.
    pub sentence: String,
    /// Where the word starts in it, if the lookup started there (ANK-024).
    pub offset: Option<usize>,
    /// The video.
    pub video: PathBuf,
    /// Where the sentence audio is cut from: the video, or the file of an
    /// external audio track.
    pub audio_input: PathBuf,
    /// The ff-index of the active audio track in `audio_input`; `None`
    /// without audio.
    pub audio_stream: Option<i64>,
    /// The clip of ANK-032 in player milliseconds; `None` without a cue.
    pub clip: Option<(i64, i64)>,
    /// The position, for the link when there is no clip.
    pub position_ms: i64,
    /// The video's uid in the library (DAT-003), for the link.
    pub uid: Option<String>,
    /// The file name without extension: the link's title and
    /// `{document-title}`.
    pub title: String,
    /// What to do.
    pub action: CardAction,
    /// The picture (ANK-034, ANK-040).
    pub frame: FrameChoice,
    /// The sentence audio goes with the card (ANK-040).
    pub audio: bool,
}

/// A card that was made or overwritten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mined {
    /// Anki's note id.
    pub note_id: u64,
    /// The deck.
    pub deck: String,
    /// An existing note was overwritten.
    pub overwritten: bool,
}

/// A card that was not made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MineFailure {
    /// Why, translated.
    pub message: String,
    /// Anki has the card already.
    pub duplicate: bool,
}

/// Waits at most the given time for the player to write a frame; if it gives
/// up first, the file is removed once the player has written it.
pub type WaitFrame = Box<dyn FnOnce(Duration) -> Result<(), String> + Send>;

/// The frame on screen being saved for a card (ANK-034). The player is asked
/// when the card is asked for, in order with its other commands, so the frame
/// is the one of the click even if playback resumes before the card is made.
/// The file goes with it, however the attempt ends (ADR 0007 §20).
pub struct PendingFrame {
    path: PathBuf,
    wait: Option<WaitFrame>,
}

impl PendingFrame {
    /// The frame the player is writing to `path`.
    pub fn new(path: PathBuf, wait: WaitFrame) -> Self {
        Self {
            path,
            wait: Some(wait),
        }
    }

    /// The PNG, once the player has written it: waits at most `timeout`.
    pub fn written(&mut self, timeout: Duration) -> Result<&Path, String> {
        let wait = self
            .wait
            .take()
            .ok_or_else(|| "the frame was waited for already".to_string())?;
        wait(timeout)?;
        Ok(&self.path)
    }
}

impl Drop for PendingFrame {
    fn drop(&mut self) {
        // not waited for: a file still to come goes when it comes
        if let Some(wait) = self.wait.take() {
            let _ = wait(Duration::ZERO);
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

impl std::fmt::Debug for PendingFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingFrame")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

/// What "Open in Anki" shows (ANK-038).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowseTarget {
    /// A note made in this run.
    Note(u64),
    /// The cards of a word.
    Word {
        /// Its headword.
        expression: String,
        /// Its reading.
        reading: String,
    },
}

/// What Anki has, for the settings page (ANK-006).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnkiLists {
    /// AnkiConnect's API version.
    pub version: u64,
    /// The decks.
    pub decks: Vec<String>,
    /// The note types.
    pub models: Vec<String>,
    /// The deck of the settings tested.
    pub deck: String,
    /// The note type of the settings tested: the fields are its.
    pub model: String,
    /// Its fields, empty if Anki does not have it.
    pub fields: Vec<String>,
}

/// What "Create Lapis deck" left in Anki (ANK-011).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LapisMade {
    /// The note type or the deck was made.
    pub created: bool,
    /// The deck's name in Anki.
    pub deck: String,
    /// The note type's name in Anki.
    pub note_type: String,
}

/// Mining, away from the UI thread: the `anki` worker of the app. `done`
/// may run on any thread.
pub trait AnkiService: Send {
    /// Which entries have a card already: `None` when Anki cannot be asked
    /// (not on this computer, or paused after a failure).
    fn check(&self, request: CheckRequest, done: Box<dyn FnOnce(Option<Vec<bool>>) + Send>);

    /// Makes, or overwrites, a card, with the frame of the click if the player
    /// could be asked for it.
    fn mine(
        &self,
        request: MineRequest,
        frame: Option<PendingFrame>,
        done: Box<dyn FnOnce(Result<Mined, MineFailure>) + Send>,
    );

    /// Opens Anki's browser.
    fn browse(&self, target: BrowseTarget, done: Box<dyn FnOnce(Result<(), String>) + Send>);

    /// Reads what the AnkiConnect of `config` has (ANK-006): the connection
    /// test of the settings.
    fn refresh(&self, config: AnkiConfig, done: Box<dyn FnOnce(Result<AnkiLists, String>) + Send>);

    /// "Create Lapis deck" (ANK-011).
    fn create_lapis(
        &self,
        config: AnkiConfig,
        done: Box<dyn FnOnce(Result<LapisMade, String>) + Send>,
    );

    /// The settings changed: cards follow them from now on.
    fn configure(&self, config: AnkiConfig);
}

/// No Anki: checks know nothing, cards are not made.
#[derive(Debug, Default)]
pub struct NoAnki(pub String);

impl AnkiService for NoAnki {
    fn check(&self, _: CheckRequest, done: Box<dyn FnOnce(Option<Vec<bool>>) + Send>) {
        done(None);
    }

    fn mine(
        &self,
        _: MineRequest,
        _: Option<PendingFrame>,
        done: Box<dyn FnOnce(Result<Mined, MineFailure>) + Send>,
    ) {
        done(Err(MineFailure {
            message: self.0.clone(),
            duplicate: false,
        }));
    }

    fn browse(&self, _: BrowseTarget, done: Box<dyn FnOnce(Result<(), String>) + Send>) {
        done(Err(self.0.clone()));
    }

    fn refresh(&self, _: AnkiConfig, done: Box<dyn FnOnce(Result<AnkiLists, String>) + Send>) {
        done(Err(self.0.clone()));
    }

    fn create_lapis(&self, _: AnkiConfig, done: Box<dyn FnOnce(Result<LapisMade, String>) + Send>) {
        done(Err(self.0.clone()));
    }

    fn configure(&self, _: AnkiConfig) {}
}

/// A job for the worker, taken by the UI layer.
#[derive(Debug)]
pub enum AnkiJob {
    /// Check the entries of popup `generation`.
    Check {
        /// The popup.
        generation: u64,
        /// What to check.
        request: CheckRequest,
    },
    /// Make a card from entry `entry` of popup `generation`.
    Mine {
        /// The popup.
        generation: u64,
        /// The entry.
        entry: usize,
        /// The card.
        request: Box<MineRequest>,
        /// The frame of the click, being saved.
        frame: Option<PendingFrame>,
    },
    /// Open Anki's browser.
    Browse(BrowseTarget),
    /// Read what Anki has, with these settings.
    Refresh(AnkiConfig),
    /// "Create Lapis deck", with these settings.
    CreateLapis(AnkiConfig),
    /// New settings.
    Configure(AnkiConfig),
}

/// The settings page's state in the session (ANK-006, ANK-011).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnkiPage {
    /// What Anki has, after the last test.
    pub lists: Option<AnkiLists>,
    /// The last outcome: a message, and whether it went well.
    pub status: Option<(String, bool)>,
    /// A request is running.
    pub busy: bool,
    /// Grows with each "Create Lapis deck" that worked: the page then chooses
    /// [`AnkiPage::lapis`].
    pub lapis_created: u64,
    /// The deck and note type the last "Create Lapis deck" left.
    pub lapis: Option<(String, String)>,
}

/// The last card made since Nikomiru started (ANK-030, ANK-037).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastCard {
    /// Anki's note id.
    pub note_id: u64,
    /// Its headword.
    pub expression: String,
    /// Its reading.
    pub reading: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-ANK-041: merged cues keep the word bold at its place
    #[test]
    fn merged_cues_keep_the_word_in_place() {
        let lines = ["前の行", "今日は寿司を食べました", "次"];
        let (sentence, offset) = merge_sentence(&lines, 1, Some("今日は".len()));
        assert_eq!(sentence, "前の行\n今日は寿司を食べました\n次");
        let o = offset.unwrap();
        assert!(sentence[o..].starts_with("寿司"));
        assert_eq!(
            merge_sentence(&lines[1..2], 0, Some(9)),
            (lines[1].to_string(), Some(9))
        );
        assert_eq!(merge_sentence(&lines, 2, None).1, None);
        assert_eq!(
            nikomiru_anki::compose::sentence(&sentence, offset, "寿司"),
            "前の行<br>今日は<b>寿司</b>を食べました<br>次"
        );
    }

    // T-ANK-030
    #[test]
    fn the_button_and_its_menu() {
        assert_eq!(CardState::Unknown.glyph(), "+");
        assert_eq!(CardState::Exists.glyph(), "✓");
        assert_eq!(CardState::Last(7).glyph(), "✓↩");
        assert!(!CardState::CanAdd.has_menu() && CardState::Exists.has_menu());
        assert_eq!(
            MenuItem::of(CardState::Exists),
            [MenuItem::OpenInAnki, MenuItem::AddAnyway],
            "overwrite only for the last card"
        );
        assert_eq!(MenuItem::of(CardState::Last(7))[0], MenuItem::Overwrite);
        assert_eq!(MenuItem::from_name("add-anyway"), Some(MenuItem::AddAnyway));
    }
}
