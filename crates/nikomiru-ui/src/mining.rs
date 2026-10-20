// SPDX-License-Identifier: GPL-3.0-only

//! Mining to Anki from the popup (ANK-030…ANK-039, Ctrl+Enter of POP-014): the
//! state of each entry's card button, the requests the `anki` worker of the
//! app runs, and the last card made since Nikomiru started ([ADR 0007]).
//!
//! [ADR 0007]: ../../../docs/adr/0007-mining-to-anki.md

use std::path::PathBuf;

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
    /// The ff-index of the active audio track; `None` without audio.
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
    /// The note type the fields belong to.
    pub model: String,
    /// Its fields, empty if Anki does not have it.
    pub fields: Vec<String>,
}

/// Mining, away from the UI thread: the `anki` worker of the app. `done`
/// may run on any thread.
pub trait AnkiService: Send {
    /// Which entries have a card already: `None` when Anki cannot be asked
    /// (not on this computer, or paused after a failure).
    fn check(&self, request: CheckRequest, done: Box<dyn FnOnce(Option<Vec<bool>>) + Send>);

    /// Makes, or overwrites, a card.
    fn mine(&self, request: MineRequest, done: Box<dyn FnOnce(Result<Mined, MineFailure>) + Send>);

    /// Opens Anki's browser.
    fn browse(&self, target: BrowseTarget, done: Box<dyn FnOnce(Result<(), String>) + Send>);

    /// Reads what the AnkiConnect of `config` has (ANK-006): the connection
    /// test of the settings.
    fn refresh(&self, config: AnkiConfig, done: Box<dyn FnOnce(Result<AnkiLists, String>) + Send>);

    /// "Create Lapis deck" (ANK-011): `true` if something was made.
    fn create_lapis(&self, config: AnkiConfig, done: Box<dyn FnOnce(Result<bool, String>) + Send>);

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

    fn mine(&self, _: MineRequest, done: Box<dyn FnOnce(Result<Mined, MineFailure>) + Send>) {
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

    fn create_lapis(&self, _: AnkiConfig, done: Box<dyn FnOnce(Result<bool, String>) + Send>) {
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
    /// Lapis.
    pub lapis_created: u64,
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
