// SPDX-License-Identifier: GPL-3.0-only

//! Word audio from the popup (AUD-001…AUD-008): the jobs the app's
//! `word-audio` worker runs off the UI thread, and the menu of an entry.

use nikomiru_core::config::WordAudioConfig;

/// An audio file of the menu (AUD-004).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordAudioItem {
    /// What the menu shows: the source, and the name it gives.
    pub label: String,
    /// Where it is: played from the cache, or fetched.
    pub url: String,
}

/// The audio menu of an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioMenu {
    /// The sources are being asked.
    Loading,
    /// What they gave, in their order.
    Items(Vec<WordAudioItem>),
}

type Done<T> = Box<dyn FnOnce(Result<T, String>) + Send>;

/// The word audio worker.
pub trait WordAudioService: Send {
    /// Plays the first audio the sources give for the word, in their order
    /// (AUD-004); `false` when none has one.
    fn play(&self, term: String, reading: String, done: Done<bool>);
    /// Every audio every source gives, asked in parallel (AUD-004).
    fn list(&self, term: String, reading: String, done: Done<Vec<WordAudioItem>>);
    /// Plays one audio of the menu.
    fn play_url(&self, url: String, done: Done<()>);
    /// The settings changed: the sources, the volume, the proxy.
    fn configure(&self, config: WordAudioConfig, proxy: String);
}

/// No word audio: every job fails with the reason.
#[derive(Debug, Default)]
pub struct NoWordAudio(pub String);

impl WordAudioService for NoWordAudio {
    fn play(&self, _: String, _: String, done: Done<bool>) {
        done(Err(self.0.clone()));
    }

    fn list(&self, _: String, _: String, done: Done<Vec<WordAudioItem>>) {
        done(Err(self.0.clone()));
    }

    fn play_url(&self, _: String, done: Done<()>) {
        done(Err(self.0.clone()));
    }

    fn configure(&self, _: WordAudioConfig, _: String) {}
}

/// A job for the word audio worker, taken from the session by the UI layer.
#[derive(Debug, Clone, PartialEq)]
pub enum WordAudioJob {
    /// Play the first audio of a word.
    Play {
        /// The headword.
        term: String,
        /// Its reading.
        reading: String,
    },
    /// The menu of an entry.
    List {
        /// The popup.
        level: usize,
        /// Its lookup, to drop an answer for another.
        generation: u64,
        /// The entry.
        entry: usize,
        /// The headword.
        term: String,
        /// Its reading.
        reading: String,
    },
    /// Play an audio of the menu.
    PlayUrl(String),
    /// New settings.
    Configure(Box<(WordAudioConfig, String)>),
}
